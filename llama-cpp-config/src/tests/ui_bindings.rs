//! End-to-end UI regression test, driven by Slint's testing backend.
//!
//! Guards the "editable widget goes stale after an edit" class of bug that
//! shipped in v1.1.1 and was fixed in v1.1.2: a one-way binding on an editable
//! widget (`text: AppState.x`) breaks the instant the user edits the field —
//! Slint's "overwritten bindings" rule — so a later model change (preset switch
//! or Revert) never reaches the widget. The fix is a two-way binding (`<=>`).
//!
//! Pure-Rust tests (form.rs round-trips) can't see this bug: it lives entirely in
//! the `.slint` binding direction and only manifests once a real widget performs
//! its internal write-back. This test builds the real `AppWindow`, simulates that
//! write-back through the widget's own accessibility action (the std widgets map
//! it to the same imperative property assignment a keystroke/click triggers), then
//! pushes a fresh model value and asserts the widget followed it.
//!
//! Coverage is one case per editable-widget *kind*, since the "overwritten
//! binding" rule is per-kind, not per-field: LineEdit (`text`), SpinBox (`value`)
//! and CheckBox (`checked`). ComboBox is out of scope — its only accessibility
//! action is "expand" (open the popup); changing the selection needs real popup
//! interaction under an event loop, which this no-event-loop harness can't drive.
//! `SegmentedControl` (the reasoning picker) is safe by construction — it reads
//! `current` purely and never self-assigns — and exposes no accessible value to
//! read back, so it isn't behaviorally testable here either.
//!
//! Requires Slint element debug info, which build.rs emits for non-release
//! profiles only (see the `PROFILE` gate there); `cargo test --release` can't find
//! the widgets. It is ONE `#[test]` on purpose: the testing backend is a
//! process-global, single-threaded platform, so a single window exercised
//! sequentially avoids cargo's parallel-test threads racing on it.

use i_slint_backend_testing::{self as itest, ElementHandle};
use slint::ComponentHandle;

use crate::gui::{AppState, AppWindow, PresetForm, ServerForm};

/// Build the window on the headless testing backend and realize its item tree so
/// the default page's widgets are materialized and findable. `init_no_event_loop`
/// sets a process-global platform; the `Once` keeps a re-run from re-setting it
/// (which panics).
fn realized_app() -> AppWindow {
    use std::sync::Once;
    static INIT: Once = Once::new();
    INIT.call_once(itest::init_no_event_loop);
    let app = AppWindow::new().expect("build AppWindow");
    // A generous size so the scrolling editor pages lay out their full content:
    // the item tree instantiates only what layout reaches, so a short window
    // leaves the lower cards (and their widgets) un-instantiated and unfindable.
    app.window().set_size(slint::PhysicalSize::new(1400, 3200));
    app.show().expect("realize window");
    app
}

/// Locate a widget by its `accessible-label`. Uses the accessibility tree, which
/// is always present (unlike element ids, which additionally need the id kept in
/// debug info). Panics with the label if nothing matches — a renamed/removed
/// widget should fail loudly, not silently skip its assertion.
fn by_label(app: &AppWindow, label: &str) -> ElementHandle {
    ElementHandle::find_by_accessible_label(app, label)
        .next()
        .unwrap_or_else(|| panic!("no widget with accessible-label {label:?}"))
}

/// The core invariant: after a simulated user edit, a *fresh* model value must
/// still reach the widget. `read` returns the widget's currently displayed value,
/// `edit` performs the imperative self-write that would break a one-way binding,
/// and `set_model` pushes a value from the Rust side (as a preset switch / Revert
/// does). With a one-way binding the widget freezes on the edited value and the
/// final assert fails; with `<=>` it tracks `reload`.
///
/// `load` and `reload` are the displayed-value strings before and after the
/// reload. They must differ from the value `edit` leaves behind, or a frozen
/// widget could coincidentally match — see the CheckBox call site.
fn assert_reload_reaches_widget(
    field: &ElementHandle,
    what: &str,
    read: impl Fn(&ElementHandle) -> String,
    edit: impl Fn(&ElementHandle),
    set_model: impl Fn(&str),
    load: &str,
    reload: &str,
) {
    set_model(load);
    assert_eq!(
        read(field),
        load,
        "{what}: widget should mirror the model on load"
    );
    edit(field); // imperative self-write — breaks a one-way binding
    set_model(reload);
    assert_eq!(
        read(field),
        reload,
        "{what}: after an edit the widget must still track a fresh model value"
    );
}

fn value_of(e: &ElementHandle) -> String {
    e.accessible_value().unwrap_or_default().to_string()
}

/// `form` is a single struct-typed property, so a field is changed by reading the
/// whole struct, mutating, and setting it back (there is no per-field setter).
fn set_form(st: &AppState, mutate: impl FnOnce(&mut PresetForm)) {
    let mut form = st.get_form();
    mutate(&mut form);
    st.set_form(form);
}

/// Same read-mutate-write dance for the server form (also one struct property).
fn set_server_form(st: &AppState, mutate: impl FnOnce(&mut ServerForm)) {
    let mut form = st.get_server_form();
    mutate(&mut form);
    st.set_server_form(form);
}

#[test]
fn editable_widgets_track_model_after_edit() {
    let app = realized_app();
    let st = app.global::<AppState>();

    // ── Server tab (shown by default) ────────────────────────────────
    // LineEdit — `text <=> AppState.server_form.port`.
    assert_reload_reaches_widget(
        &by_label(&app, "server-port"),
        "LineEdit server_form.port",
        value_of,
        |e| e.set_accessible_value("9999"),
        |v| set_server_form(&st, |f| f.port = v.into()),
        "8080",
        "1234",
    );

    // CheckBox — `checked <=> AppState.server_form.mlock`. The only edit is a
    // toggle, so the edit leaves the *opposite* of `load`. `reload` therefore
    // restores `load` (true→false→true): a frozen widget would sit on the toggled
    // value and mismatch. Found by its visible text (the checkbox's accessible-label).
    assert_reload_reaches_widget(
        &by_label(&app, "lock model in physical RAM"),
        "CheckBox server_form.mlock",
        |e| {
            e.accessible_checked()
                .map(|b| b.to_string())
                .unwrap_or_default()
        },
        |e| e.invoke_accessible_default_action(),
        |v| set_server_form(&st, |f| f.mlock = v == "true"),
        "true",
        "true",
    );

    // ── Models tab ───────────────────────────────────────────────────
    // Switch pages, then run the backend's tree-instantiation pass so the
    // conditional `if current_tab == 1 : ModelsPage {}` actually materializes
    // (a bare property change doesn't rebuild the item tree without a render).
    st.set_current_tab(1);
    itest::mock_elapsed_time(std::time::Duration::from_millis(1));

    // SpinBox — `value <=> AppState.form.ctx_size`.
    assert_reload_reaches_widget(
        &by_label(&app, "preset-ctx-size"),
        "SpinBox ctx_size",
        value_of,
        |e| e.set_accessible_value("500"),
        |v| set_form(&st, |f| f.ctx_size = v.parse().expect("int")),
        "8192",
        "65536",
    );

    // LineEdit — `text <=> AppState.form.temp` (the field the v1.1.1 bug report
    // named, alongside top-k).
    assert_reload_reaches_widget(
        &by_label(&app, "preset-temp"),
        "LineEdit form.temp",
        value_of,
        |e| e.set_accessible_value("9.9"),
        |v| set_form(&st, |f| f.temp = v.into()),
        "0.7",
        "0.2",
    );
}
