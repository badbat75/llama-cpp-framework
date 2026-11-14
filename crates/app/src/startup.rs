//! "Start at login": launch the configurator when the user's session starts.
//!
//! The Settings tab's startup toggles ARE the platform's own login entry, with
//! no INI mirror the system's own startup panel could silently desync:
//!
//! - **Windows**: the per-user Run registry entry
//!   (`HKCU\Software\Microsoft\Windows\CurrentVersion\Run`, value
//!   `org.privateai.Config`), the one Task Manager's Startup panel edits.
//! - **Linux**: an XDG autostart entry,
//!   `~/.config/autostart/org.privateai.Config.desktop` (named after the app id), which GNOME (and every
//!   other desktop following the spec) runs at login and which GNOME Tweaks'
//!   Startup Applications edits.
//!
//! On both, the entry's presence is "start at login", and whether the stored
//! command carries `gui --minimized` (start in the tray) or plain `gui` (open
//! the window) is "start minimized". Both are READ BACK from the entry, and
//! re-enabling the toggle after the install moved is what refreshes the stored
//! exe path.
//!
//! The entry registers the INSTALLED copy, not `current_exe()`: `set_enabled`
//! prefers the exe under `HKLM\Software\private-ai\InstallDir\bin` on Windows
//! and `/usr/bin/pai-config` (the deb/rpm's symlink) on Linux
//! (`installed_exe`), falling back to `current_exe()` only when no install is
//! found (portable / dev use). `current_exe()` alone is a footgun: toggling
//! the setting from a second, non-installed build (e.g. a `cargo run` debug exe)
//! would persist THAT path into the login startup, and on Windows the debug
//! build is console-subsystem (see `main.rs`), so every logon then flashes an
//! empty console window that owns the app (closing it kills the app).
//! Preferring the installed copy makes the toggle write the installed GUI
//! build regardless of which build flipped it.
//!
//! Windows: the registry through `windows-sys`.
//! Linux: a plain file. Elsewhere the toggles are unsupported: `is_enabled` is
//! `false` and the UI disables the checkboxes (`AppState.startup_supported`).

use std::io;

/// Whether this platform has a startup mechanism to toggle.
pub fn is_supported() -> bool {
    cfg!(any(windows, target_os = "linux"))
}

/// The command line the Windows Run entry launches: the quoted exe plus `gui`,
/// with `--minimized` when the logon launch should land in the tray instead of
/// opening the window. Pure; the registry write is `set_enabled`. (The Linux
/// entry quotes by the Desktop Entry rules instead: `linux::exec_line`.)
#[cfg_attr(not(windows), allow(dead_code))] // non-Windows keeps only the test caller
fn run_command(exe: &std::path::Path, minimized: bool) -> String {
    let flag = if minimized { " --minimized" } else { "" };
    format!("\"{}\" gui{flag}", exe.display())
}

/// Whether Windows lets a Run entry start, from its value in
/// `HKCU\Software\Microsoft\Windows\CurrentVersion\Explorer\StartupApproved\Run`
/// (named like the Run value), which Task Manager's Startup panel writes when
/// the entry is switched off or on. There is no Microsoft spec: the first byte
/// even is on (`0x02`, `0x06`), odd is off (`0x03`, then a FILETIME), the rule
/// Firefox's `WindowsLaunchOnLogin.sys.mjs` follows. A missing or empty value
/// is on. Pure, compiled on every OS so its test runs anywhere.
#[cfg_attr(not(windows), allow(dead_code))]
fn approved_on(data: Option<&[u8]>) -> bool {
    data.and_then(|d| d.first()).is_none_or(|b| b % 2 == 0)
}

/// The exe to register in the login startup: the installed copy when one is
/// found (`HKLM InstallDir` on Windows, `/usr/bin` on Linux), else the running
/// exe. Pure so the precedence is testable without touching the registry;
/// `set_enabled` supplies `installed` from `installed_exe()` and `current` from
/// `std::env::current_exe()`.
#[cfg_attr(not(any(windows, target_os = "linux")), allow(dead_code))]
fn choose_autostart_exe(
    installed: Option<std::path::PathBuf>,
    current: std::path::PathBuf,
) -> std::path::PathBuf {
    installed.unwrap_or(current)
}

/// Replace the login entry of release 2.4 and earlier
/// (`pai_core::migrate::OLD_LOGIN_ENTRY`: the HKCU Run value on Windows, the
/// `<name>.desktop` XDG autostart file on Linux) by the new one: when the old
/// entry exists it is removed and, when it was on, the new entry is enabled
/// with the same "start minimized" choice. `Ok(true)` when an old entry was
/// found; with none, nothing is created. Unsupported platforms: `Ok(false)`.
pub fn migrate_old_entry() -> io::Result<bool> {
    #[cfg(windows)]
    {
        win::migrate_old_entry()
    }
    #[cfg(target_os = "linux")]
    {
        let new = linux::entry_path();
        let old = new.with_file_name(format!("{}.desktop", pai_core::migrate::OLD_LOGIN_ENTRY));
        linux::migrate_entry(&old, &new)
    }
    #[cfg(not(any(windows, target_os = "linux")))]
    {
        Ok(false)
    }
}

#[cfg(windows)]
pub use win::{is_enabled, machine_reg_dword, machine_reg_sz, set_enabled, starts_minimized};

#[cfg(target_os = "linux")]
pub use linux::{is_enabled, set_enabled, starts_minimized};

#[cfg(not(any(windows, target_os = "linux")))]
pub fn is_enabled() -> bool {
    false
}

#[cfg(not(any(windows, target_os = "linux")))]
pub fn starts_minimized() -> bool {
    false
}

#[cfg(not(any(windows, target_os = "linux")))]
pub fn set_enabled(_on: bool, _minimized: bool) -> io::Result<()> {
    Err(io::Error::new(
        io::ErrorKind::Unsupported,
        crate::strings::t("login-start-unavailable"),
    ))
}

/// The XDG autostart entry (freedesktop.org Desktop Application Autostart
/// Specification): a `.desktop` file in `$XDG_CONFIG_HOME/autostart`, run by
/// the desktop session at login.
#[cfg(target_os = "linux")]
mod linux {
    use super::io;
    use std::path::{Path, PathBuf};

    const ENTRY_NAME: &str = "org.privateai.Config.desktop";

    /// Where the deb/rpm put the command. Registered as this path rather than
    /// the running executable's, so the entry survives a package that moves
    /// the real file.
    const INSTALLED_EXE: &str = "/usr/bin/pai-config";

    /// `$XDG_CONFIG_HOME/autostart/org.privateai.Config.desktop`, with the spec's
    /// `~/.config` default. Under the e2e redirect `home_dir()` is the temp
    /// tree and `XDG_CONFIG_HOME` is ignored, so a test can never write the
    /// developer's real autostart directory.
    pub(super) fn entry_path() -> PathBuf {
        let redirected = std::env::var_os("PRIVATE_AI_DATA_ROOT").is_some();
        let config = std::env::var_os("XDG_CONFIG_HOME")
            .filter(|v| !redirected && !v.is_empty())
            .map(PathBuf::from)
            .filter(|p| p.is_absolute())
            .unwrap_or_else(|| crate::paths::home_dir().join(".config"));
        config.join("autostart").join(ENTRY_NAME)
    }

    fn installed_exe() -> Option<PathBuf> {
        let exe = PathBuf::from(INSTALLED_EXE);
        exe.is_file().then_some(exe)
    }

    /// The `Exec` value: the exe quoted by the Desktop Entry rules (inside
    /// double quotes, `"`, `` ` ``, `$` and `\` take a backslash, and the
    /// backslash is doubled once more by the string escaping of the file
    /// format; `%` is a field code and is doubled), then `gui`, with
    /// `--minimized` when the login launch should land in the tray.
    pub(super) fn exec_line(exe: &Path, minimized: bool) -> String {
        let mut quoted = String::new();
        for c in exe.to_string_lossy().chars() {
            match c {
                '"' | '`' | '$' => {
                    quoted.push_str("\\\\");
                    quoted.push(c);
                }
                '\\' => quoted.push_str("\\\\\\\\"),
                '%' => quoted.push_str("%%"),
                _ => quoted.push(c),
            }
        }
        let flag = if minimized { " --minimized" } else { "" };
        format!("\"{quoted}\" gui{flag}")
    }

    /// The whole entry. `X-GNOME-Autostart-enabled` is written explicitly
    /// because it is the key GNOME's own startup panel flips to switch an
    /// entry off without deleting it.
    pub(super) fn entry_text(exe: &Path, minimized: bool) -> String {
        format!(
            "[Desktop Entry]\n\
             Type=Application\n\
             Name=Private AI Framework\n\
             Comment=Configure and run llama-server and its model presets\n\
             Exec={}\n\
             Icon=org.privateai.Config\n\
             Terminal=false\n\
             X-GNOME-Autostart-enabled=true\n",
            exec_line(exe, minimized)
        )
    }

    /// `(enabled, minimized)` as an entry's text states them. An entry the
    /// desktop was told to skip (`Hidden=true`, or GNOME's
    /// `X-GNOME-Autostart-enabled=false`) is NOT enabled, although the file is
    /// there: the checkbox shows what will happen at the next login.
    pub(super) fn parse_entry(text: &str) -> (bool, bool) {
        let value = |key: &str| {
            text.lines().find_map(|l| {
                let (k, v) = l.split_once('=')?;
                (k.trim() == key).then(|| v.trim().to_string())
            })
        };
        let Some(exec) = value("Exec") else {
            return (false, false);
        };
        let hidden = value("Hidden").is_some_and(|v| v.eq_ignore_ascii_case("true"));
        let gnome_off =
            value("X-GNOME-Autostart-enabled").is_some_and(|v| v.eq_ignore_ascii_case("false"));
        (!hidden && !gnome_off, exec.contains("--minimized"))
    }

    fn stored() -> (bool, bool) {
        std::fs::read_to_string(entry_path())
            .map(|t| parse_entry(&t))
            .unwrap_or((false, false))
    }

    /// Whether the autostart entry exists and is active.
    pub fn is_enabled() -> bool {
        stored().0
    }

    /// Whether the stored command starts in the tray (`--minimized`).
    pub fn starts_minimized() -> bool {
        stored().1
    }

    /// Write (an enable also refreshes a stale exe path and applies the
    /// current `minimized` choice) or delete the entry. Deleting an absent
    /// file is a success, as on Windows: the goal state is what was asked.
    pub fn set_enabled(on: bool, minimized: bool) -> io::Result<()> {
        write_entry(&entry_path(), on, minimized)
    }

    /// The Linux half of `migrate_old_entry`, on explicit paths: the old
    /// entry at `old` replaced by one at `new`.
    pub(super) fn migrate_entry(old: &Path, new: &Path) -> io::Result<bool> {
        let text = match std::fs::read_to_string(old) {
            Err(e) if e.kind() == io::ErrorKind::NotFound => return Ok(false),
            other => other?,
        };
        let (on, minimized) = parse_entry(&text);
        if on {
            write_entry(new, true, minimized)?;
        }
        std::fs::remove_file(old)?;
        Ok(true)
    }

    pub(super) fn write_entry(path: &Path, on: bool, minimized: bool) -> io::Result<()> {
        if on {
            let exe = super::choose_autostart_exe(installed_exe(), std::env::current_exe()?);
            if let Some(dir) = path.parent() {
                std::fs::create_dir_all(dir)?;
            }
            std::fs::write(path, entry_text(&exe, minimized))
        } else {
            match std::fs::remove_file(path) {
                Err(e) if e.kind() != io::ErrorKind::NotFound => Err(e),
                _ => Ok(()),
            }
        }
    }
}

#[cfg(windows)]
mod win {
    use super::{io, run_command};
    use windows_sys::Win32::Foundation::{ERROR_FILE_NOT_FOUND, ERROR_SUCCESS};
    // Predefined key handles are sentinel values, not real handles; no close.
    use windows_sys::Win32::System::Registry::{
        RegCloseKey, RegDeleteKeyValueW, RegDeleteValueW, RegGetValueW, RegOpenKeyExW,
        RegQueryValueExW, RegSetValueExW, HKEY as Hkey, HKEY_CURRENT_USER, HKEY_LOCAL_MACHINE,
        KEY_QUERY_VALUE, KEY_SET_VALUE, REG_DWORD, REG_SAM_FLAGS, REG_SZ, RRF_RT_REG_BINARY,
    };

    pub(super) const RUN_KEY: &str = r"Software\Microsoft\Windows\CurrentVersion\Run";
    /// Task Manager's on/off switch of each Run value (`super::approved_on`).
    const APPROVED_KEY: &str =
        r"Software\Microsoft\Windows\CurrentVersion\Explorer\StartupApproved\Run";
    const VALUE_NAME: &str = "org.privateai.Config";

    fn wide(s: &str) -> Vec<u16> {
        s.encode_utf16().chain(std::iter::once(0)).collect()
    }

    /// Open the per-user Run key (it always exists on a working profile).
    fn open_run_key(desired: REG_SAM_FLAGS) -> io::Result<Hkey> {
        let sub_key = wide(RUN_KEY);
        let mut key: Hkey = std::ptr::null_mut();
        let ret =
            unsafe { RegOpenKeyExW(HKEY_CURRENT_USER, sub_key.as_ptr(), 0, desired, &mut key) };
        if ret == ERROR_SUCCESS {
            Ok(key)
        } else {
            Err(io::Error::from_raw_os_error(ret as i32))
        }
    }

    /// Read a `REG_SZ` value from `root\subkey`, or `None` when the key/value is
    /// absent, unreadable, or not a string. Shared by `stored_command` (HKCU Run)
    /// and `install_dir` (HKLM InstallDir).
    pub(super) fn read_reg_sz(root: Hkey, subkey: &str, value: &str) -> Option<String> {
        let sub = wide(subkey);
        let mut key: Hkey = std::ptr::null_mut();
        if unsafe { RegOpenKeyExW(root, sub.as_ptr(), 0, KEY_QUERY_VALUE, &mut key) }
            != ERROR_SUCCESS
        {
            return None;
        }
        let name = wide(value);
        // Two-call pattern: size the buffer, then read. A value grown between
        // the calls fails the second with ERROR_MORE_DATA and reads as None.
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

    /// The command the Run entry currently stores, or `None` when absent /
    /// unreadable. Both public reads derive from this, so the entry stays the
    /// single source of truth for the two startup toggles.
    fn stored_command() -> Option<String> {
        read_reg_sz(HKEY_CURRENT_USER, RUN_KEY, VALUE_NAME)
    }

    /// Read one `REG_SZ` under `HKLM`, for callers outside this module. Exposed
    /// so the benchmark's environment stamp (`bench::env`) can read the display
    /// adapters' driver versions without a second copy of the registry code:
    /// this module already owns it.
    pub fn machine_reg_sz(subkey: &str, value: &str) -> Option<String> {
        read_reg_sz(HKEY_LOCAL_MACHINE, subkey, value)
    }

    /// Read one `REG_DWORD` under `HKLM`.
    pub fn machine_reg_dword(subkey: &str, value: &str) -> Option<u32> {
        let (sub, name) = (wide(subkey), wide(value));
        let mut key: Hkey = std::ptr::null_mut();
        // SAFETY: NUL-terminated names and live locals sized for a DWORD.
        unsafe {
            if RegOpenKeyExW(
                HKEY_LOCAL_MACHINE,
                sub.as_ptr(),
                0,
                KEY_QUERY_VALUE,
                &mut key,
            ) != ERROR_SUCCESS
            {
                return None;
            }
            let (mut kind, mut data, mut len) = (0u32, 0u32, 4u32);
            let ret = RegQueryValueExW(
                key,
                name.as_ptr(),
                std::ptr::null_mut(),
                &mut kind,
                (&mut data as *mut u32).cast::<u8>(),
                &mut len,
            );
            RegCloseKey(key);
            (ret == ERROR_SUCCESS && kind == REG_DWORD).then_some(data)
        }
    }

    const INSTALL_KEY: &str = r"Software\private-ai";
    const INSTALL_VALUE: &str = "InstallDir";

    /// The exe of the INSTALLED copy (`InstallDir\bin\pai-config.exe`) as
    /// recorded by the installer, or `None` when no
    /// install is registered (portable / dev use) or the recorded exe is missing. `set_enabled`
    /// prefers this over `current_exe()` so the logon entry always launches the
    /// installed GUI build, never a debug build that happens to flip the toggle.
    fn installed_exe() -> Option<std::path::PathBuf> {
        let dir = read_reg_sz(HKEY_LOCAL_MACHINE, INSTALL_KEY, INSTALL_VALUE)?;
        if dir.is_empty() {
            return None;
        }
        let exe = std::path::Path::new(&dir)
            .join("bin")
            .join("pai-config.exe");
        exe.is_file().then_some(exe)
    }

    /// Whether the logon Run entry exists. Presence is the state: the stored
    /// command isn't compared against the current exe path, so a moved install
    /// still reads as enabled (re-toggling refreshes the path).
    pub fn is_enabled() -> bool {
        stored_command().is_some()
    }

    /// Whether the stored command starts in the tray (`--minimized`). `false`
    /// when the entry is absent; the caller gates on `is_enabled` anyway.
    pub fn starts_minimized() -> bool {
        stored_command().is_some_and(|c| c.contains("--minimized"))
    }

    /// Create (or overwrite: an enable also refreshes a stale exe path and
    /// applies the current `minimized` choice) or delete the Run entry. The
    /// stored exe is the INSTALLED copy when one is registered (`installed_exe`),
    /// else the running exe, so the logon entry launches the Program Files GUI
    /// build even when the toggle is flipped from a dev/debug build.
    /// Deleting an absent value is a success: the goal state, not the
    /// transition, is what the caller asked for.
    pub fn set_enabled(on: bool, minimized: bool) -> io::Result<()> {
        let key = open_run_key(KEY_SET_VALUE)?;
        let name = wide(VALUE_NAME);
        let ret = if on {
            let exe = super::choose_autostart_exe(installed_exe(), std::env::current_exe()?);
            let data = wide(&run_command(&exe, minimized));
            let byte_len = u32::try_from(data.len() * 2).map_err(|_| {
                io::Error::new(
                    io::ErrorKind::InvalidInput,
                    crate::strings::t("command-too-long"),
                )
            })?;
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
            Err(io::Error::from_raw_os_error(ret as i32))
        }
    }

    /// A `REG_BINARY` value under HKCU, or `None` when the key or value is
    /// absent, unreadable or of another type.
    fn read_user_binary(subkey: &str, value: &str) -> Option<Vec<u8>> {
        let (sub, name) = (wide(subkey), wide(value));
        let mut len: u32 = 0;
        // SAFETY: NUL-terminated names, live locals, a buffer of `len` bytes.
        // Two-call pattern as in `read_reg_sz`.
        unsafe {
            if RegGetValueW(
                HKEY_CURRENT_USER,
                sub.as_ptr(),
                name.as_ptr(),
                RRF_RT_REG_BINARY,
                std::ptr::null_mut(),
                std::ptr::null_mut(),
                &mut len,
            ) != ERROR_SUCCESS
            {
                return None;
            }
            let mut buf = vec![0u8; len as usize];
            if RegGetValueW(
                HKEY_CURRENT_USER,
                sub.as_ptr(),
                name.as_ptr(),
                RRF_RT_REG_BINARY,
                std::ptr::null_mut(),
                buf.as_mut_ptr().cast(),
                &mut len,
            ) != ERROR_SUCCESS
            {
                return None;
            }
            buf.truncate(len as usize);
            Some(buf)
        }
    }

    /// Delete a value under HKCU; an absent one is a success.
    fn delete_user_value(subkey: &str, value: &str) -> io::Result<()> {
        let (sub, name) = (wide(subkey), wide(value));
        // SAFETY: NUL-terminated names.
        match unsafe { RegDeleteKeyValueW(HKEY_CURRENT_USER, sub.as_ptr(), name.as_ptr()) } {
            ERROR_SUCCESS | ERROR_FILE_NOT_FOUND => Ok(()),
            other => Err(io::Error::from_raw_os_error(other as i32)),
        }
    }

    /// The Windows half of `super::migrate_old_entry`: the old Run value's
    /// `--minimized` choice carried into the new value, then the old one
    /// deleted with its StartupApproved value. With the new value not written,
    /// the old one stays. An old value switched off in Task Manager is only
    /// deleted: the new entry is not enabled.
    pub fn migrate_old_entry() -> io::Result<bool> {
        let old = pai_core::migrate::OLD_LOGIN_ENTRY;
        let Some(command) = read_reg_sz(HKEY_CURRENT_USER, RUN_KEY, old) else {
            return Ok(false);
        };
        let approved = read_user_binary(APPROVED_KEY, old);
        if super::approved_on(approved.as_deref()) {
            set_enabled(true, command.contains("--minimized"))?;
        }
        delete_user_value(RUN_KEY, old)?;
        if approved.is_some() {
            delete_user_value(APPROVED_KEY, old)?;
        }
        Ok(true)
    }
}

/// E2E phase for the login entry, for the shared UI test to call: under the
/// process-wide `PRIVATE_AI_DATA_ROOT` redirect (`paths::redirect_for_tests`)
/// the XDG autostart file is `<redirect>/.config/autostart/` named after the
/// app id, written by an enable and removed by a disable. Linux only: the
/// Windows entry is the real HKCU Run key, which no test writes.
#[cfg(all(test, target_os = "linux"))]
pub(crate) fn startup_e2e() {
    let dir = tempfile::tempdir().expect("tempdir");
    crate::paths::redirect_for_tests(dir.path());
    let autostart = dir.path().join(".config").join("autostart");
    let entry = autostart.join("org.privateai.Config.desktop");

    assert!(!is_enabled(), "nothing written yet");
    set_enabled(true, true).expect("enable");
    assert!(entry.is_file(), "the entry is named after the app id");
    assert_eq!(
        std::fs::read_dir(&autostart)
            .expect("autostart dir")
            .count(),
        1,
        "one entry, no file under the old name"
    );
    assert!(is_enabled() && starts_minimized());
    set_enabled(false, false).expect("disable");
    assert!(!entry.exists());
    assert!(!is_enabled());
}

#[cfg(test)]
mod tests {
    use super::{approved_on, choose_autostart_exe, run_command};
    use std::path::PathBuf;

    // The autostart entry registers the installed copy when one is recorded,
    // falling back to the running exe only when no install is present.
    #[test]
    fn choose_autostart_exe_prefers_the_installed_copy() {
        let installed = PathBuf::from(r"C:\Program Files\private-ai\bin\pai-config.exe");
        let current = PathBuf::from(r"C:\dev\target\debug\pai-config.exe");
        assert_eq!(
            choose_autostart_exe(Some(installed.clone()), current.clone()),
            installed
        );
        assert_eq!(choose_autostart_exe(None, current.clone()), current);
    }

    // Task Manager's switch on a Run entry: no value (never touched) and an
    // empty one are on; an even first byte is on, an odd one off.
    #[test]
    fn approved_on_reads_task_managers_switch() {
        let filetime = [0x10u8, 0x32, 0x54, 0x76, 0x98, 0xba, 0xdc, 0x01];
        let with = |first: u8| {
            let mut v = vec![first, 0, 0, 0];
            v.extend_from_slice(&filetime);
            v
        };
        assert!(approved_on(None), "no value: never switched off");
        assert!(approved_on(Some(&[])), "empty value");
        assert!(approved_on(Some(&with(0x02))), "0x02: on");
        assert!(approved_on(Some(&with(0x06))), "0x06: on");
        assert!(approved_on(Some(&[0x02; 12])), "on, any tail");
        assert!(!approved_on(Some(&with(0x03))), "0x03: off");
        assert!(!approved_on(Some(&with(0x01))), "odd: off");
        assert!(!approved_on(Some(&with(0x07))), "odd: off");
        assert!(!approved_on(Some(&[0x03])), "off, short value");
    }

    // The exe path is quoted (Program Files has a space) and the launch goes
    // through `gui`, with `--minimized` only when the tray toggle asks for it.
    // The registry legs stay out of the normal run (thin FFI over a fixed key,
    // and `cargo test` must not write the developer's real HKCU Run key), but
    // see the #[ignore] round-trip below for a manual check.
    #[test]
    fn run_command_quotes_the_exe_and_carries_the_minimized_choice() {
        let exe = std::path::Path::new(r"C:\Program Files\private-ai\bin\pai-config.exe");
        assert_eq!(
            run_command(exe, true),
            r#""C:\Program Files\private-ai\bin\pai-config.exe" gui --minimized"#
        );
        assert_eq!(
            run_command(exe, false),
            r#""C:\Program Files\private-ai\bin\pai-config.exe" gui"#
        );
    }

    // The Linux autostart entry: the Exec line quotes the exe by the Desktop
    // Entry rules and carries the minimized choice, and what is read back is
    // what was written.
    #[cfg(target_os = "linux")]
    #[test]
    fn autostart_entry_round_trips_through_its_text() {
        use super::linux::{entry_text, exec_line, parse_entry};
        let exe = std::path::Path::new("/usr/bin/pai-config");
        assert_eq!(
            exec_line(exe, true),
            r#""/usr/bin/pai-config" gui --minimized"#
        );
        assert_eq!(
            exec_line(std::path::Path::new("/opt/my $apps/100%/x"), false),
            r#""/opt/my \\$apps/100%%/x" gui"#
        );
        assert_eq!(parse_entry(&entry_text(exe, true)), (true, true));
        assert_eq!(parse_entry(&entry_text(exe, false)), (true, false));
    }

    // The entry names the product by its display name and its icon by the
    // app id, as the packaged desktop entry does.
    #[cfg(target_os = "linux")]
    #[test]
    fn autostart_entry_names_the_product_and_the_app_id() {
        let text = super::linux::entry_text(std::path::Path::new("/usr/bin/pai-config"), false);
        let lines: Vec<&str> = text.lines().collect();
        assert!(lines.contains(&"Name=Private AI Framework"), "{text}");
        assert!(lines.contains(&"Icon=org.privateai.Config"), "{text}");
        assert!(!text.contains("llama-cpp"), "{text}");
    }

    // An entry GNOME's startup panel switched off (or one marked Hidden) reads
    // as disabled although the file exists; a file with no Exec is no entry.
    #[cfg(target_os = "linux")]
    #[test]
    fn autostart_entry_switched_off_by_the_desktop_reads_as_disabled() {
        use super::linux::{entry_text, parse_entry};
        let on = entry_text(std::path::Path::new("/usr/bin/pai-config"), true);
        let gnome_off = on.replace(
            "X-GNOME-Autostart-enabled=true",
            "X-GNOME-Autostart-enabled=false",
        );
        assert_eq!(parse_entry(&gnome_off), (false, true));
        assert_eq!(parse_entry(&format!("{on}Hidden=true\n")), (false, true));
        assert_eq!(
            parse_entry("[Desktop Entry]\nType=Application\n"),
            (false, false)
        );
    }

    // The file legs, against a temp path (never the real ~/.config/autostart):
    // enable writes, a second enable rewrites, disable removes, and disabling
    // what is already gone stays Ok.
    #[cfg(target_os = "linux")]
    #[test]
    fn autostart_entry_file_is_written_rewritten_and_removed() {
        use super::linux::{parse_entry, write_entry};
        let dir = std::env::temp_dir().join(format!("pai-config-autostart-{}", std::process::id()));
        let path = dir.join("autostart").join("org.privateai.Config.desktop");
        let read = || parse_entry(&std::fs::read_to_string(&path).unwrap());

        write_entry(&path, true, true).expect("enable minimized");
        assert_eq!(read(), (true, true));
        write_entry(&path, true, false).expect("rewrite without --minimized");
        assert_eq!(read(), (true, false));
        write_entry(&path, false, false).expect("disable");
        assert!(!path.exists());
        write_entry(&path, false, false).expect("disable when already absent");
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// A 2.4 autostart entry, as that release wrote it, `minimized` and
    /// switched on or off by GNOME's startup panel.
    #[cfg(target_os = "linux")]
    fn old_entry_text(minimized: bool, on: bool) -> String {
        let old = pai_core::migrate::OLD_LOGIN_ENTRY;
        let flag = if minimized { " --minimized" } else { "" };
        format!(
            "[Desktop Entry]\nType=Application\nName={}\n\
             Comment=Configure and run llama-server and its model presets\n\
             Exec=\"/usr/bin/{old}\" gui{flag}\nIcon={old}\nTerminal=false\n\
             X-GNOME-Autostart-enabled={on}\n",
            concat!("llama.cpp", " Config")
        )
    }

    // The old entry, when there is one, gives way to the new one with the
    // same minimized choice; the old file is gone.
    #[cfg(target_os = "linux")]
    #[test]
    fn an_old_autostart_entry_is_replaced_by_the_new_one() {
        use super::linux::{migrate_entry, parse_entry};
        for minimized in [true, false] {
            let dir = tempfile::tempdir().expect("tempdir");
            let old = dir
                .path()
                .join(format!("{}.desktop", pai_core::migrate::OLD_LOGIN_ENTRY));
            let new = dir.path().join("org.privateai.Config.desktop");
            std::fs::write(&old, old_entry_text(minimized, true)).expect("old entry");

            assert!(migrate_entry(&old, &new).expect("migrated"), "{minimized}");
            assert!(!old.exists(), "the old entry is removed");
            let text = std::fs::read_to_string(&new).expect("new entry");
            assert_eq!(parse_entry(&text), (true, minimized), "{text}");
            assert!(text.contains("Name=Private AI Framework"), "{text}");
        }
    }

    // No old entry: nothing is created, and that is not an error.
    #[cfg(target_os = "linux")]
    #[test]
    fn no_old_autostart_entry_creates_nothing() {
        use super::linux::migrate_entry;
        let dir = tempfile::tempdir().expect("tempdir");
        let old = dir
            .path()
            .join(format!("{}.desktop", pai_core::migrate::OLD_LOGIN_ENTRY));
        let new = dir.path().join("org.privateai.Config.desktop");
        assert!(!migrate_entry(&old, &new).expect("no error"));
        assert!(!new.exists());
        assert_eq!(std::fs::read_dir(dir.path()).expect("dir").count(), 0);
    }

    // An old entry the desktop had switched off: the setting was off, so it
    // is removed and no new entry is enabled.
    #[cfg(target_os = "linux")]
    #[test]
    fn a_switched_off_old_autostart_entry_is_removed_and_not_enabled() {
        use super::linux::migrate_entry;
        let dir = tempfile::tempdir().expect("tempdir");
        let old = dir
            .path()
            .join(format!("{}.desktop", pai_core::migrate::OLD_LOGIN_ENTRY));
        let new = dir.path().join("org.privateai.Config.desktop");
        std::fs::write(&old, old_entry_text(true, false)).expect("old entry");
        assert!(migrate_entry(&old, &new).expect("no error"));
        assert!(!old.exists() && !new.exists());
    }

    // The new entry cannot be written (read-only autostart directory): the
    // error comes back and the old entry stays, so the setting is not lost.
    #[cfg(target_os = "linux")]
    #[test]
    fn a_failed_new_autostart_entry_keeps_the_old_one() {
        use std::os::unix::fs::PermissionsExt;
        let dir = tempfile::tempdir().expect("tempdir");
        let old = dir
            .path()
            .join(format!("{}.desktop", pai_core::migrate::OLD_LOGIN_ENTRY));
        let new = dir.path().join("org.privateai.Config.desktop");
        let text = old_entry_text(true, true);
        std::fs::write(&old, &text).expect("old entry");
        std::fs::set_permissions(dir.path(), std::fs::Permissions::from_mode(0o555))
            .expect("chmod");
        if std::fs::write(dir.path().join("probe"), "x").is_ok() {
            std::fs::set_permissions(dir.path(), std::fs::Permissions::from_mode(0o755))
                .expect("chmod back");
            eprintln!("skipped: running as root, a mode-555 directory is still writable");
            return;
        }
        let result = super::linux::migrate_entry(&old, &new);
        std::fs::set_permissions(dir.path(), std::fs::Permissions::from_mode(0o755))
            .expect("chmod back");
        let err = result.expect_err("the new entry cannot be written");
        assert_eq!(err.kind(), std::io::ErrorKind::PermissionDenied, "{err}");
        assert_eq!(std::fs::read_to_string(&old).expect("old kept"), text);
        assert!(!new.exists());
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
        assert!(
            super::starts_minimized(),
            "command should carry --minimized"
        );

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

    // MANUAL-ONLY (`cargo test startup -- --ignored`): the old Run value of
    // release 2.4 gives way to the new one. Switched on (no StartupApproved
    // value), it is moved with its minimized choice and a second run finds
    // nothing; switched off in Task Manager (StartupApproved first byte odd),
    // it is removed with its StartupApproved value and no new entry is
    // enabled. A guard records the four values involved first and puts them
    // back in its Drop, so the developer's registry is restored on every
    // path, a failed assertion included.
    #[cfg(windows)]
    #[test]
    #[ignore = "writes the real HKCU Run key; run explicitly with -- --ignored"]
    fn registry_old_entry_migrates_and_restores_initial_state() {
        use super::win::RUN_KEY;
        use windows_sys::Win32::Foundation::{ERROR_FILE_NOT_FOUND, ERROR_SUCCESS};
        use windows_sys::Win32::System::Registry::{
            RegDeleteKeyValueW, RegGetValueW, RegSetKeyValueW, HKEY_CURRENT_USER, REG_BINARY,
            REG_SZ, RRF_RT_ANY,
        };
        const APPROVED: &str =
            r"Software\Microsoft\Windows\CurrentVersion\Explorer\StartupApproved\Run";
        const NEW: &str = "org.privateai.Config";
        let old = pai_core::migrate::OLD_LOGIN_ENTRY;

        fn wide(s: &str) -> Vec<u16> {
            s.encode_utf16().chain(std::iter::once(0)).collect()
        }
        /// A value as stored: its type and its bytes, or None when absent.
        fn get(sub: &str, name: &str) -> Option<(u32, Vec<u8>)> {
            let (s, n) = (wide(sub), wide(name));
            let (mut kind, mut len) = (0u32, 0u32);
            // SAFETY: NUL-terminated names, live locals, a buffer of `len` bytes.
            unsafe {
                let sized = RegGetValueW(
                    HKEY_CURRENT_USER,
                    s.as_ptr(),
                    n.as_ptr(),
                    RRF_RT_ANY,
                    &mut kind,
                    std::ptr::null_mut(),
                    &mut len,
                );
                if sized != ERROR_SUCCESS {
                    return None;
                }
                let mut buf = vec![0u8; len as usize];
                let read = RegGetValueW(
                    HKEY_CURRENT_USER,
                    s.as_ptr(),
                    n.as_ptr(),
                    RRF_RT_ANY,
                    &mut kind,
                    buf.as_mut_ptr().cast(),
                    &mut len,
                );
                if read != ERROR_SUCCESS {
                    return None;
                }
                buf.truncate(len as usize);
                Some((kind, buf))
            }
        }
        /// Write a value (creating the subkey) or, for None, delete it.
        fn put(sub: &str, name: &str, value: Option<&(u32, Vec<u8>)>) -> u32 {
            let (s, n) = (wide(sub), wide(name));
            // SAFETY: NUL-terminated names and a buffer of the given length.
            let ret = unsafe {
                match value {
                    Some((kind, data)) => RegSetKeyValueW(
                        HKEY_CURRENT_USER,
                        s.as_ptr(),
                        n.as_ptr(),
                        *kind,
                        data.as_ptr().cast(),
                        data.len() as u32,
                    ),
                    None => RegDeleteKeyValueW(HKEY_CURRENT_USER, s.as_ptr(), n.as_ptr()),
                }
            };
            if ret == ERROR_FILE_NOT_FOUND && value.is_none() {
                ERROR_SUCCESS
            } else {
                ret
            }
        }
        fn sz(text: &str) -> (u32, Vec<u8>) {
            (
                REG_SZ,
                wide(text).iter().flat_map(|u| u.to_le_bytes()).collect(),
            )
        }

        /// One value the test touches: its key, its name and its starting
        /// content (type and bytes), `None` when it did not exist.
        type Saved = (&'static str, String, Option<(u32, Vec<u8>)>);
        /// The starting state of every value the test touches, put back on drop.
        struct Restore(Vec<Saved>);
        impl Drop for Restore {
            fn drop(&mut self) {
                for (sub, name, value) in &self.0 {
                    let ret = put(sub, name, value.as_ref());
                    if ret != ERROR_SUCCESS {
                        // No panic in a Drop: it could run during a panic.
                        eprintln!("could not restore {sub}\\{name}: error {ret}");
                    }
                }
            }
        }
        let touched = [
            (RUN_KEY, NEW),
            (RUN_KEY, old),
            (APPROVED, NEW),
            (APPROVED, old),
        ];
        let _restore = Restore(
            touched
                .iter()
                .map(|(sub, name)| (*sub, name.to_string(), get(sub, name)))
                .collect(),
        );
        let clear = || {
            for (sub, name) in touched {
                assert_eq!(put(sub, name, None), ERROR_SUCCESS, "clear {sub}\\{name}");
            }
        };

        // Switched on: moved, minimized carried over; a second run finds nothing.
        clear();
        let command = r#""C:\old\bin\old.exe" gui --minimized"#;
        assert_eq!(put(RUN_KEY, old, Some(&sz(command))), ERROR_SUCCESS);
        assert!(
            super::migrate_old_entry().expect("migrated"),
            "an old value was found"
        );
        assert_eq!(
            (super::is_enabled(), super::starts_minimized()),
            (true, true),
            "enabled, minimized carried over"
        );
        assert_eq!(get(RUN_KEY, old), None, "the old value was deleted");
        assert!(
            !super::migrate_old_entry().expect("second run"),
            "nothing left"
        );

        // Switched off in Task Manager: removed with its StartupApproved value,
        // and the new entry is not enabled.
        clear();
        assert_eq!(put(RUN_KEY, old, Some(&sz(command))), ERROR_SUCCESS);
        let off = (
            REG_BINARY,
            vec![
                0x03, 0, 0, 0, 0x10, 0x32, 0x54, 0x76, 0x98, 0xba, 0xdc, 0x01,
            ],
        );
        assert_eq!(put(APPROVED, old, Some(&off)), ERROR_SUCCESS);
        assert!(
            super::migrate_old_entry().expect("migrated"),
            "an old value was found"
        );
        assert!(!super::is_enabled(), "a switched-off entry is not enabled");
        assert_eq!(get(RUN_KEY, old), None, "the old Run value was deleted");
        assert_eq!(get(APPROVED, old), None, "its StartupApproved value too");
    }
}
