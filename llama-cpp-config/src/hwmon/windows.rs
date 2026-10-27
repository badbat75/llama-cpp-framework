//! The Windows sources of the HW Monitor: D3DKMT and the GPU performance
//! counters for the adapters, `GetSystemTimes` and `GetPerformanceInfo` for
//! the CPU and memory, `IOCTL_DISK_PERFORMANCE` for the disk. The table of
//! what each bar reads is in the parent module's header.
//!
//! Raw Win32 FFI, no extra crates (as `single_instance.rs`). What decided the
//! sources:
//!
//! - **The GPU counters are the ones Task Manager shows**, for every vendor,
//!   which is why they are the base and NVML only an addition: no AMD or
//!   Intel library is needed, and Shared memory (where an oversubscribed card
//!   silently spills on Windows) exists nowhere else. They are added by their
//!   ENGLISH names (`PdhAddEnglishCounterW`): a counter path is localized,
//!   and this must work on an Italian Windows too.
//! - **A counter instance names its adapter by LUID** (`luid_0x..._0x...`),
//!   which is also what D3DKMT enumerates adapters by; the LUID changes at
//!   every boot, so a device's id is its PCI address instead.
//! - **An adapter has one GPU bar but many engines** (3D, Compute, Copy,
//!   Video Decode...), each with one counter instance per process. The bar is
//!   the busiest engine, each engine being the sum of its processes: the
//!   figure Task Manager heads the adapter's page with.
//! - **An NVIDIA card is paired with its NVML device by PCI address**, then
//!   reads GPU, VRAM, Memory activity and PCIe from NVML (as on Linux) and
//!   only Shared memory from the counters.
//! - **The disk is opened with no access right at all**, which is what lets a
//!   standard user open `\\.\PhysicalDriveN`; the statistics and identity
//!   IOCTLs ask for none.

use super::*;
use std::collections::HashMap;
use std::os::windows::ffi::OsStrExt;
use std::path::Path;

type Handle = *mut c_void;

fn wide(s: &str) -> Vec<u16> {
    s.encode_utf16().chain(std::iter::once(0)).collect()
}

fn from_wide(s: &[u16]) -> String {
    let end = s.iter().position(|c| *c == 0).unwrap_or(s.len());
    String::from_utf16_lossy(&s[..end])
}

// ── Runtime library loading (NVML) ──────────────────────────────────

/// `PERFORMANCE_INFORMATION`: every size is in pages.
#[repr(C)]
#[derive(Default)]
struct PerformanceInformation {
    cb: u32,
    commit_total: usize,
    commit_limit: usize,
    commit_peak: usize,
    physical_total: usize,
    physical_available: usize,
    system_cache: usize,
    kernel_total: usize,
    kernel_paged: usize,
    kernel_nonpaged: usize,
    page_size: usize,
    handle_count: u32,
    process_count: u32,
    thread_count: u32,
}

#[link(name = "kernel32")]
extern "system" {
    fn LoadLibraryExA(name: *const c_char, file: Handle, flags: u32) -> *mut c_void;
    // Declared as in `gguf\ffi.rs`: one symbol, one signature.
    fn GetProcAddress(module: *mut c_void, name: *const u8) -> *const c_void;
    // Each out-parameter is a FILETIME, which is a u64 in two halves.
    fn GetSystemTimes(idle: *mut u64, kernel: *mut u64, user: *mut u64) -> i32;
    fn K32GetPerformanceInfo(info: *mut PerformanceInformation, cb: u32) -> i32;
    fn GetVolumePathNameW(path: *const u16, volume: *mut u16, len: u32) -> i32;
    fn GetVolumeNameForVolumeMountPointW(mount: *const u16, name: *mut u16, len: u32) -> i32;
    fn CreateFileW(
        name: *const u16,
        access: u32,
        share: u32,
        security: *const c_void,
        disposition: u32,
        flags: u32,
        template: Handle,
    ) -> Handle;
    fn DeviceIoControl(
        device: Handle,
        code: u32,
        input: *const c_void,
        input_len: u32,
        output: *mut c_void,
        output_len: u32,
        returned: *mut u32,
        overlapped: *mut c_void,
    ) -> i32;
    fn CloseHandle(handle: Handle) -> i32;
}

const LOAD_LIBRARY_SEARCH_SYSTEM32: u32 = 0x800;

pub(super) const NVML_LIBRARY: &CStr = c"nvml.dll";

/// From System32 only, where the driver installs it: a bare name would look
/// beside the exe and through the PATH first.
pub(super) unsafe fn open_library(name: &CStr) -> *mut c_void {
    LoadLibraryExA(
        name.as_ptr(),
        std::ptr::null_mut(),
        LOAD_LIBRARY_SEARCH_SYSTEM32,
    )
}

pub(super) unsafe fn library_symbol(lib: *mut c_void, name: &CStr) -> *mut c_void {
    GetProcAddress(lib, name.as_ptr().cast()).cast_mut()
}

// ── Performance counters (PDH) ──────────────────────────────────────

/// `PDH_FMT_COUNTERVALUE` formatted as a double.
#[repr(C)]
struct CounterValue {
    status: u32,
    value: f64,
}

/// `PDH_FMT_COUNTERVALUE_ITEM_W`: one instance of a wildcard counter.
#[repr(C)]
struct CounterItem {
    name: *const u16,
    value: CounterValue,
}

#[link(name = "pdh")]
extern "system" {
    fn PdhOpenQueryW(source: *const u16, user: usize, query: *mut Handle) -> u32;
    fn PdhAddEnglishCounterW(
        query: Handle,
        path: *const u16,
        user: usize,
        counter: *mut Handle,
    ) -> u32;
    fn PdhCollectQueryData(query: Handle) -> u32;
    fn PdhGetFormattedCounterValue(
        counter: Handle,
        format: u32,
        kind: *mut u32,
        value: *mut CounterValue,
    ) -> u32;
    fn PdhGetFormattedCounterArrayW(
        counter: Handle,
        format: u32,
        size: *mut u32,
        count: *mut u32,
        items: *mut CounterItem,
    ) -> u32;
    fn PdhCloseQuery(query: Handle) -> u32;
}

const PDH_FMT_DOUBLE: u32 = 0x200;
const PDH_MORE_DATA: u32 = 0x8000_07D2;
/// `PDH_CSTATUS_VALID_DATA` is 0 and `PDH_CSTATUS_NEW_DATA` is 1; anything
/// above is a value that cannot be used (a rate with one reading, for one).
const PDH_CSTATUS_NEW_DATA: u32 = 1;

/// A PDH query: the counters added to it are read together by `collect` and
/// die with it.
struct Query(Handle);

impl Query {
    fn open() -> Option<Query> {
        let mut query: Handle = std::ptr::null_mut();
        // SAFETY: the out-pointer is a live local.
        (unsafe { PdhOpenQueryW(std::ptr::null(), 0, &mut query) } == 0).then_some(Query(query))
    }

    fn add(&self, path: &str) -> Option<Handle> {
        let mut counter: Handle = std::ptr::null_mut();
        // SAFETY: `path` is NUL-terminated and outlives the call.
        let status = unsafe { PdhAddEnglishCounterW(self.0, wide(path).as_ptr(), 0, &mut counter) };
        (status == 0).then_some(counter)
    }

    fn collect(&self) -> bool {
        unsafe { PdhCollectQueryData(self.0) == 0 }
    }
}

impl Drop for Query {
    fn drop(&mut self) {
        unsafe { PdhCloseQuery(self.0) };
    }
}

/// The value of a single-instance counter, as of the last `collect`.
fn counter_value(counter: Handle) -> Option<f64> {
    let mut value = CounterValue {
        status: 0,
        value: 0.0,
    };
    // SAFETY: `counter` belongs to a live query; the out-pointer is a local.
    let status = unsafe {
        PdhGetFormattedCounterValue(counter, PDH_FMT_DOUBLE, std::ptr::null_mut(), &mut value)
    };
    (status == 0 && value.status <= PDH_CSTATUS_NEW_DATA).then_some(value.value)
}

/// Every instance of a wildcard counter as `(instance name, value)`, as of
/// the last `collect`. `None` when the counter cannot be read at all; an
/// instance without a usable value is left out.
fn counter_values(counter: Handle) -> Option<Vec<(String, f64)>> {
    // The instance list can grow between the sizing call and the read.
    for _ in 0..3 {
        let (mut size, mut count) = (0u32, 0u32);
        // SAFETY: a null buffer with size 0 is the documented sizing call.
        let status = unsafe {
            PdhGetFormattedCounterArrayW(
                counter,
                PDH_FMT_DOUBLE,
                &mut size,
                &mut count,
                std::ptr::null_mut(),
            )
        };
        if status != PDH_MORE_DATA {
            return None;
        }
        // The items come first and the names they point at behind them, in
        // the same buffer; u64 storage keeps the items aligned.
        let mut buf = vec![0u64; (size as usize).div_ceil(8)];
        let items = buf.as_mut_ptr() as *mut CounterItem;
        // SAFETY: `buf` holds at least the `size` bytes the sizing call asked for.
        let status = unsafe {
            PdhGetFormattedCounterArrayW(counter, PDH_FMT_DOUBLE, &mut size, &mut count, items)
        };
        if status == PDH_MORE_DATA {
            continue;
        }
        if status != 0 {
            return None;
        }
        // SAFETY: PDH wrote `count` items at the head of `buf`, each naming
        // a NUL-terminated string inside it.
        let items = unsafe { std::slice::from_raw_parts(items, count as usize) };
        return Some(
            items
                .iter()
                .filter(|item| item.value.status <= PDH_CSTATUS_NEW_DATA)
                .map(|item| {
                    let len = (0..)
                        .take_while(|i| unsafe { *item.name.add(*i) } != 0)
                        .count();
                    let name = unsafe { std::slice::from_raw_parts(item.name, len) };
                    (String::from_utf16_lossy(name), item.value.value)
                })
                .collect(),
        );
    }
    None
}

/// The adapter a counter instance belongs to: `luid_0x00000000_0x00016D04`
/// inside its name, as one number.
fn instance_luid(name: &str) -> Option<u64> {
    let rest = &name[name.find("luid_0x")? + 7..];
    let high = u32::from_str_radix(rest.get(..8)?, 16).ok()?;
    let low = u32::from_str_radix(rest.get(8..)?.strip_prefix("_0x")?.get(..8)?, 16).ok()?;
    Some(((high as u64) << 32) | low as u64)
}

/// The engine index of a `GPU Engine` instance: the `3` of
/// `pid_1234_luid_..._phys_0_eng_3_engtype_Compute 0`.
fn instance_engine(name: &str) -> Option<u32> {
    let rest = &name[name.find("_eng_")? + 5..];
    rest[..rest.find('_').unwrap_or(rest.len())].parse().ok()
}

/// Each adapter's utilization out of the `GPU Engine` instances, one per
/// process and engine: an engine is the sum of its processes, an adapter its
/// busiest engine.
fn busiest_engines<'a>(samples: impl IntoIterator<Item = (&'a str, f64)>) -> HashMap<u64, f64> {
    let mut engines: HashMap<(u64, u32), f64> = HashMap::new();
    for (name, percent) in samples {
        if let (Some(luid), Some(engine)) = (instance_luid(name), instance_engine(name)) {
            *engines.entry((luid, engine)).or_default() += percent;
        }
    }
    let mut busy: HashMap<u64, f64> = HashMap::new();
    for ((luid, _), percent) in engines {
        let top = busy.entry(luid).or_default();
        *top = top.max(percent.min(100.0));
    }
    busy
}

/// A per-adapter counter (`GPU Adapter Memory`) summed by adapter, in bytes.
fn by_adapter(samples: &[(String, f64)]) -> HashMap<u64, u64> {
    let mut sums: HashMap<u64, u64> = HashMap::new();
    for (name, value) in samples {
        if let Some(luid) = instance_luid(name) {
            *sums.entry(luid).or_default() += value.max(0.0) as u64;
        }
    }
    sums
}

/// What the counters say of every adapter at one sample, by LUID. A half
/// left `None` was not read: its counters are missing, no adapter that needs
/// it is on, or (utilization only) it has no baseline yet.
#[derive(Default)]
struct GpuUsage {
    busy: Option<HashMap<u64, f64>>,
    dedicated: Option<HashMap<u64, u64>>,
    shared: Option<HashMap<u64, u64>>,
}

/// The two GPU queries. They are separate because utilization is the costly
/// one (an instance per process and engine, several hundred of them) and an
/// NVIDIA card read through NVML does not need it.
struct GpuCounters {
    /// Query, `Dedicated Usage`, `Shared Usage`.
    memory: Option<(Query, Handle, Handle)>,
    /// Query, `Utilization Percentage`.
    engines: Option<(Query, Handle)>,
    /// Whether the previous sample collected `engines`. Utilization is a rate
    /// between two collections: after a gap the first one is only a baseline,
    /// or the bar would average over the time the adapter was switched off.
    engines_primed: bool,
}

impl GpuCounters {
    fn open() -> Self {
        let memory = Query::open().and_then(|q| {
            let dedicated = q.add(r"\GPU Adapter Memory(*)\Dedicated Usage")?;
            let shared = q.add(r"\GPU Adapter Memory(*)\Shared Usage")?;
            Some((q, dedicated, shared))
        });
        let engines = Query::open().and_then(|q| {
            let busy = q.add(r"\GPU Engine(*)\Utilization Percentage")?;
            Some((q, busy))
        });
        GpuCounters {
            memory,
            engines,
            engines_primed: false,
        }
    }

    fn read(&mut self, adapters: &[Adapter], off: &HashSet<String>) -> GpuUsage {
        let on = |a: &&Adapter| !off.contains(&a.id);
        let mut usage = GpuUsage::default();
        if adapters.iter().any(|a| on(&a)) {
            if let Some((query, dedicated, shared)) = &self.memory {
                if query.collect() {
                    usage.dedicated = counter_values(*dedicated).map(|v| by_adapter(&v));
                    usage.shared = counter_values(*shared).map(|v| by_adapter(&v));
                }
            }
        }
        let wanted = adapters.iter().filter(on).any(|a| a.nvml.is_none());
        let collected = wanted && self.engines.as_ref().is_some_and(|(q, _)| q.collect());
        if collected && self.engines_primed {
            if let Some((_, busy)) = &self.engines {
                usage.busy = counter_values(*busy)
                    .map(|v| busiest_engines(v.iter().map(|(name, p)| (name.as_str(), *p))));
            }
        }
        self.engines_primed = collected;
        usage
    }
}

/// Task Manager's "Cached": the standby lists plus the modified list.
struct CachedCounters {
    query: Query,
    lists: Vec<Handle>,
}

impl CachedCounters {
    fn open() -> Option<Self> {
        let query = Query::open()?;
        let lists = [
            r"\Memory\Standby Cache Normal Priority Bytes",
            r"\Memory\Standby Cache Reserve Bytes",
            r"\Memory\Standby Cache Core Bytes",
            r"\Memory\Modified Page List Bytes",
        ]
        .iter()
        .map(|path| query.add(path))
        .collect::<Option<Vec<Handle>>>()?;
        Some(CachedCounters { query, lists })
    }

    fn read(&self) -> Option<u64> {
        if !self.query.collect() {
            return None;
        }
        let mut bytes = 0.0;
        for list in &self.lists {
            bytes += counter_value(*list)?;
        }
        Some(bytes as u64)
    }
}

// ── Adapters (D3DKMT) ───────────────────────────────────────────────

#[repr(C)]
#[derive(Default, Clone, Copy)]
struct Luid {
    low: u32,
    high: i32,
}

/// `D3DKMT_ADAPTERINFO`.
#[repr(C)]
#[derive(Default, Clone, Copy)]
struct AdapterInfo {
    handle: u32,
    luid: Luid,
    sources: u32,
    precise_present_regions: i32,
}

/// `D3DKMT_ENUMADAPTERS2`.
#[repr(C)]
struct EnumAdapters {
    count: u32,
    adapters: *mut AdapterInfo,
}

/// `D3DKMT_QUERYADAPTERINFO`.
#[repr(C)]
struct QueryAdapterInfo {
    handle: u32,
    kind: u32,
    data: *mut c_void,
    size: u32,
}

/// `D3DKMT_CLOSEADAPTER`.
#[repr(C)]
struct CloseAdapter {
    handle: u32,
}

/// `D3DKMT_SEGMENTSIZEINFO`: the totals behind Task Manager's "Dedicated GPU
/// memory" and "Shared GPU memory".
#[repr(C)]
#[derive(Default)]
struct SegmentSize {
    dedicated_video: u64,
    dedicated_system: u64,
    shared_system: u64,
}

/// `D3DKMT_ADAPTERADDRESS`.
#[repr(C)]
#[derive(Default)]
struct AdapterAddress {
    bus: u32,
    device: u32,
    function: u32,
}

/// `D3DKMT_ADAPTERREGISTRYINFO`: four `MAX_PATH` strings.
#[repr(C)]
struct RegistryInfo {
    adapter: [u16; 260],
    bios: [u16; 260],
    dac: [u16; 260],
    chip: [u16; 260],
}

/// `D3DKMT_QUERY_DEVICE_IDS`.
#[repr(C)]
#[derive(Default)]
struct DeviceIds {
    physical_adapter: u32,
    vendor: u32,
    device: u32,
    sub_vendor: u32,
    sub_system: u32,
    revision: u32,
    bus_type: u32,
}

// KMTQUERYADAPTERINFOTYPE.
const KMTQAITYPE_GETSEGMENTSIZE: u32 = 3;
const KMTQAITYPE_ADAPTERADDRESS: u32 = 6;
const KMTQAITYPE_ADAPTERREGISTRYINFO: u32 = 8;
const KMTQAITYPE_ADAPTERTYPE: u32 = 15;
const KMTQAITYPE_PHYSICALADAPTERDEVICEIDS: u32 = 31;
/// `D3DKMT_ADAPTERTYPE.SoftwareDevice`: the Microsoft Basic Render Driver.
const ADAPTERTYPE_SOFTWARE: u32 = 1 << 2;

#[link(name = "gdi32")]
extern "system" {
    fn D3DKMTEnumAdapters2(arg: *mut EnumAdapters) -> i32;
    fn D3DKMTQueryAdapterInfo(arg: *mut QueryAdapterInfo) -> i32;
    fn D3DKMTCloseAdapter(arg: *const CloseAdapter) -> i32;
}

/// Ask D3DKMT for one fact about an adapter.
///
/// SAFETY: the caller guarantees `T` is the struct `kind` fills.
unsafe fn adapter_info<T>(handle: u32, kind: u32, out: &mut T) -> bool {
    let mut query = QueryAdapterInfo {
        handle,
        kind,
        data: out as *mut T as *mut c_void,
        size: std::mem::size_of::<T>() as u32,
    };
    D3DKMTQueryAdapterInfo(&mut query) >= 0
}

/// What D3DKMT says of one adapter.
#[derive(Debug, Clone, Default, PartialEq)]
struct AdapterFacts {
    luid: u64,
    /// The driver's name for the card; empty when it gave none.
    name: String,
    vendor: u32,
    device: u32,
    /// PCI (bus, device, function).
    address: Option<(u32, u32, u32)>,
    dedicated: u64,
    shared: u64,
}

/// Every hardware adapter, in PCI order: the system's own enumeration order
/// is not the same at every boot.
fn enumerate_adapters() -> Vec<AdapterFacts> {
    let mut list = Vec::new();
    // SAFETY: the first call only counts; the second fills a buffer of the
    // size it was told, and every out-struct is the one its query fills.
    unsafe {
        let mut request = EnumAdapters {
            count: 0,
            adapters: std::ptr::null_mut(),
        };
        if D3DKMTEnumAdapters2(&mut request) < 0 || request.count == 0 {
            return list;
        }
        let mut infos = vec![AdapterInfo::default(); request.count as usize];
        request.adapters = infos.as_mut_ptr();
        if D3DKMTEnumAdapters2(&mut request) < 0 {
            return list;
        }
        for info in &infos[..(request.count as usize).min(infos.len())] {
            let mut kind = 0u32;
            let mut segments = SegmentSize::default();
            let mut address = AdapterAddress::default();
            let mut ids = DeviceIds::default();
            let mut registry: RegistryInfo = std::mem::zeroed();
            let hardware = adapter_info(info.handle, KMTQAITYPE_ADAPTERTYPE, &mut kind)
                && kind & ADAPTERTYPE_SOFTWARE == 0;
            if hardware {
                adapter_info(info.handle, KMTQAITYPE_GETSEGMENTSIZE, &mut segments);
                adapter_info(info.handle, KMTQAITYPE_PHYSICALADAPTERDEVICEIDS, &mut ids);
                adapter_info(info.handle, KMTQAITYPE_ADAPTERREGISTRYINFO, &mut registry);
                // An adapter that is not on a PCI bus answers with all ones.
                let located = adapter_info(info.handle, KMTQAITYPE_ADAPTERADDRESS, &mut address)
                    && address.bus <= 0xff;
                list.push(AdapterFacts {
                    luid: ((info.luid.high as u32 as u64) << 32) | info.luid.low as u64,
                    name: from_wide(&registry.adapter).trim().to_string(),
                    vendor: ids.vendor,
                    device: ids.device,
                    address: located.then_some((address.bus, address.device, address.function)),
                    dedicated: segments.dedicated_video,
                    shared: segments.shared_system,
                });
            }
            D3DKMTCloseAdapter(&CloseAdapter {
                handle: info.handle,
            });
        }
    }
    list.sort_by_key(|a| (a.address.is_none(), a.address, a.vendor, a.device));
    list
}

fn vendor_name(vendor: u32) -> String {
    match vendor {
        0x1002 => "AMD".to_string(),
        0x10de => "NVIDIA".to_string(),
        0x8086 => "Intel".to_string(),
        0x5143 => "Qualcomm".to_string(),
        other => format!("vendor {other:04x}"),
    }
}

const NVIDIA: u32 = 0x10de;

/// One card of the window.
struct Adapter {
    /// `gpu-<PCI address>`, or the card's UUID where NVML answers for it (as
    /// on Linux).
    id: String,
    title: String,
    detail: String,
    luid: u64,
    /// Totals of the VRAM and Shared memory bars.
    dedicated: u64,
    shared: u64,
    /// The NVML device this adapter is, when it is an NVIDIA card NVML knows.
    nvml: Option<NvmlDevice>,
    pcie_peak: f64,
}

impl Adapter {
    fn new(facts: &AdapterFacts, nvml: Option<&NvmlGpu>) -> Self {
        let vendor = vendor_name(facts.vendor);
        let address = facts
            .address
            .map(|(bus, device, function)| format!("{bus:02x}:{device:02x}.{function:x}"));
        let title = match nvml {
            Some(gpu) => gpu.name.clone(),
            None if facts.name.is_empty() => {
                format!("{vendor} GPU {:04x}:{:04x}", facts.vendor, facts.device)
            }
            None => facts.name.clone(),
        };
        let mut detail = vendor;
        if nvml.is_some() {
            detail.push_str(" · NVML");
        }
        if let Some(address) = &address {
            detail.push_str(&format!(" · PCI {address}"));
        }
        let id = match (nvml, &address) {
            (Some(gpu), _) => gpu.id.clone(),
            (None, Some(address)) => ini_safe(&format!("gpu-{address}")),
            (None, None) => format!("gpu-{:04x}-{:04x}", facts.vendor, facts.device),
        };
        Adapter {
            id,
            title,
            detail,
            luid: facts.luid,
            dedicated: facts.dedicated,
            shared: facts.shared,
            nvml: nvml.map(|gpu| gpu.handle),
            pcie_peak: 0.0,
        }
    }

    /// The bars the counters give: GPU, VRAM and Shared memory, each only
    /// where its half of `usage` was read. An adapter the counters do not
    /// list holds nothing and is idle.
    fn counter_bars(&self, usage: &GpuUsage) -> (Option<Bar>, Option<Bar>, Option<Bar>) {
        let of = |map: &Option<HashMap<u64, u64>>, label: &str, total: u64| {
            let used = map.as_ref()?.get(&self.luid).copied().unwrap_or(0);
            (total > 0).then(|| bytes_bar(label, used, total))
        };
        (
            usage
                .busy
                .as_ref()
                .map(|busy| percent_bar("GPU", busy.get(&self.luid).copied().unwrap_or(0.0))),
            of(&usage.dedicated, "VRAM", self.dedicated),
            of(&usage.shared, "Shared memory", self.shared),
        )
    }

    fn bars(&mut self, usage: &GpuUsage) -> Vec<Bar> {
        let (gpu, vram, shared) = self.counter_bars(usage);
        let mut bars = match (self.nvml, nvml()) {
            (Some(device), Some(nvml)) => nvidia_bars(nvml, device, &mut self.pcie_peak),
            _ => gpu.into_iter().chain(vram).collect(),
        };
        // Shared memory sits under VRAM, whichever source VRAM came from.
        let at = bars
            .iter()
            .position(|b| b.label == "VRAM")
            .map_or(bars.len(), |i| i + 1);
        if let Some(shared) = shared {
            bars.insert(at, shared);
        }
        bars
    }
}

/// The adapters, each NVIDIA one paired with the NVML device at its PCI
/// address.
fn find_adapters() -> Vec<Adapter> {
    enumerate_adapters()
        .iter()
        .map(|facts| {
            let gpu = (facts.vendor == NVIDIA)
                .then(|| {
                    let (bus, device, _) = facts.address?;
                    let devices = &nvml()?.devices;
                    devices.iter().find(|g| g.address == Some((bus, device)))
                })
                .flatten();
            Adapter::new(facts, gpu)
        })
        .collect()
}

// ── CPU and memory ──────────────────────────────────────────────────

/// `(busy, total)` processor time, in 100 ns units summed over every core.
/// Kernel time includes the idle time.
fn cpu_times() -> Option<(u64, u64)> {
    let (mut idle, mut kernel, mut user) = (0u64, 0u64, 0u64);
    // SAFETY: three live locals of FILETIME's size and layout.
    if unsafe { GetSystemTimes(&mut idle, &mut kernel, &mut user) } == 0 {
        return None;
    }
    let total = kernel + user;
    Some((total.saturating_sub(idle), total))
}

/// The memory figures of Task Manager's Performance page, in bytes.
struct Mem {
    total: u64,
    available: u64,
    /// The commit charge and its limit (RAM plus page files): what an
    /// allocation is refused against, whatever RAM is free.
    committed: u64,
    commit_limit: u64,
}

fn memory() -> Option<Mem> {
    let mut info = PerformanceInformation {
        cb: std::mem::size_of::<PerformanceInformation>() as u32,
        ..Default::default()
    };
    // SAFETY: `info` is the struct the call fills, and `cb` its size.
    if unsafe { K32GetPerformanceInfo(&mut info, info.cb) } == 0 {
        return None;
    }
    let bytes = |pages: usize| pages as u64 * info.page_size as u64;
    Some(Mem {
        total: bytes(info.physical_total),
        available: bytes(info.physical_available),
        committed: bytes(info.commit_total),
        commit_limit: bytes(info.commit_limit),
    })
}

/// `hwmon::system_ram`: total and available physical memory, in bytes.
pub fn system_ram() -> Option<(u64, u64)> {
    memory().map(|m| (m.total, m.available))
}

// ── Disk ────────────────────────────────────────────────────────────

const FILE_SHARE_READ_WRITE: u32 = 0x1 | 0x2;
const OPEN_EXISTING: u32 = 3;
const IOCTL_STORAGE_GET_DEVICE_NUMBER: u32 = 0x002D_1080;
const IOCTL_STORAGE_QUERY_PROPERTY: u32 = 0x002D_1400;
const IOCTL_DISK_PERFORMANCE: u32 = 0x0007_0020;

/// `STORAGE_DEVICE_NUMBER`.
#[repr(C)]
#[derive(Default)]
struct DeviceNumber {
    device_type: u32,
    number: u32,
    partition: u32,
}

/// `DISK_PERFORMANCE`. The times are in 100 ns units.
#[repr(C)]
#[derive(Default)]
struct DiskPerformance {
    bytes_read: i64,
    bytes_written: i64,
    read_time: i64,
    write_time: i64,
    idle_time: i64,
    read_count: u32,
    write_count: u32,
    queue_depth: u32,
    split_count: u32,
    query_time: i64,
    storage_device_number: u32,
    storage_manager_name: [u16; 8],
}

/// An open volume or drive, closed on drop.
struct Device(Handle);

impl Device {
    fn open(name: &str) -> Option<Device> {
        // SAFETY: `name` is NUL-terminated and outlives the call.
        let handle = unsafe {
            CreateFileW(
                wide(name).as_ptr(),
                0,
                FILE_SHARE_READ_WRITE,
                std::ptr::null(),
                OPEN_EXISTING,
                0,
                std::ptr::null_mut(),
            )
        };
        // INVALID_HANDLE_VALUE is -1.
        (handle as isize != -1).then_some(Device(handle))
    }

    /// An IOCTL that takes `input` and fills `output`.
    ///
    /// SAFETY: the caller guarantees `I` and `O` are the structs `code` takes
    /// and fills.
    unsafe fn control<I, O>(&self, code: u32, input: Option<&I>, output: &mut O) -> bool {
        let mut returned = 0u32;
        DeviceIoControl(
            self.0,
            code,
            input.map_or(std::ptr::null(), |i| i as *const I as *const c_void),
            input.map_or(0, |_| std::mem::size_of::<I>() as u32),
            output as *mut O as *mut c_void,
            std::mem::size_of::<O>() as u32,
            &mut returned,
            std::ptr::null_mut(),
        ) != 0
    }
}

impl Drop for Device {
    fn drop(&mut self) {
        unsafe { CloseHandle(self.0) };
    }
}

/// The counters of one `IOCTL_DISK_PERFORMANCE` reading this module uses.
#[derive(Debug, Clone, Copy, PartialEq)]
struct DiskStat {
    read_bytes: i64,
    written_bytes: i64,
    /// Time the device had no request in flight, in 100 ns units.
    idle: i64,
    /// When the counters were read, on the same clock as `idle`.
    at: i64,
}

/// The device's name out of a `STORAGE_DEVICE_DESCRIPTOR`: its vendor and
/// product strings, each an offset into the descriptor (0 = absent). An NVMe
/// drive has no vendor string; its product string is the whole model.
fn descriptor_model(descriptor: &[u8]) -> String {
    let text = |offset_at: usize| {
        let offset = descriptor
            .get(offset_at..offset_at + 4)
            .map(|b| u32::from_le_bytes([b[0], b[1], b[2], b[3]]) as usize)
            .filter(|o| *o != 0)?;
        let bytes = descriptor.get(offset..)?;
        let end = bytes.iter().position(|b| *b == 0).unwrap_or(bytes.len());
        let text = String::from_utf8_lossy(&bytes[..end]).trim().to_string();
        (!text.is_empty()).then_some(text)
    };
    // VendorIdOffset and ProductIdOffset.
    [text(12), text(16)]
        .into_iter()
        .flatten()
        .collect::<Vec<_>>()
        .join(" ")
}

/// The whole disk a path is stored on.
struct Disk {
    device: Device,
    title: String,
    detail: String,
}

fn disk_for(path: &Path) -> Option<Disk> {
    // A models directory that does not exist yet still has a disk: the one
    // its nearest existing ancestor is on.
    let path = path.ancestors().find(|p| p.exists())?;
    let path: Vec<u16> = path.as_os_str().encode_wide().chain(Some(0)).collect();
    let mut mount = [0u16; 512];
    let mut volume = [0u16; 64];
    // SAFETY: each buffer is passed with its own length. The mount point is
    // `E:\` (or a folder a volume is mounted on); the volume name is
    // `\\?\Volume{GUID}\`, which a network share does not have.
    let found = unsafe {
        GetVolumePathNameW(path.as_ptr(), mount.as_mut_ptr(), mount.len() as u32) != 0
            && GetVolumeNameForVolumeMountPointW(
                mount.as_ptr(),
                volume.as_mut_ptr(),
                volume.len() as u32,
            ) != 0
    };
    if !found {
        return None;
    }
    let mount = from_wide(&mount);
    let mount = mount.trim_end_matches('\\');
    // Without its trailing backslash the name is the volume itself; with it,
    // the volume's root directory.
    let volume = Device::open(from_wide(&volume).trim_end_matches('\\'))?;

    // The drive under the volume. A volume that spans several has no single
    // drive number: its own counters are read instead.
    let mut number = DeviceNumber::default();
    // SAFETY: STORAGE_DEVICE_NUMBER is what this IOCTL fills.
    let drive =
        unsafe { volume.control(IOCTL_STORAGE_GET_DEVICE_NUMBER, None::<&u32>, &mut number) }
            .then(|| Device::open(&format!(r"\\.\PhysicalDrive{}", number.number)))
            .flatten();
    let (device, name) = match drive {
        Some(drive) => (drive, format!("PhysicalDrive{} · {mount}", number.number)),
        None => (volume, mount.to_string()),
    };

    // STORAGE_PROPERTY_QUERY for StorageDeviceProperty, PropertyStandardQuery
    // (both 0), answered with a STORAGE_DEVICE_DESCRIPTOR and its strings.
    let query = [0u32; 3];
    let mut descriptor = [0u8; 1024];
    // SAFETY: the query struct and a byte buffer the answer fits in.
    let described =
        unsafe { device.control(IOCTL_STORAGE_QUERY_PROPERTY, Some(&query), &mut descriptor) };
    let model = if described {
        descriptor_model(&descriptor)
    } else {
        String::new()
    };
    Some(Disk {
        title: if model.is_empty() {
            format!("Disk {mount}")
        } else {
            model
        },
        detail: format!("{name} · the models directory's disk"),
        device,
    })
}

impl Disk {
    fn stat(&self) -> Option<DiskStat> {
        let mut perf = DiskPerformance::default();
        // SAFETY: DISK_PERFORMANCE is what this IOCTL fills.
        unsafe {
            self.device
                .control(IOCTL_DISK_PERFORMANCE, None::<&u32>, &mut perf)
        }
        .then_some(DiskStat {
            read_bytes: perf.bytes_read,
            written_bytes: perf.bytes_written,
            idle: perf.idle_time,
            at: perf.query_time,
        })
    }
}

/// The disk's bars between two readings; none when no time passed.
fn disk_bars(a: DiskStat, b: DiskStat, peak: &mut f64) -> Vec<Bar> {
    let ticks = (b.at - a.at) as f64;
    if ticks <= 0.0 {
        return Vec::new();
    }
    let rate = |x: i64, y: i64| (y - x).max(0) as f64 / (ticks / 1e7);
    let read = rate(a.read_bytes, b.read_bytes);
    let write = rate(a.written_bytes, b.written_bytes);
    // Both directions share one scale, so the two bars compare.
    *peak = peak.max(read).max(write);
    let busy = 100.0 * (1.0 - (b.idle - a.idle) as f64 / ticks);
    vec![
        rate_bar("Read", read, peak),
        rate_bar("Write", write, peak),
        percent_bar("Busy", busy.clamp(0.0, 100.0)),
    ]
}

// ── The sampler ─────────────────────────────────────────────────────

/// The counters a rate is the difference of.
struct Counters {
    cpu: Option<(u64, u64)>,
    disk: Option<DiskStat>,
}

pub struct Sampler {
    adapters: Vec<Adapter>,
    gpu: GpuCounters,
    cached: Option<CachedCounters>,
    disk: Option<Disk>,
    cpu_title: String,
    threads: usize,
    prev: Counters,
    disk_peak: f64,
}

impl Sampler {
    /// Find the devices and take the baseline the first `sample()` is a
    /// difference against. `models_dir` picks the disk; `off` is the set
    /// `sample` takes.
    pub fn new(models_dir: &Path, off: &HashSet<String>) -> Self {
        let cpu_title = crate::startup::machine_reg_sz(
            r"HARDWARE\DESCRIPTION\System\CentralProcessor\0",
            "ProcessorNameString",
        )
        .map(|name| name.trim().to_string())
        .filter(|name| !name.is_empty());
        let mut s = Sampler {
            adapters: find_adapters(),
            gpu: GpuCounters::open(),
            cached: CachedCounters::open(),
            disk: disk_for(models_dir),
            cpu_title: cpu_title.unwrap_or_else(|| "CPU".to_string()),
            threads: std::thread::available_parallelism().map_or(0, |n| n.get()),
            prev: Counters {
                cpu: None,
                disk: None,
            },
            disk_peak: 0.0,
        };
        s.prev = s.counters(off);
        s.gpu.read(&s.adapters, off);
        s
    }

    /// The counters of the devices that are on. One left `None` makes the
    /// next sample's rate wait for a second reading instead of spanning the
    /// time the device was off.
    fn counters(&self, off: &HashSet<String>) -> Counters {
        Counters {
            cpu: (!off.contains(CPU_ID)).then(cpu_times).flatten(),
            disk: self
                .disk
                .as_ref()
                .filter(|_| !off.contains(DISK_ID))
                .and_then(Disk::stat),
        }
    }

    /// Everything the window shows, GPUs first. A device whose `id` is
    /// in `off` comes back as a group without bars, unread.
    pub fn sample(&mut self, off: &HashSet<String>) -> Vec<Group> {
        let now = self.counters(off);
        let usage = self.gpu.read(&self.adapters, off);
        let mut groups = Vec::new();

        for adapter in &mut self.adapters {
            let (id, title, detail) = (
                adapter.id.clone(),
                adapter.title.clone(),
                adapter.detail.clone(),
            );
            groups.push(group(off, &id, title, detail, || adapter.bars(&usage)));
        }

        let cpu = match (self.prev.cpu, now.cpu) {
            (Some((b0, t0)), Some((b1, t1))) if t1 > t0 => {
                Some(100.0 * b1.saturating_sub(b0) as f64 / (t1 - t0) as f64)
            }
            _ => None,
        };
        let detail = format!("{} threads", self.threads);
        groups.push(group(off, CPU_ID, self.cpu_title.clone(), detail, || {
            cpu.map(|p| percent_bar("CPU", p)).into_iter().collect()
        }));

        let cached = &self.cached;
        groups.push(group(
            off,
            MEMORY_ID,
            "Memory".to_string(),
            String::new(),
            || {
                let Some(mem) = memory() else {
                    return Vec::new();
                };
                let mut bars = vec![bytes_bar(
                    "RAM",
                    mem.total.saturating_sub(mem.available),
                    mem.total,
                )];
                if let Some(bytes) = cached.as_ref().and_then(CachedCounters::read) {
                    bars.push(bytes_bar("Cached", bytes, mem.total));
                }
                bars.push(bytes_bar("Committed", mem.committed, mem.commit_limit));
                bars
            },
        ));

        if let Some(disk) = &self.disk {
            let peak = &mut self.disk_peak;
            let (before, after) = (self.prev.disk, now.disk);
            let (title, detail) = (disk.title.clone(), disk.detail.clone());
            groups.push(group(off, DISK_ID, title, detail, || {
                match (before, after) {
                    (Some(a), Some(b)) => disk_bars(a, b, peak),
                    _ => Vec::new(),
                }
            }));
        }

        self.prev = now;
        groups
    }
}

// Everything that parses or formats is tested against made-up readings, so
// only the one test that says so depends on the machine it runs on.
#[cfg(test)]
mod tests {
    use super::*;

    const R9700: u64 = 0x0001_AA91;
    const IGPU: u64 = 0x0001_6D04;

    #[test]
    fn a_counter_instance_names_its_adapter_and_engine() {
        let engine = "pid_12340_luid_0x00000000_0x00016D04_phys_0_eng_10_engtype_Video Codec 0";
        assert_eq!(instance_luid(engine), Some(IGPU));
        assert_eq!(instance_engine(engine), Some(10));
        // The high half counts, and PowerShell prints the digits in lower case.
        assert_eq!(
            instance_luid("luid_0x00000001_0x0001aa91_phys_0"),
            Some((1 << 32) | R9700)
        );
        // A memory instance has an adapter and no engine.
        assert_eq!(instance_engine("luid_0x00000000_0x0001AA91_phys_0"), None);
        assert_eq!(instance_luid("_Total"), None);
        assert_eq!(instance_luid("luid_0x0000_0x1"), None);
    }

    #[test]
    fn an_adapter_is_as_busy_as_its_busiest_engine() {
        let busy = busiest_engines([
            // Two processes on the R9700's compute engine add up...
            (
                "pid_1_luid_0x00000000_0x0001AA91_phys_0_eng_2_engtype_Compute 0",
                40.0,
            ),
            (
                "pid_2_luid_0x00000000_0x0001AA91_phys_0_eng_2_engtype_Compute 0",
                35.0,
            ),
            // ...and its 3D engine does not add to them.
            (
                "pid_1_luid_0x00000000_0x0001AA91_phys_0_eng_0_engtype_3D",
                20.0,
            ),
            // Rounding across processes must not read as more than all of it.
            (
                "pid_1_luid_0x00000000_0x00016D04_phys_0_eng_0_engtype_3D",
                70.0,
            ),
            (
                "pid_2_luid_0x00000000_0x00016D04_phys_0_eng_0_engtype_3D",
                31.0,
            ),
            ("_Total", 99.0),
        ]);
        assert_eq!(busy.len(), 2);
        assert_eq!(busy[&R9700], 75.0);
        assert_eq!(busy[&IGPU], 100.0);
    }

    fn facts(vendor: u32, address: Option<(u32, u32, u32)>, name: &str) -> AdapterFacts {
        AdapterFacts {
            luid: R9700,
            name: name.to_string(),
            vendor,
            device: 0x7551,
            address,
            dedicated: 32 << 30,
            shared: 48 << 30,
        }
    }

    #[test]
    fn an_adapter_is_named_by_its_driver_and_told_apart_by_its_address() {
        let card = Adapter::new(
            &facts(0x1002, Some((0x12, 0, 0)), "AMD Radeon AI PRO R9700"),
            None,
        );
        assert_eq!(card.title, "AMD Radeon AI PRO R9700");
        assert_eq!(card.detail, "AMD · PCI 12:00.0");
        assert_eq!(card.id, "gpu-12:00.0");
        assert!(card.nvml.is_none());

        // No driver name and no PCI address: the ids still say which card.
        let bare = Adapter::new(&facts(0x1234, None, ""), None);
        assert_eq!(bare.title, "vendor 1234 GPU 1234:7551");
        assert_eq!(bare.detail, "vendor 1234");
        assert_eq!(bare.id, "gpu-1234-7551");

        // An NVIDIA card NVML knows takes its name and its UUID from there.
        let gpu = NvmlGpu {
            handle: std::ptr::null_mut(),
            name: "NVIDIA GeForce RTX 4070 SUPER".to_string(),
            id: "GPU-5a3b".to_string(),
            address: Some((1, 0)),
        };
        let card = Adapter::new(&facts(NVIDIA, Some((1, 0, 0)), "ignored"), Some(&gpu));
        assert_eq!(card.title, "NVIDIA GeForce RTX 4070 SUPER");
        assert_eq!(card.detail, "NVIDIA · NVML · PCI 01:00.0");
        assert_eq!(card.id, "GPU-5a3b");
    }

    #[test]
    fn a_counter_that_was_not_read_only_drops_its_bar() {
        let mut card = Adapter::new(&facts(0x1002, Some((0x12, 0, 0)), "Card"), None);
        let labels =
            |bars: &[Bar]| -> Vec<String> { bars.iter().map(|b| b.label.clone()).collect() };

        let all = GpuUsage {
            busy: Some(HashMap::from([(R9700, 82.4)])),
            dedicated: Some(HashMap::from([(R9700, 16 << 30)])),
            shared: Some(HashMap::from([(R9700, 12 << 30)])),
        };
        let bars = card.bars(&all);
        assert_eq!(labels(&bars), ["GPU", "VRAM", "Shared memory"]);
        assert_eq!(bars[0].text, "82 %");
        assert_eq!(bars[1].text, "16.0 / 32.0 GiB");
        assert_eq!(bars[2].fraction, 0.25);

        // No utilization baseline yet: the memory bars are already there.
        let memory_only = GpuUsage { busy: None, ..all };
        assert_eq!(labels(&card.bars(&memory_only)), ["VRAM", "Shared memory"]);
        assert!(card.bars(&GpuUsage::default()).is_empty());

        // An adapter the counters do not list is idle and empty, not absent.
        let other = GpuUsage {
            busy: Some(HashMap::from([(IGPU, 50.0)])),
            dedicated: Some(HashMap::new()),
            shared: None,
        };
        let bars = card.bars(&other);
        assert_eq!(labels(&bars), ["GPU", "VRAM"]);
        assert_eq!((bars[0].text.as_str(), bars[1].fraction), ("0 %", 0.0));

        // A total of zero (an adapter without dedicated memory) is not a bar.
        card.dedicated = 0;
        assert_eq!(labels(&card.bars(&other)), ["GPU"]);
    }

    #[test]
    fn disk_bars_are_rates_over_the_drive_s_own_clock() {
        let a = DiskStat {
            read_bytes: 1 << 30,
            written_bytes: 0,
            idle: 0,
            at: 0,
        };
        // Two seconds later: 64 MiB read, 16 MiB written, idle for 1.5 s.
        let b = DiskStat {
            read_bytes: (1 << 30) + (64 << 20),
            written_bytes: 16 << 20,
            idle: 15_000_000,
            at: 20_000_000,
        };
        let mut peak = 0.0;
        let bars = disk_bars(a, b, &mut peak);
        assert_eq!(bars[0].text, "32.0 MiB/s · peak 32.0 MiB/s");
        // One scale for both directions.
        assert_eq!(bars[1].text, "8.0 MiB/s · peak 32.0 MiB/s");
        assert_eq!(bars[2].text, "25 %");
        // The same reading twice is no interval at all.
        assert!(disk_bars(b, b, &mut peak).is_empty());
    }

    #[test]
    fn a_drive_is_named_by_its_descriptor_strings() {
        let mut d = vec![0u8; 64];
        // VendorIdOffset = 0 (absent), ProductIdOffset = 40.
        d[16] = 40;
        d[40..54].copy_from_slice(b"CT1000T700SSD5");
        assert_eq!(descriptor_model(&d), "CT1000T700SSD5");
        // A vendor string too, padded with blanks as SCSI inquiry data is.
        d[12] = 30;
        d[30..38].copy_from_slice(b"WD      ");
        assert_eq!(descriptor_model(&d), "WD CT1000T700SSD5");
        // An offset past the end, or no strings at all, is no name.
        d[16] = 200;
        assert_eq!(descriptor_model(&d), "WD");
        assert_eq!(descriptor_model(&[0u8; 8]), "");
    }

    // Against the real machine, but only through what every Windows has: with
    // CPU and memory off the sample still lists them, empty, and a device
    // switched back on needs one more reading before its rate shows.
    #[test]
    fn sample_skips_what_is_off_and_resumes_a_rate_one_reading_later() {
        let none = HashSet::new();
        let mut s = Sampler::new(Path::new("C:\\"), &none);
        let off = HashSet::from([CPU_ID.to_string(), MEMORY_ID.to_string()]);
        let find =
            |groups: &[Group], id: &str| groups.iter().find(|g| g.id == id).cloned().unwrap();

        let groups = s.sample(&off);
        for id in &off {
            let g = find(&groups, id);
            assert_eq!((g.enabled, g.bars.len()), (false, 0), "{id:?}");
        }
        assert!(s.prev.cpu.is_none(), "the processor times were not read");
        // Every id is unique: it is the only thing a checkbox acts on.
        let ids: HashSet<&str> = groups.iter().map(|g| g.id.as_str()).collect();
        assert_eq!(ids.len(), groups.len());

        // Back on: memory is instantaneous, the CPU rate has no baseline yet.
        let groups = s.sample(&none);
        let memory = find(&groups, MEMORY_ID);
        assert_eq!(memory.bars.first().map(|b| b.label.as_str()), Some("RAM"));
        assert_eq!(
            memory.bars.last().map(|b| b.label.as_str()),
            Some("Committed")
        );
        let cpu = find(&groups, CPU_ID);
        assert_eq!((cpu.enabled, cpu.bars.len()), (true, 0));
        // The processor times advance in clock ticks of about 16 ms.
        std::thread::sleep(std::time::Duration::from_millis(100));
        assert_eq!(find(&s.sample(&none), CPU_ID).bars.len(), 1);
    }
}
