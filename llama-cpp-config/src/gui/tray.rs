//! System-tray callback wiring (open window / start / stop / quit). `AppTray` is
//! a separate Slint root that does NOT use `AppState`; Rust pushes its state
//! directly. Helpers live in the parent `gui` module (`use super::*`).

use super::*;

pub(super) fn wire(app: &AppWindow, tray: &AppTray) {
    {
        let app_weak = app.as_weak();
        tray.on_open_window(move || {
            if let Some(app) = app_weak.upgrade() {
                app.show().ok();
            }
        });
    }
    {
        let app_weak = app.as_weak();
        let tray_weak = tray.as_weak();
        tray.on_start_server(move || {
            start_server(app_weak.clone(), tray_weak.clone());
        });
    }
    {
        let app_weak = app.as_weak();
        let tray_weak = tray.as_weak();
        tray.on_stop_server(move || {
            stop_server_async(app_weak.clone(), tray_weak.clone());
        });
    }
    tray.on_quit_app(|| {
        slint::quit_event_loop().ok();
    });
}
