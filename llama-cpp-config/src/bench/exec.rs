//! Running a benchmark: the impure half of the Benchmark tab (the planning and
//! aggregation live in `bench.rs`). One worker thread per run, reporting through
//! a callback; the GUI wraps that callback in `invoke_from_event_loop`.
//!
//! ## The two preflights are OPPOSITE, and neither is cosmetic
//!
//! The live mode needs llama-server UP: it measures that process. The synthetic
//! mode needs it DOWN, because llama-server holds the whole model resident and
//! benching alongside it does not fail cleanly: the second copy spills into
//! shared memory over PCIe and the numbers read as a backend regression rather
//! than as a mistake, which is why this is a hard stop and not a warning.
//!
//! ## A preset is written when it is done, and only then
//!
//! Everything a run produces goes into bench.db (`db` module header): each
//! preset's run is committed, points, samples, parameters and the engine's log
//! together, when the engine moves past it, so a run that dies at its third
//! preset keeps the first two. A preset that fails or is cancelled is dropped
//! (the database holds measurements, not attempts); its error message carries
//! the tail of the engine's output instead. The markdown report is rendered at
//! the end of a successful run and stored with its campaign.
//!
//! ## Why the live mode warms up, and why it groups by preset
//!
//! llama-server's router loads a model on demand, so the FIRST request for a
//! preset pays a model load of tens of seconds; measuring that would put a disk
//! read in a tokens-per-second column. A throwaway request with a two-word
//! prompt loads it without prefilling anything worth timing. And the reps of one
//! preset run together rather than interleaved, because switching preset is
//! another model load: the cost of interleaving is minutes per switch, the cost
//! of grouping is that a thermal drift is charged to the last preset, which the
//! report says out loud.

use std::io::{BufRead, BufReader};
use std::process::{Command, Stdio};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc;
use std::sync::Arc;
use std::time::Duration;

use super::db::Sample;
use super::record::Recorder;
use super::{Mode, Plan, Point, RunMeta};
use crate::{http, paths, presets, proc, runstate, server_cfg};

/// Read timeout for one live request. Generous for the same reason the snapshot
/// transfer's is: a cold prefill of a long prompt on a big model runs into
/// minutes, and the cost of guessing low is a completed measurement reported as
/// a timeout.
const LIVE_TIMEOUT: Duration = Duration::from_secs(900);

/// How often the synthetic runner looks up from the output pipe to notice a
/// cancel. Short enough to feel immediate, long enough not to spin.
const CANCEL_POLL: Duration = Duration::from_millis(200);

/// What the runner reports back, on the worker thread.
pub enum Event {
    /// One line of human-readable progress for the status area.
    Progress(String),
    /// The whole current result set. The UI replaces its table with it rather
    /// than patching rows: a live point is revised as its repetitions land.
    Points(Vec<Point>),
    /// The run ended: its bench.db campaign, or why it did not.
    Done(Result<String, String>),
}

/// A running benchmark. Dropping it does NOT stop the run (the worker owns its
/// own state); `cancel()` does, at the next repetition or test boundary.
pub struct Handle {
    cancel: Arc<AtomicBool>,
}

impl Handle {
    pub fn cancel(&self) {
        self.cancel.store(true, Ordering::SeqCst);
    }
}

/// Start a run on its own thread. `emit` is called from that thread.
pub fn spawn(plan: Plan, emit: impl Fn(Event) + Send + 'static) -> Handle {
    let cancel = Arc::new(AtomicBool::new(false));
    let flag = cancel.clone();
    std::thread::spawn(move || {
        let result = run(&plan, &flag, &emit);
        emit(Event::Done(result));
    });
    Handle { cancel }
}

/// Everything one run owns, plus the accumulating results.
struct Run {
    plan: Plan,
    meta: RunMeta,
    points: Vec<Point>,
    /// The flash-attention setting each preset RESOLVED to, read back off
    /// llama-bench's rows. A preset that pins nothing runs at `auto`, which
    /// resolves per backend, so two legs of a comparison can silently land on
    /// different kernels; `flash_attn_mismatch` turns that into a warning at the
    /// top of the report.
    flash_attn: std::collections::BTreeMap<String, String>,
    /// bench.db: where every finished preset goes. An `Option` only so a
    /// failed preset can be dropped through the same path.
    rec: Option<Recorder>,
    /// What each preset's device ids resolved to, for its db run.
    resolved: Vec<(String, Vec<(String, String)>)>,
}

impl Run {
    /// Record (or revise) one point: into memory and out to the UI.
    fn put(&mut self, point: Point, emit: &impl Fn(Event)) {
        match self
            .points
            .iter_mut()
            .find(|p| p.preset == point.preset && p.test == point.test)
        {
            Some(slot) => *slot = point,
            None => self.points.push(point),
        }
        emit(Event::Points(self.points.clone()));
    }

    fn note(&self, text: &str) {
        if let Some(rec) = &self.rec {
            rec.log(text);
        }
    }

    /// Hold the db run for the preset about to be measured, committing the one
    /// before it: the engine only moves on after a preset is done.
    fn begin_preset(&mut self, preset: &presets::Preset, cfg: &server_cfg::ServerConfig) {
        self.commit_preset();
        let devices = self
            .resolved
            .iter()
            .find(|(id, _)| *id == preset.id)
            .map(|(_, d)| d.clone())
            .unwrap_or_default();
        let started = super::stamp_human(super::now_secs());
        if let Some(rec) = &mut self.rec {
            rec.start(preset, cfg, &devices, &started);
        }
    }

    fn commit_preset(&mut self) {
        let Some(preset) = self
            .rec
            .as_ref()
            .and_then(|r| r.current_preset().map(str::to_string))
        else {
            return;
        };
        let notes = self.db_notes(&preset);
        let points: Vec<Point> = self
            .points
            .iter()
            .filter(|p| p.preset == preset)
            .cloned()
            .collect();
        if let Some(Err(e)) = self.rec.as_mut().map(|r| r.commit(&notes, &points)) {
            self.meta
                .warnings
                .push(format!("bench.db: {e}; this run is not in the database."));
        }
    }

    fn sample(&mut self, s: Sample) {
        if let Some(rec) = &mut self.rec {
            rec.sample(s);
        }
    }

    /// A db run's notes: the run's warnings, the machine state the report's
    /// Environment table carries, and what flash-attention resolved to.
    fn db_notes(&self, preset: &str) -> String {
        let mut lines: Vec<String> = self.meta.warnings.clone();
        for (label, value) in &self.meta.env {
            if !label.starts_with("devices (") {
                lines.push(format!("{label}: {value}"));
            }
        }
        if let Some(fa) = self.flash_attn.get(preset) {
            lines.push(format!("flash-attn: {fa}"));
        }
        lines.join("\n")
    }

    /// The last preset is committed only when the run as a whole succeeded;
    /// a failed or cancelled one is dropped (`db` module header).
    fn close_db(&mut self, outcome: &Result<(), String>) {
        match outcome {
            Ok(()) => self.commit_preset(),
            Err(_) => {
                if let Some(rec) = &mut self.rec {
                    rec.discard();
                }
            }
        }
    }
}

/// The last lines of the held run's log, for an error message: a failed
/// preset is not recorded, so this is where its evidence goes.
fn log_tail(run: &Run, lines: usize) -> String {
    let Some(sink) = run.rec.as_ref().and_then(Recorder::log_sink) else {
        return String::new();
    };
    let log = sink.lock().map(|l| l.clone()).unwrap_or_default();
    let tail: Vec<&str> = log.lines().filter(|l| !l.trim().is_empty()).collect();
    tail[tail.len().saturating_sub(lines)..].join("\n")
}

fn run(plan: &Plan, cancel: &Arc<AtomicBool>, emit: &impl Fn(Event)) -> Result<String, String> {
    if let Some(problem) = super::validate(plan) {
        return Err(problem);
    }
    let cfg = server_cfg::load();
    let running = runstate::is_running();
    match plan.mode {
        Mode::Live if !running => {
            return Err(
                "llama-server is not running. The live mode measures the running \
                        server, so start it first (the Start button on the left)."
                    .into(),
            )
        }
        Mode::Synthetic if running => {
            return Err(
                "llama-server is running and holds the model in VRAM. Stop it first: \
                        benching alongside it does not fail, it spills into shared memory \
                        and reads as a backend regression."
                    .into(),
            )
        }
        _ => {}
    }

    let all = presets::load_all();
    let chosen: Vec<presets::Preset> = plan
        .presets
        .iter()
        .filter_map(|id| all.iter().find(|p| &p.id == id).cloned())
        .collect();
    if chosen.len() != plan.presets.len() {
        return Err(
            "A selected preset is no longer in presets.ini. Refresh (F5) and \
                    re-select."
                .into(),
        );
    }

    let dir = super::bench_dir();
    std::fs::create_dir_all(&dir).map_err(|e| format!("cannot create {}: {e}", dir.display()))?;
    let stamp = super::stamp(super::now_secs());
    // Opened BEFORE anything is measured: a database that cannot be written is
    // a run whose results would have nowhere to go.
    let rec = Recorder::begin(&super::db::work_path(), plan, &stamp)
        .map_err(|e| format!("bench.db: {e}"))?;

    // What each preset's device ids point at RIGHT NOW. Recorded because the ids
    // move with driver state, so "ROCm0" in a six-week-old report is not
    // necessarily the card you are comparing against today.
    let devs = crate::devices::probed();
    let resolved: Vec<(String, Vec<(String, String)>)> = chosen
        .iter()
        .map(|p| (p.id.clone(), super::device_names(p.device.trim(), &devs)))
        .collect();
    let mut warnings = Vec::new();
    if let Some(w) = super::unknown_device_warning(&resolved) {
        warnings.push(w);
    }

    let now = super::now_secs();
    let mut env = super::env::stamp(now);
    for (preset, ids) in &resolved {
        if ids.is_empty() {
            continue;
        }
        let listed: Vec<String> = ids
            .iter()
            .map(|(id, name)| {
                if name.is_empty() {
                    format!("{id} (not detected)")
                } else {
                    format!("{id} = {name}")
                }
            })
            .collect();
        env.push((format!("devices ({preset})"), listed.join(", ")));
    }

    let meta = RunMeta {
        stamp: super::stamp_human(now),
        server_version: crate::server_version::probe().unwrap_or_default(),
        bench_build: None,
        exe: match plan.mode {
            Mode::Live => paths::llama_server_exe(),
            Mode::Synthetic => paths::llama_bench_exe(),
        }
        .map(|p| p.to_string_lossy().into_owned())
        .unwrap_or_default(),
        env,
        warnings,
    };

    let campaign = rec.campaign().to_string();
    let mut run = Run {
        plan: plan.clone(),
        meta,
        points: Vec::new(),
        flash_attn: std::collections::BTreeMap::new(),
        rec: Some(rec),
        resolved,
    };

    let outcome = match plan.mode {
        Mode::Live => run_live(&mut run, &cfg, &chosen, cancel, emit),
        Mode::Synthetic => run_synthetic(&mut run, &cfg, &chosen, cancel, emit),
    };

    // Read back off the rows rather than assumed from the presets: a preset that
    // pins nothing runs at `auto`, and what `auto` became is a property of the
    // backend, not of the INI.
    if let Some(w) = super::flash_attn_mismatch(&run.flash_attn) {
        run.meta.warnings.push(w);
    }
    run.close_db(&outcome);
    for (preset, fa) in &run.flash_attn {
        run.meta
            .env
            .push((format!("flash-attn ({preset})"), fa.clone()));
    }

    outcome?;
    // A sweep leg's campaign is the sweep's, whose report is the sweep digest.
    if plan.campaign.is_none() {
        let report = super::render_report(&run.plan, &run.points, &run.meta);
        let conn = super::db::open(&super::db::work_path())?;
        super::db::set_report(&conn, &campaign, &report).map_err(|e| format!("bench.db: {e}"))?;
    }
    Ok(campaign)
}

fn cancelled(cancel: &Arc<AtomicBool>) -> bool {
    cancel.load(Ordering::SeqCst)
}

// ── Live ─────────────────────────────────────────────────────────────────

fn run_live(
    run: &mut Run,
    cfg: &server_cfg::ServerConfig,
    chosen: &[presets::Preset],
    cancel: &Arc<AtomicBool>,
    emit: &impl Fn(Event),
) -> Result<(), String> {
    let port = u16::try_from(cfg.port_or_default()).map_err(|_| "invalid port in server.ini")?;
    let plan = run.plan.clone();

    for preset in chosen {
        if cancelled(cancel) {
            return Err("Cancelled.".into());
        }
        run.begin_preset(preset, cfg);
        emit(Event::Progress(format!(
            "{}: loading the model (warm-up request)",
            preset.id
        )));
        // Discarded on purpose, INCLUDING its error: a router that cannot warm
        // the model will fail the first real repetition too, with a message
        // about the actual request rather than about a warm-up.
        let warm = warmup_body(&preset.id);
        let _ = http::request(port, "POST", super::LIVE_PATH, Some(&warm), LIVE_TIMEOUT);

        let body = super::live_body(&preset.id, &plan);
        let mut prefill: Vec<f64> = Vec::new();
        let mut decode: Vec<f64> = Vec::new();
        let mut cache_seen: i64 = 0;
        let mut draft_n: i64 = 0;
        let mut draft_ok: i64 = 0;
        // The two decode phases, per repetition: (rate, span) pairs, kept only
        // for repetitions where the span has a rate.
        let mut reasoning: Vec<(f64, super::Phase)> = Vec::new();
        let mut answer: Vec<(f64, super::Phase)> = Vec::new();

        for rep in 1..=plan.reps {
            if cancelled(cancel) {
                return Err("Cancelled.".into());
            }
            emit(Event::Progress(format!(
                "{}: repetition {rep}/{}",
                preset.id, plan.reps
            )));
            let res = http::request(port, "POST", super::LIVE_PATH, Some(&body), LIVE_TIMEOUT)
                .map_err(|e| format!("{}: {e}", preset.id))?;
            if res.status != 200 {
                let body: String = res.body.chars().take(600).collect();
                return Err(format!(
                    "{}: llama-server answered HTTP {}: {body}",
                    preset.id, res.status
                ));
            }
            let s = super::parse_stream(&res.body).map_err(|e| format!("{}: {e}", preset.id))?;
            let t = &s.timings;
            prefill.push(t.prompt_tps);
            decode.push(t.predicted_tps);
            cache_seen = cache_seen.max(t.cache_n);
            draft_n += t.draft_n;
            draft_ok += t.draft_accepted;
            if let Some(split) = &s.split {
                if let Some(r) = split.reasoning.tps() {
                    reasoning.push((r, split.reasoning.clone()));
                }
                if let Some(a) = split.answer.tps() {
                    answer.push((a, split.answer.clone()));
                }
            }

            run.sample(Sample {
                rep: rep as i64,
                temperature: plan.temp,
                cache_prompt: Some(false),
                prompt_tokens: Some(t.prompt_n),
                prefill_tps: Some(t.prompt_tps),
                gen_tokens: Some(t.predicted_n),
                decode_tps: Some(t.predicted_tps),
                draft_gen: (t.draft_n > 0).then_some(t.draft_n),
                draft_acc: (t.draft_n > 0).then_some(t.draft_accepted),
                cache_n: Some(t.cache_n),
                reasoning_tps: s.split.as_ref().and_then(|p| p.reasoning.tps()),
                answer_tps: s.split.as_ref().and_then(|p| p.answer.tps()),
                detail: Some(
                    serde_json::json!({
                        "phases": s.split.as_ref().map(super::phases_json),
                        "trace": super::trace_json(&s.trace),
                    })
                    .to_string(),
                ),
                ..Default::default()
            });
            // After a REAL prefill, not the warm-up: the free memory the
            // probe reads then includes what the first long prompt grew.
            if rep == 1 {
                if let Some(rec) = &mut run.rec {
                    rec.read_memory();
                }
            }

            let (mean, sd) = super::mean_sd(&prefill);
            run.put(
                Point {
                    preset: preset.id.clone(),
                    test: super::LIVE_PREFILL.into(),
                    mean,
                    sd,
                    n: prefill.len() as i32,
                    // The one note that invalidates the number it hangs off:
                    // the request asked for no prefix reuse, so a cache hit
                    // means this row is not a cold prefill rate.
                    note: if cache_seen > 0 {
                        format!("{cache_seen} prompt tokens came from the cache")
                    } else {
                        String::new()
                    },
                },
                emit,
            );
            let (mean, sd) = super::mean_sd(&decode);
            run.put(
                Point {
                    preset: preset.id.clone(),
                    test: super::LIVE_DECODE.into(),
                    mean,
                    sd,
                    n: decode.len() as i32,
                    note: if draft_n > 0 {
                        format!(
                            "draft accepted {:.0}%",
                            100.0 * draft_ok as f64 / draft_n as f64
                        )
                    } else {
                        String::new()
                    },
                },
                emit,
            );
            for (test, spans) in [
                (super::LIVE_DECODE_REASONING, &reasoning),
                (super::LIVE_DECODE_ANSWER, &answer),
            ] {
                if let Some(point) = super::phase_point(&preset.id, test, spans) {
                    run.put(point, emit);
                }
            }
        }
    }
    Ok(())
}

/// The throwaway request that loads the model. A two-word prompt on purpose:
/// what needs warming is the model load, and prefilling the real prompt here
/// would double the run's cost for a number nobody reads.
fn warmup_body(preset_id: &str) -> String {
    serde_json::json!({
        "model": preset_id,
        "messages": [{ "role": "user", "content": "hi" }],
        "max_tokens": 1,
        "stream": false,
        "cache_prompt": false,
    })
    .to_string()
}

// ── Synthetic ────────────────────────────────────────────────────────────

fn run_synthetic(
    run: &mut Run,
    cfg: &server_cfg::ServerConfig,
    chosen: &[presets::Preset],
    cancel: &Arc<AtomicBool>,
    emit: &impl Fn(Event),
) -> Result<(), String> {
    let exe = paths::llama_bench_exe().ok_or_else(|| {
        format!(
            "{} not found next to llama-server. Build llama.cpp first, or \
             reinstall: the installer stages it into bin\\.",
            paths::binary_name("llama-bench")
        )
    })?;
    let plan = run.plan.clone();

    // Two invocations per preset (prefill, then decode): see
    // `bench::synthetic_sweeps` for why they are not one.
    let sweeps = super::synthetic_sweeps(&plan);
    for preset in chosen {
        run.begin_preset(preset, cfg);
        for sweep in &sweeps {
            if cancelled(cancel) {
                return Err("Cancelled.".into());
            }
            emit(Event::Progress(format!(
                "{}: llama-bench, {} sweep",
                preset.id, sweep.name
            )));
            let argv = super::synthetic_argv(preset, cfg, &plan, sweep);
            run.note(&format!(
                "\n--- {} / {} ---\n{} {}",
                preset.id,
                sweep.name,
                exe.display(),
                argv.join(" ")
            ));
            run_one(run, &exe, &argv, cfg, preset, cancel, emit)?;
        }
    }
    Ok(())
}

/// One llama-bench invocation, streamed.
#[allow(clippy::too_many_arguments)]
fn run_one(
    run: &mut Run,
    exe: &std::path::Path,
    argv: &[String],
    cfg: &server_cfg::ServerConfig,
    preset: &presets::Preset,
    cancel: &Arc<AtomicBool>,
    emit: &impl Fn(Event),
) -> Result<(), String> {
    let reps = run.plan.reps;
    let mut cmd = Command::new(exe);
    cmd.args(argv)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .stdin(Stdio::null());
    proc::hide_console(&mut cmd);
    proc::prepend_rocm_path(&mut cmd);
    // The same environment llama-server would get. ROCBLAS_USE_HIPBLASLT is the
    // one that decides whether a BF16/F16 model runs at all on gfx1201, so a
    // bench without it measures a configuration that cannot serve.
    for (k, v) in runstate::env_vars(cfg) {
        cmd.env(k, v);
    }

    let mut child = cmd
        .spawn()
        .map_err(|e| format!("cannot start llama-bench: {e}"))?;

    // stderr is llama-bench's own chatter (backend init, model load, the
    // --progress ticks). Tee'd to the run log on its own thread so a run in
    // progress can be watched and a run that dies leaves evidence.
    if let Some(err) = child.stderr.take() {
        // Into the held run's log, which travels with it into bench.db.
        let sink = run.rec.as_ref().and_then(Recorder::log_sink);
        std::thread::spawn(move || {
            for line in BufReader::new(err).lines().map_while(Result::ok) {
                if let Some(sink) = &sink {
                    super::record::push_line(sink, &line);
                }
            }
        });
    }
    // stdout carries the jsonl rows. Read on a thread so the loop below can
    // still notice a cancel while the pipe is quiet (a single test can take
    // minutes).
    let (tx, rx) = mpsc::channel::<String>();
    if let Some(out) = child.stdout.take() {
        std::thread::spawn(move || {
            for line in BufReader::new(out).lines().map_while(Result::ok) {
                if tx.send(line).is_err() {
                    break;
                }
            }
        });
    }

    let mut killed = false;
    loop {
        match rx.recv_timeout(CANCEL_POLL) {
            Ok(line) => {
                if run.meta.bench_build.is_none() {
                    run.meta.bench_build = super::parse_bench_build(&line);
                }
                if let Some(point) = super::parse_bench_line(&line, &preset.id, reps) {
                    // The row goes into bench.db VERBATIM beside the point
                    // derived from it (`record::bench_samples`): everything
                    // llama-bench measured that the point drops (flash_attn,
                    // n_batch, n_threads, backends, gpu_info) is unrecoverable
                    // otherwise.
                    for sample in super::record::bench_samples(&line) {
                        run.sample(sample);
                    }
                    if let Some(fa) = super::parse_bench_flash_attn(&line) {
                        run.flash_attn.insert(preset.id.clone(), fa);
                    }
                    emit(Event::Progress(format!("{}: {}", preset.id, point.test)));
                    run.put(point, emit);
                }
            }
            Err(mpsc::RecvTimeoutError::Timeout) => {
                if cancelled(cancel) {
                    let _ = child.kill();
                    killed = true;
                    break;
                }
            }
            Err(mpsc::RecvTimeoutError::Disconnected) => break,
        }
    }
    let status = child.wait().map_err(|e| format!("llama-bench: {e}"))?;
    if killed {
        return Err("Cancelled.".into());
    }
    if !status.success() {
        return Err(format!(
            "llama-bench exited with {status} on preset '{}'. Its last output:\n{}",
            preset.id,
            log_tail(run, 8)
        ));
    }
    Ok(())
}
