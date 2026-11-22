//! Slice AN3: `NodeInfo` / `NodeInfoAs` in the manager
//! (`manager::service::Manager::node_info`, ARCHITECTURE.md "About and node
//! info", "The manager method").
//!
//! One read answers the node's own document: `os` (its name, version and
//! kernel), `vulkan` (what the node's Vulkan read answered) and `gpus` (the
//! kernel's rows with the Vulkan driver joined onto the row whose bus id it
//! names). The gate is the gate of `devices`, because the read may spawn the
//! Vulkan child: the caller's own read is `control` asked WITHOUT interaction,
//! the `As` form is the bff's account vouching for a current member of `pai`,
//! and every refusal answers `NotAuthorized` with nothing read and no child
//! spawned.
//!
//! Every figure is answered by [`FakeNodeFacts`]: the fake reads no
//! `os-release`, walks no `/sys`, loads no Vulkan driver and spawns no child,
//! so a reply carrying a fixture text is the fake's answer, and the join is
//! `pai_core::vulkan::join` (AN2) driven through the manager. Nothing here
//! touches `paths::` (the read resolves no path), so these plain `#[test]`s run
//! in parallel beside the crate's `data_root_e2e`.
//!
//! Names the tests assume, the contract for the coder: the seam is
//! `service::NodeFacts` (`os`, `gpus`, `vulkan`), wired with
//! `Manager::with_node_facts`, whose default `NoNodeFacts` answers a hole; the
//! read is `Manager::node_info(&Requester)`.

use crate::manager::service::fakes::{
    FakeActionLog, FakeAuthority, FakeNodeFacts, FakePasswords, FakeResolver, FakeService,
};
use crate::manager::service::{Action, Caller, Manager, ManagerError, Principal, Requester};
use pai_core::client::NodeInfoReply;
use pai_core::host::OsInfo;
use pai_core::vulkan::{Driver, NodeGpu, Status};
use std::sync::Arc;

type Core = Manager<FakeService, FakeAuthority, FakeResolver>;

/// The seam's answer counter, shared so a test pins how much a call read.
type Asked = Arc<std::sync::Mutex<Vec<String>>>;

/// The bff's account on the bus, and the desktop user's.
fn bff() -> Caller {
    Caller(":1.9".into())
}

fn desktop() -> Caller {
    Caller(":1.42".into())
}

/// The bff vouching for `who`: the `As` form.
fn for_user(who: &str) -> Requester {
    Requester {
        caller: bff(),
        principal: Some(Principal(who.into())),
    }
}

/// The caller's own read, no principal.
fn own(caller: Caller) -> Requester {
    Requester {
        caller,
        principal: None,
    }
}

/// A manager with the node facts of `node`, polkit answering the prompting
/// check with `allow` and the quiet one with `quiet`, and the bff's account
/// known as the bff. The seam's `asked` list and the log's lines come back, so
/// a test pins how much a call read and what it recorded.
fn core(allow: bool, quiet: bool, node: FakeNodeFacts) -> (Core, Asked, Asked) {
    use crate::manager::service::fakes::FakeGroup;
    let asked = node.asked.clone();
    let log = FakeActionLog::new();
    let lines = log.lines.clone();
    let manager = Manager::new(
        FakeService::new(),
        FakeAuthority::new(allow).quietly(quiet).bff(&bff()),
        FakeResolver::lan(),
    )
    .with_group(FakeGroup::new())
    .with_passwords(FakePasswords::new())
    .with_log(log)
    .with_node_facts(node);
    (manager, asked, lines)
}

/// The node of the fixtures: Ubuntu 26.04 on a kernel no real host runs, two
/// kernel rows (an amdgpu with its module's version, an nvidia with none and
/// no name, as Linux names nothing), and the child answering `ok` with a
/// device on each row's bus id, one device with no bus id at all, and a CPU
/// device (llvmpipe), which is no driver of a card.
fn node(status: Status) -> FakeNodeFacts {
    FakeNodeFacts::new("Ubuntu", "26.04", "6.14.0-24-generic")
        .with_kernel_rows(&[
            ("0000:03:00.0", "1002", "", "amdgpu", "2.5.0"),
            ("0000:08:00.0", "10de", "", "nvidia", ""),
        ])
        .with_vulkan(
            status,
            &[
                (
                    "0000:03:00.0",
                    "AMD Radeon AI PRO R9700",
                    false,
                    "RADV",
                    "Mesa 25.2.0",
                ),
                (
                    "0000:08:00.0",
                    "NVIDIA GeForce RTX 4070 SUPER",
                    false,
                    "NVIDIA",
                    "560.35.03",
                ),
                ("", "Asus Vega", false, "RADV", "Mesa 25.1.0"),
                ("", "llvmpipe", true, "llvmpipe", "Mesa 25.2.0"),
            ],
        )
}

fn row(
    pci: Option<&str>,
    vendor: Option<&str>,
    name: Option<&str>,
    driver: Option<&str>,
    version: Option<&str>,
    vulkan: Option<(&str, &str)>,
) -> NodeGpu {
    let text = |one: Option<&str>| one.map(str::to_string);
    NodeGpu {
        pci: text(pci),
        vendor: text(vendor),
        name: text(name),
        driver: text(driver),
        version: text(version),
        vulkan: vulkan.map(|(driver, info)| Driver {
            driver: Some(driver.to_string()),
            info: Some(info.to_string()),
        }),
    }
}

/// The document of [`node`] when the child answered `ok`: the two kernel rows
/// with the Vulkan driver joined onto the row whose bus id the device names,
/// their names filled from `deviceName` (the kernel names nothing on Linux),
/// and the device with no bus id appended as its own row with no kernel
/// driver. The CPU device is not a row.
fn joined() -> NodeInfoReply {
    NodeInfoReply {
        os: OsInfo {
            name: Some("Ubuntu".into()),
            version: Some("26.04".into()),
            kernel: Some("6.14.0-24-generic".into()),
        },
        vulkan: Status::Ok,
        gpus: vec![
            row(
                Some("0000:03:00.0"),
                Some("1002"),
                Some("AMD Radeon AI PRO R9700"),
                Some("amdgpu"),
                Some("2.5.0"),
                Some(("RADV", "Mesa 25.2.0")),
            ),
            row(
                Some("0000:08:00.0"),
                Some("10de"),
                Some("NVIDIA GeForce RTX 4070 SUPER"),
                Some("nvidia"),
                None,
                Some(("NVIDIA", "560.35.03")),
            ),
            row(
                None,
                None,
                Some("Asus Vega"),
                None,
                None,
                Some(("RADV", "Mesa 25.1.0")),
            ),
        ],
    }
}

/// The bff's read for a signed-in member: the whole document, the OS the host
/// named, the Vulkan word the child answered, and the rows joined in bus id
/// order, kernel rows first. The read records no action and reaches no service
/// manager: it is a read.
#[test]
fn node_info_answers_the_os_the_vulkan_word_and_the_joined_rows() {
    let (m, asked, lines) = core(true, true, node(Status::Ok));
    assert_eq!(
        m.node_info(&for_user("alice")),
        Ok(joined()),
        "the document of the fakes, joined and in order"
    );
    assert_eq!(
        *asked.lock().unwrap(),
        vec!["os".to_string(), "gpus".to_string(), "vulkan".to_string()],
        "one read of each part"
    );
    assert_eq!(
        *m.service().calls.lock().unwrap(),
        Vec::<crate::manager::service::fakes::Call>::new(),
        "the read reaches no service manager"
    );
    assert_eq!(
        *lines.lock().unwrap(),
        Vec::<String>::new(),
        "a read records no action"
    );
    assert_eq!(
        *m.authority().asked.lock().unwrap(),
        Vec::<(Caller, Action)>::new(),
        "the pair is the right: polkit is asked for nothing, not even quietly"
    );
}

/// The caller's own read: `control` asked WITHOUT interaction, never with a
/// prompt (a node's drivers are worth no challenge over the user's session),
/// and the same document.
#[test]
fn the_callers_own_read_asks_control_quietly_and_never_with_a_prompt() {
    let (m, asked, _lines) = core(true, true, node(Status::Ok));
    assert_eq!(
        m.node_info(&own(bff())),
        Ok(joined()),
        "the bff's own read, asked quietly"
    );
    assert_eq!(
        *m.authority().asked_quietly.lock().unwrap(),
        vec![(bff(), Action::Control)],
        "control asked without interaction, as `devices` asks it"
    );
    assert_eq!(
        *m.authority().asked.lock().unwrap(),
        Vec::<(Caller, Action)>::new(),
        "never asked with a prompt"
    );
    assert_eq!(*asked.lock().unwrap(), vec!["os", "gpus", "vulkan"]);
}

/// A node that answers nothing is a hole, and the read still succeeds: no OS,
/// no kernel row, a Vulkan read that gave nothing to trust. A manager wired
/// with no seam spawns no child, so the default answers the hole and the page
/// shows it.
#[test]
fn a_node_that_answers_nothing_is_a_hole_and_not_an_error() {
    let manager = Manager::new(
        FakeService::new(),
        FakeAuthority::new(true).quietly(true).bff(&bff()),
        FakeResolver::lan(),
    )
    .with_passwords(FakePasswords::new());
    assert_eq!(
        manager.node_info(&own(desktop())),
        Ok(NodeInfoReply {
            os: OsInfo {
                name: None,
                version: None,
                kernel: None,
            },
            vulkan: Status::Failed,
            gpus: Vec::new(),
        }),
        "no seam: a hole, never an error, and no child"
    );
}

/// A Vulkan read of `none` (no loader, no ICD) answers the word and no row's
/// `vulkan`: the kernel's rows stay, each with `vulkan: null`, because the node
/// has GPUs and none of them has a Vulkan driver.
#[test]
fn a_vulkan_read_of_none_answers_the_word_and_no_rows_vulkan() {
    let (m, _asked, _lines) = core(
        true,
        true,
        FakeNodeFacts::new("Ubuntu", "26.04", "6.14.0-24-generic")
            .with_kernel_rows(&[("0000:03:00.0", "1002", "", "amdgpu", "")])
            .with_vulkan(Status::None, &[]),
    );
    assert_eq!(
        m.node_info(&for_user("alice")),
        Ok(NodeInfoReply {
            os: OsInfo {
                name: Some("Ubuntu".into()),
                version: Some("26.04".into()),
                kernel: Some("6.14.0-24-generic".into()),
            },
            vulkan: Status::None,
            gpus: vec![row(
                Some("0000:03:00.0"),
                Some("1002"),
                // The kernel names no device and the child printed none: the
                // name is a hole, never a guess, and the bus has no empty
                // text to answer with.
                None,
                Some("amdgpu"),
                None,
                None,
            )],
        }),
        "`none` says no Vulkan driver, and the kernel's row stands"
    );
}

/// A Vulkan read that gave nothing to trust (killed at its bound, crashed,
/// printing text that is not the document) answers `failed` with the kernel's
/// rows kept and no row's `vulkan`. The node's drivers are known from the
/// kernel, and the page says so: a failed child is not an error the caller has
/// to answer.
#[test]
fn a_vulkan_read_that_gives_nothing_to_trust_answers_failed_and_keeps_the_rows() {
    let (m, _asked, _lines) = core(
        true,
        true,
        FakeNodeFacts::new("Fedora", "43", "6.14.0-100.fc43.x86_64")
            .with_kernel_rows(&[("0000:03:00.0", "8086", "Intel Arc A770", "i915", "")])
            .with_vulkan(
                Status::Failed,
                // A child that answers `failed` prints no device; the seam's
                // list is here to prove the read ignores it.
                &[("0000:03:00.0", "Intel Arc A770", false, "intel", "25.0")],
            ),
    );
    assert_eq!(
        m.node_info(&for_user("alice")),
        Ok(NodeInfoReply {
            os: OsInfo {
                name: Some("Fedora".into()),
                version: Some("43".into()),
                kernel: Some("6.14.0-100.fc43.x86_64".into()),
            },
            vulkan: Status::Failed,
            gpus: vec![row(
                Some("0000:03:00.0"),
                Some("8086"),
                Some("Intel Arc A770"),
                Some("i915"),
                None,
                None,
            )],
        }),
        "`failed` joins nothing and keeps what the kernel answered"
    );
}

/// A caller polkit would challenge (the quiet check refused, the prompting one
/// allowed) is refused, and the node is read by nobody: the seam's counter is
/// empty, so no child was spawned. `devices` refuses this caller too, which is
/// the gate the design asks for.
#[test]
fn a_caller_polkit_would_challenge_is_refused_and_reads_nothing() {
    let (m, asked, _lines) = core(true, false, node(Status::Ok));
    assert_eq!(
        m.node_info(&own(desktop())),
        Err(ManagerError::NotAuthorized),
        "a caller polkit would challenge, or refuses quietly, gets no node info"
    );
    assert_eq!(
        *asked.lock().unwrap(),
        Vec::<String>::new(),
        "a refused read runs no Vulkan child and reads no host"
    );
}

/// No control at all: refused, nothing read.
#[test]
fn a_caller_without_control_is_refused_and_reads_nothing() {
    let (m, asked, _lines) = core(false, false, node(Status::Ok));
    assert_eq!(
        m.node_info(&own(desktop())),
        Err(ManagerError::NotAuthorized),
        "a caller without control"
    );
    assert_eq!(
        *asked.lock().unwrap(),
        Vec::<String>::new(),
        "a refused read runs no Vulkan child and reads no host"
    );
}

/// The `As` form for a principal outside `pai` (bob): refused, nothing read.
/// The bff answers this 403 and ends the session.
#[test]
fn an_as_read_for_a_principal_outside_pai_is_refused_and_reads_nothing() {
    let (m, asked, _lines) = core(true, true, node(Status::Ok));
    assert_eq!(
        m.node_info(&for_user("bob")),
        Err(ManagerError::NotAuthorized),
        "bob is no member of `pai`"
    );
    assert_eq!(
        *asked.lock().unwrap(),
        Vec::<String>::new(),
        "a non-member's read runs no Vulkan child"
    );
}

/// The `As` form from a caller that is not the bff's account: refused, nothing
/// read. The caller's own right is `control` and the desktop user has it here:
/// a vouched name the manager did not vouch for is refused, never answered as
/// the caller's own read.
#[test]
fn an_as_read_from_a_caller_that_is_not_the_bff_is_refused_and_reads_nothing() {
    let (m, asked, _lines) = core(true, true, node(Status::Ok));
    let forger = Requester {
        caller: desktop(),
        principal: Some(Principal("alice".into())),
    };
    assert_eq!(
        m.node_info(&forger),
        Err(ManagerError::NotAuthorized),
        "only the bff's account vouches"
    );
    assert_eq!(
        *asked.lock().unwrap(),
        Vec::<String>::new(),
        "a forged pair runs no Vulkan child"
    );
}

/// A malformed principal name is refused with nothing read: the name is gated
/// before the membership lookup, so a name that is not a name reaches no
/// account and no child.
#[test]
fn an_as_read_for_a_malformed_principal_is_refused_and_reads_nothing() {
    // The name's own rules (`principal_name_is_well_formed`): not empty, at
    // most `MAX_PRINCIPAL_BYTES` (256) bytes, no control character.
    let too_long = "a".repeat(257);
    for name in ["", "alice\n", "a\u{7}b", "\u{1f}", &too_long] {
        let (m, asked, _lines) = core(true, true, node(Status::Ok));
        assert_eq!(
            m.node_info(&for_user(name)),
            Err(ManagerError::NotAuthorized),
            "{name:?} is no principal name"
        );
        assert_eq!(
            *asked.lock().unwrap(),
            Vec::<String>::new(),
            "{name:?}: a refused read runs no Vulkan child"
        );
    }
}
