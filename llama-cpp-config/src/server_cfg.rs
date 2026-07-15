//! server.ini schema and IO for llama.cpp-framework.
//!
//! `save()` rewrites the whole FILE from a `ServerConfig` (server.ini is fully
//! generated — unlike presets.ini, hand-added content outside the template does
//! not survive a save). Unset optional fields are emitted as commented
//! `; Key = example  ; help` hint lines (never omitted) so the file
//! self-documents every available knob.
//!
//! ADD A SERVER FIELD — the mirror of the preset recipe (top of presets.rs): a
//! smaller schema, but a LARGER per-field fan-out — it also reaches the CLI (the
//! server has a per-field `set`) and the launch path (steps 7-8 below). Trace an
//! existing field like `threads` (PascalCase INI key ↔ snake_case Rust field):
//!   1. `ServerConfig` struct field (+ doc)   — below
//!   2. `from_keys` (backs `load`)            — INI read; `keys.get("Key")` → field
//!   3. `render` (backs `save`)               — INI write; an `int_line_or_hint` /
//!      `str_line_or_hint` line + a `{…_line}` slot in the `[Server]` body
//!   4. `ServerForm` struct                   — ui/types.slint (a NUMERIC field
//!      rides as a `string` plus a paired `<field>_default` bool — the "omit the
//!      flag" checkbox)
//!   5. the input widget                      — ui/server_page.slint, bind two-way
//!      `<=>`: DefaultLineEdit for numerics (`input_type: InputType.number` for an
//!      integer; wire BOTH `value` and `default`). Never a SpinBox — it edits itself
//!      on a stray mouse-wheel, and `binding_lint`'s `no_spinbox_widgets_anywhere`
//!      fails the build if one returns
//!   6. `config_to_form` + `form_to_config`   — src/server_form.rs (BOTH
//!      directions; derive `<field>_default` via `is_none()` / `if <field>_default`)
//!   7. THREE spots in src/cli.rs             — the `ServerSet` flag field, a
//!      `row(...)` in `show_lines`, and `ServerSet::apply` copying the flag into `cfg`
//!   8. `runstate::server_args`               — map the field to its llama-server
//!      flag (or wave it through with a comment if it's launch-env only, like
//!      ModelsDir → LLAMA_CACHE in `start()`)
//!   9. PATH-VALUED field only: add it to `validate_for_save` below (and extend
//!      `save_validation_rejects_comment_markers_in_models_dir`) — the INI
//!      format can't escape `;`/`#`, so an unvalidated path saves fine and
//!      reloads TRUNCATED. Like the widget, nothing fails if you skip this.
//!
//! Guards: the save→load round-trip test in this file (steps 2–3: a key-name typo
//! or wrong `keep` rule fails it), the form round-trip in server_form.rs
//! (`form_to_config(config_to_form(c)) == c`, step 6), cli.rs's
//! `server_set_apply_copies_every_field` + `show_lines_prints_every_field`
//! (step 7 — the first is airtight, whole-struct equality against an exhaustive
//! literal; the second's destructure only forces the BIND — extend its manual
//! `needles` array too, or the Show row goes unguarded), and runstate's
//! `server_args_covers_every_config_field` (step 8 — its exhaustive destructure
//! breaks compilation until the launch path consumes the field). Give the new
//! field a NON-DEFAULT value when extending the rich fixtures: `None` satisfies
//! the compiler but makes every round-trip vacuous for that field.

use std::fs;
use std::io;

use crate::ini;
use crate::paths;

// ── Schema: ServerConfig ─────────────────────────────────────────────────
#[derive(Debug, Clone, Default, PartialEq)]
pub struct ServerConfig {
    pub port: Option<i32>,
    pub hostname: Option<String>,
    pub mlock: Option<bool>,
    /// Read the weights into RAM instead of memory-mapping the GGUF (--no-mmap).
    /// None = the framework default (off = mmap on, llama.cpp's own default).
    /// Load is slower and costs the model's size in RAM, but nothing can be
    /// dropped back to the file later. Pairs with `mlock`, which locks whatever
    /// is resident: with mmap on, pages fault in from the GGUF as they're first
    /// touched, so mlock alone only pins what has already been read.
    pub no_mmap: Option<bool>,
    pub threads: Option<i32>,
    pub cache_reuse: Option<i32>,
    pub threads_batch: Option<i32>,
    pub models_max: Option<i32>,
    pub models_dir: Option<String>,
    /// The GPUs every model runs on by default (--device): one id ("CUDA0") or a
    /// comma-separated list in split order ("ROCm1,CUDA0"). Empty/None = let
    /// llama.cpp use all detected devices — which on a mixed box means an iGPU and
    /// duplicate Vulkan views of the same cards. Set here, this OVERRIDES every
    /// preset's own `device`: llama-server's router passes its CLI args on top of
    /// each preset. Written by the GPU distribution table (src/gpu_split.rs).
    pub device: Option<String>,
    /// Multi-GPU split strategy (--split-mode / -sm): "none" | "layer" | "row".
    /// Empty/None = llama.cpp default ("layer"). Identical on CUDA and HIP.
    pub split_mode: Option<String>,
    /// Per-GPU weight proportions (--tensor-split / -ts), e.g. "3,1" for 75/25 —
    /// positional over `device` above, in that order. Empty/None with 2+ devices =
    /// llama.cpp splits by each device's FREE memory at load (not evenly).
    pub tensor_split: Option<String>,
    /// Tensor-placement rules applied to EVERY model (--override-tensor / -ot):
    /// `<regex>=<buffer type>` rules joined by `,`, e.g. `token_embd\.weight=ROCm0`.
    /// None/blank = no rules (every tensor goes wherever its layer went). Written
    /// by the tensor-placement table (src/tensor_override.rs), same schema as the
    /// per-preset `override-tensor`.
    ///
    /// Set here, this REPLACES every preset's own rules — it does not add to them.
    /// `-ot` is a `push_back` onto a vector inside llama.cpp, so accumulating would
    /// be the natural guess, but the router never lets two reach the child: it
    /// merges its own CLI args into each preset as a KEY→VALUE map
    /// (`common_preset::merge`, `options[opt] = val`), with the CLI as the writer.
    /// Verified against b9976 — a preset asking for `token_embd\.weight=CPU` under a
    /// server-wide `token_embd\.weight=CUDA0` spawns its child with the CUDA0 rule
    /// and no trace of the CPU one. `AppState.tensor_override_warning` says so in
    /// the Models tab, the same way the GPU table's does.
    ///
    /// One asymmetry with the per-preset field: the buffer type is validated by the
    /// ROUTER at startup, against its own device registry. A rule naming a device
    /// this machine doesn't have ("unknown buffer type") therefore kills the whole
    /// SERVER, not just the model that would have used it.
    pub override_tensor: Option<String>,
    /// The GPU the multimodal projector — the mmproj/CLIP image encoder — runs on.
    /// NOT a llama-server flag: it is the `MTMD_BACKEND_DEVICE` env var, set on the
    /// child in `runstate::start`. It needs its own knob because the encoder ignores
    /// `--device` entirely — `clip_ctx` takes the FIRST GPU backend the registry
    /// offers, which on a CUDA+ROCm box is the NVIDIA card even when the model is on
    /// the AMD one. It then holds that card's VRAM for the model's whole lifetime
    /// while computing only on image requests, which reads exactly like a GPU that
    /// has been "assigned something" and does nothing. None = leave llama.cpp to it.
    pub mmproj_device: Option<String>,
    /// Enable the built-in web UI's MCP CORS proxy (--webui-mcp-proxy).
    /// None = the framework default (on) — the bundled chat UI needs it to call
    /// MCP tools. Experimental upstream (llama.cpp defaults it OFF); don't enable
    /// on an untrusted network.
    pub webui_mcp_proxy: Option<bool>,
    /// Let llama.cpp auto-shrink unset args to fit device memory (-fit on|off).
    /// None = the framework default (off): the GUI's "default" n-gpu-layers means
    /// "offload every layer", which -fit on would silently override.
    pub fit: Option<bool>,
    /// Continue a TRAILING assistant message instead of answering it
    /// (--prefill-assistant / --no-prefill-assistant). None = llama.cpp's own
    /// default, which is ON: when the last message of a request is an assistant
    /// message, the server treats it as the START of the reply and generates the
    /// continuation, rather than as a finished turn to answer.
    ///
    /// It is a SERVER-scope flag upstream (`set_examples({LLAMA_EXAMPLE_SERVER})`)
    /// and read from the child's `params_base` (`server-context.cpp` →
    /// `chat_params.prefill_assistant`), not from anything model-specific — which
    /// is why it lives here and not in `presets.ini`. Turn it OFF when a client
    /// legitimately ends a conversation on an assistant turn and expects a fresh
    /// reply, and the model instead resumes mid-sentence.
    pub prefill_assistant: Option<bool>,
    /// llama-server log verbosity threshold (-lv / --log-verbosity N): messages
    /// above this level are dropped. None = the framework default (4, per-request
    /// logging into the captured llama-server.log). Always passed to the launch.
    /// llama.cpp defines exactly six levels — 0 output, 1 error, 2 warning,
    /// 3 info, 4 trace, 5 debug (`common/arg.cpp`, `common/log.h`) — which is why
    /// the GUI offers them as a dropdown rather than a free number.
    pub log_verbosity: Option<i32>,
    /// Explicit override for the integration base URL (opencode.json + Claude Code
    /// snippet). When set, this value is used instead of auto-deriving from
    /// hostname + port. Useful for reverse-proxy setups where the client-facing
    /// URL differs from the server bind address. None = auto-derive from server.ini.
    pub opencode_base_url: Option<String>,
    /// API key for the integration provider. Written as `apiKey` in opencode.json
    /// and used by Claude Code env snippet. Useful when the server is exposed
    /// through a proxy or LLM gateway that requires authentication. None = unset.
    pub opencode_api_key: Option<String>,
}

// ── Defaults & accessors ─────────────────────────────────────────────────

/// Default ModelsDir when server.ini leaves it unset. ModelsDir is the *root*
/// the four fixed subfolders hang off (models\, mmprojs\, mtps\, dflashs\ —
/// see `model_scan`), so this is a bare `~\.llama.cpp`, not `…\models`.
pub fn default_models_dir() -> String {
    paths::home_dir()
        .join(".llama.cpp")
        .to_string_lossy()
        .into_owned()
}

impl ServerConfig {
    /// The configured ModelsDir, or `default_models_dir()` when unset/blank —
    /// the single home for the "blank ModelsDir ⇒ default dir" rule shared by
    /// `save()`, `runstate::start()`, and `server_form::config_to_form`.
    pub fn models_dir_or_default(&self) -> String {
        self.models_dir
            .as_deref()
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .map_or_else(default_models_dir, str::to_string)
    }

    // The always-written trio's defaults (Port / Hostname / Mlock render as real
    // lines even when unset). ONE home each — `render()`, `server_form`,
    // `runstate::server_args`, and the GUI all pull from here, so changing a
    // default is a one-line edit instead of a four-file hunt.

    /// The configured port, or 8080 when unset.
    pub fn port_or_default(&self) -> i32 {
        self.port.unwrap_or(8080)
    }

    /// The configured bind host, or "localhost" when unset/blank. This is what
    /// llama-server LISTENS on (`--host`) — for the address clients connect to,
    /// use [`Self::client_host`].
    pub fn hostname_or_default(&self) -> String {
        self.hostname
            .as_deref()
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .map_or_else(|| "localhost".into(), str::to_string)
    }

    /// The configured mlock flag, or `true` (the framework default) when unset.
    pub fn mlock_or_default(&self) -> bool {
        self.mlock.unwrap_or(true)
    }

    /// The no-mmap flag, or `false` (the framework default: keep llama.cpp's
    /// mmap) when unset.
    pub fn no_mmap_or_default(&self) -> bool {
        self.no_mmap.unwrap_or(false)
    }

    /// The web-UI MCP proxy flag, or `true` (the framework default) when unset.
    pub fn webui_mcp_proxy_or_default(&self) -> bool {
        self.webui_mcp_proxy.unwrap_or(true)
    }

    /// The -fit flag, or `false` (the framework default: off) when unset.
    pub fn fit_or_default(&self) -> bool {
        self.fit.unwrap_or(false)
    }

    /// Prefill a trailing assistant message, or `true` (llama.cpp's own default)
    /// when unset.
    pub fn prefill_assistant_or_default(&self) -> bool {
        self.prefill_assistant.unwrap_or(true)
    }

    /// The log verbosity (-lv), or `4` (the framework default) when unset.
    pub fn log_verbosity_or_default(&self) -> i32 {
        self.log_verbosity.unwrap_or(4)
    }

    /// The integration base URL: explicit override if set, otherwise auto-derived
    /// from hostname + port + `/v1` suffix. Used by opencode.json and Claude Code.
    pub fn opencode_base_url_or_default(&self) -> String {
        if let Some(url) = self.opencode_base_url.as_deref().map(str::trim) {
            if !url.is_empty() {
                return url.to_string();
            }
        }
        format!("http://{}:{}", self.client_host(), self.port_or_default())
    }

    /// The host a CLIENT on this machine should connect to. Same as
    /// `hostname_or_default` except the all-interfaces bind `0.0.0.0` maps to
    /// `localhost`: it is a listen address, not a connectable one (Windows
    /// refuses it as a destination). Used by the Open-chat URL and the
    /// Integrations base URL.
    pub fn client_host(&self) -> String {
        let host = self.hostname_or_default();
        if host == "0.0.0.0" {
            "localhost".into()
        } else {
            host
        }
    }
}

// ── Construct & parse (from_keys = INI read) ─────────────────────────────

pub fn load() -> ServerConfig {
    let path = paths::server_ini();
    from_keys(&ini::read_section(&path, "Server"))
}

/// Parse the `[Server]` key/value map into a config. Split out of `load()` so
/// the save→load round-trip test can run against `render()`'s output without
/// touching the real config path.
fn from_keys(keys: &std::collections::BTreeMap<String, String>) -> ServerConfig {
    ServerConfig {
        port: keys.get("Port").and_then(|v| ini::parse_int(v)),
        hostname: opt_nonblank(keys.get("Hostname").cloned()),
        mlock: keys.get("Mlock").and_then(|v| ini::parse_bool(v)),
        no_mmap: keys.get("NoMmap").and_then(|v| ini::parse_bool(v)),
        threads: keys.get("Threads").and_then(|v| ini::parse_int(v)),
        cache_reuse: keys.get("CacheReuse").and_then(|v| ini::parse_int(v)),
        threads_batch: keys.get("ThreadsBatch").and_then(|v| ini::parse_int(v)),
        models_max: keys.get("ModelsMax").and_then(|v| ini::parse_int(v)),
        models_dir: opt_nonblank(keys.get("ModelsDir").cloned()),
        device: opt_nonblank(keys.get("Device").cloned()),
        split_mode: opt_nonblank(keys.get("SplitMode").cloned()),
        tensor_split: opt_nonblank(keys.get("TensorSplit").cloned()),
        override_tensor: opt_nonblank(keys.get("OverrideTensor").cloned()),
        mmproj_device: opt_nonblank(keys.get("MmprojDevice").cloned()),
        webui_mcp_proxy: keys.get("WebuiMcpProxy").and_then(|v| ini::parse_bool(v)),
        fit: keys.get("Fit").and_then(|v| ini::parse_bool(v)),
        prefill_assistant: keys
            .get("PrefillAssistant")
            .and_then(|v| ini::parse_bool(v)),
        log_verbosity: keys.get("LogVerbosity").and_then(|v| ini::parse_int(v)),
        opencode_base_url: opt_nonblank(keys.get("OpencodeBaseUrl").cloned()),
        opencode_api_key: opt_nonblank(keys.get("OpencodeApiKey").cloned()),
    }
}

/// `Some(s)` unless `s` is blank (whitespace only), in which case `None`. The
/// single home for the "blank means unset" rule EVERY optional string field
/// shares on both `load()` and the CLI `server set` — a hand-edited
/// `Hostname =` line must load as unset (falling back to the default), not as
/// `Some("")`.
pub fn opt_nonblank(s: Option<String>) -> Option<String> {
    s.filter(|v| !v.trim().is_empty())
}

// ── Save & render (INI write) ────────────────────────────────────────────

pub fn save(cfg: &ServerConfig) -> io::Result<()> {
    validate_for_save(cfg)?;
    let models_dir = cfg.models_dir_or_default();
    let path = paths::server_ini();
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    // models_dir_or_default() never returns blank (it falls back to the
    // home-derived default), so this is unconditionally safe to create.
    let _ = fs::create_dir_all(&models_dir);
    ini::atomic_write(&path, &render(cfg))
}

/// Save-boundary validation, pure so the unit test never touches `paths::` —
/// the mirror of `presets::validate_for_save`. The INI format can't escape
/// `;`/`#`, so a ModelsDir containing one would silently reload truncated
/// (see `ini::reject_comment_markers`). OverrideTensor gets the same treatment
/// (a custom regex is free-text, so it CAN hold a `#`) plus its own grammar
/// check, for the reason spelled out on `presets::validate_for_save`: a rule
/// with no device is a `throw` during llama.cpp's arg parsing. Here it is worse
/// than there — the router parses `-ot` for the whole SERVER, so a bad value
/// means nothing starts at all, not just one model.
fn validate_for_save(cfg: &ServerConfig) -> io::Result<()> {
    ini::reject_comment_markers("ModelsDir", &cfg.models_dir_or_default())?;
    let overrides = cfg.override_tensor.clone().unwrap_or_default();
    ini::reject_comment_markers("OverrideTensor", &overrides)?;
    crate::tensor_override::validate(&overrides)
        .map_err(|e| io::Error::new(io::ErrorKind::InvalidInput, e))?;
    ini::reject_comment_markers("OpencodeBaseUrl", cfg.opencode_base_url.as_deref().unwrap_or(""))?;
    ini::reject_comment_markers("OpencodeApiKey", cfg.opencode_api_key.as_deref().unwrap_or(""))?;
    Ok(())
}

/// Render the whole server.ini body. Pure (no IO) — the file-writing wrapper is
/// `save()`; the round-trip test drives this directly, mirroring
/// `presets::render_section`.
fn render(cfg: &ServerConfig) -> String {
    let hostname = cfg.hostname_or_default();
    let mlock = cfg.mlock_or_default();

    let models_dir = cfg.models_dir_or_default();

    // Each optional field renders as either `Key = value` or, when unset/default,
    // a commented `; Key = example  ; help` hint line (see int_line_or_hint /
    // str_line_or_hint). The `keep` predicate is where the per-field "is this
    // worth writing?" rule lives (n > 0, n != 1, …).
    // Port collapses to a commented hint when unset (the UI "default" checkbox),
    // so None round-trips as None instead of a forced 8080 — otherwise "default"
    // would silently re-materialize as an explicit port on the next reload, and
    // the launch line would keep passing --port 8080.
    let port_line = int_line_or_hint(
        cfg.port,
        "Port",
        "; Port = 8080  ; TCP port llama-server binds (--port); commented = llama.cpp default 8080",
        |n| n > 0,
    );
    let threads_line = int_line_or_hint(
        cfg.threads,
        "Threads",
        "; Threads = 12  ; optional override; auto-detected if commented",
        |n| n > 0,
    );
    let cache_reuse_line = int_line_or_hint(
        cfg.cache_reuse,
        "CacheReuse",
        "; CacheReuse = 256  ; minimum chunk size for prompt cache reuse (--cache-reuse)",
        |n| n > 0,
    );
    let threads_batch_line = int_line_or_hint(
        cfg.threads_batch,
        "ThreadsBatch",
        "; ThreadsBatch = 12  ; optional override; auto-detected if commented",
        |n| n > 0,
    );
    // Any non-negative value is an explicit choice now that the UI carries the
    // unset state in a "default" checkbox (0 = unlimited is a real value); only
    // None collapses to the hint. The old `n != 1` treated 1 as "the default",
    // a stale remnant of when the launch path forced --models-max 1.
    let models_max_line = int_line_or_hint(
        cfg.models_max,
        "ModelsMax",
        "; ModelsMax = 2  ; cap resident models (0 = unlimited); commented = llama.cpp default 4",
        |n| n >= 0,
    );
    let device_line = str_line_or_hint(
        cfg.device.as_deref(),
        "Device",
        "; Device = ROCm1,CUDA0  ; the GPUs models run on (--device), in split order; blank = all detected",
    );
    let split_mode_line = str_line_or_hint(
        cfg.split_mode.as_deref(),
        "SplitMode",
        "; SplitMode = layer  ; multi-GPU split (--split-mode): none|layer|row; blank = layer (default)",
    );
    let tensor_split_line = str_line_or_hint(
        cfg.tensor_split.as_deref(),
        "TensorSplit",
        "; TensorSplit = 3,1  ; how much of the model each Device holds (--tensor-split), same order; blank = by free VRAM",
    );
    let override_tensor_line = str_line_or_hint(
        cfg.override_tensor.as_deref(),
        "OverrideTensor",
        r"; OverrideTensor = token_embd\.weight=ROCm0  ; put matching tensors on a named device (--override-tensor); blank = none",
    );
    let mmproj_device_line = str_line_or_hint(
        cfg.mmproj_device.as_deref(),
        "MmprojDevice",
        "; MmprojDevice = ROCm1  ; GPU for the image encoder (env MTMD_BACKEND_DEVICE); blank = first GPU found",
    );
    let opencode_base_url_line = str_line_or_hint(
        cfg.opencode_base_url.as_deref(),
        "OpencodeBaseUrl",
        "; OpencodeBaseUrl = https://llm.example.com  ; base URL for integrations (opencode.json + Claude Code); /v1 appended automatically; blank = auto from host+port",
    );
    let opencode_api_key_line = str_line_or_hint(
        cfg.opencode_api_key.as_deref(),
        "OpencodeApiKey",
        "; OpencodeApiKey = sk-xxx  ; API key for proxy/gateway authentication (opencode.json apiKey + Claude Code); blank = none",
    );

    let mlock_lit = if mlock { "true" } else { "false" };
    // Always written like Mlock (a bool with a framework default), so an unset
    // one reloads as the explicit default rather than None.
    let no_mmap_lit = if cfg.no_mmap_or_default() {
        "true"
    } else {
        "false"
    };
    let webui_lit = if cfg.webui_mcp_proxy_or_default() {
        "true"
    } else {
        "false"
    };
    let fit_lit = if cfg.fit_or_default() {
        "true"
    } else {
        "false"
    };
    let prefill_lit = if cfg.prefill_assistant_or_default() {
        "true"
    } else {
        "false"
    };
    let log_verbosity = cfg.log_verbosity_or_default();

    format!(
        "; Generated by llama-cpp-config.
;
; llama-server runtime configuration (machine-wide).
; Per-model knobs live in presets.ini.

[Server]
{port_line}
Hostname = {hostname}
Mlock = {mlock_lit}
; NoMmap: read the weights into RAM instead of memory-mapping the GGUF
; (--no-mmap). Off by default (llama.cpp mmaps). Slower to load and costs the
; model's size in RAM, but the pages can't be dropped back to the file.
NoMmap = {no_mmap_lit}
; WebuiMcpProxy: enable the built-in web UI's MCP CORS proxy (--webui-mcp-proxy).
; The bundled chat UI needs it to call MCP tools; disable on an untrusted network.
WebuiMcpProxy = {webui_lit}
; Fit: let llama.cpp auto-shrink unset args to fit device memory (-fit on|off).
; Off by default — a per-preset offload-all-layers choice would be overridden.
Fit = {fit_lit}
; PrefillAssistant: when a request's LAST message is an assistant message, treat
; it as the start of the reply and continue it (--prefill-assistant, llama.cpp's
; default) instead of answering it as a finished turn (--no-prefill-assistant).
PrefillAssistant = {prefill_lit}
; LogVerbosity: llama-server log threshold (-lv). llama.cpp defines six levels:
; 0 output, 1 error, 2 warning, 3 info, 4 trace, 5 debug — a message is printed
; when its level is <= this. Framework default 4 captures per-request logging
; into logs/llama-server.log; 5 also logs the per-tensor override lines.
LogVerbosity = {log_verbosity}
{threads_line}
{cache_reuse_line}
{threads_batch_line}
{models_max_line}
; GPU distribution. Device = the GPUs models run on; TensorSplit = how much of
; the model each of them holds, positional over Device IN THAT ORDER. Set here,
; both OVERRIDE every preset's own values — llama-server's router passes its own
; command line on top of each preset.
{device_line}
{split_mode_line}
{tensor_split_line}
; OverrideTensor: pin tensors matching a regex to a named device, whatever their
; layer got (--override-tensor). `token_embd\\.weight=ROCm0` is the one that pays:
; llama.cpp parks the embedding table in PINNED host RAM even at 'offloaded N/N
; layers', which Windows counts as Shared GPU memory. Set here, this REPLACES
; every preset's own rules (it does not add to them), and an unknown device name
; stops the SERVER from starting, not just one model.
{override_tensor_line}
; MmprojDevice does NOT follow Device: llama.cpp puts the image encoder on the
; first GPU backend it finds, where it holds VRAM but only computes on image
; requests. Name a device here to move it (e.g. onto the model's own GPU).
{mmproj_device_line}
; OpencodeBaseUrl overrides the auto-derived integration URL (host:port/v1).
; OpencodeApiKey is the provider's API key for authentication.
; Both are useful when exposing llama-server through a reverse proxy or LLM gateway.
{opencode_base_url_line}
{opencode_api_key_line}

; ModelsDir: root directory. Models are scanned from ModelsDir/models/, mmproj
; projection files from ModelsDir/mmprojs/, MTP/draft heads from ModelsDir/mtps/,
; DFlash drafters from ModelsDir/dflashs/.
ModelsDir = {models_dir}
"
    )
}

/// `Key = n` when `keep(n)` holds for a set value, else the commented `hint`.
fn int_line_or_hint(v: Option<i32>, key: &str, hint: &str, keep: impl Fn(i32) -> bool) -> String {
    match v {
        Some(n) if keep(n) => format!("{key} = {n}"),
        _ => hint.to_string(),
    }
}

/// `Key = value` for a non-blank string, else the commented `hint`.
fn str_line_or_hint(v: Option<&str>, key: &str, hint: &str) -> String {
    match v.map(str::trim) {
        Some(s) if !s.is_empty() => format!("{key} = {s}"),
        _ => hint.to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// `render` → parse-back through the real INI reader — the guard for steps
    /// 2–3 of the field recipe (a key-name typo between `from_keys` and the
    /// writer, or a wrong `keep` predicate, fails here).
    fn round_trip(cfg: &ServerConfig) -> ServerConfig {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("server.ini");
        std::fs::write(&path, render(cfg)).unwrap();
        from_keys(&ini::read_section(&path, "Server"))
    }

    #[test]
    fn rich_config_round_trips_through_ini() {
        // Values chosen to dodge the deliberate collapses (see the test below):
        // every `keep` predicate holds, so everything must come back verbatim.
        let original = ServerConfig {
            port: Some(8081),
            hostname: Some("0.0.0.0".into()),
            mlock: Some(false),
            no_mmap: Some(true),
            threads: Some(12),
            cache_reuse: Some(256),
            threads_batch: Some(24),
            models_max: Some(2),
            models_dir: Some(r"E:\llm".into()),
            device: Some("ROCm1,CUDA0".into()),
            split_mode: Some("row".into()),
            tensor_split: Some("3,1".into()),
            // Two rules, so the `,` that joins them has to survive the INI reader
            // — the value's own grammar, not just the key name, is under test.
            override_tensor: Some(r"token_embd\.weight=ROCm1,^output\.weight=CPU".into()),
            mmproj_device: Some("ROCm1".into()),
            webui_mcp_proxy: Some(false),
            fit: Some(true),
            // Non-default (llama.cpp prefills by default), so the round-trip is
            // not vacuous for it.
            prefill_assistant: Some(false),
            log_verbosity: Some(2),
            opencode_base_url: Some("https://llm.example.com".into()),
            opencode_api_key: Some("sk-proxy-key-123".into()),
        };
        assert_eq!(round_trip(&original), original);
    }

    // The writer's DELIBERATE collapses, pinned as documented behavior:
    // - Hostname/Mlock/NoMmap/WebuiMcpProxy/Fit/LogVerbosity are always written,
    //   so an unset one reloads as the framework default (localhost / true /
    //   false / true / false / 4) rather than None.
    // - `keep` predicates turn unset/"not worth writing" values into commented
    //   hint lines: port <= 0, threads/threads_batch <= 0, models_max unset.
    //   An unset port reloads as None (not a forced 8080), so the UI "default"
    //   checkbox round-trips and the launch line omits --port.
    // - ModelsDir falls back to default_models_dir() when unset.
    #[test]
    fn default_config_reloads_as_explicit_defaults() {
        let reloaded = round_trip(&ServerConfig::default());
        assert_eq!(reloaded.port, None);
        assert_eq!(reloaded.hostname.as_deref(), Some("localhost"));
        assert_eq!(reloaded.mlock, Some(true));
        assert_eq!(reloaded.no_mmap, Some(false));
        assert_eq!(reloaded.webui_mcp_proxy, Some(true));
        assert_eq!(reloaded.fit, Some(false));
        assert_eq!(reloaded.prefill_assistant, Some(true));
        assert_eq!(reloaded.log_verbosity, Some(4));
        assert_eq!(reloaded.threads, None);
        assert_eq!(reloaded.threads_batch, None);
        assert_eq!(reloaded.cache_reuse, None);
        assert_eq!(reloaded.models_max, None);
        assert_eq!(reloaded.device, None);
        assert_eq!(reloaded.split_mode, None);
        assert_eq!(reloaded.tensor_split, None);
        assert_eq!(reloaded.override_tensor, None);
        assert_eq!(reloaded.mmproj_device, None);
        assert_eq!(reloaded.models_dir, Some(default_models_dir()));
    }

    #[test]
    fn not_worth_writing_values_collapse_to_hints() {
        let cfg = ServerConfig {
            port: Some(0),
            threads: Some(0),
            cache_reuse: Some(-3),
            threads_batch: Some(-4),
            device: Some("   ".into()),
            ..Default::default()
        };
        let reloaded = round_trip(&cfg);
        assert_eq!(
            reloaded.port, None,
            "port <= 0 is unset (llama.cpp default)"
        );
        assert_eq!(reloaded.threads, None, "threads <= 0 is auto");
        assert_eq!(
            reloaded.cache_reuse, None,
            "cache_reuse <= 0 clears the override (same rule as the CLI set)"
        );
        assert_eq!(reloaded.threads_batch, None, "threads_batch <= 0 is auto");
        assert_eq!(reloaded.device, None, "blank device is unset");
    }

    // The "default" checkbox carries the unset state now, so every non-negative
    // models_max is an explicit value that must persist — including 1 (which the
    // old `n != 1` predicate wrongly swallowed) and 0 (unlimited).
    #[test]
    fn explicit_models_max_round_trips_including_one_and_zero() {
        for n in [0, 1, 8] {
            let reloaded = round_trip(&ServerConfig {
                models_max: Some(n),
                ..Default::default()
            });
            assert_eq!(reloaded.models_max, Some(n), "models_max {n} must persist");
        }
    }

    // Pure validation (no IO), the server-side mirror of presets'
    // save_validation_rejects_comment_markers_in_path_fields: a `;`/`#` in
    // ModelsDir would silently reload truncated through the INI comment rule,
    // so save must refuse it.
    #[test]
    fn save_validation_rejects_comment_markers_in_models_dir() {
        // Explicit clean dir: a default config would derive ModelsDir via
        // paths::home_dir(), which unit tests must stay away from (mod.rs).
        let with = |dir: &str| ServerConfig {
            models_dir: Some(dir.into()),
            ..Default::default()
        };
        assert!(validate_for_save(&with(r"E:\llm")).is_ok());
        for hostile in [r"C:\Models #1", r"C:\a;b"] {
            let err = validate_for_save(&with(hostile)).expect_err(hostile);
            assert!(
                err.to_string().contains("ModelsDir"),
                "error names the field"
            );
        }
    }

    // OverrideTensor is refused on the same two grounds as the per-preset one —
    // and the stakes are higher here: the ROUTER parses this `-ot`, so a value
    // llama.cpp throws on takes down the whole server, not one model. The regex
    // is free text, so `#`/`;` really can reach it (they'd truncate the INI line
    // on reload); a rule with no device is what a hand-edited `,` leaves behind.
    #[test]
    fn save_validation_rejects_a_broken_override_tensor() {
        let with = |ot: &str| ServerConfig {
            models_dir: Some(r"E:\llm".into()),
            override_tensor: Some(ot.into()),
            ..Default::default()
        };
        assert!(validate_for_save(&with(r"token_embd\.weight=ROCm0")).is_ok());
        assert!(validate_for_save(&with("")).is_ok(), "unset is fine");

        let err = validate_for_save(&with(r"token_embd\.weight=ROCm0,dangling")).expect_err("rule");
        assert!(
            err.to_string().contains("dangling"),
            "quotes the rule: {err}"
        );

        let err = validate_for_save(&with("blk#1=CPU")).expect_err("comment marker");
        assert!(
            err.to_string().contains("OverrideTensor"),
            "error names the field: {err}"
        );
    }

    // client_host is what the Open-chat button and the Integrations base URL
    // use: a concrete bind address passes through, but the all-interfaces
    // listen address 0.0.0.0 is not client-connectable and maps to localhost.
    #[test]
    fn client_host_maps_wildcard_bind_to_localhost() {
        let with = |h: &str| ServerConfig {
            hostname: Some(h.into()),
            ..Default::default()
        };
        assert_eq!(with("0.0.0.0").client_host(), "localhost");
        assert_eq!(with("192.168.1.5").client_host(), "192.168.1.5");
        assert_eq!(with("localhost").client_host(), "localhost");
        assert_eq!(ServerConfig::default().client_host(), "localhost");
    }

    // OpencodeBaseUrl and OpencodeApiKey are free-text fields written to
    // server.ini — like OverrideTensor, a comment marker would reload truncated.
    #[test]
    fn save_validation_rejects_comment_markers_in_integration_fields() {
        let with_key = |k: &str| ServerConfig {
            models_dir: Some(r"E:\llm".into()),
            opencode_api_key: Some(k.into()),
            ..Default::default()
        };
        let with_url = |u: &str| ServerConfig {
            models_dir: Some(r"E:\llm".into()),
            opencode_base_url: Some(u.into()),
            ..Default::default()
        };
        assert!(validate_for_save(&with_key("sk-abc123")).is_ok());
        assert!(validate_for_save(&with_url("https://gw.example.com")).is_ok());

        let err = validate_for_save(&with_key("sk#bad")).expect_err("comment marker");
        assert!(err.to_string().contains("OpencodeApiKey"), "{err}");

        let err = validate_for_save(&with_url("https://gw;bad")).expect_err("comment marker");
        assert!(err.to_string().contains("OpencodeBaseUrl"), "{err}");
    }
}
