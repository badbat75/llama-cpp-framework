//! Server-tab callback wiring (the server.ini editor), plus the nav-rail run
//! controls (`start_server` / `stop_server`: thin wrappers over the parent's
//! `start_server_async` / `stop_server_async`, which the tray shares).
//!
//! Shared state, generated Slint types, and the `load_*` / `refresh_*` / `set_status`
//! helpers all live in the parent `gui` module; `use super::*` pulls them in.

use super::*;
use crate::strings;

/// Where the server file is, to show in the status line: the seam's answer.
fn server_ini_path() -> std::path::PathBuf {
    crate::server_control::backend().config_path(crate::server_control::ConfigFile::Server)
}

pub(super) fn wire(app: &AppWindow, tray: &AppTray, state: &Rc<RefCell<State>>) {
    {
        let app_weak = app.as_weak();
        let state = state.clone();
        app.global::<AppState>().on_save_server(move || {
            let Some(app) = app_weak.upgrade() else {
                return;
            };
            let s = app.global::<AppState>();
            let form = s.get_server_form();
            // Before the conversion, which reads a mistyped integer as unset:
            // that would hand the key back to llama.cpp's default unasked.
            let bad = server_form::invalid_numbers(&form);
            if !bad.is_empty() {
                set_status(
                    &app,
                    strings::fill("not-saved-invalid", &[&bad.join("; ")]),
                    true,
                );
                return;
            }
            let cfg = server_form::form_to_config(&form);
            match server_cfg::save(&cfg) {
                Ok(()) => {
                    // A RUNNING server keeps the config it was launched with
                    // (the save only changes the file), so surface the restart
                    // step instead of implying the change is live.
                    let msg = if s.get_server_running() {
                        strings::fill("server-saved-restart", &[&server_ini_path().display()])
                    } else {
                        strings::fill("server-saved", &[&server_ini_path().display()])
                    };
                    set_status(&app, msg, false);
                    // Re-derive the saved-config projections (Command Line
                    // card + chat URL) from the file just written.
                    refresh_server_snapshot(&app);
                    refresh_file_options(&app, &state);
                    // The Base URL and API key opencode.json carries come from
                    // server.ini: the Integrations tab names the change (its
                    // Save writes it) and the Claude Code snippet follows.
                    refresh_integrations(&app);
                    snapshot_server_base(&app);
                }
                Err(e) => set_status(&app, strings::fill("save-failed", &[&e]), true),
            }
        });
    }
    {
        let app_weak = app.as_weak();
        app.global::<AppState>().on_revert_server(move || {
            let Some(app) = app_weak.upgrade() else {
                return;
            };
            load_server_into_ui(&app);
            // No refresh_file_options / refresh_integrations here: a form
            // revert never touches disk, and both hubs derive from the SAVED
            // config, so there is nothing for them to re-derive.
            set_status(
                &app,
                strings::fill("server-reloaded", &[&server_ini_path().display()]),
                false,
            );
        });
    }
    // Browse callback needs nothing from `app`; it works purely on its argument,
    // which is why one handler serves every path field on the tab. An EMPTY
    // argument (a field the user left unset; under the Windows manager
    // server_form::config_to_form_for blanks the unset models and snapshot
    // dirs) starts the picker at server_cfg::models_dir().
    app.global::<AppState>().on_browse_dir(move |current| {
        let start = if !current.is_empty() {
            PathBuf::from(current.as_str())
        } else {
            PathBuf::from(server_cfg::models_dir())
        };
        pick_dir(&start)
            .map(|p| SharedString::from(p.to_string_lossy().into_owned()))
            .unwrap_or(current)
    });

    wire_tables(app);

    {
        let app_weak = app.as_weak();
        let tray_weak = tray.as_weak();
        app.global::<AppState>().on_start_server(move || {
            start_server_async(app_weak.clone(), tray_weak.clone());
        });
    }
    {
        let app_weak = app.as_weak();
        let tray_weak = tray.as_weak();
        app.global::<AppState>().on_stop_server(move || {
            stop_server_async(app_weak.clone(), tray_weak.clone());
        });
    }
}

/// The Server tab's bind-address table. Split out of `wire` because it needs nothing but the window (no tray, no
/// shared `State`), which is what lets the e2e harness wire them without
/// fabricating a tray (`gui::wire_tabs_for_tests`). The rest of `wire` (Save,
/// Revert, Start/Stop) does need both, and stays there.
pub(super) fn wire_tables(app: &AppWindow) {
    wire_bind_rows(app);
}

/// The "Bind to" checkboxes. `server_form.hostname` IS the state (a comma-
/// separated `--host` list); a click rewrites it through `net_ifaces::toggle`
/// and REBUILDS the rows, because checking 0.0.0.0 changes rows the user did not
/// click and the delegates' one-way `checked` only survives a rebuild.
fn wire_bind_rows(app: &AppWindow) {
    let app_weak = app.as_weak();
    app.global::<AppState>().on_bind_toggle(move |value| {
        let Some(app) = app_weak.upgrade() else {
            return;
        };
        let s = app.global::<AppState>();
        let mut f = s.get_server_form();
        let hosts = net_ifaces::toggle(f.hostname.as_str(), value.as_str());
        f.hostname = hosts.as_str().into();
        s.set_server_form(f);
        populate_bind_options(&app, &hosts);
    });
}

/// Native folder picker for the "Browse…" button, seeded at `start`. Server-tab
/// only, so it lives here rather than in the shared hub. Deliberately BLOCKS
/// the UI thread (the sanctioned exception to gui.rs's threading contract): a
/// native modal dialog is supposed to hold its owner, and pumping our loop
/// underneath it would let the 5 s status tick repaint a window the user can't
/// interact with anyway.
fn pick_dir(start: &std::path::Path) -> Option<PathBuf> {
    rfd::FileDialog::new()
        .set_title(strings::t("pick-a-folder"))
        .set_directory(start)
        .pick_folder()
}

/// Re-baseline the server form after a save so `server_dirty` reads false until
/// the next edit: the server analog of `apply_form`'s base handling.
fn snapshot_server_base(app: &AppWindow) {
    let s = app.global::<AppState>();
    s.set_server_form_base(s.get_server_form());
}

/// E2E phase, for the shared UI test to call (`gui::server_tab_e2e`): the
/// Server tab under a manager that withheld the API key (the user may not
/// configure) says a key is set that only an administrator can see, and
/// leaves the key field unset, so a save without a key typed keeps the
/// manager's (`server_control`). With the key read in full, no such note.
/// The note is pinned by whether a search finds it: it must be built only
/// when withheld (an `if` in the page), since a `visible: false` element
/// still answers a search.
#[cfg(test)]
pub(super) fn e2e() {
    use crate::server_control::fake::{self, FakeClient};
    use i_slint_backend_testing::ElementHandle;
    let dir = tempfile::tempdir().expect("tempdir");
    crate::paths::redirect_for_tests(dir.path());

    let app = AppWindow::new().expect("build AppWindow");
    install_strings(app.global::<Strings>());
    app.window().set_size(slint::PhysicalSize::new(1400, 3200));
    app.show().expect("realize window");
    let st = app.global::<AppState>();
    st.set_current_tab(0); // Server
    let note = strings::t("api-key-withheld");
    let shown = |label: &str| {
        ElementHandle::find_by_accessible_label(&app, label)
            .next()
            .is_some()
    };

    for withhold in [true, false] {
        let log = fake::log();
        let text = if withhold {
            "[Server]\nPort = 8099\nOpencodeApiKey =\n"
        } else {
            "[Server]\nPort = 8099\nOpencodeApiKey = sk-local-3f9a\n"
        };
        let c = FakeClient::new(&log, pai_core::client::InstanceState::Inactive)
            .with_file("server@default.ini", text);
        *c.withhold.lock().unwrap() = withhold;
        fake::scoped(fake::backend(c), || load_server_into_ui(&app));
        let form = st.get_server_form();
        assert_eq!(form.port.as_str(), "8099", "loaded: {withhold}");
        if withhold {
            assert!(form.api_key_nokey, "withheld: the key field left unset");
            assert_eq!(form.opencode_api_key.as_str(), "");
        } else {
            assert!(!form.api_key_nokey);
            assert_eq!(form.opencode_api_key.as_str(), "sk-local-3f9a");
        }
        assert_eq!(shown(&note), withhold, "the note when withheld: {withhold}");
    }
    assert_eq!(
        note, "A key is set. Only an administrator can see it.",
        "the note is in the strings file"
    );

    // An administrator's second save keeps the key too. On Linux polkit's
    // auth_admin_keep lets the quiet check pass after the first save, so the
    // read that follows it comes back in full while the form still holds no
    // key: the second write must still ask the manager to keep its own.
    {
        let log = fake::log();
        let c = FakeClient::new(&log, pai_core::client::InstanceState::Inactive).with_file(
            "server@default.ini",
            "[Server]\nPort = 8099\nOpencodeApiKey =\n",
        );
        let withhold = c.withhold.clone();
        *withhold.lock().unwrap() = true;
        let kept = format!("write_file server@default.ini {}", fake::KEEP_API_KEY);
        // What the Save button does (`on_save_server`, wired only with a
        // tray): the form to a config, the save, then the snapshot re-read.
        let save = |app: &AppWindow| {
            let cfg = server_form::form_to_config(&app.global::<AppState>().get_server_form());
            server_cfg::save(&cfg).expect("save");
            refresh_server_snapshot(app);
        };
        fake::scoped(fake::backend(c), || {
            load_server_into_ui(&app);
            assert!(st.get_server_form().api_key_nokey, "loaded withheld");
            // The first save's configure check (auth_admin_keep) is what lets
            // every quiet check pass from here on, the save's own re-read
            // included.
            *withhold.lock().unwrap() = false;
            save(&app);
            assert_eq!(
                fake::writes(&log),
                std::slice::from_ref(&kept),
                "first save"
            );

            log.lock().unwrap().clear();
            let mut form = st.get_server_form();
            form.port = "8098".into();
            st.set_server_form(form);
            save(&app);
            assert_eq!(
                fake::writes(&log),
                std::slice::from_ref(&kept),
                "second save"
            );
        });
    }

    // ── The "Host memory as GTT" checkbox (GT3) ──────────────────────────
    //
    // The Server tab's widget for `RocmHostMemoryGtt`, mirroring the Web UI's
    // (fe/briefs/25-rocm-host-memory.md) and this tab's own Web UI toggle: a
    // plain bool CheckBox, so the key's ABSENCE shows it CHECKED (the launch
    // exports the HSA pair when the key is unset) and the Save writes the bool
    // the box shows, `true` or `false`. Pinned through the widget and through
    // the bytes the Save hands the manager: those are the two halves a user
    // sees, and the pair is what a widget wired to the wrong state breaks.
    {
        let log = fake::log();
        let c = FakeClient::new(&log, pai_core::client::InstanceState::Inactive)
            .with_file("server@default.ini", "[Server]\nPort = 8099\n");
        let files = c.files.clone();
        // The keys the strings file carries (en_US.toml): the row's label, the
        // box's own text (a CheckBox's accessible name), and the help text.
        let toggle = strings::t("rocm-gtt-toggle");
        let label = strings::t("rocm-gtt");
        fake::scoped(fake::backend(c), || {
            load_server_into_ui(&app);
            let box_ = ElementHandle::find_by_accessible_label(&app, &toggle)
                .next()
                .unwrap_or_else(|| panic!("no checkbox with accessible-label {toggle:?}"));
            assert_eq!(
                box_.accessible_checked(),
                Some(true),
                "a file with no RocmHostMemoryGtt line is the on state: the box is checked"
            );
            assert!(
                ElementHandle::find_by_accessible_label(&app, &label)
                    .next()
                    .is_some(),
                "the row shows its label {label:?}"
            );
            let hint = strings::t("rocm-gtt-hint");
            assert_ne!(
                hint, "rocm-gtt-hint",
                "the row has a help text in the strings file"
            );
            assert!(hint.contains("GTT"), "the help text names GTT: {hint}");
            box_.invoke_accessible_default_action();
            assert_eq!(
                box_.accessible_checked(),
                Some(false),
                "the box toggles off, and the form follows it"
            );
            let cfg = server_form::form_to_config(&st.get_server_form());
            server_cfg::save(&cfg).expect("save");
            let text = files
                .lock()
                .unwrap()
                .get("server@default.ini")
                .cloned()
                .expect("the file the fake holds");
            assert_eq!(
                server_cfg::from_text(&text).rocm_host_memory_gtt,
                Some(false),
                "unchecking must reach the file as false:\n{text}"
            );
        });
    }

    // The box mirrors the file, it does not reset to on: a file that says
    // `false` shows it unchecked, and a Save that touches nothing keeps false.
    {
        let log = fake::log();
        let c = FakeClient::new(&log, pai_core::client::InstanceState::Inactive).with_file(
            "server@default.ini",
            "[Server]\nPort = 8099\nRocmHostMemoryGtt = false\n",
        );
        let files = c.files.clone();
        let toggle = strings::t("rocm-gtt-toggle");
        fake::scoped(fake::backend(c), || {
            load_server_into_ui(&app);
            let box_ = ElementHandle::find_by_accessible_label(&app, &toggle)
                .next()
                .unwrap_or_else(|| panic!("no checkbox with accessible-label {toggle:?}"));
            assert_eq!(
                box_.accessible_checked(),
                Some(false),
                "a file with RocmHostMemoryGtt = false shows the box unchecked"
            );
            server_cfg::save(&server_form::form_to_config(&st.get_server_form())).expect("save");
            let text = files
                .lock()
                .unwrap()
                .get("server@default.ini")
                .cloned()
                .expect("the file the fake holds");
            assert_eq!(
                server_cfg::from_text(&text).rocm_host_memory_gtt,
                Some(false),
                "a Save that does not touch the box keeps false:\n{text}"
            );
        });
    }
}
