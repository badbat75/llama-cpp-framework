//! ggml-base glue: the `KvSource` backed by llama.cpp's own gguf reader, loaded
//! at run time (`ggml-base.dll` on Windows, `libggml-base.so.0` on Linux).
//!
//! Split out of gguf.rs so the field-extraction logic (`ModelInfo::from_kv` &
//! friends) reads without the ~230 lines of Win32 FFI below. The module's public
//! surface is just `open(path) -> Option<Ctx>` plus `Ctx: KvSource`; the parent
//! picks the right platform impl through the re-exports here.

// Only `open` is re-exported: callers use the returned `Ctx` (a `KvSource`) by
// value/ref without ever naming its type, so re-exporting `Ctx` too would be an
// unused import.
#[cfg(any(windows, target_os = "linux"))]
pub use loaded_impl::open;
#[cfg(not(any(windows, target_os = "linux")))]
pub use stub_impl::open;

/// Dynamically load ggml-base and expose a `KvSource` over a live
/// `gguf_context`. On Windows the DLL ships next to `llama-cpp-config.exe` in
/// `bin\`; on Linux the package puts `libggml-base.so.0` in `lib/` beside
/// `bin/`, and a build tree keeps it beside llama-server.
#[cfg(any(windows, target_os = "linux"))]
mod loaded_impl {
    use crate::gguf::{KvSource, TensorInfo};
    use core::ffi::c_void;
    use std::ffi::{CStr, CString};
    use std::os::raw::c_char;
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
    const T_F32: i32 = 6;
    const T_F64: i32 = 12;

    #[repr(C)]
    struct InitParams {
        no_alloc: bool,
        ctx: *mut GgufCtx,
    }

    type FnInit = unsafe extern "C" fn(*const c_char, InitParams) -> GgufCtx;
    type FnFree = unsafe extern "C" fn(GgufCtx);
    type FnFind = unsafe extern "C" fn(GgufCtx, *const c_char) -> i64;
    type FnType = unsafe extern "C" fn(GgufCtx, i64) -> i32;
    // Tensor infos (not KV): `gguf_find_tensor` returns -1 when absent, and the
    // type is a `ggml_type` enum (ggml.h), NOT the `general.file_type` LLAMA_FTYPE
    // that `ftype_name` maps; the two enums disagree on nearly every value.
    type FnFindTensor = unsafe extern "C" fn(GgufCtx, *const c_char) -> i64;
    type FnTensorType = unsafe extern "C" fn(GgufCtx, i64) -> i32;
    type FnTensorSize = unsafe extern "C" fn(GgufCtx, i64) -> usize;
    type FnNTensors = unsafe extern "C" fn(GgufCtx) -> i64;
    type FnTensorName = unsafe extern "C" fn(GgufCtx, i64) -> *const c_char;
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
    type FnF32 = unsafe extern "C" fn(GgufCtx, i64) -> f32;
    type FnF64 = unsafe extern "C" fn(GgufCtx, i64) -> f64;

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
        v_f32: FnF32,
        v_f64: FnF64,
        find_tensor: FnFindTensor,
        tensor_type: FnTensorType,
        tensor_size: FnTensorSize,
        n_tensors: FnNTensors,
        tensor_name: FnTensorName,
    }
    // `Api` holds only C function pointers (Send + Sync), so it is safe to cache
    // in a `static OnceLock`. Only SUCCESS is cached; see `api()`.

    static API: OnceLock<Api> = OnceLock::new();

    #[cfg(windows)]
    mod os {
        use super::HModule;
        use core::ffi::c_void;
        use std::os::windows::ffi::OsStrExt;
        use std::path::{Path, PathBuf};
        use windows_sys::Win32::System::LibraryLoader::{GetProcAddress, LoadLibraryW};

        fn wide(p: &Path) -> Vec<u16> {
            p.as_os_str()
                .encode_wide()
                .chain(std::iter::once(0))
                .collect()
        }

        pub(super) fn load_lib() -> Option<HModule> {
            // Prefer the DLL next to our own exe (installed `bin\`); fall back to
            // the default search order.
            let mut candidates: Vec<PathBuf> = Vec::new();
            if let Ok(exe) = std::env::current_exe() {
                if let Some(dir) = exe.parent() {
                    candidates.push(dir.join("ggml-base.dll"));
                }
            }
            candidates.push(PathBuf::from("ggml-base.dll"));
            for cand in candidates {
                let w = wide(&cand);
                let h = unsafe { LoadLibraryW(w.as_ptr()) };
                if !h.is_null() {
                    return Some(h);
                }
            }
            None
        }

        pub(super) fn proc(h: HModule, name: &[u8]) -> Option<*const c_void> {
            let p = unsafe { GetProcAddress(h, name.as_ptr()) };
            p.map(|f| f as *const c_void)
        }
    }

    #[cfg(target_os = "linux")]
    mod os {
        use super::HModule;
        use core::ffi::c_void;
        use std::ffi::CString;
        use std::os::raw::{c_char, c_int};
        use std::path::PathBuf;

        // libc's own (glibc 2.34+ carries them; older ones through libdl, which
        // std already links). Declared as in hwmon/linux.rs.
        extern "C" {
            fn dlopen(filename: *const c_char, flags: c_int) -> *mut c_void;
            fn dlsym(handle: *mut c_void, symbol: *const c_char) -> *mut c_void;
        }
        const RTLD_NOW: c_int = 2;
        const LIB: &str = "libggml-base.so.0";

        pub(super) fn load_lib() -> Option<HModule> {
            // The package: `<prefix>/bin/llama-cpp-config` and
            // `<prefix>/lib/libggml-base.so.0`. A build tree: beside llama-server
            // (`llama.cpp-cmake/bin`). Last, the loader's own search path.
            let mut candidates: Vec<PathBuf> = Vec::new();
            if let Some(dir) = std::env::current_exe()
                .ok()
                .and_then(|e| e.parent().map(|d| d.to_path_buf()))
            {
                candidates.push(dir.join("..").join("lib").join(LIB));
                candidates.push(dir.join(LIB));
            }
            if let Some(dir) =
                crate::paths::llama_server_exe().and_then(|e| e.parent().map(|d| d.to_path_buf()))
            {
                candidates.push(dir.join(LIB));
            }
            candidates.push(PathBuf::from(LIB));
            for cand in candidates {
                // A bare name goes to the loader's search; a path is opened as is.
                if cand.components().count() > 1 && !cand.exists() {
                    continue;
                }
                let Ok(c) = CString::new(cand.to_string_lossy().as_bytes()) else {
                    continue;
                };
                let h = unsafe { dlopen(c.as_ptr(), RTLD_NOW) };
                if !h.is_null() {
                    return Some(h);
                }
            }
            None
        }

        pub(super) fn proc(h: HModule, name: &[u8]) -> Option<*const c_void> {
            let p = unsafe { dlsym(h, name.as_ptr().cast()) };
            (!p.is_null()).then_some(p.cast_const())
        }
    }

    fn load() -> Option<Api> {
        let h = os::load_lib()?;
        // Resolve a symbol and transmute it to its declared fn-pointer alias.
        // A macro (not a fn) so each symbol name stays a plain, greppable literal
        // and the target type is explicit at the call site; `?` propagates a
        // missing symbol as `None`.
        macro_rules! sym {
            ($ty:ty, $name:literal) => {
                std::mem::transmute::<*const c_void, $ty>(os::proc(h, $name)?)
            };
        }
        // SAFETY: symbols resolved from ggml-base have the C signatures
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
                v_f32: sym!(FnF32, b"gguf_get_val_f32\0"),
                v_f64: sym!(FnF64, b"gguf_get_val_f64\0"),
                find_tensor: sym!(FnFindTensor, b"gguf_find_tensor\0"),
                tensor_type: sym!(FnTensorType, b"gguf_get_tensor_type\0"),
                tensor_size: sym!(FnTensorSize, b"gguf_get_tensor_size\0"),
                n_tensors: sym!(FnNTensors, b"gguf_get_n_tensors\0"),
                tensor_name: sym!(FnTensorName, b"gguf_get_tensor_name\0"),
            })
        }
    }

    // Memoize SUCCESS only: caching a failed load would leave the Model-info
    // box dead until restart when the configurator started before the DLL
    // existed (e.g. mid `xtask build`); the exe tree can change under us,
    // and the other probes (version, devices) do re-probe on Refresh. A failed
    // `LoadLibraryW` is cheap, so retrying per read costs nothing.
    fn api() -> Option<&'static Api> {
        if let Some(a) = API.get() {
            return Some(a);
        }
        let a = load()?;
        // If another thread won a race, its identical instance is kept.
        Some(API.get_or_init(|| a))
    }

    pub struct Ctx {
        ptr: GgufCtx,
        api: &'static Api,
    }

    /// Open a GGUF file's metadata (header + tensor infos only; `no_alloc`).
    pub fn open(path: &Path) -> Option<Ctx> {
        let api = api()?;
        // ggml_fopen converts this UTF-8 path back to wide (`_wfopen`) on
        // Windows, so Unicode paths are fine; Linux takes the bytes as they are.
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
        /// Every tensor this file holds (one shard's worth for a split model),
        /// in file order, with its size in bytes.
        pub fn tensors(&self) -> Vec<TensorInfo> {
            // SAFETY: ids run 0..n_tensors on this ctx; the name pointer is
            // owned by the ctx and copied out before it is freed.
            unsafe {
                let n = (self.api.n_tensors)(self.ptr).max(0);
                (0..n)
                    .filter_map(|id| {
                        let p = (self.api.tensor_name)(self.ptr, id);
                        if p.is_null() {
                            return None;
                        }
                        Some(TensorInfo {
                            name: CStr::from_ptr(p).to_string_lossy().into_owned(),
                            bytes: (self.api.tensor_size)(self.ptr, id) as u64,
                        })
                    })
                    .collect()
            }
        }

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

        fn f64(&self, key: &str) -> Option<f64> {
            let id = self.find(key)?;
            // SAFETY: each getter is called only for its own stored type.
            unsafe {
                match (self.api.kv_type)(self.ptr, id) {
                    T_F32 => Some(crate::gguf::f32_decimal((self.api.v_f32)(self.ptr, id))),
                    T_F64 => Some((self.api.v_f64)(self.ptr, id)),
                    _ => self.i64(key).map(|v| v as f64),
                }
            }
        }

        fn i64(&self, key: &str) -> Option<i64> {
            let id = self.find(key)?;
            // SAFETY: each getter is called only for its own stored type.
            unsafe {
                match (self.api.kv_type)(self.ptr, id) {
                    T_I64 => Some((self.api.v_i64)(self.ptr, id)),
                    T_I32 => Some((self.api.v_i32)(self.ptr, id) as i64),
                    T_I16 => Some((self.api.v_i16)(self.ptr, id) as i64),
                    T_I8 => Some((self.api.v_i8)(self.ptr, id) as i64),
                    T_U64 => i64::try_from((self.api.v_u64)(self.ptr, id)).ok(),
                    T_U32 => Some((self.api.v_u32)(self.ptr, id) as i64),
                    T_U16 => Some((self.api.v_u16)(self.ptr, id) as i64),
                    T_U8 => Some((self.api.v_u8)(self.ptr, id) as i64),
                    _ => None,
                }
            }
        }

        fn tensor(&self, name: &str) -> Option<(u32, u64)> {
            let c = CString::new(name).ok()?;
            // SAFETY: find_tensor returns -1 for an absent name; the getters are
            // only called with an id it handed back, which indexes this ctx's
            // tensor infos (read at `open` under no_alloc).
            unsafe {
                let id = (self.api.find_tensor)(self.ptr, c.as_ptr());
                if id < 0 {
                    return None;
                }
                let ty = (self.api.tensor_type)(self.ptr, id);
                let size = (self.api.tensor_size)(self.ptr, id);
                Some((u32::try_from(ty).ok()?, size as u64))
            }
        }
    }
}

/// Any other OS: no ggml-base to load, so reads yield `None` (the info box
/// shows "unavailable"). The stub keeps `from_kv` referenced on every platform.
#[cfg(not(any(windows, target_os = "linux")))]
mod stub_impl {
    use crate::gguf::{KvSource, TensorInfo};
    use std::path::Path;

    pub struct Ctx;

    pub fn open(_path: &Path) -> Option<Ctx> {
        None
    }

    impl Ctx {
        pub fn tensors(&self) -> Vec<TensorInfo> {
            Vec::new()
        }
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
        fn f64(&self, _key: &str) -> Option<f64> {
            None
        }
        fn i64(&self, _key: &str) -> Option<i64> {
            None
        }
        fn tensor(&self, _name: &str) -> Option<(u32, u64)> {
            None
        }
    }
}
