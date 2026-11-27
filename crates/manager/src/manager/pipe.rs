//! The manager over a byte stream: the face of `pai-manager` on Windows,
//! where the configurator reaches it over a local named pipe. OS-neutral:
//! the frame format and the requests are `pai_core::client::pipe`'s (its
//! module header), the errors the same (name, message) pairs the D-Bus
//! interface replies with. The guarded methods answer in their `As` form
//! too, with the principal the caller vouches for; `Version` answers the
//! framework version and [`PROTOCOL`] to anybody. The pipe itself, its
//! security descriptor and the caller's identity are the Windows side's.

pub use super::service::error_reply;
use super::service::{
    state_name, Authority, Caller, Manager, ManagerError, Principal, Requester, Resolver,
    ServiceControl,
};
use pai_core::client::pipe::{read_frame, write_frame, FrameError};
use serde_json::{json, Value};
use std::io::{Error, ErrorKind, Read, Write};

/// The error name of the reply to a frame that is no request (not JSON, an
/// unknown method, a missing argument, over `MAX_FRAME`); the connection
/// ends after it.
pub const BAD_REQUEST: &str = "org.privateai.Manager1.Error.BadRequest";

/// The protocol version this manager answers with, the number `Version`
/// reports beside the framework version. 2 is the `As` methods; 3 adds
/// `SetModelProfile`, the assignment verb, and `SetServerProfile`, the switch
/// of an instance to another server profile, each with its `As` form, and
/// `Instances`, the list with each instance's tags, and `SetInstanceTags`
/// with its `As` form; 3 also carries `CreateServerProfile` and
/// `DeleteServerProfile`, each with its `As` form, and `ServerProfiles`, the
/// list of the node's profiles, and `CreateInstance` and `RemoveInstance`,
/// each with its `As` form, and `ModelInfo` with its `As` form, the layer
/// facts of one saved model profile, and `NodeInfo` with its `As` form, the
/// node's own info; 3 also carries `ModelFiles` with its `As` form, the node's
/// one models folder as the model-file picker's list.
pub const PROTOCOL: u32 = 3;

/// The suffix of the `As` form of a guarded method, the one that carries a
/// `principal`.
const AS_SUFFIX: &str = "As";

/// A request, its arguments read (`pai_core::client::pipe`'s header). The
/// guarded methods carry the principal the caller vouches for, None when
/// the call is the caller's own.
enum Request {
    ListInstances,
    /// Every instance with the tags of its server profile and its own.
    Instances,
    Status(String),
    Version,
    Start(String, Option<Principal>),
    Stop(String, Option<Principal>),
    Restart(String, Option<Principal>),
    ReadFile(String, Option<Principal>),
    WriteFile(String, String, bool, Option<Principal>),
    WriteFileIf(String, String, String, Option<Principal>),
    BootEnabled(String),
    SetBootEnabled(String, bool, Option<Principal>),
    SetModelProfile(String, String, String, Option<Principal>),
    /// `instance`, `profile`: point an instance at a server profile.
    SetServerProfile(String, String, Option<Principal>),
    /// `instance`, `tags`: the record's own tags of an instance.
    SetInstanceTags(String, Vec<String>, Option<Principal>),
    /// `instance`, `label`: the label of an instance, the text the web UI shows
    /// beside its id (IL1). The empty text removes the key.
    SetInstanceLabel(String, String, Option<Principal>),
    /// `engine`, `profile`, `port`: writes a new server profile (`0` leaves
    /// the file no `Port` key).
    CreateServerProfile(String, String, u16, Option<Principal>),
    /// `engine`, `profile`: removes a server profile no instance points at.
    DeleteServerProfile(String, String, Option<Principal>),
    // ── The instance profile verbs (`ARCHITECTURE.md` "Instance profiles").
    /// No arguments: the node's instance profiles.
    InstanceProfiles(Option<Principal>),
    /// `profile`: one instance profile, with the pool's text as revision.
    GetInstanceProfile(String, Option<Principal>),
    /// `profile`, `server_profile`: writes a new instance profile.
    CreateInstanceProfile(String, String, Option<Principal>),
    /// `profile`: removes an instance profile no instance runs.
    DeleteInstanceProfile(String, Option<Principal>),
    /// `profile`, `model`: gives an instance profile a model profile.
    AddInstanceProfileModel(String, String, Option<Principal>),
    /// `profile`, `model`: takes a model profile away from an instance profile.
    RemoveInstanceProfileModel(String, String, Option<Principal>),
    /// `instance`, `profile`, `revision`: points an instance at an instance
    /// profile, the revision the text of `instance-profiles.ini` the caller
    /// read.
    SetInstanceProfile(String, String, String, Option<Principal>),
    /// `instance`, `host`, `port`, `revision`: the address an instance
    /// listens on, the port a text, the revision the text of `instances.ini`
    /// the caller read.
    SetInstanceAddress(String, String, String, String, Option<Principal>),
    /// `profile`, `from`, `values`: writes a new model profile, `from` a
    /// profile to copy (`""` for none) and `values` the keys of the file pick.
    CreateModelProfile(String, String, Vec<(String, String)>, Option<Principal>),
    /// `profile`: removes a switched-off, unassigned model profile.
    DeleteModelProfile(String, Option<Principal>),
    /// `engine`, `instance`, `server_profile`: creates an instance.
    CreateInstance(String, String, String, Option<Principal>),
    /// `instance`: removes an instance.
    RemoveInstance(String, Option<Principal>),
    /// Every server profile of the node, with the instance running it.
    ServerProfiles,
    /// The node's compute devices, from the engine's own probe.
    Devices(Option<Principal>),
    /// `profile`: the layer facts of one saved model profile's model.
    ModelInfo(String, Option<Principal>),
    /// The node's own info: its OS and kernel, the Vulkan word and the rows.
    NodeInfo(Option<Principal>),
    /// The node's one models folder: its path, its `.gguf` rows, the walk's
    /// bound and the folder's keyed why.
    ModelFiles(Option<Principal>),
    Authenticate(String, String),
}

impl Request {
    /// The request a frame's body holds, None when it holds none.
    fn parse(body: &[u8]) -> Option<Request> {
        let v: Value = serde_json::from_slice(body).ok()?;
        let method = v.get("method")?.as_str()?;
        // The principal travels only on the `As` form, and always: a method
        // and a field that do not agree is no request. Every other unknown
        // field stays ignored, as a client of an older protocol expects.
        let as_form = method.ends_with(AS_SUFFIX);
        if v.get("principal").is_some() != as_form {
            return None;
        }
        let arg = |name: &str| v.get(name)?.as_str().map(str::to_string);
        // A missing principal, or one that is not a string, is no request (the
        // `?` at the use); present, it is the `Some` the `As` request carries.
        let principal = || Some(Principal(v.get("principal")?.as_str()?.to_string()));
        // The tags list: an array of strings. An entry that is not a string,
        // or a value that is no array, is no request: the rule of the list is
        // the manager's, not the frame's.
        let tags = || {
            v.get("tags")?
                .as_array()?
                .iter()
                .map(|t| t.as_str().map(str::to_string))
                .collect::<Option<Vec<String>>>()
        };
        // The port of a create: an integer 0 to 65535, 0 for no port. A
        // string, a fraction, a sign, a number out of range, or no field, is
        // no request: the shape of the frame is the frame's rule, what a port
        // means is the core's.
        let port = || u16::try_from(v.get("port")?.as_u64()?).ok();
        // The source of a model profile create: `from` the name of the profile
        // to copy, no field being none, and `values` the keys of the file pick,
        // an object of strings, required. A `from` that is no string, a
        // `values` that is no object (an array of pairs is the bus's shape, not
        // the frame's), an entry that is no string, or `values` missing, is no
        // request.
        let from = || match v.get("from") {
            Some(value) => value.as_str().map(str::to_string),
            None => Some(String::new()),
        };
        let values = || {
            v.get("values")?
                .as_object()?
                .iter()
                .map(|(key, value)| Some((key.clone(), value.as_str()?.to_string())))
                .collect::<Option<Vec<(String, String)>>>()
        };
        Some(match method {
            "ListInstances" => Request::ListInstances,
            "Instances" => Request::Instances,
            "Status" => Request::Status(arg("id")?),
            "Version" => Request::Version,
            "Start" => Request::Start(arg("id")?, None),
            "StartAs" => Request::Start(arg("id")?, principal().map(Some)?),
            "Stop" => Request::Stop(arg("id")?, None),
            "StopAs" => Request::Stop(arg("id")?, principal().map(Some)?),
            "Restart" => Request::Restart(arg("id")?, None),
            "RestartAs" => Request::Restart(arg("id")?, principal().map(Some)?),
            "ReadFile" => Request::ReadFile(arg("name")?, None),
            "ReadFileAs" => Request::ReadFile(arg("name")?, principal().map(Some)?),
            "WriteFile" => Request::WriteFile(
                arg("name")?,
                arg("contents")?,
                v.get("keep_api_key")?.as_bool()?,
                None,
            ),
            "WriteFileAs" => Request::WriteFile(
                arg("name")?,
                arg("contents")?,
                v.get("keep_api_key")?.as_bool()?,
                principal().map(Some)?,
            ),
            // The compare and swap names no `keep_api_key`: the caller holds
            // the file's own text, key line included, and sends it back.
            "WriteFileIf" => {
                Request::WriteFileIf(arg("name")?, arg("contents")?, arg("expected")?, None)
            }
            "WriteFileIfAs" => Request::WriteFileIf(
                arg("name")?,
                arg("contents")?,
                arg("expected")?,
                principal().map(Some)?,
            ),
            "BootEnabled" => Request::BootEnabled(arg("id")?),
            "SetBootEnabled" => {
                Request::SetBootEnabled(arg("id")?, v.get("enabled")?.as_bool()?, None)
            }
            "SetBootEnabledAs" => Request::SetBootEnabled(
                arg("id")?,
                v.get("enabled")?.as_bool()?,
                principal().map(Some)?,
            ),
            // The assignment verb: instance, model profile and value, the
            // names the method's own (the frame's fields are camelCase, as
            // every other method's).
            "SetModelProfile" => {
                Request::SetModelProfile(arg("instance")?, arg("profile")?, arg("value")?, None)
            }
            "SetModelProfileAs" => Request::SetModelProfile(
                arg("instance")?,
                arg("profile")?,
                arg("value")?,
                principal().map(Some)?,
            ),
            // The switch verb: the instance and the server profile it names.
            "SetServerProfile" => {
                Request::SetServerProfile(arg("instance")?, arg("profile")?, None)
            }
            "SetServerProfileAs" => {
                Request::SetServerProfile(arg("instance")?, arg("profile")?, principal().map(Some)?)
            }
            // The tags verb: the instance and the list that replaces the
            // record's own tags of it.
            "SetInstanceTags" => Request::SetInstanceTags(arg("instance")?, tags()?, None),
            "SetInstanceTagsAs" => {
                Request::SetInstanceTags(arg("instance")?, tags()?, principal().map(Some)?)
            }
            // The label verb: the instance and the text the web UI shows beside
            // its id. The text is required, and `""` is a text: the clear.
            "SetInstanceLabel" => Request::SetInstanceLabel(arg("instance")?, arg("label")?, None),
            "SetInstanceLabelAs" => {
                Request::SetInstanceLabel(arg("instance")?, arg("label")?, principal().map(Some)?)
            }
            // The create of a server profile: the engine, the name and the port.
            "CreateServerProfile" => {
                Request::CreateServerProfile(arg("engine")?, arg("profile")?, port()?, None)
            }
            "CreateServerProfileAs" => Request::CreateServerProfile(
                arg("engine")?,
                arg("profile")?,
                port()?,
                principal().map(Some)?,
            ),
            // Its removal.
            "DeleteServerProfile" => {
                Request::DeleteServerProfile(arg("engine")?, arg("profile")?, None)
            }
            "DeleteServerProfileAs" => Request::DeleteServerProfile(
                arg("engine")?,
                arg("profile")?,
                principal().map(Some)?,
            ),
            // The instance profile verbs (`ARCHITECTURE.md` "Instance
            // profiles"). The reads take no argument beside `profile`; the
            // writes carry `revision`, the file's text the caller read.
            "InstanceProfiles" => Request::InstanceProfiles(None),
            "InstanceProfilesAs" => Request::InstanceProfiles(principal().map(Some)?),
            "GetInstanceProfile" => Request::GetInstanceProfile(arg("profile")?, None),
            "GetInstanceProfileAs" => {
                Request::GetInstanceProfile(arg("profile")?, principal().map(Some)?)
            }
            "CreateInstanceProfile" => {
                Request::CreateInstanceProfile(arg("profile")?, arg("server_profile")?, None)
            }
            "CreateInstanceProfileAs" => Request::CreateInstanceProfile(
                arg("profile")?,
                arg("server_profile")?,
                principal().map(Some)?,
            ),
            "DeleteInstanceProfile" => Request::DeleteInstanceProfile(arg("profile")?, None),
            "DeleteInstanceProfileAs" => {
                Request::DeleteInstanceProfile(arg("profile")?, principal().map(Some)?)
            }
            "AddInstanceProfileModel" => {
                Request::AddInstanceProfileModel(arg("profile")?, arg("model")?, None)
            }
            "AddInstanceProfileModelAs" => Request::AddInstanceProfileModel(
                arg("profile")?,
                arg("model")?,
                principal().map(Some)?,
            ),
            "RemoveInstanceProfileModel" => {
                Request::RemoveInstanceProfileModel(arg("profile")?, arg("model")?, None)
            }
            "RemoveInstanceProfileModelAs" => Request::RemoveInstanceProfileModel(
                arg("profile")?,
                arg("model")?,
                principal().map(Some)?,
            ),
            "SetInstanceProfile" => Request::SetInstanceProfile(
                arg("instance")?,
                arg("profile")?,
                arg("revision")?,
                None,
            ),
            "SetInstanceProfileAs" => Request::SetInstanceProfile(
                arg("instance")?,
                arg("profile")?,
                arg("revision")?,
                principal().map(Some)?,
            ),
            "SetInstanceAddress" => Request::SetInstanceAddress(
                arg("instance")?,
                arg("host")?,
                arg("port")?,
                arg("revision")?,
                None,
            ),
            "SetInstanceAddressAs" => Request::SetInstanceAddress(
                arg("instance")?,
                arg("host")?,
                arg("port")?,
                arg("revision")?,
                principal().map(Some)?,
            ),
            // The create of a model profile: the id, the profile to copy (no
            // field is none) and the keys of the file pick, an object of
            // strings. A `values` that is no object, a value that is no
            // string, a `from` that is no string, is no request.
            "CreateModelProfile" => {
                Request::CreateModelProfile(arg("profile")?, from()?, values()?, None)
            }
            "CreateModelProfileAs" => Request::CreateModelProfile(
                arg("profile")?,
                from()?,
                values()?,
                principal().map(Some)?,
            ),
            // Its removal.
            "DeleteModelProfile" => Request::DeleteModelProfile(arg("profile")?, None),
            "DeleteModelProfileAs" => {
                Request::DeleteModelProfile(arg("profile")?, principal().map(Some)?)
            }
            // The create of an instance: the engine, the id and the server
            // profile it runs. The id is the frame's `instance`, the name every
            // verb keyed on an instance uses.
            "CreateInstance" => Request::CreateInstance(
                arg("engine")?,
                arg("instance")?,
                arg("server_profile")?,
                None,
            ),
            "CreateInstanceAs" => Request::CreateInstance(
                arg("engine")?,
                arg("instance")?,
                arg("server_profile")?,
                principal().map(Some)?,
            ),
            // Its removal.
            "RemoveInstance" => Request::RemoveInstance(arg("instance")?, None),
            "RemoveInstanceAs" => Request::RemoveInstance(arg("instance")?, principal().map(Some)?),
            // The list, like `Instances`, with no `As` form: it is answered to
            // any caller.
            "ServerProfiles" => Request::ServerProfiles,
            // The device list: guarded, because the manager runs the engine to
            // read it. It takes no argument.
            "Devices" => Request::Devices(None),
            "DevicesAs" => Request::Devices(principal().map(Some)?),
            // The layer facts of one saved model profile: guarded as reading
            // `models.ini` is, because the manager reads the model file.
            "ModelInfo" => Request::ModelInfo(arg("profile")?, None),
            "ModelInfoAs" => Request::ModelInfo(arg("profile")?, principal().map(Some)?),
            // The node's own info: guarded as `Devices` is, because the read
            // may spawn the Vulkan child. It takes no argument.
            "NodeInfo" => Request::NodeInfo(None),
            "NodeInfoAs" => Request::NodeInfo(principal().map(Some)?),
            // The models folder list: guarded as `Devices` is, because the
            // manager, not the caller, reads the folder. It takes no argument.
            "ModelFiles" => Request::ModelFiles(None),
            "ModelFilesAs" => Request::ModelFiles(principal().map(Some)?),
            "Authenticate" => Request::Authenticate(arg("user")?, arg("password")?),
            _ => return None,
        })
    }
}

/// The requester of a guarded call: the caller, and the principal the
/// request vouches for when it is an `As` method. A caller with no identity
/// and no principal is refused before anybody is asked, as a D-Bus call
/// without a sender is (`dbus::caller`); with a principal, the caller's
/// identity is the core's business (only the bff vouches,
/// `Manager::check_principal`).
fn requester(caller: &Caller, principal: Option<Principal>) -> Result<Requester, ManagerError> {
    if principal.is_none() && caller.0.is_empty() {
        Err(ManagerError::NotAuthorized)
    } else {
        Ok(Requester {
            caller: caller.clone(),
            principal,
        })
    }
}

/// The reply to `request`, run through `manager` as `caller`.
fn answer<S, A, R>(manager: &Manager<S, A, R>, caller: &Caller, request: Request) -> Value
where
    S: ServiceControl,
    A: Authority,
    R: Resolver,
{
    let result = match request {
        Request::ListInstances => Ok(json!(manager.list_instances())),
        // One object per instance, the two tag lists kept apart: the web UI
        // shows the profile's first and says which is which. The server
        // profile of a section that names none is the empty string, the same
        // answer the bus gives. The label is JSON `null` when the section has
        // none: the pipe has a null, so the row says "no label" and not "an
        // empty label".
        Request::Instances => Ok(json!(manager
            .instances()
            .iter()
            .map(|i| json!({
                "id": i.id,
                "engine": i.engine,
                "server_profile": i.server_profile.clone().unwrap_or_default(),
                "port": i.port,
                "profile_tags": i.profile_tags,
                "instance_tags": i.instance_tags,
                "label": i.label,
            }))
            .collect::<Vec<_>>())),
        Request::Status(id) => manager.status(&id).map(|s| json!(state_name(s))),
        // The version asks nothing of anybody: every caller gets it, the
        // empty one included.
        Request::Version => Ok(json!({"version": env!("CARGO_PKG_VERSION"), "protocol": PROTOCOL})),
        Request::Start(id, principal) => requester(caller, principal)
            .and_then(|who| manager.start(&who, &id))
            .map(|()| Value::Null),
        Request::Stop(id, principal) => requester(caller, principal)
            .and_then(|who| manager.stop(&who, &id))
            .map(|()| Value::Null),
        Request::Restart(id, principal) => requester(caller, principal)
            .and_then(|who| manager.restart(&who, &id))
            .map(|()| Value::Null),
        Request::ReadFile(name, principal) => manager
            .read_file(requester(caller, principal).ok().as_ref(), &name)
            .map(|file| json!(file.map(|f| json!({"text": f.text, "withheld": f.withheld})))),
        Request::WriteFile(name, contents, keep, principal) => requester(caller, principal)
            .and_then(|who| manager.write_file(&who, &name, &contents, keep))
            .map(|()| Value::Null),
        // The compare and swap: `expected` is the text the caller read, and
        // the core answers `Changed` when the file no longer holds it.
        Request::WriteFileIf(name, contents, expected, principal) => requester(caller, principal)
            .and_then(|who| manager.write_file_if(&who, &name, &contents, &expected))
            .map(|()| Value::Null),
        Request::BootEnabled(id) => manager.boot_enabled(&id).map(|on| json!(on)),
        Request::SetBootEnabled(id, enabled, principal) => requester(caller, principal)
            .and_then(|who| manager.set_boot_enabled(&who, &id, enabled))
            .map(|()| Value::Null),
        // The assignment of one model profile to one instance.
        Request::SetModelProfile(instance, profile, value, principal) => {
            requester(caller, principal)
                .and_then(|who| manager.set_model_profile(&who, &instance, &profile, &value))
                .map(|()| Value::Null)
        }
        // The switch of an instance to another server profile.
        Request::SetServerProfile(instance, profile, principal) => requester(caller, principal)
            .and_then(|who| manager.set_server_profile(&who, &instance, &profile))
            .map(|()| Value::Null),
        // The record's own tags of one instance.
        Request::SetInstanceTags(instance, tags, principal) => requester(caller, principal)
            .and_then(|who| manager.set_instance_tags(&who, &instance, &tags))
            .map(|()| Value::Null),
        // The label of one instance, the text the web UI shows beside its id.
        Request::SetInstanceLabel(instance, label, principal) => requester(caller, principal)
            .and_then(|who| manager.set_instance_label(&who, &instance, &label))
            .map(|()| Value::Null),
        // A new server profile. The port travels as an integer and 0 is no
        // port, so the file the core writes carries no `Port` key.
        Request::CreateServerProfile(engine, profile, port, principal) => {
            requester(caller, principal)
                .and_then(|who| {
                    manager.create_server_profile(
                        &who,
                        &engine,
                        &profile,
                        (port != 0).then_some(port),
                    )
                })
                .map(|()| Value::Null)
        }
        // Its removal.
        Request::DeleteServerProfile(engine, profile, principal) => requester(caller, principal)
            .and_then(|who| manager.delete_server_profile(&who, &engine, &profile))
            .map(|()| Value::Null),
        // A new model profile, born switched off: the id, the profile to copy
        // and the keys of the file pick.
        Request::CreateModelProfile(profile, from, values, principal) => {
            requester(caller, principal)
                .and_then(|who| manager.create_model_profile(&who, &profile, &from, &values))
                .map(|()| Value::Null)
        }
        // Its removal.
        Request::DeleteModelProfile(profile, principal) => requester(caller, principal)
            .and_then(|who| manager.delete_model_profile(&who, &profile))
            .map(|()| Value::Null),
        // A new instance. The reply is empty as every write's: the caller sent
        // the id, and the create answers before the record is written.
        Request::CreateInstance(engine, instance, server_profile, principal) => {
            requester(caller, principal)
                .and_then(|who| manager.create_instance(&who, &engine, &instance, &server_profile))
                .map(|()| Value::Null)
        }
        // Its removal.
        Request::RemoveInstance(instance, principal) => requester(caller, principal)
            .and_then(|who| manager.remove_instance(&who, &instance))
            .map(|()| Value::Null),
        // ── The instance profile verbs (`ARCHITECTURE.md` "Instance
        // profiles"). The answers are the shape the bff reads: one object per
        // profile, the pool's text as `revision` on `GetInstanceProfile`
        // alone (the pool has one text).
        Request::InstanceProfiles(principal) => requester(caller, principal)
            .and_then(|who| manager.instance_profiles(&who))
            .map(|rows| {
                json!(rows
                    .iter()
                    .map(|p| json!({
                        "id": p.id,
                        "server_profile": p.server_profile,
                        "entries": p.entries,
                    }))
                    .collect::<Vec<_>>())
            }),
        Request::GetInstanceProfile(profile, principal) => requester(caller, principal)
            .and_then(|who| manager.instance_profile(&who, &profile))
            .map(|p| {
                json!({
                    "server_profile": p.server_profile,
                    "entries": p.entries,
                    "revision": p.revision,
                })
            }),
        Request::CreateInstanceProfile(profile, server_profile, principal) => {
            requester(caller, principal)
                .and_then(|who| manager.create_instance_profile(&who, &profile, &server_profile))
                .map(|()| Value::Null)
        }
        Request::DeleteInstanceProfile(profile, principal) => requester(caller, principal)
            .and_then(|who| manager.delete_instance_profile(&who, &profile))
            .map(|()| Value::Null),
        Request::AddInstanceProfileModel(profile, model, principal) => requester(caller, principal)
            .and_then(|who| manager.add_instance_profile_model(&who, &profile, &model))
            // The client's verb answers nothing: the stream carries the
            // running instances from slice IP4 on.
            .map(|_| Value::Null),
        Request::RemoveInstanceProfileModel(profile, model, principal) => {
            requester(caller, principal)
                .and_then(|who| manager.remove_instance_profile_model(&who, &profile, &model))
                // The client's verb answers nothing: the stream carries the
                // running instances from slice IP4 on.
                .map(|_| Value::Null)
        }
        Request::SetInstanceProfile(instance, profile, revision, principal) => {
            requester(caller, principal)
                .and_then(|who| manager.set_instance_profile(&who, &instance, &profile, &revision))
                .map(|()| Value::Null)
        }
        Request::SetInstanceAddress(instance, host, port, revision, principal) => {
            requester(caller, principal)
                .and_then(|who| {
                    manager.set_instance_address(&who, &instance, &host, &port, &revision)
                })
                .map(|()| Value::Null)
        }
        // One object per server profile, the instance that runs it the empty
        // string when no instance does: the same answer the bus gives.
        Request::ServerProfiles => Ok(json!(manager
            .server_profiles()
            .iter()
            .map(|p| json!({
                "id": p.id,
                "engine": p.engine,
                "port": p.port,
                "tags": p.tags,
                "instance": p.instance.clone().unwrap_or_default(),
            }))
            .collect::<Vec<_>>())),
        // The node's compute devices, as the probe read them, with the second
        // it ran beside them: 0 is no probe, as on the bus. The rows are the
        // probe's own, in its order, the CPU pseudo-device left out by the
        // core. Beside them the host's inventory (R1): `ram_mib` MiB of RAM and
        // `cpu_threads` logical CPUs, the node's own reading, answered whatever
        // the probe did, 0 for a figure the host did not give.
        Request::Devices(principal) => requester(caller, principal)
            .and_then(|who| manager.devices(&who))
            .map(|list| {
                json!({
                    "probed_at": list.probed_at.unwrap_or_default(),
                    "ram_mib": list.ram_mib,
                    "cpu_threads": list.cpu_threads,
                    "devices": list
                        .devices
                        .iter()
                        .map(|d| json!({
                            "id": d.id,
                            "name": d.name,
                            "backend": d.backend,
                            "total_mib": d.total_mib,
                            "free_mib": d.free_mib,
                        }))
                        .collect::<Vec<_>>(),
                })
            }),
        // The layer facts of one saved model profile, the reply of the design:
        // `model` the path it resolved (null when the section names none),
        // `info` the four counts with the tensor list (`tensors` rows of `name`
        // and `bytes`, `tensors_truncated`), `reason` the keyed why. Exactly one
        // of the two is ever there: an outcome the read could not answer is a
        // success of the call with `info` null, and the reader's own text never
        // travels, so no byte of a file it refused reaches the frame.
        Request::ModelInfo(profile, principal) => requester(caller, principal)
            .and_then(|who| manager.model_info(&who, &profile))
            .map(|reply| {
                json!({
                    "model": reply.model,
                    "info": reply.info.map(|f| json!({
                        "block_count": f.block_count,
                        "moe": f.moe,
                        "expert_count": f.expert_count,
                        "expert_used": f.expert_used,
                        "tensors": f
                            .tensors
                            .iter()
                            .map(|t| json!({"name": t.name, "bytes": t.bytes}))
                            .collect::<Vec<_>>(),
                        "tensors_truncated": f.tensors_truncated,
                    })),
                    "reason": reply
                        .reason
                        .map(|r| json!({"key": r.key, "args": r.args})),
                })
            }),
        // The node's own info, the document of the design: `os` its three
        // texts, `vulkan` the word the child answered, `gpus` one row per GPU
        // with the kernel's driver and the Vulkan one joined onto it. Every
        // text is a string or null, and the serializer writes every key: a
        // fact the host did not answer is `null`, and the row's `vulkan` is
        // the pair or null, never a pair of nothing.
        Request::NodeInfo(principal) => requester(caller, principal)
            .and_then(|who| manager.node_info(&who))
            .map(|reply| {
                json!({
                    "os": {
                        "name": reply.os.name,
                        "version": reply.os.version,
                        "kernel": reply.os.kernel,
                    },
                    "vulkan": pai_core::vulkan::word(reply.vulkan),
                    "gpus": reply
                        .gpus
                        .iter()
                        .map(|g| json!({
                            "pci": g.pci,
                            "vendor": g.vendor,
                            "name": g.name,
                            "driver": g.driver,
                            "version": g.version,
                            "vulkan": g.vulkan.as_ref().map(|d| json!({
                                "driver": d.driver,
                                "info": d.info,
                            })),
                        }))
                        .collect::<Vec<_>>(),
                })
            }),
        // The node's one models folder, the document of the design: `dir` the
        // folder as an absolute path, `files` one row per `.gguf` with its
        // bytes, its shard count and its category (the first subfolder, `null`
        // for a file of the folder itself), `truncated` the walk's bound, and
        // `error` the folder's keyed why. A folder that is missing, is not a
        // folder, or cannot be read is an OK answer with `files` empty and the
        // key there: the picker's empty list with a text, never the call's
        // failure.
        Request::ModelFiles(principal) => requester(caller, principal)
            .and_then(|who| manager.model_files(&who))
            .map(|reply| {
                json!({
                    "dir": reply.dir,
                    "files": reply
                        .files
                        .iter()
                        .map(|f| json!({
                            "path": f.path,
                            "size": f.size,
                            "shards": f.shards,
                            "category": f.category,
                        }))
                        .collect::<Vec<_>>(),
                    "truncated": reply.truncated,
                    "error": reply
                        .error
                        .map(|e| json!({"key": e.key, "args": e.args})),
                })
            }),
        // The empty caller too goes to the core, which answers only the bff.
        Request::Authenticate(user, password) => manager
            .authenticate(caller, &user, &password)
            .map(|(ok, admin)| json!({"authenticated": ok, "can_configure": admin})),
    };
    match result {
        Ok(value) => json!({"ok": value}),
        Err(e) => error_value(error_reply(&e)),
    }
}

fn error_value((name, message): (String, String)) -> Value {
    json!({"error": {"name": name, "message": message}})
}

/// Answer BadRequest, then end the connection with an error.
fn bad_request(stream: &mut impl Write) -> std::io::Result<()> {
    let reply = error_value((BAD_REQUEST.to_string(), "BadRequest".to_string()));
    write_frame(stream, reply.to_string().as_bytes())?;
    Err(Error::new(ErrorKind::InvalidData, BAD_REQUEST))
}

/// Serve one connection: read requests until the stream ends, answer each
/// through `manager` in order, as `caller` (who opened the connection; an
/// empty one is refused its own changes, nobody asked; an `As` request goes
/// to the core, which gates the principal on the caller). Ok when the stream
/// ends between frames; an error when it ends inside one, fails, or carries
/// a frame that is no request (answered with [`BAD_REQUEST`] first, the rest
/// of the stream left unread). Never panics on what it reads.
pub fn serve_stream<S, A, R>(
    manager: &Manager<S, A, R>,
    caller: Caller,
    mut stream: impl Read + Write,
) -> std::io::Result<()>
where
    S: ServiceControl,
    A: Authority,
    R: Resolver,
{
    loop {
        let body = match read_frame(&mut stream) {
            Ok(Some(body)) => body,
            Ok(None) => return Ok(()),
            Err(FrameError::Io(e)) => return Err(e),
            Err(FrameError::TooLong(_)) => return bad_request(&mut stream),
        };
        let Some(request) = Request::parse(&body) else {
            return bad_request(&mut stream);
        };
        let reply = answer(manager, &caller, request);
        write_frame(&mut stream, reply.to_string().as_bytes())?;
    }
}

/// E2E-free tests: nothing here reaches `paths::` (the requests are refused
/// before the core reads a file, or ask only a fake), so they are plain
/// `#[test]`s. The round trips through the files are the e2e phase in
/// `src/tests/pipe_client.rs`.
#[cfg(test)]
mod tests {
    use super::super::service::fakes::{
        Call, FakeAuthority, FakeGroup, FakePasswords, FakeResolver, FakeService,
    };
    use super::super::service::{Action, ServiceError, UnitState};
    use super::*;
    use pai_core::client::pipe::MAX_FRAME;
    use serde_json::{json, Value};
    use std::io::{Cursor, ErrorKind};
    use std::sync::{Arc, Mutex};

    type Core = Manager<FakeService, FakeAuthority, FakeResolver>;

    const PREFIX: &str = "org.privateai.Manager1.Error.";

    fn core_in(state: UnitState, allow: bool) -> Core {
        let mut service = FakeService::new();
        service.state = state;
        Manager::new(service, FakeAuthority::new(allow), FakeResolver::lan())
            .with_group(crate::manager::service::fakes::FakeGroup::new())
    }

    fn core(allow: bool) -> Core {
        core_in(UnitState::Active, allow)
    }

    fn calls(c: &Core) -> Vec<Call> {
        c.service().calls.lock().unwrap().clone()
    }

    fn asked(c: &Core) -> Vec<(Caller, Action)> {
        c.authority().asked.lock().unwrap().clone()
    }

    /// The caller a connection was opened by: an opaque string to the core
    /// (here shaped like a Windows user SID).
    fn alice() -> Caller {
        Caller("S-1-5-21-3623811015-3361044348-30300820-1013".into())
    }

    /// A client's side of a connection: what it sent (`input`, read by the
    /// server) and what the server wrote back; `write_error` makes every
    /// write fail.
    struct Script {
        input: Cursor<Vec<u8>>,
        replies: Arc<Mutex<Vec<u8>>>,
        write_error: Option<ErrorKind>,
    }

    impl Read for Script {
        fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
            self.input.read(buf)
        }
    }

    impl Write for Script {
        fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
            match self.write_error {
                Some(kind) => Err(kind.into()),
                None => {
                    self.replies.lock().unwrap().extend_from_slice(buf);
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

    fn requests(values: &[Value]) -> Vec<u8> {
        values
            .iter()
            .flat_map(|v| frame(v.to_string().as_bytes()))
            .collect()
    }

    /// The frames in `bytes`, each body parsed as JSON; a short or non-JSON
    /// frame fails the test.
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

    /// Serve `input` to its end as `caller`: the result and the replies.
    fn serve(c: &Core, caller: Caller, input: Vec<u8>) -> (std::io::Result<()>, Vec<Value>) {
        let replies = Arc::new(Mutex::new(Vec::new()));
        let script = Script {
            input: Cursor::new(input),
            replies: replies.clone(),
            write_error: None,
        };
        let result = serve_stream(c, caller, script);
        let bytes = replies.lock().unwrap().clone();
        (result, frames(&bytes))
    }

    fn error(kind: &str, message: &str) -> Value {
        json!({"error": {"name": format!("{PREFIX}{kind}"), "message": message}})
    }

    /// The reply is the BadRequest error, with a message.
    fn assert_bad_request(reply: &Value, what: &str) {
        assert_eq!(reply["error"]["name"], BAD_REQUEST, "{what}: {reply}");
        let message = reply["error"]["message"].as_str().unwrap_or_default();
        assert!(!message.is_empty(), "{what}: {reply}");
    }

    // ── Failures first ──────────────────────────────────────────────────

    #[test]
    fn the_bad_request_name_is_under_the_prefix() {
        assert_eq!(BAD_REQUEST, "org.privateai.Manager1.Error.BadRequest");
    }

    /// A client that connects and leaves: nothing answered, nothing asked.
    #[test]
    fn a_stream_that_ends_at_once_is_a_clean_end() {
        let c = core(true);
        let (result, replies) = serve(&c, alice(), Vec::new());
        assert!(result.is_ok(), "{result:?}");
        assert_eq!(replies, Vec::<Value>::new());
        assert_eq!(calls(&c), vec![]);
        assert_eq!(asked(&c), vec![]);
    }

    /// A stream that ends inside a frame: the request never arrived, so it
    /// is neither answered nor run.
    #[test]
    fn a_stream_that_ends_inside_a_frame_is_an_error() {
        let whole = frame(
            json!({"method": "Start", "id": "default"})
                .to_string()
                .as_bytes(),
        );
        for cut in [1, 3, 4, 5, whole.len() - 1] {
            let c = core(true);
            let (result, replies) = serve(&c, alice(), whole[..cut].to_vec());
            assert!(result.is_err(), "cut after {cut} bytes");
            assert_eq!(replies, Vec::<Value>::new(), "cut after {cut} bytes");
            assert_eq!(calls(&c), vec![], "cut after {cut} bytes");
            assert_eq!(asked(&c), vec![], "cut after {cut} bytes");
        }
        // After a whole request, the truncated next one ends the connection
        // and the first is still answered.
        let c = core(true);
        let mut input = requests(&[json!({"method": "Status", "id": "default"})]);
        input.extend_from_slice(&whole[..whole.len() - 1]);
        let (result, replies) = serve(&c, alice(), input);
        assert!(result.is_err());
        assert_eq!(replies, vec![json!({"ok": "active"})]);
        assert_eq!(calls(&c), vec![Call::Status("default".into())]);
    }

    /// A frame that is no request is answered BadRequest and ends the
    /// connection: the requests after it are never read, the core never
    /// reached.
    #[test]
    fn a_malformed_request_is_answered_and_ends_the_connection() {
        let bodies: [&[u8]; 21] = [
            b"",
            b"not json",
            b"{\"method\":",
            b"\xff\xfe{\"method\":\"ListInstances\"}",
            b"[\"Start\",\"default\"]",
            b"\"ListInstances\"",
            b"{\"id\":\"default\"}",
            b"{\"method\":\"Reload\",\"id\":\"default\"}",
            b"{\"method\":\"start\",\"id\":\"default\"}",
            b"{\"method\":\"Start\"}",
            b"{\"method\":\"Start\",\"id\":5}",
            b"{\"method\":\"WriteFile\",\"name\":\"models.ini\"}",
            // `keep_api_key` is required, a bool, as SetBootEnabled's `enabled`.
            b"{\"method\":\"WriteFile\",\"name\":\"models.ini\",\"contents\":\"\"}",
            b"{\"method\":\"WriteFile\",\"name\":\"models.ini\",\"contents\":\"\",\"keep_api_key\":\"no\"}",
            // `WriteFileIf` requires the content the file must still hold:
            // missing, a number, null, is the same BadRequest. A frame with no
            // `expected` is refused, never treated as an empty expectation.
            b"{\"method\":\"WriteFileIf\",\"name\":\"models.ini\",\"contents\":\"x\"}",
            b"{\"method\":\"WriteFileIf\",\"name\":\"models.ini\",\"contents\":\"x\",\"expected\":42}",
            b"{\"method\":\"WriteFileIf\",\"name\":\"models.ini\",\"contents\":\"x\",\"expected\":null}",
            // The name and the new content are arguments of both spellings.
            b"{\"method\":\"WriteFileIf\",\"contents\":\"x\",\"expected\":\"\"}",
            b"{\"method\":\"WriteFileIf\",\"name\":\"models.ini\",\"expected\":\"\"}",
            b"{\"method\":\"WriteFileIfAs\",\"principal\":\"alice\",\"name\":\"models.ini\",\"contents\":\"x\"}",
            b"{\"method\":\"WriteFileIfAs\",\"name\":\"models.ini\",\"contents\":\"x\",\"expected\":\"\"}",
        ];
        for body in bodies {
            let c = core(true);
            let mut input = frame(body);
            input.extend(requests(&[json!({"method": "Start", "id": "default"})]));
            let (result, replies) = serve(&c, alice(), input);
            let what = String::from_utf8_lossy(body).into_owned();
            assert!(result.is_err(), "{what}");
            assert_eq!(replies.len(), 1, "{what}: only the BadRequest: {replies:?}");
            assert_bad_request(&replies[0], &what);
            assert_eq!(calls(&c), vec![], "{what}");
            assert_eq!(asked(&c), vec![], "{what}");
        }
    }

    /// A frame over the cap is refused from its length alone: answered
    /// BadRequest, its body and everything after it left unread.
    #[test]
    fn a_frame_over_the_cap_is_answered_and_ends_the_connection() {
        let start = json!({"method": "Start", "id": "default"}).to_string();
        for len in [MAX_FRAME + 1, u32::MAX] {
            let c = core(true);
            let mut input = len.to_le_bytes().to_vec();
            input.extend_from_slice(start.as_bytes());
            input.extend(requests(&[json!({"method": "Start", "id": "default"})]));
            let (result, replies) = serve(&c, alice(), input);
            assert!(result.is_err(), "length {len}");
            assert_eq!(replies.len(), 1, "length {len}: {replies:?}");
            assert_bad_request(&replies[0], &format!("length {len}"));
            assert_eq!(calls(&c), vec![], "length {len}");
            assert_eq!(asked(&c), vec![], "length {len}");
        }
    }

    /// A frame of exactly the cap is read and answered.
    #[test]
    fn a_frame_at_the_cap_is_served() {
        let shell = json!({"method": "ReadFile", "name": ""}).to_string();
        let name = "x".repeat(MAX_FRAME as usize - shell.len());
        let body = json!({"method": "ReadFile", "name": name}).to_string();
        assert_eq!(body.len(), MAX_FRAME as usize);
        let c = core(true);
        let (result, replies) = serve(&c, alice(), frame(body.as_bytes()));
        assert!(result.is_ok(), "{result:?}");
        assert_eq!(replies, vec![error("BadFileName", "BadFileName")]);
    }

    /// A client gone before its reply: the server stops with an error, no
    /// panic.
    #[test]
    fn a_reply_that_cannot_be_written_ends_the_connection() {
        let c = core(true);
        let script = Script {
            input: Cursor::new(requests(&[
                json!({"method": "Status", "id": "default"}),
                json!({"method": "Stop", "id": "default"}),
            ])),
            replies: Arc::new(Mutex::new(Vec::new())),
            write_error: Some(ErrorKind::BrokenPipe),
        };
        let result = serve_stream(&c, alice(), script);
        assert!(result.is_err());
        assert_eq!(
            calls(&c),
            vec![Call::Status("default".into())],
            "the request after the failed reply is not run"
        );
    }

    /// Every change asks the authority about the connection's caller, per
    /// action; a refused caller changes nothing.
    #[test]
    fn a_refused_caller_is_asked_with_its_own_identity_and_nothing_is_done() {
        let c = core(false);
        let (result, replies) = serve(
            &c,
            alice(),
            requests(&[
                json!({"method": "Start", "id": "default"}),
                json!({"method": "Stop", "id": "default"}),
                json!({"method": "Restart", "id": "default"}),
                json!({"method": "WriteFile", "name": "models.ini", "contents": "[x]\nmodel = /m/x.gguf\n", "keep_api_key": false}),
                // The compare-and-swap write is a guarded call like the plain
                // one: refused for a caller without the right, no path
                // resolved, nothing written.
                json!({"method": "WriteFileIf", "name": "server.ini", "contents": "x", "expected": ""}),
            ]),
        );
        assert!(result.is_ok(), "{result:?}");
        assert_eq!(replies, vec![error("NotAuthorized", "NotAuthorized"); 5]);
        assert_eq!(calls(&c), vec![]);
        assert_eq!(
            asked(&c),
            vec![
                (alice(), Action::Control),
                (alice(), Action::Control),
                (alice(), Action::Control),
                (alice(), Action::Configure),
                (alice(), Action::Configure),
            ]
        );
    }

    /// An instance file can hold a secret (`OpencodeApiKey`): reading one
    /// asks the authority for control as the connection's caller, and a
    /// refused caller gets NotAuthorized without the file being read (here
    /// before any path is resolved: this test never reaches `paths::`), an
    /// instance that does not exist included.
    #[test]
    fn reading_an_instance_file_asks_control_and_a_refused_caller_reads_nothing() {
        let c = core(false);
        let (result, replies) = serve(
            &c,
            alice(),
            requests(&[
                json!({"method": "ReadFile", "name": "server@default.ini"}),
                json!({"method": "ReadFile", "name": "server@nope.ini"}),
            ]),
        );
        assert!(result.is_ok(), "{result:?}");
        assert_eq!(replies, vec![error("NotAuthorized", "NotAuthorized"); 2]);
        assert_eq!(asked(&c), vec![(alice(), Action::Control); 2]);
        assert_eq!(calls(&c), vec![]);
    }

    /// A connection with no identity (the D-Bus call with no sender) is
    /// refused every change before anybody is asked, even by an authority
    /// that would allow it.
    #[test]
    fn an_empty_caller_is_refused_every_change_unasked() {
        let c = core(true);
        let (result, replies) = serve(
            &c,
            Caller(String::new()),
            requests(&[
                json!({"method": "Start", "id": "default"}),
                json!({"method": "Stop", "id": "default"}),
                json!({"method": "Restart", "id": "default"}),
                json!({"method": "WriteFile", "name": "models.ini", "contents": "", "keep_api_key": false}),
                json!({"method": "WriteFileIf", "name": "models.ini", "contents": "", "expected": ""}),
                json!({"method": "ReadFile", "name": "server@default.ini"}),
                json!({"method": "Status", "id": "default"}),
            ]),
        );
        assert!(result.is_ok(), "{result:?}");
        let refused = error("NotAuthorized", "NotAuthorized");
        assert_eq!(
            replies,
            vec![
                refused.clone(),
                refused.clone(),
                refused.clone(),
                refused.clone(),
                refused.clone(),
                refused,
                json!({"ok": "active"}),
            ],
            "an instance file's read is refused like a change; status needs no caller"
        );
        assert_eq!(asked(&c), vec![], "nobody asked");
        assert_eq!(calls(&c), vec![Call::Status("default".into())]);
    }

    // ── Start at boot ───────────────────────────────────────────────────
    // Refused before the core resolves a path: plain tests. The round trips
    // are in `src/tests/pipe_client.rs`.

    /// `SetBootEnabled` asks the authority for configure as the
    /// connection's caller; a refused caller changes nothing.
    #[test]
    fn the_start_at_boot_change_asks_configure_and_a_refused_caller_changes_nothing() {
        let c = core(false);
        let (result, replies) = serve(
            &c,
            alice(),
            requests(&[
                json!({"method": "SetBootEnabled", "id": "default", "enabled": true}),
                json!({"method": "SetBootEnabled", "id": "default", "enabled": false}),
            ]),
        );
        assert!(result.is_ok(), "{result:?}");
        assert_eq!(replies, vec![error("NotAuthorized", "NotAuthorized"); 2]);
        assert_eq!(asked(&c), vec![(alice(), Action::Configure); 2]);
        assert_eq!(calls(&c), vec![]);
    }

    /// A connection with no identity is refused the change unasked; the
    /// read needs no caller (here an invalid id, refused as one).
    #[test]
    fn an_empty_caller_is_refused_the_start_at_boot_change_unasked() {
        let c = core(true);
        let (result, replies) = serve(
            &c,
            Caller(String::new()),
            requests(&[
                json!({"method": "SetBootEnabled", "id": "default", "enabled": true}),
                json!({"method": "BootEnabled", "id": "../x"}),
            ]),
        );
        assert!(result.is_ok(), "{result:?}");
        assert_eq!(
            replies,
            vec![
                error("NotAuthorized", "NotAuthorized"),
                error("InvalidId", "InvalidId"),
            ]
        );
        assert_eq!(asked(&c), vec![]);
        assert_eq!(calls(&c), vec![]);
    }

    /// An invalid id is refused either way, the change after its
    /// authorization.
    #[test]
    fn the_start_at_boot_of_an_invalid_id_is_refused() {
        let c = core(true);
        let (result, replies) = serve(
            &c,
            alice(),
            requests(&[
                json!({"method": "BootEnabled", "id": "Default"}),
                json!({"method": "SetBootEnabled", "id": "a_b", "enabled": true}),
            ]),
        );
        assert!(result.is_ok(), "{result:?}");
        assert_eq!(replies, vec![error("InvalidId", "InvalidId"); 2]);
        assert_eq!(asked(&c), vec![(alice(), Action::Configure)]);
        assert_eq!(calls(&c), vec![]);
    }

    /// `enabled` is a JSON boolean and required; `id` too.
    #[test]
    fn a_malformed_start_at_boot_request_is_a_bad_request() {
        for body in [
            json!({"method": "BootEnabled"}),
            json!({"method": "BootEnabled", "id": 5}),
            json!({"method": "SetBootEnabled", "id": "default"}),
            json!({"method": "SetBootEnabled", "id": "default", "enabled": "yes"}),
            json!({"method": "SetBootEnabled", "id": "default", "enabled": 1}),
            json!({"method": "SetBootEnabled", "enabled": true}),
        ] {
            let c = core(true);
            let (result, replies) = serve(&c, alice(), requests(std::slice::from_ref(&body)));
            assert!(result.is_err(), "{body}");
            assert_eq!(replies.len(), 1, "{body}: {replies:?}");
            assert_bad_request(&replies[0], &body.to_string());
            assert_eq!(asked(&c), vec![], "{body}");
            assert_eq!(calls(&c), vec![], "{body}");
        }
    }

    // ── The wire ────────────────────────────────────────────────────────

    /// `SetServerProfile` asks the authority for configure as the
    /// connection's caller; a refused caller changes nothing. The instance id
    /// is refused by the core's own existence check, so nothing here reaches a
    /// file.
    #[test]
    fn the_server_profile_change_asks_configure_and_a_refused_caller_changes_nothing() {
        let c = core(false);
        let (result, replies) = serve(
            &c,
            alice(),
            requests(&[
                json!({"method": "SetServerProfile", "instance": "default", "profile": "big"}),
                json!({"method": "SetServerProfile", "instance": "default", "profile": "big"}),
            ]),
        );
        assert!(result.is_ok(), "{result:?}");
        assert_eq!(replies, vec![error("NotAuthorized", "NotAuthorized"); 2]);
        assert_eq!(asked(&c), vec![(alice(), Action::Configure); 2]);
        assert_eq!(calls(&c), vec![]);
    }

    /// `SetServerProfile` of an id that is no instance: refused after its
    /// authorization, nothing done. Only the ids the core refuses before it
    /// reads a record are used here: this unit test does not redirect the data
    /// root (see `tests::mod`'s warning).
    #[test]
    fn the_server_profile_of_an_invalid_id_is_refused() {
        let c = core(true);
        let (result, replies) = serve(
            &c,
            alice(),
            requests(&[
                json!({"method": "SetServerProfile", "instance": "a_b", "profile": "big"}),
                json!({"method": "SetServerProfile", "instance": "Default", "profile": "big"}),
                json!({"method": "SetServerProfile", "instance": "", "profile": "big"}),
            ]),
        );
        assert!(result.is_ok(), "{result:?}");
        assert_eq!(replies, vec![error("InvalidId", "InvalidId"); 3]);
        assert_eq!(asked(&c), vec![(alice(), Action::Configure); 3]);
        assert_eq!(calls(&c), vec![]);
    }

    /// `SetInstanceTags` asks the authority for configure as the connection's
    /// caller; a refused caller changes nothing. Every id here is one the core
    /// refuses before it reads a record, so nothing reaches `paths::` (this
    /// unit test does not redirect the data root).
    #[test]
    fn the_tags_change_asks_configure_and_a_refused_caller_changes_nothing() {
        let c = core(false);
        let (result, replies) = serve(
            &c,
            alice(),
            requests(&[
                json!({"method": "SetInstanceTags", "instance": "a_b", "tags": ["edge"]}),
                json!({"method": "SetInstanceTags", "instance": "default", "tags": ["edge", "v2"]}),
            ]),
        );
        assert!(result.is_ok(), "{result:?}");
        assert_eq!(
            replies,
            vec![
                error("NotAuthorized", "NotAuthorized"),
                error("NotAuthorized", "NotAuthorized")
            ]
        );
        assert_eq!(asked(&c), vec![(alice(), Action::Configure); 2]);
        assert_eq!(calls(&c), vec![]);
    }

    /// The refusals of the verb, over the wire: an id that is no id, answered
    /// first, then a list the rule refuses, its message the encoded refusal the
    /// client decodes. Nothing is done, and no path is read.
    #[test]
    fn the_tags_of_an_invalid_id_or_a_refused_list_are_answered() {
        let refusal = |list: &[&str]| -> String {
            pai_core::checks::tags_rule(&list.iter().map(|t| t.to_string()).collect::<Vec<_>>())
                .expect_err("the rule refuses this list")
                .encode()
        };
        let c = core(true);
        let (result, replies) = serve(
            &c,
            alice(),
            requests(&[
                json!({"method": "SetInstanceTags", "instance": "a_b", "tags": ["edge"]}),
                json!({"method": "SetInstanceTags", "instance": "", "tags": ["edge"]}),
                json!({"method": "SetInstanceTags", "instance": "default", "tags": ["Edge"]}),
                json!({"method": "SetInstanceTags", "instance": "default", "tags": ["ok", "ok"]}),
            ]),
        );
        assert!(result.is_ok(), "{result:?}");
        assert_eq!(
            replies,
            vec![
                error("InvalidId", "InvalidId"),
                error("InvalidId", "InvalidId"),
                error("Invalid", &refusal(&["Edge"])),
                error("Invalid", &refusal(&["ok", "ok"])),
            ],
            "the id first, then the list, the rule's answer encoded as every Invalid reply is"
        );
        assert_eq!(asked(&c), vec![(alice(), Action::Configure); 4]);
        assert_eq!(calls(&c), vec![]);
    }

    /// `instance` is a required string and `tags` a required array of strings:
    /// a frame missing one, carrying one of the wrong type, naming an entry
    /// that is not a string, a principal that disagrees with the `As`
    /// spelling, or a method spelled otherwise, is no request.
    #[test]
    fn a_malformed_tags_request_is_a_bad_request() {
        for body in [
            json!({"method": "SetInstanceTags", "tags": ["edge"]}),
            json!({"method": "SetInstanceTags", "instance": "default"}),
            json!({"method": "SetInstanceTags", "instance": 5, "tags": ["edge"]}),
            json!({"method": "SetInstanceTags", "instance": "default", "tags": "edge"}),
            json!({"method": "SetInstanceTags", "instance": "default", "tags": null}),
            json!({"method": "SetInstanceTags", "instance": "default", "tags": ["edge", 5]}),
            json!({"method": "SetInstanceTags", "instance": "default", "tags": [{"t": "edge"}]}),
            json!({"method": "SetInstanceTags", "instance": "default", "tags": [null]}),
            json!({"method": "SetInstanceTagsAs", "principal": "alice", "instance": "default"}),
            json!({"method": "SetInstanceTags", "principal": "alice", "instance": "default", "tags": ["edge"]}),
            json!({"method": "setInstanceTags", "instance": "default", "tags": ["edge"]}),
            json!({"method": "SetInstanceTag", "instance": "default", "tags": ["edge"]}),
        ] {
            let c = core(true);
            let (result, replies) = serve(&c, alice(), requests(std::slice::from_ref(&body)));
            assert!(result.is_err(), "{body}");
            assert_eq!(replies.len(), 1, "{body}: {replies:?}");
            assert_bad_request(&replies[0], &body.to_string());
            assert_eq!(asked(&c), vec![], "{body}");
            assert_eq!(calls(&c), vec![], "{body}");
        }
    }

    /// The `As` form of the tags verb carries the principal and runs for the
    /// bff's account alone: a member's call reaches the core's own checks, a
    /// principal outside `pai` is refused, the membership asked for the name
    /// sent, polkit never asked for a principal.
    #[test]
    fn the_tags_as_form_acts_for_the_principal() {
        let (c, admins) = principal_core(FakePasswords::new());
        let (result, replies) = serve(
            &c,
            bff(),
            requests(&[
                json!({"method": "SetInstanceTagsAs", "principal": "alice", "instance": "../x", "tags": ["edge"]}),
                json!({"method": "SetInstanceTagsAs", "principal": "bob", "instance": "../x", "tags": ["edge"]}),
            ]),
        );
        assert!(result.is_ok(), "{result:?}");
        assert_eq!(
            replies,
            vec![
                error("InvalidId", "InvalidId"),
                error("NotAuthorized", "NotAuthorized")
            ],
            "a member reaches the core's own checks, a caller outside `pai` is refused"
        );
        assert_eq!(
            asked_admin(&admins),
            vec!["alice".to_string(), "bob".to_string()],
            "the membership of the name the request vouches for, once per call"
        );
        assert_eq!(
            asked(&c),
            vec![],
            "polkit asked, so the call ran as the bff's own, not for the principal"
        );
        assert_eq!(asked_bff(&c), vec![bff(); 2]);
        assert_eq!(calls(&c), vec![]);
    }

    /// `SetInstanceLabel` asks the authority for configure as the connection's
    /// caller, like the tags verb; a refused caller changes nothing. Every id
    /// here is one the core refuses before it reads a record, so nothing reaches
    /// `paths::` (this unit test does not redirect the data root).
    #[test]
    fn the_label_change_asks_configure_and_a_refused_caller_changes_nothing() {
        let c = core(false);
        let (result, replies) = serve(
            &c,
            alice(),
            requests(&[
                json!({"method": "SetInstanceLabel", "instance": "a_b", "label": "Coding box"}),
                json!({"method": "SetInstanceLabel", "instance": "default", "label": "Coding box"}),
            ]),
        );
        assert!(result.is_ok(), "{result:?}");
        assert_eq!(
            replies,
            vec![
                error("NotAuthorized", "NotAuthorized"),
                error("NotAuthorized", "NotAuthorized")
            ]
        );
        assert_eq!(asked(&c), vec![(alice(), Action::Configure); 2]);
        assert_eq!(calls(&c), vec![]);
    }

    /// The refusals of the label verb, over the wire: an id that is no id,
    /// answered first, then a text the rule refuses, its message the encoded
    /// refusal the client decodes. The rule is answered before the record is
    /// read, so a refused text touches no path, and nothing is done.
    #[test]
    fn the_label_of_an_invalid_id_or_a_refused_text_is_answered() {
        let refusal = |text: &str| -> String {
            pai_core::checks::instance_label(text)
                .expect_err("the rule refuses this text")
                .encode()
        };
        let c = core(true);
        let long = "z".repeat(pai_core::checks::LABEL_MAX + 1);
        let (result, replies) = serve(
            &c,
            alice(),
            requests(&[
                json!({"method": "SetInstanceLabel", "instance": "a_b", "label": "Coding box"}),
                json!({"method": "SetInstanceLabel", "instance": "", "label": "Coding box"}),
                json!({"method": "SetInstanceLabel", "instance": "default", "label": &long}),
                json!({"method": "SetInstanceLabel", "instance": "default", "label": "two\nlines"}),
                json!({"method": "SetInstanceLabel", "instance": "default", "label": "a;b"}),
                json!({"method": "SetInstanceLabel", "instance": "default", "label": "Box #1"}),
            ]),
        );
        assert!(result.is_ok(), "{result:?}");
        assert_eq!(
            replies,
            vec![
                error("InvalidId", "InvalidId"),
                error("InvalidId", "InvalidId"),
                error("Invalid", &refusal(&long)),
                error("Invalid", &refusal("two\nlines")),
                error("Invalid", &refusal("a;b")),
                error("Invalid", &refusal("Box #1")),
            ],
            "the id first, then the text, the rule's answer encoded as every Invalid reply is"
        );
        assert_eq!(asked(&c), vec![(alice(), Action::Configure); 6]);
        assert_eq!(calls(&c), vec![]);
    }

    /// `instance` is a required string and `label` a required string, the empty
    /// one included (it is the clear): a frame missing one, carrying one of the
    /// wrong type, a principal that disagrees with the `As` spelling, or a
    /// method spelled otherwise, is no request.
    #[test]
    fn a_malformed_label_request_is_a_bad_request() {
        for body in [
            json!({"method": "SetInstanceLabel", "label": "Coding box"}),
            json!({"method": "SetInstanceLabel", "instance": "default"}),
            json!({"method": "SetInstanceLabel", "instance": 5, "label": "Coding box"}),
            json!({"method": "SetInstanceLabel", "instance": "default", "label": 5}),
            json!({"method": "SetInstanceLabel", "instance": "default", "label": null}),
            json!({"method": "SetInstanceLabel", "instance": "default", "label": ["Coding box"]}),
            json!({"method": "SetInstanceLabel", "instance": "default", "label": {"l": "box"}}),
            json!({"method": "SetInstanceLabelAs", "principal": "alice", "instance": "default"}),
            json!({"method": "SetInstanceLabel", "principal": "alice", "instance": "default", "label": "Coding box"}),
            json!({"method": "setInstanceLabel", "instance": "default", "label": "Coding box"}),
            json!({"method": "SetInstanceLabel", "instance": "default", "labels": "Coding box"}),
        ] {
            let c = core(true);
            let (result, replies) = serve(&c, alice(), requests(std::slice::from_ref(&body)));
            assert!(result.is_err(), "{body}");
            assert_eq!(replies.len(), 1, "{body}: {replies:?}");
            assert_bad_request(&replies[0], &body.to_string());
            assert_eq!(asked(&c), vec![], "{body}");
            assert_eq!(calls(&c), vec![], "{body}");
        }
    }

    /// The `As` form of the label verb carries the principal and runs for the
    /// bff's account alone: a member's call reaches the core's own checks, a
    /// principal outside `pai` is refused, the membership asked for the name
    /// sent, polkit never asked for a principal.
    #[test]
    fn the_label_as_form_acts_for_the_principal() {
        let (c, admins) = principal_core(FakePasswords::new());
        let (result, replies) = serve(
            &c,
            bff(),
            requests(&[
                json!({"method": "SetInstanceLabelAs", "principal": "alice", "instance": "../x", "label": "Coding box"}),
                json!({"method": "SetInstanceLabelAs", "principal": "bob", "instance": "../x", "label": "Coding box"}),
            ]),
        );
        assert!(result.is_ok(), "{result:?}");
        assert_eq!(
            replies,
            vec![
                error("InvalidId", "InvalidId"),
                error("NotAuthorized", "NotAuthorized")
            ],
            "a member reaches the core's own checks, a caller outside `pai` is refused"
        );
        assert_eq!(
            asked_admin(&admins),
            vec!["alice".to_string(), "bob".to_string()],
            "the membership of the name the request vouches for, once per call"
        );
        assert_eq!(
            asked(&c),
            vec![],
            "polkit asked, so the call ran as the bff's own, not for the principal"
        );
        assert_eq!(asked_bff(&c), vec![bff(); 2]);
        assert_eq!(calls(&c), vec![]);
    }

    /// The `As` form carries the principal and runs for the bff's account
    /// alone: a member's call reaches the core's checks, a principal outside
    /// `pai` is refused, the membership asked for the name sent, polkit never
    /// asked for a principal.
    #[test]
    fn the_server_profile_as_form_acts_for_the_principal() {
        let (c, admins) = principal_core(FakePasswords::new());
        let (result, replies) = serve(
            &c,
            bff(),
            requests(&[
                json!({"method": "SetServerProfileAs", "principal": "alice", "instance": "../x", "profile": "big"}),
                json!({"method": "SetServerProfileAs", "principal": "bob", "instance": "../x", "profile": "big"}),
            ]),
        );
        assert!(result.is_ok(), "{result:?}");
        assert_eq!(
            replies,
            vec![
                error("InvalidId", "InvalidId"),
                error("NotAuthorized", "NotAuthorized")
            ],
            "a member reaches the core's own checks, a caller outside `pai` is refused"
        );
        assert_eq!(
            asked_admin(&admins),
            vec!["alice".to_string(), "bob".to_string()],
            "the membership of the name the request vouches for, once per call"
        );
        assert_eq!(
            asked(&c),
            vec![],
            "polkit asked, so the call ran as the bff's own, not for the principal"
        );
        assert_eq!(asked_bff(&c), vec![bff(); 2]);
        assert_eq!(calls(&c), vec![]);
    }

    /// `instance` and `profile` are both required strings, and the name's
    /// spelling is exactly the method's: a frame missing one, carrying one
    /// that is not a string, or spelled otherwise, is no request.
    #[test]
    fn a_malformed_server_profile_request_is_a_bad_request() {
        for body in [
            json!({"method": "SetServerProfile", "instance": "default"}),
            json!({"method": "SetServerProfile", "profile": "big"}),
            json!({"method": "SetServerProfile", "instance": 5, "profile": "big"}),
            json!({"method": "SetServerProfile", "instance": "default", "profile": null}),
            json!({"method": "SetServerProfile", "instance": "default", "profile": ["big"]}),
            json!({"method": "SetServerProfileAs", "principal": "alice", "instance": "default"}),
            json!({"method": "setServerProfile", "instance": "default", "profile": "big"}),
            json!({"method": "SetServerProfiles", "instance": "default", "profile": "big"}),
        ] {
            let c = core(true);
            let (result, replies) = serve(&c, alice(), requests(std::slice::from_ref(&body)));
            assert!(result.is_err(), "{body}");
            assert_eq!(replies.len(), 1, "{body}: {replies:?}");
            assert_bad_request(&replies[0], &body.to_string());
            assert_eq!(asked(&c), vec![], "{body}");
            assert_eq!(calls(&c), vec![], "{body}");
        }
    }

    /// A rule's refusal as the wire carries it: the key, then its values.
    fn refusal(key: &'static str, values: &[&str]) -> String {
        pai_core::checks::Refusal {
            key,
            values: values.iter().map(|v| v.to_string()).collect(),
        }
        .encode()
    }

    /// Creating and deleting a server profile ask `configure` as every write of
    /// a served file does; a caller the authority refuses is answered
    /// NotAuthorized with nothing done. The names here are all ones the core
    /// refuses before it resolves a path, so nothing reaches `paths::` (this
    /// unit test does not redirect the data root).
    #[test]
    fn the_server_profile_create_and_delete_ask_configure_and_a_refused_caller_changes_nothing() {
        let c = core(false);
        let (result, replies) = serve(
            &c,
            alice(),
            requests(&[
                json!({"method": "CreateServerProfile", "engine": "llama.cpp", "profile": "big", "port": 8123}),
                json!({"method": "DeleteServerProfile", "engine": "llama.cpp", "profile": "big"}),
            ]),
        );
        assert!(result.is_ok(), "{result:?}");
        assert_eq!(
            replies,
            vec![
                error("NotAuthorized", "NotAuthorized"),
                error("NotAuthorized", "NotAuthorized")
            ]
        );
        assert_eq!(asked(&c), vec![(alice(), Action::Configure); 2]);
        assert_eq!(calls(&c), vec![]);
    }

    /// The refusals of the two verbs, over the wire: the engine the manager
    /// does not serve and a name that is no profile, each with its own key
    /// encoded as every `Invalid` reply is; a name with no file, and a bad
    /// name on a delete, answer `NoServerProfile` with the name.
    #[test]
    fn the_server_profile_verbs_answer_the_rules_refusals() {
        let c = core(true);
        let (result, replies) = serve(
            &c,
            alice(),
            requests(&[
                json!({"method": "CreateServerProfile", "engine": "vllm", "profile": "fine", "port": 8123}),
                json!({"method": "CreateServerProfile", "engine": "llama.cpp", "profile": "../x", "port": 8123}),
                json!({"method": "CreateServerProfile", "engine": "llama.cpp", "profile": "X", "port": 8123}),
                json!({"method": "CreateServerProfile", "engine": "llama.cpp", "profile": "a_b", "port": 8123}),
                json!({"method": "CreateServerProfile", "engine": "llama.cpp", "profile": "", "port": 8123}),
                json!({"method": "DeleteServerProfile", "engine": "vllm", "profile": "fine"}),
                json!({"method": "DeleteServerProfile", "engine": "llama.cpp", "profile": "../x"}),
                json!({"method": "DeleteServerProfile", "engine": "llama.cpp", "profile": "nope"}),
            ]),
        );
        assert!(result.is_ok(), "{result:?}");
        assert_eq!(
            replies,
            vec![
                error("Invalid", &refusal("unknown-engine", &["vllm"])),
                error("Invalid", &refusal("server-profile-bad-id", &["../x"])),
                error("Invalid", &refusal("server-profile-bad-id", &["X"])),
                error("Invalid", &refusal("server-profile-bad-id", &["a_b"])),
                error("Invalid", &refusal("server-profile-bad-id", &[""])),
                error("Invalid", &refusal("unknown-engine", &["vllm"])),
                error("NoServerProfile", "../x"),
                error("NoServerProfile", "nope"),
            ],
            "the engine, then the name, both as the check's key; a name with no file \
             is the profile that is not there"
        );
        assert_eq!(asked(&c), vec![(alice(), Action::Configure); 8]);
        assert_eq!(calls(&c), vec![]);
    }

    /// `engine`, `profile` and `port` are all required and of their own types:
    /// `port` is an integer 0 to 65535, so a string, a fraction, a sign, a
    /// number out of range, a missing field, a principal that disagrees with
    /// the `As` spelling, or a method spelled otherwise, is no request.
    #[test]
    fn a_malformed_server_profile_create_request_is_a_bad_request() {
        for body in [
            json!({"method": "CreateServerProfile", "engine": "llama.cpp", "profile": "big"}),
            json!({"method": "CreateServerProfile", "engine": "llama.cpp", "port": 8123}),
            json!({"method": "CreateServerProfile", "profile": "big", "port": 8123}),
            json!({"method": "CreateServerProfile", "engine": "llama.cpp", "profile": "big", "port": "8123"}),
            json!({"method": "CreateServerProfile", "engine": "llama.cpp", "profile": "big", "port": null}),
            json!({"method": "CreateServerProfile", "engine": "llama.cpp", "profile": "big", "port": 8123.5}),
            json!({"method": "CreateServerProfile", "engine": "llama.cpp", "profile": "big", "port": true}),
            json!({"method": "CreateServerProfile", "engine": "llama.cpp", "profile": "big", "port": []}),
            json!({"method": "CreateServerProfile", "engine": "llama.cpp", "profile": "big", "port": 65536}),
            json!({"method": "CreateServerProfile", "engine": "llama.cpp", "profile": "big", "port": -1}),
            json!({"method": "CreateServerProfile", "engine": "llama.cpp", "profile": "big", "port": 4294967296i64}),
            json!({"method": "CreateServerProfile", "engine": 5, "profile": "big", "port": 8123}),
            json!({"method": "CreateServerProfile", "engine": "llama.cpp", "profile": 5, "port": 8123}),
            json!({"method": "CreateServerProfileAs", "principal": "alice", "engine": "llama.cpp", "profile": "big"}),
            json!({"method": "CreateServerProfileAs", "engine": "llama.cpp", "profile": "big", "port": 8123}),
            json!({"method": "CreateServerProfile", "principal": "alice", "engine": "llama.cpp", "profile": "big", "port": 8123}),
            json!({"method": "createServerProfile", "engine": "llama.cpp", "profile": "big", "port": 8123}),
            json!({"method": "CreateServerProfiles", "engine": "llama.cpp", "profile": "big", "port": 8123}),
            json!({"method": "DeleteServerProfile", "engine": "llama.cpp"}),
            json!({"method": "DeleteServerProfile", "profile": "nope"}),
            json!({"method": "DeleteServerProfile", "engine": "llama.cpp", "profile": 5}),
            json!({"method": "DeleteServerProfileAs", "principal": "alice", "engine": "llama.cpp"}),
            json!({"method": "DeleteServerProfile", "engine": "llama.cpp", "profile": "nope", "principal": "alice"}),
            json!({"method": "DeleteServerProfileAs", "engine": "llama.cpp", "profile": "nope"}),
            json!({"method": "ServerProfilesAs", "principal": "alice"}),
        ] {
            let c = core(true);
            let (result, replies) = serve(&c, alice(), requests(std::slice::from_ref(&body)));
            assert!(result.is_err(), "{body}");
            assert_eq!(replies.len(), 1, "{body}: {replies:?}");
            assert_bad_request(&replies[0], &body.to_string());
            assert_eq!(asked(&c), vec![], "{body}");
            assert_eq!(calls(&c), vec![], "{body}");
        }
    }

    /// The edges of the port are requests, not refusals of the frame: 0 (no
    /// port) and 65535 reach the core, which here answers the name it was
    /// given, before any path.
    #[test]
    fn the_ports_edges_are_a_request() {
        let c = core(true);
        let (result, replies) = serve(
            &c,
            alice(),
            requests(&[
                json!({"method": "CreateServerProfile", "engine": "llama.cpp", "profile": "X", "port": 0}),
                json!({"method": "CreateServerProfile", "engine": "llama.cpp", "profile": "X", "port": 65535}),
            ]),
        );
        assert!(result.is_ok(), "{result:?}");
        assert_eq!(
            replies,
            vec![error("Invalid", &refusal("server-profile-bad-id", &["X"])); 2],
            "not BadRequest: the frame is well formed, the name is the core's to refuse"
        );
        assert_eq!(asked(&c), vec![(alice(), Action::Configure); 2]);
    }

    /// The `As` forms carry the principal and run for the bff's account alone:
    /// a member's call reaches the core's own checks, a principal outside `pai`
    /// is refused, the membership asked for the name sent, polkit never asked.
    #[test]
    fn the_server_profile_crud_as_forms_act_for_the_principal() {
        let (c, admins) = principal_core(FakePasswords::new());
        let (result, replies) = serve(
            &c,
            bff(),
            requests(&[
                json!({"method": "CreateServerProfileAs", "principal": "alice", "engine": "llama.cpp", "profile": "../x", "port": 8123}),
                json!({"method": "DeleteServerProfileAs", "principal": "alice", "engine": "llama.cpp", "profile": "../x"}),
                json!({"method": "CreateServerProfileAs", "principal": "bob", "engine": "llama.cpp", "profile": "../x", "port": 8123}),
                json!({"method": "DeleteServerProfileAs", "principal": "bob", "engine": "llama.cpp", "profile": "../x"}),
            ]),
        );
        assert!(result.is_ok(), "{result:?}");
        assert_eq!(
            replies,
            vec![
                error("Invalid", &refusal("server-profile-bad-id", &["../x"])),
                error("NoServerProfile", "../x"),
                error("NotAuthorized", "NotAuthorized"),
                error("NotAuthorized", "NotAuthorized"),
            ],
            "a member reaches the core's own checks, a caller outside `pai` is refused"
        );
        assert_eq!(
            asked_admin(&admins),
            vec![
                "alice".to_string(),
                "alice".into(),
                "bob".into(),
                "bob".into()
            ],
            "the membership of the name the request vouches for, once per call"
        );
        assert_eq!(
            asked(&c),
            vec![],
            "polkit asked, so the call ran as the bff's own, not for the principal"
        );
        assert_eq!(asked_bff(&c), vec![bff(); 4]);
        assert_eq!(calls(&c), vec![]);
    }

    // ── Creating and removing an instance (slice 16) ──────────────────────
    // Refused before the core resolves a path: plain tests. The round trips are
    // in `src/tests/pipe_client.rs`.

    /// Creating and removing an instance ask `configure` as every write of a
    /// served file does; a caller the authority refuses is answered
    /// NotAuthorized with nothing done, the id of both calls one the rule
    /// refuses, so nothing reaches `paths::` (this unit test does not redirect
    /// the data root).
    #[test]
    fn the_instance_create_and_remove_ask_configure_and_a_refused_caller_changes_nothing() {
        let c = core(false);
        let (result, replies) = serve(
            &c,
            alice(),
            requests(&[
                json!({"method": "CreateInstance", "engine": "llama.cpp", "instance": "../x", "server_profile": "free"}),
                json!({"method": "RemoveInstance", "instance": "../x"}),
            ]),
        );
        assert!(result.is_ok(), "{result:?}");
        assert_eq!(
            replies,
            vec![
                error("NotAuthorized", "NotAuthorized"),
                error("NotAuthorized", "NotAuthorized")
            ]
        );
        assert_eq!(asked(&c), vec![(alice(), Action::Configure); 2]);
        assert_eq!(calls(&c), vec![]);
    }

    /// A connection with no identity is refused both verbs before anybody is
    /// asked, even by an authority that would allow it.
    #[test]
    fn an_empty_caller_is_refused_the_instance_create_and_remove_unasked() {
        let c = core(true);
        let (result, replies) = serve(
            &c,
            Caller(String::new()),
            requests(&[
                json!({"method": "CreateInstance", "engine": "llama.cpp", "instance": "newone", "server_profile": "free"}),
                json!({"method": "RemoveInstance", "instance": "default"}),
            ]),
        );
        assert!(result.is_ok(), "{result:?}");
        let refused = error("NotAuthorized", "NotAuthorized");
        assert_eq!(
            replies,
            vec![refused.clone(), refused],
            "no caller, no call"
        );
        assert_eq!(asked(&c), vec![], "nobody asked");
        assert_eq!(calls(&c), vec![]);
    }

    /// The refusals of the two verbs over the wire: the engine the manager does
    /// not serve and an id the rule refuses, each with its own key encoded as
    /// every `Invalid` reply is. `RemoveInstance` of an id the rule refuses is
    /// the older `InvalidId`, as every verb keyed on an instance id is.
    #[test]
    fn the_instance_verbs_answer_the_rules_refusals() {
        let c = core(true);
        let (result, replies) = serve(
            &c,
            alice(),
            requests(&[
                json!({"method": "CreateInstance", "engine": "vllm", "instance": "newone", "server_profile": "free"}),
                json!({"method": "CreateInstance", "engine": "llama.cpp", "instance": "../x", "server_profile": "free"}),
                json!({"method": "CreateInstance", "engine": "llama.cpp", "instance": "A", "server_profile": "free"}),
                json!({"method": "CreateInstance", "engine": "llama.cpp", "instance": "", "server_profile": "free"}),
                json!({"method": "CreateInstance", "engine": "llama.cpp", "instance": "a_b", "server_profile": "free"}),
                json!({"method": "RemoveInstance", "instance": "../x"}),
            ]),
        );
        assert!(result.is_ok(), "{result:?}");
        assert_eq!(
            replies,
            vec![
                error("Invalid", &refusal("unknown-engine", &["vllm"])),
                error("Invalid", &refusal("instance-bad-id", &["../x"])),
                error("Invalid", &refusal("instance-bad-id", &["A"])),
                error("Invalid", &refusal("instance-bad-id", &[""])),
                error("Invalid", &refusal("instance-bad-id", &["a_b"])),
                error("InvalidId", "InvalidId"),
            ],
            "the engine, then the id, both as the check's key; an id the rule refuses \
             on a remove is the verb's own InvalidId"
        );
        assert_eq!(asked(&c), vec![(alice(), Action::Configure); 6]);
        assert_eq!(calls(&c), vec![]);
    }

    /// `engine`, `instance` and `server_profile` are all required and of their
    /// own types, and `RemoveInstance` requires `instance`: a missing field, a
    /// string where a name belongs, a null, a principal that disagrees with the
    /// `As` spelling, a list verb with an `As` form, or a method spelled
    /// otherwise, is no request.
    #[test]
    fn a_malformed_instance_create_request_is_a_bad_request() {
        for body in [
            json!({"method": "CreateInstance", "instance": "newone", "server_profile": "free"}),
            json!({"method": "CreateInstance", "engine": "llama.cpp", "server_profile": "free"}),
            json!({"method": "CreateInstance", "engine": "llama.cpp", "instance": "newone"}),
            json!({"method": "CreateInstance", "engine": 5, "instance": "newone", "server_profile": "free"}),
            json!({"method": "CreateInstance", "engine": "llama.cpp", "instance": 5, "server_profile": "free"}),
            json!({"method": "CreateInstance", "engine": "llama.cpp", "instance": null, "server_profile": "free"}),
            json!({"method": "CreateInstance", "engine": "llama.cpp", "instance": "newone", "server_profile": 5}),
            json!({"method": "CreateInstance", "engine": "llama.cpp", "instance": "newone", "server_profile": null}),
            json!({"method": "CreateInstance", "engine": "llama.cpp", "instance": "newone", "server_profile": ["free"]}),
            json!({"method": "CreateInstanceAs", "principal": "alice", "engine": "llama.cpp", "instance": "newone"}),
            json!({"method": "CreateInstanceAs", "engine": "llama.cpp", "instance": "newone", "server_profile": "free"}),
            json!({"method": "CreateInstance", "principal": "alice", "engine": "llama.cpp", "instance": "newone", "server_profile": "free"}),
            json!({"method": "createInstance", "engine": "llama.cpp", "instance": "newone", "server_profile": "free"}),
            json!({"method": "CreateInstances", "engine": "llama.cpp", "instance": "newone", "server_profile": "free"}),
            json!({"method": "RemoveInstance"}),
            json!({"method": "RemoveInstance", "instance": 5}),
            json!({"method": "RemoveInstance", "instance": null}),
            json!({"method": "RemoveInstanceAs", "principal": "alice"}),
            json!({"method": "RemoveInstance", "instance": "default", "principal": "alice"}),
            json!({"method": "removeInstance", "instance": "default"}),
            json!({"method": "RemoveInstances", "instance": "default"}),
            json!({"method": "InstancesAs", "principal": "alice"}),
        ] {
            let c = core(true);
            let (result, replies) = serve(&c, alice(), requests(std::slice::from_ref(&body)));
            assert!(result.is_err(), "{body}");
            assert_eq!(replies.len(), 1, "{body}: {replies:?}");
            assert_bad_request(&replies[0], &body.to_string());
            assert_eq!(asked(&c), vec![], "{body}");
            assert_eq!(calls(&c), vec![], "{body}");
        }
    }

    /// The `As` forms carry the principal and run for the bff's account alone:
    /// a member's call reaches the core's own checks, a principal outside `pai`
    /// is refused, the membership asked for the name sent, polkit never asked.
    #[test]
    fn the_instance_crud_as_forms_act_for_the_principal() {
        let (c, admins) = principal_core(FakePasswords::new());
        let (result, replies) = serve(
            &c,
            bff(),
            requests(&[
                json!({"method": "CreateInstanceAs", "principal": "alice", "engine": "llama.cpp", "instance": "../x", "server_profile": "free"}),
                json!({"method": "RemoveInstanceAs", "principal": "alice", "instance": "../x"}),
                json!({"method": "CreateInstanceAs", "principal": "bob", "engine": "llama.cpp", "instance": "../x", "server_profile": "free"}),
                json!({"method": "RemoveInstanceAs", "principal": "bob", "instance": "../x"}),
            ]),
        );
        assert!(result.is_ok(), "{result:?}");
        assert_eq!(
            replies,
            vec![
                error("Invalid", &refusal("instance-bad-id", &["../x"])),
                error("InvalidId", "InvalidId"),
                error("NotAuthorized", "NotAuthorized"),
                error("NotAuthorized", "NotAuthorized"),
            ],
            "a member reaches the core's own checks, a caller outside `pai` is refused"
        );
        assert_eq!(
            asked_admin(&admins),
            vec![
                "alice".to_string(),
                "alice".into(),
                "bob".into(),
                "bob".into()
            ],
            "the membership of the name the request vouches for, once per call"
        );
        assert_eq!(
            asked(&c),
            vec![],
            "polkit asked, so the call ran as the bff's own, not for the principal"
        );
        assert_eq!(asked_bff(&c), vec![bff(); 4]);
        assert_eq!(calls(&c), vec![]);
    }

    /// The raw replies, as pinned in `pai_core::client::pipe`'s header:
    /// requests on one connection answered in order.
    #[test]
    fn replies_are_the_pinned_frames_in_order() {
        let c = core(true);
        let (result, replies) = serve(
            &c,
            alice(),
            requests(&[
                json!({"method": "Status", "id": "default"}),
                json!({"method": "Stop", "id": "default"}),
                json!({"method": "Status", "id": "../x"}),
                json!({"method": "ReadFile", "name": "/etc/passwd"}),
                json!({"method": "WriteFile", "name": "server.ini", "contents": "x", "keep_api_key": false}),
            ]),
        );
        assert!(result.is_ok(), "{result:?}");
        assert_eq!(
            replies,
            vec![
                json!({"ok": "active"}),
                json!({"ok": null}),
                error("InvalidId", "InvalidId"),
                error("BadFileName", "BadFileName"),
                error("BadFileName", "BadFileName"),
            ]
        );
        assert_eq!(
            calls(&c),
            vec![Call::Status("default".into()), Call::Stop("default".into())]
        );
        assert_eq!(
            asked(&c),
            vec![(alice(), Action::Control), (alice(), Action::Configure)]
        );
    }

    #[test]
    fn status_names_every_state_as_dbus_does() {
        for (state, name) in [
            (UnitState::Active, "active"),
            (UnitState::Inactive, "inactive"),
            (UnitState::Failed, "failed"),
            (UnitState::Activating, "activating"),
            (UnitState::Deactivating, "deactivating"),
        ] {
            let c = core_in(state, true);
            let (result, replies) = serve(
                &c,
                alice(),
                requests(&[json!({"method": "Status", "id": "default"})]),
            );
            assert!(result.is_ok(), "{state:?}: {result:?}");
            assert_eq!(replies, vec![json!({"ok": name})], "{state:?}");
        }
    }

    /// The service manager's failure comes back with its own message.
    #[test]
    fn a_service_failure_carries_its_message() {
        let mut service = FakeService::new();
        service.failing = vec!["default".into()];
        let c = Manager::new(service, FakeAuthority::new(true), FakeResolver::lan())
            .with_group(crate::manager::service::fakes::FakeGroup::new());
        let (result, replies) = serve(
            &c,
            alice(),
            requests(&[
                json!({"method": "Stop", "id": "default"}),
                json!({"method": "Status", "id": "default"}),
            ]),
        );
        assert!(result.is_ok(), "{result:?}");
        assert_eq!(
            replies,
            vec![
                error("Service", "llama.cpp@default.service failed"),
                error("Service", "llama.cpp@default.service failed"),
            ]
        );
    }

    fn every_error() -> Vec<(ManagerError, &'static str)> {
        vec![
            (ManagerError::NotAuthorized, "NotAuthorized"),
            (ManagerError::InvalidId, "InvalidId"),
            (ManagerError::NoInstance, "NoInstance"),
            (ManagerError::BadFileName, "BadFileName"),
            (ManagerError::AddressInUse("default".into()), "AddressInUse"),
            (
                ManagerError::UnresolvableHost("gpu-box.lan".into()),
                "UnresolvableHost",
            ),
            (
                ManagerError::Service(ServiceError(
                    "Unit llama.cpp@default.service not found.".into(),
                )),
                "Service",
            ),
            (ManagerError::Io(ErrorKind::PermissionDenied), "Io"),
            (
                ManagerError::Invalid(pai_core::checks::Refusal {
                    key: "invalid-model-profile-id",
                    values: vec!["Qwen 3 (v2) é".into()],
                }),
                "Invalid",
            ),
            (ManagerError::Changed, "Changed"),
            (
                ManagerError::NoServerProfile("nope café".into()),
                "NoServerProfile",
            ),
            (
                ManagerError::ServerProfileInUse("lan 2 (café)".into(), "lan-2".into()),
                "ServerProfileInUse",
            ),
            (
                ManagerError::NoInstanceProfile("nope café".into()),
                "NoInstanceProfile",
            ),
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
        ]
    }

    /// Each error has its name under the prefix and a message the client's
    /// `from_reply` reads back to the same error.
    #[test]
    fn every_error_has_its_name_and_reads_back() {
        use pai_core::client::ClientError;
        for (e, kind) in every_error() {
            let (name, message) = error_reply(&e);
            assert_eq!(name, format!("{PREFIX}{kind}"), "{e:?}");
            let expected = match &e {
                ManagerError::NotAuthorized => ClientError::NotAuthorized,
                ManagerError::InvalidId => ClientError::InvalidId,
                ManagerError::NoInstance => ClientError::NoInstance,
                ManagerError::BadFileName => ClientError::BadFileName,
                ManagerError::AddressInUse(id) => ClientError::AddressInUse(id.clone()),
                ManagerError::NoServerProfile(n) => ClientError::NoServerProfile(n.clone()),
                ManagerError::NoInstanceProfile(n) => ClientError::NoInstanceProfile(n.clone()),
                ManagerError::ServerProfileInUse(n, id) => {
                    ClientError::ServerProfileInUse(n.clone(), id.clone())
                }
                ManagerError::ServerProfileExists(n) => ClientError::ServerProfileExists(n.clone()),
                ManagerError::InstanceProfileExists(n) => {
                    ClientError::InstanceProfileExists(n.clone())
                }
                ManagerError::InstanceProfileInUse(n, id) => {
                    ClientError::InstanceProfileInUse(n.clone(), id.clone())
                }
                ManagerError::UnresolvableHost(h) => ClientError::UnresolvableHost(h.clone()),
                ManagerError::Service(s) => ClientError::Service(s.0.clone()),
                ManagerError::Io(k) => ClientError::Io(k.to_string()),
                ManagerError::Invalid(r) => ClientError::Invalid(r.clone()),
                ManagerError::Changed => ClientError::Changed,
            };
            assert_eq!(ClientError::from_reply(&name, &message), expected, "{e:?}");
        }
    }

    /// The very pair the D-Bus interface replies with, so both transports
    /// map identically.
    #[cfg(target_os = "linux")]
    #[test]
    fn every_error_is_the_dbus_pair() {
        for (e, _) in every_error() {
            assert_eq!(
                error_reply(&e),
                super::super::dbus::error_reply(&e),
                "{e:?}"
            );
        }
    }

    // ── Authenticate ────────────────────────────────────────────────────

    /// The bff's account on Windows, `NT SERVICE\private-ai-bff`, by its
    /// service SID (the one `windows::PIPE_SDDL` grants).
    fn bff() -> Caller {
        Caller("S-1-5-80-4088784262-1422927641-1363593751-33154763-2531830098".into())
    }

    /// A core taking [`bff`] for the bff's account over the fake accounts
    /// (alice an administrator, bob not), and its checker's log.
    fn auth_core(
        passwords: super::super::service::fakes::FakePasswords,
    ) -> (Core, crate::manager::service::fakes::Checked) {
        let checked = passwords.checked.clone();
        let core = Manager::new(
            FakeService::new(),
            FakeAuthority::new(true).bff(&bff()),
            FakeResolver::lan(),
        )
        .with_group(crate::manager::service::fakes::FakeGroup::new())
        .with_passwords(passwords);
        (core, checked)
    }

    fn auth(user: &str, password: &str) -> Value {
        json!({"method": "Authenticate", "user": user, "password": password})
    }

    /// `{"method":"Authenticate","user","password"}` from the bff:
    /// `{"ok":{"authenticated":bool,"can_configure":bool}}`, in the
    /// snake_case of `keep_api_key`.
    #[test]
    fn the_bff_authenticates_over_the_pipe() {
        let (c, checked) = auth_core(super::super::service::fakes::FakePasswords::new());
        let (result, replies) = serve(
            &c,
            bff(),
            requests(&[
                auth("alice", "s3cret-Pw"),
                auth("bob", "hunter2!"),
                auth("alice", "wrong"),
                auth("mallory", "s3cret-Pw"),
                auth("", "s3cret-Pw"),
                auth("alice", ""),
            ]),
        );
        assert!(result.is_ok(), "{result:?}");
        let answer =
            |ok: bool, admin: bool| json!({"ok": {"authenticated": ok, "can_configure": admin}});
        assert_eq!(
            replies,
            vec![
                answer(true, true),
                answer(true, false),
                answer(false, false),
                answer(false, false),
                answer(false, false),
                answer(false, false),
            ]
        );
        assert_eq!(checked.lock().unwrap().len(), 4, "empty ones never checked");
        assert_eq!(calls(&c), vec![]);
        assert_eq!(asked(&c), vec![], "polkit-like rights are not asked");
    }

    /// Any other caller, a Windows user or an administrator, or none:
    /// NotAuthorized, no password checked, the connection kept.
    #[test]
    fn only_the_bff_may_authenticate_over_the_pipe() {
        for caller in [alice(), Caller("S-1-5-18".into()), Caller(String::new())] {
            let (c, checked) = auth_core(super::super::service::fakes::FakePasswords::new());
            let (result, replies) = serve(
                &c,
                caller.clone(),
                requests(&[
                    auth("alice", "s3cret-Pw"),
                    json!({"method": "Status", "id": "default"}),
                ]),
            );
            assert!(result.is_ok(), "{caller:?}: {result:?}");
            assert_eq!(
                replies,
                vec![
                    error("NotAuthorized", "NotAuthorized"),
                    json!({"ok": "active"})
                ],
                "{caller:?}"
            );
            assert_eq!(*checked.lock().unwrap(), vec![], "{caller:?}");
        }
    }

    /// A request missing its user or password, or with one not a string,
    /// is BadRequest: the connection ends and no password is checked. The
    /// reply never echoes the password.
    #[test]
    fn a_malformed_authenticate_is_a_bad_request() {
        for request in [
            json!({"method": "Authenticate", "user": "alice"}),
            json!({"method": "Authenticate", "password": "s3cret-Pw"}),
            json!({"method": "Authenticate"}),
            json!({"method": "Authenticate", "user": "alice", "password": 7}),
            json!({"method": "Authenticate", "user": null, "password": "s3cret-Pw"}),
            json!({"method": "Authenticate", "user": ["alice"], "password": "s3cret-Pw"}),
        ] {
            let (c, checked) = auth_core(super::super::service::fakes::FakePasswords::new());
            let (result, replies) = serve(&c, bff(), requests(std::slice::from_ref(&request)));
            assert!(result.is_err(), "{request}");
            assert_eq!(replies.len(), 1, "{request}");
            assert_bad_request(&replies[0], &request.to_string());
            assert!(!replies[0].to_string().contains("s3cret-Pw"), "{request}");
            assert_eq!(*checked.lock().unwrap(), vec![], "{request}");
        }
    }

    /// A failing checker: the Service error with its message, never the
    /// password, and the connection kept.
    #[test]
    fn a_failing_checker_answers_service_over_the_pipe() {
        let failure = "LogonUserW: ERROR_NO_LOGON_SERVERS (1311)";
        let (c, _) = auth_core(super::super::service::fakes::FakePasswords::failing(
            failure,
        ));
        let (result, replies) = serve(&c, bff(), requests(&[auth("alice", "s3cret-Pw")]));
        assert!(result.is_ok(), "{result:?}");
        assert_eq!(replies, vec![error("Service", failure)]);
        assert!(!replies[0].to_string().contains("s3cret-Pw"));
    }

    // ── Acting for a signed-in user: the `As` methods ───────────────────
    // Every call here is refused before a path is resolved, or asks a fake
    // only, so these never reach `paths::`.

    /// The accounts of [`principal_core`], with the log of the membership
    /// questions its calls ask.
    type Admins = Arc<Mutex<Vec<String>>>;

    /// A core taking [`bff`] for the bff's account over `passwords` (alice a
    /// member of `pai`, bob not) and polkit that would allow the caller
    /// everything, plus the log of the membership questions.
    fn principal_core(passwords: FakePasswords) -> (Core, Admins) {
        let admins = passwords.asked_admin.clone();
        let core = Manager::new(
            FakeService::new(),
            FakeAuthority::new(true).bff(&bff()),
            FakeResolver::lan(),
        )
        .with_group(FakeGroup::new())
        .with_passwords(passwords);
        (core, admins)
    }

    fn asked_admin(admins: &Admins) -> Vec<String> {
        admins.lock().unwrap().clone()
    }

    fn asked_bff(c: &Core) -> Vec<Caller> {
        c.authority().asked_bff.lock().unwrap().clone()
    }

    /// An `As` method from the bff reaches the core as a request for that
    /// principal: the membership of the name is asked once per call, polkit
    /// is not (the right is the pair), and the action runs.
    #[test]
    fn an_as_method_from_the_bff_reaches_the_core_with_the_principal() {
        let (c, admins) = principal_core(FakePasswords::new());
        let (result, replies) = serve(
            &c,
            bff(),
            requests(&[
                json!({"method": "StopAs", "principal": "alice", "id": "default"}),
                json!({"method": "SetBootEnabledAs", "principal": "alice", "id": "../x", "enabled": true}),
                json!({"method": "ReadFileAs", "principal": "alice", "name": "server.ini"}),
                json!({"method": "WriteFileAs", "principal": "alice", "name": "server.ini", "contents": "x", "keep_api_key": false}),
                json!({"method": "WriteFileIfAs", "principal": "alice", "name": "server.ini", "contents": "x", "expected": ""}),
                json!({"method": "SetServerProfileAs", "principal": "alice", "instance": "../x", "profile": "big"}),
                json!({"method": "CreateServerProfileAs", "principal": "alice", "engine": "llama.cpp", "profile": "../x", "port": 8123}),
                json!({"method": "DeleteServerProfileAs", "principal": "alice", "engine": "llama.cpp", "profile": "../x"}),
            ]),
        );
        assert!(result.is_ok(), "{result:?}");
        assert_eq!(
            replies,
            vec![
                json!({"ok": null}),
                error("InvalidId", "InvalidId"),
                error("BadFileName", "BadFileName"),
                error("BadFileName", "BadFileName"),
                error("BadFileName", "BadFileName"),
                error("InvalidId", "InvalidId"),
                error("Invalid", &refusal("server-profile-bad-id", &["../x"])),
                error("NoServerProfile", "../x"),
            ]
        );
        assert_eq!(calls(&c), vec![Call::Stop("default".into())]);
        assert_eq!(
            asked_admin(&admins),
            vec!["alice".to_string(); 8],
            "the membership of the name the request vouches for, once per call"
        );
        assert_eq!(
            asked(&c),
            vec![],
            "polkit asked, so the call ran as the bff's own, not for the principal"
        );
        assert_eq!(asked_bff(&c), vec![bff(); 8]);
    }

    /// A caller that is not the bff naming a principal: NotAuthorized for
    /// every `As` method, the membership never looked up, nothing asked and
    /// nothing done, the connection kept. Refused, never run as the caller's
    /// own (the caller polkit would allow everything here).
    #[test]
    fn an_as_method_from_a_caller_that_is_not_the_bff_is_refused_unasked() {
        for caller in [alice(), Caller("S-1-5-18".into()), Caller(String::new())] {
            let (c, admins) = principal_core(FakePasswords::new());
            let mut expected = vec![error("NotAuthorized", "NotAuthorized"); 10];
            expected.push(json!({"ok": "active"}));
            let (result, replies) = serve(
                &c,
                caller.clone(),
                requests(&[
                    json!({"method": "StartAs", "principal": "alice", "id": "default"}),
                    json!({"method": "StopAs", "principal": "alice", "id": "default"}),
                    json!({"method": "RestartAs", "principal": "alice", "id": "default"}),
                    json!({"method": "ReadFileAs", "principal": "alice", "name": "server@default.ini"}),
                    json!({"method": "WriteFileAs", "principal": "alice", "name": "models.ini", "contents": "", "keep_api_key": false}),
                    json!({"method": "WriteFileIfAs", "principal": "alice", "name": "models.ini", "contents": "", "expected": ""}),
                    json!({"method": "SetBootEnabledAs", "principal": "alice", "id": "default", "enabled": true}),
                    json!({"method": "SetServerProfileAs", "principal": "alice", "instance": "default", "profile": "big"}),
                    json!({"method": "CreateServerProfileAs", "principal": "alice", "engine": "llama.cpp", "profile": "big", "port": 8123}),
                    json!({"method": "DeleteServerProfileAs", "principal": "alice", "engine": "llama.cpp", "profile": "big"}),
                    json!({"method": "Status", "id": "default"}),
                ]),
            );
            assert!(result.is_ok(), "{caller:?}: {result:?}");
            assert_eq!(replies, expected, "{caller:?}");
            assert_eq!(
                asked_admin(&admins),
                Vec::<String>::new(),
                "{caller:?}: a membership looked up for a caller that is not the bff"
            );
            assert_eq!(
                asked(&c),
                vec![],
                "{caller:?}: polkit asked, so the call ran as the caller's own"
            );
            assert_eq!(
                calls(&c),
                vec![Call::Status("default".into())],
                "{caller:?}: something beside the Status probe reached the service manager"
            );
            // A connection with no identity is refused as any other; whether
            // the face asks `is_bff` for it, as it does for Authenticate, is
            // its own choice, so only a named caller's log is pinned.
            if !caller.0.is_empty() {
                assert_eq!(
                    asked_bff(&c),
                    vec![caller.clone(); 10],
                    "{caller:?}: the caller is checked first, once per call"
                );
            }
        }
    }

    /// The bff naming an account that is no member of `pai` (bob, an ordinary
    /// account) or is unknown: NotAuthorized, the membership asked for that
    /// name, nothing done.
    #[test]
    fn an_as_method_for_a_principal_who_is_not_a_member_is_refused() {
        for user in ["bob", "mallory"] {
            let (c, admins) = principal_core(FakePasswords::new());
            let (result, replies) = serve(
                &c,
                bff(),
                requests(&[
                    json!({"method": "StopAs", "principal": user, "id": "default"}),
                    json!({"method": "SetBootEnabledAs", "principal": user, "id": "../x", "enabled": true}),
                    json!({"method": "WriteFileIfAs", "principal": user, "name": "server.ini", "contents": "x", "expected": ""}),
                    json!({"method": "CreateServerProfileAs", "principal": user, "engine": "llama.cpp", "profile": "big", "port": 8123}),
                    json!({"method": "DeleteServerProfileAs", "principal": user, "engine": "llama.cpp", "profile": "big"}),
                ]),
            );
            assert!(result.is_ok(), "{user}: {result:?}");
            assert_eq!(
                replies,
                vec![error("NotAuthorized", "NotAuthorized"); 5],
                "{user}"
            );
            assert_eq!(
                asked_admin(&admins),
                vec![user.to_string(); 5],
                "{user}: the membership asked for the name sent"
            );
            assert_eq!(asked(&c), vec![], "{user}: polkit asked");
            assert_eq!(
                calls(&c),
                vec![],
                "{user}: something reached the service manager"
            );
        }
    }

    /// An `As` method that carries no principal is no request: the connection
    /// ends with BadRequest, the core never reached. A principal has exactly
    /// one spelling, and it is required.
    #[test]
    fn an_as_method_without_a_principal_is_a_bad_request() {
        for request in [
            json!({"method": "StartAs", "id": "default"}),
            json!({"method": "StopAs", "id": "default"}),
            json!({"method": "RestartAs", "id": "default"}),
            json!({"method": "ReadFileAs", "name": "models.ini"}),
            json!({"method": "WriteFileAs", "name": "models.ini", "contents": "", "keep_api_key": false}),
            json!({"method": "WriteFileIfAs", "name": "models.ini", "contents": "", "expected": ""}),
            json!({"method": "SetBootEnabledAs", "id": "default", "enabled": true}),
            json!({"method": "StartAs", "principal": 5, "id": "default"}),
            json!({"method": "WriteFileIfAs", "principal": 5, "name": "models.ini", "contents": "", "expected": ""}),
            json!({"method": "StartAs", "principal": null, "id": "default"}),
            json!({"method": "StartAs", "principal": ["alice"], "id": "default"}),
            json!({"method": "StartAs", "principal": {"name": "alice"}, "id": "default"}),
            // The `As` methods take the same arguments as the plain ones.
            json!({"method": "StartAs", "principal": "alice"}),
            json!({"method": "ReadFileAs", "principal": "alice"}),
            json!({"method": "WriteFileAs", "principal": "alice", "name": "models.ini"}),
            json!({"method": "WriteFileIfAs", "principal": "alice", "name": "models.ini", "contents": ""}),
            json!({"method": "WriteFileIfAs", "principal": "alice", "name": "models.ini", "expected": ""}),
            json!({"method": "SetBootEnabledAs", "principal": "alice", "id": "default"}),
            json!({"method": "SetBootEnabledAs", "principal": "alice", "id": "default", "enabled": "yes"}),
            json!({"method": "SetServerProfileAs", "instance": "default", "profile": "big"}),
            json!({"method": "SetServerProfileAs", "principal": "alice", "instance": "default"}),
            json!({"method": "SetServerProfileAs", "principal": 5, "instance": "default", "profile": "big"}),
            json!({"method": "SetServerProfileAs", "principal": null, "instance": "default", "profile": "big"}),
            json!({"method": "CreateServerProfileAs", "principal": "alice", "engine": "llama.cpp", "profile": "big"}),
            json!({"method": "CreateServerProfileAs", "engine": "llama.cpp", "profile": "big", "port": 8123}),
            json!({"method": "CreateServerProfileAs", "principal": 5, "engine": "llama.cpp", "profile": "big", "port": 8123}),
            json!({"method": "CreateServerProfileAs", "principal": null, "engine": "llama.cpp", "profile": "big", "port": 8123}),
            json!({"method": "DeleteServerProfileAs", "principal": "alice", "engine": "llama.cpp"}),
            json!({"method": "DeleteServerProfileAs", "engine": "llama.cpp", "profile": "big"}),
            json!({"method": "DeleteServerProfileAs", "principal": 5, "engine": "llama.cpp", "profile": "big"}),
            // The name says it carries a principal: the plain spelling is
            // unknown with one, and the `As` spelling is unknown without.
            json!({"method": "startAs", "principal": "alice", "id": "default"}),
        ] {
            let (c, admins) = principal_core(FakePasswords::new());
            let (result, replies) = serve(&c, bff(), requests(std::slice::from_ref(&request)));
            assert!(result.is_err(), "{request}");
            assert_eq!(replies.len(), 1, "{request}: {replies:?}");
            assert_bad_request(&replies[0], &request.to_string());
            assert_eq!(asked(&c), vec![], "{request}");
            assert_eq!(asked_admin(&admins), Vec::<String>::new(), "{request}");
            assert_eq!(
                asked_bff(&c),
                Vec::<Caller>::new(),
                "{request}: the core was reached"
            );
            assert_eq!(calls(&c), vec![], "{request}");
        }
    }

    /// A `principal` field on a method that takes none is no request: the
    /// principal travels only on an `As` method, so a client that puts it
    /// elsewhere is refused, never run as the caller's own call.
    #[test]
    fn a_principal_on_a_method_that_takes_none_is_a_bad_request() {
        for request in [
            json!({"method": "ListInstances", "principal": "alice"}),
            json!({"method": "Status", "id": "default", "principal": "alice"}),
            json!({"method": "Start", "id": "default", "principal": "alice"}),
            json!({"method": "Stop", "id": "default", "principal": "alice"}),
            json!({"method": "Restart", "id": "default", "principal": "alice"}),
            json!({"method": "ReadFile", "name": "models.ini", "principal": "alice"}),
            json!({"method": "WriteFile", "name": "models.ini", "contents": "", "keep_api_key": false, "principal": "alice"}),
            json!({"method": "WriteFileIf", "name": "models.ini", "contents": "", "expected": "", "principal": "alice"}),
            json!({"method": "BootEnabled", "id": "default", "principal": "alice"}),
            json!({"method": "SetBootEnabled", "id": "default", "enabled": true, "principal": "alice"}),
            json!({"method": "SetServerProfile", "instance": "default", "profile": "big", "principal": "alice"}),
            json!({"method": "CreateServerProfile", "engine": "llama.cpp", "profile": "big", "port": 8123, "principal": "alice"}),
            json!({"method": "DeleteServerProfile", "engine": "llama.cpp", "profile": "big", "principal": "alice"}),
            json!({"method": "ServerProfiles", "principal": "alice"}),
            json!({"method": "Authenticate", "user": "alice", "password": "s3cret-Pw", "principal": "alice"}),
            json!({"method": "Version", "principal": "alice"}),
            json!({"method": "Start", "id": "default", "principal": 5}),
            json!({"method": "WriteFileIf", "name": "models.ini", "contents": "", "expected": "", "principal": 5}),
            json!({"method": "Start", "id": "default", "principal": null}),
        ] {
            let (c, admins) = principal_core(FakePasswords::new());
            let (result, replies) = serve(&c, bff(), requests(std::slice::from_ref(&request)));
            assert!(result.is_err(), "{request}");
            assert_eq!(replies.len(), 1, "{request}: {replies:?}");
            assert_bad_request(&replies[0], &request.to_string());
            assert_eq!(asked(&c), vec![], "{request}");
            assert_eq!(asked_admin(&admins), Vec::<String>::new(), "{request}");
            assert_eq!(
                asked_bff(&c),
                Vec::<Caller>::new(),
                "{request}: the core was reached"
            );
            assert_eq!(calls(&c), vec![], "{request}");
        }
    }

    /// The rule is the `principal` field alone: every other field the parser
    /// does not know stays ignored, as it is today, so a client of an older
    /// protocol is not refused for what it sends.
    #[test]
    fn a_field_the_manager_does_not_know_is_still_ignored() {
        let (c, admins) = principal_core(FakePasswords::new());
        let (result, replies) = serve(
            &c,
            alice(),
            requests(&[
                json!({"method": "Status", "id": "default", "Principal": "alice"}),
                json!({"method": "Version", "id": "default"}),
                json!({"method": "BootEnabled", "id": "../x", "nonsense": [1, 2]}),
                // `WriteFileIf` carries no `keep_api_key`: the compare and
                // swap needs no key to preserve, so the field is one the
                // manager does not know and the call runs (the name is the
                // core's to refuse, here before any path is resolved).
                json!({"method": "WriteFileIf", "name": "server.ini", "contents": "x", "expected": "", "keep_api_key": true, "nonsense": [1, 2]}),
            ]),
        );
        assert!(result.is_ok(), "{result:?}");
        assert_eq!(
            replies,
            vec![
                json!({"ok": "active"}),
                json!({"ok": {"version": env!("CARGO_PKG_VERSION"), "protocol": 3}}),
                error("InvalidId", "InvalidId"),
                error("BadFileName", "BadFileName"),
            ]
        );
        assert_eq!(calls(&c), vec![Call::Status("default".into())]);
        assert_eq!(asked_admin(&admins), Vec::<String>::new());
    }

    /// An empty `principal` is a principal: the face sends it, and the core
    /// refuses the name. BadRequest is for a request the manager cannot read,
    /// not for a name it refuses, so the connection is kept.
    #[test]
    fn an_empty_principal_reaches_the_core_and_is_refused_by_the_name() {
        let (c, admins) = principal_core(FakePasswords::new());
        let (result, replies) = serve(
            &c,
            bff(),
            requests(&[
                json!({"method": "StopAs", "principal": "", "id": "default"}),
                json!({"method": "WriteFileIfAs", "principal": "", "name": "server.ini", "contents": "x", "expected": ""}),
                json!({"method": "Status", "id": "default"}),
            ]),
        );
        assert!(result.is_ok(), "{result:?}");
        assert_eq!(
            replies,
            vec![
                error("NotAuthorized", "NotAuthorized"),
                error("NotAuthorized", "NotAuthorized"),
                json!({"ok": "active"}),
            ]
        );
        assert_eq!(
            asked_admin(&admins),
            Vec::<String>::new(),
            "the name is refused before any account is looked up"
        );
        assert_eq!(
            calls(&c),
            vec![Call::Status("default".into())],
            "the refused call ran nothing: only the Status probe reached the service manager"
        );
    }

    // ── Version ─────────────────────────────────────────────────────────

    /// `{"method":"Version"}` answers the framework version and the protocol
    /// version, to anyone (the empty caller included), asking no right and
    /// running nothing.
    #[test]
    fn version_answers_the_framework_version_and_the_protocol() {
        for caller in [alice(), bff(), Caller(String::new())] {
            let (c, admins) = principal_core(FakePasswords::new());
            let (result, replies) = serve(
                &c,
                caller.clone(),
                requests(&[
                    json!({"method": "Version"}),
                    json!({"method": "Version", "id": "default"}),
                    json!({"method": "Status", "id": "default"}),
                ]),
            );
            let version = json!({"ok": {"version": env!("CARGO_PKG_VERSION"), "protocol": 3}});
            assert!(result.is_ok(), "{caller:?}: {result:?}");
            assert_eq!(
                replies,
                vec![version.clone(), version, json!({"ok": "active"})],
                "{caller:?}"
            );
            assert_eq!(
                calls(&c),
                vec![Call::Status("default".into())],
                "{caller:?}"
            );
            assert_eq!(
                asked(&c),
                vec![],
                "{caller:?}: a right was asked for the version"
            );
            assert_eq!(asked_bff(&c), Vec::<Caller>::new(), "{caller:?}");
            assert_eq!(asked_admin(&admins), Vec::<String>::new(), "{caller:?}");
        }
    }

    /// The whole protocol at once: pai-core's client, bound to a principal
    /// and asking for the version, against this server over a Unix socket
    /// pair (the named pipe's OS-neutral half, never a real pipe). The client
    /// end has a read deadline: a server that panics or hangs fails the test,
    /// it does not hang.
    #[cfg(unix)]
    #[test]
    fn an_acting_client_round_trips_over_a_socket_pair() {
        use pai_core::client::pipe::PipeClient;
        use pai_core::client::{ClientError, InstanceState, ManagerClient, ManagerVersion};
        use std::os::unix::net::UnixStream;
        use std::time::Duration;

        const DEADLINE: Duration = Duration::from_secs(10);

        let (core, admins) = principal_core(FakePasswords::new());
        let core = Arc::new(core);
        let (server_end, client_end) = UnixStream::pair().expect("socket pair");
        client_end
            .set_read_timeout(Some(DEADLINE))
            .expect("read timeout");
        let served = {
            let core = core.clone();
            let caller = bff();
            std::thread::spawn(move || serve_stream(&core, caller, server_end))
        };
        let client = PipeClient::new(client_end);
        {
            // A block: a bound client that borrows `client` must be gone
            // before the connection is used again.
            let bound = client.acting_for("alice");
            assert_eq!(
                bound.stop("default"),
                Ok(()),
                "the bff stops an instance for a member"
            );
            assert_eq!(
                bound.status("default"),
                Ok(InstanceState::Active),
                "an unguarded method of a bound client"
            );
            // The compare-and-swap write goes over the `As` form for the
            // member: the gate runs before any path is resolved, so a name
            // the core refuses answers BadFileName. The round trip against a
            // data root is in `src/tests/pipe_client.rs`.
            assert_eq!(
                bound.write_file_if("server.ini", "x", ""),
                Err(ClientError::BadFileName),
                "the compare-and-swap write of a bound client, for the member"
            );
        }
        assert_eq!(
            client.version(),
            Ok(Some(ManagerVersion {
                version: env!("CARGO_PKG_VERSION").to_string(),
                protocol: 3,
            })),
            "the version this manager answers"
        );
        assert_eq!(
            calls(&core),
            vec![Call::Stop("default".into()), Call::Status("default".into())],
            "the bound stop ran once for the member, the status probe recorded its own call, nothing else"
        );
        assert_eq!(
            asked_admin(&admins),
            vec!["alice".to_string(); 2],
            "the membership of the name, once per bound call"
        );
        assert_eq!(
            asked(&core),
            vec![],
            "polkit asked, so the call ran as the bff's own"
        );
        drop(client);
        let served = served.join().expect("the server thread panicked");
        assert!(served.is_ok(), "{served:?}");
    }

    // ── The model profile verbs (slice MP1) ───────────────────────────────

    /// Creating and deleting a model profile ask `configure` as every write of a
    /// served file does; a caller the authority refuses is answered NotAuthorized
    /// with nothing done. The `As` forms name a principal: a caller that is not
    /// the bff's account is answered NotAuthorized by the principal gate, before
    /// the action's action is asked. The ids and the values here are all ones the
    /// core refuses before it resolves a path, so nothing reaches `paths::`
    /// (this unit test does not redirect the data root).
    #[test]
    fn the_model_profile_create_and_delete_ask_configure_and_a_refused_caller_changes_nothing() {
        let c = core(false);
        let (result, replies) = serve(
            &c,
            alice(),
            requests(&[
                json!({"method": "CreateModelProfile", "profile": "new-mp", "values": {"model": "/m/new.gguf"}}),
                json!({"method": "DeleteModelProfile", "profile": "old-mp"}),
            ]),
        );
        assert!(result.is_ok(), "{result:?}");
        assert_eq!(
            replies,
            vec![
                error("NotAuthorized", "NotAuthorized"),
                error("NotAuthorized", "NotAuthorized")
            ]
        );
        assert_eq!(asked(&c), vec![(alice(), Action::Configure); 2]);
        assert_eq!(calls(&c), vec![]);

        // The `As` forms, from a caller that is not the bff: the principal gate
        // decides, and the action is never asked.
        let c = core(true);
        let (result, replies) = serve(
            &c,
            alice(),
            requests(&[
                json!({"method": "CreateModelProfileAs", "principal": "alice", "profile": "new-mp", "values": {"model": "/m/new.gguf"}}),
                json!({"method": "DeleteModelProfileAs", "principal": "alice", "profile": "old-mp"}),
            ]),
        );
        assert!(result.is_ok(), "{result:?}");
        assert_eq!(
            replies,
            vec![
                error("NotAuthorized", "NotAuthorized"),
                error("NotAuthorized", "NotAuthorized")
            ],
            "a caller that is not the bff vouches for nobody"
        );
        assert_eq!(
            asked(&c),
            vec![],
            "the principal gate refused, so the action was never asked"
        );
        assert_eq!(calls(&c), vec![]);
    }

    /// The refusals of the two verbs, over the wire, each with its own key
    /// encoded as every `Invalid` reply is: the id rule, the shape of the source,
    /// a value that is no text, a marker in a fetch value, and a key that is no
    /// key of the INI. The source's
    /// shape is judged before the id, so a call with no id of its own and both
    /// sources answers `model-profile-bad-source`; a delete of an id the id rule
    /// refuses answers the pool's `no-model-profile-named`, not
    /// `invalid-model-profile-id`.
    #[test]
    fn the_model_profile_verbs_answer_the_rules_refusals() {
        let c = core(true);
        let (result, replies) = serve(
            &c,
            alice(),
            requests(&[
                // The id rule, and the source's shape first.
                json!({"method": "CreateModelProfile", "profile": "*", "values": {"model": "/m/x.gguf"}}),
                json!({"method": "CreateModelProfile", "profile": "a b", "values": {"model": "/m/x.gguf"}}),
                json!({"method": "CreateModelProfile", "profile": "", "values": {"model": "/m/x.gguf"}}),
                json!({"method": "CreateModelProfile", "profile": "../x", "values": {"model": "/m/x.gguf"}}),
                json!({"method": "CreateModelProfile", "profile": "a[b]", "values": {"model": "/m/x.gguf"}}),
                json!({"method": "CreateModelProfile", "profile": "*", "from": "*", "values": {"model": "/m/x.gguf"}}),
                // The source: both, and neither.
                json!({"method": "CreateModelProfile", "profile": "dup", "from": "src", "values": {"model": "/m/a.gguf"}}),
                json!({"method": "CreateModelProfile", "profile": "dup", "values": {"model": "/m/a.gguf", "hf-repo": "org/model"}}),
                json!({"method": "CreateModelProfile", "profile": "dup", "values": {"hf-file": "m.gguf"}}),
                json!({"method": "CreateModelProfile", "profile": "dup", "values": {"n-gpu-layers": "99"}}),
                // A value that is no text, and a marker in a fetch value.
                json!({"method": "CreateModelProfile", "profile": "dup", "values": {"model": ""}}),
                json!({"method": "CreateModelProfile", "profile": "dup", "values": {"model": "  "}}),
                json!({"method": "CreateModelProfile", "profile": "dup", "values": {"model": "/m/a\nb"}}),
                json!({"method": "CreateModelProfile", "profile": "dup", "values": {"model": "/m/a\rb"}}),
                json!({"method": "CreateModelProfile", "profile": "dup", "values": {"model": "/m/a\0b"}}),
                json!({"method": "CreateModelProfile", "profile": "dup", "values": {"hf-repo": "org/model\u{2003}"}}),
                json!({"method": "CreateModelProfile", "profile": "dup", "values": {"model": "/m/a;b"}}),
                json!({"method": "CreateModelProfile", "profile": "dup", "values": {"hf-file": "m;m.gguf"}}),
                // A key that is no key of the INI, on a clone: the shape lets
                // any key through, so only the key rule keeps the forged line
                // out of the file.
                json!({"method": "CreateModelProfile", "profile": "hk-wire", "from": "src", "values": {"a\n[x]\nmodel": "1"}}),
                // The source the pool does not hold, and no source at all: the
                // pool's own refusal, answered before any file of the tree.
                json!({"method": "CreateModelProfile", "profile": "dup", "from": "*", "values": {}}),
                json!({"method": "CreateModelProfile", "profile": "dup", "from": "*", "values": {"temp": "0.2"}}),
                // The delete: no id of its own is the pool's refusal.
                json!({"method": "DeleteModelProfile", "profile": "*"}),
                json!({"method": "DeleteModelProfile", "profile": "a b"}),
                json!({"method": "DeleteModelProfile", "profile": ""}),
                json!({"method": "DeleteModelProfile", "profile": "../x"}),
            ]),
        );
        assert!(result.is_ok(), "{result:?}");
        assert_eq!(
            replies,
            vec![
                error("Invalid", &refusal("invalid-model-profile-id", &["*"])),
                error("Invalid", &refusal("invalid-model-profile-id", &["a b"])),
                error("Invalid", &refusal("invalid-model-profile-id", &[""])),
                error("Invalid", &refusal("invalid-model-profile-id", &["../x"])),
                error("Invalid", &refusal("invalid-model-profile-id", &["a[b]"])),
                error("Invalid", &refusal("model-profile-bad-source", &["*"])),
                error("Invalid", &refusal("model-profile-bad-source", &["dup"])),
                error("Invalid", &refusal("model-profile-bad-source", &["dup"])),
                error("Invalid", &refusal("model-profile-bad-source", &["dup"])),
                error("Invalid", &refusal("model-profile-bad-source", &["dup"])),
                error("Invalid", &refusal("model-profile-bad-text", &["model"])),
                error("Invalid", &refusal("model-profile-bad-text", &["model"])),
                error("Invalid", &refusal("model-profile-bad-text", &["model"])),
                error("Invalid", &refusal("model-profile-bad-text", &["model"])),
                error("Invalid", &refusal("model-profile-bad-text", &["model"])),
                error("Invalid", &refusal("model-profile-bad-text", &["hf-repo"])),
                error("Invalid", &refusal("comment-markers", &["model"])),
                error("Invalid", &refusal("comment-markers", &["hf-file"])),
                error(
                    "Invalid",
                    &refusal("model-profile-bad-text", &["a\n[x]\nmodel"])
                ),
                error("Invalid", &refusal("no-model-profile-named", &["*"])),
                error("Invalid", &refusal("no-model-profile-named", &["*"])),
                error("Invalid", &refusal("no-model-profile-named", &["*"])),
                error("Invalid", &refusal("no-model-profile-named", &["a b"])),
                error("Invalid", &refusal("no-model-profile-named", &[""])),
                error("Invalid", &refusal("no-model-profile-named", &["../x"])),
            ],
            "the shape and the values first, then the id, then the pool: every key of \
             the design, encoded as every refusal is"
        );
        assert_eq!(asked(&c), vec![(alice(), Action::Configure); 25]);
        assert_eq!(
            calls(&c),
            vec![],
            "no verb of a model profile touches a unit"
        );
    }

    /// `profile` and `values` are required and of their own types, `from` is a
    /// name when it is there, and `values` is an object of strings: a missing
    /// field, a value that is not a string, `values` of another shape, a
    /// principal that disagrees with the `As` spelling, or a method spelled
    /// otherwise, is no request. A field the manager does not know (`port`,
    /// `tags`) stays ignored, as it is for every other verb, so it is not
    /// among these.
    #[test]
    fn a_malformed_model_profile_request_is_a_bad_request() {
        for body in [
            json!({"method": "CreateModelProfile"}),
            json!({"method": "CreateModelProfile", "profile": "x"}),
            json!({"method": "CreateModelProfile", "values": {"model": "/m/x.gguf"}}),
            json!({"method": "CreateModelProfile", "profile": 5, "values": {"model": "/m/x.gguf"}}),
            json!({"method": "CreateModelProfile", "profile": "x", "values": "model=/m/x.gguf"}),
            json!({"method": "CreateModelProfile", "profile": "x", "values": ["model=/m/x.gguf"]}),
            json!({"method": "CreateModelProfile", "profile": "x", "values": [["model", "/m/x.gguf"]]}),
            json!({"method": "CreateModelProfile", "profile": "x", "values": null}),
            json!({"method": "CreateModelProfile", "profile": "x", "values": 5}),
            json!({"method": "CreateModelProfile", "profile": "x", "values": {"model": 5}}),
            json!({"method": "CreateModelProfile", "profile": "x", "values": {"model": null}}),
            json!({"method": "CreateModelProfile", "profile": "x", "values": {"model": true}}),
            json!({"method": "CreateModelProfile", "profile": "x", "values": {"model": ["a"]}}),
            json!({"method": "CreateModelProfile", "profile": "x", "values": {"model": {"p": "/m/x.gguf"}}}),
            json!({"method": "CreateModelProfile", "profile": "x", "from": 5, "values": {"model": "/m/x.gguf"}}),
            json!({"method": "CreateModelProfile", "profile": "x", "from": null, "values": {"model": "/m/x.gguf"}}),
            json!({"method": "CreateModelProfile", "profile": "x", "from": ["a"], "values": {"model": "/m/x.gguf"}}),
            json!({"method": "CreateModelProfileAs", "profile": "x", "values": {"model": "/m/x.gguf"}}),
            json!({"method": "CreateModelProfileAs", "principal": 5, "profile": "x", "values": {"model": "/m/x.gguf"}}),
            json!({"method": "CreateModelProfileAs", "principal": null, "profile": "x", "values": {"model": "/m/x.gguf"}}),
            json!({"method": "CreateModelProfile", "principal": "alice", "profile": "x", "values": {"model": "/m/x.gguf"}}),
            json!({"method": "createModelProfile", "profile": "x", "values": {"model": "/m/x.gguf"}}),
            json!({"method": "CreateModelProfiles", "profile": "x", "values": {"model": "/m/x.gguf"}}),
            json!({"method": "DeleteModelProfile"}),
            json!({"method": "DeleteModelProfile", "profile": 5}),
            json!({"method": "DeleteModelProfile", "profile": null}),
            json!({"method": "DeleteModelProfile", "profile": ["x"]}),
            json!({"method": "DeleteModelProfileAs", "profile": "x"}),
            json!({"method": "DeleteModelProfileAs", "principal": 5, "profile": "x"}),
            json!({"method": "DeleteModelProfile", "principal": "alice", "profile": "x"}),
            json!({"method": "deleteModelProfile", "profile": "x"}),
        ] {
            let c = core(true);
            let (result, replies) = serve(&c, alice(), requests(std::slice::from_ref(&body)));
            // A frame that is no request is answered and ends the connection,
            // as every other verb's malformed frame does.
            assert!(result.is_err(), "{body}");
            assert_eq!(replies.len(), 1, "{body}");
            assert_bad_request(&replies[0], &body.to_string());
            assert_eq!(
                asked(&c),
                vec![],
                "{body}: a frame that is no request asks the authority nothing"
            );
            assert_eq!(calls(&c), vec![], "{body}");
        }
    }
}
