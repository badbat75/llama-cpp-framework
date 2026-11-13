//! The Linux sources of the HW Monitor: sysfs for amdgpu and the disk,
//! `/proc` for the CPU and memory. The table of what each bar reads is in the
//! parent module's header.

use super::*;
use crate::devices::{Cpu, MemoryModule, PcieLink, PhysicalGpu};
use std::collections::HashMap;
use std::os::unix::fs::MetadataExt;
use std::path::{Path, PathBuf};
use std::time::Instant;

/// `/sys/block/*/stat` counts sectors of 512 bytes whatever the device's own
/// sector size is.
const SECTOR: u64 = 512;

// ── Runtime library loading (NVML) ──────────────────────────────────

extern "C" {
    fn dlopen(filename: *const c_char, flags: c_int) -> *mut c_void;
    fn dlsym(handle: *mut c_void, symbol: *const c_char) -> *mut c_void;
}
const RTLD_NOW: c_int = 2;

pub(super) const NVML_LIBRARY: &CStr = c"libnvidia-ml.so.1";

pub(super) unsafe fn open_library(name: &CStr) -> *mut c_void {
    dlopen(name.as_ptr(), RTLD_NOW)
}

pub(super) unsafe fn library_symbol(lib: *mut c_void, name: &CStr) -> *mut c_void {
    dlsym(lib, name.as_ptr())
}

// ── CPU and memory ──────────────────────────────────────────────────

/// `(busy, total, iowait)` jiffies from the aggregate `cpu` line of `/proc/stat`.
/// Idle and iowait are the not-busy part; guest time is already inside user.
fn parse_cpu(stat: &str) -> Option<(u64, u64, u64)> {
    parse_cpu_line(stat.lines().find(|l| l.starts_with("cpu "))?)
}

/// The same per socket, adding up the lines of each socket's threads (`cpu<N>`).
/// With one socket that is the aggregate line.
fn parse_sockets(
    stat: &str,
    thread_socket: &HashMap<u32, usize>,
    sockets: usize,
) -> Option<Vec<(u64, u64, u64)>> {
    if sockets <= 1 {
        return parse_cpu(stat).map(|times| vec![times]);
    }
    let mut totals = vec![(0, 0, 0); sockets];
    for line in stat.lines() {
        let thread = line
            .split_whitespace()
            .next()
            .and_then(|name| name.strip_prefix("cpu"))
            .and_then(|n| n.parse::<u32>().ok());
        let total = thread
            .and_then(|t| thread_socket.get(&t))
            .and_then(|s| totals.get_mut(*s));
        if let (Some(total), Some((busy, all, iowait))) = (total, parse_cpu_line(line)) {
            total.0 += busy;
            total.1 += all;
            total.2 += iowait;
        }
    }
    Some(totals)
}

fn parse_cpu_line(line: &str) -> Option<(u64, u64, u64)> {
    let v: Vec<u64> = line
        .split_whitespace()
        .skip(1)
        .take(8)
        .filter_map(|f| f.parse().ok())
        .collect();
    if v.len() < 5 {
        return None;
    }
    let total: u64 = v.iter().sum();
    Some((total - v[3] - v[4], total, v[4]))
}

/// A sysfs file's text, trimmed.
fn read_trimmed(path: &Path) -> Option<String> {
    Some(std::fs::read_to_string(path).ok()?.trim().to_string())
}

/// A sysfs cache size: `32K`, `1024K`, `32M`.
fn cache_kib(size: &str) -> Option<u64> {
    match size.strip_suffix('K') {
        Some(kib) => kib.parse().ok(),
        None => size
            .strip_suffix('M')?
            .parse::<u64>()
            .ok()
            .map(|mib| mib * 1024),
    }
}

/// The subdirectories named `<prefix><number>`, with their numbers.
fn numbered_with(dir: &Path, prefix: &str) -> Vec<(u32, PathBuf)> {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return Vec::new();
    };
    entries
        .flatten()
        .filter_map(|e| {
            let name = e.file_name();
            let n = name.to_str()?.strip_prefix(prefix)?.parse().ok()?;
            Some((n, e.path()))
        })
        .collect()
}

fn numbered(dir: &Path, prefix: &str) -> Vec<PathBuf> {
    numbered_with(dir, prefix)
        .into_iter()
        .map(|(_, path)| path)
        .collect()
}

/// Which socket each thread is on, the sockets counted from 0 in the order of
/// their ids, and how many there are (at least one).
fn thread_sockets(cpu_dir: &Path) -> (HashMap<u32, usize>, usize) {
    let threads: Vec<(u32, u32)> = numbered_with(cpu_dir, "cpu")
        .into_iter()
        .filter_map(|(thread, dir)| {
            let package = read_trimmed(&dir.join("topology/physical_package_id"))?;
            Some((thread, package.parse().ok()?))
        })
        .collect();
    let mut packages: Vec<u32> = threads.iter().map(|(_, package)| *package).collect();
    packages.sort_unstable();
    packages.dedup();
    let socket = |package: &u32| packages.iter().position(|p| p == package).unwrap_or(0);
    (
        threads
            .iter()
            .map(|(thread, package)| (*thread, socket(package)))
            .collect(),
        packages.len().max(1),
    )
}

/// The processor's facts from `cpu_dir` (`/sys/devices/system/cpu`) and
/// `node_dir` (`/sys/devices/system/node`). A cache shared between threads
/// appears under each of them, so one is told by what shares it.
fn read_cpu_sysfs(cpu_dir: &Path, node_dir: &Path, cpu: &mut Cpu) {
    let (mut threads, mut sockets, mut cores, mut caches) =
        (0u32, HashSet::new(), HashSet::new(), HashSet::new());
    let mut cache_totals = [0u64; 3];
    for thread in numbered(cpu_dir, "cpu") {
        threads += 1;
        let package = read_trimmed(&thread.join("topology/physical_package_id"));
        let core = read_trimmed(&thread.join("topology/core_id"));
        if let (Some(package), Some(core)) = (package, core) {
            sockets.insert(package.clone());
            cores.insert((package, core));
        }
        for index in numbered(&thread.join("cache"), "index") {
            let text = |file: &str| read_trimmed(&index.join(file));
            let (Some(level), Some(kind), Some(shared), Some(kib)) = (
                text("level").and_then(|l| l.parse::<usize>().ok()),
                text("type"),
                text("shared_cpu_list"),
                text("size").and_then(|s| cache_kib(&s)),
            ) else {
                continue;
            };
            if (1..=3).contains(&level) && caches.insert((level, kind, shared)) {
                cache_totals[level - 1] += kib;
            }
        }
    }
    let count = |n: usize| (n > 0).then_some(n as u32);
    cpu.threads = (threads > 0).then_some(threads);
    cpu.sockets = count(sockets.len());
    cpu.cores = count(cores.len());
    cpu.numa_nodes = count(numbered(node_dir, "node").len());
    let kib = |n: u64| (n > 0).then_some(n);
    (cpu.l1_kib, cpu.l2_kib, cpu.l3_kib) = (
        kib(cache_totals[0]),
        kib(cache_totals[1]),
        kib(cache_totals[2]),
    );
    let mhz = |file: &str| {
        read_trimmed(&cpu_dir.join("cpu0/cpufreq").join(file))?
            .parse::<u32>()
            .ok()
            .map(|khz| khz / 1000)
    };
    cpu.max_mhz = mhz("cpuinfo_max_freq");
    cpu.base_mhz = mhz("base_frequency").or_else(|| mhz("amd_pstate_nominal_freq"));
}

pub fn cpu_facts() -> Cpu {
    let mut cpu = Cpu {
        instructions: super::cpu_instructions(),
        ..Default::default()
    };
    read_cpu_sysfs(
        Path::new("/sys/devices/system/cpu"),
        Path::new("/sys/devices/system/node"),
        &mut cpu,
    );
    cpu
}

/// The modules in udev's record of the DMI device (`/run/udev/data/+dmi:id`, its
/// `MEMORY_DEVICE_<n>_<field>` properties): the one readable copy of the firmware's
/// table, the table itself being root's alone. Empty where nothing records them.
fn parse_udev_memory(record: &str) -> Vec<MemoryModule> {
    let mut modules: std::collections::BTreeMap<u32, MemoryModule> = Default::default();
    let known = |value: &str| {
        let lower = value.to_ascii_lowercase();
        let placeholder = ["unknown", "not specified", "none", "undefined"];
        (!placeholder.contains(&lower.as_str())).then(|| value.trim().to_string())
    };
    let mut slots: HashMap<u32, (String, String)> = HashMap::new();
    for line in record.lines() {
        let Some(property) = line.strip_prefix("E:MEMORY_DEVICE_") else {
            continue;
        };
        let Some((name, value)) = property.split_once('=') else {
            continue;
        };
        let Some((n, field)) = name.split_once('_') else {
            continue;
        };
        let Ok(n) = n.parse::<u32>() else {
            continue;
        };
        let module = modules.entry(n).or_default();
        match field {
            "SIZE" => module.bytes = value.parse().unwrap_or(0),
            "TYPE" => module.kind = known(value),
            "MANUFACTURER" => module.maker = known(value).unwrap_or_default(),
            "PART_NUMBER" => module.part = known(value).unwrap_or_default(),
            "SPEED_MTS" => module.rated_mts = value.parse().ok().filter(|v| *v != 0),
            "CONFIGURED_SPEED_MTS" => {
                module.configured_mts = value.parse().ok().filter(|v| *v != 0);
            }
            "CONFIGURED_VOLTAGE" => {
                module.configured_millivolts = value.parse().ok().filter(|v| *v != 0);
            }
            "BANK_LOCATOR" => slots.entry(n).or_default().0 = known(value).unwrap_or_default(),
            "LOCATOR" => slots.entry(n).or_default().1 = known(value).unwrap_or_default(),
            _ => {}
        }
    }
    modules
        .into_iter()
        .filter(|(_, m)| m.bytes > 0)
        .map(|(n, mut m)| {
            if let Some((bank, slot)) = slots.get(&n) {
                let names = [bank.as_str(), slot.as_str()];
                m.slot = names
                    .into_iter()
                    .filter(|s| !s.is_empty())
                    .collect::<Vec<_>>()
                    .join(" / ");
            }
            m
        })
        .collect()
}

pub fn memory_modules() -> Vec<MemoryModule> {
    parse_udev_memory(&std::fs::read_to_string("/run/udev/data/+dmi:id").unwrap_or_default())
}

/// The `/proc/meminfo` fields the window shows, in bytes.
#[derive(Debug, Default, PartialEq)]
struct Mem {
    total: u64,
    available: u64,
    cached: u64,
    swap_total: u64,
    swap_free: u64,
}

/// `hwmon::system_ram`: `MemTotal` and `MemAvailable`, in bytes.
pub fn system_ram() -> Option<(u64, u64)> {
    let m = parse_meminfo(&std::fs::read_to_string("/proc/meminfo").ok()?);
    (m.total > 0).then_some((m.total, m.available))
}

fn parse_meminfo(s: &str) -> Mem {
    let mut m = Mem::default();
    for line in s.lines() {
        let Some((key, rest)) = line.split_once(':') else {
            continue;
        };
        let slot = match key {
            "MemTotal" => &mut m.total,
            "MemAvailable" => &mut m.available,
            "Cached" => &mut m.cached,
            "SwapTotal" => &mut m.swap_total,
            "SwapFree" => &mut m.swap_free,
            _ => continue,
        };
        let kb: u64 = rest
            .split_whitespace()
            .next()
            .and_then(|v| v.parse().ok())
            .unwrap_or(0);
        *slot = kb * 1024;
    }
    m
}

fn cpu_model(cpuinfo: &str) -> Option<String> {
    cpuinfo
        .lines()
        .find(|l| l.starts_with("model name"))
        .and_then(|l| l.split_once(':'))
        .map(|(_, v)| v.trim().to_string())
}

// ── Disk ────────────────────────────────────────────────────────────

/// The counters of one `/sys/block/<disk>/stat` line this module uses.
#[derive(Debug, Clone, Copy, PartialEq)]
struct BlockStat {
    read_sectors: u64,
    write_sectors: u64,
    /// Milliseconds the device had at least one request in flight.
    io_ticks: u64,
    /// The I/Os completed, and the milliseconds they took between them.
    io_count: u64,
    io_wait_ms: u64,
}

fn parse_block_stat(s: &str) -> Option<BlockStat> {
    let v: Vec<u64> = s
        .split_whitespace()
        .filter_map(|f| f.parse().ok())
        .collect();
    (v.len() >= 10).then(|| BlockStat {
        read_sectors: v[2],
        write_sectors: v[6],
        io_ticks: v[9],
        io_count: v[0] + v[4],
        io_wait_ms: v[3] + v[7],
    })
}

/// Split a `st_dev` into the kernel's (major, minor), as glibc's macros do.
fn major_minor(dev: u64) -> (u64, u64) {
    (
        ((dev >> 8) & 0xfff) | ((dev >> 32) & !0xfff),
        (dev & 0xff) | ((dev >> 12) & !0xff),
    )
}

/// Undo `/proc/self/mountinfo`'s octal escapes (`\040` for a space).
fn unescape_mount(s: &str) -> String {
    let b = s.as_bytes();
    let mut out = Vec::with_capacity(b.len());
    let mut i = 0;
    while i < b.len() {
        let octal = (b[i] == b'\\' && i + 3 < b.len())
            .then(|| std::str::from_utf8(&b[i + 1..i + 4]).ok())
            .flatten()
            .and_then(|d| u8::from_str_radix(d, 8).ok());
        match octal {
            Some(c) => {
                out.push(c);
                i += 4;
            }
            None => {
                out.push(b[i]);
                i += 1;
            }
        }
    }
    String::from_utf8_lossy(&out).into_owned()
}

/// The source device of the mount `path` lives on: the longest mount point
/// that is a prefix of it. Needed where `st_dev` is not a block device at all
/// (btrfs hands out an anonymous one per subvolume).
fn mount_source(mountinfo: &str, path: &Path) -> Option<String> {
    let mut best: Option<(usize, String)> = None;
    for line in mountinfo.lines() {
        let Some((head, tail)) = line.split_once(" - ") else {
            continue;
        };
        let (Some(point), Some(source)) = (head.split(' ').nth(4), tail.split(' ').nth(1)) else {
            continue;
        };
        let point = unescape_mount(point);
        let depth = Path::new(&point).components().count();
        if path.starts_with(&point) && best.as_ref().is_none_or(|(d, _)| depth >= *d) {
            best = Some((depth, unescape_mount(source)));
        }
    }
    best.map(|(_, source)| source)
}

/// The whole disk a path is stored on.
struct Disk {
    /// Kernel name (`nvme0n1`, `dm-0`).
    name: String,
    model: String,
    stat: PathBuf,
}

fn block_node(dev: u64) -> Option<PathBuf> {
    let (major, minor) = major_minor(dev);
    let node = PathBuf::from(format!("/sys/dev/block/{major}:{minor}"));
    node.exists().then_some(node)
}

fn disk_for(path: &Path) -> Option<Disk> {
    // A models directory that does not exist yet still has a disk: the one
    // its nearest existing ancestor is on.
    let path = std::fs::canonicalize(path.ancestors().find(|p| p.exists())?).ok()?;
    let node = block_node(std::fs::metadata(&path).ok()?.dev()).or_else(|| {
        let mounts = std::fs::read_to_string("/proc/self/mountinfo").ok()?;
        let source = mount_source(&mounts, &path)?;
        block_node(std::fs::metadata(source).ok()?.rdev())
    })?;
    let mut dir = std::fs::canonicalize(node).ok()?;
    if dir.join("partition").exists() {
        dir = dir.parent()?.to_path_buf();
    }
    Some(Disk {
        name: dir.file_name()?.to_string_lossy().into_owned(),
        model: std::fs::read_to_string(dir.join("device/model"))
            .map(|m| m.trim().to_string())
            .unwrap_or_default(),
        stat: dir.join("stat"),
    })
}

// ── PCIe path (sysfs) ───────────────────────────────────────────────

/// A sysfs link: `16.0 GT/s PCIe` and `4`. `None` for a device without one
/// (`Unknown`, or a width of 0 on a link that is down).
fn parse_link(speed: &str, width: &str) -> Option<PcieLink> {
    let rate = speed.split_whitespace().next()?;
    let generation = match rate {
        "2.5" => 1,
        "5.0" | "5" => 2,
        "8.0" | "8" => 3,
        "16.0" | "16" => 4,
        "32.0" | "32" => 5,
        "64.0" | "64" => 6,
        _ => return None,
    };
    let width = width.trim().parse().ok().filter(|w| *w > 0)?;
    Some(PcieLink { generation, width })
}

/// Fill in the link between the card and the CPU from the card's sysfs
/// directory: the device path runs through every bridge, each with its own
/// link, and the slowest of them is what the card gets. Unlike Windows this
/// sees past a card's own switch. A switch shows as two bridges (upstream and
/// downstream port) under the one root port.
fn read_path(gpu: &mut PhysicalGpu, device_dir: &Path) {
    let Ok(path) = std::fs::canonicalize(device_dir) else {
        return;
    };
    let hops: Vec<&Path> = path
        .ancestors()
        .take_while(|p| {
            p.file_name()
                .and_then(|n| n.to_str())
                .is_some_and(|n| crate::devices::pci_address(n).is_some())
        })
        .collect();
    let slowest = |speed: &str, width: &str| {
        hops.iter()
            .filter_map(|hop| {
                let read = |f: &str| std::fs::read_to_string(hop.join(f)).ok();
                parse_link(&read(speed)?, &read(width)?)
            })
            .min_by_key(|link| link.rate())
    };
    gpu.link = slowest("current_link_speed", "current_link_width");
    gpu.max_link = slowest("max_link_speed", "max_link_width");
    // Past the card itself, a root port and two bridges per switch.
    let bridges = hops.len().saturating_sub(1) as u32;
    gpu.switches = (bridges > 0).then(|| (bridges - 1) / 2);
}

// ── amdgpu (sysfs) ──────────────────────────────────────────────────

fn read_u64(path: &Path) -> Option<u64> {
    std::fs::read_to_string(path).ok()?.trim().parse().ok()
}

fn read_hex(path: &Path) -> Option<u16> {
    let s = std::fs::read_to_string(path).ok()?;
    u16::from_str_radix(s.trim().trim_start_matches("0x"), 16).ok()
}

/// A device's name out of the system's `pci.ids`. Device lines sit under
/// their vendor's line, indented by ONE tab (two tabs are subsystems).
fn pci_name(ids: &str, vendor: u16, device: u16) -> Option<String> {
    let (vendor, device) = (format!("{vendor:04x}  "), format!("\t{device:04x}  "));
    let mut lines = ids.lines().skip_while(|l| !l.starts_with(&vendor)).skip(1);
    lines
        .by_ref()
        .take_while(|l| l.starts_with('\t') || l.starts_with('#') || l.is_empty())
        .find(|l| l.starts_with(&device))
        .map(|l| l[device.len()..].trim().to_string())
}

/// `pci.ids` names a card as `Codename [Marketing name]`; the bracketed half
/// is the one a user recognizes.
fn marketing_name(pci: &str) -> &str {
    match (pci.rfind('['), pci.rfind(']')) {
        (Some(open), Some(close)) if open < close => &pci[open + 1..close],
        _ => pci,
    }
}

struct AmdCard {
    /// `amdgpu-<PCI address>`.
    id: String,
    title: String,
    detail: String,
    gpu: Option<PhysicalGpu>,
    /// `/sys/class/drm/cardN/device`.
    dir: PathBuf,
}

/// Every amdgpu card under `drm_root` (`/sys/class/drm`), in card order.
/// `card1-DP-1` and friends are connectors, not cards.
fn amd_cards(drm_root: &Path, pci_ids: &str) -> Vec<AmdCard> {
    let Ok(entries) = std::fs::read_dir(drm_root) else {
        return Vec::new();
    };
    let mut cards: Vec<(u32, AmdCard)> = entries
        .flatten()
        .filter_map(|e| {
            let name = e.file_name().to_string_lossy().into_owned();
            let index: u32 = name.strip_prefix("card")?.parse().ok()?;
            let dir = e.path().join("device");
            let driver = std::fs::read_link(dir.join("driver")).ok()?;
            if driver.file_name()? != "amdgpu" {
                return None;
            }
            let ids = (read_hex(&dir.join("vendor")), read_hex(&dir.join("device")));
            let title = match ids {
                (Some(v), Some(d)) => pci_name(pci_ids, v, d)
                    .map(|n| marketing_name(&n).to_string())
                    .unwrap_or_else(|| format!("AMD GPU {v:04x}:{d:04x}")),
                _ => "AMD GPU".to_string(),
            };
            // The device directory is a symlink to the PCI device, whose
            // name is its address.
            let address = std::fs::canonicalize(&dir)
                .ok()
                .and_then(|p| p.file_name().map(|n| n.to_string_lossy().into_owned()))
                .unwrap_or_default();
            let detail = format!("amdgpu · {name} · {address}");
            let id = ini_safe(&format!(
                "amdgpu-{}",
                if address.is_empty() { &name } else { &address }
            ));
            Some((
                index,
                AmdCard {
                    id,
                    detail,
                    gpu: crate::devices::pci_address(&address).map(|pci| {
                        let mib = |file: &str| read_u64(&dir.join(file)).unwrap_or(0) >> 20;
                        let mut gpu = PhysicalGpu {
                            pci,
                            name: title.clone(),
                            vendor: "AMD".to_string(),
                            pci_id: match ids {
                                (Some(v), Some(d)) => format!("{v:04x}:{d:04x}"),
                                _ => String::new(),
                            },
                            vram_mib: mib("mem_info_vram_total"),
                            shared_mib: mib("mem_info_gtt_total"),
                            ..Default::default()
                        };
                        read_path(&mut gpu, &dir);
                        gpu
                    }),
                    title,
                    dir,
                },
            ))
        })
        .collect();
    cards.sort_by_key(|(index, _)| *index);
    cards.into_iter().map(|(_, card)| card).collect()
}

fn pci_ids() -> String {
    ["/usr/share/hwdata/pci.ids", "/usr/share/misc/pci.ids"]
        .iter()
        .find_map(|p| std::fs::read_to_string(p).ok())
        .unwrap_or_default()
}

/// What the system knows of every amdgpu and NVIDIA card.
pub(super) fn physical_gpus() -> Vec<PhysicalGpu> {
    let mut gpus: Vec<PhysicalGpu> = amd_cards(Path::new("/sys/class/drm"), &pci_ids())
        .into_iter()
        .filter_map(|card| card.gpu)
        .collect();
    for gpu in nvml().iter().flat_map(|nvml| &nvml.devices) {
        if let Some((bus, device)) = gpu.address {
            let pci = crate::devices::pci_text(bus, device, 0);
            let mut card = PhysicalGpu {
                name: gpu.name.clone(),
                vendor: "NVIDIA".to_string(),
                ..Default::default()
            };
            read_path(
                &mut card,
                Path::new(&format!("/sys/bus/pci/devices/0000:{pci}")),
            );
            card.pci = pci;
            gpus.push(card);
        }
    }
    gpus
}

fn amd_bars(dir: &Path) -> Vec<Bar> {
    let pair =
        |used: &str, total: &str| Some((read_u64(&dir.join(used))?, read_u64(&dir.join(total))?));
    let mut bars = Vec::new();
    if let Some(p) = read_u64(&dir.join("gpu_busy_percent")) {
        bars.push(percent_bar("GPU", p as f64));
    }
    if let Some((used, total)) = pair("mem_info_vram_used", "mem_info_vram_total") {
        bars.push(bytes_bar("VRAM", used, total));
    }
    if let Some((used, total)) = pair("mem_info_gtt_used", "mem_info_gtt_total") {
        bars.push(bytes_bar("GTT", used, total));
    }
    if let Some(p) = read_u64(&dir.join("mem_busy_percent")) {
        bars.push(percent_bar("Memory activity", p as f64));
    }
    bars
}

// ── The sampler ─────────────────────────────────────────────────────

/// The counters a rate is the difference of.
struct Counters {
    at: Instant,
    /// One per socket.
    cpu: Option<Vec<(u64, u64, u64)>>,
    disk: Option<BlockStat>,
}

pub struct Sampler {
    amd: Vec<AmdCard>,
    disk: Option<Disk>,
    cpu_title: String,
    thread_socket: HashMap<u32, usize>,
    sockets: usize,
    memory_detail: String,
    prev: Counters,
    disk_peak: f64,
    disk_wait_peak: f64,
    /// One per NVML device.
    pcie_peaks: Vec<f64>,
}

impl Sampler {
    /// Find the devices and take the baseline the first `sample()` is a
    /// difference against. `models_dir` picks the disk; `off` is the set
    /// `sample` takes.
    pub fn new(models_dir: &Path, off: &HashSet<String>) -> Self {
        let pci_ids = pci_ids();
        let cpuinfo = std::fs::read_to_string("/proc/cpuinfo").unwrap_or_default();
        let (thread_socket, sockets) = thread_sockets(Path::new("/sys/devices/system/cpu"));
        let mut s = Sampler {
            thread_socket,
            sockets,
            amd: amd_cards(Path::new("/sys/class/drm"), &pci_ids),
            disk: disk_for(models_dir),
            cpu_title: cpu_model(&cpuinfo).unwrap_or_else(|| "CPU".to_string()),
            memory_detail: memory_detail(),
            prev: Counters {
                at: Instant::now(),
                cpu: None,
                disk: None,
            },
            disk_peak: 0.0,
            disk_wait_peak: 0.0,
            pcie_peaks: vec![0.0; nvml().map_or(0, |n| n.devices.len())],
        };
        s.prev = s.counters(off);
        s
    }

    fn disk_names(disk: &Disk) -> (String, String) {
        let title = if disk.model.is_empty() {
            &disk.name
        } else {
            &disk.model
        };
        (
            title.clone(),
            format!("{} · the models directory's disk", disk.name),
        )
    }

    /// The counters of the devices that are on. One left `None` makes the
    /// next sample's rate wait for a second reading instead of spanning the
    /// time the device was off.
    fn counters(&self, off: &HashSet<String>) -> Counters {
        Counters {
            at: Instant::now(),
            cpu: (0..self.sockets)
                .any(|i| !off.contains(&cpu_id(i)))
                .then(|| std::fs::read_to_string("/proc/stat").ok())
                .flatten()
                .and_then(|s| parse_sockets(&s, &self.thread_socket, self.sockets)),
            disk: self
                .disk
                .as_ref()
                .filter(|_| !off.contains(DISK_ID))
                .and_then(|d| std::fs::read_to_string(&d.stat).ok())
                .and_then(|s| parse_block_stat(&s)),
        }
    }

    /// Everything the window shows, GPUs first. A device whose `id` is
    /// in `off` comes back as a group without bars, unread.
    pub fn sample(&mut self, off: &HashSet<String>) -> Vec<Group> {
        let now = self.counters(off);
        let secs = now.at.duration_since(self.prev.at).as_secs_f64().max(1e-3);
        let mut groups = Vec::new();

        if let Some(nvml) = nvml() {
            let devices = nvml.devices.iter().zip(&mut self.pcie_peaks).enumerate();
            for (i, (gpu, peak)) in devices {
                let detail = format!("nvidia · NVML · GPU {i}");
                let pci = gpu
                    .address
                    .map(|(bus, device)| crate::devices::pci_text(bus, device, 0));
                groups.push(
                    group(off, &gpu.id, gpu.name.clone(), detail, || {
                        nvidia_bars(nvml, gpu.handle, peak)
                    })
                    .at_pci(pci.as_deref()),
                );
            }
        }
        for card in &self.amd {
            let (title, detail) = (card.title.clone(), card.detail.clone());
            groups.push(
                group(off, &card.id, title, detail, || amd_bars(&card.dir))
                    .at_pci(card.gpu.as_ref().map(|gpu| gpu.pci.as_str())),
            );
        }

        for i in 0..self.sockets {
            let before = self.prev.cpu.as_ref().and_then(|c| c.get(i));
            let after = now.cpu.as_ref().and_then(|c| c.get(i));
            let usage = match (before, after) {
                (Some(&(b0, t0, w0)), Some(&(b1, t1, w1))) if t1 > t0 => {
                    let share =
                        |x: u64, y: u64| 100.0 * y.saturating_sub(x) as f64 / (t1 - t0) as f64;
                    Some((share(b0, b1), share(w0, w1)))
                }
                _ => None,
            };
            let title = self.cpu_title.clone();
            groups.push(group(off, &cpu_id(i), title, cpu_detail(i), || {
                usage.map_or(Vec::new(), |(busy, iowait)| {
                    vec![percent_bar("CPU", busy), percent_bar("I/O wait", iowait)]
                })
            }));
        }

        groups.push(group(
            off,
            MEMORY_ID,
            memory_title(&crate::devices::memory()),
            self.memory_detail.clone(),
            || {
                let mem =
                    parse_meminfo(&std::fs::read_to_string("/proc/meminfo").unwrap_or_default());
                vec![
                    bytes_bar("RAM", mem.total.saturating_sub(mem.available), mem.total),
                    bytes_bar("Page cache", mem.cached, mem.total),
                    bytes_bar(
                        "Swap",
                        mem.swap_total.saturating_sub(mem.swap_free),
                        mem.swap_total,
                    ),
                ]
            },
        ));

        if let Some(disk) = &self.disk {
            let (title, detail) = Self::disk_names(disk);
            let (peak, wait_peak) = (&mut self.disk_peak, &mut self.disk_wait_peak);
            let (before, after) = (self.prev.disk, now.disk);
            groups.push(group(off, DISK_ID, title, detail, || {
                let (Some(a), Some(b)) = (before, after) else {
                    return Vec::new();
                };
                let rate = |x: u64, y: u64| (y.saturating_sub(x) * SECTOR) as f64 / secs;
                let read = rate(a.read_sectors, b.read_sectors);
                let write = rate(a.write_sectors, b.write_sectors);
                // Both directions share one scale, so the two bars compare.
                *peak = peak.max(read).max(write);
                let busy = b.io_ticks.saturating_sub(a.io_ticks) as f64 / (secs * 10.0);
                vec![
                    rate_bar("Read", read, peak),
                    rate_bar("Write", write, peak),
                    percent_bar("Busy", busy),
                    wait_bar(
                        b.io_wait_ms.saturating_sub(a.io_wait_ms) as f64,
                        b.io_count.saturating_sub(a.io_count),
                        wait_peak,
                    ),
                ]
            }));
        }

        self.prev = now;
        groups
    }
}

// Everything file-shaped is tested against text or a temp directory, so no
// test here depends on the hardware it runs on.
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cpu_line_splits_into_busy_and_total() {
        let stat = "cpu  100 5 50 800 40 3 2 0 0 0\ncpu0 1 1 1 1 1 1 1 1 1 1\n";
        // total = 1000; idle 800 + iowait 40 are the not-busy part.
        assert_eq!(parse_cpu(stat), Some((160, 1000, 40)));
        assert_eq!(parse_cpu("intr 1 2 3\n"), None);
    }

    #[test]
    fn each_socket_adds_up_its_threads() {
        let stat = "cpu  9 9 9 9 9 0 0 0\ncpu0 10 0 0 80 10 0 0 0\ncpu1 20 0 0 70 10 0 0 0\n\
                    cpu2 30 0 0 60 10 0 0 0\nintr 1 2\n";
        let map = HashMap::from([(0, 0), (1, 0), (2, 1)]);
        assert_eq!(
            parse_sockets(stat, &map, 2),
            Some(vec![(30, 200, 20), (30, 100, 10)])
        );
        // One socket reads the aggregate line.
        assert_eq!(parse_sockets(stat, &map, 1), Some(vec![(27, 45, 9)]));
    }

    #[test]
    fn sockets_are_counted_from_0_whatever_the_ids() {
        let root = tempfile::tempdir().unwrap();
        for (thread, package) in [(0, "3"), (1, "3"), (2, "7")] {
            let dir = root.path().join(format!("cpu{thread}/topology"));
            std::fs::create_dir_all(&dir).unwrap();
            std::fs::write(dir.join("physical_package_id"), package).unwrap();
        }
        std::fs::create_dir_all(root.path().join("cpufreq")).unwrap();
        let (map, sockets) = thread_sockets(root.path());
        assert_eq!(sockets, 2);
        assert_eq!(map, HashMap::from([(0, 0), (1, 0), (2, 1)]));
        assert_eq!(thread_sockets(&root.path().join("none")).1, 1);
    }

    #[test]
    fn the_processor_is_read_from_sysfs() {
        let root = tempfile::tempdir().unwrap();
        let put = |path: &str, text: &str| {
            let path = root.path().join(path);
            std::fs::create_dir_all(path.parent().unwrap()).unwrap();
            std::fs::write(path, text).unwrap();
        };
        // One socket, one core, two threads sharing every cache.
        for thread in ["cpu0", "cpu1"] {
            put(&format!("cpu/{thread}/topology/physical_package_id"), "0\n");
            put(&format!("cpu/{thread}/topology/core_id"), "0\n");
            for (index, level, kind, size) in [
                ("index0", "1", "Data", "48K"),
                ("index1", "1", "Instruction", "32K"),
                ("index2", "2", "Unified", "1024K"),
                ("index3", "3", "Unified", "32M"),
            ] {
                put(&format!("cpu/{thread}/cache/{index}/level"), level);
                put(&format!("cpu/{thread}/cache/{index}/type"), kind);
                put(&format!("cpu/{thread}/cache/{index}/size"), size);
                put(
                    &format!("cpu/{thread}/cache/{index}/shared_cpu_list"),
                    "0-1",
                );
            }
        }
        put("cpu/cpufreq/policy0", "not a thread");
        put("cpu/cpu0/cpufreq/cpuinfo_max_freq", "5600000\n");
        put("cpu/cpu0/cpufreq/amd_pstate_nominal_freq", "4400000\n");
        put("node/node0/x", "");
        let mut cpu = Cpu::default();
        read_cpu_sysfs(
            &root.path().join("cpu"),
            &root.path().join("node"),
            &mut cpu,
        );
        assert_eq!(
            cpu,
            Cpu {
                sockets: Some(1),
                cores: Some(1),
                threads: Some(2),
                base_mhz: Some(4400),
                max_mhz: Some(5600),
                l1_kib: Some(80),
                l2_kib: Some(1024),
                l3_kib: Some(32 << 10),
                numa_nodes: Some(1),
                ..Default::default()
            }
        );
    }

    #[test]
    fn modules_are_read_from_udev_s_record() {
        let record = "I:123\nE:MEMORY_ARRAY_LOCATION=System Board\n\
            E:MEMORY_DEVICE_0_PRESENT=1\nE:MEMORY_DEVICE_0_SIZE=34359738368\n\
            E:MEMORY_DEVICE_0_TYPE=DDR5\nE:MEMORY_DEVICE_0_SPEED_MTS=6400\n\
            E:MEMORY_DEVICE_0_CONFIGURED_SPEED_MTS=6000\n\
            E:MEMORY_DEVICE_0_CONFIGURED_VOLTAGE=1100\n\
            E:MEMORY_DEVICE_0_MANUFACTURER=CORSAIR\n\
            E:MEMORY_DEVICE_0_PART_NUMBER=CMK64\n\
            E:MEMORY_DEVICE_0_BANK_LOCATOR=P0 CHANNEL A\n\
            E:MEMORY_DEVICE_0_LOCATOR=DIMM 1\n\
            E:MEMORY_DEVICE_1_PRESENT=0\nE:MEMORY_DEVICE_1_LOCATOR=DIMM 2\n";
        assert_eq!(
            parse_udev_memory(record),
            [MemoryModule {
                maker: "CORSAIR".to_string(),
                part: "CMK64".to_string(),
                slot: "P0 CHANNEL A / DIMM 1".to_string(),
                bytes: 32 << 30,
                kind: Some("DDR5".to_string()),
                configured_mts: Some(6000),
                rated_mts: Some(6400),
                configured_millivolts: Some(1100),
            }]
        );
        assert!(parse_udev_memory("").is_empty());
    }

    #[test]
    fn meminfo_reads_kilobytes_as_bytes_and_ignores_lookalikes() {
        let m = parse_meminfo(
            "MemTotal:       62318280 kB\nMemAvailable:   48633928 kB\n\
             Cached:         41600396 kB\nSwapCached:            8 kB\n\
             SwapTotal:       8388604 kB\nSwapFree:        8388596 kB\n",
        );
        assert_eq!(m.total, 62318280 * 1024);
        assert_eq!(m.available, 48633928 * 1024);
        // SwapCached must not land in `cached`.
        assert_eq!(m.cached, 41600396 * 1024);
        assert_eq!(m.swap_total - m.swap_free, 8 * 1024);
    }

    #[test]
    fn block_stat_picks_sectors_and_io_ticks() {
        let s = "  522 0 13216 41 7 0 900 3 0 40 41 0 0 0 0 0 0\n";
        assert_eq!(
            parse_block_stat(s),
            Some(BlockStat {
                read_sectors: 13216,
                write_sectors: 900,
                io_ticks: 40,
                io_count: 529,
                io_wait_ms: 44,
            })
        );
        assert_eq!(parse_block_stat("1 2 3"), None);
    }

    #[test]
    fn st_dev_splits_like_glibc() {
        // nvme1n1p1 is 259:1 on the development machine.
        assert_eq!(major_minor((259 << 8) | 1), (259, 1));
        // A minor above 255 spills into the high bits.
        assert_eq!(major_minor((8 << 8) | 0x10 | (1 << 20)), (8, 0x110));
    }

    #[test]
    fn mount_source_takes_the_deepest_mount_and_unescapes_it() {
        let info = "\
22 1 0:20 / / rw shared:1 - btrfs /dev/nvme0n1p3 rw,subvol=/root\n\
40 22 259:1 / /mnt/data rw shared:9 - ntfs3 /dev/nvme1n1p1 rw\n\
41 22 8:1 / /mnt/my\\040disk rw - ext4 /dev/sda1 rw\n";
        let src = |p: &str| mount_source(info, Path::new(p));
        assert_eq!(src("/home/u/models").as_deref(), Some("/dev/nvme0n1p3"));
        assert_eq!(src("/mnt/data/models").as_deref(), Some("/dev/nvme1n1p1"));
        assert_eq!(src("/mnt/my disk/models").as_deref(), Some("/dev/sda1"));
        // A sibling that merely shares the name prefix is not under the mount.
        assert_eq!(src("/mnt/database").as_deref(), Some("/dev/nvme0n1p3"));
    }

    // An R9700 behind the chipset: its own link reads 5.0 x16, and the slot's
    // four lanes show only further up the path.
    #[test]
    fn the_link_is_the_slowest_hop_up_to_the_cpu() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path().join("pci0000:00");
        let chain = [
            "0000:00:02.1",
            "0000:0c:00.0",
            "0000:0d:00.0",
            "0000:14:00.0",
        ];
        let hops = [
            ("16.0 GT/s PCIe", "4"),
            ("16.0 GT/s PCIe", "4"),
            ("32.0 GT/s PCIe", "16"),
            ("32.0 GT/s PCIe", "16"),
        ];
        let mut at = root;
        for (name, (speed, width)) in chain.iter().zip(hops) {
            at = at.join(name);
            std::fs::create_dir_all(&at).unwrap();
            for kind in ["current", "max"] {
                std::fs::write(at.join(format!("{kind}_link_speed")), speed).unwrap();
                std::fs::write(at.join(format!("{kind}_link_width")), width).unwrap();
            }
        }
        let mut gpu = PhysicalGpu::default();
        read_path(&mut gpu, &at);
        let four_lanes = PcieLink {
            generation: 4,
            width: 4,
        };
        assert_eq!(
            (gpu.link, gpu.max_link),
            (Some(four_lanes), Some(four_lanes))
        );
        assert_eq!(gpu.switches, Some(1));

        assert_eq!(parse_link("Unknown", "4"), None);
        assert_eq!(parse_link("16.0 GT/s PCIe", "0"), None);
    }

    #[test]
    fn pci_name_stays_inside_its_vendor() {
        let ids = "\
1002  Advanced Micro Devices, Inc. [AMD/ATI]\n\
\t13c0  Granite Ridge [Radeon Graphics]\n\
\t\t1458 d000  A subsystem line\n\
\t7551  Navi 48 [Radeon AI PRO R9700]\n\
10de  NVIDIA Corporation\n\
\t13c0  GM204 [GeForce GTX 980]\n";
        assert_eq!(
            pci_name(ids, 0x1002, 0x7551).as_deref(),
            Some("Navi 48 [Radeon AI PRO R9700]")
        );
        // 13c0 exists under both vendors.
        assert_eq!(
            pci_name(ids, 0x10de, 0x13c0).as_deref(),
            Some("GM204 [GeForce GTX 980]")
        );
        // 7551 is AMD's: it must not be found by reading on past NVIDIA's block.
        assert_eq!(pci_name(ids, 0x10de, 0x7551), None);
        assert_eq!(
            marketing_name("Navi 48 [Radeon AI PRO R9700]"),
            "Radeon AI PRO R9700"
        );
        assert_eq!(marketing_name("Some Card"), "Some Card");
    }

    // Against the real machine, but only through what every Linux has: with
    // CPU and memory off the sample still lists them, empty, and a device
    // switched back on needs one more reading before its rate shows.
    #[test]
    fn sample_skips_what_is_off_and_resumes_a_rate_one_reading_later() {
        let none = HashSet::new();
        let mut s = Sampler::new(Path::new("/"), &none);
        let off = HashSet::from([CPU_ID.to_string(), MEMORY_ID.to_string()]);
        let find =
            |groups: &[Group], id: &str| groups.iter().find(|g| g.id == id).cloned().unwrap();

        let groups = s.sample(&off);
        for id in &off {
            let g = find(&groups, id);
            assert_eq!((g.enabled, g.bars.len()), (false, 0), "{id:?}");
        }
        assert!(s.prev.cpu.is_none(), "/proc/stat was not read");
        // Every id is unique: it is the only thing a checkbox acts on.
        let ids: HashSet<&str> = groups.iter().map(|g| g.id.as_str()).collect();
        assert_eq!(ids.len(), groups.len());

        // Back on: memory is instantaneous, the CPU rate has no baseline yet.
        let groups = s.sample(&none);
        assert_eq!(find(&groups, MEMORY_ID).bars.len(), 3);
        let cpu = find(&groups, CPU_ID);
        assert_eq!((cpu.enabled, cpu.bars.len()), (true, 0));
        std::thread::sleep(std::time::Duration::from_millis(30));
        assert_eq!(find(&s.sample(&none), CPU_ID).bars.len(), 2);
    }

    /// A fake `/sys/class/drm` with one card of each kind.
    fn fake_drm(root: &Path) {
        for (card, driver, address) in [
            ("card1", "nvidia", "0000:01:00.0"),
            ("card2", "amdgpu", "0000:12:00.0"),
            ("card3", "amdgpu", "0000:7b:00.0"),
        ] {
            // As in sysfs: `device` is a symlink to the PCI device, whose
            // directory name is its address.
            let pci = root.join("pci").join(address);
            std::fs::create_dir_all(&pci).unwrap();
            std::fs::create_dir_all(root.join(card)).unwrap();
            let dev = root.join(card).join("device");
            std::os::unix::fs::symlink(&pci, &dev).unwrap();
            let target = root.join("drivers").join(driver);
            std::fs::create_dir_all(&target).unwrap();
            std::os::unix::fs::symlink(&target, dev.join("driver")).unwrap();
            std::fs::write(dev.join("vendor"), "0x1002\n").unwrap();
        }
        // A connector directory: named like a card, but not one.
        std::fs::create_dir_all(root.join("card2-DP-1")).unwrap();
        let write = |card: &str, file: &str, value: &str| {
            std::fs::write(root.join(card).join("device").join(file), value).unwrap();
        };
        write("card2", "device", "0x7551\n");
        write("card2", "gpu_busy_percent", "82\n");
        write("card2", "mem_busy_percent", "86\n");
        write("card2", "mem_info_vram_used", "17179869184\n");
        write("card2", "mem_info_vram_total", "34359738368\n");
        write("card2", "mem_info_gtt_used", "0\n");
        write("card2", "mem_info_gtt_total", "34359738368\n");
        // The integrated one: no mem_busy_percent, no GTT total.
        write("card3", "device", "0x13c0\n");
        write("card3", "gpu_busy_percent", "0\n");
        write("card3", "mem_info_gtt_used", "5\n");
    }

    #[test]
    fn amd_cards_are_found_by_driver_and_missing_files_only_drop_their_bar() {
        let dir = tempfile::tempdir().unwrap();
        fake_drm(dir.path());
        let ids = "1002  AMD\n\t7551  Navi 48 [Radeon AI PRO R9700]\n";
        let cards = amd_cards(dir.path(), ids);

        // The nvidia card and the connector are skipped; card order is kept.
        let titles: Vec<&str> = cards.iter().map(|c| c.title.as_str()).collect();
        assert_eq!(titles, ["Radeon AI PRO R9700", "AMD GPU 1002:13c0"]);
        assert_eq!(cards[0].detail, "amdgpu · card2 · 0000:12:00.0");
        let ids: Vec<&str> = cards.iter().map(|c| c.id.as_str()).collect();
        assert_eq!(ids, ["amdgpu-0000:12:00.0", "amdgpu-0000:7b:00.0"]);

        let full = amd_bars(&cards[0].dir);
        let labels: Vec<&str> = full.iter().map(|b| b.label.as_str()).collect();
        assert_eq!(labels, ["GPU", "VRAM", "GTT", "Memory activity"]);
        assert_eq!(full[0].text, "82 %");
        assert_eq!(full[1].text, "16.0 / 32.0 GiB");
        assert_eq!(full[1].fraction, 0.5);

        // A used value without its total is not a bar.
        let partial = amd_bars(&cards[1].dir);
        let labels: Vec<&str> = partial.iter().map(|b| b.label.as_str()).collect();
        assert_eq!(labels, ["GPU"]);
    }
}
