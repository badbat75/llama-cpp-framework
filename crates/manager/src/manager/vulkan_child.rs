//! The child of slice AN2: `pai-manager vulkan-probe`, the node's Vulkan
//! drivers printed as ONE JSON document on stdout, which
//! [`crate::manager::service::VulkanProbe`] reads back through
//! [`pai_core::vulkan::parse`].
//!
//! The child is its own process because it is the one that opens the platform's
//! Vulkan loader: `ash`'s loaded entry `dlopen`s it at run time, so the
//! manager's image needs no Vulkan at build time and starts on a node that has
//! none, and a driver that hangs or crashes takes the child down, where
//! `proc`'s bound kills it and its capped reader answers nothing, instead of
//! taking the manager, which serves every client, down with it
//! (ARCHITECTURE.md "About and node info", option (a)).
//!
//! The child answers and never fails: every reading that ends with no device
//! list is a word of the document, so the parent reads a node with no Vulkan and
//! a node whose driver refused as facts, and the manager's caller has no error
//! of the node to answer. The bus id of each device is the whole point of the
//! document: it is what ties a device to the kernel's row of the same card
//! (AN2's join), and a device whose Vulkan answers no bus id is printed all the
//! same, as a row the join leaves unjoined.

use ash::vk;
use serde_json::{Map, Value};
use std::ffi::{c_char, CStr};

/// The versions the plan compares, packed as Vulkan packs them.
const V1_0: u32 = vk::make_api_version(0, 1, 0, 0);
const V1_1: u32 = vk::make_api_version(0, 1, 1, 0);
const V1_2: u32 = vk::make_api_version(0, 1, 2, 0);

/// How the child's reading of the node ended when it ended with no device list.
enum Ended {
    /// No loader to open, or a loader with no driver willing to answer an
    /// instance: the node has no Vulkan.
    NoVulkan,
    /// A Vulkan call answered an error: the node's Vulkan is unread.
    Failed,
}

/// What the child asks of the loader, and what it reads of one device. The
/// decision is this crate's to state (the AN2 review): `VK_EXT_pci_bus_info`
/// and `VK_KHR_driver_properties` are DEVICE extensions, so a device is read
/// for them only when THAT device's own list names them, and the instance's
/// version is what makes the properties2 chain core or an instance extension.
pub(crate) struct ChainPlan {
    /// The `apiVersion` the instance is asked for.
    pub instance_api: u32,
    /// The properties2 call is core (a loader of Vulkan 1.1 or newer).
    pub use_props2_core: bool,
    /// The properties2 call is the instance extension
    /// `VK_KHR_get_physical_device_properties2` (a 1.0 loader that carries it).
    pub use_props2_khr: bool,
    /// The device's PCI bus id is read.
    pub pci: bool,
    /// The device's driver name and info are read.
    pub driver: bool,
}

/// The plan for one device of one loader. `loader_version` is the loader's own
/// instance version (`Entry::try_enumerate_instance_version`, `None` on a
/// Vulkan 1.0 loader, which answers no version at all); `instance_exts` and
/// `device_exts` are the extension names of the instance and of THIS device
/// (the device's from `Instance::enumerate_device_extension_properties`);
/// `device_api_version` is that device's `api_version`.
pub(crate) fn chain_plan(
    loader_version: Option<u32>,
    instance_exts: &[&CStr],
    device_exts: &[&CStr],
    device_api_version: u32,
) -> ChainPlan {
    // The properties2 call is core from Vulkan 1.1; a loader that answers no
    // version is a 1.0 loader, which has the call only as its instance
    // extension.
    let use_props2_core = loader_version.is_some_and(|version| version >= V1_1);
    let use_props2_khr =
        !use_props2_core && lists(instance_exts, vk::KHR_GET_PHYSICAL_DEVICE_PROPERTIES2_NAME);
    // Asking 1.1 is what every loader of 1.1 and up answers; a 1.0 loader
    // refuses an instance asked above 1.0 (`ERROR_INCOMPATIBLE_DRIVER`), which
    // would read a node that has Vulkan as a node with none.
    let instance_api = if use_props2_core { V1_1 } else { V1_0 };
    // The bus id and the driver's texts are properties of a properties2 chain,
    // so no properties2 call reads neither. The driver properties are core from
    // Vulkan 1.2, the extension promoted there.
    let props2 = use_props2_core || use_props2_khr;
    ChainPlan {
        instance_api,
        use_props2_core,
        use_props2_khr,
        pci: props2 && lists(device_exts, vk::EXT_PCI_BUS_INFO_NAME),
        driver: props2
            && (lists(device_exts, vk::KHR_DRIVER_PROPERTIES_NAME) || device_api_version >= V1_2),
    }
}

/// Whether `name` is one of an enumeration of extension names.
fn lists(names: &[&CStr], name: &CStr) -> bool {
    names.contains(&name)
}

/// The properties2 call of a plan: the node has one of these, and it is the
/// only way to chain a property onto a device's properties. A node with neither
/// answers no device at all, its read stopping before the devices: a row with no
/// bus id is no row the join can use.
enum Props2<'a> {
    /// Vulkan 1.1 and up: the core call.
    Core(&'a ash::Instance),
    /// A Vulkan 1.0 loader carrying `VK_KHR_get_physical_device_properties2`:
    /// the extension's call. A 1.0 loader answers no pointer for the core one,
    /// so the core call is never made on such a node.
    Khr(ash::khr::get_physical_device_properties2::Instance),
}

impl Props2<'_> {
    /// The properties2 call on `device`, filling `props` and whatever is
    /// chained onto it. A device is read through here only when its own plan
    /// says the node has the call.
    ///
    /// # Safety
    ///
    /// `props`, and every struct chained onto it, must outlive the call, which
    /// fills them in place, and `device` must be a device of the instance the
    /// call belongs to.
    unsafe fn read(
        &self,
        device: vk::PhysicalDevice,
        props: &mut vk::PhysicalDeviceProperties2<'_>,
    ) {
        match self {
            Props2::Core(instance) => {
                // SAFETY: the caller's rule, and the instance is live.
                unsafe { instance.get_physical_device_properties2(device, props) };
            }
            Props2::Khr(khr) => {
                // SAFETY: the caller's rule, and the instance is live.
                unsafe { khr.get_physical_device_properties2(device, props) };
            }
        }
    }
}

/// The child's whole run: the document on stdout, and success. The document
/// carries the answer whatever the node is, so the child has no failure of the
/// node to report; a Vulkan call that panics is the child's death, which the
/// parent reads as a read of nothing.
pub(crate) fn run() -> Result<(), (i32, String)> {
    let (status, devices) = match read_devices() {
        Ok(devices) if devices.is_empty() => (pai_core::vulkan::STATUS_NONE, Vec::new()),
        Ok(devices) => (pai_core::vulkan::STATUS_OK, devices),
        Err(Ended::NoVulkan) => (pai_core::vulkan::STATUS_NONE, Vec::new()),
        Err(Ended::Failed) => (pai_core::vulkan::STATUS_FAILED, Vec::new()),
    };
    let mut document = Map::new();
    document.insert("vulkan".to_string(), Value::String(status.to_string()));
    document.insert("gpus".to_string(), Value::Array(devices));
    let document = Value::Object(document);
    // One line, ended by a break: the writer flushes a line, so a child killed
    // at its bound leaves its answer behind, and the parser reads the text
    // around it as it stands.
    println!("{document}");
    Ok(())
}

/// The instance, destroyed when the child is done with it: the read ends on
/// every path (the rows, a list of none, the reading with no properties2, and
/// every Vulkan call that answered an error, which the read returns with `?`),
/// and the instance is destroyed on all of them.
struct InstanceGuard {
    instance: ash::Instance,
}

impl Drop for InstanceGuard {
    fn drop(&mut self) {
        // SAFETY: the guard owns the instance the child created of this entry
        // and is its only owner; it is declared after the entry, so it drops
        // before it, and no Vulkan call runs after this.
        unsafe { self.instance.destroy_instance(None) };
    }
}

/// The node's Vulkan devices as the document's rows, and the empty list when
/// the node has none to show.
fn read_devices() -> Result<Vec<Value>, Ended> {
    // SAFETY: `Entry::load` opens the platform's Vulkan loader. The entry owns
    // it and is dropped at the end of this function, after the instance it made
    // was destroyed, and no Vulkan call runs after that: ash's rule for a
    // loaded entry.
    let entry = unsafe { ash::Entry::load() }.map_err(|_| Ended::NoVulkan)?;
    // The loader's own instance version: `None` is a Vulkan 1.0 loader.
    let loader_version = unsafe { entry.try_enumerate_instance_version() }
        .ok()
        .flatten();
    // SAFETY: the entry is the loader the call opened and stays open, and the
    // call only reads its list of instance extensions.
    let listed = unsafe { entry.enumerate_instance_extension_properties(None) }
        .map_err(|_| Ended::Failed)?;
    let instance_exts = extension_names(&listed);
    // The instance's half of the plan, of the loader's version and the
    // extensions the loader carries as INSTANCE extensions: the device
    // arguments are the plan's device half, which the instance does not read.
    let plan = chain_plan(loader_version, &instance_exts, &[], V1_0);
    // Only `VK_KHR_get_physical_device_properties2` is ever enabled on the
    // instance, and only when the plan needs it: the bus id and the driver's
    // texts are not instance extensions, they are asked of each device.
    let mut enabled: Vec<&CStr> = Vec::new();
    if plan.use_props2_khr {
        enabled.push(vk::KHR_GET_PHYSICAL_DEVICE_PROPERTIES2_NAME);
    }
    let names: Vec<*const c_char> = enabled.iter().map(|name| name.as_ptr()).collect();
    let app = vk::ApplicationInfo {
        api_version: plan.instance_api,
        ..Default::default()
    };
    let create = vk::InstanceCreateInfo {
        p_application_info: &app,
        pp_enabled_extension_names: names.as_ptr(),
        enabled_extension_count: names.len() as u32,
        ..Default::default()
    };
    // SAFETY: the create info is the struct of the call, its extension names are
    // the loader's own, read out of the enumeration above, and every byte they
    // point at outlives the call.
    let instance = unsafe { entry.create_instance(&create, None) }.map_err(|err| {
        if err == vk::Result::ERROR_INCOMPATIBLE_DRIVER {
            Ended::NoVulkan
        } else {
            Ended::Failed
        }
    })?;
    let guard = InstanceGuard { instance };
    let instance = &guard.instance;
    // The properties2 call the plan chose. A node with neither has no chained
    // property to read, so no device of it can carry a bus id: the node answers
    // no device.
    let props2 = if plan.use_props2_core {
        Props2::Core(instance)
    } else if plan.use_props2_khr {
        Props2::Khr(ash::khr::get_physical_device_properties2::Instance::new(
            &entry, instance,
        ))
    } else {
        return Ok(Vec::new());
    };
    // SAFETY: the instance is live and the call reads its device list.
    let devices = unsafe { instance.enumerate_physical_devices() }.map_err(|_| Ended::Failed)?;
    let mut rows = Vec::new();
    for device in &devices {
        // SAFETY: the device is one of the instance's, and the call only reads
        // its fixed properties.
        let props = unsafe { instance.get_physical_device_properties(*device) };
        // A device of type `cpu` is a software driver (llvmpipe): it is no card
        // of the node, and the join is written to leave it out.
        if props.device_type == vk::PhysicalDeviceType::CPU {
            continue;
        }
        // This device's own extension list: a device of the same node can carry
        // an extension another of them does not. A device that answers no list
        // is read as a device that lists nothing, and its row keeps its name.
        // SAFETY: the device is one of the instance's, and the call only reads
        // the extensions the device lists.
        let listed =
            unsafe { instance.enumerate_device_extension_properties(*device) }.unwrap_or_default();
        let device_exts = extension_names(&listed);
        // The reads of THIS device, of its own list and its own version.
        let plan = chain_plan(
            loader_version,
            &instance_exts,
            &device_exts,
            props.api_version,
        );
        let mut row = Map::new();
        if plan.pci {
            put(&mut row, "pci", bus_id(&props2, *device));
        }
        put(&mut row, "name", text(props.device_name.as_ptr()));
        if plan.driver {
            let (driver, info) = driver_texts(&props2, *device);
            put(&mut row, "driver", driver);
            put(&mut row, "info", info);
        }
        rows.push(Value::Object(row));
    }
    Ok(rows)
}

/// The names of an extension enumeration, as the registry's C strings: a name
/// the bindings cannot read is left out.
fn extension_names(extensions: &[vk::ExtensionProperties]) -> Vec<&CStr> {
    let mut names: Vec<&CStr> = Vec::new();
    for extension in extensions {
        let Ok(name) = extension.extension_name_as_c_str() else {
            continue;
        };
        names.push(name);
    }
    names
}

/// `key` with `text` on it, in the row: a key the child leaves out is read as no
/// value by the parser, which is what a device that answers nothing gets.
fn put(row: &mut Map<String, Value>, key: &str, text: Option<String>) {
    if let Some(text) = text {
        row.insert(key.to_string(), Value::String(text));
    }
}

/// The device's PCI bus id, in the form sysfs names a PCI device
/// (`0000:03:00.0`, lower-case hex, the function a single digit 0 to 7): the
/// form of the kernel's row, which is the row this device has to be joined onto.
/// `VK_EXT_pci_bus_info` answers it, and the caller asks only for a device the
/// plan read the extension on.
fn bus_id(props2: &Props2, device: vk::PhysicalDevice) -> Option<String> {
    let mut pci = vk::PhysicalDevicePCIBusInfoPropertiesEXT::default();
    {
        let mut props = vk::PhysicalDeviceProperties2::default().push_next(&mut pci);
        // SAFETY: `props` is the struct of the call with `pci` chained onto it
        // and both of them live for the call, which fills them in place.
        unsafe { props2.read(device, &mut props) };
    }
    Some(format!(
        "{:04x}:{:02x}:{:02x}.{}",
        pci.pci_domain, pci.pci_bus, pci.pci_device, pci.pci_function
    ))
}

/// The driver's name and its readable version, from `VK_KHR_driver_properties`
/// (core from Vulkan 1.2, an extension before): the pair the node's page shows.
/// The caller asks only for a device the plan read the properties on, so a node
/// that cannot answer them is never read here.
fn driver_texts(props2: &Props2, device: vk::PhysicalDevice) -> (Option<String>, Option<String>) {
    let mut driver = vk::PhysicalDeviceDriverProperties::default();
    {
        let mut props = vk::PhysicalDeviceProperties2::default().push_next(&mut driver);
        // SAFETY: as in `bus_id`.
        unsafe { props2.read(device, &mut props) };
    }
    (
        text(driver.driver_name.as_ptr()),
        text(driver.driver_info.as_ptr()),
    )
}

/// The text of one of the fixed-size name arrays of a Vulkan struct: the driver
/// NUL-terminates such an array within its size. A name of nothing is no name,
/// so the key is left out rather than answered as an empty text.
fn text(name: *const c_char) -> Option<String> {
    // SAFETY: the caller passes the first character of one of those arrays, of
    // the size Vulkan fixes for it, so the bytes up to its NUL are there.
    let text = unsafe { CStr::from_ptr(name) }.to_string_lossy();
    (!text.is_empty()).then(|| text.into_owned())
}
