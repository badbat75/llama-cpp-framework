//! The Windows accounts the manager asks about: a user's password
//! (`LogonUserW`, a network logon, the token closed at once) and whether a
//! user is in a local group by its name (`NetUserGetLocalGroups`). Checked
//! in a real run only: no test logs anybody on.

use crate::manager::service::PasswordError;
use windows_sys::Win32::Foundation::{
    CloseHandle, GetLastError, ERROR_ACCOUNT_DISABLED, ERROR_ACCOUNT_EXPIRED,
    ERROR_ACCOUNT_LOCKED_OUT, ERROR_ACCOUNT_RESTRICTION, ERROR_INVALID_LOGON_HOURS,
    ERROR_INVALID_WORKSTATION, ERROR_LOGON_FAILURE, ERROR_LOGON_TYPE_NOT_GRANTED,
    ERROR_NO_SUCH_USER, ERROR_PASSWORD_EXPIRED, ERROR_PASSWORD_MUST_CHANGE, HANDLE,
};
use windows_sys::Win32::NetworkManagement::NetManagement::{
    NERR_Success, NetApiBufferFree, NetUserGetLocalGroups, LG_INCLUDE_INDIRECT,
    LOCALGROUP_USERS_INFO_0, MAX_PREFERRED_LENGTH,
};
use windows_sys::Win32::Security::{LogonUserW, LOGON32_LOGON_NETWORK, LOGON32_PROVIDER_DEFAULT};

/// `LogonUserW`'s errors that mean this account may not sign in now (bad
/// credentials, disabled, locked, expired, a password to change): a no,
/// not a failure.
const REFUSALS: [u32; 11] = [
    ERROR_LOGON_FAILURE,
    ERROR_NO_SUCH_USER,
    ERROR_ACCOUNT_RESTRICTION,
    ERROR_INVALID_LOGON_HOURS,
    ERROR_INVALID_WORKSTATION,
    ERROR_PASSWORD_EXPIRED,
    ERROR_ACCOUNT_DISABLED,
    ERROR_ACCOUNT_EXPIRED,
    ERROR_PASSWORD_MUST_CHANGE,
    ERROR_ACCOUNT_LOCKED_OUT,
    ERROR_LOGON_TYPE_NOT_GRANTED,
];

/// `s` as a NUL-terminated UTF-16 string.
fn wide(s: &str) -> Vec<u16> {
    s.encode_utf16().chain(std::iter::once(0)).collect()
}

/// Whether `password` is `user`'s: `DOMAIN\user` on that domain, a UPN
/// (`user@domain`) as it is, any other name a local account.
pub fn check_password(user: &str, password: &str) -> Result<bool, PasswordError> {
    // A NUL inside is nobody's name or password.
    if user.contains('\0') || password.contains('\0') {
        return Ok(false);
    }
    let (domain, name) = match user.split_once('\\') {
        Some((domain, name)) => (Some(domain), name),
        None if user.contains('@') => (None, user),
        None => (Some("."), user),
    };
    let name = wide(name);
    let domain = domain.map(wide);
    let password = wide(password);
    let mut token: HANDLE = std::ptr::null_mut();
    // SAFETY: NUL-terminated strings alive for the call, a valid out pointer.
    let ok = unsafe {
        LogonUserW(
            name.as_ptr(),
            domain.as_ref().map_or(std::ptr::null(), |d| d.as_ptr()),
            password.as_ptr(),
            LOGON32_LOGON_NETWORK,
            LOGON32_PROVIDER_DEFAULT,
            &mut token,
        )
    };
    if ok != 0 {
        // SAFETY: the token `LogonUserW` opened, not used after.
        unsafe { CloseHandle(token) };
        return Ok(true);
    }
    // SAFETY: no other call since `LogonUserW`.
    let error = unsafe { GetLastError() };
    if REFUSALS.contains(&error) {
        Ok(false)
    } else {
        Err(PasswordError(format!("LogonUserW: error {error}")))
    }
}

/// Whether `user` is in the local group `group` (directly or through a
/// global group), compared as Windows compares names, case aside; false when
/// the lookup fails.
pub fn in_local_group(user: &str, group: &str) -> bool {
    let name = wide(user);
    let mut buffer: *mut u8 = std::ptr::null_mut();
    let (mut read, mut total) = (0u32, 0u32);
    // SAFETY: a NUL-terminated name and valid out pointers; the buffer is
    // freed below with `NetApiBufferFree`.
    let status = unsafe {
        NetUserGetLocalGroups(
            std::ptr::null(),
            name.as_ptr(),
            0,
            LG_INCLUDE_INDIRECT,
            &mut buffer,
            MAX_PREFERRED_LENGTH,
            &mut read,
            &mut total,
        )
    };
    if buffer.is_null() {
        return false;
    }
    let found = status == NERR_Success && {
        // SAFETY: on success the buffer holds `read` level-0 entries.
        let groups = unsafe {
            std::slice::from_raw_parts(buffer as *const LOCALGROUP_USERS_INFO_0, read as usize)
        };
        groups.iter().any(|g| {
            if g.lgrui0_name.is_null() {
                return false;
            }
            // SAFETY: a NUL-terminated name inside the buffer.
            let len = (0..)
                .take_while(|&i| unsafe { *g.lgrui0_name.add(i) } != 0)
                .count();
            // SAFETY: `len` units before the NUL.
            let units = unsafe { std::slice::from_raw_parts(g.lgrui0_name, len) };
            String::from_utf16_lossy(units).eq_ignore_ascii_case(group)
        })
    };
    // SAFETY: the buffer `NetUserGetLocalGroups` allocated, not used after.
    unsafe { NetApiBufferFree(buffer as *const std::ffi::c_void) };
    found
}
