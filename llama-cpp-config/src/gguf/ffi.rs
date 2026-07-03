// ggml-base.dll glue — the `KvSource` backed by llama.cpp's own gguf reader.
//
// Split out of gguf.rs so the field-extraction logic (`ModelInfo::from_kv` &
// friends) reads without the ~230 lines of Win32 FFI below. The module's public
// surface is just `open(path) -> Option<Ctx>` plus `Ctx: KvSource`; the parent
// picks the right platform impl through the re-exports here.

// Only `open` is re-exported: callers use the returned `Ctx` (a `KvSource`) by
// value/ref without ever naming its type, so re-exporting `Ctx` too would be an
// unused import.
#[cfg(not(windows))]
pub use stub_impl::open;
#[cfg(windows)]
pub use windows_impl::open;

/// Windows: dynamically load `ggml-base.dll` and expose a `KvSource` over a live
/// `gguf_context`. The DLL ships next to `llama-cpp-config.exe` in `bin\`.
#[cfg(windows)]
mod windows_impl {
    use crate::gguf::KvSource;
    use core::ffi::c_void;
    use std::ffi::{CStr, CString};
    use std::os::raw::c_char;
    use std::os::windows::ffi::OsStrExt;
    use std::path::Path;
    use std::sync::OnceLock;

    type HModule = *mut c_void;
    type GgufCtx = *mut c_void;

    // GGUF scalar value-type tags (ggml/include/gguf.h).
    const T_U8: i32 = 0;
    const T_I8: i32 = 1;
    const T_U16: i32 = 2;
    const T_I16: i32 = 3;
    const T_U32: i32 = 4;
    const T_I32: i32 = 5;
    const T_BOOL: i32 = 7;
    const T_STRING: i32 = 8;
    const T_U64: i32 = 10;
    const T_I64: i32 = 11;

    #[repr(C)]
    struct InitParams {
        no_alloc: bool,
        ctx: *mut GgufCtx,
    }

    type FnInit = unsafe extern "C" fn(*const c_char, InitParams) -> GgufCtx;
    type FnFree = unsafe extern "C" fn(GgufCtx);
    type FnFind = unsafe extern "C" fn(GgufCtx, *const c_char) -> i64;
    type FnType = unsafe extern "C" fn(GgufCtx, i64) -> i32;
    type FnU8 = unsafe extern "C" fn(GgufCtx, i64) -> u8;
    type FnI8 = unsafe extern "C" fn(GgufCtx, i64) -> i8;
    type FnU16 = unsafe extern "C" fn(GgufCtx, i64) -> u16;
    type FnI16 = unsafe extern "C" fn(GgufCtx, i64) -> i16;
    type FnU32 = unsafe extern "C" fn(GgufCtx, i64) -> u32;
    type FnI32 = unsafe extern "C" fn(GgufCtx, i64) -> i32;
    type FnU64 = unsafe extern "C" fn(GgufCtx, i64) -> u64;
    type FnI64 = unsafe extern "C" fn(GgufCtx, i64) -> i64;
    type FnBool = unsafe extern "C" fn(GgufCtx, i64) -> bool;
    type FnStr = unsafe extern "C" fn(GgufCtx, i64) -> *const c_char;

    struct Api {
        init: FnInit,
        free: FnFree,
        find_key: FnFind,
        kv_type: FnType,
        v_u8: FnU8,
        v_i8: FnI8,
        v_u16: FnU16,
        v_i16: FnI16,
        v_u32: FnU32,
        v_i32: FnI32,
        v_u64: FnU64,
        v_i64: FnI64,
        v_bool: FnBool,
        v_str: FnStr,
    }
    // `Api` holds only C function pointers (Send + Sync), so it is safe to cache
    // in a `static OnceLock`.

    #[link(name = "kernel32")]
    extern "system" {
        fn LoadLibraryW(name: *const u16) -> HModule;
        fn GetProcAddress(module: HModule, name: *const u8) -> *const c_void;
    }

    static API: OnceLock<Option<Api>> = OnceLock::new();

    fn wide(p: &Path) -> Vec<u16> {
        p.as_os_str()
            .encode_wide()
            .chain(std::iter::once(0))
            .collect()
    }

    fn load_dll() -> Option<HModule> {
        // Prefer the DLL next to our own exe (installed `bin\`); fall back to the
        // default search order.
        let mut candidates: Vec<std::path::PathBuf> = Vec::new();
        if let Ok(exe) = std::env::current_exe() {
            if let Some(dir) = exe.parent() {
                candidates.push(dir.join("ggml-base.dll"));
            }
        }
        candidates.push(std::path::PathBuf::from("ggml-base.dll"));
        for cand in candidates {
            let w = wide(&cand);
            let h = unsafe { LoadLibraryW(w.as_ptr()) };
            if !h.is_null() {
                return Some(h);
            }
        }
        None
    }

    fn proc(h: HModule, name: &[u8]) -> Option<*const c_void> {
        let p = unsafe { GetProcAddress(h, name.as_ptr()) };
        if p.is_null() {
            None
        } else {
            Some(p)
        }
    }

    fn load() -> Option<Api> {
        let h = load_dll()?;
        // Resolve a symbol and transmute it to its declared fn-pointer alias.
        // A macro (not a fn) so each symbol name stays a plain, greppable literal
        // and the target type is explicit at the call site; `?` propagates a
        // missing symbol as `None`.
        macro_rules! sym {
            ($ty:ty, $name:literal) => {
                std::mem::transmute::<*const c_void, $ty>(proc(h, $name)?)
            };
        }
        // SAFETY: symbols resolved from ggml-base.dll have the C signatures
        // declared in `gguf.h`; each is transmuted to its matching fn pointer.
        unsafe {
            Some(Api {
                init: sym!(FnInit, b"gguf_init_from_file\0"),
                free: sym!(FnFree, b"gguf_free\0"),
                find_key: sym!(FnFind, b"gguf_find_key\0"),
                kv_type: sym!(FnType, b"gguf_get_kv_type\0"),
                v_u8: sym!(FnU8, b"gguf_get_val_u8\0"),
                v_i8: sym!(FnI8, b"gguf_get_val_i8\0"),
                v_u16: sym!(FnU16, b"gguf_get_val_u16\0"),
                v_i16: sym!(FnI16, b"gguf_get_val_i16\0"),
                v_u32: sym!(FnU32, b"gguf_get_val_u32\0"),
                v_i32: sym!(FnI32, b"gguf_get_val_i32\0"),
                v_u64: sym!(FnU64, b"gguf_get_val_u64\0"),
                v_i64: sym!(FnI64, b"gguf_get_val_i64\0"),
                v_bool: sym!(FnBool, b"gguf_get_val_bool\0"),
                v_str: sym!(FnStr, b"gguf_get_val_str\0"),
            })
        }
    }

    fn api() -> Option<&'static Api> {
        API.get_or_init(load).as_ref()
    }

    pub struct Ctx {
        ptr: GgufCtx,
        api: &'static Api,
    }

    /// Open a GGUF file's metadata (header + tensor infos only; `no_alloc`).
    pub fn open(path: &Path) -> Option<Ctx> {
        let api = api()?;
        // ggml_fopen converts this UTF-8 path back to wide (`_wfopen`), so
        // Unicode paths are fine.
        let c = CString::new(path.to_string_lossy().as_bytes()).ok()?;
        let params = InitParams {
            no_alloc: true,
            ctx: core::ptr::null_mut(),
        };
        let ptr = unsafe { (api.init)(c.as_ptr(), params) };
        if ptr.is_null() {
            return None;
        }
        Some(Ctx { ptr, api })
    }

    impl Ctx {
        fn find(&self, key: &str) -> Option<i64> {
            let c = CString::new(key).ok()?;
            let id = unsafe { (self.api.find_key)(self.ptr, c.as_ptr()) };
            (id >= 0).then_some(id)
        }
    }

    impl Drop for Ctx {
        fn drop(&mut self) {
            unsafe { (self.api.free)(self.ptr) }
        }
    }

    impl KvSource for Ctx {
        fn u32(&self, key: &str) -> Option<u32> {
            let id = self.find(key)?;
            // SAFETY: `id` came from find_key on this ctx; getters read the value
            // according to its stored type.
            unsafe {
                match (self.api.kv_type)(self.ptr, id) {
                    T_U32 => Some((self.api.v_u32)(self.ptr, id)),
                    T_I32 => u32::try_from((self.api.v_i32)(self.ptr, id)).ok(),
                    T_U64 => u32::try_from((self.api.v_u64)(self.ptr, id)).ok(),
                    T_I64 => u32::try_from((self.api.v_i64)(self.ptr, id)).ok(),
                    T_U16 => Some((self.api.v_u16)(self.ptr, id) as u32),
                    T_I16 => u32::try_from((self.api.v_i16)(self.ptr, id)).ok(),
                    T_U8 => Some((self.api.v_u8)(self.ptr, id) as u32),
                    T_I8 => u32::try_from((self.api.v_i8)(self.ptr, id)).ok(),
                    _ => None,
                }
            }
        }

        fn string(&self, key: &str) -> Option<String> {
            let id = self.find(key)?;
            // SAFETY: only read as a string when the stored type is STRING; the
            // returned pointer is owned by the ctx and copied out before free.
            unsafe {
                if (self.api.kv_type)(self.ptr, id) != T_STRING {
                    return None;
                }
                let p = (self.api.v_str)(self.ptr, id);
                if p.is_null() {
                    return None;
                }
                Some(CStr::from_ptr(p).to_string_lossy().into_owned())
            }
        }

        fn boolean(&self, key: &str) -> Option<bool> {
            let id = self.find(key)?;
            // SAFETY: only read as a bool when the stored type is BOOL.
            unsafe {
                if (self.api.kv_type)(self.ptr, id) != T_BOOL {
                    return None;
                }
                Some((self.api.v_bool)(self.ptr, id))
            }
        }
    }
}

/// Non-Windows: no ggml-base.dll to load, so reads yield `None` (the info box
/// shows "unavailable"). The stub keeps `from_kv` referenced on every platform.
#[cfg(not(windows))]
mod stub_impl {
    use crate::gguf::KvSource;
    use std::path::Path;

    pub struct Ctx;

    pub fn open(_path: &Path) -> Option<Ctx> {
        None
    }

    impl KvSource for Ctx {
        fn u32(&self, _key: &str) -> Option<u32> {
            None
        }
        fn string(&self, _key: &str) -> Option<String> {
            None
        }
        fn boolean(&self, _key: &str) -> Option<bool> {
            None
        }
    }
}
