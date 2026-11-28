use super::*;

const PREFIX: &str = "org.privateai.Manager1.Error.";

fn reply(kind: &str, message: &str) -> ClientError {
    ClientError::from_reply(&format!("{PREFIX}{kind}"), message)
}

fn refusal(key: &'static str, values: &[&str]) -> Refusal {
    Refusal {
        key,
        values: values.iter().map(|v| v.to_string()).collect(),
    }
}

/// The error keys of the client, with how many `{N}` each takes.
const KEYS: [(&str, usize); 14] = [
    ("client-not-authorized", 0),
    ("client-invalid-id", 0),
    ("client-no-instance", 0),
    ("client-bad-file-name", 0),
    ("client-address-in-use", 1),
    ("client-no-server-profile", 0),
    ("client-server-profile-in-use", 2),
    ("client-server-profile-exists", 1),
    ("client-unresolvable-host", 1),
    ("client-service-failed", 1),
    ("client-io-failed", 1),
    ("client-changed", 0),
    ("client-unavailable", 0),
    ("client-other", 1),
];

// ── The instance verbs (slice 16) ───────────────────────────────────

/// A client that speaks only the methods with no default body: the shape
/// of a transport of an older manager.
struct Bare;

impl ManagerClient for Bare {
    fn list_instances(&self) -> Result<Vec<String>, ClientError> {
        Ok(Vec::new())
    }
    fn status(&self, _id: &str) -> Result<InstanceState, ClientError> {
        Ok(InstanceState::Inactive)
    }
    fn start(&self, _id: &str) -> Result<(), ClientError> {
        Ok(())
    }
    fn stop(&self, _id: &str) -> Result<(), ClientError> {
        Ok(())
    }
    fn restart(&self, _id: &str) -> Result<(), ClientError> {
        Ok(())
    }
    fn read_file(&self, _name: &str) -> Result<Option<ManagerFile>, ClientError> {
        Ok(None)
    }
    fn write_file(
        &self,
        _name: &str,
        _contents: &str,
        _keep_api_key: bool,
    ) -> Result<(), ClientError> {
        Ok(())
    }
    fn boot_enabled(&self, _id: &str) -> Result<bool, ClientError> {
        Ok(false)
    }
    fn set_boot_enabled(&self, _id: &str, _enabled: bool) -> Result<(), ClientError> {
        Ok(())
    }
    fn authenticate(&self, _user: &str, _password: &str) -> Result<(bool, bool), ClientError> {
        Ok((true, true))
    }
}

/// `create_instance(engine, id, instance_profile)` and `remove_instance(id)`
/// are on the trait, each answering the default body that names the verb a
/// transport does not speak: a new fake client gains both without writing
/// either, as slice 14's `create_server_profile` does. A transport that
/// speaks them overrides them.
#[test]
fn the_instance_verbs_are_on_the_trait_with_the_default_body() {
    for (verb, answer) in [
        (
            "CreateInstance",
            Bare.create_instance("llama.cpp", "lan-2", "big")
                .map(|_| ()),
        ),
        ("RemoveInstance", Bare.remove_instance("lan-2")),
    ] {
        match answer {
            Err(ClientError::Other(detail)) => {
                assert!(detail.contains(verb), "{verb}: {detail:?}");
            }
            other => panic!("{verb}: the default body is not Other: {other:?}"),
        }
    }
}

// ── States ──────────────────────────────────────────────────────────

#[test]
fn every_state_the_manager_sends_has_its_variant() {
    for (name, state) in [
        ("active", InstanceState::Active),
        ("inactive", InstanceState::Inactive),
        ("activating", InstanceState::Activating),
        ("deactivating", InstanceState::Deactivating),
        ("failed", InstanceState::Failed),
    ] {
        assert_eq!(InstanceState::from_name(name), state, "{name}");
    }
}

#[test]
fn an_unknown_state_keeps_its_name() {
    for name in ["maintenance", "Active", "", "no such state"] {
        assert_eq!(
            InstanceState::from_name(name),
            InstanceState::Other(name.to_string()),
            "{name:?}"
        );
    }
}

/// The name a state was read from is the name it gives back, an unknown
/// one included: what the bff passes on to the web UI.
#[test]
fn every_state_gives_back_the_manager_s_name() {
    for name in [
        "active",
        "inactive",
        "activating",
        "deactivating",
        "failed",
        "maintenance",
        "",
    ] {
        assert_eq!(InstanceState::from_name(name).name(), name, "{name:?}");
    }
}

// ── Error replies ───────────────────────────────────────────────────

#[test]
fn every_manager_error_name_has_its_variant() {
    assert_eq!(
        reply("NotAuthorized", "NotAuthorized"),
        ClientError::NotAuthorized
    );
    assert_eq!(reply("InvalidId", "InvalidId"), ClientError::InvalidId);
    assert_eq!(reply("NoInstance", "NoInstance"), ClientError::NoInstance);
    assert_eq!(
        reply("BadFileName", "BadFileName"),
        ClientError::BadFileName
    );
    assert_eq!(
        reply("AddressInUse", "default"),
        ClientError::AddressInUse("default".into())
    );
    // The server profile refusals of `SetServerProfile`: the name alone,
    // and the pair the message carries, profile first.
    assert_eq!(
        reply("NoServerProfile", "big"),
        ClientError::NoServerProfile("big".into())
    );
    assert_eq!(
        reply("ServerProfileInUse", "big\u{1f}lan-2"),
        ClientError::ServerProfileInUse("big".into(), "lan-2".into())
    );
    assert_eq!(
        reply("ServerProfileInUse", "\u{1f}lan-2"),
        ClientError::ServerProfileInUse(String::new(), "lan-2".into()),
        "an empty profile name is still the pair the manager sent"
    );
    // A message that is not a pair is no answer the client can spell out.
    match reply("ServerProfileInUse", "big") {
        ClientError::Other(detail) => {
            assert!(detail.contains("big"), "{detail:?}");
        }
        other => panic!("a name with no separator: {other:?}"),
    }
    assert_eq!(
        reply("UnresolvableHost", "nowhere.invalid"),
        ClientError::UnresolvableHost("nowhere.invalid".into())
    );
    let systemd = "Unit llama.cpp@default.service not found.";
    assert_eq!(
        reply("Service", systemd),
        ClientError::Service(systemd.into())
    );
    assert_eq!(
        reply("Io", "permission denied"),
        ClientError::Io("permission denied".into())
    );
    // The compare and swap refused: the wire name of the face's
    // `ManagerError::Changed` (both transports reply it).
    assert_eq!(reply("Changed", "Changed"), ClientError::Changed);
}

#[test]
fn an_invalid_reply_decodes_the_refusal() {
    let r = refusal(
        "not-whole-number-in-range",
        &["Port", "70000", "1", "65535"],
    );
    assert_eq!(reply("Invalid", &r.encode()), ClientError::Invalid(r));
    let r = refusal("moe-cache-with-tensor-split", &[]);
    assert_eq!(reply("Invalid", &r.encode()), ClientError::Invalid(r));
}

/// An Invalid whose message is no refusal (a newer manager's key, say)
/// is not dropped: it comes back as Other, with what it said.
#[test]
fn an_invalid_reply_that_is_no_refusal_is_other() {
    for message in ["", "no-such-key\u{1f}x"] {
        match reply("Invalid", message) {
            ClientError::Other(detail) => {
                assert!(detail.contains(message), "{detail:?} for {message:?}")
            }
            other => panic!("{message:?}: {other:?}"),
        }
    }
}

/// The bus's answers when nobody owns the name, and a peer that does
/// not serve the manager's object: no manager to ask.
#[test]
fn no_manager_on_the_other_end_is_unavailable() {
    for name in [
        "org.freedesktop.DBus.Error.ServiceUnknown",
        "org.freedesktop.DBus.Error.NameHasNoOwner",
        "org.freedesktop.DBus.Error.UnknownObject",
    ] {
        assert_eq!(
            ClientError::from_reply(name, "The name is not activatable"),
            ClientError::Unavailable,
            "{name}"
        );
    }
}

#[test]
fn any_other_error_name_is_other_and_keeps_name_and_message() {
    for (name, message) in [
        (
            "org.freedesktop.DBus.Error.AccessDenied",
            "Rejected send message",
        ),
        (
            "org.freedesktop.DBus.Error.UnknownMethod",
            "Unknown method 'Reload'",
        ),
        ("org.privateai.Manager1.Error.NewKind", "detail"),
        ("org.privateai.Manager1.Error.", ""),
    ] {
        match ClientError::from_reply(name, message) {
            ClientError::Other(detail) => {
                assert!(detail.contains(name), "{detail:?} names {name}");
                assert!(detail.contains(message), "{detail:?} keeps {message:?}");
            }
            other => panic!("{name}: {other:?}"),
        }
    }
}

// ── Texts ───────────────────────────────────────────────────────────

fn placeholders(text: &str) -> usize {
    (0..10)
        .filter(|n| text.contains(&format!("{{{n}}}")))
        .count()
}

#[test]
fn every_error_key_is_in_both_strings_files_with_its_placeholders() {
    for file in [
        include_str!("../strings/cli.toml"),
        include_str!("../strings/en_US.toml"),
    ] {
        let table = crate::strings::parse(file);
        for (key, n) in KEYS {
            let text = table.get(key).unwrap_or_else(|| panic!("{key} missing"));
            assert_eq!(placeholders(text), n, "{key}: {text:?}");
        }
    }
}

#[test]
fn every_error_renders_its_own_text() {
    let gui = crate::strings::parse(include_str!("../strings/en_US.toml"));
    let fill = |key: &str, arg: &str| {
        gui.get(key)
            .unwrap_or_else(|| panic!("{key} missing from en_US.toml"))
            .replace("{0}", arg)
    };
    for (error, expected) in [
        (
            ClientError::NotAuthorized,
            fill("client-not-authorized", ""),
        ),
        (ClientError::InvalidId, fill("client-invalid-id", "")),
        (ClientError::NoInstance, fill("client-no-instance", "")),
        (ClientError::BadFileName, fill("client-bad-file-name", "")),
        (
            ClientError::AddressInUse("default".into()),
            fill("client-address-in-use", "default"),
        ),
        (
            ClientError::NoServerProfile("big".into()),
            gui.get("client-no-server-profile")
                .expect("client-no-server-profile in en_US.toml")
                .clone(),
        ),
        (
            ClientError::ServerProfileInUse("big".into(), "lan-2".into()),
            gui.get("client-server-profile-in-use")
                .expect("client-server-profile-in-use in en_US.toml")
                .replace("{0}", "big")
                .replace("{1}", "lan-2"),
        ),
        (
            ClientError::UnresolvableHost("gpu-box.lan".into()),
            fill("client-unresolvable-host", "gpu-box.lan"),
        ),
        (
            ClientError::Service("Unit llama.cpp@default.service not found.".into()),
            fill(
                "client-service-failed",
                "Unit llama.cpp@default.service not found.",
            ),
        ),
        (
            ClientError::Io("permission denied".into()),
            fill("client-io-failed", "permission denied"),
        ),
        (ClientError::Unavailable, fill("client-unavailable", "")),
        (ClientError::Changed, fill("client-changed", "")),
        (
            ClientError::Other("connection reset".into()),
            fill("client-other", "connection reset"),
        ),
    ] {
        assert_eq!(error.text(), expected, "{error:?}");
    }
}

/// The reply of a create that found the file already there: its own error,
/// named, with the profile in its text. A client that does not know the
/// reply lands it in `Other`, and the web service answers a clash as a
/// broken manager (502) instead of a conflict (409).
#[test]
fn a_server_profile_that_exists_decodes_to_its_own_error() {
    let e = ClientError::from_reply(&format!("{ERROR_PREFIX}ServerProfileExists"), "big");
    assert!(
        !matches!(e, ClientError::Other(_)),
        "a known reply must not land in Other: {e:?}"
    );
    let gui = crate::strings::parse(include_str!("../strings/en_US.toml"));
    let text = gui
        .get("client-server-profile-exists")
        .expect("client-server-profile-exists in en_US.toml")
        .replace("{0}", "big");
    assert_eq!(e.text(), text, "{e:?}");
    assert!(e.text().contains("big"), "the text names the profile");
}

/// The address-in-use sentence names the holder of the address and
/// nothing beside it: no lead-in, so it reads the same for a competing
/// instance and for the web service, whose name is longer than any
/// instance's. The GUI's file, byte for byte.
#[test]
fn the_address_in_use_text_of_the_gui_file_names_only_the_holder() {
    assert_eq!(
        crate::strings::parse(include_str!("../strings/en_US.toml"))
            .get("client-address-in-use")
            .expect("client-address-in-use in en_US.toml"),
        "{0} already uses that address and port."
    );
}

/// The same sentence in the CLI's file, byte for byte: the two files say
/// the same thing, only the GUI's is read by `text()` here.
#[test]
fn the_address_in_use_text_of_the_cli_file_names_only_the_holder() {
    assert_eq!(
        crate::strings::parse(include_str!("../strings/cli.toml"))
            .get("client-address-in-use")
            .expect("client-address-in-use in cli.toml"),
        "{0} already uses that address and port."
    );
}

/// Filled with the web service's name, the text is the sentence about it:
/// the name is the whole subject, the sentence carries no word that the
/// service's name does not fit.
#[test]
fn the_address_in_use_text_filled_with_the_service_name() {
    assert_eq!(
        ClientError::AddressInUse("private-ai-bff".into()).text(),
        "private-ai-bff already uses that address and port."
    );
}

/// The in-use sentence of a server profile names the profile and the
/// instance that holds it, in the order the manager sends them: the CLI's
/// file and the GUI's say the same, `text()` reads the GUI's.
#[test]
fn the_server_profile_in_use_text_names_the_profile_then_the_instance() {
    assert_eq!(
        ClientError::ServerProfileInUse("big".into(), "lan-2".into()).text(),
        "The server profile big is already used by instance lan-2."
    );
    assert_eq!(
        crate::strings::parse(include_str!("../strings/cli.toml"))
            .get("client-server-profile-in-use")
            .expect("client-server-profile-in-use in cli.toml"),
        "The server profile {0} is already used by instance {1}."
    );
}

/// A server profile the engine has none of: the sentence says there is
/// none, as the one about an instance does, and names nothing.
#[test]
fn the_no_server_profile_text_says_there_is_none() {
    assert_eq!(
        ClientError::NoServerProfile("big".into()).text(),
        "There is no server profile of that name."
    );
    assert_eq!(
        crate::strings::parse(include_str!("../strings/cli.toml"))
            .get("client-no-server-profile")
            .expect("client-no-server-profile in cli.toml"),
        "There is no server profile of that name."
    );
}

/// A refused write shows the refusal's own text, values filled in.
#[test]
fn an_invalid_error_renders_the_refusal() {
    let r = refusal("comment-markers", &["StateDir"]);
    let text = ClientError::Invalid(r.clone()).text();
    assert_eq!(text, r.text());
    assert!(
        text.contains("StateDir") && text != "comment-markers",
        "{text:?}"
    );
}

/// A client that speaks none of the compare and swap: the trait's default
/// body, for a transport that has no such method.
struct NoCompareSwap;

impl ManagerClient for NoCompareSwap {
    fn list_instances(&self) -> Result<Vec<String>, ClientError> {
        Ok(vec![])
    }
    fn status(&self, _id: &str) -> Result<InstanceState, ClientError> {
        Ok(InstanceState::Inactive)
    }
    fn start(&self, _id: &str) -> Result<(), ClientError> {
        Ok(())
    }
    fn stop(&self, _id: &str) -> Result<(), ClientError> {
        Ok(())
    }
    fn restart(&self, _id: &str) -> Result<(), ClientError> {
        Ok(())
    }
    fn read_file(&self, _name: &str) -> Result<Option<ManagerFile>, ClientError> {
        Ok(None)
    }
    fn write_file(
        &self,
        _name: &str,
        _contents: &str,
        _keep_api_key: bool,
    ) -> Result<(), ClientError> {
        Ok(())
    }
    fn boot_enabled(&self, _id: &str) -> Result<bool, ClientError> {
        Ok(false)
    }
    fn set_boot_enabled(&self, _id: &str, _enabled: bool) -> Result<(), ClientError> {
        Ok(())
    }
    fn authenticate(&self, _user: &str, _password: &str) -> Result<(bool, bool), ClientError> {
        Ok((false, false))
    }
}

/// The default body: a call the client cannot make, said as `Other` with
/// a detail. Never Ok, never one of the manager's refusals: a caller
/// must not read a transport with no compare and swap as a write the
/// manager refused, and never Ok: a client that cannot set a model
/// profile must not report one as set. A transport that speaks the
/// method overrides the body (the round trips are in `pai-manager`'s
/// e2e).
#[test]
fn a_client_of_the_default_body_reports_other() {
    match NoCompareSwap.write_file_if("models.ini", "[x]\nmodel = /m/x.gguf\n", "") {
        Err(ClientError::Other(detail)) => {
            assert!(!detail.is_empty(), "the detail says why: {detail:?}")
        }
        other => panic!("the trait's default body: {other:?}"),
    }
    match NoCompareSwap.set_model_profile("default", "Qwen 3 (v2) é", "on") {
        Err(ClientError::Other(detail)) => {
            assert!(!detail.is_empty(), "the detail says why: {detail:?}")
        }
        other => panic!("the trait's default body: {other:?}"),
    }
}

// ── D-Bus ───────────────────────────────────────────────────────────

#[cfg(target_os = "linux")]
#[test]
fn the_dbus_names_are_the_managers() {
    assert_eq!(dbus::BUS_NAME, "org.privateai.Manager1");
    assert_eq!(dbus::OBJECT_PATH, "/org/privateai/Manager1");
    assert_eq!(dbus::INTERFACE, "org.privateai.Manager1");
}

// ── The node's devices (G2, ARCHITECTURE.md 1) ───────────────────────

/// The shape every transport of the manager must read: the probe's moment
/// (Unix seconds UTC, `None` when no probe answered) and one entry per
/// device, in the probe's order, with the id `device` takes, the product
/// name, the backend and both sizes in MiB. The sizes are u64: a card's
/// total fits no u32 once a node grows.
///
/// R1 adds the host's inventory beside the list (ARCHITECTURE.md (1) "The
/// node inventory"): `ram_mib`, the host's total physical memory in MiB,
/// and `cpu_threads`, the logical CPUs the manager may use (the count
/// `std::thread::available_parallelism` answers, the width of the D-Bus
/// `u`). The names are the contract for every transport, the bff's JSON
/// `ramMib` and `cpuThreads`, and the manager's `ram_mib` and
/// `cpu_threads`.
#[test]
fn the_device_list_carries_the_probe_moment_and_each_device_in_mib() {
    let got = NodeDevices {
        probed_at: Some(1_766_271_243u64),
        ram_mib: 65_536,
        cpu_threads: 16,
        devices: vec![
            DeviceInfo {
                id: "ROCm0".to_string(),
                name: "AMD Radeon AI PRO R9700".to_string(),
                backend: "ROCm".to_string(),
                total_mib: 32624u64,
                free_mib: 32420u64,
            },
            DeviceInfo {
                id: "CUDA0".to_string(),
                name: "NVIDIA GeForce RTX 4070 SUPER".to_string(),
                backend: "CUDA".to_string(),
                total_mib: 12281u64,
                free_mib: 10844u64,
            },
        ],
    };
    assert_eq!(got.probed_at, Some(1_766_271_243));
    assert_eq!(got.devices[0].id, "ROCm0");
    assert_eq!(got.devices[0].backend, "ROCm");
    assert_eq!(got.devices[1].total_mib, 12281);
    assert_eq!(got.devices[1].free_mib, 10844);
    assert_eq!(got.ram_mib, 65_536, "the host's RAM in MiB");
    assert_eq!(got.cpu_threads, 16, "the host's logical CPUs");
}

/// The inventory is the host's, not the probe's (R1): a node whose probe
/// answered nothing, was killed, or printed only the CPU line, still
/// answers its RAM and its threads. The two travel on the same reply
/// because they are one read of what the node holds, never because the
/// engine reported them.
#[test]
fn the_inventory_is_answered_with_no_probe_and_no_device() {
    let node = NodeDevices {
        probed_at: None,
        devices: vec![],
        ram_mib: 65_536,
        cpu_threads: 16,
    };
    assert_eq!(node.probed_at, None, "no probe: no time");
    assert_eq!(node.devices, Vec::<DeviceInfo>::new(), "no device");
    assert_eq!(
        (node.ram_mib, node.cpu_threads),
        (65_536, 16),
        "the host's inventory is answered whatever the probe did"
    );
}

/// `devices()` is a verb of the manager as its siblings are: the same
/// `Result<_, ClientError>` shape, and a verb of the trait object, so every
/// transport has it, with the default body that says the manager speaks it
/// and a `None` principal picks the plain method.
#[test]
fn devices_is_a_verb_of_the_manager() {
    // The verb, called on the trait object, with the shape the design
    // pins. A method of `dyn ManagerClient` cannot be named as a function
    // (its `&self` outlives no lifetime a `fn` item can carry), so the
    // call is the pin.
    let ask: fn(&dyn ManagerClient) -> Result<NodeDevices, ClientError> = |c| c.devices();
    let _ = ask;
}

/// `model_info(profile)` is a verb of the manager as its siblings are: the
/// same `Result<_, ClientError>` shape, and a verb of the trait object, so
/// every transport has it. Its reply is the three fields of the design
/// (L1, ARCHITECTURE.md (1)): the resolved `model`, `info` the four counts,
/// `reason` the keyed why. The names are the contract for every transport
/// and for the bff's JSON `model`, `info` (`blockCount`, `moe`,
/// `expertCount`, `expertUsed`) and `reason` (`key`, `args`).
#[test]
fn model_info_is_a_verb_of_the_manager() {
    let ask: fn(&dyn ManagerClient, &str) -> Result<ModelInfoReply, ClientError> =
        |c, profile| c.model_info(profile);
    let _ = ask;
    let facts = ModelInfoReply {
        model: Some("/var/lib/llama.cpp/models/m.gguf".into()),
        info: Some(ModelFacts {
            block_count: 64,
            moe: true,
            expert_count: 128,
            expert_used: 8,
            tensors: Vec::new(),
            tensors_truncated: false,
        }),
        reason: None,
    };
    assert_eq!(
        (
            facts.info.as_ref().map(|f| f.block_count),
            facts.info.as_ref().map(|f| f.expert_count),
            facts.info.as_ref().map(|f| f.expert_used)
        ),
        (Some(64), Some(128), Some(8)),
        "the counts are u64: a header declaring more than a u32 counts is answered"
    );
    // No facts: `model` and `reason`, never an empty `info` (the page
    // shows the reason's text, so it needs the key and its values).
    let none = ModelInfoReply {
        model: Some("/var/lib/llama.cpp/models/m.gguf".into()),
        info: None,
        reason: Some(Reason {
            key: "model-info-unreadable".into(),
            args: vec!["/var/lib/llama.cpp/models/m.gguf".into()],
        }),
    };
    assert_eq!(none.info, None);
    assert_eq!(
        none.reason.as_ref().map(|r| r.key.as_str()),
        Some("model-info-unreadable")
    );
    assert_eq!(
        ModelInfoReply {
            model: None,
            info: None,
            reason: Some(Reason {
                key: "model-profile-no-model".into(),
                args: vec!["p".into()],
            }),
        }
        .model,
        None,
        "a profile naming no `model` answers no path"
    );
}

/// A transport that speaks no `ModelInfo` (a manager of an older protocol)
/// says so, and answers no facts: a page that read a default body must not
/// show a model with 0 blocks.
#[test]
fn a_client_of_the_default_body_answers_no_model_info() {
    match Bare.model_info("Qwen3.8-27B") {
        Err(ClientError::Other(detail)) => {
            assert!(!detail.is_empty(), "the detail says why: {detail:?}")
        }
        other => panic!("the trait's default body: {other:?}"),
    }
}

/// `node_info()` is a verb of the manager as its siblings are: the same
/// `Result<_, ClientError>` shape, and a verb of the trait object, so every
/// transport has it (AN3, `ARCHITECTURE.md` "About and node info"). Its
/// reply is the document of the design: `os` with `name`, `version` and
/// `kernel`, `vulkan` one of the three words, `gpus` rows of `pci`, `vendor`,
/// `name`, `driver`, `version` and the `vulkan` pair (`driver`, `info`).
/// Every text is optional: a fact the host did not answer is `None`, and
/// these are the names the bff's `/api/node` JSON answers.
#[test]
fn node_info_is_a_verb_of_the_manager() {
    let ask: fn(&dyn ManagerClient) -> Result<NodeInfoReply, ClientError> = |c| c.node_info();
    let _ = ask;
    let info = NodeInfoReply {
        os: OsInfo {
            name: Some("Ubuntu".into()),
            version: Some("26.04".into()),
            kernel: Some("6.14.0-24-generic".into()),
        },
        vulkan: Status::Ok,
        gpus: vec![NodeGpu {
            pci: Some("0000:03:00.0".into()),
            vendor: Some("1002".into()),
            name: Some("AMD Radeon AI PRO R9700".into()),
            driver: Some("amdgpu".into()),
            version: Some("2.5.0".into()),
            vulkan: Some(crate::vulkan::Driver {
                driver: Some("RADV".into()),
                info: Some("Mesa 25.2.0".into()),
            }),
        }],
    };
    assert_eq!(info.os.name.as_deref(), Some("Ubuntu"));
    assert_eq!(info.os.version.as_deref(), Some("26.04"));
    assert_eq!(info.os.kernel.as_deref(), Some("6.14.0-24-generic"));
    assert_eq!(info.gpus.len(), 1, "one GPU, one row");
    let row = &info.gpus[0];
    assert_eq!(row.pci.as_deref(), Some("0000:03:00.0"));
    assert_eq!(
        row.vendor.as_deref(),
        Some("1002"),
        "4 lower-case hex digits"
    );
    assert_eq!(row.driver.as_deref(), Some("amdgpu"));
    assert_eq!(row.version.as_deref(), Some("2.5.0"));
    assert_eq!(
        row.vulkan.as_ref().and_then(|v| v.driver.as_deref()),
        Some("RADV")
    );
    assert_eq!(
        row.vulkan.as_ref().and_then(|v| v.info.as_deref()),
        Some("Mesa 25.2.0")
    );
    // A node that answered nothing is a reply of `None`s, never an error:
    // the page shows a hole, and the three Vulkan words are all answers.
    for status in [Status::Ok, Status::None, Status::Failed] {
        let empty = NodeInfoReply {
            os: OsInfo {
                name: None,
                version: None,
                kernel: None,
            },
            vulkan: status,
            gpus: Vec::new(),
        };
        assert_eq!(empty.os.name, None);
        assert!(empty.gpus.is_empty(), "{status:?}: no row");
    }
}

/// A transport that speaks no `NodeInfo` (a manager of an older protocol)
/// says so and answers no node: a page that read a default body must not
/// show a node with no OS and no GPU, which is the shape of a node with
/// neither.
#[test]
fn a_client_of_the_default_body_answers_no_node_info() {
    match Bare.node_info() {
        Err(ClientError::Other(detail)) => {
            assert!(!detail.is_empty(), "the detail says why: {detail:?}")
        }
        other => panic!("the trait's default body: {other:?}"),
    }
}

/// No system bus at all (its socket missing) is Unavailable, never a
/// panic or a hang. The only test that touches `DBUS_SYSTEM_BUS_ADDRESS`,
/// pointed at a socket that does not exist, so nothing real is reached.
#[cfg(target_os = "linux")]
#[test]
fn no_system_bus_is_unavailable() {
    let dir = tempfile::tempdir().expect("tempdir");
    let socket = dir.path().join("no-bus-here");
    std::env::set_var(
        "DBUS_SYSTEM_BUS_ADDRESS",
        format!("unix:path={}", socket.display()),
    );
    let result = std::thread::spawn(dbus::DbusClient::connect_system);
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(10);
    while !result.is_finished() {
        assert!(
            std::time::Instant::now() < deadline,
            "connect_system did not return within 10 s"
        );
        std::thread::sleep(std::time::Duration::from_millis(20));
    }
    match result.join().expect("connect_system panicked") {
        Err(ClientError::Unavailable) => {}
        Err(other) => panic!("not Unavailable: {other:?}"),
        Ok(_) => panic!("connected to a bus that does not exist"),
    }
}
