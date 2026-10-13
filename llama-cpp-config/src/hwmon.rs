//! The sampler behind the HW Monitor window (`gui/hw_window.rs`): one
//! `Sampler::sample()` per second turns the system's counters into the bars
//! the window draws. This file holds what both systems share (the bar and
//! group types, the formatting, NVML); the sources themselves are one module
//! per system, each exporting its own `Sampler`: `hwmon/linux.rs` and
//! `hwmon/windows.rs`. `main.rs` gates the module to those two.
//!
//! Where each bar comes from, none of it needing root or elevation. On Linux:
//!
//! | Bar | Source |
//! |-----|--------|
//! | GPU, VRAM, GTT, Memory activity (amdgpu) | `/sys/class/drm/cardN/device/`: `gpu_busy_percent`, `mem_info_vram_*`, `mem_info_gtt_*`, `mem_busy_percent` |
//! | GPU, VRAM, Memory activity, PCIe RX/TX (NVIDIA) | NVML (`libnvidia-ml.so.1`, loaded at runtime) |
//! | CPU | `/proc/stat`, first line |
//! | RAM, Page cache, Swap | `/proc/meminfo` |
//! | Disk read, write, busy | `/sys/block/<disk>/stat` of the disk holding the models directory |
//!
//! On Windows:
//!
//! | Bar | Source |
//! |-----|--------|
//! | GPU, VRAM, Shared memory (every adapter) | The performance counters Task Manager reads, through PDH: `\GPU Engine(*)\Utilization Percentage`, `\GPU Adapter Memory(*)\Dedicated Usage` and `Shared Usage`. The adapters, their names and both totals come from D3DKMT (`gdi32`) |
//! | GPU, VRAM, Memory activity, PCIe RX/TX (NVIDIA) | NVML (`nvml.dll`, loaded at runtime from System32), with Shared memory from PDH as above |
//! | CPU | `GetSystemTimes` |
//! | RAM, Committed | `GetPerformanceInfo` |
//! | Cached | PDH: the standby lists plus the modified list, which is Task Manager's "Cached" |
//! | Disk read, write, busy | `IOCTL_DISK_PERFORMANCE` on the physical drive holding the models directory |
//!
//! Things that are not obvious from the tables:
//!
//! - **GTT is what Windows calls Shared GPU memory**: system RAM the GPU has
//!   mapped. NVIDIA has no such bar on Linux, since its driver never spills
//!   into system RAM there; on Windows every adapter has one, and a card
//!   that is oversubscribed fills it instead of failing the allocation.
//! - **Every GPU source is optional.** Which amdgpu files exist depends on
//!   the card (an integrated GPU has no `mem_busy_percent`, for one), and on
//!   Windows the performance counters can be missing or broken. A missing
//!   source drops its bar; it never drops the card.
//! - **A rate has no natural 100%**, so the PCIe and disk rate bars are drawn
//!   against the highest rate seen since the window opened (never below
//!   `RATE_FLOOR`, so an idle link does not read as a full bar), and the text
//!   names that peak.
//!
//! Each device has a checkbox in the window. `sample` takes the set of the
//! ones switched off (by `Group::id`; the window keeps it in settings.ini) and
//! reads NOTHING for them: no sysfs file, no NVML call, no counter. A rate
//! needs two readings, so a device switched back on shows its rate bars from
//! the second sample on. (The one approximation is on Windows: a PDH query
//! collects every adapter at once, so it is skipped only when no adapter that
//! needs it is on.)
//!
//! NVML is loaded at runtime, not linked: a machine without the NVIDIA
//! driver simply has no NVIDIA readings. `nvmlDeviceGetPcieThroughput`
//! measures over 20 ms per call, which is why the sampler runs on a worker
//! thread and not on a UI timer.

use std::collections::HashSet;
use std::ffi::{c_char, c_int, c_uint, c_void, CStr};
use std::sync::OnceLock;

#[cfg(target_os = "linux")]
mod linux;
#[cfg(target_os = "linux")]
use linux as os;
#[cfg(windows)]
mod windows;
#[cfg(windows)]
use windows as os;

pub use os::Sampler;

const GIB: f64 = (1u64 << 30) as f64;
const MIB: f64 = (1u64 << 20) as f64;
/// Lowest full-scale value of a rate bar, in bytes per second.
const RATE_FLOOR: f64 = 16.0 * MIB;

/// One horizontal bar: `fraction` (0 to 1) is the fill, `text` the readout.
#[derive(Debug, Clone, PartialEq)]
pub struct Bar {
    pub label: String,
    pub text: String,
    pub fraction: f32,
}

/// One card of the window: a device and its bars. `id`, `title` and `detail`
/// never change between two samples, which is what lets the window update the
/// bars in place.
#[derive(Debug, Clone, PartialEq)]
pub struct Group {
    /// The device's identity: what it is switched off by, and what
    /// settings.ini's `HwMonitorHidden` lists. Not the title, which two cards
    /// of one model share, and made only of characters an INI value keeps
    /// (`ini_safe`): a GPU's UUID or PCI address, or one of the fixed
    /// `cpu` / `memory` / `disk`.
    pub id: String,
    pub title: String,
    pub detail: String,
    /// False for a device the user switched off: it keeps its place in the
    /// window, with no bars, and none of its sources was read.
    pub enabled: bool,
    pub bars: Vec<Bar>,
}

const CPU_ID: &str = "cpu";
const MEMORY_ID: &str = "memory";
/// The disk holding the models directory, whichever one that is.
const DISK_ID: &str = "disk";

/// An id reduced to what survives a comma-separated INI value: no separator,
/// no comment marker, no blank.
fn ini_safe(id: &str) -> String {
    id.chars()
        .map(|c| match c {
            c if c.is_ascii_alphanumeric() => c,
            '-' | '_' | '.' | ':' => c,
            _ => '_',
        })
        .collect()
}

/// A device's group: `bars` runs, and so its sources are read, only while the
/// device is not in `off`.
fn group(
    off: &HashSet<String>,
    id: &str,
    title: String,
    detail: String,
    bars: impl FnOnce() -> Vec<Bar>,
) -> Group {
    let enabled = !off.contains(id);
    Group {
        id: id.to_string(),
        bars: if enabled { bars() } else { Vec::new() },
        title,
        detail,
        enabled,
    }
}

fn percent_bar(label: &str, percent: f64) -> Bar {
    Bar {
        label: label.to_string(),
        text: format!("{percent:.0} %"),
        fraction: (percent / 100.0).clamp(0.0, 1.0) as f32,
    }
}

fn bytes_bar(label: &str, used: u64, total: u64) -> Bar {
    Bar {
        label: label.to_string(),
        text: format!("{:.1} / {:.1} GiB", used as f64 / GIB, total as f64 / GIB),
        fraction: if total == 0 {
            0.0
        } else {
            (used as f64 / total as f64).clamp(0.0, 1.0) as f32
        },
    }
}

fn rate_text(bytes_per_s: f64) -> String {
    if bytes_per_s >= GIB {
        format!("{:.2} GiB/s", bytes_per_s / GIB)
    } else {
        format!("{:.1} MiB/s", bytes_per_s / MIB)
    }
}

/// A rate bar against the running `peak`, which this call raises when needed.
fn rate_bar(label: &str, bytes_per_s: f64, peak: &mut f64) -> Bar {
    *peak = peak.max(bytes_per_s).max(RATE_FLOOR);
    Bar {
        label: label.to_string(),
        text: format!("{} · peak {}", rate_text(bytes_per_s), rate_text(*peak)),
        fraction: (bytes_per_s / *peak).clamp(0.0, 1.0) as f32,
    }
}

// ── NVIDIA (NVML) ───────────────────────────────────────────────────

type NvmlDevice = *mut c_void;

#[repr(C)]
#[derive(Default)]
struct NvmlMemory {
    total: u64,
    free: u64,
    used: u64,
}

#[repr(C)]
#[derive(Default)]
struct NvmlUtilization {
    gpu: c_uint,
    memory: c_uint,
}

/// `nvmlPciInfo_t`. Only Windows needs it: there the card's PCI address is
/// what pairs an NVML device with the adapter the system's counters describe.
#[cfg(windows)]
#[repr(C)]
struct NvmlPciInfo {
    bus_id_legacy: [c_char; 16],
    domain: c_uint,
    bus: c_uint,
    device: c_uint,
    pci_device_id: c_uint,
    pci_subsystem_id: c_uint,
    bus_id: [c_char; 32],
}

// nvmlPcieUtilCounter_t.
const NVML_PCIE_TX: c_int = 0;
const NVML_PCIE_RX: c_int = 1;

/// One card NVML answers for.
struct NvmlGpu {
    handle: NvmlDevice,
    name: String,
    /// The card's UUID, as `GPU-...`.
    id: String,
    /// PCI (bus, device), where NVML says.
    #[cfg(windows)]
    address: Option<(u32, u32)>,
}

/// The NVML entry points in use, plus the device handles, resolved once per
/// process. NVML is never shut down: the handles stay valid for as long as the
/// configurator runs, and initializing it holds no VRAM.
struct Nvml {
    devices: Vec<NvmlGpu>,
    memory: unsafe extern "C" fn(NvmlDevice, *mut NvmlMemory) -> c_int,
    utilization: unsafe extern "C" fn(NvmlDevice, *mut NvmlUtilization) -> c_int,
    /// Kilobytes per second, measured over 20 ms inside the call.
    pcie: unsafe extern "C" fn(NvmlDevice, c_int, *mut c_uint) -> c_int,
}

// SAFETY: NVML is documented thread-safe, and the device handles are opaque
// tokens it hands out for the lifetime of the library.
unsafe impl Send for Nvml {}
unsafe impl Sync for Nvml {}

/// Resolve `name` in `lib` as a function pointer of type `T`.
///
/// SAFETY: the caller guarantees `T` is the `extern "C" fn` type NVML's header
/// declares for `name`.
unsafe fn symbol<T: Copy>(lib: *mut c_void, name: &CStr) -> Option<T> {
    let p = os::library_symbol(lib, name);
    (!p.is_null()).then(|| std::mem::transmute_copy(&p))
}

fn nvml() -> Option<&'static Nvml> {
    static NVML: OnceLock<Option<Nvml>> = OnceLock::new();
    NVML.get_or_init(|| unsafe { load_nvml() }).as_ref()
}

unsafe fn load_nvml() -> Option<Nvml> {
    let lib = os::open_library(os::NVML_LIBRARY);
    if lib.is_null() {
        return None;
    }
    let init: unsafe extern "C" fn() -> c_int = symbol(lib, c"nvmlInit_v2")?;
    let count: unsafe extern "C" fn(*mut c_uint) -> c_int = symbol(lib, c"nvmlDeviceGetCount_v2")?;
    let handle: unsafe extern "C" fn(c_uint, *mut NvmlDevice) -> c_int =
        symbol(lib, c"nvmlDeviceGetHandleByIndex_v2")?;
    let name: unsafe extern "C" fn(NvmlDevice, *mut c_char, c_uint) -> c_int =
        symbol(lib, c"nvmlDeviceGetName")?;
    let uuid: Option<unsafe extern "C" fn(NvmlDevice, *mut c_char, c_uint) -> c_int> =
        symbol(lib, c"nvmlDeviceGetUUID");
    #[cfg(windows)]
    let pci: Option<unsafe extern "C" fn(NvmlDevice, *mut NvmlPciInfo) -> c_int> =
        symbol(lib, c"nvmlDeviceGetPciInfo_v3");
    let memory = symbol(lib, c"nvmlDeviceGetMemoryInfo")?;
    let utilization = symbol(lib, c"nvmlDeviceGetUtilizationRates")?;
    let pcie = symbol(lib, c"nvmlDeviceGetPcieThroughput")?;

    let mut n: c_uint = 0;
    if init() != 0 || count(&mut n) != 0 {
        return None;
    }
    let mut devices = Vec::new();
    for i in 0..n {
        let mut dev: NvmlDevice = std::ptr::null_mut();
        if handle(i, &mut dev) != 0 {
            continue;
        }
        let mut buf = [0 as c_char; 96];
        let title = if name(dev, buf.as_mut_ptr(), buf.len() as c_uint) == 0 {
            CStr::from_ptr(buf.as_ptr()).to_string_lossy().into_owned()
        } else {
            format!("NVIDIA GPU {i}")
        };
        // The UUID stays with the card when another is added or removed; the
        // index does not.
        let mut buf = [0 as c_char; 96];
        let id = match uuid {
            Some(uuid) if uuid(dev, buf.as_mut_ptr(), buf.len() as c_uint) == 0 => {
                CStr::from_ptr(buf.as_ptr()).to_string_lossy().into_owned()
            }
            _ => format!("nvidia-{i}"),
        };
        #[cfg(windows)]
        let address = pci.and_then(|pci| {
            let mut info: NvmlPciInfo = std::mem::zeroed();
            (pci(dev, &mut info) == 0).then_some((info.bus, info.device))
        });
        devices.push(NvmlGpu {
            handle: dev,
            name: title,
            id: ini_safe(&id),
            #[cfg(windows)]
            address,
        });
    }
    Some(Nvml {
        devices,
        memory,
        utilization,
        pcie,
    })
}

fn nvidia_bars(nvml: &Nvml, dev: NvmlDevice, peak: &mut f64) -> Vec<Bar> {
    let mut bars = Vec::new();
    let mut util = NvmlUtilization::default();
    // SAFETY: `dev` came from this library, and each out-pointer is a live
    // local of the struct type the call fills.
    let has_util = unsafe { (nvml.utilization)(dev, &mut util) } == 0;
    if has_util {
        bars.push(percent_bar("GPU", util.gpu as f64));
    }
    let mut mem = NvmlMemory::default();
    if unsafe { (nvml.memory)(dev, &mut mem) } == 0 {
        bars.push(bytes_bar("VRAM", mem.used, mem.total));
    }
    if has_util {
        bars.push(percent_bar("Memory activity", util.memory as f64));
    }
    let throughput = |counter: c_int| {
        let mut kb: c_uint = 0;
        (unsafe { (nvml.pcie)(dev, counter, &mut kb) } == 0).then_some(kb as f64 * 1024.0)
    };
    if let (Some(rx), Some(tx)) = (throughput(NVML_PCIE_RX), throughput(NVML_PCIE_TX)) {
        *peak = peak.max(rx).max(tx);
        bars.push(rate_bar("PCIe RX", rx, peak));
        bars.push(rate_bar("PCIe TX", tx, peak));
    }
    bars
}

// What both systems share. The tests of each system's own sources sit in its
// module.
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rate_bars_scale_to_the_peak_and_never_below_the_floor() {
        let mut peak = 0.0;
        // An idle link is an empty bar against the floor, not a full one.
        let idle = rate_bar("Read", 0.0, &mut peak);
        assert_eq!(idle.fraction, 0.0);
        assert_eq!(peak, RATE_FLOOR);
        let burst = rate_bar("Read", 64.0 * MIB, &mut peak);
        assert_eq!(burst.fraction, 1.0);
        let after = rate_bar("Read", 16.0 * MIB, &mut peak);
        assert_eq!(after.fraction, 0.25);
        assert_eq!(after.text, "16.0 MiB/s · peak 64.0 MiB/s");
    }

    #[test]
    fn a_device_switched_off_is_not_read_and_keeps_its_place() {
        let off = HashSet::from(["amdgpu-0000:12:00.0".to_string()]);
        let read = std::cell::Cell::new(false);
        let bars = || {
            read.set(true);
            vec![percent_bar("GPU", 50.0)]
        };
        // Same title, another card: the id is what tells them apart.
        let other = group(
            &off,
            "amdgpu-0000:7b:00.0",
            "Card".into(),
            String::new(),
            bars,
        );
        assert!(other.enabled && other.bars.len() == 1 && read.get());

        read.set(false);
        let g = group(
            &off,
            "amdgpu-0000:12:00.0",
            "Card".into(),
            String::new(),
            bars,
        );
        assert_eq!((g.enabled, g.bars.len()), (false, 0));
        assert_eq!(g.title, "Card", "it keeps its heading, and so its checkbox");
        assert!(
            !read.get(),
            "a switched-off device's sources must not be touched"
        );
    }

    // An id is written into a comma-separated INI value, where `;` and `#`
    // start a comment: none of those may survive in one.
    #[test]
    fn ids_keep_only_what_an_ini_list_keeps() {
        assert_eq!(ini_safe("amdgpu-0000:12:00.0"), "amdgpu-0000:12:00.0");
        assert_eq!(ini_safe("GPU-5a3b_c1.d"), "GPU-5a3b_c1.d");
        assert_eq!(ini_safe("a,b;c#d e=f"), "a_b_c_d_e_f");
    }
}
