//! [`ManagerClient`] over D-Bus: interface `org.privateai.Manager1` at
//! `/org/privateai/Manager1`, on the system bus or on a given connection.
//!
//! A client [`DbusClient::acting_for`] a signed-in user vouches for: `Start`,
//! `Stop`, `Restart`, `ReadFile`, `WriteFile`, `WriteFileIf`,
//! `SetBootEnabled`, `SetModelProfile`, `SetServerProfile` and
//! `SetInstanceTags` go as the
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
//! SetInstanceTagsAs(s principal, s instance, as tags)
//! Instances() -> a(ssqasas)           (id, engine, port, profile tags, instance tags)
//! Version() -> (s version, u protocol)
//! ```

use super::{ClientError, InstanceInfo, InstanceState, ManagerClient, ManagerFile, ManagerVersion};
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

/// One row of `Instances`, as the bus answers it: `(s id, s engine, q port,
/// as profile_tags, as instance_tags)`, the whole list one `a(ssqasas)`.
type InstanceRow = (String, String, u16, Vec<String>, Vec<String>);

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
    fn instances(&self) -> Result<Vec<InstanceInfo>, ClientError> {
        let rows: Vec<InstanceRow> = body(self.call("Instances", &())?)?;
        Ok(rows
            .into_iter()
            .map(
                |(id, engine, port, profile_tags, instance_tags)| InstanceInfo {
                    id,
                    engine,
                    port,
                    profile_tags,
                    instance_tags,
                },
            )
            .collect())
    }
    fn set_instance_tags(&self, instance: &str, tags: &[String]) -> Result<(), ClientError> {
        match self.acting() {
            Some(principal) => self.call("SetInstanceTagsAs", &(principal, instance, tags)),
            None => self.call("SetInstanceTags", &(instance, tags)),
        }
        .map(|_| ())
    }
    fn authenticate(&self, user: &str, password: &str) -> Result<(bool, bool), ClientError> {
        body(self.call("Authenticate", &(user, password))?)
    }
}
