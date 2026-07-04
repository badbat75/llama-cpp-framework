// server.ini schema and IO for llama.cpp-framework.
//
// `save()` rewrites the whole FILE from a `ServerConfig` (server.ini is fully
// generated — unlike presets.ini, hand-added content outside the template does
// not survive a save). Unset optional fields are emitted as commented
// `; Key = example  ; help` hint lines (never omitted) so the file
// self-documents every available knob.
//
// ADD A SERVER FIELD — the mirror of the preset recipe (top of presets.rs), one
// tier smaller but reaching the CLI (the server has a per-field `set`). Trace an
// existing field like `threads` (PascalCase INI key ↔ snake_case Rust field):
//   1. `ServerConfig` struct field (+ doc)   — below
//   2. `from_keys` (backs `load`)            — INI read; `keys.get("Key")` → field
//   3. `render` (backs `save`)               — INI write; an `int_line_or_hint` /
//      `str_line_or_hint` line + a `{…_line}` slot in the `[Server]` body
//   4. `ServerForm` struct                   — ui/types.slint
//   5. the input widget                      — ui/server_page.slint (bind two-way `<=>`)
//   6. `config_to_form` + `form_to_config`   — src/server_form.rs (BOTH directions)
//   7. THREE spots in src/cli.rs             — the `ServerSet` flag field, a
//      `row(...)` in `show_lines`, and `ServerSet::apply` copying the flag into `cfg`
// Guards: the save→load round-trip test in this file (steps 2–3: a key-name typo
// or wrong `keep` rule fails it), the form round-trip in server_form.rs
// (`form_to_config(config_to_form(c)) == c`, step 6), and cli.rs's
// `server_set_apply_copies_every_field` + `show_lines_prints_every_field`
// (step 7 — `apply`'s exhaustive struct literal breaks compilation when a
// `ServerSet` field is added but the test isn't extended).

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
            .clone()
            .filter(|s| !s.trim().is_empty())
            .unwrap_or_else(default_models_dir)
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
            .clone()
            .filter(|s| !s.trim().is_empty())
            .unwrap_or_else(|| "localhost".into())
    }

    /// The configured mlock flag, or `true` (the framework default) when unset.
    pub fn mlock_or_default(&self) -> bool {
        self.mlock.unwrap_or(true)
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
        threads: keys.get("Threads").and_then(|v| ini::parse_int(v)),
        cache_reuse: keys.get("CacheReuse").and_then(|v| ini::parse_int(v)),
        threads_batch: keys.get("ThreadsBatch").and_then(|v| ini::parse_int(v)),
        models_max: keys.get("ModelsMax").and_then(|v| ini::parse_int(v)),
        models_dir: opt_nonblank(keys.get("ModelsDir").cloned()),
        device: opt_nonblank(keys.get("Device").cloned()),
        split_mode: opt_nonblank(keys.get("SplitMode").cloned()),
        tensor_split: opt_nonblank(keys.get("TensorSplit").cloned()),
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

pub fn save(cfg: &ServerConfig) -> io::Result<()> {
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

/// Render the whole server.ini body. Pure (no IO) — the file-writing wrapper is
/// `save()`; the round-trip test drives this directly, mirroring
/// `presets::render_section`.
fn render(cfg: &ServerConfig) -> String {
    let port = cfg.port_or_default();
    let hostname = cfg.hostname_or_default();
    let mlock = cfg.mlock_or_default();

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
        |n| n > 0,
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

    format!(
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
            threads: Some(12),
            cache_reuse: Some(256),
            threads_batch: Some(24),
            models_max: Some(2),
            models_dir: Some(r"E:\llm".into()),
            device: Some("CUDA0".into()),
            split_mode: Some("row".into()),
            tensor_split: Some("3,1".into()),
        };
        assert_eq!(round_trip(&original), original);
    }

    // The writer's DELIBERATE collapses, pinned as documented behavior:
    // - Port/Hostname/Mlock are always written, so an unset one reloads as the
    //   default (8080 / localhost / true) rather than None.
    // - `keep` predicates turn "not worth writing" values into commented hint
    //   lines: threads/threads_batch <= 0, models_max == 1.
    // - ModelsDir falls back to default_models_dir() when unset.
    #[test]
    fn default_config_reloads_as_explicit_defaults() {
        let reloaded = round_trip(&ServerConfig::default());
        assert_eq!(reloaded.port, Some(8080));
        assert_eq!(reloaded.hostname.as_deref(), Some("localhost"));
        assert_eq!(reloaded.mlock, Some(true));
        assert_eq!(reloaded.threads, None);
        assert_eq!(reloaded.threads_batch, None);
        assert_eq!(reloaded.cache_reuse, None);
        assert_eq!(reloaded.models_max, None);
        assert_eq!(reloaded.device, None);
        assert_eq!(reloaded.split_mode, None);
        assert_eq!(reloaded.tensor_split, None);
        assert_eq!(reloaded.models_dir, Some(default_models_dir()));
    }

    #[test]
    fn not_worth_writing_values_collapse_to_hints() {
        let cfg = ServerConfig {
            threads: Some(0),
            cache_reuse: Some(-3),
            threads_batch: Some(-4),
            models_max: Some(1),
            device: Some("   ".into()),
            ..Default::default()
        };
        let reloaded = round_trip(&cfg);
        assert_eq!(reloaded.threads, None, "threads <= 0 is auto");
        assert_eq!(
            reloaded.cache_reuse, None,
            "cache_reuse <= 0 clears the override (same rule as the CLI set)"
        );
        assert_eq!(reloaded.threads_batch, None, "threads_batch <= 0 is auto");
        assert_eq!(reloaded.models_max, None, "models_max == 1 is the default");
        assert_eq!(reloaded.device, None, "blank device is unset");
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
}
