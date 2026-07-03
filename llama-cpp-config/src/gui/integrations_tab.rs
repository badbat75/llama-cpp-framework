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
            let base_url = s.get_integration_base_url().to_string();
            match integrations::save_opencode_models(&checked, &base_url) {
                Ok(()) => {
                    set_status(&app, "Saved model list to opencode.json.".into(), false);
                    refresh_integrations(&app);
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
            refresh_integrations(&app);
            set_status(&app, "Reloaded integration state.".into(), false);
        });
    }
}
