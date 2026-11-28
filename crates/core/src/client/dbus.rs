//! [`ManagerClient`] over D-Bus: interface `org.privateai.Manager1` at
//! `/org/privateai/Manager1`, on the system bus or on a given connection.
//!
//! A client [`DbusClient::acting_for`] a signed-in user vouches for: `Start`,
//! `Stop`, `Restart`, `ReadFile`, `WriteFile`, `WriteFileIf`,
//! `SetBootEnabled`, `SetModelProfile`, `SetServerProfile`,
//! `CreateServerProfile`, `DeleteServerProfile` and
//! `SetInstanceTags` and `SetInstanceLabel` and `ModelInfo` and `NodeInfo` and
//! `ModelFiles` go as the `As` form of the same method, with the user's name as
//! its first argument
//! (the manager answers a principal for the bff's account alone) and answers
//! the plain method's shape. The methods that need no right are unchanged,
//! `Instances` among them: the name the caller vouches for is no credential.
//!
//! ```text
//! StartAs(s principal, s id)          ReadFileAs(s principal, s name)
//! StopAs(s principal, s id)           RestartAs(s principal, s id)
//! SetBootEnabledAs(s principal, s id, b enabled)
//! WriteFileAs(s principal, s name, s contents, b keep_api_key)
//! WriteFileIfAs(s principal, s name, s contents, s expected)
//! SetModelProfileAs(s principal, s instance, s profile, s value)
//! SetServerProfileAs(s principal, s instance, s profile)
//! CreateServerProfileAs(s principal, s engine, s profile, q port)
//! DeleteServerProfileAs(s principal, s engine, s profile)
//! SetInstanceTagsAs(s principal, s instance, as tags)
//! SetInstanceLabel(s instance, s label)
//! SetInstanceLabelAs(s principal, s instance, s label)
//! ModelInfo(s profile) -> (s model, (b has_info, t block_count, b moe,
//!                                    t expert_count, t expert_used),
//!                          (s key, as args))
//! ModelInfoAs(s principal, s profile)    (the same reply)
//! NodeInfo() -> (s name, s version, s kernel, s vulkan, a(sssssbss) gpus)
//! NodeInfoAs(s principal)                (the same reply)
//! ModelFiles() -> (s dir, a(sttbs) files, b truncated, (s key, as args))
//! ModelFilesAs(s principal)              (the same reply)
//! Instances() -> a(sssqasassss)   (id, engine, server profile, port, profile
//!                                   tags, instance tags, label, instance
//!                                   profile, hostname: each empty when unset)
//! InstanceStates() -> a(sbssasb)  (id, converged, reason key, reason args,
//!                                   restart pending: the states of the apply)
//! CreateInstance(s engine, s id, s instance_profile) -> (q port)
//! CreateInstanceAs(s principal, s engine, s id, s instance_profile) (same)
//! ServerProfiles() -> a(ssqass)       (id, engine, port, tags, instance)
//! Version() -> (s version, u protocol)
//! ```

use super::{
    Affected, ClientError, DeviceInfo, InstanceInfo, InstanceProfileInfo, InstanceState,
    ManagerClient, ManagerFile, ManagerVersion, ModelFacts, ModelFileRow, ModelFilesReply,
    ModelInfoReply, ModelProfileSource, NodeDevices, NodeInfoReply, NotConverged, NotConvergedRow,
    Reason, ServerProfileInfo, TensorRow,
};
use crate::host::OsInfo;
use crate::vulkan::{status_of, Driver, NodeGpu, Status};
use zbus::blocking::Connection;
use zbus::export::serde::Serialize;
use zbus::message::Message;
use zbus::zvariant::DynamicType;

/// The well-known name the manager owns on the system bus.
pub const BUS_NAME: &str = "org.privateai.Manager1";
/// The object the interface is served at.
pub const OBJECT_PATH: &str = "/org/privateai/Manager1";
/// The interface name.
pub const INTERFACE: &str = "org.privateai.Manager1";

/// The D-Bus error for a method the served object does not have: a manager
/// built before `Version` answers [`DbusClient::version`] with it.
const UNKNOWN_METHOD: &str = "org.freedesktop.DBus.Error.UnknownMethod";

/// One row of `Instances`, as the bus answers it: `(s id, s engine,
/// s server_profile, q port, as profile_tags, as instance_tags, s label,
/// s instance_profile, s hostname)`, the whole list one `a(sssqasassss)`. A
/// section naming no server profile is the empty string, which the client reads
/// as none; so is a section with no label, no instance profile or no host (the
/// bus has no null). The last two fields are appended to the row (IP4b).
type InstanceRow = (
    String,
    String,
    String,
    u16,
    Vec<String>,
    Vec<String>,
    String,
    String,
    String,
);

/// One row of `InstanceStates`, as the bus answers it: `(s id, b converged,
/// s reason key, as reason args, b pendingRestart)`, the whole list one
/// `a(sbssasb)`, the sibling method of `Instances` (IP3c review 6). The bus has
/// no null, so a converged row carries the empty reason key and no args.
type StateRow = (String, bool, String, Vec<String>, bool);

/// One row of `ServerProfiles`, as the bus answers it: `(s id, s engine,
/// q port, as tags, s instance)`, the whole list one `a(ssqass)`. A profile no
/// instance runs names no instance: the empty string, which the client reads
/// as none.
type ServerProfileRow = (String, String, u16, Vec<String>, String);

/// One row of `InstanceProfiles`, as the bus answers it: `(s id,
/// s server_profile, as entries)`, the whole list one `a(ssas)`.
type InstanceProfileRow = (String, String, Vec<String>);

/// One row of `Devices`, as the bus answers it: `(s id, s name, s backend,
/// t total_mib, t free_mib)`, the list beside the moment of the probe and the
/// host's inventory: `(t probed_at, a(ssstt) devices, t ram_mib, u cpu_threads)`.
/// The sizes are uint64: a card's total in
/// MiB grows past a uint16, and a signed type has no reading for a device
/// with no memory. A probe that answered nothing is 0 with an empty list.
type DeviceRow = (String, String, String, u64, u64);

/// The `ModelInfo` reply as the bus answers it:
/// `(s model, (b has_info, t block_count, b moe, t expert_count,
/// t expert_used), (s key, as args))`. The bus has no null, so the path is
/// the empty string when the profile names none, `has_info` says the four
/// counts are there, and a non-empty `key` is the keyed why. The half of the
/// reply that is not there carries zeros and the empty strings.
type ModelInfoRow = (
    String,
    (bool, u64, bool, u64, u64, Vec<(String, u64)>, bool),
    (String, Vec<String>),
);

/// The manager's client over one D-Bus connection.
pub struct DbusClient {
    conn: Connection,
    /// The signed-in user this client vouches for, None when it acts as its
    /// own caller: a guarded method then names the `As` form of the method,
    /// with the name as its first argument.
    principal: Option<String>,
}

impl DbusClient {
    /// A client over `conn` (a bus or a peer-to-peer connection).
    pub fn new(conn: Connection) -> DbusClient {
        DbusClient {
            conn,
            principal: None,
        }
    }

    /// A client on the system bus; no bus is [`ClientError::Unavailable`].
    pub fn connect_system() -> Result<DbusClient, ClientError> {
        Connection::system()
            .map(DbusClient::new)
            .map_err(|_| ClientError::Unavailable)
    }

    /// The connection under the client: for a call the client itself does
    /// not wrap (a method of a sibling interface, called by name).
    pub fn connection(&self) -> &Connection {
        &self.conn
    }

    /// A client of the same connection whose guarded calls act for
    /// `principal`, the signed-in user this caller vouches for.
    ///
    /// The connection is the same, because it is the caller: the manager
    /// checks the account a call comes from before it looks at the name.
    pub fn acting_for(&self, principal: &str) -> Self {
        Self {
            conn: self.conn.clone(),
            principal: Some(principal.to_string()),
        }
    }

    /// The manager's framework version, beside the protocol number of the
    /// `As` methods.
    ///
    /// A manager that answers no `Version` (one built before the method) is
    /// `Ok(None)`: the version is a question, never a call that must be
    /// allowed.
    pub fn version(&self) -> Result<Option<ManagerVersion>, ClientError> {
        match self.call_raw("Version", &()) {
            Ok(reply) => {
                let (version, protocol) = body(reply)?;
                Ok(Some(ManagerVersion { version, protocol }))
            }
            // The one transport error that is an answer: the served object has
            // no such method, so the manager predates it. Nothing else is: a
            // refused, timed out or missing call stays an error.
            Err(zbus::Error::MethodError(name, _, _)) if name.as_str() == UNKNOWN_METHOD => {
                Ok(None)
            }
            Err(e) => Err(error(e)),
        }
    }

    /// The reply to `method` with `body`.
    fn call<B: Serialize + DynamicType>(
        &self,
        method: &str,
        body: &B,
    ) -> Result<Message, ClientError> {
        self.call_raw(method, body).map_err(error)
    }

    /// The reply to `method` with `body`, with the transport's own error:
    /// [`DbusClient::version`] reads an error *name* out of it, as the pipe's
    /// `version` reads the reply header.
    fn call_raw<B: Serialize + DynamicType>(
        &self,
        method: &str,
        body: &B,
    ) -> Result<Message, zbus::Error> {
        self.conn
            .call_method(Some(BUS_NAME), OBJECT_PATH, Some(INTERFACE), method, body)
    }

    /// `method` on an instance id, with no reply value, as the client's own
    /// caller.
    fn on_instance(&self, method: &str, id: &str) -> Result<(), ClientError> {
        self.call(method, &(id,)).map(|_| ())
    }

    /// The acting name, when this client vouches for a principal.
    fn acting(&self) -> Option<&str> {
        self.principal.as_deref()
    }

    /// The three states of the apply of every instance, as the sibling method
    /// of `Instances` answers them (IP3c review 6): the manager derives them
    /// from the files, so it is a call of its own beside the list. A manager
    /// that answers no `InstanceStates` (one built before the method) is read
    /// as answering no states, exactly as [`DbusClient::version`] reads the
    /// method it lacks: the rows beside it read converged, with no reason and
    /// no drift.
    fn instance_states(&self) -> Result<Vec<StateRow>, ClientError> {
        match self.call_raw("InstanceStates", &()) {
            Ok(reply) => body(reply),
            Err(zbus::Error::MethodError(name, _, _)) if name.as_str() == UNKNOWN_METHOD => {
                Ok(Vec::new())
            }
            Err(e) => Err(error(e)),
        }
    }
}

/// The error a failed call stands for: an error reply by its name, a
/// connection that is gone as no manager to ask.
fn error(e: zbus::Error) -> ClientError {
    use zbus::DBusError;
    match e {
        zbus::Error::MethodError(name, detail, _) => {
            ClientError::from_reply(name.as_str(), detail.as_deref().unwrap_or(""))
        }
        zbus::Error::FDO(e) => {
            ClientError::from_reply(e.name().as_str(), e.description().unwrap_or(""))
        }
        zbus::Error::InputOutput(_) => ClientError::Unavailable,
        other => ClientError::Other(other.to_string()),
    }
}

/// The row of the node's GPU list (AN3, `ARCHITECTURE.md` "About and node
/// info") as the client reads it: `(s pci, s vendor, s name, s driver,
/// s version, b has_vulkan, s v_driver, s v_info)`. The bus has no null, so
/// an absent text is the empty string, and the Vulkan pair travels only with
/// its flag: an empty driver name is not no Vulkan device.
type NodeGpuRow = (String, String, String, String, String, bool, String, String);

/// The whole `NodeInfo` reply as the client reads it: `(s name, s version,
/// s kernel, s vulkan, a(sssssbss) gpus)`, the kernel's rows first, then a
/// Vulkan device no kernel row carries.
type NodeInfoRow = (String, String, String, String, Vec<NodeGpuRow>);

/// One row of `ModelFiles`' list (M3, `ARCHITECTURE.md`) as the client reads
/// it: `(s path, t size, t shards, b has_category, s category)`. The bus has no
/// null, so the category travels with its flag, the way `NodeGpuRow`'s Vulkan
/// pair does: a file of the folder itself is the flag false and the empty
/// string.
type ModelFileRowWire = (String, u64, u64, bool, String);

/// The whole `ModelFiles` reply as the client reads it: `(s dir, a(sttbs)
/// files, b truncated, (s key, as args))`, the last part the folder's keyed
/// why, its `key` empty when the folder read.
type ModelFilesRow = (String, Vec<ModelFileRowWire>, bool, (String, Vec<String>));

/// The text of the wire, `None` when it is empty: the bus carries no null, so
/// the empty string is the fact the host did not answer, the way the stream's
/// `null` says it.
fn text(value: String) -> Option<String> {
    (!value.is_empty()).then_some(value)
}

/// The body of `reply`; one that is not `T` is [`ClientError::Other`].
fn body<T>(reply: Message) -> Result<T, ClientError>
where
    T: for<'d> zbus::export::serde::Deserialize<'d> + zbus::zvariant::Type,
{
    reply
        .body()
        .deserialize()
        .map_err(|e| ClientError::Other(e.to_string()))
}

/// The answer of a save that re-renders, as the bus carries it (IP3c,
/// `ARCHITECTURE.md` "Convergence"): the `a{sv}` of [`affected_of`].
type AffectedFields = std::collections::HashMap<String, zbus::zvariant::OwnedValue>;

/// The value a variant carries. The bus wraps every value of an `a{sv}` and
/// every element of an `av` in a variant, so the rows and their fields arrive
/// as `Value::Value`, and the field is read through the wrapper.
fn plain<'a>(value: &'a zbus::zvariant::Value<'a>) -> &'a zbus::zvariant::Value<'a> {
    match value {
        zbus::zvariant::Value::Value(inner) => plain(inner),
        other => other,
    }
}

/// A field's text, the empty string for a field that is no text: the row's own
/// fields are read by name, as every other row of the bus reads them.
fn field_text(value: &zbus::zvariant::Value<'_>) -> String {
    match plain(value) {
        zbus::zvariant::Value::Str(text) => text.as_str().to_string(),
        _ => String::new(),
    }
}

/// A field's list of texts, empty for a field that is no list.
fn field_texts(value: &zbus::zvariant::Value<'_>) -> Vec<String> {
    match plain(value) {
        zbus::zvariant::Value::Array(list) => list.inner().iter().map(field_text).collect(),
        _ => Vec::new(),
    }
}

/// One row of a save's `notConverged` (an `a{sv}` naming the instance, the key
/// of the check that stopped its pair and that check's args).
fn not_converged_row(value: &zbus::zvariant::Value<'_>) -> NotConvergedRow {
    let mut row = NotConvergedRow::default();
    if let zbus::zvariant::Value::Dict(fields) = plain(value) {
        for (key, field) in fields.iter() {
            match field_text(key).as_str() {
                "instance" => row.instance = field_text(field),
                "key" => row.key = field_text(field),
                "args" => row.args = field_texts(field),
                _ => {}
            }
        }
    }
    row
}

/// The reply of a save that re-renders, read back into [`Affected`] (IP4a):
/// `running`, the ids of the running instances the write left drifting, and
/// `notConverged`, one row per instance that kept its last good pair. The two
/// fields are the shape every such save answers (the manager answers both,
/// empty when it moved nothing), so a reply that carries neither list is no
/// answer: the rows are never read out of an answer that has none.
fn affected_of(reply: Message) -> Result<Affected, ClientError> {
    use zbus::zvariant::{OwnedValue, Value};
    let mut fields: AffectedFields = body(reply)?;
    let mut list = |name: &str| -> Result<Vec<OwnedValue>, ClientError> {
        match fields.remove(name) {
            Some(value) => match plain(&value) {
                Value::Array(list) => list
                    .inner()
                    .to_vec()
                    .into_iter()
                    .map(OwnedValue::try_from)
                    .collect::<Result<Vec<_>, _>>()
                    .map_err(|e| ClientError::Other(format!("{name}: {e}"))),
                other => Err(ClientError::Other(format!("{name} is no list: {other}"))),
            },
            None => Err(ClientError::Other(format!("the answer carries no {name}"))),
        }
    };
    let running = list("running")?
        .into_iter()
        .map(|id| {
            String::try_from(id)
                .map_err(|e| ClientError::Other(format!("a running id is no text: {e}")))
        })
        .collect::<Result<Vec<_>, ClientError>>()?;
    let mut not_converged = Vec::new();
    for row in list("notConverged")? {
        not_converged.push(not_converged_row(&row));
    }
    Ok(Affected {
        running,
        not_converged,
    })
}

impl ManagerClient for DbusClient {
    fn list_instances(&self) -> Result<Vec<String>, ClientError> {
        body(self.call("ListInstances", &())?)
    }
    fn status(&self, id: &str) -> Result<InstanceState, ClientError> {
        let name: String = body(self.call("Status", &(id,))?)?;
        Ok(InstanceState::from_name(&name))
    }
    fn start(&self, id: &str) -> Result<(), ClientError> {
        match self.acting() {
            Some(principal) => self.call("StartAs", &(principal, id)).map(|_| ()),
            None => self.on_instance("Start", id),
        }
    }
    fn stop(&self, id: &str) -> Result<(), ClientError> {
        match self.acting() {
            Some(principal) => self.call("StopAs", &(principal, id)).map(|_| ()),
            None => self.on_instance("Stop", id),
        }
    }
    fn restart(&self, id: &str) -> Result<(), ClientError> {
        match self.acting() {
            Some(principal) => self.call("RestartAs", &(principal, id)).map(|_| ()),
            None => self.on_instance("Restart", id),
        }
    }
    /// The manager answers (exists, text, withheld): a missing file is
    /// (false, "", false).
    fn read_file(&self, name: &str) -> Result<Option<ManagerFile>, ClientError> {
        let reply = match self.acting() {
            Some(principal) => self.call("ReadFileAs", &(principal, name))?,
            None => self.call("ReadFile", &(name,))?,
        };
        let (exists, text, withheld): (bool, String, bool) = body(reply)?;
        Ok(exists.then_some(ManagerFile { text, withheld }))
    }
    fn write_file(
        &self,
        name: &str,
        contents: &str,
        keep_api_key: bool,
    ) -> Result<(), ClientError> {
        match self.acting() {
            Some(principal) => self.call("WriteFileAs", &(principal, name, contents, keep_api_key)),
            None => self.call("WriteFile", &(name, contents, keep_api_key)),
        }
        .map(|_| ())
    }
    fn write_file_if(
        &self,
        name: &str,
        contents: &str,
        expected: &str,
    ) -> Result<Affected, ClientError> {
        let reply = match self.acting() {
            Some(principal) => self.call("WriteFileIfAs", &(principal, name, contents, expected)),
            None => self.call("WriteFileIf", &(name, contents, expected)),
        }?;
        affected_of(reply)
    }
    fn boot_enabled(&self, id: &str) -> Result<bool, ClientError> {
        body(self.call("BootEnabled", &(id,))?)
    }
    fn set_boot_enabled(&self, id: &str, enabled: bool) -> Result<(), ClientError> {
        match self.acting() {
            Some(principal) => self.call("SetBootEnabledAs", &(principal, id, enabled)),
            None => self.call("SetBootEnabled", &(id, enabled)),
        }
        .map(|_| ())
    }
    fn set_model_profile(
        &self,
        instance: &str,
        profile: &str,
        value: &str,
    ) -> Result<(), ClientError> {
        match self.acting() {
            Some(principal) => {
                self.call("SetModelProfileAs", &(principal, instance, profile, value))
            }
            None => self.call("SetModelProfile", &(instance, profile, value)),
        }
        .map(|_| ())
    }
    fn set_server_profile(&self, instance: &str, profile: &str) -> Result<(), ClientError> {
        match self.acting() {
            Some(principal) => self.call("SetServerProfileAs", &(principal, instance, profile)),
            None => self.call("SetServerProfile", &(instance, profile)),
        }
        .map(|_| ())
    }
    fn create_server_profile(
        &self,
        engine: &str,
        profile: &str,
        port: Option<u16>,
    ) -> Result<(), ClientError> {
        // No port is 0 on the wire: the file the manager writes carries no
        // `Port` key.
        let port = port.unwrap_or(0);
        match self.acting() {
            Some(principal) => {
                self.call("CreateServerProfileAs", &(principal, engine, profile, port))
            }
            None => self.call("CreateServerProfile", &(engine, profile, port)),
        }
        .map(|_| ())
    }
    fn delete_server_profile(&self, engine: &str, profile: &str) -> Result<(), ClientError> {
        match self.acting() {
            Some(principal) => self.call("DeleteServerProfileAs", &(principal, engine, profile)),
            None => self.call("DeleteServerProfile", &(engine, profile)),
        }
        .map(|_| ())
    }
    // The instance profile verbs, mirroring the server profile pair's shape,
    // the `As` form carrying the principal first. The two reads are guarded
    // too: the pool is a `configure` read, so a caller vouching for a signed-in
    // user asks for that user.
    fn instance_profiles(&self) -> Result<Vec<InstanceProfileInfo>, ClientError> {
        let reply = match self.acting() {
            Some(principal) => self.call("InstanceProfilesAs", &(principal,))?,
            None => self.call("InstanceProfiles", &())?,
        };
        let rows: Vec<InstanceProfileRow> = body(reply)?;
        Ok(rows
            .into_iter()
            .map(|(id, server_profile, entries)| InstanceProfileInfo {
                id,
                server_profile,
                entries,
                // The pool has one text: the list answers no revision.
                revision: String::new(),
            })
            .collect())
    }
    fn instance_profile(&self, profile: &str) -> Result<InstanceProfileInfo, ClientError> {
        let reply = match self.acting() {
            Some(principal) => self.call("GetInstanceProfileAs", &(principal, profile))?,
            None => self.call("GetInstanceProfile", &(profile,))?,
        };
        let (server_profile, entries, revision): (String, Vec<String>, String) = body(reply)?;
        Ok(InstanceProfileInfo {
            id: profile.to_string(),
            server_profile,
            entries,
            revision,
        })
    }
    fn create_instance_profile(
        &self,
        profile: &str,
        server_profile: &str,
    ) -> Result<(), ClientError> {
        match self.acting() {
            Some(principal) => self.call(
                "CreateInstanceProfileAs",
                &(principal, profile, server_profile),
            ),
            None => self.call("CreateInstanceProfile", &(profile, server_profile)),
        }
        .map(|_| ())
    }
    fn delete_instance_profile(&self, profile: &str) -> Result<(), ClientError> {
        match self.acting() {
            Some(principal) => self.call("DeleteInstanceProfileAs", &(principal, profile)),
            None => self.call("DeleteInstanceProfile", &(profile,)),
        }
        .map(|_| ())
    }
    fn add_instance_profile_model(
        &self,
        profile: &str,
        model: &str,
    ) -> Result<Affected, ClientError> {
        let reply = match self.acting() {
            Some(principal) => self.call("AddInstanceProfileModelAs", &(principal, profile, model)),
            None => self.call("AddInstanceProfileModel", &(profile, model)),
        }?;
        affected_of(reply)
    }
    fn remove_instance_profile_model(
        &self,
        profile: &str,
        model: &str,
    ) -> Result<Affected, ClientError> {
        let reply = match self.acting() {
            Some(principal) => {
                self.call("RemoveInstanceProfileModelAs", &(principal, profile, model))
            }
            None => self.call("RemoveInstanceProfileModel", &(profile, model)),
        }?;
        affected_of(reply)
    }
    /// `SetInstanceProfileModelAs` / `SetInstanceProfileModel`: the entry whole,
    /// as `Vec<(String, String)>`, beside the revision it swaps against. The
    /// entry belongs to the profile, so the save names no instance: every
    /// instance running the profile renders again, and the save answers the rows
    /// that write left ([`affected_of`]).
    fn set_instance_profile_model(
        &self,
        profile: &str,
        model: &str,
        values: &[(String, String)],
        revision: &str,
    ) -> Result<Affected, ClientError> {
        let reply = match self.acting() {
            Some(principal) => self.call(
                "SetInstanceProfileModelAs",
                &(principal, profile, model, values, revision),
            ),
            None => self.call(
                "SetInstanceProfileModel",
                &(profile, model, values, revision),
            ),
        }?;
        affected_of(reply)
    }
    /// `SetInstanceProfileServerAs` / `SetInstanceProfileServer`: the switch of
    /// an instance profile to another server profile, answering the rows that
    /// write left, the instances whose card the switch took away among them.
    fn set_instance_profile_server(
        &self,
        profile: &str,
        server_profile: &str,
    ) -> Result<Affected, ClientError> {
        let reply = match self.acting() {
            Some(principal) => self.call(
                "SetInstanceProfileServerAs",
                &(principal, profile, server_profile),
            ),
            None => self.call("SetInstanceProfileServer", &(profile, server_profile)),
        }?;
        affected_of(reply)
    }
    fn set_instance_profile(
        &self,
        instance: &str,
        profile: &str,
        revision: &str,
    ) -> Result<Affected, ClientError> {
        let reply = match self.acting() {
            Some(principal) => self.call(
                "SetInstanceProfileAs",
                &(principal, instance, profile, revision),
            ),
            None => self.call("SetInstanceProfile", &(instance, profile, revision)),
        }?;
        affected_of(reply)
    }
    fn set_instance_address(
        &self,
        instance: &str,
        host: &str,
        port: &str,
        revision: &str,
    ) -> Result<Affected, ClientError> {
        let reply = match self.acting() {
            Some(principal) => self.call(
                "SetInstanceAddressAs",
                &(principal, instance, host, port, revision),
            ),
            None => self.call("SetInstanceAddress", &(instance, host, port, revision)),
        }?;
        affected_of(reply)
    }
    fn create_model_profile(
        &self,
        profile: &str,
        source: &ModelProfileSource,
    ) -> Result<(), ClientError> {
        let (from, values) = source.as_source();
        match self.acting() {
            Some(principal) => {
                self.call("CreateModelProfileAs", &(principal, profile, from, values))
            }
            None => self.call("CreateModelProfile", &(profile, from, values)),
        }
        .map(|_| ())
    }
    fn delete_model_profile(&self, profile: &str) -> Result<Affected, ClientError> {
        let reply = match self.acting() {
            Some(principal) => self.call("DeleteModelProfileAs", &(principal, profile)),
            None => self.call("DeleteModelProfile", &(profile,)),
        }?;
        affected_of(reply)
    }
    /// `CreateInstance`: the engine, the id, and the instance profile the new
    /// instance runs, the third name the manager's rule judges (IP4b). The
    /// reply is the port the manager proposed for the new instance.
    fn create_instance(
        &self,
        engine: &str,
        id: &str,
        instance_profile: &str,
    ) -> Result<u16, ClientError> {
        let reply = match self.acting() {
            Some(principal) => self.call(
                "CreateInstanceAs",
                &(principal, engine, id, instance_profile),
            ),
            None => self.call("CreateInstance", &(engine, id, instance_profile)),
        }?;
        body(reply)
    }
    fn remove_instance(&self, id: &str) -> Result<(), ClientError> {
        match self.acting() {
            Some(principal) => self.call("RemoveInstanceAs", &(principal, id)),
            None => self.call("RemoveInstance", &(id,)),
        }
        .map(|_| ())
    }
    fn instances(&self) -> Result<Vec<InstanceInfo>, ClientError> {
        let rows: Vec<InstanceRow> = body(self.call("Instances", &())?)?;
        let states = self.instance_states()?;
        Ok(rows
            .into_iter()
            // A row naming no server profile is a section that names no
            // server file: no instance to act on, and its row is left out, as
            // over the pipe.
            .filter(|row| !row.2.is_empty())
            .map(
                |(
                    id,
                    engine,
                    server_profile,
                    port,
                    profile_tags,
                    instance_tags,
                    label,
                    instance_profile,
                    hostname,
                )| {
                    // The states of the apply ride the sibling method, keyed by
                    // the id (IP3c review 6). A row the states do not name
                    // reads converged, with no reason and no drift.
                    let state = states.iter().find(|state| state.0 == id);
                    InstanceInfo {
                        id,
                        engine,
                        server_profile: Some(server_profile),
                        port,
                        profile_tags,
                        instance_tags,
                        // The bus has no null: the empty string is "this
                        // section has no label".
                        label: (!label.is_empty()).then_some(label),
                        converged: state.is_some_and(|state| state.1),
                        // Likewise the empty reason key is no reason.
                        reason: state.and_then(|state| {
                            (!state.2.is_empty()).then(|| NotConverged {
                                key: state.2.clone(),
                                args: state.3.clone(),
                            })
                        }),
                        pending_restart: state.is_some_and(|state| state.4),
                        // The two appended fields (IP4b), each the empty string
                        // when the section names none.
                        instance_profile: (!instance_profile.is_empty())
                            .then_some(instance_profile),
                        hostname: (!hostname.is_empty()).then_some(hostname),
                    }
                },
            )
            .collect())
    }
    fn server_profiles(&self) -> Result<Vec<ServerProfileInfo>, ClientError> {
        let rows: Vec<ServerProfileRow> = body(self.call("ServerProfiles", &())?)?;
        Ok(rows
            .into_iter()
            .map(|(id, engine, port, tags, instance)| ServerProfileInfo {
                id,
                engine,
                port,
                tags,
                instance: (!instance.is_empty()).then_some(instance),
            })
            .collect())
    }
    fn set_instance_tags(&self, instance: &str, tags: &[String]) -> Result<(), ClientError> {
        match self.acting() {
            Some(principal) => self.call("SetInstanceTagsAs", &(principal, instance, tags)),
            None => self.call("SetInstanceTags", &(instance, tags)),
        }
        .map(|_| ())
    }
    fn set_instance_label(&self, instance: &str, label: &str) -> Result<(), ClientError> {
        match self.acting() {
            Some(principal) => self.call("SetInstanceLabelAs", &(principal, instance, label)),
            None => self.call("SetInstanceLabel", &(instance, label)),
        }
        .map(|_| ())
    }
    fn devices(&self) -> Result<NodeDevices, ClientError> {
        // `(t probed_at, a(ssstt) devices, t ram_mib, u cpu_threads)`: the
        // list with the host's inventory beside it (R1), the RAM `t` like the
        // card sizes, the thread count a `u`. The inventory is the host's, so
        // it is answered whatever the probe did, 0 for "no figure".
        let (probed_at, rows, ram_mib, cpu_threads): (u64, Vec<DeviceRow>, u64, u32) =
            match self.acting() {
                Some(principal) => body(self.call("DevicesAs", &(principal,))?)?,
                None => body(self.call("Devices", &())?)?,
            };
        Ok(NodeDevices {
            // 0 on the wire is no probe, as the pipe reads it.
            probed_at: (probed_at != 0).then_some(probed_at),
            ram_mib,
            cpu_threads,
            devices: rows
                .into_iter()
                .map(|(id, name, backend, total_mib, free_mib)| DeviceInfo {
                    id,
                    name,
                    backend,
                    total_mib,
                    free_mib,
                })
                .collect(),
        })
    }
    fn model_info(&self, profile: &str) -> Result<ModelInfoReply, ClientError> {
        // `(s model, (b has_info, t block_count, b moe, t expert_count,
        // t expert_used, a(st) tensors, b tensors_truncated),
        // (s key, as args))`: the path the read opened, the four counts and the
        // tensor list when there are facts, the keyed why when there are
        // none. The bus has no null and no shape to get wrong: the flags say
        // which half of the reply is there, the way the stream's nulls do,
        // and the manager answers exactly one of the two.
        let (model, facts, reason): ModelInfoRow = match self.acting() {
            Some(principal) => body(self.call("ModelInfoAs", &(principal, profile))?)?,
            None => body(self.call("ModelInfo", &(profile,))?)?,
        };
        let (has_info, block_count, moe, expert_count, expert_used, tensors, tensors_truncated) =
            facts;
        let (key, args) = reason;
        Ok(ModelInfoReply {
            model: (!model.is_empty()).then_some(model),
            info: has_info.then_some(ModelFacts {
                block_count,
                moe,
                expert_count,
                expert_used,
                tensors: tensors
                    .into_iter()
                    .map(|(name, bytes)| TensorRow { name, bytes })
                    .collect(),
                tensors_truncated,
            }),
            reason: (!key.is_empty()).then_some(Reason { key, args }),
        })
    }
    fn authenticate(&self, user: &str, password: &str) -> Result<(bool, bool), ClientError> {
        body(self.call("Authenticate", &(user, password))?)
    }
    fn node_info(&self) -> Result<NodeInfoReply, ClientError> {
        // `(s name, s version, s kernel, s vulkan, a(sssssbss) gpus)`: the
        // three OS texts, the Vulkan word, the rows (AN3). The bus has no
        // null: an empty text is a fact the host did not answer, the way the
        // stream reads its `null`, and a row's Vulkan pair travels only with
        // its flag. A word that is not one of the three is a manager of
        // another protocol, read as `failed`, the way the child's parser
        // reads a document with no contract word.
        let (name, version, kernel, vulkan, rows): NodeInfoRow = match self.acting() {
            Some(principal) => body(self.call("NodeInfoAs", &(principal,))?)?,
            None => body(self.call("NodeInfo", &())?)?,
        };
        Ok(NodeInfoReply {
            os: OsInfo {
                name: text(name),
                version: text(version),
                kernel: text(kernel),
            },
            vulkan: status_of(&vulkan).unwrap_or(Status::Failed),
            gpus: rows
                .into_iter()
                .map(
                    |(pci, vendor, name, driver, version, has_vulkan, v_driver, v_info)| NodeGpu {
                        pci: text(pci),
                        vendor: text(vendor),
                        name: text(name),
                        driver: text(driver),
                        version: text(version),
                        vulkan: has_vulkan.then_some(Driver {
                            driver: text(v_driver),
                            info: text(v_info),
                        }),
                    },
                )
                .collect(),
        })
    }
    fn model_files(&self) -> Result<ModelFilesReply, ClientError> {
        // `(s dir, a(sttbs) files, b truncated, (s key, as args))`: the folder
        // as an absolute path, the rows in the walk's order, the flag saying
        // the walk stopped at its bound, and the folder's keyed why (M3). The
        // bus has no null: the category travels with its flag, and the why with
        // its key, an empty key being the folder having read, the way the
        // stream's `null` says it.
        let (dir, rows, truncated, (key, args)): ModelFilesRow = match self.acting() {
            Some(principal) => body(self.call("ModelFilesAs", &(principal,))?)?,
            None => body(self.call("ModelFiles", &())?)?,
        };
        Ok(ModelFilesReply {
            dir,
            files: rows
                .into_iter()
                .map(
                    |(path, size, shards, has_category, category)| ModelFileRow {
                        path,
                        size,
                        shards,
                        category: has_category.then_some(category),
                    },
                )
                .collect(),
            truncated,
            error: (!key.is_empty()).then_some(Reason { key, args }),
        })
    }
}

#[cfg(test)]
#[path = "dbus_tests.rs"]
mod tests;
