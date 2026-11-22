//! The AN2 review's two defects in the Vulkan child (`manager/vulkan_child.rs`),
//! pinned as the decision the child must make, over a pure function: no test of
//! this crate opens a Vulkan loader, touches a GPU or runs
//! `pai-manager vulkan-probe`, so what is testable is the DECISION, not the
//! calls it drives.
//!
//! The registry (Khronos, the extension's own page): `VK_EXT_pci_bus_info` and
//! `VK_KHR_driver_properties` are DEVICE extensions, and
//! `VK_KHR_get_physical_device_properties2` is an INSTANCE extension, promoted
//! to core in Vulkan 1.1 (`VK_KHR_driver_properties` in 1.2). Today the child
//! looks for all three in the loader's INSTANCE list, so on a real node no
//! device ever gets a bus id, a driver name or driver info: the bus id is the
//! whole point of the document (AN4's join). Its `ApplicationInfo` is
//! `default()`, which asks no version above 1.0, while it calls the core-1.1
//! properties2.
//!
//! The contract for the coder, named in the review:
//! `vulkan_child::chain_plan(loader_version, instance_exts, device_exts,
//! device_api_version) -> ChainPlan { instance_api, use_props2_core,
//! use_props2_khr, pci, driver }`, with the instance's `apiVersion` and the two
//! properties2 routes read out of the plan, and the device's extension list
//! read from `enumerate_device_extension_properties`. `ChainPlan` and
//! `chain_plan` exist as a stub that panics: every test here is red on it.

use crate::manager::vulkan_child::chain_plan;
use ash::vk;
use std::ffi::CStr;

/// The three names, taken from the bindings so they are the registry's own.
const PCI: &CStr = vk::EXT_PCI_BUS_INFO_NAME;
const DRIVER: &CStr = vk::KHR_DRIVER_PROPERTIES_NAME;
const PROPS2: &CStr = vk::KHR_GET_PHYSICAL_DEVICE_PROPERTIES2_NAME;

/// The versions, packed as Vulkan packs them.
const V1_0: u32 = vk::make_api_version(0, 1, 0, 0);
const V1_1: u32 = vk::make_api_version(0, 1, 1, 0);
const V1_2: u32 = vk::make_api_version(0, 1, 2, 0);
const V1_3: u32 = vk::make_api_version(0, 1, 3, 0);

// ── The device's extensions (defect 1) ────────────────────────────────────

/// Defect 1, the exact shape of it: today the child finds these two names in
/// the loader's instance list and reads every device for them.
#[test]
fn an_instance_list_naming_the_device_extensions_plans_no_pci_and_no_driver() {
    let plan = chain_plan(Some(V1_1), &[PROPS2, PCI, DRIVER], &[], V1_1);

    assert!(
        !plan.pci,
        "`{}` is a DEVICE extension: an instance that names it says nothing about \
         a device, so no bus id may be read",
        PCI.to_str().expect("utf-8")
    );
    assert!(
        !plan.driver,
        "`{}` is a DEVICE extension: an instance that names it says nothing about \
         a device, so no driver text may be read",
        DRIVER.to_str().expect("utf-8")
    );
}

#[test]
fn a_device_that_lists_pci_bus_info_is_planned_for_pci() {
    let plan = chain_plan(Some(V1_1), &[], &[PCI], V1_1);

    assert!(
        plan.pci,
        "the device lists `{}`: its bus id is readable, and the bus id is what \
         ties the row to the kernel's row of the same card",
        PCI.to_str().expect("utf-8")
    );
}

#[test]
fn a_device_that_lists_driver_properties_is_planned_for_driver() {
    // A Vulkan 1.0 device carrying the extension (the shape of an older driver).
    let plan = chain_plan(Some(V1_1), &[PROPS2], &[DRIVER], V1_0);

    assert!(
        plan.driver,
        "the device lists `{}`: its driver name and info are readable",
        DRIVER.to_str().expect("utf-8")
    );
}

#[test]
fn a_device_of_api_1_2_is_planned_for_driver_without_the_extension() {
    // The driver properties are core from Vulkan 1.2, the extension promoted there.
    let plan = chain_plan(Some(V1_2), &[], &[], V1_2);

    assert!(
        plan.driver,
        "a device of Vulkan 1.2 answers the driver properties as core"
    );
}

#[test]
fn a_device_older_than_1_2_that_does_not_list_the_extension_is_planned_for_no_driver() {
    let plan = chain_plan(Some(V1_1), &[], &[], V1_1);

    assert!(
        !plan.driver,
        "a device of Vulkan 1.1 with no `{}` answers no driver text",
        DRIVER.to_str().expect("utf-8")
    );
}

// ── The loader's version (defect 2) ───────────────────────────────────────

#[test]
fn the_instance_is_asked_for_1_1_when_the_loader_is_1_1() {
    let plan = chain_plan(Some(V1_1), &[], &[PCI], V1_1);

    assert_eq!(
        plan.instance_api, V1_1,
        "the child calls the core properties2, which is Vulkan 1.1, so the \
         instance is asked for 1.1 (`ApplicationInfo::default()` asks no version \
         above 1.0)"
    );
    assert!(
        plan.use_props2_core,
        "a loader of 1.1 carries properties2 as core"
    );
    assert!(
        !plan.use_props2_khr,
        "with properties2 core the instance extension is not needed"
    );
}

#[test]
fn a_loader_newer_than_1_1_is_asked_for_1_1_not_for_its_own_version() {
    let plan = chain_plan(Some(V1_3), &[], &[PCI, DRIVER], V1_3);

    assert_eq!(
        plan.instance_api, V1_1,
        "asking 1.1 is what every loader from 1.1 up answers: a plan that asks \
         the loader's own version fails on a node for no reason"
    );
    assert!(plan.use_props2_core, "and reads properties2 as core");
    assert!(plan.pci, "the device's own list decides the reads");
}

#[test]
fn a_loader_of_1_0_is_asked_for_1_0_and_reads_properties2_only_by_the_instance_extension() {
    let plan = chain_plan(Some(V1_0), &[PROPS2], &[PCI], V1_0);

    assert_eq!(
        plan.instance_api, V1_0,
        "a 1.0 loader refuses an instance asked for 1.1 (`ERROR_INCOMPATIBLE_DRIVER`): \
         the child would read a node that has Vulkan as a node with none"
    );
    assert!(
        !plan.use_props2_core,
        "a 1.0 loader has no core properties2"
    );
    assert!(
        plan.use_props2_khr,
        "the instance carries `{}`: that is the only properties2 this node has",
        PROPS2.to_str().expect("utf-8")
    );
    assert!(plan.pci, "and the bus id is readable through it");
}

#[test]
fn a_loader_that_answers_no_version_is_a_1_0_loader() {
    // `Entry::try_enumerate_instance_version` answers `None` on a Vulkan 1.0 loader.
    let plan = chain_plan(None, &[PROPS2], &[PCI, DRIVER], V1_1);

    assert_eq!(plan.instance_api, V1_0, "no version is a 1.0 loader");
    assert!(!plan.use_props2_core, "no core properties2");
    assert!(
        plan.use_props2_khr,
        "the instance extension is the route, and the device's list still decides \
         the reads"
    );
    assert!(plan.pci);
    assert!(plan.driver);
}

/// A node with neither route: the row is name and type, as the module's header
/// says, and a row without a bus id is no row the join can use.
#[test]
fn with_no_properties2_the_device_is_planned_for_name_and_type_only() {
    let plan = chain_plan(None, &[], &[PCI, DRIVER], V1_2);

    assert!(
        !plan.use_props2_core && !plan.use_props2_khr,
        "a 1.0 loader with no `{}` has no properties2 at all",
        PROPS2.to_str().expect("utf-8")
    );
    assert!(
        !plan.pci,
        "the bus id is a properties2 chain: with no properties2 it is not read, \
         even for a device that lists `{}`",
        PCI.to_str().expect("utf-8")
    );
    assert!(!plan.driver, "the driver text is a properties2 chain too");
}

// ── The wiring (the source's text, comments left out) ─────────────────────

/// The decision is a pure function, so the registry's split is pinned above;
/// these two pins are that the child ACTS on it: it asks the device for its
/// extensions, and it asks the instance the plan's version. `read_devices` is
/// the child's whole reading of the node, and no test on this OS runs it (the
/// real child opens the node's loader, a hand run with the user's go-ahead).
#[test]
fn the_child_reads_the_extension_list_of_the_device() {
    let child = code(include_str!("../manager/vulkan_child.rs"));
    let read = free_fn(&child, "read_devices");

    assert!(
        read.contains("enumerate_device_extension_properties"),
        "`read_devices` never asks a device which extensions IT carries, so the \
         plan's device list is the instance's: no device of a real node gets a \
         bus id"
    );
}

#[test]
fn the_child_asks_the_instance_the_api_version_of_the_plan() {
    let child = code(include_str!("../manager/vulkan_child.rs"));
    let read = free_fn(&child, "read_devices");

    assert!(
        read.contains("api_version"),
        "the `ApplicationInfo` is `default()`, which asks no version, while the \
         child calls the core-1.1 properties2"
    );
    assert!(
        read.contains("instance_api"),
        "the version asked is the plan's, not a figure of the child's own"
    );
}

// ── The source readers ────────────────────────────────────────────────────

/// `src` with the whole-line comments left out, so a needle in prose greens
/// nothing.
fn code(src: &str) -> String {
    src.lines()
        .filter(|l| !l.trim_start().starts_with("//"))
        .collect::<Vec<_>>()
        .join("\n")
}

/// The text of the file-level `fn <name>`, from its signature to the brace
/// that closes it at column 0 (a child's functions are free functions, not
/// methods).
fn free_fn(src: &str, name: &str) -> String {
    for prefix in ["fn ", "pub fn ", "pub(crate) fn "] {
        let open = format!("\n{prefix}{name}(");
        if let Some(at) = src.find(&open) {
            let body = &src[at..];
            let end = body
                .find("\n}\n")
                .unwrap_or_else(|| panic!("the end of `fn {name}`"));
            return body[..end].to_string();
        }
    }
    panic!("no `fn {name}` at column 0 in this source");
}
