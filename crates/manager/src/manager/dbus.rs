//! The D-Bus face of `pai-manager` on Linux: interface
//! `org.privateai.Manager1` at `/org/privateai/Manager1` on the system bus,
//! over the core [`Manager`]. The caller of a method is its sender (the
//! unique bus name the bus stamps), handed to the core as a [`Requester`].
//! The guarded methods are served in their `As` form too, with the principal
//! the caller vouches for as their first argument (the manager answers a
//! principal for the bff's account alone); `Version` answers the framework
//! version and [`PROTOCOL`] to anybody. Every [`ManagerError`] replies as a
//! D-Bus error `org.privateai.Manager1.Error.<Kind>`.

use super::action_log::WriterLog;
use super::pipe::PROTOCOL;
pub use super::service::state_name;
use super::service::{
    Action, Affected, Authority, Caller, HostNodeFacts, Manager, ManagerError, Principal,
    ProbedGpus, Requester, Resolver, ServiceControl, ServiceError, SystemResolver, UnitState,
};
use std::sync::Arc;
use zbus::blocking::{Connection, Proxy};
use zbus::zvariant::OwnedObjectPath;

/// The well-known name the service owns on the system bus.
pub const BUS_NAME: &str = "org.privateai.Manager1";
/// The object the interface is served at.
pub const OBJECT_PATH: &str = "/org/privateai/Manager1";
/// The interface name.
pub const INTERFACE: &str = "org.privateai.Manager1";

/// The D-Bus error name and message `e` replies with.
pub fn error_reply(e: &ManagerError) -> (String, String) {
    use zbus::DBusError;
    let error = Error::from(e.clone());
    let message = error.description().unwrap_or_default().to_string();
    (error.name().to_string(), message)
}

/// The caller of a method call from its sender; a call without one is
/// refused before anything is asked or done.
pub fn caller(sender: Option<&str>) -> Result<Caller, ManagerError> {
    match sender {
        Some(name) if !name.is_empty() => Ok(Caller(name.to_string())),
        _ => Err(ManagerError::NotAuthorized),
    }
}

/// The error replies of the interface, `org.privateai.Manager1.Error.<Kind>`,
/// one per [`ManagerError`], the message carrying its detail.
#[derive(Debug, zbus::DBusError)]
#[zbus(prefix = "org.privateai.Manager1.Error")]
pub enum Error {
    NotAuthorized(String),
    InvalidId(String),
    NoInstance(String),
    BadFileName(String),
    AddressInUse(String),
    NoServerProfile(String),
    /// The pool has no instance profile of this name; its message is the
    /// name.
    NoInstanceProfile(String),
    /// The message is the profile and the instance, split at the separator
    /// `Refusal::encode` uses.
    ServerProfileInUse(String),
    /// A create found an entry already at the profile's name; its message is
    /// the profile.
    ServerProfileExists(String),
    /// A create found an entry already at the instance profile's name; its
    /// message is the profile.
    InstanceProfileExists(String),
    /// The message is the instance profile and the instance that runs it,
    /// split at the separator `Refusal::encode` uses.
    InstanceProfileInUse(String),
    UnresolvableHost(String),
    Service(String),
    Io(String),
    /// The message is `Refusal::encode`'s string: the client decodes it.
    Invalid(String),
    /// A compare-and-swap write found the file no longer holding the text it
    /// was given to expect; its message is the kind's own word.
    Changed(String),
}

/// The message is `service::error_reply`'s, the same on every transport.
impl From<ManagerError> for Error {
    fn from(e: ManagerError) -> Self {
        let (_, message) = super::service::error_reply(&e);
        match e {
            ManagerError::NotAuthorized => Error::NotAuthorized(message),
            ManagerError::InvalidId => Error::InvalidId(message),
            ManagerError::NoInstance => Error::NoInstance(message),
            ManagerError::BadFileName => Error::BadFileName(message),
            ManagerError::AddressInUse(_) => Error::AddressInUse(message),
            ManagerError::NoServerProfile(_) => Error::NoServerProfile(message),
            ManagerError::NoInstanceProfile(_) => Error::NoInstanceProfile(message),
            ManagerError::ServerProfileInUse(_, _) => Error::ServerProfileInUse(message),
            ManagerError::ServerProfileExists(_) => Error::ServerProfileExists(message),
            ManagerError::InstanceProfileExists(_) => Error::InstanceProfileExists(message),
            ManagerError::InstanceProfileInUse(_, _) => Error::InstanceProfileInUse(message),
            ManagerError::UnresolvableHost(_) => Error::UnresolvableHost(message),
            ManagerError::Service(_) => Error::Service(message),
            ManagerError::Io(_) => Error::Io(message),
            ManagerError::Invalid(_) => Error::Invalid(message),
            ManagerError::Changed => Error::Changed(message),
        }
    }
}

/// The served object: the core, shared with whoever else holds it. zbus
/// runs every method of a connection on that connection's one executor
/// thread, so each method runs its core call on a thread of the `blocking`
/// pool: a caller polkit is still asking about holds up no other call.
pub struct ManagerInterface<S: ServiceControl, A: Authority, R: Resolver> {
    manager: Arc<Manager<S, A, R>>,
}

impl<S: ServiceControl, A: Authority, R: Resolver> ManagerInterface<S, A, R> {
    pub fn new(manager: Arc<Manager<S, A, R>>) -> Self {
        Self { manager }
    }
}

impl<S, A, R> ManagerInterface<S, A, R>
where
    S: ServiceControl + Send + Sync + 'static,
    A: Authority + Send + Sync + 'static,
    R: Resolver + Send + Sync + 'static,
{
    /// `f` over the core, off the connection's executor thread.
    async fn core<T: Send + 'static>(
        &self,
        f: impl FnOnce(&Manager<S, A, R>) -> T + Send + 'static,
    ) -> T {
        let manager = self.manager.clone();
        blocking::unblock(move || f(&manager)).await
    }
}

/// The caller of a call from its header.
fn sender_of(header: &zbus::message::Header<'_>) -> Result<Caller, ManagerError> {
    caller(header.sender().map(|s| s.as_str()))
}

/// The requester of a call from its header: its sender, with no principal
/// (the D-Bus face carries none: the `As` methods come with the client that
/// sends one).
fn requester_of(header: &zbus::message::Header<'_>) -> Result<Requester, ManagerError> {
    sender_of(header).map(|caller| Requester {
        caller,
        principal: None,
    })
}

/// The requester of an `As` call: its sender and the principal the caller
/// vouches for. A call with no sender is refused like the plain methods: the
/// core checks the caller's own account is the bff's before it trusts a
/// principal.
fn requester_as(
    header: &zbus::message::Header<'_>,
    principal: String,
) -> Result<Requester, ManagerError> {
    sender_of(header).map(|caller| Requester {
        caller,
        principal: Some(Principal(principal)),
    })
}

/// The port of `CreateServerProfile`: `0` on the wire is no port, the file
/// written with no `Port` key at all.
fn port_of(port: u16) -> Option<u16> {
    (port != 0).then_some(port)
}

/// One row of `Devices`, `a(ssstt)`: `(s id, s name, s backend, t total_mib,
/// t free_mib)`. The sizes are uint64: a card's total in MiB grows past a
/// uint16, and a signed type has no reading for a device that reports none.
type DeviceRow = (String, String, String, u64, u64);

/// The bus type of a save that re-renders answers, `a{sv}`
/// ([`affected_answer`], IP3c).
type AffectedAnswer = std::collections::HashMap<String, zbus::zvariant::Value<'static>>;

/// The node's device list as the bus answers it, with the moment of the probe
/// and the host's inventory beside it: `Devices` is
/// `(t probed_at, a(ssstt) devices, t ram_mib, u cpu_threads)` (R1,
/// ARCHITECTURE.md (1) "The node inventory"). `0` is no probe and no figure,
/// as `port_of`'s `0` is no port.
fn device_list(list: pai_core::client::NodeDevices) -> (u64, Vec<DeviceRow>, u64, u32) {
    (
        list.probed_at.unwrap_or_default(),
        list.devices
            .into_iter()
            .map(|d| (d.id, d.name, d.backend, d.total_mib, d.free_mib))
            .collect(),
        list.ram_mib,
        list.cpu_threads,
    )
}

/// The answer of a save that re-renders, as the bus carries it (IP3c,
/// `ARCHITECTURE.md` "Convergence"): `a{sv}` with `running`, the ids of the
/// running instances the save moved to a restart (`as`), and `notConverged`, one
/// `a{sv}` row per instance that kept its last good pair (`av`), each with the
/// `instance`, the `key` of the check that stopped it and its `args`. The fields
/// are named, as the page reads them by name; the stream's answer is the same
/// object. A dict of values is the one shape that carries both a list of texts
/// and a list of dicts, the bus having no null and no optional field.
fn affected_answer(affected: Affected) -> AffectedAnswer {
    use std::collections::HashMap;
    use zbus::zvariant::{OwnedValue, Value};
    let rows = affected
        .not_converged
        .iter()
        .map(|row| {
            let mut fields: HashMap<String, Value<'static>> = HashMap::new();
            fields.insert("instance".to_string(), Value::new(row.instance.clone()));
            fields.insert("key".to_string(), Value::new(row.key.clone()));
            fields.insert("args".to_string(), Value::new(row.args.clone()));
            OwnedValue::from(fields)
        })
        .collect::<Vec<_>>();
    let mut answer: AffectedAnswer = HashMap::new();
    answer.insert("running".to_string(), Value::new(affected.running));
    answer.insert("notConverged".to_string(), Value::new(rows));
    answer
}

/// The `ModelInfo` reply as the bus answers it:
/// `(s model, (b has_info, t block_count, b moe, t expert_count,
/// t expert_used, a(st) tensors, b tensors_truncated), (s key, as args))`.
/// `model` is the path the read opened, the empty string when the profile names
/// none; `has_info` is the facts' part, `key` the keyed why's. The bus has no
/// null, so the flag says which half of the reply is there, and the half that
/// is not carries zeros, the empty strings and an empty list: exactly what the
/// stream's nulls say.
type ModelInfoRow = (
    String,
    (bool, u64, bool, u64, u64, Vec<(String, u64)>, bool),
    (String, Vec<String>),
);

fn model_info_row(reply: pai_core::client::ModelInfoReply) -> ModelInfoRow {
    let facts = reply
        .info
        .map(|f| {
            (
                true,
                f.block_count,
                f.moe,
                f.expert_count,
                f.expert_used,
                f.tensors.into_iter().map(|t| (t.name, t.bytes)).collect(),
                f.tensors_truncated,
            )
        })
        .unwrap_or((false, 0, false, 0, 0, Vec::new(), false));
    let reason = reply.reason.map(|r| (r.key, r.args)).unwrap_or_default();
    (reply.model.unwrap_or_default(), facts, reason)
}

/// One row of `NodeInfo`'s GPU list, as the bus answers it: `(s pci, s vendor,
/// s name, s driver, s version, b has_vulkan, s v_driver, s v_info)`, the list
/// `a(sssssbss)`. The bus has no null, so every text the host did not answer
/// is the empty string, and the Vulkan pair carries its own flag, the way
/// `ModelInfoRow` flags its facts: an empty driver name is not no Vulkan
/// device.
type NodeGpuRow = (String, String, String, String, String, bool, String, String);

/// The `NodeInfo` reply as the bus answers it: `(s name, s version, s kernel,
/// s vulkan, a(sssssbss) gpus)`: the three OS texts, the Vulkan word (`ok`,
/// `none` or `failed`), then the rows in the manager's order, kernel rows
/// first.
type NodeInfoRow = (String, String, String, String, Vec<NodeGpuRow>);

fn node_info_row(info: pai_core::client::NodeInfoReply) -> NodeInfoRow {
    (
        info.os.name.unwrap_or_default(),
        info.os.version.unwrap_or_default(),
        info.os.kernel.unwrap_or_default(),
        pai_core::vulkan::word(info.vulkan).to_string(),
        info.gpus
            .into_iter()
            .map(|g| {
                // The pair travels only with its flag: no Vulkan device is
                // the flag false and the texts empty.
                let (has_vulkan, driver, v_info) = match g.vulkan {
                    Some(d) => (
                        true,
                        d.driver.unwrap_or_default(),
                        d.info.unwrap_or_default(),
                    ),
                    None => (false, String::new(), String::new()),
                };
                (
                    g.pci.unwrap_or_default(),
                    g.vendor.unwrap_or_default(),
                    g.name.unwrap_or_default(),
                    g.driver.unwrap_or_default(),
                    g.version.unwrap_or_default(),
                    has_vulkan,
                    driver,
                    v_info,
                )
            })
            .collect(),
    )
}

/// One row of `ModelFiles`' list, as the bus answers it: `(s path, t size,
/// t shards, b has_category, s category)`, the list `a(sttbs)`. The bus has no
/// null, so the category travels with its flag the way `NodeGpuRow`'s Vulkan
/// pair does: a file of the folder itself is the flag false and the empty
/// string, not a category of nothing.
type ModelFileBusRow = (String, u64, u64, bool, String);

/// The `ModelFiles` reply as the bus answers it: `(s dir, a(sttbs) files,
/// b truncated, (s key, as args))`: the folder as an absolute path, the rows
/// in the walk's order, the flag saying the walk stopped at its bound, and the
/// folder's keyed why (`key` the empty string when the folder read).
type ModelFilesRow = (String, Vec<ModelFileBusRow>, bool, (String, Vec<String>));

fn model_files_row(reply: pai_core::client::ModelFilesReply) -> ModelFilesRow {
    (
        reply.dir,
        reply
            .files
            .into_iter()
            .map(|f| {
                let (has_category, category) = match f.category {
                    Some(c) => (true, c),
                    None => (false, String::new()),
                };
                (f.path, f.size, f.shards, has_category, category)
            })
            .collect(),
        reply.truncated,
        reply.error.map(|e| (e.key, e.args)).unwrap_or_default(),
    )
}

#[zbus::interface(name = "org.privateai.Manager1")]
impl<S, A, R> ManagerInterface<S, A, R>
where
    S: ServiceControl + Send + Sync + 'static,
    A: Authority + Send + Sync + 'static,
    R: Resolver + Send + Sync + 'static,
{
    async fn list_instances(&self) -> Vec<String> {
        self.core(|m| m.list_instances()).await
    }

    /// `Instances`: one row per instance, `(sssqasass)`, the third field the
    /// name of the server profile its section names (the empty string when it
    /// names none), the last two arrays the tags of that profile and the tags
    /// of its own section, the last field the label of its section (the
    /// empty string when it has none: the bus has no null). Answered to any
    /// caller, as the list of ids is. The three states of the apply (IP3c) are
    /// not in this row: they are the sibling method [`Self::instance_states`],
    /// which the client merges back into these rows.
    async fn instances(
        &self,
    ) -> Vec<(
        String,
        String,
        String,
        u16,
        Vec<String>,
        Vec<String>,
        String,
    )> {
        self.core(|m| {
            m.instances()
                .into_iter()
                .map(|i| {
                    (
                        i.id,
                        i.engine,
                        i.server_profile.unwrap_or_default(),
                        i.port,
                        i.profile_tags,
                        i.instance_tags,
                        i.label.unwrap_or_default(),
                    )
                })
                .collect()
        })
        .await
    }

    /// `InstanceStates`: the three states of the apply of every instance of the
    /// record, `a(sbssasb)`, one row `(s id, b converged, s reason key,
    /// as reason args, b pendingRestart)` per instance, the reason the empty
    /// string and no args for a converged instance. The sibling method of
    /// [`Self::instances`], whose row keeps its signature (IP3c review 6): the
    /// client of the bus calls both and merges the rows, as the web service
    /// reads them in one row over the stream. Converged with its reason are the
    /// rows the last render pass left in memory (review 8), the drift the compare
    /// of the pair on disk against what the instance's launch applied: the call
    /// asks the service manager for no unit and the engine's probe for no card.
    /// Answered to any caller, as the list it belongs to is.
    async fn instance_states(&self) -> Vec<(String, bool, String, Vec<String>, bool)> {
        self.core(|m| {
            m.instances()
                .into_iter()
                .map(|i| {
                    let (key, args) = match i.reason {
                        Some(reason) => (reason.key, reason.args),
                        None => (String::new(), Vec::new()),
                    };
                    (i.id, i.converged, key, args, i.pending_restart)
                })
                .collect()
        })
        .await
    }

    /// `ServerProfiles`: one row per server profile of the node, `a(ssqass)`,
    /// the last field the id of the instance that runs it (the empty string
    /// when no instance does). Answered to any caller, as the list of instances
    /// is.
    async fn server_profiles(&self) -> Vec<(String, String, u16, Vec<String>, String)> {
        self.core(|m| {
            m.server_profiles()
                .into_iter()
                .map(|p| {
                    (
                        p.id,
                        p.engine,
                        p.port,
                        p.tags,
                        p.instance.unwrap_or_default(),
                    )
                })
                .collect()
        })
        .await
    }

    /// `Devices`: the node's compute devices as the engine's own probe read
    /// them, `(t probed_at, a(ssstt) devices, t ram_mib, u cpu_threads)`: the
    /// first the Unix seconds of the probe, 0 when no probe answered, the rows
    /// the id `device` takes, the product name, the backend and the device
    /// memory in MiB, in the probe's order, the CPU pseudo-device left out;
    /// then the host's own RAM in MiB and its logical CPUs, answered whatever
    /// the probe did. `control` asked of polkit WITHOUT interaction: the
    /// manager runs the engine to read this, and a device list is worth no
    /// challenge over the caller's own session.
    async fn devices(
        &self,
        #[zbus(header)] header: zbus::message::Header<'_>,
    ) -> Result<(u64, Vec<DeviceRow>, u64, u32), Error> {
        let who = requester_of(&header)?;
        Ok(device_list(self.core(move |m| m.devices(&who)).await?))
    }

    /// `DevicesAs`: the same list, asked as a principal the caller vouches for:
    /// the bff's account, and a current member of `pai`. No polkit question at
    /// all: the pair is the right.
    async fn devices_as(
        &self,
        #[zbus(header)] header: zbus::message::Header<'_>,
        principal: String,
    ) -> Result<(u64, Vec<DeviceRow>, u64, u32), Error> {
        let who = requester_as(&header, principal)?;
        Ok(device_list(self.core(move |m| m.devices(&who)).await?))
    }

    /// `ModelInfo`: the layer facts of one saved model profile's `model` file,
    /// `(s model, (b has_info, t block_count, b moe, t expert_count,
    /// t expert_used, a(st) tensors, b tensors_truncated), (s key, as args))`,
    /// the parts of `model_info_row`. The
    /// gate of reading `models.ini`: `control` asked of polkit WITHOUT
    /// interaction, and a refused read opens nothing. An id the pool has no
    /// section for, and `*`, are the keyed `no-model-profile-named`.
    async fn model_info(
        &self,
        #[zbus(header)] header: zbus::message::Header<'_>,
        profile: String,
    ) -> Result<ModelInfoRow, Error> {
        let who = requester_of(&header)?;
        Ok(model_info_row(
            self.core(move |m| m.model_info(&who, &profile)).await?,
        ))
    }

    /// `ModelInfoAs`: the same facts, asked as a principal the caller vouches
    /// for: the bff's account, and a current member of `pai`. No polkit
    /// question at all: the pair is the right.
    async fn model_info_as(
        &self,
        #[zbus(header)] header: zbus::message::Header<'_>,
        principal: String,
        profile: String,
    ) -> Result<ModelInfoRow, Error> {
        let who = requester_as(&header, principal)?;
        Ok(model_info_row(
            self.core(move |m| m.model_info(&who, &profile)).await?,
        ))
    }

    /// `NodeInfo`: the node's own info, `(s name, s version, s kernel,
    /// s vulkan, a(sssssbss) gpus)` (AN3, ARCHITECTURE.md "About and node
    /// info"): the three OS texts, the word the node's Vulkan read answered,
    /// and one row per GPU with the kernel's driver and the Vulkan one joined
    /// onto it, kernel rows first. `control` asked of polkit WITHOUT
    /// interaction, as `Devices` asks it: the read may spawn the Vulkan child,
    /// and a node's drivers are worth no challenge over the caller's own
    /// session.
    async fn node_info(
        &self,
        #[zbus(header)] header: zbus::message::Header<'_>,
    ) -> Result<NodeInfoRow, Error> {
        let who = requester_of(&header)?;
        Ok(node_info_row(self.core(move |m| m.node_info(&who)).await?))
    }

    /// `NodeInfoAs`: the same document, asked as a principal the caller
    /// vouches for: the bff's account, and a current member of `pai`. No
    /// polkit question at all: the pair is the right.
    async fn node_info_as(
        &self,
        #[zbus(header)] header: zbus::message::Header<'_>,
        principal: String,
    ) -> Result<NodeInfoRow, Error> {
        let who = requester_as(&header, principal)?;
        Ok(node_info_row(self.core(move |m| m.node_info(&who)).await?))
    }

    /// `ModelFiles`: the node's one models folder as the model-file picker's
    /// list, `(s dir, a(sttbs) files, b truncated, (s key, as args))`, the
    /// parts of `model_files_row`. The gate of [`Self::devices`]: `control`
    /// asked of polkit WITHOUT interaction, and a refused read opens no folder.
    /// A folder that is missing, is not a folder, or cannot be read is a
    /// SUCCESS with no rows and the keyed `models-dir-unreadable` in the last
    /// part: the picker's empty list with a text, never an error reply.
    async fn model_files(
        &self,
        #[zbus(header)] header: zbus::message::Header<'_>,
    ) -> Result<ModelFilesRow, Error> {
        let who = requester_of(&header)?;
        Ok(model_files_row(
            self.core(move |m| m.model_files(&who)).await?,
        ))
    }

    /// `ModelFilesAs`: the same list, asked as a principal the caller vouches
    /// for: the bff's account, and a current member of `pai`. No polkit
    /// question at all: the pair is the right.
    async fn model_files_as(
        &self,
        #[zbus(header)] header: zbus::message::Header<'_>,
        principal: String,
    ) -> Result<ModelFilesRow, Error> {
        let who = requester_as(&header, principal)?;
        Ok(model_files_row(
            self.core(move |m| m.model_files(&who)).await?,
        ))
    }

    async fn status(&self, id: String) -> Result<String, Error> {
        let state = self.core(move |m| m.status(&id)).await?;
        Ok(state_name(state).to_string())
    }

    async fn start(
        &self,
        #[zbus(header)] header: zbus::message::Header<'_>,
        id: String,
    ) -> Result<(), Error> {
        let who = requester_of(&header)?;
        Ok(self.core(move |m| m.start(&who, &id)).await?)
    }

    async fn stop(
        &self,
        #[zbus(header)] header: zbus::message::Header<'_>,
        id: String,
    ) -> Result<(), Error> {
        let who = requester_of(&header)?;
        Ok(self.core(move |m| m.stop(&who, &id)).await?)
    }

    async fn restart(
        &self,
        #[zbus(header)] header: zbus::message::Header<'_>,
        id: String,
    ) -> Result<(), Error> {
        let who = requester_of(&header)?;
        Ok(self.core(move |m| m.restart(&who, &id)).await?)
    }

    async fn boot_enabled(&self, id: String) -> Result<bool, Error> {
        Ok(self.core(move |m| m.boot_enabled(&id)).await?)
    }

    async fn set_boot_enabled(
        &self,
        #[zbus(header)] header: zbus::message::Header<'_>,
        id: String,
        enabled: bool,
    ) -> Result<(), Error> {
        let who = requester_of(&header)?;
        Ok(self
            .core(move |m| m.set_boot_enabled(&who, &id, enabled))
            .await?)
    }

    async fn read_file(
        &self,
        #[zbus(header)] header: zbus::message::Header<'_>,
        name: String,
    ) -> Result<(bool, String, bool), Error> {
        let who = requester_of(&header).ok();
        Ok(
            match self.core(move |m| m.read_file(who.as_ref(), &name)).await? {
                Some(file) => (true, file.text, file.withheld),
                None => (false, String::new(), false),
            },
        )
    }

    /// The bff's sign-in: (ok, can_configure); only the bff may ask.
    async fn authenticate(
        &self,
        #[zbus(header)] header: zbus::message::Header<'_>,
        user: String,
        password: String,
    ) -> Result<(bool, bool), Error> {
        let caller = sender_of(&header)?;
        Ok(self
            .core(move |m| m.authenticate(&caller, &user, &password))
            .await?)
    }

    async fn write_file(
        &self,
        #[zbus(header)] header: zbus::message::Header<'_>,
        name: String,
        contents: String,
        keep_api_key: bool,
    ) -> Result<(), Error> {
        let who = requester_of(&header)?;
        Ok(self
            .core(move |m| m.write_file(&who, &name, &contents, keep_api_key))
            .await?)
    }

    /// The compare and swap: `expected` is the text the caller read, and the
    /// file still holds (`Manager::write_file_if`).
    async fn write_file_if(
        &self,
        #[zbus(header)] header: zbus::message::Header<'_>,
        name: String,
        contents: String,
        expected: String,
    ) -> Result<AffectedAnswer, Error> {
        let who = requester_of(&header)?;
        Ok(affected_answer(
            self.core(move |m| m.write_file_if(&who, &name, &contents, &expected))
                .await?,
        ))
    }

    /// `Start` for `principal`, the signed-in user the caller vouches for;
    /// the principal comes first, as the design's `StartAs(s principal, s id)`.
    async fn start_as(
        &self,
        #[zbus(header)] header: zbus::message::Header<'_>,
        principal: String,
        id: String,
    ) -> Result<(), Error> {
        let who = requester_as(&header, principal)?;
        Ok(self.core(move |m| m.start(&who, &id)).await?)
    }

    /// `Stop` for `principal`.
    async fn stop_as(
        &self,
        #[zbus(header)] header: zbus::message::Header<'_>,
        principal: String,
        id: String,
    ) -> Result<(), Error> {
        let who = requester_as(&header, principal)?;
        Ok(self.core(move |m| m.stop(&who, &id)).await?)
    }

    /// `Restart` for `principal`.
    async fn restart_as(
        &self,
        #[zbus(header)] header: zbus::message::Header<'_>,
        principal: String,
        id: String,
    ) -> Result<(), Error> {
        let who = requester_as(&header, principal)?;
        Ok(self.core(move |m| m.restart(&who, &id)).await?)
    }

    /// `SetBootEnabled` for `principal`.
    async fn set_boot_enabled_as(
        &self,
        #[zbus(header)] header: zbus::message::Header<'_>,
        principal: String,
        id: String,
        enabled: bool,
    ) -> Result<(), Error> {
        let who = requester_as(&header, principal)?;
        Ok(self
            .core(move |m| m.set_boot_enabled(&who, &id, enabled))
            .await?)
    }

    /// `SetModelProfile`: the model profile `profile` assigned to instance
    /// `instance` as `value`, for the caller acting as itself (`sss`).
    async fn set_model_profile(
        &self,
        #[zbus(header)] header: zbus::message::Header<'_>,
        instance: String,
        profile: String,
        value: String,
    ) -> Result<AffectedAnswer, Error> {
        let who = requester_of(&header)?;
        Ok(affected_answer(
            self.core(move |m| m.set_model_profile(&who, &instance, &profile, &value))
                .await?,
        ))
    }

    /// `SetModelProfile` for `principal`, the principal first as every `As`
    /// method has it (`ssss`).
    async fn set_model_profile_as(
        &self,
        #[zbus(header)] header: zbus::message::Header<'_>,
        principal: String,
        instance: String,
        profile: String,
        value: String,
    ) -> Result<AffectedAnswer, Error> {
        let who = requester_as(&header, principal)?;
        Ok(affected_answer(
            self.core(move |m| m.set_model_profile(&who, &instance, &profile, &value))
                .await?,
        ))
    }

    /// `SetServerProfile`: the server profile an instance runs, `(ss)`.
    async fn set_server_profile(
        &self,
        #[zbus(header)] header: zbus::message::Header<'_>,
        instance: String,
        profile: String,
    ) -> Result<AffectedAnswer, Error> {
        let who = requester_of(&header)?;
        Ok(affected_answer(
            self.core(move |m| m.set_server_profile(&who, &instance, &profile))
                .await?,
        ))
    }

    /// `SetServerProfile` for `principal`, the principal first as every `As`
    /// method has it (`sss`).
    async fn set_server_profile_as(
        &self,
        #[zbus(header)] header: zbus::message::Header<'_>,
        principal: String,
        instance: String,
        profile: String,
    ) -> Result<AffectedAnswer, Error> {
        let who = requester_as(&header, principal)?;
        Ok(affected_answer(
            self.core(move |m| m.set_server_profile(&who, &instance, &profile))
                .await?,
        ))
    }

    /// `SetInstanceTags`: the record's own tags of one instance, `(sas)`.
    async fn set_instance_tags(
        &self,
        #[zbus(header)] header: zbus::message::Header<'_>,
        instance: String,
        tags: Vec<String>,
    ) -> Result<(), Error> {
        let who = requester_of(&header)?;
        Ok(self
            .core(move |m| m.set_instance_tags(&who, &instance, &tags))
            .await?)
    }

    /// `SetInstanceTags` for `principal`, the principal first as every `As`
    /// method has it (`ssas`).
    async fn set_instance_tags_as(
        &self,
        #[zbus(header)] header: zbus::message::Header<'_>,
        principal: String,
        instance: String,
        tags: Vec<String>,
    ) -> Result<(), Error> {
        let who = requester_as(&header, principal)?;
        Ok(self
            .core(move |m| m.set_instance_tags(&who, &instance, &tags))
            .await?)
    }

    /// `SetInstanceLabel`: the label of one instance, `(ss)`. The empty text
    /// removes the key.
    async fn set_instance_label(
        &self,
        #[zbus(header)] header: zbus::message::Header<'_>,
        instance: String,
        label: String,
    ) -> Result<(), Error> {
        let who = requester_of(&header)?;
        Ok(self
            .core(move |m| m.set_instance_label(&who, &instance, &label))
            .await?)
    }

    /// `SetInstanceLabel` for `principal`, the principal first as every `As`
    /// method has it (`sss`).
    async fn set_instance_label_as(
        &self,
        #[zbus(header)] header: zbus::message::Header<'_>,
        principal: String,
        instance: String,
        label: String,
    ) -> Result<(), Error> {
        let who = requester_as(&header, principal)?;
        Ok(self
            .core(move |m| m.set_instance_label(&who, &instance, &label))
            .await?)
    }

    /// `CreateServerProfile`: writes a new server profile, `(ssq)`, the engine,
    /// the name and the port (`0` leaves the file no `Port` key). Its reply is
    /// the unit.
    async fn create_server_profile(
        &self,
        #[zbus(header)] header: zbus::message::Header<'_>,
        engine: String,
        profile: String,
        port: u16,
    ) -> Result<(), Error> {
        let who = requester_of(&header)?;
        Ok(self
            .core(move |m| m.create_server_profile(&who, &engine, &profile, port_of(port)))
            .await?)
    }

    /// `CreateServerProfile` for `principal`, the principal first as every `As`
    /// method has it (`sssq`).
    async fn create_server_profile_as(
        &self,
        #[zbus(header)] header: zbus::message::Header<'_>,
        principal: String,
        engine: String,
        profile: String,
        port: u16,
    ) -> Result<(), Error> {
        let who = requester_as(&header, principal)?;
        Ok(self
            .core(move |m| m.create_server_profile(&who, &engine, &profile, port_of(port)))
            .await?)
    }

    /// `DeleteServerProfile`: removes an unused server profile, `(ss)`, the
    /// engine and the name.
    async fn delete_server_profile(
        &self,
        #[zbus(header)] header: zbus::message::Header<'_>,
        engine: String,
        profile: String,
    ) -> Result<(), Error> {
        let who = requester_of(&header)?;
        Ok(self
            .core(move |m| m.delete_server_profile(&who, &engine, &profile))
            .await?)
    }

    /// `DeleteServerProfile` for `principal`, the principal first as every `As`
    /// method has it (`sss`).
    async fn delete_server_profile_as(
        &self,
        #[zbus(header)] header: zbus::message::Header<'_>,
        principal: String,
        engine: String,
        profile: String,
    ) -> Result<(), Error> {
        let who = requester_as(&header, principal)?;
        Ok(self
            .core(move |m| m.delete_server_profile(&who, &engine, &profile))
            .await?)
    }

    // ── The instance profile verbs (`ARCHITECTURE.md` "Instance profiles").

    /// `InstanceProfiles`: the node's instance profiles, `a(ssas)`: each one
    /// the id, the server profile its section names and the model profiles
    /// its entries give it.
    async fn instance_profiles(
        &self,
        #[zbus(header)] header: zbus::message::Header<'_>,
    ) -> Result<Vec<(String, String, Vec<String>)>, Error> {
        let who = requester_of(&header)?;
        Ok(self
            .core(move |m| {
                m.instance_profiles(&who).map(|rows| {
                    rows.into_iter()
                        .map(|p| (p.id, p.server_profile, p.entries))
                        .collect()
                })
            })
            .await?)
    }

    /// `InstanceProfiles` for `principal`, the principal first (`s`).
    async fn instance_profiles_as(
        &self,
        #[zbus(header)] header: zbus::message::Header<'_>,
        principal: String,
    ) -> Result<Vec<(String, String, Vec<String>)>, Error> {
        let who = requester_as(&header, principal)?;
        Ok(self
            .core(move |m| {
                m.instance_profiles(&who).map(|rows| {
                    rows.into_iter()
                        .map(|p| (p.id, p.server_profile, p.entries))
                        .collect()
                })
            })
            .await?)
    }

    /// `GetInstanceProfile`: one instance profile, `(s profile)`, answering
    /// `(s server_profile, as entries, s revision)`, the revision the text of
    /// `instance-profiles.ini` the read returned.
    async fn get_instance_profile(
        &self,
        #[zbus(header)] header: zbus::message::Header<'_>,
        profile: String,
    ) -> Result<(String, Vec<String>, String), Error> {
        let who = requester_of(&header)?;
        Ok(self
            .core(move |m| {
                m.instance_profile(&who, &profile)
                    .map(|p| (p.server_profile, p.entries, p.revision))
            })
            .await?)
    }

    /// `GetInstanceProfile` for `principal`, the principal first (`ss`).
    async fn get_instance_profile_as(
        &self,
        #[zbus(header)] header: zbus::message::Header<'_>,
        principal: String,
        profile: String,
    ) -> Result<(String, Vec<String>, String), Error> {
        let who = requester_as(&header, principal)?;
        Ok(self
            .core(move |m| {
                m.instance_profile(&who, &profile)
                    .map(|p| (p.server_profile, p.entries, p.revision))
            })
            .await?)
    }

    /// `CreateInstanceProfile`: writes a new instance profile, `(ss)`: its id
    /// and the server profile it runs.
    async fn create_instance_profile(
        &self,
        #[zbus(header)] header: zbus::message::Header<'_>,
        profile: String,
        server_profile: String,
    ) -> Result<(), Error> {
        let who = requester_of(&header)?;
        Ok(self
            .core(move |m| m.create_instance_profile(&who, &profile, &server_profile))
            .await?)
    }

    /// `CreateInstanceProfile` for `principal`, the principal first (`sss`).
    async fn create_instance_profile_as(
        &self,
        #[zbus(header)] header: zbus::message::Header<'_>,
        principal: String,
        profile: String,
        server_profile: String,
    ) -> Result<(), Error> {
        let who = requester_as(&header, principal)?;
        Ok(self
            .core(move |m| m.create_instance_profile(&who, &profile, &server_profile))
            .await?)
    }

    /// `DeleteInstanceProfile`: removes an instance profile no instance runs,
    /// `(s)`, its id.
    async fn delete_instance_profile(
        &self,
        #[zbus(header)] header: zbus::message::Header<'_>,
        profile: String,
    ) -> Result<(), Error> {
        let who = requester_of(&header)?;
        Ok(self
            .core(move |m| m.delete_instance_profile(&who, &profile))
            .await?)
    }

    /// `DeleteInstanceProfile` for `principal`, the principal first (`ss`).
    async fn delete_instance_profile_as(
        &self,
        #[zbus(header)] header: zbus::message::Header<'_>,
        principal: String,
        profile: String,
    ) -> Result<(), Error> {
        let who = requester_as(&header, principal)?;
        Ok(self
            .core(move |m| m.delete_instance_profile(&who, &profile))
            .await?)
    }

    /// `AddInstanceProfileModel`: gives an instance profile a model profile,
    /// `(ss)`: the profile and the model.
    async fn add_instance_profile_model(
        &self,
        #[zbus(header)] header: zbus::message::Header<'_>,
        profile: String,
        model: String,
    ) -> Result<AffectedAnswer, Error> {
        let who = requester_of(&header)?;
        Ok(affected_answer(
            self.core(move |m| {
                m.add_instance_profile_model(&who, &profile, &model)
                    .and_then(|_| m.affected())
            })
            .await?,
        ))
    }

    /// `AddInstanceProfileModel` for `principal`, the principal first (`sss`).
    async fn add_instance_profile_model_as(
        &self,
        #[zbus(header)] header: zbus::message::Header<'_>,
        principal: String,
        profile: String,
        model: String,
    ) -> Result<AffectedAnswer, Error> {
        let who = requester_as(&header, principal)?;
        Ok(affected_answer(
            self.core(move |m| {
                m.add_instance_profile_model(&who, &profile, &model)
                    .and_then(|_| m.affected())
            })
            .await?,
        ))
    }

    /// `RemoveInstanceProfileModel`: takes a model profile away from an
    /// instance profile, `(ss)`: the profile and the model.
    async fn remove_instance_profile_model(
        &self,
        #[zbus(header)] header: zbus::message::Header<'_>,
        profile: String,
        model: String,
    ) -> Result<AffectedAnswer, Error> {
        let who = requester_of(&header)?;
        Ok(affected_answer(
            self.core(move |m| {
                m.remove_instance_profile_model(&who, &profile, &model)
                    .and_then(|_| m.affected())
            })
            .await?,
        ))
    }

    /// `RemoveInstanceProfileModel` for `principal`, the principal first
    /// (`sss`).
    async fn remove_instance_profile_model_as(
        &self,
        #[zbus(header)] header: zbus::message::Header<'_>,
        principal: String,
        profile: String,
        model: String,
    ) -> Result<AffectedAnswer, Error> {
        let who = requester_as(&header, principal)?;
        Ok(affected_answer(
            self.core(move |m| {
                m.remove_instance_profile_model(&who, &profile, &model)
                    .and_then(|_| m.affected())
            })
            .await?,
        ))
    }

    /// `SetInstanceProfileModel`: the keys of one entry of an instance profile,
    /// `(ssa(ss)s)`: the profile, the model, the entry's keys as they were
    /// sent, and the revision, the text of `instance-profiles.ini` the caller
    /// read. The entry belongs to the profile (IP4): it lands in the render of
    /// every instance running the profile, so the save names no instance. The
    /// reply is the apply's rows ([`affected_answer`], IP3c): the running
    /// instances the save moved to a restart, and the instances that kept their
    /// last good pair.
    async fn set_instance_profile_model(
        &self,
        #[zbus(header)] header: zbus::message::Header<'_>,
        profile: String,
        model: String,
        values: Vec<(String, String)>,
        revision: String,
    ) -> Result<AffectedAnswer, Error> {
        let who = requester_of(&header)?;
        Ok(affected_answer(
            self.core(move |m| {
                m.set_instance_profile_model(&who, &profile, &model, &values, &revision)
            })
            .await?,
        ))
    }

    /// `SetInstanceProfileModel` for `principal`, the principal first
    /// (`sssa(ss)s`).
    async fn set_instance_profile_model_as(
        &self,
        #[zbus(header)] header: zbus::message::Header<'_>,
        principal: String,
        profile: String,
        model: String,
        values: Vec<(String, String)>,
        revision: String,
    ) -> Result<AffectedAnswer, Error> {
        let who = requester_as(&header, principal)?;
        Ok(affected_answer(
            self.core(move |m| {
                m.set_instance_profile_model(&who, &profile, &model, &values, &revision)
            })
            .await?,
        ))
    }

    /// `SetInstanceProfileServer`: points an instance profile at another server
    /// profile, `(ss)`: the profile and the server profile. The switch moves the
    /// ground of every entry of the profile, so it answers the apply's rows
    /// ([`affected_answer`], IP4a), the instances whose card the switch took
    /// away among them.
    async fn set_instance_profile_server(
        &self,
        #[zbus(header)] header: zbus::message::Header<'_>,
        profile: String,
        server_profile: String,
    ) -> Result<AffectedAnswer, Error> {
        let who = requester_of(&header)?;
        Ok(affected_answer(
            self.core(move |m| m.set_instance_profile_server(&who, &profile, &server_profile))
                .await?,
        ))
    }

    /// `SetInstanceProfileServer` for `principal`, the principal first
    /// (`sss`).
    async fn set_instance_profile_server_as(
        &self,
        #[zbus(header)] header: zbus::message::Header<'_>,
        principal: String,
        profile: String,
        server_profile: String,
    ) -> Result<AffectedAnswer, Error> {
        let who = requester_as(&header, principal)?;
        Ok(affected_answer(
            self.core(move |m| m.set_instance_profile_server(&who, &profile, &server_profile))
                .await?,
        ))
    }

    /// `SetInstanceProfile`: points an instance at an instance profile,
    /// `(sss)`: the instance, the profile and the revision, the text of
    /// `instance-profiles.ini` the caller read.
    async fn set_instance_profile(
        &self,
        #[zbus(header)] header: zbus::message::Header<'_>,
        instance: String,
        profile: String,
        revision: String,
    ) -> Result<AffectedAnswer, Error> {
        let who = requester_of(&header)?;
        Ok(affected_answer(
            self.core(move |m| m.set_instance_profile(&who, &instance, &profile, &revision))
                .await?,
        ))
    }

    /// `SetInstanceProfile` for `principal`, the principal first (`ssss`).
    async fn set_instance_profile_as(
        &self,
        #[zbus(header)] header: zbus::message::Header<'_>,
        principal: String,
        instance: String,
        profile: String,
        revision: String,
    ) -> Result<AffectedAnswer, Error> {
        let who = requester_as(&header, principal)?;
        Ok(affected_answer(
            self.core(move |m| m.set_instance_profile(&who, &instance, &profile, &revision))
                .await?,
        ))
    }

    /// `SetInstanceAddress`: the address an instance listens on, `(ssss)`: the
    /// instance, the host, the port as a text, and the revision, the text of
    /// `instances.ini` the caller read.
    async fn set_instance_address(
        &self,
        #[zbus(header)] header: zbus::message::Header<'_>,
        instance: String,
        host: String,
        port: String,
        revision: String,
    ) -> Result<AffectedAnswer, Error> {
        let who = requester_of(&header)?;
        Ok(affected_answer(
            self.core(move |m| m.set_instance_address(&who, &instance, &host, &port, &revision))
                .await?,
        ))
    }

    /// `SetInstanceAddress` for `principal`, the principal first (`sssss`).
    async fn set_instance_address_as(
        &self,
        #[zbus(header)] header: zbus::message::Header<'_>,
        principal: String,
        instance: String,
        host: String,
        port: String,
        revision: String,
    ) -> Result<AffectedAnswer, Error> {
        let who = requester_as(&header, principal)?;
        Ok(affected_answer(
            self.core(move |m| m.set_instance_address(&who, &instance, &host, &port, &revision))
                .await?,
        ))
    }

    /// `CreateInstance`: writes the record section, the render and the
    /// assignment of a new instance, `(sss)`: the engine, the id and the server
    /// profile it runs. The reply is empty, as every write's: the caller sent
    /// the id, and the create is answered before the record is written, so the
    /// id it would name is the one it has.
    async fn create_instance(
        &self,
        #[zbus(header)] header: zbus::message::Header<'_>,
        engine: String,
        id: String,
        server_profile: String,
    ) -> Result<(), Error> {
        let who = requester_of(&header)?;
        Ok(self
            .core(move |m| m.create_instance(&who, &engine, &id, &server_profile))
            .await?)
    }

    /// `CreateInstance` for `principal`, the principal first as every `As`
    /// method has it (`ssss`).
    async fn create_instance_as(
        &self,
        #[zbus(header)] header: zbus::message::Header<'_>,
        principal: String,
        engine: String,
        id: String,
        server_profile: String,
    ) -> Result<(), Error> {
        let who = requester_as(&header, principal)?;
        Ok(self
            .core(move |m| m.create_instance(&who, &engine, &id, &server_profile))
            .await?)
    }

    /// `RemoveInstance`: takes an instance away, `(s)`, its id. The id is the
    /// caller's own, so a create answers no id and a remove asks for none.
    async fn remove_instance(
        &self,
        #[zbus(header)] header: zbus::message::Header<'_>,
        id: String,
    ) -> Result<(), Error> {
        let who = requester_of(&header)?;
        Ok(self.core(move |m| m.remove_instance(&who, &id)).await?)
    }

    /// `RemoveInstance` for `principal`, the principal first (`ss`).
    async fn remove_instance_as(
        &self,
        #[zbus(header)] header: zbus::message::Header<'_>,
        principal: String,
        id: String,
    ) -> Result<(), Error> {
        let who = requester_as(&header, principal)?;
        Ok(self.core(move |m| m.remove_instance(&who, &id)).await?)
    }

    /// `CreateModelProfile`: writes a new model profile, `(ssa(ss))`, the id,
    /// the profile to copy (`""` for none) and the keys of the file pick. Its
    /// reply is the unit.
    async fn create_model_profile(
        &self,
        #[zbus(header)] header: zbus::message::Header<'_>,
        profile: String,
        from: String,
        values: Vec<(String, String)>,
    ) -> Result<(), Error> {
        let who = requester_of(&header)?;
        Ok(self
            .core(move |m| m.create_model_profile(&who, &profile, &from, &values))
            .await?)
    }

    /// `CreateModelProfile` for `principal`, the principal first as every `As`
    /// method has it (`sssa(ss)`).
    async fn create_model_profile_as(
        &self,
        #[zbus(header)] header: zbus::message::Header<'_>,
        principal: String,
        profile: String,
        from: String,
        values: Vec<(String, String)>,
    ) -> Result<(), Error> {
        let who = requester_as(&header, principal)?;
        Ok(self
            .core(move |m| m.create_model_profile(&who, &profile, &from, &values))
            .await?)
    }

    /// `DeleteModelProfile`: removes a switched-off, unassigned model profile,
    /// `(s)`, its id.
    async fn delete_model_profile(
        &self,
        #[zbus(header)] header: zbus::message::Header<'_>,
        profile: String,
    ) -> Result<AffectedAnswer, Error> {
        let who = requester_of(&header)?;
        Ok(affected_answer(
            self.core(move |m| m.delete_model_profile(&who, &profile))
                .await?,
        ))
    }

    /// `DeleteModelProfile` for `principal`, the principal first (`ss`).
    async fn delete_model_profile_as(
        &self,
        #[zbus(header)] header: zbus::message::Header<'_>,
        principal: String,
        profile: String,
    ) -> Result<AffectedAnswer, Error> {
        let who = requester_as(&header, principal)?;
        Ok(affected_answer(
            self.core(move |m| m.delete_model_profile(&who, &profile))
                .await?,
        ))
    }

    /// `ReadFile` for `principal`. The reply has the plain method's shape;
    /// a member of `pai` may configure, so a member's read is never withheld.
    async fn read_file_as(
        &self,
        #[zbus(header)] header: zbus::message::Header<'_>,
        principal: String,
        name: String,
    ) -> Result<(bool, String, bool), Error> {
        let who = requester_as(&header, principal)?;
        Ok(
            match self.core(move |m| m.read_file(Some(&who), &name)).await? {
                Some(file) => (true, file.text, file.withheld),
                None => (false, String::new(), false),
            },
        )
    }

    /// `WriteFile` for `principal`.
    async fn write_file_as(
        &self,
        #[zbus(header)] header: zbus::message::Header<'_>,
        principal: String,
        name: String,
        contents: String,
        keep_api_key: bool,
    ) -> Result<(), Error> {
        let who = requester_as(&header, principal)?;
        Ok(self
            .core(move |m| m.write_file(&who, &name, &contents, keep_api_key))
            .await?)
    }

    /// `WriteFileIf` for `principal`.
    async fn write_file_if_as(
        &self,
        #[zbus(header)] header: zbus::message::Header<'_>,
        principal: String,
        name: String,
        contents: String,
        expected: String,
    ) -> Result<AffectedAnswer, Error> {
        let who = requester_as(&header, principal)?;
        Ok(affected_answer(
            self.core(move |m| m.write_file_if(&who, &name, &contents, &expected))
                .await?,
        ))
    }

    /// The framework version this manager was built with, beside the protocol
    /// number of the `As` methods. It asks nothing of anybody: every caller
    /// gets it, a call with no sender included.
    async fn version(&self) -> (String, u32) {
        (env!("CARGO_PKG_VERSION").to_string(), PROTOCOL)
    }
}

/// [`ServiceControl`] over systemd's D-Bus API (`org.freedesktop.systemd1.Manager`
/// `StartUnit` / `StopUnit` / `RestartUnit`, mode "replace", on
/// `llama.cpp@<id>.service`; the state is the unit's `ActiveState`, a unit
/// not loaded reading as inactive; the start at boot is its unit file state
/// "enabled", changed by `EnableUnitFiles` / `DisableUnitFiles` then
/// `Reload`, as `systemctl enable/disable` does). The start, stop and state
/// are checked in a real run; the start at boot against a stand-in too.
/// Its connection is not the one the interface is served on: a blocking
/// call from a method handler would stall that connection's own executor.
pub struct SystemdService {
    conn: Connection,
}

impl SystemdService {
    pub fn new(conn: Connection) -> Self {
        Self { conn }
    }

    fn unit_job(&self, method: &str, id: &str) -> Result<(), ServiceError> {
        let unit = format!("llama.cpp@{id}.service");
        self.manager()?
            .call::<_, _, OwnedObjectPath>(method, &(unit.as_str(), "replace"))
            .map(|_job| ())
            .map_err(service_error)
    }

    fn manager(&self) -> Result<Proxy<'_>, ServiceError> {
        Proxy::new(
            &self.conn,
            "org.freedesktop.systemd1",
            "/org/freedesktop/systemd1",
            "org.freedesktop.systemd1.Manager",
        )
        .map_err(service_error)
    }
}

fn service_error(e: zbus::Error) -> ServiceError {
    ServiceError(e.to_string())
}

impl ServiceControl for SystemdService {
    fn start(&self, id: &str) -> Result<(), ServiceError> {
        self.unit_job("StartUnit", id)
    }

    fn stop(&self, id: &str) -> Result<(), ServiceError> {
        self.unit_job("StopUnit", id)
    }

    fn restart(&self, id: &str) -> Result<(), ServiceError> {
        self.unit_job("RestartUnit", id)
    }

    fn status(&self, id: &str) -> Result<UnitState, ServiceError> {
        let unit = format!("llama.cpp@{id}.service");
        let path: OwnedObjectPath = self
            .manager()?
            .call("LoadUnit", &(unit.as_str(),))
            .map_err(service_error)?;
        let state: String = Proxy::new(
            &self.conn,
            "org.freedesktop.systemd1",
            path,
            "org.freedesktop.systemd1.Unit",
        )
        .and_then(|unit| unit.get_property("ActiveState"))
        .map_err(service_error)?;
        Ok(match state.as_str() {
            "active" => UnitState::Active,
            "failed" => UnitState::Failed,
            "activating" => UnitState::Activating,
            "deactivating" => UnitState::Deactivating,
            _ => UnitState::Inactive,
        })
    }

    fn boot_enabled(&self, id: &str) -> Result<bool, ServiceError> {
        let unit = format!("llama.cpp@{id}.service");
        let state: String = self
            .manager()?
            .call("GetUnitFileState", &(unit.as_str(),))
            .map_err(service_error)?;
        Ok(state == "enabled")
    }

    fn set_boot_enabled(&self, id: &str, enabled: bool) -> Result<(), ServiceError> {
        let unit = format!("llama.cpp@{id}.service");
        let units: &[&str] = &[unit.as_str()];
        let manager = self.manager()?;
        if enabled {
            manager
                .call::<_, _, (bool, Vec<(String, String, String)>)>(
                    "EnableUnitFiles",
                    &(units, false, false),
                )
                .map(|_changes| ())
        } else {
            manager
                .call::<_, _, Vec<(String, String, String)>>("DisableUnitFiles", &(units, false))
                .map(|_changes| ())
        }
        .map_err(service_error)?;
        manager
            .call::<_, _, ()>("Reload", &())
            .map_err(service_error)
    }

    /// systemd's own three steps, in this order: the unit's enablement gone
    /// (`DisableUnitFiles`, the `wants/` links of the next boot included), the
    /// manager reloaded so the unit is dropped from its runtime set, and the
    /// unit's failed state reset. A unit systemd has never seen has nothing to
    /// reset: `ResetFailedUnit` answering an error is not the removal's failure,
    /// while a refusal of `DisableUnitFiles` is, and nothing after it runs. The
    /// unit file itself stays on disk: the package owns it, and a later instance
    /// of the same id starts from it.
    fn remove(&self, id: &str) -> Result<(), ServiceError> {
        let unit = format!("llama.cpp@{id}.service");
        let units: &[&str] = &[unit.as_str()];
        let manager = self.manager()?;
        manager
            .call::<_, _, Vec<(String, String, String)>>("DisableUnitFiles", &(units, false))
            .map_err(service_error)?;
        manager
            .call::<_, _, ()>("Reload", &())
            .map_err(service_error)?;
        let _ = manager.call::<_, _, ()>("ResetFailedUnit", &(unit.as_str(),));
        Ok(())
    }
}

/// [`Authority`] over polkit (`org.freedesktop.PolicyKit1.Authority.CheckAuthorization`,
/// subject "system-bus-name" with the caller's name, AllowUserInteraction or,
/// for the quiet question, no flags;
/// any failure answers not authorized). Checked in a real run, not by tests.
/// Like [`SystemdService`], on a connection other than the served one.
pub struct PolkitAuthority {
    conn: Connection,
}

impl PolkitAuthority {
    pub fn new(conn: Connection) -> Self {
        Self { conn }
    }
}

/// The `CheckAuthorizationFlags` of a polkit check: `AllowUserInteraction`
/// when the caller may be prompted, `None` otherwise.
pub fn check_flags(allow_interaction: bool) -> u32 {
    u32::from(allow_interaction)
}

impl PolkitAuthority {
    /// polkit's answer for `caller` and `action` with `flags`; a challenge
    /// (the caller would have to authenticate) and any failure are false.
    fn check(&self, caller: &Caller, action: Action, flags: u32) -> bool {
        use std::collections::HashMap;
        use zbus::zvariant::Value;
        let subject = (
            "system-bus-name",
            HashMap::from([("name", Value::from(caller.0.as_str()))]),
        );
        let details: HashMap<&str, &str> = HashMap::new();
        let result = Proxy::new(
            &self.conn,
            "org.freedesktop.PolicyKit1",
            "/org/freedesktop/PolicyKit1/Authority",
            "org.freedesktop.PolicyKit1.Authority",
        )
        .and_then(|authority| {
            authority.call::<_, _, (bool, bool, HashMap<String, String>)>(
                "CheckAuthorization",
                &(subject, action.id(), details, flags, ""),
            )
        });
        matches!(result, Ok((true, _, _)))
    }
}

impl Authority for PolkitAuthority {
    fn is_authorized_quietly(&self, caller: &Caller, action: Action) -> bool {
        self.check(caller, action, check_flags(false))
    }

    fn is_authorized(&self, caller: &Caller, action: Action) -> bool {
        self.check(caller, action, check_flags(true))
    }

    /// The caller's uid from the bus (`GetConnectionUnixUser`) against the
    /// uid of the user `private-ai-bff`, both looked up at each call.
    fn is_bff(&self, caller: &Caller) -> bool {
        let caller_uid = zbus::blocking::fdo::DBusProxy::new(&self.conn)
            .ok()
            .and_then(|bus| {
                let name = zbus::names::BusName::try_from(caller.0.as_str()).ok()?;
                bus.get_connection_unix_user(name).ok()
            });
        is_bff_uid(caller_uid, super::accounts::uid_of(BFF_USER))
    }
}

/// The bff's system user (its sysusers.d entry).
const BFF_USER: &str = "private-ai-bff";

/// Whether a caller whose uid is `caller_uid` (the bus's
/// `GetConnectionUnixUser`) is the bff, whose account `private-ai-bff` has
/// `bff_uid`; either unknown (a failed lookup, no such user) is false.
pub fn is_bff_uid(caller_uid: Option<u32>, bff_uid: Option<u32>) -> bool {
    caller_uid.is_some() && caller_uid == bff_uid
}

/// The system-bus service: [`ManagerInterface`] over systemd and polkit
/// served at [`OBJECT_PATH`] under [`BUS_NAME`]; it starts no instance:
/// systemd starts the enabled ones at boot. The connection to keep alive
/// while serving.
pub fn serve() -> zbus::Result<Connection> {
    let client = Connection::system()?;
    let core = Arc::new(
        Manager::new(
            SystemdService::new(client.clone()),
            PolkitAuthority::new(client),
            SystemResolver,
        )
        // Every record goes on stderr, which the unit sends to the journal.
        .with_log(WriterLog::new(std::sync::Arc::new(std::sync::Mutex::new(
            std::io::stderr(),
        ))))
        // The memory check measures against the node's own cards.
        .with_gpus(ProbedGpus::for_node())
        // The node-info read answers the host: os-release, the kernel, the PCI
        // tree, and the cached Vulkan child.
        .with_node_facts(HostNodeFacts::for_node()),
    );
    // An install of the old model files is migrated before anything reads them,
    // and the instances read `models@<id>.ini`, so the pool on disk is rendered
    // before anyone can ask for it. A render that fails leaves the files as
    // they are, names its instance in the log, and the next write of a pool
    // file renders again.
    let failures = core.start_up();
    core.report_render_failures(&failures);
    zbus::blocking::connection::Builder::system()?
        .serve_at(OBJECT_PATH, ManagerInterface::new(core))?
        .name(BUS_NAME)?
        .build()
}

#[cfg(test)]
#[path = "dbus_tests.rs"]
mod tests;
// The items the moved file declares at its top level: every
// caller names them here, as it always did.
#[cfg(test)]
pub(crate) use tests::*;
