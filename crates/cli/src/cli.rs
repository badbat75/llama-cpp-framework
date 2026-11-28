//! The command line of the Private AI Framework (`pai`).

use anyhow::{Context, Result};
use clap::{Args, Parser, Subcommand};

use crate::server_control::{self, ConfigFile};
use crate::{bench, model_profiles, server_cfg, strings};

// ── Command definitions (clap) ───────────────────────────────────────────

#[derive(Parser, Debug)]
#[command(
    name = "pai",
    version,
    about = "Configure the Private AI Framework: llama-server and model profiles."
)]
pub struct Cli {
    #[command(subcommand)]
    pub command: Command,
}

#[derive(Subcommand, Debug)]
pub enum Command {
    /// Server-wide settings (server.ini).
    #[command(subcommand)]
    Server(ServerCmd),
    /// Per-model profiles (models.ini).
    #[command(name = "model-profile", subcommand)]
    ModelProfile(ModelProfileCmd),
    /// Control the llama-server process.
    #[command(subcommand)]
    Control(ControlCmd),
    /// Benchmarks that run unattended (the GUI's Benchmark tab, in a loop).
    #[command(subcommand)]
    Bench(BenchCmd),
    /// The configuration files as a whole.
    #[command(subcommand)]
    Config(ConfigCmd),
}

#[derive(Subcommand, Debug)]
pub enum BenchCmd {
    /// Benchmark one model profile as it is, or once per value of ONE of its keys.
    ///
    /// Measured by the same engine the Benchmark tab uses, each finished leg a
    /// run of one bench.db campaign. Without --key the model profile is measured as
    /// models.ini has it, and the file is not touched. With --key and
    /// --values each value is written to models.ini and the key is put back
    /// afterwards; a live sweep restarts llama-server per value, because the
    /// key rides the child's launch args.
    // Boxed for the reason `ServerSet` is: one payload-carrying variant next to
    // the small ones would otherwise blow clippy's size threshold as the flags
    // grow. clap unwraps the Box itself, so the CLI surface is unchanged.
    Run(Box<BenchRun>),
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
    /// The model profile's device contains TEXT (ROCm0, Vulkan).
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
pub struct BenchRun {
    /// Model profile id to benchmark (as in `model-profile list`).
    #[arg(long)]
    pub model_profile: String,
    /// The models.ini key to vary, e.g. spec-draft-n-max. Omitted (with
    /// --values) = the model profile as it is, one leg.
    #[arg(long, requires = "values")]
    pub key: Option<String>,
    /// Values to measure: `2,3,4`, the inclusive range `2..8`, or, when a value
    /// itself contains a comma (device, tensor-split), `;`-separated:
    /// `Vulkan1,CUDA0;CUDA0`. `unset` removes the key for that leg.
    #[arg(long, requires = "key")]
    pub values: Option<String>,
    /// live = the running llama-server (real prompt, the model profile's drafter);
    /// synthetic = llama-bench (no prompt, no sampler, llama-server must be down).
    #[arg(long, default_value = "live", value_parser = ["live", "synthetic"])]
    pub mode: String,
    /// Repetitions per value.
    #[arg(long)]
    pub reps: Option<i32>,
    /// What the model profile is left on when the sweep ends.
    #[arg(long, default_value = "original", value_parser = ["original", "best", "last"])]
    pub apply: String,
    /// Treat this as the value the key had before the sweep, instead of reading
    /// it from models.ini. For after an interrupted sweep, which leaves the key
    /// on one of its leg values (the value to pass is in its pending marker,
    /// and the sweep warns naming both).
    #[arg(long, requires = "key")]
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
    /// (`{"messages": [...], "tools": [...]}`), sent as given; an `image_file`
    /// part in it is an image, fitted into `--image-size` when sent.
    #[arg(long)]
    pub prompt_file: Option<std::path::PathBuf>,
    /// Live only: sampling temperature, or `preset` to leave the model profile's alone.
    /// Defaults to 0, which is what makes two legs comparable.
    #[arg(long)]
    pub temp: Option<String>,
    /// Live only: output cap per repetition. Omitted (or 0) = no cap: the
    /// request carries no `max_tokens` and the model stops on its own (or at
    /// the model profile's `n-predict`), so the output length is part of the result.
    #[arg(long)]
    pub max_tokens: Option<i32>,
    /// Live only: the box an image prompt's image is fitted into when sent,
    /// `WxH` (aspect kept, never enlarged) or `original`. Default 1344x896,
    /// `standard`'s `ImageSize`. A prompt without an image ignores it.
    #[arg(long)]
    pub image_size: Option<String>,
    /// Live only: an image file sent in place of every image prompt's own
    /// (JPEG or PNG), fitted into `--image-size`. Recorded as `image` and
    /// `image-sha256`; beside `--spec` the runs are derived from it.
    #[arg(long)]
    pub image: Option<std::path::PathBuf>,
    /// Live only: requests per repetition. `single` (default) sends one;
    /// the others send one per slot of the model profile at once (N = `parallel`,
    /// absent = 4; N >= 2), each pinned with `id_slot`: `prefill` (all N
    /// prefill the whole prompt), `decode` (every slot primed with the prompt
    /// first, so all N only decode), `prefill+decode` (N-1 primed slots decode
    /// while the last one prefills). The leg's run label ends in
    /// `concurrency=<scenario>`. `all` = single, then every concurrent
    /// scenario when the model profile has two slots or more, one campaign.
    #[arg(long, value_parser = ["single", "prefill", "decode", "prefill+decode", "all"])]
    pub concurrency: Option<String>,
    /// Live only: run a versioned benchmark spec: `standard` (the framework's
    /// newest published version), `standard-<X.Y>` (that version: every one
    /// stays installed, in share/benchmarks/specs, the repo's
    /// benchmarks/specs for a tree build) or an INI file of your own. Its prompts (pinned by sha256), passes (temperature,
    /// repetitions, concurrency, model profile keys overridden for the pass) and
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
    /// With --spec: run only these of its prompts (file names,
    /// comma-separated), each with the passes that run it. A selection of the
    /// spec's legs, not a change of method: the runs are the spec's, so a new
    /// version can measure just the legs it added.
    #[arg(long, requires = "spec", value_delimiter = ',')]
    pub only: Vec<String>,
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
    /// spelled from the key, the model profile and the values.
    #[arg(long)]
    pub question: Option<String>,
}

#[derive(Subcommand, Debug)]
pub enum ConfigCmd {
    /// Hand your per-user server.ini and model profiles to the manager service
    /// (files it already has are kept).
    Import,
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
    /// Show whether llama-server starts when the system starts, or set it
    /// with on or off.
    Boot {
        #[arg(value_enum)]
        state: Option<OnOff>,
    },
}

/// A switch's two positions on the command line.
#[derive(clap::ValueEnum, Clone, Copy, Debug, PartialEq, Eq)]
pub enum OnOff {
    On,
    Off,
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
    #[arg(long, allow_hyphen_values = true)]
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
    /// ROCm host memory as GTT (envs HSA_USERPTR_FOR_PAGED_MEM=0 and
    /// HSA_SVM_FOR_PAGED_MEM=0): on = export them, which is what a file with no
    /// line does too; off = export neither, so ROCm's own defaults apply;
    /// default = clear the key back to the commented line. A tri-state word like
    /// the flag above, because the GUI and the web UI both write a bool: the CLI
    /// is the only way back to "no line in the file".
    #[arg(long, value_parser = clap::builder::PossibleValuesParser::new(["default", "on", "off"]))]
    pub rocm_host_memory_gtt: Option<String>,
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
    /// Folder for those snapshots (one file per model, sized by the conversation it holds).
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
            cfg.rocblas_use_hipblaslt = crate::model_profiles::tri_bool(hb);
        }
        // The GTT switch, same rule: `default` leaves the key out of the file,
        // which the launch reads as the on state. The GUI and the web UI write a
        // bool, so this is the only way back to no line.
        if let Some(g) = &self.rocm_host_memory_gtt {
            cfg.rocm_host_memory_gtt = crate::model_profiles::tri_bool(g);
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
pub enum ModelProfileCmd {
    /// List model profile ids and the resolved model path for each, switched-off
    /// ones included (marked).
    List,
    /// Dump one model profile as INI.
    Show { id: String },
    /// Delete a model profile section; with --delete-files, the model files it links
    /// too. Without that flag every file stays on disk.
    Delete {
        id: String,
        /// Also delete these files the model profile links, when they are on disk:
        /// `all`, or a comma-separated list of `model` (every shard of a split
        /// model), `mmproj` and `draft`. The folders a Hugging Face download
        /// leaves empty under ModelsDir's models / mmprojs / mtps / dflashs go
        /// too; nothing outside them, and never a folder with content. A file
        /// another model profile references (in models.ini or models.ini.disabled)
        /// is refused, before anything is deleted, unless --even-if-shared.
        #[arg(long, value_name = "WHICH", value_delimiter = ',')]
        delete_files: Vec<FileKind>,
        /// Delete a picked file even when another model profile references it (that
        /// profile then points at a missing file).
        #[arg(long, requires = "delete_files")]
        even_if_shared: bool,
    },
    /// Switch a model profile on: move it back into models.ini, so llama-server
    /// offers it again (after a restart).
    Enable { id: String },
    /// Switch a model profile off: park it in models.ini.disabled, unchanged, where
    /// llama-server no longer sees it.
    Disable { id: String },
    /// Download a GGUF from Hugging Face with llama.cpp's own downloader and
    /// create a model profile for it. Lists the repo's quantizations first; the files
    /// land in ModelsDir's models / mmprojs / mtps / dflashs subfolders.
    ///
    /// Given words instead of a repository (`"qwen3 8b"`), it SEARCHES: one
    /// row per GGUF repository, most downloaded first, with its downloads and
    /// the quantizations it offers, and exits; run it again with the chosen
    /// `org/repo`.
    Download(Box<ModelProfileDownload>),
}

/// Which linked files `model-profile delete --delete-files` takes (`linked_files::Role`).
#[derive(clap::ValueEnum, Clone, Copy, Debug, PartialEq, Eq)]
pub enum FileKind {
    /// Every file the model profile links.
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

/// The refusal `model-profile delete` stops on when a picked file is in use, naming
/// each such file and the model profiles holding it; `None` when nothing is shared.
fn shared_refusal(chosen: &[crate::linked_files::LinkedFile]) -> Option<String> {
    let shared: Vec<String> = chosen
        .iter()
        .filter(|f| f.in_use())
        .map(|f| {
            let ids: Vec<String> = f.used_by.iter().map(|id| format!("[{id}]")).collect();
            strings::fill("file-used-by", &[&f.path.display(), &ids.join(", ")])
        })
        .collect();
    (!shared.is_empty()).then(|| strings::fill("nothing-deleted-shared", &[&shared.join("; ")]))
}

#[derive(Args, Debug)]
pub struct ModelProfileDownload {
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
    /// The model profile id. Default: derived from the model's file name.
    #[arg(long)]
    pub id: Option<String>,
}

// ── Dispatch & rendering ─────────────────────────────────────────────────

pub fn run(cli: Cli) -> Result<()> {
    match cli.command {
        Command::Server(c) => run_server(c),
        Command::ModelProfile(c) => run_model_profile(c),
        Command::Control(c) => run_control(c),
        Command::Bench(c) => run_bench(c),
        Command::Config(c) => run_config(c),
    }
}

/// The `bench run` leg: turn the flags into a `sweep::Opts` and run it. Every
/// flag left out falls back to the Benchmark tab's own default, so a sweep and a
/// run started from the GUI measure the same thing.
fn run_bench(c: BenchCmd) -> Result<()> {
    let a = match c {
        BenchCmd::Run(a) => a,
        BenchCmd::Query(q) => return run_bench_query(*q),
        BenchCmd::Answer { campaign, text, db } => {
            let path = db.unwrap_or_else(bench::db::work_path);
            bench::query::answer(&path, &campaign, &text).map_err(|e| anyhow::anyhow!(e))?;
            println!(
                "{}",
                strings::fill("answer-recorded", &[&campaign, &path.display()])
            );
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
                println!(
                    "{}",
                    strings::fill("runs-copied", &[&name, &runs, &to.display()])
                );
            }
            return Ok(());
        }
    };
    let values = match a.values.as_deref() {
        Some(v) => bench::sweep::parse_values(v).map_err(|e| anyhow::anyhow!(e))?,
        None => Vec::new(),
    };

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
                    .with_context(|| strings::fill("temp-not-a-number", &[&temp]))?,
            )
        };
    }
    // No flag = no cap (unlike the tab, whose field always holds a number):
    // how long the model talks under a setting is part of what a sweep shows.
    plan.max_tokens = a.max_tokens.unwrap_or(0);
    if let Some(v) = a.image_size.as_deref() {
        plan.image_size =
            bench::parse_image_size(v).map_err(|e| anyhow::anyhow!("--image-size: {e}"))?;
    }
    if let Some(img) = a.image.as_deref() {
        // Refused now, not after the first restart: an unreadable image.
        std::fs::read(img)
            .map_err(|e| anyhow::anyhow!("--image {}: {e}", img.display()))
            .and_then(|b| {
                image::guess_format(&b)
                    .map_err(|e| anyhow::anyhow!("--image {}: {e}", img.display()))
            })?;
        plan.image = Some(img.to_path_buf());
    }
    let concurrency_all = a.concurrency.as_deref() == Some("all");
    if let Some(c) = a.concurrency.as_deref().filter(|c| *c != "all") {
        plan.concurrency = bench::Concurrency::from_str(c)
            .ok_or_else(|| anyhow::anyhow!(strings::fill("concurrency-unknown", &[&c])))?;
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
        image_size: a.image_size.is_some().then_some(plan.image_size),
        image: a.image.is_some().then(|| plan.image.clone()),
        reps: a.reps,
        concurrency: a
            .concurrency
            .as_deref()
            .map(|_| (!concurrency_all).then_some(plan.concurrency)),
    };
    let opts = bench::sweep::Opts {
        model_profile: a.model_profile,
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
    bench::interrupt::install().map_err(|e| anyhow::anyhow!(e))?;
    // For the whole sweep, every pass and scenario included: another
    // benchmark on the same cards would be measured as this one's.
    let _gpu = bench::gpu_lock::acquire(|m| println!("{m}")).map_err(|e| anyhow::anyhow!(e))?;
    if let Some(spec) = &spec {
        bench::spec::run(spec, &opts, &derive, &a.only).map_err(|e| anyhow::anyhow!(e))?;
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
    for c in bench::sweep::all_scenarios(&opts.model_profile).map_err(|e| anyhow::anyhow!(e))? {
        let o = bench::sweep::Opts {
            plan: bench::Plan {
                concurrency: c,
                ..opts.plan.clone()
            },
            campaign: campaign.clone(),
            ..opts.clone()
        };
        if let Err(e) = bench::sweep::run(&o) {
            problems.push(strings::fill("concurrency-failed", &[&c.as_str(), &e]));
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
///
/// For the backend `kind` on `os` (pure): an unset ModelsDir or
/// StateDir prints the folder the instance really uses with "(default)":
/// under the manager on Linux and Windows the instance's defaults
/// `instance_models` and `instance_state` (`paths::instance_models_dir`,
/// `paths::instance_state_dir` with the instance's id joined), not the
/// user's own. A key the manager withheld (`api_key_withheld`) prints as
/// "(set, withheld)".
fn show_lines_for(
    cfg: &server_cfg::ServerConfig,
    kind: server_control::Kind,
    os: server_control::Os,
    instance_models: &std::path::Path,
    instance_state: &std::path::Path,
    api_key_withheld: bool,
) -> String {
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
    let configured = |v: &Option<String>| v.as_deref().is_some_and(|s| !s.trim().is_empty());
    row(
        "ModelsDir:",
        if server_control::dirs_left_unset(kind, os) && !configured(&cfg.models_dir) {
            format!("{} (default)", instance_models.display())
        } else {
            cfg.models_dir.clone().unwrap_or_else(|| "-".into())
        },
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
        "RocmHostMemoryGtt:",
        cfg.rocm_host_memory_gtt.map_or_else(
            || "on (default, GTT)".into(),
            |v| {
                if v {
                    "on (GTT)".into()
                } else {
                    "off (the runtime's defaults)".to_string()
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
    row("RocprofAttach:", cfg.rocprof_attach.to_string());
    row(
        "Webui:",
        cfg.webui
            .map_or_else(|| "true (default)".into(), |v| v.to_string()),
    );
    row(
        "WebuiMcpProxy:",
        cfg.webui_mcp_proxy
            .map_or_else(|| "false (default)".into(), |v| v.to_string()),
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
        if configured(&cfg.state_dir) {
            cfg.state_dir.clone().unwrap_or_default()
        } else {
            let dir = server_cfg::effective_state_dir(cfg, kind, os, instance_state);
            format!("{} (default)", dir.display())
        },
    );
    row(
        "OpencodeBaseUrl:",
        cfg.opencode_base_url
            .clone()
            .unwrap_or_else(|| "auto (host:port, /v1 appended)".into()),
    );
    row(
        "OpencodeApiKey:",
        cfg.opencode_api_key.clone().unwrap_or_else(|| {
            if api_key_withheld {
                "(set, withheld)".into()
            } else {
                "(none)".into()
            }
        }),
    );
    // The profile's tags: metadata for the web UI, never a llama-server flag.
    // The value is the file's own spelling, the `a,b` of `Tags = a,b`, so what
    // is read back is what the profile carries.
    row(
        "Tags:",
        if cfg.tags.is_empty() {
            "-".into()
        } else {
            cfg.tags.join(",")
        },
    );
    out
}

/// Where a configuration file is, to show the user: the seam's answer.
fn config_path(file: ConfigFile) -> std::path::PathBuf {
    server_control::backend().config_path(file)
}

fn run_server(c: ServerCmd) -> Result<()> {
    match c {
        ServerCmd::Show => {
            let cfg = server_cfg::load();
            println!("server.ini: {}", config_path(ConfigFile::Server).display());
            // The StateDir shown as the default is the folder of the instance
            // this command drives: the engine's state root joined with its id,
            // as the launch passes it.
            let instance_state = crate::paths::instance_state_dir().join("default");
            print!(
                "{}",
                show_lines_for(
                    &cfg,
                    server_control::backend().kind(),
                    server_control::Os::current(),
                    &crate::paths::instance_models_dir(),
                    &instance_state,
                    server_control::backend().api_key_withheld(),
                )
            );
            Ok(())
        }
        ServerCmd::Set(s) => {
            let mut cfg = server_cfg::load();
            s.apply(&mut cfg);
            server_cfg::save(&cfg).with_context(|| strings::t("save-server-settings-failed"))?;
            println!(
                "{}",
                strings::fill("wrote-file", &[&config_path(ConfigFile::Server).display()])
            );
            Ok(())
        }
        ServerCmd::Boot { state } => {
            println!(
                "{}",
                run_server_boot_with(state, server_control::backend())?
            );
            Ok(())
        }
    }
}

/// `server boot [on|off]` on `server`: the line to print, the start at boot
/// as it stands once set.
fn run_server_boot_with(
    state: Option<OnOff>,
    server: &crate::server_control::Backend,
) -> Result<String> {
    let on = match state {
        None => server.boot_enabled().map_err(anyhow::Error::msg)?,
        Some(state) => {
            let on = state == OnOff::On;
            server.set_boot_enabled(on).map_err(anyhow::Error::msg)?;
            on
        }
    };
    Ok(if on {
        strings::t("server-boot-on")
    } else {
        strings::t("server-boot-off")
    })
}

/// Look a model profile up in BOTH files, case-insensitively like the whole INI section
/// layer (read_section, rename_section, delete_section all use
/// eq_ignore_ascii_case). The writers below act on the STORED id so the header
/// is hit whatever case the user typed.
fn find_listed(id: &str) -> Result<model_profiles::Listed> {
    model_profiles::load_listed()
        .into_iter()
        .find(|l| l.model_profile.id.eq_ignore_ascii_case(id))
        .ok_or_else(|| anyhow::anyhow!(strings::fill("model-profile-not-listed", &[&id])))
}

fn run_model_profile(c: ModelProfileCmd) -> Result<()> {
    match c {
        ModelProfileCmd::List => {
            let listed = model_profiles::load_listed();
            println!("models.ini: {}", config_path(ConfigFile::Models).display());
            if listed.iter().any(|l| !l.enabled) {
                println!(
                    "{}",
                    strings::fill(
                        "switched-off-file",
                        &[&config_path(ConfigFile::ModelsDisabled).display()]
                    )
                );
            }
            if listed.is_empty() {
                println!("{}", strings::t("no-model-profiles-defined"));
            }
            for l in listed {
                let off = if l.enabled {
                    String::new()
                } else {
                    strings::t("model-profile-off-mark")
                };
                println!(
                    "  [{}]{off}  model={}",
                    l.model_profile.id, l.model_profile.model
                );
            }
            Ok(())
        }
        ModelProfileCmd::Show { id } => {
            let l = find_listed(&id)?;
            if !l.enabled {
                println!("{}", strings::t("model-profile-parked"));
            }
            println!("{}", model_profiles::render_section(&l.model_profile));
            Ok(())
        }
        ModelProfileCmd::Delete {
            id,
            delete_files,
            even_if_shared,
        } => run_model_profile_delete(&id, &delete_files, even_if_shared),
        ModelProfileCmd::Enable { id } => set_model_profile_enabled(&id, true),
        ModelProfileCmd::Disable { id } => set_model_profile_enabled(&id, false),
        ModelProfileCmd::Download(a) => run_model_profile_download(*a),
    }
}

/// `model-profile delete`: the section, then the linked files `which` names (none by
/// default). Everything that can refuse is checked BEFORE the section goes: an
/// unknown id, and a picked file another model profile references (`shared_refusal`,
/// lifted by `even_if_shared`). A file that then fails to delete does not undo
/// the rest; it is reported and the command exits with an error.
fn run_model_profile_delete(id: &str, which: &[FileKind], even_if_shared: bool) -> Result<()> {
    use crate::linked_files;
    // ini::delete_section is a documented no-op for a missing section,
    // so look the id up first (mirroring Show), or a typo'd id gets a
    // "Removed" message for a model profile that never existed.
    let listed = model_profiles::load_listed();
    let l = find_listed(id)?;
    let real_id = l.model_profile.id.clone();
    let all: Vec<model_profiles::ModelProfile> =
        listed.into_iter().map(|l| l.model_profile).collect();
    let linked = linked_files::linked_files(&l.model_profile, &all);
    let chosen = pick_files(&linked, which);
    for kind in which.iter().filter(|k| **k != FileKind::All) {
        if !linked.iter().any(|f| kind.covers(f.role)) {
            let word = clap::ValueEnum::to_possible_value(kind)
                .map(|v| v.get_name().to_string())
                .unwrap_or_default();
            eprintln!("{}", strings::fill("no-file-of-kind", &[&word, &real_id]));
        }
    }
    if !even_if_shared {
        if let Some(refusal) = shared_refusal(&chosen) {
            anyhow::bail!(refusal);
        }
    }
    model_profiles::delete(&real_id).with_context(|| strings::t("delete-model-profile-failed"))?;
    let file = if l.enabled {
        config_path(ConfigFile::Models)
    } else {
        config_path(ConfigFile::ModelsDisabled)
    };
    println!(
        "{}",
        strings::fill("removed-model-profile-from", &[&real_id, &file.display()])
    );
    let mut failed = 0;
    if chosen.is_empty() {
        if !linked.is_empty() {
            let names: Vec<String> = linked.iter().map(|f| f.name()).collect();
            println!("{}", strings::fill("kept-files", &[&names.join(", ")]));
        }
    } else {
        let models_dir = server_cfg::models_dir();
        let out = linked_files::delete(&chosen, &models_dir, even_if_shared);
        for (f, size) in &out.removed {
            println!(
                "{}",
                strings::fill(
                    "deleted-file",
                    &[&f.display(), &crate::hf::format_size(*size)]
                )
            );
        }
        for d in &out.dirs_removed {
            println!("{}", strings::fill("removed-empty-folder", &[&d.display()]));
        }
        for (f, e) in &out.failed {
            eprintln!("{}", strings::fill("file-not-deleted", &[&f.display(), &e]));
        }
        failed = out.failed.len();
    }
    if failed > 0 {
        anyhow::bail!(strings::fill("model-profile-gone-files-not", &[&failed]));
    }
    Ok(())
}

/// A `--id` for `model-profile download`, checked before anything is fetched: a
/// valid section name (`model_profiles::valid_id`) that no model profile holds in ANY case
/// (`model_profiles::id_taken`; headers match case-insensitively, so a namesake
/// would be replaced wholesale).
fn check_download_id(id: &str, existing: &[String]) -> Result<()> {
    if id.is_empty() || !model_profiles::valid_id(id) {
        anyhow::bail!(strings::fill("id-flag-invalid", &[&id]));
    }
    if model_profiles::id_taken(id, existing) {
        anyhow::bail!(strings::fill("id-flag-taken", &[&id]));
    }
    Ok(())
}

/// `model-profile download`: words are a search (`search_report`, then exit);
/// a repository is listed, picked, downloaded (progress on stderr, one line
/// rewritten in place), derive the model profile, save it. Ctrl+C ends this process;
/// the child, which has no console of its own (`CREATE_NO_WINDOW`), then reads
/// EOF on its stdin pipe and exits, leaving its partial in the staging tree,
/// so the next run of the same download resumes it.
fn run_model_profile_download(a: ModelProfileDownload) -> Result<()> {
    use crate::hf;
    let token = hf::token_from_env();
    // The dialog's rule (`hf::classify_input`): a spec lists, words search.
    let input = if a.search {
        hf::Input::Query(hf::parse_query(&a.spec)?)
    } else {
        hf::classify_input(&a.spec)?.ok_or_else(|| anyhow::anyhow!(strings::t("enter-a-search")))?
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
    let existing: Vec<String> = model_profiles::load_listed()
        .into_iter()
        .map(|l| l.model_profile.id)
        .collect();
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
        Some(d) => Some(
            hf::Sidecar::parse(d)
                .ok_or_else(|| anyhow::anyhow!(strings::fill("draft-flag-invalid", &[&d])))?,
        ),
        None => None,
    };
    if a.list {
        // The placement estimate per quantization (`hf_fit`): every header
        // read in parallel, against this machine's GPUs and RAM.
        let fit = crate::hf_fit::budget(&crate::devices::list(), crate::hf_fit::system_ram_total());
        let shapes = match &fit {
            Some(_) => {
                eprintln!(
                    "{}",
                    strings::fill("reading-gguf-headers", &[&listing.candidates.len()])
                );
                let got: std::sync::Mutex<Vec<crate::hf_fit::ShapeResult>> =
                    std::sync::Mutex::new(vec![
                        Err(strings::t("shape-not-read"));
                        listing.candidates.len()
                    ]);
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
        let mut drafts: std::collections::HashMap<
            String,
            Result<std::sync::Arc<crate::hf_fit::DraftShape>, String>,
        > = std::collections::HashMap::new();
        if let (Some(_), Some(kind)) = (&fit, sidecar) {
            for f in listing.candidates.iter().filter_map(|c| c.sidecar(kind)) {
                if !drafts.contains_key(&f.path) {
                    let r = crate::hf_fit::fetch_draft_shape(
                        &hf::Curl,
                        &hf::endpoint(),
                        &listing.repo,
                        &listing.commit,
                        f,
                        token.as_ref(),
                    );
                    drafts.insert(f.path.clone(), r.map_err(|e| e.to_string()));
                }
            }
        }
        let extras: Vec<crate::hf_fit::Extras> = listing
            .candidates
            .iter()
            .map(|c| {
                crate::hf_fit::extras_of(c, !a.no_mmproj, sidecar, &|f| {
                    drafts.get(&f.path).cloned()
                })
                .unwrap_or_default()
            })
            .collect();
        for line in list_report(
            &listing,
            fit.as_ref()
                .map(|b| (b, shapes.as_slice(), extras.as_slice())),
        ) {
            println!("{line}");
        }
        if fit.is_none() {
            println!(
                "{}",
                strings::fill(
                    "placement-estimate-unavailable",
                    &[&crate::hf_fit::no_gpu()]
                )
            );
        }
        return Ok(());
    }
    for line in list_report(&listing, None) {
        println!("{line}");
    }

    let models_dir = std::path::PathBuf::from(server_cfg::models_dir());
    let job = hf::DownloadJob::new(
        &listing,
        listing.default,
        !a.no_mmproj,
        sidecar,
        models_dir.clone(),
        token,
    )?;
    println!(
        "{}",
        strings::fill(
            "downloading-into",
            &[&job.candidate.primary().path, &models_dir.display()]
        )
    );

    let (tx, rx) = std::sync::mpsc::channel();
    let _handle = hf::spawn_download(
        job,
        |p| {
            eprint!(
                "{}",
                strings::fill(
                    "download-progress",
                    &[
                        &hf::format_size(p.done),
                        &hf::format_size(p.total),
                        &p.files_done,
                        &p.files_total,
                        &format!("{:<60}", p.file),
                    ],
                )
            );
        },
        move |r| {
            let _ = tx.send(r);
        },
    );
    let done = rx
        .recv()
        .with_context(|| strings::t("download-thread-ended"))?;
    eprintln!();
    let d = done?;
    println!(
        "{}",
        strings::fill("downloaded-model", &[&d.model.display()])
    );
    if let Some(m) = &d.mmproj {
        println!("mmproj: {}", m.display());
    }
    if let Some((k, p)) = &d.draft {
        println!("{}: {}", k.label(), p.display());
    }

    // Re-read: another process may have added a model profile during the transfer.
    let existing: Vec<String> = model_profiles::load_listed()
        .into_iter()
        .map(|l| l.model_profile.id)
        .collect();
    // The placement the --list rows show, from the downloaded headers on this
    // machine (`hf_fit::placement`); left to the fit without a GPU estimate.
    let budget = crate::hf_fit::budget(&crate::devices::list(), crate::hf_fit::system_ram_total());
    let mut model_profile = hf::model_profile_from_download(&d, &existing, budget.as_ref());
    if let Some(id) = a.id {
        check_download_id(&id, &existing)?;
        model_profile.id = id;
    }
    hf::save_model_profile(&model_profile, &models_dir)
        .with_context(|| strings::t("save-model-profile-failed"))?;
    println!(
        "{}",
        strings::fill(
            "created-model-profile-in",
            &[
                &model_profile.id,
                &config_path(ConfigFile::Models).display()
            ]
        )
    );
    if let Some(line) = placement_line(&model_profile) {
        println!("{line}");
    }
    Ok(())
}

/// `Placement: device = ROCm1,CUDA0, tensor-split = 26,23, ctx-size = 32768
/// (estimated at download ...)`: the placement keys a derived model profile got, and
/// its note (pure); `None` when the download left them to the fit.
fn placement_line(p: &model_profiles::ModelProfile) -> Option<String> {
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
        (p.mmproj_offload == Some(false) && !p.mmproj.is_empty())
            .then(|| "mmproj-offload = false".to_string()),
    ]
    .into_iter()
    .flatten()
    .collect();
    Some(strings::fill(
        "placement-line",
        &[&keys.join(", "), &p.placement_note],
    ))
}

/// The listing as `model-profile download` prints it (pure): `repo @ commit`, then
/// one row per quantization (`*` on llama.cpp's default: tag, size, path,
/// sidecars). With `fit` (the `--list` estimate: the machine's budget, each
/// row's shape, or why it is missing, and the extras each row counts, the
/// mmproj and drafter the download would bring; a missing one counts
/// nothing) every row gets its placement line under it, and the report ends
/// with what the estimates assume and the recommended quantization
/// (`hf_fit::recommend`).
fn list_report(
    l: &crate::hf::Listing,
    fit: Option<(
        &crate::hf_fit::Budget,
        &[crate::hf_fit::ShapeResult],
        &[crate::hf_fit::Extras],
    )>,
) -> Vec<String> {
    use crate::{hf, hf_fit};
    let none = hf_fit::Extras::default();
    let estimates: Vec<Option<(hf_fit::Estimate, u64)>> = match fit {
        Some((b, shapes, extras)) => (0..l.candidates.len())
            .map(|i| match shapes.get(i) {
                Some(Ok(s)) => Some((
                    hf_fit::estimate_with(s, b, extras.get(i).unwrap_or(&none)),
                    s.ctx,
                )),
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
    let rec = if fit.is_some() {
        hf_fit::recommend(&rows)
    } else {
        None
    };
    let mut out = vec![format!(
        "{} @ {}",
        l.repo,
        &l.commit[..12.min(l.commit.len())]
    )];
    for (i, c) in l.candidates.iter().enumerate() {
        let mark = if i == l.default { "*" } else { " " };
        let mut extra: Vec<String> = Vec::new();
        if c.shards.len() > 1 {
            extra.push(strings::fill("shards", &[&c.shards.len()]));
        }
        if c.mmproj.is_some() {
            extra.push("mmproj".into());
        }
        for s in hf::Sidecar::ALL {
            if c.sidecar(s).is_some() {
                extra.push(s.label().into());
            }
        }
        let extra = if extra.is_empty() {
            String::new()
        } else {
            format!("  [{}]", extra.join(", "))
        };
        out.push(format!(
            "{mark} {:<12} {:>10}  {}{extra}",
            c.tag,
            hf::format_size(c.size),
            c.primary().path
        ));
        if let Some((_, shapes, _)) = fit {
            let label = match (&estimates[i], shapes.get(i)) {
                (Some((e, ctx)), _) => hf_fit::estimate_label(e, *ctx),
                (None, Some(Err(m))) => strings::fill("estimate-unavailable", &[&m]),
                (None, _) => strings::t("estimate-missing"),
            };
            let reason = rec
                .as_ref()
                .filter(|r| r.index == i)
                .map(|r| r.reason.as_str());
            out.push(format!("    {}", hf_fit::detail_line(&label, reason)));
        }
    }
    if let Some((b, _, _)) = fit {
        out.push(format!("{}.", hf_fit::basis_label(b)));
        out.push(match &rec {
            Some(r) => strings::fill("recommended-here", &[&l.candidates[r.index].tag, &r.reason]),
            None => hf_fit::none_recommended(),
        });
    }
    out
}

/// What `model-profile download <words>` prints (pure): a header, one aligned row
/// per repository (`org/repo`, downloads over the last 30 days, the model
/// marker when the search knows it (`MoE 30.5B`, `dense 8.2B`), the quant
/// tags, the sidecars, gated/private), and how to go on.
fn search_report(query: &str, hits: &[crate::hf::SearchHit]) -> Vec<String> {
    use crate::hf;
    if hits.is_empty() {
        return vec![strings::fill("results-none", &[&query])];
    }
    let mut out = vec![strings::fill("gguf-repositories-heading", &[&query])];
    let width = hits.iter().map(|h| h.repo.len()).max().unwrap_or(0);
    let kinds: Vec<String> = hits
        .iter()
        .map(|h| hf::kind_label(h.kind, h.params).unwrap_or_default())
        .collect();
    let kind_width = kinds.iter().map(String::len).max().unwrap_or(0);
    for (h, kind) in hits.iter().zip(&kinds) {
        let quants = if h.quants.is_empty() {
            strings::t("no-gguf-model-file")
        } else {
            h.quants.join(" ")
        };
        let mut extra: Vec<&str> = Vec::new();
        if h.mmproj {
            extra.push("mmproj");
        }
        extra.extend(h.drafters.iter().map(|s| s.label()));
        let extra = if extra.is_empty() {
            String::new()
        } else {
            format!("  [{}]", extra.join(", "))
        };
        let access = match (h.gated, h.private) {
            (true, true) => strings::t("access-gated-private"),
            (true, false) => strings::t("access-gated"),
            (false, true) => strings::t("access-private"),
            (false, false) => String::new(),
        };
        let kind = if kind_width == 0 {
            String::new()
        } else {
            format!("{kind:<kind_width$}  ")
        };
        out.push(format!(
            "  {:<width$}  {:>7}  {kind}{quants}{extra}{access}",
            h.repo,
            hf::format_count(h.downloads)
        ));
    }
    out.push(strings::t("pick-a-repository"));
    out
}

fn set_model_profile_enabled(id: &str, enabled: bool) -> Result<()> {
    let l = find_listed(id)?;
    let real_id = l.model_profile.id;
    if l.enabled == enabled {
        let already = if enabled {
            strings::fill("model-profile-already-on", &[&real_id])
        } else {
            strings::fill("model-profile-already-off", &[&real_id])
        };
        println!("{already}");
        return Ok(());
    }
    model_profiles::set_enabled(&real_id, enabled)
        .with_context(|| strings::t("switch-model-profile-failed"))?;
    let switched = if enabled {
        strings::fill("model-profile-switched-on", &[&real_id])
    } else {
        strings::fill("model-profile-switched-off", &[&real_id])
    };
    println!("{switched}");
    // The router reads models.ini once, at launch.
    if crate::server_control::backend().is_running() {
        println!("{}", strings::t("restart-to-apply"));
    }
    Ok(())
}

// ── Control commands ────────────────────────────────────────────────────

/// Dump every loaded model's slot before a stop, printing what happened. Only
/// the client that kills llama-server saves: under the manager the instance
/// service saves the slot snapshots at its own stop, so a stop from the
/// command line asks for none of its own.
///
/// Deliberately infallible: a snapshot that cannot be written must NOT stop the
/// stop. Aborting a shutdown over a cache the user can rebuild by prefilling
/// would trade a recoverable loss for an unusable command. Silent when the
/// feature is off, which is the default.
fn snapshot_before_stop(server: &crate::server_control::Backend) {
    if !crate::server_control::client_saves_snapshots(server) {
        return;
    }
    let cfg = crate::server_cfg::load();
    if !cfg.save_state_on_shutdown_or_default() {
        return;
    }
    println!("{}", strings::t("snapshot-saving"));
    let (saved, errors) = crate::slot_state::save_all(&cfg);
    for t in &saved {
        println!("{}", strings::fill("snapshot-saved-row", &[&t.describe()]));
    }
    for e in &errors {
        eprintln!("{}", strings::fill("snapshot-failed-row", &[&e]));
    }
}

/// Push the last shutdown's conversation back in after a start. Takes the config
/// `start()` ACTUALLY launched with, so a server that was already running is
/// never handed a stale snapshot over whatever it has been doing since.
///
/// Slow by nature (the proxied restore makes the router load the model, then
/// reads the KV cache off disk) and, like its twin above, never fatal: a server
/// that started is started whether or not the cache came back.
fn snapshot_after_start(
    cfg: &crate::server_cfg::ServerConfig,
    server: &crate::server_control::Backend,
) {
    if !cfg.save_state_on_shutdown_or_default() {
        return;
    }
    let known: Vec<String> = crate::model_profiles::load_all()
        .into_iter()
        .map(|p| p.id)
        .collect();
    println!("{}", strings::t("snapshot-restoring"));
    match crate::slot_state::restore_newest_with(cfg, &known, || server.is_running()) {
        Ok(Some(t)) => println!("{}", strings::fill("snapshot-restored", &[&t.describe()])),
        // Nothing on disk: the ordinary first-run state, not worth a line.
        Ok(None) => {}
        Err(e) => eprintln!("{}", strings::fill("snapshot-not-restored", &[&e])),
    }
}

fn run_config(c: ConfigCmd) -> Result<()> {
    match c {
        ConfigCmd::Import => {
            let b = crate::server_control::backend();
            let done = crate::migration::import_user_config(b, &crate::paths::config_dir());
            let mut lines = done.lines(b);
            let refusal = done.refused.is_some().then(|| lines.pop()).flatten();
            for line in lines {
                println!("{line}");
            }
            match refusal {
                Some(text) => Err(anyhow::anyhow!(text)),
                None => Ok(()),
            }
        }
    }
}

fn run_control(c: ControlCmd) -> Result<()> {
    run_control_with(c, crate::server_control::backend())
}

/// `run_control` on `server`, the backend that runs llama-server.
fn run_control_with(c: ControlCmd, server: &crate::server_control::Backend) -> Result<()> {
    use crate::runstate;
    match c {
        ControlCmd::Start => {
            match server.start() {
                Ok(Some(cfg)) => {
                    println!("{}", strings::t("server-started"));
                    snapshot_after_start(&cfg, server);
                }
                Ok(None) => println!("{}", strings::t("server-already-running")),
                Err(e) => anyhow::bail!(strings::fill("start-failed", &[&e])),
            }
            Ok(())
        }
        ControlCmd::Stop => {
            snapshot_before_stop(server);
            if !server.stop().map_err(anyhow::Error::msg)? {
                anyhow::bail!(strings::fill(
                    "stop-failed",
                    &[&runstate::STOP_GRACE.as_secs()]
                ));
            }
            println!("{}", strings::t("server-stopped"));
            Ok(())
        }
        ControlCmd::Restart => {
            // The full round trip: the snapshot the stop writes is the one the
            // start below puts back, which is what makes a restart cheap on a
            // long conversation instead of a fresh multi-minute prefill. Under
            // the manager that save happens in the service's stop, before the
            // start (this command saves none of its own).
            snapshot_before_stop(server);
            match server.restart() {
                Ok(Some(cfg)) => {
                    println!("{}", strings::t("server-restarted"));
                    snapshot_after_start(&cfg, server);
                }
                Ok(None) => println!("{}", strings::t("server-already-running")),
                Err(e) => anyhow::bail!(strings::fill("restart-failed", &[&e])),
            }
            Ok(())
        }
        ControlCmd::StopAndClose => {
            snapshot_before_stop(server);
            if let Err(e) = server.stop() {
                eprintln!("{e}");
            }
            std::thread::sleep(std::time::Duration::from_millis(500));
            #[cfg(windows)]
            pai_app::gui_close::signal_close();
            println!("{}", strings::t("server-stopped-gui-closed"));
            Ok(())
        }
    }
}

// The e2e phase below follows this module: `binding_lint` reads the CLI's print
// paths up to the test module (the `mod tests {` of a module inline in this
// file, its declaration here), and the phase's literals are test data.
#[cfg(test)]
#[allow(clippy::items_after_test_module)]
#[path = "cli_tests.rs"]
mod tests;

/// E2E phase (an isolated test of `src/tests/mod.rs`): the
/// `control` commands on the manager backend (a recording fake, with a
/// llama-server stand-in on a loopback port): each asks the manager for the
/// `default` instance; the command line sends NO snapshot request, because
/// under the manager the instance saves the slot snapshots at its own stop,
/// the conversation is put back AFTER the command's own start, and the
/// manager's errors reach the user as their own text.
#[cfg(test)]
pub(crate) fn cli_control_e2e() {
    use crate::server_control::fake::{self, FakeClient};
    use pai_core::client::{ClientError, InstanceState};
    let dir = tempfile::tempdir().expect("tempdir");
    crate::paths::redirect_for_tests(dir.path());
    let log = fake::log();
    let port = fake::http_stub(&log);
    let state = dir.path().join("state");
    std::fs::create_dir_all(&state).expect("state dir");
    // The config: the default backend's (the stop's snapshot reads it) and
    // the manager's, which a start or restart reads back.
    let server_text = format!(
        "[Server]\nPort = {port}\nSaveStateOnShutdown = true\nStateDir = {}\n",
        state.display()
    );
    let server_ini = crate::paths::server_ini();
    std::fs::create_dir_all(server_ini.parent().unwrap()).expect("config dir");
    std::fs::write(&server_ini, &server_text).expect("server.ini");
    std::fs::write(
        crate::paths::models_ini(),
        "[Qwen3.8-27B]\nmodel = /models/Qwen3.8-27B-Q8_0.gguf\n",
    )
    .expect("models.ini");
    // The last shutdown's dump, for the restore after a start.
    std::fs::write(
        state.join(crate::slot_state::state_file_name("Qwen3.8-27B")),
        b"dump",
    )
    .expect("dump");

    let position = |entries: &[String], what: &str| {
        entries
            .iter()
            .position(|e| e.starts_with(what))
            .unwrap_or_else(|| panic!("no {what:?} in {entries:?}"))
    };
    const SNAPSHOT: &str = "GET /models";
    const RESTORE: &str = "POST /slots/0?action=restore";
    // What reached the llama-server stand-in (the manager's own records hold
    // no path).
    let served = |entries: &[String]| -> Vec<String> {
        entries
            .iter()
            .filter(|e| e.contains(" /"))
            .cloned()
            .collect()
    };

    // Stop: the manager's stop of default, and nothing at all sent to
    // llama-server: the instance saves the snapshots as it stops, a second
    // save here would write the same gigabytes twice.
    log.lock().unwrap().clear();
    let b = fake::backend(FakeClient::new(&log, InstanceState::Active));
    run_control_with(ControlCmd::Stop, &b).expect("stop");
    let entries = log.lock().unwrap().clone();
    assert!(
        served(&entries).is_empty(),
        "no snapshot request under the manager: {entries:?}"
    );
    // What the manager was asked (the stub's requests are `<METHOD> /...`).
    let asked: Vec<String> = fake::actions(&log)
        .into_iter()
        .filter(|e| !e.contains(" /"))
        .collect();
    assert_eq!(asked, ["stop default"]);

    // Start: the manager's start of default, then the restore.
    log.lock().unwrap().clear();
    let b = fake::backend(
        FakeClient::new(&log, InstanceState::Inactive)
            .with_file("server@default.ini", &server_text),
    );
    run_control_with(ControlCmd::Start, &b).expect("start");
    let entries = log.lock().unwrap().clone();
    assert!(
        position(&entries, "start default") < position(&entries, RESTORE),
        "restore after the start: {entries:?}"
    );

    // Already running: nothing started, nothing restored over a live slot.
    log.lock().unwrap().clear();
    let b = fake::backend(FakeClient::new(&log, InstanceState::Active));
    run_control_with(ControlCmd::Start, &b).expect("start when running");
    let entries = log.lock().unwrap().clone();
    assert!(
        !entries
            .iter()
            .any(|e| e == "start default" || e.starts_with(RESTORE)),
        "{entries:?}"
    );

    // Restart: the manager's restart, then the restore of the dump the
    // instance wrote as it stopped. No save request of the command's own.
    log.lock().unwrap().clear();
    let b = fake::backend(
        FakeClient::new(&log, InstanceState::Active).with_file("server@default.ini", &server_text),
    );
    run_control_with(ControlCmd::Restart, &b).expect("restart");
    let entries = log.lock().unwrap().clone();
    let restart = position(&entries, "restart default");
    assert!(
        !entries.iter().any(|e| e.starts_with(SNAPSHOT)),
        "no snapshot save under the manager: {entries:?}"
    );
    assert!(restart < position(&entries, RESTORE), "{entries:?}");
    assert!(
        !entries
            .iter()
            .any(|e| e == "stop default" || e == "start default"),
        "one restart, not a stop and a start: {entries:?}"
    );

    // StopAndClose: the manager's stop, nothing sent to llama-server.
    log.lock().unwrap().clear();
    let b = fake::backend(FakeClient::new(&log, InstanceState::Active));
    run_control_with(ControlCmd::StopAndClose, &b).expect("stop-and-close");
    let entries = log.lock().unwrap().clone();
    assert!(
        served(&entries).is_empty(),
        "no snapshot request under the manager: {entries:?}"
    );
    assert!(
        entries.iter().any(|e| e == "stop default"),
        "the stop is still asked: {entries:?}"
    );

    // The manager's errors: the command fails with the client's own text.
    for (cmd, e) in [
        (ControlCmd::Start, ClientError::Unavailable),
        (ControlCmd::Stop, ClientError::Unavailable),
        (ControlCmd::Restart, ClientError::Unavailable),
        (ControlCmd::Start, ClientError::NotAuthorized),
        (ControlCmd::Stop, ClientError::NotAuthorized),
        (ControlCmd::Restart, ClientError::NotAuthorized),
    ] {
        let label = format!("{cmd:?} {e:?}");
        let mut c = FakeClient::new(&log, InstanceState::Inactive);
        c.fail_actions = Some(e.clone());
        if e == ClientError::Unavailable {
            c.fail_status = Some(e.clone());
        }
        match run_control_with(cmd, &fake::backend(c)) {
            Ok(()) => panic!("{label}: succeeded"),
            Err(err) => assert!(
                format!("{err:#}").contains(&e.text()),
                "{label}: {err:#} carries {:?}",
                e.text()
            ),
        }
    }
}

/// E2E phase (an isolated test of `src/tests/mod.rs`): `server
/// set` and `model-profile enable` / `disable` write the manager's files through the
/// seam and nothing under the per-user config directory, and a write the
/// manager refuses fails the command with the refusal's own text, which
/// `main` prints on stderr before exiting non-zero.
#[cfg(test)]
pub(crate) fn cli_config_e2e() {
    use crate::server_control::fake::{self, FakeClient};
    use clap::Parser;
    use pai_core::client::{ClientError, InstanceState};
    let dir = tempfile::tempdir().expect("tempdir");
    crate::paths::redirect_for_tests(dir.path());
    let user = crate::paths::config_dir();
    let cli = |args: &[&str]| Cli::try_parse_from(args).expect("parses");
    let nothing_per_user = |when: &str| {
        for name in ["server.ini", "models.ini", "models.ini.disabled"] {
            assert!(
                !user.join(name).exists(),
                "{when}: {name} written under the per-user config directory"
            );
        }
    };
    let log = fake::log();
    let seeded = || {
        FakeClient::new(&log, InstanceState::Inactive)
            .with_file("server@default.ini", "[Server]\nPort = 8080\n")
            .with_file(
                "models.ini",
                "[Qwen3.8-27B]\nmodel = /models/Qwen3.8-27B-Q8_0.gguf\n",
            )
            .with_file(
                "models.ini.disabled",
                "[Qwen3.8-Flash-Next]\nmodel = /models/Qwen3.8-Flash-Next-IQ3_S.gguf\n",
            )
    };

    let c = seeded();
    let files = c.files.clone();
    let file = |name: &str| files.lock().unwrap().get(name).cloned().unwrap_or_default();
    fake::scoped(fake::backend(c), || {
        log.lock().unwrap().clear();
        run(cli(&["x", "server", "set", "--port", "8099"])).expect("server set");
        assert_eq!(fake::writes(&log), ["write_file server@default.ini"]);
        assert_eq!(
            crate::server_cfg::from_text(&file("server@default.ini")).port,
            Some(8099)
        );
        nothing_per_user("server set");

        log.lock().unwrap().clear();
        run(cli(&["x", "model-profile", "disable", "Qwen3.8-27B"])).expect("model-profile disable");
        assert_eq!(
            fake::writes(&log),
            ["write_file models.ini.disabled", "write_file models.ini"]
        );
        assert!(!file("models.ini").contains("[Qwen3.8-27B]"));
        assert!(file("models.ini.disabled").contains("[Qwen3.8-27B]"));
        nothing_per_user("model-profile disable");
    });

    // The key withheld from this caller: `server set` saves back a file
    // with no key, and asks the manager to keep its own; a key given with
    // --opencode-api-key is sent as given, nothing kept.
    {
        let c = FakeClient::new(&log, InstanceState::Inactive)
            .with_file(
                "server@default.ini",
                "[Server]\nPort = 8080\nOpencodeApiKey =\n",
            )
            .with_file(
                "models.ini",
                "[Qwen3.8-27B]\nmodel = /models/Qwen3.8-27B-Q8_0.gguf\n",
            );
        *c.withhold.lock().unwrap() = true;
        let files = c.files.clone();
        let file = || {
            files
                .lock()
                .unwrap()
                .get("server@default.ini")
                .cloned()
                .unwrap_or_default()
        };
        fake::scoped(fake::backend(c), || {
            log.lock().unwrap().clear();
            run(cli(&["x", "server", "set", "--port", "8099"])).expect("server set");
            assert_eq!(
                fake::writes(&log),
                [format!(
                    "write_file server@default.ini {}",
                    fake::KEEP_API_KEY
                )]
            );
            assert_eq!(crate::server_cfg::from_text(&file()).port, Some(8099));

            log.lock().unwrap().clear();
            run(cli(&[
                "x",
                "server",
                "set",
                "--opencode-api-key",
                "sk-typed-0000",
            ]))
            .expect("server set with a key");
            assert_eq!(fake::writes(&log), ["write_file server@default.ini"]);
            assert_eq!(
                crate::server_cfg::from_text(&file()).opencode_api_key,
                Some("sk-typed-0000".into())
            );
        });
    }

    // Refused: the command fails with the refusal's text, nothing written.
    let refusal = pai_core::checks::Refusal {
        key: "not-whole-number-in-range",
        values: ["Port", "99999", "1", "65535"].map(String::from).to_vec(),
    };
    for e in [ClientError::Invalid(refusal), ClientError::NotAuthorized] {
        for args in [
            &["x", "server", "set", "--port", "8099"][..],
            &["x", "model-profile", "enable", "Qwen3.8-Flash-Next"][..],
        ] {
            let mut c = seeded();
            c.fail_write = Some(e.clone());
            let files = c.files.clone();
            let before = files.lock().unwrap().clone();
            let label = format!("{args:?} {e:?}");
            match fake::scoped(fake::backend(c), || run(cli(args))) {
                Ok(()) => panic!("{label}: succeeded"),
                Err(err) => assert!(
                    format!("{err:#}").contains(&e.text()),
                    "{label}: {err:#} carries {:?}",
                    e.text()
                ),
            }
            assert_eq!(*files.lock().unwrap(), before, "{label}: nothing changed");
            nothing_per_user(&label);
        }
    }
}

/// E2E phase (an isolated test of `src/tests/mod.rs`): `server
/// set` refuses what the Server tab refuses, before anything is written, on
/// the manager backend (which would refuse some of it too) and on the direct
/// one (macOS, no manager), with the refusal's text; and stores the base URL
/// without its trailing `/` and `/v1`, the GUI's rule, so a stored `/v1` is
/// never appended twice.
#[cfg(test)]
pub(crate) fn cli_server_checks_e2e() {
    use crate::server_control::fake::{self, FakeClient};
    use crate::server_control::Backend;
    use clap::Parser;
    use pai_core::checks::Refusal;
    use pai_core::client::InstanceState;
    let dir = tempfile::tempdir().expect("tempdir");
    crate::paths::redirect_for_tests(dir.path());
    let parse = |args: &[&str]| {
        Cli::try_parse_from(args).unwrap_or_else(|e| panic!("{args:?} does not parse: {e}"))
    };
    let cpus = std::thread::available_parallelism()
        .expect("the CPU count")
        .get();
    let over = (cpus + 1).to_string();
    let all = cpus.to_string();
    let not_int = |key: &str, text: &str, lo: &str, hi: &str| Refusal {
        key: "not-whole-number-in-range",
        values: [key, text, lo, hi].map(String::from).to_vec(),
    };
    let max = i32::MAX.to_string();
    let refused: Vec<(Vec<&str>, Refusal)> = vec![
        (
            vec!["--models-max", "-1"],
            not_int("ModelsMax", "-1", "0", &max),
        ),
        (vec!["--port", "0"], not_int("Port", "0", "1", "65535")),
        (
            vec!["--log-verbosity", "7"],
            not_int("LogVerbosity", "7", "0", "5"),
        ),
        (
            vec!["--hostname", "0.0.0.0,127.0.0.1"],
            Refusal {
                key: "bind-all-with-others",
                values: vec![],
            },
        ),
        (
            vec!["--threads", &over],
            not_int("Threads", &over, "1", &all),
        ),
        (
            vec!["--threads-batch", &over],
            not_int("ThreadsBatch", &over, "1", &all),
        ),
    ];
    let urls = [
        ("http://h:1/v1/", "http://h:1"),
        ("http://h:1/v1", "http://h:1"),
        ("http://h:1/v1//", "http://h:1"),
        ("http://h:1/", "http://h:1"),
        ("https://gw.example.com/llm", "https://gw.example.com/llm"),
    ];
    let seed = "[Server]\nPort = 8080\n";

    // ── The manager backend: refused before the manager is asked ─────────
    let log = fake::log();
    for (flags, want) in &refused {
        let c =
            FakeClient::new(&log, InstanceState::Inactive).with_file("server@default.ini", seed);
        let files = c.files.clone();
        let args: Vec<&str> = ["x", "server", "set"]
            .iter()
            .copied()
            .chain(flags.iter().copied())
            .collect();
        let cli = parse(&args);
        log.lock().unwrap().clear();
        match fake::scoped(fake::backend(c), || run(cli)) {
            Ok(()) => panic!("manager {flags:?}: written"),
            Err(err) => assert!(
                format!("{err:#}").contains(&want.text()),
                "manager {flags:?}: {err:#} does not carry {:?}",
                want.text()
            ),
        }
        assert!(
            fake::writes(&log).is_empty(),
            "manager {flags:?}: {:?}",
            fake::writes(&log)
        );
        assert_eq!(
            files
                .lock()
                .unwrap()
                .get("server@default.ini")
                .map(String::as_str),
            Some(seed),
            "manager {flags:?}: the file changed"
        );
    }
    for (typed, stored) in urls {
        let c =
            FakeClient::new(&log, InstanceState::Inactive).with_file("server@default.ini", seed);
        let files = c.files.clone();
        let cli = parse(&["x", "server", "set", "--opencode-base-url", typed]);
        fake::scoped(fake::backend(c), || run(cli)).expect("server set");
        let text = files
            .lock()
            .unwrap()
            .get("server@default.ini")
            .cloned()
            .unwrap_or_default();
        assert_eq!(
            crate::server_cfg::from_text(&text)
                .opencode_base_url
                .as_deref(),
            Some(stored),
            "manager {typed:?}"
        );
    }
    // The CPU count itself is a thread count the sliders reach.
    let c = FakeClient::new(&log, InstanceState::Inactive).with_file("server@default.ini", seed);
    let files = c.files.clone();
    let cli = parse(&[
        "x",
        "server",
        "set",
        "--threads",
        &all,
        "--threads-batch",
        &all,
    ]);
    fake::scoped(fake::backend(c), || run(cli)).expect("server set --threads <cpus>");
    let text = files
        .lock()
        .unwrap()
        .get("server@default.ini")
        .cloned()
        .unwrap_or_default();
    assert_eq!(
        crate::server_cfg::from_text(&text).threads,
        Some(cpus as i32)
    );

    // ── The direct backend (macOS): the per-user server.ini ─────────────
    let ini = crate::paths::server_ini();
    std::fs::create_dir_all(ini.parent().expect("a parent")).expect("config dir");
    for (flags, want) in &refused {
        std::fs::write(&ini, seed).expect("seed server.ini");
        let args: Vec<&str> = ["x", "server", "set"]
            .iter()
            .copied()
            .chain(flags.iter().copied())
            .collect();
        let cli = parse(&args);
        match fake::scoped(Backend::Direct, || run(cli)) {
            Ok(()) => panic!("direct {flags:?}: written"),
            Err(err) => assert!(
                format!("{err:#}").contains(&want.text()),
                "direct {flags:?}: {err:#} does not carry {:?}",
                want.text()
            ),
        }
        assert_eq!(
            std::fs::read_to_string(&ini).ok().as_deref(),
            Some(seed),
            "direct {flags:?}: server.ini changed"
        );
    }
    for (typed, stored) in urls {
        std::fs::write(&ini, seed).expect("seed server.ini");
        let cli = parse(&["x", "server", "set", "--opencode-base-url", typed]);
        fake::scoped(Backend::Direct, || run(cli)).expect("server set");
        assert_eq!(
            crate::server_cfg::load_from(&ini)
                .opencode_base_url
                .as_deref(),
            Some(stored),
            "direct {typed:?}"
        );
    }
}

/// E2E phase (an isolated test of `src/tests/mod.rs`):
/// `config import` hands the per-user files to the manager through the seam,
/// leaves them in place, fails with the refusal's text (which `main` prints
/// on stderr before exiting non-zero) after keeping what was written, and
/// succeeds with nothing to do when there is nothing to import.
#[cfg(test)]
pub(crate) fn cli_import_e2e() {
    use crate::server_control::fake::{self, FakeClient};
    use clap::Parser;
    use pai_core::client::{ClientError, InstanceState};
    let dir = tempfile::tempdir().expect("tempdir");
    crate::paths::redirect_for_tests(dir.path());
    let user = crate::paths::config_dir();
    let import = || run(Cli::try_parse_from(["x", "config", "import"]).expect("parses"));
    const SERVER: &str = "[Server]\r\nPort = 8080\r\nModelsMax = 6\r\n";
    const MODELS: &str = "[Qwen3.8-27B]\r\nmodel = /mnt/data/models/Qwen3.8-27B-Q8_0.gguf\r\n";

    // Nothing per user: nothing to import, success, even with no manager.
    let log = fake::log();
    let mut c = FakeClient::new(&log, InstanceState::Inactive);
    c.fail_read = Some(ClientError::Unavailable);
    c.fail_write = Some(ClientError::Unavailable);
    fake::scoped(fake::backend(c), import).expect("nothing to import is a success");
    assert_eq!(fake::writes(&log), Vec::<String>::new());

    std::fs::create_dir_all(&user).expect("config dir");
    std::fs::write(user.join("server.ini"), SERVER).expect("server.ini");
    // The model profiles file of an earlier install, under the name that install
    // wrote it under: the hand-over reads that name, never the manager's.
    std::fs::write(user.join("presets.ini"), MODELS).expect("presets.ini");

    // No manager, with files to import: the client's text, nothing written.
    let log = fake::log();
    let mut c = FakeClient::new(&log, InstanceState::Inactive);
    c.fail_read = Some(ClientError::Unavailable);
    c.fail_write = Some(ClientError::Unavailable);
    match fake::scoped(fake::backend(c), import) {
        Ok(()) => panic!("no manager: succeeded"),
        Err(err) => assert!(
            format!("{err:#}").contains(&ClientError::Unavailable.text()),
            "{err:#}"
        ),
    }

    // models.ini refused: fails with the refusal's text; the server file
    // written before it stays.
    let refusal = pai_core::checks::Refusal {
        key: "load-on-startup-over-max",
        values: ["7", "6"].map(String::from).to_vec(),
    };
    let e = ClientError::Invalid(refusal);
    let log = fake::log();
    let mut c = FakeClient::new(&log, InstanceState::Inactive);
    c.fail_write_of.insert("models.ini".into(), e.clone());
    let files = c.files.clone();
    match fake::scoped(fake::backend(c), import) {
        Ok(()) => panic!("refused: succeeded"),
        Err(err) => assert!(format!("{err:#}").contains(&e.text()), "{err:#}"),
    }
    assert_eq!(
        fake::writes(&log),
        ["write_file server@default.ini", "write_file models.ini"]
    );
    assert_eq!(
        files
            .lock()
            .unwrap()
            .get("server@default.ini")
            .map(String::as_str),
        Some(SERVER)
    );

    // Again: only what is left, a success.
    let log = fake::log();
    let mut c = FakeClient::new(&log, InstanceState::Inactive);
    c.files = files.clone();
    fake::scoped(fake::backend(c), import).expect("the rest imported");
    assert_eq!(fake::writes(&log), ["write_file models.ini"]);
    assert_eq!(
        files.lock().unwrap().get("models.ini").map(String::as_str),
        Some(MODELS)
    );

    // Once more: nothing to import, a success, nothing written.
    let log = fake::log();
    let mut c = FakeClient::new(&log, InstanceState::Inactive);
    c.files = files.clone();
    fake::scoped(fake::backend(c), import).expect("nothing left is a success");
    assert_eq!(fake::writes(&log), Vec::<String>::new());

    // The user's files are where they were, byte for byte.
    assert_eq!(
        std::fs::read_to_string(user.join("server.ini")).unwrap(),
        SERVER
    );
    assert_eq!(
        std::fs::read_to_string(user.join("presets.ini")).unwrap(),
        MODELS
    );
}

/// E2E phase (an isolated test of `src/tests/mod.rs`): the server
/// file `pai server …` reads and writes is the profile the manager's record
/// names, never `server@<id>.ini`, and a record that cannot be read fails the
/// command with its own text and no server file read (`ARCHITECTURE.md`,
/// slice 13).
#[cfg(test)]
pub(crate) fn cli_server_profile_e2e() {
    use crate::server_control::fake::{self, FakeClient};
    use clap::Parser;
    use pai_core::client::{ClientError, InstanceState};
    let dir = tempfile::tempdir().expect("tempdir");
    crate::paths::redirect_for_tests(dir.path());
    let user = crate::paths::config_dir();
    let cli = |args: &[&str]| Cli::try_parse_from(args).expect("parses");
    /// The file reads the fake was asked for, in order.
    fn reads(log: &fake::Log) -> Vec<String> {
        log.lock()
            .unwrap()
            .iter()
            .filter(|e| e.starts_with("read_file "))
            .cloned()
            .collect()
    }
    const SEED: &str = "[Server]\nPort = 8080\n";

    // The record's row for `default` names the profile `big`: the read and the
    // write are `server@big.ini`, and the file of the id is left as it was.
    let log = fake::log();
    let c = FakeClient::new(&log, InstanceState::Inactive)
        .with_instance("default", "big", 0)
        .with_file("server@big.ini", SEED)
        .with_file("server@default.ini", "[Server]\nPort = 7777\n");
    let files = c.files.clone();
    fake::scoped(fake::backend(c), || {
        log.lock().unwrap().clear();
        run(cli(&["x", "server", "set", "--port", "8642"])).expect("server set");
        assert_eq!(
            // The save reads the model profile pool to check the startup count,
            // so its read sits between the file read and the write.
            reads(&log),
            ["read_file server@big.ini", "read_file models.ini"],
            "the read is the profile's file: {log:?}"
        );
        assert_eq!(
            fake::writes(&log),
            ["write_file server@big.ini"],
            "the write names the profile's file"
        );
        let big = files
            .lock()
            .unwrap()
            .get("server@big.ini")
            .cloned()
            .unwrap_or_default();
        assert_eq!(crate::server_cfg::from_text(&big).port, Some(8642), "{big}");
        assert_eq!(
            crate::server_cfg::from_text(
                &files
                    .lock()
                    .unwrap()
                    .get("server@default.ini")
                    .cloned()
                    .unwrap_or_default()
            )
            .port,
            Some(7777),
            "server@default.ini is not touched"
        );
    });

    // `server show` reads the same file: the port it prints is the profile's.
    let log = fake::log();
    let c = FakeClient::new(&log, InstanceState::Inactive)
        .with_instance("default", "big", 0)
        .with_file("server@big.ini", "[Server]\nPort = 9011\n");
    fake::scoped(fake::backend(c), || {
        log.lock().unwrap().clear();
        run(cli(&["x", "server", "show"])).expect("server show");
        assert_eq!(
            reads(&log),
            ["read_file server@big.ini"],
            "the read is the profile's file: {log:?}"
        );
    });

    // The record that cannot be read fails the command with its own text: no
    // server file is read, because the record is what names it, and nothing is
    // written. The only read is the save's own of the model profile pool
    // (`server_cfg::save` checks the startup count before it writes).
    let log = fake::log();
    let mut c = FakeClient::new(&log, InstanceState::Inactive)
        .with_file("server@default.ini", SEED)
        .with_file("models.ini", "");
    c.fail_instances = Some(ClientError::Unavailable);
    let files = c.files.clone();
    let before = files.lock().unwrap().clone();
    match fake::scoped(fake::backend(c), || {
        run(cli(&["x", "server", "set", "--port", "8642"]))
    }) {
        Ok(()) => panic!("a record that cannot be read: succeeded"),
        Err(err) => assert!(
            format!("{err:#}").contains(&ClientError::Unavailable.text()),
            "{err:#} carries the manager's news"
        ),
    }
    assert_eq!(
        reads(&log),
        ["read_file models.ini"],
        "no server file is read, the save's pool read is all: {log:?}"
    );
    assert!(fake::writes(&log).is_empty(), "nothing written");
    assert_eq!(*files.lock().unwrap(), before, "the files as they were");

    // A row whose section names no profile is no instance: the command fails,
    // and `server@<id>.ini` is never read.
    let log = fake::log();
    let mut c =
        FakeClient::new(&log, InstanceState::Inactive).with_file("server@default.ini", SEED);
    c.instances = vec![pai_core::client::InstanceInfo {
        id: "default".into(),
        engine: "llama.cpp".into(),
        server_profile: None,
        port: 0,
        profile_tags: vec![],
        instance_tags: vec![],
        label: None,
        converged: true,
        reason: None,
        pending_restart: false,
        instance_profile: None,
        hostname: None,
    }];
    let _files = c.files.clone();
    match fake::scoped(fake::backend(c), || {
        run(cli(&["x", "server", "set", "--port", "8642"]))
    }) {
        Ok(()) => panic!("a section naming no profile: succeeded"),
        Err(err) => assert!(
            format!("{err:#}").contains(&ClientError::NoInstance.text()),
            "{err:#} names no instance"
        ),
    }
    assert_eq!(
        reads(&log),
        ["read_file models.ini"],
        "no server file is read, the save's pool read is all: {log:?}"
    );
    assert!(fake::writes(&log).is_empty(), "nothing written");

    // The hand-over of the per-user server file goes to the profile's name, so
    // the next `server show` reads what the import wrote.
    std::fs::create_dir_all(&user).expect("config dir");
    std::fs::write(user.join("server.ini"), "[Server]\r\nPort = 8080\r\n").expect("server.ini");
    let log = fake::log();
    let c = FakeClient::new(&log, InstanceState::Inactive).with_instance("default", "big", 0);
    let files = c.files.clone();
    fake::scoped(fake::backend(c), || {
        run(cli(&["x", "config", "import"])).expect("import");
        assert_eq!(
            fake::writes(&log),
            ["write_file server@big.ini"],
            "the import writes the profile's file"
        );
        assert!(
            files.lock().unwrap().contains_key("server@big.ini"),
            "the profile's file: {:?}",
            files.lock().unwrap().keys().clone().collect::<Vec<_>>()
        );
    });
}
