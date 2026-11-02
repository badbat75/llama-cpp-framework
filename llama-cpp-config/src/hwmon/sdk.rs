//! Which physical card each `--device` id is. `--list-devices` names no PCI
//! address, the system does not know an SDK's numbering, and HIP does not number
//! in bus order, so two cards of one model can only be told apart by asking each
//! SDK itself: `hipDeviceGetPCIBusId`, `cuDeviceGetPCIBusId`, and Vulkan's
//! `VK_EXT_pci_bus_info`.
//!
//! The asking runs in a short-lived child of this binary (`sdk-devices`, a hidden
//! command, the `print` below), like `--list-devices` itself: a GPU runtime
//! initialized once stays in its process, and one that crashes must not take the
//! GUI with it. The child loads the runtime llama-server would load and inherits
//! the same environment, which is what makes the numbering the same.
//!
//! What it prints is believed only if it agrees with the probe (`confirmed`): an
//! SDK whose ids or names differ from `--list-devices` (llama.cpp filtered a
//! device, or split one into virtual ones) gives no address at all, and the ids
//! stay at the model's level (`hwmon::name_sdk_ids`).
//!
//! The GUI asks once at startup and again on a refresh (`hwmon::publish_gpus`,
//! behind `gui::spawn_device_probe`). No CLI command asks: none needs it, and it
//! would add a process that loads every GPU runtime.

use super::{os, symbol};
use crate::devices::{pci_address, DeviceOption, Sdk};
use std::collections::HashMap;
use std::ffi::{c_char, c_int, c_void, CStr};
use std::path::{Path, PathBuf};

/// One device an SDK reports, as a line of the child's output.
#[derive(Debug, Clone, PartialEq, Eq)]
struct Seen {
    id: String,
    pci: String,
    name: String,
}

/// From a run of the child: `id -> PCI address` for the devices of a probe, and
/// the SDKs whose runtime loaded.
pub fn ask(devs: &[DeviceOption]) -> (HashMap<String, String>, Vec<Sdk>) {
    let Some(output) = std::env::current_exe()
        .ok()
        .and_then(|exe| crate::proc::run_hidden_probe(&exe, ["sdk-devices"]))
    else {
        return (HashMap::new(), Vec::new());
    };
    let text = String::from_utf8_lossy(&output.stdout);
    (confirmed(devs, &parse_devices(&text)), parse_sdks(&text))
}

fn parse_devices(text: &str) -> Vec<Seen> {
    text.lines()
        .filter_map(|line| {
            let mut fields = line.splitn(3, '\t');
            let (id, bus, name) = (fields.next()?, fields.next()?, fields.next()?);
            Some(Seen {
                id: id.to_string(),
                pci: pci_address(bus)?,
                name: name.trim().to_string(),
            })
        })
        .collect()
}

/// The `@name<TAB>library<TAB>version` lines, each with the version this binary
/// was built against.
fn parse_sdks(text: &str) -> Vec<Sdk> {
    text.lines()
        .filter_map(|line| {
            let mut fields = line.strip_prefix('@')?.splitn(3, '\t');
            let (name, library, version) = (fields.next()?, fields.next()?, fields.next()?);
            let built_with = match name {
                "ROCm" => env!("LLAMA_FW_ROCM"),
                "CUDA" => env!("LLAMA_FW_CUDA"),
                "Vulkan" => env!("LLAMA_FW_VULKAN_SDK"),
                _ => "",
            };
            let known = |text: &str| (!text.is_empty()).then(|| text.to_string());
            Some(Sdk {
                name: name.to_string(),
                library: library.to_string(),
                version: known(version.trim()),
                built_with: known(built_with),
            })
        })
        .collect()
}

/// `ROCm1` -> `ROCm`.
pub fn backend(id: &str) -> &str {
    id.trim_end_matches(|c: char| c.is_ascii_digit())
}

/// The addresses of the SDKs that report exactly the probe's devices: every id,
/// no other, each under the probe's name.
fn confirmed(devs: &[DeviceOption], seen: &[Seen]) -> HashMap<String, String> {
    let mut known = HashMap::new();
    let mut backends: Vec<&str> = seen.iter().map(|s| backend(&s.id)).collect();
    backends.dedup();
    for sdk in backends {
        let probed: Vec<&DeviceOption> = devs.iter().filter(|d| backend(&d.id) == sdk).collect();
        let reported: Vec<&Seen> = seen.iter().filter(|s| backend(&s.id) == sdk).collect();
        let agree = probed.len() == reported.len()
            && probed.iter().all(|d| {
                reported
                    .iter()
                    .any(|s| s.id == d.id && s.name.eq_ignore_ascii_case(&d.name))
            });
        if agree {
            known.extend(reported.iter().map(|s| (s.id.clone(), s.pci.clone())));
        }
    }
    known
}

// ── The child ───────────────────────────────────────────────────────

/// The hidden `sdk-devices` command: for every SDK whose runtime loads here, a
/// `@name<TAB>library<TAB>version` line and one `id<TAB>bus id<TAB>name` line per
/// device. One line at a time, so a runtime that crashes on a later device leaves
/// the SDKs before it.
pub fn print() {
    // SAFETY: each symbol is looked up under the name its SDK's header declares
    // it by, with that header's signature.
    unsafe {
        for path in hip_candidates() {
            let lib = open_path(&path);
            if !lib.is_null() {
                let _ = report_driver_api(lib, "ROCm", &HIP);
                break;
            }
        }
        let cuda = os::open_library(CUDA_LIBRARY);
        if !cuda.is_null() {
            let _ = report_driver_api(cuda, "CUDA", &CUDA);
        }
        if std::env::var_os("GGML_VK_VISIBLE_DEVICES").is_none() {
            for path in vulkan_candidates() {
                let lib = open_path(&path);
                if !lib.is_null() {
                    let _ = report_vulkan(lib);
                    break;
                }
            }
        }
    }
}

/// Vulkan's loader in the order llama-server finds it: a package bundles it, and
/// a build tree leaves it to the system's search (on Windows that can be the
/// path, since not every machine has it in System32).
fn vulkan_candidates() -> Vec<PathBuf> {
    let server = crate::paths::llama_server_exe();
    let server_dir = server.as_deref().and_then(Path::parent);
    let mut found: Vec<PathBuf> = Vec::new();
    #[cfg(not(windows))]
    found.extend(
        server_dir
            .and_then(Path::parent)
            .map(|d| d.join("lib").join(VULKAN_LIBRARY)),
    );
    found.extend(server_dir.map(|d| d.join(VULKAN_LIBRARY)));
    found.push(PathBuf::from(VULKAN_LIBRARY));
    found
}

fn show(id: &str, bus: &str, name: &str) {
    println!("{id}\t{bus}\t{name}");
}

fn show_sdk(name: &str, lib: *mut c_void, file_stem: &str, version: Option<String>) {
    let library = unsafe { loaded_path(lib, file_stem) };
    println!("@{name}\t{library}\t{}", version.unwrap_or_default());
}

/// Where a loaded library came from.
#[cfg(windows)]
unsafe fn loaded_path(lib: *mut c_void, file_stem: &str) -> String {
    use windows_sys::Win32::System::LibraryLoader::GetModuleFileNameW;
    let mut buf = [0u16; 1024];
    match GetModuleFileNameW(lib, buf.as_mut_ptr(), buf.len() as u32) as usize {
        0 => file_stem.to_string(),
        len => String::from_utf16_lossy(&buf[..len]),
    }
}

/// Where a loaded library came from: its line in the process's own memory map.
#[cfg(not(windows))]
unsafe fn loaded_path(_lib: *mut c_void, file_stem: &str) -> String {
    std::fs::read_to_string("/proc/self/maps")
        .ok()
        .and_then(|maps| {
            maps.lines()
                .filter_map(|line| line.split_whitespace().nth(5))
                .find(|path| {
                    Path::new(path)
                        .file_name()
                        .and_then(|n| n.to_str())
                        .is_some_and(|n| n.starts_with(file_stem))
                })
                .map(str::to_string)
        })
        .unwrap_or_else(|| file_stem.to_string())
}

/// HIP's runtime in the order llama-server finds it: beside it (what an install
/// bundles), the directory its children get on the path, then the system's.
fn hip_candidates() -> Vec<PathBuf> {
    let server = crate::paths::llama_server_exe();
    let server_dir = server.as_deref().and_then(Path::parent);
    let mut dirs: Vec<PathBuf> = server_dir.map(Path::to_path_buf).into_iter().collect();
    #[cfg(windows)]
    let (prefix, suffix, system) = {
        dirs.extend(crate::paths::rocm_bin_dir());
        ("amdhip64", ".dll", "amdhip64_7.dll")
    };
    #[cfg(not(windows))]
    let (prefix, suffix, system) = {
        dirs.extend(server_dir.and_then(Path::parent).map(|d| d.join("lib")));
        for var in ["HIP_PATH", "ROCM_PATH"] {
            dirs.extend(std::env::var_os(var).map(|root| PathBuf::from(root).join("lib")));
        }
        ("libamdhip64", ".so", "libamdhip64.so")
    };
    let mut found: Vec<PathBuf> = dirs
        .iter()
        .flat_map(|dir| {
            let mut here: Vec<PathBuf> = std::fs::read_dir(dir)
                .into_iter()
                .flatten()
                .flatten()
                .map(|entry| entry.path())
                .filter(|path| {
                    path.file_name().and_then(|n| n.to_str()).is_some_and(|name| {
                        name.starts_with(prefix) && (name.ends_with(suffix) || name.contains(".so."))
                    })
                })
                .collect();
            here.sort();
            here
        })
        .collect();
    found.push(PathBuf::from(system));
    found
}

/// A runtime by path, its dependencies resolved beside it; a bare name goes
/// through the system's own search.
#[cfg(windows)]
unsafe fn open_path(path: &Path) -> *mut c_void {
    use std::os::windows::ffi::OsStrExt;
    use windows_sys::Win32::System::LibraryLoader::{
        LoadLibraryExW, LOAD_WITH_ALTERED_SEARCH_PATH,
    };
    let wide: Vec<u16> = path.as_os_str().encode_wide().chain(Some(0)).collect();
    let flags = if path.is_absolute() { LOAD_WITH_ALTERED_SEARCH_PATH } else { 0 };
    LoadLibraryExW(wide.as_ptr(), std::ptr::null_mut(), flags)
}

#[cfg(not(windows))]
unsafe fn open_path(path: &Path) -> *mut c_void {
    use std::os::unix::ffi::OsStrExt;
    match std::ffi::CString::new(path.as_os_str().as_bytes()) {
        Ok(name) => os::open_library(&name),
        Err(_) => std::ptr::null_mut(),
    }
}

// ── HIP and CUDA: one driver API under two prefixes ─────────────────

struct DriverApi {
    file_stem: &'static str,
    /// Its runtime's version call, and how that number reads.
    version: &'static CStr,
    read_version: fn(c_int) -> String,
    init: &'static CStr,
    count: &'static CStr,
    get: &'static CStr,
    bus_id: &'static CStr,
    name: &'static CStr,
}

/// `HIP_VERSION`'s form: major * 10000000 + minor * 100000 + patch.
fn hip_version(v: c_int) -> String {
    format!("{}.{}.{}", v / 10_000_000, v / 100_000 % 100, v % 100_000)
}

/// `CUDA_VERSION`'s form: major * 1000 + minor * 10.
fn cuda_version(v: c_int) -> String {
    format!("{}.{}", v / 1000, v % 1000 / 10)
}

const HIP: DriverApi = DriverApi {
    #[cfg(windows)]
    file_stem: "amdhip64",
    #[cfg(not(windows))]
    file_stem: "libamdhip64",
    version: c"hipRuntimeGetVersion",
    read_version: hip_version,
    init: c"hipInit",
    count: c"hipGetDeviceCount",
    get: c"hipDeviceGet",
    bus_id: c"hipDeviceGetPCIBusId",
    name: c"hipDeviceGetName",
};

const CUDA: DriverApi = DriverApi {
    #[cfg(windows)]
    file_stem: "nvcuda",
    #[cfg(not(windows))]
    file_stem: "libcuda",
    version: c"cuDriverGetVersion",
    read_version: cuda_version,
    init: c"cuInit",
    count: c"cuDeviceGetCount",
    get: c"cuDeviceGet",
    bus_id: c"cuDeviceGetPCIBusId",
    name: c"cuDeviceGetName",
};

#[cfg(windows)]
const CUDA_LIBRARY: &CStr = c"nvcuda.dll";
#[cfg(not(windows))]
const CUDA_LIBRARY: &CStr = c"libcuda.so.1";
#[cfg(windows)]
const VULKAN_LIBRARY: &str = "vulkan-1.dll";
#[cfg(not(windows))]
const VULKAN_LIBRARY: &str = "libvulkan.so.1";

unsafe fn report_driver_api(lib: *mut c_void, sdk: &str, api: &DriverApi) -> Option<()> {
    type Text = unsafe extern "C" fn(*mut c_char, c_int, c_int) -> c_int;
    let init: unsafe extern "C" fn(u32) -> c_int = symbol(lib, api.init)?;
    let count: unsafe extern "C" fn(*mut c_int) -> c_int = symbol(lib, api.count)?;
    let get: unsafe extern "C" fn(*mut c_int, c_int) -> c_int = symbol(lib, api.get)?;
    let bus_id: Text = symbol(lib, api.bus_id)?;
    let name: Text = symbol(lib, api.name)?;
    if init(0) != 0 {
        return None;
    }
    let version: Option<unsafe extern "C" fn(*mut c_int) -> c_int> = symbol(lib, api.version);
    let version = version.and_then(|read| {
        let mut v = 0;
        (read(&mut v) == 0).then(|| (api.read_version)(v))
    });
    show_sdk(sdk, lib, api.file_stem, version);
    let mut n = 0;
    if count(&mut n) != 0 {
        return None;
    }
    for ordinal in 0..n {
        let mut device = 0;
        if get(&mut device, ordinal) != 0 {
            return None;
        }
        let text = |read: Text| {
            let mut buf = [0 as c_char; 256];
            (read(buf.as_mut_ptr(), buf.len() as c_int, device) == 0)
                .then(|| CStr::from_ptr(buf.as_ptr()).to_string_lossy().into_owned())
        };
        show(&format!("{sdk}{ordinal}"), &text(bus_id)?, &text(name)?);
    }
    Some(())
}

// ── Vulkan ──────────────────────────────────────────────────────────

const STRUCTURE_APPLICATION_INFO: u32 = 0;
const STRUCTURE_INSTANCE_CREATE_INFO: u32 = 1;
const STRUCTURE_PROPERTIES_2: u32 = 1000059001;
const STRUCTURE_PCI_BUS_INFO: u32 = 1000212000;
const API_VERSION_1_1: u32 = (1 << 22) | (1 << 12);
const DEVICE_TYPE_INTEGRATED_GPU: u32 = 1;
const DEVICE_TYPE_DISCRETE_GPU: u32 = 2;
/// In `VkPhysicalDeviceProperties`: `deviceType`, then `deviceName`.
const DEVICE_TYPE_AT: usize = 16;
const DEVICE_NAME_AT: usize = 20;
const DEVICE_NAME_SIZE: usize = 256;

#[repr(C)]
struct ApplicationInfo {
    s_type: u32,
    next: *const c_void,
    application_name: *const c_char,
    application_version: u32,
    engine_name: *const c_char,
    engine_version: u32,
    api_version: u32,
}

#[repr(C)]
struct InstanceCreateInfo {
    s_type: u32,
    next: *const c_void,
    flags: u32,
    application_info: *const ApplicationInfo,
    layer_count: u32,
    layers: *const *const c_char,
    extension_count: u32,
    extensions: *const *const c_char,
}

#[repr(C)]
struct PciBusInfo {
    s_type: u32,
    next: *mut c_void,
    domain: u32,
    bus: u32,
    device: u32,
    function: u32,
}

/// `VkPhysicalDeviceProperties2`. Only the head of the properties is read, the
/// rest is room: the array is larger than the structure has ever been.
#[repr(C, align(8))]
struct Properties2 {
    s_type: u32,
    next: *mut PciBusInfo,
    properties: [u8; 2048],
}

/// The devices as llama.cpp's Vulkan backend numbers them: every integrated or
/// discrete GPU, in enumeration order. It keeps one of two entries for a card
/// with two drivers, and which one can reorder the rest, so a card seen twice
/// prints nothing: `confirmed` then finds the ids missing.
unsafe fn report_vulkan(lib: *mut c_void) -> Option<()> {
    type ProcAddr = unsafe extern "system" fn(*mut c_void, *const c_char) -> *mut c_void;
    type Create = unsafe extern "system" fn(*const InstanceCreateInfo, *const c_void, *mut *mut c_void) -> i32;
    type Enumerate = unsafe extern "system" fn(*mut c_void, *mut u32, *mut *mut c_void) -> i32;
    type GetProperties = unsafe extern "system" fn(*mut c_void, *mut Properties2);

    let proc_addr: ProcAddr = symbol(lib, c"vkGetInstanceProcAddr")?;
    let create = proc_addr(std::ptr::null_mut(), c"vkCreateInstance".as_ptr());
    let create: Create = (!create.is_null()).then(|| std::mem::transmute_copy(&create))?;
    let version = proc_addr(std::ptr::null_mut(), c"vkEnumerateInstanceVersion".as_ptr());
    let version = (!version.is_null()).then(|| {
        let read: unsafe extern "system" fn(*mut u32) -> i32 = std::mem::transmute_copy(&version);
        let mut v = 0u32;
        (read(&mut v) == 0).then(|| format!("{}.{}.{}", (v >> 22) & 0x7f, (v >> 12) & 0x3ff, v & 0xfff))
    });
    #[cfg(windows)]
    let file_stem = "vulkan-1";
    #[cfg(not(windows))]
    let file_stem = "libvulkan";
    show_sdk("Vulkan", lib, file_stem, version.flatten());

    let application = ApplicationInfo {
        s_type: STRUCTURE_APPLICATION_INFO,
        next: std::ptr::null(),
        application_name: std::ptr::null(),
        application_version: 0,
        engine_name: std::ptr::null(),
        engine_version: 0,
        api_version: API_VERSION_1_1,
    };
    let info = InstanceCreateInfo {
        s_type: STRUCTURE_INSTANCE_CREATE_INFO,
        next: std::ptr::null(),
        flags: 0,
        application_info: &application,
        layer_count: 0,
        layers: std::ptr::null(),
        extension_count: 0,
        extensions: std::ptr::null(),
    };
    let mut instance = std::ptr::null_mut();
    if create(&info, std::ptr::null(), &mut instance) != 0 {
        return None;
    }
    let by_name = |name: &CStr| proc_addr(instance, name.as_ptr());
    let enumerate = by_name(c"vkEnumeratePhysicalDevices");
    let enumerate: Enumerate = (!enumerate.is_null()).then(|| std::mem::transmute_copy(&enumerate))?;
    let properties = by_name(c"vkGetPhysicalDeviceProperties2");
    let properties: GetProperties =
        (!properties.is_null()).then(|| std::mem::transmute_copy(&properties))?;

    let mut count = 0u32;
    if enumerate(instance, &mut count, std::ptr::null_mut()) != 0 {
        return None;
    }
    let mut devices = vec![std::ptr::null_mut(); count as usize];
    if enumerate(instance, &mut count, devices.as_mut_ptr()) != 0 {
        return None;
    }

    let mut gpus: Vec<(String, String)> = Vec::new();
    for device in devices.into_iter().take(count as usize) {
        let mut bus = PciBusInfo {
            s_type: STRUCTURE_PCI_BUS_INFO,
            next: std::ptr::null_mut(),
            // Left as it is by a driver without the extension.
            domain: u32::MAX,
            bus: 0,
            device: 0,
            function: 0,
        };
        let mut props = Properties2 {
            s_type: STRUCTURE_PROPERTIES_2,
            next: &mut bus,
            properties: [0; 2048],
        };
        properties(device, &mut props);
        let head = &props.properties;
        let kind = u32::from_ne_bytes(head[DEVICE_TYPE_AT..DEVICE_TYPE_AT + 4].try_into().ok()?);
        if kind != DEVICE_TYPE_INTEGRATED_GPU && kind != DEVICE_TYPE_DISCRETE_GPU {
            continue;
        }
        if bus.domain == u32::MAX {
            return None;
        }
        let name = CStr::from_bytes_until_nul(&head[DEVICE_NAME_AT..DEVICE_NAME_AT + DEVICE_NAME_SIZE])
            .ok()?
            .to_string_lossy()
            .into_owned();
        let address = format!(
            "{:04x}:{:02x}:{:02x}.{:x}",
            bus.domain, bus.bus, bus.device, bus.function
        );
        if gpus.iter().any(|(known, _)| *known == address) {
            return None;
        }
        gpus.push((address, name));
    }
    for (ordinal, (address, name)) in gpus.iter().enumerate() {
        show(&format!("Vulkan{ordinal}"), address, name);
    }
    Some(())
}

#[cfg(test)]
mod tests {
    use super::*;

    const PROBE: &str = "  ROCm0: iGPU (1 MiB, 1 MiB free)\n  \
         ROCm1: Card (1 MiB, 1 MiB free)\n  \
         ROCm2: Card (1 MiB, 1 MiB free)\n  \
         Vulkan0: Card (1 MiB, 1 MiB free)\n  \
         CPU: Ryzen (1 MiB, 1 MiB free)\n";

    #[test]
    fn an_sdk_is_believed_only_if_it_reports_the_probe_s_devices() {
        let devs = crate::devices::parse(PROBE);
        let report = "@ROCm\tC:\\rt\\amdhip64_7.dll\t7.13.1\n\
                      ROCm0\t0000:7d:00.0\tiGPU\n\
                      ROCm1\t0000:14:00.0\tCard\n\
                      ROCm2\t0000:03:00.0\tCARD\n\
                      not a device line\n\
                      @Other\tlib\t\n\
                      Vulkan0\t0000:03:00.0\tCard\n";
        let known = confirmed(&devs, &parse_devices(report));
        assert_eq!(known["ROCm1"], "14:00.0");
        assert_eq!(known["ROCm2"], "03:00.0");
        assert_eq!(known["Vulkan0"], "03:00.0");
        assert_eq!(known.len(), 4);

        // A device the probe does not have: that SDK says nothing, others stand.
        let extra = format!("{report}Vulkan1\t0000:14:00.0\tCard\n");
        let known = confirmed(&devs, &parse_devices(&extra));
        assert!(!known.contains_key("Vulkan0"));
        assert_eq!(known["ROCm1"], "14:00.0");

        // The same id under another name is another numbering.
        let renamed = report.replace("ROCm0\t0000:7d:00.0\tiGPU", "ROCm0\t0000:7d:00.0\tOther");
        assert!(!confirmed(&devs, &parse_devices(&renamed)).contains_key("ROCm1"));

        // The SDK lines are read apart from the devices, a missing version too.
        let sdks = parse_sdks(report);
        assert_eq!(sdks.len(), 2);
        assert_eq!(sdks[0].name, "ROCm");
        assert_eq!(sdks[0].library, "C:\\rt\\amdhip64_7.dll");
        assert_eq!(sdks[0].version.as_deref(), Some("7.13.1"));
        assert_eq!((sdks[1].version.as_deref(), sdks[1].built_with.as_deref()), (None, None));
    }

    #[test]
    fn a_runtime_s_version_number_reads_as_its_header_defines_it() {
        assert_eq!(hip_version(71_300_123), "7.13.123");
        assert_eq!(cuda_version(12_080), "12.8");
    }
}
