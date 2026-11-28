//! Unattended parameter sweeps: one benchmark leg per value of ONE model profile key.
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
//! llama-server's router spawns one child per model and hands it the model profile's
//! keys at spawn time; nothing re-reads models.ini afterwards. A live sweep
//! therefore restarts the server between values, and the restart is deliberately
//! the bare `stop()` + `start()` pair of `server_control` rather than the one `control
//! restart` performs: that path saves and restores the conversation snapshot,
//! which on a model of this size is gigabytes of IO per leg to reinstate a KV
//! cache the benchmark then refuses to use (`cache_prompt: false`).
//!
//! The synthetic engine wants the opposite state (llama-server DOWN: a second
//! resident copy of the weights does not fail, it spills into shared memory and
//! reads as a backend regression) and needs no restart at all, since llama-bench
//! is a fresh process per leg and the leg builds its argv from models.ini.
//!
//! ## `;` separates the values, because `,` cannot
//!
//! `device = Vulkan1,CUDA0` and `tensor-split = 60,6` are single values that
//! contain commas, so a comma-separated `--values` could never sweep them. A `;`
//! cannot appear in an INI value at all (`ini::reject_comment_markers`:
//! llama-server's model profile reader cuts a value at the first `;` or `#`), which
//! makes it the one separator guaranteed not to collide with the data. So
//! `--values` splits on `;` when it sees one, and on `,` otherwise.
//!
//! ## models.ini is edited, and says so before it is
//!
//! A sweep writes the key it sweeps, so an interrupted sweep leaves the model profile
//! on whatever value it was measuring. The original is written into the
//! sweep's PENDING marker (`bench/sweep-pending-<stamp>.json`) before any leg
//! runs, and the marker is deleted only once the key is put back: a machine
//! that reboots mid-sweep still has the value to restore on disk, and the next
//! sweep of the same model profile and key warns about it. The marker is the one file
//! a sweep writes, and it exists only while the model profile is not as it was. Ctrl-C
//! puts the key back as well (`interrupt.rs`), so the marker is left only by what
//! no handler survives: a kill -9, a crash, a power cut.

use crate::strings;
use std::path::PathBuf;
use std::sync::mpsc;
use std::time::{Duration, Instant};

use super::exec::Event;
use super::{Concurrency, Mode, Plan, Point};
use crate::server_control::ConfigFile;
use crate::{http, ini, model_profiles, server_cfg, server_control};

/// The value that means "the key is absent from the section". Spelled rather
/// than left blank so it survives a round trip through a comma-separated list
/// and reads as a deliberate choice in the summary table.
pub const UNSET: &str = "unset";

/// The one leg's value when no key is varied: the model profile as models.ini has it.
pub const AS_IS: &str = "as is";

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
/// them, and asks again a second later. Seen here on a 27B model split over
/// a Vulkan R9700 and a CUDA 4070S: the leg after a restart died with
/// `vk::Device::allocateMemory: ErrorOutOfDeviceMemory` on a buffer far smaller
/// than what `--list-devices` reported free on that same card moments
/// later; an earlier run failed differently for the same reason (ROCm
/// reporting "no ROCm-capable device is detected" mid-teardown, which then made
/// a vision-encoder device of `ROCm0` invalid and stopped the server from starting
/// at all). Both are the previous child still being torn down. The wait costs
/// seconds against a leg that runs for minutes.
const GPU_SETTLE: Duration = Duration::from_secs(15);

/// A short read timeout for the readiness probe: `/v1/models` is answered off
/// the router's own state, so a router that needs seconds for it is not up yet.
const PROBE_TIMEOUT: Duration = Duration::from_secs(5);

// ── What may be swept ────────────────────────────────────────────────────

type Setter = fn(&mut model_profiles::ModelProfile, &str) -> Result<(), String>;

/// The model profile keys a sweep may vary, each with the typed field it writes.
///
/// A table and not a raw `ini::replace_key` for two reasons. A typo would
/// otherwise silently add a key llama-server does not know (its model profile reader
/// only warns), and the leg would then measure the unchanged configuration and
/// report it as a data point. And going through the typed field means the value
/// is parsed HERE, before the sweep spends an hour of GPU time, and the section
/// is rewritten by `model_profiles::save`, so the file keeps the comments the GUI
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
        crate::model_profiles::validate_fit_target(&p.fit_target)
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
        .ok_or_else(|| strings::fill("not-sweepable", &[&key, &keys()]))
}

/// Every value of `key` applied to a throwaway copy of `model_profile`: the Benchmark
/// tab's A/B check, the same one `run` makes before its first leg.
pub fn check_values(model_profile: &str, key: &str, values: &[String]) -> Result<(), String> {
    let set = setter(key)?;
    let base = load_one(model_profile)?;
    for v in values {
        let mut probe = base.clone();
        set(&mut probe, v).map_err(|e| strings::fill("key-value-refused", &[&key, &v, &e]))?;
        crate::model_profiles::validate_for_save(&probe)
            .map_err(|e| strings::fill("key-value-refused", &[&key, &v, &e]))?;
    }
    Ok(())
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
        .map_err(|_| strings::fill("not-whole-number-or", &[&v, &UNSET]))
}

fn float(v: &str) -> Result<Option<f64>, String> {
    if v.eq_ignore_ascii_case(UNSET) {
        return Ok(None);
    }
    v.parse::<f64>()
        .ok()
        .filter(|x| x.is_finite())
        .map(Some)
        .ok_or_else(|| strings::fill("not-number-or", &[&v, &UNSET]))
}

fn boolean(v: &str) -> Result<Option<bool>, String> {
    if v.eq_ignore_ascii_case(UNSET) {
        return Ok(None);
    }
    match v.to_ascii_lowercase().as_str() {
        "true" | "on" | "yes" | "1" => Ok(Some(true)),
        "false" | "off" | "no" | "0" => Ok(Some(false)),
        _ => Err(strings::fill("not-boolean-or", &[&v, &UNSET])),
    }
}

fn text(v: &str) -> Result<String, String> {
    if v.eq_ignore_ascii_case(UNSET) {
        return Ok(String::new());
    }
    // The same gate `model_profiles::save` applies, run here so a bad value fails
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
        return Err(strings::t("values-empty"));
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
        Some((lo, hi)) if lo > hi => Err(strings::fill("range-backwards", &[&token])),
        Some((lo, hi)) if hi - lo + 1 > MAX_RANGE => {
            Err(strings::fill("range-too-long", &[&token, &(hi - lo + 1)]))
        }
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

/// What the model profile is left on when the sweep finishes.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Apply {
    /// Put back the value the model profile had before the sweep (the default: a
    /// measurement should not silently reconfigure the machine).
    Original,
    /// Keep the value that won on the primary test.
    Best,
    /// Keep the last value measured.
    Last,
}

impl Apply {
    // Public since the move into pai-app, where clippy reads it as the
    // FromStr look-alike it has always been: kept as written.
    #[allow(clippy::should_implement_trait)]
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

/// One sweep as the CLI describes it. `plan.model_profiles` is ignored: a sweep varies
/// one key of ONE model profile, so the leg's plan is built from `model_profile` here.
#[derive(Clone, Debug)]
pub struct Opts {
    pub model_profile: String,
    /// The key to vary; `None` measures the model profile as it is: one leg, and
    /// models.ini is not touched (no write, no marker, nothing to restore).
    pub key: Option<String>,
    /// The key's values; ignored without a key.
    pub values: Vec<String>,
    pub plan: Plan,
    pub apply: Apply,
    /// What `--apply original` puts back, when the value in models.ini is not
    /// it. The case this exists for: an interrupted sweep left the key on one of
    /// its leg values, so "the value it had before" is no longer readable from
    /// the file (it is in that sweep's pending marker, which the warning names).
    pub restore_to: Option<String>,
    /// The bench.db campaign; empty for `sweep-<stamp>`.
    pub campaign: String,
    /// What the sweep asks; empty for one spelled from the key and model profile.
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

    let mut md = match &opts.key {
        Some(key) => format!(
            "# Benchmark sweep: {key} on {}\n\nGenerated {}.\n\n",
            opts.model_profile, s.stamp
        ),
        None => format!(
            "# Benchmark: {} as it is\n\nGenerated {}.\n\n",
            opts.model_profile, s.stamp
        ),
    };

    md.push_str("| setting | value |\n|---|---|\n");
    md.push_str(&format!("| model profile | {} |\n", opts.model_profile));
    if let Some(key) = &opts.key {
        md.push_str(&format!("| key | {key} |\n"));
        md.push_str(&format!("| values | {} |\n", opts.values.join(", ")));
    }
    md.push_str(&format!("| engine | {engine} |\n"));
    md.push_str(&format!("| repetitions | {} |\n", opts.plan.reps));
    if opts.plan.mode == Mode::Live {
        md.push_str(&format!(
            "| temperature | {} |\n",
            match opts.plan.temp {
                Some(t) => format!("{t}"),
                None => "the model profile's own".to_string(),
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
    if let Some(key) = &opts.key {
        md.push_str(&format!("| {key} before the sweep | {} |\n", s.original));
        md.push_str(&format!("| {key} after it | {} |\n", s.applied));
    }
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

    let mut head = format!("| {} |", opts.key.as_deref().unwrap_or("model profile"));
    let mut sep = String::from("|---|");
    for (i, label) in labels.iter().enumerate() {
        head.push_str(&format!(" {label} | +- |"));
        sep.push_str("---:|---:|");
        if i == 0 && opts.key.is_some() {
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
            if i == 0 && opts.key.is_some() {
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

    if let (Some(key), Some(i)) = (&opts.key, best(s.legs, &primary)) {
        let leg = &s.legs[i];
        let mean = leg.point(&primary).map(|p| p.mean).unwrap_or_default();
        let gain = match baseline {
            Some(b) if b > 0.0 => format!(", {:.2}x the first value", mean / b),
            _ => String::new(),
        };
        md.push_str(&format!(
            "**Best: {key} = {}** at {mean:.1} t/s {primary}{gain}.\n\n",
            leg.value
        ));
    }

    md.push_str("## Caveats\n\n");
    for c in sweep_caveats(opts.plan.mode, opts.key.is_some()) {
        md.push_str(&format!("- {c}\n"));
    }
    for c in super::caveats(opts.plan.mode) {
        md.push_str(&format!("- {c}\n"));
    }
    md
}

/// What a sweep adds to the mode's own caveats: everything about measuring one
/// key by moving it that a single run cannot mislead you about; without a key
/// (`swept` false) only the ones that hold for a single leg.
pub fn sweep_caveats(mode: Mode, swept: bool) -> Vec<&'static str> {
    let mut out = Vec::new();
    if swept {
        out.push("Legs ran in the order listed, so a card that heats up over a long sweep charges the drift to the LAST values. Re-run with the order reversed when two values are close.");
        out.push("Only the swept key moved; every other key came from models.ini as it stood when the sweep started, and a hand edit made while it ran is not reflected in the rows above it.");
    }
    out.extend([
        "Each leg's run in bench.db (`bench query runs --campaign <this sweep>`) carries the environment, the device ids resolved by name, every parameter and the raw repetitions. This table is a digest of them, not a replacement.",
    ]);
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
        "preset": opts.model_profile,
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
    pub model_profile: String,
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
        model_profile: header["preset"].as_str().unwrap_or_default().to_string(),
        key: header["key"].as_str().unwrap_or_default().to_string(),
        restore: header["restore"].as_str().unwrap_or_default().to_string(),
    })
}

/// The markers left for this model profile and key, oldest first.
fn markers(model_profile: &str, key: &str) -> Vec<(PathBuf, State)> {
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
            (state.model_profile == model_profile && state.key == key).then_some((p, state))
        })
        .collect()
}

/// The newest unfinished sweep of this model profile and key: its marker and the
/// value it recorded to restore.
fn interrupted(model_profile: &str, key: &str) -> Option<(PathBuf, String)> {
    markers(model_profile, key)
        .pop()
        .map(|(path, state)| (path, state.restore))
}

// ── The run ──────────────────────────────────────────────────────────────

/// Run the sweep, committing each leg to bench.db as it finishes and the
/// digest at the end. Returns the campaign's name; an `Err` lists the legs and
/// the cleanup steps that failed (the finished legs are in the database all
/// the same).
pub fn run(opts: &Opts) -> Result<String, String> {
    let set = opts.key.as_deref().map(setter).transpose()?;
    let base = load_one(&opts.model_profile)?;
    // Every value is applied to a throwaway copy up front: a typo in the last of
    // seven values is worth catching now, not forty minutes from now.
    if let Some(set) = set {
        for v in &opts.values {
            let mut probe = base.clone();
            set(&mut probe, v).map_err(|e| strings::fill("value-refused", &[&v, &e]))?;
            crate::model_profiles::validate_for_save(&probe)
                .map_err(|e| strings::fill("value-refused", &[&v, &e]))?;
        }
    }
    if let Some(problem) = super::validate(&leg_plan(opts, &opts.model_profile)) {
        return Err(problem);
    }

    let original = match (&opts.key, &opts.restore_to) {
        (None, _) => String::new(),
        (Some(_), Some(v)) => v.clone(),
        (Some(key), None) => current_value(&opts.model_profile, key),
    };
    let was_running = server_control::backend().is_running();
    super::interrupt::note_server(was_running);
    let port = u16::try_from(server_cfg::load().port_or_default())
        .map_err(|_| strings::t("invalid-port"))?;

    // BEFORE this sweep's own marker exists, and that ordering is the whole
    // correctness of the check: `interrupted` answers with the NEWEST marker of
    // this model profile and key, which the moment the one below is created is this
    // sweep's. Scanning after creating it therefore reports every sweep as
    // interrupted (v1.13.0 did) and, worse, hides a real leftover.
    let leftover = match (&opts.key, &opts.restore_to) {
        (Some(key), None) => interrupted(&opts.model_profile, key),
        _ => None,
    };

    let stamp = super::stamp(super::now_secs());
    let campaign = super::record::Campaign {
        name: if opts.campaign.is_empty() {
            format!("sweep-{stamp}")
        } else {
            opts.campaign.clone()
        },
        question: match (&opts.key, opts.question.is_empty()) {
            (_, false) => opts.question.clone(),
            (Some(key), true) => format!(
                "How does `{key}` change {} on model profile {}? Values: {}.",
                primary_label(opts.plan.mode),
                opts.model_profile,
                opts.values.join(", ")
            ),
            (None, true) => format!(
                "What {} does model profile {} reach as it is?",
                primary_label(opts.plan.mode),
                opts.model_profile
            ),
        },
        label: None,
    };
    let dir = super::bench_dir();
    std::fs::create_dir_all(&dir)
        .map_err(|e| strings::fill("cannot-create", &[&dir.display(), &e]))?;
    let reps = if opts.plan.reps == 1 {
        strings::fill(
            "engine-one-repetition",
            &[&opts.plan.mode.as_str(), &opts.plan.reps],
        )
    } else {
        strings::fill(
            "engine-repetitions",
            &[&opts.plan.mode.as_str(), &opts.plan.reps],
        )
    };
    if let Some(key) = &opts.key {
        let marker = pending_path(&stamp);
        std::fs::write(&marker, header_json(opts, &original, &stamp))
            .map_err(|e| strings::fill("cannot-write", &[&marker.display(), &e]))?;
        super::interrupt::hold(&opts.model_profile, key, &original, marker.clone());
        println!(
            "{}",
            strings::fill(
                "sweeping-key",
                &[&key, &opts.values.join(", "), &opts.model_profile, &reps]
            )
        );
        println!(
            "{}",
            strings::fill(
                "model-profiles-being-edited",
                &[&key, &original, &marker.display()]
            )
        );
    } else {
        println!(
            "{}",
            strings::fill("measuring-model-profile", &[&opts.model_profile, &reps])
        );
    }
    // An interrupted sweep left the key on one of ITS leg values, so the value
    // read out of models.ini above is that leftover and not the configuration
    // this machine ran. Restoring it would quietly promote a half-measured
    // value to the permanent one, so say so while there is still time to stop.
    if let (Some(key), Some((path, restore))) = (&opts.key, &leftover) {
        println!(
            "{}",
            strings::fill(
                "sweep-leftover-warning",
                &[&path.display(), &key, &original, &restore]
            )
        );
    }

    // The engine resolves the model profile's device ids to card names through the
    // probe cache, which only the GUI fills on its own; without this every leg
    // records the ids as unknown. Once, before the first leg: the ids name the
    // same cards for the whole sweep.
    crate::devices::set_probed(crate::devices::list());

    // Without a key there is one leg, on the model profile as it is.
    let as_is = [AS_IS.to_string()];
    let values: &[String] = match set {
        Some(_) => &opts.values,
        None => &as_is,
    };
    let mut legs: Vec<Leg> = Vec::new();
    for (i, value) in values.iter().enumerate() {
        if let Some(key) = &opts.key {
            println!("\n[{}/{}] {key} = {value}", i + 1, values.len());
        }
        let started = Instant::now();
        let written = match set {
            Some(set) => write_value(&opts.model_profile, set, value),
            None => Ok(()),
        };
        let leg = match written.and_then(|()| prepare_server(opts.plan.mode, port)) {
            Ok(()) => measure(opts, value, &campaign),
            Err(e) => {
                println!("{}", strings::fill("leg-failed", &[&e]));
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

    // Restore: the model profile first, then the process, so a running llama-server is
    // never left on a value models.ini no longer names.
    let primary = primary_test(opts.plan.mode, &legs);
    let applied = match opts.apply {
        _ if set.is_none() => String::new(),
        Apply::Original => original.clone(),
        Apply::Last => legs
            .last()
            .map(|l| l.value.clone())
            .unwrap_or_else(|| original.clone()),
        Apply::Best => best(&legs, &primary)
            .map(|i| legs[i].value.clone())
            .unwrap_or_else(|| original.clone()),
    };
    let restore = match set {
        Some(set) => write_value(&opts.model_profile, set, &applied),
        None => Ok(()),
    };
    let server = restore_server(was_running);
    // The key is back: this sweep's marker goes, and so do older ones of the
    // same model profile and key, whose leftover this restore has overwritten (the
    // newest sweep of a pair decides). Only after the restore, so a sweep that
    // died putting the key back still reads as interrupted.
    if let Some(key) = &opts.key {
        if restore.is_ok() {
            super::interrupt::release(&pending_path(&stamp));
            for (path, _) in markers(&opts.model_profile, key) {
                let _ = std::fs::remove_file(path);
            }
        }
        println!("\n{key} = {applied} (models.ini).");
    }

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
        println!("{}", strings::fill("campaign-recorded", &[&campaign.name]));
    } else {
        println!("{}", strings::t("no-campaign-recorded"));
    }

    let mut problems: Vec<String> = legs
        .iter()
        .filter(|l| !l.error.is_empty())
        .map(|l| match &opts.key {
            Some(key) => format!("{key} = {}: {}", l.value, l.error),
            None => l.error.clone(),
        })
        .collect();
    if let (Some(key), Err(e)) = (&opts.key, restore) {
        problems.push(strings::fill("key-not-restored", &[&key, &e]));
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

/// A leg's run label: `key=value`, the spec pass, and the concurrency scenario
/// when it is not `single`, so one campaign can hold the same value under
/// several. `None` when there is nothing to tell (no key, no spec, single):
/// the run is then labelled by its model profile.
fn leg_label(opts: &Opts, value: &str) -> Option<String> {
    let mut parts = Vec::new();
    if let Some(key) = &opts.key {
        parts.push(format!("{key}={value}"));
    }
    if let Some(tag) = &opts.plan.spec {
        parts.push(format!("pass={}", tag.pass));
    }
    if opts.plan.concurrency != super::Concurrency::Single {
        parts.push(format!("concurrency={}", opts.plan.concurrency.as_str()));
    }
    (!parts.is_empty()).then(|| parts.join(" "))
}

/// Hold `key` of `model_profile` for a temporary write (a spec pass's override): a
/// pending marker carrying the value to put back, written BEFORE the change,
/// so an interrupted run is reported (and restorable) like an interrupted
/// sweep of that key. Returns the current value and the marker.
pub fn hold_key(model_profile: &str, key: &str) -> Result<(String, PathBuf), String> {
    setter(key)?;
    let original = current_value(model_profile, key);
    let stamp = format!("{}-{key}", super::stamp(super::now_secs()));
    let dir = super::bench_dir();
    std::fs::create_dir_all(&dir)
        .map_err(|e| strings::fill("cannot-create", &[&dir.display(), &e]))?;
    let marker = pending_path(&stamp);
    let header = serde_json::json!({
        "kind": "sweep",
        "stamp": stamp,
        "preset": model_profile,
        "key": key,
        "restore": original,
        "override": true,
    });
    std::fs::write(&marker, header.to_string())
        .map_err(|e| strings::fill("cannot-write", &[&marker.display(), &e]))?;
    super::interrupt::hold(model_profile, key, &original, marker.clone());
    Ok((original, marker))
}

/// Write one key of `model_profile` (`unset` removes it).
pub fn set_key(model_profile: &str, key: &str, value: &str) -> Result<(), String> {
    write_value(model_profile, setter(key)?, value)
}

/// `set_key` for the interrupt handler, which already holds the gate.
pub(super) fn write_key(model_profile: &str, key: &str, value: &str) -> Result<(), String> {
    write_unlocked(model_profile, setter(key)?, value)
}

/// The plan one leg runs: the caller's, pinned to the swept model profile.
fn leg_plan(opts: &Opts, model_profile: &str) -> Plan {
    Plan {
        model_profiles: vec![model_profile.to_string()],
        ..opts.plan.clone()
    }
}

/// What `--concurrency all` runs for `model_profile`: one request, then, when the
/// model profile has two slots or more (absent `parallel` = llama-server's auto 4),
/// every concurrent scenario at N = its `parallel`.
pub fn all_scenarios(model_profile: &str) -> Result<Vec<Concurrency>, String> {
    let n = load_one(model_profile)?
        .parallel
        .unwrap_or(super::AUTO_PARALLEL);
    Ok(scenarios_for(n))
}

/// Whether the model profile loads an mmproj, so it can read an image prompt.
pub fn has_mmproj(model_profile: &str) -> Result<bool, String> {
    Ok(!load_one(model_profile)?.mmproj.trim().is_empty())
}

fn scenarios_for(parallel: i32) -> Vec<Concurrency> {
    if parallel >= 2 {
        Concurrency::ALL.to_vec()
    } else {
        vec![Concurrency::Single]
    }
}

fn load_one(id: &str) -> Result<model_profiles::ModelProfile, String> {
    let all = model_profiles::load_all();
    all.iter().find(|p| p.id == id).cloned().ok_or_else(|| {
        // A switched-off model profile is still listed everywhere else, so say why
        // this one cannot be swept rather than pretend it does not exist.
        if model_profiles::load_listed()
            .iter()
            .any(|l| !l.enabled && l.model_profile.id == id)
        {
            return strings::fill("model-profile-off-cannot-run", &[&id]);
        }
        let known: Vec<&str> = all.iter().map(|p| p.id.as_str()).collect();
        strings::fill(
            "unknown-model-profile",
            &[&id, &"models.ini", &known.join(", ")],
        )
    })
}

/// The key's value as the file has it right now, or `unset` when the section
/// does not carry it. Read from the INI rather than from the parsed model profile so
/// the restore puts back exactly what was there.
fn current_value(model_profile: &str, key: &str) -> String {
    server_control::read_section(ConfigFile::Models, model_profile)
        .get(key)
        .map(|v| v.trim().to_string())
        .filter(|v| !v.is_empty())
        .unwrap_or_else(|| UNSET.to_string())
}

fn write_value(model_profile: &str, set: Setter, value: &str) -> Result<(), String> {
    let _gate = super::interrupt::gate();
    write_unlocked(model_profile, set, value)
}

fn write_unlocked(model_profile: &str, set: Setter, value: &str) -> Result<(), String> {
    // Re-read rather than mutate one cached copy: the sweep may run for an hour
    // and the rest of the section belongs to whoever edited it last.
    let mut p = load_one(model_profile)?;
    set(&mut p, value)?;
    model_profiles::save(&p).map_err(|e| format!("cannot write models.ini: {e}"))
}

/// Put llama-server into the state this engine needs: up (and freshly started,
/// so it reads the value just written) for live, down for synthetic.
pub(super) fn prepare_server(mode: Mode, port: u16) -> Result<(), String> {
    match mode {
        Mode::Live => {
            {
                let _gate = super::interrupt::gate();
                let server = server_control::backend();
                server.stop()?;
                wait_gone()?;
                std::thread::sleep(GPU_SETTLE);
                // Bare start, not the `control restart` path: no snapshot save or
                // restore, see the module header.
                server
                    .start()
                    .map_err(|e| format!("cannot start llama-server: {e}"))?;
            }
            wait_ready(port)
        }
        Mode::Synthetic => {
            let _gate = super::interrupt::gate();
            let server = server_control::backend();
            if server.is_running() {
                server.stop()?;
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
/// models.ini now names) when it was running before, stopped when it was not.
pub(super) fn restore_server(was_running: bool) -> Result<(), String> {
    let _gate = super::interrupt::gate();
    put_server_back(was_running)
}

/// `restore_server` for the interrupt handler, which already holds the gate.
pub(super) fn put_server_back(was_running: bool) -> Result<(), String> {
    let server = server_control::backend();
    if was_running {
        server.stop()?;
        wait_gone()?;
        server
            .start()
            .map_err(|e| strings::fill("cannot-restart-server", &[&e]))?;
        return Ok(());
    }
    if server.is_running() {
        server.stop()?;
        wait_gone()?;
    }
    Ok(())
}

fn wait_gone() -> Result<(), String> {
    let deadline = Instant::now() + STOP_TIMEOUT;
    while server_control::backend().is_running() {
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

/// The campaign's notes in bench.db: what was swept, what the model profile was left
/// on, and the values that did not finish, which have no run of their own
/// (only finished runs are recorded) but are a result all the same.
pub fn campaign_notes(opts: &Opts, legs: &[Leg], original: &str, applied: &str) -> String {
    let reps = format!(
        "{}, {} repetition{}",
        opts.plan.mode.as_str(),
        opts.plan.reps,
        if opts.plan.reps == 1 { "" } else { "s" }
    );
    let Some(key) = &opts.key else {
        let mut out = format!(
            "run of model profile {} as it is ({reps}); models.ini not changed",
            opts.model_profile
        );
        for leg in legs.iter().filter(|l| !l.error.is_empty()) {
            out.push_str(&format!("\nnot finished: {}", leg.error));
        }
        return out;
    };
    let mut out = format!(
        "sweep of `{key}` on model profile {} ({reps}): {}\n\
         before: {original}; left on: {applied} (--apply {})",
        opts.model_profile,
        opts.values.join(", "),
        opts.apply.as_str()
    );
    for leg in legs.iter().filter(|l| !l.error.is_empty()) {
        out.push_str(&format!(
            "\nnot finished: {key} = {}: {}",
            leg.value, leg.error
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
            label: leg_label(opts, value),
            ..campaign.clone()
        }),
        ..leg_plan(opts, &opts.model_profile)
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
        println!(
            "{}",
            strings::fill("leg-failed-after", &[&leg.secs, &leg.error])
        );
        return;
    }
    let primary = primary_test(mode, std::slice::from_ref(leg));
    match leg.point(&primary) {
        Some(p) => println!(
            "{}",
            strings::fill(
                "leg-result",
                &[
                    &primary,
                    &format!("{:.1}", p.mean),
                    &format!("{:.1}", p.sd),
                    &p.n,
                    &leg.secs,
                    &if p.note.is_empty() {
                        String::new()
                    } else {
                        format!(", {}", p.note)
                    },
                ],
            )
        ),
        None => println!("{}", strings::fill("leg-no-row", &[&primary, &leg.secs])),
    }
}

/// E2E phase (an isolated test of `tests/mod.rs`): the
/// value a sweep will put back is read from the instance's models.ini through
/// the seam (the manager's file), not from the per-user one.
#[cfg(test)]
pub(crate) fn sweep_config_e2e() {
    use crate::server_control::fake::{self, FakeClient};
    use pai_core::client::InstanceState;
    let dir = tempfile::tempdir().expect("tempdir");
    crate::paths::redirect_for_tests(dir.path());
    let user_model_profiles = crate::paths::models_ini();
    std::fs::create_dir_all(user_model_profiles.parent().unwrap()).expect("config dir");
    std::fs::write(&user_model_profiles, "[Qwen3.8-27B]\nctx-size = 4096\n")
        .expect("user models.ini");
    let log = fake::log();
    let c = FakeClient::new(&log, InstanceState::Inactive).with_file(
        "models.ini",
        "[Qwen3.8-27B]\nmodel = /models/Qwen3.8-27B-Q8_0.gguf\nctx-size = 172032\n",
    );
    fake::scoped(fake::backend(c), || {
        assert_eq!(current_value("Qwen3.8-27B", "ctx-size"), "172032");
        assert_eq!(current_value("Qwen3.8-27B", "ubatch-size"), UNSET);
    });
    assert!(
        log.lock()
            .unwrap()
            .iter()
            .any(|e| e == "read_file models.ini"),
        "read from the manager: {log:?}"
    );
}

/// E2E phase (an isolated test of `tests/mod.rs`): a value
/// the setter takes but the save refuses (`parallel = 0`) fails the
/// Benchmark tab's A/B check with the save's own refusal: nothing is written,
/// no marker made. The sweep's own up-front check (`run`, which only the CLI
/// may call) is pinned by `run_refuses_a_value_the_save_refuses_before_any_leg`
/// in `mod tests`.
#[cfg(test)]
pub(crate) fn sweep_checks_values_with_the_save_e2e() {
    use crate::server_control::fake::{self, FakeClient};
    use pai_core::client::InstanceState;
    let dir = tempfile::tempdir().expect("tempdir");
    crate::paths::redirect_for_tests(dir.path());
    let log = fake::log();
    let c = FakeClient::new(&log, InstanceState::Inactive).with_file(
        "models.ini",
        "[Qwen3.8-27B]\nmodel = /models/Qwen3.8-27B-Q8_0.gguf\nparallel = 2\n",
    );
    let refused = pai_core::checks::Refusal {
        key: "not-whole-number-in-range",
        values: ["parallel", "0", "1", "2147483647"]
            .iter()
            .map(|v| v.to_string())
            .collect(),
    }
    .text();
    fake::scoped(fake::backend(c), || {
        assert_eq!(
            check_values("Qwen3.8-27B", "parallel", &["1".into(), "4".into()]),
            Ok(())
        );
        let err = check_values("Qwen3.8-27B", "parallel", &["2".into(), "0".into()])
            .expect_err("parallel 0 refused by the A/B check");
        assert!(err.contains(&refused), "A/B check: {err}");
    });
    assert_eq!(fake::writes(&log), Vec::<String>::new(), "nothing written");
    assert!(
        !super::bench_dir().exists(),
        "no bench folder, no pending marker"
    );
}

#[cfg(test)]
#[path = "sweep_tests.rs"]
mod tests;
