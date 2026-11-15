//! The Linux accounts the manager asks about: a user's password through PAM
//! (service `private-ai`: `pam_authenticate`, then `pam_acct_mgmt`), and a
//! user's uid and group names by name (NSS, through `nix`). Checked in a real
//! run only: no test calls PAM.
//!
//! libpam is linked by its run-time name `libpam.so.0`, so building needs no
//! PAM development package. The declarations follow Linux-PAM's
//! `security/_pam_types.h` and `security/pam_appl.h`.

use super::service::PasswordError;
use nix::libc;
use std::ffi::{c_char, c_int, c_void, CStr, CString};

/// The PAM service whose stack checks the web sign-in (`/etc/pam.d/private-ai`).
const SERVICE: &CStr = c"private-ai";

const PAM_SUCCESS: c_int = 0;
const PAM_BUF_ERR: c_int = 5;
const PAM_PERM_DENIED: c_int = 6;
const PAM_AUTH_ERR: c_int = 7;
const PAM_USER_UNKNOWN: c_int = 10;
const PAM_NEW_AUTHTOK_REQD: c_int = 12;
const PAM_ACCT_EXPIRED: c_int = 13;
const PAM_CONV_ERR: c_int = 19;
const PAM_AUTHTOK_EXPIRED: c_int = 27;

const PAM_SILENT: c_int = 0x8000;
const PAM_DISALLOW_NULL_AUTHTOK: c_int = 0x0001;

const PAM_PROMPT_ECHO_OFF: c_int = 1;
const PAM_PROMPT_ECHO_ON: c_int = 2;
const PAM_ERROR_MSG: c_int = 3;
const PAM_TEXT_INFO: c_int = 4;

#[repr(C)]
struct PamMessage {
    msg_style: c_int,
    msg: *const c_char,
}

#[repr(C)]
struct PamResponse {
    resp: *mut c_char,
    resp_retcode: c_int,
}

type Converse =
    extern "C" fn(c_int, *mut *const PamMessage, *mut *mut PamResponse, *mut c_void) -> c_int;

#[repr(C)]
struct PamConv {
    conv: Converse,
    appdata_ptr: *mut c_void,
}

#[link(name = "libpam.so.0", kind = "dylib", modifiers = "+verbatim")]
extern "C" {
    fn pam_start(
        service: *const c_char,
        user: *const c_char,
        conv: *const PamConv,
        pamh: *mut *mut c_void,
    ) -> c_int;
    fn pam_authenticate(pamh: *mut c_void, flags: c_int) -> c_int;
    fn pam_acct_mgmt(pamh: *mut c_void, flags: c_int) -> c_int;
    fn pam_end(pamh: *mut c_void, status: c_int) -> c_int;
    fn pam_strerror(pamh: *mut c_void, errnum: c_int) -> *const c_char;
}

/// What the conversation answers with.
struct Credentials {
    user: CString,
    password: CString,
}

/// PAM's conversation: the password to every hidden prompt, the user name to
/// a visible one, nothing to a message. The responses are `calloc`ed and
/// `strdup`ed: PAM frees them.
extern "C" fn converse(
    count: c_int,
    messages: *mut *const PamMessage,
    responses: *mut *mut PamResponse,
    data: *mut c_void,
) -> c_int {
    if count <= 0 || messages.is_null() || responses.is_null() || data.is_null() {
        return PAM_CONV_ERR;
    }
    let count = count as usize;
    // SAFETY: `data` is the `Credentials` `check_password` handed to
    // `pam_start`, alive until `pam_end`.
    let credentials = unsafe { &*(data as *const Credentials) };
    // SAFETY: calloc of `count` zeroed responses; null is handled.
    let out =
        unsafe { libc::calloc(count, std::mem::size_of::<PamResponse>()) } as *mut PamResponse;
    if out.is_null() {
        return PAM_BUF_ERR;
    }
    // SAFETY: frees what was filled so far, then the array.
    let give_up = |filled: usize, status: c_int| unsafe {
        for i in 0..filled {
            libc::free((*out.add(i)).resp as *mut c_void);
        }
        libc::free(out as *mut c_void);
        status
    };
    for i in 0..count {
        // SAFETY: Linux-PAM passes `count` message pointers.
        let message = unsafe { *messages.add(i) };
        if message.is_null() {
            return give_up(i, PAM_CONV_ERR);
        }
        // SAFETY: a non-null message from PAM.
        let answer = match unsafe { (*message).msg_style } {
            PAM_PROMPT_ECHO_OFF => Some(&credentials.password),
            PAM_PROMPT_ECHO_ON => Some(&credentials.user),
            PAM_ERROR_MSG | PAM_TEXT_INFO => None,
            _ => return give_up(i, PAM_CONV_ERR),
        };
        if let Some(answer) = answer {
            // SAFETY: a NUL-terminated string; null is handled.
            let copy = unsafe { libc::strdup(answer.as_ptr()) };
            if copy.is_null() {
                return give_up(i, PAM_BUF_ERR);
            }
            // SAFETY: `i < count`, inside the array.
            unsafe { (*out.add(i)).resp = copy };
        }
    }
    // SAFETY: `responses` is PAM's out pointer.
    unsafe { *responses = out };
    PAM_SUCCESS
}

/// PAM's text for `status` (a fixed string, never the password).
fn pam_text(pamh: *mut c_void, status: c_int) -> String {
    // SAFETY: Linux-PAM returns a static string for any status.
    let text = unsafe { pam_strerror(pamh, status) };
    if text.is_null() {
        return format!("PAM error {status}");
    }
    // SAFETY: non-null, NUL-terminated.
    unsafe { CStr::from_ptr(text) }
        .to_string_lossy()
        .into_owned()
}

/// Whether `password` is `user`'s and the account may sign in now. A wrong
/// password, an unknown user and an account PAM refuses (expired, locked,
/// its password to be changed) are `Ok(false)`; PAM itself failing is the
/// error, with PAM's text.
pub fn check_password(user: &str, password: &str) -> Result<bool, PasswordError> {
    // A NUL inside is nobody's name or password.
    let (Ok(user), Ok(password)) = (CString::new(user), CString::new(password)) else {
        return Ok(false);
    };
    let credentials = Credentials { user, password };
    let conv = PamConv {
        conv: converse,
        appdata_ptr: &credentials as *const Credentials as *mut c_void,
    };
    let mut pamh: *mut c_void = std::ptr::null_mut();
    // SAFETY: valid strings and conversation, alive until `pam_end`.
    let status = unsafe {
        pam_start(
            SERVICE.as_ptr(),
            credentials.user.as_ptr(),
            &conv,
            &mut pamh,
        )
    };
    if status != PAM_SUCCESS {
        return Err(PasswordError(format!(
            "pam_start: {}",
            pam_text(pamh, status)
        )));
    }
    let flags = PAM_SILENT | PAM_DISALLOW_NULL_AUTHTOK;
    // SAFETY: a handle `pam_start` opened.
    let mut step = "pam_authenticate";
    let mut status = unsafe { pam_authenticate(pamh, flags) };
    if status == PAM_SUCCESS {
        step = "pam_acct_mgmt";
        // SAFETY: as above.
        status = unsafe { pam_acct_mgmt(pamh, flags) };
    }
    let text = pam_text(pamh, status);
    // SAFETY: closes the handle; nothing uses it after.
    unsafe { pam_end(pamh, status) };
    match status {
        PAM_SUCCESS => Ok(true),
        PAM_AUTH_ERR | PAM_USER_UNKNOWN | PAM_PERM_DENIED | PAM_ACCT_EXPIRED
        | PAM_NEW_AUTHTOK_REQD | PAM_AUTHTOK_EXPIRED => Ok(false),
        _ => Err(PasswordError(format!("{step}: {text}"))),
    }
}

/// The names of `user`'s groups, the primary one included; none when the
/// user or a lookup fails.
pub fn group_names(user: &str) -> Vec<String> {
    use nix::unistd::{getgrouplist, Group, User};
    let Ok(Some(account)) = User::from_name(user) else {
        return Vec::new();
    };
    let Ok(name) = CString::new(user) else {
        return Vec::new();
    };
    getgrouplist(&name, account.gid)
        .unwrap_or_default()
        .into_iter()
        .filter_map(|gid| Group::from_gid(gid).ok().flatten())
        .map(|group| group.name)
        .collect()
}

/// The uid of the user `name`, None when there is none or the lookup fails.
pub fn uid_of(name: &str) -> Option<u32> {
    nix::unistd::User::from_name(name)
        .ok()
        .flatten()
        .map(|user| user.uid.as_raw())
}
