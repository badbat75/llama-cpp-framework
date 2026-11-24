//! server.ini schema and IO for the Private AI Framework.
//!
//! `save()` (the configurator's `server_cfg.rs`) rewrites the whole FILE from
//! a `ServerConfig` (server.ini is fully generated; unlike models.ini,
//! hand-added content outside the template does not survive a save). Unset
//! optional fields are emitted as commented `; Key = example  ; help` hint
//! lines (never omitted) so the file self-documents every available knob.
//!
//! ADD A SERVER FIELD, the mirror of the model profile recipe (top of model_profiles.rs): a
//! smaller schema, but a LARGER per-field fan-out; it also reaches the CLI (the
//! server has a per-field `set`) and the launch path (steps 7-8 below). Trace an
//! existing field like `threads` (PascalCase INI key ↔ snake_case Rust field):
//!   1. `ServerConfig` struct field (+ doc)   : below
//!   2. `from_keys` (backs `load`)            : INI read; `keys.get("Key")` → field
//!   3. `render` (backs `save`)               : INI write; an `int_line_or_hint` /
//!      `str_line_or_hint` line + a `{…_line}` slot in the `[Server]` body
//!   4. `ServerForm` struct                   : ui/types.slint (a NUMERIC field
//!      rides as a `string` plus a paired `<field>_default` bool, the "omit the
//!      flag" checkbox)
//!   5. the input widget                      : ui/server_page.slint, bind two-way
//!      `<=>`: DefaultLineEdit for numerics (`input_type: InputType.number` for an
//!      integer; wire BOTH `value` and `default`). Never a SpinBox; it edits itself
//!      on a stray mouse-wheel, and `binding_lint`'s `no_spinbox_widgets_anywhere`
//!      fails the build if one returns
//!   6. `config_to_form` + `form_to_config`   : src/server_form.rs (BOTH
//!      directions; derive `<field>_default` via `is_none()` / `if <field>_default`).
//!      An INTEGER text field also joins `server_form::int_fields` with the range
//!      `form_to_config` reads it with, so Save refuses a mistyped value instead
//!      of writing it as unset. Nothing fails if you skip this one
//!   7. THREE spots in src/cli.rs             : the `ServerSet` flag field, a
//!      `row(...)` in `show_lines`, and `ServerSet::apply` copying the flag into `cfg`
//!   8. `launch::server_args`                 : map the field to its llama-server
//!      flag (or wave it through with a comment if it's launch-env only, like
//!      ModelsDir → LLAMA_CACHE in `start()`)
//!   9. PATH-VALUED field only: add it to `validate_for_save` below (and extend
//!      `save_validation_rejects_comment_markers_in_models_dir`), because the
//!      INI format can't escape `;`/`#`, so an unvalidated path saves fine and
//!      reloads TRUNCATED. Like the widget, nothing fails if you skip this.
//!
//! Guards: the save→load round-trip test in this file (steps 2–3: a key-name typo
//! or wrong `keep` rule fails it), the form round-trip in server_form.rs
//! (`form_to_config(config_to_form(c)) == c`, step 6), cli.rs's
//! `server_set_apply_copies_every_field` + `show_lines_prints_every_field`
//! (step 7: the first is airtight, whole-struct equality against an exhaustive
//! literal; the second's destructure only forces the BIND, so extend its manual
//! `needles` array too, or the Show row goes unguarded), and runstate's
//! `server_args_covers_every_config_field` (step 8: its exhaustive destructure
//! breaks compilation until the launch path consumes the field). Give the new
//! field a NON-DEFAULT value when extending the rich fixtures: `None` satisfies
//! the compiler but makes every round-trip vacuous for that field.

use std::io;

use crate::checks;
use crate::ini;
use crate::paths;

// ── Schema: ServerConfig ─────────────────────────────────────────────────
#[derive(Debug, Clone, Default, PartialEq)]
pub struct ServerConfig {
    pub port: Option<i32>,
    pub hostname: Option<String>,
    pub threads: Option<i32>,
    pub threads_batch: Option<i32>,
    pub models_max: Option<i32>,
    /// MiB of memory the router keeps free on every device
    /// (`--models-memory-margin`, llama.cpp v0.8.0): before loading a model it
    /// unloads the least recently used ones until the new one fits. `None` omits
    /// the flag, i.e. llama.cpp's default 0, which disables the accounting and
    /// leaves `models_max` as the only cap.
    pub models_memory_margin: Option<i32>,
    /// The GPUs of this server profile, as `[Server] Devices`: the probe's
    /// ids in order comma-separated (`ROCm0,ROCm1`), or `none` for CPU only.
    /// `None` is every GPU, the state today. R2 (ARCHITECTURE.md "Resources
    /// belong to the server profile").
    pub devices: Option<String>,
    pub models_dir: Option<String>,
    /// Which GEMM backend rocBLAS uses under the HIP build (env var
    /// `ROCBLAS_USE_HIPBLASLT`): `Some(false)` exports `0` (force Tensile),
    /// `Some(true)` exports `1` (force hipBLASLt), `None` leaves the variable
    /// unset so rocBLAS routes as it sees fit. A TRI-state, not a bool, for the
    /// same reason `--flash-attn` is one: "don't set it" is a third instruction,
    /// and it is the one the framework defaults to.
    ///
    /// It has no llama-server flag at all: it is read by
    /// rocBLAS itself, so it can only ride the environment (`runstate::env_vars`,
    /// exported by `start()` and shown in the Command Line card; the router's
    /// per-model children inherit it). Nothing else reads it, so it is inert on a
    /// CUDA or Vulkan build.
    ///
    /// The state that pays is OFF. On gfx1201 (Radeon AI PRO R9700) under
    /// ROCm/TheRock 7.14, rocBLAS's default routing sends 16-bit GEMMs through
    /// hipBLASLt, and the SECOND 16-bit GEMM in a process fails at kernel
    /// launch; f32 is immune, and the identical call sequence passes on Tensile
    /// (standalone repro: default and `=1` fail, `=0` passes for
    /// batched/strided/plain × bf16/f16). llama-server aborts on the first
    /// prefill batch of any BF16/F16 model. Forcing Tensile is the cure: its
    /// prefill is far ahead of the older
    /// `GGML_CUDA_CUBLAS_COMPUTE_TYPE=f32` workaround. Upstream: llama.cpp
    /// #25836, ROCm #6461, TheRock #7271.
    ///
    /// What fails is a MIX, not a ROCm version, and that is where the other cure
    /// comes from. Windows resolves a static import from the exe's own directory
    /// and then System32, both BEFORE PATH, and the AMD driver keeps its own
    /// `amdhip64_7.dll` in System32; so llama-server runs the dist's
    /// `rocblas.dll` + `libhipblaslt.dll` against the DRIVER's HIP runtime,
    /// never the dist's own. Copy the dist's runtime next to the exe and the
    /// same GEMMs pass on 7.14.0, 10.0.0 and nightly 10.1.0a20260901 alike, with
    /// `=1` forced as well, so hipBLASLt itself is healthy (measured 2026-09-01
    /// as a same-directory A/B with that file as the only variable, and
    /// end-to-end on `test-backend-ops test -o MUL_MAT -b ROCm0 -p type_a=bf16`).
    /// Hence `xtask package` bundles the dist's whole runtime into `bin\`,
    /// `amdhip64_*.dll` included, so an installed tree needs no knob here at
    /// all (a build from source still runs against `HIP_PATH\bin` over PATH,
    /// i.e. the driver's runtime). The bundle also carries the dist's
    /// `amd_comgr*.dll`: the runtime asks for it by bare name and System32
    /// keeps the DRIVER's own (a different build that precedes PATH), a mix
    /// that stood until an iGPU appeared and then killed `hipGetDeviceCount`
    /// outright, dropping the whole HIP backend (2026-09-03).
    ///
    /// Left unset by default all the same: the bug is per-arch, whether Tensile
    /// costs speed is unmeasured (the R9700 guide), and the runtime finding rests on one
    /// driver (32.0.22042.14002) on one box.
    pub rocblas_use_hipblaslt: Option<bool>,
    /// Paged host memory as the driver's GTT, exported as the pair
    /// `HSA_USERPTR_FOR_PAGED_MEM=0` and `HSA_SVM_FOR_PAGED_MEM=0`: with both at
    /// `0` the ROCm runtime maps that memory through the driver's GTT
    /// (libhsakmt `fmm.c` `fmm_allocate_host_gpu`), so compaction and reclaim no
    /// longer evict the process's GPU queues. Unset or `true` exports both,
    /// `false` exports neither and the runtime's own defaults return. Why a way
    /// to turn it off exists: that memory stops being swappable and the GTT is
    /// a capped window over the RAM (the cap, the design and the measures:
    /// `ARCHITECTURE.md` "ROCm host memory as GTT"), so a large pinned host
    /// allocation can fail where it used to succeed. Read by libhsakmt, below
    /// llama.cpp, so it rides `launch::env_vars` like the knobs around it, and
    /// goes out for every backend (one rule, no backend detection: CUDA, Vulkan
    /// and Metal never read it).
    pub rocm_host_memory_gtt: Option<bool>,
    /// Which AllReduce `--split-mode tensor` reduces through, exported as
    /// `GGML_CUDA_ALLREDUCE`: one of [`ALLREDUCE_MODES`]. None = never export
    /// the variable, which is the backend's own default: on Linux the
    /// collective library when one is linked (RCCL in ggml-hip; NCCL in
    /// ggml-cuda only on a build that found it), then ggml's internal
    /// AllReduce; elsewhere the internal one. Not a llama-server flag: ggml-cuda
    /// reads it with `getenv` when a tensor split is set up (`ggml-cuda.cu`
    /// `ggml_backend_cuda_comm_init`), hence `env_vars`, like the knob above.
    ///
    /// ONE name for both backends: ggml-hip is the same source, so it reads
    /// `GGML_CUDA_ALLREDUCE` too, and a value cannot be given to CUDA and
    /// another to ROCm. Server-wide for the same reason the env is: it reaches
    /// every router child. Inert outside the tensor mode.
    ///
    /// `none` is not only a slower choice, it is the one value under which a
    /// split MIXING the two backends loads at all. ggml-cuda and ggml-hip
    /// declare the same backend guid, so each takes the other's device for one
    /// of its own, brings its direct AllReduce up over the pair and aborts on
    /// the first reduction (`ROCm error: invalid argument` in
    /// `ggml_cuda_ar_allreduce`, measured 2026-10-13 on v0.6.0 with an R9700 +
    /// an RTX 4070 Super, under the default and under `internal` alike). With
    /// `none` the meta backend's generic reduction runs instead: on that pair
    /// prefill falls well behind the layer split and decode does not move
    /// (`docs/amd_ai_pro_r9700.md` section 4).
    ///
    /// An unknown value is NOT refused by the backend: it warns and behaves as
    /// `none`, so a typo would silently cost the fast path. Hence the closed
    /// list here and `normalize_allreduce` on read.
    pub cuda_allreduce: Option<String>,
    /// Export `GGML_CUDA_DISABLE_GRAPHS=1`: the CUDA and the ROCm backend stop
    /// capturing the decode step into a CUDA / HIP graph and launch every kernel
    /// one by one. A DIAGNOSTIC switch, never a tuning one: it costs decode speed
    /// (graphs exist to amortize the launch overhead), and it is what to try
    /// first when decode crashes or misbehaves while prefill is fine, since the
    /// graph path is only taken for small batches. ggml tests the variable's
    /// PRESENCE (`getenv(...) != nullptr`, ggml-cuda/common.cuh), so even `=0`
    /// would disable graphs: hence a plain bool that exports `1` or nothing,
    /// never a tri-state. Inert on a Vulkan build.
    pub cuda_disable_graphs: bool,
    /// Export `LLAMA_ATTN_ROT_DISABLE=1`: llama.cpp stops applying the Hadamard
    /// rotation it puts on a QUANTIZED K/V cache (`llama-kv-cache.cpp`, logged
    /// as a warning at load). The rotation spreads outliers before quantizing,
    /// which is what keeps a q8_0 / q4_0 cache close to f16 quality; switching it
    /// off is for an A/B on quality or speed, or for a backend where the rotation
    /// misbehaves. Inert with an f16/bf16 cache and on heads not a multiple of
    /// 64; the DeepSeek-3.2/4, GLM-DSA and dots3note indexers rotate regardless.
    /// Read by llama.cpp itself with `atoi`, hence the `1`, and an env var
    /// because no flag reaches it.
    pub attn_rot_disable: bool,
    /// Export `ROCP_TOOL_ATTACH=1`: lets `rocprofv3 --attach <pid>` (ROCm's
    /// profiler, shipped in the TheRock dist) attach a kernel trace to the
    /// running llama-server. rocprofiler-sdk's process-attachment docs require
    /// it of the TARGET process (unless its `rocprofiler-register` was built
    /// with default attachment); without it the attach is refused. Read by the
    /// ROCm runtime, so inert on CUDA and Vulkan. Off (nothing exported) by
    /// default: it is a profiling switch, not a tuning one.
    pub rocprof_attach: bool,
    /// Serve the built-in web UI (the chat page at `/`): `Some(false)` launches
    /// with `--no-ui`, `Some(true)` and None pass nothing, since upstream's
    /// default is enabled (`common/arg.cpp`, `--ui` / `--no-ui`, env
    /// `LLAMA_ARG_UI`). A plain bool with a materialized default like
    /// `webui_mcp_proxy`, not a tri-state: "default" and "on" would launch the
    /// same line, so a third entry would name no third behavior.
    ///
    /// The ROUTER serves the page: `server_http_context::init` (server-http.cpp)
    /// registers `/` and the embedded assets only when `params.ui` holds, and
    /// main() runs it on the router's own params before any model loads, so the
    /// flag belongs on the router's line. It also reaches every child through
    /// `base_preset` (`LLAMA_ARG_UI` is not in `unset_reserved_args`), which is
    /// harmless: a child listens on a loopback port nobody browses. Off, `/`
    /// answers 404 and upstream serves a `sw.js` that unregisters the browser's
    /// cached UI; the API (`/v1/*`, `/models`, `/health`, `/slots`) is untouched,
    /// so the Integrations tab, the benchmark and the snapshots keep working.
    ///
    /// Off also drops `--webui-mcp-proxy` from the launch (the proxy exists only
    /// for the UI's MCP calls) without clearing `webui_mcp_proxy`, and the GUI
    /// greys out every Open chat UI entry while the RUNNING server has no UI.
    pub webui: Option<bool>,
    /// Enable the built-in web UI's MCP CORS proxy (--webui-mcp-proxy).
    /// None = off, llama.cpp's default: the proxy fetches any URL for whoever can
    /// reach the server. The bundled chat UI uses it for an MCP server the user
    /// marks "Use llama-server proxy". Experimental upstream.
    pub webui_mcp_proxy: Option<bool>,
    /// llama-server log verbosity threshold (-lv / --log-verbosity N): messages
    /// above this level are dropped. None = the framework default (4, per-request
    /// logging into the captured llama-server.log). Always passed to the launch.
    /// llama.cpp defines exactly six levels: 0 output, 1 error, 2 warning,
    /// 3 info, 4 trace, 5 debug (`common/arg.cpp`, `common/log.h`), which is why
    /// the GUI offers them as a dropdown rather than a free number.
    pub log_verbosity: Option<i32>,
    /// Dump each loaded model's KV cache to disk when the server stops, and hand
    /// the newest one back on the next start (`--slot-save-path` plus the
    /// `/slots/{id}?action=save|restore` calls the framework makes around the
    /// launch; the mechanism and its two upstream surprises are documented in
    /// `slot_state`). None = the framework default (off).
    ///
    /// What it buys is the one cost no amount of VRAM tuning touches: prefill is
    /// QUADRATIC in the prompt length, and the KV cache is process memory, so a
    /// restart re-reads the whole conversation from zero, where reloading the
    /// same state is only sequential IO (`crates/config/README.md`,
    /// "Conversation snapshots", has the measured cost).
    ///
    /// Off by default because it is not free: each snapshot is roughly one byte
    /// on disk per byte of KV cache, the stop path waits for the write, and the
    /// start path loads the model eagerly to push the state back in. A machine
    /// running short prompts pays all of that for nothing.
    pub save_state_on_shutdown: Option<bool>,
    /// Where those snapshots live (`--slot-save-path`). None/blank =
    /// `slot_state::default_state_dir()`, a `state\` sibling of `config\` and
    /// `logs\` under the user's runtime root.
    ///
    /// It is overridable rather than fixed for one reason: the default sits on
    /// the SYSTEM drive and these files are large. The size follows the tokens
    /// the slot actually HOLDS, not `ctx-size`: the KV cache is dumped for the
    /// conversation, not for the allocated context (size per token:
    /// `docs/qwen3.8_27b.md` section 1). Since v0.7.0 the file also carries the
    /// context checkpoints and the draft state (`slot_state` module header).
    /// A full context is the ceiling to plan disk against, never a fixed cost of enabling
    /// the feature.
    /// A user whose models already live on another disk will want the dumps
    /// there too, and a default that fills C:\ with no way out would be a poor
    /// one.
    ///
    /// The directory must EXIST before the launch, which is why
    /// `runstate::start` creates it: llama.cpp validates the path while parsing
    /// the flag (`fs_is_directory` or it throws "not a directory"), so a missing
    /// directory does not degrade to "no snapshots", it stops the server from
    /// starting at all.
    pub state_dir: Option<String>,
    /// Explicit override for the integration base URL (opencode.json + Claude Code
    /// snippet). When set, this value is used instead of auto-deriving from
    /// hostname + port. Useful for reverse-proxy setups where the client-facing
    /// URL differs from the server bind address. None = auto-derive from server.ini.
    pub opencode_base_url: Option<String>,
    /// API key for the integration provider. Written as `apiKey` in opencode.json
    /// and used by Claude Code env snippet. Useful when the server is exposed
    /// through a proxy or LLM gateway that requires authentication. None = unset.
    pub opencode_api_key: Option<String>,

    /// The tags of this server profile, for the web UI: what the profile is
    /// for (`edge`, `vision`, `v2`), shown next to the instance that uses it.
    /// Not a llama-server flag, never in the environment and never in a
    /// request: metadata, in the file beside the server keys. Written as one
    /// `Tags` line, the list joined with `,`, and only when there is a tag.
    /// The rule is [`checks::tags_rule`], applied to the file by
    /// [`checks::server_file`] and to this form by [`check`], so every write
    /// that runs the checks refuses a list the rule refuses.
    pub tags: Vec<String>,
}

// ── Defaults & accessors ─────────────────────────────────────────────────

/// Every value `-lm` accepts, in the dropdown's order (`common/arg.cpp`'s
/// `--load-mode` help; `enum llama_load_mode` in include/llama.h), read by the
/// per-model-profile `load-mode` key. Mirrors `Options.load_modes` in
/// ui/components.slint after its leading "default" (the omitted key): the e2e
/// (`src/tests/ui_bindings.rs`) asserts the two lists agree, because a value the
/// dropdown does not contain leaves the combo showing its first entry and
/// quietly drops the key on the next save.
pub const LOAD_MODES: [&str; 6] = ["auto", "none", "mmap", "mlock", "mmap+mlock", "dio"];

/// `v` as one of [`LOAD_MODES`], ignoring case and surrounding space; `None` when
/// it names no mode llama.cpp has. Handing back the CANONICAL `&'static str` is the
/// point: a hand-edited `load-mode = MMAP+MLOCK` then shows and re-saves as the
/// dropdown's own entry instead of falling back to "auto" and silently rewriting a
/// setting the user did choose.
pub fn normalize_load_mode(v: &str) -> Option<&'static str> {
    let v = v.trim();
    LOAD_MODES.into_iter().find(|m| m.eq_ignore_ascii_case(v))
}

/// The values `GGML_CUDA_ALLREDUCE` takes (`ggml_backend_cuda_comm_init`,
/// ggml-cuda.cu), passed verbatim. Mirrors `Options.allreduce_modes` in
/// ui/components.slint after its leading "default" (the unset state); the e2e
/// (`src/tests/ui_bindings.rs`) asserts the two lists agree.
pub const ALLREDUCE_MODES: [&str; 3] = ["nccl", "internal", "none"];

/// `v` as one of [`ALLREDUCE_MODES`], ignoring case and surrounding space; `None`
/// for anything else, "default" and a blank included: unset is the only other
/// state, and the backend would read an unknown word as `none`.
pub fn normalize_allreduce(v: &str) -> Option<&'static str> {
    let v = v.trim();
    ALLREDUCE_MODES
        .into_iter()
        .find(|m| m.eq_ignore_ascii_case(v))
}

/// llama-server's own `--port` default (`common_params::port`, common/common.h),
/// which is what it binds when `Port` is unset and the launch omits the flag.
/// It MUST track the bundled llama.cpp: every client URL the configurator
/// builds (health check, Open chat, the live benchmark, opencode.json, the
/// Claude Code snippet) reads it, so a stale value points them all at a port
/// nothing listens on. v0.7.0 moved it from 8080 to 9931 (#30159).
pub const DEFAULT_PORT: i32 = 9931;

/// llama-server's own `--models-max` default (4), what an unset `ModelsMax`
/// yields: the default the configurator's form and the web UI show.
pub const DEFAULT_MODELS_MAX: i32 = 4;

/// llama-server's own `--models-memory-margin` default (0, off), what an unset
/// `ModelsMemoryMargin` yields: the default the form and the web UI show.
pub const DEFAULT_MODELS_MEMORY_MARGIN: i32 = 0;

/// Default ModelsDir when server.ini leaves it unset. ModelsDir is the *root*
/// the four fixed subfolders hang off (models\, mmprojs\, mtps\, dflashs\;
/// see `model_scan`), so this is a bare `~\.llama.cpp`, not `…\models`.
pub fn default_models_dir() -> String {
    paths::home_dir()
        .join(".llama.cpp")
        .to_string_lossy()
        .into_owned()
}

impl ServerConfig {
    /// The configured ModelsDir, or `default_models_dir()` when unset/blank:
    /// the single home for the "blank ModelsDir ⇒ default dir" rule shared by
    /// `save()`, `runstate::start()`, and `server_form::config_to_form`.
    pub fn models_dir_or_default(&self) -> String {
        self.models_dir
            .as_deref()
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .map_or_else(default_models_dir, str::to_string)
    }

    // The always-written fields' defaults (Port / Hostname render as
    // real lines even when unset). ONE home each: `render()`, `server_form`,
    // `launch::server_args`, and the GUI all pull from here, so changing a
    // default is a one-line edit instead of a four-file hunt.

    /// The configured port, or [`DEFAULT_PORT`] when unset.
    pub fn port_or_default(&self) -> i32 {
        self.port.unwrap_or(DEFAULT_PORT)
    }

    /// The configured bind host, or "localhost" when unset/blank. This is what
    /// llama-server LISTENS on (`--host`); for the address clients connect to,
    /// use [`Self::client_host`].
    pub fn hostname_or_default(&self) -> String {
        self.hostname
            .as_deref()
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .map_or_else(|| "localhost".into(), str::to_string)
    }

    /// The web UI switch, or `true` (llama.cpp's default: enabled) when unset.
    pub fn webui_or_default(&self) -> bool {
        self.webui.unwrap_or(true)
    }

    /// The web-UI MCP proxy flag, or `false` (llama.cpp's default) when unset.
    /// The STORED choice: whether the launch passes the flag also depends on
    /// [`Self::webui_or_default`] (`launch::server_args`).
    pub fn webui_mcp_proxy_or_default(&self) -> bool {
        self.webui_mcp_proxy.unwrap_or(false)
    }

    /// The log verbosity (-lv), or `4` (the framework default) when unset.
    pub fn log_verbosity_or_default(&self) -> i32 {
        self.log_verbosity.unwrap_or(4)
    }

    /// Snapshot the slot on shutdown, or `false` (the framework default: off)
    /// when unset.
    pub fn save_state_on_shutdown_or_default(&self) -> bool {
        self.save_state_on_shutdown.unwrap_or(false)
    }

    /// The configured snapshot directory, or `slot_state::default_state_dir()`
    /// when unset/blank. Delegates so the fallback has ONE home, shared by
    /// `render()`, the launch path, the form and the save/restore legs.
    pub fn state_dir_or_default(&self) -> String {
        crate::slot_state::state_dir(self)
            .to_string_lossy()
            .into_owned()
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

    /// The host a CLIENT on this machine should connect to. `Hostname` may be a
    /// comma-separated list (llama.cpp v0.5.0 binds each address), so pick one:
    /// a loopback entry when the list has one, since every client this builds a
    /// URL for runs on this machine, otherwise the first. The all-interfaces
    /// bind `0.0.0.0` maps to `localhost`: it is a listen address, not a
    /// connectable one (Windows refuses it as a destination). Used by the
    /// Open-chat URL and the Integrations base URL.
    pub fn client_host(&self) -> String {
        let hosts = crate::net_ifaces::parse_hosts(&self.hostname_or_default());
        let is_local = |h: &&String| {
            h.eq_ignore_ascii_case("localhost")
                || h.starts_with("127.")
                || h.as_str() == crate::net_ifaces::ALL_INTERFACES
        };
        let host = hosts
            .iter()
            .find(is_local)
            .or(hosts.first())
            .cloned()
            .unwrap_or_else(|| "localhost".into());
        if host == crate::net_ifaces::ALL_INTERFACES {
            "localhost".into()
        } else {
            host
        }
    }
}

// ── Construct & parse (from_keys = INI read) ─────────────────────────────

pub fn load() -> ServerConfig {
    load_from(&paths::server_ini())
}

/// The `[Server]` section of the INI at `path`, read as `load()` reads
/// server.ini: the service launcher reads an instance's `server@<id>.ini`.
pub fn load_from(path: &std::path::Path) -> ServerConfig {
    from_keys(&ini::read_section(path, "Server"))
}

/// The `[Server]` section of INI `text`, read as `load_from` reads a file:
/// the manager checks an instance file before writing it.
pub fn from_text(text: &str) -> ServerConfig {
    let keys = ini::parse_sections(text)
        .into_iter()
        .find(|s| s.id.eq_ignore_ascii_case("Server"))
        .map(|s| s.keys)
        .unwrap_or_default();
    from_keys(&keys)
}

/// The `Tags` key as a list: `a,b`, each entry trimmed, an empty entry (a
/// doubled comma, a trailing one) dropped. No key, no tags.
fn tags_of(keys: &std::collections::BTreeMap<String, String>) -> Vec<String> {
    keys.get("Tags")
        .map(|v| {
            v.split(',')
                .map(str::trim)
                .filter(|s| !s.is_empty())
                .map(String::from)
                .collect()
        })
        .unwrap_or_default()
}

/// Parse the `[Server]` key/value map into a config. Split out of `load()` so
/// the save→load round-trip test can run against `render()`'s output without
/// touching the real config path.
fn from_keys(keys: &std::collections::BTreeMap<String, String>) -> ServerConfig {
    ServerConfig {
        port: keys.get("Port").and_then(|v| ini::parse_int(v)),
        hostname: opt_nonblank(keys.get("Hostname").cloned()),
        threads: keys.get("Threads").and_then(|v| ini::parse_int(v)),
        threads_batch: keys.get("ThreadsBatch").and_then(|v| ini::parse_int(v)),
        models_max: keys.get("ModelsMax").and_then(|v| ini::parse_int(v)),
        models_memory_margin: keys
            .get("ModelsMemoryMargin")
            .and_then(|v| ini::parse_int(v)),
        devices: opt_nonblank(keys.get("Devices").cloned()),
        models_dir: opt_nonblank(keys.get("ModelsDir").cloned()),
        // Absent (or unparseable) stays None: "leave the env var alone" is a
        // real third state here, not a stand-in for a framework default.
        rocblas_use_hipblaslt: keys
            .get("RocblasUseHipblaslt")
            .and_then(|v| ini::parse_bool(v)),
        // Same three states: unset is not a stand-in for a default, it is the
        // state the launch exports the pair from (see the field's doc).
        rocm_host_memory_gtt: keys
            .get("RocmHostMemoryGtt")
            .and_then(|v| ini::parse_bool(v)),
        cuda_allreduce: keys
            .get("CudaAllreduce")
            .and_then(|v| normalize_allreduce(v))
            .map(str::to_string),
        cuda_disable_graphs: keys
            .get("CudaDisableGraphs")
            .and_then(|v| ini::parse_bool(v))
            .unwrap_or(false),
        attn_rot_disable: keys
            .get("AttnRotDisable")
            .and_then(|v| ini::parse_bool(v))
            .unwrap_or(false),
        rocprof_attach: keys
            .get("RocprofAttach")
            .and_then(|v| ini::parse_bool(v))
            .unwrap_or(false),
        webui: keys.get("Webui").and_then(|v| ini::parse_bool(v)),
        webui_mcp_proxy: keys.get("WebuiMcpProxy").and_then(|v| ini::parse_bool(v)),
        log_verbosity: keys.get("LogVerbosity").and_then(|v| ini::parse_int(v)),
        save_state_on_shutdown: keys
            .get("SaveStateOnShutdown")
            .and_then(|v| ini::parse_bool(v)),
        state_dir: opt_nonblank(keys.get("StateDir").cloned()),
        opencode_base_url: opt_nonblank(keys.get("OpencodeBaseUrl").cloned()),
        opencode_api_key: opt_nonblank(keys.get("OpencodeApiKey").cloned()),
        tags: tags_of(keys),
    }
}

/// `Some(s)` unless `s` is blank (whitespace only), in which case `None`. The
/// single home for the "blank means unset" rule EVERY optional string field
/// shares on both `load()` and the CLI `server set`: a hand-edited
/// `Hostname =` line must load as unset (falling back to the default), not as
/// `Some("")`.
pub fn opt_nonblank(s: Option<String>) -> Option<String> {
    s.filter(|v| !v.trim().is_empty())
}

/// The OpenCode base URL as stored: without its trailing `/`s, then `/v1`,
/// then `/`s again (the convention "store without suffix", so a typed `/v1`
/// is never appended twice); blank is unset.
pub fn normalize_base_url(url: &str) -> Option<String> {
    let url = url.trim().trim_end_matches('/');
    let cleaned = url.strip_suffix("/v1").unwrap_or(url).trim_end_matches('/');
    opt_nonblank(Some(cleaned.to_string()))
}

// ── Save & render (INI write) ────────────────────────────────────────────

/// Save-boundary validation, pure so the unit test never touches `paths::`,
/// the mirror of `model_profiles::validate_for_save`. The INI format can't escape
/// `;`/`#`, so a ModelsDir containing one would silently reload truncated
/// (see `ini::reject_comment_markers`).
pub fn validate_for_save(cfg: &ServerConfig) -> io::Result<()> {
    ini::reject_comment_markers("ModelsDir", &cfg.models_dir_or_default())?;
    // Same rule as ModelsDir, and with a second bite here: a truncated StateDir
    // would reload as a path that does not exist, and llama.cpp rejects
    // `--slot-save-path` on a missing directory by refusing to START.
    //
    // Only the EXPLICIT value is checked, not the resolved default: a marker can
    // only arrive by being typed, and rejecting every save on a machine whose
    // %LOCALAPPDATA% happens to contain a `;` would wedge the app over a path
    // the user cannot edit from here. It also keeps this function free of
    // `paths::`, which unit tests must not touch (see src/tests/mod.rs).
    ini::reject_comment_markers("StateDir", cfg.state_dir.as_deref().unwrap_or(""))?;
    ini::reject_comment_markers(
        "OpencodeBaseUrl",
        cfg.opencode_base_url.as_deref().unwrap_or(""),
    )?;
    ini::reject_comment_markers(
        "OpencodeApiKey",
        cfg.opencode_api_key.as_deref().unwrap_or(""),
    )?;
    Ok(())
}

/// The checks a server config must pass before it is written, the same for
/// every writer (the GUI, `pai`, the bff) and every backend (macOS has no
/// manager): the manager's own content checks (`checks::server_file`) on the
/// values as set, and what needs this machine: `Threads` and `ThreadsBatch`
/// from 1 to `cpus`, the logical CPU count. Pure, no `paths::`.
pub fn check(cfg: &ServerConfig, cpus: usize) -> Result<(), checks::Refusal> {
    let ints = [
        ("Port", cfg.port),
        ("ModelsMax", cfg.models_max),
        ("ModelsMemoryMargin", cfg.models_memory_margin),
        ("LogVerbosity", cfg.log_verbosity),
    ];
    for (key, value) in ints {
        if let (Some(v), Some(range)) = (value, checks::server_int_range(key)) {
            checks::whole_number(key, &v.to_string(), &range)?;
        }
    }
    let cpus = 1..=i32::try_from(cpus).unwrap_or(i32::MAX);
    for (key, value) in [
        ("Threads", cfg.threads),
        ("ThreadsBatch", cfg.threads_batch),
    ] {
        if let Some(v) = value {
            checks::whole_number(key, &v.to_string(), &cpus)?;
        }
    }
    let texts = [
        ("Devices", &cfg.devices),
        ("ModelsDir", &cfg.models_dir),
        ("StateDir", &cfg.state_dir),
        ("OpencodeBaseUrl", &cfg.opencode_base_url),
        ("OpencodeApiKey", &cfg.opencode_api_key),
    ];
    for (key, value) in texts {
        if let Some(v) = value {
            checks::comment_markers(key, v)?;
        }
    }
    checks::bind_list(cfg.hostname.as_deref().unwrap_or_default())?;
    if let Some(v) = &cfg.devices {
        checks::device_list("Devices", v)?;
    }
    checks::tags_rule(&cfg.tags)
}

/// Render the whole server.ini body. Pure (no IO): the file-writing wrapper is
/// `save()`; the round-trip test drives this directly, mirroring
/// `model_profiles::render_section`.
pub fn render(cfg: &ServerConfig) -> String {
    render_for(cfg, false)
}

/// `render`, except that with `leave_dirs_unset` an unset or blank ModelsDir
/// or StateDir is written as its commented hint line, so it reloads unset,
/// instead of resolved through `paths::`. For the manager's
/// `server@<id>.ini` (Windows and Linux): an instance whose file leaves them
/// unset uses the engine's folders (`launch::with_instance_defaults`), which
/// the saving user's `~/.llama.cpp` would override. Without it, exactly
/// `render(cfg)`. Rendered for the OS this runs on: `render_for_os` with
/// `cfg!(windows)`.
pub fn render_for(cfg: &ServerConfig, leave_dirs_unset: bool) -> String {
    render_for_os(cfg, leave_dirs_unset, cfg!(windows))
}

/// `render_for` for the OS the file is rendered for (`windows`): the
/// commented hint of an unset ModelsDir or StateDir gives a path example of
/// that OS (`D:\llm` on Windows, `/srv/llm` elsewhere).
pub fn render_for_os(cfg: &ServerConfig, leave_dirs_unset: bool, windows: bool) -> String {
    // Canonical `a,b` (a hand-edited `a , b` is tidied, not rejected: llama.cpp
    // trims each entry the same way).
    let hostname =
        crate::net_ifaces::join_hosts(&crate::net_ifaces::parse_hosts(&cfg.hostname_or_default()));

    // Written resolved, unless the caller leaves an unset one unset.
    let unset = |v: &Option<String>| v.as_deref().is_none_or(|s| s.trim().is_empty());
    let models_dir_line = if leave_dirs_unset && unset(&cfg.models_dir) {
        let example = if windows { "D:\\llm" } else { "/srv/llm" };
        format!("; ModelsDir = {example}  ; commented = the engine's models folder")
    } else {
        format!("ModelsDir = {}", cfg.models_dir_or_default())
    };

    // Tags: metadata for the web UI, never a flag. A list with nothing in it
    // writes no key at all (the line carries its own end of line, so an empty
    // list leaves the template exactly as it is), and an empty one round-trips
    // as an empty one.
    let tags_line = if cfg.tags.is_empty() {
        String::new()
    } else {
        format!("Tags = {}\n", cfg.tags.join(","))
    };

    // Each optional field renders as either `Key = value` or, when unset/default,
    // a commented `; Key = example  ; help` hint line (see int_line_or_hint /
    // str_line_or_hint). The `keep` predicate is where the per-field "is this
    // worth writing?" rule lives (n > 0, n != 1, …).
    // Port collapses to a commented hint when unset (the UI "default" checkbox),
    // so None round-trips as None instead of a forced DEFAULT_PORT; otherwise
    // "default" would silently re-materialize as an explicit port on the next
    // reload, and the launch line would keep passing --port.
    let port_line = int_line_or_hint(
        cfg.port,
        "Port",
        &format!(
            "; Port = {DEFAULT_PORT}  ; TCP port llama-server binds (--port); commented = llama.cpp default {DEFAULT_PORT}"
        ),
        |n| n > 0,
    );
    let threads_line = int_line_or_hint(
        cfg.threads,
        "Threads",
        "; Threads = 12  ; optional override; auto-detected if commented",
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
    // Same rule as ModelsMax: the "default" checkbox carries the unset state, so
    // any non-negative value (0 = off, as llama.cpp's own default) is explicit.
    let models_memory_margin_line = int_line_or_hint(
        cfg.models_memory_margin,
        "ModelsMemoryMargin",
        "; ModelsMemoryMargin = 1024  ; MiB kept free per device, unloading LRU models until a new one fits (--models-memory-margin); commented = llama.cpp default 0 (off)",
        |n| n >= 0,
    );
    // The GPUs of this profile, written only when set: unset is the state
    // today (every GPU of the node) and needs no line, and a commented example
    // would name device ids of one particular node.
    let devices_line = opt_nonblank(cfg.devices.clone())
        .map(|v| format!("Devices = {v}\n"))
        .unwrap_or_default();
    // The one bool with a real unset state (see the field's doc): unset means the
    // env var is never exported, which is neither `true` nor `false`.
    let rocblas_hipblaslt_line = bool_line_or_hint(
        cfg.rocblas_use_hipblaslt,
        "RocblasUseHipblaslt",
        "; RocblasUseHipblaslt = false  ; rocBLAS GEMM backend (env ROCBLAS_USE_HIPBLASLT): false = Tensile, true = hipBLASLt; commented = rocBLAS decides",
    );
    // The GTT pair (see the field's doc): unset is the ON state for the launch,
    // so the hint says the pair is exported; the commented line reloads as None.
    let rocm_host_memory_gtt_line = bool_line_or_hint(
        cfg.rocm_host_memory_gtt,
        "RocmHostMemoryGtt",
        "; RocmHostMemoryGtt = true  ; paged host memory as the driver's GTT (exports HSA_USERPTR_FOR_PAGED_MEM=0 and HSA_SVM_FOR_PAGED_MEM=0): true exports both, false exports neither and the ROCm runtime's own defaults return; commented = both exported, the on state",
    );
    let cuda_allreduce_line = str_line_or_hint(
        cfg.cuda_allreduce.as_deref(),
        "CudaAllreduce",
        "; CudaAllreduce = none  ; AllReduce of split-mode tensor (env GGML_CUDA_ALLREDUCE): nccl|internal|none; commented = the backend decides",
    );
    // Off is the state that exports nothing, so it collapses to the hint.
    let cuda_disable_graphs_line = if cfg.cuda_disable_graphs {
        "CudaDisableGraphs = true".to_string()
    } else {
        "; CudaDisableGraphs = true  ; env GGML_CUDA_DISABLE_GRAPHS=1: no CUDA/HIP graphs (diagnostic, slower decode); commented = graphs on".to_string()
    };
    let attn_rot_disable_line = if cfg.attn_rot_disable {
        "AttnRotDisable = true".to_string()
    } else {
        "; AttnRotDisable = true  ; env LLAMA_ATTN_ROT_DISABLE=1: no Hadamard rotation of a quantized KV cache; commented = rotation on".to_string()
    };
    let rocprof_attach_line = if cfg.rocprof_attach {
        "RocprofAttach = true".to_string()
    } else {
        "; RocprofAttach = true  ; env ROCP_TOOL_ATTACH=1: rocprofv3 --attach may trace the running server; commented = attach refused".to_string()
    };
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

    // Always written like Hostname (a value with a framework default), so an
    // unset one reloads as the explicit default rather than None.
    let ui_lit = if cfg.webui_or_default() {
        "true"
    } else {
        "false"
    };
    let webui_lit = if cfg.webui_mcp_proxy_or_default() {
        "true"
    } else {
        "false"
    };
    let log_verbosity = cfg.log_verbosity_or_default();

    // Like ModelsDir, StateDir is written with its RESOLVED value rather than
    // collapsing to a hint (unless `leave_dirs_unset`): the directory holds multi-GiB dumps, so where
    // they land has to be visible in the file, not implied by a blank line.
    let save_state_lit = if cfg.save_state_on_shutdown_or_default() {
        "true"
    } else {
        "false"
    };
    let state_dir_line = if leave_dirs_unset && unset(&cfg.state_dir) {
        let example = if windows {
            "D:\\llama-state"
        } else {
            "/srv/llama-state"
        };
        format!("; StateDir = {example}  ; commented = the engine's state folder")
    } else {
        format!("StateDir = {}", cfg.state_dir_or_default())
    };

    format!(
        "; Generated by pai-config.
;
; llama-server runtime configuration (machine-wide).
; Per-model knobs live in models.ini, the GPU distribution (device,
; tensor-split, split-mode), the tensor placement (override-tensor), the load
; mode (load-mode), the VRAM auto-fit (fit), prefill-assistant and the image
; encoder's GPU (mmproj-device) among them.

[Server]
{port_line}
; Hostname: the address(es) llama-server listens on (--host). Comma-separated to
; bind several (llama.cpp v0.5.0+); 0.0.0.0 = all interfaces, never combined with
; another address (they would overlap).
Hostname = {hostname}
; Webui: serve the built-in chat UI at / (false launches with --no-ui). The
; OpenAI-compatible API keeps working either way.
Webui = {ui_lit}
; WebuiMcpProxy: enable the built-in web UI's MCP CORS proxy (--webui-mcp-proxy).
; The chat UI uses it for an MCP server marked to use the proxy. Anyone who
; can reach this server can then make it fetch any URL: keep it off on an untrusted
; network.
; Ignored while Webui = false (the flag is not passed).
WebuiMcpProxy = {webui_lit}
; LogVerbosity: llama-server log threshold (-lv). llama.cpp defines six levels:
; 0 output, 1 error, 2 warning, 3 info, 4 trace, 5 debug; a message is printed
; when its level is <= this. Framework default 4 captures per-request logging
; into logs/llama-server.log; 5 also logs the per-tensor override lines.
LogVerbosity = {log_verbosity}
{threads_line}
{threads_batch_line}
{models_max_line}
{models_memory_margin_line}
; The GPUs of this profile, under the key Devices, in the order llama.cpp
; numbers them: comma-separated with no space (ROCm0,ROCm1), in the names the
; node's device list gives, the same names a model profile's device takes, or
; none for CPU only. It is the router's --device, so it is the GPU set of every
; model of the instance and wins over a profile's own device, and the auto-fit
; places a model over exactly these cards. No line: every GPU of the node.
{devices_line}
; SDK tuning: vendor-library environment variables, not llama-server flags;
; llama.cpp never sees them, so nothing validates them and each only bites the
; backend it belongs to. RocblasUseHipblaslt picks rocBLAS's GEMM backend:
; false forces Tensile, true forces hipBLASLt, commented leaves rocBLAS to its
; own routing. An installed framework should not need it: the installer puts
; ROCm's own runtime beside llama-server. A Windows build from source runs the
; graphics driver's runtime, and there on gfx1201 (Radeon AI PRO R9700) hipBLASLt
; fails the second 16-bit GEMM of a process, so every BF16/F16 model aborts on
; its first prefill batch (ROCm issue #6461): set it false in that case. Inert
; on CUDA / Vulkan builds.
{rocblas_hipblaslt_line}
; RocmHostMemoryGtt exports HSA_USERPTR_FOR_PAGED_MEM=0 and
; HSA_SVM_FOR_PAGED_MEM=0, which the ROCm runtime (libhsakmt) reads below
; llama.cpp: with both at 0 the paged host memory is the driver's GTT, so
; compaction and reclaim stop evicting this process's GPU queues. The pair goes
; out for every backend, only ROCm reading it. true and commented export it;
; false exports neither, putting the runtime's own defaults back: that memory is
; no longer swappable and the GTT is a capped window over the RAM (the cap, the
; design and the measures: the ROCm host memory as GTT record in
; ARCHITECTURE.md), so a large pinned host allocation can fail where it used to
; succeed.
{rocm_host_memory_gtt_line}
; CudaAllreduce picks the AllReduce split-mode tensor reduces through (env
; GGML_CUDA_ALLREDUCE, read by the CUDA and the ROCm backend alike; inert in
; the other split modes): nccl = the collective library (RCCL on ROCm, NCCL on
; CUDA where the build links it), internal = ggml's own, staged through pinned
; host memory, none = the generic reduction. Commented leaves the choice to
; the backend. Set it to none for a tensor split that MIXES a CUDA and a ROCm
; device: under any other value that model aborts while loading.
{cuda_allreduce_line}
; CudaDisableGraphs exports GGML_CUDA_DISABLE_GRAPHS=1: the CUDA and the ROCm
; backend launch the decode kernels one by one instead of replaying a graph.
; Diagnostic only (decode gets slower): try it when decode crashes or
; misbehaves while prefill is fine. AttnRotDisable exports
; LLAMA_ATTN_ROT_DISABLE=1, read by llama.cpp itself: no Hadamard rotation of a
; quantized K/V cache, for a quality or speed A/B. Both: commented = unset.
{cuda_disable_graphs_line}
{attn_rot_disable_line}
; RocprofAttach exports ROCP_TOOL_ATTACH=1, which rocprofiler-sdk requires of a
; process before `rocprofv3 --attach <pid>` (in the TheRock dist) may attach a
; kernel trace to it; without it the attach is refused. Commented = unset.
{rocprof_attach_line}
; SaveStateOnShutdown: dump each loaded model's KV cache to StateDir when the
; server stops (--slot-save-path + a /slots save call), and push the newest one
; back on the next start. It buys back the one cost VRAM tuning cannot touch:
; prefill is QUADRATIC in the prompt length and the cache dies with the process,
; so a restart re-reads the whole conversation, where restoring a snapshot is
; only sequential IO. Off by default: each snapshot costs disk, as much as the KV
; cache it holds, the stop waits for the write, and the next start
; loads the model eagerly to push the state back in.
SaveStateOnShutdown = {save_state_lit}
; StateDir: where those snapshots go. One file per model, overwritten each time,
; and dumps of models that are no longer loaded are pruned on save. Point it at a
; data disk if the system drive is tight: a snapshot is the KV cache of the
; conversation, plus the context checkpoints and the draft state it also holds.
{state_dir_line}
; OpencodeBaseUrl overrides the auto-derived integration URL (host:port/v1).
; OpencodeApiKey is the provider's API key for authentication.
; Both are useful when exposing llama-server through a reverse proxy or LLM gateway.
{opencode_base_url_line}
{opencode_api_key_line}
{tags_line}
; ModelsDir: root directory. Models are scanned from ModelsDir/models/, mmproj
; projection files from ModelsDir/mmprojs/, MTP/draft heads from ModelsDir/mtps/,
; DFlash drafters from ModelsDir/dflashs/.
{models_dir_line}
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

/// `Key = true|false` for a set bool, else the commented `hint`. The bool twin of
/// `str_line_or_hint`, for the one bool whose unset state is a real third
/// instruction (never export the variable) rather than a framework default; the
/// always-written bools above (Webui, WebuiMcpProxy, LogVerbosity) render
/// their default instead of a hint, which is why they don't use this.
fn bool_line_or_hint(v: Option<bool>, key: &str, hint: &str) -> String {
    match v {
        Some(b) => format!("{key} = {b}"),
        None => hint.to_string(),
    }
}

/// `Key = value` for a non-blank string, else the commented `hint`.
fn str_line_or_hint(v: Option<&str>, key: &str, hint: &str) -> String {
    match v.map(str::trim) {
        Some(s) if !s.is_empty() => format!("{key} = {s}"),
        _ => hint.to_string(),
    }
}

/// E2E phase for the defaults a saved config resolves, for this crate's e2e
/// test to call (it sets the process-wide `PRIVATE_AI_DATA_ROOT` redirect,
/// `src/tests/mod.rs`): an unset ModelsDir is written as
/// `default_models_dir()`, an unset StateDir as the default state directory.
/// Not a unit test: both go through `paths::`, which the other phases
/// redirect on this thread.
#[cfg(test)]
pub(crate) fn server_cfg_defaults_e2e() {
    let dir = tempfile::tempdir().expect("tempdir");
    crate::paths::redirect_for_tests(dir.path());
    let path = dir.path().join("server.ini");
    std::fs::write(&path, render(&ServerConfig::default())).expect("write server.ini");
    let reloaded = from_keys(&ini::read_section(&path, "Server"));
    assert_eq!(reloaded.models_dir, Some(default_models_dir()));
    assert_eq!(
        reloaded.state_dir,
        Some(ServerConfig::default().state_dir_or_default())
    );
    assert!(
        default_models_dir().starts_with(&*dir.path().to_string_lossy()),
        "under the redirect: {}",
        default_models_dir()
    );
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The engine's defaults, checked in beside this crate and shipped
    /// read-only by the `llama.cpp` package in
    /// `/usr/share/private-ai/llama.cpp/`, whose maintainer scripts copy each
    /// to `/etc/llama.cpp/` when it is missing. Read at run time so a missing
    /// file is a red, not a compile error.
    fn shipped_default(name: &str) -> String {
        let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("defaults")
            .join(name);
        std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("{}: {e}", path.display()))
    }

    /// The engine's starting values for the instance `default`: the keys
    /// llama-server needs to come up under the service (where it listens,
    /// where the models are) with today's defaults: the models in the shared
    /// `/var/lib/llama.cpp/models` (the engine's tmpfiles.d entry creates
    /// it, everyone reads it). No StateDir, on every OS: the snapshots of an
    /// instance live in `paths::instance_state_dir()/<id>`, which the launch
    /// fills in (`launch::with_instance_defaults`), so a StateDir naming the
    /// root here would make every instance share one folder (the snapshots
    /// record of ARCHITECTURE.md). Windows ships the same keys, neither
    /// folder (`defaults/windows/server@default.ini`).
    fn starting_values() -> ServerConfig {
        ServerConfig {
            port: Some(DEFAULT_PORT),
            hostname: Some("localhost".into()),
            models_dir: Some("/var/lib/llama.cpp/models".into()),
            ..ServerConfig::default()
        }
    }

    /// The format belongs to the framework: the engine's shipped
    /// `server@default.ini` holds only a `[Server]` section with the starting
    /// values and at most a two-line comment (the manager rewrites it in
    /// pai's format at its first write), never a copy of pai's template.
    #[test]
    fn the_shipped_default_server_file_holds_only_starting_values() {
        let text = shipped_default("server@default.ini");
        let sections = ini::parse_sections(&text);
        let ids: Vec<&str> = sections.iter().map(|s| s.id.as_str()).collect();
        assert_eq!(ids, ["Server"], "one [Server] section: {text}");
        let keys: Vec<&str> = sections[0].keys.keys().map(String::as_str).collect();
        assert_eq!(
            keys,
            ["Hostname", "ModelsDir", "Port"],
            "only the starting values, no StateDir: {text}"
        );
        assert!(
            !keys.contains(&"StateDir"),
            "the shipped file sets no StateDir, the launch gives the \
             instance its own folder: {text}"
        );
        assert!(
            keys.contains(&"ModelsDir"),
            "the shared models folder is kept: {text}"
        );
        let comments = text
            .lines()
            .filter(|l| {
                let t = l.trim_start();
                t.starts_with(';') || t.starts_with('#')
            })
            .count();
        assert!(
            comments <= 2,
            "at most a two-line comment, {comments}: {text}"
        );
        let lines = text.lines().filter(|l| !l.trim().is_empty()).count();
        let template = render(&starting_values());
        let template_lines = template.lines().filter(|l| !l.trim().is_empty()).count();
        assert!(
            lines * 5 < template_lines,
            "far fewer lines than pai's template ({lines} vs {template_lines}): {text}"
        );
        assert_ne!(text, template, "no copy of pai's template");
        assert!(!text.contains('\u{2014}'), "no em-dash");
    }

    /// The manager reads the shipped file like any other: the starting values
    /// it holds, and pai's defaults for every key it leaves out.
    #[test]
    fn loading_the_shipped_default_server_file_gives_the_starting_values() {
        let text = shipped_default("server@default.ini");
        assert_eq!(from_text(&text), starting_values());
    }

    /// What the manager writes at its first save is the loaded config
    /// rendered: pai's full template, the starting values in it, and
    /// `StateDir` still unset in the manager's write (the instance's own
    /// folder comes from the launch).
    #[test]
    fn rendering_the_shipped_default_server_file_gives_pais_template() {
        let loaded = from_text(&shipped_default("server@default.ini"));
        let written = render(&loaded);
        assert_eq!(written, render(&starting_values()));
        assert_eq!(written.lines().next(), Some("; Generated by pai-config."));
        assert_eq!(
            written.lines().count(),
            render(&ServerConfig::default()).lines().count(),
            "the whole template: {written}"
        );
        for line in [
            "Port = 9931",
            "Hostname = localhost",
            "ModelsDir = /var/lib/llama.cpp/models",
        ] {
            assert!(written.lines().any(|l| l == line), "{line}: {written}");
        }
        // The old root is gone: the file named it, and every instance wrote
        // its snapshots in one folder.
        assert!(
            !written.contains(concat!(
                "/var/lib/private-ai/.local/share/",
                "private-ai/state"
            )),
            "the state root is not carried over: {written}"
        );
        // The manager's write (it leaves an unset directory unset) keeps the
        // key commented, so it reloads unset and the launch resolves
        // `paths::instance_state_dir()/<id>`.
        let managed = render_for(&loaded, true);
        assert!(
            managed.lines().any(|l| l.starts_with("; StateDir = ")),
            "the hint line, commented: {managed}"
        );
        assert!(
            !managed.lines().any(|l| l.starts_with("StateDir = ")),
            "the manager's write sets no StateDir: {managed}"
        );
        assert_eq!(
            from_text(&managed).state_dir,
            None,
            "it reloads unset: {managed}"
        );
    }

    /// The manager accepts the shipped defaults as they are: the server file
    /// against the shipped (empty) model profiles, and the model profiles file as a first
    /// write. The model profiles file holds no section: the instance exists from
    /// the first install and starts once a model profile is added.
    #[test]
    fn the_manager_accepts_the_shipped_defaults() {
        let server = shipped_default("server@default.ini");
        let model_profiles = shipped_default("models.ini");
        assert_eq!(
            model_profiles.trim(),
            "",
            "no preset shipped: {model_profiles:?}"
        );
        assert_eq!(crate::checks::server_file(&server, &model_profiles), Ok(()));
        assert_eq!(
            crate::checks::models_file(&model_profiles, "", "", None),
            Ok(())
        );
    }

    /// `render` → parse-back through the real INI reader: the guard for steps
    /// 2–3 of the field recipe (a key-name typo between `from_keys` and the
    /// writer, or a wrong `keep` predicate, fails here).
    fn round_trip(cfg: &ServerConfig) -> ServerConfig {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("server.ini");
        std::fs::write(&path, render(cfg)).unwrap();
        from_keys(&ini::read_section(&path, "Server"))
    }

    // ── Leaving ModelsDir and StateDir unset (the Windows manager) ────────

    /// Unset stays unset: neither directory is resolved into the file, so
    /// the instance falls back to the engine's folders, not the saving
    /// user's home. Blank is unset, as everywhere else in server.ini.
    #[test]
    fn unset_dirs_stay_unset_when_left_unset() {
        for cfg in [
            ServerConfig {
                port: Some(8100),
                ..ServerConfig::default()
            },
            ServerConfig {
                port: Some(8100),
                models_dir: Some("  ".into()),
                state_dir: Some(String::new()),
                ..ServerConfig::default()
            },
        ] {
            let text = render_for(&cfg, true);
            let dir = tempfile::tempdir().unwrap();
            let path = dir.path().join("server@default.ini");
            std::fs::write(&path, &text).unwrap();
            let reloaded = from_keys(&ini::read_section(&path, "Server"));
            assert_eq!(reloaded.models_dir, None, "{text}");
            assert_eq!(reloaded.state_dir, None, "{text}");
            assert_eq!(reloaded.port, Some(8100), "{text}");
            assert!(
                !text.contains(".llama.cpp"),
                "the user's default ModelsDir was written: {text}"
            );
        }
    }

    /// The hint of a directory left unset gives an example path of the OS
    /// the file is for: a Linux manager's file shows a Linux path, not
    /// `D:\\llm`, and a Windows one keeps the `D:\\` examples. On Linux,
    /// `render_for` renders for Linux.
    #[test]
    fn the_hints_of_dirs_left_unset_fit_the_os() {
        let cfg = ServerConfig {
            port: Some(8100),
            ..ServerConfig::default()
        };
        let hint = |text: &str, key: &str| -> String {
            text.lines()
                .find(|l| l.trim_start().starts_with(&format!("; {key} = ")))
                .unwrap_or_else(|| panic!("no {key} hint:\n{text}"))
                .to_string()
        };

        let linux = render_for_os(&cfg, true, false);
        for key in ["ModelsDir", "StateDir"] {
            let line = hint(&linux, key);
            let example = line.trim_start()[format!("; {key} = ").len()..].to_string();
            assert!(example.starts_with('/'), "not a Linux path: {line}");
        }
        assert!(
            !linux.contains("D:\\"),
            "a Windows path in a Linux file:\n{linux}"
        );
        assert_eq!(from_text(&linux).models_dir, None, "{linux}");
        assert_eq!(from_text(&linux).state_dir, None, "{linux}");

        let windows = render_for_os(&cfg, true, true);
        assert!(
            hint(&windows, "ModelsDir").starts_with("; ModelsDir = D:\\llm"),
            "{windows}"
        );
        assert!(
            hint(&windows, "StateDir").starts_with("; StateDir = D:\\llama-state"),
            "{windows}"
        );
        assert_eq!(from_text(&windows).models_dir, None, "{windows}");
        assert_eq!(from_text(&windows).state_dir, None, "{windows}");

        #[cfg(target_os = "linux")]
        assert_eq!(render_for(&cfg, true), linux);
    }

    /// The same template either way: each directory still has its line,
    /// a commented hint, and nothing else of the file changes.
    #[test]
    fn leaving_dirs_unset_changes_only_their_two_lines() {
        let set = ServerConfig {
            models_dir: Some(r"E:\llm".into()),
            state_dir: Some(r"E:\llama-state".into()),
            ..ServerConfig::default()
        };
        let unset = ServerConfig {
            models_dir: None,
            state_dir: None,
            ..set.clone()
        };
        let resolved = render(&set);
        let left = render_for(&unset, true);
        assert_eq!(left.lines().count(), resolved.lines().count(), "{left}");
        let differing: Vec<(&str, &str)> = resolved
            .lines()
            .zip(left.lines())
            .filter(|(a, b)| a != b)
            .collect();
        assert_eq!(differing.len(), 2, "{differing:#?}");
        for (was, now) in differing {
            assert!(
                was.starts_with("ModelsDir = ") || was.starts_with("StateDir = "),
                "{was}"
            );
            assert!(now.trim_start().starts_with(';'), "not a hint: {now}");
        }
    }

    /// What the file names is written as it is: leaving unset only touches
    /// what is unset, so a configured directory round-trips.
    #[test]
    fn configured_dirs_are_written_when_left_unset() {
        let cfg = ServerConfig {
            models_dir: Some(r"E:\llm".into()),
            state_dir: Some(r"E:\llama-state".into()),
            port: Some(8100),
            ..ServerConfig::default()
        };
        assert_eq!(render_for(&cfg, true), render(&cfg));
    }

    /// Without it, nothing changes: Linux and macOS keep writing both
    /// directories resolved (explicit here, since resolving goes through
    /// `paths::`, which the e2e redirects on another thread).
    #[test]
    fn render_for_without_leaving_unset_is_render() {
        let cfg = ServerConfig {
            models_dir: Some("/var/lib/llama.cpp/models".into()),
            state_dir: Some("/var/lib/private-ai/.local/share/private-ai/state".into()),
            ..ServerConfig::default()
        };
        assert_eq!(render_for(&cfg, false), render(&cfg));
    }

    /// The manager accepts what is written with both directories unset,
    /// as it accepts the shipped Windows default, which names neither.
    #[test]
    fn the_manager_accepts_a_file_with_the_dirs_left_unset() {
        let text = render_for(&ServerConfig::default(), true);
        assert_eq!(crate::checks::server_file(&text, ""), Ok(()));
    }

    /// The file says which program wrote it, by the configurator's new name.
    #[test]
    fn the_header_names_pai_config() {
        let text = render(&ServerConfig::default());
        assert_eq!(text.lines().next(), Some("; Generated by pai-config."));
        assert!(!text.contains(concat!("llama-cpp", "-config")), "{text}");
    }

    #[test]
    fn rich_config_round_trips_through_ini() {
        // Values chosen to dodge the deliberate collapses (see the test below):
        // every `keep` predicate holds, so everything must come back verbatim.
        let original = ServerConfig {
            port: Some(8081),
            hostname: Some("0.0.0.0".into()),
            threads: Some(12),
            threads_batch: Some(24),
            models_max: Some(2),
            models_memory_margin: Some(1536),
            // R2: the GPU list, kept verbatim (no collapse).
            devices: Some("CUDA0,ROCm1".into()),
            models_dir: Some(r"E:\llm".into()),
            // Non-default (unset is the framework default): the state that
            // matters, and the one a `keep`-style collapse would swallow.
            rocblas_use_hipblaslt: Some(false),
            // GT1: same shape, and `true` is the state a `keep`-style collapse
            // (only write when false) would swallow.
            rocm_host_memory_gtt: Some(true),
            // Non-default (unset is the default), like the knob above.
            cuda_allreduce: Some("none".into()),
            // true: false is the default and the hint line.
            cuda_disable_graphs: true,
            attn_rot_disable: true,
            // Non-default (4 is the framework default); true for the same
            // reason as the toggles above.
            rocprof_attach: true,
            // Non-default (on is llama.cpp's default).
            webui: Some(false),
            webui_mcp_proxy: Some(false),
            log_verbosity: Some(2),
            // Non-default (off is the framework default), and a StateDir off the
            // system drive, which is the whole reason the path is overridable.
            save_state_on_shutdown: Some(true),
            state_dir: Some(r"E:\llama-state".into()),
            opencode_base_url: Some("https://llm.example.com".into()),
            opencode_api_key: Some("sk-proxy-key-123".into()),
            // Metadata for the web UI, in the file beside the server keys: the
            // order is part of the value.
            tags: vec!["edge".into(), "v2".into(), "a-1".into()],
        };
        assert_eq!(round_trip(&original), original);
    }

    // The writer's DELIBERATE collapses, pinned as documented behavior:
    // - Hostname/Webui/WebuiMcpProxy/LogVerbosity are always written, so an
    //   unset one reloads as the framework default (localhost / true / true /
    //   4) rather than None.
    // - `keep` predicates turn unset/"not worth writing" values into commented
    //   hint lines: port <= 0, threads/threads_batch <= 0, models_max unset.
    //   An unset port reloads as None (not a forced DEFAULT_PORT), so the UI "default"
    //   checkbox round-trips and the launch line omits --port.
    // - ModelsDir falls back to default_models_dir() when unset (pinned in
    //   `server_cfg_defaults_e2e`, under the redirect).
    #[test]
    fn default_config_reloads_as_explicit_defaults() {
        let reloaded = round_trip(&ServerConfig::default());
        assert_eq!(reloaded.port, None);
        assert_eq!(reloaded.hostname.as_deref(), Some("localhost"));
        assert_eq!(reloaded.webui, Some(true));
        assert_eq!(reloaded.webui_mcp_proxy, Some(false));
        assert_eq!(reloaded.log_verbosity, Some(4));
        // Unset renders as a commented line and stays unset: the default (4)
        // lives in the code, and `env_vars` still exports it.
        assert!(!reloaded.rocprof_attach);
        assert_eq!(reloaded.threads, None);
        assert_eq!(reloaded.threads_batch, None);
        assert_eq!(reloaded.models_max, None);
        assert_eq!(reloaded.models_memory_margin, None);
        // The one bool that stays None: unset means the env var is never
        // exported, so materializing a default would BE a decision.
        assert_eq!(reloaded.rocblas_use_hipblaslt, None);
        // GT1: unset stays unset too. Here it is the ON state for the launch,
        // so writing a default line for it would be a decision the file never
        // made, and the commented hint is what reloads as None.
        assert_eq!(reloaded.rocm_host_memory_gtt, None);
        assert_eq!(reloaded.cuda_allreduce, None);
        // ModelsDir and StateDir are written resolved, through `paths::`,
        // which `data_root_e2e` redirects on another thread: here only that
        // both materialize; their exact values are `server_cfg_defaults_e2e`.
        let models = reloaded.models_dir.expect("ModelsDir materialized");
        assert!(models.ends_with(".llama.cpp"), "{models:?}");
        // Snapshots are off until asked for, and StateDir materializes like
        // ModelsDir: a directory of large dumps has to be visible in the file.
        assert_eq!(reloaded.save_state_on_shutdown, Some(false));
        let state = reloaded.state_dir.expect("StateDir materialized");
        assert!(std::path::Path::new(&state).is_absolute(), "{state:?}");
    }

    /// `RocblasUseHipblaslt = false` is a value, not an absence: the trap in a
    /// bool that renders through a hint line, since the obvious "only write it
    /// when true" would silently turn the workaround state back into "unset" on
    /// the next save.
    #[test]
    fn rocblas_hipblaslt_round_trips_both_states_and_stays_unset_when_absent() {
        for state in [Some(true), Some(false), None] {
            let cfg = ServerConfig {
                rocblas_use_hipblaslt: state,
                ..Default::default()
            };
            assert_eq!(round_trip(&cfg).rocblas_use_hipblaslt, state, "{state:?}");
        }
    }

    /// GT1: `RocmHostMemoryGtt` is a bool. `true` and `false` are values, a word
    /// the reader does not know is unset (the launch then exports the pair, as
    /// for an absent key), and an absent key stays unset.
    #[test]
    fn rocm_host_memory_gtt_reads_its_bool_and_drops_a_non_bool() {
        for (text, want) in [
            ("[Server]\nRocmHostMemoryGtt = true\n", Some(true)),
            ("[Server]\nRocmHostMemoryGtt = false\n", Some(false)),
            ("[Server]\nRocmHostMemoryGtt = sometimes\n", None),
            ("[Server]\nRocmHostMemoryGtt =\n", None),
            ("[Server]\n", None),
        ] {
            assert_eq!(from_text(text).rocm_host_memory_gtt, want, "reads {text:?}");
        }
    }

    /// GT1: the three states round-trip through the writer. `false` is a value,
    /// not an absence (the same trap as `RocblasUseHipblaslt`: "only write it
    /// when true" would turn the off state back into unset on the next save),
    /// `true` is the explicit on, and an absent key stays unset, which is the
    /// on state for the launch.
    #[test]
    fn rocm_host_memory_gtt_round_trips_both_states_and_stays_unset_when_absent() {
        for state in [Some(true), Some(false), None] {
            let cfg = ServerConfig {
                rocm_host_memory_gtt: state,
                ..Default::default()
            };
            assert_eq!(round_trip(&cfg).rocm_host_memory_gtt, state, "{state:?}");
        }
    }

    /// GT1: the render writes the key line only when the profile sets it; an
    /// unset one writes the commented default line, which must say the pair is
    /// on (unset is the on state, so a hint reading like an off state would
    /// send a reader to the wrong side).
    #[test]
    fn rocm_host_memory_gtt_renders_as_a_key_only_when_set() {
        // The directories are named so the render resolves nothing through
        // `paths::`, which `data_root_e2e` redirects on another thread.
        let dirs = || ServerConfig {
            models_dir: Some("/var/lib/llama.cpp/models".into()),
            state_dir: Some("/var/lib/llama.cpp/state".into()),
            ..Default::default()
        };
        for (state, lit) in [(Some(true), "true"), (Some(false), "false")] {
            let text = render(&ServerConfig {
                rocm_host_memory_gtt: state,
                ..dirs()
            });
            let line = format!("RocmHostMemoryGtt = {lit}");
            assert!(
                text.lines().any(|l| l == line),
                "{state:?} writes {line:?}: {text}"
            );
        }
        let text = render(&dirs());
        assert!(
            !text
                .lines()
                .any(|l| l.trim_start().starts_with("RocmHostMemoryGtt =")),
            "unset writes no key line: {text}"
        );
        assert!(
            text.lines()
                .any(|l| l.starts_with("; RocmHostMemoryGtt = ")),
            "unset writes its commented default line: {text}"
        );
    }

    /// `CudaAllreduce` is a closed list: every member round-trips, a hand-edit
    /// in another case comes back canonical, and a word the backend does not
    /// know (it would run it as `none` behind a warning) is dropped on read
    /// rather than kept and exported.
    #[test]
    fn cuda_allreduce_round_trips_its_modes_and_drops_unknown_words() {
        for mode in ALLREDUCE_MODES {
            let cfg = ServerConfig {
                cuda_allreduce: Some(mode.into()),
                ..Default::default()
            };
            assert_eq!(round_trip(&cfg).cuda_allreduce.as_deref(), Some(mode));
        }
        let read = |v: &str| {
            from_keys(&std::collections::BTreeMap::from([(
                "CudaAllreduce".to_string(),
                v.to_string(),
            )]))
            .cuda_allreduce
        };
        assert_eq!(read(" Internal ").as_deref(), Some("internal"));
        assert_eq!(read("default"), None);
        assert_eq!(read("butterfly"), None);
    }

    /// The tags of the profile: a plain list in the order it was written, on
    /// one `Tags` line joined with `,`. A hand-edited ` gpu , edge ` comes back
    /// trimmed, an empty entry (a doubled comma) is dropped, and a list with
    /// nothing in it writes no key, so it round-trips as nothing.
    #[test]
    fn tags_round_trip_in_order_and_an_empty_list_writes_no_key() {
        for list in [
            Vec::new(),
            vec!["gpu".to_string()],
            vec!["edge".to_string(), "v2".to_string(), "gpu".to_string()],
        ] {
            let cfg = ServerConfig {
                tags: list.clone(),
                ..Default::default()
            };
            assert_eq!(round_trip(&cfg).tags, list, "{list:?}");
        }
        let written = render(&ServerConfig {
            tags: vec!["edge".to_string(), "v2".to_string()],
            ..Default::default()
        });
        assert!(
            written.contains("\nTags = edge,v2\n"),
            "the list is one line, joined with a comma:\n{written}"
        );
        for line in render(&ServerConfig::default()).lines() {
            assert!(
                !line
                    .trim_start()
                    .trim_start_matches(';')
                    .trim_start()
                    .starts_with("Tags"),
                "an empty list wrote a Tags line: {line:?}"
            );
        }
        let read = |v: &str| {
            from_keys(&std::collections::BTreeMap::from([(
                "Tags".to_string(),
                v.to_string(),
            )]))
            .tags
        };
        assert_eq!(
            read(" gpu , edge "),
            vec!["gpu".to_string(), "edge".to_string()]
        );
        assert_eq!(
            read("gpu,,edge"),
            vec!["gpu".to_string(), "edge".to_string()]
        );
        assert_eq!(read(""), Vec::<String>::new());
    }

    /// R2 (ARCHITECTURE.md "Resources belong to the server profile"): the
    /// `[Server] Devices` key, the probe ids in order comma-separated, or
    /// `none`; absent means every GPU. It round-trips through the file (a save
    /// that edits another key keeps the line), a blank line loads unset, and an
    /// unset field writes no line at all.
    #[test]
    fn devices_round_trips_and_absent_writes_no_key() {
        for value in ["ROCm0,ROCm1", "none", "ROCm7"] {
            let cfg = ServerConfig {
                devices: Some(value.into()),
                ..Default::default()
            };
            assert_eq!(
                round_trip(&cfg).devices.as_deref(),
                Some(value),
                "Devices = {value:?} reloads as itself"
            );
            assert!(
                render(&cfg).contains(&format!("\nDevices = {value}\n")),
                "the key is one line: {value:?}"
            );
            assert_eq!(check(&cfg, 16), Ok(()), "Devices = {value:?} is taken");
        }
        // Absent: no line of its own, not even a hint.
        for line in render(&ServerConfig::default()).lines() {
            assert!(
                !line
                    .trim_start()
                    .trim_start_matches(';')
                    .trim_start()
                    .starts_with("Devices"),
                "an unset Devices wrote a line: {line:?}"
            );
        }
        assert_eq!(
            from_text("[Server]\nDevices = ROCm0,ROCm1\n")
                .devices
                .as_deref(),
            Some("ROCm0,ROCm1"),
            "the file's list reads into the field"
        );
        assert_eq!(
            from_text("[Server]\nDevices =\n").devices,
            None,
            "a blank line is unset, as every optional text"
        );
        assert_eq!(
            from_text("[Server]\n").devices,
            None,
            "absent is unset: every GPU, as today"
        );
    }

    /// A hand-edited `load-mode` is canonicalized rather than silently dropped,
    /// and a value that names no mode is refused, never handed to llama.cpp,
    /// which exits on an argument it rejects.
    #[test]
    fn load_mode_normalizes() {
        for (input, expected) in [
            ("MMAP+MLOCK", Some("mmap+mlock")),
            ("  mlock  ", Some("mlock")),
            ("Auto", Some("auto")),
            ("--mlock", None),
            ("", None),
        ] {
            assert_eq!(normalize_load_mode(input), expected, "input {input:?}");
        }
    }

    #[test]
    fn not_worth_writing_values_collapse_to_hints() {
        let cfg = ServerConfig {
            port: Some(0),
            threads: Some(0),
            threads_batch: Some(-4),
            ..Default::default()
        };
        let reloaded = round_trip(&cfg);
        assert_eq!(
            reloaded.port, None,
            "port <= 0 is unset (llama.cpp default)"
        );
        assert_eq!(reloaded.threads, None, "threads <= 0 is auto");
        assert_eq!(reloaded.threads_batch, None, "threads_batch <= 0 is auto");
    }

    // The "default" checkbox carries the unset state now, so every non-negative
    // models_max is an explicit value that must persist, including 1 (which the
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

    // Pure validation (no IO), the server-side mirror of model profiles'
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

    /// StateDir gets the same treatment, and it matters more than for most
    /// paths: a truncated one reloads as a directory that does not exist, and
    /// llama.cpp refuses to START on a `--slot-save-path` it cannot stat.
    /// ModelsDir is pinned alongside so the clean case is not vacuous.
    #[test]
    fn save_validation_rejects_comment_markers_in_state_dir() {
        let with = |dir: &str| ServerConfig {
            models_dir: Some(r"E:\llm".into()),
            state_dir: Some(dir.into()),
            ..Default::default()
        };
        assert!(validate_for_save(&with(r"E:\llama-state")).is_ok());
        for hostile in [r"E:\state #1", r"E:\a;b"] {
            let err = validate_for_save(&with(hostile)).expect_err(hostile);
            assert!(
                err.to_string().contains("StateDir"),
                "error names the field"
            );
        }
        // An unset StateDir must not be validated against the machine-derived
        // default: that would drag `paths::` into a pure function.
        assert!(validate_for_save(&ServerConfig {
            models_dir: Some(r"E:\llm".into()),
            ..Default::default()
        })
        .is_ok());
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
        // A bind LIST (llama.cpp v0.5.0): a loopback entry wins wherever it
        // sits, otherwise the first address; never the raw `a,b`.
        assert_eq!(with("192.168.1.5,localhost").client_host(), "localhost");
        assert_eq!(with("192.168.1.5, 127.0.0.1").client_host(), "127.0.0.1");
        assert_eq!(with("192.168.1.5,10.0.0.2").client_host(), "192.168.1.5");
    }

    // OpencodeBaseUrl and OpencodeApiKey are free-text fields written to
    // server.ini; a comment marker would reload truncated.
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

    // ── The checks before a write (`check`) ─────────────────────────────

    /// A config with both folders explicit: their defaults resolve through
    /// `paths::`, which unit tests must not touch.
    fn checked_base() -> ServerConfig {
        ServerConfig {
            models_dir: Some("/models".into()),
            state_dir: Some("/var/lib/private-ai/state".into()),
            ..ServerConfig::default()
        }
    }

    fn not_int(key: &'static str, text: &str, lo: i32, hi: i32) -> crate::checks::Refusal {
        crate::checks::Refusal {
            key: "not-whole-number-in-range",
            values: vec![key.into(), text.into(), lo.to_string(), hi.to_string()],
        }
    }

    /// A config as the configurator writes it passes, all keys set or none.
    #[test]
    fn the_check_passes_a_config_as_the_configurator_writes_it() {
        let rich = ServerConfig {
            port: Some(9090),
            hostname: Some("127.0.0.1,192.168.1.10".into()),
            threads: Some(12),
            threads_batch: Some(16),
            models_max: Some(0),
            models_memory_margin: Some(0),
            log_verbosity: Some(5),
            opencode_base_url: Some("https://gw.example.com/llm".into()),
            opencode_api_key: Some("sk-abc123".into()),
            ..checked_base()
        };
        assert_eq!(check(&rich, 16), Ok(()));
        assert_eq!(check(&checked_base(), 16), Ok(()));
        let all = ServerConfig {
            hostname: Some("0.0.0.0".into()),
            log_verbosity: Some(0),
            port: Some(65535),
            ..checked_base()
        };
        assert_eq!(check(&all, 1), Ok(()));
    }

    /// What the manager refuses in `server@<id>.ini`, refused here too, on the
    /// value as set: a port of 0 or a negative ModelsMax would otherwise
    /// render as the commented hint, i.e. silently unset.
    #[test]
    #[allow(clippy::type_complexity)]
    fn the_check_refuses_what_the_manager_refuses() {
        use crate::checks::Refusal;
        let max = i32::MAX;
        let cases: Vec<(&str, fn(&mut ServerConfig), Refusal)> = vec![
            (
                "port 0",
                |c| c.port = Some(0),
                not_int("Port", "0", 1, 65535),
            ),
            (
                "port 65536",
                |c| c.port = Some(65536),
                not_int("Port", "65536", 1, 65535),
            ),
            (
                "ModelsMax -1",
                |c| c.models_max = Some(-1),
                not_int("ModelsMax", "-1", 0, max),
            ),
            (
                "ModelsMemoryMargin -1",
                |c| c.models_memory_margin = Some(-1),
                not_int("ModelsMemoryMargin", "-1", 0, max),
            ),
            (
                "LogVerbosity 7",
                |c| c.log_verbosity = Some(7),
                not_int("LogVerbosity", "7", 0, 5),
            ),
            (
                "LogVerbosity -1",
                |c| c.log_verbosity = Some(-1),
                not_int("LogVerbosity", "-1", 0, 5),
            ),
            (
                "0.0.0.0 with another address",
                |c| c.hostname = Some("0.0.0.0,127.0.0.1".into()),
                Refusal {
                    key: "bind-all-with-others",
                    values: vec![],
                },
            ),
            (
                "a comment marker",
                |c| c.models_dir = Some("/models #1".into()),
                Refusal {
                    key: "comment-markers",
                    values: vec!["ModelsDir".into()],
                },
            ),
        ];
        for (label, set, want) in cases {
            let mut cfg = checked_base();
            set(&mut cfg);
            assert_eq!(check(&cfg, 16), Err(want), "{label}");
        }
    }

    /// `Devices` is free text with a list rule that never looks at a comment
    /// marker, so the marker rule the other free-text server keys obey must run
    /// on it too: `ROCm0;x` is written and reloads as `ROCm0` (the INI format
    /// has no escaping), GPUs the user never typed.
    #[test]
    fn the_check_refuses_a_comment_marker_in_devices() {
        for value in ["ROCm0;x", "ROCm0#y"] {
            let cfg = ServerConfig {
                devices: Some(value.into()),
                ..checked_base()
            };
            assert_eq!(
                check(&cfg, 16),
                Err(crate::checks::Refusal {
                    key: "comment-markers",
                    values: vec!["Devices".into()],
                }),
                "Devices = {value:?}"
            );
        }
    }

    /// The thread counts run from 1 to this machine's logical CPU count, as
    /// the Server tab's sliders do.
    #[test]
    fn the_check_holds_the_thread_counts_to_the_cpu_count() {
        for (key, set) in [
            (
                "Threads",
                (|c, n| c.threads = Some(n)) as fn(&mut ServerConfig, i32),
            ),
            ("ThreadsBatch", |c, n| c.threads_batch = Some(n)),
        ] {
            for bad in [17, 0, -1] {
                let mut cfg = checked_base();
                set(&mut cfg, bad);
                assert_eq!(
                    check(&cfg, 16),
                    Err(not_int(key, &bad.to_string(), 1, 16)),
                    "{key} = {bad}"
                );
            }
            for good in [1, 16] {
                let mut cfg = checked_base();
                set(&mut cfg, good);
                assert_eq!(check(&cfg, 16), Ok(()), "{key} = {good}");
            }
        }
    }

    /// The profile's tags obey the manager's rule (`checks::tags_rule`) before
    /// a save, on every surface that runs this check: the GUI's Server tab,
    /// `pai server set` and the web's save. The refusal is the rule's own, so
    /// the same words name the same fault on every one of them, and an empty
    /// list (the tags cleared) passes.
    #[test]
    fn the_check_judges_the_tags_by_the_managers_rule() {
        let want = |reason: &str, list: &str| crate::checks::Refusal {
            key: "server-bad-tags",
            values: vec![
                reason.into(),
                list.into(),
                crate::checks::TAGS_MAX.to_string(),
                crate::checks::TAG_MIN.to_string(),
                crate::checks::TAG_MAX.to_string(),
            ],
        };
        let tags = |list: &[&str]| -> Vec<String> { list.iter().map(|t| t.to_string()).collect() };
        let over: Vec<String> = (0..=crate::checks::TAGS_MAX)
            .map(|i| format!("t{i}"))
            .collect();
        let over_list = over.join(",");
        let long = "z".repeat(crate::checks::TAG_MAX + 1);
        for (list, reason, named) in [
            (tags(&["Edge"]), "bad characters", "Edge".to_string()),
            (
                tags(&["gpu", "gpu"]),
                "the same tag twice",
                "gpu,gpu".into(),
            ),
            (
                tags(&["gpu", long.as_str()]),
                "too long",
                format!("gpu,{long}"),
            ),
            (tags(&[""]), "empty tag", String::new()),
            (over.clone(), "too many tags", over_list),
        ] {
            let cfg = ServerConfig {
                tags: list.clone(),
                ..checked_base()
            };
            assert_eq!(check(&cfg, 16), Err(want(reason, &named)), "tags {list:?}");
        }
        for list in [
            Vec::new(),
            tags(&["edge", "v2"]),
            tags(&["a"]),
            tags(&["0-9"]),
        ] {
            let cfg = ServerConfig {
                tags: list.clone(),
                ..checked_base()
            };
            assert_eq!(check(&cfg, 16), Ok(()), "tags {list:?}");
        }
    }
}
