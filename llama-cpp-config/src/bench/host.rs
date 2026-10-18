//! What bench.db records about the machine and the software stack a run ran on.
//!
//! `bench::env` stamps a report with the facts that change under a machine
//! between two runs (boot time, display driver); this module describes the
//! machine itself, so runs from different boxes can sit in one database and be
//! told apart: CPU, RAM (amount, type, speed, modules), the discrete GPUs with
//! their VRAM and driver, OS and kernel. A new driver or kernel is a new
//! machine row, which is the point: the row a run points at is the state it
//! measured.
//!
//! Best effort, like `bench::env`: a value that cannot be read is left empty
//! rather than guessed. The RAM modules come from SMBIOS, which Linux exposes
//! readable to every user only through udev's properties of the DMI device
//! (`dmidecode` needs root), and Windows through CIM.
//!
//! ## The fingerprint is a canonical text
//!
//! A machine row is found again by `fingerprint`, the sha256 of
//! `Machine::canonical`, never by its id: `publish` matches it across two
//! databases, so the same box lands on the same row in both.

use crate::bench::db::{Gpu, Machine, Software};

/// The discrete GPUs, their VRAM and driver. Integrated graphics are left
/// out: they compute nothing a preset places, and on an AMD desktop their
/// llama.cpp entry reports system RAM as VRAM.
const MIN_DISCRETE_MIB: u64 = 8192;

pub fn machine() -> Machine {
    let mut m = os_machine();
    m.gpus
        .sort_by(|a, b| (&a.backend, &a.name).cmp(&(&b.backend, &b.name)));
    m
}

/// llama.cpp's version banner plus what this binary was built beside.
pub fn software() -> Software {
    let banner = crate::paths::llama_server_exe()
        .and_then(|exe| crate::proc::run_hidden_probe(&exe, ["--version"]))
        .map(|o| crate::proc::combined_output(&o))
        .unwrap_or_default();
    let (tag, build, commit) = parse_version(&banner);
    let opt = |s: &str| (!s.is_empty()).then(|| s.to_string());
    Software {
        llama_cpp_tag: tag,
        llama_cpp_build: build,
        llama_cpp_commit: commit,
        framework_version: Some(env!("CARGO_PKG_VERSION").to_string()),
        rocm: opt(env!("LLAMA_FW_ROCM")),
        cuda: opt(env!("LLAMA_FW_CUDA")),
        vulkan_sdk: opt(env!("LLAMA_FW_VULKAN_SDK")),
        patches: opt(env!("LLAMA_FW_PATCHES")),
    }
}

/// `version: 0.7.0 (build 11669, commit 620d2225c)` into (`v0.7.0`, 11669,
/// `620d2225c`). The older `version: 9870 (2d973636e)` shape has no release:
/// its tag is the nightly's `b9870`. `?` when no line carries a version.
pub fn parse_version(banner: &str) -> (String, Option<i64>, Option<String>) {
    let Some(line) = banner
        .lines()
        .map(str::trim)
        .find_map(|l| l.strip_prefix("version: "))
    else {
        return ("?".into(), None, None);
    };
    let (ver, rest) = line.split_once(' ').unwrap_or((line, ""));
    let inner = rest.trim().trim_start_matches('(').trim_end_matches(')');
    let field = |key: &str| {
        inner
            .split(',')
            .map(str::trim)
            .find_map(|f| f.strip_prefix(key))
            .map(|v| v.trim().to_string())
    };
    if let Some(build) = field("build ") {
        let build = build.parse().ok();
        return (format!("v{ver}"), build, field("commit "));
    }
    let commit = (!inner.is_empty()).then(|| inner.to_string());
    (format!("b{ver}"), ver.parse().ok(), commit)
}

/// `34359738368` bytes as SMBIOS tools print a module: `32 GB`.
fn module_size(bytes: u64) -> String {
    format!("{} GB", bytes >> 30)
}

/// `2 x 32 GB CMK64GX5M2B6000Z30`: identical modules counted, in first-seen
/// order.
pub fn modules_line(modules: &[(String, String)]) -> Option<String> {
    let mut kinds: Vec<(String, usize)> = Vec::new();
    for (size, part) in modules {
        let k = format!("{size} {part}").trim().to_string();
        match kinds.iter_mut().find(|(n, _)| *n == k) {
            Some((_, c)) => *c += 1,
            None => kinds.push((k, 1)),
        }
    }
    (!kinds.is_empty()).then(|| {
        kinds
            .iter()
            .map(|(k, n)| format!("{n} x {k}"))
            .collect::<Vec<_>>()
            .join(", ")
    })
}

/// The CUDA cards as nvidia-smi reports them. nvidia-smi, not llama.cpp's
/// device list: the latter reports the total minus what the driver keeps,
/// and that moves with the driver state.
fn nvidia_gpus() -> Vec<Gpu> {
    let Ok(out) = std::process::Command::new("nvidia-smi")
        .args([
            "--query-gpu=name,memory.total,driver_version",
            "--format=csv,noheader,nounits",
        ])
        .output()
    else {
        return Vec::new();
    };
    String::from_utf8_lossy(&out.stdout)
        .lines()
        .filter_map(|l| {
            let f: Vec<&str> = l.split(',').map(str::trim).collect();
            Some(Gpu {
                name: f.first()?.to_string(),
                vram_mib: f.get(1)?.parse().ok()?,
                backend: "CUDA".into(),
                driver: f.get(2).map(|s| s.to_string()),
            })
        })
        .collect()
}

#[cfg(not(windows))]
fn os_machine() -> Machine {
    let read = |p: &str| std::fs::read_to_string(p).unwrap_or_default();
    let cpuinfo = read("/proc/cpuinfo");
    let mut cpu = String::new();
    let mut threads = 0;
    let mut cores = std::collections::HashSet::new();
    let mut phys = String::new();
    for line in cpuinfo.lines() {
        let Some((k, v)) = line.split_once(':') else {
            continue;
        };
        let (k, v) = (k.trim(), v.trim());
        match k {
            "processor" => threads += 1,
            "model name" if cpu.is_empty() => cpu = v.to_string(),
            "physical id" => phys = v.to_string(),
            "core id" => {
                cores.insert((phys.clone(), v.to_string()));
            }
            _ => {}
        }
    }
    let ram_mib = read("/proc/meminfo")
        .lines()
        .find_map(|l| l.strip_prefix("MemTotal:"))
        .and_then(|v| v.split_whitespace().next()?.parse::<u64>().ok())
        .map(|kib| kib / 1024)
        .unwrap_or(0);
    let (ram_type, ram_speed_mts, ram_modules) = dmi_memory();
    let os = read("/etc/os-release")
        .lines()
        .find_map(|l| l.strip_prefix("PRETTY_NAME="))
        .map(|v| v.trim_matches('"').to_string())
        .unwrap_or_else(|| "Linux".into());
    let kernel = read("/proc/sys/kernel/osrelease").trim().to_string();
    let mut gpus = nvidia_gpus();
    gpus.extend(amd_gpus(&kernel));
    Machine {
        cpu,
        cores: cores.len().max(1) as i64,
        threads,
        ram_mib: ram_mib as i64,
        ram_type,
        ram_speed_mts,
        ram_modules,
        os,
        kernel,
        gpus,
    }
}

/// The populated memory devices from udev's view of SMBIOS:
/// `MEMORY_DEVICE_<n>_SIZE`, `_TYPE`, `_CONFIGURED_SPEED_MTS`, `_PART_NUMBER`.
#[cfg(not(windows))]
fn dmi_memory() -> (Option<String>, Option<i64>, Option<String>) {
    let out = std::process::Command::new("udevadm")
        .args(["info", "-q", "property", "-p", "/devices/virtual/dmi/id"])
        .output()
        .map(|o| String::from_utf8_lossy(&o.stdout).into_owned())
        .unwrap_or_default();
    parse_udev_memory(&out)
}

#[cfg(any(not(windows), test))]
pub fn parse_udev_memory(props: &str) -> (Option<String>, Option<i64>, Option<String>) {
    let mut by_dev: std::collections::BTreeMap<u32, std::collections::HashMap<String, String>> =
        Default::default();
    for line in props.lines() {
        let Some(rest) = line.strip_prefix("MEMORY_DEVICE_") else {
            continue;
        };
        let Some((key, value)) = rest.split_once('=') else {
            continue;
        };
        let Some((n, field)) = key.split_once('_') else {
            continue;
        };
        if let Ok(n) = n.parse() {
            by_dev
                .entry(n)
                .or_default()
                .insert(field.to_string(), value.to_string());
        }
    }
    let present: Vec<_> = by_dev
        .values()
        .filter(|d| {
            d.get("SIZE")
                .and_then(|s| s.parse::<u64>().ok())
                .unwrap_or(0)
                > 0
        })
        .collect();
    let Some(first) = present.first() else {
        return (None, None, None);
    };
    let modules: Vec<(String, String)> = present
        .iter()
        .map(|d| {
            (
                module_size(d["SIZE"].parse().unwrap_or(0)),
                d.get("PART_NUMBER").cloned().unwrap_or_default(),
            )
        })
        .collect();
    (
        first.get("TYPE").cloned(),
        first
            .get("CONFIGURED_SPEED_MTS")
            .and_then(|s| s.parse().ok()),
        modules_line(&modules),
    )
}

/// The amdgpu cards with a discrete card's VRAM, named by llama.cpp's device
/// list (matched on the total, which both read off the same driver counter).
#[cfg(not(windows))]
fn amd_gpus(kernel: &str) -> Vec<Gpu> {
    let devs = crate::devices::list();
    let mut out = Vec::new();
    let Ok(cards) = std::fs::read_dir("/sys/class/drm") else {
        return out;
    };
    let mut cards: Vec<_> = cards
        .flatten()
        .map(|e| e.path())
        .filter(|p| {
            p.file_name()
                .and_then(|n| n.to_str())
                .is_some_and(|n| n.starts_with("card") && !n.contains('-'))
        })
        .collect();
    cards.sort();
    for card in cards {
        let dev = card.join("device");
        let read = |f: &str| {
            std::fs::read_to_string(dev.join(f))
                .map(|s| s.trim().to_string())
                .unwrap_or_default()
        };
        if read("vendor") != "0x1002" {
            continue;
        }
        let Ok(bytes) = read("mem_info_vram_total").parse::<u64>() else {
            continue;
        };
        let vram_mib = bytes >> 20;
        if vram_mib < MIN_DISCRETE_MIB {
            continue;
        }
        let name = devs
            .iter()
            .find(|d| d.id.starts_with("ROCm") && d.total_mib == vram_mib)
            .map(|d| d.name.clone())
            .unwrap_or_else(|| card.file_name().unwrap().to_string_lossy().into_owned());
        let vbios = read("vbios_version");
        out.push(Gpu {
            name,
            vram_mib: vram_mib as i64,
            backend: "ROCm".into(),
            driver: Some(format!("amdgpu (kernel {kernel}), vbios {vbios}")),
        });
    }
    out
}

/// One PowerShell call for the CIM classes this needs: spawning it costs about
/// a second, once per recorded run.
#[cfg(windows)]
fn os_machine() -> Machine {
    const SCRIPT: &str = "$ErrorActionPreference='SilentlyContinue'; @{ \
        cpu = @(Get-CimInstance Win32_Processor | Select-Object Name, NumberOfCores, NumberOfLogicalProcessors); \
        mem = @(Get-CimInstance Win32_PhysicalMemory | Select-Object Capacity, PartNumber, ConfiguredClockSpeed, SMBIOSMemoryType); \
        os = Get-CimInstance Win32_OperatingSystem | Select-Object Caption, Version, BuildNumber, TotalVisibleMemorySize \
        } | ConvertTo-Json -Depth 3 -Compress";
    let mut cmd = std::process::Command::new("powershell.exe");
    cmd.args(["-NoProfile", "-NonInteractive", "-Command", SCRIPT]);
    crate::proc::hide_console(&mut cmd);
    let text = cmd
        .output()
        .map(|o| String::from_utf8_lossy(&o.stdout).into_owned())
        .unwrap_or_default();
    let v: serde_json::Value = serde_json::from_str(text.trim()).unwrap_or_default();
    let mut m = parse_cim(&v);
    m.gpus = nvidia_gpus();
    m.gpus.extend(amd_gpus());
    m
}

#[cfg(any(windows, test))]
/// SMBIOS memory type codes (DSP0134, table 76) for the generations a machine
/// running this could carry.
pub fn smbios_memory_type(code: i64) -> Option<&'static str> {
    Some(match code {
        24 => "DDR3",
        26 => "DDR4",
        30 => "LPDDR4",
        34 => "DDR5",
        35 => "LPDDR5",
        _ => return None,
    })
}

#[cfg(any(windows, test))]
/// The CIM document `os_machine` asks PowerShell for, GPUs aside.
pub fn parse_cim(v: &serde_json::Value) -> Machine {
    let s = |x: &serde_json::Value| x.as_str().unwrap_or_default().trim().to_string();
    let i = |x: &serde_json::Value| x.as_i64().unwrap_or(0);
    let cpus = v["cpu"].as_array().cloned().unwrap_or_default();
    let mem = v["mem"].as_array().cloned().unwrap_or_default();
    let os = &v["os"];
    let modules: Vec<(String, String)> = mem
        .iter()
        .map(|d| {
            (
                module_size(d["Capacity"].as_u64().unwrap_or(0)),
                s(&d["PartNumber"]),
            )
        })
        .collect();
    Machine {
        cpu: cpus.first().map(|c| s(&c["Name"])).unwrap_or_default(),
        cores: cpus.iter().map(|c| i(&c["NumberOfCores"])).sum(),
        threads: cpus
            .iter()
            .map(|c| i(&c["NumberOfLogicalProcessors"]))
            .sum(),
        ram_mib: i(&os["TotalVisibleMemorySize"]) / 1024,
        ram_type: mem
            .first()
            .and_then(|d| smbios_memory_type(i(&d["SMBIOSMemoryType"])))
            .map(str::to_string),
        ram_speed_mts: mem
            .first()
            .map(|d| i(&d["ConfiguredClockSpeed"]))
            .filter(|&v| v > 0),
        ram_modules: modules_line(&modules),
        os: s(&os["Caption"]),
        kernel: s(&os["Version"]),
        gpus: Vec::new(),
    }
}

/// The AMD cards from llama.cpp's device list, with the display driver the
/// registry names for them. The list reports the total VRAM on Windows too.
#[cfg(windows)]
fn amd_gpus() -> Vec<Gpu> {
    let adapters = super::env::adapters();
    crate::devices::list()
        .into_iter()
        .filter(|d| d.id.starts_with("ROCm") || d.id.starts_with("HIP"))
        .filter(|d| d.total_mib >= MIN_DISCRETE_MIB)
        .map(|d| Gpu {
            driver: adapters
                .iter()
                .find(|(desc, _, _)| desc.contains(&d.name) || d.name.contains(desc.as_str()))
                .map(|(_, ver, date)| format!("{ver}, dated {date}")),
            name: d.name,
            vram_mib: d.total_mib as i64,
            backend: "ROCm".into(),
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_release_banner_gives_tag_build_and_commit() {
        let b = "load_backend: loaded CUDA\nversion: 0.7.0 (build 11669, commit 620d2225c)\nbuilt with clang";
        assert_eq!(
            parse_version(b),
            ("v0.7.0".into(), Some(11669), Some("620d2225c".into()))
        );
    }

    #[test]
    fn the_nightly_banner_gives_a_b_tag() {
        assert_eq!(
            parse_version("version: 9870 (2d973636e)"),
            ("b9870".into(), Some(9870), Some("2d973636e".into()))
        );
        assert_eq!(parse_version("nothing here"), ("?".into(), None, None));
    }

    #[test]
    fn udev_memory_counts_only_populated_slots() {
        let props = "MEMORY_DEVICE_0_PRESENT=0\nMEMORY_DEVICE_0_LOCATOR=DIMM 0\n\
            MEMORY_DEVICE_1_SIZE=34359738368\nMEMORY_DEVICE_1_TYPE=DDR5\n\
            MEMORY_DEVICE_1_CONFIGURED_SPEED_MTS=6000\nMEMORY_DEVICE_1_PART_NUMBER=CMK64GX5M2B6000Z30\n\
            MEMORY_DEVICE_3_SIZE=34359738368\nMEMORY_DEVICE_3_TYPE=DDR5\n\
            MEMORY_DEVICE_3_CONFIGURED_SPEED_MTS=6000\nMEMORY_DEVICE_3_PART_NUMBER=CMK64GX5M2B6000Z30\n";
        assert_eq!(
            parse_udev_memory(props),
            (
                Some("DDR5".into()),
                Some(6000),
                Some("2 x 32 GB CMK64GX5M2B6000Z30".into())
            )
        );
        assert_eq!(parse_udev_memory(""), (None, None, None));
    }

    #[test]
    fn cim_document_reads_into_a_machine() {
        let v = serde_json::json!({
            "cpu": [{"Name": "AMD Ryzen 9 9900X 12-Core Processor ", "NumberOfCores": 12, "NumberOfLogicalProcessors": 24}],
            "mem": [
                {"Capacity": 34359738368u64, "PartNumber": "CMK64GX5M2B6000Z30  ", "ConfiguredClockSpeed": 6000, "SMBIOSMemoryType": 34},
                {"Capacity": 34359738368u64, "PartNumber": "CMK64GX5M2B6000Z30  ", "ConfiguredClockSpeed": 6000, "SMBIOSMemoryType": 34}
            ],
            "os": {"Caption": "Microsoft Windows 11 Pro", "Version": "10.0.26200", "TotalVisibleMemorySize": 66_000_000}
        });
        let m = parse_cim(&v);
        assert_eq!(m.cpu, "AMD Ryzen 9 9900X 12-Core Processor");
        assert_eq!((m.cores, m.threads, m.ram_mib), (12, 24, 64453));
        assert_eq!(m.ram_type.as_deref(), Some("DDR5"));
        assert_eq!(m.ram_speed_mts, Some(6000));
        assert_eq!(
            m.ram_modules.as_deref(),
            Some("2 x 32 GB CMK64GX5M2B6000Z30")
        );
        assert_eq!(m.kernel, "10.0.26200");
    }

    /// Prints what this machine records. Spawns nvidia-smi, udevadm and
    /// llama-server, so it runs only on request: `cargo test -- --ignored`.
    #[test]
    #[ignore]
    fn print_this_machine() {
        let m = machine();
        println!("{}fingerprint={}", m.canonical(), m.fingerprint());
        println!("{:?}", software());
    }
}
