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
//! ## Everything is written as it lands
//!
//! A run is minutes of sustained GPU load, which on this class of machine is
//! precisely when a TDR or a bugcheck arrives. So each point is appended to the
//! jsonl the moment it is computed, llama-bench's stderr is tee'd to the run's
//! `.log` while it runs, and the markdown report is rendered from whatever was
//! collected, cancellation and failure included. A run that dies is a partial
//! result, never an empty file.
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

use std::io::{BufRead, BufReader, Write};
use std::path::PathBuf;
use std::process::{Command, Stdio};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc;
use std::sync::Arc;
use std::time::Duration;

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
    /// The run ended: the report path, or why it did not.
    Done(Result<PathBuf, String>),
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

/// Everything one run owns on disk, plus the accumulating results.
struct Run {
    plan: Plan,
    meta: RunMeta,
    jsonl: std::fs::File,
    log_path: PathBuf,
    md_path: PathBuf,
    points: Vec<Point>,
    /// The flash-attention setting each preset RESOLVED to, read back off
    /// llama-bench's rows. A preset that pins nothing runs at `auto`, which
    /// resolves per backend, so two legs of a comparison can silently land on
    /// different kernels; `flash_attn_mismatch` turns that into a warning at the
    /// top of the report.
    flash_attn: std::collections::BTreeMap<String, String>,
}

impl Run {
    /// Record (or revise) one point: into memory, onto disk, and out to the UI.
    fn put(&mut self, point: Point, emit: &impl Fn(Event)) {
        let _ = writeln!(self.jsonl, "{}", super::point_json(&point));
        let _ = self.jsonl.flush();
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

    fn raw(&mut self, line: &str) {
        let _ = writeln!(self.jsonl, "{line}");
    }

    fn note(&self, text: &str) {
        append_log(&self.log_path, text);
    }
}

fn append_log(path: &PathBuf, text: &str) {
    if let Ok(mut f) = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(path)
    {
        let _ = writeln!(f, "{text}");
    }
}

fn run(plan: &Plan, cancel: &Arc<AtomicBool>, emit: &impl Fn(Event)) -> Result<PathBuf, String> {
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
    let (jsonl_path, md_path, log_path) = super::run_paths(&stamp);

    // What each preset's device ids point at RIGHT NOW. Recorded because the ids
    // move with driver state, so "ROCm0" in a six-week-old report is not
    // necessarily the card you are comparing against today.
    let devs = crate::devices::probed();
    let resolved: Vec<(String, Vec<(String, String)>)> = chosen
        .iter()
        .map(|p| {
            let eff = super::effective(p, &cfg);
            (p.id.clone(), super::device_names(&eff.device, &devs))
        })
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

    let jsonl = std::fs::File::create(&jsonl_path)
        .map_err(|e| format!("cannot write {}: {e}", jsonl_path.display()))?;
    let mut run = Run {
        plan: plan.clone(),
        meta,
        jsonl,
        log_path,
        md_path,
        points: Vec::new(),
        flash_attn: std::collections::BTreeMap::new(),
    };
    let header = super::run_header_json(plan, &run.meta);
    run.raw(&header);
    run.note(&format!("=== {} ===", run.meta.stamp));

    // A failure past this point still writes the report: the rows already
    // collected are the result, and the message is what says the run is partial.
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
    for (preset, fa) in &run.flash_attn {
        run.meta
            .env
            .push((format!("flash-attn ({preset})"), fa.clone()));
    }

    let report = super::render_report(&run.plan, &run.points, &run.meta);
    std::fs::write(&run.md_path, &report)
        .map_err(|e| format!("cannot write {}: {e}", run.md_path.display()))?;

    match outcome {
        Ok(()) => Ok(run.md_path.clone()),
        Err(e) => Err(format!(
            "{e} (partial results saved to {})",
            run.md_path.display()
        )),
    }
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
                run.note(&format!("{} HTTP {}: {}", preset.id, res.status, res.body));
                return Err(format!(
                    "{}: llama-server answered HTTP {}. See the run log.",
                    preset.id, res.status
                ));
            }
            let t = super::parse_timings(&res.body).map_err(|e| format!("{}: {e}", preset.id))?;
            prefill.push(t.prompt_tps);
            decode.push(t.predicted_tps);
            cache_seen = cache_seen.max(t.cache_n);
            draft_n += t.draft_n;
            draft_ok += t.draft_accepted;

            let detail = serde_json::json!({
                "prompt_n": t.prompt_n,
                "predicted_n": t.predicted_n,
                "cache_n": t.cache_n,
                "draft_n": t.draft_n,
                "draft_n_accepted": t.draft_accepted,
                "draft_acceptance_pct": t.acceptance(),
            });
            let prefill_line = super::sample_json(
                &preset.id,
                super::LIVE_PREFILL,
                rep,
                t.prompt_tps,
                detail.clone(),
            );
            run.raw(&prefill_line);
            let decode_line =
                super::sample_json(&preset.id, super::LIVE_DECODE, rep, t.predicted_tps, detail);
            run.raw(&decode_line);

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
        "llama-bench.exe not found next to llama-server. Build llama.cpp first, or \
         reinstall: the installer stages it into bin\\."
            .to_string()
    })?;
    let plan = run.plan.clone();

    // Two invocations per preset (prefill, then decode): see
    // `bench::synthetic_sweeps` for why they are not one.
    let sweeps = super::synthetic_sweeps(&plan);
    for preset in chosen {
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
        let path = run.log_path.clone();
        std::thread::spawn(move || {
            for line in BufReader::new(err).lines().map_while(Result::ok) {
                append_log(&path, &line);
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
                    // The row goes to disk VERBATIM beside the point derived
                    // from it: everything llama-bench measured that the point
                    // drops (flash_attn, n_batch, n_threads, backends, gpu_info)
                    // is unrecoverable otherwise.
                    let raw = super::bench_row_json(&preset.id, &line);
                    run.raw(&raw);
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
            "llama-bench exited with {status} on preset '{}'. See the run log ({}).",
            preset.id,
            run.log_path.display()
        ));
    }
    Ok(())
}
