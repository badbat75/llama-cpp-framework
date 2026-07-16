//! "Start with Windows" — the per-user logon Run registry entry.
//!
//! The Settings tab's startup toggles ARE this entry
//! (`HKCU\Software\Microsoft\Windows\CurrentVersion\Run`, value
//! `llama-cpp-config`): its presence is "start with Windows", and whether the
//! stored command carries `gui --minimized` (start in the tray) or plain `gui`
//! (open the window) is "start minimized". Both are READ BACK from the entry —
//! no INI mirror that Task Manager's Startup panel (which edits the same key)
//! could silently desync — and re-enabling the toggle after the install moved
//! is what refreshes the stored exe path.
//!
//! Raw Win32 FFI (no extra crates), matching `single_instance.rs`. On
//! non-Windows the toggles are unsupported: `is_enabled` is `false` and the UI
//! disables the checkboxes (`AppState.startup_supported`).

use std::io;

/// Whether this platform has a startup mechanism to toggle.
pub fn is_supported() -> bool {
    cfg!(windows)
}

/// The command line the Run entry launches: the quoted exe plus `gui`, with
/// `--minimized` when the logon launch should land in the tray instead of
/// opening the window. Pure; the registry write is `set_enabled`.
#[cfg_attr(not(windows), allow(dead_code))] // non-Windows keeps only the test caller
fn run_command(exe: &std::path::Path, minimized: bool) -> String {
    let flag = if minimized { " --minimized" } else { "" };
    format!("\"{}\" gui{flag}", exe.display())
}

#[cfg(windows)]
pub use win::{is_enabled, set_enabled, starts_minimized};

#[cfg(not(windows))]
pub fn is_enabled() -> bool {
    false
}

#[cfg(not(windows))]
pub fn starts_minimized() -> bool {
    false
}

#[cfg(not(windows))]
pub fn set_enabled(_on: bool, _minimized: bool) -> io::Result<()> {
    Err(io::Error::new(
        io::ErrorKind::Unsupported,
        "start-with-Windows is only available on Windows",
    ))
}

#[cfg(windows)]
mod win {
    use super::{io, run_command};
    use core::ffi::c_void;

    type Hkey = *mut c_void;

    // Predefined key handles are sentinel values, not real handles — no close.
    const HKEY_CURRENT_USER: Hkey = 0x8000_0001_u32 as usize as Hkey;
    const KEY_QUERY_VALUE: u32 = 0x0001;
    const KEY_SET_VALUE: u32 = 0x0002;
    const REG_SZ: u32 = 1;
    const ERROR_SUCCESS: i32 = 0;
    const ERROR_FILE_NOT_FOUND: i32 = 2;

    #[link(name = "advapi32")]
    extern "system" {
        fn RegOpenKeyExW(
            key: Hkey,
            sub_key: *const u16,
            options: u32,
            desired: u32,
            result: *mut Hkey,
        ) -> i32;
        fn RegQueryValueExW(
            key: Hkey,
            value_name: *const u16,
            reserved: *mut u32,
            kind: *mut u32,
            data: *mut u8,
            data_len: *mut u32,
        ) -> i32;
        fn RegSetValueExW(
            key: Hkey,
            value_name: *const u16,
            reserved: u32,
            kind: u32,
            data: *const u8,
            data_len: u32,
        ) -> i32;
        fn RegDeleteValueW(key: Hkey, value_name: *const u16) -> i32;
        fn RegCloseKey(key: Hkey) -> i32;
    }

    const RUN_KEY: &str = r"Software\Microsoft\Windows\CurrentVersion\Run";
    const VALUE_NAME: &str = "llama-cpp-config";

    fn wide(s: &str) -> Vec<u16> {
        s.encode_utf16().chain(std::iter::once(0)).collect()
    }

    /// Open the per-user Run key (it always exists on a working profile).
    fn open_run_key(desired: u32) -> io::Result<Hkey> {
        let sub_key = wide(RUN_KEY);
        let mut key: Hkey = std::ptr::null_mut();
        let ret =
            unsafe { RegOpenKeyExW(HKEY_CURRENT_USER, sub_key.as_ptr(), 0, desired, &mut key) };
        if ret == ERROR_SUCCESS {
            Ok(key)
        } else {
            Err(io::Error::from_raw_os_error(ret))
        }
    }

    /// The command the Run entry currently stores, or `None` when absent /
    /// unreadable. Both public reads derive from this, so the entry stays the
    /// single source of truth for the two startup toggles.
    fn stored_command() -> Option<String> {
        let key = open_run_key(KEY_QUERY_VALUE).ok()?;
        let name = wide(VALUE_NAME);
        // Two-call pattern: size the buffer, then read. A value grown between
        // the calls fails the second with ERROR_MORE_DATA and reads as None —
        // fine for a value only this app and the Startup panel touch.
        let mut kind: u32 = 0;
        let mut byte_len: u32 = 0;
        let sized = unsafe {
            RegQueryValueExW(
                key,
                name.as_ptr(),
                std::ptr::null_mut(),
                &mut kind,
                std::ptr::null_mut(),
                &mut byte_len,
            )
        };
        if sized != ERROR_SUCCESS || kind != REG_SZ {
            unsafe { RegCloseKey(key) };
            return None;
        }
        let mut buf: Vec<u16> = vec![0; byte_len as usize / 2 + 1];
        let ret = unsafe {
            RegQueryValueExW(
                key,
                name.as_ptr(),
                std::ptr::null_mut(),
                std::ptr::null_mut(),
                buf.as_mut_ptr().cast::<u8>(),
                &mut byte_len,
            )
        };
        unsafe { RegCloseKey(key) };
        if ret != ERROR_SUCCESS {
            return None;
        }
        let mut s = String::from_utf16_lossy(&buf[..byte_len as usize / 2]);
        while s.ends_with('\0') {
            s.pop();
        }
        Some(s)
    }

    /// Whether the logon Run entry exists. Presence is the state — the stored
    /// command isn't compared against the current exe path, so a moved install
    /// still reads as enabled (re-toggling refreshes the path).
    pub fn is_enabled() -> bool {
        stored_command().is_some()
    }

    /// Whether the stored command starts in the tray (`--minimized`). `false`
    /// when the entry is absent — the caller gates on `is_enabled` anyway.
    pub fn starts_minimized() -> bool {
        stored_command().is_some_and(|c| c.contains("--minimized"))
    }

    /// Create (or overwrite — an enable also refreshes a stale exe path and
    /// applies the current `minimized` choice) or delete the Run entry.
    /// Deleting an absent value is a success: the goal state, not the
    /// transition, is what the caller asked for.
    pub fn set_enabled(on: bool, minimized: bool) -> io::Result<()> {
        let key = open_run_key(KEY_SET_VALUE)?;
        let name = wide(VALUE_NAME);
        let ret = if on {
            let exe = std::env::current_exe()?;
            let data = wide(&run_command(&exe, minimized));
            let byte_len = u32::try_from(data.len() * 2)
                .map_err(|_| io::Error::new(io::ErrorKind::InvalidInput, "command too long"))?;
            unsafe {
                RegSetValueExW(
                    key,
                    name.as_ptr(),
                    0,
                    REG_SZ,
                    data.as_ptr().cast::<u8>(),
                    byte_len,
                )
            }
        } else {
            match unsafe { RegDeleteValueW(key, name.as_ptr()) } {
                ERROR_FILE_NOT_FOUND => ERROR_SUCCESS,
                other => other,
            }
        };
        unsafe { RegCloseKey(key) };
        if ret == ERROR_SUCCESS {
            Ok(())
        } else {
            Err(io::Error::from_raw_os_error(ret))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::run_command;

    // The exe path is quoted (Program Files has a space) and the launch goes
    // through `gui`, with `--minimized` only when the tray toggle asks for it.
    // The registry legs stay out of the normal run — thin FFI over a fixed key,
    // and `cargo test` must not write the developer's real HKCU Run key —
    // but see the #[ignore] round-trip below for a manual check.
    #[test]
    fn run_command_quotes_the_exe_and_carries_the_minimized_choice() {
        let exe = std::path::Path::new(r"C:\Program Files\llama.cpp\bin\llama-cpp-config.exe");
        assert_eq!(
            run_command(exe, true),
            r#""C:\Program Files\llama.cpp\bin\llama-cpp-config.exe" gui --minimized"#
        );
        assert_eq!(
            run_command(exe, false),
            r#""C:\Program Files\llama.cpp\bin\llama-cpp-config.exe" gui"#
        );
    }

    // MANUAL-ONLY (`cargo test startup -- --ignored`): drives the real FFI
    // against the real HKCU Run key, restoring the initial state either way.
    // Ignored by default exactly because it touches the developer's registry.
    #[cfg(windows)]
    #[test]
    #[ignore = "writes the real HKCU Run key; run explicitly with -- --ignored"]
    fn registry_round_trip_restores_initial_state() {
        let initially = super::is_enabled();
        let initially_min = super::starts_minimized();

        super::set_enabled(true, true).expect("enable minimized");
        assert!(super::is_enabled(), "value should exist after enable");
        assert!(super::starts_minimized(), "command should carry --minimized");

        super::set_enabled(true, false).expect("rewrite without --minimized");
        assert!(super::is_enabled());
        assert!(!super::starts_minimized(), "flag should be gone");

        super::set_enabled(false, false).expect("disable");
        assert!(!super::is_enabled(), "value should be gone after disable");
        // Double-disable must stay Ok (delete-absent is a success).
        super::set_enabled(false, false).expect("disable when already absent");

        if initially {
            super::set_enabled(true, initially_min).expect("restore");
        }
    }
}
