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
//! | CPU, I/O wait | `/proc/stat`, first line |
//! | RAM, Page cache, Swap | `/proc/meminfo` |
//! | Disk read, write, busy, I/O wait | `/sys/block/<disk>/stat` of the disk holding the models directory |
//!
//! On Windows:
//!
//! | Bar | Source |
//! |-----|--------|
//! | GPU, VRAM, Shared memory (every adapter) | The performance counters Task Manager reads, through PDH: `\GPU Engine(*)\Utilization Percentage`, `\GPU Adapter Memory(*)\Dedicated Usage` and `Shared Usage`. The adapters, their names and both totals come from D3DKMT (`gdi32`) |
//! | GPU, VRAM, Memory activity, PCIe RX/TX (NVIDIA) | NVML (`nvml.dll`, loaded at runtime from System32), with Shared memory from PDH as above |
//! | CPU | `GetSystemTimes` |
//! | I/O wait (est.) | PDH: `\PhysicalDisk(*)\Avg. Disk Queue Length`, an estimate |
//! | RAM, Committed | `GetPerformanceInfo` |
//! | Cached | PDH: the standby lists plus the modified list, which is Task Manager's "Cached" |
//! | Disk read, write, busy, I/O wait | `IOCTL_DISK_PERFORMANCE` on the physical drive holding the models directory |
//!
//! Things that are not obvious from the tables:
//!
//! - **I/O wait means two things.** The CPU card's is the share of processor
//!   time spent idle with I/O pending. Only Linux accounts it: the Windows kernel
//!   counts that time as plain idle, so there the bar is an estimate and says so
//!   (`\PhysicalDisk(*)\Avg. Disk Queue Length` over the threads, within the idle
//!   time; `windows.rs` `estimated_iowait`). The disk card's, on both, is the
//!   average time one I/O of that disk took.
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
//! What does not change while the machine runs (a GPU's facts, the SDKs, the
//! processor, the memory modules) is not sampled: `publish_gpus` reads it once at
//! the GUI's startup and on a refresh, and a sample only looks a card up by its
//! address. `sdk.rs` says which `--device` ids a card is.
//!
//! NVML is loaded at runtime, not linked: a machine without the NVIDIA
//! driver simply has no NVIDIA readings. `nvmlDeviceGetPcieThroughput`
//! measures over 20 ms per call, which is why the sampler runs on a worker
//! thread and not on a UI timer.

use std::collections::HashSet;
use std::ffi::{c_char, c_int, c_uint, c_void, CStr};
use std::sync::OnceLock;

pub mod sdk;

#[cfg(target_os = "linux")]
mod linux;
#[cfg(target_os = "linux")]
use linux as os;
#[cfg(windows)]
mod smbios;
#[cfg(windows)]
mod windows;
#[cfg(windows)]
use windows as os;

#[cfg(windows)]
pub use os::adapters_free;
pub use os::Sampler;

/// Total and available physical memory in bytes, from the same source the
/// RAM bar reads (`GetPerformanceInfo` / `/proc/meminfo`): one cheap call, no
/// sampler. `None` when the source fails. The Hugging Face placement estimate
/// (`hf_fit`) is its other reader.
pub fn system_ram() -> Option<(u64, u64)> {
    os::system_ram()
}

const GIB: f64 = (1u64 << 30) as f64;
const MIB: f64 = (1u64 << 20) as f64;
/// Lowest full-scale value of a rate bar, in bytes per second.
const RATE_FLOOR: f64 = 16.0 * MIB;
/// Lowest full-scale value of the disk's wait bar, in milliseconds per I/O.
const WAIT_FLOOR_MS: f64 = 1.0;

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
    /// What is known of a GPU; `None` for any other device, and for a GPU
    /// without a PCI address.
    pub gpu: Option<crate::devices::PhysicalGpu>,
    /// False for a device the user switched off: it keeps its place in the
    /// window, with no bars, and none of its sources was read.
    pub enabled: bool,
    pub bars: Vec<Bar>,
}

impl Group {
    fn of_gpu(mut self, gpu: Option<crate::devices::PhysicalGpu>) -> Self {
        self.gpu = gpu;
        self
    }

    /// The published GPU at this address, if the startup has got that far.
    fn at_pci(self, pci: Option<&str>) -> Self {
        let gpu = pci.and_then(|pci| {
            crate::devices::physical().into_iter().find(|gpu| gpu.pci == pci)
        });
        self.of_gpu(gpu)
    }
}

/// The probe's devices of one model, matched by name alone: one source may say
/// `Radeon AI PRO R9700` for another's `AMD Radeon AI PRO R9700`.
fn of_model<'a>(
    name: &str,
    devs: &'a [crate::devices::DeviceOption],
) -> impl Iterator<Item = &'a crate::devices::DeviceOption> {
    let wanted = name.to_ascii_lowercase();
    devs.iter().filter(move |d| {
        let name = d.name.to_ascii_lowercase();
        !d.is_cpu() && !name.is_empty() && (name.contains(&wanted) || wanted.contains(&name))
    })
}

/// Read what is known of the machine's GPUs and publish it for the session: the
/// system's facts, and from the SDKs which `--device` ids each card is. On the
/// GUI's probe thread, at startup and on a refresh; `devs` is that probe.
pub fn publish_gpus(devs: &[crate::devices::DeviceOption]) {
    let (addresses, sdks) = sdk::ask(devs);
    crate::devices::set_sdks(sdks);
    let mut gpus = os::physical_gpus();
    for gpu in &mut gpus {
        gpu.sdk_ids = devs
            .iter()
            .filter(|d| addresses.get(&d.id) == Some(&gpu.pci))
            .map(|d| d.id.clone())
            .collect();
        if gpu.vram_mib == 0 {
            gpu.vram_mib = of_model(&gpu.name, devs).map(|d| d.total_mib).max().unwrap_or(0);
        }
    }
    crate::devices::set_physical(gpus);
    crate::devices::set_machine(os::cpu_facts(), os::memory_modules());
}

/// The vector extensions the processor reports, of those inference uses.
fn cpu_instructions() -> Vec<&'static str> {
    let mut found = Vec::new();
    #[cfg(target_arch = "x86_64")]
    {
        macro_rules! check {
            ($($feature:tt => $name:literal),* $(,)?) => {
                $(if std::is_x86_feature_detected!($feature) { found.push($name); })*
            };
        }
        check!(
            "avx2" => "AVX2",
            "fma" => "FMA",
            "avx512f" => "AVX-512",
            "avx512vnni" => "AVX-512 VNNI",
            "avx512bf16" => "AVX-512 BF16",
        );
    }
    found
}

/// A socket's device id, counted from 0: the first keeps the plain id, so a
/// setting saved with it holds whatever the machine.
fn cpu_id(socket: usize) -> String {
    if socket == 0 {
        CPU_ID.to_string()
    } else {
        format!("{CPU_ID}-{}", socket + 1)
    }
}

/// Whether a device id is a socket's.
pub fn is_cpu_id(id: &str) -> bool {
    id == CPU_ID || id.strip_prefix("cpu-").is_some_and(|n| n.parse::<usize>().is_ok())
}

/// A socket's row in the CPU box: `Socket 1`.
fn cpu_detail(socket: usize) -> String {
    crate::strings::fill("hw-cpu-socket-number", &[&(socket + 1)])
}

/// The memory card's title: its modules' names, the count on each. "Memory"
/// when the firmware names none.
fn memory_title(modules: &[crate::devices::MemoryModule]) -> String {
    let mut models: Vec<(String, usize)> = Vec::new();
    for model in modules.iter().map(|m| m.model()).filter(|m| !m.is_empty()) {
        match models.iter_mut().find(|(name, _)| *name == model) {
            Some((_, count)) => *count += 1,
            None => models.push((model, 1)),
        }
    }
    if models.is_empty() {
        return "Memory".to_string();
    }
    models
        .iter()
        .map(|(name, count)| format!("{count} × {name}"))
        .collect::<Vec<_>>()
        .join(" + ")
}

/// Say which `--device` ids each GPU is. A card's own ids head its detail, the
/// SDKs joined (`ROCm1/Vulkan1`). An id no card claims (its SDK could not be
/// asked) goes in the title of its model: it belongs to a model, and twins keep
/// one title, so one box. The ids are the probe's, which lands after the window
/// opens and comes again on a refresh, so they are applied to every sample
/// rather than kept in the sampler.
pub fn name_sdk_ids(groups: &mut [Group], devs: &[crate::devices::DeviceOption]) {
    let claimed: HashSet<String> = groups
        .iter()
        .filter_map(|g| g.gpu.as_ref())
        .flat_map(|gpu| gpu.sdk_ids.iter().cloned())
        .collect();
    for g in groups.iter_mut() {
        let Some(gpu) = &g.gpu else {
            continue;
        };
        if !gpu.sdk_ids.is_empty() {
            g.detail = format!("{} · {}", gpu.sdk_ids.join("/"), g.detail);
        }
        let unclaimed: Vec<&str> = of_model(&g.title, devs)
            .filter(|d| !claimed.contains(&d.id))
            .map(|d| d.id.as_str())
            .collect();
        for id in unclaimed {
            g.title = format!("{} · {id}", g.title);
        }
    }
}

/// The memory card's detail: the RAM installed. Empty when it cannot be read.
fn memory_detail() -> String {
    os::system_ram().map_or(String::new(), |(total, _)| {
        format!("{:.1} GiB", total as f64 / GIB)
    })
}

const CPU_ID: &str = "cpu";
pub const MEMORY_ID: &str = "memory";
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
        gpu: None,
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

/// The time an I/O of the interval took on average, against the running `peak`.
/// `wait_ms` is the time the interval's `ops` spent queued and in service.
fn wait_bar(wait_ms: f64, ops: u64, peak: &mut f64) -> Bar {
    let ms = if ops == 0 { 0.0 } else { wait_ms / ops as f64 };
    *peak = peak.max(ms).max(WAIT_FLOOR_MS);
    Bar {
        label: "I/O wait".to_string(),
        text: format!("{ms:.2} ms per I/O · peak {peak:.2} ms"),
        fraction: (ms / *peak).clamp(0.0, 1.0) as f32,
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

/// `nvmlPciInfo_t`: the card's PCI address is what pairs an NVML device with
/// the ids of the device probe, and on Windows with the adapter the system's
/// counters describe.
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
        let address = pci.and_then(|pci| {
            let mut info: NvmlPciInfo = std::mem::zeroed();
            (pci(dev, &mut info) == 0).then_some((info.bus, info.device))
        });
        devices.push(NvmlGpu {
            handle: dev,
            name: title,
            id: ini_safe(&id),
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
    fn the_memory_title_names_each_kind_of_module() {
        let module = |part: &str| crate::devices::MemoryModule {
            maker: "CORSAIR".to_string(),
            part: part.to_string(),
            ..Default::default()
        };
        assert_eq!(memory_title(&[]), "Memory");
        assert_eq!(
            memory_title(&[module("A"), module("A")]),
            "2 × CORSAIR A"
        );
        // Mixed modules, in slot order; one the firmware does not name is skipped.
        assert_eq!(
            memory_title(&[module("A"), module("B"), module("A"), Default::default()]),
            "2 × CORSAIR A + 1 × CORSAIR B"
        );
    }

    #[test]
    fn a_socket_s_id_keeps_the_plain_one_first() {
        assert_eq!((cpu_id(0).as_str(), cpu_id(1).as_str()), ("cpu", "cpu-2"));
        assert!(is_cpu_id("cpu") && is_cpu_id("cpu-2"));
        assert!(!is_cpu_id("cpu-x") && !is_cpu_id("memory"));
    }

    #[test]
    fn the_wait_bar_is_time_per_io_against_its_peak() {
        let mut peak = 0.0;
        // Under the floor the scale is the floor.
        let bar = wait_bar(50.0, 100, &mut peak);
        assert_eq!(bar.text, "0.50 ms per I/O · peak 1.00 ms");
        assert_eq!(bar.fraction, 0.5);
        // No I/O in the interval is no wait, not a division by zero.
        assert_eq!(wait_bar(0.0, 0, &mut peak).text, "0.00 ms per I/O · peak 1.00 ms");
        assert_eq!(wait_bar(400.0, 100, &mut peak).fraction, 1.0);
    }

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

    #[test]
    fn a_card_is_headed_by_its_own_ids_and_the_unclaimed_stay_at_the_model() {
        let card = |title: &str, ids: &[&str]| {
            let gpu = crate::devices::PhysicalGpu {
                pci: "x".into(),
                sdk_ids: ids.iter().map(|id| id.to_string()).collect(),
                ..Default::default()
            };
            group(&HashSet::new(), "id", title.into(), "AMD".into(), Vec::new).of_gpu(Some(gpu))
        };
        let devs = crate::devices::parse(
            "  ROCm0: AMD Radeon(TM) Graphics (37078 MiB, 36926 MiB free)\n  \
             ROCm1: AMD Radeon AI PRO R9700 (32624 MiB, 32462 MiB free)\n  \
             ROCm2: AMD Radeon AI PRO R9700 (32624 MiB, 32462 MiB free)\n  \
             Vulkan1: AMD Radeon AI PRO R9700 (32624 MiB, 31707 MiB free)\n  \
             CPU: AMD Radeon AI PRO R9700 (1 MiB, 1 MiB free)\n",
        );
        // No SDK could be asked: the ids stay at the model's level, twins share a
        // title, and the name matches with or without the vendor (pci.ids).
        let mut groups = [
            card("AMD Radeon AI PRO R9700", &[]),
            card("AMD Radeon AI PRO R9700", &[]),
            card("Radeon AI PRO R9700", &[]),
            card("Another Card", &[]),
            group(&HashSet::new(), "m", "AMD Radeon(TM) Graphics".into(), "AMD".into(), Vec::new),
        ];
        name_sdk_ids(&mut groups, &devs);
        let twins = "AMD Radeon AI PRO R9700 · ROCm1 · ROCm2 · Vulkan1";
        assert_eq!((groups[0].title.as_str(), groups[1].title.as_str()), (twins, twins));
        assert_eq!(groups[2].title, "Radeon AI PRO R9700 · ROCm1 · ROCm2 · Vulkan1");
        assert_eq!(groups[3].title, "Another Card", "not a probed model");
        assert_eq!(groups[4].title, "AMD Radeon(TM) Graphics", "not a GPU");
        assert_eq!(groups[0].detail, "AMD");

        // Asked: each card gets its own ids, joined across the SDKs; an id only
        // one SDK could not place stays at the model's level.
        let mut twins = [
            card("AMD Radeon AI PRO R9700", &["ROCm1", "Vulkan1"]),
            card("AMD Radeon AI PRO R9700", &["ROCm2"]),
        ];
        name_sdk_ids(&mut twins, &devs);
        assert_eq!(twins[0].detail, "ROCm1/Vulkan1 · AMD");
        assert_eq!(twins[1].detail, "ROCm2 · AMD");
        assert_eq!(twins[0].title, "AMD Radeon AI PRO R9700");
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
