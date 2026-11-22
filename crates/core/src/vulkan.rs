//! The Vulkan driver of each GPU, as the manager's child prints it, and its
//! join with the kernel's rows (ARCHITECTURE.md "About and node info", slice
//! AN2).
//!
//! The child is `pai-manager vulkan-probe`: it opens the platform's Vulkan
//! loader and prints ONE JSON document on stdout. The manager runs it, capped
//! and bounded, and parses that text as untrusted input, so every rule here is
//! a cap and a fallback, never an error: a document over `DOC_MAX_BYTES`, one
//! that is not JSON, one whose status is not a word of the contract, and a
//! child that answered nothing, are all `Status::Failed`; a loader or an ICD
//! that is not there is `Status::None`; a text longer than `TEXT_MAX_BYTES` is
//! no value and becomes `None` in its row, and so is a value that is not a
//! string. Nothing is guessed at.
//!
//! The document (the contract the child writes and `parse` reads, the field
//! names the tests pin):
//!
//! ```json
//! {"vulkan": "ok", "gpus": [
//!   {"pci": "0000:03:00.0", "name": "AMD Radeon AI PRO R9700",
//!    "type": "discrete-gpu", "driver": "RADV", "info": "Mesa 25.2.0"}]}
//! ```
//!
//! `vulkan` is `"ok"`, `"none"` or `"failed"` (the same three words the node's
//! info sends). One row per physical device, of any type: the child is the one
//! that reads them, so `parse` keeps what it printed and `join` is what skips a
//! device of type `cpu`. `pci` is the bus id in the form sysfs names a device
//! (`domain:bus:device.function`, lower-case hex), so it matches
//! [`crate::host::GpuDriver`]'s as it stands; `null` for a device with no
//! `VK_EXT_pci_bus_info`, which cannot be joined. `driver` and `info` are the
//! driver's name and its readable version (`driverInfo`); `driverVersion` is
//! not sent, its encoding is the vendor's. A key the child leaves out is the
//! same as a value too long: `None`.
//!
//! The types and the caps below are the contract the tests of this file and of
//! `pai-manager`'s `tests/vulkan_probe.rs` pin.

use crate::host::{GpuDriver, MAX_GPU_ROWS};

/// The most bytes of one document the parser reads: the child's stdout cap
/// (`proc`'s stream cap), so what the reader keeps is a whole document or
/// nothing of it. Text over it is refused, not truncated and read.
pub const DOC_MAX_BYTES: usize = 4 << 20;

/// The most bytes of one text of the document, at and under which it is kept:
/// a driver's name and version are short, and a longer text is not the driver's
/// own. Measured in bytes of the UTF-8 text.
pub const TEXT_MAX_BYTES: usize = 256;

/// The three words the document answers in its `vulkan` key: the ones the child
/// writes and `parse` reads.
pub const STATUS_OK: &str = "ok";

/// A node with no loader and no ICD answers this.
pub const STATUS_NONE: &str = "none";

/// A read that gave nothing to trust answers this.
pub const STATUS_FAILED: &str = "failed";

/// The word a status answers: the one the child prints, the one the node's
/// info carries over the stream, and the one the bus carries in its place
/// (the bus has no null, and the word is text).
pub fn word(status: Status) -> &'static str {
    match status {
        Status::Ok => STATUS_OK,
        Status::None => STATUS_NONE,
        Status::Failed => STATUS_FAILED,
    }
}

/// The status a word answers: `None` for a text that is not one of the
/// three, which is a document that says nothing about the node.
pub fn status_of(word: &str) -> Option<Status> {
    match word {
        STATUS_OK => Some(Status::Ok),
        STATUS_NONE => Some(Status::None),
        STATUS_FAILED => Some(Status::Failed),
        _ => None,
    }
}

/// What the child's read of the node is: the loader and its ICDs answered
/// (`Ok`), there is no loader or no ICD at all (`None`), or the read gave
/// nothing to trust (`Failed`): killed at its bound, crashed, printing text
/// that is not the document.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Status {
    Ok,
    None,
    Failed,
}

/// One physical device the child printed, every text `None` when the child
/// printed none of it (the key missing, a value over the cap, a value that is
/// not a string).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Device {
    /// The PCI bus id as sysfs names it (`0000:03:00.0`), `None` for a device
    /// with no `VK_EXT_pci_bus_info`.
    pub pci: Option<String>,
    /// `deviceName`.
    pub name: Option<String>,
    /// `driverName`.
    pub driver: Option<String>,
    /// `driverInfo`: the driver's readable version.
    pub info: Option<String>,
    /// True for a device of type `cpu`, which the join leaves out.
    pub cpu: bool,
}

/// The whole answer of one child: its status, and the devices it printed.
/// `Failed` and `None` carry no devices: a read that gave nothing to trust
/// reads no GPU.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Probe {
    pub status: Status,
    pub devices: Vec<Device>,
}

/// The Vulkan driver of one GPU, the two texts the node's info sends.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Driver {
    pub driver: Option<String>,
    pub info: Option<String>,
}

/// One GPU of the node as the node's info sends it: the kernel's row with the
/// Vulkan driver joined onto it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NodeGpu {
    /// The PCI bus id, `None` for a Vulkan-only row (a device the kernel tree
    /// does not list, or one with no bus id).
    pub pci: Option<String>,
    /// The PCI vendor id as 4 lower-case hex digits, from the kernel's row.
    pub vendor: Option<String>,
    /// The product name: the kernel's when it has one (Windows), else the
    /// Vulkan device's (Linux, where the kernel names nothing).
    pub name: Option<String>,
    /// The kernel driver bound to the device, `None` on a Vulkan-only row.
    pub driver: Option<String>,
    /// The kernel driver's version, `None` on a Vulkan-only row and on a module
    /// that sets no version.
    pub version: Option<String>,
    /// The Vulkan driver of this GPU, `None` when no Vulkan device was joined
    /// onto it, and on every row when the child answered `None` or `Failed`.
    pub vulkan: Option<Driver>,
}

/// Read the child's document. See the module header for the document, the caps
/// and what each failure is.
pub fn parse(text: &str) -> Probe {
    // A document over the cap was cut on the way here: what is left is the
    // first bytes of it, which is no document at all, so it is refused whole.
    if text.len() > DOC_MAX_BYTES {
        return failed();
    }
    let Ok(value) = serde_json::from_str::<serde_json::Value>(text) else {
        return failed();
    };
    // The status is one of the contract's three words, exactly: `"OK"`, a
    // number, a `null` and a missing key all say nothing about the node.
    let status = match value
        .get("vulkan")
        .and_then(serde_json::Value::as_str)
        .and_then(status_of)
    {
        Some(status) => status,
        None => return failed(),
    };
    // A `gpus` that is not a list is no device list; one the child left out is
    // an empty one (a node whose every device is of type `cpu`).
    let gpus: &Vec<serde_json::Value> = match value.get("gpus") {
        None | Some(serde_json::Value::Null) => &NO_GPUS,
        Some(serde_json::Value::Array(gpus)) => gpus,
        Some(_) => return failed(),
    };
    // A status that says nothing reads no GPU: the rows beside it are not taken.
    let devices = if status == Status::Ok {
        gpus.iter().map(device).collect()
    } else {
        Vec::new()
    };
    Probe { status, devices }
}

/// The answer of a document that is refused: `Failed`, and no device.
fn failed() -> Probe {
    Probe {
        status: Status::Failed,
        devices: Vec::new(),
    }
}

/// A document with no `gpus` key: the child printed no device.
const NO_GPUS: Vec<serde_json::Value> = Vec::new();

/// One row of the document as a device. The row is kept whatever it holds: the
/// child read the device, and `join` is what leaves a device out.
fn device(gpu: &serde_json::Value) -> Device {
    Device {
        pci: text(gpu, "pci"),
        name: text(gpu, "name"),
        driver: text(gpu, "driver"),
        info: text(gpu, "info"),
        // `type` is Vulkan's device type and only `cpu` is of interest here;
        // its spelling is the loader's, so its case is dropped.
        cpu: text(gpu, "type").is_some_and(|kind| kind.eq_ignore_ascii_case("cpu")),
    }
}

/// The text of one key of a row: a string of at most [`TEXT_MAX_BYTES`] bytes.
/// A missing key, a `null`, a value that is not a string and a text over the
/// cap are all no value, so `None`: nothing is guessed at and nothing is cut
/// short of the cap.
fn text(gpu: &serde_json::Value, key: &str) -> Option<String> {
    let value = gpu.get(key)?.as_str()?;
    (value.len() <= TEXT_MAX_BYTES).then(|| value.to_string())
}

/// Put the child's devices onto the kernel's rows: a device whose bus id
/// matches a row fills that row's `vulkan` and, when the row has no name, its
/// name; a device that matches no row is appended as a row of its own, with
/// `driver` and `version` `None`; a device of type `cpu` is left out. The
/// kernel's rows keep their order and come first, the appended ones follow.
/// When the child answered `None` or `Failed` every row keeps `vulkan: None`
/// and nothing is appended: an unread Vulkan is a hole in the page, never a
/// row of its own. The answer is at most `MAX_GPU_ROWS` rows, the kernel
/// reader's own cap: over it the rows that fall off are the appended devices at
/// the end, the kernel's rows being first.
pub fn join(kernel: &[GpuDriver], probe: &Probe) -> Vec<NodeGpu> {
    // A read that gave nothing to trust joins nothing, and a `cpu` device (a
    // software ICD) is no driver of a card: both are out before the matching.
    let devices: Vec<&Device> = if probe.status == Status::Ok {
        probe.devices.iter().filter(|device| !device.cpu).collect()
    } else {
        Vec::new()
    };
    let mut rows: Vec<NodeGpu> = Vec::new();
    let mut taken = vec![false; devices.len()];
    for row in kernel {
        // The bus id is what ties the two readings of one card: the kernel's is
        // sysfs' name of the device and the child's is in that form. A device
        // with no bus id matches no row.
        let found = devices
            .iter()
            .position(|device| device.pci.as_deref() == Some(row.pci.as_str()));
        let device = found.map(|at| {
            taken[at] = true;
            devices[at]
        });
        // A Linux kernel row names nothing: the device's `deviceName` is the
        // only name the node has. Windows names its devices, and that name wins.
        let name = row
            .name
            .clone()
            .or_else(|| device.and_then(|device| device.name.clone()));
        rows.push(NodeGpu {
            pci: Some(row.pci.clone()),
            vendor: row.vendor.clone(),
            name,
            driver: row.driver.clone(),
            version: row.version.clone(),
            vulkan: device.map(driver),
        });
    }
    // A device no kernel row carries is a card of its own: the kernel reader
    // missed it (no display class, a tree it could not read), and it is on the
    // node.
    for (at, device) in devices.iter().enumerate() {
        if taken[at] {
            continue;
        }
        rows.push(NodeGpu {
            pci: device.pci.clone(),
            vendor: None,
            name: device.name.clone(),
            driver: None,
            version: None,
            vulkan: Some(driver(device)),
        });
    }
    // The page's one bound, the kernel reader's own cap: the appended devices
    // are the rows that fall off, the kernel's rows being first.
    rows.truncate(MAX_GPU_ROWS);
    rows
}

/// The Vulkan driver of one device, as the node's info sends it.
fn driver(device: &Device) -> Driver {
    Driver {
        driver: device.driver.clone(),
        info: device.info.clone(),
    }
}

#[cfg(test)]
mod an2_vulkan_parser {
    use super::{parse, Device, Status, DOC_MAX_BYTES, TEXT_MAX_BYTES};

    /// The document of a node with two cards: the child's keys as the module
    /// header writes them, and one key of its own (`driverID`) that the reader
    /// ignores.
    const TWO_CARDS: &str = r#"{"vulkan":"ok","gpus":[
      {"pci":"0000:03:00.0","name":"AMD Radeon AI PRO R9700","type":"discrete-gpu","driver":"RADV","info":"Mesa 25.2.0","driverID":4},
      {"pci":"0000:05:00.0","name":"NVIDIA GeForce RTX 4070 SUPER","type":"discrete-gpu","driver":"NVIDIA","info":"570.124.04"}
    ]}"#;

    /// The same document with one device whose `info` is `text`.
    fn one_device(info: &str) -> String {
        format!(
            r#"{{"vulkan":"ok","gpus":[{{"pci":"0000:03:00.0","name":"R7900","type":"discrete-gpu","driver":"RADV","info":"{info}"}}]}}"#
        )
    }

    fn device(info: &str) -> Device {
        let probe = parse(&one_device(info));
        assert_eq!(
            probe.devices.len(),
            1,
            "the document has one device: {probe:?}"
        );
        probe.devices[0].clone()
    }

    // ── A fixture document becomes rows ───────────────────────────────────

    #[test]
    fn a_fixture_document_becomes_rows() {
        let probe = parse(TWO_CARDS);
        assert_eq!(probe.status, Status::Ok, "the child read the node");
        assert_eq!(probe.devices.len(), 2, "two devices: {probe:?}");
        assert_eq!(probe.devices[0].pci.as_deref(), Some("0000:03:00.0"));
        assert_eq!(
            probe.devices[0].name.as_deref(),
            Some("AMD Radeon AI PRO R9700"),
            "`name` is the device's deviceName"
        );
        assert_eq!(probe.devices[0].driver.as_deref(), Some("RADV"));
        assert_eq!(probe.devices[0].info.as_deref(), Some("Mesa 25.2.0"));
        assert!(!probe.devices[0].cpu, "a discrete card is no CPU");
        assert_eq!(probe.devices[1].pci.as_deref(), Some("0000:05:00.0"));
        assert_eq!(probe.devices[1].driver.as_deref(), Some("NVIDIA"));
        assert_eq!(probe.devices[1].info.as_deref(), Some("570.124.04"));
    }

    #[test]
    fn a_device_below_1_2_has_no_driver_text() {
        // A device under Vulkan 1.2 with no `VK_KHR_driver_properties`: the row
        // is there, its `driver` and `info` are not.
        let probe = parse(
            r#"{"vulkan":"ok","gpus":[{"pci":"0000:03:00.0","name":"Old Card","type":"discrete-gpu"}]}"#,
        );
        assert_eq!(probe.status, Status::Ok);
        assert_eq!(probe.devices.len(), 1);
        assert_eq!(probe.devices[0].name.as_deref(), Some("Old Card"));
        assert_eq!(
            probe.devices[0].driver, None,
            "a key the child left out is no value"
        );
        assert_eq!(probe.devices[0].info, None);
    }

    #[test]
    fn a_node_with_no_gpu_is_ok_with_no_rows() {
        // Every device of the node is of type `cpu` (a software ICD), so the
        // child printed no row: the read succeeded and the node has no GPU.
        let probe = parse(r#"{"vulkan":"ok","gpus":[]}"#);
        assert_eq!(probe.status, Status::Ok);
        assert_eq!(probe.devices, Vec::new());
        // The `gpus` key missing is the same node: nothing printed.
        assert_eq!(parse(r#"{"vulkan":"ok"}"#).devices, Vec::new());
    }

    #[test]
    fn the_cpu_type_device_is_kept_by_the_parser_and_flagged() {
        // The child prints every physical device: the parser keeps what it
        // printed and the join is what leaves the CPU one out, so the type
        // reaches the join.
        let probe = parse(
            r#"{"vulkan":"ok","gpus":[{"pci":"0000:00:02.0","name":"llvmpipe (LLVM 19)","type":"cpu","driver":"llvmpipe","info":"Mesa 25.2.0"}]}"#,
        );
        assert_eq!(probe.devices.len(), 1);
        assert!(probe.devices[0].cpu, "`type: cpu` is a CPU device");
    }

    #[test]
    fn the_none_document_reads_no_device() {
        // No loader, or `vkCreateInstance` answering `VK_ERROR_INCOMPATIBLE_DRIVER`.
        for text in [
            r#"{"vulkan":"none"}"#,
            r#"{"vulkan":"none","gpus":[{"pci":"0000:03:00.0","name":"R7900","type":"discrete-gpu"}]}"#,
        ] {
            let probe = parse(text);
            assert_eq!(probe.status, Status::None, "{text:?}");
            assert_eq!(probe.devices, Vec::new(), "{text:?}");
        }
    }

    // ── Refused as `failed` ───────────────────────────────────────────────

    #[test]
    fn a_document_over_the_cap_is_failed() {
        // Text over the cap is a document the reader cannot trust: it is
        // refused whole, not read up to the cut.
        let over = format!(
            r#"{{"vulkan":"ok","gpus":[{{"pci":"0000:03:00.0","name":"{}","type":"discrete-gpu"}}]}}"#,
            "n".repeat(DOC_MAX_BYTES)
        );
        assert!(over.len() > DOC_MAX_BYTES, "the fixture is over the cap");
        let probe = parse(&over);
        assert_eq!(probe.status, Status::Failed, "over the cap is failed");
        assert_eq!(probe.devices, Vec::new(), "a refused document reads no GPU");
    }

    #[test]
    fn a_document_at_the_cap_is_read() {
        // The cap is inclusive: the parser that refuses text of exactly the
        // cap's size cannot read the child's largest document.
        let pad = DOC_MAX_BYTES - 200;
        let text = format!(
            r#"{{"vulkan":"ok","gpus":[{{"pci":"0000:03:00.0","name":"{}","type":"discrete-gpu"}}]}}"#,
            "n".repeat(pad)
        );
        assert!(
            text.len() <= DOC_MAX_BYTES,
            "the fixture is at the cap: {} bytes, cap {DOC_MAX_BYTES}",
            text.len()
        );
        assert_eq!(parse(&text).status, Status::Ok);
    }

    #[test]
    fn what_is_not_a_document_is_failed() {
        // A child that printed a log line, a crash's empty stdout, JSON that is
        // not an object, an object with no status, a status that is not one of
        // the three words, and a truncated document: all of them are failed.
        for text in [
            "",
            "\n",
            "I have no Vulkan loader",
            "[]",
            "\"ok\"",
            "5",
            "null",
            r#"{"gpus":[]}"#,
            r#"{"vulkan":null,"gpus":[]}"#,
            r#"{"vulkan":0,"gpus":[]}"#,
            r#"{"vulkan":"maybe","gpus":[]}"#,
            r#"{"vulkan":"OK","gpus":[]}"#,
            r#"{"vulkan":"ok","gpus":{"pci":"0000:03:00.0"}}"#,
            r#"{"vulkan":"ok","gpus":[{"pci":"0000:03:00.""#,
        ] {
            assert_eq!(
                parse(text).status,
                Status::Failed,
                "{text:?} is no document"
            );
        }
    }

    #[test]
    fn a_failed_document_carries_no_devices() {
        // Refusing is not taking the rows that parsed: a document with a bad
        // status and rows of its own is refused whole.
        let probe = parse(
            r#"{"vulkan":"failed","gpus":[{"pci":"0000:03:00.0","name":"R7900","type":"discrete-gpu"}]}"#,
        );
        assert_eq!(probe.status, Status::Failed);
        assert_eq!(probe.devices, Vec::new());
    }

    // ── The texts' cap: 256 bytes, the rest is no value ───────────────────

    #[test]
    fn a_text_over_256_bytes_is_cut_to_null() {
        let over = "M".repeat(TEXT_MAX_BYTES + 1);
        let d = device(&over);
        assert_eq!(d.info, None, "a text over the cap is no value");
        // The row it sits in is kept, with its other texts.
        assert_eq!(d.pci.as_deref(), Some("0000:03:00.0"));
        assert_eq!(d.name.as_deref(), Some("R7900"));
        assert_eq!(d.driver.as_deref(), Some("RADV"));
    }

    #[test]
    fn a_text_of_exactly_256_bytes_is_kept() {
        let at = "M".repeat(TEXT_MAX_BYTES);
        assert_eq!(device(&at).info.as_deref(), Some(at.as_str()));
    }

    #[test]
    fn the_cap_counts_bytes_not_characters() {
        // 128 Cyrillic characters are 256 bytes and stay; one more character is
        // 259 bytes and goes.
        let at = "д".repeat(TEXT_MAX_BYTES / 2);
        assert_eq!(at.len(), TEXT_MAX_BYTES, "the fixture is 256 bytes");
        assert_eq!(device(&at).info.as_deref(), Some(at.as_str()));
        assert_eq!(device(&format!("{at}д")).info, None);
    }

    #[test]
    fn every_text_of_the_row_is_capped() {
        // The cap is the rule for each of the document's texts, not one field's:
        // a bus id, a name and a driver name over it are no value either.
        let long = "f".repeat(TEXT_MAX_BYTES + 1);
        let probe = parse(&format!(
            r#"{{"vulkan":"ok","gpus":[{{"pci":"{long}","name":"{long}","type":"discrete-gpu","driver":"{long}","info":"{long}"}}]}}"#
        ));
        assert_eq!(
            probe.status,
            Status::Ok,
            "the document is well formed and short of the cap"
        );
        let d = &probe.devices[0];
        assert_eq!(d.pci, None, "a bus id over the cap is no bus id");
        assert_eq!(d.name, None);
        assert_eq!(d.driver, None);
        assert_eq!(d.info, None);
    }

    #[test]
    fn a_value_that_is_not_a_string_is_no_value() {
        // Fails closed: a number, a bool, an object, an array where a text is
        // expected, and a JSON `null`, are all read as no value, never as the
        // text of the number.
        let probe = parse(
            r#"{"vulkan":"ok","gpus":[{"pci":3,"name":true,"type":"discrete-gpu","driver":{"a":1},"info":["Mesa"]}]}"#,
        );
        assert_eq!(
            probe.status,
            Status::Ok,
            "the device is there, its texts are not"
        );
        let d = &probe.devices[0];
        assert_eq!(d.pci, None);
        assert_eq!(d.name, None);
        assert_eq!(d.driver, None);
        assert_eq!(d.info, None);
        assert!(!d.cpu, "a device whose type is missing is no CPU");
    }

    #[test]
    fn the_type_is_read_whatever_its_case_and_an_unknown_one_is_no_cpu() {
        for (text, cpu) in [
            (r#"{"vulkan":"ok","gpus":[{"type":"cpu"}]}"#, true),
            (r#"{"vulkan":"ok","gpus":[{"type":"CPU"}]}"#, true),
            (r#"{"vulkan":"ok","gpus":[{"type":"discrete-gpu"}]}"#, false),
            (
                r#"{"vulkan":"ok","gpus":[{"type":"integrated-gpu"}]}"#,
                false,
            ),
            (r#"{"vulkan":"ok","gpus":[{"type":"virtual"}]}"#, false),
            (r#"{"vulkan":"ok","gpus":[{"type":"llvmpipe"}]}"#, false),
        ] {
            let probe = parse(text);
            assert_eq!(probe.devices.len(), 1, "{text:?}");
            assert_eq!(probe.devices[0].cpu, cpu, "{text:?}");
        }
    }
}

#[cfg(test)]
mod an2_vulkan_join {
    use super::{join, Device, Probe, Status};
    use crate::host::GpuDriver;

    /// The kernel's row of one amdgpu card, as AN1's reader answers it on Linux
    /// (no name, the DKMS module's version).
    fn kernel(pci: &str, driver: Option<&str>, version: Option<&str>) -> GpuDriver {
        GpuDriver {
            pci: pci.to_string(),
            vendor: Some("1002".to_string()),
            name: None,
            driver: driver.map(|d| d.to_string()),
            version: version.map(|v| v.to_string()),
        }
    }

    /// A device as the parser answers it.
    fn device(pci: Option<&str>, name: Option<&str>, driver: Option<&str>, cpu: bool) -> Device {
        Device {
            pci: pci.map(|p| p.to_string()),
            name: name.map(|n| n.to_string()),
            driver: driver.map(|d| d.to_string()),
            info: Some("Mesa 25.2.0".to_string()),
            cpu,
        }
    }

    fn ok(devices: Vec<Device>) -> Probe {
        Probe {
            status: Status::Ok,
            devices,
        }
    }

    #[test]
    fn a_device_whose_bus_id_matches_fills_the_kernel_row() {
        let rows = join(
            &[kernel("0000:03:00.0", Some("amdgpu"), Some("7.1.9"))],
            &ok(vec![device(
                Some("0000:03:00.0"),
                Some("AMD Radeon AI PRO R9700"),
                Some("RADV"),
                false,
            )]),
        );
        assert_eq!(rows.len(), 1, "one card, one row: {rows:?}");
        assert_eq!(rows[0].pci.as_deref(), Some("0000:03:00.0"));
        assert_eq!(rows[0].vendor.as_deref(), Some("1002"));
        assert_eq!(
            rows[0].driver.as_deref(),
            Some("amdgpu"),
            "the kernel driver stays the row's own"
        );
        assert_eq!(rows[0].version.as_deref(), Some("7.1.9"));
        let vulkan = rows[0]
            .vulkan
            .clone()
            .expect("the matching device fills the row's vulkan");
        assert_eq!(vulkan.driver.as_deref(), Some("RADV"));
        assert_eq!(vulkan.info.as_deref(), Some("Mesa 25.2.0"));
        assert_eq!(
            rows[0].name.as_deref(),
            Some("AMD Radeon AI PRO R9700"),
            "a Linux kernel row has no name: the device's deviceName fills it"
        );
    }

    #[test]
    fn a_kernel_row_that_has_a_name_keeps_it() {
        // Windows names its display devices; the Vulkan device's name does not
        // replace the kernel's.
        let mut row = kernel("0000:03:00.0", Some("amdkmdag"), None);
        row.name = Some("Radeon RX 7900".to_string());
        let rows = join(
            &[row],
            &ok(vec![device(
                Some("0000:03:00.0"),
                Some("AMD Radeon Graphics"),
                Some("AMD proprietary driver"),
                false,
            )]),
        );
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].name.as_deref(), Some("Radeon RX 7900"));
        assert_eq!(
            rows[0]
                .vulkan
                .as_ref()
                .expect("the device is joined")
                .driver
                .as_deref(),
            Some("AMD proprietary driver")
        );
    }

    #[test]
    fn an_unmatched_device_is_appended_as_its_own_row() {
        // A card the kernel tree does not list (no display class, a card the
        // reader skipped): its own row, with no kernel driver and no version,
        // after the kernel's rows.
        let rows = join(
            &[kernel("0000:03:00.0", Some("amdgpu"), Some("7.1.9"))],
            &ok(vec![device(
                Some("0000:05:00.0"),
                Some("NVIDIA GeForce RTX 4070 SUPER"),
                Some("NVIDIA"),
                false,
            )]),
        );
        assert_eq!(
            rows.len(),
            2,
            "the kernel's row first, then the device's: {rows:?}"
        );
        assert_eq!(rows[0].pci.as_deref(), Some("0000:03:00.0"));
        assert_eq!(
            rows[0].vulkan, None,
            "the kernel's row with no device of its own has no vulkan driver"
        );
        assert_eq!(rows[1].pci.as_deref(), Some("0000:05:00.0"));
        assert_eq!(
            rows[1].name.as_deref(),
            Some("NVIDIA GeForce RTX 4070 SUPER")
        );
        assert_eq!(rows[1].driver, None, "an appended row has no kernel driver");
        assert_eq!(
            rows[1].version, None,
            "an appended row has no kernel version"
        );
        assert_eq!(
            rows[1].vendor, None,
            "the vendor id is the kernel's reading, which this card has none of"
        );
        assert_eq!(
            rows[1]
                .vulkan
                .as_ref()
                .expect("the device is its own row")
                .driver
                .as_deref(),
            Some("NVIDIA")
        );
    }

    #[test]
    fn a_device_with_no_bus_id_is_its_own_row() {
        // No `VK_EXT_pci_bus_info`: nothing to match on, so the device is a row.
        let rows = join(
            &[kernel("0000:03:00.0", Some("amdgpu"), None)],
            &ok(vec![device(
                None,
                Some("llvmpipe (LLVM 19.1)"),
                Some("llvmpipe"),
                false,
            )]),
        );
        assert_eq!(rows.len(), 2, "{rows:?}");
        assert_eq!(rows[0].pci.as_deref(), Some("0000:03:00.0"));
        assert_eq!(rows[0].vulkan, None);
        assert_eq!(
            rows[1].pci, None,
            "a device the kernel tree cannot be asked about has no bus id"
        );
        assert_eq!(
            rows[1]
                .vulkan
                .as_ref()
                .expect("the device is its own row")
                .driver
                .as_deref(),
            Some("llvmpipe")
        );
    }

    #[test]
    fn a_cpu_type_device_is_skipped() {
        // A software ICD (llvmpipe) reports the type `cpu`. Its bus id is a real
        // one, so a join that matched on the id alone would put a Mesa driver on
        // the kernel's card: the CPU device is left out, and its card keeps no
        // vulkan driver.
        let rows = join(
            &[kernel("0000:03:00.0", Some("amdgpu"), None)],
            &ok(vec![device(
                Some("0000:03:00.0"),
                Some("llvmpipe"),
                Some("llvmpipe"),
                true,
            )]),
        );
        assert_eq!(rows.len(), 1, "a CPU device is no row: {rows:?}");
        assert_eq!(rows[0].pci.as_deref(), Some("0000:03:00.0"));
        assert_eq!(
            rows[0].vulkan, None,
            "the CPU device does not fill the card it shares a bus id with"
        );
        assert_eq!(rows[0].name, None, "and does not name it");
    }

    #[test]
    fn two_devices_of_one_card_fill_one_row_each_side() {
        // One card, two kernel rows (a compute and a VGA function of the same
        // card are two devices): each row takes the device with ITS bus id.
        let rows = join(
            &[
                kernel("0000:03:00.0", Some("amdgpu"), None),
                kernel("0000:03:00.1", Some("amdgpu"), None),
            ],
            &ok(vec![device(
                Some("0000:03:00.0"),
                Some("AMD Radeon AI PRO R9700"),
                Some("RADV"),
                false,
            )]),
        );
        assert_eq!(rows.len(), 2, "{rows:?}");
        assert_eq!(
            rows[0]
                .vulkan
                .as_ref()
                .expect("the device with the same bus id")
                .driver
                .as_deref(),
            Some("RADV")
        );
        assert_eq!(
            rows[1].vulkan, None,
            "the function the device list does not carry keeps its hole"
        );
    }

    #[test]
    fn a_probe_that_failed_or_is_none_leaves_every_row_with_nothing() {
        // `failed` (a hung ICD) and `none` (no loader) are holes in the page:
        // the kernel's rows are answered, every one with `vulkan: null`, and the
        // rows the child printed, if any, join nothing.
        for status in [Status::Failed, Status::None] {
            let probe = Probe {
                status,
                devices: vec![device(
                    Some("0000:03:00.0"),
                    Some("AMD Radeon AI PRO R9700"),
                    Some("RADV"),
                    false,
                )],
            };
            let rows = join(
                &[
                    kernel("0000:03:00.0", Some("amdgpu"), Some("7.1.9")),
                    kernel("0000:05:00.0", None, None),
                ],
                &probe,
            );
            assert_eq!(
                rows.len(),
                2,
                "{status:?}: the kernel's rows, nothing appended: {rows:?}"
            );
            for row in &rows {
                assert_eq!(row.vulkan, None, "{status:?}: every row's vulkan is null");
            }
            assert_eq!(rows[0].pci.as_deref(), Some("0000:03:00.0"));
            assert_eq!(rows[0].version.as_deref(), Some("7.1.9"));
            assert_eq!(rows[1].driver, None, "an unbound device keeps its own hole");
        }
    }

    #[test]
    fn a_node_with_no_kernel_row_answers_the_devices_alone() {
        // The kernel reader found nothing (no `/sys`, a Windows host where
        // CfgMgr32 answered nothing): the devices are the node's GPUs.
        let rows = join(
            &[],
            &ok(vec![
                device(Some("0000:03:00.0"), Some("R7900"), Some("RADV"), false),
                device(None, Some("llvmpipe"), Some("llvmpipe"), false),
                device(Some("0000:07:00.0"), None, None, true),
            ]),
        );
        assert_eq!(rows.len(), 2, "the CPU device is out: {rows:?}");
        assert_eq!(rows[0].pci.as_deref(), Some("0000:03:00.0"));
        assert_eq!(rows[1].pci, None);
        assert_eq!(rows[1].name.as_deref(), Some("llvmpipe"));
    }

    /// The node's row bound (ARCHITECTURE.md, "About and node info": at most 32
    /// rows), the same cap the kernel reader applies to its own rows
    /// (`host::MAX_GPU_ROWS`). Written here as the number the design fixes: the
    /// join applies it to the rows it appends.
    const ROW_CAP: usize = 32;

    #[test]
    fn the_appended_rows_stop_at_the_cap() {
        // 30 kernel rows and 5 devices that match none of them: 35 rows to
        // write, 32 kept. The kernel's rows come first in their order and the
        // cap cuts the TAIL of the appended ones, so the last three devices
        // are the ones that fall off the page.
        let kernel: Vec<GpuDriver> = (0..30u32)
            .map(|i| kernel(&format!("0000:{i:02x}:00.0"), Some("amdgpu"), None))
            .collect();
        let probe = ok((0..5u32)
            .map(|i| {
                device(
                    Some(&format!("0000:{:02x}:00.0", 40 + i)),
                    Some("R7900"),
                    Some("RADV"),
                    false,
                )
            })
            .collect());
        let rows = join(&kernel, &probe);
        assert_eq!(
            rows.len(),
            ROW_CAP,
            "30 kernel rows plus 5 appended devices are 35 rows: the cap keeps \
             {ROW_CAP}, saw {}",
            rows.len()
        );
        // The whole list, by bus id: the kernel's 30 in their order, then the
        // appended ones in the device list's order, up to the cap.
        let kept: Vec<&str> = rows
            .iter()
            .map(|row| row.pci.as_deref().unwrap_or(""))
            .collect();
        let want: Vec<String> = (0..30u32)
            .map(|i| format!("0000:{i:02x}:00.0"))
            .chain((0..2u32).map(|i| format!("0000:{:02x}:00.0", 40 + i)))
            .collect();
        assert_eq!(
            kept,
            want.iter().map(String::as_str).collect::<Vec<_>>(),
            "the kernel's rows first, then the first two appended devices"
        );
        for row in rows.iter().take(30) {
            assert_eq!(
                row.driver.as_deref(),
                Some("amdgpu"),
                "the first 30 rows are the kernel's own: {}",
                row.pci.as_deref().unwrap_or("")
            );
            assert_eq!(
                row.vulkan, None,
                "no device matches a kernel row, so none of them is filled"
            );
        }
        for row in rows.iter().skip(30) {
            assert_eq!(row.vendor, None, "an appended row has no vendor id");
            assert_eq!(row.driver, None, "an appended row has no kernel driver");
            assert_eq!(
                row.vulkan.as_ref().map(|d| d.driver.as_deref()),
                Some(Some("RADV")),
                "an appended row is the device it came from"
            );
        }
        for i in 2..5u32 {
            let gone = format!("0000:{:02x}:00.0", 40 + i);
            assert!(
                !kept.contains(&gone.as_str()),
                "the device the cap cut out is named nowhere: {gone}"
            );
        }
    }
}
