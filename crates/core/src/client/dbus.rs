//! [`ManagerClient`] over D-Bus: interface `org.privateai.Manager1` at
//! `/org/privateai/Manager1`, on the system bus or on a given connection.
//!
//! A client [`DbusClient::acting_for`] a signed-in user vouches for: `Start`,
//! `Stop`, `Restart`, `ReadFile`, `WriteFile`, `WriteFileIf`,
//! `SetBootEnabled`, `SetModelProfile`, `SetServerProfile`,
//! `CreateServerProfile`, `DeleteServerProfile` and
//! `SetInstanceTags` and `ModelInfo` and `NodeInfo` go as the
//! `As` form of the same method, with the user's name as its first argument
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
//! ModelInfo(s profile) -> (s model, (b has_info, t block_count, b moe,
//!                                    t expert_count, t expert_used),
//!                          (s key, as args))
//! ModelInfoAs(s principal, s profile)    (the same reply)
//! NodeInfo() -> (s name, s version, s kernel, s vulkan, a(sssssbss) gpus)
//! NodeInfoAs(s principal)                (the same reply)
//! Instances() -> a(sssqasas)          (id, engine, port, profile tags, instance tags)
//! ServerProfiles() -> a(ssqass)       (id, engine, port, tags, instance)
//! Version() -> (s version, u protocol)
//! ```

use super::{
    ClientError, DeviceInfo, InstanceInfo, InstanceState, ManagerClient, ManagerFile,
    ManagerVersion, ModelFacts, ModelInfoReply, NodeDevices, NodeInfoReply, Reason,
    ServerProfileInfo,
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
/// s server_profile, q port, as profile_tags, as instance_tags)`, the whole
/// list one `a(sssqasas)`. A section naming no server profile is the empty
/// string, which the client reads as none.
type InstanceRow = (String, String, String, u16, Vec<String>, Vec<String>);

/// One row of `ServerProfiles`, as the bus answers it: `(s id, s engine,
/// q port, as tags, s instance)`, the whole list one `a(ssqass)`. A profile no
/// instance runs names no instance: the empty string, which the client reads
/// as none.
type ServerProfileRow = (String, String, u16, Vec<String>, String);

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
type ModelInfoRow = (String, (bool, u64, bool, u64, u64), (String, Vec<String>));

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
    fn write_file_if(&self, name: &str, contents: &str, expected: &str) -> Result<(), ClientError> {
        match self.acting() {
            Some(principal) => self.call("WriteFileIfAs", &(principal, name, contents, expected)),
            None => self.call("WriteFileIf", &(name, contents, expected)),
        }
        .map(|_| ())
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
    fn create_instance(
        &self,
        engine: &str,
        id: &str,
        server_profile: &str,
    ) -> Result<(), ClientError> {
        match self.acting() {
            Some(principal) => {
                self.call("CreateInstanceAs", &(principal, engine, id, server_profile))
            }
            None => self.call("CreateInstance", &(engine, id, server_profile)),
        }
        .map(|_| ())
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
        Ok(rows
            .into_iter()
            // A row naming no server profile is a section that names no
            // server file: no instance to act on, and its row is left out, as
            // over the pipe.
            .filter(|row| !row.2.is_empty())
            .map(
                |(id, engine, server_profile, port, profile_tags, instance_tags)| InstanceInfo {
                    id,
                    engine,
                    server_profile: Some(server_profile),
                    port,
                    profile_tags,
                    instance_tags,
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
        // t expert_used), (s key, as args))`: the path the read opened, the
        // four counts when there are facts, the keyed why when there are
        // none. The bus has no null and no shape to get wrong: the flags say
        // which half of the reply is there, the way the stream's nulls do,
        // and the manager answers exactly one of the two.
        let (model, facts, reason): ModelInfoRow = match self.acting() {
            Some(principal) => body(self.call("ModelInfoAs", &(principal, profile))?)?,
            None => body(self.call("ModelInfo", &(profile,))?)?,
        };
        let (has_info, block_count, moe, expert_count, expert_used) = facts;
        let (key, args) = reason;
        Ok(ModelInfoReply {
            model: (!model.is_empty()).then_some(model),
            info: has_info.then_some(ModelFacts {
                block_count,
                moe,
                expert_count,
                expert_used,
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
}

#[cfg(test)]
mod tests {
    use super::{DeviceRow, InstanceRow};
    use zbus::zvariant::Type;

    /// The row of the node's GPU list (AN3, `ARCHITECTURE.md` "About and node
    /// info"), as the client reads it: `pci` (the bus id to join the Vulkan
    /// device onto, empty when the row is a Vulkan device no kernel row
    /// carries), `vendor` (the 4 hex digits), the product `name`, the kernel
    /// `driver` and its `version`, then the Vulkan pair: `has_vulkan`, its
    /// `driver` and its `info`.
    ///
    /// The bus has no null, so an absent text is the empty string and the
    /// Vulkan pair carries its own flag, the way `ModelInfoRow` says which
    /// half of its reply is there: a driver name the host did not answer is
    /// `""` on the bus and `None` in the reply, and `has_vulkan` false answers
    /// the pair empty, never a row with a Vulkan driver of nothing.
    type NodeGpuRow = (String, String, String, String, String, bool, String, String);

    /// The whole document: `os` name, version and kernel, the `vulkan` word
    /// (`ok`, `none` or `failed`), then the rows. The kernel is the host's own
    /// `uname(2)`, so it travels as text like the other two.
    type NodeInfoRow = (String, String, String, String, Vec<NodeGpuRow>);

    /// The row `Instances` answers, as the client reads it: one D-Bus struct
    /// `(s id, s engine, s server_profile, q port, as profile_tags,
    /// as instance_tags)`, the list `a(sssqasas)`. Slice 13 puts the server
    /// profile after the engine: a client that reads five fields never learns
    /// the name, and the only name left to it is the id, which is not the
    /// file the instance reads.
    #[test]
    fn the_instances_row_is_the_struct_with_the_server_profile() {
        assert_eq!(
            <InstanceRow as Type>::SIGNATURE.to_string(),
            "(sssqasas)",
            "the row is a struct of two strings, the profile, a uint16 and two arrays of strings"
        );
    }

    /// The row of the device list (G2, ARCHITECTURE.md 1, "Wire"): one row of
    /// the `a(ssstt)` `Devices()` answers, the id `device` takes, the product
    /// name, the backend and the two sizes as uint64. The sizes are `t`, not
    /// `q` or `i`: a card's total in MiB grows past a uint16, and a signed type
    /// has no reading for a device with no memory.
    #[test]
    fn the_devices_row_is_the_struct_of_the_wire() {
        assert_eq!(
            <DeviceRow as Type>::SIGNATURE.to_string(),
            "(ssstt)",
            "the row is a struct of three strings and two uint64"
        );
    }

    /// The whole reply, widened by R1 (ARCHITECTURE.md (1) "The node
    /// inventory", slice R1 "Wire"): `(t probed_at, a(ssstt) devices,
    /// t ram_mib, u cpu_threads)`. The RAM is `t` like the card sizes; the
    /// thread count is `u`, a uint32, the width the bff's `cpuThreads` reads.
    /// Additive at the end, so a reader of the list alone reads the fields it
    /// knows and never moves.
    #[test]
    fn the_devices_reply_is_the_list_with_the_inventory() {
        assert_eq!(
            <(u64, Vec<DeviceRow>, u64, u32) as Type>::SIGNATURE.to_string(),
            "(ta(ssstt)tu)",
            "the probe's moment, the rows, the MiB of RAM, the thread count"
        );
    }

    /// The row of the node's GPU list (AN3, `ARCHITECTURE.md` "About and node
    /// info"): `(s pci, s vendor, s name, s driver, s version, b has_vulkan,
    /// s v_driver, s v_info)`, the list `a(sssssbss)`. Every text is `s` and
    /// never a variant: the bus has no null, so the empty string carries "the
    /// host did not answer this", and the Vulkan pair needs its own `b`
    /// because an empty driver name is not the same as no Vulkan device.
    #[test]
    fn the_node_gpu_row_is_the_struct_of_the_wire() {
        assert_eq!(
            <NodeGpuRow as Type>::SIGNATURE.to_string(),
            "(sssssbss)",
            "the five texts, the Vulkan flag, the Vulkan driver and its info"
        );
    }

    /// The whole `NodeInfo()` reply: `(s name, s version, s kernel,
    /// s vulkan, a(sssssbss) gpus)`. The three OS texts, the one Vulkan word
    /// (`ok`, `none` or `failed`), then the rows in the manager's order.
    #[test]
    fn the_node_info_reply_is_the_document_of_the_wire() {
        assert_eq!(
            <NodeInfoRow as Type>::SIGNATURE.to_string(),
            "(ssssa(sssssbss))",
            "the OS, the Vulkan word, the GPU rows"
        );
    }
}
