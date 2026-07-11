//! Conversion between the Slint server form (`ServerForm`) and the
//! `server_cfg::ServerConfig` schema — the server-side mirror of `form.rs`.
//! Kept out of `gui.rs` (which only shuttles the whole `ServerForm`, never
//! per-field) so adding a server field touches this file plus `ServerForm`
//! (ui/types.slint), the widget (ui/server_page.slint), `ServerConfig`
//! (server_cfg.rs), and the CLI (three spots in cli.rs — the full checklist
//! lives at the top of server_cfg.rs) — not the GUI wiring.

use crate::gui::ServerForm;
use crate::server_cfg;

/// `ServerConfig` → the editable form. Materializes the display defaults the UI
/// needs for always-present controls (the models dir, and the split-mode combo's
/// "default" sentinel); the port / cache-reuse / models-max now carry a
/// `*_default` checkbox (checked = omit the flag → fall back to llama.cpp's
/// own default), with the int seeded from the schema default so the disabled
/// SpinBox shows a hint; the thread counts map `None` → the slider's "auto"
/// flag. `form_to_config` reverses each of these.
pub fn config_to_form(cfg: &server_cfg::ServerConfig) -> ServerForm {
    ServerForm {
        port: cfg.port.unwrap_or(8080),
        port_default: cfg.port.is_none(),
        hostname: cfg.hostname_or_default().into(),
        mlock: cfg.mlock_or_default(),
        no_mmap: cfg.no_mmap_or_default(),
        // Thread counts are auto-flagged sliders: unset ⇒ "auto" (omit the flag).
        threads: cfg.threads.unwrap_or(0),
        threads_auto: cfg.threads.is_none(),
        cache_reuse: cfg.cache_reuse.unwrap_or(0),
        cache_reuse_default: cfg.cache_reuse.is_none(),
        threads_batch: cfg.threads_batch.unwrap_or(0),
        threads_batch_auto: cfg.threads_batch.is_none(),
        // Seed the disabled SpinBox with llama.cpp's own default (4) so the
        // "default" hint the user sees matches what omitting the flag yields.
        models_max: cfg.models_max.unwrap_or(4),
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
        // Plain bool toggles (framework defaults materialized when unset), same
        // shape as `mlock`.
        webui_mcp_proxy: cfg.webui_mcp_proxy_or_default(),
        fit: cfg.fit_or_default(),
        // Always has a value (framework default 4 when unset) — a plain SpinBox,
        // no "default" checkbox: the launch always passes -lv.
        log_verbosity: cfg.log_verbosity_or_default(),
    }
}

/// The editable form → `ServerConfig`. The `*_default` checkbox decides
/// None/Some for port / cache-reuse / models-max (checked ⇒ None); the thread
/// counts use their existing `_auto` flags; blank / sentinel string fields
/// collapse back to `None` (server.ini's `save` renders those as commented
/// hint lines).
pub fn form_to_config(f: &ServerForm) -> server_cfg::ServerConfig {
    server_cfg::ServerConfig {
        port: if f.port_default {
            None
        } else {
            Some(f.port).filter(|v| *v > 0)
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
            Some(f.cache_reuse).filter(|v| *v > 0)
        },
        threads_batch: if f.threads_batch_auto {
            None
        } else {
            Some(f.threads_batch)
        },
        models_max: if f.models_max_default {
            None
        } else {
            Some(f.models_max)
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
        webui_mcp_proxy: Some(f.webui_mcp_proxy),
        fit: Some(f.fit),
        log_verbosity: Some(f.log_verbosity),
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
            device: Some("CUDA0".into()),
            split_mode: Some("row".into()),
            tensor_split: Some("3,1".into()),
            webui_mcp_proxy: Some(false),
            fit: Some(true),
            log_verbosity: Some(2),
        };
        assert_eq!(form_to_config(&config_to_form(&cfg)), cfg);
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
}
