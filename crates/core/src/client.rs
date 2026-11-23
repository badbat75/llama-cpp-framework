//! The configurator's client of the manager (`pai-manager`): the instances it
//! runs and the files of the system directory, behind [`ManagerClient`]. On
//! Linux the client speaks D-Bus ([`dbus`]).

use crate::checks::Refusal;
use crate::host::OsInfo;
use crate::strings;
use crate::vulkan::{NodeGpu, Status};

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
    /// A server profile with that name is already there: a create overwrites
    /// nothing.
    ServerProfileExists(String),
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
            "ServerProfileExists" => ClientError::ServerProfileExists(message.to_string()),
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
            ClientError::ServerProfileExists(profile) => {
                strings::fill("client-server-profile-exists", &[profile])
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
/// server profile its section names, the port that profile answers on, the
/// tags of that profile and the tags of the record. The two lists stay apart:
/// the web UI shows the profile's first, and says which is which.
///
/// `server_profile` is the `ServerProfile` of the instance's `instances.ini`
/// section verbatim: the server file the instance reads is
/// `server@<server_profile>.ini`, not one named after the id. `None` is a
/// manager of an older protocol, which answers no field.
///
/// `label` is the `Label` of the instance's section, trimmed as the writer
/// wrote it (IL1): the name the web UI shows beside the id. `None` is no
/// label, which is what a section with no `Label` key is, and what a manager
/// of an older protocol answers: the UI falls back to the engine and the port.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InstanceInfo {
    pub id: String,
    pub engine: String,
    pub server_profile: Option<String>,
    pub port: u16,
    pub profile_tags: Vec<String>,
    pub instance_tags: Vec<String>,
    pub label: Option<String>,
}

/// One compute device of the node, as the engine's own probe reported it: the
/// id `device` takes (`ROCm0`), the product name, the backend (the id without
/// its trailing digits, case kept), and the device memory in MiB at the moment
/// of the probe.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DeviceInfo {
    pub id: String,
    pub name: String,
    pub backend: String,
    pub total_mib: u64,
    pub free_mib: u64,
}

/// The node's inventory, as [`ManagerClient::devices`] answers it: the
/// moment of the probe in Unix seconds UTC (`None` when no probe answered,
/// which is no list either) and one entry per device in the probe's order,
/// the CPU pseudo-device left out. The list lives as long as the manager's
/// probe does, so the moment travels with it: the memory of a card is what it
/// was at the probe, never now.
///
/// Beside the list, the host the cards sit in (ARCHITECTURE.md (1) "The node
/// inventory"): `ram_mib`, the total physical memory in MiB, and
/// `cpu_threads`, the logical CPUs the manager may use. They travel with the
/// list because they are one read of what the node holds, never because the
/// engine reported them: a probe that failed leaves both answered, and 0 on
/// each is the host's own "no figure" (a caller caps nothing at 0).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NodeDevices {
    pub probed_at: Option<u64>,
    pub ram_mib: u64,
    pub cpu_threads: u32,
    pub devices: Vec<DeviceInfo>,
}

/// One server profile, as `server_profiles()` answers it: the name of its file
/// (`server@<id>.ini`), the engine it serves, the port its `Port` answers (the
/// default when the file names none), the tags of its `[Server]` section, and
/// the first instance whose `instances.ini` section points at it (`None` when
/// no section does).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ServerProfileInfo {
    pub id: String,
    pub engine: String,
    pub port: u16,
    pub tags: Vec<String>,
    pub instance: Option<String>,
}

/// What a new model profile is built from (`CreateModelProfile`,
/// `ARCHITECTURE.md` "Create, clone and delete model profiles from the web"):
/// a copy of a profile the pool holds, a `model` file of the node, or a
/// Hugging Face repository with an optional file inside it (the `value` object
/// `GET api/hf/files` answers). Exactly one source, which is why the client
/// carries one enum: the manager judges the shape of a call that arrives with
/// two, or none.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ModelProfileSource {
    /// A copy of the pool's model profile `id`, its section's lines kept.
    From(String),
    /// A new profile with one `model` line.
    Model(String),
    /// A new profile with `hf-repo`, and its `hf-file` when the pick named one.
    Hf { repo: String, file: Option<String> },
}

impl ModelProfileSource {
    /// The pair `CreateModelProfile` takes: the profile to copy, `""` when the
    /// source is a pick, and the pick's keys, `""` for a clone. A transport
    /// expands the enum this way, the manager never seeing it.
    pub fn as_source(&self) -> (String, Vec<(String, String)>) {
        match self {
            Self::From(id) => (id.clone(), Vec::new()),
            Self::Model(model) => (String::new(), vec![("model".into(), model.clone())]),
            Self::Hf { repo, file } => {
                let mut values = vec![("hf-repo".into(), repo.clone())];
                if let Some(file) = file {
                    values.push(("hf-file".into(), file.clone()));
                }
                (String::new(), values)
            }
        }
    }
}

/// One tensor of a model, as the tensor list of [`ModelFacts`] carries it: the
/// name the GGUF header spells (`blk.7.ffn_gate_exps.weight`), and the bytes the
/// tensor occupies in its shard. The page places weights by these.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TensorRow {
    pub name: String,
    pub bytes: u64,
}

/// The layer facts of one saved model profile's `model` file, as
/// [`ManagerClient::model_info`] answers them: what the model profile page's
/// sliders run over. The counts are the GGUF header's own (`<arch>.block_count`,
/// `<arch>.expert_count`, `<arch>.expert_used_count`), the block count falling
/// back to the blocks the tensor names mention, and an expert count the header
/// lacks is 0. `moe` is the file having routed-expert tensors. `tensors` is every
/// tensor of every shard, each shard's tensors in data-offset order, shard
/// after shard, at most `gguf_shape::MAX_TENSOR_ROWS` of them and within
/// `gguf_shape::MAX_TENSOR_LIST_BYTES` of their JSON, and `tensors_truncated`
/// says the model has more than the list carries.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ModelFacts {
    pub block_count: u64,
    pub moe: bool,
    pub expert_count: u64,
    pub expert_used: u64,
    pub tensors: Vec<TensorRow>,
    pub tensors_truncated: bool,
}

/// Why a model profile answers no facts, as the page shows it: a key of the
/// strings files and the values of its `{0}`, `{1}` (the wire's `{key, args}`).
/// It is the answer of a read that worked, not a refusal: the manager answers
/// `Ok` with `reason` where it has no facts to read.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Reason {
    pub key: String,
    pub args: Vec<String>,
}

impl Reason {
    /// The text of the key with its args filled in, in the strings file this
    /// process reads, the way [`Refusal::text`] renders a refusal.
    ///
    /// One arm per key, each with its own args, so the strings lint
    /// (`binding_lint`) holds every text to the values it passes. A key this
    /// build does not know (a manager of another protocol) renders as the key
    /// itself, what `strings::t` makes of a key with no entry.
    pub fn text(&self) -> String {
        let v = |i: usize| self.args.get(i).map_or("", String::as_str);
        match self.key.as_str() {
            "model-info-fetched" => strings::fill("model-info-fetched", &[&v(0)]),
            "model-profile-no-model" => strings::fill("model-profile-no-model", &[&v(0)]),
            "model-info-missing" => strings::fill("model-info-missing", &[&v(0)]),
            "model-info-unreadable" => strings::fill("model-info-unreadable", &[&v(0)]),
            other => strings::t(other),
        }
    }
}

/// The `ModelInfo` / `ModelInfoAs` reply: `model` is the profile's `model`
/// resolved to an absolute path (the pool's `[*]` standing in for a section
/// that names none; `None` when there is nothing to resolve), `info` the facts
/// of that file, and `reason` why there are none. Exactly one of `info` and
/// `reason` is `Some`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ModelInfoReply {
    pub model: Option<String>,
    pub info: Option<ModelFacts>,
    pub reason: Option<Reason>,
}

/// The node's own info, as [`ManagerClient::node_info`] answers it
/// (`ARCHITECTURE.md`, "About and node info", slice AN3): the operating system
/// the host runs, what the node's Vulkan read answered, and one row per GPU
/// with the kernel's driver and the Vulkan one. The three parts are the
/// readers' own types ([`OsInfo`], [`Status`] and [`NodeGpu`]), so the reply is
/// the host's answer with nothing added to it: a fact the host did not answer
/// is `None`, and a Vulkan read that gave nothing to trust is [`Status::Failed`].
///
/// The bounds are the manager's, read into these types by `pai-core`'s own
/// readers: at most `MAX_GPU_ROWS` rows (32, `host`, the join in `vulkan`
/// truncating to the same const) and texts of at most 128 bytes for a sysfs
/// value (`SYSFS_VALUE_MAX_BYTES`) and `TEXT_MAX_BYTES` (256) for a Vulkan one,
/// a text over the cap being `None`. A caller of this reply reads the answer it
/// is given: the bff sends every field as it arrived and caps nothing again, so
/// one reader's bound is the only one a row ever meets.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NodeInfoReply {
    pub os: OsInfo,
    pub vulkan: Status,
    pub gpus: Vec<NodeGpu>,
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
    /// Creates the instance `id` of the engine `engine`, pointing it at the
    /// server profile `server_profile`. Guarded as a write of a served file is.
    fn create_instance(
        &self,
        engine: &str,
        id: &str,
        server_profile: &str,
    ) -> Result<(), ClientError> {
        let _ = (engine, id, server_profile);
        Err(ClientError::Other(
            "this transport of the manager has no CreateInstance".into(),
        ))
    }
    /// Removes the instance `id`: its unit, its record section, its assignment
    /// sections and its model-profile render, leaving the server profile, the
    /// model profiles and the state. Guarded as a write of a served file is.
    fn remove_instance(&self, id: &str) -> Result<(), ClientError> {
        let _ = id;
        Err(ClientError::Other(
            "this transport of the manager has no RemoveInstance".into(),
        ))
    }
    /// Writes the server profile `profile` of the engine `engine`: a new
    /// `server@<profile>.ini` with the server config's defaults and `port`
    /// (`None` leaves the file no `Port`, so the default port answers). An
    /// entry already at that name is [`ClientError::ServerProfileExists`], and
    /// nothing is written. Guarded as a write of a served file is.
    ///
    /// The default body is for a transport that speaks to a manager of an
    /// older protocol: the call is one it cannot make, said as
    /// [`ClientError::Other`]. A transport that speaks the method overrides it.
    fn create_server_profile(
        &self,
        engine: &str,
        profile: &str,
        port: Option<u16>,
    ) -> Result<(), ClientError> {
        let _ = (engine, profile, port);
        Err(ClientError::Other(
            "this transport of the manager has no CreateServerProfile".into(),
        ))
    }
    /// Removes the server profile `profile` of the engine `engine`, when no
    /// instance points at it: one a section names is
    /// [`ClientError::ServerProfileInUse`], one that is no file
    /// [`ClientError::NoServerProfile`]. Guarded as a write of a served file is.
    ///
    /// The default body is for a transport that speaks to a manager of an
    /// older protocol: the call is one it cannot make, said as
    /// [`ClientError::Other`]. A transport that speaks the method overrides it.
    fn delete_server_profile(&self, engine: &str, profile: &str) -> Result<(), ClientError> {
        let _ = (engine, profile);
        Err(ClientError::Other(
            "this transport of the manager has no DeleteServerProfile".into(),
        ))
    }
    /// The node's server profiles, each with the instance that points at it.
    /// Answered to any caller, as the list of instances is.
    ///
    /// The default body is for a transport that speaks to a manager of an
    /// older protocol: the call is one it cannot make, said as
    /// [`ClientError::Other`]. A transport that speaks the method overrides it.
    fn server_profiles(&self) -> Result<Vec<ServerProfileInfo>, ClientError> {
        Err(ClientError::Other(
            "this transport of the manager has no ServerProfiles".into(),
        ))
    }
    /// Writes the model profile `profile` into `models.ini.disabled`: born
    /// switched off and assigned nothing, never overwriting a name either pool
    /// file holds (`ClientError::Invalid` with `model-profile-name-taken`).
    /// Guarded as a write of a served file is.
    ///
    /// The default body is for a transport that speaks to a manager of an
    /// older protocol: the call is one it cannot make, said as
    /// [`ClientError::Other`]. A transport that speaks the method overrides it.
    fn create_model_profile(
        &self,
        profile: &str,
        source: &ModelProfileSource,
    ) -> Result<(), ClientError> {
        let _ = (profile, source);
        Err(ClientError::Other(
            "this transport of the manager has no CreateModelProfile".into(),
        ))
    }
    /// Removes the model profile `profile` from `models.ini.disabled`, when it
    /// is switched off and no instance is assigned it: a profile of `models.ini`
    /// is `model-profile-enabled`, one a key names is `model-profile-assigned`
    /// naming the instances, and a name neither file holds is
    /// `no-model-profile-named` (all `ClientError::Invalid`). Guarded as a write
    /// of a served file is.
    ///
    /// The default body is for a transport that speaks to a manager of an
    /// older protocol: the call is one it cannot make, said as
    /// [`ClientError::Other`]. A transport that speaks the method overrides it.
    fn delete_model_profile(&self, profile: &str) -> Result<(), ClientError> {
        let _ = profile;
        Err(ClientError::Other(
            "this transport of the manager has no DeleteModelProfile".into(),
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
    /// Writes the label of the instance `instance` (IL1): the text the web UI
    /// shows beside the id, with the spaces and tabs of its ends trimmed; the
    /// empty text removes the key, leaving no label. Guarded as a write of a
    /// served file is: a caller who may not configure is
    /// [`ClientError::NotAuthorized`], a text the rule refuses is
    /// [`ClientError::Invalid`] with `instance-bad-label` or `comment-markers`,
    /// a label another instance of the node holds is [`ClientError::Invalid`]
    /// with `instance-label-taken`, and an id the record lacks is
    /// [`ClientError::NoInstance`], each with nothing written. It works while
    /// the instance runs: the label is nothing the engine reads.
    ///
    /// The default body is for a transport that speaks to a manager of an
    /// older protocol: the call is one it cannot make, said as
    /// [`ClientError::Other`]. A transport that speaks the method overrides it.
    fn set_instance_label(&self, instance: &str, label: &str) -> Result<(), ClientError> {
        let _ = (instance, label);
        Err(ClientError::Other(
            "this transport of the manager has no SetInstanceLabel".into(),
        ))
    }
    /// The node's compute devices, from the engine's own probe, with the
    /// moment of that probe: the list a model profile distributes a model
    /// over, the CPU pseudo-device left out. The reply carries the host's
    /// inventory beside the list ([`NodeDevices`]'s `ram_mib` and
    /// `cpu_threads`): the manager reads its own machine for those, so the
    /// numbers are the node's, not the engine's. The manager runs its probe
    /// for this, so it is guarded as `control` (asked without interaction,
    /// the way a read that can prompt for nothing is asked), and as every
    /// `As` method for the bff's read of a signed-in user: a caller with no
    /// right is [`ClientError::NotAuthorized`] and runs no probe.
    ///
    /// The default body is for a transport that speaks to a manager of an
    /// older protocol: the call is one it cannot make, said as
    /// [`ClientError::Other`]. A transport that speaks the method overrides it.
    fn devices(&self) -> Result<NodeDevices, ClientError> {
        Err(ClientError::Other(
            "this transport of the manager has no Devices".into(),
        ))
    }
    /// The layer facts of the model of ONE saved model profile, by its id: the
    /// manager reads the file (its `model`, resolved as the GPU memory check
    /// resolves it) and answers the counts, the resolved path, or the keyed
    /// [`Reason`] why it has none (a profile to fetch, no `model`, a file that
    /// is missing, or one it cannot read as a GGUF). The read is the memory
    /// check's own, so it is guarded as reading `models.ini` is: `control`
    /// asked without interaction, and as every `As` method for the bff's read
    /// for a signed-in user, who must be a current member of `pai`; a caller
    /// with no right is [`ClientError::NotAuthorized`] with nothing read. An
    /// id the pool has no section for, and `*`, are
    /// [`ClientError::Invalid`] `no-model-profile-named`.
    ///
    /// The default body is for a transport that speaks to a manager of an
    /// older protocol: the call is one it cannot make, said as
    /// [`ClientError::Other`]. A transport that speaks the method overrides it.
    fn model_info(&self, profile: &str) -> Result<ModelInfoReply, ClientError> {
        let _ = profile;
        Err(ClientError::Other(
            "this transport of the manager has no ModelInfo".into(),
        ))
    }
    /// The node's own info ([`NodeInfoReply`]): its operating system and kernel,
    /// what its Vulkan read answered, and one row per GPU with both drivers.
    /// The manager reads the host and, for the Vulkan part, runs a child of its
    /// own executable, so the read is guarded exactly as `devices` is: `control`
    /// asked without interaction, and as every `As` method for the bff's read
    /// for a signed-in user, who must be a current member of `pai`; a caller
    /// with no right is [`ClientError::NotAuthorized`] with no child spawned.
    ///
    /// The default body is for a transport that speaks to a manager of an
    /// older protocol: the call is one it cannot make, said as
    /// [`ClientError::Other`]. A transport that speaks the method overrides it.
    fn node_info(&self) -> Result<NodeInfoReply, ClientError> {
        Err(ClientError::Other(
            "this transport of the manager has no NodeInfo".into(),
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

    /// `create_instance(engine, id, server_profile)` and `remove_instance(id)`
    /// are on the trait, each answering the default body that names the verb a
    /// transport does not speak: a new fake client gains both without writing
    /// either, as slice 14's `create_server_profile` does. A transport that
    /// speaks them overrides them.
    #[test]
    fn the_instance_verbs_are_on_the_trait_with_the_default_body() {
        for (verb, answer) in [
            (
                "CreateInstance",
                Bare.create_instance("llama.cpp", "lan-2", "big"),
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
}
