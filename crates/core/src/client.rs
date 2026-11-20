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
    /// No server profile of that name: no such file, or a name that builds
    /// none.
    NoServerProfile(String),
    /// The server profile is used by this other instance. The wire message is
    /// the profile and the instance, in that order, split on `SEP`, as a
    /// refusal's values are.
    ServerProfileInUse(String, String),
    /// The host that does not resolve.
    UnresolvableHost(String),
    /// The service manager's own message.
    Service(String),
    /// The I/O error kind.
    Io(String),
    /// The content check that refused the write.
    Invalid(Refusal),
    /// The file no longer holds the content the caller named, or is not
    /// there: the compare and swap refused, nothing was written.
    Changed,
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
            "Changed" => ClientError::Changed,
            // The server profile that is not there, and the instance running
            // it: both name the profile, the second names the holder too,
            // after the separator `checks::SEP` (the manager's `encode`).
            "NoServerProfile" => ClientError::NoServerProfile(message.to_string()),
            "ServerProfileInUse" => {
                const SEP: char = '\u{1f}';
                message
                    .split_once(SEP)
                    .map_or_else(&other, |(profile, instance)| {
                        ClientError::ServerProfileInUse(profile.to_string(), instance.to_string())
                    })
            }
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
            // The arms are forced by the exhaustive match, the smallest the
            // tests below pin.
            ClientError::NoServerProfile(_) => strings::t("client-no-server-profile"),
            ClientError::ServerProfileInUse(profile, instance) => {
                strings::fill("client-server-profile-in-use", &[profile, instance])
            }
            ClientError::UnresolvableHost(host) => {
                strings::fill("client-unresolvable-host", &[host])
            }
            ClientError::Service(m) => strings::fill("client-service-failed", &[m]),
            ClientError::Io(kind) => strings::fill("client-io-failed", &[kind]),
            ClientError::Invalid(r) => r.text(),
            ClientError::Changed => strings::t("client-changed"),
            ClientError::Unavailable => strings::t("client-unavailable"),
            ClientError::Other(detail) => strings::fill("client-other", &[detail]),
        }
    }
}

/// A file the manager read: its text, and whether the manager withheld
/// `OpencodeApiKey` from it (the value blanked: the caller may not configure).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ManagerFile {
    pub text: String,
    pub withheld: bool,
}

/// What the manager's `Version` answers: the framework version it runs, and
/// the version of the protocol it speaks, which grows with each additive
/// change to the manager's methods.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ManagerVersion {
    pub version: String,
    pub protocol: u32,
}

/// One instance, as `instances()` answers it: the id, the engine it runs, the
/// port its server profile answers on, the tags of that profile and the tags
/// of the record. The two lists stay apart: the web UI shows the profile's
/// first, and says which is which.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InstanceInfo {
    pub id: String,
    pub engine: String,
    pub port: u16,
    pub profile_tags: Vec<String>,
    pub instance_tags: Vec<String>,
}

/// What the configurator asks the manager.
pub trait ManagerClient {
    fn list_instances(&self) -> Result<Vec<String>, ClientError>;
    fn status(&self, id: &str) -> Result<InstanceState, ClientError>;
    fn start(&self, id: &str) -> Result<(), ClientError>;
    fn stop(&self, id: &str) -> Result<(), ClientError>;
    fn restart(&self, id: &str) -> Result<(), ClientError>;
    /// The file, or None when it does not exist.
    fn read_file(&self, name: &str) -> Result<Option<ManagerFile>, ClientError>;
    /// Replace the file with `contents`. With `keep_api_key`, an instance
    /// file keeps the `OpencodeApiKey` the manager holds, whatever `contents`
    /// says of it: for a caller the key was withheld from.
    fn write_file(&self, name: &str, contents: &str, keep_api_key: bool)
        -> Result<(), ClientError>;
    /// Replace the file with `contents` only while it still holds `expected`,
    /// the text the caller read: a file that is not there, or holds something
    /// else, is [`ClientError::Changed`], and nothing is written. Guarded as
    /// `write_file` is. No `keep_api_key`: the caller that holds the file's
    /// own text has no key to preserve.
    ///
    /// The default body is for a transport that speaks no compare and swap:
    /// the call is one it cannot make, said as [`ClientError::Other`]. A
    /// transport that speaks the method overrides it.
    fn write_file_if(&self, name: &str, contents: &str, expected: &str) -> Result<(), ClientError> {
        let _ = (name, contents, expected);
        Err(ClientError::Other(
            "this transport of the manager speaks no compare and swap".into(),
        ))
    }
    /// Whether the instance starts when the system starts.
    fn boot_enabled(&self, id: &str) -> Result<bool, ClientError>;
    /// Make the instance start when the system starts, or not.
    fn set_boot_enabled(&self, id: &str, enabled: bool) -> Result<(), ClientError>;
    /// Assigns the model profile `profile` to instance `instance` as `value`:
    /// `on`, `off`, `inherit` (the pool's own state) or `none` (no assignment
    /// of its own). `*` is every model profile of the pool, and takes `none`
    /// and `inherit` only. Guarded as a write of a served file is.
    ///
    /// The default body is for a transport that speaks to a manager of an
    /// older protocol: the call is one it cannot make, said as
    /// [`ClientError::Other`]. A transport that speaks the method overrides it.
    fn set_model_profile(
        &self,
        instance: &str,
        profile: &str,
        value: &str,
    ) -> Result<(), ClientError> {
        let _ = (instance, profile, value);
        Err(ClientError::Other(
            "this transport of the manager has no SetModelProfile".into(),
        ))
    }
    /// Points the instance `instance` at the server profile `profile`.
    /// Guarded as a write of a served file is.
    ///
    /// The default body is for a transport that speaks to a manager of an
    /// older protocol: the call is one it cannot make, said as
    /// [`ClientError::Other`]. A transport that speaks the method overrides it.
    fn set_server_profile(&self, instance: &str, profile: &str) -> Result<(), ClientError> {
        let _ = (instance, profile);
        Err(ClientError::Other(
            "this transport of the manager has no SetServerProfile".into(),
        ))
    }
    /// The instances the manager knows, each with the tags of the server
    /// profile it points at and the tags of the record itself. Answered to any
    /// caller, as the list of ids is.
    ///
    /// The default body is for a transport that speaks to a manager of an
    /// older protocol: the call is one it cannot make, said as
    /// [`ClientError::Other`]. A transport that speaks the method overrides it.
    fn instances(&self) -> Result<Vec<InstanceInfo>, ClientError> {
        Err(ClientError::Other(
            "this transport of the manager has no Instances".into(),
        ))
    }
    /// Replaces the tags of the instance `instance`: the record's own list,
    /// shown by the web UI beside the tags of its server profile. Guarded as a
    /// write of a served file is: a caller who may not configure is
    /// [`ClientError::NotAuthorized`], and a list the rule refuses is
    /// [`ClientError::Invalid`], with nothing written.
    ///
    /// The default body is for a transport that speaks to a manager of an
    /// older protocol: the call is one it cannot make, said as
    /// [`ClientError::Other`]. A transport that speaks the method overrides it.
    fn set_instance_tags(&self, instance: &str, tags: &[String]) -> Result<(), ClientError> {
        let _ = (instance, tags);
        Err(ClientError::Other(
            "this transport of the manager has no SetInstanceTags".into(),
        ))
    }
    /// Whether `password` is `user`'s OS password, and whether that user may
    /// configure: (ok, can_configure). The manager answers only the bff's
    /// account; anyone else is [`ClientError::NotAuthorized`].
    fn authenticate(&self, user: &str, password: &str) -> Result<(bool, bool), ClientError>;
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
    const KEYS: [(&str, usize); 13] = [
        ("client-not-authorized", 0),
        ("client-invalid-id", 0),
        ("client-no-instance", 0),
        ("client-bad-file-name", 0),
        ("client-address-in-use", 1),
        ("client-no-server-profile", 0),
        ("client-server-profile-in-use", 2),
        ("client-unresolvable-host", 1),
        ("client-service-failed", 1),
        ("client-io-failed", 1),
        ("client-changed", 0),
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
