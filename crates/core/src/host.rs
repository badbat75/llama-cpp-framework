//! What the node's own host is, read once per call and answered to the caller
//! the gate admits (ARCHITECTURE.md (1) "The node inventory"): the total
//! physical memory and the logical CPUs the manager may use. The manager is
//! the reader, on both of its systems; no caller reads the machine itself,
//! because the bff runs unprivileged (`PrivateDevices=yes`) and, across
//! machines, only the node's manager sees its hardware.
//!
//! A reading that fails answers 0, which is the host's own "no figure": 0 MiB
//! is no RAM to show and 0 is no thread to cap at, so a caller that caps
//! something at the count (the web form's thread limit) keeps its own. Nothing
//! is guessed at, and a test never reads the host: every figure a test sees
//! came from a seam.

#[cfg(windows)]
use windows_sys::Win32::Devices::Properties::{
    DEVPKEY_Device_Address, DEVPKEY_Device_BusNumber, DEVPKEY_Device_DriverProvider,
    DEVPKEY_Device_DriverVersion, DEVPKEY_NAME,
};
#[cfg(windows)]
use windows_sys::Win32::Foundation::DEVPROPKEY;

/// The host's total physical memory, in whole MiB, rounded down: the inventory
/// shows a size and budgets nothing (the RAM budget is a later slice).
pub fn ram_mib() -> u64 {
    read_ram_mib()
}

/// The logical CPUs the process may use: `std::thread::available_parallelism`,
/// which honours an affinity mask and a cgroup quota, so it is the width the
/// manager can actually fill and not the count the box was sold with. A count
/// no uint32 holds saturates at the largest uint32; a host that answers
/// nothing is 0.
pub fn cpu_threads() -> u32 {
    std::thread::available_parallelism()
        .map(|n| u32::try_from(n.get()).unwrap_or(u32::MAX))
        .unwrap_or_default()
}

/// The operating system of the machine the reader runs on, the shape
/// `ARCHITECTURE.md` ("About and node info") sends: every field is what the
/// host answered, `None` when it answered nothing.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OsInfo {
    /// os-release's `NAME` (`Ubuntu`).
    pub name: Option<String>,
    /// os-release's `VERSION_ID` (`26.04`), never its `VERSION`.
    pub version: Option<String>,
    /// The kernel's release: `uname(2)` on Linux, the build number on Windows.
    pub kernel: Option<String>,
}

/// One display device of the host with the kernel driver bound to it, the row
/// shape `ARCHITECTURE.md` ("About and node info") sends.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GpuDriver {
    /// The PCI bus id, as sysfs names the device (`0000:03:00.0`).
    pub pci: String,
    /// The PCI vendor id as 4 lower-case hex digits (`1002`).
    pub vendor: Option<String>,
    /// The product name. A kernel row on Linux has none: the Vulkan join (AN2)
    /// fills it from `deviceName`.
    pub name: Option<String>,
    /// The kernel driver bound to the device (`amdgpu`), `None` when unbound.
    pub driver: Option<String>,
    /// `/sys/module/<driver>/version`, `None` when the module sets no version
    /// (an in-tree amdgpu, i915, xe) or the driver is unknown.
    pub version: Option<String>,
}

/// The operating system of the machine, read under `root` (the real host is `/`,
/// a test is a fixture tree): on Linux `NAME` and `VERSION_ID` of `/etc/os-release`,
/// falling back to `/usr/lib/os-release` only when `/etc` has none (os-release(5),
/// never a mix of the two), and the kernel's release from `uname(2)`. The kernel
/// is never read out of a file. A fact the host did not answer is `None`.
pub fn os(root: &std::path::Path) -> OsInfo {
    read_os(root)
}

/// The host's display devices, read under `root`: on Linux every
/// `/sys/bus/pci/devices/*` whose `class` is a display controller, with the
/// kernel driver bound to it and that module's version. The rows come in bus id
/// order, at most `MAX_GPU_ROWS` of them: a device the reader cannot judge is no
/// row, and a value it cannot read is `None` in a row it keeps.
pub fn gpu_drivers(root: &std::path::Path) -> Vec<GpuDriver> {
    let mut rows = display_rows(root);
    rows.sort_by(|a, b| a.pci.cmp(&b.pci));
    rows.truncate(MAX_GPU_ROWS);
    rows
}

/// Linux: `MemTotal`, in MiB.
#[cfg(unix)]
fn read_ram_mib() -> u64 {
    mib_from_meminfo(&std::fs::read_to_string("/proc/meminfo").unwrap_or_default())
}

/// Windows: `ullTotalPhys` of `GlobalMemoryStatusEx`, in MiB.
#[cfg(windows)]
fn read_ram_mib() -> u64 {
    use windows_sys::Win32::System::SystemInformation::{GlobalMemoryStatusEx, MEMORYSTATUSEX};
    let mut status = MEMORYSTATUSEX {
        // The call reads the struct's size here, and refuses the call without it.
        dwLength: u32::try_from(std::mem::size_of::<MEMORYSTATUSEX>())
            .expect("a struct of 72 bytes fits a uint32"),
        ..MEMORYSTATUSEX::default()
    };
    // SAFETY: `status` is the struct the call fills, of the size it declares in
    // `dwLength`; its fields are read only after a successful call.
    if unsafe { GlobalMemoryStatusEx(&mut status) } == 0 {
        return 0;
    }
    status.ullTotalPhys / (1 << 20)
}

/// `MemTotal` of a `/proc/meminfo` text, in MiB: proc_meminfo(5) writes the
/// line in kilobytes (`MemTotal: 32778324 kB`), so the number is divided by
/// 1024. A text with no such line, and a number no parse reads, are 0.
#[cfg(unix)]
fn mib_from_meminfo(text: &str) -> u64 {
    text.lines()
        .find_map(|line| {
            let (key, value) = line.split_once(':')?;
            (key.trim() == "MemTotal")
                .then(|| value.split_whitespace().next()?.trim().parse::<u64>().ok())?
        })
        .unwrap_or_default()
        / 1024
}

/// At most this many GPU rows: a node has a handful of GPUs, and a list the
/// reader caps cannot be a reply that grows without bound
/// (`ARCHITECTURE.md` "About and node info"). The rows kept are the lowest bus
/// ids, the reader sorting before it truncates.
const MAX_GPU_ROWS: usize = 32;

/// The largest an os-release file is: the keys read are a few lines, and a file
/// over this is not an os-release. The bound is the file's length, the versions
/// files' rule, so a reader that took the first bytes of a longer file would
/// answer a name out of a file it must refuse.
#[cfg(unix)]
const OS_RELEASE_MAX_BYTES: usize = 4096;

/// The largest one sysfs value is, the versions files' cap: a driver version is
/// a line, and a longer file is no value.
#[cfg(unix)]
const SYSFS_VALUE_MAX_BYTES: usize = 128;

/// Linux: the OS name, the version and the kernel.
#[cfg(unix)]
fn read_os(root: &std::path::Path) -> OsInfo {
    let release = read_os_release(root);
    OsInfo {
        name: release
            .as_deref()
            .and_then(|text| os_release_value(text, "NAME")),
        version: release
            .as_deref()
            .and_then(|text| os_release_value(text, "VERSION_ID")),
        kernel: kernel_release(),
    }
}

/// Windows: `RtlGetVersion` (ntdll), whose answer does not depend on the
/// caller's manifest. The name is the one the design sends, and the version and
/// the kernel are the same `<major>.<minor>.<build>`: Windows has no kernel
/// release of its own to show beside the build.
#[cfg(windows)]
fn read_os(_root: &std::path::Path) -> OsInfo {
    use windows_sys::Wdk::System::SystemServices::RtlGetVersion;
    use windows_sys::Win32::Foundation::STATUS_SUCCESS;
    use windows_sys::Win32::System::SystemInformation::OSVERSIONINFOW;
    let mut info = OSVERSIONINFOW {
        // The call reads the struct's size here, and refuses the call without it.
        dwOSVersionInfoSize: u32::try_from(std::mem::size_of::<OSVERSIONINFOW>())
            .expect("a struct of 276 bytes fits a uint32"),
        ..OSVERSIONINFOW::default()
    };
    // SAFETY: `info` is the struct the call fills, of the size it declares in
    // `dwOSVersionInfoSize`; its fields are read only after a successful call.
    if unsafe { RtlGetVersion(&mut info) } != STATUS_SUCCESS {
        return OsInfo {
            name: None,
            version: None,
            kernel: None,
        };
    }
    let build = format!(
        "{}.{}.{}",
        info.dwMajorVersion, info.dwMinorVersion, info.dwBuildNumber
    );
    OsInfo {
        name: Some(OS_NAME.to_string()),
        version: Some(build.clone()),
        kernel: Some(build),
    }
}

/// The name Windows answers, where a Linux distro answers `Ubuntu`: data the
/// reader sends, not a text of the interface (the web UI names it).
#[cfg(windows)]
const OS_NAME: &str = "Windows";

/// The text of a file of at most `max` bytes: `None` when it is missing,
/// unreadable, longer than `max`, or not UTF-8. `max + 1` bytes are read: the
/// extra one is what shows a file over the cap without reading all of it.
#[cfg(unix)]
fn read_capped(path: &std::path::Path, max: usize) -> Option<String> {
    use std::io::Read;
    let file = std::fs::File::open(path).ok()?;
    let mut bytes: Vec<u8> = Vec::new();
    if file.take((max + 1) as u64).read_to_end(&mut bytes).is_err() {
        return None;
    }
    if bytes.len() > max {
        return None;
    }
    String::from_utf8(bytes).ok()
}

/// The os-release text of the machine under `root`: `/etc`'s when it is there
/// (the distro's own file carries the split's overrides), `/usr/lib`'s only when
/// `/etc` has none. The bytes of a file that is there and cannot be read are
/// `None`, and the other file is not looked at: the two are never mixed.
#[cfg(unix)]
fn read_os_release(root: &std::path::Path) -> Option<String> {
    let etc = root.join("etc/os-release");
    if etc.is_file() {
        return read_capped(&etc, OS_RELEASE_MAX_BYTES);
    }
    read_capped(&root.join("usr/lib/os-release"), OS_RELEASE_MAX_BYTES)
}

/// The value of one `key` of an os-release text: the whole key on the left of
/// the `=`, so `LOGO` is not `NAME` and `VERSION` is not `VERSION_ID`, and a line
/// opening with `#` is a comment and answers nothing. The first line that carries
/// the key with a value is the one read.
#[cfg(unix)]
fn os_release_value(text: &str, key: &str) -> Option<String> {
    text.lines().find_map(|line| {
        let line = line.trim_start();
        if line.starts_with('#') {
            return None;
        }
        let (name, value) = line.split_once('=')?;
        if name.trim() != key {
            return None;
        }
        unquoted(value)
    })
}

/// The value of a shell assignment: the spaces around it and a pair of shell
/// quotes dropped, and a value that is empty or is not printable ASCII is no
/// value. The shell quoting is os-release's, so Linux's reader is its only user.
#[cfg(unix)]
fn unquoted(value: &str) -> Option<String> {
    let text = value.trim().trim_matches(|c| c == '"' || c == '\'').trim();
    printable_value(text)
}

/// A string the host answered, as a value: trimmed (sysfs pads a value with a
/// trailing break), not empty, printable ASCII, the versions files' rule.
fn printable_value(text: &str) -> Option<String> {
    let text = text.trim();
    if text.is_empty() || !text.bytes().all(|b| b.is_ascii_graphic() || b == b' ') {
        return None;
    }
    Some(text.to_string())
}

/// Linux: the kernel's release, `uname(2)`'s own string.
#[cfg(unix)]
fn kernel_release() -> Option<String> {
    let name = nix::sys::utsname::uname().ok()?;
    printable_value(&name.release().to_string_lossy())
}

/// One sysfs value: the file's bytes within `SYSFS_VALUE_MAX_BYTES`, trimmed and
/// printable ASCII, `None` for anything else.
#[cfg(unix)]
fn sysfs_value(path: &std::path::Path) -> Option<String> {
    printable_value(&read_capped(path, SYSFS_VALUE_MAX_BYTES)?)
}

/// A PCI class code is `0x` and six hex digits, the top byte the base class:
/// `0x03` is a display controller, the VGA class and the 3D controller class
/// (PCI-SIG, PCI Code and ID Assignment Specification). A code no parse reads is
/// no display device.
#[cfg(unix)]
fn is_display_class(class: &str) -> bool {
    let digits = strip_hex_prefix(class);
    u32::from_str_radix(digits, 16).is_ok_and(|code| code >> 16 == 0x03)
}

/// A PCI vendor id, as 4 lower-case hex digits (`1002`): sysfs writes `0x10DE`,
/// the row answers `10de`. Anything that is not a 4-digit hex id is no vendor.
fn vendor_id(text: &str) -> Option<String> {
    let digits = strip_hex_prefix(text);
    u16::from_str_radix(digits, 16)
        .ok()
        .map(|id| format!("{id:04x}"))
}

/// `0x` dropped, case left to the parse.
fn strip_hex_prefix(text: &str) -> &str {
    text.strip_prefix("0x")
        .or_else(|| text.strip_prefix("0X"))
        .unwrap_or(text)
}

/// Linux: the host's display devices, every entry of `/sys/bus/pci/devices` that
/// carries a display class. The entry is a symlink to the device node, so the
/// class file is what says whether this is a device to judge at all; a stray
/// file, and a device dir with no `class`, answer nothing.
#[cfg(unix)]
fn display_rows(root: &std::path::Path) -> Vec<GpuDriver> {
    let entries = match std::fs::read_dir(root.join("sys/bus/pci/devices")) {
        Ok(entries) => entries,
        Err(_) => return Vec::new(),
    };
    let mut rows = Vec::new();
    for entry in entries.flatten() {
        let dev = entry.path();
        let class = match sysfs_value(&dev.join("class")) {
            Some(class) => class,
            None => continue,
        };
        if !is_display_class(&class) {
            continue;
        }
        let driver = bound_driver(&dev);
        let version = driver
            .as_deref()
            .and_then(|driver| sysfs_value(&root.join("sys/module").join(driver).join("version")));
        rows.push(GpuDriver {
            pci: entry.file_name().to_string_lossy().into_owned(),
            vendor: sysfs_value(&dev.join("vendor")).and_then(|text| vendor_id(&text)),
            // A kernel row has no product name: the Vulkan join (AN2) fills it.
            name: None,
            driver,
            version,
        });
    }
    rows
}

/// The kernel driver bound to a PCI device: sysfs' `driver` symlink names the
/// module it is bound to, and sysfs writes the link relative
/// (`../../../module/amdgpu`). No link is an unbound device; a link aimed at a
/// module dir that is not there still names its driver.
#[cfg(unix)]
fn bound_driver(dev: &std::path::Path) -> Option<String> {
    let target = std::fs::read_link(dev.join("driver")).ok()?;
    printable_value(&target.file_name()?.to_string_lossy())
}

/// Windows: the device class of a display adapter, `DEVPKEY_Device_ClassGuid`'s
/// value for the display class (Microsoft Learn, Device Classes): the filter
/// `CM_Get_Device_ID_ListW` takes for "the display devices".
#[cfg(windows)]
const DISPLAY_CLASS_GUID: &str = "{4d36e968-e325-11ce-bfc1-08002be10318}";

/// Windows: the host's display devices, one row per present device of the
/// display class. The driver and its version are the INF's provider and
/// `DriverVer`, the name the device's own, and the bus id is built from the
/// node's bus number and address, the domain taken as 0 (unverified on hardware).
#[cfg(windows)]
fn display_rows(_root: &std::path::Path) -> Vec<GpuDriver> {
    let mut rows = Vec::new();
    for (node, id) in display_nodes() {
        // A node with no bus number is not a PCI device: no bus id to send.
        let Some(bus) = node_u32(node, &DEVPKEY_Device_BusNumber) else {
            continue;
        };
        let address = node_u32(node, &DEVPKEY_Device_Address).unwrap_or_default();
        rows.push(GpuDriver {
            pci: format!(
                "0000:{bus:02x}:{:02x}.{:x}",
                address >> 16,
                address & 0xffff
            ),
            vendor: vendor_of_id(&id),
            name: node_text(node, &DEVPKEY_NAME),
            driver: node_text(node, &DEVPKEY_Device_DriverProvider),
            version: node_text(node, &DEVPKEY_Device_DriverVersion),
        });
    }
    rows
}

/// The present devices of the display class: their device nodes and their
/// instance ids, which carry the PCI vendor id.
#[cfg(windows)]
fn display_nodes() -> Vec<(u32, String)> {
    use windows_sys::Win32::Devices::DeviceAndDriverInstallation::{
        CM_Get_Device_ID_ListW, CM_Get_Device_ID_List_SizeW, CM_Locate_DevNodeW,
        CM_GETIDLIST_FILTER_CLASS, CM_GETIDLIST_FILTER_PRESENT, CM_LOCATE_DEVNODE_NORMAL,
        CR_SUCCESS,
    };
    let filter = wide(DISPLAY_CLASS_GUID);
    let flags = CM_GETIDLIST_FILTER_CLASS | CM_GETIDLIST_FILTER_PRESENT;
    let mut len = 0u32;
    // SAFETY: the sizing call fills a local; the list call gets a buffer of that
    // many characters, and both take a NUL-terminated filter.
    let listed = unsafe {
        CM_Get_Device_ID_List_SizeW(&mut len, filter.as_ptr(), flags) == CR_SUCCESS && len > 0
    };
    if !listed {
        return Vec::new();
    }
    let mut ids = vec![0u16; len as usize];
    if unsafe { CM_Get_Device_ID_ListW(filter.as_ptr(), ids.as_mut_ptr(), len, flags) }
        != CR_SUCCESS
    {
        return Vec::new();
    }
    ids.split(|c| *c == 0)
        .filter(|id| !id.is_empty())
        .filter_map(|id| {
            let id: Vec<u16> = id.iter().copied().chain(Some(0)).collect();
            let mut node = 0u32;
            // SAFETY: `id` is NUL-terminated, and the out-pointer a live local.
            (unsafe { CM_Locate_DevNodeW(&mut node, id.as_ptr(), CM_LOCATE_DEVNODE_NORMAL) }
                == CR_SUCCESS)
                .then(|| (node, from_wide(&id)))
        })
        .collect()
}

/// A device node's text property, as a value: printable ASCII within the buffer
/// the call fills, `None` for a node that answers none of this type.
#[cfg(windows)]
fn node_text(node: u32, key: &DEVPROPKEY) -> Option<String> {
    use windows_sys::Win32::Devices::DeviceAndDriverInstallation::{
        CM_Get_DevNode_PropertyW, CR_SUCCESS,
    };
    use windows_sys::Win32::Devices::Properties::DEVPROP_TYPE_STRING;
    let mut buf = [0u16; 128];
    let (mut kind, mut size) = (0u32, std::mem::size_of_val(&buf) as u32);
    // SAFETY: the buffer is the local array, passed with its size in bytes.
    let status = unsafe {
        CM_Get_DevNode_PropertyW(
            node,
            key,
            &mut kind,
            buf.as_mut_ptr() as *mut u8,
            &mut size,
            0,
        )
    };
    (status == CR_SUCCESS && kind == DEVPROP_TYPE_STRING)
        .then(|| from_wide(&buf))
        .and_then(|text| printable_value(&text))
}

/// A device node's `u32` property.
#[cfg(windows)]
fn node_u32(node: u32, key: &DEVPROPKEY) -> Option<u32> {
    use windows_sys::Win32::Devices::DeviceAndDriverInstallation::{
        CM_Get_DevNode_PropertyW, CR_SUCCESS,
    };
    use windows_sys::Win32::Devices::Properties::DEVPROP_TYPE_UINT32;
    let (mut kind, mut value, mut size) = (0u32, 0u32, std::mem::size_of::<u32>() as u32);
    // SAFETY: the buffer is the local u32, passed with its size in bytes.
    let status = unsafe {
        CM_Get_DevNode_PropertyW(
            node,
            key,
            &mut kind,
            &mut value as *mut u32 as *mut u8,
            &mut size,
            0,
        )
    };
    (status == CR_SUCCESS && kind == DEVPROP_TYPE_UINT32).then_some(value)
}

/// The PCI vendor id of a device instance path (`PCI\VEN_1002&DEV_73DF\...`),
/// as the row sends it.
#[cfg(windows)]
fn vendor_of_id(id: &str) -> Option<String> {
    let upper = id.to_ascii_uppercase();
    let vendor = upper
        .strip_prefix("PCI\\")?
        .split('&')
        .find(|part| part.starts_with("VEN_"))
        .and_then(|part| part.strip_prefix("VEN_"))?;
    vendor_id(&format!("0x{vendor}"))
}

/// A NUL-terminated UTF-16 string, for the calls that take one.
#[cfg(windows)]
fn wide(text: &str) -> Vec<u16> {
    text.encode_utf16().chain(std::iter::once(0)).collect()
}

/// The text up to the first NUL of a UTF-16 buffer.
#[cfg(windows)]
fn from_wide(text: &[u16]) -> String {
    let end = text.iter().position(|c| *c == 0).unwrap_or(text.len());
    String::from_utf16_lossy(&text[..end])
}

#[cfg(all(test, unix))]
mod tests {
    use super::mib_from_meminfo;

    /// The line proc_meminfo(5) writes is in kilobytes, so the MiB the
    /// inventory answers is the number over 1024, and the file's other lines
    /// (a `MemFree`, an `HugePages_Total` with no unit) are not RAM.
    #[test]
    fn memtotals_kilobytes_become_mib() {
        let text = "\
MemTotal:       32778324 kB
MemFree:         1048576 kB
HugePages_Total:        8
MemAvailable:   24000000 kB
";
        assert_eq!(mib_from_meminfo(text), 32_010, "32778324 / 1024");
    }

    /// No line, a line with no number, and a number no parse reads, are all
    /// 0: the host answered nothing, and an inventory that guessed at a size
    /// would show it.
    #[test]
    fn a_meminfo_with_no_readable_memtotal_is_no_figure() {
        for text in [
            String::new(),
            "MemFree: 1 kB\n".to_string(),
            "MemTotal:\n".to_string(),
            "MemTotal: many kB\n".to_string(),
            "MemTotal: -4 kB\n".to_string(),
        ] {
            assert_eq!(mib_from_meminfo(&text), 0, "{text:?}");
        }
    }
}

/// Slice AN1 of `ARCHITECTURE.md` ("About and node info"): the OS and the GPU
/// kernel drivers, read over a fixture root. Every tree is one the test lays
/// out in a tempdir, nothing here reads the real `/etc` or `/sys`. The fixture
/// layout (os-release, sysfs) is Linux's, so these tests are Unix: what Linux
/// cannot prove about the Windows readers is only that they type-check
/// (`cargo check -p pai-core --all-targets --target x86_64-pc-windows-gnu`).
#[cfg(all(test, unix))]
mod an1_host_readers {
    use super::{gpu_drivers, os, GpuDriver};
    use std::path::Path;

    /// The file os-release(5) writes on a shipped distro: a quoted name, and
    /// the `VERSION` prose line beside the `VERSION_ID` the reader must take.
    const UBUNTU: &str = "PRETTY_NAME=\"Ubuntu 26.04 LTS\"\nNAME=\"Ubuntu\"\nVERSION_ID=\"26.04\"\nVERSION=\"26.04 LTS (Resolute Raccoon)\"\nID=ubuntu\n";

    /// `<rel>` written under `root` with `text`, its parents created.
    fn write_at(root: &Path, rel: &str, text: &str) {
        let path = root.join(rel);
        std::fs::create_dir_all(path.parent().expect("a parent")).expect("mkdir");
        std::fs::write(&path, text).expect("write");
    }

    /// The os-release text of exactly `len` bytes, its first two lines the name
    /// and the version id.
    fn os_release_of_len(len: usize) -> String {
        let head = "NAME=\"Ubuntu\"\nVERSION_ID=\"26.04\"\n";
        format!("{head}{}", "x".repeat(len - head.len()))
    }

    /// One PCI device laid out as sysfs does: the `class` and `vendor` files
    /// under `sys/bus/pci/devices/<bdf>`, and when `driver` is `Some` the
    /// `driver` symlink with the relative path sysfs itself writes
    /// (`../../../module/<driver>`). The module dir always exists when a driver
    /// is bound; its `version` file is written only when `version` is `Some`,
    /// which is what an in-tree driver (i915, xe, an amdgpu with no DKMS) is.
    fn pci_device(
        root: &Path,
        bdf: &str,
        class: &str,
        vendor: &str,
        driver: Option<&str>,
        version: Option<&str>,
    ) {
        let dev = format!("sys/bus/pci/devices/{bdf}");
        write_at(root, &format!("{dev}/class"), &format!("{class}\n"));
        write_at(root, &format!("{dev}/vendor"), &format!("{vendor}\n"));
        let Some(driver) = driver else { return };
        write_at(root, &format!("sys/module/{driver}/uevent"), "MAJOR=232\n");
        std::os::unix::fs::symlink(
            format!("../../../module/{driver}"),
            root.join(&dev).join("driver"),
        )
        .expect("symlink");
        if let Some(version) = version {
            write_at(
                root,
                &format!("sys/module/{driver}/version"),
                &format!("{version}\n"),
            );
        }
    }

    /// A device with a `class` and a `vendor` and a `driver` link aimed at a
    /// module dir that is not there.
    fn dangling_device(root: &Path, bdf: &str, class: &str, vendor: &str, driver: &str) {
        let dev = format!("sys/bus/pci/devices/{bdf}");
        write_at(root, &format!("{dev}/class"), &format!("{class}\n"));
        write_at(root, &format!("{dev}/vendor"), &format!("{vendor}\n"));
        std::os::unix::fs::symlink(
            format!("../../../module/{driver}"),
            root.join(&dev).join("driver"),
        )
        .expect("symlink");
    }

    /// The row a device must answer. `name` is `None` in every row here: on
    /// Linux a kernel row has no product name, the Vulkan join (AN2) fills it.
    fn row(
        bdf: &str,
        vendor: Option<&str>,
        driver: Option<&str>,
        version: Option<&str>,
    ) -> GpuDriver {
        GpuDriver {
            pci: bdf.to_string(),
            vendor: vendor.map(str::to_string),
            name: None,
            driver: driver.map(str::to_string),
            version: version.map(str::to_string),
        }
    }

    /// The kernel release is the host's own reading (`uname(2)`), which no
    /// fixture carries, so the pin is its shape and never this machine's value.
    fn assert_kernel(kernel: &Option<String>) {
        let kernel = kernel.as_deref().expect("the host's kernel release");
        assert!(!kernel.is_empty(), "the kernel is empty");
        assert!(
            kernel.bytes().all(|b| b.is_ascii_graphic()),
            "the kernel {kernel:?} is not printable ASCII"
        );
    }

    /// An `/etc/os-release` fixture gives `Ubuntu` and `26.04`: the `NAME` and
    /// the `VERSION_ID`, the shell quotes stripped, and `VERSION`'s prose left
    /// where it is.
    #[test]
    fn an_etc_os_release_fixture_gives_ubuntu_and_its_version() {
        let root = tempfile::tempdir().expect("tempdir");
        write_at(root.path(), "etc/os-release", UBUNTU);
        let got = os(root.path());
        assert_eq!(got.name.as_deref(), Some("Ubuntu"), "the NAME");
        assert_eq!(
            got.version.as_deref(),
            Some("26.04"),
            "the VERSION_ID, not the VERSION prose"
        );
        assert_kernel(&got.kernel);
    }

    /// With no `/etc/os-release`, the `/usr/lib/os-release` is the file read
    /// (os-release(5)).
    #[test]
    fn the_usr_lib_os_release_is_read_when_etc_has_none() {
        let root = tempfile::tempdir().expect("tempdir");
        write_at(
            root.path(),
            "usr/lib/os-release",
            "NAME=\"Debian GNU/Linux\"\nVERSION_ID=\"13\"\nID=debian\n",
        );
        let got = os(root.path());
        assert_eq!(got.name.as_deref(), Some("Debian GNU/Linux"));
        assert_eq!(got.version.as_deref(), Some("13"));
    }

    /// With both files, only `/etc`'s is read, never both: the distro's own
    /// file carries the split's overrides, so mixing the two would answer a
    /// name that is on no machine.
    #[test]
    fn etc_is_the_os_release_read_when_both_are_there() {
        let root = tempfile::tempdir().expect("tempdir");
        write_at(root.path(), "etc/os-release", UBUNTU);
        write_at(
            root.path(),
            "usr/lib/os-release",
            "NAME=\"Debian GNU/Linux\"\nVERSION_ID=\"13\"\nID=debian\n",
        );
        let got = os(root.path());
        assert_eq!(got.name.as_deref(), Some("Ubuntu"), "only /etc's name");
        assert_eq!(got.version.as_deref(), Some("26.04"), "only /etc's version");
    }

    /// No os-release file: name and version are null (the reader has nothing to
    /// guess from), the kernel stays the host's. An `etc` directory holding
    /// other files is the same absence.
    #[test]
    fn no_os_release_file_is_no_name_and_no_version() {
        let root = tempfile::tempdir().expect("tempdir");
        let got = os(root.path());
        assert_eq!(got.name, None, "no file, no name");
        assert_eq!(got.version, None, "no file, no version");
        assert_kernel(&got.kernel);
        write_at(root.path(), "etc/hostname", "node\n");
        let got = os(root.path());
        assert_eq!(got.name, None, "an etc with no os-release");
        assert_eq!(got.version, None, "an etc with no os-release");
    }

    /// The read is bounded at 4 KiB and the bound is the file's length, the
    /// versions files' rule: a 5 KiB file is all null, and so is a 4097-byte
    /// file that STARTS with a name (a reader that took the first 4 KiB and
    /// parsed them would answer Ubuntu here); a file of exactly 4096 is read.
    #[test]
    fn an_os_release_over_the_bound_is_all_null() {
        for len in [5120usize, 4097] {
            let root = tempfile::tempdir().expect("tempdir");
            write_at(root.path(), "etc/os-release", &os_release_of_len(len));
            let got = os(root.path());
            assert_eq!(got.name, None, "a {len} byte file is not a name");
            assert_eq!(got.version, None, "a {len} byte file is not a version");
        }
        let root = tempfile::tempdir().expect("tempdir");
        write_at(root.path(), "etc/os-release", &os_release_of_len(4096));
        assert_eq!(
            os(root.path()).name.as_deref(),
            Some("Ubuntu"),
            "a file of exactly the bound is read"
        );
    }

    /// `#` opens a comment line, so a name inside one is not the name, and the
    /// keys outside it are still read.
    #[test]
    fn a_name_inside_a_comment_is_not_the_name() {
        let root = tempfile::tempdir().expect("tempdir");
        write_at(
            root.path(),
            "etc/os-release",
            "# NAME=Fedora\nVERSION_ID=43\n",
        );
        let got = os(root.path());
        assert_eq!(got.name, None, "the commented line is not the name");
        assert_eq!(
            got.version.as_deref(),
            Some("43"),
            "the key outside the comment is read"
        );
    }

    /// A key is a whole key: a file with `LOGO` and `VERSION` and no `NAME` and
    /// no `VERSION_ID` answers nothing.
    #[test]
    fn a_key_that_is_not_the_key_is_not_read() {
        let root = tempfile::tempdir().expect("tempdir");
        write_at(
            root.path(),
            "etc/os-release",
            "LOGO=Ubuntu-icon\nVERSION=26.04 LTS (Resolute Raccoon)\n",
        );
        let got = os(root.path());
        assert_eq!(got.name, None, "LOGO is not NAME");
        assert_eq!(got.version, None, "VERSION is not VERSION_ID");
    }

    /// The kernel is the host's reading, never a value a fixture supplies: a
    /// file that names a kernel does not move it, and the reader answers the
    /// same kernel twice.
    #[test]
    fn the_kernel_is_the_host_reading_not_the_fixtures() {
        let root = tempfile::tempdir().expect("tempdir");
        write_at(
            root.path(),
            "etc/os-release",
            "NAME=\"Ubuntu\"\nVERSION_ID=\"26.04\"\nKERNEL=4.0.0-fake\n",
        );
        write_at(root.path(), "proc/sys/kernel/osrelease", "4.0.0-fake\n");
        write_at(
            root.path(),
            "usr/lib/os-release",
            "NAME=Fake\nKERNEL=4.0.0-fake\n",
        );
        let got = os(root.path());
        assert_kernel(&got.kernel);
        assert_ne!(
            got.kernel.as_deref(),
            Some("4.0.0-fake"),
            "the kernel is not read out of a file"
        );
        assert_eq!(
            os(root.path()).kernel,
            got.kernel,
            "the host answers the same kernel twice"
        );
    }

    /// A `0x030000` amdgpu device with `module/amdgpu/version` gives one row
    /// carrying its version, the bus id, the vendor and the driver.
    #[test]
    fn a_display_device_with_its_module_version_is_one_row() {
        let root = tempfile::tempdir().expect("tempdir");
        pci_device(
            root.path(),
            "0000:03:00.0",
            "0x030000",
            "0x1002",
            Some("amdgpu"),
            Some("7.1.9.31600000"),
        );
        assert_eq!(
            gpu_drivers(root.path()),
            vec![row(
                "0000:03:00.0",
                Some("1002"),
                Some("amdgpu"),
                Some("7.1.9.31600000")
            )],
            "the bus id, the vendor as 4 lower-case hex digits, the driver and its version"
        );
    }

    /// A `0x030200` 3D controller (NVIDIA's) is a display device like the VGA
    /// one, so it is a row with its driver's version.
    #[test]
    fn a_3d_controller_is_a_row() {
        let root = tempfile::tempdir().expect("tempdir");
        pci_device(
            root.path(),
            "0000:01:00.0",
            "0x030200",
            "0x10de",
            Some("nvidia"),
            Some("580.95.05"),
        );
        assert_eq!(
            gpu_drivers(root.path()),
            vec![row(
                "0000:01:00.0",
                Some("10de"),
                Some("nvidia"),
                Some("580.95.05")
            )]
        );
    }

    /// A `0x020000` network controller is no row even with a driver and a
    /// version: what makes a row is the class, not the presence of a driver. A
    /// tree of nothing else is no rows at all.
    #[test]
    fn a_network_device_is_no_row() {
        let root = tempfile::tempdir().expect("tempdir");
        pci_device(
            root.path(),
            "0000:00:1f.6",
            "0x020000",
            "0x8086",
            Some("igc"),
            Some("1.7.0"),
        );
        pci_device(
            root.path(),
            "0000:05:00.0",
            "0x038000",
            "0x1002",
            Some("amdgpu"),
            Some("6.4.4"),
        );
        assert_eq!(
            gpu_drivers(root.path()),
            vec![row(
                "0000:05:00.0",
                Some("1002"),
                Some("amdgpu"),
                Some("6.4.4")
            )],
            "only the display controller is a row"
        );
        let nic_only = tempfile::tempdir().expect("tempdir");
        pci_device(
            nic_only.path(),
            "0000:00:1f.6",
            "0x020000",
            "0x8086",
            Some("igc"),
            Some("1.7.0"),
        );
        assert_eq!(
            gpu_drivers(nic_only.path()),
            Vec::<GpuDriver>::new(),
            "a node whose only device is a NIC has no GPU row"
        );
    }

    /// A driver whose module sets no version (an in-tree amdgpu, i915, xe) is a
    /// row with `version: null`; the driver name is the `driver` link's own
    /// name, so a module dir with no version file, and one the link does not
    /// point at, both keep the driver.
    #[test]
    fn a_driver_with_no_version_file_is_a_row_with_no_version() {
        let root = tempfile::tempdir().expect("tempdir");
        pci_device(
            root.path(),
            "0000:00:02.0",
            "0x030000",
            "0x8086",
            Some("i915"),
            None,
        );
        assert_eq!(
            gpu_drivers(root.path()),
            vec![row("0000:00:02.0", Some("8086"), Some("i915"), None)]
        );
        let gone = tempfile::tempdir().expect("tempdir");
        dangling_device(gone.path(), "0000:03:00.0", "0x030000", "0x1002", "amdgpu");
        assert_eq!(
            gpu_drivers(gone.path()),
            vec![row("0000:03:00.0", Some("1002"), Some("amdgpu"), None)],
            "the link names the driver, the missing module gives no version"
        );
    }

    /// An unbound device, with no `driver` link, is a row with `driver: null`.
    #[test]
    fn an_unbound_device_is_a_row_with_no_driver() {
        let root = tempfile::tempdir().expect("tempdir");
        pci_device(
            root.path(),
            "0000:03:00.0",
            "0x030000",
            "0x1002",
            None,
            None,
        );
        assert_eq!(
            gpu_drivers(root.path()),
            vec![row("0000:03:00.0", Some("1002"), None, None)]
        );
    }

    /// The rows are in bus id order, whatever order the directory hands over.
    #[test]
    fn the_rows_are_in_bus_id_order() {
        let root = tempfile::tempdir().expect("tempdir");
        for bdf in [
            "0000:07:00.0",
            "0000:01:00.0",
            "0000:03:00.0",
            "0000:00:02.0",
        ] {
            pci_device(
                root.path(),
                bdf,
                "0x030000",
                "0x1002",
                Some("amdgpu"),
                Some("1"),
            );
        }
        let pci: Vec<String> = gpu_drivers(root.path())
            .into_iter()
            .map(|r| r.pci)
            .collect();
        assert_eq!(
            pci,
            [
                "0000:00:02.0",
                "0000:01:00.0",
                "0000:03:00.0",
                "0000:07:00.0"
            ]
        );
    }

    /// 40 display devices give 32 rows, the cap, and the rows kept are the
    /// first 32 in bus id order (the design fixes the order of the rows; which
    /// half a capped list drops is pinned here as the lowest bus ids).
    #[test]
    fn forty_display_devices_give_thirty_two_rows() {
        let root = tempfile::tempdir().expect("tempdir");
        for i in 0..40u32 {
            pci_device(
                root.path(),
                &format!("0000:{i:02x}:00.0"),
                "0x030000",
                "0x1002",
                Some("amdgpu"),
                Some("1"),
            );
        }
        let got = gpu_drivers(root.path());
        assert_eq!(got.len(), 32, "40 devices, 32 rows");
        let pci: Vec<String> = got.into_iter().map(|r| r.pci).collect();
        let want: Vec<String> = (0..32u32).map(|i| format!("0000:{i:02x}:00.0")).collect();
        assert_eq!(pci, want, "the 32 lowest bus ids, in order");
    }

    /// Each sysfs value is read up to 128 bytes and a longer one is no value:
    /// a version of 4000 bytes is `null` while the row and its driver stay, a
    /// version of 64 bytes is read.
    #[test]
    fn a_sysfs_value_over_128_bytes_is_no_value() {
        let over = tempfile::tempdir().expect("tempdir");
        pci_device(
            over.path(),
            "0000:03:00.0",
            "0x030000",
            "0x1002",
            Some("amdgpu"),
            Some(&"7".repeat(4000)),
        );
        assert_eq!(
            gpu_drivers(over.path()),
            vec![row("0000:03:00.0", Some("1002"), Some("amdgpu"), None)],
            "a version of 4000 bytes is not a value, the row stays"
        );
        let at = tempfile::tempdir().expect("tempdir");
        pci_device(
            at.path(),
            "0000:03:00.0",
            "0x030000",
            "0x1002",
            Some("amdgpu"),
            Some(&"7".repeat(64)),
        );
        assert_eq!(
            gpu_drivers(at.path()),
            vec![row(
                "0000:03:00.0",
                Some("1002"),
                Some("amdgpu"),
                Some(&"7".repeat(64))
            )],
            "a version well inside the bound is read"
        );
    }

    /// A sysfs value that is not printable ASCII is no value (the versions
    /// files' rule); the padding a sysfs file writes around a value is trimmed.
    #[test]
    fn a_sysfs_value_that_is_not_printable_is_no_value() {
        let bell = tempfile::tempdir().expect("tempdir");
        pci_device(
            bell.path(),
            "0000:03:00.0",
            "0x030000",
            "0x1002",
            Some("amdgpu"),
            Some("7.1.9\u{7}9"),
        );
        assert_eq!(
            gpu_drivers(bell.path()),
            vec![row("0000:03:00.0", Some("1002"), Some("amdgpu"), None)],
            "the bell is not a version"
        );
        let padded = tempfile::tempdir().expect("tempdir");
        pci_device(
            padded.path(),
            "0000:03:00.0",
            "0x030000",
            "0x1002",
            Some("amdgpu"),
            Some("  7.1.9  "),
        );
        assert_eq!(
            gpu_drivers(padded.path()),
            vec![row(
                "0000:03:00.0",
                Some("1002"),
                Some("amdgpu"),
                Some("7.1.9")
            )],
            "the padding is trimmed"
        );
    }

    /// `vendor` is the PCI vendor id as 4 lower-case hex digits: sysfs writes
    /// `0x10DE`, the row answers `10de`; a value that is not a hex id is no
    /// vendor, and the row is kept with its driver.
    #[test]
    fn the_vendor_is_four_lower_case_hex_digits() {
        let upper = tempfile::tempdir().expect("tempdir");
        pci_device(
            upper.path(),
            "0000:03:00.0",
            "0x030000",
            "0x10DE",
            Some("amdgpu"),
            Some("1"),
        );
        assert_eq!(
            gpu_drivers(upper.path()),
            vec![row("0000:03:00.0", Some("10de"), Some("amdgpu"), Some("1"))]
        );
        let junk = tempfile::tempdir().expect("tempdir");
        pci_device(
            junk.path(),
            "0000:03:00.0",
            "0x030000",
            "not-a-vendor",
            Some("amdgpu"),
            Some("1"),
        );
        assert_eq!(
            gpu_drivers(junk.path()),
            vec![row("0000:03:00.0", None, Some("amdgpu"), Some("1"))],
            "no vendor id, and the row is still a row"
        );
    }

    /// A tree that names no display device is no rows, and the reader guesses
    /// at nothing: no `sys/bus/pci/devices` at all, a stray file in it, and a
    /// device dir with no `class` file to judge, are all nothing.
    #[test]
    fn a_tree_that_names_no_display_device_is_no_rows() {
        let root = tempfile::tempdir().expect("tempdir");
        assert_eq!(gpu_drivers(root.path()), Vec::<GpuDriver>::new());
        write_at(root.path(), "sys/bus/pci/devices/not-a-device", "x\n");
        write_at(
            root.path(),
            "sys/bus/pci/devices/0000:03:00.0/vendor",
            "0x1002\n",
        );
        assert_eq!(
            gpu_drivers(root.path()),
            Vec::<GpuDriver>::new(),
            "a file, and a device with no class, are no rows"
        );
    }
}
