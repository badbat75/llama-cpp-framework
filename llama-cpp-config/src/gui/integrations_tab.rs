//! Integrations-tab callback wiring (opencode.json + the Claude Code snippet).
//! Helpers live in the parent `gui` module; `use super::*` pulls them in. The
//! initial seed (`refresh_integrations`) runs in `gui::run()` alongside the other
//! tabs' seeds; `wire()` here is pure callback attachment.
//!
//! There is no model list to edit here: OpenCode lists every ENABLED preset (the
//! switch beside each one in the Models tab), see the `integrations` module
//! header. opencode.json is written by this tab's Save alone, never by a preset
//! change, and Save first shows a preview (`integrations::opencode_plan`): one
//! ticked row per model whose entry would change. Unticking a row leaves that
//! model's entry as the file holds it.

use crate::integrations::{ChangeKind, FieldDiff, ModelChange, OpencodePlan};
use crate::strings;
use super::*;

/// The preview's models and their ticks, between Save and the confirm.
type Pending = Rc<RefCell<Vec<(ModelChange, bool)>>>;

pub(super) fn wire(app: &AppWindow) {
    let pending: Pending = Rc::new(RefCell::new(Vec::new()));
    {
        let app_weak = app.as_weak();
        let pending = pending.clone();
        app.global::<AppState>().on_save_integrations(move || {
            let Some(app) = app_weak.upgrade() else {
                return;
            };
            let cfg = server_cfg::load();
            let plan = match integrations::opencode_plan(&cfg.opencode_base_url_or_default(), cfg.opencode_api_key.as_deref()) {
                Ok(plan) => plan,
                Err(e) => {
                    set_status(&app, strings::fill("save-failed", &[&format!("{e:#}")]), true);
                    return;
                }
            };
            if plan.is_empty() {
                set_status(&app, strings::t("opencode-up-to-date"), false);
                refresh_integrations(&app);
                return;
            }
            let s = app.global::<AppState>();
            s.set_opencode_provider_summary(provider_summary(&plan).into());
            *pending.borrow_mut() = plan.models.into_iter().map(|c| (c, true)).collect();
            push_rows(&app, &pending);
            s.set_show_opencode_dialog(true);
        });
    }
    {
        let app_weak = app.as_weak();
        let pending = pending.clone();
        app.global::<AppState>().on_opencode_change_toggle(move |i| {
            let Some(app) = app_weak.upgrade() else {
                return;
            };
            if let Some(row) = pending.borrow_mut().get_mut(i as usize) {
                row.1 = !row.1;
            }
            push_rows(&app, &pending);
        });
    }
    {
        let app_weak = app.as_weak();
        let pending = pending.clone();
        app.global::<AppState>().on_opencode_confirm(move || {
            let Some(app) = app_weak.upgrade() else {
                return;
            };
            app.global::<AppState>().set_show_opencode_dialog(false);
            let keep: Vec<String> =
                pending.borrow_mut().drain(..).filter(|(_, ticked)| !ticked).map(|(c, _)| c.id).collect();
            let cfg = server_cfg::load();
            match integrations::save_opencode_models(
                &cfg.opencode_base_url_or_default(),
                cfg.opencode_api_key.as_deref(),
                &keep,
            ) {
                Ok(()) => set_status(&app, strings::t("opencode-saved"), false),
                Err(e) => set_status(&app, strings::fill("save-failed", &[&format!("{e:#}")]), true),
            }
            refresh_integrations(&app);
        });
    }
    {
        let app_weak = app.as_weak();
        app.global::<AppState>().on_opencode_cancel(move || {
            if let Some(app) = app_weak.upgrade() {
                pending.borrow_mut().clear();
                app.global::<AppState>().set_show_opencode_dialog(false);
            }
        });
    }
    {
        app.global::<AppState>().on_open_opencode_folder(|| {
            let path = paths::opencode_user_config();
            if let Some(parent) = path.parent() {
                let _ = paths::open_in_desktop(parent);
            }
        });
    }
}

/// Rebuild the preview's rows from `pending` (the ticks included).
fn push_rows(app: &AppWindow, pending: &Pending) {
    let pending = pending.borrow();
    let rows: Vec<OpencodeChangeRow> = pending
        .iter()
        .map(|(c, ticked)| OpencodeChangeRow {
            label: change_label(c).into(),
            detail: change_detail(c).into(),
            checked: *ticked,
        })
        .collect();
    let s = app.global::<AppState>();
    s.set_opencode_ticked(pending.iter().filter(|(_, t)| *t).count() as i32);
    s.set_opencode_change_rows(model(rows));
}

fn change_label(c: &ModelChange) -> String {
    match c.kind {
        ChangeKind::Add => strings::fill("opencode-change-add", &[&c.id]),
        ChangeKind::Remove => strings::fill("opencode-change-remove", &[&c.id]),
        ChangeKind::Update => strings::fill("opencode-change-update", &[&c.id]),
    }
}

fn change_detail(c: &ModelChange) -> String {
    if c.kind == ChangeKind::Remove {
        return strings::t("opencode-remove-detail");
    }
    fields_text(&c.fields)
}

/// `path: old → new`, `;`-separated; an absent side reads as such, and the API
/// key's values are masked (the preview is on screen, the key is a secret).
fn fields_text(fields: &[FieldDiff]) -> String {
    let none = strings::t("opencode-field-absent");
    let hidden = strings::t("opencode-key-hidden");
    fields
        .iter()
        .map(|f| {
            let shown = |v: &Option<String>| match v {
                None => none.clone(),
                Some(_) if f.path.ends_with("apiKey") => hidden.clone(),
                Some(v) => v.clone(),
            };
            strings::fill("opencode-field-diff", &[&f.path, &shown(&f.old), &shown(&f.new)])
        })
        .collect::<Vec<_>>()
        .join("; ")
}

/// The provider line under the preview's rows: empty when the provider's own
/// fields stay as they are.
fn provider_summary(plan: &OpencodePlan) -> String {
    if plan.provider.is_empty() {
        return String::new();
    }
    let fields = fields_text(&plan.provider);
    if plan.provider_active {
        strings::fill("opencode-provider-update", &[&fields])
    } else {
        strings::fill("opencode-provider-create", &[&fields])
    }
}
