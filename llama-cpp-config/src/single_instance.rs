// Windows single-instance guard for the GUI.
//
// A named mutex marks "the configurator is already running". A second launch
// finds the mutex already taken, pokes a named auto-reset event so the live
// instance surfaces its window (which may be hidden in the tray or minimized),
// then exits without building a second UI. The first instance owns the mutex +
// event for its whole lifetime and runs a tiny background thread that waits on
// the event and calls back into the UI thread.
//
// Implemented with raw Win32 FFI (no extra crates), matching the style of
// `main.rs`'s `attach_parent_console`.

#![cfg(windows)]

use core::ffi::c_void;

type Handle = *mut c_void;

const ERROR_ALREADY_EXISTS: u32 = 183;
const WAIT_OBJECT_0: u32 = 0;
const INFINITE: u32 = 0xFFFF_FFFF;
// AllowSetForegroundWindow(ASFW_ANY): let *any* process steal the foreground,
// so the already-running instance can raise itself when we poke it.
const ASFW_ANY: u32 = 0xFFFF_FFFF;

#[link(name = "kernel32")]
extern "system" {
    fn CreateMutexW(attr: *const c_void, initial_owner: i32, name: *const u16) -> Handle;
    fn CreateEventW(
        attr: *const c_void,
        manual_reset: i32,
        initial_state: i32,
        name: *const u16,
    ) -> Handle;
    fn SetEvent(handle: Handle) -> i32;
    fn WaitForSingleObject(handle: Handle, millis: u32) -> u32;
    fn CloseHandle(handle: Handle) -> i32;
    fn GetLastError() -> u32;
}

#[link(name = "user32")]
extern "system" {
    fn AllowSetForegroundWindow(process_id: u32) -> i32;
}

// Per-session names (`Local\`): the goal is one running instance per user
// session, which is also what survives fast-user-switching / RDP cleanly.
const MUTEX_NAME: &str = r"Local\llama-cpp-config.singleton";
const EVENT_NAME: &str = r"Local\llama-cpp-config.activate";

fn wide(s: &str) -> Vec<u16> {
    s.encode_utf16().chain(std::iter::once(0)).collect()
}

pub enum Acquire {
    /// We are the only instance. Keep the guard alive for the whole process.
    Primary(InstanceGuard),
    /// Another instance was already running; it has been asked to surface.
    Secondary,
}

/// Holds the singleton mutex + activation event open for the process lifetime.
pub struct InstanceGuard {
    mutex: Handle,
    event: Handle,
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
        Acquire::Primary(InstanceGuard { mutex, event })
    }
}

impl InstanceGuard {
    /// Spawn a background thread that runs `on_activate` each time another
    /// launch pokes the activation event.
    pub fn spawn_listener<F: Fn() + Send + 'static>(&self, on_activate: F) {
        // HANDLE isn't Send; move it across as an integer and rebuild it. It
        // stays valid because `self` (the owner) lives for the whole process.
        let event = self.event as usize;
        if event == 0 {
            return;
        }
        std::thread::spawn(move || {
            let event = event as Handle;
            loop {
                // SAFETY: the handle is open until the guard drops at exit; a
                // closed/failed wait returns a non-signalled code and ends the loop.
                if unsafe { WaitForSingleObject(event, INFINITE) } != WAIT_OBJECT_0 {
                    break;
                }
                on_activate();
            }
        });
    }
}

impl Drop for InstanceGuard {
    fn drop(&mut self) {
        unsafe {
            if !self.event.is_null() {
                CloseHandle(self.event);
            }
            if !self.mutex.is_null() {
                CloseHandle(self.mutex);
            }
        }
    }
}
