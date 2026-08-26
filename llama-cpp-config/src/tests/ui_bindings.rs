//! End-to-end UI regression test, driven by Slint's testing backend.
//!
//! Guards the "editable widget goes stale after an edit" class of bug that
//! shipped in v1.1.1 and was fixed in v1.1.2: a one-way binding on an editable
//! widget (`text: AppState.x`) breaks the instant the user edits the field
//! (Slint's "overwritten bindings" rule), so a later model change (preset switch
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
//! binding" rule is per-kind, not per-field: LineEdit (`text`, which is now every
//! numeric field of both forms too, integers included; the SpinBox they used to be
//! is gone, see ui/components.slint),
//! CheckBox (`checked`) and Slider (`value`, via `AutoSlider`: the std Slider
//! imperatively self-assigns `value` on every drag/set, so it can never hold a
//! plain one-way binding; the AutoSlider's `changed shown` push is what this
//! test pins). ComboBox is out of scope: its only accessibility
//! action is "expand" (open the popup); changing the selection needs real popup
//! interaction under an event loop, which this no-event-loop harness can't drive.
//! `SegmentedControl` (the reasoning + reason-format pickers and the draft
//! on-GPU/on-CPU control) is safe by construction (it reads `current` purely and
//! never self-assigns), so it has no staleness mode for this test to catch (its
//! pills do expose `accessible-checked`, but only for assistive tech).
//!
//! Requires Slint element debug info, which build.rs emits for non-release
//! profiles only (see the `PROFILE` gate there); `cargo test --release` can't find
//! the widgets. It is ONE `#[test]` on purpose: every e2e phase shares it and
//! this window; topology rationale: `src/tests/mod.rs`.

use i_slint_backend_testing::{self as itest, ElementHandle};
use slint::{ComponentHandle, Model};

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
/// debug info). Panics with the label if nothing matches: a renamed/removed
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
/// widget could coincidentally match; see the CheckBox call site.
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
    edit(field); // imperative self-write: breaks a one-way binding
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

/// The "a wrapped value escapes its card" regression (shipped in v1.11.2, fixed
/// in v1.11.3): a long `Draft` value in the Models tab's "Model info" card was
/// painted OUTSIDE the card's rounded rectangle, over the heading of the card
/// below it.
///
/// Cause, and why only a geometry test can see it: Slint measures a wrapping
/// `Text` against a width the layout supplies, and across a component boundary
/// (`InfoRow` is one) that width is the component's own *preferred* one, i.e.
/// what the text would take on a single unwrapped line. A value wrapping to
/// three lines was therefore published as one, the card was sized two lines
/// short, and the rows kept painting at their real height. Every string involved
/// is correct, so no round-trip or binding test can catch it; only the rows'
/// position RELATIVE to their card gives it away. `ui/components.slint`'s
/// `WrapText` is the fix, and this is what pins it.
///
/// The values are set directly on `AppState` (the GGUF path needs real model
/// files) and cleared again, so the later phases see the state they expect.
fn assert_wrapped_info_row_stays_inside_its_card(app: &AppWindow, st: &AppState) {
    use i_slint_backend_testing::ElementQuery;

    // Long enough to wrap whatever width the window in `realized_app` gives the
    // editor column, and shaped like the real thing (the report was an MTP
    // drafter's line). ASCII separators: the glyph whitelist is not the subject
    // here, and a real `·` would tie this test to `binding_lint`'s RENDERABLE.
    st.set_model_info_ready(true);
    st.set_model_info_has_draft_file(true);
    st.set_model_info_draft(slint::SharedString::from(
        "MTP: 1 nextn layer / Qwen3.8-27B-Uncensored-HauhauCS-Aggressive-FastMTP-32K.gguf: \
         qwen35 / Q3_K_M / 65 layers / nextn 1, plus enough further text that this value has \
         to wrap over three lines even in a generously wide window, which is exactly the room \
         the card around it has to make",
    ));
    st.set_model_info_draft_file(slint::SharedString::from(
        "qwen35 / Q3_K_M / 65 layers / nextn 1",
    ));
    let check = |at: &str| {
        itest::mock_elapsed_time(std::time::Duration::from_millis(16));
        // The card is found by its heading: `SectionCard` starts AT its heading
        // Text, so the two share a top edge.
        let heading = ElementQuery::from_root(app)
            .match_descendants()
            .match_predicate(|e| e.accessible_label().is_some_and(|l| l == "Model info"))
            .find_first()
            .expect("no 'Model info' heading on the Models tab");
        let top = heading.absolute_position().y;
        let card = ElementQuery::from_root(app)
            .match_descendants()
            .match_inherits("SectionCard")
            .find_all()
            .into_iter()
            .find(|e| (e.absolute_position().y - top).abs() < 0.5)
            .expect("no SectionCard at the 'Model info' heading");
        let card_bottom = card.absolute_position().y + card.size().height;

        // The rows all live in that one card (the Models tab has no other
        // InfoRow), so the lowest bottom edge is the one to check.
        let rows = ElementQuery::from_root(app)
            .match_descendants()
            .match_inherits("InfoRow")
            .find_all();
        let wrapped = rows.iter().any(|e| e.size().height > 20.0);
        assert!(
            wrapped,
            "{at}: no InfoRow wrapped, so the fixture stopped exercising the bug"
        );
        let rows_bottom = rows
            .iter()
            .map(|e| e.absolute_position().y + e.size().height)
            .fold(f32::MIN, f32::max);
        assert!(
            rows_bottom <= card_bottom,
            "{at}: a Model-info row paints outside its card: rows end at \
             {rows_bottom}, the card at {card_bottom} (card top {top}, height {})",
            card.size().height
        );
    };

    // Both widths, because the whole defect is width-dependent: the narrow one
    // is the window's own `min-width` (ui/app.slint), i.e. the tightest the user
    // can pull the editor column, and the one the report came from.
    check("wide window");
    app.window().set_size(slint::PhysicalSize::new(888, 3200));
    check("narrow window");
    app.window().set_size(slint::PhysicalSize::new(1400, 3200));

    st.set_model_info_ready(false);
    st.set_model_info_has_draft_file(false);
    st.set_model_info_draft(slint::SharedString::from(""));
    st.set_model_info_draft_file(slint::SharedString::from(""));
    itest::mock_elapsed_time(std::time::Duration::from_millis(16));
}

#[test]
fn editable_widgets_track_model_after_edit() {
    let app = realized_app();
    let st = app.global::<AppState>();

    // The two homes of the "all layers on GPU" sentinel must agree: the
    // mirror comments in form.rs / components.slint can't fail a build.
    assert_eq!(
        app.global::<crate::gui::Options>().get_all_layers(),
        crate::form::ALL_LAYERS,
        "Options.all_layers (ui/components.slint) drifted from form::ALL_LAYERS"
    );

    // Same drift guard for the -lv dropdown: `server_form` maps a level to the
    // LABEL the combo shows, so a label it can't find leaves the combo on its
    // first entry (OUTPUT:0), and SAVES that on the next write, silencing the log.
    let slint_levels: Vec<String> = app
        .global::<crate::gui::Options>()
        .get_log_levels()
        .iter()
        .map(|s| s.to_string())
        .collect();
    let rust_levels: Vec<String> = crate::server_form::LOG_LEVELS
        .iter()
        .map(|(label, _)| (*label).to_string())
        .collect();
    assert_eq!(
        slint_levels, rust_levels,
        "Options.log_levels (ui/components.slint) drifted from server_form::LOG_LEVELS"
    );

    // Same drift guard for the -lm dropdown, where the stakes are one step
    // higher: its entries are passed to llama-server VERBATIM, so a mode the
    // Rust list has and the combo doesn't leaves the combo on "auto" and saves
    // that over the user's choice, while the reverse puts a value on the launch
    // line that llama-server rejects outright ("invalid value"); nothing starts.
    let slint_modes: Vec<String> = app
        .global::<crate::gui::Options>()
        .get_load_modes()
        .iter()
        .map(|s| s.to_string())
        .collect();
    assert_eq!(
        slint_modes,
        crate::server_cfg::LOAD_MODES.to_vec(),
        "Options.load_modes (ui/components.slint) drifted from server_cfg::LOAD_MODES"
    );

    // ── Server tab (shown by default) ────────────────────────────────
    // LineEdit (inside DefaultLineEdit): `text <=> AppState.server_form.port`.
    // Every numeric field of both forms is one of these since v1.5.0, integers
    // included: the SpinBox they used to be edits itself on a stray mouse-wheel
    // over the page (ui/components.slint spells it out). So the *kind* under test
    // here is the same as `form.temp` below: this case stays because it is the
    // only Server-tab numeric, and it pins the int-as-text conversion.
    assert_reload_reaches_widget(
        &by_label(&app, "server-port"),
        "LineEdit server_form.port",
        value_of,
        |e| e.set_accessible_value("9999"),
        |v| set_server_form(&st, |f| f.port = v.into()),
        "8080",
        "1234",
    );

    // CheckBox: `checked <=> AppState.server_form.webui_mcp_proxy`. The only
    // edit is a toggle, so the edit leaves the *opposite* of `load`. `reload`
    // therefore restores `load` (true→false→true): a frozen widget would sit on
    // the toggled value and mismatch. Found by its visible text (the checkbox's
    // accessible-label). Any plain-bool toggle pins the KIND: this one because
    // it also defaults to true, so the toggle actually changes something.
    assert_reload_reaches_widget(
        &by_label(&app, "serve the web UI's MCP proxy endpoint"),
        "CheckBox server_form.webui_mcp_proxy",
        |e| {
            e.accessible_checked()
                .map(|b| b.to_string())
                .unwrap_or_default()
        },
        |e| e.invoke_accessible_default_action(),
        |v| set_server_form(&st, |f| f.webui_mcp_proxy = v == "true"),
        "true",
        "true",
    );

    // Slider: the thumb inside the CPU-threads AutoSlider. The std Slider
    // self-assigns `value` on every user set (drag, keys, this accessibility
    // set-value), which killed the component's old one-way `value:` binding,
    // the v1.2.9 stale-thumb bug. External updates now reach it through
    // AutoSlider's `changed shown` push; this is the case that pins it.
    // The slider reports its accessible value as a float: normalize to int.
    // The push rides a `changed` callback, which Slint dispatches on the next
    // event-loop turn: mock one after each model write or the read races it.
    set_server_form(&st, |f| f.threads_auto = false);
    assert_reload_reaches_widget(
        &by_label(&app, "server-threads"),
        "Slider server_form.threads",
        |e| {
            value_of(e)
                .parse::<f64>()
                .map(|v| (v.round() as i64).to_string())
                .unwrap_or_default()
        },
        |e| e.set_accessible_value("20"),
        |v| {
            set_server_form(&st, |f| f.threads = v.parse().expect("int"));
            itest::mock_elapsed_time(std::time::Duration::from_millis(1));
        },
        "8",
        "16",
    );

    // ── Models tab ───────────────────────────────────────────────────
    // Switch pages, then run the backend's tree-instantiation pass so the
    // conditional `if current_tab == 1 : ModelsPage {}` actually materializes
    // (a bare property change doesn't rebuild the item tree without a render).
    st.set_current_tab(1);
    itest::mock_elapsed_time(std::time::Duration::from_millis(1));

    // A wrapped Model-info value must not paint outside its card (v1.11.3).
    assert_wrapped_info_row_stays_inside_its_card(&app, &st);

    // LineEdit: `text <=> AppState.form.ctx_size` (an INTEGER field: since v1.5.0
    // they are all text, see the note on the port above).
    assert_reload_reaches_widget(
        &by_label(&app, "preset-ctx-size"),
        "LineEdit ctx_size",
        value_of,
        |e| e.set_accessible_value("500"),
        |v| set_form(&st, |f| f.ctx_size = v.into()),
        "8192",
        "65536",
    );

    // LineEdit: `text <=> AppState.form.temp` (the field the v1.1.1 bug report
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

    // ── E2E save/revert/delete flow (src/tests/save_flow.rs) ─────────────
    // Shares this single #[test] and window: the testing backend is a
    // process-global, single-threaded platform (see the header note).
    super::save_flow::run(&app);
}
