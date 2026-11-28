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
//! {"method":"CopyServerProfile","engine":"llama.cpp","from":"lab","profile":"team"}
//! {"method":"CopyInstanceProfile","from":"lab","profile":"team"}
//! {"method":"CreateModelProfile","profile":"qwen3","from":"","values":{"model":"/m/a.gguf"}}
//! {"method":"DeleteModelProfile","profile":"qwen3"}
//! {"method":"ServerProfiles"}
//! {"method":"Devices"}
//! {"method":"Authenticate","user":"alice","password":"..."}
//! {"method":"ModelInfo","profile":"qwen3"}
//! {"method":"NodeInfo"}
//! {"method":"ModelFiles"}
//! {"method":"Version"}
//! ```
//!
//! A client [`PipeClient::acting_for`] a signed-in user vouches for: `Start`,
//! `Stop`, `Restart`, `ReadFile`, `WriteFile`, `WriteFileIf`,
//! `SetBootEnabled`, `SetModelProfile`, `SetServerProfile`,
//! `CreateServerProfile`, `DeleteServerProfile`, `CopyServerProfile`,
//! `CopyInstanceProfile`, `CreateModelProfile`,
//! `DeleteModelProfile`,
//! `SetInstanceTags`, `Devices` and `ModelInfo` and `ModelFiles` go as the
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
//! {"method":"ModelInfoAs","principal":"alice","profile":"qwen3"}
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
//! {"ok":8581}                               CreateInstance (the port the
//!                                           create proposed for the instance)
//! {"ok":{"running":["default"],
//!         "notConverged":[{"instance":"lab","key":"no-model-profile-named",
//!                          "args":["qwen3"]}]}}
//!                                           The saves that re-render (IP3c,
//!                                           WriteFileIf, SetModelProfile,
//!                                           SetServerProfile,
//!                                           SetInstanceProfile,
//!                                           SetInstanceAddress, the model
//!                                           profile delete, the entry add,
//!                                           remove and save, and the server
//!                                           profile switch, IP4a): running,
//!                                           the instances the save moved to a
//!                                           restart; notConverged, the
//!                                           instances that kept their last
//!                                           good pair, each with the check
//!                                           that stopped it
//! {"ok":{"version":"1.2.3","protocol":1}}   Version (protocol: the methods
//!                                           this manager speaks)
//! {"error":{"name":"org.privateai.Manager1.Error.<Kind>","message":"..."}}
//! ```

use super::{
    Affected, ClientError, DeviceInfo, InstanceInfo, InstanceProfileInfo, InstanceState,
    ManagerClient, ManagerFile, ManagerVersion, ModelFacts, ModelFileRow, ModelFilesReply,
    ModelInfoReply, ModelProfileSource, NodeDevices, NodeInfoReply, NotConverged, NotConvergedRow,
    Reason, ServerProfileInfo, TensorRow,
};
use crate::host::OsInfo;
use crate::vulkan::{status_of, Driver, NodeGpu};
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

/// The answer of a save that re-renders, as the frame carries it (IP3c,
/// `ARCHITECTURE.md` "Convergence", read back by IP4a): `running`, the ids of
/// the running instances the write left drifting, and `notConverged`, one
/// `{instance, key, args}` row per instance that kept its last good pair. The
/// two fields are the shape every such save answers, so a reply that carries
/// neither list is no answer: the rows are never read out of an answer that has
/// none. A row's own fields are read by name, as the row of `Instances` reads
/// its `reason`.
fn affected_of(v: &Value) -> Result<Affected, ClientError> {
    let list = |name: &str| -> Result<&Vec<Value>, ClientError> {
        v.get(name)
            .and_then(Value::as_array)
            .ok_or_else(|| ClientError::Other(format!("the answer carries no {name}: {v}")))
    };
    let bad = || ClientError::Other(v.to_string());
    let running = list("running")?
        .iter()
        .map(|id| id.as_str().map(str::to_string).ok_or_else(bad))
        .collect::<Result<Vec<String>, ClientError>>()?;
    let mut not_converged = Vec::new();
    for row in list("notConverged")? {
        let text = |key: &str| -> String {
            row.get(key)
                .and_then(Value::as_str)
                .unwrap_or_default()
                .to_string()
        };
        not_converged.push(NotConvergedRow {
            instance: text("instance"),
            key: text("key"),
            args: row
                .get("args")
                .and_then(Value::as_array)
                .map(|args| {
                    args.iter()
                        .filter_map(|a| a.as_str().map(str::to_string))
                        .collect::<Vec<String>>()
                })
                .unwrap_or_default(),
        });
    }
    Ok(Affected {
        running,
        not_converged,
    })
}

/// One instance profile row of the frame reply, as the stream answers it: `server_profile`, `entries`, and the pool's text as
/// `revision`, which only `GetInstanceProfile` carries (the pool has one
/// text, not one per row). A row of the wrong shape is no answer.
fn profile_row(v: &Value) -> Result<InstanceProfileInfo, ClientError> {
    let entries = v
        .get("entries")
        .and_then(Value::as_array)
        .ok_or_else(|| ClientError::Other(v.to_string()))?
        .iter()
        .map(|e| {
            e.as_str()
                .map(str::to_string)
                .ok_or_else(|| ClientError::Other(v.to_string()))
        })
        .collect::<Result<Vec<String>, ClientError>>()?;
    Ok(InstanceProfileInfo {
        id: v
            .get("id")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_string(),
        server_profile: v
            .get("server_profile")
            .and_then(Value::as_str)
            .ok_or_else(|| ClientError::Other(v.to_string()))?
            .to_string(),
        entries,
        revision: v
            .get("revision")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_string(),
    })
}

impl<S: Read + Write> ManagerClient for PipeClient<S> {
    fn list_instances(&self) -> Result<Vec<String>, ClientError> {
        let v = self.call(json!({"method": "ListInstances"}))?;
        // One row per instance (IP3c): the row carries the states of the apply
        // beside the id. A manager of the older answer sends a list of plain
        // ids, which reads the same way.
        let rows = v
            .as_array()
            .ok_or_else(|| ClientError::Other(v.to_string()))?;
        rows.iter()
            .map(|row| match row {
                Value::String(id) => Ok(id.clone()),
                row => row
                    .get("id")
                    .and_then(Value::as_str)
                    .map(str::to_string)
                    .ok_or_else(|| ClientError::Other(v.to_string())),
            })
            .collect()
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
    fn write_file_if(
        &self,
        name: &str,
        contents: &str,
        expected: &str,
    ) -> Result<Affected, ClientError> {
        // The compare and swap is a save that re-renders, so it answers the apply
        // (IP3c): the running instances the write moved to a restart, beside the
        // instances that kept their last good pair.
        let reply = self.call(self.guarded(
            "WriteFileIf",
            &[
                ("name", json!(name)),
                ("contents", json!(contents)),
                ("expected", json!(expected)),
            ],
        ))?;
        affected_of(&reply)
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
        // The two switches of slice 13 are saves that re-render, so they answer
        // the apply too (IP3c), read the same way.
        self.call(self.guarded(
            "SetModelProfile",
            &[
                ("instance", json!(instance)),
                ("profile", json!(profile)),
                ("value", json!(value)),
            ],
        ))
        .map(|_| ())
    }

    fn set_server_profile(&self, instance: &str, profile: &str) -> Result<(), ClientError> {
        self.call(self.guarded(
            "SetServerProfile",
            &[("instance", json!(instance)), ("profile", json!(profile))],
        ))
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
    fn copy_server_profile(
        &self,
        engine: &str,
        from: &str,
        profile: &str,
    ) -> Result<(), ClientError> {
        self.call_unit(self.guarded(
            "CopyServerProfile",
            &[
                ("engine", json!(engine)),
                ("from", json!(from)),
                ("profile", json!(profile)),
            ],
        ))
    }

    /// `CreateModelProfile`: the source expanded as the frame carries it, `from`
    /// the profile to copy (`""` for none) and `values` the pick's keys, an
    /// object of strings, `{}` for a clone.
    fn create_model_profile(
        &self,
        profile: &str,
        source: &ModelProfileSource,
    ) -> Result<(), ClientError> {
        let (from, values) = source.as_source();
        self.call_unit(self.guarded(
            "CreateModelProfile",
            &[
                ("profile", json!(profile)),
                ("from", json!(from)),
                (
                    "values",
                    Value::Object(values.into_iter().map(|(k, v)| (k, json!(v))).collect()),
                ),
            ],
        ))
    }

    fn delete_model_profile(&self, profile: &str) -> Result<Affected, ClientError> {
        // The delete re-renders, so it answers the apply (IP4a).
        let reply =
            self.call(self.guarded("DeleteModelProfile", &[("profile", json!(profile))]))?;
        affected_of(&reply)
    }

    /// `CreateInstance`: the three names the manager's rule asks for, the
    /// instance's id sent as the frame's `instance`, the name every verb
    /// keyed on an instance uses, and the instance profile the new instance
    /// runs (IP4b). The frame names no port: the manager proposes the new
    /// instance's own and answers it.
    fn create_instance(
        &self,
        engine: &str,
        id: &str,
        instance_profile: &str,
    ) -> Result<u16, ClientError> {
        let reply = self.call(self.guarded(
            "CreateInstance",
            &[
                ("engine", json!(engine)),
                ("instance", json!(id)),
                ("instance_profile", json!(instance_profile)),
            ],
        ))?;
        let port = reply
            .as_u64()
            .ok_or_else(|| ClientError::Other(reply.to_string()))?;
        u16::try_from(port).map_err(|_| ClientError::Other(reply.to_string()))
    }

    // The instance profile verbs, the frame fields named as the server profile
    // pair names them. The two reads are guarded too: an acting client asks the
    // pool for the signed-in user, and the pool is a `configure` read.
    fn instance_profiles(&self) -> Result<Vec<InstanceProfileInfo>, ClientError> {
        let v = self.call(self.guarded("InstanceProfiles", &[]))?;
        let rows = v
            .as_array()
            .ok_or_else(|| ClientError::Other(v.to_string()))?;
        rows.iter().map(profile_row).collect()
    }
    fn instance_profile(&self, profile: &str) -> Result<InstanceProfileInfo, ClientError> {
        let v = self.call(self.guarded("GetInstanceProfile", &[("profile", json!(profile))]))?;
        profile_row(&v).map(|mut row| {
            row.id = profile.to_string();
            row
        })
    }
    fn create_instance_profile(
        &self,
        profile: &str,
        server_profile: &str,
    ) -> Result<(), ClientError> {
        self.call_unit(self.guarded(
            "CreateInstanceProfile",
            &[
                ("profile", json!(profile)),
                ("server_profile", json!(server_profile)),
            ],
        ))
    }
    fn delete_instance_profile(&self, profile: &str) -> Result<(), ClientError> {
        self.call_unit(self.guarded("DeleteInstanceProfile", &[("profile", json!(profile))]))
    }
    fn copy_instance_profile(&self, from: &str, profile: &str) -> Result<(), ClientError> {
        self.call_unit(self.guarded(
            "CopyInstanceProfile",
            &[("from", json!(from)), ("profile", json!(profile))],
        ))
    }
    fn add_instance_profile_model(
        &self,
        profile: &str,
        model: &str,
    ) -> Result<Affected, ClientError> {
        // The apply's answer (IP3c): the running instances the write moved to a
        // restart, and the instances that kept their last good pair.
        let reply = self.call(self.guarded(
            "AddInstanceProfileModel",
            &[("profile", json!(profile)), ("model", json!(model))],
        ))?;
        affected_of(&reply)
    }
    fn remove_instance_profile_model(
        &self,
        profile: &str,
        model: &str,
    ) -> Result<Affected, ClientError> {
        let reply = self.call(self.guarded(
            "RemoveInstanceProfileModel",
            &[("profile", json!(profile)), ("model", json!(model))],
        ))?;
        affected_of(&reply)
    }
    /// `set_instance_profile_model` (IP4a): the entry whole as
    /// `[{key, value}, …]`, beside the revision it swaps against. The list keeps
    /// the order the page sent its keys in, which a JSON object of them would
    /// not: the entry holds its keys as the page sent them.
    fn set_instance_profile_model(
        &self,
        profile: &str,
        model: &str,
        values: &[(String, String)],
        revision: &str,
    ) -> Result<Affected, ClientError> {
        let entry: Vec<Value> = values
            .iter()
            .map(|(key, value)| json!({"key": key, "value": value}))
            .collect();
        let reply = self.call(self.guarded(
            "SetInstanceProfileModel",
            &[
                ("profile", json!(profile)),
                ("model", json!(model)),
                ("values", json!(entry)),
                ("revision", json!(revision)),
            ],
        ))?;
        affected_of(&reply)
    }
    /// `set_instance_profile_server` (IP4a): the switch of an instance profile to
    /// another server profile, whose rows include the instances whose card the
    /// switch took away.
    fn set_instance_profile_server(
        &self,
        profile: &str,
        server_profile: &str,
    ) -> Result<Affected, ClientError> {
        let reply = self.call(self.guarded(
            "SetInstanceProfileServer",
            &[
                ("profile", json!(profile)),
                ("server_profile", json!(server_profile)),
            ],
        ))?;
        affected_of(&reply)
    }
    fn set_instance_profile(
        &self,
        instance: &str,
        profile: &str,
        revision: &str,
    ) -> Result<Affected, ClientError> {
        // The record's two writes are saves that re-render, so they answer the
        // apply too (IP3c).
        let reply = self.call(self.guarded(
            "SetInstanceProfile",
            &[
                ("instance", json!(instance)),
                ("profile", json!(profile)),
                ("revision", json!(revision)),
            ],
        ))?;
        affected_of(&reply)
    }
    fn set_instance_address(
        &self,
        instance: &str,
        host: &str,
        port: &str,
        revision: &str,
    ) -> Result<Affected, ClientError> {
        let reply = self.call(self.guarded(
            "SetInstanceAddress",
            &[
                ("instance", json!(instance)),
                ("host", json!(host)),
                ("port", json!(port)),
                ("revision", json!(revision)),
            ],
        ))?;
        affected_of(&reply)
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
                    // The pipe has a null: `null` is "this section has no
                    // label". The field is new, so a row of an older manager
                    // answers none: that row simply carries no label. A field
                    // that is no text and not `null` is a shape, like every
                    // other one.
                    let label = match row.get("label") {
                        None | Some(Value::Null) => None,
                        Some(Value::String(text)) => Some(text.clone()),
                        Some(_) => return Err(ClientError::Other(v.to_string())),
                    };
                    // The states of the apply (IP3c), the same way: a row of a
                    // manager that answers none of them reads converged, with no
                    // reason and no drift.
                    let reason = match row.get("reason") {
                        None | Some(Value::Null) => None,
                        Some(value) => {
                            let key = value.get("key").and_then(Value::as_str);
                            let args = value.get("args").and_then(Value::as_array).map(|list| {
                                list.iter()
                                    .filter_map(|a| a.as_str().map(str::to_string))
                                    .collect::<Vec<String>>()
                            });
                            match (key, args) {
                                (Some(key), Some(args)) => Some(NotConverged {
                                    key: key.to_string(),
                                    args,
                                }),
                                _ => return Err(ClientError::Other(v.to_string())),
                            }
                        }
                    };
                    out.push(InstanceInfo {
                        id,
                        engine,
                        server_profile: Some(profile.to_string()),
                        port: u16::try_from(port).map_err(|_| ClientError::Other(v.to_string()))?,
                        profile_tags,
                        instance_tags,
                        label,
                        converged: row
                            .get("converged")
                            .and_then(Value::as_bool)
                            .unwrap_or(true),
                        reason,
                        pending_restart: row
                            .get("pendingRestart")
                            .and_then(Value::as_bool)
                            .unwrap_or(false),
                        // The two fields appended to the row (IP4b), read as
                        // the label is: `null`, the empty string, and a row of
                        // an older manager that answers neither, all read as
                        // no instance profile and no host.
                        instance_profile: text(row, "instance_profile").filter(|t| !t.is_empty()),
                        hostname: text(row, "hostname").filter(|t| !t.is_empty()),
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
    fn set_instance_label(&self, instance: &str, label: &str) -> Result<(), ClientError> {
        self.call_unit(self.guarded(
            "SetInstanceLabel",
            &[("instance", json!(instance)), ("label", json!(label))],
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
    fn model_info(&self, profile: &str) -> Result<ModelInfoReply, ClientError> {
        let v = self.call(self.guarded("ModelInfo", &[("profile", json!(profile))]))?;
        // Every field read strictly, like the other rows: a page that showed a
        // half-reply would set its sliders at the wrong end, and a 0 blocks is
        // what a model with 64 looks like when a field is not there.
        let bad = || ClientError::Other(v.to_string());
        let model = match v.get("model") {
            Some(Value::Null) => None,
            Some(one) => Some(one.as_str().ok_or_else(&bad)?.to_string()),
            None => return Err(bad()),
        };
        // Each of the two an object or null, its key always there, and exactly
        // one of them: a reply with both says what it read and what it did
        // not, and one with neither says nothing.
        let part = |key: &str| -> Result<Option<&Value>, ClientError> {
            match v.get(key) {
                None => Err(bad()),
                Some(Value::Null) => Ok(None),
                Some(one @ Value::Object(_)) => Ok(Some(one)),
                Some(_) => Err(bad()),
            }
        };
        let (facts, why) = (part("info")?, part("reason")?);
        if facts.is_some() == why.is_some() {
            return Err(bad());
        }
        let info = facts
            .map(|o| -> Result<ModelFacts, ClientError> {
                let count = |key: &str| o.get(key).and_then(Value::as_u64);
                // The tensor list the page places weights from: the manager
                // writes it with every read, so a `tensors` that is not a list
                // of `{name, bytes}` rows, or a `tensors_truncated` that is not
                // a bool, is the shape of no manager.
                let rows = o
                    .get("tensors")
                    .and_then(Value::as_array)
                    .ok_or_else(&bad)?
                    .iter()
                    .map(|row| -> Result<TensorRow, ClientError> {
                        Ok(TensorRow {
                            name: row
                                .get("name")
                                .and_then(Value::as_str)
                                .ok_or_else(&bad)?
                                .to_string(),
                            bytes: row.get("bytes").and_then(Value::as_u64).ok_or_else(&bad)?,
                        })
                    })
                    .collect::<Result<Vec<_>, _>>()?;
                match (
                    count("block_count"),
                    o.get("moe").and_then(Value::as_bool),
                    count("expert_count"),
                    count("expert_used"),
                    o.get("tensors_truncated").and_then(Value::as_bool),
                ) {
                    (
                        Some(block_count),
                        Some(moe),
                        Some(expert_count),
                        Some(expert_used),
                        Some(tensors_truncated),
                    ) => Ok(ModelFacts {
                        block_count,
                        moe,
                        expert_count,
                        expert_used,
                        tensors: rows,
                        tensors_truncated,
                    }),
                    _ => Err(bad()),
                }
            })
            .transpose()?;
        let reason = why
            .map(|o| -> Result<Reason, ClientError> {
                let key = o.get("key").and_then(Value::as_str).ok_or_else(&bad)?;
                let args = o.get("args").and_then(Value::as_array).ok_or_else(&bad)?;
                Ok(Reason {
                    key: key.to_string(),
                    args: args
                        .iter()
                        .map(|a| a.as_str().map(str::to_string).ok_or_else(&bad))
                        .collect::<Result<Vec<_>, _>>()?,
                })
            })
            .transpose()?;
        Ok(ModelInfoReply {
            model,
            info,
            reason,
        })
    }
    fn node_info(&self) -> Result<NodeInfoReply, ClientError> {
        let v = self.call(self.guarded("NodeInfo", &[]))?;
        let bad = || ClientError::Other(v.to_string());
        // The manager writes every key of this document, so a key that is not
        // there is the shape of no manager, not a fact the host left out: each
        // text is read strictly, and the one that is answered with nothing is
        // the `null`, which arrives as `None`. A client that guessed at a
        // missing key would show a driver the node does not have.
        let text = |o: &serde_json::Map<String, Value>,
                    key: &str|
         -> Result<Option<String>, ClientError> {
            match o.get(key) {
                None => Err(bad()),
                Some(Value::Null) => Ok(None),
                Some(one) => Ok(Some(one.as_str().ok_or_else(&bad)?.to_string())),
            }
        };
        let os = v.get("os").and_then(Value::as_object).ok_or_else(&bad)?;
        let os = OsInfo {
            name: text(os, "name")?,
            version: text(os, "version")?,
            kernel: text(os, "kernel")?,
        };
        // The word the node's Vulkan read answered, exactly one of the three:
        // a `null`, `"OK"` with its capitals, a number or a word the contract
        // does not know say nothing about the node, and a client that guessed
        // `none` from them would hide a probe that failed.
        let vulkan = v
            .get("vulkan")
            .and_then(Value::as_str)
            .and_then(status_of)
            .ok_or_else(&bad)?;
        let rows = v.get("gpus").and_then(Value::as_array).ok_or_else(&bad)?;
        let mut gpus = Vec::with_capacity(rows.len());
        for row in rows {
            let row = row.as_object().ok_or_else(&bad)?;
            // The row's Vulkan driver: null (no Vulkan device on this card),
            // or the pair with both of its texts, each a string or null. An
            // object of one key, an object of none, a text and a list are all
            // a shape the manager never answers.
            let vulkan = match row.get("vulkan") {
                None => return Err(bad()),
                Some(Value::Null) => None,
                Some(one) => Some(Driver {
                    driver: text(one.as_object().ok_or_else(&bad)?, "driver")?,
                    info: text(one.as_object().ok_or_else(&bad)?, "info")?,
                }),
            };
            gpus.push(NodeGpu {
                pci: text(row, "pci")?,
                vendor: text(row, "vendor")?,
                name: text(row, "name")?,
                driver: text(row, "driver")?,
                version: text(row, "version")?,
                vulkan,
            });
        }
        Ok(NodeInfoReply { os, vulkan, gpus })
    }
    fn model_files(&self) -> Result<ModelFilesReply, ClientError> {
        let v = self.call(self.guarded("ModelFiles", &[]))?;
        let bad = || ClientError::Other(v.to_string());
        // The manager writes every key of this document, so a key that is not
        // there is the shape of no manager, not a folder that left a file out:
        // every field is read strictly, the way `node_info` reads its texts. A
        // client that guessed at a missing key would pick a file the node does
        // not have.
        let obj = v.as_object().ok_or_else(&bad)?;
        let text = |o: &serde_json::Map<String, Value>, key: &str| -> Result<String, ClientError> {
            o.get(key)
                .and_then(Value::as_str)
                .map(str::to_string)
                .ok_or_else(&bad)
        };
        let rows = obj
            .get("files")
            .and_then(Value::as_array)
            .ok_or_else(&bad)?;
        let mut files = Vec::with_capacity(rows.len());
        for row in rows {
            let row = row.as_object().ok_or_else(&bad)?;
            // The row's category: null (a file of the folder itself), or the
            // first subfolder's name. A number and a list are shapes the
            // manager never answers.
            let category = match row.get("category") {
                None => return Err(bad()),
                Some(Value::Null) => None,
                Some(one) => Some(one.as_str().ok_or_else(&bad)?.to_string()),
            };
            files.push(ModelFileRow {
                path: text(row, "path")?,
                size: row.get("size").and_then(Value::as_u64).ok_or_else(&bad)?,
                shards: row.get("shards").and_then(Value::as_u64).ok_or_else(&bad)?,
                category,
            });
        }
        // The folder's keyed why: null (the folder read), or the key with its
        // args of strings. An unreadable folder is an OK answer with an empty
        // list, so the key travels inside the reply, never as the call's
        // failure: the picker shows the empty list with the folder's text.
        let error = match obj.get("error") {
            None => return Err(bad()),
            Some(Value::Null) => None,
            Some(one) => {
                let one = one.as_object().ok_or_else(&bad)?;
                Some(Reason {
                    key: text(one, "key")?,
                    args: one
                        .get("args")
                        .and_then(Value::as_array)
                        .ok_or_else(&bad)?
                        .iter()
                        .map(|a| a.as_str().map(str::to_string).ok_or_else(&bad))
                        .collect::<Result<Vec<String>, ClientError>>()?,
                })
            }
        };
        Ok(ModelFilesReply {
            dir: text(obj, "dir")?,
            files,
            truncated: obj
                .get("truncated")
                .and_then(Value::as_bool)
                .ok_or_else(&bad)?,
            error,
        })
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
#[path = "pipe_tests.rs"]
mod tests;
