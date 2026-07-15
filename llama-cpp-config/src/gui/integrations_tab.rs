//! Integrations-tab callback wiring (the opencode.json model list + Claude Code
//! snippet). Helpers live in the parent `gui` module; `use super::*` pulls them
//! in. The initial seed (`refresh_integrations`) runs in `gui::run()` alongside
//! the other tabs' seeds; `wire()` here is pure callback attachment.

use super::*;

pub(super) fn wire(app: &AppWindow) {
    {
        let app_weak = app.as_weak();
        app.global::<AppState>()
            .on_toggle_integration_model(move |index| {
                let Some(app) = app_weak.upgrade() else {
                    return;
                };
                let models = app.global::<AppState>().get_integration_models();
                let Ok(idx) = usize::try_from(index) else {
                    return;
                };
                // Flip the one row in place rather than rebuilding the whole model.
                // SAFETY OF THE ONE-WAY BINDING: the row CheckBox binds
                // `checked: item.enabled` one-way, and clicking it self-assigns
                // `checked` — permanently breaking that delegate's binding (the
                // "overwritten bindings" class). That stays invisible ONLY
                // because this in-place write originates from the clicked row's
                // own widget, whose broken binding already shows the new value.
                // Any OTHER enabled-state change (an "Enable all" button, a
                // partial refresh) must rebuild the whole model instead — see
                // refresh_integrations, which replaces the ModelRc so the `for`
                // delegates are recreated with fresh bindings.
                if let Some(mut entry) = models.row_data(idx) {
                    entry.enabled = !entry.enabled;
                    models.set_row_data(idx, entry);
                }
            });
    }
    {
        let app_weak = app.as_weak();
        app.global::<AppState>().on_save_integrations(move || {
            let Some(app) = app_weak.upgrade() else {
                return;
            };
            let s = app.global::<AppState>();
            let checked: Vec<String> = s
                .get_integration_models()
                .iter()
                .filter(|m| m.enabled)
                .map(|m| m.id.to_string())
                .collect();
            let cfg = server_cfg::load();
            let base_url = cfg.opencode_base_url_or_default();
            let api_key = cfg.opencode_api_key.as_deref();
            match integrations::save_opencode_models(&checked, &base_url, api_key) {
                Ok(()) => {
                    set_status(&app, "Saved model list to opencode.json.".into(), false);
                    refresh_integrations_reset(&app);
                }
                Err(e) => set_status(&app, format!("Save failed: {e}"), true),
            }
        });
    }
    {
        let app_weak = app.as_weak();
        app.global::<AppState>().on_revert_integrations(move || {
            let Some(app) = app_weak.upgrade() else {
                return;
            };
            refresh_integrations_reset(&app);
            set_status(&app, "Reloaded integration state.".into(), false);
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
