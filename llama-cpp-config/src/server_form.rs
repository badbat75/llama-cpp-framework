// Conversion between the Slint server form (`ServerForm`) and the
// `server_cfg::ServerConfig` schema — the server-side mirror of `form.rs`.
// Kept out of `gui.rs` (which only shuttles the whole `ServerForm`, never
// per-field) so adding a server field touches this file plus `ServerForm`
// (ui/types.slint), the widget (ui/server_page.slint), `ServerConfig`
// (server_cfg.rs), and the CLI (three spots in cli.rs — the full checklist
// lives at the top of server_cfg.rs) — not the GUI wiring. Numerics ride as
// blank-able strings, like the preset form.

use crate::form::txt;
use crate::gui::ServerForm;
use crate::{ini, server_cfg};

/// `ServerConfig` → the editable form. Materializes the display defaults the UI
/// needs for always-present controls (port "8080", the models dir, and the
/// split-mode combo's "default" sentinel); the blank-able fields stay "" when
/// unset, and the thread counts map `None` → the slider's "auto" flag.
/// `form_to_config` reverses each of these.
pub fn config_to_form(cfg: &server_cfg::ServerConfig) -> ServerForm {
    ServerForm {
        // The display defaults for the always-present trio have ONE owner:
        // the *_or_default helpers on ServerConfig (see server_cfg.rs).
        port: cfg.port_or_default().to_string().into(),
        hostname: cfg.hostname_or_default().into(),
        mlock: cfg.mlock_or_default(),
        // Thread counts are auto-flagged sliders: unset ⇒ "auto" (omit the flag).
        threads: cfg.threads.unwrap_or(0),
        threads_auto: cfg.threads.is_none(),
        cache_reuse: txt(cfg.cache_reuse),
        threads_batch: cfg.threads_batch.unwrap_or(0),
        threads_batch_auto: cfg.threads_batch.is_none(),
        models_max: txt(cfg.models_max),
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
    }
}

/// The editable form → `ServerConfig`. Blank / sentinel string fields collapse
/// back to `None` (server.ini's `save` renders those as commented hint lines).
pub fn form_to_config(f: &ServerForm) -> server_cfg::ServerConfig {
    server_cfg::ServerConfig {
        port: ini::parse_int(f.port.as_str()),
        hostname: Some(f.hostname.to_string()),
        mlock: Some(f.mlock),
        // "auto" ⇒ omit the flag; otherwise the slider's value.
        threads: if f.threads_auto {
            None
        } else {
            Some(f.threads)
        },
        // Same "0 or negative clears the override" rule as the CLI's set and
        // save()'s keep predicate — all three legs must agree.
        cache_reuse: ini::parse_int(f.cache_reuse.as_str()).filter(|v| *v > 0),
        threads_batch: if f.threads_batch_auto {
            None
        } else {
            Some(f.threads_batch)
        },
        models_max: ini::parse_int(f.models_max.as_str()),
        models_dir: Some(f.models_dir.to_string()),
        device: server_cfg::opt_nonblank(Some(f.device.to_string())),
        // "" and the combo sentinel "default" both mean "no explicit split".
        split_mode: match f.split_mode.as_str() {
            "" | "default" => None,
            other => Some(other.to_string()),
        },
        tensor_split: server_cfg::opt_nonblank(Some(f.tensor_split.to_string())),
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
            threads: Some(8),
            cache_reuse: Some(256),
            threads_batch: Some(12),
            models_max: Some(2),
            models_dir: Some(r"E:\models".into()),
            device: Some("CUDA0".into()),
            split_mode: Some("row".into()),
            tensor_split: Some("3,1".into()),
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
