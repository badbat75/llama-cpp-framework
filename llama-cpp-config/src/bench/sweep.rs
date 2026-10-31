//! Unattended parameter sweeps: one benchmark leg per value of ONE preset key.
//!
//! The Benchmark tab measures a configuration; a sweep measures a QUESTION
//! about one ("how far does raising `spec-draft-n-max` keep paying?"). The
//! difference is only a loop, but that loop is why this lives in the CLI rather
//! than behind a button: a live leg is a model load plus `reps` cold prefills,
//! so a seven-value sweep is half an hour during which nothing else may touch
//! the machine. That is a thing to start from a terminal and walk away from.
//!
//! A sweep is ONE bench.db campaign (`sweep-<stamp>`, or `--campaign`) and
//! every leg is a run of it labelled `key=value`, committed when the leg
//! finishes: a sweep killed at the fourth value leaves three finished runs the
//! tab reloads side by side, not one truncated file. A leg that fails is not a
//! run (only finished runs are recorded); the campaign's notes name it. The
//! cross-leg digest is the campaign's report.
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
//! sweep's PENDING marker (`bench/sweep-pending-<stamp>.json`) before any leg
//! runs, and the marker is deleted only once the key is put back: a machine
//! that reboots mid-sweep still has the value to restore on disk, and the next
//! sweep of the same preset and key warns about it. The marker is the one file
//! a sweep writes, and it exists only while the preset is not as it was.

use std::path::PathBuf;
use std::sync::mpsc;
use std::time::{Duration, Instant};

use super::exec::Event;
use super::{Concurrency, Mode, Plan, Point};
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

/// How long to leave the GPUs alone after llama-server is gone, before the next
/// leg loads the model again.
///
/// The process vanishing is NOT the same event as the driver releasing its
/// memory, and a sweep is the one workload that asks for tens of GiB, drops
/// them, and asks again a second later. Measured here on a 27B model split over
/// a Vulkan R9700 and a CUDA 4070S: the leg after a restart died with
/// `vk::Device::allocateMemory: ErrorOutOfDeviceMemory` on a 1,052,686,976-byte
/// buffer, while `--list-devices` moments later reported 31 GiB free on that
/// same card; an earlier run failed differently for the same reason (ROCm
/// reporting "no ROCm-capable device is detected" mid-teardown, which then made
/// a vision-encoder device of `ROCm0` invalid and stopped the server from starting
/// at all). Both are the previous child still being torn down. The wait costs
/// seconds against a leg that runs for minutes.
const GPU_SETTLE: Duration = Duration::from_secs(15);

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
/// resolve. Sampler VALUES and reasoning keys are deliberately absent (the live
/// engine overrides the temperature anyway, and the synthetic one ignores both);
/// `backend-sampling` is in because it moves WHERE the chain runs, not what it
/// samples.
pub const SWEEPABLE: &[(&str, Setter)] = &[
    // Speculative decoding.
    ("spec-draft-n-max", |p, v| {
        p.spec_draft_n_max = int(v)?;
        Ok(())
    }),
    ("spec-draft-p-min", |p, v| {
        p.spec_draft_p_min = float(v)?;
        Ok(())
    }),
    ("spec-draft-n-min", |p, v| {
        p.spec_draft_n_min = int(v)?;
        Ok(())
    }),
    ("spec-draft-n-ubatch", |p, v| {
        p.spec_draft_n_ubatch = int(v)?;
        Ok(())
    }),
    ("spec-type", |p, v| {
        p.spec_type = text(v)?;
        Ok(())
    }),
    ("spec-ngram-mod-n-match", |p, v| {
        p.spec_ngram_mod_n_match = int(v)?;
        Ok(())
    }),
    ("spec-ngram-mod-n-min", |p, v| {
        p.spec_ngram_mod_n_min = int(v)?;
        Ok(())
    }),
    ("spec-ngram-mod-n-max", |p, v| {
        p.spec_ngram_mod_n_max = int(v)?;
        Ok(())
    }),
    ("spec-ngram-simple-size-n", |p, v| {
        p.spec_ngram_simple_size_n = int(v)?;
        Ok(())
    }),
    ("spec-ngram-simple-size-m", |p, v| {
        p.spec_ngram_simple_size_m = int(v)?;
        Ok(())
    }),
    ("spec-ngram-map-k-size-n", |p, v| {
        p.spec_ngram_map_k_size_n = int(v)?;
        Ok(())
    }),
    ("spec-ngram-map-k-size-m", |p, v| {
        p.spec_ngram_map_k_size_m = int(v)?;
        Ok(())
    }),
    ("spec-ngram-map-k4v-size-n", |p, v| {
        p.spec_ngram_map_k4v_size_n = int(v)?;
        Ok(())
    }),
    ("spec-ngram-map-k4v-size-m", |p, v| {
        p.spec_ngram_map_k4v_size_m = int(v)?;
        Ok(())
    }),
    ("spec-ngram-map-k4v-min-hits", |p, v| {
        p.spec_ngram_map_k4v_min_hits = int(v)?;
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
    ("n-cpu-ffn", |p, v| {
        p.n_cpu_ffn = int(v)?;
        Ok(())
    }),
    // Live engine only: llama-bench has no such option (`bench::caveats`).
    ("moe-cache-mib", |p, v| {
        p.moe_cache_mib = int(v)?;
        Ok(())
    }),
    ("op-offload", |p, v| {
        p.op_offload = boolean(v)?;
        Ok(())
    }),
    ("repack", |p, v| {
        p.repack = boolean(v)?;
        Ok(())
    }),
    // `false` and `unset` are one state: the key has no negative form.
    ("no-host", |p, v| {
        p.no_host = boolean(v)? == Some(true);
        Ok(())
    }),
    ("lazy-mode", |p, v| {
        p.lazy_mode = text(v)?;
        Ok(())
    }),
    ("load-mode", |p, v| {
        p.load_mode = text(v)?;
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
    ("kv-unified", |p, v| {
        p.kv_unified = boolean(v)?;
        Ok(())
    }),
    ("kv-unified-per-slot", |p, v| {
        p.kv_unified_per_slot = int(v)?;
        Ok(())
    }),
    ("cache-ram", |p, v| {
        p.cache_ram = int(v)?;
        Ok(())
    }),
    // Only a multi-turn workload sees these: the live engine sends
    // `cache_prompt: false`, so a sweep over them measures the RAM side and
    // the per-request cost of making a checkpoint, not the reuse they buy.
    ("ctx-checkpoints", |p, v| {
        p.ctx_checkpoints = int(v)?;
        Ok(())
    }),
    ("checkpoint-min-step", |p, v| {
        p.checkpoint_min_step = int(v)?;
        Ok(())
    }),
    ("fit-ctx", |p, v| {
        p.fit_ctx = int(v)?;
        Ok(())
    }),
    ("fit-target", |p, v| {
        p.fit_target = text(v)?;
        crate::presets::validate_fit_target(&p.fit_target)
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
    // Sampling PLACEMENT, not a sampler value: it moves where the chain runs,
    // which is exactly what the live engine measures (the synthetic one never
    // samples and ignores it, like every other sampling key).
    ("backend-sampling", |p, v| {
        p.backend_sampling = boolean(v)?;
        Ok(())
    }),
];

/// The setter for `key`, or an error naming every key that has one.
pub fn setter(key: &str) -> Result<Setter, String> {
    SWEEPABLE
        .iter()
        .find(|(k, _)| *k == key)
        .map(|(_, f)| *f)
        .ok_or_else(|| {
            format!(
                "`{key}` is not a sweepable preset key. Sweepable: {}",
                keys()
            )
        })
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

fn float(v: &str) -> Result<Option<f64>, String> {
    if v.eq_ignore_ascii_case(UNSET) {
        return Ok(None);
    }
    v.parse::<f64>()
        .ok()
        .filter(|x| x.is_finite())
        .map(Some)
        .ok_or_else(|| format!("`{v}` is not a number (or `{UNSET}`)"))
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
    /// the file (it is in that sweep's pending marker, which the warning names).
    pub restore_to: Option<String>,
    /// The bench.db campaign; empty for `sweep-<stamp>`.
    pub campaign: String,
    /// What the sweep asks; empty for one spelled from the key and preset.
    pub question: String,
}

/// One measured value: the leg's points, and what went wrong if something did. A failed leg is kept rather than dropped: "this value
/// does not load" is a result, and a summary that omits it reads as if the value
/// was never tried.
#[derive(Clone, Debug, Default)]
pub struct Leg {
    pub value: String,
    pub points: Vec<Point>,
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
            if opts.plan.max_tokens > 0 {
                opts.plan.max_tokens.to_string()
            } else {
                "no cap (the model stops on its own)".into()
            }
        ));
        md.push_str(&format!(
            "| concurrency | {} |\n",
            opts.plan.concurrency.as_str()
        ));
        // Same three rows the single-run report carries, from the same builder:
        // a sweep is a comparison too, and "which prompt was this" has to be
        // answerable from either file.
        for (label, value) in super::prompt_report_rows(&opts.plan) {
            md.push_str(&format!("| {label} | {value} |\n"));
        }
    }
    md.push_str(&format!(
        "| {} before the sweep | {} |\n",
        opts.key, s.original
    ));
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
    head.push_str(" note |");
    sep.push_str("---|");
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
        row.push_str(&format!(" {note} |"));
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
        "Each leg's run in bench.db (`bench query runs --campaign <this sweep>`) carries the environment, the device ids resolved by name, every parameter and the raw repetitions. This table is a digest of them, not a replacement.",
    ];
    if mode == Mode::Live {
        out.push(
            "Every leg restarted llama-server without saving or restoring the conversation snapshot, so the numbers are cold-start numbers and no leg inherited another's KV cache.",
        );
    }
    out
}

// ── The pending marker ───────────────────────────────────────────────────

/// The marker of a sweep that has not put its key back yet. One per sweep,
/// named by stamp, so two sweeps of different keys never share one.
pub fn pending_path(stamp: &str) -> PathBuf {
    super::bench_dir().join(format!("sweep-pending-{stamp}.json"))
}

/// The marker's content, written BEFORE any leg runs: it carries the value to
/// restore if the sweep never reaches its own cleanup.
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

/// What a marker says about its sweep.
pub struct State {
    pub preset: String,
    pub key: String,
    pub restore: String,
}

/// Read a marker. `None` when it is not a sweep header (an empty or truncated
/// file).
pub fn sweep_state(content: &str) -> Option<State> {
    let header: serde_json::Value = serde_json::from_str(content.trim()).ok()?;
    if header.get("kind")?.as_str()? != "sweep" {
        return None;
    }
    Some(State {
        preset: header["preset"].as_str().unwrap_or_default().to_string(),
        key: header["key"].as_str().unwrap_or_default().to_string(),
        restore: header["restore"].as_str().unwrap_or_default().to_string(),
    })
}

/// The markers left for this preset and key, oldest first.
fn markers(preset: &str, key: &str) -> Vec<(PathBuf, State)> {
    let Ok(dir) = std::fs::read_dir(super::bench_dir()) else {
        return Vec::new();
    };
    let mut files: Vec<PathBuf> = dir
        .flatten()
        .map(|e| e.path())
        .filter(|p| {
            p.file_name()
                .and_then(|n| n.to_str())
                .is_some_and(|n| n.starts_with("sweep-pending-") && n.ends_with(".json"))
        })
        .collect();
    // The stamp is `yyyymmdd-hhmmss`, so a lexical sort IS chronological.
    files.sort();
    files
        .into_iter()
        .filter_map(|p| {
            let state = sweep_state(&std::fs::read_to_string(&p).ok()?)?;
            (state.preset == preset && state.key == key).then_some((p, state))
        })
        .collect()
}

/// The newest unfinished sweep of this preset and key: its marker and the
/// value it recorded to restore.
fn interrupted(preset: &str, key: &str) -> Option<(PathBuf, String)> {
    markers(preset, key)
        .pop()
        .map(|(path, state)| (path, state.restore))
}

// ── The run ──────────────────────────────────────────────────────────────

/// Run the sweep, committing each leg to bench.db as it finishes and the
/// digest at the end. Returns the campaign's name; an `Err` lists the legs and
/// the cleanup steps that failed (the finished legs are in the database all
/// the same).
pub fn run(opts: &Opts) -> Result<String, String> {
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

    // BEFORE this sweep's own marker exists, and that ordering is the whole
    // correctness of the check: `interrupted` answers with the NEWEST marker of
    // this preset and key, which the moment the one below is created is this
    // sweep's. Scanning after creating it therefore reports every sweep as
    // interrupted (v1.13.0 did) and, worse, hides a real leftover.
    let leftover = match opts.restore_to {
        Some(_) => None,
        None => interrupted(&opts.preset, &opts.key),
    };

    let stamp = super::stamp(super::now_secs());
    let campaign = super::record::Campaign {
        name: if opts.campaign.is_empty() {
            format!("sweep-{stamp}")
        } else {
            opts.campaign.clone()
        },
        question: if opts.question.is_empty() {
            format!(
                "How does `{}` change {} on preset {}? Values: {}.",
                opts.key,
                primary_label(opts.plan.mode),
                opts.preset,
                opts.values.join(", ")
            )
        } else {
            opts.question.clone()
        },
        label: None,
    };
    let dir = super::bench_dir();
    std::fs::create_dir_all(&dir).map_err(|e| format!("cannot create {}: {e}", dir.display()))?;
    let marker = pending_path(&stamp);
    std::fs::write(&marker, header_json(opts, &original, &stamp))
        .map_err(|e| format!("cannot write {}: {e}", marker.display()))?;

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
        marker.display()
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

    // The engine resolves the preset's device ids to card names through the
    // probe cache, which only the GUI fills on its own; without this every leg
    // records the ids as unknown. Once, before the first leg: the ids name the
    // same cards for the whole sweep.
    crate::devices::set_probed(crate::devices::list());

    let mut legs: Vec<Leg> = Vec::new();
    for (i, value) in opts.values.iter().enumerate() {
        println!("\n[{}/{}] {} = {value}", i + 1, opts.values.len(), opts.key);
        let started = Instant::now();
        let leg = match write_value(&opts.preset, set, value)
            .and_then(|()| prepare_server(opts.plan.mode, port))
        {
            Ok(()) => measure(opts, value, &campaign),
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
    // The key is back: this sweep's marker goes, and so do older ones of the
    // same preset and key, whose leftover this restore has overwritten (the
    // newest sweep of a pair decides). Only after the restore, so a sweep that
    // died putting the key back still reads as interrupted.
    if restore.is_ok() {
        for (path, _) in markers(&opts.preset, &opts.key) {
            let _ = std::fs::remove_file(path);
        }
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
    println!("\n{summary}");
    if record_digest(
        &campaign.name,
        &campaign_notes(opts, &legs, &original, &applied),
        &summary,
    ) {
        println!(
            "Campaign {} in bench.db: `llama-cpp-config bench query runs --campaign {}`.",
            campaign.name, campaign.name
        );
    } else {
        println!("No leg finished, so bench.db has no campaign for this sweep.");
    }

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
        Ok(campaign.name)
    } else {
        Err(problems.join("; "))
    }
}

/// A leg's run label: `key=value`, plus the concurrency scenario when it is
/// not `single`, so one campaign can hold the same value under several.
fn leg_label(opts: &Opts, value: &str) -> String {
    let mut label = format!("{}={value}", opts.key);
    if let Some(tag) = &opts.plan.spec {
        label.push_str(&format!(" pass={}", tag.pass));
    }
    if opts.plan.concurrency != super::Concurrency::Single {
        label.push_str(&format!(" concurrency={}", opts.plan.concurrency.as_str()));
    }
    label
}

/// Hold `key` of `preset` for a temporary write (a spec pass's override): a
/// pending marker carrying the value to put back, written BEFORE the change,
/// so an interrupted run is reported (and restorable) like an interrupted
/// sweep of that key. Returns the current value and the marker.
pub fn hold_key(preset: &str, key: &str) -> Result<(String, PathBuf), String> {
    setter(key)?;
    let original = current_value(preset, key);
    let stamp = format!("{}-{key}", super::stamp(super::now_secs()));
    let dir = super::bench_dir();
    std::fs::create_dir_all(&dir).map_err(|e| format!("cannot create {}: {e}", dir.display()))?;
    let marker = pending_path(&stamp);
    let header = serde_json::json!({
        "kind": "sweep",
        "stamp": stamp,
        "preset": preset,
        "key": key,
        "restore": original,
        "override": true,
    });
    std::fs::write(&marker, header.to_string())
        .map_err(|e| format!("cannot write {}: {e}", marker.display()))?;
    Ok((original, marker))
}

/// Write one key of `preset` (`unset` removes it).
pub fn set_key(preset: &str, key: &str, value: &str) -> Result<(), String> {
    write_value(preset, setter(key)?, value)
}

/// The plan one leg runs: the caller's, pinned to the swept preset.
fn leg_plan(opts: &Opts, preset: &str) -> Plan {
    Plan {
        presets: vec![preset.to_string()],
        ..opts.plan.clone()
    }
}

/// What `--concurrency all` runs for `preset`: one request, then, when the
/// preset has two slots or more (absent `parallel` = llama-server's auto 4),
/// every concurrent scenario at N = its `parallel`.
pub fn all_scenarios(preset: &str) -> Result<Vec<Concurrency>, String> {
    let n = load_one(preset)?.parallel.unwrap_or(super::AUTO_PARALLEL);
    Ok(scenarios_for(n))
}

fn scenarios_for(parallel: i32) -> Vec<Concurrency> {
    if parallel >= 2 {
        Concurrency::ALL.to_vec()
    } else {
        vec![Concurrency::Single]
    }
}

fn load_one(id: &str) -> Result<presets::Preset, String> {
    let all = presets::load_all();
    all.iter().find(|p| p.id == id).cloned().ok_or_else(|| {
        // A switched-off preset is still listed everywhere else, so say why
        // this one cannot be swept rather than pretend it does not exist.
        if presets::load_listed()
            .iter()
            .any(|l| !l.enabled && l.preset.id == id)
        {
            return format!(
                "preset `{id}` is switched off, so llama-server does not offer it: \
                     `llama-cpp-config preset enable {id}` first"
            );
        }
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
            std::thread::sleep(GPU_SETTLE);
            // Bare start, not the `control restart` path: no snapshot save or
            // restore, see the module header.
            runstate::start().map_err(|e| format!("cannot start llama-server: {e}"))?;
            wait_ready(port)
        }
        Mode::Synthetic => {
            if runstate::is_running() {
                runstate::stop();
                wait_gone()?;
                // llama-bench asks the same cards for the same weights, so it
                // needs the same settling the live path does.
                std::thread::sleep(GPU_SETTLE);
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

/// What the primary test is called before any leg has run.
fn primary_label(mode: Mode) -> &'static str {
    match mode {
        Mode::Live => "live decode",
        Mode::Synthetic => "llama-bench decode",
    }
}

/// The campaign's notes in bench.db: what was swept, what the preset was left
/// on, and the values that did not finish, which have no run of their own
/// (only finished runs are recorded) but are a result all the same.
pub fn campaign_notes(opts: &Opts, legs: &[Leg], original: &str, applied: &str) -> String {
    let mut out = format!(
        "sweep of `{}` on preset {} ({}, {} repetition{}): {}\n\
         before: {original}; left on: {applied} (--apply {})",
        opts.key,
        opts.preset,
        opts.plan.mode.as_str(),
        opts.plan.reps,
        if opts.plan.reps == 1 { "" } else { "s" },
        opts.values.join(", "),
        opts.apply.as_str()
    );
    for leg in legs.iter().filter(|l| !l.error.is_empty()) {
        out.push_str(&format!(
            "\nnot finished: {} = {}: {}",
            opts.key, leg.value, leg.error
        ));
    }
    out
}

/// The campaign's notes and report. `false` when there is no campaign, i.e.
/// no leg finished.
fn record_digest(campaign: &str, notes: &str, report: &str) -> bool {
    let Ok(conn) = super::db::open(&super::db::work_path()) else {
        return false;
    };
    super::db::set_notes(&conn, campaign, notes).unwrap_or(false)
        && super::db::set_report(&conn, campaign, report).unwrap_or(false)
}

/// Run one leg through the ordinary benchmark engine and collect what it
/// produced. Blocks until the worker thread is done: the sweep is sequential by
/// nature, two legs at once would measure each other.
fn measure(opts: &Opts, value: &str, campaign: &super::record::Campaign) -> Leg {
    let (tx, rx) = mpsc::channel();
    let plan = Plan {
        campaign: Some(super::record::Campaign {
            label: Some(leg_label(opts, value)),
            ..campaign.clone()
        }),
        ..leg_plan(opts, &opts.preset)
    };
    let _handle = super::exec::spawn(plan, move |ev| {
        let _ = tx.send(ev);
    });

    let mut points: Vec<Point> = Vec::new();
    let mut outcome: Option<Result<String, String>> = None;
    // Ends when the worker drops its sender, which it does after emitting Done.
    for ev in rx {
        match ev {
            Event::Progress(msg) => println!("    {msg}"),
            Event::Points(p) => points = p,
            Event::Done(r) => outcome = Some(r),
        }
    }

    let error = match outcome {
        Some(Ok(_)) => String::new(),
        Some(Err(e)) => e,
        None => "the benchmark worker ended silently".into(),
    };
    Leg {
        value: value.to_string(),
        points,
        error,
        secs: 0,
    }
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
            campaign: String::new(),
            question: String::new(),
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
        assert_eq!(
            parse_values("1.5,q8_0,a..b").unwrap(),
            ["1.5", "q8_0", "a..b"]
        );
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
                "flash-attn" | "mmproj-offload" | "backend-sampling" | "op-offload" | "repack"
                | "no-host" | "kv-unified" => "true",
                "device" => "CUDA0",
                "tensor-split" => "3,1",
                "split-mode" => "layer",
                "lazy-mode" => "on",
                "load-mode" => "mmap",
                "override-tensor" => "token_embd\\.weight=CUDA0",
                "cache-type-k" | "cache-type-v" | "spec-draft-type-k" | "spec-draft-type-v" => {
                    "q8_0"
                }
                "spec-type" => "draft-mtp",
                "model-draft" => "C:\\d.gguf",
                "device-draft" => "CUDA0",
                "spec-draft-p-min" => "0.5",
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
        assert!(
            setter("spec-draft-n-max").unwrap()(&mut presets::Preset::default(), "six").is_err()
        );
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
        assert!(
            md.contains("| spec-draft-n-max | decode | +- | vs 2 |"),
            "{md}"
        );
        assert!(md.contains("| 2 | 39.2 | 0.5 | 1.00x |"), "{md}");
        assert!(md.contains("| 5 | 59.3 | 0.5 | 1.51x |"), "{md}");
        assert!(
            md.contains("FAILED: llama-server answered HTTP 500"),
            "{md}"
        );
        assert!(md.contains("1 of 3 legs failed"), "{md}");
        assert!(
            md.contains(
                "**Best: spec-draft-n-max = 5** at 59.3 t/s decode, 1.51x the first value."
            ),
            "{md}"
        );
        // The value to put back is in the table, not only in the marker.
        assert!(
            md.contains("| spec-draft-n-max before the sweep | 6 |"),
            "{md}"
        );
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

    /// A marker that is not a sweep header is not a sweep: a truncated file,
    /// an empty one, or anything else that got the name.
    #[test]
    fn sweep_state_reads_a_marker_and_ignores_anything_else() {
        let o = opts(&["3", "4"]);
        let state = sweep_state(&header_json(&o, "6", "20260901-192445")).unwrap();
        assert_eq!(state.restore, "6");
        assert_eq!(state.key, "spec-draft-n-max");
        assert_eq!(state.preset, "Qwen");
        assert!(sweep_state("").is_none());
        assert!(sweep_state("{\"kind\":\"run\",\"mode\":\"live\"}").is_none());
        assert!(sweep_state("not json at all").is_none());
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
