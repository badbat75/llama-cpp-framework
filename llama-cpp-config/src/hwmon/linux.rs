//! The Linux sources of the HW Monitor: sysfs for amdgpu and the disk,
//! `/proc` for the CPU and memory. The table of what each bar reads is in the
//! parent module's header.

use super::*;
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

/// `(busy, total)` jiffies from the aggregate `cpu` line of `/proc/stat`.
/// Idle and iowait are the not-busy part; guest time is already inside user.
fn parse_cpu(stat: &str) -> Option<(u64, u64)> {
    let line = stat.lines().find(|l| l.starts_with("cpu "))?;
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
    Some((total - v[3] - v[4], total))
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
                    title,
                    detail,
                    dir,
                },
            ))
        })
        .collect();
    cards.sort_by_key(|(index, _)| *index);
    cards.into_iter().map(|(_, card)| card).collect()
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
    cpu: Option<(u64, u64)>,
    disk: Option<BlockStat>,
}

pub struct Sampler {
    amd: Vec<AmdCard>,
    disk: Option<Disk>,
    cpu_title: String,
    threads: usize,
    prev: Counters,
    disk_peak: f64,
    /// One per NVML device.
    pcie_peaks: Vec<f64>,
}

impl Sampler {
    /// Find the devices and take the baseline the first `sample()` is a
    /// difference against. `models_dir` picks the disk; `off` is the set
    /// `sample` takes.
    pub fn new(models_dir: &Path, off: &HashSet<String>) -> Self {
        let pci_ids = ["/usr/share/hwdata/pci.ids", "/usr/share/misc/pci.ids"]
            .iter()
            .find_map(|p| std::fs::read_to_string(p).ok())
            .unwrap_or_default();
        let cpuinfo = std::fs::read_to_string("/proc/cpuinfo").unwrap_or_default();
        let mut s = Sampler {
            amd: amd_cards(Path::new("/sys/class/drm"), &pci_ids),
            disk: disk_for(models_dir),
            cpu_title: cpu_model(&cpuinfo).unwrap_or_else(|| "CPU".to_string()),
            threads: std::thread::available_parallelism().map_or(0, |n| n.get()),
            prev: Counters {
                at: Instant::now(),
                cpu: None,
                disk: None,
            },
            disk_peak: 0.0,
            pcie_peaks: vec![0.0; nvml().map_or(0, |n| n.devices.len())],
        };
        s.prev = s.counters(off);
        s
    }

    fn cpu_detail(&self) -> String {
        format!("{} threads", self.threads)
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
            cpu: (!off.contains(CPU_ID))
                .then(|| std::fs::read_to_string("/proc/stat").ok())
                .flatten()
                .and_then(|s| parse_cpu(&s)),
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
                groups.push(group(off, &gpu.id, gpu.name.clone(), detail, || {
                    nvidia_bars(nvml, gpu.handle, peak)
                }));
            }
        }
        for card in &self.amd {
            let (title, detail) = (card.title.clone(), card.detail.clone());
            groups.push(group(off, &card.id, title, detail, || amd_bars(&card.dir)));
        }

        let cpu = match (self.prev.cpu, now.cpu) {
            (Some((b0, t0)), Some((b1, t1))) if t1 > t0 => {
                Some(100.0 * b1.saturating_sub(b0) as f64 / (t1 - t0) as f64)
            }
            _ => None,
        };
        let (title, detail) = (self.cpu_title.clone(), self.cpu_detail());
        groups.push(group(off, CPU_ID, title, detail, || {
            cpu.map(|p| percent_bar("CPU", p)).into_iter().collect()
        }));

        groups.push(group(
            off,
            MEMORY_ID,
            "Memory".to_string(),
            String::new(),
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
            let peak = &mut self.disk_peak;
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
        assert_eq!(parse_cpu(stat), Some((160, 1000)));
        assert_eq!(parse_cpu("intr 1 2 3\n"), None);
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
                io_ticks: 40
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
        assert_eq!(find(&s.sample(&none), CPU_ID).bars.len(), 1);
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
