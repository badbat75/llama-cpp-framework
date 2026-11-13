//! The manager's named pipe: each connection on its own thread, served by
//! `manager::pipe::serve_stream` as the caller its token names
//! ([`TokenAuthority::with_caller`]). Only the Win32 calls live here.

use super::scm::ScmService;
use super::{TokenAuthority, TokenFacts, TokenGroup, PIPE_MODE, PIPE_SDDL};
use crate::manager::pipe::serve_stream;
use crate::manager::service::{Manager, SystemResolver};
use pai_core::client::pipe::PIPE_NAME;
use std::ffi::{c_void, OsStr};
use std::fs::File;
use std::io::{Read, Write};
use std::os::windows::ffi::OsStrExt;
use std::os::windows::io::{AsRawHandle, FromRawHandle, OwnedHandle, RawHandle};
use std::ptr::{null, null_mut};
use std::sync::Arc;
use windows_sys::Win32::Foundation::{
    GetLastError, LocalFree, ERROR_PIPE_CONNECTED, HANDLE, INVALID_HANDLE_VALUE,
};
use windows_sys::Win32::Security::Authorization::{
    ConvertSidToStringSidW, ConvertStringSecurityDescriptorToSecurityDescriptorW, SDDL_REVISION_1,
};
use windows_sys::Win32::Security::{
    GetTokenInformation, RevertToSelf, TokenGroups, TokenUser, PSECURITY_DESCRIPTOR, PSID,
    SECURITY_ATTRIBUTES, SID_AND_ATTRIBUTES, TOKEN_GROUPS, TOKEN_INFORMATION_CLASS, TOKEN_QUERY,
    TOKEN_USER,
};
use windows_sys::Win32::Storage::FileSystem::{FILE_FLAG_FIRST_PIPE_INSTANCE, PIPE_ACCESS_DUPLEX};
use windows_sys::Win32::System::Pipes::{
    ConnectNamedPipe, CreateNamedPipeW, ImpersonateNamedPipeClient, PIPE_UNLIMITED_INSTANCES,
};
use windows_sys::Win32::System::Threading::{GetCurrentThread, OpenThreadToken};

/// The Windows manager.
pub type WinManager = Manager<ScmService, TokenAuthority, SystemResolver>;

/// Each pipe instance's buffers, either way: a frame of any size streams through.
const BUFFER: u32 = 64 * 1024;

/// Serve the pipe until creating an instance fails (another process
/// already serves it: `FILE_FLAG_FIRST_PIPE_INSTANCE`).
pub fn serve(manager: Arc<WinManager>) -> std::io::Result<()> {
    let descriptor = Descriptor::from_sddl(PIPE_SDDL)?;
    let name: Vec<u16> = OsStr::new(PIPE_NAME)
        .encode_wide()
        .chain(std::iter::once(0))
        .collect();
    let mut first = FILE_FLAG_FIRST_PIPE_INSTANCE;
    loop {
        let attributes = SECURITY_ATTRIBUTES {
            nLength: std::mem::size_of::<SECURITY_ATTRIBUTES>() as u32,
            lpSecurityDescriptor: descriptor.0,
            bInheritHandle: 0,
        };
        // SAFETY: a NUL-terminated name, valid attributes for the call.
        let h = unsafe {
            CreateNamedPipeW(
                name.as_ptr(),
                PIPE_ACCESS_DUPLEX | first,
                PIPE_MODE,
                PIPE_UNLIMITED_INSTANCES,
                BUFFER,
                BUFFER,
                0,
                &attributes,
            )
        };
        if h == INVALID_HANDLE_VALUE {
            return Err(std::io::Error::last_os_error());
        }
        first = 0;
        // SAFETY: a handle just created and owned by nothing else.
        let pipe = unsafe { File::from_raw_handle(h as RawHandle) };
        // SAFETY: a valid pipe handle, synchronous.
        let connected = unsafe { ConnectNamedPipe(h, null_mut()) } != 0
            || unsafe { GetLastError() } == ERROR_PIPE_CONNECTED;
        if connected {
            let manager = Arc::clone(&manager);
            std::thread::spawn(move || connection(&manager, pipe));
        }
    }
}

/// Serve one client: its first byte read (impersonation takes the security
/// context of the last message read), its token's facts as the caller, then
/// every request.
fn connection(manager: &WinManager, pipe: File) {
    let mut head = [0u8; 1];
    match (&pipe).read(&mut head) {
        Ok(1) => {}
        _ => return,
    }
    let facts = client_facts(pipe.as_raw_handle() as HANDLE).unwrap_or(TokenFacts {
        sid: String::new(),
        interactive: false,
        admin_member: false,
    });
    let stream = Prefixed {
        head: std::io::Cursor::new(head),
        pipe: &pipe,
    };
    let _ = TokenAuthority::with_caller(facts, |caller| {
        serve_stream(manager, caller.clone(), stream)
    });
    // Waits until the client has read the last reply, which closing the
    // handle would otherwise discard.
    let _ = pipe.sync_all();
}

/// The pipe with the bytes read before the stream was handed over.
struct Prefixed<'a> {
    head: std::io::Cursor<[u8; 1]>,
    pipe: &'a File,
}

impl Read for Prefixed<'_> {
    fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
        match self.head.read(buf)? {
            0 => self.pipe.read(buf),
            n => Ok(n),
        }
    }
}

impl Write for Prefixed<'_> {
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        self.pipe.write(buf)
    }
    fn flush(&mut self) -> std::io::Result<()> {
        self.pipe.flush()
    }
}

/// The facts of the client's token: impersonate it (identification level
/// is enough to open and query the token), open the thread token as the
/// service, revert at once. None when any step fails.
fn client_facts(pipe: HANDLE) -> Option<TokenFacts> {
    // SAFETY: a connected pipe handle from which a byte was read.
    if unsafe { ImpersonateNamedPipeClient(pipe) } == 0 {
        return None;
    }
    let mut token: HANDLE = null_mut();
    // SAFETY: the current thread's pseudo-handle; out-parameter for the token.
    let opened = unsafe { OpenThreadToken(GetCurrentThread(), TOKEN_QUERY, 1, &mut token) } != 0;
    // SAFETY: no preconditions. Running on as the client is never an option.
    if unsafe { RevertToSelf() } == 0 {
        std::process::abort();
    }
    if !opened {
        return None;
    }
    // SAFETY: a token handle just opened, owned here.
    let token = unsafe { OwnedHandle::from_raw_handle(token as RawHandle) };
    let token = token.as_raw_handle() as HANDLE;
    let user = token_info(token, TokenUser)?;
    // SAFETY: the buffer holds a TOKEN_USER, aligned (u64s).
    let sid = sid_string(unsafe { (*user.as_ptr().cast::<TOKEN_USER>()).User.Sid })?;
    let groups = token_info(token, TokenGroups)?;
    // SAFETY: the buffer holds a TOKEN_GROUPS and its `GroupCount` entries.
    let groups = unsafe {
        let list = groups.as_ptr().cast::<TOKEN_GROUPS>();
        std::slice::from_raw_parts(
            std::ptr::addr_of!((*list).Groups).cast::<SID_AND_ATTRIBUTES>(),
            (*list).GroupCount as usize,
        )
    };
    let groups: Vec<TokenGroup> = groups
        .iter()
        .filter_map(|g| {
            Some(TokenGroup {
                sid: sid_string(g.Sid)?,
                attributes: g.Attributes,
            })
        })
        .collect();
    Some(TokenFacts::from_groups(&sid, &groups))
}

/// `GetTokenInformation` of `class` in a buffer aligned for its structure.
fn token_info(token: HANDLE, class: TOKEN_INFORMATION_CLASS) -> Option<Vec<u64>> {
    let mut len = 0u32;
    // SAFETY: a size query: no buffer.
    unsafe { GetTokenInformation(token, class, null_mut(), 0, &mut len) };
    if len == 0 {
        return None;
    }
    let mut buffer = vec![0u64; (len as usize).div_ceil(8)];
    // SAFETY: the buffer is at least `len` bytes.
    let ok = unsafe {
        GetTokenInformation(
            token,
            class,
            buffer.as_mut_ptr().cast::<c_void>(),
            len,
            &mut len,
        )
    } != 0;
    ok.then_some(buffer)
}

/// A SID as its string (`S-1-5-...`).
fn sid_string(sid: PSID) -> Option<String> {
    let mut text: *mut u16 = null_mut();
    // SAFETY: a SID from the token buffer, alive for the call.
    if unsafe { ConvertSidToStringSidW(sid, &mut text) } == 0 {
        return None;
    }
    // SAFETY: a NUL-terminated string the call allocated with LocalAlloc.
    let s = unsafe {
        let mut len = 0;
        while *text.add(len) != 0 {
            len += 1;
        }
        String::from_utf16_lossy(std::slice::from_raw_parts(text, len))
    };
    // SAFETY: freed once, as ConvertSidToStringSidW asks.
    unsafe { LocalFree(text.cast()) };
    Some(s)
}

/// A security descriptor from SDDL, freed when dropped.
struct Descriptor(PSECURITY_DESCRIPTOR);

impl Descriptor {
    fn from_sddl(sddl: &str) -> std::io::Result<Self> {
        let text: Vec<u16> = OsStr::new(sddl)
            .encode_wide()
            .chain(std::iter::once(0))
            .collect();
        let mut sd: PSECURITY_DESCRIPTOR = null_mut();
        // SAFETY: a NUL-terminated string; the size is not wanted.
        let ok = unsafe {
            ConvertStringSecurityDescriptorToSecurityDescriptorW(
                text.as_ptr(),
                SDDL_REVISION_1,
                &mut sd,
                null::<u32>().cast_mut(),
            )
        } != 0;
        if ok {
            Ok(Self(sd))
        } else {
            Err(std::io::Error::last_os_error())
        }
    }
}

impl Drop for Descriptor {
    fn drop(&mut self) {
        // SAFETY: allocated by the conversion, freed once.
        unsafe { LocalFree(self.0) };
    }
}
