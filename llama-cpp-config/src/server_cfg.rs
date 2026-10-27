//! server.ini schema and IO for llama.cpp-framework.
//!
//! `save()` rewrites the whole FILE from a `ServerConfig` (server.ini is fully
//! generated; unlike presets.ini, hand-added content outside the template does
//! not survive a save). Unset optional fields are emitted as commented
//! `; Key = example  ; help` hint lines (never omitted) so the file
//! self-documents every available knob.
//!
//! ADD A SERVER FIELD, the mirror of the preset recipe (top of presets.rs): a
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
//!   8. `runstate::server_args`               : map the field to its llama-server
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

use std::fs;
use std::io;

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
    /// prefill batch of any BF16/F16 model. Forcing Tensile is a full-speed cure
    /// rather than a degradation: measured pp512 587 t/s, tg128 35 t/s on Ornith
    /// BF16, against ~19 t/s prefill for the older
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
    /// Hence `03-package.ps1` bundles the dist's whole runtime into `bin\`,
    /// `amdhip64_*.dll` included, so an installed tree needs no knob here at
    /// all (a build from source still runs against `HIP_PATH\bin` over PATH,
    /// i.e. the driver's runtime). The bundle also carries the dist's
    /// `amd_comgr*.dll`: the runtime asks for it by bare name and System32
    /// keeps the DRIVER's own (a different build that precedes PATH), a mix
    /// that stood until an iGPU appeared and then killed `hipGetDeviceCount`
    /// outright, dropping the whole HIP backend (2026-09-03).
    ///
    /// Left unset by default all the same: the bug is per-arch, hipBLASLt is the
    /// faster path everywhere it works, and the runtime finding rests on one
    /// driver (32.0.22042.14002) on one box.
    pub rocblas_use_hipblaslt: Option<bool>,
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
    /// prefill 599 t/s against 1290 for the layer split, decode unchanged.
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
    /// The FlashAttention band on RDNA4, exported as `GGML_HIP_FA_BAND_WMMA`:
    /// one of [`HIP_FA_BANDS`], `0` = off (the variable is not exported). None
    /// = the tool's default, `4` ([`Self::hip_fa_band_or_default`]), which IS
    /// exported; it lives in the code, not in server.ini, so unset renders as a
    /// commented hint line and reloads as None.
    ///
    /// An opt-in path in the framework's PATCHED `libggml-hip`: the 1-8 query
    /// FlashAttention (the single-token decode and the MTP verify batch) of a
    /// head-256 / GQA 6 model (Qwen3.8-27B) runs on the WMMA kernel with the KV
    /// cache split round-robin over the CUs. Measured on the R9700 (2026-10-20):
    /// FLASH_ATTN_EXT -15% at 1 query and -38% at 3-4 queries, decode at a 110k
    /// context -5%, and greedy output identical between MTP and plain decode.
    /// `2` only helps at 1-2 queries and falls back at 3-4, so `4` is the one to
    /// use. Inert on an unpatched llama.cpp, on a non-RDNA4 GPU, on any other
    /// head size or GQA, and on CUDA. Not a llama-server flag: ggml reads it
    /// with `getenv` at the first attention call, hence `env_vars`.
    pub hip_fa_band: Option<i32>,
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
    /// None = the framework default (on): the bundled chat UI needs it to call
    /// MCP tools. Experimental upstream (llama.cpp defaults it OFF); don't enable
    /// on an untrusted network.
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
    /// restart re-reads the whole conversation from zero. Measured on a 234k
    /// prompt (Qwen3.8-27B, hybrid `qwen35`): about 17 minutes of prefill, against
    /// ~8.3 GiB of sequential IO to reload the same state.
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
    /// conversation, not for the allocated context, so a 127,579-token
    /// conversation on Qwen3.8-27B measured 4,703,061,864 bytes (4.38 GiB, i.e.
    /// the arch's 36 KiB per token) where a full 262k context would be ~8.3 GiB.
    /// Measured on v0.6.0; since v0.7.0 the file also carries the context
    /// checkpoints and the draft state (`slot_state` module header), so more.
    /// That is a ceiling to plan disk against, never a fixed cost of enabling
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
}

// ── Defaults & accessors ─────────────────────────────────────────────────

/// Every value `-lm` accepts, in the dropdown's order (`common/arg.cpp`'s
/// `--load-mode` help; `enum llama_load_mode` in include/llama.h), read by the
/// per-preset `load-mode` key. Mirrors `Options.load_modes` in
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

/// The values `HipFaBand` takes (`GGML_HIP_FA_BAND_WMMA` of the patched
/// libggml-hip), `0` meaning off. Mirrors `Options.hip_fa_bands` in
/// ui/components.slint after its leading "default" (the unset state), through
/// `server_form::hip_fa_band_label`; the e2e
/// (`src/tests/ui_bindings.rs`) asserts the two lists agree.
pub const HIP_FA_BANDS: [i32; 3] = [0, 2, 4];

/// `v` as one of [`HIP_FA_BANDS`]; `None` for anything else ("default" and a
/// blank included), so a hand-edited value the patch does not know reloads as
/// unset, i.e. as the default.
pub fn normalize_hip_fa_band(v: &str) -> Option<i32> {
    ini::parse_int(v).filter(|n| HIP_FA_BANDS.contains(n))
}

/// llama-server's own `--port` default (`common_params::port`, common/common.h),
/// which is what it binds when `Port` is unset and the launch omits the flag.
/// It MUST track the bundled llama.cpp: every client URL the configurator
/// builds (health check, Open chat, the live benchmark, opencode.json, the
/// Claude Code snippet) reads it, so a stale value points them all at a port
/// nothing listens on. v0.7.0 moved it from 8080 to 9931 (#30159).
pub const DEFAULT_PORT: i32 = 9931;

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
    // `runstate::server_args`, and the GUI all pull from here, so changing a
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

    /// The web-UI MCP proxy flag, or `true` (the framework default) when unset.
    /// The STORED choice: whether the launch passes the flag also depends on
    /// [`Self::webui_or_default`] (`runstate::server_args`).
    pub fn webui_mcp_proxy_or_default(&self) -> bool {
        self.webui_mcp_proxy.unwrap_or(true)
    }

    /// The FlashAttention band (`GGML_HIP_FA_BAND_WMMA`), or `4` (the framework
    /// default) when unset.
    pub fn hip_fa_band_or_default(&self) -> i32 {
        self.hip_fa_band.unwrap_or(4)
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
        threads: keys.get("Threads").and_then(|v| ini::parse_int(v)),
        threads_batch: keys.get("ThreadsBatch").and_then(|v| ini::parse_int(v)),
        models_max: keys.get("ModelsMax").and_then(|v| ini::parse_int(v)),
        models_dir: opt_nonblank(keys.get("ModelsDir").cloned()),
        // Absent (or unparseable) stays None: "leave the env var alone" is a
        // real third state here, not a stand-in for a framework default.
        rocblas_use_hipblaslt: keys
            .get("RocblasUseHipblaslt")
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
        hip_fa_band: keys.get("HipFaBand").and_then(|v| normalize_hip_fa_band(v)),
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

/// Save-boundary validation, pure so the unit test never touches `paths::`,
/// the mirror of `presets::validate_for_save`. The INI format can't escape
/// `;`/`#`, so a ModelsDir containing one would silently reload truncated
/// (see `ini::reject_comment_markers`).
fn validate_for_save(cfg: &ServerConfig) -> io::Result<()> {
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

/// Render the whole server.ini body. Pure (no IO): the file-writing wrapper is
/// `save()`; the round-trip test drives this directly, mirroring
/// `presets::render_section`.
fn render(cfg: &ServerConfig) -> String {
    // Canonical `a,b` (a hand-edited `a , b` is tidied, not rejected: llama.cpp
    // trims each entry the same way).
    let hostname =
        crate::net_ifaces::join_hosts(&crate::net_ifaces::parse_hosts(&cfg.hostname_or_default()));

    let models_dir = cfg.models_dir_or_default();

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
    // The one bool with a real unset state (see the field's doc): unset means the
    // env var is never exported, which is neither `true` nor `false`.
    let rocblas_hipblaslt_line = bool_line_or_hint(
        cfg.rocblas_use_hipblaslt,
        "RocblasUseHipblaslt",
        "; RocblasUseHipblaslt = false  ; rocBLAS GEMM backend (env ROCBLAS_USE_HIPBLASLT): false = Tensile, true = hipBLASLt; commented = rocBLAS decides",
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
    // Unset collapses to the hint, like the other SDK keys, although unset
    // still EXPORTS the default (4): the default belongs to the code, so a
    // later change of it reaches every server.ini that never chose. Every
    // explicit value is kept, 0 (off) included.
    let hip_fa_band_line = int_line_or_hint(
        cfg.hip_fa_band,
        "HipFaBand",
        "; HipFaBand = 4  ; env GGML_HIP_FA_BAND_WMMA: 0 = off (not exported), 2, 4; commented = the default, 4",
        |_| true,
    );
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

    // Like ModelsDir, StateDir is always written with its RESOLVED value rather
    // than collapsing to a hint: the directory holds multi-GiB dumps, so where
    // they land has to be visible in the file, not implied by a blank line.
    let save_state_lit = if cfg.save_state_on_shutdown_or_default() {
        "true"
    } else {
        "false"
    };
    let state_dir = cfg.state_dir_or_default();

    format!(
        "; Generated by llama-cpp-config.
;
; llama-server runtime configuration (machine-wide).
; Per-model knobs live in presets.ini, the GPU distribution (device,
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
; The bundled chat UI needs it to call MCP tools; disable on an untrusted network.
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
; SDK tuning: vendor-library environment variables, not llama-server flags;
; llama.cpp never sees them, so nothing validates them and each only bites the
; backend it belongs to. RocblasUseHipblaslt picks rocBLAS's GEMM backend:
; false forces Tensile, true forces hipBLASLt, commented leaves rocBLAS to its
; own routing. Set it false on gfx1201 (Radeon AI PRO R9700): hipBLASLt fails
; the second 16-bit GEMM of a process there, so every BF16/F16 model aborts on
; its first prefill batch (ROCm issue #6461). Inert on CUDA / Vulkan builds.
{rocblas_hipblaslt_line}
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
; HipFaBand exports GGML_HIP_FA_BAND_WMMA (0 | 2 | 4; 0 = not exported), read
; by the framework's PATCHED ROCm backend: the 1-8 query FlashAttention (decode
; and the MTP verify batch) of a head-256 / GQA 6 model (Qwen3.8-27B) runs on
; the WMMA kernel with the KV cache split round-robin over the CUs. Measured on
; the R9700: FLASH_ATTN_EXT -15% at 1 query, -38% at 3-4, decode at 110k -5%,
; same greedy output. 2 only helps at 1-2 queries, so 4 is the one to use.
; Inert on an unpatched llama.cpp, on non-RDNA4 GPUs, other head sizes or GQA,
; and on CUDA. Commented = the default, 4 (exported); 0 switches it off.
{hip_fa_band_line}
; RocprofAttach exports ROCP_TOOL_ATTACH=1, which rocprofiler-sdk requires of a
; process before `rocprofv3 --attach <pid>` (in the TheRock dist) may attach a
; kernel trace to it; without it the attach is refused. Commented = unset.
{rocprof_attach_line}
; SaveStateOnShutdown: dump each loaded model's KV cache to StateDir when the
; server stops (--slot-save-path + a /slots save call), and push the newest one
; back on the next start. It buys back the one cost VRAM tuning cannot touch:
; prefill is QUADRATIC in the prompt length and the cache dies with the process,
; so a restart re-reads the whole conversation. Measured on a 234k-token prompt:
; ~17 minutes of prefill against ~8.3 GiB of sequential IO. Off by default: each
; snapshot costs that much disk, the stop waits for the write, and the next start
; loads the model eagerly to push the state back in.
SaveStateOnShutdown = {save_state_lit}
; StateDir: where those snapshots go. One file per model, overwritten each time,
; and dumps of models that are no longer loaded are pruned on save. Point it at a
; data disk if the system drive is tight: a 262k-context snapshot is ~8.3 GiB
; of KV cache, plus the context checkpoints and the draft state it also holds.
StateDir = {state_dir}
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

#[cfg(test)]
mod tests {
    use super::*;

    /// `render` → parse-back through the real INI reader: the guard for steps
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
            threads: Some(12),
            threads_batch: Some(24),
            models_max: Some(2),
            models_dir: Some(r"E:\llm".into()),
            // Non-default (unset is the framework default): the state that
            // matters, and the one a `keep`-style collapse would swallow.
            rocblas_use_hipblaslt: Some(false),
            // Non-default (unset is the default), like the knob above.
            cuda_allreduce: Some("none".into()),
            // true: false is the default and the hint line.
            cuda_disable_graphs: true,
            attn_rot_disable: true,
            // Non-default (4 is the framework default); true for the same
            // reason as the toggles above.
            hip_fa_band: Some(2),
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
    // - ModelsDir falls back to default_models_dir() when unset.
    #[test]
    fn default_config_reloads_as_explicit_defaults() {
        let reloaded = round_trip(&ServerConfig::default());
        assert_eq!(reloaded.port, None);
        assert_eq!(reloaded.hostname.as_deref(), Some("localhost"));
        assert_eq!(reloaded.webui, Some(true));
        assert_eq!(reloaded.webui_mcp_proxy, Some(true));
        assert_eq!(reloaded.log_verbosity, Some(4));
        // Unset renders as a commented line and stays unset: the default (4)
        // lives in the code, and `env_vars` still exports it.
        assert_eq!(reloaded.hip_fa_band, None);
        assert!(!reloaded.rocprof_attach);
        assert_eq!(reloaded.threads, None);
        assert_eq!(reloaded.threads_batch, None);
        assert_eq!(reloaded.models_max, None);
        // The one bool that stays None: unset means the env var is never
        // exported, so materializing a default would BE a decision.
        assert_eq!(reloaded.rocblas_use_hipblaslt, None);
        assert_eq!(reloaded.cuda_allreduce, None);
        assert_eq!(reloaded.models_dir, Some(default_models_dir()));
        // Snapshots are off until asked for, and StateDir materializes like
        // ModelsDir: an 8 GiB-per-dump directory has to be visible in the file.
        assert_eq!(reloaded.save_state_on_shutdown, Some(false));
        assert_eq!(
            reloaded.state_dir,
            Some(ServerConfig::default().state_dir_or_default())
        );
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

    /// `HipFaBand = 0` is a value (off), not an absence, so it must survive the
    /// round trip rather than fall back to the default 4; unset comes back
    /// unset (a commented line, not `HipFaBand = 4`); a number the patch does
    /// not know reloads as unset, i.e. as the default.
    #[test]
    fn hip_fa_band_round_trips_its_values_and_drops_unknown_numbers() {
        for band in HIP_FA_BANDS {
            let cfg = ServerConfig {
                hip_fa_band: Some(band),
                ..Default::default()
            };
            assert_eq!(round_trip(&cfg).hip_fa_band, Some(band));
        }
        let unset = ServerConfig {
            models_dir: Some(r"E:\llm".into()),
            ..Default::default()
        };
        assert!(render(&unset)
            .lines()
            .any(|l| l.starts_with("; HipFaBand = 4")));
        assert!(!render(&unset).lines().any(|l| l.starts_with("HipFaBand")));
        assert_eq!(round_trip(&unset).hip_fa_band, None);
        for bad in ["3", "8", "-1", "on", "default", ""] {
            assert_eq!(normalize_hip_fa_band(bad), None, "{bad:?}");
        }
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
}
