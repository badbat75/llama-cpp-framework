use super::{DeviceRow, InstanceRow};
use zbus::zvariant::Type;

/// The row of the node's GPU list (AN3, `ARCHITECTURE.md` "About and node
/// info"), as the client reads it: `pci` (the bus id to join the Vulkan
/// device onto, empty when the row is a Vulkan device no kernel row
/// carries), `vendor` (the 4 hex digits), the product `name`, the kernel
/// `driver` and its `version`, then the Vulkan pair: `has_vulkan`, its
/// `driver` and its `info`.
///
/// The bus has no null, so an absent text is the empty string and the
/// Vulkan pair carries its own flag, the way `ModelInfoRow` says which
/// half of its reply is there: a driver name the host did not answer is
/// `""` on the bus and `None` in the reply, and `has_vulkan` false answers
/// the pair empty, never a row with a Vulkan driver of nothing.
type NodeGpuRow = (String, String, String, String, String, bool, String, String);

/// The whole document: `os` name, version and kernel, the `vulkan` word
/// (`ok`, `none` or `failed`), then the rows. The kernel is the host's own
/// `uname(2)`, so it travels as text like the other two.
type NodeInfoRow = (String, String, String, String, Vec<NodeGpuRow>);

/// The row `Instances` answers, as the client reads it: one D-Bus struct
/// `(s id, s engine, s server_profile, q port, as profile_tags,
/// as instance_tags, s label, s instance_profile, s hostname)`, the list
/// `a(sssqasassss)`. Slice 13 puts the
/// server profile after the engine: a client that reads five fields never
/// learns the name, and the only name left to it is the id, which is not
/// the file the instance reads. Slice IL1 puts the label last, as `s`: the
/// bus has no null, so the empty string carries "this instance has no
/// label", the reading every other text of the row already has. IP4b
/// appends the instance profile and the host the same way, each the empty
/// string when the section names none.
#[test]
fn the_instances_row_is_the_struct_with_the_server_profile() {
    assert_eq!(
        <InstanceRow as Type>::SIGNATURE.to_string(),
        "(sssqasassss)",
        "the row is a struct of two strings, the profile, a uint16, two arrays of \
         strings, the label, the instance profile and the host"
    );
}

/// The row of the device list (G2, ARCHITECTURE.md 1, "Wire"): one row of
/// the `a(ssstt)` `Devices()` answers, the id `device` takes, the product
/// name, the backend and the two sizes as uint64. The sizes are `t`, not
/// `q` or `i`: a card's total in MiB grows past a uint16, and a signed type
/// has no reading for a device with no memory.
#[test]
fn the_devices_row_is_the_struct_of_the_wire() {
    assert_eq!(
        <DeviceRow as Type>::SIGNATURE.to_string(),
        "(ssstt)",
        "the row is a struct of three strings and two uint64"
    );
}

/// The whole reply, widened by R1 (ARCHITECTURE.md (1) "The node
/// inventory", slice R1 "Wire"): `(t probed_at, a(ssstt) devices,
/// t ram_mib, u cpu_threads)`. The RAM is `t` like the card sizes; the
/// thread count is `u`, a uint32, the width the bff's `cpuThreads` reads.
/// Additive at the end, so a reader of the list alone reads the fields it
/// knows and never moves.
#[test]
fn the_devices_reply_is_the_list_with_the_inventory() {
    assert_eq!(
        <(u64, Vec<DeviceRow>, u64, u32) as Type>::SIGNATURE.to_string(),
        "(ta(ssstt)tu)",
        "the probe's moment, the rows, the MiB of RAM, the thread count"
    );
}

/// The row of the node's GPU list (AN3, `ARCHITECTURE.md` "About and node
/// info"): `(s pci, s vendor, s name, s driver, s version, b has_vulkan,
/// s v_driver, s v_info)`, the list `a(sssssbss)`. Every text is `s` and
/// never a variant: the bus has no null, so the empty string carries "the
/// host did not answer this", and the Vulkan pair needs its own `b`
/// because an empty driver name is not the same as no Vulkan device.
#[test]
fn the_node_gpu_row_is_the_struct_of_the_wire() {
    assert_eq!(
        <NodeGpuRow as Type>::SIGNATURE.to_string(),
        "(sssssbss)",
        "the five texts, the Vulkan flag, the Vulkan driver and its info"
    );
}

/// The whole `NodeInfo()` reply: `(s name, s version, s kernel,
/// s vulkan, a(sssssbss) gpus)`. The three OS texts, the one Vulkan word
/// (`ok`, `none` or `failed`), then the rows in the manager's order.
#[test]
fn the_node_info_reply_is_the_document_of_the_wire() {
    assert_eq!(
        <NodeInfoRow as Type>::SIGNATURE.to_string(),
        "(ssssa(sssssbss))",
        "the OS, the Vulkan word, the GPU rows"
    );
}
