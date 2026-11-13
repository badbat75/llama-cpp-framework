//! Windows single-instance guard for the GUI.
//!
//! A named mutex marks "the configurator is already running". A second launch
//! finds the mutex already taken, pokes a named auto-reset event so the live
//! instance surfaces its window (which may be hidden in the tray or minimized),
//! then exits without building a second UI. The first instance owns the mutex +
//! event for its whole lifetime and runs a tiny background thread that waits on
//! the event and calls back into the UI thread.
//!
//! Win32 through `windows-sys`.

#![cfg(windows)]

use windows_sys::Win32::Foundation::{
    CloseHandle, GetLastError, ERROR_ALREADY_EXISTS, HANDLE as Handle, WAIT_OBJECT_0,
};
use windows_sys::Win32::System::Threading::{
    CreateEventW, CreateMutexW, SetEvent, WaitForMultipleObjects, INFINITE,
};
// AllowSetForegroundWindow(ASFW_ANY): let *any* process steal the foreground,
// so the already-running instance can raise itself when we poke it.
use windows_sys::Win32::UI::WindowsAndMessaging::{AllowSetForegroundWindow, ASFW_ANY};

// Per-session names (`Local\`): the goal is one running instance per user
// session, which is also what survives fast-user-switching / RDP cleanly.
const MUTEX_NAME: &str = r"Local\llama-cpp-config.singleton";
const EVENT_NAME: &str = r"Local\llama-cpp-config.activate";
const CLOSE_EVENT_NAME: &str = r"Local\llama-cpp-config.close";

fn wide(s: &str) -> Vec<u16> {
    s.encode_utf16().chain(std::iter::once(0)).collect()
}

pub enum Acquire {
    /// We are the only instance. Keep the guard alive for the whole process.
    Primary(InstanceGuard),
    /// Another instance was already running; it has been asked to surface.
    Secondary,
}

/// Holds the singleton mutex + activation event + close event open for the process lifetime.
pub struct InstanceGuard {
    mutex: Handle,
    event: Handle,
    close_event: Handle,
}

/// Try to become the single instance. If one is already running, poke it and
/// return [`Acquire::Secondary`].
pub fn acquire() -> Acquire {
    let mutex_name = wide(MUTEX_NAME);
    let event_name = wide(EVENT_NAME);
    unsafe {
        let mutex = CreateMutexW(std::ptr::null(), 0, mutex_name.as_ptr());
        // GetLastError must be read right after CreateMutexW; is_null() makes no
        // syscall so it can't clobber the error in between.
        if !mutex.is_null() && GetLastError() == ERROR_ALREADY_EXISTS {
            // CreateEventW on an existing name opens that event.
            let event = CreateEventW(std::ptr::null(), 0, 0, event_name.as_ptr());
            if !event.is_null() {
                AllowSetForegroundWindow(ASFW_ANY);
                SetEvent(event);
                CloseHandle(event);
            }
            CloseHandle(mutex);
            return Acquire::Secondary;
        }
        // First instance: own an auto-reset event for the process lifetime.
        let event = CreateEventW(std::ptr::null(), 0, 0, event_name.as_ptr());
        let close_event = CreateEventW(std::ptr::null(), 0, 0, wide(CLOSE_EVENT_NAME).as_ptr());
        Acquire::Primary(InstanceGuard {
            mutex,
            event,
            close_event,
        })
    }
}

impl InstanceGuard {
    /// Spawn a background thread that runs `on_event` each time another
    /// launch pokes the activation event or a close request is made.
    pub fn spawn_listener<F: Fn(Event) + Send + 'static>(&self, on_event: F) {
        let event = self.event as usize;
        let close_event = self.close_event as usize;
        if event == 0 || close_event == 0 {
            return;
        }
        std::thread::spawn(move || {
            let event = event as Handle;
            let close_event = close_event as Handle;
            loop {
                let ret = unsafe {
                    let handles = [event, close_event];
                    WaitForMultipleObjects(2, handles.as_ptr(), 0, INFINITE)
                };
                if ret == WAIT_OBJECT_0 {
                    on_event(Event::Activate);
                } else if ret == WAIT_OBJECT_0 + 1 {
                    on_event(Event::Close);
                } else {
                    break;
                }
            }
        });
    }
}

/// Signal the close request to the primary instance of the configurator.
pub fn signal_close() {
    let name = wide(CLOSE_EVENT_NAME);
    unsafe {
        let event = CreateEventW(std::ptr::null(), 0, 0, name.as_ptr());
        if !event.is_null() {
            SetEvent(event);
            CloseHandle(event);
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Event {
    Activate,
    Close,
}

impl Drop for InstanceGuard {
    fn drop(&mut self) {
        unsafe {
            if !self.close_event.is_null() {
                CloseHandle(self.close_event);
            }
            if !self.event.is_null() {
                CloseHandle(self.event);
            }
            if !self.mutex.is_null() {
                CloseHandle(self.mutex);
            }
        }
    }
}
