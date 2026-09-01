//! Unattended parameter sweeps: one benchmark leg per value of ONE preset key.
//!
//! The Benchmark tab measures a configuration; a sweep measures a QUESTION
//! about one ("how far does raising `spec-draft-n-max` keep paying?"). The
//! difference is only a loop, but that loop is why this lives in the CLI rather
//! than behind a button: a live leg is a model load plus `reps` cold prefills,
//! so a seven-value sweep is half an hour during which nothing else may touch
//! the machine. That is a thing to start from a terminal and walk away from.
//!
//! It is also why every leg writes the ordinary `bench-<stamp>.{jsonl,md,log}`
//! files the tab already produces and lists, with only the cross-leg summary
//! added on top: a sweep killed at the fourth value leaves four finished,
//! openable runs behind, not one truncated file. The summary is named
//! `sweep-<stamp>.*` precisely so it stays OUT of that listing, since
//! `bench::past_runs` matches `bench-*.jsonl` and `load_run` could not parse it.
//!
//! ## The key rides the CHILD's launch args, so a live leg must restart
//!
//! llama-server's router spawns one child per model and hands it the preset's
//! keys at spawn time; nothing re-reads presets.ini afterwards. A live sweep
//! therefore restarts the server between values, and the restart is deliberately
//! the bare `runstate::stop()` + `start()` pair rather than the one `control
//! restart` performs: that path saves and restores the conversation snapshot,
//! which on a model of this size is gigabytes of IO per leg to reinstate a KV
//! cache the benchmark then refuses to use (`cache_prompt: false`).
//!
//! The synthetic engine wants the opposite state (llama-server DOWN: a second
//! resident copy of the weights does not fail, it spills into shared memory and
//! reads as a backend regression) and needs no restart at all, since llama-bench
//! is a fresh process per leg and the leg builds its argv from presets.ini.
//!
//! ## `;` separates the values, because `,` cannot
//!
//! `device = Vulkan1,CUDA0` and `tensor-split = 60,6` are single values that
//! contain commas, so a comma-separated `--values` could never sweep them. A `;`
//! cannot appear in an INI value at all (`ini::reject_comment_markers`:
//! llama-server's preset reader cuts a value at the first `;` or `#`), which
//! makes it the one separator guaranteed not to collide with the data. So
//! `--values` splits on `;` when it sees one, and on `,` otherwise.
//!
//! ## presets.ini is edited, and says so before it is
//!
//! A sweep writes the key it sweeps, so an interrupted sweep leaves the preset
//! on whatever value it was measuring. The original is written into the
//! summary's FIRST jsonl line before any leg runs, which is why that line exists
//! at all: a machine that reboots mid-sweep still has the value to put back
//! recorded on disk.

use std::io::Write;
use std::path::PathBuf;
use std::sync::mpsc;
use std::time::{Duration, Instant};

use super::exec::Event;
use super::{Mode, Plan, Point};
use crate::{http, ini, paths, presets, runstate, server_cfg};

/// The value that means "the key is absent from the section". Spelled rather
/// than left blank so it survives a round trip through a comma-separated list
/// and reads as a deliberate choice in the summary table.
pub const UNSET: &str = "unset";

/// How long to wait for llama-server to vanish after a stop, and to answer after
/// a start. The stop is a `taskkill /f`, so the wait is only for the handles to
/// close; the start binds the router's socket before any model is loaded, which
/// is the thing polled for here (the model itself loads on the leg's warm-up
/// request, under the benchmark's own generous timeout).
const STOP_TIMEOUT: Duration = Duration::from_secs(60);
const READY_TIMEOUT: Duration = Duration::from_secs(120);
const POLL: Duration = Duration::from_millis(500);

/// A short read timeout for the readiness probe: `/v1/models` is answered off
/// the router's own state, so a router that needs seconds for it is not up yet.
const PROBE_TIMEOUT: Duration = Duration::from_secs(5);

// ── What may be swept ────────────────────────────────────────────────────

type Setter = fn(&mut presets::Preset, &str) -> Result<(), String>;

/// The preset keys a sweep may vary, each with the typed field it writes.
///
/// A table and not a raw `ini::replace_key` for two reasons. A typo would
/// otherwise silently add a key llama-server does not know (its preset reader
/// only warns), and the leg would then measure the unchanged configuration and
/// report it as a data point. And going through the typed field means the value
/// is parsed HERE, before the sweep spends an hour of GPU time, and the section
/// is rewritten by `presets::save`, so the file keeps the comments the GUI
/// writes rather than growing a bare key.
///
/// The list is the launch-affecting knobs: things a benchmark can actually
/// resolve. Sampler and reasoning keys are deliberately absent (the live engine
/// overrides the temperature anyway, and the synthetic one ignores both).
pub const SWEEPABLE: &[(&str, Setter)] = &[
    // Speculative decoding.
    ("spec-draft-n-max", |p, v| {
        p.spec_draft_n_max = int(v)?;
        Ok(())
    }),
    ("spec-type", |p, v| {
        p.spec_type = text(v)?;
        Ok(())
    }),
    ("model-draft", |p, v| {
        p.model_draft = text(v)?;
        Ok(())
    }),
    ("device-draft", |p, v| {
        p.device_draft = text(v)?;
        Ok(())
    }),
    ("n-gpu-layers-draft", |p, v| {
        p.n_gpu_layers_draft = int(v)?;
        Ok(())
    }),
    ("spec-draft-type-k", |p, v| {
        p.spec_draft_type_k = text(v)?;
        Ok(())
    }),
    ("spec-draft-type-v", |p, v| {
        p.spec_draft_type_v = text(v)?;
        Ok(())
    }),
    // Placement.
    ("device", |p, v| {
        p.device = text(v)?;
        Ok(())
    }),
    ("tensor-split", |p, v| {
        p.tensor_split = text(v)?;
        Ok(())
    }),
    ("split-mode", |p, v| {
        p.split_mode = text(v)?;
        Ok(())
    }),
    ("override-tensor", |p, v| {
        p.override_tensor = text(v)?;
        Ok(())
    }),
    ("n-gpu-layers", |p, v| {
        p.n_gpu_layers = int(v)?;
        Ok(())
    }),
    ("n-cpu-moe", |p, v| {
        p.n_cpu_moe = int(v)?;
        Ok(())
    }),
    // Context, batching and cache.
    ("ctx-size", |p, v| {
        p.ctx_size = int(v)?;
        Ok(())
    }),
    ("batch-size", |p, v| {
        p.batch_size = int(v)?;
        Ok(())
    }),
    ("ubatch-size", |p, v| {
        p.ubatch_size = int(v)?;
        Ok(())
    }),
    ("parallel", |p, v| {
        p.parallel = int(v)?;
        Ok(())
    }),
    ("cache-ram", |p, v| {
        p.cache_ram = int(v)?;
        Ok(())
    }),
    ("cache-type-k", |p, v| {
        p.cache_type_k = text(v)?;
        Ok(())
    }),
    ("cache-type-v", |p, v| {
        p.cache_type_v = text(v)?;
        Ok(())
    }),
    ("flash-attn", |p, v| {
        p.flash_attn = boolean(v)?;
        Ok(())
    }),
    ("mmproj-offload", |p, v| {
        p.mmproj_offload = boolean(v)?;
        Ok(())
    }),
];

/// The setter for `key`, or an error naming every key that has one.
pub fn setter(key: &str) -> Result<Setter, String> {
    SWEEPABLE
        .iter()
        .find(|(k, _)| *k == key)
        .map(|(_, f)| *f)
        .ok_or_else(|| format!("`{key}` is not a sweepable preset key. Sweepable: {}", keys()))
}

/// The sweepable keys, comma-separated, for help and error text.
pub fn keys() -> String {
    SWEEPABLE
        .iter()
        .map(|(k, _)| *k)
        .collect::<Vec<_>>()
        .join(", ")
}

fn int(v: &str) -> Result<Option<i32>, String> {
    if v.eq_ignore_ascii_case(UNSET) {
        return Ok(None);
    }
    v.parse::<i32>()
        .map(Some)
        .map_err(|_| format!("`{v}` is not a whole number (or `{UNSET}`)"))
}

fn boolean(v: &str) -> Result<Option<bool>, String> {
    if v.eq_ignore_ascii_case(UNSET) {
        return Ok(None);
    }
    match v.to_ascii_lowercase().as_str() {
        "true" | "on" | "yes" | "1" => Ok(Some(true)),
        "false" | "off" | "no" | "0" => Ok(Some(false)),
        _ => Err(format!("`{v}` is not a boolean (true/false, or `{UNSET}`)")),
    }
}

fn text(v: &str) -> Result<String, String> {
    if v.eq_ignore_ascii_case(UNSET) {
        return Ok(String::new());
    }
    // The same gate `presets::save` applies, run here so a bad value fails
    // before the sweep starts rather than on the leg that writes it.
    ini::reject_comment_markers("value", v).map_err(|e| e.to_string())?;
    Ok(v.to_string())
}

// ── Values ───────────────────────────────────────────────────────────────

/// Expand `--values` into the list of values to measure, in order.
///
/// Splits on `;` when the spec contains one and on `,` otherwise (see the module
/// header), and expands an `a..b` token into the inclusive integer range, which
/// is the shape most sweeps here have.
pub fn parse_values(spec: &str) -> Result<Vec<String>, String> {
    let sep = if spec.contains(';') { ';' } else { ',' };
    let mut out: Vec<String> = Vec::new();
    for token in spec.split(sep) {
        let token = token.trim();
        if token.is_empty() {
            continue;
        }
        expand(token, &mut out)?;
    }
    if out.is_empty() {
        return Err("--values is empty: there is nothing to sweep.".into());
    }
    Ok(out)
}

/// Cap on one `a..b` token. A range past this is a typo far more often than an
/// intent, and the intent can still be spelled out value by value.
const MAX_RANGE: i64 = 64;

fn expand(token: &str, out: &mut Vec<String>) -> Result<(), String> {
    let range = token.split_once("..").and_then(|(a, b)| {
        let lo = a.trim().parse::<i64>().ok()?;
        let hi = b.trim().parse::<i64>().ok()?;
        Some((lo, hi))
    });
    match range {
        Some((lo, hi)) if lo > hi => Err(format!("range `{token}` runs backwards")),
        Some((lo, hi)) if hi - lo + 1 > MAX_RANGE => Err(format!(
            "range `{token}` is {} values; list them explicitly if that is meant",
            hi - lo + 1
        )),
        Some((lo, hi)) => {
            out.extend((lo..=hi).map(|n| n.to_string()));
            Ok(())
        }
        None => {
            out.push(token.to_string());
            Ok(())
        }
    }
}

// ── Plan ─────────────────────────────────────────────────────────────────

/// What the preset is left on when the sweep finishes.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Apply {
    /// Put back the value the preset had before the sweep (the default: a
    /// measurement should not silently reconfigure the machine).
    Original,
    /// Keep the value that won on the primary test.
    Best,
    /// Keep the last value measured.
    Last,
}

impl Apply {
    pub fn from_str(s: &str) -> Apply {
        match s {
            "best" => Apply::Best,
            "last" => Apply::Last,
            _ => Apply::Original,
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Apply::Original => "original",
            Apply::Best => "best",
            Apply::Last => "last",
        }
    }
}

/// One sweep as the CLI describes it. `plan.presets` is ignored: a sweep varies
/// one key of ONE preset, so the leg's plan is built from `preset` here.
#[derive(Clone, Debug)]
pub struct Opts {
    pub preset: String,
    pub key: String,
    pub values: Vec<String>,
    pub plan: Plan,
    pub apply: Apply,
    /// What `--apply original` puts back, when the value in presets.ini is not
    /// it. The case this exists for: an interrupted sweep left the key on one of
    /// its leg values, so "the value it had before" is no longer readable from
    /// the file (it is in that sweep's jsonl header, which the warning names).
    pub restore_to: Option<String>,
}

/// One measured value: the leg's points, where its report landed, and what went
/// wrong if something did. A failed leg is kept rather than dropped: "this value
/// does not load" is a result, and a summary that omits it reads as if the value
/// was never tried.
#[derive(Clone, Debug, Default)]
pub struct Leg {
    pub value: String,
    pub points: Vec<Point>,
    /// File name (not path) of the leg's own report, empty when it wrote none.
    pub report: String,
    pub error: String,
    pub secs: u64,
}

impl Leg {
    fn point(&self, test: &str) -> Option<&Point> {
        self.points.iter().find(|p| p.test == test)
    }
}

/// The test whose column decides the winner: `decode` live, the first `tg` row
/// synthetic (prefill is dominated by the batch, not by the knobs worth
/// sweeping, and a sweep with no decode row at all has nothing else to rank by).
pub fn primary_test(mode: Mode, legs: &[Leg]) -> String {
    if mode == Mode::Live {
        return super::LIVE_DECODE.to_string();
    }
    for leg in legs {
        for p in &leg.points {
            if p.test.starts_with("tg") {
                return p.test.clone();
            }
        }
    }
    legs.iter()
        .flat_map(|l| l.points.first())
        .map(|p| p.test.clone())
        .next()
        .unwrap_or_default()
}

/// Index of the leg with the highest mean on `test`, if any leg measured it.
pub fn best(legs: &[Leg], test: &str) -> Option<usize> {
    legs.iter()
        .enumerate()
        .filter_map(|(i, l)| l.point(test).map(|p| (i, p.mean)))
        .fold(None, |acc: Option<(usize, f64)>, (i, mean)| match acc {
            Some((_, best)) if best >= mean => acc,
            _ => Some((i, mean)),
        })
        .map(|(i, _)| i)
}

// ── Summary report ───────────────────────────────────────────────────────

/// Everything the summary needs that the legs do not carry.
pub struct Summary<'a> {
    pub opts: &'a Opts,
    pub legs: &'a [Leg],
    pub original: &'a str,
    pub applied: &'a str,
    pub stamp: &'a str,
    pub server_version: &'a str,
}

/// Test labels in column order: the primary first, then the rest as they first
/// appear.
fn test_labels(legs: &[Leg], primary: &str) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    if !primary.is_empty() {
        out.push(primary.to_string());
    }
    for leg in legs {
        for p in &leg.points {
            if !out.contains(&p.test) {
                out.push(p.test.clone());
            }
        }
    }
    out
}

/// Render the cross-leg summary: one row per value, the winner spelled out
/// below it.
pub fn render_summary(s: &Summary) -> String {
    let opts = s.opts;
    let primary = primary_test(opts.plan.mode, s.legs);
    let labels = test_labels(s.legs, &primary);
    let engine = match opts.plan.mode {
        Mode::Live => "live (llama-server)",
        Mode::Synthetic => "synthetic (llama-bench)",
    };

    let mut md = format!(
        "# Benchmark sweep: {} on {}\n\nGenerated {}.\n\n",
        opts.key, opts.preset, s.stamp
    );

    md.push_str("| setting | value |\n|---|---|\n");
    md.push_str(&format!("| preset | {} |\n", opts.preset));
    md.push_str(&format!("| key | {} |\n", opts.key));
    md.push_str(&format!("| values | {} |\n", opts.values.join(", ")));
    md.push_str(&format!("| engine | {engine} |\n"));
    md.push_str(&format!("| repetitions | {} |\n", opts.plan.reps));
    if opts.plan.mode == Mode::Live {
        md.push_str(&format!(
            "| temperature | {} |\n",
            match opts.plan.temp {
                Some(t) => format!("{t}"),
                None => "the preset's own".to_string(),
            }
        ));
        md.push_str(&format!(
            "| max output tokens | {} |\n",
            opts.plan.max_tokens
        ));
        md.push_str(&format!(
            "| prompt | {} chars |\n",
            opts.plan.prompt.chars().count()
        ));
    }
    md.push_str(&format!("| {} before the sweep | {} |\n", opts.key, s.original));
    md.push_str(&format!("| {} after it | {} |\n", opts.key, s.applied));
    if !s.server_version.is_empty() {
        md.push_str(&format!("| llama-server | {} |\n", s.server_version));
    }
    md.push('\n');

    let failed = s.legs.iter().filter(|l| !l.error.is_empty()).count();
    if failed > 0 {
        md.push_str(&format!(
            "> **WARNING** {failed} of {} legs failed; their rows carry the error instead of a rate.\n\n",
            s.legs.len()
        ));
    }

    md.push_str("## Results (tokens/s, mean +- sample stddev)\n\n");
    if s.legs.is_empty() {
        md.push_str("No legs ran.\n\n");
        return md;
    }

    let baseline = s
        .legs
        .first()
        .and_then(|l| l.point(&primary))
        .map(|p| p.mean);

    let mut head = format!("| {} |", opts.key);
    let mut sep = String::from("|---|");
    for (i, label) in labels.iter().enumerate() {
        head.push_str(&format!(" {label} | +- |"));
        sep.push_str("---:|---:|");
        if i == 0 {
            head.push_str(&format!(" vs {} |", s.legs[0].value));
            sep.push_str("---:|");
        }
    }
    head.push_str(" note | run |");
    sep.push_str("---|---|");
    md.push_str(&head);
    md.push('\n');
    md.push_str(&sep);
    md.push('\n');

    for leg in s.legs {
        let mut row = format!("| {} |", leg.value);
        for (i, label) in labels.iter().enumerate() {
            match leg.point(label) {
                Some(p) => row.push_str(&format!(" {:.1} | {:.1} |", p.mean, p.sd)),
                None => row.push_str(" - | - |"),
            }
            if i == 0 {
                let ratio = match (baseline, leg.point(label)) {
                    (Some(b), Some(p)) if b > 0.0 => format!("{:.2}x", p.mean / b),
                    _ => "-".to_string(),
                };
                row.push_str(&format!(" {ratio} |"));
            }
        }
        let note = if leg.error.is_empty() {
            leg.point(&primary)
                .map(|p| p.note.clone())
                .unwrap_or_default()
        } else {
            format!("FAILED: {}", leg.error)
        };
        let run = if leg.report.is_empty() {
            "-".to_string()
        } else {
            format!("`{}`", leg.report)
        };
        row.push_str(&format!(" {note} | {run} |"));
        md.push_str(&row);
        md.push('\n');
    }
    md.push('\n');

    if let Some(i) = best(s.legs, &primary) {
        let leg = &s.legs[i];
        let mean = leg.point(&primary).map(|p| p.mean).unwrap_or_default();
        let gain = match baseline {
            Some(b) if b > 0.0 => format!(", {:.2}x the first value", mean / b),
            _ => String::new(),
        };
        md.push_str(&format!(
            "**Best: {} = {}** at {mean:.1} t/s {primary}{gain}.\n\n",
            opts.key, leg.value
        ));
    }

    md.push_str("## Caveats\n\n");
    for c in sweep_caveats(opts.plan.mode) {
        md.push_str(&format!("- {c}\n"));
    }
    for c in super::caveats(opts.plan.mode) {
        md.push_str(&format!("- {c}\n"));
    }
    md
}

/// What a sweep adds to the mode's own caveats: everything about measuring one
/// key by moving it that a single run cannot mislead you about.
pub fn sweep_caveats(mode: Mode) -> Vec<&'static str> {
    let mut out = vec![
        "Legs ran in the order listed, so a card that heats up over a long sweep charges the drift to the LAST values. Re-run with the order reversed when two values are close.",
        "Only the swept key moved; every other key came from presets.ini as it stood when the sweep started, and a hand edit made while it ran is not reflected in the rows above it.",
        "Each leg's own report (the `run` column) carries the full environment block, the device ids resolved by name and the raw repetitions. This table is a digest of them, not a replacement.",
    ];
    if mode == Mode::Live {
        out.push(
            "Every leg restarted llama-server without saving or restoring the conversation snapshot, so the numbers are cold-start numbers and no leg inherited another's KV cache.",
        );
    }
    out
}

// ── Where a sweep writes ─────────────────────────────────────────────────

/// The two files a sweep writes on top of its legs'. Named `sweep-` and not
/// `bench-` so the Benchmark tab's saved-runs list (which globs `bench-*.jsonl`
/// and parses every line) never tries to load one.
pub fn sweep_paths(stamp: &str) -> (PathBuf, PathBuf) {
    let dir = super::bench_dir();
    (
        dir.join(format!("sweep-{stamp}.jsonl")),
        dir.join(format!("sweep-{stamp}.md")),
    )
}

/// The first line written, BEFORE any leg runs: it carries the value to restore
/// if the sweep never reaches its own cleanup.
pub fn header_json(opts: &Opts, original: &str, stamp: &str) -> String {
    serde_json::json!({
        "kind": "sweep",
        "stamp": stamp,
        "mode": opts.plan.mode.as_str(),
        "preset": opts.preset,
        "key": opts.key,
        "values": opts.values,
        "restore": original,
        "apply": opts.apply.as_str(),
        "reps": opts.plan.reps,
    })
    .to_string()
}

/// The line that CLOSES a sweep file, written after the key is put back.
///
/// Its absence is the signal: a file with a header and no end line is a sweep
/// that was interrupted, which means presets.ini is still on one of its leg
/// values. `interrupted` reads exactly that, because the next sweep would
/// otherwise record the leftover as the value to restore and put THAT back,
/// quietly promoting a half-measured configuration to the permanent one.
pub fn end_json(applied: &str, legs: usize) -> String {
    serde_json::json!({ "kind": "end", "applied": applied, "legs": legs }).to_string()
}

/// What a sweep file says about itself.
pub struct State {
    pub preset: String,
    pub key: String,
    pub restore: String,
    /// The end line is there. It is the primary signal and the only one this
    /// pure half can see; `interrupted` adds a second, the summary report next
    /// to the file, which is the last thing a sweep writes and so covers a file
    /// written before the end line existed.
    pub closed_line: bool,
}

/// Read a sweep file's header and whether it was closed. `None` when the first
/// line is not a sweep header (an empty or truncated file).
pub fn sweep_state(content: &str) -> Option<State> {
    let mut lines = content.lines();
    let header: serde_json::Value = serde_json::from_str(lines.next()?.trim()).ok()?;
    if header.get("kind")?.as_str()? != "sweep" {
        return None;
    }
    Some(State {
        preset: header["preset"].as_str().unwrap_or_default().to_string(),
        key: header["key"].as_str().unwrap_or_default().to_string(),
        restore: header["restore"].as_str().unwrap_or_default().to_string(),
        closed_line: lines.any(|l| l.contains("\"kind\":\"end\"")),
    })
}

/// The newest sweep of this preset and key, when it was interrupted: its file
/// and the value it recorded to restore. A newer FINISHED sweep of the same pair
/// clears the warning, which is why the newest match decides rather than the
/// newest unfinished one.
fn interrupted(preset: &str, key: &str) -> Option<(PathBuf, String)> {
    let mut files: Vec<PathBuf> = std::fs::read_dir(super::bench_dir())
        .ok()?
        .flatten()
        .map(|e| e.path())
        .filter(|p| {
            p.file_name()
                .and_then(|n| n.to_str())
                .is_some_and(|n| n.starts_with("sweep-") && n.ends_with(".jsonl"))
        })
        .collect();
    // The stamp is `yyyymmdd-hhmmss`, so a lexical sort IS chronological.
    files.sort();
    for path in files.iter().rev() {
        let Ok(content) = std::fs::read_to_string(path) else {
            continue;
        };
        let Some(state) = sweep_state(&content) else {
            continue;
        };
        if state.preset != preset || state.key != key {
            continue;
        }
        // The report is written last, after the key is put back, so its
        // presence closes a file whose end line is missing because the build
        // that wrote it did not have one yet.
        let closed = state.closed_line || path.with_extension("md").is_file();
        return (!closed).then(|| (path.clone(), state.restore));
    }
    None
}

/// One finished leg, appended as it lands.
pub fn leg_json(leg: &Leg) -> String {
    let points: Vec<serde_json::Value> = leg
        .points
        .iter()
        .map(|p| {
            serde_json::json!({
                "test": p.test,
                "mean": p.mean,
                "sd": p.sd,
                "n": p.n,
                "note": p.note,
            })
        })
        .collect();
    serde_json::json!({
        "kind": "leg",
        "value": leg.value,
        "points": points,
        "report": leg.report,
        "error": leg.error,
        "secs": leg.secs,
    })
    .to_string()
}

// ── The run ──────────────────────────────────────────────────────────────

/// Run the sweep, writing each leg's report as it finishes and the summary at
/// the end. Returns the summary's path; an `Err` still means the summary was
/// written (its message says where), the way a partial benchmark run does.
pub fn run(opts: &Opts) -> Result<PathBuf, String> {
    let set = setter(&opts.key)?;
    let base = load_one(&opts.preset)?;
    // Every value is applied to a throwaway copy up front: a typo in the last of
    // seven values is worth catching now, not forty minutes from now.
    for v in &opts.values {
        let mut probe = base.clone();
        set(&mut probe, v).map_err(|e| format!("value `{v}`: {e}"))?;
    }
    if let Some(problem) = super::validate(&leg_plan(opts, &opts.preset)) {
        return Err(problem);
    }

    let original = match &opts.restore_to {
        Some(v) => v.clone(),
        None => current_value(&opts.preset, &opts.key),
    };
    let was_running = runstate::is_running();
    let port = u16::try_from(server_cfg::load().port_or_default())
        .map_err(|_| "invalid port in server.ini".to_string())?;

    // BEFORE this run's own file exists, and that ordering is the whole
    // correctness of the check: `interrupted` answers with the NEWEST sweep of
    // this preset and key, which the moment the file below is created is this
    // one, carrying neither its end line nor its report yet. Scanning after
    // creating it therefore reports every sweep as interrupted (v1.13.0 did)
    // and, worse, hides a real leftover behind the current file.
    let leftover = match opts.restore_to {
        Some(_) => None,
        None => interrupted(&opts.preset, &opts.key),
    };

    let stamp = super::stamp(super::now_secs());
    let dir = super::bench_dir();
    std::fs::create_dir_all(&dir).map_err(|e| format!("cannot create {}: {e}", dir.display()))?;
    let (jsonl_path, md_path) = sweep_paths(&stamp);
    let mut jsonl = std::fs::File::create(&jsonl_path)
        .map_err(|e| format!("cannot write {}: {e}", jsonl_path.display()))?;
    let _ = writeln!(jsonl, "{}", header_json(opts, &original, &stamp));
    let _ = jsonl.flush();

    println!(
        "Sweeping {} = {} on preset {} ({} engine, {} repetition{}).",
        opts.key,
        opts.values.join(", "),
        opts.preset,
        opts.plan.mode.as_str(),
        opts.plan.reps,
        if opts.plan.reps == 1 { "" } else { "s" }
    );
    println!(
        "presets.ini is being edited; `{} = {original}` is restored at the end \
         (also recorded in {}).",
        opts.key,
        jsonl_path.display()
    );
    // An interrupted sweep left the key on one of ITS leg values, so the value
    // read out of presets.ini above is that leftover and not the configuration
    // this machine ran. Restoring it would quietly promote a half-measured
    // value to the permanent one, so say so while there is still time to stop.
    if let Some((path, restore)) = &leftover {
        println!(
            "WARNING: {} never finished, so `{} = {original}` may be ITS leftover. \
             That sweep recorded `{restore}` as the value to put back; \
             pass --restore-to {restore} to restore that instead.",
            path.display(),
            opts.key
        );
    }

    let mut legs: Vec<Leg> = Vec::new();
    for (i, value) in opts.values.iter().enumerate() {
        println!(
            "\n[{}/{}] {} = {value}",
            i + 1,
            opts.values.len(),
            opts.key
        );
        let started = Instant::now();
        let leg = match write_value(&opts.preset, set, value)
            .and_then(|()| prepare_server(opts.plan.mode, port))
        {
            Ok(()) => measure(opts, value),
            Err(e) => {
                println!("    failed: {e}");
                Leg {
                    value: value.clone(),
                    error: e,
                    ..Default::default()
                }
            }
        };
        let leg = Leg {
            secs: started.elapsed().as_secs(),
            ..leg
        };
        let _ = writeln!(jsonl, "{}", leg_json(&leg));
        let _ = jsonl.flush();
        report_leg(&leg, opts.plan.mode);
        legs.push(leg);
    }

    // Restore: the preset first, then the process, so a running llama-server is
    // never left on a value presets.ini no longer names.
    let primary = primary_test(opts.plan.mode, &legs);
    let applied = match opts.apply {
        Apply::Original => original.clone(),
        Apply::Last => legs
            .last()
            .map(|l| l.value.clone())
            .unwrap_or_else(|| original.clone()),
        Apply::Best => best(&legs, &primary)
            .map(|i| legs[i].value.clone())
            .unwrap_or_else(|| original.clone()),
    };
    let restore = write_value(&opts.preset, set, &applied);
    let server = restore_server(was_running);
    // Closes the file, which is what tells the NEXT sweep that presets.ini is
    // not holding a leg value of this one. Written only after the restore, so
    // a sweep that died putting the key back still reads as interrupted.
    if restore.is_ok() {
        let _ = writeln!(jsonl, "{}", end_json(&applied, legs.len()));
        let _ = jsonl.flush();
    }
    println!("\n{} = {applied} (presets.ini).", opts.key);

    let summary = render_summary(&Summary {
        opts,
        legs: &legs,
        original: &original,
        applied: &applied,
        stamp: &super::stamp_human(super::now_secs()),
        server_version: &crate::server_version::probe().unwrap_or_default(),
    });
    std::fs::write(&md_path, &summary)
        .map_err(|e| format!("cannot write {}: {e}", md_path.display()))?;
    println!("Summary: {}", md_path.display());

    let mut problems: Vec<String> = legs
        .iter()
        .filter(|l| !l.error.is_empty())
        .map(|l| format!("{} = {}: {}", opts.key, l.value, l.error))
        .collect();
    if let Err(e) = restore {
        problems.push(format!("could not restore {}: {e}", opts.key));
    }
    if let Err(e) = server {
        problems.push(e);
    }
    if problems.is_empty() {
        Ok(md_path)
    } else {
        Err(format!(
            "{} (summary written to {})",
            problems.join("; "),
            md_path.display()
        ))
    }
}

/// The plan one leg runs: the caller's, pinned to the swept preset.
fn leg_plan(opts: &Opts, preset: &str) -> Plan {
    Plan {
        presets: vec![preset.to_string()],
        ..opts.plan.clone()
    }
}

fn load_one(id: &str) -> Result<presets::Preset, String> {
    let all = presets::load_all();
    all.iter()
        .find(|p| p.id == id)
        .cloned()
        .ok_or_else(|| {
            let known: Vec<&str> = all.iter().map(|p| p.id.as_str()).collect();
            format!(
                "no preset `{id}` in presets.ini. Known presets: {}",
                known.join(", ")
            )
        })
}

/// The key's value as the file has it right now, or `unset` when the section
/// does not carry it. Read from the INI rather than from the parsed preset so
/// the restore puts back exactly what was there.
fn current_value(preset: &str, key: &str) -> String {
    ini::read_section(&paths::presets_ini(), preset)
        .get(key)
        .map(|v| v.trim().to_string())
        .filter(|v| !v.is_empty())
        .unwrap_or_else(|| UNSET.to_string())
}

fn write_value(preset: &str, set: Setter, value: &str) -> Result<(), String> {
    // Re-read rather than mutate one cached copy: the sweep may run for an hour
    // and the rest of the section belongs to whoever edited it last.
    let mut p = load_one(preset)?;
    set(&mut p, value)?;
    presets::save(&p).map_err(|e| format!("cannot write presets.ini: {e}"))
}

/// Put llama-server into the state this engine needs: up (and freshly started,
/// so it reads the value just written) for live, down for synthetic.
fn prepare_server(mode: Mode, port: u16) -> Result<(), String> {
    match mode {
        Mode::Live => {
            runstate::stop();
            wait_gone()?;
            // Bare start, not the `control restart` path: no snapshot save or
            // restore, see the module header.
            runstate::start().map_err(|e| format!("cannot start llama-server: {e}"))?;
            wait_ready(port)
        }
        Mode::Synthetic => {
            if runstate::is_running() {
                runstate::stop();
                wait_gone()?;
            }
            Ok(())
        }
    }
}

/// Leave the machine as the sweep found it: llama-server running (on the value
/// presets.ini now names) when it was running before, stopped when it was not.
fn restore_server(was_running: bool) -> Result<(), String> {
    if was_running {
        runstate::stop();
        wait_gone()?;
        runstate::start().map_err(|e| format!("cannot restart llama-server: {e}"))?;
        return Ok(());
    }
    if runstate::is_running() {
        runstate::stop();
        wait_gone()?;
    }
    Ok(())
}

fn wait_gone() -> Result<(), String> {
    let deadline = Instant::now() + STOP_TIMEOUT;
    while runstate::is_running() {
        if Instant::now() >= deadline {
            return Err("llama-server did not stop".into());
        }
        std::thread::sleep(POLL);
    }
    Ok(())
}

/// Wait for the router's socket, not for a model: children are spawned on the
/// first request, which the leg's own warm-up makes.
fn wait_ready(port: u16) -> Result<(), String> {
    let deadline = Instant::now() + READY_TIMEOUT;
    loop {
        if let Ok(res) = http::request(port, "GET", "/v1/models", None, PROBE_TIMEOUT) {
            if res.status == 200 {
                return Ok(());
            }
        }
        if Instant::now() >= deadline {
            return Err(format!(
                "llama-server did not answer on port {port} within {} s",
                READY_TIMEOUT.as_secs()
            ));
        }
        std::thread::sleep(POLL);
    }
}

/// Run one leg through the ordinary benchmark engine and collect what it
/// produced. Blocks until the worker thread is done: the sweep is sequential by
/// nature, two legs at once would measure each other.
fn measure(opts: &Opts, value: &str) -> Leg {
    let (tx, rx) = mpsc::channel();
    let _handle = super::exec::spawn(leg_plan(opts, &opts.preset), move |ev| {
        let _ = tx.send(ev);
    });

    let mut points: Vec<Point> = Vec::new();
    let mut outcome: Option<Result<PathBuf, String>> = None;
    // Ends when the worker drops its sender, which it does after emitting Done.
    for ev in rx {
        match ev {
            Event::Progress(msg) => println!("    {msg}"),
            Event::Points(p) => points = p,
            Event::Done(r) => outcome = Some(r),
        }
    }

    let (report, error) = match outcome {
        Some(Ok(path)) => (file_name(&path), String::new()),
        // A failed run still writes its partial report, and the message says so.
        Some(Err(e)) => (String::new(), e),
        None => (String::new(), "the benchmark worker ended silently".into()),
    };
    Leg {
        value: value.to_string(),
        points,
        report,
        error,
        secs: 0,
    }
}

fn file_name(path: &std::path::Path) -> String {
    path.file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_default()
}

fn report_leg(leg: &Leg, mode: Mode) {
    if !leg.error.is_empty() {
        println!("    -> failed after {} s: {}", leg.secs, leg.error);
        return;
    }
    let primary = primary_test(mode, std::slice::from_ref(leg));
    match leg.point(&primary) {
        Some(p) => println!(
            "    -> {primary} {:.1} t/s (+- {:.1}, n={}) in {} s{}",
            p.mean,
            p.sd,
            p.n,
            leg.secs,
            if p.note.is_empty() {
                String::new()
            } else {
                format!(", {}", p.note)
            }
        ),
        None => println!("    -> no {primary} row in {} s", leg.secs),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn leg(value: &str, decode: f64, note: &str) -> Leg {
        Leg {
            value: value.into(),
            points: vec![Point {
                preset: "P".into(),
                test: super::super::LIVE_DECODE.into(),
                mean: decode,
                sd: 0.5,
                n: 3,
                note: note.into(),
            }],
            report: format!("bench-{value}.md"),
            error: String::new(),
            secs: 300,
        }
    }

    /// Does the rendered section actually EMIT the key, on a line of its own?
    /// A plain `contains` cannot tell: `render_section` documents each field in
    /// a comment above it, and those comments spell the key out with its `=`
    /// ("; spec-draft-n-max = max drafted tokens per step"), so a setter that
    /// did nothing at all would still pass a substring check.
    fn emits(ini: &str, key: &str) -> bool {
        let head = format!("{key} = ");
        ini.lines().any(|l| {
            let line = l.trim_start();
            !line.starts_with(';') && line.starts_with(&head)
        })
    }

    fn opts(values: &[&str]) -> Opts {
        Opts {
            preset: "Qwen".into(),
            key: "spec-draft-n-max".into(),
            values: values.iter().map(|v| (*v).to_string()).collect(),
            plan: Plan::default(),
            apply: Apply::Original,
            restore_to: None,
        }
    }

    #[test]
    fn values_split_on_commas_and_expand_ranges() {
        assert_eq!(parse_values("2,3, 4").unwrap(), ["2", "3", "4"]);
        assert_eq!(parse_values("2..5").unwrap(), ["2", "3", "4", "5"]);
        assert_eq!(parse_values("1,3..5,9").unwrap(), ["1", "3", "4", "5", "9"]);
    }

    /// A value that itself contains a comma (`device`, `tensor-split`) is only
    /// reachable through the `;` form, which is the whole reason it exists.
    #[test]
    fn semicolon_wins_when_present() {
        assert_eq!(
            parse_values("Vulkan1,CUDA0;CUDA0;ROCm0,CUDA0").unwrap(),
            ["Vulkan1,CUDA0", "CUDA0", "ROCm0,CUDA0"]
        );
    }

    #[test]
    fn empty_and_backwards_and_huge_ranges_are_errors() {
        assert!(parse_values("  ").is_err());
        assert!(parse_values(",,").is_err());
        assert!(parse_values("8..2").unwrap_err().contains("backwards"));
        assert!(parse_values("1..500").unwrap_err().contains("500 values"));
    }

    /// A token that is not a range stays a literal, dots included: `1.5` is a
    /// value, not half of a range.
    #[test]
    fn non_range_tokens_stay_literal() {
        assert_eq!(parse_values("1.5,q8_0,a..b").unwrap(), ["1.5", "q8_0", "a..b"]);
    }

    #[test]
    fn unknown_key_names_the_sweepable_ones() {
        let e = setter("spec-draft-nmax").unwrap_err();
        assert!(e.contains("spec-draft-n-max"), "{e}");
        assert!(e.contains("tensor-split"), "{e}");
    }

    /// Every entry in the table writes a key `render_section` actually emits.
    /// A key that only existed here would sweep nothing: the leg would measure
    /// the unchanged preset and report it as a data point.
    #[test]
    fn every_sweepable_key_reaches_the_ini() {
        for (key, set) in SWEEPABLE {
            let mut p = presets::Preset {
                id: "T".into(),
                model: "C:\\m.gguf".into(),
                ..Default::default()
            };
            let value = match *key {
                "flash-attn" | "mmproj-offload" => "true",
                "device" => "CUDA0",
                "tensor-split" => "3,1",
                "split-mode" => "layer",
                "override-tensor" => "token_embd\\.weight=CUDA0",
                "cache-type-k" | "cache-type-v" | "spec-draft-type-k" | "spec-draft-type-v" => {
                    "q8_0"
                }
                "spec-type" => "draft-mtp",
                "model-draft" => "C:\\d.gguf",
                "device-draft" => "CUDA0",
                _ => "7",
            };
            set(&mut p, value).unwrap_or_else(|e| panic!("{key}: {e}"));
            let ini = presets::render_section(&p);
            assert!(emits(&ini, key), "{key} never reaches presets.ini");
        }
    }

    #[test]
    fn unset_clears_a_key() {
        let mut p = presets::Preset {
            id: "T".into(),
            model: "C:\\m.gguf".into(),
            spec_draft_n_max: Some(6),
            flash_attn: Some(true),
            ..Default::default()
        };
        setter("spec-draft-n-max").unwrap()(&mut p, UNSET).unwrap();
        setter("flash-attn").unwrap()(&mut p, "UNSET").unwrap();
        assert_eq!(p.spec_draft_n_max, None);
        assert_eq!(p.flash_attn, None);
        let ini = presets::render_section(&p);
        assert!(!emits(&ini, "spec-draft-n-max"));
        assert!(!emits(&ini, "flash-attn"));
    }

    #[test]
    fn bad_values_fail_before_the_gpu_time() {
        assert!(setter("spec-draft-n-max").unwrap()(&mut presets::Preset::default(), "six").is_err());
        assert!(setter("flash-attn").unwrap()(&mut presets::Preset::default(), "maybe").is_err());
        // A `;` or `#` in a text value reloads truncated, so it is refused here
        // exactly as `presets::save` refuses it.
        assert!(setter("device").unwrap()(&mut presets::Preset::default(), "CUDA0;x").is_err());
    }

    #[test]
    fn best_is_the_highest_primary_mean() {
        let legs = [leg("2", 39.2, ""), leg("5", 59.3, ""), leg("6", 43.6, "")];
        assert_eq!(best(&legs, super::super::LIVE_DECODE), Some(1));
        assert_eq!(best(&legs, "prefill"), None);
        assert_eq!(best(&[], super::super::LIVE_DECODE), None);
    }

    /// A leg that failed keeps its row: "this value does not load" is a result,
    /// and a table that omits it reads as if the value was never tried.
    #[test]
    fn summary_keeps_failed_legs_and_ranks_the_rest() {
        let legs = vec![
            leg("2", 39.2, "draft accepted 85%"),
            Leg {
                value: "3".into(),
                error: "llama-server answered HTTP 500".into(),
                ..Default::default()
            },
            leg("5", 59.3, "draft accepted 80%"),
        ];
        let o = opts(&["2", "3", "5"]);
        let md = render_summary(&Summary {
            opts: &o,
            legs: &legs,
            original: "6",
            applied: "6",
            stamp: "2026-09-01 18:00:00 UTC",
            server_version: "0.3.0 · b10621",
        });
        assert!(md.contains("| spec-draft-n-max | decode | +- | vs 2 |"), "{md}");
        assert!(md.contains("| 2 | 39.2 | 0.5 | 1.00x |"), "{md}");
        assert!(md.contains("| 5 | 59.3 | 0.5 | 1.51x |"), "{md}");
        assert!(md.contains("FAILED: llama-server answered HTTP 500"), "{md}");
        assert!(md.contains("1 of 3 legs failed"), "{md}");
        assert!(
            md.contains("**Best: spec-draft-n-max = 5** at 59.3 t/s decode, 1.51x the first value."),
            "{md}"
        );
        // The value to put back is in the table, not only in the jsonl.
        assert!(md.contains("| spec-draft-n-max before the sweep | 6 |"), "{md}");
    }

    #[test]
    fn header_json_carries_the_value_to_restore() {
        let o = opts(&["2", "3"]);
        let line = header_json(&o, "6", "20260901-180000");
        let v: serde_json::Value = serde_json::from_str(&line).unwrap();
        assert_eq!(v["kind"], "sweep");
        assert_eq!(v["restore"], "6");
        assert_eq!(v["key"], "spec-draft-n-max");
        assert_eq!(v["values"][1], "3");
    }

    #[test]
    fn leg_json_carries_the_points() {
        let v: serde_json::Value =
            serde_json::from_str(&leg_json(&leg("5", 59.3, "draft accepted 80%"))).unwrap();
        assert_eq!(v["kind"], "leg");
        assert_eq!(v["value"], "5");
        assert_eq!(v["points"][0]["test"], "decode");
        assert_eq!(v["points"][0]["mean"], 59.3);
        assert_eq!(v["report"], "bench-5.md");
    }

    /// A file with a header and no end line is the signal that presets.ini is
    /// still holding one of that sweep's leg values.
    #[test]
    fn a_sweep_file_is_closed_only_by_its_end_line() {
        let o = opts(&["3", "4"]);
        let header = header_json(&o, "6", "20260901-192445");
        let open = sweep_state(&format!("{header}\n{}\n", leg_json(&leg("3", 47.3, "")))).unwrap();
        assert!(!open.closed_line);
        assert_eq!(open.restore, "6");
        assert_eq!(open.key, "spec-draft-n-max");
        assert_eq!(open.preset, "Qwen");

        let closed = sweep_state(&format!("{header}\n{}\n", end_json("6", 2))).unwrap();
        assert!(closed.closed_line);
    }

    /// Anything that is not a sweep header is not a sweep: a truncated file, an
    /// empty one, or a `bench-` run that somehow got here.
    #[test]
    fn sweep_state_ignores_files_that_are_not_sweeps() {
        assert!(sweep_state("").is_none());
        assert!(sweep_state("{\"kind\":\"run\",\"mode\":\"live\"}").is_none());
        assert!(sweep_state("not json at all").is_none());
    }

    /// The summary must never land in the tab's saved-runs list: `past_runs`
    /// globs `bench-*.jsonl` and `load_run` would choke on these lines.
    #[test]
    fn summary_files_are_not_named_like_runs() {
        let (jsonl, md) = sweep_paths("20260901-180000");
        assert!(file_name(&jsonl).starts_with("sweep-"));
        assert!(file_name(&md).starts_with("sweep-"));
        assert!(!file_name(&jsonl).starts_with("bench-"));
    }

    #[test]
    fn synthetic_ranks_on_the_first_tg_row() {
        let mut l = leg("2", 0.0, "");
        l.points = vec![
            Point {
                preset: "P".into(),
                test: "pp2048".into(),
                mean: 900.0,
                ..Default::default()
            },
            Point {
                preset: "P".into(),
                test: "tg128".into(),
                mean: 21.0,
                ..Default::default()
            },
        ];
        assert_eq!(primary_test(Mode::Synthetic, &[l]), "tg128");
        assert_eq!(primary_test(Mode::Live, &[]), "decode");
    }
}
