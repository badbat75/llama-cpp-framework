//! The configurator's client of the manager (`pai-manager`): the instances it
//! runs and the files of the system directory, behind [`ManagerClient`]. On
//! Linux the client speaks D-Bus ([`dbus`]).

use crate::checks::Refusal;
use crate::strings;

/// The prefix of the manager's error names, `<prefix><Kind>`.
const ERROR_PREFIX: &str = "org.privateai.Manager1.Error.";

#[cfg(target_os = "linux")]
pub mod dbus;
pub mod pipe;

/// The state of an instance, as the manager names it (systemd's
/// `ActiveState`); a name it does not know is kept as [`InstanceState::Other`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum InstanceState {
    Active,
    Inactive,
    Activating,
    Deactivating,
    Failed,
    Other(String),
}

impl InstanceState {
    /// The state the manager's `Status` reply `name` stands for.
    pub fn from_name(name: &str) -> InstanceState {
        match name {
            "active" => InstanceState::Active,
            "inactive" => InstanceState::Inactive,
            "activating" => InstanceState::Activating,
            "deactivating" => InstanceState::Deactivating,
            "failed" => InstanceState::Failed,
            other => InstanceState::Other(other.to_string()),
        }
    }

    /// The manager's name of the state, the one [`InstanceState::from_name`]
    /// read.
    pub fn name(&self) -> &str {
        match self {
            InstanceState::Active => "active",
            InstanceState::Inactive => "inactive",
            InstanceState::Activating => "activating",
            InstanceState::Deactivating => "deactivating",
            InstanceState::Failed => "failed",
            InstanceState::Other(name) => name,
        }
    }
}

/// Why a call to the manager failed: one variant per error the manager
/// replies with (`org.privateai.Manager1.Error.<Kind>`), plus
/// [`ClientError::Unavailable`] when there is no manager to ask.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ClientError {
    NotAuthorized,
    InvalidId,
    NoInstance,
    BadFileName,
    /// The id of the instance already on that address.
    AddressInUse(String),
    /// The host that does not resolve.
    UnresolvableHost(String),
    /// The service manager's own message.
    Service(String),
    /// The I/O error kind.
    Io(String),
    /// The content check that refused the write.
    Invalid(Refusal),
    /// No manager answers: not running, its name not on the bus, no bus.
    Unavailable,
    /// Any other failure, with its detail.
    Other(String),
}

impl ClientError {
    /// The error an error reply named `name` with message `message` stands for.
    pub fn from_reply(name: &str, message: &str) -> ClientError {
        let other = || ClientError::Other(format!("{name}: {message}"));
        match name {
            "org.freedesktop.DBus.Error.ServiceUnknown"
            | "org.freedesktop.DBus.Error.NameHasNoOwner"
            | "org.freedesktop.DBus.Error.UnknownObject" => return ClientError::Unavailable,
            _ => {}
        }
        let Some(kind) = name.strip_prefix(ERROR_PREFIX) else {
            return other();
        };
        match kind {
            "NotAuthorized" => ClientError::NotAuthorized,
            "InvalidId" => ClientError::InvalidId,
            "NoInstance" => ClientError::NoInstance,
            "BadFileName" => ClientError::BadFileName,
            "AddressInUse" => ClientError::AddressInUse(message.to_string()),
            "UnresolvableHost" => ClientError::UnresolvableHost(message.to_string()),
            "Service" => ClientError::Service(message.to_string()),
            "Io" => ClientError::Io(message.to_string()),
            "Invalid" => Refusal::decode(message).map_or_else(other, ClientError::Invalid),
            _ => other(),
        }
    }

    /// The text the configurator shows, from the strings file.
    pub fn text(&self) -> String {
        match self {
            ClientError::NotAuthorized => strings::t("client-not-authorized"),
            ClientError::InvalidId => strings::t("client-invalid-id"),
            ClientError::NoInstance => strings::t("client-no-instance"),
            ClientError::BadFileName => strings::t("client-bad-file-name"),
            ClientError::AddressInUse(id) => strings::fill("client-address-in-use", &[id]),
            ClientError::UnresolvableHost(host) => {
                strings::fill("client-unresolvable-host", &[host])
            }
            ClientError::Service(m) => strings::fill("client-service-failed", &[m]),
            ClientError::Io(kind) => strings::fill("client-io-failed", &[kind]),
            ClientError::Invalid(r) => r.text(),
            ClientError::Unavailable => strings::t("client-unavailable"),
            ClientError::Other(detail) => strings::fill("client-other", &[detail]),
        }
    }
}

/// What the configurator asks the manager.
pub trait ManagerClient {
    fn list_instances(&self) -> Result<Vec<String>, ClientError>;
    fn status(&self, id: &str) -> Result<InstanceState, ClientError>;
    fn start(&self, id: &str) -> Result<(), ClientError>;
    fn stop(&self, id: &str) -> Result<(), ClientError>;
    fn restart(&self, id: &str) -> Result<(), ClientError>;
    /// The file's text, or None when it does not exist.
    fn read_file(&self, name: &str) -> Result<Option<String>, ClientError>;
    fn write_file(&self, name: &str, contents: &str) -> Result<(), ClientError>;
}

#[cfg(test)]
mod tests {
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
    const KEYS: [(&str, usize); 10] = [
        ("client-not-authorized", 0),
        ("client-invalid-id", 0),
        ("client-no-instance", 0),
        ("client-bad-file-name", 0),
        ("client-address-in-use", 1),
        ("client-unresolvable-host", 1),
        ("client-service-failed", 1),
        ("client-io-failed", 1),
        ("client-unavailable", 0),
        ("client-other", 1),
    ];

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
            (
                ClientError::Other("connection reset".into()),
                fill("client-other", "connection reset"),
            ),
        ] {
            assert_eq!(error.text(), expected, "{error:?}");
        }
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

    // ── D-Bus ───────────────────────────────────────────────────────────

    #[cfg(target_os = "linux")]
    #[test]
    fn the_dbus_names_are_the_managers() {
        assert_eq!(dbus::BUS_NAME, "org.privateai.Manager1");
        assert_eq!(dbus::OBJECT_PATH, "/org/privateai/Manager1");
        assert_eq!(dbus::INTERFACE, "org.privateai.Manager1");
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
}
