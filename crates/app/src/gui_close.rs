//! The close request `pai control stop-and-close` sends a running GUI on
//! Windows (the MSI's graceful stop): a named auto-reset event the GUI's
//! single-instance guard waits on (pai-config's `single_instance.rs`). The
//! name lives here only, so the side that waits and the side that signals
//! cannot drift apart.

#![cfg(windows)]

use windows_sys::Win32::Foundation::CloseHandle;
use windows_sys::Win32::System::Threading::{CreateEventW, SetEvent};

/// Per session (`Local\`), as the GUI's other single-instance names.
pub const CLOSE_EVENT_NAME: &str = r"Local\org.privateai.Config.close";

/// Signal the close request to the primary instance of the configurator.
pub fn signal_close() {
    let name: Vec<u16> = CLOSE_EVENT_NAME
        .encode_utf16()
        .chain(std::iter::once(0))
        .collect();
    unsafe {
        let event = CreateEventW(std::ptr::null(), 0, 0, name.as_ptr());
        if !event.is_null() {
            SetEvent(event);
            CloseHandle(event);
        }
    }
}
