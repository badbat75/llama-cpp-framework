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
    state_name, Affected, Authority, Caller, Manager, ManagerError, NotConverged, Principal,
    Requester, Resolver, ServiceControl,
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
/// one models folder as the model-file picker's list; 3 also carries
/// `SetInstanceProfileModel` with its `As` form, the keys of one entry of an
/// instance profile, and `SetInstanceProfileServer` with its `As` form, the
/// switch of an instance profile to another server profile (IP4a); 4 appends
/// the instance profile and the host to the rows of `Instances` and
/// `ListInstances`, and makes `CreateInstance` take the INSTANCE profile and
/// answer the port it proposed for the new instance (IP4b).
pub const PROTOCOL: u32 = 4;

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
    /// `profile`, `model`, `values`, `revision`: the keys of one entry of an
    /// instance profile, and `values` the entry holds afterwards, in the order
    /// they were sent. The entry belongs to the profile: the save names no
    /// instance (IP4).
    SetInstanceProfileModel(
        String,
        String,
        Vec<(String, String)>,
        String,
        Option<Principal>,
    ),
    /// `profile`, `server_profile`: points an instance profile at another server
    /// profile (IP4a).
    SetInstanceProfileServer(String, String, Option<Principal>),
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
    /// `engine`, `instance`, `instance_profile`: creates an instance, answering
    /// the port the create proposed for it (IP4b).
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
        // The keys of one entry of an instance profile: `values` as an array of
        // `{key, value}` objects, in the order the caller sent them. The entry
        // holds exactly the keys sent, in that order (IP3b), so the frame carries
        // a list, not an object of them: a JSON object has no order to keep. A
        // `values` that is no array, an entry with no `key` or no `value` or a
        // `value` that is no string, is no request.
        let entry = || {
            v.get("values")?
                .as_array()?
                .iter()
                .map(|pair| {
                    Some((
                        pair.get("key")?.as_str()?.to_string(),
                        pair.get("value")?.as_str()?.to_string(),
                    ))
                })
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
            // The entry save: the keys the page sent, as an ordered list of
            // `{key, value}` objects (`values` is required). The entry belongs to
            // the profile, so the save names no instance (IP4).
            "SetInstanceProfileModel" => Request::SetInstanceProfileModel(
                arg("profile")?,
                arg("model")?,
                entry()?,
                arg("revision")?,
                None,
            ),
            "SetInstanceProfileModelAs" => Request::SetInstanceProfileModel(
                arg("profile")?,
                arg("model")?,
                entry()?,
                arg("revision")?,
                principal().map(Some)?,
            ),
            // The switch of an instance profile to another server profile.
            "SetInstanceProfileServer" => {
                Request::SetInstanceProfileServer(arg("profile")?, arg("server_profile")?, None)
            }
            "SetInstanceProfileServerAs" => Request::SetInstanceProfileServer(
                arg("profile")?,
                arg("server_profile")?,
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
            // The create of an instance: the engine, the id and the INSTANCE
            // profile it runs (IP4b), the server profile one step behind it.
            // The id is the frame's `instance`, the name every verb keyed on an
            // instance uses.
            "CreateInstance" => Request::CreateInstance(
                arg("engine")?,
                arg("instance")?,
                arg("instance_profile")?,
                None,
            ),
            "CreateInstanceAs" => Request::CreateInstance(
                arg("engine")?,
                arg("instance")?,
                arg("instance_profile")?,
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

/// One instance of the list, as the pipe answers it: the id, the engine, the
/// server profile and the port, the two tag lists, the label, and the three
/// states of the apply (IP3c, `ARCHITECTURE.md` "Convergence"). `converged`
/// says the instance's rendered pair is the pair its profiles render, `reason`
/// is the keyed check that stopped that pair (`null` for a converged instance),
/// and `pendingRestart` says the pair the instance launched with, as its
/// `applied` record holds it, is not the pair on disk: a compare of the files,
/// asking no unit (IP3c review 4). `ListInstances` and `Instances` give the same
/// rows, so a client of the older list of plain ids reads the id out of each
/// row. The label is
/// JSON `null` when the section has none: the pipe has a null, so the row says
/// "no label" and not "an empty label". The instance profile the section runs
/// and the `Hostname` its section sets are appended to the row (IP4b), each
/// `null` when the section names none, the same spelling the label has.
fn instance_row(i: &pai_core::client::InstanceInfo) -> Value {
    json!({
        "id": i.id,
        "engine": i.engine,
        "server_profile": i.server_profile.clone().unwrap_or_default(),
        "port": i.port,
        "profile_tags": i.profile_tags,
        "instance_tags": i.instance_tags,
        "label": i.label,
        "converged": i.converged,
        "reason": i.reason.as_ref().map(|r| json!({
            "key": r.key,
            "args": r.args,
        })),
        "pendingRestart": i.pending_restart,
        "instance_profile": i.instance_profile,
        "hostname": i.hostname,
    })
}

/// One row of a save's `notConverged`: the instance that kept its last good
/// pair, and the keyed check that stopped it, with that check's arguments. The
/// same shape the row of `Instances` carries its `reason` in, so one page reads
/// the tile and a save's answer the same way.
fn not_converged_row(row: &NotConverged) -> Value {
    json!({
        "instance": row.instance,
        "key": row.key,
        "args": row.args,
    })
}

/// The answer of a save that re-renders, as the stream carries it (IP3c,
/// `ARCHITECTURE.md` "Convergence"): the running instances the save moved to a
/// restart, and the instances that kept their last good pair, each with the
/// check that stopped it.
fn affected_answer(affected: Affected) -> Value {
    json!({
        "running": affected.running,
        "notConverged": affected
            .not_converged
            .iter()
            .map(not_converged_row)
            .collect::<Vec<_>>(),
    })
}

/// The answer of a write of the instance profile pool: the same object every
/// save that re-renders answers (IP3c). The entry add and remove write the pool,
/// which is a source of every instance's render, so the states they leave are
/// the states of the node: [`Manager::affected`] reads them from the files, and
/// asks the units for nothing more.
fn pool_answer<S, A, R>(manager: &Manager<S, A, R>) -> Result<Value, ManagerError>
where
    S: ServiceControl,
    A: Authority,
    R: Resolver,
{
    manager.affected().map(affected_answer)
}

/// The reply to `request`, run through `manager` as `caller`.
fn answer<S, A, R>(manager: &Manager<S, A, R>, caller: &Caller, request: Request) -> Value
where
    S: ServiceControl,
    A: Authority,
    R: Resolver,
{
    let result = match request {
        // One row per instance (`instance_row`): `ListInstances` and `Instances`
        // answer the same rows, so a client of the older list of plain ids reads
        // the id out of each row.
        Request::ListInstances => Ok(json!(manager
            .instances()
            .iter()
            .map(instance_row)
            .collect::<Vec<_>>())),
        // The two tag lists kept apart: the web UI shows the profile's first and
        // says which is which. The server profile of a section that names none is
        // the empty string, the same answer the bus gives.
        Request::Instances => Ok(json!(manager
            .instances()
            .iter()
            .map(instance_row)
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
            .map(affected_answer),
        Request::BootEnabled(id) => manager.boot_enabled(&id).map(|on| json!(on)),
        Request::SetBootEnabled(id, enabled, principal) => requester(caller, principal)
            .and_then(|who| manager.set_boot_enabled(&who, &id, enabled))
            .map(|()| Value::Null),
        // The assignment of one model profile to one instance.
        Request::SetModelProfile(instance, profile, value, principal) => {
            requester(caller, principal)
                .and_then(|who| manager.set_model_profile(&who, &instance, &profile, &value))
                .map(affected_answer)
        }
        // The switch of an instance to another server profile.
        Request::SetServerProfile(instance, profile, principal) => requester(caller, principal)
            .and_then(|who| manager.set_server_profile(&who, &instance, &profile))
            .map(affected_answer),
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
        // Its removal. The delete re-renders, so it answers the apply's rows
        // like every save that re-renders (IP4a).
        Request::DeleteModelProfile(profile, principal) => requester(caller, principal)
            .and_then(|who| manager.delete_model_profile(&who, &profile))
            .map(affected_answer),
        // A new instance. The reply is the port the create proposed for it
        // (IP4b), answered before the record is written.
        Request::CreateInstance(engine, instance, instance_profile, principal) => {
            requester(caller, principal)
                .and_then(|who| {
                    manager.create_instance(&who, &engine, &instance, &instance_profile)
                })
                .map(|port| json!(port))
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
        // An entry of the pool is the render of every instance running the
        // profile, so these two writes answer the apply as every save does
        // (IP3c, `ARCHITECTURE.md` "Convergence").
        Request::AddInstanceProfileModel(profile, model, principal) => requester(caller, principal)
            .and_then(|who| manager.add_instance_profile_model(&who, &profile, &model))
            .and_then(|_| pool_answer(manager)),
        Request::RemoveInstanceProfileModel(profile, model, principal) => {
            requester(caller, principal)
                .and_then(|who| manager.remove_instance_profile_model(&who, &profile, &model))
                .and_then(|_| pool_answer(manager))
        }
        // The keys of one entry: the entry rides the render of every instance
        // running the profile, so the answer is the whole apply, the running
        // instances the write left drifting beside the rows of the node
        // (`Manager::set_instance_profile_model`). The save names no instance
        // (IP4).
        Request::SetInstanceProfileModel(profile, model, values, revision, principal) => {
            requester(caller, principal)
                .and_then(|who| {
                    manager.set_instance_profile_model(&who, &profile, &model, &values, &revision)
                })
                .map(affected_answer)
        }
        // The switch of an instance profile to another server profile: it moves
        // the ground of every entry of the profile, so it answers the apply too,
        // the instances whose card the switch took away among them (IP4a).
        Request::SetInstanceProfileServer(profile, server_profile, principal) => {
            requester(caller, principal)
                .and_then(|who| {
                    manager.set_instance_profile_server(&who, &profile, &server_profile)
                })
                .map(affected_answer)
        }
        Request::SetInstanceProfile(instance, profile, revision, principal) => {
            requester(caller, principal)
                .and_then(|who| manager.set_instance_profile(&who, &instance, &profile, &revision))
                .map(affected_answer)
        }
        Request::SetInstanceAddress(instance, host, port, revision, principal) => {
            requester(caller, principal)
                .and_then(|who| {
                    manager.set_instance_address(&who, &instance, &host, &port, &revision)
                })
                .map(affected_answer)
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

#[cfg(test)]
#[path = "pipe_tests.rs"]
mod tests;
