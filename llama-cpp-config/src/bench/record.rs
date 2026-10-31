//! A benchmark run's way into bench.db: the impure glue between `exec` (which
//! measures) and `db` (which stores).
//!
//! One `Recorder` per tab run or sweep leg. It detects the machine and the
//! software once, then holds one run per preset as the engine reaches it and
//! writes it when that preset is done, so the order of the runs in the
//! campaign is the order the presets were measured in (the tab's baseline is
//! the first). A run that fails or is cancelled is never written, and neither
//! is a campaign with no finished run (`db` module header).
//!
//! Recording is NEVER allowed to fail a benchmark: a database that cannot be
//! opened or written turns into one warning in the report and the run goes on.
//! The measurement is the expensive part; losing its index is recoverable,
//! losing the measurement is not.

use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use super::db::{self, Sample};
use super::{Mode, Plan, Point};
use crate::{gguf, ini, paths, presets, runstate, server_cfg};

/// Which campaign a run lands in, and under which label. `None` in the plan
/// means "a campaign of its own", named after the run's stamp.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Campaign {
    pub name: String,
    pub question: String,
    /// The run's label when the plan has ONE preset (a sweep leg's
    /// `key=value`); the preset id otherwise.
    pub label: Option<String>,
}

pub struct Recorder {
    path: PathBuf,
    plan: Plan,
    spec: Campaign,
    started: String,
    machine: db::Machine,
    software: db::Software,
    current: Option<Pending>,
}

/// The run being measured, held in memory until it is committed or dropped.
struct Pending {
    preset: String,
    label: String,
    started_utc: String,
    /// When the last sample landed: the run's end, read off the clock.
    ended_utc: String,
    model: db::Model,
    params: Vec<(&'static str, Vec<(String, String)>)>,
    gpus: Vec<(String, String)>,
    samples: Vec<Sample>,
    log: Arc<Mutex<String>>,
    /// The preset's `device` ids: recorded in `run_device` even when no
    /// buffer line names them.
    pinned: Vec<String>,
    /// `run_device`, read once after the first measured prompt (`read_memory`).
    devices: Vec<db::DeviceMemory>,
}

impl Recorder {
    /// `stamp` is the run's `yyyymmdd-hhmmss`, the default campaign's name.
    /// Opens the database once, so one that cannot be written is reported
    /// before the measuring starts rather than lost after it.
    pub fn begin(path: &Path, plan: &Plan, stamp: &str) -> Result<Recorder, String> {
        db::open(path)?;
        let mut spec = plan.campaign.clone().unwrap_or_else(|| Campaign {
            name: format!("bench-{stamp}"),
            question: default_question(plan),
            label: None,
        });
        if plan.presets.len() != 1 {
            spec.label = None;
        }
        Ok(Recorder {
            path: path.to_path_buf(),
            plan: plan.clone(),
            spec,
            started: iso_date(stamp),
            machine: super::host::machine(),
            software: super::host::software(),
            current: None,
        })
    }

    /// Hold a new run for one preset, with every parameter it runs with. Read
    /// NOW, not at commit: a sweep rewrites the preset between legs.
    pub fn start(
        &mut self,
        preset: &presets::Preset,
        cfg: &server_cfg::ServerConfig,
        devices: &[(String, String)],
        started_utc: &str,
    ) {
        let pairs = |m: std::collections::BTreeMap<String, String>| -> Vec<(String, String)> {
            m.into_iter().collect()
        };
        let gpus: Vec<(String, String)> = if devices.is_empty() {
            // No `device` key: llama.cpp uses every GPU it found.
            crate::devices::probed()
                .into_iter()
                .filter(|d| !d.is_cpu())
                .map(|d| (d.id, d.name))
                .collect()
        } else {
            devices.to_vec()
        };
        self.current = Some(Pending {
            preset: preset.id.clone(),
            label: self.spec.label.clone().unwrap_or_else(|| preset.id.clone()),
            started_utc: started_utc.to_string(),
            ended_utc: String::new(),
            model: model_row(Path::new(&preset.model)),
            params: vec![
                (
                    "preset",
                    pairs(ini::read_section(&paths::presets_ini(), &preset.id)),
                ),
                (
                    "server",
                    pairs(ini::read_section(&paths::server_ini(), "Server")),
                ),
                ("env", runstate::env_vars(cfg)),
                ("bench", plan_params(&self.plan)),
            ],
            gpus,
            samples: Vec::new(),
            log: Arc::new(Mutex::new(String::new())),
            pinned: devices.iter().map(|(id, _)| id.clone()).collect(),
            devices: Vec::new(),
        });
    }

    /// The preset being measured, if a run is held.
    pub fn current_preset(&self) -> Option<&str> {
        self.current.as_ref().map(|p| p.preset.as_str())
    }

    pub fn sample(&mut self, s: Sample) {
        if let Some(p) = &mut self.current {
            p.samples.push(s);
            p.ended_utc = super::stamp_human(super::now_secs());
        }
    }

    /// Where the engine's output goes, for a thread that outlives a borrow.
    pub fn log_sink(&self) -> Option<Arc<Mutex<String>>> {
        self.current.as_ref().map(|p| p.log.clone())
    }

    pub fn log(&self, text: &str) {
        if let Some(sink) = self.log_sink() {
            push_line(&sink, text);
        }
    }

    /// The held run's memory per device, read once, after its first measured
    /// prompt: the buffers of the preset's latest load off llama-server's log
    /// (`buffer_sizes`), and the free memory a fresh device probe reports now,
    /// a real prefill after the load. The live engine calls it; llama-bench
    /// is its own process and its output carries no load lines to read.
    pub fn read_memory(&mut self) {
        let Some(p) = &mut self.current else {
            return;
        };
        if !p.devices.is_empty() {
            return;
        }
        let log = std::fs::read(paths::server_log()).unwrap_or_default();
        let buffers = buffer_sizes(&String::from_utf8_lossy(&log), &p.preset);
        p.devices = device_rows(buffers, &p.pinned, &crate::devices::list());
    }

    /// Drop the held run: it failed or was cancelled.
    pub fn discard(&mut self) {
        self.current = None;
    }

    /// Write the held run, and whatever it needs (campaign, machine, software,
    /// model, prompt), in one transaction. A run without a single result is
    /// dropped like a failed one: there is nothing in it to compare.
    pub fn commit(&mut self, notes: &str, points: &[Point]) -> Result<(), String> {
        let Some(p) = self.current.take() else {
            return Ok(());
        };
        if p.samples.is_empty() {
            return Ok(());
        }
        let mut conn = db::open(&self.path)?;
        let e = |e: rusqlite::Error| format!("{}: {e}", self.path.display());
        let tx = conn.transaction().map_err(e)?;
        let campaign =
            db::campaign_id(&tx, &self.spec.name, &self.spec.question, &self.started).map_err(e)?;
        let machine = db::machine_id(&tx, &self.machine).map_err(e)?;
        let software = db::software_id(&tx, &self.software).map_err(e)?;
        let model = db::model_id(&tx, &p.model).map_err(e)?;
        let prompt = match self.plan.mode {
            Mode::Live => {
                Some(db::prompt_id(&tx, &prompt_name(&self.plan), &self.plan.prompt).map_err(e)?)
            }
            Mode::Synthetic => None,
        };
        let log = p.log.lock().map(|l| l.clone()).unwrap_or_default();
        let run = db::insert_run(
            &tx,
            &db::NewRun {
                campaign,
                label: &p.label,
                started_utc: &p.started_utc,
                ended_utc: &p.ended_utc,
                machine,
                software,
                model,
                tool: match self.plan.mode {
                    Mode::Live => "live",
                    Mode::Synthetic => "llama-bench",
                },
                notes,
                log: &log,
                points: &points_json(points),
            },
        )
        .map_err(e)?;
        for (scope, kv) in &p.params {
            db::add_params(&tx, run, scope, kv).map_err(e)?;
        }
        db::add_gpus(&tx, run, &p.gpus).map_err(e)?;
        db::add_devices(&tx, run, &p.devices).map_err(e)?;
        for s in &p.samples {
            let s = Sample {
                prompt,
                ..s.clone()
            };
            if let (Some(id), Some(n)) = (prompt, s.prompt_tokens) {
                db::prompt_tokens(&tx, id, n).map_err(e)?;
            }
            db::add_sample(&tx, run, &s).map_err(e)?;
        }
        tx.commit().map_err(e)
    }

    /// The campaign this recorder writes into.
    pub fn campaign(&self) -> &str {
        &self.spec.name
    }
}

/// The buffers the preset's LATEST load allocated, per backend, off
/// llama-server's log. The router logs `spawning server instance with
/// name=<preset> on port <port>` and hands the child's lines back prefixed
/// `[<port>] `; the load ends at the child's `llama_server: model loaded`, so
/// the reserves a request triggers later are not read. Per device:
///
/// - `model`: every `model buffer` (`load_tensors`: the weights, a separate
///   draft file's included).
/// - `kv`: every `KV buffer` (`llama_kv_cache`, one line per cache: the main
///   one, an iSWA cache's two halves, the MTP draft context's) plus every `RS
///   buffer` (`llama_memory_recurrent`, the recurrent state): all cache state.
/// - `compute`: one `compute buffer` per context, summed over the contexts
///   (`sched_reserve` after each `constructing llama_context`, plus the image
///   encoder's `reserve_compute_meta`). A context's later reserve REPLACES its
///   earlier one: it reports the same scheduler's buffer again.
///
/// `output buffer` (the logits, a few MiB of host memory) is left out.
pub fn buffer_sizes(log: &str, preset: &str) -> Vec<db::DeviceMemory> {
    let spawn = format!("spawning server instance with name={preset} on port ");
    let mut prefix = String::new();
    let mut done = true;
    let mut ctx = 0usize;
    let mut rows: Vec<db::DeviceMemory> = Vec::new();
    let mut compute: Vec<((usize, String), f64)> = Vec::new();
    for line in log.lines() {
        if let Some((_, port)) = line.split_once(&spawn) {
            prefix = format!("[{}] ", port.trim());
            (done, ctx) = (false, 0);
            rows.clear();
            compute.clear();
            continue;
        }
        let Some(body) = line.strip_prefix(prefix.as_str()).filter(|_| !done) else {
            continue;
        };
        if body.contains("llama_server: model loaded") {
            done = true;
        } else if body.contains("constructing llama_context") {
            ctx += 1;
        }
        let Some((head, tail)) = body.split_once(" buffer size =") else {
            continue;
        };
        let Some(mib) = tail
            .trim()
            .strip_suffix("MiB")
            .and_then(|v| v.trim().parse::<f64>().ok())
        else {
            continue;
        };
        let mut words = head.split_whitespace().rev();
        let (Some(kind), Some(device), Some(func)) = (words.next(), words.next(), words.next())
        else {
            continue;
        };
        match kind {
            "model" => *row(&mut rows, device).model_mib.get_or_insert(0.0) += mib,
            "KV" | "RS" => *row(&mut rows, device).kv_mib.get_or_insert(0.0) += mib,
            "compute" => {
                row(&mut rows, device);
                // The image encoder is a context of its own, never constructed
                // through llama_context.
                let owner = if func == "reserve_compute_meta:" {
                    usize::MAX
                } else {
                    ctx
                };
                let key = (owner, device.to_string());
                match compute.iter_mut().find(|(k, _)| *k == key) {
                    Some(slot) => slot.1 = mib,
                    None => compute.push((key, mib)),
                }
            }
            _ => {}
        }
    }
    for ((_, device), mib) in compute {
        if let Some(r) = rows.iter_mut().find(|r| r.device == device) {
            *r.compute_mib.get_or_insert(0.0) += mib;
        }
    }
    let round = |v: &mut Option<f64>| *v = v.map(|x| (x * 100.0).round() / 100.0);
    for r in &mut rows {
        round(&mut r.model_mib);
        round(&mut r.kv_mib);
        round(&mut r.compute_mib);
    }
    rows
}

/// The device's row, appended on first sight so the rows keep the log's order.
fn row<'a>(rows: &'a mut Vec<db::DeviceMemory>, device: &str) -> &'a mut db::DeviceMemory {
    let i = match rows.iter().position(|r| r.device == device) {
        Some(i) => i,
        None => {
            rows.push(db::DeviceMemory {
                device: device.to_string(),
                ..Default::default()
            });
            rows.len() - 1
        }
    };
    &mut rows[i]
}

/// `run_device`'s rows: the devices the buffers name, then the preset's pinned
/// ones no buffer line named, each with the free memory the probe reports for
/// that id (none for a host buffer such as `CPU_Mapped` or `ROCm_Host`, which
/// the probe does not list).
pub fn device_rows(
    mut rows: Vec<db::DeviceMemory>,
    pinned: &[String],
    probe: &[crate::devices::DeviceOption],
) -> Vec<db::DeviceMemory> {
    for id in pinned {
        if !rows.iter().any(|r| &r.device == id) {
            rows.push(db::DeviceMemory {
                device: id.clone(),
                ..Default::default()
            });
        }
    }
    for r in &mut rows {
        r.free_mib = probe
            .iter()
            .find(|d| d.id == r.device && d.total_mib > 0)
            .map(|d| d.free_mib as i64);
    }
    rows
}

/// The tab's means of one run, as stored in `run.points`; the preset is the
/// run itself, so it is not repeated.
pub fn points_json(points: &[Point]) -> String {
    serde_json::Value::Array(
        points
            .iter()
            .map(|p| {
                serde_json::json!({
                    "test": p.test, "mean": p.mean, "sd": p.sd, "n": p.n, "note": p.note,
                })
            })
            .collect(),
    )
    .to_string()
}

/// `run.points` back into points, under the run's label as their preset.
pub fn points_from_json(label: &str, json: &str) -> Vec<Point> {
    let Ok(serde_json::Value::Array(a)) = serde_json::from_str::<serde_json::Value>(json) else {
        return Vec::new();
    };
    a.iter()
        .map(|v| Point {
            preset: label.to_string(),
            test: v["test"].as_str().unwrap_or_default().to_string(),
            mean: v["mean"].as_f64().unwrap_or(0.0),
            sd: v["sd"].as_f64().unwrap_or(0.0),
            n: v["n"].as_i64().unwrap_or(0) as i32,
            note: v["note"].as_str().unwrap_or_default().to_string(),
        })
        .collect()
}

pub fn push_line(sink: &Mutex<String>, text: &str) {
    if let Ok(mut l) = sink.lock() {
        l.push_str(text);
        l.push('\n');
    }
}

fn default_question(plan: &Plan) -> String {
    format!(
        "{} benchmark of {}",
        match plan.mode {
            Mode::Live => "Live",
            Mode::Synthetic => "Synthetic",
        },
        plan.presets.join(", ")
    )
}

/// `20261016-181500` into `2026-10-16`.
pub fn iso_date(stamp: &str) -> String {
    let d = stamp.get(..8).unwrap_or(stamp);
    if d.len() == 8 && d.chars().all(|c| c.is_ascii_digit()) {
        format!("{}-{}-{}", &d[..4], &d[4..6], &d[6..])
    } else {
        stamp.to_string()
    }
}

/// The prompt's name: its file's stem, or `inline` for `--prompt`.
fn prompt_name(plan: &Plan) -> String {
    plan.prompt_file
        .as_ref()
        .and_then(|p| p.file_stem())
        .map(|s| s.to_string_lossy().into_owned())
        .unwrap_or_else(|| "inline".into())
}

/// The plan's half that the mode reads, as `bench` parameters.
pub fn plan_params(plan: &Plan) -> Vec<(String, String)> {
    let list = |v: &[i32]| super::render_int_list(v);
    let mut out = vec![
        ("mode".to_string(), plan.mode.as_str().to_string()),
        ("reps".to_string(), plan.reps.to_string()),
    ];
    match &plan.spec {
        None => out.push(("spec".into(), "custom".into())),
        // Overridden from the tab: not the spec's method, so custom, with
        // what it started from and what it changed.
        Some(t) if !t.derived.is_empty() => {
            out.push(("spec".into(), "custom".into()));
            out.push(("spec-derived-from".into(), format!("{} v{}", t.name, t.version)));
            out.push(("spec-derived-sha256".into(), t.sha256.clone()));
            out.push(("spec-derived-fields".into(), t.derived.join(",")));
            out.push(("spec-pass".into(), t.pass.clone()));
        }
        Some(t) => {
            out.push(("spec".into(), t.name.clone()));
            out.push(("spec-version".into(), t.version.clone()));
            out.push(("spec-sha256".into(), t.sha256.clone()));
            out.push(("spec-pass".into(), t.pass.clone()));
        }
    }
    match plan.mode {
        Mode::Live => {
            out.push((
                "temp".into(),
                plan.temp
                    .map(|t| t.to_string())
                    .unwrap_or_else(|| "preset".into()),
            ));
            out.push((
                "max-tokens".into(),
                if plan.max_tokens > 0 {
                    plan.max_tokens.to_string()
                } else {
                    "none".into()
                },
            ));
            out.push(("concurrency".into(), plan.concurrency.as_str().into()));
            out.push(("prompt".into(), prompt_name(plan)));
        }
        Mode::Synthetic => {
            out.push(("prompt-lens".into(), list(&plan.prompt_lens)));
            out.push(("depths".into(), list(&plan.depths)));
            out.push(("n-gen".into(), plan.n_gen.to_string()));
        }
    }
    out
}

/// `X-00001-of-00003.gguf` and its siblings: every shard of a split model.
fn shards(first: &Path) -> Vec<PathBuf> {
    let name = first
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_default();
    let Some(pos) = name.find("-00001-of-") else {
        return vec![first.to_path_buf()];
    };
    let (head, tail) = (&name[..pos], &name[pos + "-00001".len()..]);
    let dir = first.parent().unwrap_or(Path::new("."));
    let mut out: Vec<PathBuf> = std::fs::read_dir(dir)
        .map(|d| {
            d.flatten()
                .map(|e| e.path())
                .filter(|p| {
                    p.file_name()
                        .and_then(|n| n.to_str())
                        .is_some_and(|n| n.starts_with(head) && n.ends_with(tail))
                })
                .collect()
        })
        .unwrap_or_default();
    if out.is_empty() {
        out.push(first.to_path_buf());
    }
    out
}

/// `27B` into 27.0; `35B-A3B` into 35.0. `None` for anything else.
pub fn params_b(size_label: &str) -> Option<f64> {
    let head = size_label.split('-').next()?.trim();
    let (num, mult) = match head.chars().last()? {
        'B' | 'b' => (&head[..head.len() - 1], 1.0),
        'M' | 'm' => (&head[..head.len() - 1], 1e-3),
        'T' | 't' => (&head[..head.len() - 1], 1e3),
        _ => return None,
    };
    let num = num.rsplit('x').next()?;
    num.parse::<f64>().ok().map(|v| v * mult)
}

pub fn model_row(path: &Path) -> db::Model {
    let file = path
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_default();
    let size_bytes: u64 = shards(path)
        .iter()
        .filter_map(|p| std::fs::metadata(p).ok())
        .map(|m| m.len())
        .sum();
    let stem = file.trim_end_matches(".gguf");
    let name = match stem.find("-00001-of-") {
        Some(i) => &stem[..i],
        None => stem,
    }
    .to_string();
    let info = gguf::read_model_info(path);
    let opt = |s: &str| (!s.is_empty()).then(|| s.to_string());
    db::Model {
        file,
        size_bytes: size_bytes as i64,
        name,
        arch: info.as_ref().and_then(|i| opt(&i.arch)),
        quant: info.as_ref().and_then(|i| opt(&i.quant)),
        params_b: info.as_ref().and_then(|i| params_b(&i.size_label)),
        layers: info.as_ref().map(|i| i.n_layer as i64).filter(|&n| n > 0),
        experts: info.as_ref().filter(|i| i.is_moe).map(|i| {
            let mut s = format!("{} per layer, {} used", i.expert_count, i.expert_used);
            if i.expert_shared_count > 0 {
                s.push_str(&format!(", plus {} shared", i.expert_shared_count));
            }
            s
        }),
        mtp: info.as_ref().is_some_and(|i| i.nextn_predict_layers > 0),
    }
}

/// One llama-bench jsonl row into one sample per repetition (`samples_ts`),
/// or one sample at the mean when the row carries no per-repetition rates.
/// The verbatim row rides on the first: everything llama-bench measured that
/// the columns drop (flash_attn, n_batch, backends, gpu_info) stays readable.
pub fn bench_samples(line: &str) -> Vec<Sample> {
    let Ok(v) = serde_json::from_str::<serde_json::Value>(line.trim()) else {
        return Vec::new();
    };
    let int = |k: &str| v.get(k).and_then(serde_json::Value::as_i64).unwrap_or(0);
    let Some(avg) = v.get("avg_ts").and_then(serde_json::Value::as_f64) else {
        return Vec::new();
    };
    let (n_prompt, n_gen) = (int("n_prompt"), int("n_gen"));
    let test = super::bench_test_label(n_prompt, n_gen, int("n_depth"));
    let rates: Vec<f64> = v
        .get("samples_ts")
        .and_then(serde_json::Value::as_array)
        .map(|a| a.iter().filter_map(serde_json::Value::as_f64).collect())
        .filter(|r: &Vec<f64>| !r.is_empty())
        .unwrap_or_else(|| vec![avg]);
    rates
        .iter()
        .enumerate()
        .map(|(i, &tps)| Sample {
            rep: i as i64 + 1,
            test: Some(test.clone()),
            prompt_tokens: (n_prompt > 0).then_some(n_prompt),
            gen_tokens: (n_gen > 0).then_some(n_gen),
            prefill_tps: (n_prompt > 0).then_some(tps),
            decode_tps: (n_prompt == 0).then_some(tps),
            detail: (i == 0).then(|| v.to_string()),
            ..Default::default()
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn size_labels_read_as_billions() {
        assert_eq!(params_b("27B"), Some(27.0));
        assert_eq!(params_b("35B-A3B"), Some(35.0));
        assert_eq!(params_b("8x7B"), Some(7.0));
        assert_eq!(params_b("600M"), Some(0.6));
        assert_eq!(params_b(""), None);
        assert_eq!(params_b("big"), None);
    }

    #[test]
    fn stamps_become_dates() {
        assert_eq!(iso_date("20261016-181500"), "2026-10-16");
        assert_eq!(iso_date("renamed"), "renamed");
    }

    #[test]
    fn a_bench_row_gives_one_sample_per_repetition() {
        let row = r#"{"n_prompt":2048,"n_gen":0,"n_depth":8192,"avg_ts":1000.0,"samples_ts":[990.0,1010.0],"flash_attn":true}"#;
        let s = bench_samples(row);
        assert_eq!(s.len(), 2);
        assert_eq!(s[0].test.as_deref(), Some("pp2048 @ d8192"));
        assert_eq!((s[0].rep, s[1].rep), (1, 2));
        assert_eq!(s[1].prefill_tps, Some(1010.0));
        assert_eq!(s[1].decode_tps, None);
        assert!(s[0].detail.as_deref().unwrap().contains("flash_attn"));
        assert!(s[1].detail.is_none());

        let tg = r#"{"n_prompt":0,"n_gen":128,"n_depth":0,"avg_ts":35.5}"#;
        let s = bench_samples(tg);
        assert_eq!(s.len(), 1);
        assert_eq!(s[0].decode_tps, Some(35.5));
        assert_eq!(s[0].gen_tokens, Some(128));
        assert!(bench_samples("not json").is_empty());
        assert!(bench_samples(r#"{"build_number":11669}"#).is_empty());
    }

    /// Real lines (llama.cpp v0.7.0, Qwen3.8-27B with MTP heads and an mmproj),
    /// trimmed: an earlier load of the same preset, another model's child, the
    /// draft context's re-reserve and a request's reserve after the load.
    const LOAD_LOG: &str = "\
0.00.200.000 I srv          load: spawning server instance with name=Qwen3.8-27B on port 40001
[40001] 0.00.615.092 I load_tensors:        ROCm0 model buffer size = 99999.00 MiB
0.02.549.361 I srv          load: spawning server instance with name=Qwen3.8-Flash-Next on port 57115
0.02.549.361 I srv          load: spawning server instance with name=Qwen3.8-27B on port 52043
[57115] 0.00.555.535 I load_tensors:          CPU model buffer size = 10047.07 MiB
[52043] 0.00.615.092 I load_tensors:        CUDA0 model buffer size =  5995.64 MiB
[52043] 0.00.615.093 I load_tensors:    CUDA_Host model buffer size =  1288.28 MiB
[52043] 0.00.615.093 I load_tensors:        ROCm0 model buffer size = 20428.15 MiB
[52043] 0.07.459.188 I llama_context: constructing llama_context
[52043] 0.07.460.048 I llama_context:  ROCm_Host  output buffer size =     3.79 MiB
[52043] 0.07.461.022 I llama_kv_cache:      CUDA0 KV buffer size =  2688.00 MiB
[52043] 0.07.467.479 I llama_kv_cache:      ROCm0 KV buffer size =  8064.00 MiB
[52043] 0.07.484.154 I llama_kv_cache: size = 10752.00 MiB (172032 cells,  16 layers,  1/1 seqs), K (f16): 5376.00 MiB, V (f16): 5376.00 MiB
[52043] 0.07.484.845 I llama_memory_recurrent:      CUDA0 RS buffer size =   149.62 MiB
[52043] 0.07.485.744 I llama_memory_recurrent:      ROCm0 RS buffer size =   448.88 MiB
[52043] 0.07.485.756 I sched_reserve: reserving ...
[52043] 0.07.560.383 I sched_reserve:      CUDA0 compute buffer size =   620.05 MiB
[52043] 0.07.560.386 I sched_reserve:      ROCm0 compute buffer size =   620.03 MiB
[52043] 0.07.560.386 I sched_reserve:  CUDA_Host compute buffer size =   376.05 MiB
[52043] 0.07.747.736 I llama_context: constructing llama_context
[52043] 0.07.749.658 I llama_kv_cache:      ROCm0 KV buffer size =   672.00 MiB
[52043] 0.07.811.618 I sched_reserve:      CUDA0 compute buffer size =    20.00 MiB
[52043] 0.07.811.621 I sched_reserve:      ROCm0 compute buffer size =   472.03 MiB
[52043] 0.07.811.622 I sched_reserve:  CUDA_Host compute buffer size =   356.04 MiB
[52043] 0.08.211.732 I reserve_compute_meta:      ROCm0 compute buffer size =   248.10 MiB
[52043] 0.08.211.734 I reserve_compute_meta:        CPU compute buffer size =    24.93 MiB
[52043] 0.08.214.904 I sched_reserve: reserving ...
[52043] 0.08.291.800 I sched_reserve:      CUDA0 compute buffer size =    20.00 MiB
[52043] 0.08.291.804 I sched_reserve:      ROCm0 compute buffer size =   472.03 MiB
[52043] 0.08.291.805 I sched_reserve:  CUDA_Host compute buffer size =   356.04 MiB
[52043] 0.08.301.600 I srv  llama_server: model loaded
[52043] 0.14.447.694 I sched_reserve:      ROCm0 compute buffer size =  9999.00 MiB
";

    #[test]
    fn buffer_sizes_sum_the_latest_load_per_device() {
        let rows = buffer_sizes(LOAD_LOG, "Qwen3.8-27B");
        let got: Vec<_> = rows
            .iter()
            .map(|r| (r.device.as_str(), r.model_mib, r.kv_mib, r.compute_mib))
            .collect();
        assert_eq!(
            got,
            vec![
                ("CUDA0", Some(5995.64), Some(2837.62), Some(640.05)),
                ("CUDA_Host", Some(1288.28), None, Some(732.09)),
                ("ROCm0", Some(20428.15), Some(9184.88), Some(1340.16)),
                ("CPU", None, None, Some(24.93)),
            ]
        );
        assert!(rows.iter().all(|r| r.free_mib.is_none()));
        assert!(buffer_sizes(LOAD_LOG, "Qwen3.8").is_empty());
        assert!(buffer_sizes("", "Qwen3.8-27B").is_empty());
    }

    #[test]
    fn device_rows_add_pinned_ids_and_the_probe_free_memory() {
        let dev = |id: &str, total: u64, free: u64| crate::devices::DeviceOption {
            id: id.into(),
            label: String::new(),
            name: String::new(),
            total_mib: total,
            free_mib: free,
        };
        let buffers = buffer_sizes(LOAD_LOG, "Qwen3.8-27B");
        let probe = [
            dev("CUDA0", 11876, 1200),
            dev("ROCm0", 32624, 900),
            dev("CPU", 60857, 30000),
        ];
        let rows = device_rows(buffers, &["ROCm0".into(), "Vulkan2".into()], &probe);
        let free: Vec<(&str, Option<i64>)> = rows
            .iter()
            .map(|r| (r.device.as_str(), r.free_mib))
            .collect();
        assert_eq!(
            free,
            vec![
                ("CUDA0", Some(1200)),
                ("CUDA_Host", None),
                ("ROCm0", Some(900)),
                ("CPU", Some(30000)),
                ("Vulkan2", None),
            ]
        );
    }

    #[test]
    fn plan_params_follow_the_mode() {
        let live = Plan {
            prompt_file: Some(PathBuf::from("bench-prompt-43k.txt")),
            ..Plan::default()
        };
        let p = plan_params(&live);
        assert!(p.contains(&("prompt".into(), "bench-prompt-43k".into())));
        assert!(p.contains(&("temp".into(), "0".into())));
        assert!(p.contains(&("spec".into(), "custom".into())));
        let tagged = Plan {
            spec: Some(super::super::spec::Tag {
                name: "standard".into(),
                version: "1".into(),
                sha256: "abc".into(),
                pass: "engine".into(),
                derived: Vec::new(),
            }),
            ..live.clone()
        };
        let p = plan_params(&tagged);
        assert!(p.contains(&("spec".into(), "standard".into())));
        assert!(p.contains(&("spec-version".into(), "1".into())));
        assert!(p.contains(&("spec-pass".into(), "engine".into())));
        assert!(!p.iter().any(|(k, _)| k == "depths"));
        let mut derived = tagged.clone();
        derived.spec.as_mut().unwrap().derived = vec!["reps".into()];
        let p = plan_params(&derived);
        assert!(p.contains(&("spec".into(), "custom".into())));
        assert!(p.contains(&("spec-derived-from".into(), "standard v1".into())));
        assert!(p.contains(&("spec-derived-fields".into(), "reps".into())));
        assert!(!p.iter().any(|(k, _)| k == "spec-version"));
        let syn = Plan {
            mode: Mode::Synthetic,
            ..Plan::default()
        };
        assert!(plan_params(&syn).iter().any(|(k, _)| k == "depths"));
    }
}
