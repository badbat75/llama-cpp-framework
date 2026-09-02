//! Benchmark planning, argument building and result aggregation: the pure half
//! of the Benchmark tab. The execution (spawning llama-bench, talking to a live
//! llama-server) is `bench/exec.rs`; this file is unit-testable and touches no
//! process and no socket.
//!
//! ## Two engines, because one measurement cannot be both things
//!
//! **Live** (`Mode::Live`) drives the RUNNING llama-server: one
//! `/v1/chat/completions` per repetition, with a real prompt and a temperature
//! override, reading the `timings` block off the response. It is the only mode
//! that measures the configuration the framework actually ships: the preset's
//! chat template, its speculative drafter (MTP / DFlash), its KV cache, its
//! `--parallel`. Its numbers are production numbers.
//!
//! **Synthetic** (`Mode::Synthetic`) runs `llama-bench`, which takes no prompt
//! and no sampler settings AT ALL: `-p` is a token COUNT, not a text, and the
//! tool never samples (`tools/llama-bench/README.md`: "the measurements with
//! llama-bench do not include the times for tokenization and for sampling").
//! That is a stronger reproducibility guarantee than a fixed prompt at
//! temperature 0, not a weaker one, since the result cannot depend on what the
//! model says. The price is that it cannot load the preset's drafter, so its
//! `tg` reads roughly HALF of what the same preset does live with MTP on, and
//! it ignores the mmproj, the chat template and every reasoning flag. The
//! default sweep here (`DEFAULT_PROMPT_LENS` / `DEFAULT_DEPTHS` / `DEFAULT_GEN`)
//! is carried over from the shell harness this tab replaced, so runs recorded
//! before it stay comparable with runs recorded after.
//!
//! ## The live prompt is a FILE, and the GUI edits that file
//!
//! `Plan.prompt` is the resolved TEXT, but nothing stores it: the GUI and the
//! CLI both carry a PATH (`Plan.prompt_file`, default
//! `config\bench-prompt.txt`) and read it once, when the run starts. Three
//! things follow, and each one was a defect of the field-in-the-window it
//! replaced.
//!
//! *One source for both front ends.* The CLI grew `--prompt-file` because the
//! interesting prompt is a long one (a decode rate at 40k of context is a
//! different number from the same setting's at 2k) and that does not fit on a
//! command line. A GUI text box next to it is a second prompt that no flag can
//! reach, so the two would measure different things without saying so. Now
//! `bench sweep` with no flag and the tab's Run button read the same bytes.
//!
//! *The prompt SHIPS without being installed.* `DEFAULT_PROMPT` is a seed:
//! `ensure_prompt_file` writes it on first use. Installing the file into
//! `bin\` instead would put it under `$PROGRAMFILES64\llama.cpp`, where the
//! editor could not save it without elevation and the next upgrade's `File`
//! directive would overwrite the user's edit.
//!
//! *Identity, not a copy.* The prompt was previously embedded verbatim in the
//! report, which is what let an old comparison be checked. A file that anyone
//! can edit between two runs breaks that, and a 100k-character prompt breaks
//! the embedding too, so the report carries the path, the size and the
//! **sha256** of the normalized text, and embeds the text up to
//! `REPORT_PROMPT_CHARS`. `normalize_prompt` is why the digest means anything:
//! the same prompt saved by Notepad (CRLF, maybe a BOM) and by the built-in
//! editor must hash the same, because it tokenizes the same.
//!
//! ## The unit is the PRESET, never the model file
//!
//! Two presets may point at the same `.gguf` with different placement, KV types
//! or drafters, and comparing exactly that pair is the most useful thing this
//! tab does. So the selection is a list of preset ids, the results are keyed by
//! preset id, and the ratio column is against the FIRST selected preset.
//!
//! ## Three separator traps between an INI value and a llama-bench flag
//!
//! Every one of them is silent: llama-bench accepts the value and benchmarks
//! something other than what the preset describes.
//!
//!  * `--tensor-split` is `/`-separated for llama-bench, `,`-separated in the
//!    INI. A comma there means "run these as SEPARATE configurations", so
//!    `54,12` turns one split into two benchmarks of one device each.
//!  * `--override-tensor` is `;`-separated for llama-bench, `,`-separated in the
//!    INI (llama.cpp's own `-ot` parser splits rules on `,`). llama-bench splits
//!    on `,` FIRST, into configuration groups, and only then on `;` into rules
//!    (`llama-bench.cpp`), so a two-rule preset value becomes two runs with one
//!    rule each.
//!  * `--device` is `/`-separated for llama-bench, `,`-separated in the INI, with
//!    the same "separate configurations" meaning for a comma.
//!
//! ## The server-wide keys shadow the preset's, here as at launch
//!
//! llama-server's router merges its own CLI args over every preset
//! (`preset.merge(base_preset)`), so a server-wide `device` / `split-mode` /
//! `override-tensor` is what a model really runs with. `effective` applies that
//! same shadowing, or the synthetic mode would benchmark a placement the machine
//! never uses. The live mode needs no equivalent: it asks the real server, which
//! has already done the merge.

use std::collections::BTreeMap;
use std::path::PathBuf;

use crate::presets::Preset;
use crate::server_cfg::ServerConfig;

pub mod env;
pub mod exec;
pub mod sweep;

// ── Defaults ─────────────────────────────────────────────────────────────

/// Prompt LENGTHS for the synthetic prefill sweep, and KV depths for its decode
/// sweep, both carried over from the shell harness this tab replaced so its
/// older results stay comparable with these.
pub const DEFAULT_PROMPT_LENS: &str = "2048,8192,32768";
pub const DEFAULT_DEPTHS: &str = "0,8192,32768";
pub const DEFAULT_GEN: i32 = 128;

/// Samples per measurement, in BOTH modes: llama-bench's `-r`, and how many
/// times the live mode repeats its request. One constant because the two are
/// the same instruction ("how much spread do you want to see"), even though a
/// live repetition costs a whole cold prefill and a synthetic one does not.
pub const DEFAULT_REPS: i32 = 3;

/// Seconds between llama-bench tests. Deliberately BELOW the ~9.5 s WDDM idle
/// threshold that evicts a headless GPU's VRAM on this class of machine: a
/// longer settle time risks measuring a page-in instead of a kernel.
pub const DEFAULT_DELAY_SECS: i32 = 2;

/// Output cap for the live request. Long enough that the decode rate settles,
/// short enough that a slow model does not turn one repetition into minutes.
pub const DEFAULT_MAX_TOKENS: i32 = 256;

/// The live mode's default prompt. The synthetic engine has no equivalent to
/// inherit (its prompt parameter is a length, not a text), so this is the
/// framework's own: fixed, self-contained, and phrased to pull a few hundred
/// tokens of ordinary prose out of any instruct model, since a prompt that some
/// models answer in one line and others in twenty is not a benchmark.
///
/// It is a SEED, not the prompt: `ensure_prompt_file` writes it to
/// `config\bench-prompt.txt` the first time anything asks for a prompt, and
/// every reader goes to the file from then on. That is what makes the shipped
/// prompt editable (and replaceable) without an installer that writes into
/// `$PROGRAMFILES64` and overwrites the edit on the next upgrade.
pub const DEFAULT_PROMPT: &str = "Explain how a transformer decoder's key-value cache works, \
why the cost of processing a prompt grows with its length, and what a speculative decoding \
draft model changes about the decode step. Write about 200 words of plain prose.";

/// The long-context prompt, shipped beside the short one and seeded as
/// `config\bench-prompt-long.txt`: ~169k characters, roughly 40k tokens.
///
/// It is here because the default prompt measures a regime nobody works in. A
/// benchmark ranks settings in the regime it measured, and 230 characters is
/// the shallow end: at 40k of context the same preset that benchmarks at 57
/// t/s serves at 24 to 40, and the RANKING moves too (a verify pass costs more
/// relative to a draft iteration at depth, so `spec-draft-n-max` peaks at 5
/// near the start of a conversation and at 3 at 43k). Shipping only the short
/// prompt would mean shipping the tab's least useful measurement.
///
/// Compiled in rather than installed, for the same reason `DEFAULT_PROMPT` is:
/// see the module header. It is a frozen snapshot of this repo's own docs;
/// `assets\README.md` says why it must not be regenerated when they change.
pub const LONG_PROMPT: &str = include_str!("../assets/bench-prompt-long.txt");

/// How much of the prompt the pasteable preview shows before eliding. The
/// preview is a request body meant to be read at a glance and pasted into
/// `curl`; a 100k-character prompt inlined there is neither.
const PREVIEW_PROMPT_CHARS: usize = 400;

/// How much of the prompt the saved report embeds verbatim. Short prompts (the
/// shipped one is 230 characters) stay wholly readable in the report, which is
/// what made an old comparison checkable; a long one would otherwise turn a
/// 4 KB report into a 400 KB one, so past that the sha256 in the settings block
/// is what identifies it.
const REPORT_PROMPT_CHARS: usize = 4096;

// ── Mode ─────────────────────────────────────────────────────────────────

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Mode {
    /// Real requests to the running llama-server (prompt + temperature).
    Live,
    /// `llama-bench` over synthetic token counts (no prompt, no sampling).
    Synthetic,
}

impl Mode {
    pub fn as_str(self) -> &'static str {
        match self {
            Mode::Live => "live",
            Mode::Synthetic => "synthetic",
        }
    }

    pub fn from_str(s: &str) -> Mode {
        if s == "synthetic" {
            Mode::Synthetic
        } else {
            Mode::Live
        }
    }
}

// ── The live prompt lives in a FILE ──────────────────────────────────────

/// Fold a prompt file's raw contents into the text that will be sent.
///
/// Two normalizations, both of which decide whether two runs are comparable at
/// all, since the tokenizer sees exactly these bytes:
///  * a leading UTF-8 BOM is dropped. Notepad still writes one on request and
///    several editors do it silently; left in, it is a token of the prompt and
///    an invisible difference between "the same" prompt saved two ways.
///  * CRLF becomes LF. The same text edited in the built-in window (which
///    writes what the widget holds) and in Notepad (which writes CRLF) would
///    otherwise tokenize differently, and the digest below would call them
///    different prompts, correctly but uselessly.
///
/// Trailing whitespace is NOT touched: a prompt ending in a newline or not is a
/// deliberate difference (some templates are sensitive to it), and this is the
/// wrong place to decide it.
pub fn normalize_prompt(raw: &str) -> String {
    raw.strip_prefix('\u{feff}')
        .unwrap_or(raw)
        .replace("\r\n", "\n")
}

/// Read a prompt file, normalized. The error is the message shown verbatim in
/// the GUI footer and printed by the CLI, so it names the path: "the prompt is
/// empty" without saying which file is useless once the path is configurable.
pub fn load_prompt_file(path: &std::path::Path) -> Result<String, String> {
    let raw = std::fs::read_to_string(path)
        .map_err(|e| format!("cannot read the prompt file {}: {e}", path.display()))?;
    let text = normalize_prompt(&raw);
    if text.trim().is_empty() {
        return Err(format!("the prompt file {} is empty", path.display()));
    }
    Ok(text)
}

/// Which shipped prompt seeds this path, decided by FILE NAME.
///
/// The long prompt has to be restorable: delete `bench-prompt-long.txt` and the
/// next seed must put the long text back, not quietly fill the file with the
/// 230-character default and leave a benchmark measuring the shallow regime
/// under a name that promises the deep one. Any other name is a file the user
/// named, and the short prompt is the better thing to start it from.
fn seed_for(path: &std::path::Path) -> &'static str {
    let long = crate::paths::bench_long_prompt_file();
    match (path.file_name(), long.file_name()) {
        (Some(a), Some(b)) if a == b => LONG_PROMPT,
        _ => DEFAULT_PROMPT,
    }
}

/// The configured prompt file, seeded from the binary when it does not exist
/// yet, so a fresh install has a prompt to run and to edit without the
/// installer having placed one anywhere.
///
/// Seeding only ever CREATES: an existing file is never rewritten (the user's
/// edits are the point), and a failed write is not fatal here, since the caller
/// reports the read failure that follows with a better message than a write
/// error nobody asked for.
pub fn ensure_prompt_file(path: &std::path::Path) -> &std::path::Path {
    if !path.exists() {
        if let Some(parent) = path.parent() {
            let _ = std::fs::create_dir_all(parent);
        }
        let _ = crate::ini::atomic_write(path, seed_for(path));
    }
    path
}

/// Put BOTH shipped prompts on disk: the short default and the long one.
///
/// The long one is seeded even though nothing points at it yet, because a file
/// nobody can find is a file nobody uses: Browse… opens on the configured
/// prompt's own folder, so the two sit side by side and choosing the deep
/// regime is one click rather than a documentation lookup.
pub fn ensure_shipped_prompt_files() {
    ensure_prompt_file(&crate::paths::bench_prompt_file());
    ensure_prompt_file(&crate::paths::bench_long_prompt_file());
}

/// The prompt's identity, for the report and for the tab's readout.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct PromptInfo {
    pub chars: usize,
    pub bytes: usize,
    /// sha256 of the NORMALIZED text (see `normalize_prompt`), which is what
    /// reaches the model. Deliberately not the file's bytes: two files that
    /// send the identical prompt must produce the identical digest, and the
    /// price is that this does not match `Get-FileHash` on a CRLF file.
    pub sha256: String,
}

pub fn prompt_info(text: &str) -> PromptInfo {
    PromptInfo {
        chars: text.chars().count(),
        bytes: text.len(),
        sha256: crate::sha256::hex(text.as_bytes()),
    }
}

/// The one-line readout under the Benchmark tab's prompt field.
pub fn prompt_summary(text: &str) -> String {
    let info = prompt_info(text);
    format!(
        "{} chars, {} bytes, sha256 {}",
        info.chars,
        info.bytes,
        crate::sha256::short(text.as_bytes())
    )
}

// ── Plan ─────────────────────────────────────────────────────────────────

/// One benchmark run as configured in the GUI. Both modes' fields live in one
/// struct (the tab keeps the workload card's values across a mode switch), and
/// each mode reads only its own half.
#[derive(Clone, Debug, PartialEq)]
pub struct Plan {
    pub mode: Mode,
    /// Preset ids in run order. The first is the baseline of the ratio column.
    pub presets: Vec<String>,
    pub reps: i32,
    // Live half.
    /// The text that will be sent, already resolved. Whoever builds the plan
    /// reads `prompt_file` into this, ONCE, at the moment the run starts: the
    /// GUI keeps only a path, so a prompt edited in Notepad while the tab sits
    /// open is picked up, and a cached copy can never be what gets benchmarked.
    pub prompt: String,
    /// Where the text above came from, for the report. `None` means it was
    /// given inline (`bench --prompt`), which is the one way to run a prompt
    /// that is not in a file.
    pub prompt_file: Option<PathBuf>,
    /// `None` leaves the preset's own `--temp` alone. The default is `Some(0.0)`:
    /// greedy sampling is what makes two runs of the same prompt comparable.
    pub temp: Option<f64>,
    pub max_tokens: i32,
    // Synthetic half.
    pub prompt_lens: Vec<i32>,
    pub depths: Vec<i32>,
    /// `-n`, llama-bench's generation length. Not named `gen`: that is a
    /// keyword from edition 2024 on.
    pub n_gen: i32,
}

impl Default for Plan {
    fn default() -> Self {
        Plan {
            mode: Mode::Live,
            presets: Vec::new(),
            reps: DEFAULT_REPS,
            // Unresolved: the caller reads `prompt_file` into it. Defaulting to
            // DEFAULT_PROMPT here would make a plan whose prompt disagrees with
            // the file every reader is pointed at, silently, whenever the read
            // step is forgotten.
            prompt: String::new(),
            prompt_file: Some(crate::paths::bench_prompt_file()),
            temp: Some(0.0),
            max_tokens: DEFAULT_MAX_TOKENS,
            prompt_lens: parse_int_list(DEFAULT_PROMPT_LENS),
            depths: parse_int_list(DEFAULT_DEPTHS),
            n_gen: DEFAULT_GEN,
        }
    }
}

/// Parse a comma-separated integer list ("2048, 8192"), dropping anything that
/// is not a number. Empty in, empty out; the caller decides whether that is an
/// error (a sweep with no lengths benchmarks nothing).
pub fn parse_int_list(s: &str) -> Vec<i32> {
    s.split(',')
        .map(str::trim)
        .filter(|p| !p.is_empty())
        .filter_map(|p| p.parse::<i32>().ok())
        .collect()
}

/// Render an integer list back for the UI field.
pub fn render_int_list(v: &[i32]) -> String {
    v.iter().map(i32::to_string).collect::<Vec<_>>().join(",")
}

/// Why this plan cannot run, or `None`. Checked before anything is spawned so a
/// bad workload fails in the UI rather than as an unreadable llama-bench error.
pub fn validate(plan: &Plan) -> Option<String> {
    if plan.presets.is_empty() {
        return Some("Select at least one preset to benchmark.".into());
    }
    if plan.reps < 1 {
        return Some("Repetitions must be at least 1.".into());
    }
    match plan.mode {
        Mode::Live => {
            if plan.prompt.trim().is_empty() {
                // Reaching here means the prompt was never resolved (or the
                // file held nothing but whitespace). Name the file: with the
                // prompt out in one, "needs a prompt" does not say where to
                // put it.
                return Some(match &plan.prompt_file {
                    Some(p) => format!("The live mode needs a prompt: {} is empty.", p.display()),
                    None => "The live mode needs a prompt.".into(),
                });
            }
            if plan.max_tokens < 1 {
                return Some("Max output tokens must be at least 1.".into());
            }
        }
        Mode::Synthetic => {
            if synthetic_sweeps(plan).is_empty() {
                return Some(
                    "Give at least one prompt length, or a non-zero generation length.".into(),
                );
            }
        }
    }
    None
}

// ── Effective placement (server-wide shadows the preset) ─────────────────

/// The placement keys as a launch really sees them: the server-wide value when
/// it is set, the preset's otherwise. See the module header for why.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Effective {
    pub device: String,
    pub tensor_split: String,
    pub split_mode: String,
    pub override_tensor: String,
}

fn shadow(server: Option<&String>, preset: &str) -> String {
    match server {
        Some(v) if !v.trim().is_empty() => v.trim().to_string(),
        _ => preset.trim().to_string(),
    }
}

pub fn effective(p: &Preset, cfg: &ServerConfig) -> Effective {
    // `device` and `tensor_split` are one selection (the vector is positional
    // over the device list), so they shadow TOGETHER: taking the server's
    // devices with the preset's weights would index a ratio into the wrong
    // devices. The server-wide device list winning means its split wins too,
    // blank included, which is exactly what the router does.
    let server_devices = cfg
        .device
        .as_ref()
        .filter(|v| !v.trim().is_empty())
        .is_some();
    Effective {
        device: shadow(cfg.device.as_ref(), &p.device),
        tensor_split: if server_devices {
            cfg.tensor_split.clone().unwrap_or_default().trim().into()
        } else {
            p.tensor_split.trim().to_string()
        },
        split_mode: shadow(cfg.split_mode.as_ref(), &p.split_mode),
        override_tensor: shadow(cfg.override_tensor.as_ref(), &p.override_tensor),
    }
}

/// `a,b` as llama-bench's `a/b`. See the module header: a comma there means
/// "benchmark these as separate configurations", not "these values together".
fn slashed(csv: &str) -> String {
    csv.split(',')
        .map(str::trim)
        .filter(|p| !p.is_empty())
        .collect::<Vec<_>>()
        .join("/")
}

/// `a=CPU,b=ROCm0` as llama-bench's `a=CPU;b=ROCm0`. Same trap as `slashed`,
/// one level deeper: llama-bench splits `-ot` on `,` into configuration groups
/// first, and only then on `;` into rules.
fn semicoloned(csv: &str) -> String {
    csv.split(',')
        .map(str::trim)
        .filter(|p| !p.is_empty())
        .collect::<Vec<_>>()
        .join(";")
}

// ── Synthetic: the llama-bench command line ──────────────────────────────

/// One llama-bench invocation's workload half.
#[derive(Clone, Debug, PartialEq)]
pub struct Sweep {
    pub name: &'static str,
    pub args: Vec<String>,
}

/// Split the workload into a prefill sweep and a decode sweep, one invocation
/// each.
///
/// Not one invocation with both, and the reason is not tidiness: llama-bench
/// runs the CARTESIAN PRODUCT of its test parameters, and `-d` multiplies the pp
/// tests as well as the tg ones. The default sweep in one call is 3 lengths x 3
/// depths + 3 depths = 12 tests; split, it is 3 + 3 = 6, which is the same
/// information (prefill at each length, decode at each depth) for half the wall
/// clock. Splitting also means a run interrupted after the first invocation
/// still produced a usable half.
pub fn synthetic_sweeps(plan: &Plan) -> Vec<Sweep> {
    let mut out = Vec::new();
    if !plan.prompt_lens.is_empty() {
        out.push(Sweep {
            name: "prefill",
            args: vec![
                "-p".into(),
                render_int_list(&plan.prompt_lens),
                "-n".into(),
                "0".into(),
            ],
        });
    }
    if plan.n_gen > 0 {
        let mut args = vec!["-p".into(), "0".into(), "-n".into(), plan.n_gen.to_string()];
        if !plan.depths.is_empty() {
            args.push("-d".into());
            args.push(render_int_list(&plan.depths));
        }
        out.push(Sweep {
            name: "decode",
            args,
        });
    }
    out
}

/// Build one `llama-bench` argument list: the preset mirrored, plus `sweep`.
///
/// What is deliberately NOT mapped, because llama-bench has no equivalent and a
/// silent omission is what makes a benchmark lie about its subject:
/// `--model-draft` / `--spec-type` (no speculative decoding at all), `--mmproj`,
/// `--ctx-size` (llama-bench derives `n_ctx` from `-p + -n + -d`), `--parallel`,
/// `--cache-ram`, and every chat / reasoning / sampling key. `caveats()` is what
/// says so in the report.
pub fn synthetic_argv(p: &Preset, cfg: &ServerConfig, plan: &Plan, sweep: &Sweep) -> Vec<String> {
    let eff = effective(p, cfg);
    let mut a: Vec<String> = Vec::new();
    let mut push = |flag: &str, value: String| {
        a.push(flag.to_string());
        a.push(value);
    };

    push("-m", p.model.clone());
    if !eff.device.is_empty() {
        push("-dev", slashed(&eff.device));
    }
    if !eff.tensor_split.is_empty() {
        push("-ts", slashed(&eff.tensor_split));
    }
    if !eff.split_mode.is_empty() && eff.split_mode != "default" {
        push("-sm", eff.split_mode.clone());
    }
    if !eff.override_tensor.is_empty() {
        push("-ot", semicoloned(&eff.override_tensor));
    }
    if let Some(n) = p.n_gpu_layers {
        push("-ngl", n.to_string());
    }
    if let Some(n) = p.n_cpu_moe {
        push("-ncmoe", n.to_string());
    }
    if !p.cache_type_k.is_empty() {
        push("-ctk", p.cache_type_k.clone());
    }
    if !p.cache_type_v.is_empty() {
        push("-ctv", p.cache_type_v.clone());
    }
    // Pinned rather than left at llama-bench's `auto`, because `auto` may
    // resolve differently per backend and would confound a comparison whose
    // whole point is that one thing moves. An unset preset key leaves it at auto, which
    // is llama.cpp's own behaviour and so still what the preset describes.
    if let Some(fa) = p.flash_attn {
        push("-fa", if fa { "on".into() } else { "off".into() });
    }
    if let Some(n) = p.batch_size {
        push("-b", n.to_string());
    }
    if let Some(n) = p.ubatch_size {
        push("-ub", n.to_string());
    }
    // Server-scope keys that still change the numbers: the load mode decides
    // whether the weights are mmap'd, the thread count drives every CPU-side op.
    let lm = cfg.load_mode_or_default();
    if lm != "auto" {
        push("-lm", lm.to_string());
    }
    if let Some(t) = cfg.threads {
        push("-t", t.to_string());
    }

    for arg in &sweep.args {
        a.push(arg.clone());
    }
    let mut push = |flag: &str, value: String| {
        a.push(flag.to_string());
        a.push(value);
    };
    push("-r", plan.reps.to_string());
    push("--delay", DEFAULT_DELAY_SECS.to_string());
    // jsonl, not csv or json: one object per COMPLETED test, flushed as it
    // lands (`jsonl_printer::print_test`), so a run killed by a driver reset
    // leaves every test it finished on disk and in the table. `-o json` writes
    // one array at the end, i.e. nothing at all when the run dies.
    push("-o", "jsonl".into());
    a.push("--progress".into());
    a
}

// ── Live: the request ────────────────────────────────────────────────────

pub const LIVE_PATH: &str = "/v1/chat/completions";

/// The JSON body of one live benchmark request.
///
/// `cache_prompt: false` is the load-bearing field. Without it the SECOND
/// repetition of the same prompt hits llama-server's prefix cache, reports a
/// prompt rate of tens of thousands of tokens per second, and the mean it feeds
/// is meaningless. With it, every repetition is a cold prefill, which is the
/// thing being measured. (`timings.cache_n` is read back anyway, so a build that
/// ignored the field would be caught rather than believed.)
pub fn live_body(preset_id: &str, plan: &Plan) -> String {
    live_body_with(preset_id, plan, &plan.prompt)
}

/// The body with the message content substituted. One formatter for the real
/// request and for the preview: the preview shows an ELIDED prompt (a prompt
/// file can be 100k characters, and a request body nobody can read is not a
/// preview), and routing both through here is what keeps that the only
/// difference between them, instead of a second formatter that drifts.
fn live_body_with(preset_id: &str, plan: &Plan, content: &str) -> String {
    let mut body = serde_json::json!({
        "model": preset_id,
        "messages": [{ "role": "user", "content": content }],
        "max_tokens": plan.max_tokens,
        "stream": false,
        "cache_prompt": false,
    });
    if let Some(t) = plan.temp {
        body["temperature"] = serde_json::json!(t);
    }
    serde_json::to_string_pretty(&body).unwrap_or_default()
}

/// Cut `text` to `cap` characters, saying how much was cut. Char-based, not
/// byte-based: a byte cut can land inside a UTF-8 sequence, and the counts the
/// prompt is described by everywhere else are chars.
fn elide(text: &str, cap: usize) -> String {
    let total = text.chars().count();
    if total <= cap {
        return text.to_string();
    }
    let head: String = text.chars().take(cap).collect();
    format!("{head}\n[truncated: {cap} of {total} chars shown]")
}

/// The pasteable preview of a live request: what `curl` would send, with the
/// prompt elided past `PREVIEW_PROMPT_CHARS`.
pub fn live_preview(port: u16, preset_id: &str, plan: &Plan) -> String {
    let shown = elide(&plan.prompt, PREVIEW_PROMPT_CHARS);
    format!(
        "POST http://127.0.0.1:{port}{LIVE_PATH}\n{}",
        live_body_with(preset_id, plan, &shown)
    )
}

/// The `timings` block llama-server returns on a non-streamed completion
/// (`server_slot_stats::to_json`, `tools/server/server-common.cpp`).
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Timings {
    pub prompt_n: i64,
    pub prompt_tps: f64,
    pub predicted_n: i64,
    pub predicted_tps: f64,
    /// Prompt tokens served from the cache instead of being processed. Must be 0
    /// under `cache_prompt: false`; anything else means the prefill number below
    /// is not a prefill number.
    pub cache_n: i64,
    /// Present only when the preset actually drafted (MTP / DFlash).
    pub draft_n: i64,
    pub draft_accepted: i64,
}

impl Timings {
    /// Accepted draft tokens as a percentage, or `None` when nothing drafted.
    /// Reported next to the rate, never instead of it: acceptance is not the
    /// metric, throughput is (a lower acceptance on fast steps beats a higher
    /// one on slow steps).
    pub fn acceptance(&self) -> Option<f64> {
        (self.draft_n > 0).then(|| 100.0 * self.draft_accepted as f64 / self.draft_n as f64)
    }
}

/// Pull the `timings` block out of a completion response body.
pub fn parse_timings(body: &str) -> Result<Timings, String> {
    let v: serde_json::Value =
        serde_json::from_str(body).map_err(|e| format!("unreadable response: {e}"))?;
    if let Some(msg) = v.pointer("/error/message").and_then(|m| m.as_str()) {
        return Err(msg.to_string());
    }
    let t = v
        .get("timings")
        .ok_or_else(|| "no timings in the response (is this llama-server?)".to_string())?;
    let num = |k: &str| t.get(k).and_then(serde_json::Value::as_f64).unwrap_or(0.0);
    let int = |k: &str| t.get(k).and_then(serde_json::Value::as_i64).unwrap_or(0);
    Ok(Timings {
        prompt_n: int("prompt_n"),
        prompt_tps: num("prompt_per_second"),
        predicted_n: int("predicted_n"),
        predicted_tps: num("predicted_per_second"),
        cache_n: int("cache_n"),
        draft_n: int("draft_n"),
        draft_accepted: int("draft_n_accepted"),
    })
}

/// The two test labels a live repetition produces.
pub const LIVE_PREFILL: &str = "prefill";
pub const LIVE_DECODE: &str = "decode";

// ── Results ──────────────────────────────────────────────────────────────

/// One measured configuration: what llama-bench already aggregates over its own
/// `-r` repetitions, and what the live mode aggregates itself over ours.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Point {
    pub preset: String,
    pub test: String,
    pub mean: f64,
    pub sd: f64,
    pub n: i32,
    /// Whatever the number alone would hide: a cache hit that invalidates a
    /// prefill figure, the drafter's acceptance. Empty when there is nothing to
    /// say.
    pub note: String,
}

/// Sample mean and sample standard deviation (n-1). A single value has no
/// spread, not a zero one,
/// but 0 is the honest rendering of "no spread measured" and the `n` column
/// says how many samples that verdict rests on.
pub fn mean_sd(values: &[f64]) -> (f64, f64) {
    if values.is_empty() {
        return (0.0, 0.0);
    }
    let mean = values.iter().sum::<f64>() / values.len() as f64;
    if values.len() < 2 {
        return (mean, 0.0);
    }
    let var =
        values.iter().map(|v| (v - mean) * (v - mean)).sum::<f64>() / (values.len() - 1) as f64;
    (mean, var.sqrt())
}

/// llama-bench's own label for a test row, mirroring its markdown printer:
/// `pp2048`, `tg128`, `tg128 @ d8192`.
pub fn bench_test_label(n_prompt: i64, n_gen: i64, n_depth: i64) -> String {
    let head = if n_prompt > 0 {
        format!("pp{n_prompt}")
    } else {
        format!("tg{n_gen}")
    };
    if n_depth > 0 {
        format!("{head} @ d{n_depth}")
    } else {
        head
    }
}

/// One `-o jsonl` line from llama-bench into a `Point`.
///
/// llama-bench has already averaged its `-r` repetitions, so the line carries
/// `avg_ts` / `stddev_ts` and the repetition count is ours to supply: the
/// preset id likewise, since llama-bench knows nothing about presets.
pub fn parse_bench_line(line: &str, preset: &str, reps: i32) -> Option<Point> {
    let v: serde_json::Value = serde_json::from_str(line.trim()).ok()?;
    let int = |k: &str| v.get(k).and_then(serde_json::Value::as_i64).unwrap_or(0);
    let num = |k: &str| v.get(k).and_then(serde_json::Value::as_f64);
    let mean = num("avg_ts")?;
    Some(Point {
        preset: preset.to_string(),
        test: bench_test_label(int("n_prompt"), int("n_gen"), int("n_depth")),
        mean,
        sd: num("stddev_ts").unwrap_or(0.0),
        n: reps,
        note: String::new(),
    })
}

/// llama-bench's build number from a jsonl line, for the report's header.
pub fn parse_bench_build(line: &str) -> Option<i64> {
    serde_json::from_str::<serde_json::Value>(line.trim())
        .ok()?
        .get("build_number")?
        .as_i64()
}

/// The flash-attention value llama-bench actually RESOLVED for a test.
///
/// This is the one column that must be read back rather than assumed. A preset
/// that leaves `--flash-attn` unset runs at llama.cpp's `auto`, which resolves
/// per backend, so a ROCm leg and a Vulkan leg of the same comparison can end up
/// on different kernels with nothing in the rates to show it. The tab does not
/// PIN the value, because pinning would benchmark a configuration the preset
/// does not run (the tab's whole premise is that it measures the configuration
/// in use); it reads what happened and says so when the legs disagree.
///
/// llama-bench emits the field as a bool in jsonl and as 0/1 in csv, so both
/// spellings are accepted.
pub fn parse_bench_flash_attn(line: &str) -> Option<String> {
    let v: serde_json::Value = serde_json::from_str(line.trim()).ok()?;
    let fa = v.get("flash_attn")?;
    Some(match fa {
        serde_json::Value::Bool(b) => (if *b { "on" } else { "off" }).to_string(),
        serde_json::Value::Number(n) => {
            (if n.as_i64() == Some(0) { "off" } else { "on" }).to_string()
        }
        serde_json::Value::String(s) => s.clone(),
        _ => return None,
    })
}

/// The warning to print when the compared presets did not end up on the same
/// flash-attention setting, or `None` when they agree (or when only one preset
/// ran, which is nothing to compare).
///
/// Takes what was OBSERVED, keyed by preset, so it reports reality rather than
/// intent: a preset pinning `on` and one resolving `on` from `auto` agree, and
/// the warning stays silent, which is correct.
pub fn flash_attn_mismatch(seen: &BTreeMap<String, String>) -> Option<String> {
    let mut values: Vec<&String> = seen.values().collect();
    values.sort();
    values.dedup();
    if values.len() < 2 {
        return None;
    }
    let detail: Vec<String> = seen.iter().map(|(p, v)| format!("{p}={v}")).collect();
    Some(format!(
        "The compared presets ran with DIFFERENT flash-attention settings ({}), so the \
         rates below differ by more than the thing you are comparing. Pin `flash-attn` \
         explicitly on every preset in the comparison and re-run.",
        detail.join(", ")
    ))
}

/// Resolve a preset's `--device` list against the probed devices, as
/// `(id, name)` pairs with an empty name for an id the probe does not know.
///
/// Recorded in the report because the ids are NOT stable: the same machine has
/// enumerated its discrete AMD card as `ROCm0` on one boot and `ROCm1` on
/// another, and on this box `Vulkan0` is the integrated GPU. A benchmark that
/// silently ran on the wrong card looks disappointing rather than wrong, so the
/// report names what each id pointed at when the run happened.
pub fn device_names(list: &str, devs: &[crate::devices::DeviceOption]) -> Vec<(String, String)> {
    list.split(',')
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(|id| {
            let name = devs
                .iter()
                .find(|d| d.id.eq_ignore_ascii_case(id))
                .map(|d| d.name.clone())
                .unwrap_or_default();
            (id.to_string(), name)
        })
        .collect()
}

/// The warning for device ids the probe does not know, or `None`.
///
/// An unknown id is not cosmetic: llama.cpp is about to be handed a device that
/// does not exist on this machine, so the run either fails or quietly lands
/// somewhere else.
pub fn unknown_device_warning(resolved: &[(String, Vec<(String, String)>)]) -> Option<String> {
    let mut bad: Vec<String> = Vec::new();
    for (preset, devs) in resolved {
        for (id, name) in devs {
            if name.is_empty() {
                bad.push(format!("{preset} -> {id}"));
            }
        }
    }
    (!bad.is_empty()).then(|| {
        format!(
            "Device ids this machine's probe does not know: {}. The ids are not stable \
             across driver states, so a preset can point at a card that is not there.",
            bad.join(", ")
        )
    })
}

/// A `Point` plus its ratio against the baseline preset, ready for the table.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Row {
    pub test: String,
    pub preset: String,
    pub mean: String,
    pub sd: String,
    pub samples: i32,
    /// `1.00x` for the baseline itself, `-` where the baseline has no such test.
    pub ratio: String,
    pub note: String,
}

/// Order the points for display and attach the ratio column.
///
/// Rows are grouped by TEST (so the presets of one test sit together, which is
/// the comparison), and within a test they follow the plan's preset order, so
/// the baseline is always the first row of its group.
pub fn rows(points: &[Point], order: &[String]) -> Vec<Row> {
    let baseline = order.first().cloned().unwrap_or_default();
    let rank = |id: &str| order.iter().position(|p| p == id).unwrap_or(usize::MAX);

    // BTreeMap keyed by first appearance keeps the tests in the order they were
    // produced (pp before tg, shallow depth before deep), which is llama-bench's
    // own order and the one the live mode emits.
    let mut seen: Vec<String> = Vec::new();
    for p in points {
        if !seen.contains(&p.test) {
            seen.push(p.test.clone());
        }
    }

    let mut base_of: BTreeMap<&str, f64> = BTreeMap::new();
    for p in points {
        if p.preset == baseline {
            base_of.insert(p.test.as_str(), p.mean);
        }
    }

    let mut out = Vec::new();
    for test in &seen {
        let mut group: Vec<&Point> = points.iter().filter(|p| &p.test == test).collect();
        group.sort_by_key(|p| rank(&p.preset));
        for p in group {
            let ratio = match base_of.get(p.test.as_str()) {
                Some(b) if *b > 0.0 => format!("{:.2}x", p.mean / b),
                _ => "-".to_string(),
            };
            out.push(Row {
                test: p.test.clone(),
                preset: p.preset.clone(),
                mean: format!("{:.1}", p.mean),
                sd: format!("{:.1}", p.sd),
                samples: p.n,
                ratio,
                note: p.note.clone(),
            });
        }
    }
    out
}

// ── Report ───────────────────────────────────────────────────────────────

/// What the run knew about itself that the rows cannot carry: which llama.cpp
/// built the numbers, and which machine state produced them.
#[derive(Clone, Debug, Default)]
pub struct RunMeta {
    pub stamp: String,
    pub server_version: String,
    pub bench_build: Option<i64>,
    pub exe: String,
    /// `(label, value)` rows describing the machine: boot time, display driver
    /// versions. See `bench::env` for why a report that does not say what it ran
    /// on cannot be compared with another one.
    pub env: Vec<(String, String)>,
    /// Everything that makes the numbers less comparable than they look. Printed
    /// above the results, not below, because a reader who stops at the table
    /// must still have seen them.
    pub warnings: Vec<String>,
}

/// The caveats that belong to a mode, spelled out in the report because every
/// one of them is a way to misread the numbers above them.
pub fn caveats(mode: Mode) -> Vec<&'static str> {
    match mode {
        Mode::Live => vec![
            "Numbers include the preset's speculative drafter, chat template and sampler: they are production rates, not a hardware ceiling.",
            "Every repetition asks for `cache_prompt: false`, so each prefill is cold. A non-zero `cache` note on a row means the server reused a prefix anyway and that row's prefill rate is not one.",
            "Presets run grouped, all repetitions of one before the next, because switching preset makes the router load another model. A card that heats up over a long run therefore charges the drift to the LAST preset: re-run with the order reversed if a difference is small.",
            "Temperature 0 makes two runs comparable, but it is not the sampling a preset ships with; with a drafter on, acceptance (and so throughput) depends on the content being generated.",
        ],
        Mode::Synthetic => vec![
            "No speculative decoding: llama-bench cannot load the preset's drafter, so `tg` here is the unassisted rate and reads well below what the same preset does live with MTP on.",
            "No sampling at all, hence no temperature and no prompt: the results are content-independent by construction, which is the point of this mode.",
            "The mmproj, the chat template and every reasoning flag are ignored.",
            "`pp` is batch prefill throughput, not time to first token.",
            "`--ctx-size` is not honoured: llama-bench sizes the context as prompt + generated + depth.",
            "llama-server must be stopped: benching alongside it does not fail cleanly, it spills into shared memory and reads as a backend regression.",
        ],
    }
}

/// The settings-block rows describing the live prompt: where it came from, how
/// big it is, and what it WAS.
///
/// The digest is the load-bearing one now that the prompt is a file anyone can
/// edit between two runs. Embedding the text (below, up to a cap) makes a short
/// prompt readable; only the sha256 makes "these two reports measured the same
/// prompt" a checkable claim for a long one, and a report that cannot be
/// checked is a report that has to be believed.
pub(crate) fn prompt_report_rows(plan: &Plan) -> Vec<(String, String)> {
    let info = prompt_info(&plan.prompt);
    vec![
        (
            "prompt".to_string(),
            match &plan.prompt_file {
                Some(p) => format!("`{}`", p.display()),
                None => "given inline (`--prompt`)".to_string(),
            },
        ),
        (
            "prompt size".to_string(),
            format!("{} chars, {} bytes", info.chars, info.bytes),
        ),
        ("prompt sha256".to_string(), format!("`{}`", info.sha256)),
    ]
}

/// Render the markdown report: a settings block, the rows, the prompt, and the
/// caveats, shaped to paste straight into an issue.
pub fn render_report(plan: &Plan, points: &[Point], meta: &RunMeta) -> String {
    let mut md = String::new();
    let engine = match plan.mode {
        Mode::Live => "live (llama-server)",
        Mode::Synthetic => "synthetic (llama-bench)",
    };
    md.push_str(&format!("# Benchmark: {engine}\n\n"));
    md.push_str(&format!("Generated {}.\n\n", meta.stamp));

    md.push_str("| setting | value |\n|---|---|\n");
    md.push_str(&format!("| presets | {} |\n", plan.presets.join(", ")));
    md.push_str(&format!("| repetitions | {} |\n", plan.reps));
    match plan.mode {
        Mode::Live => {
            md.push_str(&format!(
                "| temperature | {} |\n",
                match plan.temp {
                    Some(t) => format!("{t}"),
                    None => "the preset's own".to_string(),
                }
            ));
            md.push_str(&format!("| max output tokens | {} |\n", plan.max_tokens));
            for (label, value) in prompt_report_rows(plan) {
                md.push_str(&format!("| {label} | {value} |\n"));
            }
        }
        Mode::Synthetic => {
            md.push_str(&format!(
                "| prompt lengths | {} |\n",
                render_int_list(&plan.prompt_lens)
            ));
            md.push_str(&format!("| generated | {} |\n", plan.n_gen));
            md.push_str(&format!("| depths | {} |\n", render_int_list(&plan.depths)));
        }
    }
    if !meta.server_version.is_empty() {
        md.push_str(&format!("| llama-server | {} |\n", meta.server_version));
    }
    if let Some(b) = meta.bench_build {
        md.push_str(&format!("| llama-bench | build {b} |\n"));
    }
    if !meta.exe.is_empty() {
        md.push_str(&format!("| binary | `{}` |\n", meta.exe));
    }
    md.push('\n');

    // ABOVE the results, deliberately: a reader who stops at the table has to
    // have seen why it may not mean what it looks like.
    if !meta.warnings.is_empty() {
        for w in &meta.warnings {
            md.push_str(&format!("> **WARNING** {w}\n\n"));
        }
    }

    let rows = rows(points, &plan.presets);
    let baseline = plan.presets.first().cloned().unwrap_or_default();
    md.push_str("## Results (tokens/s, mean +- sample stddev)\n\n");
    if rows.is_empty() {
        md.push_str("No results were collected.\n\n");
    } else {
        md.push_str(&format!(
            "| test | preset | t/s | +- | samples | vs {baseline} | note |\n|---|---|---:|---:|---:|---:|---|\n"
        ));
        for r in &rows {
            md.push_str(&format!(
                "| {} | {} | {} | {} | {} | {} | {} |\n",
                r.test, r.preset, r.mean, r.sd, r.samples, r.ratio, r.note
            ));
        }
        md.push('\n');
    }

    md.push_str("## Prompt\n\n");
    if plan.mode == Mode::Live {
        md.push_str("```\n");
        md.push_str(elide(plan.prompt.trim(), REPORT_PROMPT_CHARS).as_str());
        md.push_str("\n```\n\n");
        md.push_str(
            "The sha256 above is of the prompt as SENT (a leading BOM dropped, CRLF folded to \
             LF), not of the file's bytes, so it will not match `Get-FileHash` on a CRLF file. \
             Two runs whose digests agree measured the same prompt.\n\n",
        );
    } else {
        md.push_str(
            "This mode takes no prompt: `-p` is a token count and the tool never samples.\n\n",
        );
    }

    // The machine, so two reports can be compared at all. A driver that changed
    // underneath a comparison is invisible in the rates and obvious here.
    if !meta.env.is_empty() {
        md.push_str("## Environment\n\n");
        md.push_str("| what | value |\n|---|---|\n");
        for (label, value) in &meta.env {
            md.push_str(&format!("| {label} | {value} |\n"));
        }
        md.push('\n');
    }

    md.push_str("## Caveats\n\n");
    for c in caveats(plan.mode) {
        md.push_str(&format!("- {c}\n"));
    }
    md
}

// ── Where runs live ──────────────────────────────────────────────────────

/// `%LOCALAPPDATA%\llama.cpp\bench\`: a sibling of `config\`, `logs\` and
/// `state\`, not the repo's `build\bench\`, because the GUI also runs on an
/// installed machine that has no repo.
pub fn bench_dir() -> PathBuf {
    crate::paths::data_root().join("bench")
}

/// The three files one run writes, by stamp: the machine-readable stream, the
/// report, and the engine's own chatter.
pub fn run_paths(stamp: &str) -> (PathBuf, PathBuf, PathBuf) {
    let dir = bench_dir();
    (
        dir.join(format!("bench-{stamp}.jsonl")),
        dir.join(format!("bench-{stamp}.md")),
        dir.join(format!("bench-{stamp}.log")),
    )
}

// ── The jsonl stream ─────────────────────────────────────────────────────
//
// APPEND-ONLY, and written as each point lands rather than at the end. That is
// the whole design: a benchmark is minutes of sustained GPU load, which is
// exactly when a driver reset or a bugcheck happens, and results written only
// at the end are results lost on the first crash (a full day of measurements
// went that way here on 2026-08-24). A live point is REVISED as its repetitions accumulate,
// so the same key is written several times and the LAST line wins on reload:
// a run killed halfway therefore reloads with the means it had reached.

/// The first line of every run file: the plan, so a reload knows the preset
/// order (which is what the ratio column is measured against) and the mode.
pub fn run_header_json(plan: &Plan, meta: &RunMeta) -> String {
    let mut header = serde_json::json!({
        "kind": "run",
        "mode": plan.mode.as_str(),
        "stamp": meta.stamp,
        "presets": plan.presets,
        "reps": plan.reps,
        "server_version": meta.server_version,
    });
    // The prompt's identity, in the machine-readable stream and not only in the
    // markdown: comparing two old runs is a question about this line, and until
    // the prompt moved into a file the jsonl carried nothing about it at all.
    if plan.mode == Mode::Live {
        let info = prompt_info(&plan.prompt);
        header["prompt_file"] = match &plan.prompt_file {
            Some(p) => serde_json::json!(p.to_string_lossy()),
            None => serde_json::Value::Null,
        };
        header["prompt_chars"] = serde_json::json!(info.chars);
        header["prompt_sha256"] = serde_json::json!(info.sha256);
    }
    header.to_string()
}

/// One point as a jsonl line.
pub fn point_json(p: &Point) -> String {
    serde_json::json!({
        "kind": "point",
        "preset": p.preset,
        "test": p.test,
        "mean": p.mean,
        "sd": p.sd,
        "n": p.n,
        "note": p.note,
    })
    .to_string()
}

/// llama-bench's own row, kept verbatim beside the point derived from it.
///
/// The point keeps mean, spread and count; the row carries everything else
/// llama-bench measured and this app throws away, `flash_attn`, `n_batch`,
/// `n_threads`, `backends`, `gpu_info` among them. Storing it costs a line and
/// buys the ability to answer, months later, a question nobody thought to ask
/// during the run. Skipping it is how a comparison ends up unfalsifiable: the
/// first version of this file kept only the aggregate, and the resolved
/// flash-attention setting of a finished ROCm-versus-Vulkan run was simply not
/// recoverable.
pub fn bench_row_json(preset: &str, row: &str) -> String {
    let parsed: serde_json::Value =
        serde_json::from_str(row.trim()).unwrap_or(serde_json::Value::Null);
    serde_json::json!({ "kind": "bench_row", "preset": preset, "row": parsed }).to_string()
}

/// One raw repetition, kept beside the points it feeds. Nothing reads it back;
/// it is the audit trail for a number that looks wrong later.
pub fn sample_json(
    preset: &str,
    test: &str,
    rep: i32,
    tps: f64,
    detail: serde_json::Value,
) -> String {
    serde_json::json!({
        "kind": "sample",
        "preset": preset,
        "test": test,
        "rep": rep,
        "tps": tps,
        "detail": detail,
    })
    .to_string()
}

/// A run read back from its jsonl.
#[derive(Clone, Debug, Default)]
pub struct Loaded {
    pub mode_label: String,
    pub presets: Vec<String>,
    pub points: Vec<Point>,
}

/// Reload a past run. Unreadable lines are skipped rather than fatal: the file
/// is append-only and a run killed mid-write can end in half a line.
pub fn load_run(path: &std::path::Path) -> Result<Loaded, String> {
    let text = std::fs::read_to_string(path).map_err(|e| format!("{}: {e}", path.display()))?;
    let mut out = Loaded::default();
    // Keyed by (preset, test): the last line for a key is the final value.
    let mut latest: Vec<Point> = Vec::new();
    for line in text.lines() {
        let Ok(v) = serde_json::from_str::<serde_json::Value>(line.trim()) else {
            continue;
        };
        match v.get("kind").and_then(serde_json::Value::as_str) {
            Some("run") => {
                out.mode_label = v
                    .get("mode")
                    .and_then(serde_json::Value::as_str)
                    .unwrap_or_default()
                    .to_string();
                out.presets = v
                    .get("presets")
                    .and_then(serde_json::Value::as_array)
                    .map(|a| {
                        a.iter()
                            .filter_map(|x| x.as_str().map(str::to_string))
                            .collect()
                    })
                    .unwrap_or_default();
            }
            Some("point") => {
                let str_of = |k: &str| {
                    v.get(k)
                        .and_then(serde_json::Value::as_str)
                        .unwrap_or_default()
                        .to_string()
                };
                let p = Point {
                    preset: str_of("preset"),
                    test: str_of("test"),
                    mean: v
                        .get("mean")
                        .and_then(serde_json::Value::as_f64)
                        .unwrap_or(0.0),
                    sd: v
                        .get("sd")
                        .and_then(serde_json::Value::as_f64)
                        .unwrap_or(0.0),
                    n: v.get("n").and_then(serde_json::Value::as_i64).unwrap_or(0) as i32,
                    note: str_of("note"),
                };
                match latest
                    .iter_mut()
                    .find(|q| q.preset == p.preset && q.test == p.test)
                {
                    Some(slot) => *slot = p,
                    None => latest.push(p),
                }
            }
            _ => {}
        }
    }
    // A preset that only appears in the points (a hand-edited file, or a header
    // that never made it to disk) still needs a column order.
    for p in &latest {
        if !out.presets.contains(&p.preset) {
            out.presets.push(p.preset.clone());
        }
    }
    out.points = latest;
    Ok(out)
}

/// Past runs, newest first, as (stamp, report path). Read from the directory
/// listing rather than from an index file: a run killed mid-flight still leaves
/// its jsonl, and the list has to show it.
pub fn past_runs() -> Vec<(String, PathBuf)> {
    let Ok(entries) = std::fs::read_dir(bench_dir()) else {
        return Vec::new();
    };
    let mut out: Vec<(String, PathBuf)> = entries
        .flatten()
        .filter_map(|e| {
            let path = e.path();
            let name = path.file_name()?.to_str()?.to_string();
            let stamp = name.strip_prefix("bench-")?.strip_suffix(".jsonl")?;
            Some((stamp.to_string(), path))
        })
        .collect();
    // The stamp is `yyyymmdd-hhmmss`, so a lexical sort IS chronological.
    out.sort_by(|a, b| b.0.cmp(&a.0));
    out
}

// ── Timestamps (no time crate in this tree) ──────────────────────────────

/// Seconds since the Unix epoch, or 0 if the clock is before it.
pub fn now_secs() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

/// `yyyymmdd-hhmmss` in UTC. UTC and not local time on purpose: this crate has
/// no time-zone database and the stamp is also the file name, so a value that
/// silently jumped an hour twice a year would break the ordering `past_runs`
/// relies on. The report spells the zone out.
pub fn stamp(secs: u64) -> String {
    let (y, mo, d, h, mi, s) = civil(secs);
    format!("{y:04}{mo:02}{d:02}-{h:02}{mi:02}{s:02}")
}

/// The same instant, spelled for a human.
pub fn stamp_human(secs: u64) -> String {
    let (y, mo, d, h, mi, s) = civil(secs);
    format!("{y:04}-{mo:02}-{d:02} {h:02}:{mi:02}:{s:02} UTC")
}

/// A stamp back into a human-readable date, for the saved-runs list. Anything
/// not stamp-shaped is handed back unchanged: a file the user renamed is still
/// worth listing.
pub fn stamp_label(stamp: &str) -> String {
    let b = stamp.as_bytes();
    if b.len() != 15
        || b[8] != b'-'
        || !stamp
            .chars()
            .enumerate()
            .all(|(i, c)| i == 8 || c.is_ascii_digit())
    {
        return stamp.to_string();
    }
    format!(
        "{}-{}-{} {}:{}:{} UTC",
        &stamp[0..4],
        &stamp[4..6],
        &stamp[6..8],
        &stamp[9..11],
        &stamp[11..13],
        &stamp[13..15]
    )
}

/// Civil date and time (UTC) from a Unix timestamp. Howard Hinnant's
/// `civil_from_days`, which is exact for every date this will ever see and needs
/// no table.
fn civil(secs: u64) -> (i64, u32, u32, u32, u32, u32) {
    let days = (secs / 86_400) as i64;
    let rem = secs % 86_400;
    let z = days + 719_468;
    let era = if z >= 0 { z } else { z - 146_096 } / 146_097;
    let doe = (z - era * 146_097) as u64;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let y = yoe as i64 + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = (doy - (153 * mp + 2) / 5 + 1) as u32;
    let m = if mp < 10 { mp + 3 } else { mp - 9 } as u32;
    let year = if m <= 2 { y + 1 } else { y };
    (
        year,
        m,
        d,
        (rem / 3600) as u32,
        ((rem % 3600) / 60) as u32,
        (rem % 60) as u32,
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn preset(id: &str) -> Preset {
        Preset {
            id: id.into(),
            model: r"E:\models\m.gguf".into(),
            ..Default::default()
        }
    }

    fn argv_of(a: &[String], flag: &str) -> Option<String> {
        a.iter()
            .position(|x| x == flag)
            .and_then(|i| a.get(i + 1))
            .cloned()
    }

    /// The prefill sweep of the default plan, the one most assertions want.
    fn first_sweep(plan: &Plan) -> Sweep {
        synthetic_sweeps(plan).first().cloned().expect("a sweep")
    }

    // One invocation with both halves would run the CARTESIAN product: `-d`
    // multiplies the pp tests too, so the default sweep would be 3 lengths x 3
    // depths + 3 depths = 12 tests instead of 6, for the same information at
    // twice the wall clock. This pins the split (and that the depths ride the
    // decode half only).
    #[test]
    fn the_workload_splits_into_a_prefill_and_a_decode_sweep() {
        let sweeps = synthetic_sweeps(&Plan::default());
        assert_eq!(sweeps.len(), 2);
        assert_eq!(sweeps[0].name, "prefill");
        assert_eq!(
            argv_of(&sweeps[0].args, "-p").as_deref(),
            Some("2048,8192,32768")
        );
        assert_eq!(argv_of(&sweeps[0].args, "-n").as_deref(), Some("0"));
        assert!(
            !sweeps[0].args.iter().any(|a| a == "-d"),
            "no depths on prefill"
        );

        assert_eq!(sweeps[1].name, "decode");
        assert_eq!(argv_of(&sweeps[1].args, "-p").as_deref(), Some("0"));
        assert_eq!(argv_of(&sweeps[1].args, "-n").as_deref(), Some("128"));
        assert_eq!(
            argv_of(&sweeps[1].args, "-d").as_deref(),
            Some("0,8192,32768")
        );

        // Each half can be asked for alone, and a plan with neither is what
        // `validate` refuses.
        let only_pp = Plan {
            n_gen: 0,
            ..Plan::default()
        };
        assert_eq!(synthetic_sweeps(&only_pp).len(), 1);
        assert_eq!(synthetic_sweeps(&only_pp)[0].name, "prefill");
        let only_tg = Plan {
            prompt_lens: Vec::new(),
            ..Plan::default()
        };
        assert_eq!(synthetic_sweeps(&only_tg)[0].name, "decode");
        let neither = Plan {
            prompt_lens: Vec::new(),
            n_gen: 0,
            presets: vec!["a".into()],
            mode: Mode::Synthetic,
            ..Plan::default()
        };
        assert!(synthetic_sweeps(&neither).is_empty());
        assert!(validate(&neither).is_some());
    }

    // The three separator conversions, each of which turns ONE benchmark into
    // several when it is missed (a comma is llama-bench's "run these as separate
    // configurations"), so the preset would be benchmarked in a placement it
    // never runs in.
    #[test]
    fn ini_separators_become_llama_bench_separators() {
        let p = Preset {
            device: "ROCm0,CUDA0".into(),
            tensor_split: "54,12".into(),
            override_tensor: r"token_embd\.weight=ROCm0,output\.weight=CUDA0".into(),
            ..preset("a")
        };
        let plan = Plan::default();
        let a = synthetic_argv(&p, &ServerConfig::default(), &plan, &first_sweep(&plan));
        assert_eq!(argv_of(&a, "-dev").as_deref(), Some("ROCm0/CUDA0"));
        assert_eq!(argv_of(&a, "-ts").as_deref(), Some("54/12"));
        assert_eq!(
            argv_of(&a, "-ot").as_deref(),
            Some(r"token_embd\.weight=ROCm0;output\.weight=CUDA0")
        );
        assert!(!a.iter().any(|x| x.contains(',') && x.contains('=')));
    }

    // The router copies the server's own CLI over every preset, so a benchmark
    // that used the preset's placement would measure something the machine never
    // runs. Devices and split travel together: taking the server's devices with
    // the preset's weights would index a ratio into the wrong cards.
    #[test]
    fn server_wide_placement_shadows_the_preset() {
        let p = Preset {
            device: "CUDA0".into(),
            tensor_split: "1".into(),
            override_tensor: "a=CPU".into(),
            ..preset("a")
        };
        let cfg = ServerConfig {
            device: Some("ROCm0,CUDA0".into()),
            tensor_split: Some("3,1".into()),
            override_tensor: Some("b=ROCm0".into()),
            ..Default::default()
        };
        let eff = effective(&p, &cfg);
        assert_eq!(eff.device, "ROCm0,CUDA0");
        assert_eq!(eff.tensor_split, "3,1");
        assert_eq!(eff.override_tensor, "b=ROCm0");

        // A server-wide device list with a BLANK split means auto-by-free-VRAM,
        // and that blank must win too: keeping the preset's weights here would
        // apply a two-device ratio the user did not ask for.
        let cfg = ServerConfig {
            device: Some("ROCm0,CUDA0".into()),
            ..Default::default()
        };
        assert_eq!(effective(&p, &cfg).tensor_split, "");
    }

    #[test]
    fn an_untouched_preset_passes_only_the_workload() {
        let plan = Plan::default();
        let a = synthetic_argv(
            &preset("a"),
            &ServerConfig::default(),
            &plan,
            &first_sweep(&plan),
        );
        for flag in [
            "-dev", "-ts", "-sm", "-ot", "-ngl", "-ncmoe", "-ctk", "-fa", "-b", "-ub",
        ] {
            assert!(!a.iter().any(|x| x == flag), "{flag} should be absent");
        }
        assert_eq!(argv_of(&a, "-m").as_deref(), Some(r"E:\models\m.gguf"));
        assert_eq!(argv_of(&a, "-p").as_deref(), Some("2048,8192,32768"));
        // jsonl, not json: one flushed object per completed test, so a run
        // killed by a driver reset keeps what it measured.
        assert_eq!(argv_of(&a, "-o").as_deref(), Some("jsonl"));
    }

    // cache_prompt is what keeps every repetition a cold prefill; without it the
    // second one reports the prefix cache's speed and the mean is a fiction.
    #[test]
    fn the_live_body_disables_the_prefix_cache_and_carries_the_override() {
        let plan = Plan {
            prompt: "hi".into(),
            temp: Some(0.0),
            max_tokens: 64,
            ..Plan::default()
        };
        let body: serde_json::Value = serde_json::from_str(&live_body("qwen", &plan)).unwrap();
        assert_eq!(body["cache_prompt"], serde_json::json!(false));
        assert_eq!(body["model"], "qwen");
        assert_eq!(body["temperature"], serde_json::json!(0.0));
        assert_eq!(body["max_tokens"], serde_json::json!(64));
        assert_eq!(body["stream"], serde_json::json!(false));

        // "the preset's own" must OMIT the key: sending any number would
        // override the preset with it.
        let plan = Plan { temp: None, ..plan };
        let body: serde_json::Value = serde_json::from_str(&live_body("qwen", &plan)).unwrap();
        assert!(body.get("temperature").is_none());
    }

    #[test]
    fn timings_are_read_from_the_right_keys() {
        let body = r#"{"choices":[],"timings":{
            "cache_n": 7, "prompt_n": 1024, "prompt_per_second": 900.5,
            "predicted_n": 128, "predicted_per_second": 35.25,
            "draft_n": 100, "draft_n_accepted": 74 }}"#;
        let t = parse_timings(body).unwrap();
        assert_eq!(t.prompt_n, 1024);
        assert!((t.prompt_tps - 900.5).abs() < 1e-9);
        assert!((t.predicted_tps - 35.25).abs() < 1e-9);
        assert_eq!(t.cache_n, 7);
        assert_eq!(t.acceptance().map(|a| a.round()), Some(74.0));

        // No drafter: no acceptance figure at all, rather than a 0% that would
        // read as "it drafted and everything was rejected".
        let t = parse_timings(r#"{"timings":{"predicted_per_second":10}}"#).unwrap();
        assert_eq!(t.acceptance(), None);

        // llama-server's error shape must surface as the error, not as
        // "no timings": the message is the actionable half.
        let err = parse_timings(r#"{"error":{"message":"model not found"}}"#).unwrap_err();
        assert!(err.contains("model not found"), "{err}");
    }

    #[test]
    fn a_bench_line_becomes_a_point_with_llama_bench_s_own_label() {
        let line = r#"{"n_prompt":0,"n_gen":128,"n_depth":8192,"avg_ts":35.5,"stddev_ts":0.5,"build_number":10566}"#;
        let p = parse_bench_line(line, "qwen", 3).unwrap();
        assert_eq!(p.test, "tg128 @ d8192");
        assert_eq!(p.preset, "qwen");
        assert_eq!(p.n, 3);
        assert_eq!(parse_bench_build(line), Some(10566));

        let line = r#"{"n_prompt":2048,"n_gen":0,"n_depth":0,"avg_ts":600.0}"#;
        assert_eq!(parse_bench_line(line, "q", 1).unwrap().test, "pp2048");
        // A progress line or any other non-row must not become a point.
        assert!(parse_bench_line("not json", "q", 1).is_none());
        assert!(parse_bench_line(r#"{"n_prompt":1}"#, "q", 1).is_none());
    }

    #[test]
    fn rows_group_by_test_and_ratio_against_the_first_preset() {
        let pt = |preset: &str, test: &str, mean: f64| Point {
            preset: preset.into(),
            test: test.into(),
            mean,
            sd: 0.0,
            n: 3,
            note: String::new(),
        };
        let points = vec![
            pt("a", "pp2048", 100.0),
            pt("b", "pp2048", 50.0),
            pt("a", "tg128", 20.0),
            pt("b", "tg128", 30.0),
        ];
        let order = vec!["a".to_string(), "b".to_string()];
        let r = rows(&points, &order);
        assert_eq!(r.len(), 4);
        assert_eq!((r[0].test.as_str(), r[0].preset.as_str()), ("pp2048", "a"));
        assert_eq!(r[0].ratio, "1.00x");
        assert_eq!(r[1].ratio, "0.50x");
        assert_eq!((r[2].test.as_str(), r[2].preset.as_str()), ("tg128", "a"));
        assert_eq!(r[3].ratio, "1.50x");

        // A preset that produced a test the baseline did not gets no invented
        // ratio.
        let r = rows(&[pt("b", "pp4096", 10.0)], &order);
        assert_eq!(r[0].ratio, "-");
    }

    // The confound this exists to catch, in the exact shape it took: two presets
    // that differ only by device, NEITHER pinning flash-attn, so both ran at
    // `auto` and `auto` resolved differently per backend. Nothing in the rates
    // shows it.
    #[test]
    fn a_flash_attn_difference_between_compared_presets_is_reported() {
        let seen = |pairs: &[(&str, &str)]| -> BTreeMap<String, String> {
            pairs
                .iter()
                .map(|(k, v)| (k.to_string(), v.to_string()))
                .collect()
        };
        let w = flash_attn_mismatch(&seen(&[("vk", "off"), ("rocm", "on")])).expect("warned");
        assert!(w.contains("vk=off") && w.contains("rocm=on"), "{w}");

        // Agreement is silence, however it was reached: a preset pinning `on`
        // and one resolving `on` from auto are the same configuration.
        assert_eq!(
            flash_attn_mismatch(&seen(&[("a", "on"), ("b", "on")])),
            None
        );
        // One preset is nothing to compare, and an empty map is a run that
        // produced no rows.
        assert_eq!(flash_attn_mismatch(&seen(&[("a", "on")])), None);
        assert_eq!(flash_attn_mismatch(&BTreeMap::new()), None);
    }

    // llama-bench spells the field differently per output format, and reading it
    // wrong would silence the warning above rather than fire it, which is the
    // failure that hides.
    #[test]
    fn the_resolved_flash_attn_is_read_in_every_spelling() {
        assert_eq!(
            parse_bench_flash_attn(r#"{"flash_attn":true}"#).as_deref(),
            Some("on")
        );
        assert_eq!(
            parse_bench_flash_attn(r#"{"flash_attn":false}"#).as_deref(),
            Some("off")
        );
        assert_eq!(
            parse_bench_flash_attn(r#"{"flash_attn":1}"#).as_deref(),
            Some("on")
        );
        assert_eq!(
            parse_bench_flash_attn(r#"{"flash_attn":0}"#).as_deref(),
            Some("off")
        );
        assert_eq!(
            parse_bench_flash_attn(r#"{"flash_attn":"auto"}"#).as_deref(),
            Some("auto")
        );
        assert_eq!(parse_bench_flash_attn(r#"{"avg_ts":1}"#), None);
        assert_eq!(parse_bench_flash_attn("not json"), None);
    }

    // The ids are not stable across driver states, so the report records what
    // each one pointed at, and an id the probe does not know is a warning rather
    // than a blank.
    #[test]
    fn device_ids_are_resolved_to_names_and_unknown_ones_warn() {
        let devs = crate::devices::parse(
            "Available devices:\n  \
             CUDA0: NVIDIA GeForce RTX 4070 SUPER (12281 MiB, 10844 MiB free)\n  \
             ROCm0: AMD Radeon AI PRO R9700 (32624 MiB, 32462 MiB free)\n",
        );
        let got = device_names("ROCm0, CUDA0", &devs);
        assert_eq!(got[0].1, "AMD Radeon AI PRO R9700");
        assert_eq!(got[1].1, "NVIDIA GeForce RTX 4070 SUPER");
        assert_eq!(unknown_device_warning(&[("p".into(), got)]), None);

        // A stale id keeps its slot with an empty name, and is called out.
        let stale = device_names("ROCm1", &devs);
        assert_eq!(stale, vec![("ROCm1".to_string(), String::new())]);
        let w = unknown_device_warning(&[("p".into(), stale)]).expect("warned");
        assert!(w.contains("p -> ROCm1"), "{w}");

        // An empty device list is "llama.cpp picks", not an error.
        assert!(device_names("", &devs).is_empty());
    }

    // The raw row is the audit trail; wrapping it must not lose it, and a row
    // that is not JSON must not take the run down with it.
    #[test]
    fn the_raw_bench_row_is_kept_verbatim() {
        let row = r#"{"n_prompt":2048,"avg_ts":900.0,"flash_attn":true,"n_threads":18}"#;
        let v: serde_json::Value = serde_json::from_str(&bench_row_json("qwen", row)).unwrap();
        assert_eq!(v["kind"], "bench_row");
        assert_eq!(v["preset"], "qwen");
        assert_eq!(v["row"]["n_threads"], serde_json::json!(18));
        assert_eq!(v["row"]["flash_attn"], serde_json::json!(true));

        let v: serde_json::Value = serde_json::from_str(&bench_row_json("q", "junk")).unwrap();
        assert!(v["row"].is_null(), "unparseable rows degrade, not panic");
    }

    // Warnings go ABOVE the table: a reader who stops at the numbers must
    // already have seen why they may not mean what they look like.
    #[test]
    fn warnings_precede_the_results_and_the_environment_is_recorded() {
        let plan = Plan {
            presets: vec!["a".into()],
            ..Plan::default()
        };
        let meta = RunMeta {
            warnings: vec!["legs disagree".into()],
            env: vec![("booted".into(), "yesterday".into())],
            ..RunMeta::default()
        };
        let md = render_report(&plan, &[], &meta);
        let warn_at = md.find("legs disagree").expect("warning present");
        let results_at = md.find("## Results").expect("results heading");
        assert!(warn_at < results_at, "the warning must come first");
        assert!(md.contains("## Environment"));
        assert!(md.contains("| booted | yesterday |"));
    }

    #[test]
    fn mean_and_sample_stddev() {
        let (m, sd) = mean_sd(&[10.0, 12.0, 14.0]);
        assert!((m - 12.0).abs() < 1e-9);
        assert!((sd - 2.0).abs() < 1e-9);
        assert_eq!(mean_sd(&[5.0]), (5.0, 0.0));
        assert_eq!(mean_sd(&[]), (0.0, 0.0));
    }

    #[test]
    fn validate_refuses_the_plans_that_would_benchmark_nothing() {
        assert!(validate(&Plan::default()).is_some(), "no preset selected");
        // A default plan carries a prompt FILE and no text: the caller reads
        // one into the other at Run. So it must NOT validate, or a forgotten
        // read would benchmark the model answering nothing, and the message
        // has to name the file the text was supposed to come from.
        let unresolved = Plan {
            presets: vec!["a".into()],
            ..Plan::default()
        };
        let problem = validate(&unresolved).expect("an unresolved prompt is not a plan");
        assert!(
            problem.contains("bench-prompt.txt"),
            "the message names the file: {problem}"
        );

        let base = Plan {
            prompt: DEFAULT_PROMPT.into(),
            ..unresolved
        };
        assert!(validate(&base).is_none());
        assert!(validate(&Plan {
            prompt: "  ".into(),
            ..base.clone()
        })
        .is_some());
        // The synthetic mode needs no prompt, so a blank one must not stop it.
        assert!(validate(&Plan {
            mode: Mode::Synthetic,
            prompt: String::new(),
            ..base.clone()
        })
        .is_none());
        assert!(validate(&Plan { reps: 0, ..base }).is_some());
    }

    /// The two normalizations that decide whether two runs are comparable: a
    /// BOM is a token of the prompt if it survives, and CRLF against LF is a
    /// different tokenization of "the same" text saved by two editors.
    #[test]
    fn normalize_strips_the_bom_and_folds_crlf() {
        assert_eq!(normalize_prompt("\u{feff}hello"), "hello");
        assert_eq!(normalize_prompt("a\r\nb\r\n"), "a\nb\n");
        assert_eq!(normalize_prompt("\u{feff}a\r\nb"), "a\nb");
        // A lone CR is left alone: it is not a line ending Windows editors
        // write, and rewriting bytes nobody asked about is how a prompt stops
        // being what the file says.
        assert_eq!(normalize_prompt("a\rb"), "a\rb");
        // Trailing whitespace survives: a prompt that ends in a newline is a
        // deliberately different prompt for some templates.
        assert_eq!(normalize_prompt("hi \n"), "hi \n");
        // The digest follows the normalization, which is the whole point: the
        // same prompt saved two ways must hash the same.
        assert_eq!(
            prompt_info(&normalize_prompt("a\r\nb")).sha256,
            prompt_info(&normalize_prompt("a\nb")).sha256
        );
    }

    #[test]
    fn load_prompt_file_reads_normalizes_and_names_its_failures() {
        let dir = tempfile::tempdir().unwrap();
        let missing = dir.path().join("nope.txt");
        let err = load_prompt_file(&missing).expect_err("a missing file is an error");
        assert!(err.contains("nope.txt"), "the error names the file: {err}");

        let empty = dir.path().join("empty.txt");
        std::fs::write(&empty, "   \r\n  ").unwrap();
        let err = load_prompt_file(&empty).expect_err("whitespace is not a prompt");
        assert!(err.contains("empty.txt"), "the error names the file: {err}");

        let good = dir.path().join("p.txt");
        std::fs::write(&good, "\u{feff}line one\r\nline two").unwrap();
        assert_eq!(load_prompt_file(&good).unwrap(), "line one\nline two");
    }

    /// Seeding CREATES and never overwrites: the file is the user's, and the
    /// framework's prompt is only what a fresh machine starts from.
    #[test]
    fn ensure_prompt_file_seeds_once() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("nested").join("bench-prompt.txt");
        ensure_prompt_file(&path);
        assert_eq!(std::fs::read_to_string(&path).unwrap(), DEFAULT_PROMPT);

        std::fs::write(&path, "mine").unwrap();
        ensure_prompt_file(&path);
        assert_eq!(
            std::fs::read_to_string(&path).unwrap(),
            "mine",
            "an existing prompt must survive a re-seed"
        );
    }

    /// The seed follows the FILE NAME, or deleting the long prompt would bring
    /// back a 230-character one under a name promising 40k tokens, and a sweep
    /// would then rank settings in the shallow regime while the report named
    /// the deep file.
    #[test]
    fn the_long_prompts_name_seeds_the_long_prompt() {
        let dir = tempfile::tempdir().unwrap();
        let long = dir.path().join("bench-prompt-long.txt");
        ensure_prompt_file(&long);
        assert_eq!(std::fs::read_to_string(&long).unwrap(), LONG_PROMPT);

        let other = dir.path().join("my-own.txt");
        ensure_prompt_file(&other);
        assert_eq!(std::fs::read_to_string(&other).unwrap(), DEFAULT_PROMPT);
    }

    /// The shipped long prompt is only worth shipping if it is actually long,
    /// and only usable if it survives normalization unchanged: it is compiled
    /// in as bytes, so a CRLF checkout (or a BOM) would make it a different
    /// prompt on a different machine. `.gitattributes` pins that; this notices
    /// if the pin is ever lost.
    #[test]
    fn the_shipped_long_prompt_is_long_and_already_normalized() {
        assert!(
            LONG_PROMPT.chars().count() > 100_000,
            "the long prompt is meant to be tens of thousands of tokens, got {}",
            LONG_PROMPT.chars().count()
        );
        assert_eq!(
            normalize_prompt(LONG_PROMPT),
            LONG_PROMPT,
            "the checked-in asset must already be LF and BOM-free"
        );
    }

    /// The preview elides a long prompt; the REQUEST never does. One formatter
    /// builds both, so this pins the only difference between them.
    #[test]
    fn the_preview_elides_the_prompt_and_the_request_does_not() {
        let long: String = "x".repeat(PREVIEW_PROMPT_CHARS * 3);
        let plan = Plan {
            presets: vec!["a".into()],
            prompt: long.clone(),
            ..Plan::default()
        };
        let preview = live_preview(8080, "a", &plan);
        assert!(preview.contains("chars shown]"), "the preview says it cut");
        assert!(preview.len() < long.len(), "and actually cut");

        let body = live_body("a", &plan);
        let parsed: serde_json::Value = serde_json::from_str(&body).unwrap();
        assert_eq!(parsed["messages"][0]["content"], serde_json::json!(long));
        assert_eq!(parsed["cache_prompt"], serde_json::json!(false));
    }

    /// What makes an old comparison checkable: the report identifies the prompt
    /// by path AND digest, and embeds the text only up to a cap.
    #[test]
    fn the_report_identifies_the_prompt_by_path_and_digest() {
        let plan = Plan {
            presets: vec!["a".into()],
            prompt: DEFAULT_PROMPT.into(),
            prompt_file: Some(PathBuf::from(r"D:\prompts\short.txt")),
            ..Plan::default()
        };
        let md = render_report(&plan, &[], &RunMeta::default());
        assert!(md.contains(r"short.txt"), "the path is in the report");
        assert!(md.contains(&crate::sha256::hex(DEFAULT_PROMPT.as_bytes())));
        assert!(
            md.contains(DEFAULT_PROMPT),
            "a short prompt is embedded whole"
        );

        // Inline (`--prompt`) says so instead of printing an invented path.
        let inline = Plan {
            prompt_file: None,
            ..plan.clone()
        };
        assert!(render_report(&inline, &[], &RunMeta::default()).contains("--prompt"));

        // Past the cap the text is cut, the digest is not.
        let long = Plan {
            prompt: "y".repeat(REPORT_PROMPT_CHARS * 2),
            ..plan
        };
        let md = render_report(&long, &[], &RunMeta::default());
        assert!(md.contains("chars shown]"));
        assert!(
            md.len() < REPORT_PROMPT_CHARS * 2,
            "the report stayed small"
        );
        assert!(md.contains(&crate::sha256::hex(long.prompt.as_bytes())));
    }

    /// The jsonl header carries the prompt's identity too: comparing two old
    /// runs is a machine-readable question, and until the prompt became a file
    /// this stream said nothing about it at all.
    #[test]
    fn the_run_header_carries_the_prompt_digest_for_live_runs_only() {
        let plan = Plan {
            presets: vec!["a".into()],
            prompt: DEFAULT_PROMPT.into(),
            prompt_file: Some(PathBuf::from("p.txt")),
            ..Plan::default()
        };
        let header: serde_json::Value =
            serde_json::from_str(&run_header_json(&plan, &RunMeta::default())).unwrap();
        assert_eq!(header["prompt_file"], serde_json::json!("p.txt"));
        assert_eq!(
            header["prompt_sha256"],
            serde_json::json!(crate::sha256::hex(DEFAULT_PROMPT.as_bytes()))
        );
        assert_eq!(
            header["prompt_chars"],
            serde_json::json!(DEFAULT_PROMPT.chars().count())
        );

        // The synthetic engine has no prompt to identify, so claiming one would
        // be a lie in the machine-readable half of the record.
        let synthetic = Plan {
            mode: Mode::Synthetic,
            ..plan
        };
        let header: serde_json::Value =
            serde_json::from_str(&run_header_json(&synthetic, &RunMeta::default())).unwrap();
        assert!(header.get("prompt_sha256").is_none());
    }

    #[test]
    fn stamps_are_utc_and_sort_chronologically() {
        // 2026-08-31 12:34:56 UTC.
        let secs = 1_788_179_696;
        assert_eq!(stamp(secs), "20260831-123456");
        assert_eq!(stamp_human(secs), "2026-08-31 12:34:56 UTC");
        assert_eq!(stamp_label("20260831-123456"), "2026-08-31 12:34:56 UTC");
        assert_eq!(stamp_label("whatever"), "whatever");
        assert!(stamp(secs) > stamp(secs - 1));
        assert_eq!(stamp(0), "19700101-000000");
    }

    #[test]
    fn the_report_names_what_the_mode_cannot_measure() {
        let plan = Plan {
            presets: vec!["a".into()],
            mode: Mode::Synthetic,
            ..Plan::default()
        };
        let md = render_report(&plan, &[], &RunMeta::default());
        assert!(md.contains("synthetic (llama-bench)"));
        assert!(md.contains("No results were collected."));
        assert!(md.contains("No speculative decoding"));
        assert!(md.contains("takes no prompt"));

        let plan = Plan {
            mode: Mode::Live,
            ..plan
        };
        let md = render_report(&plan, &[], &RunMeta::default());
        assert!(md.contains("cache_prompt"));
        assert!(md.contains("## Prompt"));
    }
}
