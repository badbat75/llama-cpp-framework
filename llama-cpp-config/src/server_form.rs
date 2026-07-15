//! Conversion between the Slint server form (`ServerForm`) and the
//! `server_cfg::ServerConfig` schema — the server-side mirror of `form.rs`.
//! Kept out of `gui.rs` (which only shuttles the whole `ServerForm`, never
//! per-field) so adding a server field touches this file plus `ServerForm`
//! (ui/types.slint), the widget (ui/server_page.slint), `ServerConfig`
//! (server_cfg.rs), and the CLI (three spots in cli.rs — the full checklist
//! lives at the top of server_cfg.rs) — not the GUI wiring.

use slint::SharedString;

use crate::gui::ServerForm;
use crate::ini;
use crate::server_cfg;

/// An optional int as the text its `DefaultLineEdit` shows, falling back to
/// `hint` when the key is unset — the server-side twin of `form::itxt` (and its
/// doc explains why these numerics are text and not a SpinBox `value`: Slint's
/// SpinBox edits itself on a stray scroll of the page).
fn itxt(v: Option<i32>, hint: i32) -> SharedString {
    v.unwrap_or(hint).to_string().into()
}

/// The `-lv` threshold's whole domain, as the dropdown shows it. llama.cpp defines
/// exactly these six and prints a message when its level is `<=` the threshold
/// (`common/arg.cpp`'s `--verbosity` help, `common/log.h`'s `LOG_LEVEL_*`), so a
/// free-number field could only ever offer levels that do not exist.
///
/// Mirrors `Options.log_levels` in ui/components.slint — same strings, same order.
/// The e2e (`src/tests/ui_bindings.rs`) asserts the two lists are equal, because a
/// label here that the dropdown does not contain would leave the combo showing its
/// first entry (OUTPUT:0) and quietly SAVE that on the next write.
pub(crate) const LOG_LEVELS: [(&str, i32); 6] = [
    ("OUTPUT:0", 0),
    ("ERROR:1", 1),
    ("WARN:2", 2),
    ("INFO:3", 3),
    ("TRACE:4", 4),
    ("DEBUG:5", 5),
];

/// An `-lv` value as its dropdown label. Out-of-range values are CLAMPED into the
/// six, which is behaviour-preserving where it matters: a hand-edited `LogVerbosity
/// = 7` prints exactly what `5` prints (nothing in llama.cpp logs above DEBUG), so
/// it shows — and re-saves — as DEBUG:5 rather than as a level that does not exist.
fn log_level_label(v: i32) -> SharedString {
    let clamped = v.clamp(LOG_LEVELS[0].1, LOG_LEVELS[LOG_LEVELS.len() - 1].1);
    LOG_LEVELS
        .iter()
        .find(|(_, n)| *n == clamped)
        .map(|(label, _)| SharedString::from(*label))
        .unwrap_or_else(|| SharedString::from(LOG_LEVELS[4].0))
}

/// The dropdown label back to the `-lv` number, the inverse of `log_level_label`.
/// An unknown label (only a hand-edited form could produce one) falls back to the
/// framework default rather than to 0 — silence is the one outcome nobody asked for.
fn parse_log_level(label: &str) -> i32 {
    LOG_LEVELS
        .iter()
        .find(|(name, _)| *name == label)
        .map(|(_, n)| *n)
        .unwrap_or_else(|| server_cfg::ServerConfig::default().log_verbosity_or_default())
}

/// `ServerConfig` → the editable form. Materializes the display defaults the UI
/// needs for always-present controls (the models dir, and the split-mode combo's
/// "default" sentinel); the port / cache-reuse / models-max carry a `*_default`
/// checkbox (checked = omit the flag → fall back to llama.cpp's own default), with
/// the text seeded from that same default so the disabled field shows a hint; the
/// thread counts map `None` → the slider's "auto" flag. `form_to_config` reverses
/// each of these.
pub fn config_to_form(cfg: &server_cfg::ServerConfig) -> ServerForm {
    ServerForm {
        port: itxt(cfg.port, 8080),
        port_default: cfg.port.is_none(),
        hostname: cfg.hostname_or_default().into(),
        mlock: cfg.mlock_or_default(),
        no_mmap: cfg.no_mmap_or_default(),
        // Thread counts are auto-flagged sliders: unset ⇒ "auto" (omit the flag).
        threads: cfg.threads.unwrap_or(0),
        threads_auto: cfg.threads.is_none(),
        cache_reuse: itxt(cfg.cache_reuse, 0),
        cache_reuse_default: cfg.cache_reuse.is_none(),
        threads_batch: cfg.threads_batch.unwrap_or(0),
        threads_batch_auto: cfg.threads_batch.is_none(),
        // Seed the disabled field with llama.cpp's own default (4) so the
        // "default" hint the user sees matches what omitting the flag yields.
        models_max: itxt(cfg.models_max, 4),
        models_max_default: cfg.models_max.is_none(),
        // Same "blank ⇒ default dir" rule as save()/start(), so a hand-edited
        // blank ModelsDir shows the default it will actually resolve to.
        models_dir: cfg.models_dir_or_default().into(),
        device: cfg.device.clone().unwrap_or_default().into(),
        // "default" is the combo's sentinel for "inherit / layer" — it two-way-
        // binds to this, so store the sentinel rather than "".
        split_mode: cfg
            .split_mode
            .clone()
            .unwrap_or_else(|| "default".into())
            .into(),
        tensor_split: cfg.tensor_split.clone().unwrap_or_default().into(),
        // Driven by the tensor-placement table, which reads and rewrites this one
        // string — no second copy of the rules exists (see src/tensor_override.rs).
        override_tensor: cfg.override_tensor.clone().unwrap_or_default().into(),
        mmproj_device: cfg.mmproj_device.clone().unwrap_or_default().into(),
        // Plain bool toggles (framework defaults materialized when unset), same
        // shape as `mlock`.
        webui_mcp_proxy: cfg.webui_mcp_proxy_or_default(),
        fit: cfg.fit_or_default(),
        prefill_assistant: cfg.prefill_assistant_or_default(),
        // Always has a value (framework default 4 when unset) — a dropdown, no
        // "default" checkbox: the launch always passes -lv. The form carries the
        // LABEL ("TRACE:4"), which is what the EnumComboBox matches on.
        log_verbosity: log_level_label(cfg.log_verbosity_or_default()),
        // Base URL: when unset, show auto-derived and check the default box.
        opencode_base_url: cfg
            .opencode_base_url
            .clone()
            .unwrap_or_else(|| cfg.opencode_base_url_or_default())
            .into(),
        base_url_default: cfg.opencode_base_url.is_none(),
        // API Key: when unset, show empty and check the nokey box.
        opencode_api_key: cfg.opencode_api_key.clone().unwrap_or_default().into(),
        api_key_nokey: cfg.opencode_api_key.is_none(),
    }
}

/// The editable form → `ServerConfig`. The `*_default` checkbox decides
/// None/Some for port / cache-reuse / models-max (checked ⇒ None); the thread
/// counts use their existing `_auto` flags; blank / sentinel string fields
/// collapse back to `None` (server.ini's `save` renders those as commented
/// hint lines).
pub fn form_to_config(f: &ServerForm) -> server_cfg::ServerConfig {
    server_cfg::ServerConfig {
        // The numerics are TEXT on the form (see `itxt`), so each is re-parsed:
        // unparseable or blank reads as unset (⇒ llama.cpp's own default), and the
        // range checks that were the SpinBox's `minimum`/`maximum` live here now —
        // a LineEdit cannot refuse a value the way the SpinBox did.
        port: if f.port_default {
            None
        } else {
            ini::parse_int(f.port.as_str()).filter(|v| (1..=65535).contains(v))
        },
        // Blank collapses to None like every optional string, matching what
        // the same input produces via `server set` / `load()` — a `Some("")`
        // here would only diverge the in-memory config, since every consumer
        // re-blanks it through `hostname_or_default`.
        hostname: server_cfg::opt_nonblank(Some(f.hostname.to_string())),
        mlock: Some(f.mlock),
        no_mmap: Some(f.no_mmap),
        // "auto" ⇒ omit the flag; otherwise the slider's value.
        threads: if f.threads_auto {
            None
        } else {
            Some(f.threads)
        },
        cache_reuse: if f.cache_reuse_default {
            None
        } else {
            ini::parse_int(f.cache_reuse.as_str()).filter(|v| *v > 0)
        },
        threads_batch: if f.threads_batch_auto {
            None
        } else {
            Some(f.threads_batch)
        },
        // 0 is a real value here (= unlimited), so no `> 0` filter.
        models_max: if f.models_max_default {
            None
        } else {
            ini::parse_int(f.models_max.as_str())
        },
        // Blank ⇒ None (fall back to the default dir), same rule as hostname.
        models_dir: server_cfg::opt_nonblank(Some(f.models_dir.to_string())),
        device: server_cfg::opt_nonblank(Some(f.device.to_string())),
        // "" and the combo sentinel "default" both mean "no explicit split".
        split_mode: match f.split_mode.as_str() {
            "" | "default" => None,
            other => Some(other.to_string()),
        },
        tensor_split: server_cfg::opt_nonblank(Some(f.tensor_split.to_string())),
        override_tensor: server_cfg::opt_nonblank(Some(f.override_tensor.to_string())),
        mmproj_device: server_cfg::opt_nonblank(Some(f.mmproj_device.to_string())),
        webui_mcp_proxy: Some(f.webui_mcp_proxy),
        fit: Some(f.fit),
        prefill_assistant: Some(f.prefill_assistant),
        // The one field with no "unset" state (the launch always passes -lv): the
        // form carries the dropdown's label, and `parse_log_level` maps it back.
        log_verbosity: Some(parse_log_level(f.log_verbosity.as_str())),
        // Default checked = None (auto-derived); unchecked = save the value.
        // Strip trailing slashes then /v1 so the convention "store without
        // suffix" holds even if the user typed it (e.g. https://gw.example.com/v1/).
        opencode_base_url: if f.base_url_default {
            None
        } else {
            let url = f.opencode_base_url.trim().trim_end_matches('/');
            let cleaned = url.strip_suffix("/v1").unwrap_or(url).trim_end_matches('/');
            server_cfg::opt_nonblank(Some(cleaned.to_string()))
        },
        // No key checked = None; unchecked = save the value.
        opencode_api_key: if f.api_key_nokey {
            None
        } else {
            server_cfg::opt_nonblank(Some(f.opencode_api_key.to_string()))
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::server_cfg::ServerConfig;

    // A fully-populated (all-`Some`, non-blank) config survives config → form →
    // config unchanged. The guard for the server-field fan-out: a field wired
    // into one conversion but not the other drops out here. (Configs with `None`
    // port / models_dir are NOT fixed points — the form materializes their
    // display defaults — which is why this uses an all-populated config.)
    #[test]
    fn rich_server_config_round_trips() {
        let cfg = ServerConfig {
            port: Some(9090),
            hostname: Some("0.0.0.0".into()),
            mlock: Some(false),
            no_mmap: Some(true),
            threads: Some(8),
            cache_reuse: Some(256),
            threads_batch: Some(12),
            models_max: Some(2),
            models_dir: Some(r"E:\models".into()),
            device: Some("ROCm1,CUDA0".into()),
            split_mode: Some("row".into()),
            tensor_split: Some("3,1".into()),
            override_tensor: Some(r"token_embd\.weight=ROCm1".into()),
            mmproj_device: Some("ROCm1".into()),
            webui_mcp_proxy: Some(false),
            fit: Some(true),
            // Non-default, so the round-trip is not vacuous for it.
            prefill_assistant: Some(false),
            log_verbosity: Some(2),
            opencode_base_url: Some("https://proxy.example.com".into()),
            opencode_api_key: Some("sk-test-key".into()),
        };
        assert_eq!(form_to_config(&config_to_form(&cfg)), cfg);
    }

    /// The log level is the one field the form carries as a LABEL rather than as
    /// its value, so the two halves of that mapping must compose back to identity
    /// for every level llama.cpp has — and the out-of-domain cases must land
    /// somewhere defensible rather than on OUTPUT:0 (which would silence the log).
    #[test]
    fn log_level_labels_round_trip_and_clamp() {
        for (label, n) in LOG_LEVELS {
            assert_eq!(log_level_label(n).as_str(), label);
            assert_eq!(parse_log_level(label), n);
        }
        // Above DEBUG there is nothing to print, so a hand-edited 7 IS DEBUG.
        assert_eq!(log_level_label(7).as_str(), "DEBUG:5");
        assert_eq!(log_level_label(-2).as_str(), "OUTPUT:0");
        // A label the dropdown can't produce falls back to the framework default,
        // never to 0 — losing the log entirely is the one outcome nobody wants.
        assert_eq!(parse_log_level("nonsense"), 4);
    }

    // The "default"/None ⇄ sentinel mapping specifically: an unset split-mode
    // becomes the "default" sentinel in the form and collapses back to None.
    #[test]
    fn split_mode_sentinel_round_trips() {
        let form = config_to_form(&ServerConfig {
            port: Some(8080),
            models_dir: Some(r"E:\models".into()),
            ..Default::default()
        });
        assert_eq!(form.split_mode.as_str(), "default");
        assert_eq!(form_to_config(&form).split_mode, None);
    }

    // Unset thread counts ⇄ the slider's "auto" flag: `None` becomes `*_auto` in
    // the form and collapses back to `None` (the slider value is ignored).
    #[test]
    fn threads_auto_round_trips() {
        let form = config_to_form(&ServerConfig {
            port: Some(8080),
            models_dir: Some(r"E:\models".into()),
            ..Default::default()
        });
        assert!(form.threads_auto && form.threads_batch_auto);
        let cfg = form_to_config(&form);
        assert_eq!(cfg.threads, None);
        assert_eq!(cfg.threads_batch, None);
    }

    /// form_to_config strips trailing /v1 and / from the Base URL so the
    /// convention "store without suffix" holds even when the user typed it.
    #[test]
    fn base_url_strips_trailing_v1_and_slash() {
        let cases = [
            ("https://gw.example.com/v1/", "https://gw.example.com"),
            ("https://gw.example.com/v1", "https://gw.example.com"),
            ("https://gw.example.com/", "https://gw.example.com"),
            ("https://gw.example.com", "https://gw.example.com"),
            ("https://gw.example.com/v1//", "https://gw.example.com"),
        ];
        for (input, expected) in cases {
            let form = ServerForm {
                opencode_base_url: input.into(),
                base_url_default: false,
                ..Default::default()
            };
            let cfg = form_to_config(&form);
            assert_eq!(
                cfg.opencode_base_url.as_deref(),
                Some(expected),
                "input: {input}"
            );
        }
    }
}
