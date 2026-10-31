//! Headless CLI dispatcher for llama-cpp-config.

use anyhow::{Context, Result};
use clap::{Args, Parser, Subcommand};

use crate::{bench, paths, presets, server_cfg};

// ── Command definitions (clap) ───────────────────────────────────────────

#[derive(Parser, Debug)]
#[command(
    name = "llama-cpp-config",
    version,
    about = "Configure llama.cpp-framework: llama-server and model presets. Run with no args for the GUI."
)]
pub struct Cli {
    #[command(subcommand)]
    pub command: Command,
}

#[derive(Subcommand, Debug)]
pub enum Command {
    /// Force-launch the GUI (default when no subcommand is given).
    Gui {
        /// Start hidden in the system tray instead of opening the window.
        /// This is what the "Start at login" entry launches
        /// (see src/startup.rs).
        #[arg(long)]
        minimized: bool,
    },
    /// Server-wide settings (server.ini).
    #[command(subcommand)]
    Server(ServerCmd),
    /// Per-model presets (presets.ini).
    #[command(subcommand)]
    Preset(PresetCmd),
    /// Control the llama-server process.
    #[command(subcommand)]
    Control(ControlCmd),
    /// Benchmarks that run unattended (the GUI's Benchmark tab, in a loop).
    #[command(subcommand)]
    Bench(BenchCmd),
}

#[derive(Subcommand, Debug)]
pub enum BenchCmd {
    /// Benchmark one preset once per value of ONE of its keys.
    ///
    /// Each value is written to presets.ini, measured by the same engine the
    /// Benchmark tab uses (each finished leg is a run of one bench.db campaign),
    /// and the key is put back afterwards. A live sweep restarts llama-server
    /// per value, because the key rides the child's launch args.
    // Boxed for the reason `ServerSet` is: one payload-carrying variant next to
    // the small ones would otherwise blow clippy's size threshold as the flags
    // grow. clap unwraps the Box itself, so the CLI surface is unchanged.
    Sweep(Box<BenchSweep>),
    /// Read bench.db: campaigns, models, machines, prompts, runs, summary,
    /// results, one run, the parameters two runs differ in, or any SELECT.
    Query(Box<BenchQuery>),
    /// Record a campaign's answer, the conclusion its runs support.
    Answer {
        /// The campaign's name (as `bench query campaigns` lists it).
        campaign: String,
        /// The answer, as it should read in the database.
        text: String,
        /// The database; default the working one under the data directory.
        #[arg(long)]
        db: Option<std::path::PathBuf>,
    },
    /// Copy campaigns, with everything their runs point at, into another
    /// bench.db (the repo's benchmarks/bench.db). The run logs stay behind.
    Publish {
        /// Campaign names.
        #[arg(required = true)]
        campaigns: Vec<String>,
        /// The database to copy into.
        #[arg(long)]
        to: std::path::PathBuf,
        /// The database to copy from; default the working one.
        #[arg(long)]
        from: Option<std::path::PathBuf>,
    },
}

#[derive(Args, Debug)]
pub struct BenchQuery {
    #[command(subcommand)]
    pub what: QueryWhat,
    /// The database; default the working one under the data directory (the
    /// repo's is benchmarks/bench.db).
    #[arg(long, global = true)]
    pub db: Option<std::path::PathBuf>,
    /// table, csv or json.
    #[arg(long, global = true, default_value = "table", value_parser = ["table", "csv", "json"])]
    pub format: String,
    /// Keep long text cells whole.
    #[arg(long, global = true)]
    pub full: bool,
    /// Parameter columns to add to runs and summary, comma-separated
    /// (ctx-size,cache-type-k).
    #[arg(long, global = true, value_delimiter = ',')]
    pub show: Vec<String>,
    /// Campaign name contains TEXT.
    #[arg(long, global = true)]
    pub campaign: Option<String>,
    /// Model name, file or quant contains TEXT.
    #[arg(long, global = true)]
    pub model: Option<String>,
    /// The run computed on a GPU named like TEXT (r9700, 4070).
    #[arg(long, global = true)]
    pub gpu: Option<String>,
    /// The preset's device contains TEXT (ROCm0, Vulkan).
    #[arg(long, global = true)]
    pub device: Option<String>,
    /// llama.cpp tag (v0.7.0).
    #[arg(long, global = true)]
    pub llama: Option<String>,
    /// ok, load-oom, run-oom, crash, error, interrupted.
    #[arg(long, global = true)]
    pub status: Option<String>,
    /// Runs started on or after DATE (2026-10-16).
    #[arg(long, global = true)]
    pub since: Option<String>,
    /// K=V (equals) or K~V (contains), any scope; repeatable.
    #[arg(long, global = true)]
    pub param: Vec<String>,
    /// results only: one prompt or llama-bench test.
    #[arg(long, global = true)]
    pub prompt: Option<String>,
}

#[derive(Subcommand, Debug)]
pub enum QueryWhat {
    /// The campaigns: question, answer, runs.
    Campaigns,
    /// The models tested, with runs and campaigns.
    Models,
    /// The machines: CPU, RAM, GPUs, OS.
    Machines,
    /// The prompts, with their token counts.
    Prompts,
    /// One line per run.
    Runs,
    /// One line per run and prompt (or test): mean +- sd of prefill and decode.
    Summary,
    /// One line per request (or llama-bench repetition).
    Results,
    /// Everything about one run.
    Run { id: i64 },
    /// The parameters that differ between two runs.
    Diff { a: i64, b: i64 },
    /// Any read-only query.
    Sql { query: String },
}

#[derive(Args, Debug)]
pub struct BenchSweep {
    /// Preset id to benchmark (as in `preset list`).
    #[arg(long)]
    pub preset: String,
    /// The presets.ini key to vary, e.g. spec-draft-n-max.
    #[arg(long)]
    pub key: String,
    /// Values to measure: `2,3,4`, the inclusive range `2..8`, or, when a value
    /// itself contains a comma (device, tensor-split), `;`-separated:
    /// `Vulkan1,CUDA0;CUDA0`. `unset` removes the key for that leg.
    #[arg(long)]
    pub values: String,
    /// live = the running llama-server (real prompt, the preset's drafter);
    /// synthetic = llama-bench (no prompt, no sampler, llama-server must be down).
    #[arg(long, default_value = "live", value_parser = ["live", "synthetic"])]
    pub mode: String,
    /// Repetitions per value.
    #[arg(long)]
    pub reps: Option<i32>,
    /// What the preset is left on when the sweep ends.
    #[arg(long, default_value = "original", value_parser = ["original", "best", "last"])]
    pub apply: String,
    /// Treat this as the value the key had before the sweep, instead of reading
    /// it from presets.ini. For after an interrupted sweep, which leaves the key
    /// on one of its leg values (the value to pass is in its pending marker,
    /// and the sweep warns naming both).
    #[arg(long)]
    pub restore_to: Option<String>,
    /// Live only: the prompt to send, inline. The one source that is not a
    /// file, and the report says so rather than naming one.
    #[arg(long, conflicts_with = "prompt_file")]
    pub prompt: Option<String>,
    /// Live only: read the prompt from a UTF-8 file. The way to measure the
    /// regime a long conversation actually runs in: a decode rate at 40k of
    /// context is a different number from the same setting's rate at 2k, and a
    /// prompt that size does not fit on a command line.
    ///
    /// With neither flag, the file the Benchmark tab uses (settings.ini
    /// `BenchPromptFile`, default the shipped `bench-prompt-43k-1.0.txt` in
    /// share/benchmarks/prompts), so a scripted sweep and a run from the GUI
    /// measure the same text. A `.json` prompt is a conversation with tools
    /// (`{"messages": [...], "tools": [...]}`), sent as given.
    #[arg(long)]
    pub prompt_file: Option<std::path::PathBuf>,
    /// Live only: sampling temperature, or `preset` to leave the preset's alone.
    /// Defaults to 0, which is what makes two legs comparable.
    #[arg(long)]
    pub temp: Option<String>,
    /// Live only: output cap per repetition. Omitted (or 0) = no cap: the
    /// request carries no `max_tokens` and the model stops on its own (or at
    /// the preset's `n-predict`), so the output length is part of the result.
    #[arg(long)]
    pub max_tokens: Option<i32>,
    /// Live only: requests per repetition. `single` (default) sends one;
    /// the others send one per slot of the preset at once (N = `parallel`,
    /// absent = 4; N >= 2), each pinned with `id_slot`: `prefill` (all N
    /// prefill the whole prompt), `decode` (every slot primed with the prompt
    /// first, so all N only decode), `prefill+decode` (N-1 primed slots decode
    /// while the last one prefills). The leg's run label ends in
    /// `concurrency=<scenario>`. `all` = single, then every concurrent
    /// scenario when the preset has two slots or more, one campaign.
    #[arg(long, value_parser = ["single", "prefill", "decode", "prefill+decode", "all"])]
    pub concurrency: Option<String>,
    /// Live only: run a versioned benchmark spec: `standard` (the framework's
    /// newest published version), `standard-<X.Y>` (that version: every one
    /// stays installed, in share/benchmarks/specs, the repo's
    /// benchmarks/specs for a tree build) or an INI file of your own. Its prompts (pinned by sha256), passes (temperature,
    /// repetitions, concurrency, preset keys overridden for the pass) and
    /// output cap, all in one campaign. Every run records the spec's name,
    /// version and sha256; a sweep without it records `spec = custom`, and so
    /// does a file that is not a published version of standard (its runs carry
    /// `spec-derived-from` and `spec-derived-fields = spec-file`).
    ///
    /// --prompt, --prompt-file, --temp, --max-tokens, --concurrency and --reps
    /// beside it replace the spec's value in every run, and so does a --key
    /// that a pass sets: the runs are then custom, derived from the spec.
    #[arg(long)]
    pub spec: Option<String>,
    /// Synthetic only: prompt lengths, comma-separated.
    #[arg(long)]
    pub prompt_lens: Option<String>,
    /// Synthetic only: KV depths, comma-separated.
    #[arg(long)]
    pub depths: Option<String>,
    /// Synthetic only: generation length.
    #[arg(long)]
    pub n_gen: Option<i32>,
    /// The bench.db campaign the legs are recorded in (created when new,
    /// added to when it exists). Default: `sweep-<stamp>`.
    #[arg(long)]
    pub campaign: Option<String>,
    /// The question the sweep answers, stored with the campaign. Default: one
    /// spelled from the key, the preset and the values.
    #[arg(long)]
    pub question: Option<String>,
}

#[derive(Subcommand, Debug)]
pub enum ControlCmd {
    /// Start llama-server.
    Start,
    /// Stop llama-server.
    Stop,
    /// Restart llama-server.
    Restart,
    /// Stop llama-server and close the config GUI if running.
    StopAndClose,
}

#[derive(Subcommand, Debug)]
pub enum ServerCmd {
    /// Print the current server.ini values.
    Show,
    /// Update one or more server.ini fields.
    // Boxed: `ServerSet` carries one Option per server.ini field, so it grows
    // with the schema and has outgrown clippy's enum-variant size threshold
    // against the payload-less `Show`. clap derives `Args` for the inner struct
    // and unwraps the Box itself, so the CLI surface is unchanged.
    Set(Box<ServerSet>),
}

#[derive(Args, Debug, Default)]
pub struct ServerSet {
    #[arg(long)]
    pub port: Option<i32>,
    /// Address(es) to listen on (--host): one, or several comma-separated
    /// (llama.cpp v0.5.0+). 0.0.0.0 = all interfaces.
    #[arg(long)]
    pub hostname: Option<String>,
    /// CPU threads for generation. 0 or negative clears the override (auto).
    #[arg(long)]
    pub threads: Option<i32>,
    /// CPU threads for prompt processing. 0 or negative clears the override (auto).
    #[arg(long)]
    pub threads_batch: Option<i32>,
    /// Models kept resident at once. Stored as-is (0 = unlimited); NOT cleared by 0.
    #[arg(long)]
    pub models_max: Option<i32>,
    /// MiB kept free per device; the router unloads its least recently used
    /// models until a new one fits. 0 = off; negative clears the key (llama.cpp
    /// default, also off).
    #[arg(long, allow_hyphen_values = true)]
    pub models_memory_margin: Option<i32>,
    #[arg(long)]
    pub models_dir: Option<String>,
    /// rocBLAS GEMM backend (env ROCBLAS_USE_HIPBLASLT): off = Tensile (the
    /// gfx1201 BF16/F16 workaround), on = hipBLASLt, default = leave the variable
    /// unset. A tri-state word rather than a bool, so `default` can clear it back.
    #[arg(long, value_parser = clap::builder::PossibleValuesParser::new(["default", "on", "off"]))]
    pub rocblas_use_hipblaslt: Option<String>,
    /// AllReduce of split-mode tensor (env GGML_CUDA_ALLREDUCE, CUDA and ROCm
    /// alike): nccl = the collective library, internal = ggml's own, none = the
    /// generic reduction (the one a split mixing CUDA and ROCm loads under),
    /// default = leave the variable unset.
    #[arg(long, value_parser = clap::builder::PossibleValuesParser::new(["default", "nccl", "internal", "none"]))]
    pub cuda_allreduce: Option<String>,
    /// Export GGML_CUDA_DISABLE_GRAPHS=1 (no CUDA/HIP graphs; diagnostic, slower
    /// decode). false = leave the variable unset.
    #[arg(long)]
    pub cuda_disable_graphs: Option<bool>,
    /// Export LLAMA_ATTN_ROT_DISABLE=1 (no Hadamard rotation of a quantized KV
    /// cache). false = leave the variable unset.
    #[arg(long)]
    pub attn_rot_disable: Option<bool>,
    /// FlashAttention band of the patched ROCm backend (env
    /// GGML_HIP_FA_BAND_WMMA): 4 or 2; 0 = off (the variable is not exported);
    /// default = clear the key, which exports the tool's default, 4.
    #[arg(long, value_parser = clap::builder::PossibleValuesParser::new(["default", "0", "2", "4"]))]
    pub hip_fa_band: Option<String>,
    /// Export ROCP_TOOL_ATTACH=1 (lets rocprofv3 --attach trace the running
    /// server). false = leave the variable unset.
    #[arg(long)]
    pub rocprof_attach: Option<bool>,
    /// Serve the built-in chat UI. false = launch with --no-ui (the API keeps
    /// working). true = on, llama.cpp's default.
    #[arg(long)]
    pub webui: Option<bool>,
    /// Enable the web UI's MCP CORS proxy (--webui-mcp-proxy). true = on.
    /// Not passed while --webui is false; the value is kept.
    #[arg(long)]
    pub webui_mcp_proxy: Option<bool>,
    /// llama-server log verbosity threshold (-lv / --log-verbosity): 0 output,
    /// 1 error, 2 warning, 3 info, 4 trace, 5 debug.
    #[arg(long)]
    pub log_verbosity: Option<i32>,
    /// Snapshot each loaded model's KV cache when the server stops and restore
    /// the newest one on the next start (--slot-save-path). true = on.
    #[arg(long)]
    pub save_state_on_shutdown: Option<bool>,
    /// Folder for those snapshots (~8.3 GiB per model at a 262k context).
    /// Empty = the default under the user's runtime root.
    #[arg(long)]
    pub state_dir: Option<String>,
    /// Override the integration base URL (opencode.json + Claude Code). Empty = auto.
    #[arg(long)]
    pub opencode_base_url: Option<String>,
    /// API key for the integration provider. Empty = none.
    #[arg(long)]
    pub opencode_api_key: Option<String>,
}

impl ServerSet {
    /// Copy every provided flag into `cfg`, applying each field's clearing rule
    /// (see the per-field docs above): a `None` flag leaves the field untouched;
    /// non-positive thread/reuse values clear the override; a blank string
    /// unsets any optional string field (`opt_nonblank`, matching `load()`).
    /// The single, unit-tested home for `server set`'s field mapping; keep it
    /// in lockstep with the `ServerConfig` schema.
    fn apply(&self, cfg: &mut server_cfg::ServerConfig) {
        if let Some(p) = self.port {
            cfg.port = Some(p);
        }
        if let Some(h) = &self.hostname {
            cfg.hostname = server_cfg::opt_nonblank(Some(h.clone()));
        }
        if let Some(t) = self.threads {
            cfg.threads = if t > 0 { Some(t) } else { None };
        }
        if let Some(tb) = self.threads_batch {
            cfg.threads_batch = if tb > 0 { Some(tb) } else { None };
        }
        if let Some(m) = self.models_max {
            cfg.models_max = Some(m);
        }
        if let Some(m) = self.models_memory_margin {
            cfg.models_memory_margin = if m >= 0 { Some(m) } else { None };
        }
        if let Some(d) = &self.models_dir {
            cfg.models_dir = server_cfg::opt_nonblank(Some(d.clone()));
        }
        // The tri-state's clearing rule: `default` is how the CLI reaches the
        // unset state (a bool flag could only ever set one of the other two).
        // clap's PossibleValuesParser already refused anything else.
        if let Some(hb) = &self.rocblas_use_hipblaslt {
            cfg.rocblas_use_hipblaslt = crate::form::tri_bool(hb);
        }
        // Same clearing rule: `default` normalizes to None, the unset state.
        if let Some(ar) = &self.cuda_allreduce {
            cfg.cuda_allreduce = server_cfg::normalize_allreduce(ar).map(str::to_string);
        }
        if let Some(g) = self.cuda_disable_graphs {
            cfg.cuda_disable_graphs = g;
        }
        if let Some(r) = self.attn_rot_disable {
            cfg.attn_rot_disable = r;
        }
        // Same clearing rule as the tri-states: `default` normalizes to None,
        // the unset state; clap already refused anything but default/0/2/4.
        if let Some(b) = &self.hip_fa_band {
            cfg.hip_fa_band = server_cfg::normalize_hip_fa_band(b);
        }
        if let Some(a) = self.rocprof_attach {
            cfg.rocprof_attach = a;
        }
        if let Some(u) = self.webui {
            cfg.webui = Some(u);
        }
        if let Some(w) = self.webui_mcp_proxy {
            cfg.webui_mcp_proxy = Some(w);
        }
        if let Some(lv) = self.log_verbosity {
            cfg.log_verbosity = Some(lv);
        }
        if let Some(v) = self.save_state_on_shutdown {
            cfg.save_state_on_shutdown = Some(v);
        }
        if let Some(dir) = &self.state_dir {
            cfg.state_dir = server_cfg::opt_nonblank(Some(dir.clone()));
        }
        if let Some(url) = &self.opencode_base_url {
            cfg.opencode_base_url = server_cfg::opt_nonblank(Some(url.clone()));
        }
        if let Some(key) = &self.opencode_api_key {
            cfg.opencode_api_key = server_cfg::opt_nonblank(Some(key.clone()));
        }
    }
}

#[derive(Subcommand, Debug)]
pub enum PresetCmd {
    /// List preset ids and the resolved model path for each, switched-off ones
    /// included (marked).
    List,
    /// Dump one preset as INI.
    Show { id: String },
    /// Delete a preset section; with --delete-files, the model files it links
    /// too. Without that flag every file stays on disk.
    Delete {
        id: String,
        /// Also delete these files the preset links, when they are on disk:
        /// `all`, or a comma-separated list of `model` (every shard of a split
        /// model), `mmproj` and `draft`. The folders a Hugging Face download
        /// leaves empty under ModelsDir's models / mmprojs / mtps / dflashs go
        /// too; nothing outside them, and never a folder with content. A file
        /// another preset references (in presets.ini or presets.ini.disabled)
        /// is refused, before anything is deleted, unless --even-if-shared.
        #[arg(long, value_name = "WHICH", value_delimiter = ',')]
        delete_files: Vec<FileKind>,
        /// Delete a picked file even when another preset references it (that
        /// preset then points at a missing file).
        #[arg(long, requires = "delete_files")]
        even_if_shared: bool,
    },
    /// Switch a preset on: move it back into presets.ini, so llama-server
    /// (after a restart) and OpenCode offer it again.
    Enable { id: String },
    /// Switch a preset off: park it in presets.ini.disabled, unchanged, where
    /// llama-server and OpenCode no longer see it.
    Disable { id: String },
    /// Download a GGUF from Hugging Face with llama.cpp's own downloader and
    /// create a preset for it. Lists the repo's quantizations first; the files
    /// land in ModelsDir's models / mmprojs / mtps / dflashs subfolders.
    ///
    /// Given words instead of a repository (`"qwen3 8b"`), it SEARCHES: one
    /// row per GGUF repository, most downloaded first, with its downloads and
    /// the quantizations it offers, and exits; run it again with the chosen
    /// `org/repo`.
    Download(Box<PresetDownload>),
}

/// Which linked files `preset delete --delete-files` takes (`linked_files::Role`).
#[derive(clap::ValueEnum, Clone, Copy, Debug, PartialEq, Eq)]
pub enum FileKind {
    /// Every file the preset links.
    All,
    /// The GGUF model, every shard of a split one.
    Model,
    /// The multimodal projector.
    Mmproj,
    /// The speculative drafter (`model-draft`: MTP, DFlash or DSpark).
    Draft,
}

impl FileKind {
    fn covers(self, role: crate::linked_files::Role) -> bool {
        use crate::linked_files::Role;
        matches!(
            (self, role),
            (FileKind::All, _)
                | (FileKind::Model, Role::Model)
                | (FileKind::Mmproj, Role::Mmproj)
                | (FileKind::Draft, Role::Draft)
        )
    }
}

/// The entries of `linked` that `which` asks for, in `linked`'s order.
fn pick_files(
    linked: &[crate::linked_files::LinkedFile],
    which: &[FileKind],
) -> Vec<crate::linked_files::LinkedFile> {
    linked
        .iter()
        .filter(|f| which.iter().any(|k| k.covers(f.role)))
        .cloned()
        .collect()
}

/// The refusal `preset delete` stops on when a picked file is in use, naming
/// each such file and the presets holding it; `None` when nothing is shared.
fn shared_refusal(chosen: &[crate::linked_files::LinkedFile]) -> Option<String> {
    let shared: Vec<String> = chosen
        .iter()
        .filter(|f| f.in_use())
        .map(|f| {
            let ids: Vec<String> = f.used_by.iter().map(|id| format!("[{id}]")).collect();
            format!("{} is used by {}", f.path.display(), ids.join(", "))
        })
        .collect();
    (!shared.is_empty()).then(|| {
        format!(
            "nothing deleted: {}. Leave it out of --delete-files, or pass --even-if-shared to delete it anyway.",
            shared.join("; ")
        )
    })
}

#[derive(Args, Debug)]
pub struct PresetDownload {
    /// `org/repo`, `org/repo:QUANT`, the repo's huggingface.co URL, or words
    /// to search for (`"qwen3 8b"`: anything that is not a repository spec).
    pub spec: String,
    /// Search even when the text would parse as a repository (`unsloth/Qwen3`
    /// is a valid repo id; with --search it is a search for those words).
    #[arg(long)]
    pub search: bool,
    /// How many repositories a search prints.
    #[arg(long, default_value_t = crate::hf::SEARCH_LIMIT, value_name = "N")]
    pub limit: usize,
    /// Only list the quantizations (size, sidecars) and exit.
    #[arg(long)]
    pub list: bool,
    /// The quantization to fetch, by tag (`Q4_K_M`) or by its file path in the
    /// repo. Default: the spec's tag, else llama.cpp's own pick (Q4_K_M, then
    /// Q8_0, then the first).
    #[arg(long)]
    pub quant: Option<String>,
    /// Skip the multimodal projector even when the repo has one.
    #[arg(long)]
    pub no_mmproj: bool,
    /// Also fetch a speculative drafter: mtp, dflash or dspark.
    #[arg(long, value_name = "KIND")]
    pub draft: Option<String>,
    /// The preset id. Default: derived from the model's file name.
    #[arg(long)]
    pub id: Option<String>,
}

// ── Dispatch & rendering ─────────────────────────────────────────────────

pub fn run(cli: Cli) -> Result<()> {
    match cli.command {
        Command::Gui { minimized } => crate::gui::run(minimized),
        Command::Server(c) => run_server(c),
        Command::Preset(c) => run_preset(c),
        Command::Control(c) => run_control(c),
        Command::Bench(c) => run_bench(c),
    }
}

/// The `bench sweep` leg: turn the flags into a `sweep::Opts` and run it. Every
/// flag left out falls back to the Benchmark tab's own default, so a sweep and a
/// run started from the GUI measure the same thing.
fn run_bench(c: BenchCmd) -> Result<()> {
    let a = match c {
        BenchCmd::Sweep(a) => a,
        BenchCmd::Query(q) => return run_bench_query(*q),
        BenchCmd::Answer { campaign, text, db } => {
            let path = db.unwrap_or_else(bench::db::work_path);
            bench::query::answer(&path, &campaign, &text).map_err(|e| anyhow::anyhow!(e))?;
            println!("{campaign}: answer recorded in {}", path.display());
            return Ok(());
        }
        BenchCmd::Publish {
            campaigns,
            to,
            from,
        } => {
            let from = from.unwrap_or_else(bench::db::work_path);
            for (name, runs) in
                bench::query::publish(&from, &to, &campaigns).map_err(|e| anyhow::anyhow!(e))?
            {
                println!("{name}: {runs} run(s) copied to {}", to.display());
            }
            return Ok(());
        }
    };
    let values = bench::sweep::parse_values(&a.values).map_err(|e| anyhow::anyhow!(e))?;

    let mut plan = bench::Plan {
        mode: bench::Mode::from_str(&a.mode),
        ..bench::Plan::default()
    };
    if let Some(reps) = a.reps {
        plan.reps = reps;
    }
    // The prompt, in the one place both front ends agree on: a FILE, read here
    // rather than in the sweep so a missing or unreadable one fails before
    // anything restarts llama-server. `--prompt` is the single escape hatch and
    // the only source that is not a file (hence the `None` provenance, which is
    // what the report prints). With neither flag the configured file is used
    // (the shipped 43k prompt when none is): the same bytes the Benchmark tab runs, so a sweep from a script and a run
    // from the GUI are comparable by construction.
    let prompt_given = a.prompt.is_some() || a.prompt_file.is_some();
    if let Some(prompt) = a.prompt {
        plan.prompt = bench::normalize_prompt(&prompt);
        plan.prompt_file = None;
    } else if plan.mode == bench::Mode::Live && (a.spec.is_none() || a.prompt_file.is_some()) {
        let path = match a.prompt_file.as_deref() {
            Some(p) => p.to_path_buf(),
            None => crate::settings::load().bench_prompt_path(),
        };
        plan.prompt = bench::load_prompt_file(&path).map_err(|e| anyhow::anyhow!(e))?;
        plan.prompt_file = Some(path);
    }
    if let Some(temp) = a.temp.as_deref() {
        plan.temp = if temp.eq_ignore_ascii_case("preset") {
            None
        } else {
            Some(
                temp.parse::<f64>()
                    .with_context(|| format!("--temp {temp} is not a number (or `preset`)"))?,
            )
        };
    }
    // No flag = no cap (unlike the tab, whose field always holds a number):
    // how long the model talks under a setting is part of what a sweep shows.
    plan.max_tokens = a.max_tokens.unwrap_or(0);
    let concurrency_all = a.concurrency.as_deref() == Some("all");
    if let Some(c) = a.concurrency.as_deref().filter(|c| *c != "all") {
        plan.concurrency = bench::Concurrency::from_str(c)
            .ok_or_else(|| anyhow::anyhow!("--concurrency {c}: unknown scenario"))?;
    }
    // Refused by value rather than parsed leniently: an entry the parser drops
    // is a length or a depth the sweep silently never measures.
    if let Some(lens) = a.prompt_lens.as_deref() {
        if let Some(problem) = bench::int_list_problem("--prompt-lens", lens) {
            anyhow::bail!(problem);
        }
        plan.prompt_lens = bench::parse_int_list(lens);
    }
    if let Some(depths) = a.depths.as_deref() {
        if let Some(problem) = bench::int_list_problem("--depths", depths) {
            anyhow::bail!(problem);
        }
        plan.depths = bench::parse_int_list(depths);
    }
    if let Some(n_gen) = a.n_gen {
        plan.n_gen = n_gen;
    }

    // Beside --spec, every method flag given replaces the spec's value.
    let derive = bench::spec::Derive {
        prompt: prompt_given.then(|| vec![(plan.prompt_file.clone(), plan.prompt.clone())]),
        temp: a.temp.is_some().then_some(plan.temp),
        max_tokens: a.max_tokens,
        reps: a.reps,
        concurrency: a
            .concurrency
            .as_deref()
            .map(|_| (!concurrency_all).then_some(plan.concurrency)),
    };
    let opts = bench::sweep::Opts {
        preset: a.preset,
        key: a.key,
        values,
        plan,
        apply: bench::sweep::Apply::from_str(&a.apply),
        restore_to: a.restore_to,
        campaign: a.campaign.unwrap_or_default(),
        question: a.question.unwrap_or_default(),
    };
    // Loaded before the lock, so a broken spec fails at once instead of after
    // a wait for the cards.
    let spec = match a.spec.as_deref() {
        Some(s) => Some(bench::spec::load(s).map_err(|e| anyhow::anyhow!(e))?),
        None => None,
    };
    // For the whole sweep, every pass and scenario included: another
    // benchmark on the same cards would be measured as this one's.
    let _gpu = bench::gpu_lock::acquire(|m| println!("{m}")).map_err(|e| anyhow::anyhow!(e))?;
    if let Some(spec) = &spec {
        bench::spec::run(spec, &opts, &derive).map_err(|e| anyhow::anyhow!(e))?;
        return Ok(());
    }
    if !concurrency_all {
        bench::sweep::run(&opts).map_err(|e| anyhow::anyhow!(e))?;
        return Ok(());
    }
    // One campaign for every scenario: name it now, or each sweep would
    // stamp its own.
    let campaign = if opts.campaign.is_empty() {
        format!("sweep-{}", bench::stamp(bench::now_secs()))
    } else {
        opts.campaign.clone()
    };
    let mut problems = Vec::new();
    for c in bench::sweep::all_scenarios(&opts.preset).map_err(|e| anyhow::anyhow!(e))? {
        let o = bench::sweep::Opts {
            plan: bench::Plan {
                concurrency: c,
                ..opts.plan.clone()
            },
            campaign: campaign.clone(),
            ..opts.clone()
        };
        if let Err(e) = bench::sweep::run(&o) {
            problems.push(format!("concurrency {}: {e}", c.as_str()));
        }
    }
    if !problems.is_empty() {
        anyhow::bail!(problems.join("; "));
    }
    Ok(())
}

fn run_bench_query(q: BenchQuery) -> Result<()> {
    use bench::query::{Cmd, Filters, Format, Opts};
    let path = q.db.unwrap_or_else(bench::db::work_path);
    let conn = bench::db::open_ro(&path).map_err(|e| anyhow::anyhow!(e))?;
    let cmd = match q.what {
        QueryWhat::Campaigns => Cmd::Campaigns,
        QueryWhat::Models => Cmd::Models,
        QueryWhat::Machines => Cmd::Machines,
        QueryWhat::Prompts => Cmd::Prompts,
        QueryWhat::Runs => Cmd::Runs,
        QueryWhat::Summary => Cmd::Summary,
        QueryWhat::Results => Cmd::Results,
        QueryWhat::Run { id } => Cmd::Run(id),
        QueryWhat::Diff { a, b } => Cmd::Diff(a, b),
        QueryWhat::Sql { query } => Cmd::Sql(query),
    };
    let filters = Filters {
        run_id: None,
        campaign: q.campaign,
        model: q.model,
        gpu: q.gpu,
        device: q.device,
        llama: q.llama,
        status: q.status,
        since: q.since,
        params: q.param,
        prompt: q.prompt,
    };
    let opts = Opts {
        format: Format::from_str(&q.format),
        full: q.full,
        show: q.show,
    };
    let table = bench::query::run(&conn, &cmd, &filters, &opts).map_err(|e| anyhow::anyhow!(e))?;
    println!("{}", bench::query::render(&table, &opts));
    Ok(())
}

/// The aligned body of `server show`, one `  Label        value` row per field
/// (the label column fits the longest key, "RocblasUseHipblaslt:"). Pure so the
/// test below can pin that every `ServerConfig` field is printed; a field added
/// to the schema but forgotten here would otherwise be a silent omission.
fn show_lines(cfg: &server_cfg::ServerConfig) -> String {
    let mut out = String::new();
    let mut row = |label: &str, value: String| {
        out.push_str(&format!("  {label:<21} {value}\n"));
    };
    row("Port:", cfg.port.map_or("-".into(), |v| v.to_string()));
    row(
        "Hostname:",
        cfg.hostname.clone().unwrap_or_else(|| "-".into()),
    );
    row(
        "Threads:",
        cfg.threads.map_or_else(|| "auto".into(), |v| v.to_string()),
    );
    row(
        "ThreadsBatch:",
        cfg.threads_batch
            .map_or_else(|| "auto".into(), |v| v.to_string()),
    );
    row(
        "ModelsMax:",
        cfg.models_max
            .map_or_else(|| "auto (default: 4)".into(), |v| v.to_string()),
    );
    row(
        "ModelsMemoryMargin:",
        cfg.models_memory_margin
            .map_or_else(|| "off (default)".into(), |v| format!("{v} MiB")),
    );
    row(
        "ModelsDir:",
        cfg.models_dir.clone().unwrap_or_else(|| "-".into()),
    );
    row(
        "RocblasUseHipblaslt:",
        cfg.rocblas_use_hipblaslt.map_or_else(
            || "default (rocBLAS decides)".into(),
            |v| {
                if v {
                    "on (hipBLASLt)".into()
                } else {
                    "off (Tensile)".to_string()
                }
            },
        ),
    );
    row(
        "CudaAllreduce:",
        cfg.cuda_allreduce
            .clone()
            .unwrap_or_else(|| "default (the backend decides)".into()),
    );
    row("CudaDisableGraphs:", cfg.cuda_disable_graphs.to_string());
    row("AttnRotDisable:", cfg.attn_rot_disable.to_string());
    row(
        "HipFaBand:",
        cfg.hip_fa_band
            .map_or_else(|| "4 (default)".into(), |v| v.to_string()),
    );
    row("RocprofAttach:", cfg.rocprof_attach.to_string());
    row(
        "Webui:",
        cfg.webui
            .map_or_else(|| "true (default)".into(), |v| v.to_string()),
    );
    row(
        "WebuiMcpProxy:",
        cfg.webui_mcp_proxy
            .map_or_else(|| "true (default)".into(), |v| v.to_string()),
    );
    row(
        "LogVerbosity:",
        cfg.log_verbosity
            .map_or_else(|| "4 (default)".into(), |v| v.to_string()),
    );
    row(
        "SaveStateOnShutdown:",
        cfg.save_state_on_shutdown
            .map_or_else(|| "false (default)".into(), |v| v.to_string()),
    );
    row(
        "StateDir:",
        cfg.state_dir
            .clone()
            .unwrap_or_else(|| format!("{} (default)", cfg.state_dir_or_default())),
    );
    row(
        "OpencodeBaseUrl:",
        cfg.opencode_base_url
            .clone()
            .unwrap_or_else(|| "auto (host:port, /v1 appended)".into()),
    );
    row(
        "OpencodeApiKey:",
        cfg.opencode_api_key
            .clone()
            .unwrap_or_else(|| "(none)".into()),
    );
    out
}

fn run_server(c: ServerCmd) -> Result<()> {
    match c {
        ServerCmd::Show => {
            let cfg = server_cfg::load();
            println!("server.ini: {}", paths::server_ini().display());
            print!("{}", show_lines(&cfg));
            Ok(())
        }
        ServerCmd::Set(s) => {
            let mut cfg = server_cfg::load();
            s.apply(&mut cfg);
            server_cfg::save(&cfg).context("save server.ini")?;
            println!("Wrote {}", paths::server_ini().display());
            Ok(())
        }
    }
}

/// Look a preset up in BOTH files, case-insensitively like the whole INI section
/// layer (read_section, rename_section, delete_section all use
/// eq_ignore_ascii_case). The writers below act on the STORED id so the header
/// is hit whatever case the user typed.
fn find_listed(id: &str) -> Result<presets::Listed> {
    presets::load_listed()
        .into_iter()
        .find(|l| l.preset.id.eq_ignore_ascii_case(id))
        .ok_or_else(|| {
            anyhow::anyhow!("No preset named `{id}`. Run `llama-cpp-config preset list`.")
        })
}

/// The same follow-up the GUI makes after every preset change: a configured
/// OpenCode provider re-derives its model list from the enabled presets. Never
/// fatal: the preset change itself has already landed.
fn follow_opencode() {
    match crate::integrations::follow_presets() {
        Ok(true) => println!(
            "Updated the model list in {}",
            paths::opencode_user_config().display()
        ),
        Ok(false) => {}
        Err(e) => eprintln!("opencode.json not updated: {e:#}"),
    }
}

fn run_preset(c: PresetCmd) -> Result<()> {
    match c {
        PresetCmd::List => {
            let listed = presets::load_listed();
            println!("presets.ini: {}", paths::presets_ini().display());
            if listed.iter().any(|l| !l.enabled) {
                println!("switched off: {}", paths::presets_disabled_ini().display());
            }
            if listed.is_empty() {
                println!("  (no presets defined)");
            }
            for l in listed {
                let off = if l.enabled { "" } else { "  (off)" };
                println!("  [{}]{off}  model={}", l.preset.id, l.preset.model);
            }
            Ok(())
        }
        PresetCmd::Show { id } => {
            let l = find_listed(&id)?;
            if !l.enabled {
                println!("; switched off: parked in presets.ini.disabled");
            }
            println!("{}", presets::render_section(&l.preset));
            Ok(())
        }
        PresetCmd::Delete {
            id,
            delete_files,
            even_if_shared,
        } => run_preset_delete(&id, &delete_files, even_if_shared),
        PresetCmd::Enable { id } => set_preset_enabled(&id, true),
        PresetCmd::Disable { id } => set_preset_enabled(&id, false),
        PresetCmd::Download(a) => run_preset_download(*a),
    }
}

/// `preset delete`: the section, then the linked files `which` names (none by
/// default). Everything that can refuse is checked BEFORE the section goes: an
/// unknown id, and a picked file another preset references (`shared_refusal`,
/// lifted by `even_if_shared`). A file that then fails to delete does not undo
/// the rest; it is reported and the command exits with an error.
fn run_preset_delete(id: &str, which: &[FileKind], even_if_shared: bool) -> Result<()> {
    use crate::linked_files;
    // ini::delete_section is a documented no-op for a missing section,
    // so look the id up first (mirroring Show), or a typo'd id gets a
    // "Removed" message for a preset that never existed.
    let listed = presets::load_listed();
    let l = find_listed(id)?;
    let real_id = l.preset.id.clone();
    let all: Vec<presets::Preset> = listed.into_iter().map(|l| l.preset).collect();
    let linked = linked_files::linked_files(&l.preset, &all);
    let chosen = pick_files(&linked, which);
    for kind in which.iter().filter(|k| **k != FileKind::All) {
        if !linked.iter().any(|f| kind.covers(f.role)) {
            let word = clap::ValueEnum::to_possible_value(kind)
                .map(|v| v.get_name().to_string())
                .unwrap_or_default();
            eprintln!("No {word} file of [{real_id}] is on disk: nothing to delete there.");
        }
    }
    if !even_if_shared {
        if let Some(refusal) = shared_refusal(&chosen) {
            anyhow::bail!(refusal);
        }
    }
    presets::delete(&real_id).context("delete preset")?;
    let file = if l.enabled {
        paths::presets_ini()
    } else {
        paths::presets_disabled_ini()
    };
    println!("Removed [{real_id}] from {}", file.display());
    let mut failed = 0;
    if chosen.is_empty() {
        if !linked.is_empty() {
            let names: Vec<String> = linked.iter().map(|f| f.name()).collect();
            println!(
                "Kept its files: {} (--delete-files removes them).",
                names.join(", ")
            );
        }
    } else {
        let models_dir = server_cfg::load().models_dir_or_default();
        let out = linked_files::delete(&chosen, &models_dir, even_if_shared);
        for (f, size) in &out.removed {
            println!("Deleted {} ({})", f.display(), crate::hf::format_size(*size));
        }
        for d in &out.dirs_removed {
            println!("Removed the empty folder {}", d.display());
        }
        for (f, e) in &out.failed {
            eprintln!("Could not delete {}: {e}", f.display());
        }
        failed = out.failed.len();
    }
    follow_opencode();
    if failed > 0 {
        anyhow::bail!("the preset is deleted, but {failed} file(s) could not be");
    }
    Ok(())
}

/// A `--id` for `preset download`, checked before anything is fetched: a
/// valid section name (`presets::valid_id`) that no preset holds in ANY case
/// (`presets::id_taken`; headers match case-insensitively, so a namesake
/// would be replaced wholesale).
fn check_download_id(id: &str, existing: &[String]) -> Result<()> {
    if id.is_empty() || !presets::valid_id(id) {
        anyhow::bail!("invalid --id `{id}`: use letters, digits, '.', '-', '_'");
    }
    if presets::id_taken(id, existing) {
        anyhow::bail!("a preset [{id}] already exists (ids ignore case): pick another --id");
    }
    Ok(())
}

/// `preset download`: words are a search (`search_report`, then exit);
/// a repository is listed, picked, downloaded (progress on stderr, one line
/// rewritten in place), derive the preset, save it. Ctrl+C ends this process;
/// the child, which has no console of its own (`CREATE_NO_WINDOW`), then reads
/// EOF on its stdin pipe and exits, leaving its partial in the staging tree,
/// so the next run of the same download resumes it.
fn run_preset_download(a: PresetDownload) -> Result<()> {
    use crate::hf;
    let token = hf::token_from_env();
    // The dialog's rule (`hf::classify_input`): a spec lists, words search.
    let input = if a.search {
        hf::Input::Query(hf::parse_query(&a.spec)?)
    } else {
        hf::classify_input(&a.spec)?
            .ok_or_else(|| anyhow::anyhow!("enter a model name to search for, or a repository: org/repo"))?
    };
    let mut spec = match input {
        hf::Input::Spec(spec) => spec,
        hf::Input::Query(query) => {
            let hits = hf::search(&query, a.limit, token.as_ref())?;
            for line in search_report(&query, &hits) {
                println!("{line}");
            }
            return Ok(());
        }
    };
    let existing: Vec<String> = presets::load_listed().into_iter().map(|l| l.preset.id).collect();
    if let Some(id) = &a.id {
        check_download_id(id, &existing)?;
    }
    // `--quant` overrides the spec's own tag or file.
    if let Some(q) = &a.quant {
        if q.ends_with(".gguf") {
            spec.file = Some(q.clone());
        } else {
            spec.tag = Some(q.clone());
            spec.file = None;
        }
    }
    let listing = hf::list_repo(&spec, token.as_ref())?;
    let sidecar = match a.draft.as_deref() {
        Some(d) => Some(hf::Sidecar::parse(d).ok_or_else(|| anyhow::anyhow!("--draft takes mtp, dflash or dspark, not {d}"))?),
        None => None,
    };
    if a.list {
        // The placement estimate per quantization (`hf_fit`): every header
        // read in parallel, against this machine's GPUs and RAM.
        let fit = crate::hf_fit::budget(&crate::devices::list(), crate::hf_fit::system_ram_total());
        let shapes = match &fit {
            Some(_) => {
                eprintln!("Reading the GGUF headers of {} quantizations...", listing.candidates.len());
                let got: std::sync::Mutex<Vec<crate::hf_fit::ShapeResult>> =
                    std::sync::Mutex::new(vec![Err("not read".into()); listing.candidates.len()]);
                crate::hf_fit::fetch_shapes(
                    &hf::Curl,
                    &hf::endpoint(),
                    &listing.repo,
                    &listing.commit,
                    &listing.candidates,
                    token.as_ref(),
                    &|| true,
                    &|i, r| got.lock().unwrap()[i] = r.map_err(|e| e.to_string()),
                );
                got.into_inner().unwrap()
            }
            None => Vec::new(),
        };
        // The extras each row counts: its mmproj unless --no-mmproj, the
        // --draft drafter where the row has one (its header read once per
        // file; unreadable, its size alone).
        let mut drafts: std::collections::HashMap<String, Result<std::sync::Arc<crate::hf_fit::DraftShape>, String>> =
            std::collections::HashMap::new();
        if let (Some(_), Some(kind)) = (&fit, sidecar) {
            for f in listing.candidates.iter().filter_map(|c| c.sidecar(kind)) {
                if !drafts.contains_key(&f.path) {
                    let r = crate::hf_fit::fetch_draft_shape(&hf::Curl, &hf::endpoint(), &listing.repo, &listing.commit, f, token.as_ref());
                    drafts.insert(f.path.clone(), r.map_err(|e| e.to_string()));
                }
            }
        }
        let extras: Vec<crate::hf_fit::Extras> = listing
            .candidates
            .iter()
            .map(|c| crate::hf_fit::extras_of(c, !a.no_mmproj, sidecar, &|f| drafts.get(&f.path).cloned()).unwrap_or_default())
            .collect();
        for line in list_report(&listing, fit.as_ref().map(|b| (b, shapes.as_slice(), extras.as_slice()))) {
            println!("{line}");
        }
        if fit.is_none() {
            println!("Placement estimate unavailable: {}.", crate::hf_fit::NO_GPU);
        }
        return Ok(());
    }
    for line in list_report(&listing, None) {
        println!("{line}");
    }

    let models_dir = std::path::PathBuf::from(server_cfg::load().models_dir_or_default());
    let job = hf::DownloadJob::new(&listing, listing.default, !a.no_mmproj, sidecar, models_dir.clone(), token)?;
    println!("Downloading {} into {}", job.candidate.primary().path, models_dir.display());

    let (tx, rx) = std::sync::mpsc::channel();
    let _handle = hf::spawn_download(
        job,
        |p| {
            eprint!(
                "\r  {} / {}  ({}/{} files)  {:<60}",
                hf::format_size(p.done),
                hf::format_size(p.total),
                p.files_done,
                p.files_total,
                p.file
            );
        },
        move |r| {
            let _ = tx.send(r);
        },
    );
    let done = rx.recv().context("download thread ended without a result")?;
    eprintln!();
    let d = done?;
    println!("Model:  {}", d.model.display());
    if let Some(m) = &d.mmproj {
        println!("mmproj: {}", m.display());
    }
    if let Some((k, p)) = &d.draft {
        println!("{}: {}", k.label(), p.display());
    }

    // Re-read: another process may have added a preset during the transfer.
    let existing: Vec<String> = presets::load_listed().into_iter().map(|l| l.preset.id).collect();
    // The placement the --list rows show, from the downloaded headers on this
    // machine (`hf_fit::placement`); left to the fit without a GPU estimate.
    let budget = crate::hf_fit::budget(&crate::devices::list(), crate::hf_fit::system_ram_total());
    let mut preset = hf::preset_from_download(&d, &existing, budget.as_ref());
    if let Some(id) = a.id {
        check_download_id(&id, &existing)?;
        preset.id = id;
    }
    hf::save_preset(&preset, &models_dir).context("save preset")?;
    println!("Created [{}] in {}", preset.id, paths::presets_ini().display());
    if let Some(line) = placement_line(&preset) {
        println!("{line}");
    }
    follow_opencode();
    Ok(())
}

/// `Placement: device = ROCm1,CUDA0, tensor-split = 26,23, ctx-size = 32768
/// (estimated at download ...)`: the placement keys a derived preset got, and
/// its note (pure); `None` when the download left them to the fit.
fn placement_line(p: &presets::Preset) -> Option<String> {
    if p.placement_note.is_empty() {
        return None;
    }
    let num = |k: &str, v: Option<i32>| v.map(|v| format!("{k} = {v}"));
    let text = |k: &str, v: &str| (!v.is_empty()).then(|| format!("{k} = {v}"));
    let keys: Vec<String> = [
        text("device", &p.device),
        text("tensor-split", &p.tensor_split),
        num("n-cpu-moe", p.n_cpu_moe),
        num("n-gpu-layers", p.n_gpu_layers),
        num("ctx-size", p.ctx_size),
        text("device-draft", &p.device_draft),
        text("mmproj-device", &p.mmproj_device),
        (p.mmproj_offload == Some(false) && !p.mmproj.is_empty()).then(|| "mmproj-offload = false".to_string()),
    ]
    .into_iter()
    .flatten()
    .collect();
    Some(format!("Placement: {} ({})", keys.join(", "), p.placement_note))
}

/// The listing as `preset download` prints it (pure): `repo @ commit`, then
/// one row per quantization (`*` on llama.cpp's default: tag, size, path,
/// sidecars). With `fit` (the `--list` estimate: the machine's budget, each
/// row's shape, or why it is missing, and the extras each row counts, the
/// mmproj and drafter the download would bring; a missing one counts
/// nothing) every row gets its placement line under it, and the report ends
/// with what the estimates assume and the recommended quantization
/// (`hf_fit::recommend`).
fn list_report(
    l: &crate::hf::Listing,
    fit: Option<(&crate::hf_fit::Budget, &[crate::hf_fit::ShapeResult], &[crate::hf_fit::Extras])>,
) -> Vec<String> {
    use crate::{hf, hf_fit};
    let none = hf_fit::Extras::default();
    let estimates: Vec<Option<(hf_fit::Estimate, u64)>> = match fit {
        Some((b, shapes, extras)) => (0..l.candidates.len())
            .map(|i| match shapes.get(i) {
                Some(Ok(s)) => Some((hf_fit::estimate_with(s, b, extras.get(i).unwrap_or(&none)), s.ctx)),
                _ => None,
            })
            .collect(),
        None => Vec::new(),
    };
    let rows: Vec<hf_fit::Row> = l
        .candidates
        .iter()
        .enumerate()
        .map(|(i, c)| hf_fit::Row {
            tag: &c.tag,
            size: c.size,
            estimate: estimates.get(i).and_then(|e| e.as_ref().map(|e| &e.0)),
        })
        .collect();
    let rec = if fit.is_some() { hf_fit::recommend(&rows) } else { None };
    let mut out = vec![format!("{} @ {}", l.repo, &l.commit[..12.min(l.commit.len())])];
    for (i, c) in l.candidates.iter().enumerate() {
        let mark = if i == l.default { "*" } else { " " };
        let mut extra: Vec<String> = Vec::new();
        if c.shards.len() > 1 {
            extra.push(format!("{} shards", c.shards.len()));
        }
        if c.mmproj.is_some() {
            extra.push("mmproj".into());
        }
        for s in hf::Sidecar::ALL {
            if c.sidecar(s).is_some() {
                extra.push(s.label().into());
            }
        }
        let extra = if extra.is_empty() { String::new() } else { format!("  [{}]", extra.join(", ")) };
        out.push(format!("{mark} {:<12} {:>10}  {}{extra}", c.tag, hf::format_size(c.size), c.primary().path));
        if let Some((_, shapes, _)) = fit {
            let label = match (&estimates[i], shapes.get(i)) {
                (Some((e, ctx)), _) => hf_fit::estimate_label(e, *ctx),
                (None, Some(Err(m))) => format!("estimate unavailable: {m}"),
                (None, _) => "estimate unavailable".to_string(),
            };
            let reason = rec.as_ref().filter(|r| r.index == i).map(|r| r.reason);
            out.push(format!("    {}", hf_fit::detail_line(&label, reason)));
        }
    }
    if let Some((b, _, _)) = fit {
        out.push(format!("{}.", hf_fit::basis_label(b)));
        out.push(match &rec {
            Some(r) => format!(
                "Recommended here: {} ({}); download it with --quant {}",
                l.candidates[r.index].tag, r.reason, l.candidates[r.index].tag
            ),
            None => hf_fit::NONE_RECOMMENDED.to_string(),
        });
    }
    out
}

/// What `preset download <words>` prints (pure): a header, one aligned row
/// per repository (`org/repo`, downloads over the last 30 days, the model
/// marker when the search knows it (`MoE 30.5B`, `dense 8.2B`), the quant
/// tags, the sidecars, gated/private), and how to go on.
fn search_report(query: &str, hits: &[crate::hf::SearchHit]) -> Vec<String> {
    use crate::hf;
    if hits.is_empty() {
        return vec![format!("No GGUF repository matches \"{query}\".")];
    }
    let mut out = vec![format!(
        "GGUF repositories for \"{query}\", most downloaded first (downloads over the last 30 days):"
    )];
    let width = hits.iter().map(|h| h.repo.len()).max().unwrap_or(0);
    let kinds: Vec<String> = hits.iter().map(|h| hf::kind_label(h.kind, h.params).unwrap_or_default()).collect();
    let kind_width = kinds.iter().map(String::len).max().unwrap_or(0);
    for (h, kind) in hits.iter().zip(&kinds) {
        let quants = if h.quants.is_empty() { "no GGUF model file".to_string() } else { h.quants.join(" ") };
        let mut extra: Vec<&str> = Vec::new();
        if h.mmproj {
            extra.push("mmproj");
        }
        extra.extend(h.drafters.iter().map(|s| s.label()));
        let extra = if extra.is_empty() { String::new() } else { format!("  [{}]", extra.join(", ")) };
        let access = match (h.gated, h.private) {
            (true, true) => "  gated, private",
            (true, false) => "  gated",
            (false, true) => "  private",
            (false, false) => "",
        };
        let kind = if kind_width == 0 { String::new() } else { format!("{kind:<kind_width$}  ") };
        out.push(format!(
            "  {:<width$}  {:>7}  {kind}{quants}{extra}{access}",
            h.repo,
            hf::format_count(h.downloads)
        ));
    }
    out.push(
        "Pick one: llama-cpp-config preset download <org/repo>[:QUANT] (add --list to see the sizes first)".into(),
    );
    out
}

fn set_preset_enabled(id: &str, enabled: bool) -> Result<()> {
    let l = find_listed(id)?;
    let real_id = l.preset.id;
    let word = if enabled { "on" } else { "off" };
    if l.enabled == enabled {
        println!("[{real_id}] is already {word}.");
        return Ok(());
    }
    presets::set_enabled(&real_id, enabled).context("switch preset")?;
    println!("Switched [{real_id}] {word}.");
    // The router reads presets.ini once, at launch.
    if crate::runstate::is_running() {
        println!(
            "llama-server is running: restart it to apply (`llama-cpp-config control restart`)."
        );
    }
    follow_opencode();
    Ok(())
}

// ── Control commands ────────────────────────────────────────────────────

/// Dump every loaded model's slot before a stop, printing what happened.
///
/// Deliberately infallible: a snapshot that cannot be written must NOT stop the
/// stop. Aborting a shutdown over a cache the user can rebuild by prefilling
/// would trade a recoverable loss for an unusable command. Silent when the
/// feature is off, which is the default.
fn snapshot_before_stop() {
    let cfg = crate::server_cfg::load();
    if !cfg.save_state_on_shutdown_or_default() {
        return;
    }
    println!("Saving the conversation snapshot…");
    let (saved, errors) = crate::slot_state::save_all(&cfg);
    for t in &saved {
        println!("  saved {}", t.describe());
    }
    for e in &errors {
        eprintln!("  snapshot failed: {e}");
    }
}

/// Push the last shutdown's conversation back in after a start. Takes the config
/// `start()` ACTUALLY launched with, so a server that was already running is
/// never handed a stale snapshot over whatever it has been doing since.
///
/// Slow by nature (the proxied restore makes the router load the model, then
/// reads the KV cache off disk) and, like its twin above, never fatal: a server
/// that started is started whether or not the cache came back.
fn snapshot_after_start(cfg: &crate::server_cfg::ServerConfig) {
    if !cfg.save_state_on_shutdown_or_default() {
        return;
    }
    let known: Vec<String> = crate::presets::load_all()
        .into_iter()
        .map(|p| p.id)
        .collect();
    println!("Restoring the conversation snapshot…");
    match crate::slot_state::restore_newest(cfg, &known) {
        Ok(Some(t)) => println!("  restored {}", t.describe()),
        // Nothing on disk: the ordinary first-run state, not worth a line.
        Ok(None) => {}
        Err(e) => eprintln!("  snapshot not restored: {e}"),
    }
}

fn run_control(c: ControlCmd) -> Result<()> {
    use crate::runstate;
    match c {
        ControlCmd::Start => {
            match runstate::start() {
                Ok(Some(cfg)) => {
                    println!("llama-server started.");
                    snapshot_after_start(&cfg);
                }
                Ok(None) => println!("llama-server is already running."),
                Err(e) => anyhow::bail!("Failed to start llama-server: {}", e),
            }
            Ok(())
        }
        ControlCmd::Stop => {
            snapshot_before_stop();
            if !runstate::stop() {
                anyhow::bail!(
                    "Failed to stop llama-server: still running {} s after the stop.",
                    runstate::STOP_GRACE.as_secs()
                );
            }
            println!("llama-server stopped.");
            Ok(())
        }
        ControlCmd::Restart => {
            // The full round trip: the snapshot taken here is the one the start
            // below puts back, which is what makes a restart cheap on a long
            // conversation instead of a fresh multi-minute prefill.
            snapshot_before_stop();
            // `stop()` returns only once the old router and its children are
            // gone (a child that held a large KV cache takes tens of seconds
            // to exit): starting earlier saw the old process, launched nothing,
            // and left no server once it exited.
            if !runstate::stop() {
                anyhow::bail!(
                    "Failed to restart llama-server: the old one is still running {} s after the stop.",
                    runstate::STOP_GRACE.as_secs()
                );
            }
            std::thread::sleep(std::time::Duration::from_millis(1000));
            match runstate::start() {
                Ok(Some(cfg)) => {
                    println!("llama-server restarted.");
                    snapshot_after_start(&cfg);
                }
                Ok(None) => println!("llama-server is already running."),
                Err(e) => anyhow::bail!("Failed to restart llama-server: {}", e),
            }
            Ok(())
        }
        ControlCmd::StopAndClose => {
            snapshot_before_stop();
            runstate::stop();
            std::thread::sleep(std::time::Duration::from_millis(500));
            #[cfg(windows)]
            crate::single_instance::signal_close();
            println!("llama-server stopped and config GUI closed.");
            Ok(())
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::server_cfg::ServerConfig;

    // The only guard on `server set`'s schema mirror: every other server-field
    // spot (server_cfg load/save, the form conversions) has a round-trip test,
    // but `apply` is the CLI-only copy; an omitted field here is silent.

    /// `preset delete` keeps every file unless asked, takes the files as a
    /// comma list, and the shared override only beside a file list.
    #[test]
    fn preset_delete_parses_its_file_flags() {
        let parse = |args: &[&str]| {
            let mut argv = vec!["llama-cpp-config", "preset", "delete", "p"];
            argv.extend_from_slice(args);
            Cli::try_parse_from(argv).map(|c| match c.command {
                Command::Preset(PresetCmd::Delete {
                    delete_files,
                    even_if_shared,
                    ..
                }) => (delete_files, even_if_shared),
                _ => panic!("not a preset delete"),
            })
        };
        assert_eq!(parse(&[]).unwrap(), (vec![], false), "keep is the default");
        assert_eq!(
            parse(&["--delete-files", "model,draft"]).unwrap(),
            (vec![FileKind::Model, FileKind::Draft], false)
        );
        assert_eq!(
            parse(&["--delete-files", "all", "--even-if-shared"]).unwrap(),
            (vec![FileKind::All], true)
        );
        assert!(parse(&["--even-if-shared"]).is_err(), "the override needs a file list");
        assert!(parse(&["--delete-files", "weights"]).is_err(), "an unknown kind");
    }

    #[test]
    fn preset_delete_picks_by_kind_and_refuses_shared_files() {
        use crate::linked_files::{LinkedFile, Role};
        let lf = |role: Role, name: &str, used: &[&str]| LinkedFile {
            role,
            path: name.into(),
            files: vec![name.into()],
            size: 1,
            used_by: used.iter().map(|u| u.to_string()).collect(),
        };
        let linked = vec![
            lf(Role::Model, "m.gguf", &[]),
            lf(Role::Mmproj, "p.gguf", &["vision", "other"]),
            lf(Role::Draft, "d.gguf", &[]),
        ];
        let roles = |which: &[FileKind]| -> Vec<Role> {
            pick_files(&linked, which).iter().map(|f| f.role).collect()
        };
        assert!(roles(&[]).is_empty());
        assert_eq!(roles(&[FileKind::All]), [Role::Model, Role::Mmproj, Role::Draft]);
        assert_eq!(roles(&[FileKind::Draft, FileKind::Model]), [Role::Model, Role::Draft]);
        assert_eq!(shared_refusal(&pick_files(&linked, &[FileKind::Model, FileKind::Draft])), None);
        let refusal = shared_refusal(&pick_files(&linked, &[FileKind::All])).expect("refused");
        assert!(
            refusal.contains("p.gguf is used by [vision], [other]") && refusal.contains("--even-if-shared"),
            "{refusal}"
        );
    }

    /// `preset download` takes the spec positionally and every choice as a
    /// flag, so a script can run it end to end without a prompt.
    #[test]
    fn preset_download_parses_its_flags() {
        let cli = Cli::try_parse_from([
            "llama-cpp-config",
            "preset",
            "download",
            "unsloth/Qwen3-8B-GGUF:Q4_K_M",
            "--no-mmproj",
            "--draft",
            "mtp",
            "--id",
            "q3",
        ])
        .unwrap();
        let Command::Preset(PresetCmd::Download(a)) = cli.command else {
            panic!("not a preset download");
        };
        assert_eq!(a.spec, "unsloth/Qwen3-8B-GGUF:Q4_K_M");
        assert!(a.no_mmproj && !a.list);
        assert_eq!((a.draft.as_deref(), a.id.as_deref(), a.quant.as_deref()), (Some("mtp"), Some("q3"), None));
        assert!(!a.search && a.limit == crate::hf::SEARCH_LIMIT);
        assert!(Cli::try_parse_from(["llama-cpp-config", "preset", "download"]).is_err());
        // Words search; --search forces it, --limit caps the rows.
        let cli = Cli::try_parse_from(["llama-cpp-config", "preset", "download", "unsloth/Qwen3", "--search", "--limit", "5"])
            .unwrap();
        let Command::Preset(PresetCmd::Download(a)) = cli.command else {
            panic!("not a preset download");
        };
        assert!(a.search && a.limit == 5 && a.spec == "unsloth/Qwen3");
    }

    /// The search rows: aligned repo column, downloads, quant tags, sidecars,
    /// access; a hint to go on, or one line when nothing matched.
    #[test]
    fn search_report_prints_repo_downloads_and_quants_per_row() {
        let hit = |repo: &str, downloads: u64, quants: &[&str], mmproj: bool, gated: bool| crate::hf::SearchHit {
            repo: repo.into(),
            downloads,
            gated,
            private: false,
            quants: quants.iter().map(|q| q.to_string()).collect(),
            mmproj,
            drafters: Vec::new(),
            kind: crate::hf::ModelKind::Unknown,
            params: None,
        };
        let lines = search_report(
            "gemma 3",
            &[
                hit("ggml-org/tinygemma3-GGUF", 73_988, &["Q8_0"], true, false),
                hit("google/gemma-3-4b-it-qat-q4_0-gguf", 3_330, &["Q4_0"], true, true),
                hit("o/r", 5, &[], false, false),
            ],
        );
        assert_eq!(lines.len(), 5);
        assert!(lines[0].contains("\"gemma 3\""));
        assert_eq!(lines[1], "  ggml-org/tinygemma3-GGUF              74.0k  Q8_0  [mmproj]");
        assert_eq!(lines[2], "  google/gemma-3-4b-it-qat-q4_0-gguf     3.3k  Q4_0  [mmproj]  gated");
        assert_eq!(lines[3], format!("  {:<34}  {:>7}  no GGUF model file", "o/r", "5"));
        assert!(lines[4].starts_with("Pick one: llama-cpp-config preset download <org/repo>"));
        assert_eq!(search_report("zzz", &[]), ["No GGUF repository matches \"zzz\"."]);

        // With the search's gguf summary: an aligned model column.
        let with_kind = |repo: &str, kind, params| crate::hf::SearchHit {
            kind,
            params,
            ..hit(repo, 10, &["Q4_K_M"], false, false)
        };
        let lines = search_report(
            "qwen3",
            &[
                with_kind("o/Qwen3-30B-A3B-GGUF", crate::hf::ModelKind::Moe, Some(30_532_122_624)),
                with_kind("o/Qwen3-8B-GGUF", crate::hf::ModelKind::Dense, Some(8_190_735_360)),
                with_kind("o/Llama-GGUF", crate::hf::ModelKind::Unknown, Some(8_030_261_312)),
                with_kind("o/x", crate::hf::ModelKind::Unknown, None),
            ],
        );
        assert_eq!(lines[1], "  o/Qwen3-30B-A3B-GGUF       10  MoE 30.5B                Q4_K_M");
        assert_eq!(lines[2], "  o/Qwen3-8B-GGUF            10  dense 8.2B               Q4_K_M");
        assert_eq!(lines[3], "  o/Llama-GGUF               10  8.0B, dense/MoE unknown  Q4_K_M");
        assert_eq!(lines[4], "  o/x                        10                           Q4_K_M");
    }

    /// `--list`: a placement line under every row, the basis, and the
    /// recommendation; without an estimate, the plain rows.
    #[test]
    fn list_report_adds_the_estimate_and_the_recommendation() {
        use crate::hf::{Candidate, Listing, RepoFile};
        use crate::hf_fit::{Budget, Gpu, Layer, Shape};
        use std::sync::Arc;
        const GIB: u64 = 1 << 30;
        let cand = |tag: &str, size: u64| Candidate {
            tag: tag.into(),
            shards: vec![RepoFile { path: format!("m-{tag}.gguf"), size, oid: String::new() }],
            size,
            mmproj: None,
            mtp: None,
            dflash: None,
            dspark: None,
        };
        let l = Listing {
            repo: "o/r".into(),
            commit: "c".repeat(40),
            project_url: String::new(),
            candidates: vec![cand("Q4_K_M", 5 * GIB), cand("Q8_0", 9 * GIB), cand("BF16", 17 * GIB)],
            default: 0,
        };
        let shape = |gib: u64| {
            Arc::new(Shape {
                arch: "qwen3".into(),
                moe: false,
                total: gib * GIB,
                token_embd: 0,
                globals: 0,
                layers: vec![Layer { experts: 0, other: gib * GIB / 4 }; 4],
                kv: GIB,
                ctx: 32768,
            })
        };
        let b = Budget { gpus: vec![Gpu { id: "CUDA0".into(), name: "G".into(), total_mib: 14 << 10 }], ram: Some(28 * GIB), ram_total: Some(32 * GIB) };
        let shapes = vec![Ok(shape(5)), Ok(shape(9)), Err("network error: boom".to_string())];
        let lines = list_report(&l, Some((&b, &shapes, &[])));
        assert_eq!(lines[0], format!("o/r @ {}", "c".repeat(12)));
        assert!(lines[1].starts_with("* Q4_K_M"));
        assert_eq!(lines[2], "    all on GPU: 6.0 of 12.0 GiB with a 32k context");
        assert_eq!(lines[4], "    recommended, the best quality that fits in GPU memory: all on GPU: 10.0 of 12.0 GiB with a 32k context");
        assert_eq!(lines[6], "    estimate unavailable: network error: boom");
        assert!(lines[7].starts_with("Estimated for G (12.0 GiB usable) and 32.0 GiB of RAM"), "{}", lines[7]);
        assert_eq!(lines[8], "Recommended here: Q8_0 (the best quality that fits in GPU memory); download it with --quant Q8_0");
        assert_eq!(lines.len(), 9);
        // A 2 GiB image encoder taken with every row (2.5 GiB with its
        // compute): the Q8_0 no longer fits beside it, so its encoder goes to
        // the CPU and the recommendation falls back to the Q4_K_M, which
        // keeps it on the card.
        let mmproj = hf_fit_extras(2 * GIB);
        let lines = list_report(&l, Some((&b, &shapes, &[mmproj.clone(), mmproj.clone(), mmproj])));
        assert_eq!(
            lines[2],
            "    recommended, the best quality that fits in GPU memory: all on GPU: 8.5 of 12.0 GiB with a 32k context; \
             with the image encoder (2.5 GiB) on CUDA0"
        );
        assert_eq!(lines[4], "    all on GPU: 10.0 of 12.0 GiB with a 32k context; with the image encoder (2.0 GiB) on the CPU");
        assert_eq!(lines[8], "Recommended here: Q4_K_M (the best quality that fits in GPU memory); download it with --quant Q4_K_M");
        // Two cards of 7 and 5 GiB usable: every row names the card or the
        // cut it would write. 6 GiB fits the first alone; the 9 GiB file (four
        // blocks of 2.25 GiB plus 0.25 of KV) fits neither, so it is cut 2 + 3
        // (the output layer on the last card).
        let pair = Budget {
            gpus: vec![
                Gpu { id: "ROCm1".into(), name: "AMD Radeon AI PRO R9700".into(), total_mib: 9 << 10 },
                Gpu { id: "CUDA0".into(), name: "NVIDIA GeForce RTX 4070 SUPER".into(), total_mib: 7 << 10 },
            ],
            ram: Some(28 * GIB),
            ram_total: Some(32 * GIB),
        };
        let lines = list_report(&l, Some((&pair, &shapes, &[])));
        assert_eq!(lines[2], "    all on GPU with a 32k context: AI PRO R9700 (ROCm1) 6.0 GiB");
        assert_eq!(
            lines[4],
            "    recommended, the best quality that fits in GPU memory: all on GPU with a 32k context: \
             AI PRO R9700 (ROCm1) 5.0 GiB + RTX 4070 SUPER (CUDA0) 5.0 GiB, tensor-split 2,3"
        );
        assert!(lines[7].starts_with("Estimated for 2 GPUs in split order, AMD Radeon AI PRO R9700 (ROCm1, 7.0 GiB usable) then "), "{}", lines[7]);

        // What a download prints once the preset is saved.
        let mut p = presets::Preset::new_default("m".into(), "m.gguf".into());
        assert_eq!(placement_line(&p), None, "left to the fit: nothing to say");
        p.device = "ROCm1,CUDA0".into();
        p.tensor_split = "2,3".into();
        p.n_cpu_moe = Some(18);
        p.ctx_size = Some(32768);
        p.placement_note = "estimated at download".into();
        assert_eq!(
            placement_line(&p).unwrap(),
            "Placement: device = ROCm1,CUDA0, tensor-split = 2,3, n-cpu-moe = 18, ctx-size = 32768 (estimated at download)"
        );
        let plain = list_report(&l, None);
        assert_eq!(plain.len(), 4, "no estimate lines without --list");
        // The encoder moved by the placement shows too.
        p.mmproj = "mm.gguf".into();
        p.mmproj_device = "CUDA0".into();
        p.device_draft = "CUDA0".into();
        assert_eq!(
            placement_line(&p).unwrap(),
            "Placement: device = ROCm1,CUDA0, tensor-split = 2,3, n-cpu-moe = 18, ctx-size = 32768, device-draft = CUDA0, \
             mmproj-device = CUDA0 (estimated at download)"
        );
        p.mmproj_device.clear();
        p.mmproj_offload = Some(false);
        assert!(placement_line(&p).unwrap().contains(", device-draft = CUDA0, mmproj-offload = false ("));
    }

    /// The extras of an image encoder of `size` bytes alone.
    fn hf_fit_extras(size: u64) -> crate::hf_fit::Extras {
        crate::hf_fit::Extras { mmproj: size, drafter: None }
    }

    /// A `--id` is refused BEFORE the download when it is not a valid section
    /// name or names an existing preset in any case (headers match
    /// case-insensitively): a multi-GB transfer must not end in a save error.
    #[test]
    fn download_id_is_checked_up_front() {
        let existing = vec!["qwen3-8b".to_string()];
        assert!(check_download_id("q3", &existing).is_ok());
        assert!(check_download_id("Qwen3-8B", &existing).unwrap_err().to_string().contains("already exists"));
        assert!(check_download_id("bad id", &existing).is_err());
        assert!(check_download_id("", &existing).is_err());
    }

    /// `--prompt` and `--prompt-file` are two ways to set one field, so clap
    /// has to refuse both at once rather than letting the later assignment win
    /// silently: a sweep that measured a prompt the caller did not intend is
    /// indistinguishable from one that measured the right one.
    #[test]
    fn prompt_and_prompt_file_are_mutually_exclusive() {
        let base = [
            "llama-cpp-config",
            "bench",
            "sweep",
            "--preset",
            "P",
            "--key",
            "spec-draft-n-max",
            "--values",
            "2,3",
        ];
        assert!(Cli::try_parse_from(base).is_ok());
        let mut with_both = base.to_vec();
        with_both.extend(["--prompt", "hi", "--prompt-file", "p.txt"]);
        assert!(Cli::try_parse_from(with_both).is_err());
        let mut with_file = base.to_vec();
        with_file.extend(["--prompt-file", "p.txt"]);
        assert!(Cli::try_parse_from(with_file).is_ok());
    }

    #[test]
    fn server_set_apply_copies_every_field() {
        let set = ServerSet {
            port: Some(9000),
            hostname: Some("0.0.0.0".into()),
            threads: Some(8),
            threads_batch: Some(16),
            models_max: Some(3),
            models_memory_margin: Some(1024),
            models_dir: Some("D:\\models".into()),
            // The tri-state arrives as its WORD (clap refuses anything else) and
            // must land as the matching Option<bool>.
            rocblas_use_hipblaslt: Some("off".into()),
            cuda_allreduce: Some("none".into()),
            cuda_disable_graphs: Some(true),
            attn_rot_disable: Some(true),
            hip_fa_band: Some("2".into()),
            rocprof_attach: Some(true),
            webui: Some(false),
            webui_mcp_proxy: Some(false),
            log_verbosity: Some(2),
            save_state_on_shutdown: Some(true),
            state_dir: Some(r"E:\llama-state".into()),
            opencode_base_url: Some("https://llm.example.com".into()),
            opencode_api_key: Some("sk-test-key".into()),
        };
        let mut cfg = ServerConfig::default();
        set.apply(&mut cfg);
        // Whole-struct equality against a second exhaustive literal: the
        // compiler forces a value for a new field in BOTH literals, and the
        // equality fails until `apply` actually copies it; an initialized-
        // but-never-copied field can't slip through.
        let expected = ServerConfig {
            port: Some(9000),
            hostname: Some("0.0.0.0".into()),
            threads: Some(8),
            threads_batch: Some(16),
            models_max: Some(3),
            models_memory_margin: Some(1024),
            models_dir: Some("D:\\models".into()),
            rocblas_use_hipblaslt: Some(false),
            cuda_allreduce: Some("none".into()),
            cuda_disable_graphs: true,
            attn_rot_disable: true,
            hip_fa_band: Some(2),
            rocprof_attach: true,
            webui: Some(false),
            webui_mcp_proxy: Some(false),
            log_verbosity: Some(2),
            save_state_on_shutdown: Some(true),
            state_dir: Some(r"E:\llama-state".into()),
            opencode_base_url: Some("https://llm.example.com".into()),
            opencode_api_key: Some("sk-test-key".into()),
        };
        assert_eq!(cfg, expected);
    }

    // The Show leg of the 3-spot CLI fan-out: every field set in a rich config
    // must surface in `server show`'s output. Complements the `apply` test
    // above so neither CLI leg can silently drop a new server field.
    #[test]
    fn show_lines_prints_every_field() {
        let cfg = ServerConfig {
            port: Some(9000),
            hostname: Some("0.0.0.0".into()),
            threads: Some(8),
            threads_batch: Some(16),
            models_max: Some(3),
            models_memory_margin: Some(1024),
            models_dir: Some(r"D:\models".into()),
            rocblas_use_hipblaslt: Some(false),
            cuda_allreduce: Some("none".into()),
            cuda_disable_graphs: true,
            attn_rot_disable: true,
            hip_fa_band: Some(2),
            rocprof_attach: true,
            webui: Some(false),
            webui_mcp_proxy: Some(false),
            log_verbosity: Some(2),
            save_state_on_shutdown: Some(true),
            state_dir: Some(r"E:\llama-state".into()),
            opencode_base_url: Some("https://llm.example.com".into()),
            opencode_api_key: Some("sk-test-key".into()),
        };
        // The exhaustive destructure breaks compilation the moment a field is
        // added, until this test decides what to do with it, but the
        // assertions come from the hand-maintained `needles` array below:
        // bind the new field AND add its needle, or its Show row goes
        // unguarded (binding alone, or `field: _`, compiles fine).
        let ServerConfig {
            port,
            hostname,
            threads,
            threads_batch,
            models_max,
            models_memory_margin,
            models_dir,
            rocblas_use_hipblaslt,
            cuda_allreduce,
            cuda_disable_graphs,
            attn_rot_disable,
            hip_fa_band,
            rocprof_attach,
            webui,
            webui_mcp_proxy,
            log_verbosity,
            save_state_on_shutdown,
            state_dir,
            opencode_base_url,
            opencode_api_key,
        } = cfg.clone();
        let needles = [
            ("Port:", port.unwrap().to_string()),
            ("Hostname:", hostname.unwrap()),
            ("Threads:", threads.unwrap().to_string()),
            ("ThreadsBatch:", threads_batch.unwrap().to_string()),
            ("ModelsMax:", models_max.unwrap().to_string()),
            (
                "ModelsMemoryMargin:",
                format!("{} MiB", models_memory_margin.unwrap()),
            ),
            ("ModelsDir:", models_dir.unwrap()),
            // The row prints the tri-state's WORD, not the bool's `to_string()`:
            // "false" would read as a value the CLI can't even be given.
            (
                "RocblasUseHipblaslt:",
                if rocblas_use_hipblaslt.unwrap() {
                    "on".to_string()
                } else {
                    "off".to_string()
                },
            ),
            ("CudaAllreduce:", cuda_allreduce.unwrap()),
            ("CudaDisableGraphs:", cuda_disable_graphs.to_string()),
            ("AttnRotDisable:", attn_rot_disable.to_string()),
            ("HipFaBand:", hip_fa_band.unwrap().to_string()),
            ("RocprofAttach:", rocprof_attach.to_string()),
            ("Webui:", webui.unwrap().to_string()),
            ("WebuiMcpProxy:", webui_mcp_proxy.unwrap().to_string()),
            ("LogVerbosity:", log_verbosity.unwrap().to_string()),
            (
                "SaveStateOnShutdown:",
                save_state_on_shutdown.unwrap().to_string(),
            ),
            ("StateDir:", state_dir.unwrap()),
            ("OpencodeBaseUrl:", opencode_base_url.unwrap()),
            ("OpencodeApiKey:", opencode_api_key.unwrap()),
        ];
        let out = show_lines(&cfg);
        for (label, value) in needles {
            // Label and value must share a LINE: two separate contains() could
            // both pass off other fields (e.g. ModelsMax "3" matching inside
            // TensorSplit's "3,1") while this field's row prints a placeholder.
            assert!(
                out.lines().any(|l| l.contains(label) && l.contains(&value)),
                "no line pairs {label:?} with {value:?} in:\n{out}"
            );
        }
    }

    // The tri-state's own clearing rule: `--rocblas-use-hipblaslt default` is the
    // ONLY way back to unset from the CLI, and unset is what keeps the env var
    // off the child entirely. A bool flag could not express it.
    #[test]
    fn server_set_rocblas_hipblaslt_takes_all_three_states() {
        for (word, expected) in [("on", Some(true)), ("off", Some(false)), ("default", None)] {
            let mut cfg = ServerConfig {
                rocblas_use_hipblaslt: Some(true),
                ..Default::default()
            };
            ServerSet {
                rocblas_use_hipblaslt: Some(word.into()),
                ..Default::default()
            }
            .apply(&mut cfg);
            assert_eq!(cfg.rocblas_use_hipblaslt, expected, "{word}");
        }
    }

    // `--hip-fa-band default` clears the key back to unset (the tool's default
    // stays in the code); 0 is an explicit off, not a clear.
    #[test]
    fn server_set_hip_fa_band_takes_values_and_default() {
        for (word, expected) in [
            ("0", Some(0)),
            ("2", Some(2)),
            ("4", Some(4)),
            ("default", None),
        ] {
            let mut cfg = ServerConfig {
                hip_fa_band: Some(2),
                ..Default::default()
            };
            ServerSet {
                hip_fa_band: Some(word.into()),
                ..Default::default()
            }
            .apply(&mut cfg);
            assert_eq!(cfg.hip_fa_band, expected, "{word}");
        }
    }

    #[test]
    fn server_set_apply_leaves_unset_flags_untouched() {
        let before = ServerConfig {
            port: Some(1234),
            hostname: Some("localhost".into()),
            threads: Some(6),
            ..Default::default()
        };
        let mut cfg = before.clone();
        ServerSet::default().apply(&mut cfg); // all flags None
        assert_eq!(cfg, before);
    }

    #[test]
    fn server_set_apply_clears_overrides_on_nonpositive_and_blank() {
        let mut cfg = ServerConfig {
            hostname: Some("0.0.0.0".into()),
            threads: Some(8),
            threads_batch: Some(4),
            models_dir: Some(r"D:\models".into()),
            ..Default::default()
        };
        let set = ServerSet {
            hostname: Some("  ".into()),
            threads: Some(0),
            threads_batch: Some(0),
            models_dir: Some(String::new()),
            ..Default::default()
        };
        set.apply(&mut cfg);
        assert_eq!(cfg.hostname, None);
        assert_eq!(cfg.threads, None);
        assert_eq!(cfg.threads_batch, None);
        assert_eq!(cfg.models_dir, None);
    }
}
