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
fn free_memory_is_the_total_less_the_system_wide_usage() {
    let card = facts(0x1002, Some((0x14, 0, 0)), "AMD Radeon AI PRO R9700");
    let unlocated = facts(0x1002, None, "AMD Radeon AI PRO R9700");
    let no_memory = AdapterFacts {
        dedicated: 0,
        ..facts(0x1002, Some((0x7d, 0, 0)), "AMD Radeon(TM) Graphics")
    };
    let used = HashMap::from([(R9700, 31u64 << 30)]);
    let free = free_per_adapter(&[card.clone(), unlocated, no_memory], &used);
    assert_eq!(
        free,
        vec![crate::devices::AdapterFree {
            pci: "14:00.0".into(),
            name: "AMD Radeon AI PRO R9700".into(),
            free_mib: 1024,
        }]
    );
    // An adapter the counters do not list holds nothing.
    assert_eq!(
        free_per_adapter(&[card], &HashMap::new())[0].free_mib,
        32768
    );
}

/// The live device tree, on whatever cards this machine has: every card on
/// a PCI bus is found, and a link that is read is a real one.
#[test]
fn the_device_tree_describes_every_located_card() {
    for gpu in physical_gpus() {
        assert!(gpu.switches.is_some(), "{gpu:?}");
        if let Some(link) = gpu.link {
            assert!(
                (1..=7).contains(&link.generation) && link.width >= 1,
                "{gpu:?}"
            );
        }
    }
}

/// The live counters, on whatever cards this machine has.
#[test]
fn the_live_reading_stays_within_the_card() {
    for a in adapters_free() {
        assert!(a.free_mib >= 0, "{a:?}");
        assert_eq!(a.pci.len(), 7, "{a:?}");
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
    let labels = |bars: &[Bar]| -> Vec<String> { bars.iter().map(|b| b.label.clone()).collect() };

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
        io_time: 0,
        io_count: 0,
    };
    // Two seconds later: 64 MiB read, 16 MiB written, idle for 1.5 s, and
    // 100 I/Os that took 0.5 s in all.
    let b = DiskStat {
        read_bytes: (1 << 30) + (64 << 20),
        written_bytes: 16 << 20,
        idle: 15_000_000,
        at: 20_000_000,
        io_time: 5_000_000,
        io_count: 100,
    };
    let (mut peak, mut wait_peak) = (0.0, 0.0);
    let bars = disk_bars(a, b, &mut peak, &mut wait_peak);
    assert_eq!(bars[0].text, "32.0 MiB/s · peak 32.0 MiB/s");
    // One scale for both directions.
    assert_eq!(bars[1].text, "8.0 MiB/s · peak 32.0 MiB/s");
    assert_eq!(bars[2].text, "25 %");
    assert_eq!(bars[3].text, "5.00 ms per I/O · peak 5.00 ms");
    // The same reading twice is no interval at all.
    assert!(disk_bars(b, b, &mut peak, &mut wait_peak).is_empty());
}

#[test]
fn topology_records_add_up_to_the_processor() {
    let record = |relationship: u32, fields: &[(usize, &[u8])]| {
        let mut r = vec![0u8; 48];
        r[0..4].copy_from_slice(&relationship.to_le_bytes());
        r[4..8].copy_from_slice(&48u32.to_le_bytes());
        for (at, bytes) in fields {
            r[*at..*at + bytes.len()].copy_from_slice(bytes);
        }
        r
    };
    let core = |mask: u64| record(0, &[(32, &mask.to_le_bytes())]);
    let cache = |level: u8, bytes: u32| record(2, &[(8, &[level]), (12, &bytes.to_le_bytes())]);
    let buffer = [
        core(0b0011),
        core(0b1100),
        record(1, &[]),
        cache(1, 48 << 10),
        cache(1, 32 << 10),
        cache(2, 1 << 20),
        cache(2, 1 << 20),
        cache(3, 32 << 20),
        record(3, &[]),
        // A relationship nobody reads counts for nothing.
        record(4, &[]),
    ]
    .concat();
    let mut cpu = Cpu::default();
    read_topology(&buffer, &mut cpu);
    assert_eq!(
        (cpu.sockets, cpu.cores, cpu.threads, cpu.numa_nodes),
        (Some(1), Some(2), Some(4), Some(1))
    );
    assert_eq!(
        (cpu.l1_kib, cpu.l2_kib, cpu.l3_kib),
        (Some(80), Some(2048), Some(32 << 10))
    );

    // A record with a nonsense size ends the walk.
    let mut cpu = Cpu::default();
    read_topology(&[0, 0, 0, 0, 2, 0, 0, 0], &mut cpu);
    assert_eq!(cpu, Cpu::default());
}

#[test]
fn a_package_lists_its_group_masks() {
    let mut record = vec![0u8; 64];
    record[0..4].copy_from_slice(&RELATION_PACKAGE.to_le_bytes());
    record[4..8].copy_from_slice(&64u32.to_le_bytes());
    record[GROUP_COUNT] = 2;
    record[GROUP_MASKS..GROUP_MASKS + 8].copy_from_slice(&0xFu64.to_le_bytes());
    record[GROUP_MASKS + 16..GROUP_MASKS + 24].copy_from_slice(&0x3u64.to_le_bytes());
    record[GROUP_MASKS + 24] = 1;
    assert_eq!(socket_masks(&record), [vec![(0, 0xF), (1, 0x3)]]);
}

#[test]
fn the_estimated_iowait_is_the_queue_over_the_threads_within_the_idle_time() {
    // One I/O in flight all the time on 24 threads, one processor's worth.
    assert!((estimated_iowait(10.0, 1.0, 24) - 100.0 / 24.0).abs() < 1e-9);
    // It cannot exceed the idle time, however long the queue.
    assert_eq!(estimated_iowait(90.0, 100.0, 24), 10.0);
    assert_eq!(estimated_iowait(100.0, 1.0, 24), 0.0);
    assert_eq!(estimated_iowait(0.0, 0.0, 24), 0.0);
    // No thread count is one thread, not a division by zero.
    assert_eq!(estimated_iowait(50.0, 0.25, 0), 25.0);

    let values = [
        ("0 C:".to_string(), 1.5),
        ("1 E:".to_string(), 0.5),
        ("_Total".to_string(), 9.0),
    ];
    assert_eq!(summed_queues(&values), 2.0);
}

// The real counter: an instance per disk, read after its baseline.
#[test]
fn the_disk_queue_counter_answers_on_this_machine() {
    let queues = DiskQueues::open().expect("the counter opens");
    std::thread::sleep(std::time::Duration::from_millis(100));
    let queue = queues.read().expect("the counter reads");
    assert!(queue >= 0.0, "{queue}");
}

#[test]
fn a_socket_is_the_mean_of_its_processors() {
    let values: Vec<(String, f64)> = [
        ("0,0", 10.0),
        ("0,1", 30.0),
        ("0,_Total", 99.0),
        ("_Total", 99.0),
        ("1,0", 50.0),
    ]
    .iter()
    .map(|(name, value)| (name.to_string(), *value))
    .collect();
    let sockets = [vec![(0, 0b11)], vec![(1, 0b1)], vec![(2, 0b1)]];
    assert_eq!(
        socket_usage(&values, &sockets),
        [Some(20.0), Some(50.0), None]
    );
}

// The real machine: whatever it is, it has a processor with threads.
#[test]
fn the_processor_is_read_on_this_machine() {
    let cpu = cpu_facts();
    assert!(cpu.cores.is_some_and(|n| n > 0), "{cpu:?}");
    assert!(cpu.threads >= cpu.cores, "{cpu:?}");
    assert!(cpu.sockets.is_some_and(|n| n > 0), "{cpu:?}");
}

// The real counter, with this machine's first socket's processors split in
// two as if they were two sockets: both answer, with a share of busy time.
#[test]
fn the_per_socket_counter_answers_on_this_machine() {
    let masks = socket_masks(&topology_buffer());
    let (group, mask) = masks[0][0];
    let low = mask.isolate_lowest_one();
    let sockets = SocketCounters::open(vec![vec![(group, low)], vec![(group, mask & !low)]])
        .expect("the counter opens");
    std::thread::sleep(std::time::Duration::from_millis(100));
    let usage = sockets.read();
    assert_eq!(usage.len(), 2);
    assert!(
        usage
            .iter()
            .all(|u| u.is_some_and(|p| (0.0..=100.0).contains(&p))),
        "{usage:?}"
    );
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
    let find = |groups: &[Group], id: &str| groups.iter().find(|g| g.id == id).cloned().unwrap();

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
    let groups = s.sample(&none);
    let cpu = find(&groups, CPU_ID);
    let labels: Vec<&str> = cpu.bars.iter().map(|b| b.label.as_str()).collect();
    assert_eq!(labels, ["CPU", "I/O wait (est.)"]);
}
