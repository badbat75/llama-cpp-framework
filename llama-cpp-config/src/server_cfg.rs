// server.ini schema and IO for llama.cpp-framework.
//
// `save()` rewrites the whole `[Server]` section from a `ServerConfig`. Unset
// optional fields are emitted as commented `; Key = example  ; help` hint lines
// (never omitted) so the file self-documents every available knob.
//
// ADD A SERVER FIELD — the mirror of the preset recipe (top of presets.rs), one
// tier smaller but reaching the CLI (the server has a per-field `set`). Trace an
// existing field like `threads` (PascalCase INI key ↔ snake_case Rust field):
//   1. `ServerConfig` struct field (+ doc)   — below
//   2. `load`                                — INI read; `keys.get("Key")` → field
//   3. `save`                                — INI write; an `int_line_or_hint` /
//      `str_line_or_hint` line + a `{…_line}` slot in the `[Server]` body
//   4. `ServerForm` struct                   — ui/types.slint
//   5. the input widget                      — ui/server_page.slint (bind two-way `<=>`)
//   6. `config_to_form` + `form_to_config`   — src/server_form.rs (BOTH directions)
//   7. THREE spots in src/cli.rs             — the `ServerSet` flag field, the
//      `Show` `println!`, and the `Set` handler that copies the flag into `cfg`
// Guard: the round-trip test in server_form.rs (`form_to_config(config_to_form(c))
// == c`) — a field wired into one conversion only drops out there.

use std::fs;
use std::io;

use crate::ini;
use crate::paths;

#[derive(Debug, Clone, Default, PartialEq)]
pub struct ServerConfig {
    pub port: Option<i32>,
    pub hostname: Option<String>,
    pub mlock: Option<bool>,
    pub threads: Option<i32>,
    pub cache_reuse: Option<i32>,
    pub threads_batch: Option<i32>,
    pub models_max: Option<i32>,
    pub models_dir: Option<String>,
    /// GPU device(s) for the main model, e.g. "CUDA0" (--device). Empty/None =
    /// let llama.cpp use all detected devices. Pinning to one device avoids
    /// splitting across an iGPU or a duplicate Vulkan view of the same GPU.
    pub device: Option<String>,
    /// Multi-GPU split strategy (--split-mode / -sm): "none" | "layer" | "row".
    /// Empty/None = llama.cpp default ("layer"). Identical on CUDA and HIP.
    pub split_mode: Option<String>,
    /// Per-GPU weight proportions (--tensor-split / -ts), e.g. "3,1" for 75/25.
    /// Empty/None = even split across the visible GPUs.
    pub tensor_split: Option<String>,
}

pub fn default_models_dir() -> String {
    paths::home_dir()
        .join(".llama.cpp")
        .join("models")
        .to_string_lossy()
        .into_owned()
}

impl ServerConfig {
    /// The configured ModelsDir, or `default_models_dir()` when unset/blank —
    /// the single home for the "blank ModelsDir ⇒ default dir" rule shared by
    /// `save()`, `runstate::start()`, and `server_form::config_to_form`.
    pub fn models_dir_or_default(&self) -> String {
        self.models_dir
            .clone()
            .filter(|s| !s.trim().is_empty())
            .unwrap_or_else(default_models_dir)
    }
}

pub fn load() -> ServerConfig {
    let path = paths::server_ini();
    let keys = ini::read_section(&path, "Server");
    ServerConfig {
        port: keys.get("Port").and_then(|v| ini::parse_int(v)),
        hostname: keys.get("Hostname").cloned(),
        mlock: keys.get("Mlock").and_then(|v| ini::parse_bool(v)),
        threads: keys.get("Threads").and_then(|v| ini::parse_int(v)),
        cache_reuse: keys.get("CacheReuse").and_then(|v| ini::parse_int(v)),
        threads_batch: keys.get("ThreadsBatch").and_then(|v| ini::parse_int(v)),
        models_max: keys.get("ModelsMax").and_then(|v| ini::parse_int(v)),
        models_dir: keys.get("ModelsDir").cloned(),
        device: opt_nonblank(keys.get("Device").cloned()),
        split_mode: opt_nonblank(keys.get("SplitMode").cloned()),
        tensor_split: opt_nonblank(keys.get("TensorSplit").cloned()),
    }
}

/// `Some(s)` unless `s` is blank (whitespace only), in which case `None`. The
/// single home for the "blank means unset" rule the string fields (device /
/// split-mode / tensor-split) share on both `load()` and the CLI `server set`.
pub fn opt_nonblank(s: Option<String>) -> Option<String> {
    s.filter(|v| !v.trim().is_empty())
}

pub fn save(cfg: &ServerConfig) -> io::Result<()> {
    let port = cfg.port.unwrap_or(8080);
    let hostname = cfg.hostname.as_deref().unwrap_or("localhost");
    let mlock = cfg.mlock.unwrap_or(true);

    let models_dir = cfg.models_dir_or_default();

    // Each optional field renders as either `Key = value` or, when unset/default,
    // a commented `; Key = example  ; help` hint line (see int_line_or_hint /
    // str_line_or_hint). The `keep` predicate is where the per-field "is this
    // worth writing?" rule lives (n > 0, n != 1, …).
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
        |_| true,
    );
    let threads_batch_line = int_line_or_hint(
        cfg.threads_batch,
        "ThreadsBatch",
        "; ThreadsBatch = 12  ; optional override; auto-detected if commented",
        |n| n > 0,
    );
    let models_max_line = int_line_or_hint(
        cfg.models_max,
        "ModelsMax",
        "; ModelsMax = 2  ; uncomment to allow N models resident at once (0 = unlimited; runtime default if unset: 1)",
        |n| n != 1,
    );
    let device_line = str_line_or_hint(
        cfg.device.as_deref(),
        "Device",
        "; Device = CUDA0  ; pin the main model to one GPU (--device); blank = all detected devices",
    );
    let split_mode_line = str_line_or_hint(
        cfg.split_mode.as_deref(),
        "SplitMode",
        "; SplitMode = layer  ; multi-GPU split (--split-mode): none|layer|row; blank = layer (default)",
    );
    let tensor_split_line = str_line_or_hint(
        cfg.tensor_split.as_deref(),
        "TensorSplit",
        "; TensorSplit = 3,1  ; per-GPU weight proportions (--tensor-split); blank = even split",
    );

    let mlock_lit = if mlock { "true" } else { "false" };

    let body = format!(
        "; Generated by llama-cpp-config.
;
; llama-server runtime configuration (machine-wide).
; Per-model knobs live in presets.ini.

[Server]
Port = {port}
Hostname = {hostname}
Mlock = {mlock_lit}
{threads_line}
{cache_reuse_line}
{threads_batch_line}
{models_max_line}
{device_line}
{split_mode_line}
{tensor_split_line}

; ModelsDir: root directory. Models are scanned from ModelsDir/models/,
; mmproj projection files from ModelsDir/mmprojs/, MTP/draft heads from ModelsDir/mtps/.
ModelsDir = {models_dir}
"
    );

    let path = paths::server_ini();
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    if !models_dir.is_empty() {
        let _ = fs::create_dir_all(&models_dir);
    }
    fs::write(&path, body)
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
