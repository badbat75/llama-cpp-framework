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
    let find = |groups: &[Group], id: &str| groups.iter().find(|g| g.id == id).cloned().unwrap();

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
