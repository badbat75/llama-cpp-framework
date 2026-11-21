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
