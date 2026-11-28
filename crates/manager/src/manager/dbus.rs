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
    /// `(sss a(ss) s)`: the profile, the model, the instance whose page the
    /// entry is written from, the entry's keys as they were sent, and the
    /// revision, the text of `instance-profiles.ini` the caller read. The entry
    /// lands in the render of every instance running the profile; the instance
    /// the page names is checked to be one the record has a section for, the
    /// same answer the record's own verbs give for a name it does not list
    /// ([`ManagerError::NoInstance`]). The reply is the apply's rows
    /// ([`affected_answer`], IP3c): the running instances the save moved to a
    /// restart, and the instances that kept their last good pair.
    async fn set_instance_profile_model(
        &self,
        #[zbus(header)] header: zbus::message::Header<'_>,
        profile: String,
        model: String,
        instance: String,
        values: Vec<(String, String)>,
        revision: String,
    ) -> Result<AffectedAnswer, Error> {
        let who = requester_of(&header)?;
        Ok(affected_answer(
            self.core(move |m| {
                m.set_instance_profile_model(&who, &profile, &model, &instance, &values, &revision)
            })
            .await?,
        ))
    }

    /// `SetInstanceProfileModel` for `principal`, the principal first
    /// (`ssssa(ss)s`).
    #[allow(clippy::too_many_arguments)]
    async fn set_instance_profile_model_as(
        &self,
        #[zbus(header)] header: zbus::message::Header<'_>,
        principal: String,
        profile: String,
        model: String,
        instance: String,
        values: Vec<(String, String)>,
        revision: String,
    ) -> Result<AffectedAnswer, Error> {
        let who = requester_as(&header, principal)?;
        Ok(affected_answer(
            self.core(move |m| {
                m.set_instance_profile_model(&who, &profile, &model, &instance, &values, &revision)
            })
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
    ) -> Result<(), Error> {
        let who = requester_of(&header)?;
        Ok(self
            .core(move |m| m.delete_model_profile(&who, &profile))
            .await?)
    }

    /// `DeleteModelProfile` for `principal`, the principal first (`ss`).
    async fn delete_model_profile_as(
        &self,
        #[zbus(header)] header: zbus::message::Header<'_>,
        principal: String,
        profile: String,
    ) -> Result<(), Error> {
        let who = requester_as(&header, principal)?;
        Ok(self
            .core(move |m| m.delete_model_profile(&who, &profile))
            .await?)
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

/// E2E phase for the D-Bus interface, for this crate's e2e test to call: it sets
/// the process-wide `PRIVATE_AI_DATA_ROOT` redirect (`src/tests/mod.rs`).
/// The interface is served over a peer-to-peer connection on a Unix socket
/// pair inside the test, never a bus; the seams are the core's fakes. A
/// client given a unique name stamps it as the sender of its calls, as the
/// bus daemon would; one without has none.
#[cfg(test)]
pub(crate) fn manager_dbus_e2e() {
    use super::service::fakes::{Call, FakeAuthority, FakeResolver, FakeService};
    use super::service::real_model_files;
    use super::service::Action;
    use std::os::unix::net::UnixStream;
    use std::time::Duration;
    use zbus::blocking::connection::Builder;
    use zbus::blocking::Connection;

    type Core = Manager<FakeService, FakeAuthority, FakeResolver>;

    let dir = tempfile::tempdir().expect("tempdir");
    crate::paths::redirect_for_tests(dir.path());
    let sys = crate::paths::engine_config_dir();
    std::fs::create_dir_all(&sys).expect("system config dir");
    let default_ini = "[Server]\nPort = 8080\nHostname = 0.0.0.0\n";
    let broken_ini = "[Server]\nPort = 8090\nHostname = 127.0.0.1\n";
    let model_profiles = real_model_files(
        dir.path(),
        "[Qwen3.8-27B]\nmodel = /models/Qwen3.8-27B-Q8_0.gguf\n",
    );
    // An instance is a section of the record, not a file: the fixture hands
    // the manager the `instances.ini` it reads. lan-2 starts as a section with
    // no `server@lan-2.ini`, the instance the accepted write below gives its
    // file.
    let record = "[default]\nServerProfile = default\n\n[broken]\nServerProfile = broken\n\n[lan-2]\nServerProfile = lan-2\n";
    std::fs::write(sys.join("server@default.ini"), default_ini).expect("write default");
    std::fs::write(sys.join("server@broken.ini"), broken_ini).expect("write broken");
    std::fs::write(sys.join("models.ini"), &model_profiles).expect("write presets");
    std::fs::write(sys.join("instances.ini"), record).expect("write instances");
    // A start loads the instance's render and refuses one with nothing to run;
    // these pins are about the transport, so each instance gets a render.
    for id in ["default", "broken", "lan-2"] {
        std::fs::write(
            sys.join(format!("models@{id}.ini")),
            "[seed]\nmodel = /models/seed.gguf\n",
        )
        .expect("render");
    }

    // Serve `core` on one end of a socket pair, the server built on its own
    // thread (both ends handshake at once); the client on the other end,
    // stamping `sender` when given. Every call has a deadline: a stub that
    // panics in the server must fail the test, not hang it.
    let connect = |core: &Arc<Core>, sender: Option<&str>| -> (Connection, Connection) {
        let (s0, s1) = UnixStream::pair().expect("socket pair");
        let iface = ManagerInterface::new(core.clone());
        let server = std::thread::spawn(move || {
            Builder::async_io_unix_stream(s0)
                .server(zbus::Guid::generate())
                .expect("server guid")
                .p2p()
                .serve_at(OBJECT_PATH, iface)
                .expect("serve_at")
                .build()
                .expect("server connection")
        });
        let client = Builder::async_io_unix_stream(s1)
            .p2p()
            .method_timeout(Duration::from_secs(10))
            .build()
            .expect("client connection");
        // The bus would stamp the sender; on a p2p client it is set by hand.
        if let Some(name) = sender {
            client.inner().set_unique_name(name).expect("unique name");
        }
        (server.join().expect("server thread"), client)
    };
    let core_in = |state: UnitState, authority: bool, failing: &[&str]| -> Arc<Core> {
        let mut service = FakeService::new();
        service.state = state;
        service.failing = failing.iter().map(|s| s.to_string()).collect();
        Arc::new(
            Manager::new(service, FakeAuthority::new(authority), FakeResolver::lan())
                .with_group(crate::manager::service::fakes::FakeGroup::new()),
        )
    };
    let core = |authority: bool, failing: &[&str]| core_in(UnitState::Active, authority, failing);
    // A method call: Ok(reply), or Err((error name, message)).
    fn call<B>(conn: &Connection, method: &str, body: &B) -> Result<zbus::Message, (String, String)>
    where
        B: serde::ser::Serialize + zbus::zvariant::DynamicType,
    {
        conn.call_method(None::<&str>, OBJECT_PATH, Some(INTERFACE), method, body)
            .map_err(|e| match e {
                zbus::Error::MethodError(name, detail, _) => {
                    (name.to_string(), detail.unwrap_or_default())
                }
                zbus::Error::FDO(e) => {
                    use zbus::DBusError;
                    (
                        e.name().to_string(),
                        e.description().unwrap_or_default().to_string(),
                    )
                }
                other => panic!("{method}: not a method error reply: {other:?}"),
            })
    }
    fn body<T: for<'de> zbus::zvariant::DynamicDeserialize<'de>>(m: &zbus::Message) -> T {
        m.body().deserialize().expect("reply body")
    }
    /// What [`answer_on_bus`] answers: the ids a save moved to a restart, and
    /// its not-converged rows as `(instance, key, args)`.
    type BusAnswer = (Vec<String>, Vec<(String, String, Vec<String>)>);
    /// The reply of a save that re-renders, read as the design's answer object
    /// `a{sv}` (IP3c, `ARCHITECTURE.md` "Convergence", `affected_answer` in this
    /// file): `running`, the `as` of ids the save moved to a restart, and
    /// `notConverged`, the `av` of `a{sv}` rows naming the `instance`, the
    /// `key` and the check's `args` (an `as`). The bus has no JSON, so the
    /// fields are decoded and compared as the texts they are.
    fn answer_on_bus(m: &zbus::Message, method: &str) -> BusAnswer {
        use zbus::zvariant::{Dict, OwnedValue, Value};
        assert_eq!(
            m.body().signature().to_string(),
            "a{sv}".to_string(),
            "IP3c: {method} answers the answer object, not the unit"
        );
        let answer: std::collections::BTreeMap<String, OwnedValue> = m
            .body()
            .deserialize()
            .expect("IP3c: the reply body of a re-rendering save is `a{sv}`");
        let field = |key: &str| {
            Value::from(
                answer
                    .get(key)
                    .unwrap_or_else(|| panic!("IP3c: {method} answers `{key}`: {answer:?}"))
                    .try_clone()
                    .expect("IP3c: the answer's value clones"),
            )
        };
        let running = field("running")
            .downcast::<Vec<String>>()
            .expect("IP3c: `running` is the array of ids (`as`)");
        let rows = field("notConverged")
            .downcast::<zbus::zvariant::Array>()
            .expect("IP3c: `notConverged` is an array of rows (`av`)");
        let not_converged = rows
            .inner()
            .iter()
            .map(|row| {
                let fields = row
                    .try_clone()
                    .expect("IP3c: the row's value clones")
                    .downcast::<Dict>()
                    .expect("IP3c: a not-converged row is a dict (`a{sv}`)");
                let text = |key: &str| {
                    let needle = Value::new(key);
                    fields
                        .iter()
                        .find(|(k, _)| *k == &needle)
                        .and_then(|(_, v)| v.try_clone().ok())
                        // The field is a variant of `a{sv}`: unwrapped, it is the text.
                        .and_then(|v| v.downcast::<String>().ok())
                        .unwrap_or_default()
                };
                let needle = Value::new("args");
                let args = fields
                    .iter()
                    .find(|(k, _)| *k == &needle)
                    .and_then(|(_, v)| v.try_clone().ok())
                    .and_then(|v| v.downcast::<Vec<String>>().ok())
                    .unwrap_or_default();
                (text("instance"), text("key"), args)
            })
            .collect();
        (running, not_converged)
    }
    let err_name = |kind: &str| format!("org.privateai.Manager1.Error.{kind}");
    let calls = |c: &Arc<Core>| c.service().calls.lock().unwrap().clone();
    let asked = |c: &Arc<Core>| c.authority().asked.lock().unwrap().clone();
    let alice = Caller(":1.42".into());

    // ── No sender: every change refused before polkit is even asked ──────
    {
        let c = core(true, &[]);
        let (_server, client) = connect(&c, None);
        for (method, id) in [
            ("Start", "default"),
            ("Stop", "default"),
            ("Restart", "default"),
        ] {
            let e = call(&client, method, &(id,)).expect_err(method);
            assert_eq!(e.0, err_name("NotAuthorized"), "{method} without a sender");
        }
        let e = call(
            &client,
            "WriteFile",
            &("models.ini", "[x]\nmodel = /m/x.gguf\n", false),
        )
        .expect_err("WriteFile");
        assert_eq!(e.0, err_name("NotAuthorized"), "WriteFile without a sender");
        assert_eq!(
            std::fs::read_to_string(sys.join("models.ini")).unwrap(),
            model_profiles,
            "a refused write leaves the file"
        );
        let e = call(
            &client,
            "SetModelProfile",
            &("default", "Qwen3.8-27B", "on"),
        )
        .expect_err("SetModelProfile");
        assert_eq!(
            e.0,
            err_name("NotAuthorized"),
            "SetModelProfile without a sender"
        );
        let e = call(&client, "SetServerProfile", &("default", "default"))
            .expect_err("SetServerProfile");
        assert_eq!(
            e.0,
            err_name("NotAuthorized"),
            "SetServerProfile without a sender"
        );
        assert!(
            !sys.join("assignments.ini").exists(),
            "an assignment refused with no identity writes nothing"
        );
        assert_eq!(calls(&c), vec![], "nothing reached the service manager");
        assert_eq!(asked(&c), vec![], "a call with no sender asks nobody");

        // Reads need no caller.
        let r = call(&client, "ListInstances", &()).expect("ListInstances");
        assert_eq!(
            body::<Vec<String>>(&r),
            vec!["broken", "default", "lan-2"],
            "the sections of instances.ini are the instances: lan-2 has no profile file of its own"
        );
        let r = call(&client, "Status", &("default",)).expect("Status");
        assert_eq!(body::<String>(&r), "active");
        let r = call(&client, "ReadFile", &("models.ini",)).expect("ReadFile");
        assert_eq!(
            body::<(bool, String, bool)>(&r),
            (true, model_profiles.to_string(), false)
        );
        assert_eq!(calls(&c), vec![Call::Status("default".into())]);

        // Start at boot: the read needs no caller, the change one.
        let e = call(&client, "SetBootEnabled", &("default", true)).expect_err("SetBootEnabled");
        assert_eq!(
            e.0,
            err_name("NotAuthorized"),
            "SetBootEnabled without a sender"
        );
        let r = call(&client, "BootEnabled", &("default",)).expect("BootEnabled");
        assert!(!body::<bool>(&r), "a new instance starts disabled");
        assert_eq!(
            calls(&c),
            vec![
                Call::Status("default".into()),
                Call::BootEnabled("default".into())
            ]
        );
        assert_eq!(asked(&c), vec![], "a call with no sender asks nobody");
    }

    // SSP-D-PRE-1 BEGIN
    // ── A caller polkit refuses: asked with its bus name, nothing done ───
    {
        let c = core(false, &[]);
        let record = std::fs::read_to_string(sys.join("instances.ini")).expect("record");
        let (_server, client) = connect(&c, Some(":1.42"));
        let e = call(&client, "Start", &("default",)).expect_err("Start");
        assert_eq!(e.0, err_name("NotAuthorized"));
        let e = call(&client, "WriteFile", &("models.ini", "", false)).expect_err("WriteFile");
        assert_eq!(e.0, err_name("NotAuthorized"));
        let e = call(&client, "SetBootEnabled", &("default", true)).expect_err("SetBootEnabled");
        assert_eq!(e.0, err_name("NotAuthorized"));
        let e = call(
            &client,
            "SetModelProfile",
            &("default", "Qwen3.8-27B", "on"),
        )
        .expect_err("SetModelProfile");
        assert_eq!(e.0, err_name("NotAuthorized"));
        let e = call(&client, "SetServerProfile", &("default", "default"))
            .expect_err("SetServerProfile");
        assert_eq!(e.0, err_name("NotAuthorized"));
        assert_eq!(calls(&c), vec![], "a refused caller starts nothing");
        assert_eq!(
            asked(&c),
            vec![
                (alice.clone(), Action::Control),
                (alice.clone(), Action::Configure),
                (alice.clone(), Action::Configure),
                (alice.clone(), Action::Configure),
                (alice.clone(), Action::Configure)
            ],
            "polkit is asked about the sender, per action"
        );
        assert!(
            !sys.join("assignments.ini").exists(),
            "a refused assignment writes nothing"
        );
        assert_eq!(
            std::fs::read_to_string(sys.join("models.ini")).unwrap(),
            model_profiles
        );
        assert_eq!(
            std::fs::read_to_string(sys.join("instances.ini")).unwrap(),
            record,
            "a refused caller writes no instance record"
        );
    }
    // SSP-D-PRE-1 END

    // ── An authorized caller: each method reaches the core ───────────────
    {
        let c = core(true, &["broken"]);
        let (_server, client) = connect(&c, Some(":1.42"));

        for method in ["Start", "Stop", "Restart"] {
            call(&client, method, &("default",)).unwrap_or_else(|e| panic!("{method}: {e:?}"));
        }
        assert_eq!(
            calls(&c),
            vec![
                Call::Start("default".into()),
                Call::Stop("default".into()),
                Call::Restart("default".into())
            ]
        );
        assert_eq!(asked(&c), vec![(alice.clone(), Action::Control); 3]);

        // Start at boot, changed and read back.
        call(&client, "SetBootEnabled", &("default", true)).expect("SetBootEnabled on");
        let r = call(&client, "BootEnabled", &("default",)).expect("BootEnabled");
        assert!(body::<bool>(&r), "enabled");
        call(&client, "SetBootEnabled", &("default", false)).expect("SetBootEnabled off");
        let r = call(&client, "BootEnabled", &("default",)).expect("BootEnabled");
        assert!(!body::<bool>(&r), "disabled again");
        assert_eq!(
            calls(&c)[3..],
            [
                Call::SetBootEnabled("default".into(), true),
                Call::BootEnabled("default".into()),
                Call::SetBootEnabled("default".into(), false),
                Call::BootEnabled("default".into()),
            ]
        );
        assert_eq!(
            asked(&c)[3..],
            [
                (alice.clone(), Action::Configure),
                (alice.clone(), Action::Configure)
            ],
            "only the changes ask, for configure"
        );
        for method in ["BootEnabled", "SetBootEnabled"] {
            // A valid id no section of `instances.ini` names: an instance is a
            // section of the record, so `nope` is no instance (lan-2 IS one,
            // the record has its section).
            let e = if method == "BootEnabled" {
                call(&client, method, &("nope",))
            } else {
                call(&client, method, &("nope", true))
            }
            .expect_err(method);
            assert_eq!(
                e.0,
                err_name("NoInstance"),
                "{method} of an id the record has no section for"
            );
            let e = if method == "BootEnabled" {
                call(&client, method, &("../x",))
            } else {
                call(&client, method, &("../x", true))
            }
            .expect_err(method);
            assert_eq!(e.0, err_name("InvalidId"), "{method} of an invalid id");
            let e = if method == "BootEnabled" {
                call(&client, method, &("broken",))
            } else {
                call(&client, method, &("broken", true))
            }
            .expect_err(method);
            assert_eq!(e.0, err_name("Service"), "{method} of a failing unit");
            assert!(e.1.contains("llama.cpp@broken.service failed"), "{e:?}");
        }

        // Refusals of the core, each with its own error name.
        for method in ["Status", "Start", "Stop", "Restart"] {
            for bad in ["", "../x", "Default", "a_b"] {
                let e = call(&client, method, &(bad,)).expect_err(method);
                assert_eq!(e.0, err_name("InvalidId"), "{method} {bad:?}");
            }
        }
        for method in ["Start", "Restart"] {
            let e = call(&client, method, &("nope",)).expect_err(method);
            assert_eq!(
                e.0,
                err_name("NoInstance"),
                "{method} of an id the record has no section for"
            );
        }
        let e = call(&client, "Start", &("broken",)).expect_err("Start broken");
        assert_eq!(e.0, err_name("Service"));
        assert!(
            e.1.contains("llama.cpp@broken.service failed"),
            "the service manager's own message: {e:?}"
        );

        // Files.
        for bad in [
            "../x",
            "server.ini",
            "settings.ini",
            "/etc/passwd",
            "server@Bad.ini",
        ] {
            let e = call(&client, "ReadFile", &(bad,)).expect_err("ReadFile");
            assert_eq!(e.0, err_name("BadFileName"), "ReadFile {bad:?}");
            let e = call(&client, "WriteFile", &(bad, "x", false)).expect_err("WriteFile");
            assert_eq!(e.0, err_name("BadFileName"), "WriteFile {bad:?}");
            let e = call(&client, "WriteFileIf", &(bad, "x", "")).expect_err("WriteFileIf");
            assert_eq!(e.0, err_name("BadFileName"), "WriteFileIf {bad:?}");
        }
        assert!(!dir.path().join("x").exists() && !sys.join("server.ini").exists());
        let r = call(&client, "ReadFile", &("models.ini.disabled",)).expect("ReadFile");
        assert_eq!(
            body::<(bool, String, bool)>(&r),
            (false, String::new(), false),
            "a missing file"
        );

        let e = call(
            &client,
            "WriteFile",
            &(
                "server@lan-2.ini",
                "[Server]\nPort = 8080\nHostname = 127.0.0.1\n",
                false,
            ),
        )
        .expect_err("colliding WriteFile");
        assert_eq!(e.0, err_name("AddressInUse"));
        assert!(e.1.contains("default"), "names the other instance: {e:?}");
        let e = call(
            &client,
            "WriteFile",
            &(
                "server@lan-2.ini",
                "[Server]\nPort = 8081\nHostname = nowhere.invalid\n",
                false,
            ),
        )
        .expect_err("unresolvable WriteFile");
        assert_eq!(e.0, err_name("UnresolvableHost"));
        assert!(e.1.contains("nowhere.invalid"), "names the host: {e:?}");
        assert!(
            !sys.join("server@lan-2.ini").exists(),
            "refused writes write nothing"
        );

        let lan2 = "[Server]\nPort = 8081\nHostname = gpu-box.lan\n";
        call(&client, "WriteFile", &("server@lan-2.ini", lan2, false)).expect("WriteFile lan-2");
        assert_eq!(
            std::fs::read_to_string(sys.join("server@lan-2.ini")).unwrap(),
            lan2
        );
        let r = call(&client, "ListInstances", &()).expect("ListInstances");
        assert_eq!(body::<Vec<String>>(&r), vec!["broken", "default", "lan-2"]);
        let r = call(&client, "ReadFile", &("server@lan-2.ini",)).expect("ReadFile");
        assert_eq!(
            body::<(bool, String, bool)>(&r),
            (true, lan2.to_string(), false)
        );

        // A read that fails for another reason than a missing file.
        std::fs::create_dir(sys.join("models.ini.disabled")).expect("dir in the way");
        let e = call(&client, "ReadFile", &("models.ini.disabled",)).expect_err("ReadFile dir");
        assert_eq!(e.0, err_name("Io"));
        std::fs::remove_dir(sys.join("models.ini.disabled")).unwrap();

        // SetModelProfile: the assignment verb, request `(sss)`, the reply the
        // answer object `a{sv}` (IP3c). The accepted call writes the record and
        // renders the instance's model profiles; refusals are the core's own
        // errors, and write nothing.
        std::fs::write(sys.join("assignments.ini"), "[default]\n").expect("assignments");
        // Each instance's launch recorded the pair standing on disk now (IP3c):
        // the drift the answer reports is a pair the save moves off that record,
        // so the answer's `running` is about a real launch, and the units are
        // asked only past a pair that moved.
        for id in ["default", "broken", "lan-2"] {
            super::service::write_applied(id).expect("the applied record");
        }
        let asked_before = asked(&c).len();
        let calls_before = calls(&c).len();
        let r = call(
            &client,
            "SetModelProfile",
            &("default", "Qwen3.8-27B", "on"),
        )
        .expect("SetModelProfile on");
        let (running, not_converged) = answer_on_bus(&r, "SetModelProfile");
        assert_eq!(
            running,
            vec!["default".to_string()],
            "IP3c: each instance launched on the pair standing on disk, which is the pair the \
             render gives it, so a save that assigns a model profile to `default` alone moves \
             one pair: the answer offers a restart to the running instance whose pair moved, \
             and asks nothing of the two whose pair the save leaves as their launch recorded: \
             {running:?}"
        );
        assert_eq!(
            not_converged,
            vec![],
            "IP3c: no instance kept a pair a fit check refused, so the answer carries no row: \
             {not_converged:?}"
        );
        assert_eq!(
            std::fs::read_to_string(sys.join("assignments.ini")).unwrap(),
            "[default]\nQwen3.8-27B = on\n",
            "the assignment landed"
        );
        assert!(
            std::fs::read_to_string(sys.join("models@default.ini"))
                .expect("the render")
                .contains("[Qwen3.8-27B]"),
            "the render carries the enabled model profile"
        );
        call(
            &client,
            "SetModelProfile",
            &("default", "Qwen3.8-27B", "off"),
        )
        .expect("SetModelProfile off");
        assert_eq!(
            std::fs::read_to_string(sys.join("assignments.ini")).unwrap(),
            "[default]\nQwen3.8-27B = off\n",
            "the same profile switched off"
        );
        assert!(
            !std::fs::read_to_string(sys.join("models@default.ini"))
                .expect("the render")
                .contains("[Qwen3.8-27B]"),
            "the render drops the disabled model profile"
        );
        assert_eq!(
            asked(&c).len(),
            asked_before + 2,
            "the guarded call a write is: configure asked of the sender, every call"
        );
        assert_eq!(
            calls(&c)[calls_before..],
            [Call::Status("default".into())],
            "IP3c: the answer's drift compare asks the state of the one instance whose pair \
             the save moves, in record order, and only for the call that moved it: the \
             assignment ON renders `default` off the pair its launch recorded, and the \
             assignment OFF puts that very pair back, so the second call sees no drift and \
             asks nothing; the other two instances' pairs stand on their record throughout; \
             no unit verb"
        );
        let e = call(&client, "SetModelProfile", &("lan-3", "Qwen3.8-27B", "on"))
            .expect_err("an assignment of no instance");
        assert_eq!(
            e.0,
            err_name("NoInstance"),
            "an id the record has no section for"
        );
        for bad in ["", "../x", "Default", "a_b"] {
            let e = call(&client, "SetModelProfile", &(bad, "Qwen3.8-27B", "on"))
                .expect_err("an invalid id");
            assert_eq!(e.0, err_name("InvalidId"), "id {bad:?}");
        }
        let e = call(&client, "SetModelProfile", &("default", "zzz", "on"))
            .expect_err("a model profile the pool has not");
        assert_eq!(
            e.0,
            err_name("Invalid"),
            "the pool has no model profile zzz"
        );
        for bad in ["maybe", "", "ON", "on,off"] {
            let e = call(&client, "SetModelProfile", &("default", "Qwen3.8-27B", bad))
                .expect_err("a value that is not one of the four");
            assert_eq!(e.0, err_name("Invalid"), "value {bad:?}");
        }
        assert_eq!(
            std::fs::read_to_string(sys.join("assignments.ini")).unwrap(),
            "[default]\nQwen3.8-27B = off\n",
            "refused calls write nothing"
        );
        // The signature is `(sss)`: no less, no more, and three strings.
        // zbus answers a shape mismatch with its own error, never one of
        // the manager's, and the file is untouched.
        for e in [
            call(&client, "SetModelProfile", &("default", "Qwen3.8-27B")).expect_err("two strings"),
            call(
                &client,
                "SetModelProfile",
                &("default", "Qwen3.8-27B", "on", "more"),
            )
            .expect_err("four strings"),
            call(&client, "SetModelProfile", &("default", 7u32, "on"))
                .expect_err("a number where a string belongs"),
        ] {
            assert!(
                !e.0.starts_with("org.privateai.Manager1."),
                "not the manager's error: {e:?}"
            );
        }
        assert_eq!(
            std::fs::read_to_string(sys.join("assignments.ini")).unwrap(),
            "[default]\nQwen3.8-27B = off\n",
            "a body the method does not take writes nothing"
        );

        // ── SetServerProfile: the server profile of an instance, `(ss)` ───
        // The sibling of `SetModelProfile`: the same verb shape, the same
        // guard rails, and the instance's own profile legal (switching back).
        // `alt` is a spare profile file, on the port `lan-2` already runs,
        // that the record names for nobody: a switch to it lands and shows in
        // the record's bytes.
        let profile_of = |id: &str| -> Option<String> {
            std::fs::read_to_string(sys.join("instances.ini"))
                .ok()
                .and_then(|t| {
                    t.split("\n\n")
                        .find(|s| s.starts_with(&format!("[{id}]\n")))
                        .and_then(|s| s.lines().find(|l| l.starts_with("ServerProfile =")))
                        .map(|l| l["ServerProfile =".len()..].trim().to_string())
                })
        };
        let fixture = std::fs::read_to_string(sys.join("instances.ini")).expect("record");
        let asked_before = asked(&c).len();
        let calls_before = calls(&c).len();
        std::fs::write(
            sys.join("server@alt.ini"),
            "[Server]\nPort = 8081\nHostname = gpu-box.lan\n",
        )
        .expect("write alt");
        let r = call(&client, "SetServerProfile", &("lan-2", "alt"))
            .expect("SetServerProfile of lan-2");
        let (running, not_converged) = answer_on_bus(&r, "SetServerProfile");
        assert_eq!(
            running,
            Vec::<String>::new(),
            "IP3c: `alt` carries the address lan-2's launch already carries, so the switch \
             writes no byte of lan-2's pair and moves no other instance's pair: the answer \
             offers a restart to nobody, because nothing stands off the pair its launch \
             recorded: {running:?}"
        );
        assert_eq!(
            not_converged,
            vec![],
            "IP3c: no instance kept a pair a fit check refused, so the answer carries no row: \
             {not_converged:?}"
        );
        assert_eq!(
            profile_of("lan-2").as_deref(),
            Some("alt"),
            "a server profile nobody runs lands"
        );
        assert_eq!(
            asked(&c).len(),
            asked_before + 1,
            "the guarded call a write is: configure asked of the sender, every call"
        );
        assert_eq!(
            calls(&c)[calls_before..],
            Vec::<Call>::new(),
            "IP3c: the accepted switch's drift compare asks NOTHING of the service manager: \
             the pair it renders is the pair `lan-2` launched on (the spare profile carries \
             its address) and the other two pairs stand on their records; the refusals ask \
             nothing, and no unit verb"
        );
        let landed = std::fs::read_to_string(sys.join("instances.ini")).unwrap();
        assert!(
            landed.contains("[lan-2]\nServerProfile = alt\n"),
            "the section the verb wrote: {landed:?}"
        );
        assert!(
            landed.starts_with("[default]\nServerProfile = default\n\n[broken]\n"),
            "the sections it did not touch keep their bytes: {landed:?}"
        );
        assert!(
            landed.ends_with("[lan-2]\nServerProfile = alt\n"),
            "the record keeps its one last section: {landed:?}"
        );
        assert_eq!(
            landed.matches("[lan-2]").count(),
            1,
            "one section per instance, no copy: {landed:?}"
        );
        let e = call(&client, "SetServerProfile", &("lan-3", "alt"))
            .expect_err("a server profile of no instance");
        assert_eq!(e.0, err_name("NoInstance"), "lan-3 has no section");
        for bad in ["", "../x", "Default", "a_b"] {
            let e = call(&client, "SetServerProfile", &(bad, "alt")).expect_err("an invalid id");
            assert_eq!(e.0, err_name("InvalidId"), "id {bad:?}");
        }
        let e = call(&client, "SetServerProfile", &("lan-2", "nope"))
            .expect_err("a server profile the pool has not");
        assert_eq!(e.0, err_name("NoServerProfile"), "no server profile nope");
        assert_eq!(e.1, "nope", "the message is the name: {e:?}");
        let e = call(&client, "SetServerProfile", &("lan-2", "broken"))
            .expect_err("the server profile of broken");
        assert_eq!(
            e.0,
            err_name("ServerProfileInUse"),
            "one server profile, one instance"
        );
        assert_eq!(
            e.1, "broken\u{1f}broken",
            "the profile and its owner, split by the unit separator: {e:?}"
        );
        std::fs::write(
            sys.join("server@over.ini"),
            "[Server]\nPort = 8080\nHostname = 127.0.0.1\n",
        )
        .expect("write over");
        let before = std::fs::read_to_string(sys.join("instances.ini")).unwrap();
        let e = call(&client, "SetServerProfile", &("lan-2", "over"))
            .expect_err("the address of default");
        assert_eq!(e.0, err_name("AddressInUse"), "the port of default: {e:?}");
        assert_eq!(
            e.1, "default",
            "an instance is named by its id on the wire: {e:?}"
        );
        assert_eq!(
            std::fs::read_to_string(sys.join("instances.ini")).unwrap(),
            before,
            "the refused switch wrote nothing"
        );
        std::fs::write(
            sys.join("server@web.ini"),
            "[Server]\nPort = 8590\nHostname = 127.0.0.1\n",
        )
        .expect("write web");
        let before = std::fs::read_to_string(sys.join("instances.ini")).unwrap();
        let e = call(&client, "SetServerProfile", &("lan-2", "web"))
            .expect_err("the address of the web service");
        assert_eq!(
            e.0,
            err_name("AddressInUse"),
            "the port of the web service: {e:?}"
        );
        assert_eq!(
            e.1, "private-ai-bff",
            "the web service is named by its service: {e:?}"
        );
        assert_eq!(
            std::fs::read_to_string(sys.join("instances.ini")).unwrap(),
            before,
            "the refused switch wrote nothing"
        );
        // The cap the switch steps into: the new profile's `ModelsMax`, under
        // the loads the instance's render starts. The pool is written for the
        // call, so the render starts two loads and `cap` caps them at 1: the
        // switch lands, and the fit check stops `lan-2`'s pair and leaves the
        // answer naming the instance with the check's key and its arguments
        // (`ARCHITECTURE.md` "Convergence", the IP3c review record).
        std::fs::write(
            sys.join("server@cap.ini"),
            "[Server]\nPort = 8085\nHostname = 127.0.0.1\nModelsMax = 1\n",
        )
        .expect("write cap");
        let pool = std::fs::read_to_string(sys.join("models.ini")).expect("pool");
        std::fs::write(
            sys.join("models.ini"),
            "[a]\nmodel = /models/a.gguf\nload-on-startup = true\n\n\
             [b]\nmodel = /models/b.gguf\nload-on-startup = true\n",
        )
        .expect("write a pool of two loads");
        // The assignment the phase seeded names `[default]` only, so `lan-2`
        // starts nothing and a cap of 1 is over nothing. Seed `lan-2` the
        // assignment that gives it the pool's two loads, so the refusal below
        // is over a render that really starts them.
        let assignments =
            std::fs::read_to_string(sys.join("assignments.ini")).expect("assignments");
        std::fs::write(
            sys.join("assignments.ini"),
            format!("{assignments}\n[lan-2]\n* = inherit\n"),
        )
        .expect("give lan-2 the pool's two loads");
        let kept = (
            std::fs::read_to_string(sys.join("models@lan-2.ini")).unwrap(),
            std::fs::read_to_string(sys.join("launch@lan-2.ini")).unwrap_or_default(),
        );
        let r = call(&client, "SetServerProfile", &("lan-2", "cap"))
            .expect("IP3c: the cap stops one instance's render, it does not refuse the switch");
        let (running, not_converged) = answer_on_bus(&r, "SetServerProfile");
        assert_eq!(
            not_converged,
            vec![(
                "lan-2".to_string(),
                "load-on-startup-over-max".to_string(),
                vec!["2".to_string(), "1".to_string(), "lan-2".to_string()],
            )],
            "IP3c: the new profile's cap is a fit check of ONE instance's pair: it stops \
             lan-2's render and the answer names the instance with that check's key and \
             its arguments, the loads over the cap, the cap, the instance: {not_converged:?}"
        );
        assert_eq!(
            running,
            Vec::<String>::new(),
            "IP3c: the pair stopped is not written, so lan-2 stands on the pair its launch \
             recorded and no instance drifts: {running:?}"
        );
        assert_eq!(
            profile_of("lan-2").as_deref(),
            Some("cap"),
            "IP3c: the switch lands, only the stopped pair does not"
        );
        assert!(
            std::fs::read_to_string(sys.join("instances.ini"))
                .unwrap()
                .starts_with("[default]\nServerProfile = default\n\n[broken]\n"),
            "IP3c: the record's other sections keep their bytes"
        );
        assert_eq!(
            (
                std::fs::read_to_string(sys.join("models@lan-2.ini")).unwrap(),
                std::fs::read_to_string(sys.join("launch@lan-2.ini")).unwrap_or_default(),
            ),
            kept,
            "IP3c: the instance whose pair the fit check refused keeps the bytes it had"
        );
        std::fs::write(sys.join("models.ini"), pool.as_bytes()).expect("the pool back");
        std::fs::write(sys.join("assignments.ini"), assignments.as_bytes())
            .expect("the assignments back");
        std::fs::create_dir(sys.join("server@folder.ini")).expect("dir in the way");
        let before = std::fs::read_to_string(sys.join("instances.ini")).unwrap();
        let e = call(&client, "SetServerProfile", &("lan-2", "folder"))
            .expect_err("a folder where a server profile belongs");
        assert_eq!(
            e.0,
            err_name("Io"),
            "a profile that cannot be read is not an empty one: {e:?}"
        );
        assert_eq!(
            std::fs::read_to_string(sys.join("instances.ini")).unwrap(),
            before,
            "the refused switch wrote nothing"
        );
        std::fs::remove_dir(sys.join("server@folder.ini")).unwrap();
        std::fs::remove_file(sys.join("instances.ini")).expect("remove the record");
        std::fs::create_dir(sys.join("instances.ini")).expect("dir in the way");
        let e = call(&client, "SetServerProfile", &("lan-2", "default"))
            .expect_err("a folder where the record belongs");
        assert_eq!(
            e.0,
            err_name("Io"),
            "a record that cannot be read is not an empty one: {e:?}"
        );
        std::fs::remove_dir(sys.join("instances.ini")).unwrap();
        std::fs::write(sys.join("instances.ini"), fixture.as_bytes()).expect("restore the record");
        // The signature is `(ss)`: no less, no more, and two strings. zbus
        // answers a shape mismatch with its own error, never one of the
        // manager's, and the record is untouched.
        let before = std::fs::read_to_string(sys.join("instances.ini")).unwrap();
        for e in [
            call(&client, "SetServerProfile", &("lan-2",)).expect_err("one string"),
            call(&client, "SetServerProfile", &("lan-2", "lan-2", "more"))
                .expect_err("three strings"),
            call(&client, "SetServerProfile", &("lan-2", 7u32))
                .expect_err("a number where a string belongs"),
        ] {
            assert!(
                !e.0.starts_with("org.privateai.Manager1."),
                "not the manager's error: {e:?}"
            );
        }
        assert_eq!(
            std::fs::read_to_string(sys.join("instances.ini")).unwrap(),
            before,
            "a body the method does not take writes nothing"
        );
        for gone in [
            "server@over.ini",
            "server@web.ini",
            "server@cap.ini",
            "server@alt.ini",
        ] {
            // A pin that did not run wrote no spare profile: its absence is not a failure.
            let _ = std::fs::remove_file(sys.join(gone));
        }
        std::fs::write(sys.join("instances.ini"), fixture.as_bytes())
            .expect("restore the fixture record");
    }

    // ── Instances and the tags of an instance: `(a(sssqasass))` and `(sas)` ─
    // `Instances` answers one row per instance: id, engine, the name of the
    // server profile its section names, port, the tags of that profile, the
    // tags of its own section, and the label of slice IL1 (the bus has no
    // null, so a section naming no label answers the empty string). The name is there because the file the instance
    // runs is not named after the instance, and a client that reads a server
    // file cannot invent it. It asks nobody anything, as the list of ids does.
    // `SetInstanceTags` answers the unit and writes one key of one section.
    {
        let seeded = "[default]\nServerProfile = default\nTags = gpu , edge \n";
        std::fs::write(sys.join("instances.ini"), seeded).expect("record");
        std::fs::write(
            sys.join("server@default.ini"),
            "[Server]\nPort = 8080\nHostname = 127.0.0.1\nTags = edge,v2\n",
        )
        .expect("profile");
        let c = Arc::new(Manager::new(
            FakeService::new(),
            FakeAuthority::new(true),
            FakeResolver::lan(),
        ));
        let (_server, client) = connect(&c, Some(":1.42"));

        let r = call(&client, "Instances", &()).expect("Instances");
        assert_eq!(
            r.body().signature().to_string(),
            "a(sssqasass)",
            "an array of a struct of two strings, the server profile, a uint16, two arrays of strings and the label"
        );
        assert_eq!(
            body::<
                Vec<(
                    String,
                    String,
                    String,
                    u16,
                    Vec<String>,
                    Vec<String>,
                    String,
                )>,
            >(&r),
            vec![(
                "default".to_string(),
                "llama.cpp".to_string(),
                "default".to_string(),
                8080u16,
                vec!["edge".to_string(), "v2".to_string()],
                vec!["gpu".to_string(), "edge".to_string()],
                String::new(),
            )],
            "the server profile the section names, the port of that profile, its tags, \
             and the section's own trimmed, and no label: the bus has no null"
        );
        assert_eq!(
            *c.authority().asked.lock().unwrap(),
            vec![],
            "the list asks polkit nothing"
        );
        let (_server, anon) = connect(&c, None);
        assert!(
            call(&anon, "Instances", &()).is_ok(),
            "a client with no sender reads the tags as it reads the ids"
        );

        let r = call(
            &client,
            "SetInstanceTags",
            &("default", vec!["edge".to_string(), "a-1".to_string()]),
        )
        .expect("SetInstanceTags of default");
        assert_eq!(
            r.body().signature().to_string(),
            String::new(),
            "the reply is the unit: ()"
        );
        assert_eq!(
            std::fs::read_to_string(sys.join("instances.ini")).unwrap(),
            "[default]\nServerProfile = default\nTags = edge,a-1\n",
            "the section's own Tags replaced in place, the list joined with a comma"
        );

        let e = call(
            &client,
            "SetInstanceTags",
            &("a_b", vec!["edge".to_string()]),
        )
        .expect_err("an invalid id");
        assert_eq!(e.0, err_name("InvalidId"), "{e:?}");
        let e = call(
            &client,
            "SetInstanceTags",
            &("default", vec!["Edge".to_string()]),
        )
        .expect_err("a tag of bad characters");
        assert_eq!(e.0, err_name("Invalid"), "{e:?}");
        assert!(
            e.1.contains("server-bad-tags"),
            "the refusal key travels in the message: {e:?}"
        );
        assert_eq!(
            std::fs::read_to_string(sys.join("instances.ini")).unwrap(),
            "[default]\nServerProfile = default\nTags = edge,a-1\n",
            "a refused list wrote nothing"
        );

        // The signature is `(sas)`: an id and an array of strings. zbus
        // answers a shape mismatch with its own error, never one of the
        // manager's, and the record is untouched.
        for e in [
            call(&client, "SetInstanceTags", &("default",)).expect_err("one string"),
            call(&client, "SetInstanceTags", &("default", "edge"))
                .expect_err("a string where an array belongs"),
            call(&client, "SetInstanceTags", &("default", vec![7u32]))
                .expect_err("a number where a string belongs"),
        ] {
            assert!(
                !e.0.starts_with("org.privateai.Manager1."),
                "not the manager's error: {e:?}"
            );
        }
        assert_eq!(
            std::fs::read_to_string(sys.join("instances.ini")).unwrap(),
            "[default]\nServerProfile = default\nTags = edge,a-1\n",
            "a body the method does not take writes nothing"
        );
        std::fs::write(
            sys.join("instances.ini"),
            "[default]\nServerProfile = default\n",
        )
        .expect("restore the record");
    }

    // ── The label of an instance (IL1): the text the web UI shows beside the id
    // `SetInstanceLabel(ss)` answers the unit: the text trimmed at both ends
    // into the section's `Label` key, the empty text REMOVING the key. The
    // `Instances` row grows one field at the end, `s label`: the bus has no
    // null, so the empty string is "this section has no label", and the UI falls
    // back to the engine and the port of the same row. A label another instance
    // carries, ignoring case, is `instance-label-taken`; a text the rule refuses
    // is `instance-bad-label` and one with a `;` or a `#` is `comment-markers`,
    // each with the record untouched. A body of the wrong shape is zbus's own
    // error, never one of the manager's.
    {
        // The two files this block rewrites, as the blocks before left them,
        // so the blocks after read what they read without this block.
        let fixture = std::fs::read_to_string(sys.join("instances.ini")).expect("record");
        let profile = std::fs::read_to_string(sys.join("server@default.ini")).expect("profile");
        let seeded = "[default]\nServerProfile = default\n\n[gpu-1]\nServerProfile = gpu-1\nLabel = GPU box\nnote = keep me\n";
        std::fs::write(sys.join("instances.ini"), seeded).expect("record");
        std::fs::write(
            sys.join("server@default.ini"),
            "[Server]\nPort = 8080\nHostname = 127.0.0.1\n",
        )
        .expect("profile");
        let c = Arc::new(Manager::new(
            FakeService::new(),
            FakeAuthority::new(true),
            FakeResolver::lan(),
        ));
        let (_server, client) = connect(&c, Some(":1.42"));
        type Rows = Vec<(
            String,
            String,
            String,
            u16,
            Vec<String>,
            Vec<String>,
            String,
        )>;
        let row = |id: &str, profile: &str, port: u16, label: &str| {
            (
                id.to_string(),
                "llama.cpp".to_string(),
                profile.to_string(),
                port,
                Vec::<String>::new(),
                Vec::<String>::new(),
                label.to_string(),
            )
        };

        let r = call(&client, "Instances", &()).expect("Instances");
        assert_eq!(
            r.body().signature().to_string(),
            "a(sssqasass)",
            "the row grows by the label at its end"
        );
        assert_eq!(
            body::<Rows>(&r),
            vec![
                row("default", "default", 8080, ""),
                row("gpu-1", "gpu-1", 0, "GPU box"),
            ],
            "a section with no Label is the empty string, the engine and the port \
             travel beside the label in the same row"
        );

        let r = call(&client, "SetInstanceLabel", &("default", "  Coding box "))
            .expect("SetInstanceLabel of default");
        assert_eq!(
            r.body().signature().to_string(),
            String::new(),
            "the reply is the unit: ()"
        );
        assert_eq!(
            std::fs::read_to_string(sys.join("instances.ini")).unwrap(),
            "[default]\nServerProfile = default\nLabel = Coding box\n\n[gpu-1]\nServerProfile = gpu-1\nLabel = GPU box\nnote = keep me\n",
            "the key written trimmed, every other byte of the record kept"
        );

        assert!(
            matches!(
                call(&client, "SetInstanceLabel", &("default", ""))
                    .ok()
                    .map(|r| r.body().signature().to_string()),
                Some(s) if s.is_empty()
            ),
            "the empty text is accepted, and answers the unit"
        );
        assert_eq!(
            std::fs::read_to_string(sys.join("instances.ini")).unwrap(),
            seeded,
            "the empty text removed the Label line, not left it empty"
        );
        assert_eq!(
            body::<Rows>(&call(&client, "Instances", &()).expect("Instances"))[0].6,
            "",
            "the row answers no label after the clear"
        );

        for (args, kind, needle) in [
            (("default", "GPU BOX"), "Invalid", "instance-label-taken"),
            (("default", "a;b"), "Invalid", "comment-markers"),
            (("default", "Coding #1"), "Invalid", "comment-markers"),
            (
                ("default", "z".repeat(65).as_str()),
                "Invalid",
                "instance-bad-label",
            ),
            (("default", "two\nlines"), "Invalid", "instance-bad-label"),
            (("lan-3", "Coding box"), "NoInstance", ""),
            (("a_b", "Coding box"), "InvalidId", ""),
        ] {
            let e =
                call(&client, "SetInstanceLabel", &args).expect_err(&format!("{args:?} refused"));
            assert_eq!(e.0, err_name(kind), "{args:?}: {e:?}");
            if !needle.is_empty() {
                assert!(
                    e.1.contains(needle),
                    "the refusal key travels in the message: {args:?}: {e:?}"
                );
            }
        }
        assert_eq!(
            std::fs::read_to_string(sys.join("instances.ini")).unwrap(),
            seeded,
            "every refusal above wrote nothing"
        );

        // The rule is unique among OTHER instances: gpu-1 carries `GPU box`, so
        // the same text in another case is its own label. The bus accepts it,
        // writes the new case, and the row reads it back.
        assert!(
            matches!(
                call(&client, "SetInstanceLabel", &("gpu-1", "GPU BOX"))
                    .ok()
                    .map(|r| r.body().signature().to_string()),
                Some(s) if s.is_empty()
            ),
            "an instance relabelling the case of its own label is no clash: ()"
        );
        assert_eq!(
            body::<Rows>(&call(&client, "Instances", &()).expect("Instances"))[1].6,
            "GPU BOX",
            "the row carries the new case of gpu-1"
        );
        assert_eq!(
            std::fs::read_to_string(sys.join("instances.ini")).unwrap(),
            seeded.replacen("Label = GPU box", "Label = GPU BOX", 1),
            "the new case, and every other byte kept"
        );
        std::fs::write(sys.join("instances.ini"), seeded).expect("restore the record");

        // The signature is `(ss)`: an id and a text. zbus answers a shape
        // mismatch with its own error, never one of the manager's.
        for e in [
            call(&client, "SetInstanceLabel", &("default",)).expect_err("one string"),
            call(&client, "SetInstanceLabel", &("default", 7u32))
                .expect_err("a number where a string belongs"),
            call(
                &client,
                "SetInstanceLabel",
                &("default", vec!["x".to_string()]),
            )
            .expect_err("an array where a string belongs"),
        ] {
            assert!(
                !e.0.starts_with("org.privateai.Manager1."),
                "not the manager's error: {e:?}"
            );
        }
        assert_eq!(
            std::fs::read_to_string(sys.join("instances.ini")).unwrap(),
            seeded,
            "a body the method does not take writes nothing"
        );
        std::fs::write(sys.join("instances.ini"), fixture).expect("restore the record");
        std::fs::write(sys.join("server@default.ini"), profile).expect("restore the profile");
    }

    // ── ReadFile says when it withheld the API key ───────────────────────
    // The reply is (exists, text, withheld): an older client expecting
    // (exists, text) fails to read it instead of taking the blanked file
    // for one with no key.
    {
        let keyed = "[Server]\nPort = 8082\nHostname = 127.0.0.1\nOpencodeApiKey = sk-local-3f9a\n";
        std::fs::write(sys.join("server@keyed.ini"), keyed).expect("write keyed");
        let quiet = |configure: bool| {
            Arc::new(
                Manager::new(
                    FakeService::new(),
                    FakeAuthority::new(true).quietly(configure),
                    FakeResolver::lan(),
                )
                .with_group(crate::manager::service::fakes::FakeGroup::new()),
            )
        };
        let c = quiet(false);
        let (_server, client) = connect(&c, Some(":1.42"));
        let r = call(&client, "ReadFile", &("server@keyed.ini",)).expect("ReadFile");
        assert_eq!(
            body::<(bool, String, bool)>(&r),
            (
                true,
                "[Server]\nPort = 8082\nHostname = 127.0.0.1\nOpencodeApiKey =\n".to_string(),
                true
            ),
            "a caller without configure: the key blanked and withheld said"
        );
        assert!(
            r.body().deserialize::<(bool, String)>().is_err(),
            "an older client cannot read the reply as (exists, text)"
        );
        assert_eq!(
            *c.authority().asked_quietly.lock().unwrap(),
            vec![(alice.clone(), Action::Configure)],
            "configure asked of the sender, without a prompt"
        );

        let c = quiet(true);
        let (_server, client) = connect(&c, Some(":1.42"));
        let r = call(&client, "ReadFile", &("server@keyed.ini",)).expect("ReadFile");
        assert_eq!(
            body::<(bool, String, bool)>(&r),
            (true, keyed.to_string(), false),
            "an administrator: the file as saved"
        );
        std::fs::remove_file(sys.join("server@keyed.ini")).expect("remove keyed");
    }

    // ── Status names every unit state as systemd does ────────────────────
    for (state, name) in [
        (UnitState::Inactive, "inactive"),
        (UnitState::Failed, "failed"),
        (UnitState::Activating, "activating"),
        (UnitState::Deactivating, "deactivating"),
    ] {
        let c = core_in(state, true, &[]);
        let (_server, client) = connect(&c, None);
        let r = call(&client, "Status", &("default",)).expect("Status");
        assert_eq!(body::<String>(&r), name, "{state:?}");
    }

    // ── WriteFileIf: the compare and swap over D-Bus ──────────────────────
    // The content on the wire is the file's own bytes, read from disk: the
    // caller that holds them writes, a caller holding a content the file no
    // longer holds is refused `org.privateai.Manager1.Error.Changed` with the
    // file left as the landed write left it, and a file that is not there is
    // `Changed` as well (the core's rule).
    {
        let c = core(true, &[]);
        let (_server, client) = connect(&c, Some(":1.42"));
        let base = std::fs::read_to_string(sys.join("server@default.ini")).expect("read default");
        let first = "[Server]\nPort = 8080\nHostname = 127.0.0.1\n";
        let second = "[Server]\nPort = 8083\nHostname = 127.0.0.1\n";
        call(
            &client,
            "WriteFileIf",
            &("server@default.ini", first, base.as_str()),
        )
        .expect("a caller holding the file's content writes");
        assert_eq!(
            std::fs::read_to_string(sys.join("server@default.ini")).unwrap(),
            first,
            "the fresh content landed"
        );
        let e = call(
            &client,
            "WriteFileIf",
            &("server@default.ini", second, base.as_str()),
        )
        .expect_err("a caller holding a content the file no longer holds");
        assert_eq!(e.0, err_name("Changed"), "the name of a stale content");
        assert!(!e.1.trim().is_empty(), "the reply carries a message: {e:?}");
        assert_eq!(
            std::fs::read_to_string(sys.join("server@default.ini")).unwrap(),
            first,
            "the refused write changed nothing on the way to the error"
        );
        let e = call(&client, "WriteFileIf", &("server@nope.ini", "x", ""))
            .expect_err("a file that is not there");
        assert_eq!(e.0, err_name("Changed"), "no file, no match");
        assert_eq!(
            asked(&c),
            vec![(alice.clone(), Action::Configure); 3],
            "the write is the guarded call it is: configure asked of the sender, every call"
        );
        assert_eq!(
            calls(&c),
            vec![Call::Status("default".into())],
            "IP3c: the accepted compare and swap answers the apply, whose drift compare asks \
             the state of `default`, the one instance whose pair the new server profile moves; \
             the refusals ask nothing, no unit verb"
        );
        std::fs::write(sys.join("server@default.ini"), default_ini).expect("restore default");
    }
}

/// E2E phase for the D-Bus interface while polkit waits for a user to
/// authenticate, for this crate's e2e test to call (it sets the process-wide
/// `PRIVATE_AI_DATA_ROOT` redirect, `src/tests/mod.rs`). On the system bus
/// every caller reaches the service over its one connection; here one p2p
/// connection carries both calls: a `Status` must answer while a `Start` on
/// the same connection waits for polkit, and the `Start` then gets its own
/// reply.
#[cfg(test)]
pub(crate) fn manager_dbus_waits_e2e() {
    use super::service::fakes::{FakeResolver, FakeService, Gate, GateAuthority, GATE_WAIT};
    use super::service::on_thread;
    use super::service::real_model_files;
    use std::os::unix::net::UnixStream;
    use std::time::Duration;
    use zbus::blocking::connection::Builder;

    let dir = tempfile::tempdir().expect("tempdir");
    crate::paths::redirect_for_tests(dir.path());
    let sys = crate::paths::engine_config_dir();
    std::fs::create_dir_all(&sys).expect("system config dir");
    let model_profiles = real_model_files(
        dir.path(),
        "[Qwen3.8-27B]\nmodel = /models/Qwen3.8-27B-Q8_0.gguf\n",
    );
    std::fs::write(
        sys.join("server@default.ini"),
        "[Server]\nPort = 8080\nHostname = 127.0.0.1\n",
    )
    .expect("write default");
    std::fs::write(sys.join("models.ini"), &model_profiles).expect("write presets");
    // An instance is a section of the record, not a file: the Start this phase
    // waits at polkit for is an instance only once `instances.ini` has it.
    std::fs::write(
        sys.join("instances.ini"),
        "[default]\nServerProfile = default\n",
    )
    .expect("write instances");
    // ... and a render with a model profile to load, so that Start is a start.
    std::fs::write(
        sys.join("models@default.ini"),
        "[seed]\nmodel = /models/seed.gguf\n",
    )
    .expect("write render");

    let alice = Caller(":1.42".into());
    let gate = Gate::new();
    let core = Manager::new(
        FakeService::new(),
        GateAuthority::new(&alice, &gate, &[(&alice, true)]),
        FakeResolver::lan(),
    )
    .with_group(crate::manager::service::fakes::FakeGroup::new());
    let iface = ManagerInterface::new(Arc::new(core));

    let (s0, s1) = UnixStream::pair().expect("socket pair");
    let server = std::thread::spawn(move || {
        Builder::async_io_unix_stream(s0)
            .server(zbus::Guid::generate())
            .expect("server guid")
            .p2p()
            .serve_at(OBJECT_PATH, iface)
            .expect("serve_at")
            .build()
            .expect("server connection")
    });
    // The client's deadline is past the gate's, so a held Start fails on the
    // gate's own timeout, never hangs.
    let client = Builder::async_io_unix_stream(s1)
        .p2p()
        .method_timeout(GATE_WAIT + Duration::from_secs(5))
        .build()
        .expect("client connection");
    client
        .inner()
        .set_unique_name(alice.0.as_str())
        .expect("unique name");
    let _server = server.join().expect("server thread");

    // A call on its own thread, its reply decoded as `T`, or the error.
    fn call<T, B>(
        client: &zbus::blocking::Connection,
        method: &'static str,
        body: B,
    ) -> std::sync::mpsc::Receiver<Result<T, String>>
    where
        T: for<'de> zbus::zvariant::DynamicDeserialize<'de> + Send + 'static,
        B: serde::ser::Serialize + zbus::zvariant::DynamicType + Send + 'static,
    {
        let client = client.clone();
        on_thread(move || {
            client
                .call_method(None::<&str>, OBJECT_PATH, Some(INTERFACE), method, &body)
                .map_err(|e| e.to_string())
                .and_then(|r| r.body().deserialize::<T>().map_err(|e| e.to_string()))
        })
    }
    // The result on `rx`, which must come while the Start is held at the
    // gate. On a timeout the gate is opened first, so nothing is left
    // waiting, then the test fails.
    fn answer<T>(gate: &Gate, what: &str, rx: std::sync::mpsc::Receiver<T>) -> T {
        let deadline = Duration::from_secs(2);
        rx.recv_timeout(deadline).unwrap_or_else(|_| {
            gate.open();
            panic!("{what}: no reply within {deadline:?} while a Start waited for polkit")
        })
    }

    let start = call::<(), _>(&client, "Start", ("default",));
    assert!(gate.arrived(1), "the Start reached polkit");

    let status = answer(
        &gate,
        "Status",
        call::<String, _>(&client, "Status", ("default",)),
    );
    assert_eq!(status, Ok("active".to_string()), "Status replies");
    let read = answer(
        &gate,
        "ReadFile",
        call::<(bool, String, bool), _>(&client, "ReadFile", ("models.ini",)),
    );
    assert_eq!(
        read,
        Ok((true, model_profiles.to_string(), false)),
        "ReadFile replies"
    );

    assert!(
        start.try_recv().is_err(),
        "the Start is still waiting for polkit"
    );
    gate.open();
    let started = start
        .recv_timeout(GATE_WAIT + Duration::from_secs(5))
        .expect("the Start replies once polkit answers");
    assert!(started.is_ok(), "the Start's own reply: {started:?}");
}

/// E2E phase for a refused write's reply over D-Bus, for this crate's e2e
/// test to call (it sets the process-wide `PRIVATE_AI_DATA_ROOT` redirect,
/// `src/tests/mod.rs`): the client gets `org.privateai.Manager1.Error.Invalid`
/// and reads the refusal back, key and values, with `Refusal::decode`.
#[cfg(test)]
pub(crate) fn manager_dbus_checks_e2e() {
    use super::service::fakes::{FakeAuthority, FakeResolver, FakeService};
    use super::service::real_model_files;
    use pai_core::checks::Refusal;
    use std::os::unix::net::UnixStream;
    use std::time::Duration;
    use zbus::blocking::connection::Builder;

    let dir = tempfile::tempdir().expect("tempdir");
    crate::paths::redirect_for_tests(dir.path());
    let sys = crate::paths::engine_config_dir();
    std::fs::create_dir_all(&sys).expect("system config dir");
    let default_ini = "[Server]\nPort = 8080\nHostname = 127.0.0.1\n";
    let model_profiles = real_model_files(
        dir.path(),
        "[Qwen3.8-27B]\nmodel = /models/Qwen3.8-27B-Q8_0.gguf\n",
    );
    std::fs::write(sys.join("server@default.ini"), default_ini).expect("write default");
    std::fs::write(sys.join("models.ini"), &model_profiles).expect("write presets");
    std::fs::write(
        sys.join("instances.ini"),
        "[default]\nServerProfile = default\n",
    )
    .expect("write instances");

    let core = Arc::new(
        Manager::new(
            FakeService::new(),
            FakeAuthority::new(true),
            FakeResolver::lan(),
        )
        .with_group(crate::manager::service::fakes::FakeGroup::new()),
    );
    let (s0, s1) = UnixStream::pair().expect("socket pair");
    let iface = ManagerInterface::new(core);
    let server = std::thread::spawn(move || {
        Builder::async_io_unix_stream(s0)
            .server(zbus::Guid::generate())
            .expect("server guid")
            .p2p()
            .serve_at(OBJECT_PATH, iface)
            .expect("serve_at")
            .build()
            .expect("server connection")
    });
    let client = Builder::async_io_unix_stream(s1)
        .p2p()
        .method_timeout(Duration::from_secs(10))
        .build()
        .expect("client connection");
    client
        .inner()
        .set_unique_name(":1.42")
        .expect("unique name");
    let _server = server.join().expect("server thread");

    // A WriteFile: Ok, or Err((error name, message)).
    let write = |name: &str, contents: &str| -> Result<(), (String, String)> {
        client
            .call_method(
                None::<&str>,
                OBJECT_PATH,
                Some(INTERFACE),
                "WriteFile",
                &(name, contents, false),
            )
            .map(|_| ())
            .map_err(|e| match e {
                zbus::Error::MethodError(name, detail, _) => {
                    (name.to_string(), detail.unwrap_or_default())
                }
                other => panic!("WriteFile: not a method error reply: {other:?}"),
            })
    };
    let refusal = |key: &'static str, values: &[&str]| Refusal {
        key,
        values: values.iter().map(|v| v.to_string()).collect(),
    };
    let invalid = "org.privateai.Manager1.Error.Invalid";

    let (name, message) = write(
        "server@default.ini",
        "[Server]\nPort = 70000\nHostname = 127.0.0.1\n",
    )
    .expect_err("an invalid port");
    assert_eq!(name, invalid);
    assert_eq!(
        Refusal::decode(&message),
        Some(refusal(
            "not-whole-number-in-range",
            &["Port", "70000", "1", "65535"]
        )),
        "{message:?}"
    );
    assert_eq!(
        std::fs::read_to_string(sys.join("server@default.ini")).unwrap(),
        default_ini,
        "refused: the file kept"
    );

    // A value with spaces, a backslash and a non-ASCII letter comes back whole.
    let hostile = "[Qwen 3 (v2) é]\nmodel = /models/q.gguf\n";
    let (name, message) = write("models.ini", hostile).expect_err("an invalid id");
    assert_eq!(name, invalid);
    assert_eq!(
        Refusal::decode(&message),
        Some(refusal("invalid-model-profile-id", &["Qwen 3 (v2) é"])),
        "{message:?}"
    );
    let (name, message) = write(
        "models.ini",
        &format!("{model_profiles}[m]\nmodel = /models/m.gguf\noverride-tensor = blk\\.0\\.ffn_up_exps\n"),
    )
    .expect_err("an override with no device");
    assert_eq!(name, invalid);
    assert_eq!(
        Refusal::decode(&message),
        Some(refusal(
            "override-no-device",
            &["1", r"blk\.0\.ffn_up_exps"]
        )),
        "{message:?}"
    );
    assert_eq!(
        std::fs::read_to_string(sys.join("models.ini")).unwrap(),
        model_profiles,
        "refused: the file kept"
    );

    // The verb's refusals carry the encoded refusal as a write's does: the
    // model profile the pool has not, and a value that is not one of the
    // four.
    std::fs::write(sys.join("assignments.ini"), "[default]\n").expect("assignments");
    let set = |instance: &str, profile: &str, value: &str| -> Result<(), (String, String)> {
        client
            .call_method(
                None::<&str>,
                OBJECT_PATH,
                Some(INTERFACE),
                "SetModelProfile",
                &(instance, profile, value),
            )
            .map(|_| ())
            .map_err(|e| match e {
                zbus::Error::MethodError(name, detail, _) => {
                    (name.to_string(), detail.unwrap_or_default())
                }
                other => panic!("SetModelProfile: not a method error reply: {other:?}"),
            })
    };
    let (name, message) = set("default", "zzz", "on").expect_err("no model profile zzz");
    assert_eq!(name, invalid);
    assert_eq!(
        Refusal::decode(&message),
        Some(refusal("no-model-profile-named", &["zzz"])),
        "{message:?}"
    );
    assert_eq!(
        std::fs::read_to_string(sys.join("assignments.ini")).unwrap(),
        "[default]\n",
        "refused: the record kept"
    );
    let (name, message) =
        set("default", "Qwen3.8-27B", "maybe").expect_err("a value that is not one of the four");
    assert_eq!(name, invalid);
    assert!(
        Refusal::decode(&message).is_some(),
        "the value's refusal carries its key: {message:?}"
    );
    assert_eq!(
        std::fs::read_to_string(sys.join("assignments.ini")).unwrap(),
        "[default]\n",
        "refused: the record kept"
    );

    // A valid write still goes through.
    let more = format!("{model_profiles}[m]\nmodel = /models/m.gguf\nctx-size = 8192\n");
    assert_eq!(write("models.ini", &more), Ok(()));
}

/// E2E phase for the D-Bus face acting for a signed-in user: the bff's
/// account naming a `pai` member through the `As` methods, over the instance
/// files a guarded call touches, so it redirects the data root
/// (`src/tests/mod.rs`). Served peer to peer on a socket pair, never a bus;
/// every seam is a fake: no polkit, no systemd, no real account is asked.
#[cfg(test)]
pub(crate) fn manager_dbus_principal_e2e() {
    use super::service::fakes::{
        Call, FakeAuthority, FakeGroup, FakePasswords, FakeResolver, FakeService,
    };
    use super::service::real_model_files;
    use std::os::unix::net::UnixStream;
    use std::sync::Mutex;
    use std::time::Duration;
    use zbus::blocking::connection::Builder;
    use zbus::blocking::Connection;

    type Core = Manager<FakeService, FakeAuthority, FakeResolver>;
    /// The membership questions a core over [`FakePasswords`] asks.
    type Admins = Arc<Mutex<Vec<String>>>;

    /// The bff's bus name in this phase.
    const BFF: &str = ":1.9";
    const PREFIX: &str = "org.privateai.Manager1.Error.";

    let dir = tempfile::tempdir().expect("tempdir");
    crate::paths::redirect_for_tests(dir.path());
    let sys = crate::paths::engine_config_dir();
    std::fs::create_dir_all(&sys).expect("engine config dir");
    let ini = "[Server]\nPort = 8080\nHostname = 127.0.0.1\nOpencodeApiKey = sk-local-3f9a\n";
    let model_profiles = real_model_files(
        dir.path(),
        "[Qwen3.8-27B]\nmodel = /m/Qwen3.8-27B-Q8_0.gguf\n",
    );
    let more = real_model_files(
        dir.path(),
        &format!("{model_profiles}[Qwen3.8-14B]\nmodel = /m/Qwen3.8-14B-Q4_K_M.gguf\n"),
    );
    std::fs::write(sys.join("server@default.ini"), ini).expect("instance file");
    std::fs::write(sys.join("models.ini"), model_profiles).expect("presets file");
    // An instance is a section of the record: the `As` verbs of this phase
    // reach the service manager only for an id `instances.ini` names.
    std::fs::write(
        sys.join("instances.ini"),
        "[default]\nServerProfile = default\n",
    )
    .expect("instances.ini");
    // A start loads the instance's render, which the manager renders from the
    // pool and the assignment: these pins are about the caller, so `default`
    // has a model profile assigned and the render that names it.
    std::fs::write(
        sys.join("assignments.ini"),
        "[default]\nQwen3.8-27B = on\nQwen3.8-14B = on\n",
    )
    .expect("assignments.ini");
    std::fs::write(
        sys.join("models@default.ini"),
        "[Qwen3.8-27B]\nmodel = /m/Qwen3.8-27B-Q8_0.gguf\n",
    )
    .expect("models@default.ini");
    // The launch of `default` recorded the pair standing now (IP3c): the writes
    // below move a pair off a real record, so their drift compare asks its state.
    super::service::write_applied("default").expect("the applied record");
    let text_of = |name: &str| std::fs::read_to_string(sys.join(name)).expect("read");

    // `core` served on one end of a socket pair (the server built on its own
    // thread: both ends handshake at once), a client on the other stamping
    // `sender` as the bus would. Every call has a deadline: a stub that
    // panics in the server fails the test, not hangs it.
    fn connect(core: &Arc<Core>, sender: Option<&str>) -> (Connection, Connection) {
        let (s0, s1) = UnixStream::pair().expect("socket pair");
        let iface = ManagerInterface::new(core.clone());
        let server = std::thread::spawn(move || {
            Builder::async_io_unix_stream(s0)
                .server(zbus::Guid::generate())
                .expect("server guid")
                .p2p()
                .serve_at(OBJECT_PATH, iface)
                .expect("serve_at")
                .build()
                .expect("server connection")
        });
        let client = Builder::async_io_unix_stream(s1)
            .p2p()
            .method_timeout(Duration::from_secs(10))
            .build()
            .expect("client connection");
        if let Some(name) = sender {
            client.inner().set_unique_name(name).expect("unique name");
        }
        (server.join().expect("server thread"), client)
    }
    // A method call: Ok(reply), or Err((error name, message)).
    fn call<B>(conn: &Connection, method: &str, body: &B) -> Result<zbus::Message, (String, String)>
    where
        B: serde::ser::Serialize + zbus::zvariant::DynamicType,
    {
        conn.call_method(None::<&str>, OBJECT_PATH, Some(INTERFACE), method, body)
            .map_err(|e| match e {
                zbus::Error::MethodError(name, detail, _) => {
                    (name.to_string(), detail.unwrap_or_default())
                }
                zbus::Error::FDO(e) => {
                    use zbus::DBusError;
                    (
                        e.name().to_string(),
                        e.description().unwrap_or_default().to_string(),
                    )
                }
                other => panic!("{method}: not a method error reply: {other:?}"),
            })
    }
    fn body<T: for<'de> zbus::zvariant::DynamicDeserialize<'de>>(m: &zbus::Message) -> T {
        m.body().deserialize().expect("reply body")
    }

    let bff = Caller(BFF.into());
    // The accounts: alice a member of `pai`, bob not. The membership log is
    // taken before the fake moves into the core.
    let passwords = FakePasswords::new();
    let admins = passwords.asked_admin.clone();
    // polkit refuses this caller everything, and would challenge it for
    // `configure`: a call for a member needs no right of the bff's own, so a
    // call that ran as the bff's own is refused here.
    let c = Arc::new(
        Manager::new(
            FakeService::new(),
            FakeAuthority::new(false).quietly(false).bff(&bff),
            FakeResolver::lan(),
        )
        .with_group(FakeGroup::new())
        .with_passwords(passwords),
    );
    let calls = |c: &Arc<Core>| c.service().calls.lock().unwrap().clone();
    let polkit = |c: &Arc<Core>| c.authority().asked.lock().unwrap().clone();
    let polkit_quiet = |c: &Arc<Core>| c.authority().asked_quietly.lock().unwrap().clone();
    let bff_checks = |c: &Arc<Core>| c.authority().asked_bff.lock().unwrap().clone();
    let asked_admin = |a: &Admins| a.lock().unwrap().clone();

    let (_server, client) = connect(&c, Some(BFF));

    // ── The bff acting for a member: the control calls run ───────────────
    // The principal is the first argument, as in the design's
    // `StartAs(s principal, s id)`: an implementation taking them the other
    // way fails here, naming an instance "alice".
    call(&client, "StartAs", &("alice", "default")).expect("StartAs for a member");
    call(&client, "StopAs", &("alice", "default")).expect("StopAs for a member");
    call(&client, "RestartAs", &("alice", "default")).expect("RestartAs for a member");
    call(&client, "SetBootEnabledAs", &("alice", "default", true))
        .expect("SetBootEnabledAs for a member");
    let r = call(&client, "BootEnabled", &("default",)).expect("BootEnabled");
    assert!(
        body::<bool>(&r),
        "the member's change of the start at boot landed"
    );
    assert_eq!(
        calls(&c),
        vec![
            Call::Start("default".into()),
            Call::Stop("default".into()),
            Call::Restart("default".into()),
            Call::SetBootEnabled("default".into(), true),
            Call::BootEnabled("default".into()),
        ],
        "every call reaches the service manager"
    );
    assert_eq!(
        polkit(&c),
        vec![],
        "polkit asked, so the call ran as the bff's own, not for the principal"
    );
    assert_eq!(
        bff_checks(&c),
        vec![bff.clone(); 4],
        "the caller is checked first, at every call"
    );
    assert_eq!(
        asked_admin(&admins),
        vec!["alice".to_string(); 4],
        "the membership of the principal, never of the caller"
    );

    // ── A member's write lands, and their read is never withheld ─────────
    // The bff is a caller polkit would challenge for `configure` here: the
    // key stays because the member may configure, so a read that fell back to
    // the caller's own would blank it and log the quiet question.
    call(
        &client,
        "WriteFileAs",
        &("alice", "models.ini", more.as_str(), false),
    )
    .expect("WriteFileAs for a member");
    assert_eq!(text_of("models.ini"), more, "the member's write landed");
    let read = call(&client, "ReadFileAs", &("alice", "server@default.ini"))
        .expect("ReadFileAs for a member");
    assert_eq!(
        body::<(bool, String, bool)>(&read),
        (true, ini.to_string(), false),
        "the key kept and nothing withheld from a member's read"
    );
    let model_profile =
        call(&client, "ReadFileAs", &("alice", "models.ini")).expect("ReadFileAs of a preset file");
    let plain = call(&client, "ReadFile", &("models.ini",)).expect("ReadFile of a preset file");
    assert_eq!(
        model_profile.body().signature(),
        plain.body().signature(),
        "the As reply has the plain method's shape"
    );
    assert_eq!(
        polkit_quiet(&c),
        vec![],
        "configure asked to decide the read: the read ran as the bff's own"
    );
    assert_eq!(polkit(&c), vec![], "control is not asked either");
    assert_eq!(
        calls(&c),
        vec![
            Call::Start("default".into()),
            Call::Stop("default".into()),
            Call::Restart("default".into()),
            Call::SetBootEnabled("default".into(), true),
            Call::BootEnabled("default".into()),
        ],
        "a file call reaches no service"
    );
    assert_eq!(
        bff_checks(&c),
        vec![bff.clone(); 7],
        "one check per As call"
    );
    assert_eq!(
        asked_admin(&admins),
        vec!["alice".to_string(); 7],
        "the membership asked at every call, never cached"
    );

    // ── A principal who is no member: refused, nothing done ──────────────
    let refused = |e: (String, String), method: &str| {
        assert_eq!(e.0, format!("{PREFIX}NotAuthorized"), "{method} for bob");
    };
    refused(
        call(&client, "StartAs", &("bob", "default")).expect_err("StartAs for a non-member"),
        "StartAs",
    );
    refused(
        call(
            &client,
            "WriteFileAs",
            &("bob", "models.ini", "junk\n", false),
        )
        .expect_err("WriteFileAs for a non-member"),
        "WriteFileAs",
    );
    refused(
        call(&client, "ReadFileAs", &("bob", "server@default.ini"))
            .expect_err("ReadFileAs for a non-member"),
        "ReadFileAs",
    );
    assert_eq!(
        text_of("models.ini"),
        more,
        "a refused write leaves the file"
    );
    assert_eq!(
        text_of("server@default.ini"),
        ini,
        "a refused read changes nothing"
    );
    assert_eq!(
        calls(&c),
        vec![
            Call::Start("default".into()),
            Call::Stop("default".into()),
            Call::Restart("default".into()),
            Call::SetBootEnabled("default".into(), true),
            Call::BootEnabled("default".into()),
        ],
        "a non-member's refused calls started nothing"
    );
    assert_eq!(polkit(&c), vec![], "a refused principal asks nobody");
    let mut asked = vec!["alice".to_string(); 7];
    asked.extend(["bob", "bob", "bob"].map(String::from));
    assert_eq!(
        asked_admin(&admins),
        asked,
        "the membership of the name the caller vouches for"
    );

    // ── A caller that is not the bff naming a member: refused, nothing started ──
    {
        let (_server, other) = connect(&c, Some(":1.42"));
        for (method, args) in [
            ("StartAs", ("alice", "default")),
            ("StopAs", ("alice", "default")),
            ("RestartAs", ("alice", "default")),
        ] {
            let e = call(&other, method, &args)
                .expect_err(&format!("{method} from a caller that is not the bff"));
            assert_eq!(e.0, format!("{PREFIX}NotAuthorized"), "{method}");
        }
        let e = call(
            &other,
            "WriteFileAs",
            &("alice", "models.ini", "junk\n", false),
        )
        .expect_err("WriteFileAs from a caller that is not the bff");
        assert_eq!(e.0, format!("{PREFIX}NotAuthorized"));
        let e = call(&other, "ReadFileAs", &("alice", "server@default.ini"))
            .expect_err("ReadFileAs from a caller that is not the bff");
        assert_eq!(e.0, format!("{PREFIX}NotAuthorized"));
        assert_eq!(
            text_of("models.ini"),
            more,
            "a refused write leaves the file"
        );
        assert!(
            text_of("server@default.ini").contains("sk-local-3f9a"),
            "a refused read answered the file to a caller with no right"
        );
        assert_eq!(
            calls(&c),
            vec![
                Call::Start("default".into()),
                Call::Stop("default".into()),
                Call::Restart("default".into()),
                Call::SetBootEnabled("default".into(), true),
                Call::BootEnabled("default".into()),
            ],
            "a caller that is not the bff started nothing"
        );
        assert_eq!(
            polkit(&c),
            vec![],
            "the refusal is the principal gate's: not a polkit refusal, and never the caller's own call"
        );
        assert_eq!(
            asked_admin(&admins),
            asked,
            "no membership looked up for a caller that is not the bff"
        );
    }

    // ── The member's compare and swap write, vouched for by the bff ───────
    // `WriteFileIfAs` with the content the file holds lands as the member's
    // own write; the same call with a content the file no longer holds is
    // `Changed`, with the file left as the landed write left it. The counts
    // are relative: this phase's log already carries every call above.
    {
        let admins_before = asked_admin(&admins).len();
        let bff_before = bff_checks(&c).len();
        let base = text_of("models.ini");
        let next = format!("{base}[Qwen3.5-9B]\nmodel = /m/Qwen3.5-9B-Q4_K_M.gguf\n");
        call(
            &client,
            "WriteFileIfAs",
            &("alice", "models.ini", next.as_str(), base.as_str()),
        )
        .expect("a member's compare and swap write, holding the file's content");
        assert_eq!(
            text_of("models.ini"),
            next,
            "the member's fresh content landed"
        );
        let e = call(
            &client,
            "WriteFileIfAs",
            &("alice", "models.ini", base.as_str(), base.as_str()),
        )
        .expect_err("the same call with a content the file no longer holds");
        assert_eq!(
            e.0,
            format!("{PREFIX}Changed"),
            "the name of a stale content"
        );
        assert_eq!(
            text_of("models.ini"),
            next,
            "the refused write changed nothing on the way to the error"
        );
        assert_eq!(
            calls(&c),
            vec![
                Call::Start("default".into()),
                Call::Stop("default".into()),
                Call::Restart("default".into()),
                Call::SetBootEnabled("default".into(), true),
                Call::BootEnabled("default".into()),
                // IP3c: the accepted compare and swap answers the apply, whose drift
                // compare asks the state of `default`, the record's one instance.
                Call::Status("default".into()),
            ],
            "the file writes start, stop and restart no unit; the accepted one answers \
             the apply, whose drift compare asks the state"
        );
        assert_eq!(polkit(&c), vec![], "a principal call asks polkit nothing");
        assert_eq!(
            asked_admin(&admins).len(),
            admins_before + 2,
            "the membership of the vouched name, asked once per write"
        );
        assert_eq!(
            bff_checks(&c).len(),
            bff_before + 2,
            "the caller checked once per principal call"
        );
    }

    // ── The member's model profile assignment, vouched for by the bff ────
    // The `As` form is `(ssss)` with the principal first, as the design's
    // `SetModelProfileAs(s principal, s instance, s profile, s value)`: an
    // implementation taking the principal last would name an instance
    // "alice" here and fail.
    {
        let admins_before = asked_admin(&admins).len();
        let bff_before = bff_checks(&c).len();
        let calls_before = calls(&c).len();
        std::fs::write(sys.join("assignments.ini"), "[default]\n").expect("assignments");
        call(
            &client,
            "SetModelProfileAs",
            &("alice", "default", "Qwen3.8-27B", "on"),
        )
        .expect("SetModelProfileAs for a member");
        assert_eq!(
            text_of("assignments.ini"),
            "[default]\nQwen3.8-27B = on\n",
            "the member's assignment landed"
        );
        assert_eq!(
            polkit(&c),
            vec![],
            "the principal gate decides the right: polkit is asked nothing"
        );
        assert_eq!(
            calls(&c)[calls_before..],
            [Call::Status("default".into())],
            "IP3c: the assignment's answer asks the state of the drifting instance, and \
             starts, stops and restarts nothing"
        );
        let e = call(
            &client,
            "SetModelProfileAs",
            &("bob", "default", "Qwen3.8-27B", "off"),
        )
        .expect_err("SetModelProfileAs for a non-member");
        assert_eq!(
            e.0,
            format!("{PREFIX}NotAuthorized"),
            "SetModelProfileAs for bob"
        );
        assert_eq!(
            text_of("assignments.ini"),
            "[default]\nQwen3.8-27B = on\n",
            "a refused assignment writes nothing"
        );
        let e = call(
            &client,
            "SetModelProfileAs",
            &("alice", "default", "Qwen3.8-27B"),
        )
        .expect_err("the As form is four strings");
        assert!(
            !e.0.starts_with(PREFIX),
            "the shape is zbus's answer, not the manager's: {e:?}"
        );
        assert_eq!(
            asked_admin(&admins).len(),
            admins_before + 2,
            "the membership of the vouched name, asked once per call"
        );
        assert_eq!(
            bff_checks(&c).len(),
            bff_before + 2,
            "the caller checked once per principal call"
        );
        // A caller that is not the bff, naming a member: refused.
        let (_server, other) = connect(&c, Some(":1.42"));
        let e = call(
            &other,
            "SetModelProfileAs",
            &("alice", "default", "Qwen3.8-27B", "off"),
        )
        .expect_err("SetModelProfileAs from a caller that is not the bff");
        assert_eq!(e.0, format!("{PREFIX}NotAuthorized"));
        assert_eq!(
            text_of("assignments.ini"),
            "[default]\nQwen3.8-27B = on\n",
            "a caller that is not the bff writes nothing"
        );
    }

    // ── The member's tags of an instance, vouched by the bff ──────────────
    // `SetInstanceTagsAs(s principal, s instance, as tags)`: the tags verb
    // with a principal. The principal is the first argument: an
    // implementation taking them the other way answers InvalidId for
    // "alice". A member's call reaches the core's own checks; a non-member
    // is refused and writes nothing; a caller that is not the bff vouches
    // for nobody.
    {
        std::fs::write(
            sys.join("instances.ini"),
            "[default]\nServerProfile = default\n",
        )
        .expect("instances.ini");
        std::fs::write(sys.join("server@default.ini"), ini).expect("server@default.ini");
        let record_of = |name: &str| std::fs::read_to_string(sys.join(name)).expect(name);

        let r = call(
            &client,
            "SetInstanceTagsAs",
            &("alice", "default", vec!["edge".to_string()]),
        )
        .expect("SetInstanceTagsAs for a member");
        assert_eq!(
            r.body().signature().to_string(),
            String::new(),
            "the reply is the unit: ()"
        );
        assert_eq!(
            record_of("instances.ini"),
            "[default]\nServerProfile = default\nTags = edge\n",
            "a member's list lands"
        );

        let e = call(
            &client,
            "SetInstanceTagsAs",
            &("bob", "default", vec!["gpu".to_string()]),
        )
        .expect_err("SetInstanceTagsAs for a non-member");
        assert_eq!(e.0, format!("{PREFIX}NotAuthorized"), "{e:?}");
        assert_eq!(
            record_of("instances.ini"),
            "[default]\nServerProfile = default\nTags = edge\n",
            "the refused call wrote nothing"
        );

        // The `As` spelling is the only one carrying a principal: the plain
        // verb takes two arguments, `(sas)`.
        let e = call(
            &client,
            "SetInstanceTags",
            &("alice", "default", vec!["edge".to_string()]),
        )
        .expect_err("the plain verb with three arguments");
        assert!(
            !e.0.starts_with("org.privateai.Manager1."),
            "not the manager's error: {e:?}"
        );

        // The record as the phase's fixture has it, for the blocks after.
        std::fs::write(
            sys.join("instances.ini"),
            "[default]\nServerProfile = default\n",
        )
        .expect("restore the record");
    }

    // ── The member's label of an instance, vouched by the bff ─────────────
    // `SetInstanceLabelAs(s principal, s instance, s label)`: the principal
    // first, as every `As` method has it. An implementation taking the
    // principal last reads "alice" as the instance id and answers InvalidId, so
    // the shape is pinned. A member's text reaches the core's own rule; a
    // non-member is refused and writes nothing.
    {
        std::fs::write(
            sys.join("instances.ini"),
            "[default]\nServerProfile = default\n\n[gpu-1]\nServerProfile = gpu-1\nLabel = GPU box\n",
        )
        .expect("instances.ini");
        std::fs::write(sys.join("server@default.ini"), ini).expect("server@default.ini");
        let record_of = |name: &str| std::fs::read_to_string(sys.join(name)).expect(name);

        let r = call(
            &client,
            "SetInstanceLabelAs",
            &("alice", "default", "  Coding box "),
        )
        .expect("SetInstanceLabelAs for a member");
        assert_eq!(
            r.body().signature().to_string(),
            String::new(),
            "the reply is the unit: ()"
        );
        assert_eq!(
            record_of("instances.ini"),
            "[default]\nServerProfile = default\nLabel = Coding box\n\n[gpu-1]\nServerProfile = gpu-1\nLabel = GPU box\n",
            "a member's label lands, trimmed, and the other section is kept"
        );

        let e = call(
            &client,
            "SetInstanceLabelAs",
            &("bob", "default", "Office box"),
        )
        .expect_err("SetInstanceLabelAs for a non-member");
        assert_eq!(e.0, format!("{PREFIX}NotAuthorized"), "{e:?}");

        // The rule runs for the vouched caller as it does for a plain one: a
        // label the node already has, ignoring case, is refused.
        let e = call(
            &client,
            "SetInstanceLabelAs",
            &("alice", "default", "GPU BOX"),
        )
        .expect_err("a label gpu-1 already carries");
        assert_eq!(e.0, format!("{PREFIX}Invalid"), "{e:?}");
        assert!(
            e.1.contains("instance-label-taken"),
            "the refusal key travels in the message: {e:?}"
        );
        assert_eq!(
            record_of("instances.ini"),
            "[default]\nServerProfile = default\nLabel = Coding box\n\n[gpu-1]\nServerProfile = gpu-1\nLabel = GPU box\n",
            "the refused calls wrote nothing"
        );

        // The `As` spelling is the only one carrying a principal: the plain
        // verb takes two arguments, `(ss)`.
        let e = call(
            &client,
            "SetInstanceLabel",
            &("alice", "default", "Coding box"),
        )
        .expect_err("the plain verb with three arguments");
        assert!(
            !e.0.starts_with("org.privateai.Manager1."),
            "not the manager's error: {e:?}"
        );

        // The record as the phase's fixture has it, for the blocks after.
        std::fs::write(
            sys.join("instances.ini"),
            "[default]\nServerProfile = default\n",
        )
        .expect("restore the record");
    }

    // ── The member's server profile of an instance, vouched by the bff ────
    // The `As` form is `(sss)` with the principal first, as the design's
    // `SetServerProfileAs(s principal, s instance, s server profile)`: an
    // implementation taking the principal last would name an instance
    // "alice" here and fail.
    {
        let admins_before = asked_admin(&admins).len();
        let bff_before = bff_checks(&c).len();
        let polkit_before = polkit(&c).len();
        let calls_before = calls(&c).len();
        std::fs::write(
            sys.join("server@alt.ini"),
            "[Server]\nPort = 8080\nHostname = 127.0.0.1\n",
        )
        .expect("alt profile");
        call(&client, "SetServerProfileAs", &("alice", "default", "alt"))
            .expect("SetServerProfileAs for a member");
        assert_eq!(
            text_of("instances.ini"),
            "[default]\nServerProfile = alt\n",
            "the member's switch landed"
        );
        assert_eq!(
            polkit(&c).len(),
            polkit_before,
            "the principal gate decides the right: polkit is asked nothing"
        );
        assert_eq!(
            calls(&c)[calls_before..],
            [Call::Status("default".into())],
            "IP3c: the switch's answer asks the state of the instance whose launch the \
             switch moves; starts, stops and restarts nothing"
        );
        let e = call(
            &client,
            "SetServerProfileAs",
            &("bob", "default", "default"),
        )
        .expect_err("SetServerProfileAs for a non-member");
        assert_eq!(
            e.0,
            format!("{PREFIX}NotAuthorized"),
            "SetServerProfileAs for bob"
        );
        assert_eq!(
            text_of("instances.ini"),
            "[default]\nServerProfile = alt\n",
            "a refused switch writes nothing"
        );
        let e = call(&client, "SetServerProfileAs", &("alice", "default"))
            .expect_err("the As form is three strings");
        assert!(
            !e.0.starts_with(PREFIX),
            "the shape is zbus's answer, not the manager's: {e:?}"
        );
        assert_eq!(
            asked_admin(&admins).len(),
            admins_before + 2,
            "the membership of the vouched name, asked once per call"
        );
        assert_eq!(
            bff_checks(&c).len(),
            bff_before + 2,
            "the caller checked once per principal call"
        );
        // A caller that is not the bff, naming a member: refused.
        let (_server, other) = connect(&c, Some(":1.42"));
        let e = call(
            &other,
            "SetServerProfileAs",
            &("alice", "default", "default"),
        )
        .expect_err("SetServerProfileAs from a caller that is not the bff");
        assert_eq!(e.0, format!("{PREFIX}NotAuthorized"));
        assert_eq!(
            text_of("instances.ini"),
            "[default]\nServerProfile = alt\n",
            "a caller that is not the bff writes nothing"
        );
        std::fs::remove_file(sys.join("server@alt.ini")).expect("remove alt");
        std::fs::write(
            sys.join("instances.ini"),
            "[default]\nServerProfile = default\n",
        )
        .expect("restore the record");
    }
}

/// The node's server profiles over D-Bus (slice 14): the signatures of the three
/// verbs and their `As` forms, and the answers on the wire. `CreateServerProfile`
/// is `(ssq)` and its reply is the unit; a name that is anything already answers
/// the new `ServerProfileExists` with the name, and a rule's refusal carries its
/// encoded key, as a write's does. `DeleteServerProfile` is `(ss)`, its in-use
/// refusal the profile and the instance split by the separator. `ServerProfiles`
/// answers `a(ssqass)`, sorted, to a caller with no sender. A body that is not
/// the signature is zbus's own error, never one of the manager's, and writes
/// nothing.
#[cfg(test)]
pub(crate) fn manager_dbus_server_profiles_e2e() {
    use super::service::fakes::{FakeAuthority, FakeGroup, FakeResolver, FakeService};
    use pai_core::checks::Refusal;
    use std::os::unix::net::UnixStream;
    use std::time::Duration;
    use zbus::blocking::connection::Builder;
    use zbus::blocking::Connection;

    type Core = Manager<FakeService, FakeAuthority, FakeResolver>;

    let dir = tempfile::tempdir().expect("tempdir");
    crate::paths::redirect_for_tests(dir.path());
    let sys = crate::paths::engine_config_dir();
    std::fs::create_dir_all(&sys).expect("system config dir");
    let default_ini = "[Server]\nPort = 8080\nHostname = 127.0.0.1\nTags = edge,v2\n";
    std::fs::write(sys.join("server@default.ini"), default_ini).expect("default profile");
    std::fs::write(
        sys.join("server@spare.ini"),
        "[Server]\nHostname = 127.0.0.1\n",
    )
    .expect("spare profile");
    std::fs::write(sys.join("server@X.ini"), "[Server]\nPort = 1\n").expect("bad name");
    std::fs::write(
        sys.join("instances.ini"),
        "[default]\nServerProfile = default\n",
    )
    .expect("record");

    // Serve `core` peer to peer on a socket pair, the client stamping `sender`
    // when given: the bus would stamp it.
    let connect = |core: &Arc<Core>, sender: Option<&str>| -> (Connection, Connection) {
        let (s0, s1) = UnixStream::pair().expect("socket pair");
        let iface = ManagerInterface::new(core.clone());
        let server = std::thread::spawn(move || {
            Builder::async_io_unix_stream(s0)
                .server(zbus::Guid::generate())
                .expect("server guid")
                .p2p()
                .serve_at(OBJECT_PATH, iface)
                .expect("serve_at")
                .build()
                .expect("server connection")
        });
        let client = Builder::async_io_unix_stream(s1)
            .p2p()
            .method_timeout(Duration::from_secs(10))
            .build()
            .expect("client connection");
        if let Some(name) = sender {
            client.inner().set_unique_name(name).expect("unique name");
        }
        (server.join().expect("server thread"), client)
    };
    let core = || -> Arc<Core> {
        Arc::new(
            Manager::new(
                FakeService::new(),
                FakeAuthority::new(true),
                FakeResolver::lan(),
            )
            .with_group(FakeGroup::new()),
        )
    };
    // A method call: Ok(reply), or Err((error name, message)).
    fn call<B>(conn: &Connection, method: &str, body: &B) -> Result<zbus::Message, (String, String)>
    where
        B: serde::ser::Serialize + zbus::zvariant::DynamicType,
    {
        conn.call_method(None::<&str>, OBJECT_PATH, Some(INTERFACE), method, body)
            .map_err(|e| match e {
                zbus::Error::MethodError(name, detail, _) => {
                    (name.to_string(), detail.unwrap_or_default())
                }
                zbus::Error::FDO(e) => {
                    use zbus::DBusError;
                    (
                        e.name().to_string(),
                        e.description().unwrap_or_default().to_string(),
                    )
                }
                other => panic!("{method}: not a method error reply: {other:?}"),
            })
    }
    fn body<T: for<'de> zbus::zvariant::DynamicDeserialize<'de>>(m: &zbus::Message) -> T {
        m.body().deserialize().expect("reply body")
    }
    let err_name = |kind: &str| format!("org.privateai.Manager1.Error.{kind}");
    let refusal = |key: &'static str, values: &[&str]| Refusal {
        key,
        values: values.iter().map(|v| v.to_string()).collect(),
    };
    let profile_text = |name: &str| {
        std::fs::read_to_string(sys.join(format!("server@{name}.ini"))).expect("profile")
    };
    // A key of the `[Server]` section, counting only real keys: the render
    // leaves a commented hint line for a key the file does not set.
    let key_line = |text: &str, key: &str| -> bool {
        text.lines()
            .any(|line| line.trim_start().starts_with(&format!("{key} ")))
    };
    let listing = || -> Vec<String> {
        let mut names: Vec<String> = std::fs::read_dir(&sys)
            .map(|rd| {
                rd.flatten()
                    .map(|e| e.file_name().to_string_lossy().into_owned())
                    .collect()
            })
            .unwrap_or_default();
        names.sort();
        names
    };

    // ── A caller with no sender: the verbs refused, the list answered ─────
    {
        let c = core();
        let (_server, client) = connect(&c, None);
        let e = call(
            &client,
            "CreateServerProfile",
            &("llama.cpp", "db-1", 8123u16),
        )
        .expect_err("CreateServerProfile");
        assert_eq!(e.0, err_name("NotAuthorized"), "a create with no sender");
        let e = call(&client, "DeleteServerProfile", &("llama.cpp", "spare"))
            .expect_err("DeleteServerProfile");
        assert_eq!(e.0, err_name("NotAuthorized"), "a delete with no sender");
        assert!(
            !sys.join("server@db-1.ini").exists(),
            "the refused create wrote nothing"
        );
        assert!(
            sys.join("server@spare.ini").exists(),
            "the refused delete removed nothing"
        );

        let r = call(&client, "ServerProfiles", &()).expect("ServerProfiles with no sender");
        assert_eq!(
            r.body().signature().to_string(),
            "a(ssqass)",
            "an array of a struct of the id, the engine, a uint16, the tags and the instance"
        );
        assert_eq!(
            body::<Vec<(String, String, u16, Vec<String>, String)>>(&r),
            vec![
                (
                    "default".to_string(),
                    "llama.cpp".to_string(),
                    8080u16,
                    vec!["edge".to_string(), "v2".to_string()],
                    "default".to_string(),
                ),
                (
                    "spare".to_string(),
                    "llama.cpp".to_string(),
                    u16::try_from(pai_core::server_cfg::DEFAULT_PORT).expect("DEFAULT_PORT"),
                    Vec::new(),
                    String::new(),
                ),
            ],
            "sorted, the empty string for a profile no section names, and no row for \
             `server@X.ini`: a name the engine refuses is no profile"
        );
    }

    // ── An authorized caller: the verbs, their answers and their signatures ─
    {
        let c = core();
        let (_server, client) = connect(&c, Some(":1.42"));

        let r = call(
            &client,
            "CreateServerProfile",
            &("llama.cpp", "db-1", 8123u16),
        )
        .expect("CreateServerProfile of a new name");
        assert_eq!(
            r.body().signature().to_string(),
            String::new(),
            "the reply is the unit: ()"
        );
        assert!(
            key_line(&profile_text("db-1"), "Port") && profile_text("db-1").contains("Port = 8123"),
            "the port of the call, in the file: {}",
            profile_text("db-1")
        );
        call(
            &client,
            "CreateServerProfile",
            &("llama.cpp", "zero-d", 0u16),
        )
        .expect("port 0");
        assert!(
            !key_line(&profile_text("zero-d"), "Port"),
            "port 0 on the wire leaves no Port key at all: {}",
            profile_text("zero-d")
        );

        let e = call(
            &client,
            "CreateServerProfile",
            &("llama.cpp", "db-1", 9001u16),
        )
        .expect_err("a second create of the same name");
        assert_eq!(e.0, err_name("ServerProfileExists"), "{e:?}");
        assert_eq!(e.1, "db-1", "the message names the profile");
        assert!(
            profile_text("db-1").contains("Port = 8123"),
            "and the file kept its bytes"
        );

        let e = call(&client, "CreateServerProfile", &("vllm", "db-2", 8123u16))
            .expect_err("an engine the manager does not serve");
        assert_eq!(e.0, err_name("Invalid"), "the engine, as a check's refusal");
        assert_eq!(
            Refusal::decode(&e.1),
            Some(refusal("unknown-engine", &["vllm"])),
            "{:?}",
            e.1
        );
        for name in ["../x", "X", "a_b", ""] {
            let e = call(
                &client,
                "CreateServerProfile",
                &("llama.cpp", name, 8123u16),
            )
            .expect_err("a name that is no profile");
            assert_eq!(e.0, err_name("Invalid"), "{name:?}");
            assert_eq!(
                Refusal::decode(&e.1),
                Some(refusal("server-profile-bad-id", &[name])),
                "{name:?}: {:?}",
                e.1
            );
        }
        assert!(
            !sys.join("server@db-2.ini").exists() && !sys.join("x.ini").exists(),
            "a refused create created nothing, and no path outside the engine's directory"
        );

        // The delete: in use, missing, unused.
        let e = call(&client, "DeleteServerProfile", &("llama.cpp", "default"))
            .expect_err("a profile a section names");
        assert_eq!(e.0, err_name("ServerProfileInUse"), "{e:?}");
        assert_eq!(
            e.1, "default\u{1f}default",
            "the profile and the instance, split by the separator Refusal::encode uses"
        );
        assert!(
            sys.join("server@default.ini").is_file(),
            "the file of an in-use profile is kept"
        );
        let e = call(&client, "DeleteServerProfile", &("llama.cpp", "nope"))
            .expect_err("a profile that is no file");
        assert_eq!(e.0, err_name("NoServerProfile"), "{e:?}");
        assert_eq!(e.1, "nope", "the message names the profile");
        call(&client, "DeleteServerProfile", &("llama.cpp", "db-1"))
            .expect("the delete of an unused profile");
        assert!(
            !sys.join("server@db-1.ini").exists(),
            "its file is gone: the create and the delete round-trip over the bus"
        );

        // The `As` forms are there, with the principal first, and gated: this
        // caller is not the bff's account.
        let e = call(
            &client,
            "CreateServerProfileAs",
            &("alice", "llama.cpp", "db-as", 8123u16),
        )
        .expect_err("CreateServerProfileAs from a caller that is not the bff");
        assert_eq!(e.0, err_name("NotAuthorized"), "{e:?}");
        let e = call(
            &client,
            "DeleteServerProfileAs",
            &("alice", "llama.cpp", "spare"),
        )
        .expect_err("DeleteServerProfileAs from a caller that is not the bff");
        assert_eq!(e.0, err_name("NotAuthorized"), "{e:?}");
        assert!(
            !sys.join("server@db-as.ini").exists() && sys.join("server@spare.ini").is_file(),
            "a refused `As` form writes and removes nothing"
        );

        // The signatures: no less, no more, and of the agreed types. zbus
        // answers a shape mismatch with its own error, never one of the
        // manager's.
        let before = listing();
        for e in [
            call(&client, "CreateServerProfile", &("llama.cpp", "db-3")).expect_err("one string"),
            call(
                &client,
                "CreateServerProfile",
                &("llama.cpp", "db-3", "8080"),
            )
            .expect_err("a string where a uint16 belongs"),
            call(
                &client,
                "CreateServerProfile",
                &("llama.cpp", "db-3", 8123u16, "more"),
            )
            .expect_err("four arguments"),
            call(&client, "DeleteServerProfile", &("llama.cpp",)).expect_err("one argument"),
            call(
                &client,
                "DeleteServerProfile",
                &("llama.cpp", "spare", "more"),
            )
            .expect_err("three arguments"),
            // `ServerProfiles` takes nothing, so an extra argument is no rule of
            // ours: zbus answers a 0-parameter method with an argument of its own
            // accord. The shape the design fixes is pinned where the list is
            // answered: a call of no argument, answering `a(ssqass)`.
            call(
                &client,
                "CreateServerProfileAs",
                &("alice", "llama.cpp", "db-3"),
            )
            .expect_err("the port missing"),
            call(
                &client,
                "DeleteServerProfileAs",
                &("alice", "llama.cpp", "spare", "more"),
            )
            .expect_err("four arguments"),
        ] {
            assert!(
                !e.0.starts_with("org.privateai.Manager1."),
                "not the manager's error: {e:?}"
            );
        }
        assert_eq!(listing(), before, "a call of the wrong shape wrote nothing");
    }
}

/// Creating and removing an instance over D-Bus (slice 16):
/// `CreateInstance(engine, id, serverProfile)` is `(sss)` and its reply the
/// unit, `RemoveInstance(id)` is `(s)` and its reply the unit, and their
/// `As` forms carry the principal first: `(ssss)` and `(ss)`. A rule's
/// refusal crosses as the keyed `Invalid` refusal, decoded to the key and
/// the id (`instance-bad-id`, `instance-exists`, `instance-protected`,
/// `instance-running`, `instance-no-model-profiles`); the older names keep
/// their own wire form (`NoServerProfile`, `ServerProfileInUse` with the
/// separator, `AddressInUse`, `NoInstance`, `InvalidId`). A body that is
/// not the signature is zbus's own error and writes nothing, and a caller
/// with no sender is refused.
#[cfg(test)]
pub(crate) fn manager_dbus_instance_crud_e2e() {
    use super::service::fakes::{
        FakeActionLog, FakeAuthority, FakeGroup, FakePasswords, FakeResolver, FakeService,
    };
    use pai_core::checks::Refusal;
    use std::os::unix::net::UnixStream;
    use std::time::Duration;
    use zbus::blocking::connection::Builder;
    use zbus::blocking::Connection;

    type Core = Manager<FakeService, FakeAuthority, FakeResolver>;

    let dir = tempfile::tempdir().expect("tempdir");
    crate::paths::redirect_for_tests(dir.path());
    let sys = crate::paths::engine_config_dir();
    std::fs::create_dir_all(&sys).expect("engine config dir");
    std::fs::write(
        sys.join("instances.ini"),
        "[default]\nServerProfile = default\n\n[lan-2]\nServerProfile = lan-2\n",
    )
    .expect("record");
    for (name, port) in [
        ("default", 8080),
        ("lan-2", 8081),
        ("free", 8123),
        ("dupe", 8081),
    ] {
        std::fs::write(
            sys.join(format!("server@{name}.ini")),
            format!("[Server]\nPort = {port}\nHostname = 127.0.0.1\n"),
        )
        .expect("profile");
    }
    std::fs::write(
        sys.join("server@badhost.ini"),
        "[Server]\nPort = 8124\nHostname = nowhere.invalid\n",
    )
    .expect("profile");
    std::fs::write(
        sys.join("models.ini"),
        "[*]\non = true\n\n[qwen]\nmodel = qwen.gguf\n",
    )
    .expect("pool");
    for id in ["default", "lan-2"] {
        std::fs::write(
            sys.join(format!("models@{id}.ini")),
            "[*]\non = true\n\n[qwen]\nmodel = qwen.gguf\n",
        )
        .expect("render");
    }

    let connect = |core: &Arc<Core>, sender: Option<&str>| -> (Connection, Connection) {
        let (s0, s1) = UnixStream::pair().expect("socket pair");
        let iface = ManagerInterface::new(core.clone());
        let server = std::thread::spawn(move || {
            Builder::async_io_unix_stream(s0)
                .server(zbus::Guid::generate())
                .expect("server guid")
                .p2p()
                .serve_at(OBJECT_PATH, iface)
                .expect("serve_at")
                .build()
                .expect("server connection")
        });
        let client = Builder::async_io_unix_stream(s1)
            .p2p()
            .method_timeout(Duration::from_secs(10))
            .build()
            .expect("client connection");
        if let Some(name) = sender {
            client.inner().set_unique_name(name).expect("unique name");
        }
        (server.join().expect("server thread"), client)
    };
    // The GPU pool the render's memory check measures against: a create renders
    // the new instance, and a check that measures a card pool probes the engine.
    // The cards are roomy: these phases are about the verbs, not about memory.
    let gpus = || crate::manager::service::fakes::FakeGpus::new(&[("CUDA0", 400_000)]);
    let core = || -> Arc<Core> {
        Arc::new(
            Manager::new(
                // The units seen stopped: a remove refuses a running instance,
                // and these phases are about the verbs, not about the state.
                FakeService::new().stopped(),
                FakeAuthority::new(true),
                FakeResolver::lan(),
            )
            .with_group(FakeGroup::new())
            .with_gpus(gpus()),
        )
    };
    // The bff's own account, vouching for a member of `pai`.
    let core_bff = || -> Arc<Core> {
        Arc::new(
            Manager::new(
                FakeService::new().stopped(),
                FakeAuthority::new(true).bff(&Caller(":1.9".into())),
                FakeResolver::lan(),
            )
            .with_group(FakeGroup::new())
            .with_gpus(gpus())
            .with_passwords(FakePasswords::new())
            .with_log(FakeActionLog::new()),
        )
    };
    fn call<B>(conn: &Connection, method: &str, body: &B) -> Result<zbus::Message, (String, String)>
    where
        B: serde::ser::Serialize + zbus::zvariant::DynamicType,
    {
        conn.call_method(None::<&str>, OBJECT_PATH, Some(INTERFACE), method, body)
            .map_err(|e| match e {
                zbus::Error::MethodError(name, detail, _) => {
                    (name.to_string(), detail.unwrap_or_default())
                }
                zbus::Error::FDO(e) => {
                    use zbus::DBusError;
                    (
                        e.name().to_string(),
                        e.description().unwrap_or_default().to_string(),
                    )
                }
                other => panic!("{method}: not a method error reply: {other:?}"),
            })
    }
    let err_name = |kind: &str| format!("org.privateai.Manager1.Error.{kind}");
    let refusal = |key: &'static str, values: &[&str]| Refusal {
        key,
        values: values.iter().map(|v| v.to_string()).collect(),
    };
    let record = || std::fs::read_to_string(sys.join("instances.ini")).expect("record");
    let listing = || -> Vec<String> {
        let mut names: Vec<String> = std::fs::read_dir(&sys)
            .map(|rd| {
                rd.flatten()
                    .map(|e| e.file_name().to_string_lossy().into_owned())
                    .collect()
            })
            .unwrap_or_default();
        names.sort();
        names
    };

    // ── A caller with no sender ───────────────────────────────────────
    {
        let c = core();
        let (_server, client) = connect(&c, None);
        let e = call(&client, "CreateInstance", &("llama.cpp", "db-1", "free"))
            .expect_err("CreateInstance with no sender");
        assert_eq!(e.0, err_name("NotAuthorized"), "{e:?}");
        let e = call(&client, "RemoveInstance", &("lan-2",)).expect_err("no sender");
        assert_eq!(e.0, err_name("NotAuthorized"), "{e:?}");
        assert!(
            record().contains("[lan-2]") && !sys.join("models@db-1.ini").exists(),
            "a caller with no sender wrote and removed nothing"
        );
    }

    // ── An authorized caller: the verbs, their answers, their shapes ───
    {
        let c = core();
        let (_server, client) = connect(&c, Some(":1.42"));

        let r = call(&client, "CreateInstance", &("llama.cpp", "db-1", "free"))
            .expect("CreateInstance of a new id");
        assert_eq!(
            r.body().signature().to_string(),
            String::new(),
            "the reply is the unit: ()"
        );
        assert_eq!(
            record(),
            "[default]\nServerProfile = default\n\n[lan-2]\nServerProfile = lan-2\n\
             [db-1]\nServerProfile = free\nPort = 8581\n",
            "the record gained the section naming the profile and carrying the \
             port the create proposed (IP3a: the first free one at or above 8581, \
             this node's instances sitting on 8080 and 8081 and the web service on \
             8590)"
        );
        assert!(
            sys.join("models@db-1.ini").is_file(),
            "and the render the launch reads"
        );

        for (body, kind, want, why) in [
            (
                ("vllm", "db-2", "free"),
                "Invalid",
                Some(refusal("unknown-engine", &["vllm"])),
                "an engine the manager does not serve",
            ),
            (
                ("llama.cpp", "lan-2", "free"),
                "Invalid",
                Some(refusal("instance-exists", &["lan-2"])),
                "an id the record has",
            ),
            (
                ("llama.cpp", "../x", "free"),
                "Invalid",
                Some(refusal("instance-bad-id", &["../x"])),
                "an id the rule refuses",
            ),
            (
                ("llama.cpp", "A", "free"),
                "Invalid",
                Some(refusal("instance-bad-id", &["A"])),
                "an upper case id",
            ),
            (
                ("llama.cpp", "db-3", "gone"),
                "NoServerProfile",
                None,
                "no profile of that name",
            ),
            (
                ("llama.cpp", "db-3", "lan-2"),
                "ServerProfileInUse",
                None,
                "the profile an instance runs",
            ),
            (
                ("llama.cpp", "db-3", "dupe"),
                "AddressInUse",
                None,
                "the address of another instance",
            ),
            (
                ("llama.cpp", "db-3", "badhost"),
                "UnresolvableHost",
                None,
                "a host that does not resolve",
            ),
        ] {
            let e = call(&client, "CreateInstance", &body).expect_err(why);
            assert_eq!(e.0, err_name(kind), "{why}: {e:?}");
            if let Some(want) = want {
                assert_eq!(
                    Refusal::decode(&e.1),
                    Some(want),
                    "{why}: the refusal crosses encoded: {:?}",
                    e.1
                );
            }
        }
        assert_eq!(
            record(),
            "[default]\nServerProfile = default\n\n[lan-2]\nServerProfile = lan-2\n\
             [db-1]\nServerProfile = free\nPort = 8581\n",
            "every refused create left the record as it was, the proposed port \
             included"
        );
        assert!(
            !sys.join("models@db-2.ini").exists() && !sys.join("models@db-3.ini").exists(),
            "and wrote no render"
        );

        // The remove, of a unit seen stopped.
        let r = call(&client, "RemoveInstance", &("db-1",)).expect("RemoveInstance");
        assert_eq!(
            r.body().signature().to_string(),
            String::new(),
            "the reply is the unit: ()"
        );
        assert!(
            !record().contains("[db-1]") && !sys.join("models@db-1.ini").exists(),
            "the section and the render are gone"
        );
        for (id, kind, want, why) in [
            (
                "default",
                "Invalid",
                Some(refusal("instance-protected", &["default"])),
                "the protected id",
            ),
            ("nope", "NoInstance", None, "an id the record has not"),
            ("../x", "InvalidId", None, "an id the rule refuses"),
        ] {
            let e = call(&client, "RemoveInstance", &(id,)).expect_err(why);
            assert_eq!(e.0, err_name(kind), "{why}: {e:?}");
            if let Some(want) = want {
                assert_eq!(Refusal::decode(&e.1), Some(want), "{why}: {:?}", e.1);
            }
        }
        assert!(
            record().contains("[default]") && record().contains("[lan-2]"),
            "a refused remove removed no section"
        );

        // The `As` forms are there, with the principal first, and gated:
        // this caller is not the bff's account.
        let e = call(
            &client,
            "CreateInstanceAs",
            &("alice", "llama.cpp", "db-as", "free"),
        )
        .expect_err("CreateInstanceAs from a caller that is not the bff");
        assert_eq!(e.0, err_name("NotAuthorized"), "{e:?}");
        let e = call(&client, "RemoveInstanceAs", &("alice", "lan-2"))
            .expect_err("RemoveInstanceAs from a caller that is not the bff");
        assert_eq!(e.0, err_name("NotAuthorized"), "{e:?}");
        assert!(
            !sys.join("models@db-as.ini").exists() && record().contains("[lan-2]"),
            "a refused `As` form wrote and removed nothing"
        );

        // The signatures: no less, no more, and of the agreed types.
        let before = listing();
        for e in [
            call(&client, "CreateInstance", &("llama.cpp", "db-9")).expect_err("a body of (ss)"),
            call(
                &client,
                "CreateInstance",
                &("llama.cpp", "db-9", "free", "more"),
            )
            .expect_err("a body of (ssss)"),
            call(&client, "CreateInstance", &(8080u16, "db-9", "free"))
                .expect_err("a uint16 where the engine belongs"),
            call(&client, "RemoveInstance", &()).expect_err("no argument"),
            call(&client, "RemoveInstance", &("lan-2", "more")).expect_err("a body of (ss)"),
            call(&client, "CreateInstanceAs", &("alice", "llama.cpp", "db-9"))
                .expect_err("the profile missing"),
            call(&client, "RemoveInstanceAs", &("alice",)).expect_err("the id missing"),
        ] {
            assert!(
                !e.0.starts_with("org.privateai.Manager1."),
                "not the manager's error: {e:?}"
            );
        }
        assert_eq!(listing(), before, "a call of the wrong shape wrote nothing");
    }

    // ── The state is asked first ──────────────────────────────────────
    {
        let mut service = FakeService::new();
        service.state = super::service::UnitState::Active;
        let c = Arc::new(
            Manager::new(service, FakeAuthority::new(true), FakeResolver::lan())
                .with_group(FakeGroup::new()),
        );
        let (_server, client) = connect(&c, Some(":1.42"));
        let e = call(&client, "RemoveInstance", &("lan-2",)).expect_err("a running instance");
        assert_eq!(e.0, err_name("Invalid"), "{e:?}");
        assert_eq!(
            Refusal::decode(&e.1),
            Some(refusal("instance-running", &["lan-2"])),
            "the running instance's refusal: {:?}",
            e.1
        );
        assert!(
            record().contains("[lan-2]") && sys.join("models@lan-2.ini").is_file(),
            "nothing removed"
        );
    }

    // ── The `As` forms, answered to the bff's account ─────────────────
    {
        let c = core_bff();
        let (_server, client) = connect(&c, Some(":1.9"));
        call(
            &client,
            "CreateInstanceAs",
            &("alice", "llama.cpp", "db-as", "free"),
        )
        .expect("CreateInstanceAs for a member of pai");
        assert!(
            record().contains("[db-as]") && sys.join("models@db-as.ini").is_file(),
            "the create for the principal wrote"
        );
        call(&client, "RemoveInstanceAs", &("alice", "db-as"))
            .expect("RemoveInstanceAs for a member of pai");
        assert!(
            !record().contains("[db-as]") && !sys.join("models@db-as.ini").exists(),
            "and its remove, for the principal, undid it"
        );
        let e = call(
            &client,
            "CreateInstanceAs",
            &("bob", "llama.cpp", "db-as", "free"),
        )
        .expect_err("a principal outside pai");
        assert_eq!(e.0, err_name("NotAuthorized"), "{e:?}");
        let e = call(&client, "RemoveInstanceAs", &("bob", "lan-2"))
            .expect_err("a principal outside pai");
        assert_eq!(e.0, err_name("NotAuthorized"), "{e:?}");
        assert!(
            !sys.join("models@db-as.ini").exists() && record().contains("[lan-2]"),
            "and nothing was written for them"
        );
    }
}

/// The instance profile verbs over D-Bus (IP3a, `ARCHITECTURE.md` "Instance
/// profiles", slice IP3 part a): the wire shapes, the answers and the bytes they
/// leave. `InstanceProfiles` is `()` answering `a(ssas)`: the id, the server
/// profile and the entries, in the file's order, and no revision (the pool is
/// one text, so only one read carries it). `GetInstanceProfile` is `(s)`
/// answering `(sass)`: the server profile, the entries and the revision, the
/// text of `instance-profiles.ini` the read returned. `CreateInstanceProfile` is
/// `(ss)` (the id and the server profile it runs), `AddInstanceProfileModel` and
/// `RemoveInstanceProfileModel` are `(ss)`, `DeleteInstanceProfile` is `(s)`,
/// `SetInstanceProfile` is `(sss)` (the instance, the profile, the revision) and
/// `SetInstanceAddress` is `(ssss)` (the instance, the host, the port as text,
/// the revision): every write's reply is the unit. Their `As` forms carry the
/// principal first and are answered only to the bff's account. The refusals on
/// the wire: `InstanceProfileExists` with the name, `InstanceProfileInUse` with
/// the profile and the instance split by the separator, `NoInstanceProfile` with
/// the name, `NoServerProfile`, `NoInstance`, `Changed`, `AddressInUse` with who
/// holds the address, and `Invalid` with the encoded key
/// (`instance-profile-bad-id`, `no-model-profile-named`,
/// `not-whole-number-in-range`). A body that is not the signature is zbus's own
/// error and writes nothing, and a caller with no sender is refused.
#[cfg(test)]
pub(crate) fn manager_dbus_ip3a_instance_profiles_e2e() {
    use super::service::fakes::{
        FakeActionLog, FakeAuthority, FakeGroup, FakePasswords, FakeResolver, FakeService,
    };
    use super::service::real_model_files;
    use pai_core::checks::Refusal;
    use std::os::unix::net::UnixStream;
    use std::time::Duration;
    use zbus::blocking::connection::Builder;
    use zbus::blocking::Connection;

    type Core = Manager<FakeService, FakeAuthority, FakeResolver>;

    let dir = tempfile::tempdir().expect("tempdir");
    crate::paths::redirect_for_tests(dir.path());
    let sys = crate::paths::engine_config_dir();
    std::fs::create_dir_all(&sys).expect("engine config dir");
    let pool_seed = "[default]\nServerProfile = default\n\n[default/alpha]\ndevice = cuda0\n\n[lab]\nServerProfile = lab\n\n[lab/alpha]\n\n[unassigned]\nServerProfile = default\n";
    let record_seed = "[default]\nInstanceProfile = default\nHostname = 127.0.0.1\nPort = 8581\n\n[lab]\nInstanceProfile = lab\nHostname = 0.0.0.0\nPort = 8681\n";
    std::fs::write(sys.join("instance-profiles.ini"), pool_seed).expect("pool");
    std::fs::write(sys.join("instances.ini"), record_seed).expect("record");
    std::fs::write(
        sys.join("server@default.ini"),
        "[Server]\nPort = 8581\nHostname = 127.0.0.1\nModelsMax = 3\n",
    )
    .expect("server@default.ini");
    std::fs::write(
        sys.join("server@lab.ini"),
        "[Server]\nPort = 8681\nHostname = 0.0.0.0\n",
    )
    .expect("server@lab.ini");
    std::fs::write(
        sys.join("models.ini"),
        real_model_files(
            dir.path(),
            "[alpha]\nmodel = /models/alpha.gguf\n\n[beta]\nmodel = /models/beta.gguf\n\n[gamma]\nmodel = /models/gamma.gguf\n",
        )
    )
    .expect("models.ini");
    std::fs::write(sys.join("models.ini.disabled"), "").expect("models.ini.disabled");
    for id in ["default", "lab"] {
        std::fs::write(sys.join(format!("models@{id}.ini")), "").expect("render");
    }

    // Serve `core` peer to peer on a socket pair, the client stamping `sender`.
    let connect = |core: &Arc<Core>, sender: Option<&str>| -> (Connection, Connection) {
        let (s0, s1) = UnixStream::pair().expect("socket pair");
        let iface = ManagerInterface::new(core.clone());
        let server = std::thread::spawn(move || {
            Builder::async_io_unix_stream(s0)
                .server(zbus::Guid::generate())
                .expect("server guid")
                .p2p()
                .serve_at(OBJECT_PATH, iface)
                .expect("serve_at")
                .build()
                .expect("server connection")
        });
        let client = Builder::async_io_unix_stream(s1)
            .p2p()
            .method_timeout(Duration::from_secs(10))
            .build()
            .expect("client connection");
        if let Some(name) = sender {
            client.inner().set_unique_name(name).expect("unique name");
        }
        (server.join().expect("server thread"), client)
    };
    let core = || -> Arc<Core> {
        Arc::new(
            Manager::new(
                FakeService::new(),
                FakeAuthority::new(true),
                FakeResolver::lan(),
            )
            .with_group(FakeGroup::new()),
        )
    };
    // The bff's own account, vouching for a member of `pai`.
    let core_bff = || -> Arc<Core> {
        Arc::new(
            Manager::new(
                FakeService::new(),
                FakeAuthority::new(true).bff(&Caller(":1.9".into())),
                FakeResolver::lan(),
            )
            .with_group(FakeGroup::new())
            .with_passwords(FakePasswords::new())
            .with_log(FakeActionLog::new()),
        )
    };
    fn call<B>(conn: &Connection, method: &str, body: &B) -> Result<zbus::Message, (String, String)>
    where
        B: serde::ser::Serialize + zbus::zvariant::DynamicType,
    {
        conn.call_method(None::<&str>, OBJECT_PATH, Some(INTERFACE), method, body)
            .map_err(|e| match e {
                zbus::Error::MethodError(name, detail, _) => {
                    (name.to_string(), detail.unwrap_or_default())
                }
                zbus::Error::FDO(e) => {
                    use zbus::DBusError;
                    (
                        e.name().to_string(),
                        e.description().unwrap_or_default().to_string(),
                    )
                }
                other => panic!("{method}: not a method error reply: {other:?}"),
            })
    }
    fn body<T: for<'de> zbus::zvariant::DynamicDeserialize<'de>>(m: &zbus::Message) -> T {
        m.body().deserialize().expect("reply body")
    }
    let err_name = |kind: &str| format!("org.privateai.Manager1.Error.{kind}");
    let refusal = |key: &'static str, values: &[&str]| Refusal {
        key,
        values: values.iter().map(|v| v.to_string()).collect(),
    };
    let pool = || std::fs::read_to_string(sys.join("instance-profiles.ini")).expect("pool");
    let record = || std::fs::read_to_string(sys.join("instances.ini")).expect("record");
    let key_of = |text: &str, section: &str, key: &str| -> Option<String> {
        pai_core::ini::section_keys(text, section)
            .into_iter()
            .find(|(k, _)| k == key)
            .map(|(_, v)| v)
    };
    let sections = |text: &str| -> Vec<String> {
        pai_core::ini::parse_sections(text)
            .into_iter()
            .map(|s| s.id)
            .collect()
    };
    type Rows = Vec<(String, String, Vec<String>)>;
    let rows = |c: &Connection| -> Rows {
        body::<Rows>(&call(c, "InstanceProfiles", &()).expect("InstanceProfiles"))
    };

    // ── A caller with no sender: every verb is a configure write ───────────
    {
        let c = core();
        let (_server, client) = connect(&c, None);
        for e in [
            call(&client, "InstanceProfiles", &()).expect_err("InstanceProfiles"),
            call(&client, "GetInstanceProfile", &("default",)).expect_err("GetInstanceProfile"),
            call(&client, "CreateInstanceProfile", &("edge", "lab"))
                .expect_err("CreateInstanceProfile"),
            call(&client, "DeleteInstanceProfile", &("edge",)).expect_err("DeleteInstanceProfile"),
            call(&client, "AddInstanceProfileModel", &("default", "beta"))
                .expect_err("AddInstanceProfileModel"),
            call(&client, "RemoveInstanceProfileModel", &("default", "beta"))
                .expect_err("RemoveInstanceProfileModel"),
            call(&client, "SetInstanceProfile", &("lab", "default", "REV"))
                .expect_err("SetInstanceProfile"),
            call(
                &client,
                "SetInstanceAddress",
                &("lab", "127.0.0.1", "8595", "REV"),
            )
            .expect_err("SetInstanceAddress"),
        ] {
            assert_eq!(e.0, err_name("NotAuthorized"), "no sender: {e:?}");
        }
        assert_eq!(
            (pool(), record()),
            (pool_seed.to_string(), record_seed.to_string()),
            "a caller with no sender wrote nothing"
        );
    }

    // ── An authorized caller: the shapes, the answers, the bytes ───────────
    {
        let c = core();
        let (_server, client) = connect(&c, Some(":1.42"));

        let r = call(&client, "InstanceProfiles", &()).expect("InstanceProfiles");
        assert_eq!(
            r.body().signature().to_string(),
            "a(ssas)",
            "an array of the id, the server profile and the entries: no revision, \
             the pool is one text"
        );
        assert_eq!(
            body::<Rows>(&r),
            vec![
                (
                    "default".to_string(),
                    "default".to_string(),
                    vec!["alpha".to_string()]
                ),
                (
                    "lab".to_string(),
                    "lab".to_string(),
                    vec!["alpha".to_string()]
                ),
                ("unassigned".to_string(), "default".to_string(), Vec::new()),
            ],
            "in the file's order, each entry section a model of its profile"
        );

        let r = call(&client, "GetInstanceProfile", &("default",)).expect("GetInstanceProfile");
        assert_eq!(
            r.body().signature().to_string(),
            "(sass)",
            "the server profile, the entries and the revision"
        );
        assert_eq!(
            body::<(String, Vec<String>, String)>(&r),
            (
                "default".to_string(),
                vec!["alpha".to_string()],
                pool_seed.to_string()
            ),
            "the revision is the text of `instance-profiles.ini` the read returned"
        );
        let e = call(&client, "GetInstanceProfile", &("nope",))
            .expect_err("GetInstanceProfile of no profile");
        assert_eq!(e.0, err_name("NoInstanceProfile"), "{e:?}");
        assert_eq!(e.1, "nope", "the refusal names the profile");

        // The create: the refusals first, the pool untouched.
        let e = call(&client, "CreateInstanceProfile", &("Bad Id", "lab"))
            .expect_err("an id the rule refuses");
        assert_eq!(e.0, err_name("Invalid"), "{e:?}");
        assert_eq!(
            Refusal::decode(&e.1),
            Some(refusal("instance-profile-bad-id", &["Bad Id"])),
            "the keyed refusal: {:?}",
            e.1
        );
        let e = call(&client, "CreateInstanceProfile", &("default", "lab"))
            .expect_err("a name the pool has");
        assert_eq!(e.0, err_name("InstanceProfileExists"), "{e:?}");
        assert_eq!(e.1, "default", "the refusal names the profile");
        let e = call(&client, "CreateInstanceProfile", &("edge", "nope"))
            .expect_err("no server profile of that name");
        assert_eq!(e.0, err_name("NoServerProfile"), "{e:?}");
        assert_eq!(
            pool(),
            pool_seed.to_string(),
            "every refused create wrote the pool"
        );

        let r = call(&client, "CreateInstanceProfile", &("edge", "lab"))
            .expect("CreateInstanceProfile of a new name");
        assert_eq!(
            r.body().signature().to_string(),
            String::new(),
            "the reply is the unit: ()"
        );
        assert_eq!(
            pool(),
            pai_core::instance_profiles::add_profile(pool_seed, "edge", "lab"),
            "the section is appended, every byte before it the file's own"
        );
        assert_eq!(record(), record_seed, "the create writes no record");

        // The entry add and remove.
        let e = call(&client, "AddInstanceProfileModel", &("nope", "alpha"))
            .expect_err("an entry of no profile");
        assert_eq!(e.0, err_name("NoInstanceProfile"), "{e:?}");
        let e = call(&client, "AddInstanceProfileModel", &("default", "nope"))
            .expect_err("an entry of no model profile");
        assert_eq!(e.0, err_name("Invalid"), "{e:?}");
        assert_eq!(
            Refusal::decode(&e.1),
            Some(refusal("no-model-profile-named", &["nope"])),
            "the keyed refusal: {:?}",
            e.1
        );
        let before = pool();
        call(&client, "AddInstanceProfileModel", &("default", "beta"))
            .expect("AddInstanceProfileModel of beta");
        assert_eq!(
            pool(),
            pai_core::instance_profiles::add_entry(&before, "default", "beta"),
            "the empty entry section is appended"
        );
        let render = std::fs::read_to_string(sys.join("models@default.ini")).expect("render");
        assert!(
            sections(&render).contains(&"beta".to_string()),
            "the entry add re-renders the instance with the model: {:?}",
            sections(&render)
        );
        let before = pool();
        call(&client, "RemoveInstanceProfileModel", &("default", "alpha"))
            .expect("RemoveInstanceProfileModel of alpha");
        assert_eq!(
            pool(),
            pai_core::ini::text::delete_section(&before, "default/alpha")
                .expect("the entry the pool held"),
            "the entry section is cut out, every other byte kept"
        );
        assert!(
            pool().contains("[default/beta]"),
            "the removal leaves the profile's other entries: {}",
            pool()
        );

        // `SetInstanceProfile`: the record's pointer, the pool's text as revision.
        let rev = pool();
        let e = call(
            &client,
            "SetInstanceProfile",
            &("ghost", "default", rev.as_str()),
        )
        .expect_err("an instance the record has no section for");
        assert_eq!(e.0, err_name("NoInstance"), "{e:?}");
        let e = call(
            &client,
            "SetInstanceProfile",
            &("lab", "nope", rev.as_str()),
        )
        .expect_err("a profile the pool has no section for");
        assert_eq!(e.0, err_name("NoInstanceProfile"), "{e:?}");
        let e = call(
            &client,
            "SetInstanceProfile",
            &("lab", "default", "STALE TEXT"),
        )
        .expect_err("a revision the pool no longer holds");
        assert_eq!(e.0, err_name("Changed"), "{e:?}");
        assert_eq!(
            record(),
            record_seed,
            "every refused switch wrote no record"
        );
        call(
            &client,
            "SetInstanceProfile",
            &("lab", "default", rev.as_str()),
        )
        .expect("SetInstanceProfile of lab");
        let rec = record();
        assert_eq!(
            (
                key_of(&rec, "lab", "InstanceProfile").as_deref(),
                key_of(&rec, "lab", "Port").as_deref(),
                key_of(&rec, "default", "InstanceProfile").as_deref(),
            ),
            (Some("default"), Some("8681"), Some("default")),
            "the section names the new profile, its address and every other \
             section kept"
        );
        assert_eq!(pool(), rev, "the switch never writes the pool");

        // `SetInstanceAddress`: the record's address, the record's text as revision.
        let e = call(
            &client,
            "SetInstanceAddress",
            &("ghost", "127.0.0.1", "8595", rec.as_str()),
        )
        .expect_err("an instance the record has no section for");
        assert_eq!(e.0, err_name("NoInstance"), "{e:?}");
        let e = call(
            &client,
            "SetInstanceAddress",
            &("default", "127.0.0.1", "8595", "STALE TEXT"),
        )
        .expect_err("a revision the record no longer holds");
        assert_eq!(e.0, err_name("Changed"), "{e:?}");
        for e in [
            call(&client, "SetInstanceAddress", &("default",)).expect_err("three strings"),
            call(
                &client,
                "SetInstanceAddress",
                &("default", "127.0.0.1", 8595u16, rec.as_str()),
            )
            .expect_err("a uint16 where the port text belongs"),
            call(
                &client,
                "SetInstanceAddress",
                &("default", "127.0.0.1", "8595", 8595u32),
            )
            .expect_err("a number where the revision belongs"),
        ] {
            assert!(
                !e.0.starts_with("org.privateai.Manager1."),
                "the port travels as text: a shape mismatch is zbus's own error, \
                 not one of the manager's: {e:?}"
            );
        }
        assert_eq!(
            record(),
            rec,
            "a body the method does not take writes nothing"
        );
        let e = call(
            &client,
            "SetInstanceAddress",
            &("default", "127.0.0.1", "not-a-port", rec.as_str()),
        )
        .expect_err("a port that is no number");
        assert_eq!(e.0, err_name("Invalid"), "{e:?}");
        let r = Refusal::decode(&e.1).expect("the keyed refusal");
        assert_eq!(r.key, "not-whole-number-in-range", "{r:?}");
        assert_eq!(
            (
                r.values.first().map(String::as_str),
                r.values.get(1).map(String::as_str)
            ),
            (Some("Port"), Some("not-a-port")),
            "the refusal names the key and the value: {r:?}"
        );
        let e = call(
            &client,
            "SetInstanceAddress",
            &("default", "127.0.0.1", "8681", rec.as_str()),
        )
        .expect_err("the address another instance holds (`lab` binds 0.0.0.0:8681)");
        assert_eq!(e.0, err_name("AddressInUse"), "{e:?}");
        assert_eq!(e.1, "lab", "the refusal names who holds it");
        let e = call(
            &client,
            "SetInstanceAddress",
            &("default", "127.0.0.1", "8590", rec.as_str()),
        )
        .expect_err("the web service's address");
        assert_eq!(e.0, err_name("AddressInUse"), "{e:?}");
        assert_eq!(e.1, "private-ai-bff", "named as its own");
        assert_eq!(record(), rec, "every refused address wrote no record");
        call(
            &client,
            "SetInstanceAddress",
            &("default", "127.0.0.1", "8595", rec.as_str()),
        )
        .expect("SetInstanceAddress of a free port");
        let rec = record();
        assert_eq!(
            (
                key_of(&rec, "default", "Hostname").as_deref(),
                key_of(&rec, "default", "Port").as_deref(),
                key_of(&rec, "lab", "Port").as_deref(),
            ),
            (Some("127.0.0.1"), Some("8595"), Some("8681")),
            "the section carries the address, every other section kept"
        );

        // The delete: the profile an instance runs first.
        let e = call(&client, "DeleteInstanceProfile", &("nope",))
            .expect_err("a profile the pool has no section for");
        assert_eq!(e.0, err_name("NoInstanceProfile"), "{e:?}");
        assert_eq!(e.1, "nope", "the refusal names the profile");
        let e = call(&client, "DeleteInstanceProfile", &("default",))
            .expect_err("the profile an instance runs");
        assert_eq!(e.0, err_name("InstanceProfileInUse"), "{e:?}");
        let parts = e.1.split('\u{1f}').collect::<Vec<_>>();
        assert_eq!(
            (parts.first().copied(), parts.get(1).copied()),
            (Some("default"), Some("default")),
            "the profile and the instance that runs it, split by the separator: {e:?}"
        );
        let e = call(&client, "DeleteInstanceProfile", &("lab",))
            .expect_err("the profile `lab` runs: `default`, the one above");
        assert_eq!(e.0, err_name("InstanceProfileInUse"), "{e:?}");
        assert_eq!(
            e.1.split('\u{1f}').collect::<Vec<_>>(),
            ["default", "lab"],
            "the refusal names the instance the record points at `default`"
        );
        let before = pool();
        let rec_before = record();
        call(&client, "DeleteInstanceProfile", &("unassigned",))
            .expect("DeleteInstanceProfile of a profile nobody runs");
        assert_eq!(
            pool(),
            pai_core::ini::text::delete_section(&before, "unassigned")
                .expect("the profile the pool held"),
            "the section is cut out, every other byte kept"
        );
        assert_eq!(record(), rec_before, "the delete writes no record");
        assert!(
            rows(&client).iter().all(|r| r.0 != "unassigned"),
            "the list no longer answers the removed profile: {:?}",
            rows(&client)
        );

        // `CreateInstance` proposes the port: `default` sits on 8595 (moved
        // above) and `lab` on 8681, so the first free port at or above 8581 is
        // 8581, and the next create takes the next free one.
        call(&client, "CreateInstance", &("llama.cpp", "db-new", "lab"))
            .expect("CreateInstance of a new id");
        assert_eq!(
            key_of(&record(), "db-new", "Port").as_deref(),
            Some("8581"),
            "the record's section carries the proposed port"
        );
        assert_eq!(
            key_of(&record(), "db-new", "Hostname").as_deref(),
            None,
            "the create names no host: the profile's bind stands"
        );
        call(&client, "CreateInstance", &("llama.cpp", "db-new2", "lab"))
            .expect("the second create");
        assert_eq!(
            key_of(&record(), "db-new2", "Port").as_deref(),
            Some("8582"),
            "the second create is given the next free port, not 8581 again"
        );
    }

    // ── The `As` forms, answered to the bff's account ──────────────────────
    {
        let c = core_bff();
        let (_server, client) = connect(&c, Some(":1.9"));

        call(
            &client,
            "CreateInstanceProfileAs",
            &("alice", "edge-as", "lab"),
        )
        .expect("CreateInstanceProfileAs for a member of pai");
        assert!(
            pool().contains("[edge-as]"),
            "the member's create wrote: {}",
            pool()
        );
        let r = call(&client, "GetInstanceProfileAs", &("alice", "edge-as"))
            .expect("GetInstanceProfileAs");
        assert_eq!(
            r.body().signature().to_string(),
            "(sass)",
            "the `As` read answers the same shape"
        );
        call(
            &client,
            "AddInstanceProfileModelAs",
            &("alice", "edge-as", "gamma"),
        )
        .expect("AddInstanceProfileModelAs");
        assert!(
            pool().contains("[edge-as/gamma]"),
            "the member's entry add wrote: {}",
            pool()
        );

        // The instance switched onto the member's profile: deleting it is now
        // the in-use refusal, and the address verb answers through `As`.
        let rev = pool();
        call(
            &client,
            "SetInstanceProfileAs",
            &("alice", "lab", "edge-as", rev.as_str()),
        )
        .expect("SetInstanceProfileAs");
        assert_eq!(
            key_of(&record(), "lab", "InstanceProfile").as_deref(),
            Some("edge-as"),
            "the member's switch wrote the record"
        );
        let e = call(&client, "DeleteInstanceProfileAs", &("alice", "edge-as"))
            .expect_err("the profile the instance now runs");
        assert_eq!(e.0, err_name("InstanceProfileInUse"), "{e:?}");
        assert_eq!(
            e.1.split('\u{1f}').collect::<Vec<_>>(),
            ["edge-as", "lab"],
            "the refusal names the instance that runs it"
        );
        call(
            &client,
            "SetInstanceAddressAs",
            &("alice", "lab", "127.0.0.1", "8699", record().as_str()),
        )
        .expect("SetInstanceAddressAs");
        assert_eq!(
            (
                key_of(&record(), "lab", "Hostname").as_deref(),
                key_of(&record(), "lab", "Port").as_deref()
            ),
            (Some("127.0.0.1"), Some("8699")),
            "the member's address write wrote the record"
        );
        assert_eq!(
            key_of(&record(), "db-new", "Port").as_deref(),
            Some("8581"),
            "and leaves every other section alone"
        );
        for e in [
            call(
                &client,
                "RemoveInstanceProfileModelAs",
                &("bob", "edge-as", "gamma"),
            )
            .expect_err("RemoveInstanceProfileModelAs for a principal outside pai"),
            call(&client, "DeleteInstanceProfileAs", &("bob", "edge-as"))
                .expect_err("DeleteInstanceProfileAs for a principal outside pai"),
        ] {
            assert_eq!(e.0, err_name("NotAuthorized"), "{e:?}");
        }
        assert!(
            pool().contains("[edge-as/gamma]"),
            "nothing was written for a principal outside pai: {}",
            pool()
        );
    }

    // ── A caller that is not the bff: the `As` forms are refused ───────────
    {
        let c = core();
        let (_server, client) = connect(&c, Some(":1.42"));
        let before = pool();
        for e in [
            call(
                &client,
                "CreateInstanceProfileAs",
                &("alice", "edge-x", "lab"),
            )
            .expect_err("CreateInstanceProfileAs not from the bff"),
            call(&client, "DeleteInstanceProfileAs", &("alice", "edge-as"))
                .expect_err("DeleteInstanceProfileAs not from the bff"),
            call(
                &client,
                "AddInstanceProfileModelAs",
                &("alice", "default", "beta"),
            )
            .expect_err("AddInstanceProfileModelAs not from the bff"),
            call(
                &client,
                "RemoveInstanceProfileModelAs",
                &("alice", "default", "beta"),
            )
            .expect_err("RemoveInstanceProfileModelAs not from the bff"),
            call(
                &client,
                "SetInstanceProfileAs",
                &("alice", "lab", "default", "x"),
            )
            .expect_err("SetInstanceProfileAs not from the bff"),
            call(
                &client,
                "SetInstanceAddressAs",
                &("alice", "lab", "127.0.0.1", "8699", "x"),
            )
            .expect_err("SetInstanceAddressAs not from the bff"),
            call(&client, "InstanceProfilesAs", &("alice",))
                .expect_err("InstanceProfilesAs not from the bff"),
            call(&client, "GetInstanceProfileAs", &("alice", "default"))
                .expect_err("GetInstanceProfileAs not from the bff"),
        ] {
            assert_eq!(e.0, err_name("NotAuthorized"), "{e:?}");
        }
        assert_eq!(pool(), before, "a caller that is not the bff wrote nothing");
    }
}

/// Creating and deleting a model profile over D-Bus (slice MP1, its rules of
/// IP3b): `CreateModelProfile(s profile, s from, a(ss) values)` and its `As`
/// form with the principal first, `(sssa(ss))`; `DeleteModelProfile(s profile)`
/// and `DeleteModelProfileAs(ss)`. Every reply is the unit. The create writes
/// the pool and joins no entry, so it moves no render; a model an entry lists
/// is not free. A rule's refusal crosses as the keyed `Invalid` (decoded to
/// `model-profile-name-taken`, `invalid-model-profile-id`,
/// `model-profile-bad-source`, `model-profile-bad-text`,
/// `model-profile-assigned`, `no-model-profile-named`), the guard's refusal as
/// `NotAuthorized`. A body that is not the signature is zbus's own error and
/// writes nothing, and a caller with no sender is refused.
#[cfg(test)]
pub(crate) fn manager_dbus_model_profiles_e2e() {
    use super::service::fakes::{
        FakeActionLog, FakeAuthority, FakeGroup, FakePasswords, FakeResolver, FakeService,
    };
    use super::service::real_model_files;
    use pai_core::checks::Refusal;
    use std::os::unix::net::UnixStream;
    use std::time::Duration;
    use zbus::blocking::connection::Builder;
    use zbus::blocking::Connection;

    type Core = Manager<FakeService, FakeAuthority, FakeResolver>;

    let dir = tempfile::tempdir().expect("tempdir");
    crate::paths::redirect_for_tests(dir.path());
    let sys = crate::paths::engine_config_dir();
    std::fs::create_dir_all(&sys).expect("engine config dir");
    // The record: two instances, both running the instance profile `gpu`.
    const RECORD: &str = "[default]\nInstanceProfile = gpu\nPort = 8080\n\n[lan-2]\nInstanceProfile = gpu\nPort = 8081\n";
    // The instance profiles: the entries ARE the model list. `gpu` lists two of
    // the pool's models, each in the other letter case; `plain` lists none.
    const PROFILES: &str = "[gpu]\nServerProfile = gpu\n\n[gpu/On-One]\n\n[gpu/TWO-PLACES]\n\n[plain]\nServerProfile = plain\n";
    std::fs::write(sys.join("instances.ini"), RECORD).expect("record");
    std::fs::write(sys.join("instance-profiles.ini"), PROFILES).expect("instance profiles");
    for name in ["gpu", "plain"] {
        std::fs::write(
            sys.join(format!("server@{name}.ini")),
            "[Server]\nDevices = CUDA0\n",
        )
        .expect("server profile");
    }
    // The pool holds `Twin`, `On-One`, `two-places`, `Star-Served` and `free-me`:
    // two of them listed by an entry, three of them by none. The retired
    // `models.ini.disabled` holds `Retired` and the retired `assignments.ini`
    // names `two-places`: neither file is read, so neither blocks a delete and
    // the test pins that neither is written.
    let pool = real_model_files(
        dir.path(),
        "[Twin]\nmodel = /models/twin.gguf\n\n[On-One]\nmodel = /models/on1.gguf\n\n[two-places]\nmodel = /models/two.gguf\n\n[Star-Served]\nmodel = /models/star.gguf\n\n[free-me]\nmodel = /models/free.gguf\n",
    );
    let retired = real_model_files(dir.path(), "[Retired]\nmodel = /models/retired.gguf\n");
    std::fs::write(sys.join("models.ini"), &pool).expect("models.ini");
    std::fs::write(sys.join("models.ini.disabled"), &retired).expect("models.ini.disabled");
    std::fs::write(
        sys.join("assignments.ini"),
        "[default]\ntwo-places = inherit\n\n[lan-2]\ntwo-places = off\n* = on\n",
    )
    .expect("assignments.ini");
    // Each instance's render, as the entries write it.
    for id in ["default", "lan-2"] {
        std::fs::write(
            sys.join(format!("models@{id}.ini")),
            pai_core::assign::render_profile(&pool, PROFILES, "gpu"),
        )
        .expect("render");
    }

    let connect = |core: &Arc<Core>, sender: Option<&str>| -> (Connection, Connection) {
        let (s0, s1) = UnixStream::pair().expect("socket pair");
        let iface = ManagerInterface::new(core.clone());
        let server = std::thread::spawn(move || {
            Builder::async_io_unix_stream(s0)
                .server(zbus::Guid::generate())
                .expect("server guid")
                .p2p()
                .serve_at(OBJECT_PATH, iface)
                .expect("serve_at")
                .build()
                .expect("server connection")
        });
        let client = Builder::async_io_unix_stream(s1)
            .p2p()
            .method_timeout(Duration::from_secs(10))
            .build()
            .expect("client connection");
        if let Some(name) = sender {
            client.inner().set_unique_name(name).expect("unique name");
        }
        (server.join().expect("server thread"), client)
    };
    let core = |allow: bool| -> Arc<Core> {
        Arc::new(
            Manager::new(
                FakeService::new(),
                FakeAuthority::new(allow)
                    .quietly(true)
                    .bff(&Caller(":1.9".into())),
                FakeResolver::lan(),
            )
            .with_group(FakeGroup::new())
            .with_passwords(FakePasswords::new())
            .with_log(FakeActionLog::new()),
        )
    };
    fn call<B>(conn: &Connection, method: &str, body: &B) -> Result<zbus::Message, (String, String)>
    where
        B: serde::ser::Serialize + zbus::zvariant::DynamicType,
    {
        conn.call_method(None::<&str>, OBJECT_PATH, Some(INTERFACE), method, body)
            .map_err(|e| match e {
                zbus::Error::MethodError(name, detail, _) => {
                    (name.to_string(), detail.unwrap_or_default())
                }
                zbus::Error::FDO(e) => {
                    use zbus::DBusError;
                    (
                        e.name().to_string(),
                        e.description().unwrap_or_default().to_string(),
                    )
                }
                other => panic!("{method}: not a method error reply: {other:?}"),
            })
    }
    let err_name = |kind: &str| format!("org.privateai.Manager1.Error.{kind}");
    let refusal = |key: &'static str, values: &[&str]| Refusal {
        key,
        values: values.iter().map(|v| v.to_string()).collect(),
    };
    let v = |pairs: &[(&str, &str)]| -> Vec<(String, String)> {
        pairs
            .iter()
            .map(|(k, value)| (k.to_string(), value.to_string()))
            .collect()
    };
    let pool_text = || std::fs::read_to_string(sys.join("models.ini")).expect("pool");
    let retired_off =
        || std::fs::read_to_string(sys.join("models.ini.disabled")).expect("the retired off file");
    let section = |id: &str| -> String {
        pai_core::ini::text::section_text(&pool_text(), id).unwrap_or_default()
    };
    let listing = || -> Vec<String> {
        let mut names: Vec<String> = std::fs::read_dir(&sys)
            .map(|rd| {
                rd.flatten()
                    .map(|e| e.file_name().to_string_lossy().into_owned())
                    .collect()
            })
            .unwrap_or_default();
        names.sort();
        names
    };
    // Every render, by name with its bytes: a create joins no entry, so it moves
    // no render, and a delete moves only those of a profile that lists the model.
    let renders = || -> Vec<(String, Vec<u8>)> {
        listing()
            .into_iter()
            .filter(|name| name.starts_with("models@"))
            .map(|name| {
                (
                    name.clone(),
                    std::fs::read(sys.join(&name)).expect("render"),
                )
            })
            .collect()
    };

    // ── A caller with no sender: both verbs refused, nothing touched ───────
    {
        let c = core(true);
        let (_server, client) = connect(&c, None);
        let e = call(
            &client,
            "CreateModelProfile",
            &("nope", "", v(&[("model", "/m/nope.gguf")])),
        )
        .expect_err("CreateModelProfile with no sender");
        assert_eq!(e.0, err_name("NotAuthorized"), "{e:?}");
        let e = call(&client, "DeleteModelProfile", &("free-me"))
            .expect_err("DeleteModelProfile with no sender");
        assert_eq!(e.0, err_name("NotAuthorized"), "{e:?}");
        assert!(
            !section("nope").contains("nope.gguf") && section("free-me").contains("free.gguf"),
            "a call with no sender wrote and removed nothing: {}",
            pool_text()
        );
    }

    // ── An authorized caller: the verbs, their answers, their refusals ─────
    {
        let c = core(true);
        let (_server, client) = connect(&c, Some(":1.42"));
        // The renders as the entries wrote them: no create here joins an entry,
        // and the delete below takes away a model no entry lists.
        let renders_before = renders();

        let r = call(
            &client,
            "CreateModelProfile",
            &("bus-x", "", v(&[("model", "/m/bus-x.gguf")])),
        )
        .expect("CreateModelProfile of a new id");
        assert_eq!(
            r.body().signature().to_string(),
            String::new(),
            "the reply is the unit: ()"
        );
        assert_eq!(
            section("bus-x"),
            "[bus-x]\nmodel = /m/bus-x.gguf\n",
            "the section of the call, in the pool: {}",
            pool_text()
        );

        // The `from` of the wire is a name, the values pairs: a clone carries
        // the source's lines and the call's own.
        let r = call(
            &client,
            "CreateModelProfile",
            &("bus-clone", "bus-x", v(&[("temp", "0.2")])),
        )
        .expect("CreateModelProfile cloning a profile");
        assert_eq!(
            r.body().signature().to_string(),
            String::new(),
            "the reply is the unit: ()"
        );
        assert_eq!(
            section("bus-clone"),
            "[bus-clone]\nmodel = /m/bus-x.gguf\ntemp = 0.2\n",
            "the source's lines, then the call's: {}",
            pool_text()
        );

        // A rule's refusal of a create crosses as the keyed `Invalid`.
        for (id, from, values, want, why) in [
            (
                "TWIN",
                "",
                v(&[("model", "/m/twin.gguf")]),
                refusal("model-profile-name-taken", &["TWIN"]),
                "an id the pool holds in the other case",
            ),
            (
                "*",
                "",
                v(&[("model", "/m/x.gguf")]),
                refusal("invalid-model-profile-id", &["*"]),
                "an id of no id",
            ),
            (
                "both",
                "bus-x",
                v(&[("model", "/m/both.gguf"), ("hf-repo", "org/repo")]),
                refusal("model-profile-bad-source", &["both"]),
                "a model and a fetch at once",
            ),
            (
                "lf",
                "",
                v(&[("model", "/m/a\ngguf")]),
                refusal("model-profile-bad-text", &["model"]),
                "a value with a line ending",
            ),
            (
                "hk-key",
                "bus-x",
                v(&[("a\n[x]\nmodel", "1")]),
                refusal("model-profile-bad-text", &["a\n[x]\nmodel"]),
                "a clone override key of no key: written raw it forges a second \
                 section the name, id and source checks never saw",
            ),
            (
                "nowhere",
                "no-such-profile",
                v(&[("model", "/m/x.gguf")]),
                refusal("model-profile-bad-source", &["nowhere"]),
                "a model of its own beside a clone's source, judged before the pool",
            ),
            (
                "clone-missing",
                "no-such-profile",
                v(&[]),
                refusal("no-model-profile-named", &["no-such-profile"]),
                "a clone whose source the pool does not hold",
            ),
        ] {
            let e = call(&client, "CreateModelProfile", &(id, from, values))
                .expect_err("a create refused by a rule");
            assert_eq!(e.0, err_name("Invalid"), "{why}: {e:?}");
            assert_eq!(
                Refusal::decode(&e.1),
                Some(want),
                "{why}: the refusal crosses encoded: {:?}",
                e.1
            );
        }
        assert!(
            !pool_text().contains("[both]")
                && !pool_text().contains("[lf]")
                && !pool_text().contains("[hk-key]")
                && !pool_text().contains("[nowhere]")
                && !pool_text().contains("[clone-missing]"),
            "a refused create wrote nothing: {}",
            pool_text()
        );
        assert_eq!(
            renders(),
            renders_before,
            "no create joined an entry, so no instance's render moved"
        );

        // A rule's refusal of a delete, the same. An entry that lists the model
        // is the refusal, naming the instance profile that lists it, in the
        // pool's own order; the retired record and the retired switched-off
        // file name and hold sections, and are no proof a model is listed.
        for (id, want, why) in [
            (
                "On-One",
                refusal("model-profile-assigned", &["On-One", "gpu"]),
                "a model the entry `[gpu/On-One]` lists",
            ),
            (
                "on-one",
                refusal("model-profile-assigned", &["on-one", "gpu"]),
                "the same entry, in the other letter case",
            ),
            (
                "TWO-PLACES",
                refusal("model-profile-assigned", &["TWO-PLACES", "gpu"]),
                "the entry `[gpu/TWO-PLACES]`, naming the profile once though two \
                 instances run it",
            ),
            (
                "gone",
                refusal("no-model-profile-named", &["gone"]),
                "a profile the pool does not hold",
            ),
            (
                "Retired",
                refusal("no-model-profile-named", &["Retired"]),
                "a section of the retired switched-off file is no proof it exists",
            ),
            (
                "*",
                refusal("no-model-profile-named", &["*"]),
                "no id: the pool's refusal, not the id rule",
            ),
        ] {
            let e =
                call(&client, "DeleteModelProfile", &(id)).expect_err("a delete refused by a rule");
            assert_eq!(e.0, err_name("Invalid"), "{why}: {e:?}");
            assert_eq!(
                Refusal::decode(&e.1),
                Some(want),
                "{why}: the refusal crosses encoded: {:?}",
                e.1
            );
        }
        assert!(
            section("Twin").contains("twin.gguf")
                && section("On-One").contains("on1.gguf")
                && section("Star-Served").contains("star.gguf")
                && retired_off().contains("[Retired]"),
            "a refused delete removed nothing, from the pool or from the retired file: {} / {}",
            pool_text(),
            retired_off()
        );

        // The accepted delete: a model no entry lists, the section alone, out.
        // The retired record's keys are nobody's rule any more: no write of it
        // is needed for a profile to be free.
        call(&client, "DeleteModelProfile", &("free-me")).expect("the delete of a free profile");
        assert_eq!(
            section("free-me"),
            "",
            "the profile is gone: {}",
            pool_text()
        );
        assert!(
            section("Star-Served").contains("star.gguf"),
            "the neighbour is kept: {}",
            pool_text()
        );
        assert_eq!(
            retired_off(),
            retired,
            "the retired switched-off file keeps every byte"
        );
        assert_eq!(
            renders(),
            renders_before,
            "no instance's render moves for a model no entry listed"
        );

        // The `As` forms are there, with the principal first, and gated: this
        // caller is not the bff's account.
        let e = call(
            &client,
            "CreateModelProfileAs",
            &("alice", "bus-as", "", v(&[("model", "/m/as.gguf")])),
        )
        .expect_err("CreateModelProfileAs from a caller that is not the bff");
        assert_eq!(e.0, err_name("NotAuthorized"), "{e:?}");
        let e = call(&client, "DeleteModelProfileAs", &("alice", "Star-Served"))
            .expect_err("DeleteModelProfileAs from a caller that is not the bff");
        assert_eq!(e.0, err_name("NotAuthorized"), "{e:?}");
        assert!(
            !pool_text().contains("[bus-as]") && section("Star-Served").contains("star.gguf"),
            "a refused `As` form writes and removes nothing: {}",
            pool_text()
        );

        // The shape: no less, no more, and of the agreed types. zbus answers a
        // mismatch with its own error, never one of the manager's.
        let before = listing();
        for e in [
            call(&client, "CreateModelProfile", &("x", "")).expect_err("the values missing"),
            call(
                &client,
                "CreateModelProfile",
                &("x", "", vec!["model".to_string()]),
            )
            .expect_err("an array of strings where pairs belong"),
            call(
                &client,
                "CreateModelProfile",
                &("x", "", v(&[("model", "/m/x.gguf")]), "more"),
            )
            .expect_err("four arguments"),
            call(&client, "DeleteModelProfile", &())
                .expect_err("DeleteModelProfile with no argument"),
            call(&client, "DeleteModelProfile", &("a", "b"))
                .expect_err("DeleteModelProfile with two"),
            call(&client, "CreateModelProfileAs", &("alice", "x", ""))
                .expect_err("the As create missing the values"),
            call(&client, "DeleteModelProfileAs", &("alice"))
                .expect_err("the As delete missing the profile"),
        ] {
            assert!(
                !e.0.starts_with("org.privateai.Manager1."),
                "not the manager's error: {e:?}"
            );
        }
        assert_eq!(listing(), before, "a call of the wrong shape wrote nothing");
    }

    // ── The bff's account: a member's call lands, an outsider's is refused ─
    {
        let c = core(true);
        let (_server, client) = connect(&c, Some(":1.9"));

        let r = call(
            &client,
            "CreateModelProfileAs",
            &("alice", "member-mp", "", v(&[("hf-repo", "org/repo")])),
        )
        .expect("CreateModelProfileAs for a member of `pai`");
        assert_eq!(
            r.body().signature().to_string(),
            String::new(),
            "the reply is the unit: ()"
        );
        assert_eq!(
            section("member-mp"),
            "[member-mp]\nhf-repo = org/repo\n",
            "the member's profile, with the fetch and no model: {}",
            pool_text()
        );

        let e = call(
            &client,
            "CreateModelProfileAs",
            &("bob", "outsider-mp", "", v(&[("model", "/m/bob.gguf")])),
        )
        .expect_err("CreateModelProfileAs for a caller outside `pai`");
        assert_eq!(e.0, err_name("NotAuthorized"), "{e:?}");

        call(&client, "DeleteModelProfileAs", &("alice", "member-mp"))
            .expect("DeleteModelProfileAs for a member of `pai`");
        assert_eq!(
            section("member-mp"),
            "",
            "the member's delete landed: {}",
            pool_text()
        );
        let e = call(&client, "DeleteModelProfileAs", &("bob", "Star-Served"))
            .expect_err("DeleteModelProfileAs for a caller outside `pai`");
        assert_eq!(e.0, err_name("NotAuthorized"), "{e:?}");
        assert!(
            !pool_text().contains("[outsider-mp]") && section("Star-Served").contains("star.gguf"),
            "a refusal of the `As` form wrote and removed nothing: {}",
            pool_text()
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::ErrorKind;

    const PREFIX: &str = "org.privateai.Manager1.Error.";
    #[test]
    fn names_are_the_agreed_ones() {
        assert_eq!(BUS_NAME, "org.privateai.Manager1");
        assert_eq!(OBJECT_PATH, "/org/privateai/Manager1");
        assert_eq!(INTERFACE, "org.privateai.Manager1");
    }

    /// polkit's `CheckAuthorizationFlags`: `None` (0) for the quiet question,
    /// so a caller who would need to authenticate comes back a challenge
    /// (not authorized) without a prompt; `AllowUserInteraction` (1) for
    /// the question that may prompt. freedesktop.org polkit docs,
    /// org.freedesktop.PolicyKit1.Authority.
    #[test]
    fn the_quiet_check_allows_no_interaction() {
        assert_eq!(check_flags(false), 0);
        assert_eq!(check_flags(true), 1);
    }

    #[test]
    fn a_call_without_a_sender_is_refused() {
        assert_eq!(caller(None), Err(ManagerError::NotAuthorized));
        assert_eq!(caller(Some("")), Err(ManagerError::NotAuthorized));
    }

    #[test]
    fn the_sender_is_the_caller() {
        assert_eq!(caller(Some(":1.42")), Ok(Caller(":1.42".into())));
        assert_eq!(caller(Some(":1.7")), Ok(Caller(":1.7".into())));
    }

    #[test]
    fn every_error_has_its_own_name_under_the_prefix() {
        let cases = [
            (ManagerError::NotAuthorized, "NotAuthorized"),
            (ManagerError::InvalidId, "InvalidId"),
            (ManagerError::NoInstance, "NoInstance"),
            (ManagerError::BadFileName, "BadFileName"),
            (ManagerError::AddressInUse("default".into()), "AddressInUse"),
            (
                ManagerError::NoServerProfile("nope café".into()),
                "NoServerProfile",
            ),
            (
                ManagerError::NoInstanceProfile("nope café".into()),
                "NoInstanceProfile",
            ),
            (
                ManagerError::UnresolvableHost("gpu-box.lan".into()),
                "UnresolvableHost",
            ),
            (
                ManagerError::Service(super::super::service::ServiceError(
                    "Unit llama.cpp@default.service not found.".into(),
                )),
                "Service",
            ),
            (ManagerError::Io(ErrorKind::PermissionDenied), "Io"),
            (
                ManagerError::Invalid(pai_core::checks::Refusal {
                    key: "comment-markers",
                    values: vec!["ModelsDir".into()],
                }),
                "Invalid",
            ),
            (ManagerError::Changed, "Changed"),
            (
                ManagerError::ServerProfileExists("big café".into()),
                "ServerProfileExists",
            ),
            // The instance profile pair.
            (
                ManagerError::InstanceProfileExists("big café".into()),
                "InstanceProfileExists",
            ),
            (
                ManagerError::InstanceProfileInUse("big café".into(), "lab-2".into()),
                "InstanceProfileInUse",
            ),
        ];
        for (error, kind) in cases {
            let (name, message) = error_reply(&error);
            assert_eq!(name, format!("{PREFIX}{kind}"), "name of {error:?}");
            assert!(!message.trim().is_empty(), "message of {error:?} is empty");
        }
    }

    #[test]
    fn error_messages_carry_the_detail() {
        let (_, m) = error_reply(&ManagerError::AddressInUse("lan-2".into()));
        assert!(m.contains("lan-2"), "the other instance's id: {m:?}");
        let (_, m) = error_reply(&ManagerError::UnresolvableHost("gpu-box.lan".into()));
        assert!(m.contains("gpu-box.lan"), "the host: {m:?}");
        let text = "Unit llama.cpp@default.service not found.";
        let (_, m) = error_reply(&ManagerError::Service(super::super::service::ServiceError(
            text.into(),
        )));
        assert!(m.contains(text), "systemd's own message: {m:?}");
        for kind in [
            ErrorKind::PermissionDenied,
            ErrorKind::IsADirectory,
            ErrorKind::StorageFull,
        ] {
            let (_, m) = error_reply(&ManagerError::Io(kind));
            assert!(m.contains(&kind.to_string()), "io kind {kind:?}: {m:?}");
        }
    }

    /// The client reads the refusal back from the message, key and values.
    #[test]
    fn an_invalid_reply_carries_the_refusal() {
        use pai_core::checks::Refusal;
        for refusal in [
            Refusal {
                key: "load-on-startup-over-max",
                values: vec!["3".into(), "2".into()],
            },
            Refusal {
                key: "moe-cache-with-tensor-split",
                values: vec![],
            },
            Refusal {
                key: "comment-markers",
                values: vec!["OpencodeApiKey".into()],
            },
        ] {
            let (name, message) = error_reply(&ManagerError::Invalid(refusal.clone()));
            assert_eq!(name, format!("{PREFIX}Invalid"));
            assert_eq!(Refusal::decode(&message), Some(refusal), "{message:?}");
        }
    }

    // ── SystemdService against a stand-in for systemd ───────────────────

    /// What the stand-in for systemd was asked, one line per call.
    type Log = Arc<std::sync::Mutex<Vec<String>>>;

    /// systemd's list of changes: (type, symlink, destination).
    type Changes = Vec<(String, String, String)>;

    /// `org.freedesktop.systemd1.Manager` as far as the start at boot needs
    /// it (org.freedesktop.systemd1(5)): `GetUnitFileState` answers `state`,
    /// `EnableUnitFiles` and `DisableUnitFiles` record their arguments (or
    /// fail with `refuse`, as systemd refuses a caller without the right),
    /// `Reload` is recorded, and `ResetFailedUnit` is recorded (or fails with
    /// `refuse_reset`, as systemd answers a unit it does not have).
    struct FakeSystemd {
        log: Log,
        state: String,
        refuse: Option<String>,
        refuse_reset: Option<String>,
    }

    #[zbus::interface(name = "org.freedesktop.systemd1.Manager")]
    impl FakeSystemd {
        fn get_unit_file_state(&self, file: String) -> String {
            self.log
                .lock()
                .unwrap()
                .push(format!("GetUnitFileState {file}"));
            self.state.clone()
        }

        fn enable_unit_files(
            &self,
            files: Vec<String>,
            runtime: bool,
            force: bool,
        ) -> zbus::fdo::Result<(bool, Changes)> {
            self.log.lock().unwrap().push(format!(
                "EnableUnitFiles {files:?} runtime={runtime} force={force}"
            ));
            if let Some(m) = &self.refuse {
                return Err(zbus::fdo::Error::AccessDenied(m.clone()));
            }
            let link = format!("/etc/systemd/system/multi-user.target.wants/{}", files[0]);
            let target = "/usr/lib/systemd/system/llama.cpp@.service".to_string();
            Ok((true, vec![("symlink".into(), link, target)]))
        }

        fn disable_unit_files(
            &self,
            files: Vec<String>,
            runtime: bool,
        ) -> zbus::fdo::Result<Changes> {
            self.log
                .lock()
                .unwrap()
                .push(format!("DisableUnitFiles {files:?} runtime={runtime}"));
            if let Some(m) = &self.refuse {
                return Err(zbus::fdo::Error::AccessDenied(m.clone()));
            }
            let link = format!("/etc/systemd/system/multi-user.target.wants/{}", files[0]);
            Ok(vec![("unlink".into(), link, String::new())])
        }

        fn reload(&self) {
            self.log.lock().unwrap().push("Reload".into());
        }

        fn reset_failed_unit(&self, name: String) -> zbus::fdo::Result<()> {
            self.log
                .lock()
                .unwrap()
                .push(format!("ResetFailedUnit {name}"));
            if let Some(m) = &self.refuse_reset {
                return Err(zbus::fdo::Error::AccessDenied(m.clone()));
            }
            Ok(())
        }
    }

    /// A [`SystemdService`] over a peer-to-peer connection to a
    /// [`FakeSystemd`] (never the system bus), the server connection to keep
    /// alive, and the log. Every call has a deadline.
    fn systemd(state: &str, refuse: Option<&str>) -> (Connection, SystemdService, Log) {
        systemds(state, refuse, None)
    }

    /// The same, with `ResetFailedUnit` answering `refuse_reset`.
    fn systemds(
        state: &str,
        refuse: Option<&str>,
        refuse_reset: Option<&str>,
    ) -> (Connection, SystemdService, Log) {
        use std::os::unix::net::UnixStream;
        use zbus::blocking::connection::Builder;
        let log = Log::default();
        let fake = FakeSystemd {
            log: log.clone(),
            state: state.to_string(),
            refuse: refuse.map(str::to_string),
            refuse_reset: refuse_reset.map(str::to_string),
        };
        let (s0, s1) = UnixStream::pair().expect("socket pair");
        let server = std::thread::spawn(move || {
            Builder::async_io_unix_stream(s0)
                .server(zbus::Guid::generate())
                .expect("server guid")
                .p2p()
                .serve_at("/org/freedesktop/systemd1", fake)
                .expect("serve_at")
                .build()
                .expect("server connection")
        });
        let client = Builder::async_io_unix_stream(s1)
            .p2p()
            .method_timeout(std::time::Duration::from_secs(10))
            .build()
            .expect("client connection");
        (
            server.join().expect("server thread"),
            SystemdService::new(client),
            log,
        )
    }

    fn logged(log: &Log) -> Vec<String> {
        log.lock().unwrap().clone()
    }

    /// The instance starts at boot when its unit file is enabled, as
    /// `systemctl is-enabled llama.cpp@<id>` reads it (systemctl(1) Table 3):
    /// "enabled" is the `WantedBy=` symlink in /etc. Every other state reads
    /// as not started at boot: "enabled-runtime" is a symlink in /run, gone
    /// before the next boot; "disabled", "masked", "linked" and the rest
    /// start nothing.
    #[test]
    fn the_start_at_boot_is_the_unit_file_state_enabled() {
        for (state, enabled) in [
            ("enabled", true),
            ("disabled", false),
            ("enabled-runtime", false),
            ("masked", false),
            ("masked-runtime", false),
            ("linked", false),
            ("linked-runtime", false),
            ("static", false),
            ("indirect", false),
            ("", false),
        ] {
            let (_server, service, log) = systemd(state, None);
            assert_eq!(service.boot_enabled("lan-2"), Ok(enabled), "{state:?}");
            assert_eq!(
                logged(&log),
                ["GetUnitFileState llama.cpp@lan-2.service"],
                "{state:?}: one question, about the instance's unit"
            );
        }
    }

    /// `systemctl enable llama.cpp@<id>`: `EnableUnitFiles` on the
    /// instance's unit, persistent (runtime false: /etc, not /run) and
    /// without replacing other symlinks (force false), then `Reload`, as
    /// systemctl reloads after an enable unless told `--no-reload`
    /// (systemctl(1)).
    #[test]
    fn enabling_at_boot_enables_the_instances_unit_persistently_and_reloads() {
        let (_server, service, log) = systemd("disabled", None);
        assert_eq!(service.set_boot_enabled("default", true), Ok(()));
        assert_eq!(
            logged(&log),
            [
                r#"EnableUnitFiles ["llama.cpp@default.service"] runtime=false force=false"#,
                "Reload",
            ]
        );
    }

    /// `systemctl disable llama.cpp@<id>`: `DisableUnitFiles` (runtime
    /// false), then `Reload`.
    #[test]
    fn disabling_at_boot_disables_the_instances_unit_and_reloads() {
        let (_server, service, log) = systemd("enabled", None);
        assert_eq!(service.set_boot_enabled("default", false), Ok(()));
        assert_eq!(
            logged(&log),
            [
                r#"DisableUnitFiles ["llama.cpp@default.service"] runtime=false"#,
                "Reload",
            ]
        );
    }

    /// Taking an instance's unit away (`systemctl disable` then a reset,
    /// systemctl(1), org.freedesktop.systemd1(5)): `DisableUnitFiles` on the
    /// instance's unit, persistent (runtime false), so a `wants/` symlink left
    /// by hand for a reused id is gone; `Reload`, as systemd must reread after
    /// a disable; `ResetFailedUnit` on the unit, so an id used again does not
    /// read `failed` from the last run.
    #[test]
    fn removing_an_instance_disables_its_unit_reloads_and_resets_it() {
        let (_server, service, log) = systemd("enabled", None);
        assert_eq!(service.remove("lan-2"), Ok(()));
        assert_eq!(
            logged(&log),
            [
                r#"DisableUnitFiles ["llama.cpp@lan-2.service"] runtime=false"#,
                "Reload",
                "ResetFailedUnit llama.cpp@lan-2.service",
            ],
            "the three, in that order, all about the instance's own unit"
        );
    }

    /// A unit systemd does not have has nothing to reset: `ResetFailedUnit`
    /// answering an error is not the remove's failure, and the two steps that
    /// come before it are done.
    #[test]
    fn a_reset_that_fails_is_ignored_and_the_remove_is_done() {
        let not_loaded = "Unit llama.cpp@gone.service not loaded.";
        let (_server, service, log) = systemds("disabled", None, Some(not_loaded));
        assert_eq!(
            service.remove("gone"),
            Ok(()),
            "the reset's error is systemd's 'not loaded', not a refusal of the removal"
        );
        assert_eq!(
            logged(&log),
            [
                r#"DisableUnitFiles ["llama.cpp@gone.service"] runtime=false"#,
                "Reload",
                "ResetFailedUnit llama.cpp@gone.service",
            ]
        );
    }

    /// A refusal of `DisableUnitFiles` is the service manager's own failure,
    /// its message kept, and the unit is not reloaded nor reset.
    #[test]
    fn a_refused_disable_of_a_removed_unit_fails_the_remove() {
        let denied = "Access denied";
        let (_server, service, log) = systemd("enabled", Some(denied));
        match service.remove("lan-2") {
            Err(ServiceError(m)) => assert!(m.contains(denied), "{m:?}"),
            other => panic!("{other:?}"),
        }
        assert_eq!(
            logged(&log),
            [r#"DisableUnitFiles ["llama.cpp@lan-2.service"] runtime=false"#],
            "nothing after the step that failed"
        );
    }

    /// A refusal of systemd comes back as the service manager's failure,
    /// its own message kept, and nothing is reloaded.
    #[test]
    fn a_refused_enable_or_disable_is_systemds_failure_and_reloads_nothing() {
        let denied = "Access denied";
        for on in [true, false] {
            let (_server, service, log) = systemd("disabled", Some(denied));
            match service.set_boot_enabled("default", on) {
                Err(ServiceError(m)) => assert!(m.contains(denied), "{on}: {m:?}"),
                other => panic!("{on}: {other:?}"),
            }
            assert!(
                !logged(&log).iter().any(|l| l == "Reload"),
                "{on}: {:?}",
                logged(&log)
            );
        }
    }

    /// No systemd on the other end: a failure, never a hang or a panic.
    #[test]
    fn no_systemd_is_a_failure() {
        let (server, service, _log) = systemd("enabled", None);
        drop(server);
        assert!(service.boot_enabled("default").is_err());
        assert!(service.set_boot_enabled("default", true).is_err());
    }

    #[test]
    fn states_are_systemd_active_state_names() {
        assert_eq!(state_name(UnitState::Active), "active");
        assert_eq!(state_name(UnitState::Inactive), "inactive");
        assert_eq!(state_name(UnitState::Failed), "failed");
        assert_eq!(state_name(UnitState::Activating), "activating");
        assert_eq!(state_name(UnitState::Deactivating), "deactivating");
    }

    // ── Authenticate over D-Bus ─────────────────────────────────────────

    type AuthCore = Manager<
        super::super::service::fakes::FakeService,
        super::super::service::fakes::FakeAuthority,
        super::super::service::fakes::FakeResolver,
    >;

    /// The bff's bus name in these tests.
    const BFF: &str = ":1.9";

    /// A core taking [`BFF`] for the bff's account, over the fake accounts
    /// (alice an administrator, bob not), and its checker's log.
    fn auth_core(
        passwords: super::super::service::fakes::FakePasswords,
    ) -> (Arc<AuthCore>, crate::manager::service::fakes::Checked) {
        use super::super::service::fakes::{FakeAuthority, FakeGroup, FakeResolver, FakeService};
        let checked = passwords.checked.clone();
        let core = Manager::new(
            FakeService::new(),
            FakeAuthority::new(true).bff(&Caller(BFF.into())),
            FakeResolver::lan(),
        )
        .with_group(FakeGroup::new())
        .with_passwords(passwords);
        (Arc::new(core), checked)
    }

    /// `core` served peer to peer on a socket pair (never a bus), a client
    /// stamping `sender` as the bus would; the server connection to keep.
    fn auth_connect(core: &Arc<AuthCore>, sender: Option<&str>) -> (Connection, Connection) {
        use std::os::unix::net::UnixStream;
        use zbus::blocking::connection::Builder;
        let (s0, s1) = UnixStream::pair().expect("socket pair");
        let iface = ManagerInterface::new(core.clone());
        let server = std::thread::spawn(move || {
            Builder::async_io_unix_stream(s0)
                .server(zbus::Guid::generate())
                .expect("server guid")
                .p2p()
                .serve_at(OBJECT_PATH, iface)
                .expect("serve_at")
                .build()
                .expect("server connection")
        });
        let client = Builder::async_io_unix_stream(s1)
            .p2p()
            .method_timeout(std::time::Duration::from_secs(10))
            .build()
            .expect("client connection");
        if let Some(name) = sender {
            client.inner().set_unique_name(name).expect("unique name");
        }
        (server.join().expect("server thread"), client)
    }

    /// `Authenticate(ss)`: the reply body, or the error (name, message).
    fn authenticate(
        client: &Connection,
        user: &str,
        password: &str,
    ) -> Result<zbus::Message, (String, String)> {
        client
            .call_method(
                None::<&str>,
                OBJECT_PATH,
                Some(INTERFACE),
                "Authenticate",
                &(user, password),
            )
            .map_err(|e| match e {
                zbus::Error::MethodError(name, detail, _) => {
                    (name.to_string(), detail.unwrap_or_default())
                }
                zbus::Error::FDO(e) => {
                    use zbus::DBusError;
                    (
                        e.name().to_string(),
                        e.description().unwrap_or_default().to_string(),
                    )
                }
                other => panic!("Authenticate: not a method error reply: {other:?}"),
            })
    }

    /// From the bff: `Authenticate(ss) -> (bb)`, (ok, can_configure).
    #[test]
    fn the_bff_authenticates_over_dbus() {
        let (core, checked) = auth_core(super::super::service::fakes::FakePasswords::new());
        let (_server, client) = auth_connect(&core, Some(BFF));
        for (user, password, expected) in [
            ("alice", "s3cret-Pw", (true, true)),
            ("bob", "hunter2!", (true, false)),
            ("alice", "wrong", (false, false)),
            ("mallory", "s3cret-Pw", (false, false)),
            ("", "s3cret-Pw", (false, false)),
            ("alice", "", (false, false)),
        ] {
            let reply =
                authenticate(&client, user, password).unwrap_or_else(|e| panic!("{user:?}: {e:?}"));
            assert_eq!(
                reply.body().signature().to_string(),
                "(bb)",
                "{user:?}: the reply is two booleans"
            );
            let got: (bool, bool) = reply.body().deserialize().expect("(bb)");
            assert_eq!(got, expected, "{user:?} / {password:?}");
        }
        assert_eq!(
            checked.lock().unwrap().len(),
            4,
            "the empty user and password are never checked"
        );
    }

    /// Any other sender, or none, is NotAuthorized and no password is checked.
    #[test]
    fn only_the_bff_may_authenticate_over_dbus() {
        for sender in [Some(":1.42"), None] {
            let (core, checked) = auth_core(super::super::service::fakes::FakePasswords::new());
            let (_server, client) = auth_connect(&core, sender);
            let err = authenticate(&client, "alice", "s3cret-Pw").expect_err("refused");
            assert_eq!(err.0, format!("{PREFIX}NotAuthorized"), "{sender:?}");
            assert!(!err.1.contains("s3cret-Pw"), "{err:?}");
            assert_eq!(*checked.lock().unwrap(), vec![], "{sender:?}");
        }
    }

    /// A failing checker replies Service with its message, never the
    /// password.
    #[test]
    fn a_failing_checker_replies_service_without_the_password() {
        let failure = "pam_start: Critical error - immediate abort";
        let (core, _) = auth_core(super::super::service::fakes::FakePasswords::failing(
            failure,
        ));
        let (_server, client) = auth_connect(&core, Some(BFF));
        let err = authenticate(&client, "alice", "s3cret-Pw").expect_err("a failure");
        assert_eq!(err, (format!("{PREFIX}Service"), failure.to_string()));
    }

    /// The bff is recognised by its uid alone: the caller's uid must equal
    /// the `private-ai-bff` user's, and either unknown is no (no such user
    /// installed is nobody, root included).
    #[test]
    fn the_bff_is_the_caller_with_the_bff_users_uid() {
        assert!(is_bff_uid(Some(990), Some(990)));
        assert!(!is_bff_uid(Some(1000), Some(990)));
        assert!(!is_bff_uid(Some(0), Some(990)), "root is not the bff");
        assert!(!is_bff_uid(None, Some(990)));
        assert!(!is_bff_uid(Some(0), None));
        assert!(!is_bff_uid(Some(990), None));
        assert!(!is_bff_uid(None, None));
    }

    // ── Acting for a signed-in user: the `As` methods ────────────────────
    // Every call here is refused before a path is resolved, or asks a fake
    // only, so these never reach `paths::`. The bff's calls that can succeed
    // are in `manager_dbus_principal_e2e`.

    use super::super::service::fakes;

    /// The membership questions [`principal_core`]'s accounts are asked.
    type Admins = std::sync::Arc<std::sync::Mutex<Vec<String>>>;
    /// One `As` method, called with `principal` as its first argument (the
    /// order the design pins: `StartAs(s principal, s id)`).
    type AsCall = fn(&Connection, &str) -> Result<zbus::Message, (String, String)>;

    /// A method call on the interface: Ok(reply), or Err((name, message)).
    fn call<B>(conn: &Connection, method: &str, body: &B) -> Result<zbus::Message, (String, String)>
    where
        B: serde::ser::Serialize + zbus::zvariant::DynamicType,
    {
        conn.call_method(None::<&str>, OBJECT_PATH, Some(INTERFACE), method, body)
            .map_err(|e| match e {
                zbus::Error::MethodError(name, detail, _) => {
                    (name.to_string(), detail.unwrap_or_default())
                }
                zbus::Error::FDO(e) => {
                    use zbus::DBusError;
                    (
                        e.name().to_string(),
                        e.description().unwrap_or_default().to_string(),
                    )
                }
                other => panic!("{method}: not a method error reply: {other:?}"),
            })
    }

    fn body<T: for<'de> zbus::zvariant::DynamicDeserialize<'de>>(m: &zbus::Message) -> T {
        m.body().deserialize().expect("reply body")
    }

    /// The `As` methods, each with its own arguments after the principal.
    fn as_methods() -> [(&'static str, AsCall); 10] {
        [
            ("StartAs", |c, p| call(c, "StartAs", &(p, "default"))),
            ("StopAs", |c, p| call(c, "StopAs", &(p, "default"))),
            ("RestartAs", |c, p| call(c, "RestartAs", &(p, "default"))),
            ("ReadFileAs", |c, p| {
                call(c, "ReadFileAs", &(p, "models.ini"))
            }),
            ("WriteFileAs", |c, p| {
                call(c, "WriteFileAs", &(p, "models.ini", "", false))
            }),
            ("WriteFileIfAs", |c, p| {
                call(c, "WriteFileIfAs", &(p, "models.ini", "", ""))
            }),
            ("SetBootEnabledAs", |c, p| {
                call(c, "SetBootEnabledAs", &(p, "default", true))
            }),
            ("SetServerProfileAs", |c, p| {
                call(c, "SetServerProfileAs", &(p, "default", "default"))
            }),
            ("CreateServerProfileAs", |c, p| {
                call(
                    c,
                    "CreateServerProfileAs",
                    &(p, "llama.cpp", "db-p", 8123u16),
                )
            }),
            ("DeleteServerProfileAs", |c, p| {
                call(c, "DeleteServerProfileAs", &(p, "llama.cpp", "db-p"))
            }),
        ]
    }

    /// [`auth_core`] (polkit would allow the caller everything, `alice` is a
    /// member of `pai`, `bob` is not), with the log of the membership
    /// questions its accounts are asked.
    fn principal_core(passwords: fakes::FakePasswords) -> (Arc<AuthCore>, Admins) {
        let admins = passwords.asked_admin.clone();
        let (core, _checked) = auth_core(passwords);
        (core, admins)
    }

    fn asked_admin(admins: &Admins) -> Vec<String> {
        admins.lock().unwrap().clone()
    }
    fn asked_bff(c: &AuthCore) -> Vec<Caller> {
        c.authority().asked_bff.lock().unwrap().clone()
    }
    fn polkit(c: &AuthCore) -> Vec<(Caller, Action)> {
        c.authority().asked.lock().unwrap().clone()
    }
    fn polkit_quiet(c: &AuthCore) -> Vec<(Caller, Action)> {
        c.authority().asked_quietly.lock().unwrap().clone()
    }
    fn service_calls(c: &AuthCore) -> Vec<fakes::Call> {
        c.service().calls.lock().unwrap().clone()
    }
    fn no_calls() -> Vec<fakes::Call> {
        Vec::new()
    }

    /// A caller that is not the bff naming a principal: NotAuthorized for
    /// every `As` method, the membership never looked up, polkit never asked,
    /// nothing done. The caller is one polkit would allow everything, so a
    /// call downgraded to the caller's own would have gone through.
    #[test]
    fn an_as_method_from_a_caller_that_is_not_the_bff_is_refused_over_dbus() {
        for sender in [Some(":1.42"), Some(":1.7"), None] {
            let (c, admins) = principal_core(fakes::FakePasswords::new());
            let (_server, client) = auth_connect(&c, sender);
            for (method, f) in as_methods() {
                let e = f(&client, "alice")
                    .expect_err(&format!("{method} from a caller that is not the bff"));
                assert_eq!(e.0, format!("{PREFIX}NotAuthorized"), "{method}");
            }
            assert_eq!(
                asked_admin(&admins),
                Vec::<String>::new(),
                "a membership looked up for a caller that is not the bff"
            );
            assert_eq!(
                polkit(&c),
                vec![],
                "polkit asked, so the call ran as the caller's own"
            );
            assert_eq!(polkit_quiet(&c), vec![], "not even the quiet question");
            assert_eq!(
                service_calls(&c),
                no_calls(),
                "a caller that is not the bff started nothing"
            );
            if let Some(name) = sender {
                assert_eq!(
                    asked_bff(&c),
                    vec![Caller(name.to_string()); 10],
                    "{sender:?}: the caller is checked first, once per call"
                );
            }
        }
    }

    /// The bff naming a member reaches the core as that principal's request:
    /// the stop runs, the membership is asked once per call and polkit never.
    /// The gate runs before the method's own check: the id and the file name
    /// are refused after the membership question, and nothing reaches the
    /// service manager.
    #[test]
    fn an_as_method_from_the_bff_reaches_the_core_with_the_principal_over_dbus() {
        let (c, admins) = principal_core(fakes::FakePasswords::new());
        let (_server, client) = auth_connect(&c, Some(BFF));
        call(&client, "StopAs", &("alice", "default")).expect("StopAs for a member");
        let e = call(&client, "SetBootEnabledAs", &("alice", "../x", true))
            .expect_err("SetBootEnabledAs of an invalid id");
        assert_eq!(e.0, format!("{PREFIX}InvalidId"), "the id, after the gate");
        let e = call(&client, "ReadFileAs", &("alice", "server.ini"))
            .expect_err("ReadFileAs of a bad name");
        assert_eq!(
            e.0,
            format!("{PREFIX}BadFileName"),
            "the name, after the gate"
        );
        let e = call(&client, "WriteFileAs", &("alice", "server.ini", "x", false))
            .expect_err("WriteFileAs of a bad name");
        assert_eq!(
            e.0,
            format!("{PREFIX}BadFileName"),
            "the name, after the gate"
        );
        let e = call(&client, "WriteFileIfAs", &("alice", "server.ini", "x", ""))
            .expect_err("WriteFileIfAs of a bad name");
        assert_eq!(
            e.0,
            format!("{PREFIX}BadFileName"),
            "the name, after the gate"
        );
        assert_eq!(
            service_calls(&c),
            vec![fakes::Call::Stop("default".into())],
            "the member's stop ran, nothing else"
        );
        assert_eq!(
            asked_admin(&admins),
            vec!["alice".to_string(); 5],
            "the membership of the name the caller vouches for, once per call"
        );
        assert_eq!(
            asked_bff(&c),
            vec![Caller(BFF.into()); 5],
            "the caller is checked first, at every call"
        );
        assert_eq!(
            polkit(&c),
            vec![],
            "polkit asked, so the call ran as the bff's own, not for the principal"
        );
        assert_eq!(
            polkit_quiet(&c),
            vec![],
            "not even the quiet question, the one that decides a read"
        );
    }

    /// The bff naming an account that is no member of `pai` (bob) or is
    /// unknown (mallory): NotAuthorized, the membership asked for that name,
    /// polkit never, nothing done.
    #[test]
    fn an_as_method_for_a_principal_who_is_no_member_is_refused_over_dbus() {
        for user in ["bob", "mallory"] {
            let (c, admins) = principal_core(fakes::FakePasswords::new());
            let (_server, client) = auth_connect(&c, Some(BFF));
            for (method, f) in as_methods() {
                let e = f(&client, user).expect_err(&format!("{method} for {user}"));
                assert_eq!(e.0, format!("{PREFIX}NotAuthorized"), "{method} for {user}");
            }
            assert_eq!(
                asked_admin(&admins),
                vec![user.to_string(); as_methods().len()],
                "the membership of {user}: asked once per `As` method, and it says no"
            );
            assert_eq!(
                polkit(&c),
                vec![],
                "polkit asked for a principal it refuses: the call ran as the bff's own"
            );
            assert_eq!(
                service_calls(&c),
                no_calls(),
                "{user}: a refused principal started nothing"
            );
        }
    }

    /// A principal name the manager will not take is refused before the
    /// account behind it is looked up: empty, one with a control character,
    /// one over the 256 bytes the bff itself bounds. A name with spaces is
    /// well formed, so it is asked about whole: the face may not trim it.
    #[test]
    fn an_ill_formed_principal_is_refused_before_the_account_is_asked_over_dbus() {
        let long = "x".repeat(257);
        for name in ["", "a\nb", long.as_str()] {
            let (c, admins) = principal_core(fakes::FakePasswords::new());
            let (_server, client) = auth_connect(&c, Some(BFF));
            let e = call(&client, "StopAs", &(name, "default"))
                .expect_err("StopAs for an ill-formed name");
            assert_eq!(e.0, format!("{PREFIX}NotAuthorized"), "{name:?}");
            assert_eq!(
                asked_admin(&admins),
                Vec::<String>::new(),
                "the account of {name:?} was looked up"
            );
            assert_eq!(service_calls(&c), no_calls(), "{name:?}");
        }
        let (c, admins) = principal_core(fakes::FakePasswords::new());
        let (_server, client) = auth_connect(&c, Some(BFF));
        let e = call(&client, "StopAs", &(" Alice ", "default"))
            .expect_err("StopAs for a name with spaces");
        assert_eq!(e.0, format!("{PREFIX}NotAuthorized"));
        assert_eq!(
            asked_admin(&admins),
            vec![" Alice ".to_string()],
            "the name travelled whole: no trim, no folding"
        );
    }

    /// `Version() -> (su)`: the framework version this crate was built with,
    /// beside the protocol number of the `As` methods. It needs no right: a
    /// named caller, another caller and no sender at all all get it, and it
    /// asks nobody anything.
    #[test]
    fn version_answers_the_framework_version_and_the_protocol_over_dbus() {
        for sender in [Some(BFF), Some(":1.42"), None] {
            let (c, admins) = principal_core(fakes::FakePasswords::new());
            let (_server, client) = auth_connect(&c, sender);
            let reply = call(&client, "Version", &()).expect("Version");
            assert_eq!(
                reply.body().signature().to_string(),
                "(su)",
                "{sender:?}: the reply is a string and a uint32"
            );
            assert_eq!(
                body::<(String, u32)>(&reply),
                (env!("CARGO_PKG_VERSION").to_string(), 3),
                "{sender:?}: the framework version and the protocol"
            );
            assert!(
                polkit(&c).is_empty() && polkit_quiet(&c).is_empty(),
                "{sender:?}: Version asks polkit nothing"
            );
            assert!(
                asked_bff(&c).is_empty() && asked_admin(&admins).is_empty(),
                "{sender:?}: Version checks no account"
            );
            assert!(
                service_calls(&c).is_empty(),
                "{sender:?}: Version touches nothing"
            );
        }
    }
}
