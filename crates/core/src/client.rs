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
    /// No instance profile of that name in the pool.
    NoInstanceProfile(String),
    /// The server profile is used by this other instance. The wire message is
    /// the profile and the instance, in that order, split on `SEP`, as a
    /// refusal's values are.
    ServerProfileInUse(String, String),
    /// A server profile with that name is already there: a create overwrites
    /// nothing.
    ServerProfileExists(String),
    /// An instance profile with that name is already there: a
    /// create overwrites nothing.
    InstanceProfileExists(String),
    /// The instance profile is the one this instance runs. The
    /// wire message is the profile and the instance, split on `SEP`.
    InstanceProfileInUse(String, String),
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
            "NoInstanceProfile" => ClientError::NoInstanceProfile(message.to_string()),
            "ServerProfileExists" => ClientError::ServerProfileExists(message.to_string()),
            // The instance profile pair, the in-use one split at
            // the separator as the server profile's is.
            "InstanceProfileExists" => ClientError::InstanceProfileExists(message.to_string()),
            "InstanceProfileInUse" => {
                message
                    .split_once('\u{1f}')
                    .map_or_else(other, |(profile, instance)| {
                        ClientError::InstanceProfileInUse(profile.to_string(), instance.to_string())
                    })
            }
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
            ClientError::NoInstanceProfile(_) => strings::t("client-no-instance-profile"),
            ClientError::ServerProfileInUse(profile, instance) => {
                strings::fill("client-server-profile-in-use", &[profile, instance])
            }
            ClientError::ServerProfileExists(profile) => {
                strings::fill("client-server-profile-exists", &[profile])
            }
            ClientError::InstanceProfileExists(profile) => {
                strings::fill("client-instance-profile-exists", &[profile])
            }
            ClientError::InstanceProfileInUse(profile, instance) => {
                strings::fill("client-instance-profile-in-use", &[profile, instance])
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
///
/// The last three are the states of the apply (IP3c, `ARCHITECTURE.md`
/// "Convergence"), derived by the manager from the files and never kept:
/// `converged` says the instance's rendered pair is the pair its profiles
/// render, `reason` is the keyed check that stopped the pair when it is not
/// (`None` for a converged instance), and `pending_restart` says the running
/// instance runs a pair that is not the one on disk. A manager of a protocol
/// that answers none of them reads converged with no drift.
///
/// The last two are appended to the row (IP4b): `instance_profile` is the
/// `InstanceProfile` of the instance's section, the instance profile it runs,
/// and `hostname` the `Hostname` its section sets. Each is `None` when the
/// section names none: the bus answers the empty string, the pipe a null, and
/// the record's blank is the same. `server_profile` sits one step behind
/// `instance_profile`, the profile the pool's section for it names.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InstanceInfo {
    pub id: String,
    pub engine: String,
    pub server_profile: Option<String>,
    pub port: u16,
    pub profile_tags: Vec<String>,
    pub instance_tags: Vec<String>,
    pub label: Option<String>,
    pub converged: bool,
    pub reason: Option<NotConverged>,
    pub pending_restart: bool,
    pub instance_profile: Option<String>,
    pub hostname: Option<String>,
}

/// One instance's not-converged reason: the keyed check that refused to write
/// its pair, with the arguments the check carried (the strings file fills them
/// in, `pai_core::checks`). The manager answers it as `{key, args}`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NotConverged {
    pub key: String,
    pub args: Vec<String>,
}

/// One instance a save's render pass refused, as the save's own answer reports
/// it (IP4a): the instance, and the keyed check that stopped its pair, with the
/// arguments that check carried. The wire row is `{instance, key, args}`.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct NotConvergedRow {
    pub instance: String,
    pub key: String,
    pub args: Vec<String>,
}

/// What every save that re-renders answers (`ARCHITECTURE.md` "Convergence",
/// IP4a): `running`, the running instances the write left drifting from the pair
/// they launched with, in id order, beside `not_converged`, one row per instance
/// whose pair the write did not leave, with the key of the check that stopped it.
/// [`ManagerClient`]'s re-rendering writes answer it.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Affected {
    pub running: Vec<String>,
    pub not_converged: Vec<NotConvergedRow>,
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

/// One instance profile, as `instance_profiles()` and
/// `instance_profile()` answer it (`ARCHITECTURE.md` "Instance profiles",
/// slice IP3): the section's id, the server profile its section names, and
/// the model profiles its entry sections give it. `revision` is the text of
/// `instance-profiles.ini` the read returned, the revision
/// `set_instance_profile` and `set_instance_address` send back: the pool has
/// one text, so `instance_profiles` leaves it empty and only
/// `instance_profile` answers it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InstanceProfileInfo {
    pub id: String,
    pub server_profile: String,
    pub entries: Vec<String>,
    pub revision: String,
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

/// One model file of the node's models folder, as the file list of
/// [`ModelFilesReply`] carries it (M3 of "Choosing the model file on the web",
/// `ARCHITECTURE.md`): the absolute path (the value a pick writes into the
/// profile, naming the same file on every instance), the bytes of the file
/// (the first shard's, for a split model), the number of shards of a split
/// model (1 for a single file), and the first subfolder under the models
/// folder (the GUI's `model_scan::Category::subdir` layout: `models`,
/// `mmprojs`, `mtps`, `dflashs`), `None` for a file that sits in the folder
/// itself.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ModelFileRow {
    pub path: String,
    pub size: u64,
    pub shards: u64,
    pub category: Option<String>,
}

/// The node's one models folder, as [`ManagerClient::model_files`] answers it:
/// the folder as an absolute path, the `.gguf` files under it (regular files,
/// any case, recursive to the manager's depth bound, a split model once as its
/// first shard, symlinked directories not followed, dot-folders skipped, at
/// most the manager's entry bound of them and then `truncated` true), and
/// `error`: the keyed [`Reason`] the folder answered nothing (missing, not a
/// folder, unreadable). That answer is a SUCCESS of the call, not a refusal:
/// the picker shows the empty list with the folder's own text.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ModelFilesReply {
    pub dir: String,
    pub files: Vec<ModelFileRow>,
    pub truncated: bool,
    pub error: Option<Reason>,
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
    /// A write of a served file re-renders, so it answers [`Affected`] (IP4a).
    ///
    /// The default body is for a transport that speaks no compare and swap:
    /// the call is one it cannot make, said as [`ClientError::Other`]. A
    /// transport that speaks the method overrides it.
    fn write_file_if(
        &self,
        name: &str,
        contents: &str,
        expected: &str,
    ) -> Result<Affected, ClientError> {
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
    /// instance profile `instance_profile`: a name the pool has no section for
    /// is [`ClientError::NoInstanceProfile`]. The manager proposes the
    /// instance's port, the first free one at or above 8581, writes it into the
    /// record's section and answers it (IP4b). Guarded as a write of a served
    /// file is.
    fn create_instance(
        &self,
        engine: &str,
        id: &str,
        instance_profile: &str,
    ) -> Result<u16, ClientError> {
        let _ = (engine, id, instance_profile);
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
    // The instance profile verbs (ARCHITECTURE.md "Instance
    // profiles", slice IP3). The default bodies say the transport speaks none
    // of them, as every verb of an older manager does.
    /// The node's instance profiles, each with its server profile, its entry
    /// models and the file's text as revision.
    fn instance_profiles(&self) -> Result<Vec<InstanceProfileInfo>, ClientError> {
        Err(ClientError::Other(
            "this transport of the manager has no InstanceProfiles".into(),
        ))
    }
    /// One instance profile, by id.
    fn instance_profile(&self, profile: &str) -> Result<InstanceProfileInfo, ClientError> {
        let _ = profile;
        Err(ClientError::Other(
            "this transport of the manager has no GetInstanceProfile".into(),
        ))
    }
    /// Writes a new instance profile running the server profile
    /// `server_profile`. Guarded as a write of a served file is.
    fn create_instance_profile(
        &self,
        profile: &str,
        server_profile: &str,
    ) -> Result<(), ClientError> {
        let _ = (profile, server_profile);
        Err(ClientError::Other(
            "this transport of the manager has no CreateInstanceProfile".into(),
        ))
    }
    /// Removes an instance profile no instance runs. Guarded as a write of a
    /// served file is.
    fn delete_instance_profile(&self, profile: &str) -> Result<(), ClientError> {
        let _ = profile;
        Err(ClientError::Other(
            "this transport of the manager has no DeleteInstanceProfile".into(),
        ))
    }
    /// Gives the instance profile `profile` the model profile `model`, an
    /// empty entry. Guarded as a write of a served file is. Answers
    /// [`Affected`]: the add re-renders every instance running the profile.
    fn add_instance_profile_model(
        &self,
        profile: &str,
        model: &str,
    ) -> Result<Affected, ClientError> {
        let _ = (profile, model);
        Err(ClientError::Other(
            "this transport of the manager has no AddInstanceProfileModel".into(),
        ))
    }
    /// Takes the model profile `model` away from the instance profile
    /// `profile`. Guarded as a write of a served file is. Answers
    /// [`Affected`].
    fn remove_instance_profile_model(
        &self,
        profile: &str,
        model: &str,
    ) -> Result<Affected, ClientError> {
        let _ = (profile, model);
        Err(ClientError::Other(
            "this transport of the manager has no RemoveInstanceProfileModel".into(),
        ))
    }
    /// Saves the entry `[<profile>/<model>]` with `values`, the entry whole as
    /// the page sent it, only while `instance-profiles.ini` still holds
    /// `revision`, the text the caller read: other bytes are
    /// [`ClientError::Changed`], and the save writes nothing. The entry belongs
    /// to the profile, so no instance is named (IP4a): the save re-renders every
    /// instance running the profile, and answers [`Affected`]. An entry of a
    /// profile that does not list the model is the keyed refusal
    /// `no-instance-profile-model`, and a value the entry checks refuse is
    /// [`ClientError::Invalid`]. Guarded as a write of a served file is.
    ///
    /// The default body is for a transport that speaks to a manager of an
    /// older protocol: the call is one it cannot make, said as
    /// [`ClientError::Other`]. A transport that speaks the method overrides it.
    fn set_instance_profile_model(
        &self,
        profile: &str,
        model: &str,
        values: &[(String, String)],
        revision: &str,
    ) -> Result<Affected, ClientError> {
        let _ = (profile, model, values, revision);
        Err(ClientError::Other(
            "this transport of the manager has no SetInstanceProfileModel".into(),
        ))
    }
    /// Points the instance profile `profile` at the server profile
    /// `server_profile`: the switch moves the ground of every entry of the
    /// profile, so every instance running it renders again. No revision: it is
    /// one set-like change. A profile the pool has no section for is
    /// [`ClientError::NoInstanceProfile`], a server profile the node has no file
    /// for is [`ClientError::NoServerProfile`]. A card the switch takes away is
    /// no refusal: the instances whose render stops on the card keep their last
    /// good pair and arrive in [`Affected`]'s `not_converged`. Guarded as a
    /// write of a served file is.
    ///
    /// The default body is for a transport that speaks to a manager of an
    /// older protocol: the call is one it cannot make, said as
    /// [`ClientError::Other`]. A transport that speaks the method overrides it.
    fn set_instance_profile_server(
        &self,
        profile: &str,
        server_profile: &str,
    ) -> Result<Affected, ClientError> {
        let _ = (profile, server_profile);
        Err(ClientError::Other(
            "this transport of the manager has no SetInstanceProfileServer".into(),
        ))
    }
    /// Points the instance `instance` at the instance profile `profile`,
    /// only while `instance-profiles.ini` still holds `revision`, the text
    /// the caller read: other bytes are `ClientError::Changed`, and the
    /// switch writes nothing. Guarded as a write of a served file is. Answers
    /// [`Affected`].
    fn set_instance_profile(
        &self,
        instance: &str,
        profile: &str,
        revision: &str,
    ) -> Result<Affected, ClientError> {
        let _ = (instance, profile, revision);
        Err(ClientError::Other(
            "this transport of the manager has no SetInstanceProfile".into(),
        ))
    }
    /// Sets the address of the instance `instance` to `host` and `port`
    /// (strings, as every address text on the wire; the empty host removes
    /// the key), only while `instances.ini` still holds `revision`, the text
    /// the caller read: other bytes are `ClientError::Changed`, and the
    /// address writes nothing. An address another instance of the node, or
    /// the web service, holds is `ClientError::AddressInUse`. Guarded as a
    /// write of a served file is. The address is a card fact, so the write
    /// re-renders and answers [`Affected`].
    fn set_instance_address(
        &self,
        instance: &str,
        host: &str,
        port: &str,
        revision: &str,
    ) -> Result<Affected, ClientError> {
        let _ = (instance, host, port, revision);
        Err(ClientError::Other(
            "this transport of the manager has no SetInstanceAddress".into(),
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
    /// of a served file is. A delete re-renders, so it answers [`Affected`].
    ///
    /// The default body is for a transport that speaks to a manager of an
    /// older protocol: the call is one it cannot make, said as
    /// [`ClientError::Other`]. A transport that speaks the method overrides it.
    fn delete_model_profile(&self, profile: &str) -> Result<Affected, ClientError> {
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
    /// The node's one models folder as the model-file picker's list
    /// ([`ModelFilesReply`], M3 of "Choosing the model file on the web",
    /// `ARCHITECTURE.md`): the manager walks the folder (the bff's account
    /// cannot read it), so the read is guarded exactly as `devices` is:
    /// `control` asked without interaction, and as every `As` method for the
    /// bff's read for a signed-in user, who must be a current member of `pai`;
    /// a caller with no right is [`ClientError::NotAuthorized`] with the
    /// folder never opened.
    ///
    /// The default body is for a transport that speaks to a manager of an
    /// older protocol: the call is one it cannot make, said as
    /// [`ClientError::Other`]. A transport that speaks the method overrides it.
    fn model_files(&self) -> Result<ModelFilesReply, ClientError> {
        Err(ClientError::Other(
            "this transport of the manager has no ModelFiles".into(),
        ))
    }
    /// Whether `password` is `user`'s OS password, and whether that user may
    /// configure: (ok, can_configure). The manager answers only the bff's
    /// account; anyone else is [`ClientError::NotAuthorized`].
    fn authenticate(&self, user: &str, password: &str) -> Result<(bool, bool), ClientError>;
}

#[cfg(test)]
#[path = "client_tests.rs"]
mod tests;
