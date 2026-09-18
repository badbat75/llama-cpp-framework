//! Integrations-tab callback wiring (opencode.json + the Claude Code snippet).
//! Helpers live in the parent `gui` module; `use super::*` pulls them in. The
//! initial seed (`refresh_integrations`) runs in `gui::run()` alongside the other
//! tabs' seeds; `wire()` here is pure callback attachment.
//!
//! There is no model list to edit here any more: OpenCode lists every ENABLED
//! preset (the switch beside each one in the Models tab), see the
//! `integrations` module header. Save is what creates the provider section, and
//! what brings a drifted file back in line; every later preset change follows
//! on its own (`gui::follow_integrations`).

use super::*;

pub(super) fn wire(app: &AppWindow) {
    {
        let app_weak = app.as_weak();
        app.global::<AppState>().on_save_integrations(move || {
            let Some(app) = app_weak.upgrade() else {
                return;
            };
            let cfg = server_cfg::load();
            let base_url = cfg.opencode_base_url_or_default();
            let api_key = cfg.opencode_api_key.as_deref();
            match integrations::save_opencode_models(&base_url, api_key) {
                Ok(()) => set_status(
                    &app,
                    "Saved the enabled presets to opencode.json.".into(),
                    false,
                ),
                Err(e) => set_status(&app, format!("Save failed: {e}"), true),
            }
            refresh_integrations(&app);
        });
    }
    {
        app.global::<AppState>().on_open_opencode_folder(|| {
            let path = paths::opencode_user_config();
            if let Some(parent) = path.parent() {
                let _ = std::process::Command::new("explorer").arg(parent).spawn();
            }
        });
    }
}
