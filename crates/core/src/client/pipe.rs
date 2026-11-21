//! [`ManagerClient`] over any byte stream: the manager's local named pipe on
//! Windows, a socket pair in the tests. The protocol is OS-neutral.
//!
//! One frame per message: the length of the body as a `u32` little-endian,
//! then the body, UTF-8 JSON, at most [`MAX_FRAME`] bytes. The client sends
//! one request and reads its reply before the next. A request names the
//! method as the D-Bus interface does, its arguments as named fields:
//!
//! ```text
//! {"method":"ListInstances"}
//! {"method":"Status","id":"default"}        (also Start, Stop, Restart)
//! {"method":"ReadFile","name":"models.ini"}
//! {"method":"WriteFile","name":"models.ini","contents":"...","keep_api_key":false}
//! {"method":"WriteFileIf","name":"models.ini","contents":"...","expected":"..."}
//! {"method":"BootEnabled","id":"default"}
//! {"method":"SetBootEnabled","id":"default","enabled":true}
//! {"method":"Instances"}
//! {"method":"SetInstanceTags","instance":"default","tags":["edge","v2"]}
//! {"method":"CreateServerProfile","engine":"llama.cpp","profile":"big","port":8123}
//! {"method":"DeleteServerProfile","engine":"llama.cpp","profile":"big"}
//! {"method":"ServerProfiles"}
//! {"method":"Devices"}
//! {"method":"Authenticate","user":"alice","password":"..."}
//! {"method":"Version"}
//! ```
//!
//! A client [`PipeClient::acting_for`] a signed-in user vouches for: `Start`,
//! `Stop`, `Restart`, `ReadFile`, `WriteFile`, `WriteFileIf`,
//! `SetBootEnabled`, `SetModelProfile`, `SetServerProfile`,
//! `CreateServerProfile`, `DeleteServerProfile`,
//! `SetInstanceTags` and `Devices` go as the
//! `As` form of the same request, with the user's name in a `principal` field
//! (the manager answers a principal for the bff's account alone). The methods
//! that need no right are unchanged, `Instances` and `ServerProfiles` among
//! them.
//!
//! ```text
//! {"method":"StartAs","principal":"alice","id":"default"}
//! {"method":"ReadFileAs","principal":"alice","name":"models.ini"}
//! {"method":"SetBootEnabledAs","principal":"alice","id":"default","enabled":true}
//! {"method":"WriteFileAs","principal":"alice","name":"models.ini",
//!                              "contents":"...","keep_api_key":false}
//! {"method":"WriteFileIfAs","principal":"alice","name":"models.ini",
//!                               "contents":"...","expected":"..."}
//! {"method":"SetInstanceTagsAs","principal":"alice","instance":"default",
//!                                      "tags":["edge","v2"]}
//! {"method":"CreateServerProfileAs","principal":"alice","engine":"llama.cpp",
//!                                         "profile":"big","port":8123}
//! {"method":"DeleteServerProfileAs","principal":"alice","engine":"llama.cpp",
//!                                         "profile":"big"}
//! {"method":"DevicesAs","principal":"alice"}
//! ```
//!
//! A reply is the value or an error with the same (name, message) pair the
//! D-Bus interface replies with, so [`ClientError::from_reply`] reads both:
//!
//! ```text
//! {"ok":["broken","default"]}               ListInstances
//! {"ok":"active"}                           Status
//! {"ok":{"text":"<text>","withheld":false}} ReadFile (withheld: the
//!                                           OpencodeApiKey value blanked)
//! {"ok":null}                               ReadFile, no such file
//! {"ok":true} or {"ok":false}              BootEnabled
//! {"ok":[{"id":"default","engine":"llama.cpp","port":8080,
//!         "profile_tags":["edge"],"instance_tags":[]}]}
//!                                           Instances
//! {"ok":[{"id":"big","engine":"llama.cpp","port":8080,
//!         "tags":["edge"],"instance":"default"}]}
//!                                           ServerProfiles (instance: the
//!                                           empty string when no instance
//!                                           points at the profile)
//! {"ok":{"authenticated":true,"can_configure":false}}
//!                                           Authenticate
//! {"ok":{"probed_at":1766271243,
//!         "devices":[{"id":"CUDA0","name":"NVIDIA GeForce RTX 4070 SUPER",
//!                     "backend":"CUDA","total_mib":12281,"free_mib":10844}]}
//!                                           Devices (probed_at: the Unix
//!                                           seconds of the probe, 0 when no
//!                                           probe answered)
//! {"ok":null}                               Start, Stop, Restart, WriteFile,
//!                                           SetBootEnabled, SetInstanceTags,
//!                                           CreateServerProfile,
//!                                           DeleteServerProfile
//! {"ok":{"version":"1.2.3","protocol":1}}   Version (protocol: the methods
//!                                           this manager speaks)
//! {"error":{"name":"org.privateai.Manager1.Error.<Kind>","message":"..."}}
//! ```

use super::{
    ClientError, DeviceInfo, InstanceInfo, InstanceState, ManagerClient, ManagerFile,
    ManagerVersion, NodeDevices, ServerProfileInfo,
};
use serde_json::{json, Value};
use std::io::{ErrorKind, Read, Write};
use std::sync::{Arc, Mutex};

/// The largest body a frame may carry, either way.
pub const MAX_FRAME: u32 = 1 << 20;

/// The name a manager of this protocol answers a frame it cannot read with
/// (a method it does not know among them), and the stream ends after it.
const BAD_REQUEST: &str = "org.privateai.Manager1.Error.BadRequest";

/// The manager's client over one stream. A stream that breaks (closed,
/// timed out, any I/O error) is [`ClientError::Unavailable`]; a reply that is
/// not one (bad JSON, the wrong value, over [`MAX_FRAME`]) is
/// [`ClientError::Other`].
pub struct PipeClient<S: Read + Write> {
    /// The connection, shared with every client [`PipeClient::acting_for`]
    /// made from this one: one request, one reply, in order.
    stream: Arc<Mutex<S>>,
    /// The signed-in user this client vouches for, None when it acts as its
    /// own caller.
    principal: Option<String>,
}

impl<S: Read + Write> PipeClient<S> {
    pub fn new(stream: S) -> Self {
        Self {
            stream: Arc::new(Mutex::new(stream)),
            principal: None,
        }
    }
}

/// Why a call did not answer with a value.
enum Failure {
    /// The manager's error reply, with its (name, message) as they came: a
    /// caller that knows a name answers it apart (`Version` and
    /// `BadRequest`).
    Reply(String, String),
    /// The client's own failure: the stream, the reply's shape.
    Client(ClientError),
}

impl Failure {
    /// The error this is: the manager's (name, message) read as over D-Bus.
    fn into_client_error(self) -> ClientError {
        match self {
            Failure::Reply(name, message) => ClientError::from_reply(&name, &message),
            Failure::Client(error) => error,
        }
    }
}

impl<S: Read + Write> PipeClient<S> {
    /// Send `request`, read its reply: the value of an `ok`, the manager's
    /// error as it came, or the client's own failure.
    fn call_value(&self, request: Value) -> Result<Value, Failure> {
        let body = request.to_string();
        if body.len() > MAX_FRAME as usize {
            return Err(Failure::Client(ClientError::Other(format!(
                "{} > {MAX_FRAME}",
                body.len()
            ))));
        }
        let mut stream = self.stream.lock().unwrap_or_else(|e| e.into_inner());
        write_frame(&mut *stream, body.as_bytes())
            .map_err(|_| Failure::Client(ClientError::Unavailable))?;
        let body = match read_frame(&mut *stream) {
            Ok(Some(body)) => body,
            Ok(None) | Err(FrameError::Io(_)) => {
                return Err(Failure::Client(ClientError::Unavailable))
            }
            Err(FrameError::TooLong(len)) => {
                return Err(Failure::Client(ClientError::Other(format!(
                    "{len} > {MAX_FRAME}"
                ))))
            }
        };
        let reply: Value = serde_json::from_slice(&body)
            .map_err(|e| Failure::Client(ClientError::Other(e.to_string())))?;
        let other = || Failure::Client(ClientError::Other(reply.to_string()));
        if let Some(error) = reply.get("error") {
            let name = error.get("name").and_then(Value::as_str);
            let message = error.get("message").and_then(Value::as_str);
            return match (name, message) {
                (Some(name), Some(message)) => {
                    Err(Failure::Reply(name.to_string(), message.to_string()))
                }
                _ => Err(other()),
            };
        }
        reply.get("ok").cloned().ok_or_else(other)
    }

    /// Send `request`, read its reply: the value of an `ok`, or the error.
    fn call(&self, request: Value) -> Result<Value, ClientError> {
        self.call_value(request).map_err(Failure::into_client_error)
    }

    /// A call whose reply carries no value.
    fn call_unit(&self, request: Value) -> Result<(), ClientError> {
        match self.call(request)? {
            Value::Null => Ok(()),
            v => Err(ClientError::Other(v.to_string())),
        }
    }

    /// The request of a guarded method: its `As` form with the principal this
    /// client vouches for, the plain form for a client acting as its own
    /// caller. `args` are the method's own fields.
    fn guarded(&self, method: &str, args: &[(&str, Value)]) -> Value {
        let mut request = match &self.principal {
            Some(principal) => {
                json!({"method": format!("{method}As"), "principal": principal})
            }
            None => json!({"method": method}),
        };
        let fields = request.as_object_mut().expect("a JSON object");
        for (key, value) in args {
            fields.insert((*key).to_string(), value.clone());
        }
        request
    }

    /// A client of the same connection whose guarded calls act for
    /// `principal`, the signed-in user this caller vouches for.
    pub fn acting_for(&self, principal: &str) -> Self {
        Self {
            stream: self.stream.clone(),
            principal: Some(principal.to_string()),
        }
    }

    /// The manager's version, None for a manager that has no `Version`
    /// method.
    pub fn version(&self) -> Result<Option<ManagerVersion>, ClientError> {
        match self.call_value(json!({"method": "Version"})) {
            Ok(v) => {
                let version = v.get("version").and_then(Value::as_str);
                let protocol = v.get("protocol").and_then(Value::as_u64);
                match (version, protocol) {
                    (Some(version), Some(protocol)) => Ok(Some(ManagerVersion {
                        version: version.to_string(),
                        protocol: u32::try_from(protocol)
                            .map_err(|_| ClientError::Other(v.to_string()))?,
                    })),
                    _ => Err(ClientError::Other(v.to_string())),
                }
            }
            // A manager without the method answers it like any unknown method.
            Err(Failure::Reply(name, _)) if name == BAD_REQUEST => Ok(None),
            Err(failure) => Err(failure.into_client_error()),
        }
    }
}

impl<S: Read + Write> ManagerClient for PipeClient<S> {
    fn list_instances(&self) -> Result<Vec<String>, ClientError> {
        let v = self.call(json!({"method": "ListInstances"}))?;
        serde_json::from_value(v.clone()).map_err(|_| ClientError::Other(v.to_string()))
    }
    fn status(&self, id: &str) -> Result<InstanceState, ClientError> {
        match self.call(json!({"method": "Status", "id": id}))? {
            Value::String(name) => Ok(InstanceState::from_name(&name)),
            v => Err(ClientError::Other(v.to_string())),
        }
    }
    fn start(&self, id: &str) -> Result<(), ClientError> {
        self.call_unit(self.guarded("Start", &[("id", json!(id))]))
    }
    fn stop(&self, id: &str) -> Result<(), ClientError> {
        self.call_unit(self.guarded("Stop", &[("id", json!(id))]))
    }
    fn restart(&self, id: &str) -> Result<(), ClientError> {
        self.call_unit(self.guarded("Restart", &[("id", json!(id))]))
    }
    fn read_file(&self, name: &str) -> Result<Option<ManagerFile>, ClientError> {
        let v = self.call(self.guarded("ReadFile", &[("name", json!(name))]))?;
        if v.is_null() {
            return Ok(None);
        }
        let text = v.get("text").and_then(Value::as_str);
        let withheld = v.get("withheld").and_then(Value::as_bool);
        match (text, withheld) {
            (Some(text), Some(withheld)) => Ok(Some(ManagerFile {
                text: text.to_string(),
                withheld,
            })),
            _ => Err(ClientError::Other(v.to_string())),
        }
    }
    fn write_file(
        &self,
        name: &str,
        contents: &str,
        keep_api_key: bool,
    ) -> Result<(), ClientError> {
        self.call_unit(self.guarded(
            "WriteFile",
            &[
                ("name", json!(name)),
                ("contents", json!(contents)),
                ("keep_api_key", json!(keep_api_key)),
            ],
        ))
    }
    fn write_file_if(&self, name: &str, contents: &str, expected: &str) -> Result<(), ClientError> {
        self.call_unit(self.guarded(
            "WriteFileIf",
            &[
                ("name", json!(name)),
                ("contents", json!(contents)),
                ("expected", json!(expected)),
            ],
        ))
    }
    fn boot_enabled(&self, id: &str) -> Result<bool, ClientError> {
        match self.call(json!({"method": "BootEnabled", "id": id}))? {
            Value::Bool(enabled) => Ok(enabled),
            v => Err(ClientError::Other(v.to_string())),
        }
    }
    fn set_boot_enabled(&self, id: &str, enabled: bool) -> Result<(), ClientError> {
        self.call_unit(self.guarded(
            "SetBootEnabled",
            &[("id", json!(id)), ("enabled", json!(enabled))],
        ))
    }
    fn set_model_profile(
        &self,
        instance: &str,
        profile: &str,
        value: &str,
    ) -> Result<(), ClientError> {
        self.call_unit(self.guarded(
            "SetModelProfile",
            &[
                ("instance", json!(instance)),
                ("profile", json!(profile)),
                ("value", json!(value)),
            ],
        ))
    }

    fn set_server_profile(&self, instance: &str, profile: &str) -> Result<(), ClientError> {
        self.call_unit(self.guarded(
            "SetServerProfile",
            &[("instance", json!(instance)), ("profile", json!(profile))],
        ))
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
        self.call_unit(self.guarded(
            "CreateServerProfile",
            &[
                ("engine", json!(engine)),
                ("profile", json!(profile)),
                ("port", json!(port)),
            ],
        ))
    }

    fn delete_server_profile(&self, engine: &str, profile: &str) -> Result<(), ClientError> {
        self.call_unit(self.guarded(
            "DeleteServerProfile",
            &[("engine", json!(engine)), ("profile", json!(profile))],
        ))
    }

    /// `CreateInstance`: the three names the manager's rule asks for, the
    /// instance's id sent as the frame's `instance`, the name every verb
    /// keyed on an instance uses.
    fn create_instance(
        &self,
        engine: &str,
        id: &str,
        server_profile: &str,
    ) -> Result<(), ClientError> {
        self.call_unit(self.guarded(
            "CreateInstance",
            &[
                ("engine", json!(engine)),
                ("instance", json!(id)),
                ("server_profile", json!(server_profile)),
            ],
        ))
    }

    fn remove_instance(&self, id: &str) -> Result<(), ClientError> {
        self.call_unit(self.guarded("RemoveInstance", &[("instance", json!(id))]))
    }

    fn instances(&self) -> Result<Vec<InstanceInfo>, ClientError> {
        let v = self.call(json!({"method": "Instances"}))?;
        let rows = v
            .as_array()
            .ok_or_else(|| ClientError::Other(v.to_string()))?;
        let text =
            |row: &Value, key: &str| row.get(key).and_then(Value::as_str).map(str::to_string);
        let tags = |row: &Value, key: &str| {
            row.get(key).and_then(Value::as_array).map(|list| {
                list.iter()
                    .filter_map(|t| t.as_str().map(str::to_string))
                    .collect()
            })
        };
        let mut out = Vec::with_capacity(rows.len());
        for row in rows {
            let port = row.get("port").and_then(Value::as_u64);
            // The field is there in every row of this protocol, so a row
            // without it is an older manager's list, which the client cannot
            // use. A row naming no profile is a section that names no server
            // file: no instance to act on, and its row is left out.
            let server_profile = row.get("server_profile").and_then(Value::as_str);
            match (
                text(row, "id"),
                text(row, "engine"),
                server_profile,
                port,
                tags(row, "profile_tags"),
                tags(row, "instance_tags"),
            ) {
                (
                    Some(id),
                    Some(engine),
                    Some(profile),
                    Some(port),
                    Some(profile_tags),
                    Some(instance_tags),
                ) => {
                    if profile.is_empty() {
                        continue;
                    }
                    out.push(InstanceInfo {
                        id,
                        engine,
                        server_profile: Some(profile.to_string()),
                        port: u16::try_from(port).map_err(|_| ClientError::Other(v.to_string()))?,
                        profile_tags,
                        instance_tags,
                    })
                }
                _ => return Err(ClientError::Other(v.to_string())),
            }
        }
        Ok(out)
    }
    fn server_profiles(&self) -> Result<Vec<ServerProfileInfo>, ClientError> {
        let v = self.call(json!({"method": "ServerProfiles"}))?;
        let rows = v
            .as_array()
            .ok_or_else(|| ClientError::Other(v.to_string()))?;
        let mut out = Vec::with_capacity(rows.len());
        for row in rows {
            let port = row.get("port").and_then(Value::as_u64);
            let tags = row.get("tags").and_then(Value::as_array).map(|list| {
                list.iter()
                    .filter_map(|t| t.as_str().map(str::to_string))
                    .collect::<Vec<String>>()
            });
            // The instance is the empty string when no section points at the
            // profile: the profile serves nothing, and the client reads it as
            // none.
            let instance = row.get("instance").and_then(Value::as_str);
            match (
                row.get("id").and_then(Value::as_str),
                row.get("engine").and_then(Value::as_str),
                port,
                tags,
                instance,
            ) {
                (Some(id), Some(engine), Some(port), Some(tags), Some(instance)) => {
                    out.push(ServerProfileInfo {
                        id: id.to_string(),
                        engine: engine.to_string(),
                        port: u16::try_from(port).map_err(|_| ClientError::Other(v.to_string()))?,
                        tags,
                        instance: (!instance.is_empty()).then(|| instance.to_string()),
                    })
                }
                _ => return Err(ClientError::Other(v.to_string())),
            }
        }
        Ok(out)
    }
    fn set_instance_tags(&self, instance: &str, tags: &[String]) -> Result<(), ClientError> {
        self.call_unit(self.guarded(
            "SetInstanceTags",
            &[("instance", json!(instance)), ("tags", json!(tags))],
        ))
    }
    fn devices(&self) -> Result<NodeDevices, ClientError> {
        let v = self.call(self.guarded("Devices", &[]))?;
        // The moment first: a reply without it is not a device list, whatever
        // its rows are. 0 is no probe, as over the bus.
        let probed_at = v
            .get("probed_at")
            .and_then(Value::as_u64)
            .ok_or_else(|| ClientError::Other(v.to_string()))?;
        // The host's inventory travels beside the list (R1): both numbers
        // always, 0 allowed (the host's "no figure"), a missing field, a
        // non-number, a negative one or a thread count past a uint32 refused
        // like a row of the wrong shape. The pair ships with this protocol, so
        // a reply without it is a shape, not an older manager to answer.
        let ram_mib = v
            .get("ram_mib")
            .and_then(Value::as_u64)
            .ok_or_else(|| ClientError::Other(v.to_string()))?;
        let cpu_threads = v
            .get("cpu_threads")
            .and_then(Value::as_u64)
            .and_then(|n| u32::try_from(n).ok())
            .ok_or_else(|| ClientError::Other(v.to_string()))?;
        let rows = v
            .get("devices")
            .and_then(Value::as_array)
            .ok_or_else(|| ClientError::Other(v.to_string()))?;
        let mut out = Vec::with_capacity(rows.len());
        for row in rows {
            let text = |key: &str| row.get(key).and_then(Value::as_str).map(str::to_string);
            let mib = |key: &str| row.get(key).and_then(Value::as_u64);
            match (
                text("id"),
                text("name"),
                text("backend"),
                mib("total_mib"),
                mib("free_mib"),
            ) {
                (Some(id), Some(name), Some(backend), Some(total_mib), Some(free_mib)) => {
                    out.push(DeviceInfo {
                        id,
                        name,
                        backend,
                        total_mib,
                        free_mib,
                    })
                }
                // A row of the wrong shape is a list the client cannot show:
                // a card guessed at from a missing field is a card that is not
                // there.
                _ => return Err(ClientError::Other(row.to_string())),
            }
        }
        Ok(NodeDevices {
            probed_at: (probed_at != 0).then_some(probed_at),
            ram_mib,
            cpu_threads,
            devices: out,
        })
    }
    fn authenticate(&self, user: &str, password: &str) -> Result<(bool, bool), ClientError> {
        let v = self.call(json!({"method": "Authenticate", "user": user, "password": password}))?;
        let ok = v.get("authenticated").and_then(Value::as_bool);
        let can_configure = v.get("can_configure").and_then(Value::as_bool);
        match (ok, can_configure) {
            (Some(ok), Some(can_configure)) => Ok((ok, can_configure)),
            _ => Err(ClientError::Other(v.to_string())),
        }
    }
}

/// Why a frame could not be read.
#[derive(Debug)]
pub enum FrameError {
    /// The stream failed or ended inside the frame.
    Io(std::io::Error),
    /// The frame's length is over [`MAX_FRAME`]; its body was not read.
    TooLong(u32),
}

/// Read one frame's body; None when the stream ends before the frame starts.
pub fn read_frame(r: &mut impl Read) -> Result<Option<Vec<u8>>, FrameError> {
    let mut len = [0u8; 4];
    let mut got = 0;
    while got < len.len() {
        match r.read(&mut len[got..]) {
            Ok(0) if got == 0 => return Ok(None),
            Ok(0) => return Err(FrameError::Io(ErrorKind::UnexpectedEof.into())),
            Ok(n) => got += n,
            Err(e) if e.kind() == ErrorKind::Interrupted => {}
            Err(e) => return Err(FrameError::Io(e)),
        }
    }
    let len = u32::from_le_bytes(len);
    if len > MAX_FRAME {
        return Err(FrameError::TooLong(len));
    }
    let mut body = vec![0u8; len as usize];
    r.read_exact(&mut body).map_err(FrameError::Io)?;
    Ok(Some(body))
}

/// Write `body` as one frame and flush it.
pub fn write_frame(w: &mut impl Write, body: &[u8]) -> std::io::Result<()> {
    let len = u32::try_from(body.len()).map_err(|_| ErrorKind::InvalidInput)?;
    let mut frame = Vec::with_capacity(4 + body.len());
    frame.extend_from_slice(&len.to_le_bytes());
    frame.extend_from_slice(body);
    w.write_all(&frame)?;
    w.flush()
}

/// The manager's named pipe, served by `pai-manager serve` and opened by the
/// client (Windows).
pub const PIPE_NAME: &str = r"\\.\pipe\private-ai-manager";

/// The rights the client opens the pipe with (`CreateFileW`'s
/// `dwDesiredAccess`): `FILE_GENERIC_READ | FILE_WRITE_DATA`, never
/// `GENERIC_WRITE`, whose `FILE_APPEND_DATA` is `FILE_CREATE_PIPE_INSTANCE`
/// and is not granted to interactive users.
pub const CLIENT_ACCESS: u32 = 0x0012_0089 | 0x2;

/// The client's `CreateFileW` flags: `SECURITY_SQOS_PRESENT |
/// SECURITY_IDENTIFICATION`, so the server can read the client's token but
/// never act as the client.
pub const CLIENT_FLAGS: u32 = 0x0010_0000 | 0x0001_0000;

/// Why opening the pipe failed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum OpenError {
    /// Every instance is in use: wait for one (`WaitNamedPipeW`) and retry.
    Busy,
    /// Give up with this error.
    Fail(ClientError),
}

/// The meaning of the Win32 error `code` of opening the pipe.
pub fn open_error(code: i32) -> OpenError {
    match code {
        // ERROR_FILE_NOT_FOUND: no manager serves the pipe.
        2 => OpenError::Fail(ClientError::Unavailable),
        // ERROR_ACCESS_DENIED: the pipe's security descriptor refused us.
        5 => OpenError::Fail(ClientError::NotAuthorized),
        // ERROR_PIPE_BUSY.
        231 => OpenError::Busy,
        _ => OpenError::Fail(ClientError::Other(format!("Win32 error {code}"))),
    }
}

/// How long [`connect`] waits for a free instance of a busy pipe.
#[cfg(windows)]
const BUSY_WAIT: std::time::Duration = std::time::Duration::from_secs(5);

/// The manager's client over its named pipe (Windows). A pipe busy for
/// longer than [`BUSY_WAIT`] is [`ClientError::Unavailable`].
#[cfg(windows)]
pub fn connect() -> Result<PipeClient<std::fs::File>, ClientError> {
    use std::os::windows::ffi::OsStrExt;
    use std::os::windows::fs::OpenOptionsExt;
    let deadline = std::time::Instant::now() + BUSY_WAIT;
    loop {
        let opened = std::fs::OpenOptions::new()
            .access_mode(CLIENT_ACCESS)
            .custom_flags(CLIENT_FLAGS)
            .open(PIPE_NAME);
        let code = match opened {
            Ok(file) => return Ok(PipeClient::new(file)),
            Err(e) => e.raw_os_error().unwrap_or_default(),
        };
        if let OpenError::Fail(e) = open_error(code) {
            return Err(e);
        }
        let left = deadline.saturating_duration_since(std::time::Instant::now());
        if left.is_zero() {
            return Err(ClientError::Unavailable);
        }
        let name: Vec<u16> = std::ffi::OsStr::new(PIPE_NAME)
            .encode_wide()
            .chain(std::iter::once(0))
            .collect();
        let ms = u32::try_from(left.as_millis()).unwrap_or(u32::MAX).max(1);
        // Returns at once when an instance is free, or at the timeout; the
        // open is retried either way, until the deadline.
        // SAFETY: `name` is a NUL-terminated UTF-16 string that outlives the call.
        unsafe { windows_sys::Win32::System::Pipes::WaitNamedPipeW(name.as_ptr(), ms) };
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::checks::Refusal;
    use serde_json::{json, Value};
    use std::io::{Cursor, ErrorKind};
    use std::sync::Arc;

    /// A stream that answers with `input` and records what the client sends;
    /// `read_error` / `write_error` make every read / write fail with it.
    struct Script {
        input: Cursor<Vec<u8>>,
        sent: Arc<Mutex<Vec<u8>>>,
        read_error: Option<ErrorKind>,
        write_error: Option<ErrorKind>,
    }

    impl Read for Script {
        fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
            match self.read_error {
                Some(kind) => Err(kind.into()),
                None => self.input.read(buf),
            }
        }
    }

    impl Write for Script {
        fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
            match self.write_error {
                Some(kind) => Err(kind.into()),
                None => {
                    self.sent.lock().unwrap().extend_from_slice(buf);
                    Ok(buf.len())
                }
            }
        }
        fn flush(&mut self) -> std::io::Result<()> {
            match self.write_error {
                Some(kind) => Err(kind.into()),
                None => Ok(()),
            }
        }
    }

    /// One frame: the body's length, u32 little-endian, then the body.
    fn frame(body: &[u8]) -> Vec<u8> {
        let mut out = (body.len() as u32).to_le_bytes().to_vec();
        out.extend_from_slice(body);
        out
    }

    /// The frames of `replies`, one after the other.
    fn input(replies: &[Value]) -> Vec<u8> {
        replies
            .iter()
            .flat_map(|r| frame(r.to_string().as_bytes()))
            .collect()
    }

    /// A client whose stream answers with `bytes`, and what it sends.
    fn client_on(bytes: Vec<u8>) -> (PipeClient<Script>, Arc<Mutex<Vec<u8>>>) {
        let sent = Arc::new(Mutex::new(Vec::new()));
        let script = Script {
            input: Cursor::new(bytes),
            sent: sent.clone(),
            read_error: None,
            write_error: None,
        };
        (PipeClient::new(script), sent)
    }

    /// The frames in `bytes`, each body parsed as JSON; a short or
    /// non-JSON frame fails the test.
    fn frames(bytes: &[u8]) -> Vec<Value> {
        let mut out = Vec::new();
        let mut rest = bytes;
        while !rest.is_empty() {
            assert!(rest.len() >= 4, "a short length prefix: {rest:?}");
            let len = u32::from_le_bytes(rest[..4].try_into().unwrap()) as usize;
            assert!(rest.len() >= 4 + len, "a short body: {rest:?}");
            let body = std::str::from_utf8(&rest[4..4 + len]).expect("a UTF-8 body");
            out.push(serde_json::from_str(body).expect("a JSON body"));
            rest = &rest[4 + len..];
        }
        out
    }

    fn sent_frames(sent: &Arc<Mutex<Vec<u8>>>) -> Vec<Value> {
        frames(&sent.lock().unwrap())
    }

    fn error_reply(kind: &str, message: &str) -> Value {
        json!({"error": {"name": format!("org.privateai.Manager1.Error.{kind}"), "message": message}})
    }

    /// One client method, its error when it fails.
    type Call = fn(&PipeClient<Script>) -> Option<ClientError>;

    /// The list one tags call sends.
    fn tags() -> Vec<String> {
        vec!["edge".to_string(), "v2".to_string()]
    }

    const EVERY_METHOD: [(&str, Call); 16] = [
        ("list_instances", |c| c.list_instances().err()),
        ("instances", |c| c.instances().err()),
        ("devices", |c| c.devices().err()),
        ("status", |c| c.status("default").err()),
        ("start", |c| c.start("default").err()),
        ("stop", |c| c.stop("default").err()),
        ("restart", |c| c.restart("default").err()),
        ("read_file", |c| c.read_file("models.ini").err()),
        ("write_file", |c| {
            c.write_file("models.ini", "", false).err()
        }),
        ("boot_enabled", |c| c.boot_enabled("default").err()),
        ("set_boot_enabled", |c| {
            c.set_boot_enabled("default", true).err()
        }),
        ("set_model_profile", |c| {
            c.set_model_profile("default", "Qwen 3 (v2) é", "on").err()
        }),
        ("set_server_profile", |c| {
            c.set_server_profile("lan-2", "Qwen 3 (v2) é").err()
        }),
        ("set_instance_tags", |c| {
            c.set_instance_tags("lan-2", &tags()).err()
        }),
        ("authenticate", |c| {
            c.authenticate("alice", "s3cret-Pw").err()
        }),
        ("version", |c| c.version().err()),
    ];

    // ── Failures first ──────────────────────────────────────────────────

    #[test]
    fn the_frame_cap_is_one_mebibyte() {
        assert_eq!(MAX_FRAME, 1024 * 1024);
    }

    /// The configurator holds its client as `Box<dyn ManagerClient + Send + Sync>`.
    #[test]
    fn the_client_can_be_shared_across_threads() {
        fn shared<T: Send + Sync>() {}
        shared::<PipeClient<Script>>();
        #[cfg(unix)]
        shared::<PipeClient<std::os::unix::net::UnixStream>>();
    }

    /// A manager that closed the stream before replying: no manager to ask.
    #[test]
    fn no_reply_at_all_is_unavailable() {
        for (method, f) in EVERY_METHOD {
            let (client, _) = client_on(Vec::new());
            assert_eq!(f(&client), Some(ClientError::Unavailable), "{method}");
        }
    }

    #[test]
    fn a_stream_that_ends_inside_a_frame_is_unavailable() {
        let whole = frame(json!({"ok": null}).to_string().as_bytes());
        for cut in [1, 3, 4, whole.len() - 1] {
            let (client, _) = client_on(whole[..cut].to_vec());
            assert_eq!(
                client.start("default"),
                Err(ClientError::Unavailable),
                "cut after {cut} bytes"
            );
        }
    }

    #[test]
    fn a_stream_that_fails_is_unavailable() {
        for kind in [
            ErrorKind::BrokenPipe,
            ErrorKind::ConnectionReset,
            ErrorKind::TimedOut,
            ErrorKind::WouldBlock,
        ] {
            for (method, f) in EVERY_METHOD {
                // The request cannot be sent.
                let script = Script {
                    input: Cursor::new(input(&[json!({"ok": null})])),
                    sent: Arc::new(Mutex::new(Vec::new())),
                    read_error: None,
                    write_error: Some(kind),
                };
                let client = PipeClient::new(script);
                assert_eq!(
                    f(&client),
                    Some(ClientError::Unavailable),
                    "{method}, write {kind:?}"
                );
                // The reply cannot be read (a read timeout included).
                let script = Script {
                    input: Cursor::new(Vec::new()),
                    sent: Arc::new(Mutex::new(Vec::new())),
                    read_error: Some(kind),
                    write_error: None,
                };
                let client = PipeClient::new(script);
                assert_eq!(
                    f(&client),
                    Some(ClientError::Unavailable),
                    "{method}, read {kind:?}"
                );
            }
        }
    }

    /// The other end gone: a socket pair whose server end is closed.
    #[cfg(unix)]
    #[test]
    fn a_closed_peer_is_unavailable() {
        use std::os::unix::net::UnixStream;
        for (method, call) in [
            (
                "list_instances",
                (|c| c.list_instances().err())
                    as fn(&PipeClient<UnixStream>) -> Option<ClientError>,
            ),
            ("status", |c| c.status("default").err()),
            ("start", |c| c.start("default").err()),
            ("write_file", |c| {
                c.write_file("models.ini", "x", false).err()
            }),
        ] {
            let (server, client) = UnixStream::pair().expect("socket pair");
            client
                .set_read_timeout(Some(std::time::Duration::from_secs(10)))
                .expect("read timeout");
            drop(server);
            let client = PipeClient::new(client);
            assert_eq!(call(&client), Some(ClientError::Unavailable), "{method}");
        }
    }

    /// A request whose frame would be over the cap is refused before
    /// anything is sent: the server would answer BadRequest and hang up,
    /// which the client could read as the manager gone. The stream stays in
    /// step for the next call.
    #[test]
    fn a_request_over_the_cap_is_other_and_sends_nothing() {
        let (client, sent) = client_on(input(&[json!({"ok": null})]));
        let huge = "x".repeat(MAX_FRAME as usize);
        match client.write_file("models.ini", &huge, false) {
            Err(ClientError::Other(detail)) => assert!(!detail.is_empty()),
            other => panic!("a request over the cap: {other:?}"),
        }
        assert!(sent.lock().unwrap().is_empty(), "nothing sent");
        assert_eq!(
            client.start("default"),
            Ok(()),
            "the next call has its reply"
        );
        assert_eq!(
            sent_frames(&sent),
            vec![json!({"method": "Start", "id": "default"})]
        );
    }

    /// A reply whose frame is over the cap is refused, not read.
    #[test]
    fn a_reply_over_the_cap_is_other() {
        let mut bytes = (MAX_FRAME + 1).to_le_bytes().to_vec();
        bytes.extend_from_slice(b"{\"ok\":null}");
        let (client, _) = client_on(bytes);
        match client.start("default") {
            Err(ClientError::Other(detail)) => assert!(!detail.is_empty()),
            other => panic!("a reply over the cap: {other:?}"),
        }
        let (client, _) = client_on(u32::MAX.to_le_bytes().to_vec());
        assert!(
            matches!(client.list_instances(), Err(ClientError::Other(_))),
            "a 4 GiB length"
        );
    }

    #[test]
    fn a_reply_that_is_not_json_is_other() {
        for body in [
            &b""[..],
            b"not json",
            b"{\"ok\":",
            b"\xff\xfe{\"ok\":null}",
            b"[\"ok\", null]",
        ] {
            let (client, _) = client_on(frame(body));
            assert!(
                matches!(client.start("default"), Err(ClientError::Other(_))),
                "{body:?}"
            );
        }
    }

    /// The list `Instances` answers, read as it is: the two tag lists kept
    /// apart and in the order the manager gave them, every row kept, and a
    /// manager with no instance answered an empty list.
    #[test]
    fn the_instances_reply_is_the_list_the_manager_answers() {
        let (client, _) = client_on(input(&[json!({"ok": [
            {"id": "default", "engine": "llama.cpp", "server_profile": "big", "port": 8080, "profile_tags": ["edge", "v2"], "instance_tags": []},
            {"id": "lan-2", "engine": "llama.cpp", "server_profile": "edge-2", "port": 8081, "profile_tags": [], "instance_tags": ["gpu"]},
            {"id": "solo", "engine": "llama.cpp", "server_profile": "", "port": 0, "profile_tags": [], "instance_tags": []},
        ]})]));
        assert_eq!(
            client.instances().expect("the list"),
            vec![
                InstanceInfo {
                    id: "default".into(),
                    engine: "llama.cpp".into(),
                    server_profile: Some("big".into()),
                    port: 8080,
                    profile_tags: vec!["edge".into(), "v2".into()],
                    instance_tags: vec![],
                },
                InstanceInfo {
                    id: "lan-2".into(),
                    engine: "llama.cpp".into(),
                    server_profile: Some("edge-2".into()),
                    port: 8081,
                    profile_tags: vec![],
                    instance_tags: vec!["gpu".into()],
                },
            ],
            "the row's fields, in the manager's order"
        );
        let (client, _) = client_on(input(&[json!({"ok": []})]));
        assert_eq!(
            client.instances().expect("no instances"),
            Vec::<InstanceInfo>::new(),
            "no instance is an empty list"
        );
    }

    /// The row of slice 13 carries `server_profile`, the `ServerProfile` of
    /// the instance's section: every client that reads a server file needs the
    /// name, because `server@<id>.ini` is not the file the instance uses. A
    /// row of an older manager, which answers no such field, is not a row with
    /// an empty profile: it is a reply the client cannot use, said as `Other`
    /// like every row of the wrong shape, so no client reads or writes a
    /// `server@.ini`.
    #[test]
    fn an_instances_row_without_the_server_profile_is_the_old_shape() {
        let (client, _) = client_on(input(&[json!({"ok": [
            {"id": "default", "engine": "llama.cpp", "port": 8080, "profile_tags": [], "instance_tags": []},
        ]})]));
        let got = client.instances();
        assert!(
            matches!(got, Err(ClientError::Other(_))),
            "an older manager's list, a row naming no profile, is refused: {got:?}"
        );
    }

    // ── The node's devices (G2, ARCHITECTURE.md 1) ──────────────────────

    /// The device list `Devices` answers, read as it is: every row kept, in the
    /// manager's order, with the id, the product name, the backend, both sizes,
    /// the moment of the probe, and the host's inventory R1 adds beside the
    /// list: `ram_mib`, the host's total RAM in MiB, and `cpu_threads`, its
    /// logical CPUs (ARCHITECTURE.md (1) "The node inventory").
    #[test]
    fn the_devices_reply_is_the_list_the_manager_answers() {
        let (client, sent) = client_on(input(&[json!({"ok": {
            "probed_at": 1_766_271_243u64,
            "ram_mib": 3_932_160u64,
            "cpu_threads": 8u64,
            "devices": [
                {"id": "ROCm0", "name": "AMD Radeon AI PRO R9700", "backend": "ROCm", "total_mib": 32624u64, "free_mib": 32420u64},
                {"id": "CUDA0", "name": "NVIDIA GeForce RTX 4070 SUPER", "backend": "CUDA", "total_mib": 12281u64, "free_mib": 10844u64},
                {"id": "Vulkan0", "name": "AMD Radeon AI PRO R9700", "backend": "Vulkan", "total_mib": 32624u64, "free_mib": 32420u64},
            ],
        }})]));
        assert_eq!(
            client.devices().expect("the device list"),
            NodeDevices {
                probed_at: Some(1_766_271_243),
                ram_mib: 3_932_160,
                cpu_threads: 8,
                devices: vec![
                    DeviceInfo {
                        id: "ROCm0".into(),
                        name: "AMD Radeon AI PRO R9700".into(),
                        backend: "ROCm".into(),
                        total_mib: 32624,
                        free_mib: 32420,
                    },
                    DeviceInfo {
                        id: "CUDA0".into(),
                        name: "NVIDIA GeForce RTX 4070 SUPER".into(),
                        backend: "CUDA".into(),
                        total_mib: 12281,
                        free_mib: 10844,
                    },
                    DeviceInfo {
                        id: "Vulkan0".into(),
                        name: "AMD Radeon AI PRO R9700".into(),
                        backend: "Vulkan".into(),
                        total_mib: 32624,
                        free_mib: 32420,
                    },
                ],
            },
            "the row's fields, in the manager's order"
        );
        assert_eq!(
            sent_frames(&sent),
            vec![json!({"method": "Devices"})],
            "the plain method, with no argument"
        );
    }

    /// A manager that never got a probe answered: `probed_at` 0 is no time
    /// (None), and no device is an empty list. The page shows the empty list
    /// with no time, never an error. The host's inventory is answered all the
    /// same: it is the host's, not the engine's, so a node with no probe
    /// reports what it is.
    #[test]
    fn a_devices_reply_of_no_probe_is_no_time_and_an_empty_list() {
        for (probed_at, devices) in [
            (0u64, json!([])),
            (
                0u64,
                json!([{"id": "CPU", "name": "CPU", "backend": "CPU", "total_mib": 0u64, "free_mib": 0u64}]),
            ),
        ] {
            let (client, _) = client_on(input(&[json!({"ok": {
                "probed_at": probed_at,
                "ram_mib": 3_932_160u64,
                "cpu_threads": 8u64,
                "devices": devices,
            }})]));
            let got = client.devices().expect("the empty device list");
            assert_eq!(got.probed_at, None, "0 is no probe ({probed_at})");
            assert_eq!(got.devices.len(), devices.as_array().unwrap().len());
            assert_eq!(
                (got.ram_mib, got.cpu_threads),
                (3_932_160, 8),
                "no probe leaves the inventory answered ({probed_at})"
            );
        }
    }

    /// A device row of the wrong shape is Other, like every other row: a client
    /// that guessed at a missing field would show a card that is not there. The
    /// inventory is read the same way: a reply with no `ram_mib`, no
    /// `cpu_threads`, one of them not a number, a negative one, or a thread
    /// count past a uint32, is refused (R1). The pair ships with protocol 3,
    /// which is in no release, so a reply without it is an incomprehensible
    /// shape, not an older manager to answer.
    #[test]
    fn a_devices_row_of_the_wrong_shape_is_other() {
        for row in [
            json!({"id": "ROCm0", "name": "AMD", "backend": "ROCm", "total_mib": 1u64}),
            json!({"id": "ROCm0", "name": "AMD", "total_mib": 1u64, "free_mib": 1u64}),
            json!({"name": "AMD", "backend": "ROCm", "total_mib": 1u64, "free_mib": 1u64}),
            json!({"id": "ROCm0", "name": "AMD", "backend": 1u64, "total_mib": 1u64, "free_mib": 1u64}),
            json!({"id": "ROCm0", "name": "AMD", "backend": "ROCm", "total_mib": "1", "free_mib": 1u64}),
            json!({"id": "ROCm0", "name": "AMD", "backend": "ROCm", "total_mib": -1i64, "free_mib": 1u64}),
            json!("not a row"),
        ] {
            let (client, _) = client_on(input(&[json!({"ok": {
                "probed_at": 1u64,
                "ram_mib": 1u64,
                "cpu_threads": 1u64,
                "devices": [row],
            }})]));
            let got = client.devices();
            assert!(
                matches!(got, Err(ClientError::Other(_))),
                "{row}: a row of the wrong shape is refused: {got:?}"
            );
        }
        // The reply itself of the wrong shape: not an object, no devices list,
        // a devices list that is not a list, no probed_at, and no inventory.
        for reply in [
            json!({"ok": []}),
            json!({"ok": null}),
            json!({"ok": {"devices": []}}),
            json!({"ok": {"probed_at": 1u64}}),
            json!({"ok": {"probed_at": 1u64, "devices": {}}}),
            json!({"ok": {"probed_at": "1", "devices": []}}),
            json!({"ok": {"probed_at": 1u64, "devices": [], "cpu_threads": 1u64}}),
            json!({"ok": {"probed_at": 1u64, "devices": [], "ram_mib": 1u64}}),
            json!({"ok": {"probed_at": 1u64, "devices": [], "ram_mib": "64 GiB", "cpu_threads": 1u64}}),
            json!({"ok": {"probed_at": 1u64, "devices": [], "ram_mib": -1i64, "cpu_threads": 1u64}}),
            json!({"ok": {"probed_at": 1u64, "devices": [], "ram_mib": 1u64, "cpu_threads": -1i64}}),
            json!({"ok": {"probed_at": 1u64, "devices": [], "ram_mib": 1u64, "cpu_threads": 4294967296u64}}),
        ] {
            let (client, _) = client_on(input(std::slice::from_ref(&reply)));
            let got = client.devices();
            assert!(
                matches!(got, Err(ClientError::Other(_))),
                "{reply}: a reply of the wrong shape is refused: {got:?}"
            );
        }
    }

    /// A client acting for the signed-in user asks `DevicesAs`, with the
    /// principal: the manager reads the devices for a member of `pai`, and the
    /// bff always asks this way.
    #[test]
    fn an_acting_client_asks_the_devices_for_the_signed_in_user() {
        let (client, sent) = client_on(input(&[json!({"ok": {
            "probed_at": 0u64,
            "ram_mib": 0u64,
            "cpu_threads": 0u64,
            "devices": [],
        }})]));
        let bound = client.acting_for("alice");
        assert_eq!(
            bound.devices().expect("the empty device list").devices,
            Vec::<DeviceInfo>::new()
        );
        assert_eq!(
            sent_frames(&sent),
            vec![json!({"method": "DevicesAs", "principal": "alice"})],
            "the As form, with the principal as given"
        );
    }

    /// The refusals of the devices read cross the wire like every other
    /// method's: the gate's `NotAuthorized` for a caller without `control`, and
    /// the same word for a `DevicesAs` principal who is no member of `pai`
    /// (the bff answers that 403 and ends the session). The client's own
    /// `Unavailable` is the manager gone, pinned for this method by
    /// `no_reply_at_all_is_unavailable`: the wire has no kind for it, so
    /// nothing here asks for one.
    #[test]
    fn a_devices_refusal_is_the_error_the_manager_sent() {
        let (client, _) = client_on(input(&[error_reply("NotAuthorized", "NotAuthorized")]));
        assert_eq!(
            client.devices(),
            Err(ClientError::NotAuthorized),
            "the plain form, refused by the quiet `control` check"
        );
        let (client, sent) = client_on(input(&[error_reply("NotAuthorized", "NotAuthorized")]));
        let bound = client.acting_for("alice");
        assert_eq!(
            bound.devices(),
            Err(ClientError::NotAuthorized),
            "the As form, refused for a principal outside `pai`"
        );
        assert_eq!(
            sent_frames(&sent),
            vec![json!({"method": "DevicesAs", "principal": "alice"})],
            "the refusal answers the As call"
        );
    }

    /// A reply that is neither a value nor an error, or the wrong value for
    /// the method, is Other, never a panic.
    #[test]
    fn a_reply_of_the_wrong_shape_is_other() {
        let cases: [(&str, Call, Value); 16] = [
            (
                "boot_enabled",
                |c| c.boot_enabled("default").err(),
                json!({"ok": "enabled"}),
            ),
            (
                "boot_enabled",
                |c| c.boot_enabled("default").err(),
                json!({"ok": null}),
            ),
            (
                "boot_enabled",
                |c| c.boot_enabled("default").err(),
                json!({"ok": 1}),
            ),
            (
                "set_boot_enabled",
                |c| c.set_boot_enabled("default", true).err(),
                json!({"ok": true}),
            ),
            (
                "list_instances",
                |c| c.list_instances().err(),
                json!({"ok": "default"}),
            ),
            (
                "list_instances",
                |c| c.list_instances().err(),
                json!({"ok": [1, 2]}),
            ),
            (
                "list_instances",
                |c| c.list_instances().err(),
                json!({"ok": null}),
            ),
            ("status", |c| c.status("default").err(), json!({"ok": 5})),
            ("status", |c| c.status("default").err(), json!({"ok": null})),
            (
                "read_file",
                |c| c.read_file("models.ini").err(),
                json!({"ok": true}),
            ),
            ("start", |c| c.start("default").err(), json!({})),
            (
                "start",
                |c| c.start("default").err(),
                json!({"error": "NotAuthorized"}),
            ),
            (
                "start",
                |c| c.start("default").err(),
                json!({"error": {"name": "org.privateai.Manager1.Error.NotAuthorized"}}),
            ),
            (
                "instances",
                |c| c.instances().err(),
                json!({"ok": "default"}),
            ),
            (
                "instances",
                |c| c.instances().err(),
                json!({"ok": [{"id": "default", "engine": "llama.cpp"}]}),
            ),
            (
                "instances",
                |c| c.instances().err(),
                json!({"ok": [{"id": "default", "engine": "llama.cpp", "port": 8080, "profile_tags": "edge", "instance_tags": []}]}),
            ),
        ];
        for (method, f, reply) in cases {
            let (client, _) = client_on(input(std::slice::from_ref(&reply)));
            assert!(
                matches!(f(&client), Some(ClientError::Other(_))),
                "{method} given {reply}"
            );
        }
    }

    /// Every error reply maps exactly as over D-Bus, through `from_reply`.
    #[test]
    fn error_replies_map_like_dbus() {
        let refusal = Refusal {
            key: "not-whole-number-in-range",
            values: vec!["Port".into(), "70000".into(), "1".into(), "65535".into()],
        };
        let cases = [
            (
                error_reply("NotAuthorized", "NotAuthorized"),
                ClientError::NotAuthorized,
            ),
            (
                error_reply("InvalidId", "InvalidId"),
                ClientError::InvalidId,
            ),
            (
                error_reply("NoInstance", "NoInstance"),
                ClientError::NoInstance,
            ),
            (
                error_reply("BadFileName", "BadFileName"),
                ClientError::BadFileName,
            ),
            (
                error_reply("AddressInUse", "default"),
                ClientError::AddressInUse("default".into()),
            ),
            (
                error_reply("UnresolvableHost", "nowhere.invalid"),
                ClientError::UnresolvableHost("nowhere.invalid".into()),
            ),
            (
                error_reply("Service", "llama.cpp@broken.service failed"),
                ClientError::Service("llama.cpp@broken.service failed".into()),
            ),
            (
                error_reply("Io", "permission denied"),
                ClientError::Io("permission denied".into()),
            ),
            (
                error_reply("Invalid", &refusal.encode()),
                ClientError::Invalid(refusal.clone()),
            ),
            (
                error_reply("NoServerProfile", "nope café"),
                ClientError::NoServerProfile("nope café".into()),
            ),
            (
                // The profile and the instance, split at the separator the
                // refusal's encode uses.
                error_reply("ServerProfileInUse", "lan 2 (café)\u{1f}lan-2"),
                ClientError::ServerProfileInUse("lan 2 (café)".into(), "lan-2".into()),
            ),
            (
                // A name the pair is not there for: the message is the
                // profile's own name, and nothing is split.
                error_reply("NoServerProfile", "lan-2\u{1f}lan-2"),
                ClientError::NoServerProfile("lan-2\u{1f}lan-2".into()),
            ),
        ];
        for (reply, expected) in cases {
            for (method, f) in EVERY_METHOD {
                let (client, _) = client_on(input(std::slice::from_ref(&reply)));
                assert_eq!(f(&client), Some(expected.clone()), "{method} given {reply}");
            }
        }
        // A name this client does not know, the manager's malformed-request
        // reply among them, keeps its name and message.
        for (name, message) in [
            ("org.privateai.Manager1.Error.BadRequest", "not a request"),
            ("org.privateai.Manager1.Error.NewKind", "detail"),
        ] {
            let reply = json!({"error": {"name": name, "message": message}});
            let (client, _) = client_on(input(&[reply]));
            match client.start("default") {
                Err(ClientError::Other(detail)) => {
                    assert!(
                        detail.contains(name) && detail.contains(message),
                        "{detail:?}"
                    )
                }
                other => panic!("{name}: {other:?}"),
            }
        }
    }

    // ── The wire ────────────────────────────────────────────────────────

    /// Each method sends one frame, its request named and shaped as pinned
    /// in the module header, and reads one reply.
    #[test]
    fn every_method_sends_its_request() {
        let contents = "[Qwen 3 (v2) é]\nmodel = /models/q.gguf\n";
        let cases: [(Call, Value); 16] = [
            (
                |c| c.boot_enabled("lan-2").err(),
                json!({"method": "BootEnabled", "id": "lan-2"}),
            ),
            (
                |c| c.set_boot_enabled("lan-2", true).err(),
                json!({"method": "SetBootEnabled", "id": "lan-2", "enabled": true}),
            ),
            (
                |c| c.set_boot_enabled("lan-2", false).err(),
                json!({"method": "SetBootEnabled", "id": "lan-2", "enabled": false}),
            ),
            (
                |c| c.list_instances().err(),
                json!({"method": "ListInstances"}),
            ),
            (
                |c| c.status("lan-2").err(),
                json!({"method": "Status", "id": "lan-2"}),
            ),
            (
                |c| c.start("lan-2").err(),
                json!({"method": "Start", "id": "lan-2"}),
            ),
            (
                |c| c.stop("lan-2").err(),
                json!({"method": "Stop", "id": "lan-2"}),
            ),
            (
                |c| c.restart("lan-2").err(),
                json!({"method": "Restart", "id": "lan-2"}),
            ),
            (
                |c| c.read_file("server@lan-2.ini").err(),
                json!({"method": "ReadFile", "name": "server@lan-2.ini"}),
            ),
            (
                |c| {
                    c.write_file(
                        "models.ini",
                        "[Qwen 3 (v2) é]\nmodel = /models/q.gguf\n",
                        false,
                    )
                    .err()
                },
                json!({"method": "WriteFile", "name": "models.ini", "contents": contents, "keep_api_key": false}),
            ),
            (
                |c| {
                    c.write_file("server@lan-2.ini", "[Server]\nOpencodeApiKey =\n", true)
                        .err()
                },
                json!({"method": "WriteFile", "name": "server@lan-2.ini", "contents": "[Server]\nOpencodeApiKey =\n", "keep_api_key": true}),
            ),
            (
                |c| {
                    c.write_file_if(
                        "models.ini",
                        "[Qwen 3 (v2) é]\nmodel = /models/q.gguf\n",
                        "",
                    )
                    .err()
                },
                // The compare and swap: the file's name, the new content and
                // the content it must still hold, and no `keep_api_key`.
                json!({"method": "WriteFileIf", "name": "models.ini", "contents": contents, "expected": ""}),
            ),
            (
                |c| c.set_model_profile("lan-2", "Qwen 3 (v2) é", "off").err(),
                json!({"method": "SetModelProfile", "instance": "lan-2", "profile": "Qwen 3 (v2) é", "value": "off"}),
            ),
            (
                |c| c.set_server_profile("lan-2", "Qwen 3 (v2) é").err(),
                json!({"method": "SetServerProfile", "instance": "lan-2", "profile": "Qwen 3 (v2) é"}),
            ),
            (|c| c.instances().err(), json!({"method": "Instances"})),
            (
                |c| c.set_instance_tags("lan-2", &tags()).err(),
                json!({"method": "SetInstanceTags", "instance": "lan-2", "tags": ["edge", "v2"]}),
            ),
        ];
        for (f, request) in cases {
            // Any reply will do: what is pinned here is the request.
            let (client, sent) = client_on(input(&[error_reply("InvalidId", "InvalidId")]));
            assert_eq!(f(&client), Some(ClientError::InvalidId), "{request}");
            assert_eq!(sent_frames(&sent), vec![request.clone()], "{request}");
        }
    }

    #[test]
    fn every_value_reply_is_read() {
        let (client, _) = client_on(input(&[json!({"ok": ["broken", "default", "lan-2"]})]));
        assert_eq!(
            client.list_instances(),
            Ok(vec!["broken".into(), "default".into(), "lan-2".into()])
        );
        let (client, _) = client_on(input(&[json!({"ok": []})]));
        assert_eq!(client.list_instances(), Ok(vec![]));

        for (name, state) in [
            ("active", InstanceState::Active),
            ("inactive", InstanceState::Inactive),
            ("failed", InstanceState::Failed),
            ("activating", InstanceState::Activating),
            ("deactivating", InstanceState::Deactivating),
            ("maintenance", InstanceState::Other("maintenance".into())),
        ] {
            let (client, _) = client_on(input(&[json!({"ok": name})]));
            assert_eq!(client.status("default"), Ok(state), "{name}");
        }

        for (method, f) in [
            (
                "start",
                (|c| c.start("default")) as fn(&PipeClient<Script>) -> Result<(), ClientError>,
            ),
            ("stop", |c| c.stop("default")),
            ("restart", |c| c.restart("default")),
            ("write_file", |c| {
                c.write_file("models.ini", "[m]\nmodel = /m.gguf\n", false)
            }),
        ] {
            let (client, _) = client_on(input(&[json!({"ok": null})]));
            assert_eq!(f(&client), Ok(()), "{method}");
        }

        for enabled in [true, false] {
            let (client, _) = client_on(input(&[json!({"ok": enabled})]));
            assert_eq!(client.boot_enabled("default"), Ok(enabled));
            let (client, _) = client_on(input(&[json!({"ok": null})]));
            assert_eq!(client.set_boot_enabled("default", enabled), Ok(()));
        }

        let text = "[Qwen3.8-27B]\nmodel = /models/Qwen3.8-27B-Q8_0.gguf\n";
        let file = |text: &str, withheld: bool| ManagerFile {
            text: text.into(),
            withheld,
        };
        let (client, _) = client_on(input(&[json!({"ok": {"text": text, "withheld": false}})]));
        assert_eq!(client.read_file("models.ini"), Ok(Some(file(text, false))));
        let (client, _) = client_on(input(&[json!({"ok": null})]));
        assert_eq!(
            client.read_file("models.ini.disabled"),
            Ok(None),
            "no such file"
        );
        let (client, _) = client_on(input(&[json!({"ok": {"text": "", "withheld": false}})]));
        assert_eq!(
            client.read_file("models.ini.disabled"),
            Ok(Some(file("", false))),
            "an empty file is not a missing one"
        );
        let blanked = "[Server]\nPort = 8080\nOpencodeApiKey =\n";
        let (client, _) = client_on(input(&[json!({"ok": {"text": blanked, "withheld": true}})]));
        assert_eq!(
            client.read_file("server@default.ini"),
            Ok(Some(file(blanked, true))),
            "the manager withheld the API key"
        );
    }

    /// A ReadFile reply in another shape is no file: the bare text of an
    /// older manager (which would not say whether it withheld the API key),
    /// an object missing a member or with a member of the wrong type.
    #[test]
    fn a_read_file_reply_of_another_shape_is_other() {
        for reply in [
            json!({"ok": "[Server]\nPort = 8080\n"}),
            json!({"ok": {"text": "[Server]\n"}}),
            json!({"ok": {"withheld": false}}),
            json!({"ok": {"text": "[Server]\n", "withheld": "no"}}),
            json!({"ok": {"text": 7, "withheld": false}}),
            json!({"ok": true}),
        ] {
            let (client, _) = client_on(input(std::slice::from_ref(&reply)));
            assert!(
                matches!(
                    client.read_file("server@default.ini"),
                    Err(ClientError::Other(_))
                ),
                "{reply}"
            );
        }
    }

    /// Calls in a row each read their own reply, in order, on one stream.
    #[test]
    fn calls_in_a_row_read_their_own_replies() {
        let (client, sent) = client_on(input(&[
            json!({"ok": ["default"]}),
            error_reply("NotAuthorized", "NotAuthorized"),
            json!({"ok": "inactive"}),
        ]));
        assert_eq!(client.list_instances(), Ok(vec!["default".to_string()]));
        assert_eq!(client.start("default"), Err(ClientError::NotAuthorized));
        assert_eq!(client.status("default"), Ok(InstanceState::Inactive));
        assert_eq!(
            sent_frames(&sent),
            vec![
                json!({"method": "ListInstances"}),
                json!({"method": "Start", "id": "default"}),
                json!({"method": "Status", "id": "default"}),
            ]
        );
    }

    // ── The named pipe (Windows) ─────────────────────────────────────────

    /// Pipe Names: `\\.\pipe\<name>`, the name without a backslash, at
    /// most 256 characters in all.
    #[test]
    fn the_pipe_is_the_manager_s_local_pipe() {
        assert_eq!(PIPE_NAME, r"\\.\pipe\private-ai-manager");
        let name = PIPE_NAME.strip_prefix(r"\\.\pipe\").expect("a local pipe");
        assert!(!name.is_empty() && !name.contains('\\'), "{PIPE_NAME}");
        assert!(PIPE_NAME.len() <= 256);
    }

    #[test]
    fn the_client_asks_to_read_and_write_data_only() {
        const FILE_GENERIC_READ: u32 = 0x0012_0089;
        const FILE_WRITE_DATA: u32 = 0x2;
        const FILE_CREATE_PIPE_INSTANCE: u32 = 0x4;
        assert_eq!(CLIENT_ACCESS, FILE_GENERIC_READ | FILE_WRITE_DATA);
        assert_eq!(CLIENT_ACCESS & FILE_CREATE_PIPE_INSTANCE, 0);
    }

    #[test]
    fn the_client_lets_the_server_identify_it_only() {
        const SECURITY_SQOS_PRESENT: u32 = 0x0010_0000;
        const SECURITY_IDENTIFICATION: u32 = 0x0001_0000;
        assert_eq!(
            CLIENT_FLAGS,
            SECURITY_SQOS_PRESENT | SECURITY_IDENTIFICATION
        );
    }

    #[test]
    fn no_pipe_means_no_manager() {
        // ERROR_FILE_NOT_FOUND: the manager is not running.
        assert_eq!(open_error(2), OpenError::Fail(ClientError::Unavailable));
    }

    /// The security descriptor keeps out everyone but SYSTEM,
    /// Administrators and interactive users: a service account or a network
    /// logon is refused at the open.
    #[test]
    fn a_refused_open_is_not_authorized() {
        assert_eq!(open_error(5), OpenError::Fail(ClientError::NotAuthorized));
    }

    #[test]
    fn a_busy_pipe_is_waited_for() {
        assert_eq!(open_error(231), OpenError::Busy);
    }

    #[test]
    fn any_other_open_failure_names_its_code() {
        for code in [3, 87, 1450] {
            assert_eq!(
                open_error(code),
                OpenError::Fail(ClientError::Other(format!("Win32 error {code}")))
            );
        }
    }

    // ── Authenticate ────────────────────────────────────────────────────

    /// `authenticate` sends `{"method":"Authenticate","user","password"}`,
    /// both exactly as given (nothing trimmed), one frame.
    #[test]
    fn authenticate_sends_its_request() {
        for (user, password) in [
            ("alice", "s3cret-Pw"),
            (" Alice ", " pässwörd \u{1f511}\"\\\n"),
            ("", ""),
        ] {
            let (client, sent) = client_on(input(&[error_reply("NotAuthorized", "NotAuthorized")]));
            assert_eq!(
                client.authenticate(user, password),
                Err(ClientError::NotAuthorized)
            );
            assert_eq!(
                sent_frames(&sent),
                vec![json!({"method": "Authenticate", "user": user, "password": password})],
                "{user:?}"
            );
        }
    }

    /// The reply `{"ok":{"authenticated":bool,"can_configure":bool}}` is
    /// (ok, can_configure).
    #[test]
    fn authenticate_reads_its_reply() {
        for (ok, admin) in [(true, true), (true, false), (false, false), (false, true)] {
            let (client, _) = client_on(input(&[
                json!({"ok": {"authenticated": ok, "can_configure": admin}}),
            ]));
            assert_eq!(client.authenticate("alice", "s3cret-Pw"), Ok((ok, admin)));
        }
    }

    /// A reply of another shape is Other, never a sign-in: a bare boolean,
    /// a member missing or of the wrong type, null, the D-Bus pair as an
    /// array, the camelCase spelling.
    #[test]
    fn an_authenticate_reply_of_another_shape_is_other() {
        for reply in [
            json!({"ok": true}),
            json!({"ok": null}),
            json!({"ok": [true, true]}),
            json!({"ok": {"authenticated": true}}),
            json!({"ok": {"can_configure": true}}),
            json!({"ok": {"authenticated": "yes", "can_configure": false}}),
            json!({"ok": {"authenticated": true, "can_configure": 1}}),
            json!({"ok": {"ok": true, "canConfigure": true}}),
        ] {
            let (client, _) = client_on(input(std::slice::from_ref(&reply)));
            assert!(
                matches!(
                    client.authenticate("alice", "s3cret-Pw"),
                    Err(ClientError::Other(_))
                ),
                "{reply}"
            );
        }
    }

    /// No error the client gives back holds the password: an error reply
    /// that does not echo it, a broken stream, a reply of the wrong shape.
    #[test]
    fn an_authenticate_error_never_holds_the_password() {
        for bytes in [
            input(&[error_reply("Service", "pam_start failed")]),
            input(&[json!({"ok": "nope"})]),
            Vec::new(),
            frame(b"not json"),
        ] {
            let (client, _) = client_on(bytes);
            let e = client
                .authenticate("alice", "s3cret-Pw")
                .expect_err("an error");
            assert!(!format!("{e:?}").contains("s3cret-Pw"), "{e:?}");
            assert!(!e.text().contains("s3cret-Pw"), "{e:?}");
        }
    }

    // ── Acting for a signed-in user: the `As` forms ─────────────────────

    /// The one frame a client bound to `principal` sends for `act`, on a
    /// stream answering `reply`. `act` takes `&dyn ManagerClient`, so the
    /// bound client's own type stays the implementation's.
    fn frame_sent_for(
        principal: &str,
        reply: Value,
        act: fn(&dyn ManagerClient) -> Option<ClientError>,
    ) -> Value {
        let (client, sent) = client_on(input(&[reply]));
        let bound = client.acting_for(principal);
        act(&bound);
        let frames = sent_frames(&sent);
        assert_eq!(frames.len(), 1, "one request per call");
        frames[0].clone()
    }

    /// One row of the frame tables below: the method's name (to name a
    /// failure), the call to make on the bound client, the frame it must
    /// send. Named: the bare tuple trips clippy::type_complexity.
    type Case<'a> = (
        &'a str,
        fn(&dyn ManagerClient) -> Option<ClientError>,
        Value,
    );

    /// A client acting for `alice` sends the `As` form of every guarded
    /// method: the principal is one named field of the request, beside the
    /// method's own arguments, and the method's name says it carries one.
    #[test]
    fn an_acting_client_sends_the_as_forms() {
        let refused = error_reply("InvalidId", "InvalidId");
        let cases: [Case; 10] = [
            (
                "start",
                |c| c.start("default").err(),
                json!({"method": "StartAs", "principal": "alice", "id": "default"}),
            ),
            (
                "stop",
                |c| c.stop("lan-2").err(),
                json!({"method": "StopAs", "principal": "alice", "id": "lan-2"}),
            ),
            (
                "restart",
                |c| c.restart("lan-2").err(),
                json!({"method": "RestartAs", "principal": "alice", "id": "lan-2"}),
            ),
            (
                "read_file",
                |c| c.read_file("server@lan-2.ini").err(),
                json!({"method": "ReadFileAs", "principal": "alice", "name": "server@lan-2.ini"}),
            ),
            (
                "write_file",
                |c| {
                    c.write_file("models.ini", "[m]\nmodel = /m/x.gguf\n", false)
                        .err()
                },
                json!({"method": "WriteFileAs", "principal": "alice", "name": "models.ini", "contents": "[m]\nmodel = /m/x.gguf\n", "keep_api_key": false}),
            ),
            (
                "write_file_if",
                |c| {
                    c.write_file_if("models.ini", "[m]\nmodel = /m/x.gguf\n", "")
                        .err()
                },
                // The `expected` content travels beside the new one, and no
                // `keep_api_key`: the caller that holds the file's text has
                // no key to preserve.
                json!({"method": "WriteFileIfAs", "principal": "alice", "name": "models.ini", "contents": "[m]\nmodel = /m/x.gguf\n", "expected": ""}),
            ),
            (
                "set_boot_enabled",
                |c| c.set_boot_enabled("lan-2", true).err(),
                json!({"method": "SetBootEnabledAs", "principal": "alice", "id": "lan-2", "enabled": true}),
            ),
            (
                "set_model_profile",
                |c| c.set_model_profile("lan-2", "Qwen 3 (v2) é", "on").err(),
                json!({"method": "SetModelProfileAs", "principal": "alice", "instance": "lan-2", "profile": "Qwen 3 (v2) é", "value": "on"}),
            ),
            (
                "set_server_profile",
                |c| c.set_server_profile("lan-2", "Qwen 3 (v2) é").err(),
                json!({"method": "SetServerProfileAs", "principal": "alice", "instance": "lan-2", "profile": "Qwen 3 (v2) é"}),
            ),
            (
                "set_instance_tags",
                |c| c.set_instance_tags("lan-2", &tags()).err(),
                json!({"method": "SetInstanceTagsAs", "principal": "alice", "instance": "lan-2", "tags": ["edge", "v2"]}),
            ),
        ];
        for (method, act, request) in cases {
            assert_eq!(
                frame_sent_for("alice", refused.clone(), act),
                request,
                "{method}"
            );
        }
    }

    /// The methods that need no right are unchanged by acting for a
    /// principal: no principal in the request, the same frame as the
    /// unbound client's.
    #[test]
    fn an_acting_client_sends_the_unguarded_methods_unchanged() {
        let cases: [Case; 5] = [
            (
                "list_instances",
                |c| c.list_instances().err(),
                json!({"method": "ListInstances"}),
            ),
            (
                "status",
                |c| c.status("lan-2").err(),
                json!({"method": "Status", "id": "lan-2"}),
            ),
            (
                "boot_enabled",
                |c| c.boot_enabled("lan-2").err(),
                json!({"method": "BootEnabled", "id": "lan-2"}),
            ),
            (
                "authenticate",
                |c| c.authenticate("alice", "s3cret-Pw").err(),
                json!({"method": "Authenticate", "user": "alice", "password": "s3cret-Pw"}),
            ),
            (
                "instances",
                |c| c.instances().err(),
                json!({"method": "Instances"}),
            ),
        ];
        for (method, act, request) in cases {
            assert_eq!(
                frame_sent_for("alice", error_reply("InvalidId", "InvalidId"), act),
                request,
                "{method}"
            );
        }
    }

    /// The principal travels exactly as given: nothing trimmed, folded,
    /// shortened or dropped, the empty name and the longest one included (the
    /// manager is the one that refuses a name).
    #[test]
    fn an_acting_client_sends_the_principal_as_given() {
        let long = "a".repeat(256);
        for principal in ["alice", " Alice ", "root", "pässwörd é", "", "a\nb", &long] {
            assert_eq!(
                frame_sent_for(principal, error_reply("InvalidId", "InvalidId"), |c| c
                    .start("default")
                    .err()),
                json!({"method": "StartAs", "principal": principal, "id": "default"}),
                "{principal:?}"
            );
        }
    }

    /// A bound client is a `ManagerClient` a service can hand to its handler
    /// threads, and it shares the connection of the client it was made from:
    /// every call of either, in order, has its own reply.
    #[test]
    fn a_bound_client_shares_the_connection() {
        fn shared<C: ManagerClient + Send + Sync>(c: C) -> C {
            c
        }
        let (client, sent) = client_on(input(&[
            json!({"ok": null}),
            json!({"ok": null}),
            json!({"ok": "inactive"}),
        ]));
        assert_eq!(client.stop("default"), Ok(()), "the client's own call");
        let bound = shared(client.acting_for("alice"));
        assert_eq!(bound.stop("default"), Ok(()), "the bound client's call");
        assert_eq!(
            bound.status("default"),
            Ok(InstanceState::Inactive),
            "an unguarded method of the bound client"
        );
        assert_eq!(
            sent_frames(&sent),
            vec![
                json!({"method": "Stop", "id": "default"}),
                json!({"method": "StopAs", "principal": "alice", "id": "default"}),
                json!({"method": "Status", "id": "default"}),
            ]
        );
    }

    // ── Version ─────────────────────────────────────────────────────────

    /// The reply of a manager that has no method it is asked for: after it
    /// the server closes the connection.
    const BAD_REQUEST: &str = "org.privateai.Manager1.Error.BadRequest";

    /// `version` sends `{"method":"Version"}` and reads
    /// `{"ok":{"version","protocol"}}`. A protocol this client does not know
    /// (a newer manager) and a reply field it does not know are read through,
    /// not refused: the protocol grows with each additive change.
    #[test]
    fn version_sends_its_request_and_reads_its_reply() {
        for (version, protocol) in [("2.99.1", 1), ("3.0.0", 2), ("", 0)] {
            let (client, sent) = client_on(input(&[json!({
                "ok": {"version": version, "protocol": protocol, "extra": "a newer manager"},
            })]));
            assert_eq!(
                client.version(),
                Ok(Some(ManagerVersion {
                    version: version.to_string(),
                    protocol,
                })),
                "{version} {protocol}"
            );
            assert_eq!(
                sent_frames(&sent),
                vec![json!({"method": "Version"})],
                "{version}"
            );
        }
    }

    /// A manager with no `Version` method answers `BadRequest`: that is no
    /// version (None), not a failure.
    #[test]
    fn version_is_none_against_a_manager_without_the_method() {
        let (client, sent) = client_on(input(&[json!({
            "error": {"name": BAD_REQUEST, "message": "not a request"},
        })]));
        assert_eq!(client.version(), Ok(None));
        assert_eq!(
            sent_frames(&sent),
            vec![json!({"method": "Version"})],
            "one request"
        );
    }

    /// Only `BadRequest` is "no such method": every other failure is the
    /// failure it is, and a reply of another shape is Other.
    #[test]
    fn a_version_reply_of_another_shape_is_not_none() {
        for reply in [
            json!({"ok": null}),
            json!({"ok": "2.99.1"}),
            json!({"ok": [1]}),
            json!({"ok": {"version": "2.99.1"}}),
            json!({"ok": {"protocol": 1}}),
            json!({"ok": {"version": 1, "protocol": 1}}),
            json!({"ok": {"version": "2.99.1", "protocol": "1"}}),
            json!({"ok": {"version": "2.99.1", "protocol": -1}}),
            json!({"ok": {"version": "2.99.1", "protocol": 4294967296u64}}),
        ] {
            let (client, _) = client_on(input(std::slice::from_ref(&reply)));
            assert!(
                matches!(client.version(), Err(ClientError::Other(_))),
                "{reply}"
            );
        }
        for (reply, expected) in [
            (
                error_reply("NotAuthorized", "NotAuthorized"),
                ClientError::NotAuthorized,
            ),
            (
                error_reply("Service", "boom"),
                ClientError::Service("boom".into()),
            ),
        ] {
            let (client, _) = client_on(input(std::slice::from_ref(&reply)));
            assert_eq!(client.version(), Err(expected), "{reply}");
        }
        let (client, _) = client_on(Vec::new());
        assert_eq!(
            client.version(),
            Err(ClientError::Unavailable),
            "no manager"
        );
    }
}
