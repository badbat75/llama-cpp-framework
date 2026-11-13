//! [`ManagerClient`] over D-Bus: interface `org.privateai.Manager1` at
//! `/org/privateai/Manager1`, on the system bus or on a given connection.

use super::{ClientError, InstanceState, ManagerClient};
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

/// The manager's client over one D-Bus connection.
pub struct DbusClient {
    conn: Connection,
}

impl DbusClient {
    /// A client over `conn` (a bus or a peer-to-peer connection).
    pub fn new(conn: Connection) -> DbusClient {
        DbusClient { conn }
    }

    /// A client on the system bus; no bus is [`ClientError::Unavailable`].
    pub fn connect_system() -> Result<DbusClient, ClientError> {
        Connection::system()
            .map(DbusClient::new)
            .map_err(|_| ClientError::Unavailable)
    }

    /// The reply to `method` with `body`.
    fn call<B: Serialize + DynamicType>(
        &self,
        method: &str,
        body: &B,
    ) -> Result<Message, ClientError> {
        self.conn
            .call_method(Some(BUS_NAME), OBJECT_PATH, Some(INTERFACE), method, body)
            .map_err(error)
    }

    /// `method` on an instance id, with no reply value.
    fn on_instance(&self, method: &str, id: &str) -> Result<(), ClientError> {
        self.call(method, &(id,)).map(|_| ())
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
        self.on_instance("Start", id)
    }
    fn stop(&self, id: &str) -> Result<(), ClientError> {
        self.on_instance("Stop", id)
    }
    fn restart(&self, id: &str) -> Result<(), ClientError> {
        self.on_instance("Restart", id)
    }
    /// The manager answers (exists, text): a missing file is (false, "").
    fn read_file(&self, name: &str) -> Result<Option<String>, ClientError> {
        let (exists, text): (bool, String) = body(self.call("ReadFile", &(name,))?)?;
        Ok(exists.then_some(text))
    }
    fn write_file(&self, name: &str, contents: &str) -> Result<(), ClientError> {
        self.call("WriteFile", &(name, contents)).map(|_| ())
    }
}
