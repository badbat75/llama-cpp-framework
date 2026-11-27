//! `GET /api/instances/{id}/server`: an instance's server profile file,
//! `server@<server_profile>.ini` (the profile the manager's record names), read
//! through the manager for a signed-in user, with the defaults, limits and
//! interfaces the web UI's Server form shows, and the form's `PUT` body to the
//! whole file's text.

use crate::origin::body_refusal;
use crate::session::{error, json_body, Sessions};
use crate::Connect;
use axum::http::{HeaderMap, StatusCode};
use axum::response::{IntoResponse, Response};
use pai_core::client::{ClientError, InstanceState, ManagerClient, ManagerFile};
use pai_core::server_cfg::{self, ServerConfig};
use pai_core::{checks, net_ifaces, paths};
use serde_json::{json, Map, Value};
use siphasher::sip::SipHasher;
use std::hash::Hasher;
use std::sync::atomic::{AtomicU8, Ordering};
use std::sync::Arc;
use std::time::Duration;

/// The answer to `GET /api/instances/{id}/server`, or `{"key", "args": []}`:
/// 401 with no session (the manager not asked), never the manager's detail.
/// The read is a call for the session's user, named as the principal of the
/// connect from the session alone: a name the request carries is never
/// asked for. The manager re-checks that user's membership of `pai` at every
/// call, so a member removed since the sign-in is refused: 403
/// `session-not-administrator` (the sign-in's key for the same fact) and the
/// session ends with the right that authorized it. The other answers keep the
/// session: they are the bff's news, not a refusal of the user. `key` is the
/// router's revision secret, which only the document's `revision` uses. The
/// file is the record's ([`profile_file`]): the record is asked before any
/// file, and an instance it does not list, or a section naming no profile, is
/// 404 `client-no-instance` with nothing read.
pub(crate) async fn read(
    connect: Connect,
    timeout: Duration,
    sessions: Arc<Sessions>,
    id: String,
    headers: HeaderMap,
    key: [u8; 16],
) -> Response {
    let Some((session, identity)) = sessions.current(&headers) else {
        return error(StatusCode::UNAUTHORIZED, "session-required", &[]);
    };
    let user = identity.user;
    let want = id.clone();
    let ask = tokio::task::spawn_blocking(move || -> Result<_, ClientError> {
        let client = connect(Some(user.as_str()))?;
        let name = profile_file(&*client, &want)?;
        client.read_file(&name)
    });
    let (status, code) = match tokio::time::timeout(timeout, ask).await {
        Ok(Ok(Ok(Some(file)))) => {
            return axum::Json(document(&file, key, Owner::Instance(&id))).into_response()
        }
        Ok(Ok(Ok(None)))
        | Ok(Ok(Err(
            ClientError::NoInstance | ClientError::BadFileName | ClientError::InvalidId,
        ))) => (StatusCode::NOT_FOUND, "client-no-instance"),
        Ok(Ok(Err(ClientError::Unavailable))) | Err(_) => {
            (StatusCode::SERVICE_UNAVAILABLE, "client-unavailable")
        }
        Ok(Ok(Err(ClientError::NotAuthorized))) => {
            sessions.end(&session);
            (StatusCode::FORBIDDEN, "session-not-administrator")
        }
        Ok(Ok(Err(_))) | Ok(Err(_)) => (StatusCode::BAD_GATEWAY, "client-other"),
    };
    error(status, code, &[])
}

/// The manager's name of the instance `id`'s server file: the server profile
/// the record names for it, `server@<server_profile>.ini`. `Instances()` is
/// the only source of the name: the file named after the id is no file the
/// instance reads, so it is never opened. An id the record does not list, and
/// a section naming no profile, are [`ClientError::NoInstance`]; a record that
/// cannot be read is the manager's own news, its error as it is.
fn profile_file(client: &dyn ManagerClient, id: &str) -> Result<String, ClientError> {
    let profile = client
        .instances()?
        .into_iter()
        .find(|row| row.id == id)
        .and_then(|row| row.server_profile)
        .ok_or(ClientError::NoInstance)?;
    Ok(format!("server@{profile}.ini"))
}

/// The width the two thread limits stop at (R1, as amended): the logical CPUs
/// of the machine the web service itself runs on. The document reads ask the
/// manager no `devices`: that call can run the engine's probe under the read's
/// bound, so a page load must never make it. A host that answers nothing is
/// one thread, as everywhere the bff bounds a thread count.
fn own_cpu_count() -> usize {
    std::thread::available_parallelism().map_or(1, |n| n.get())
}

/// The 200 document of the file. `values.OpencodeApiKey` is `null` and the
/// document carries `"withheld": ["OpencodeApiKey"]` exactly when the
/// manager's `withheld` flag is set, even when the text it sent still names
/// a key: every signed-in session is a member of the administrators' group
/// (sign-in refuses anyone else 403), so the bff keeps nothing on its own.
/// `revision` is the keyed hash of the text under `key`. `id` is the instance
/// the request names: the `defaults.StateDir` shown is that instance's folder
/// (the engine's state root joined with the id, as the launch passes it),
/// never the bare root.
///
/// The two thread limits stop at this service's own CPU count
/// ([`own_cpu_count`]): the read asks the manager no `devices` (R1, as amended).
pub(crate) fn document(file: &ManagerFile, key: [u8; 16], owner: Owner<'_>) -> Value {
    let text = file.text.as_str();
    let mut cfg = server_cfg::from_text(text);
    if file.withheld {
        cfg.opencode_api_key = None;
    }
    let cpus = own_cpu_count();
    let range = |key: &str| checks::server_int_range(key).map(|r| json!([r.start(), r.end()]));
    let base_url = ServerConfig {
        opencode_base_url: None,
        ..cfg.clone()
    }
    .opencode_base_url_or_default();
    let interfaces: Vec<Value> = net_ifaces::interfaces()
        .into_iter()
        .map(|i| json!({ "value": i.value, "label": i.label }))
        .collect();
    let state_dir: Value = match &owner {
        // Whichever instance runs the profile owns its state folder: the
        // document shows none for a profile no instance names.
        Owner::Profile(None) => Value::Null,
        Owner::Profile(Some(id)) | Owner::Instance(id) => paths::instance_state_dir()
            .join(id)
            .to_string_lossy()
            .into_owned()
            .into(),
    };
    let mut doc = json!({
        "revision": revision(text, &key),
        "values": values(&cfg),
        "defaults": {
            "Port": server_cfg::DEFAULT_PORT,
            "ModelsMax": server_cfg::DEFAULT_MODELS_MAX,
            "ModelsMemoryMargin": server_cfg::DEFAULT_MODELS_MEMORY_MARGIN,
            "OpencodeBaseUrl": base_url,
            "ModelsDir": paths::instance_models_dir().to_string_lossy(),
            "StateDir": state_dir,
        },
        "limits": {
            "Port": range("Port"),
            "ModelsMax": range("ModelsMax"),
            "ModelsMemoryMargin": range("ModelsMemoryMargin"),
            "Threads": [1, cpus],
            "ThreadsBatch": [1, cpus],
        },
        "interfaces": interfaces,
    });
    if let Owner::Profile(instance) = owner {
        doc["instance"] = match instance {
            Some(id) => json!(id),
            None => Value::Null,
        };
    }
    if file.withheld {
        doc["withheld"] = json!(["OpencodeApiKey"]);
    }
    doc
}

/// Whose folder the document's `defaults.StateDir` shows, and whether the
/// document names the profile's user: the state root joined with the id, the
/// folder the launch passes that instance.
pub(crate) enum Owner<'a> {
    /// The instance the request names, the form of the instance's Server
    /// route: that document has always named the instance in the folder, so it
    /// carries no `instance` member.
    Instance(&'a str),
    /// A server profile, the form of the profile's route: `instance` is the
    /// instance whose record names the profile, `null` when none does.
    Profile(Option<&'a str>),
}

/// The `values` object of the read's document: every key of [`FIELDS`], each as
/// the JSON type the read gives it. The save reads a body against the same
/// list, so the read and the body cannot name different keys.
fn values(cfg: &ServerConfig) -> Value {
    let mut out = Map::new();
    for field in FIELDS {
        out.insert(field.key.to_string(), (field.read)(cfg));
    }
    Value::Object(out)
}

/// One key of the Server form: its name in the server file (the key
/// `server_cfg::render_for` writes), the JSON type the read gives its value,
/// the value the read sends, and how a body's value lands in the config.
struct Field {
    key: &'static str,
    kind: Kind,
    read: fn(&ServerConfig) -> Value,
    write: fn(&mut ServerConfig, Option<&Value>),
}

/// The JSON type of a key's value; `null`, the key unset, is allowed for every
/// one of them, as the read sends it.
#[derive(Clone, Copy)]
enum Kind {
    /// A whole number: `Port`, the thread counts, `ModelsMax`.
    Int,
    /// A switch: `true`/`false`.
    Bool,
    /// Free text: a host list, a path, a URL, a mode, an API key.
    Text,
    /// The tri-state's `"on"`/`"off"`, the one key with a third state.
    Tri,
    /// A list of strings: the profile's tags. The read always sends a list
    /// (empty when the profile has none), so `null` is not a value of this
    /// kind.
    Tags,
}

impl Kind {
    /// True when `value` is the key unset (the read's `null`, which it sends
    /// for every kind) or a value of this kind. A number that is not a whole
    /// number `i32` can hold is not the type the read gives.
    fn reads(self, value: Option<&Value>) -> bool {
        if let Kind::Tags = self {
            return value
                .and_then(Value::as_array)
                .is_some_and(|tags| tags.iter().all(Value::is_string));
        }
        let Some(value) = value.filter(|value| !value.is_null()) else {
            return true;
        };
        match self {
            Kind::Int => whole(value).is_some(),
            Kind::Bool => value.as_bool().is_some(),
            Kind::Text => value.is_string(),
            Kind::Tri => matches!(value.as_str(), Some("on") | Some("off")),
            Kind::Tags => true,
        }
    }
}

/// Every key of the Server form, in the order the read sends them.
const FIELDS: &[Field] = &[
    Field {
        key: "Port",
        kind: Kind::Int,
        read: |c| json!(c.port),
        write: |c, v| c.port = int(v),
    },
    Field {
        key: "Hostname",
        kind: Kind::Text,
        read: |c| json!(c.hostname),
        write: |c, v| c.hostname = text(v),
    },
    Field {
        key: "Threads",
        kind: Kind::Int,
        read: |c| json!(c.threads),
        write: |c, v| c.threads = int(v),
    },
    Field {
        key: "ThreadsBatch",
        kind: Kind::Int,
        read: |c| json!(c.threads_batch),
        write: |c, v| c.threads_batch = int(v),
    },
    Field {
        key: "ModelsMax",
        kind: Kind::Int,
        read: |c| json!(c.models_max),
        write: |c, v| c.models_max = int(v),
    },
    Field {
        key: "ModelsMemoryMargin",
        kind: Kind::Int,
        read: |c| json!(c.models_memory_margin),
        write: |c, v| c.models_memory_margin = int(v),
    },
    Field {
        key: "Devices",
        kind: Kind::Text,
        read: |c| json!(c.devices),
        write: |c, v| c.devices = text(v),
    },
    Field {
        key: "ModelsDir",
        kind: Kind::Text,
        read: |c| json!(c.models_dir),
        write: |c, v| c.models_dir = text(v),
    },
    Field {
        key: "RocblasUseHipblaslt",
        kind: Kind::Tri,
        read: |c| {
            json!(c
                .rocblas_use_hipblaslt
                .map(|on| if on { "on" } else { "off" }))
        },
        write: |c, v| c.rocblas_use_hipblaslt = tri_state(v),
    },
    // The GTT switch (GT1): a bool WITH an unset state, so the read sends
    // `null` for a key the file does not set and the launch decides (unset
    // exports the pair); the save writes no line for a `null` body.
    Field {
        key: "RocmHostMemoryGtt",
        kind: Kind::Bool,
        read: |c| json!(c.rocm_host_memory_gtt),
        write: |c, v| c.rocm_host_memory_gtt = switch(v),
    },
    Field {
        key: "CudaAllreduce",
        kind: Kind::Text,
        read: |c| json!(c.cuda_allreduce),
        write: |c, v| c.cuda_allreduce = text(v),
    },
    Field {
        key: "CudaDisableGraphs",
        kind: Kind::Bool,
        read: |c| json!(c.cuda_disable_graphs),
        write: |c, v| c.cuda_disable_graphs = switch(v).unwrap_or(false),
    },
    Field {
        key: "AttnRotDisable",
        kind: Kind::Bool,
        read: |c| json!(c.attn_rot_disable),
        write: |c, v| c.attn_rot_disable = switch(v).unwrap_or(false),
    },
    Field {
        key: "RocprofAttach",
        kind: Kind::Bool,
        read: |c| json!(c.rocprof_attach),
        write: |c, v| c.rocprof_attach = switch(v).unwrap_or(false),
    },
    Field {
        key: "Webui",
        kind: Kind::Bool,
        read: |c| json!(c.webui),
        write: |c, v| c.webui = switch(v),
    },
    Field {
        key: "WebuiMcpProxy",
        kind: Kind::Bool,
        read: |c| json!(c.webui_mcp_proxy),
        write: |c, v| c.webui_mcp_proxy = switch(v),
    },
    Field {
        key: "LogVerbosity",
        kind: Kind::Int,
        read: |c| json!(c.log_verbosity),
        write: |c, v| c.log_verbosity = int(v),
    },
    Field {
        key: "SaveStateOnShutdown",
        kind: Kind::Bool,
        read: |c| json!(c.save_state_on_shutdown),
        write: |c, v| c.save_state_on_shutdown = switch(v),
    },
    Field {
        key: "StateDir",
        kind: Kind::Text,
        read: |c| json!(c.state_dir),
        write: |c, v| c.state_dir = text(v),
    },
    Field {
        key: "OpencodeBaseUrl",
        kind: Kind::Text,
        read: |c| json!(c.opencode_base_url),
        write: |c, v| {
            c.opencode_base_url = v
                .and_then(Value::as_str)
                .and_then(server_cfg::normalize_base_url);
        },
    },
    Field {
        key: "OpencodeApiKey",
        kind: Kind::Text,
        read: |c| json!(c.opencode_api_key),
        write: |c, v| c.opencode_api_key = text(v),
    },
    Field {
        key: "Tags",
        kind: Kind::Tags,
        read: |c| json!(c.tags),
        write: |c, v| c.tags = tag_list(v),
    },
];

/// The body's `Tags` list as the config's list: the strings of the array, in
/// its order. An empty array is no tags, and the file carries no key for it.
fn tag_list(value: Option<&Value>) -> Vec<String> {
    value
        .and_then(Value::as_array)
        .map(|tags| {
            tags.iter()
                .filter_map(Value::as_str)
                .map(str::to_string)
                .collect()
        })
        .unwrap_or_default()
}

/// `value` as the whole number a numeric key holds, None for a fraction or a
/// number past `i32` (the read never gives one).
fn whole(value: &Value) -> Option<i32> {
    value.as_i64().and_then(|n| i32::try_from(n).ok())
}

/// The body's value for a numeric key: `int`, `null` the key unset.
fn int(value: Option<&Value>) -> Option<i32> {
    whole(value?)
}

/// The body's value for a switch: `null` the key unset.
fn switch(value: Option<&Value>) -> Option<bool> {
    value?.as_bool()
}

/// The body's value for a text key: blank left unset, as the INI read leaves a
/// blank line unset (`server_cfg::opt_nonblank`).
fn text(value: Option<&Value>) -> Option<String> {
    server_cfg::opt_nonblank(value?.as_str().map(str::to_string))
}

/// The body's value for the tri-state, as the read sends it.
fn tri_state(value: Option<&Value>) -> Option<bool> {
    match value?.as_str() {
        Some("on") => Some(true),
        Some("off") => Some(false),
        _ => None,
    }
}

/// What the server form's `PUT` body answers: the whole file's text, or the
/// bff's refusal of the body (400, its shape) or of one of its values (422,
/// naming the INI key the form marks). The manager is not asked: this is the
/// step that turns the body into the text the compare-and-swap then writes.
#[derive(Debug)]
pub(crate) enum BodyAnswer {
    /// The whole file, as `server_cfg::render_for(cfg, true)` renders it.
    File(String),
    /// 400 `server-malformed`: the body is not `{revision, values}` with every
    /// value of the JSON type the read gives it.
    Malformed,
    /// 422 `{"key", "args", "field"}`: `key` and `args` are the check's
    /// refusal, `field` the INI key the form marks.
    Refused {
        key: String,
        args: Vec<String>,
        field: String,
    },
}

/// The server form's body to the whole file's text: the values to a
/// `ServerConfig` (the base URL normalized, a blank directory left unset, a
/// null `OpencodeApiKey` no key), `server_cfg::check` with `cpus` (this
/// machine's logical CPU count), `server_cfg::render_for(cfg, true)`.
pub(crate) fn body_to_file(body: &Value, cpus: usize) -> BodyAnswer {
    let Some(values) = body
        .as_object()
        .filter(|top| matches!(top.get("revision"), Some(Value::String(_))))
        .and_then(|top| top.get("values"))
        .and_then(Value::as_object)
    else {
        return BodyAnswer::Malformed;
    };
    if !FIELDS
        .iter()
        .all(|field| field.kind.reads(values.get(field.key)))
    {
        return BodyAnswer::Malformed;
    }
    let mut cfg = ServerConfig::default();
    for field in FIELDS {
        (field.write)(&mut cfg, values.get(field.key));
    }
    if let Err(refusal) = server_cfg::check(&cfg, cpus) {
        return BodyAnswer::Refused {
            field: field_of(refusal.key, &refusal.values),
            key: refusal.key.to_string(),
            args: refusal.values,
        };
    }
    BodyAnswer::File(server_cfg::render_for(&cfg, true))
}

/// The INI key a refusal belongs to, the control the form marks: the key the
/// refusal names in its first value, and for the checks that name none, the key
/// they are about. The `PUT` route hands it the manager's `Invalid` refusals
/// too, which add the load-on-startup pair (`ModelsMax`, the key they are
/// about) to the refusals of the bff's own `server_cfg::check`.
pub(crate) fn field_of(key: &str, args: &[String]) -> String {
    match key {
        "bind-all-with-others" => "Hostname".to_string(),
        "load-on-startup-over-max" | "load-on-startup-unlimited" => "ModelsMax".to_string(),
        // The tags refusal names the reason first, and the form has one
        // control for the whole list: the field is the key it is about.
        "server-bad-tags" => "tags".to_string(),
        _ => args.first().cloned().unwrap_or_default(),
    }
}

/// The file's revision: SipHash-2-4 of its whole text under `key`, the
/// router's secret, printed as 16 hex digits. A change detector, not an
/// oracle: the same text through one router gives the same revision, any
/// other text (a comment, the API key, included) gives another, and because
/// the function is keyed nobody who holds a revision and not the file can
/// check a guess about its text. A bff start draws a new secret, so every
/// open form is stale after it and reloads once.
pub(crate) fn revision(text: &str, key: &[u8; 16]) -> String {
    let mut hasher = SipHasher::new_with_key(key);
    hasher.write(text.as_bytes());
    let hash = hasher.finish();
    format!("{hash:016x}")
}

/// The state of the write of one request, in one atomic shared by the handler
/// and its blocking task: `WRITE_IDLE` while no write has been asked,
/// `WRITE_ASKED` from the moment the task sends one, `WRITE_DONE` once EVERY
/// write of the call has landed, `WRITE_CANCELLED` when the handler's time
/// limit ran out with no write asked. Each side moves the state with a compare
/// and exchange, so the pair settles the write once: a task that finds the
/// state Cancelled sends no write, a handler that finds it Asked answers 504,
/// the outcome of the sent write unknown, and a handler that finds it Done
/// answers its own 200, the file saved. The server file's save and the model profile
/// switch share it.
pub(crate) struct WriteState(AtomicU8);

impl WriteState {
    pub(crate) fn new() -> Self {
        Self(AtomicU8::new(WRITE_IDLE))
    }

    /// The task's way to a write: true when it may send one. Idle to Asked is
    /// the only way. Lost to the handler's Cancelled, the handler has answered
    /// that the manager was never asked to write, so this request is over and
    /// the task sends nothing. A state already Asked is a write this very task
    /// sent: its outcome is already unknown, so the task goes on to the next
    /// write of the same request.
    pub(crate) fn may_write(&self) -> bool {
        match self
            .0
            .compare_exchange(WRITE_IDLE, WRITE_ASKED, Ordering::SeqCst, Ordering::SeqCst)
        {
            Ok(_) => true,
            Err(WRITE_CANCELLED) => false,
            Err(_) => true,
        }
    }

    /// True when the task sent a write, so its outcome is unknown: the answer
    /// to the manager's `Unavailable` is then 504 `client-timeout`, it was
    /// reached, and not 503 `client-unavailable`. A state the task moved to
    /// Done is not this: that write landed, and what the call could not read
    /// afterwards is not the write's outcome.
    pub(crate) fn sent(&self) -> bool {
        self.0.load(Ordering::SeqCst) == WRITE_ASKED
    }

    /// The task's word that every write of the call has landed. Asked to Done
    /// is the only move, so a handler that already settled the state
    /// (Cancelled: the write was never asked) is left alone.
    pub(crate) fn landed(&self) {
        let _ =
            self.0
                .compare_exchange(WRITE_ASKED, WRITE_DONE, Ordering::SeqCst, Ordering::SeqCst);
    }

    /// The task's word that the call needs no write at all, the text it was
    /// asked for being the text the file holds. Idle to Done is the only move,
    /// so a handler that already settled the state (Cancelled: the limit ran
    /// out on the reads) is left alone, and a limit that runs out on what
    /// follows finds the call DONE, answering its own 200.
    pub(crate) fn nothing_to_write(&self) {
        let _ = self
            .0
            .compare_exchange(WRITE_IDLE, WRITE_DONE, Ordering::SeqCst, Ordering::SeqCst);
    }

    /// The handler's answer when its time limit ran out and its task is still
    /// running. Idle to Cancelled settles it with the task: won, no write was
    /// ever asked and the task will never ask one, so [`TimedOut::Cancelled`];
    /// lost, the task settled the state itself: [`TimedOut::Asked`] while a
    /// write's outcome is unknown, [`TimedOut::Done`] once the call is over,
    /// every write of it landed or none needed.
    pub(crate) fn timed_out(&self) -> TimedOut {
        if self
            .0
            .compare_exchange(
                WRITE_IDLE,
                WRITE_CANCELLED,
                Ordering::SeqCst,
                Ordering::SeqCst,
            )
            .is_ok()
        {
            return TimedOut::Cancelled;
        }
        if self.0.load(Ordering::SeqCst) == WRITE_DONE {
            TimedOut::Done
        } else {
            TimedOut::Asked
        }
    }
}

/// What a handler's time limit found in the shared write state ([`WriteState`]),
/// and the answer each write route makes of it.
pub(crate) enum TimedOut {
    /// No write was asked, and the task left behind by the limit will never
    /// ask one: 503 `client-unavailable`, the manager not reached.
    Cancelled,
    /// A write is sent and its outcome is unknown: 504 `client-timeout`.
    Asked,
    /// Every write of the call landed, or the call needed no write at all, so
    /// it is DONE: the limit ran out on a call that follows the writes. The
    /// handler answers its own 200, the file saved as the body asked, and the
    /// state it cannot read (`running`) is true, the safe answer.
    Done,
}

const WRITE_IDLE: u8 = 0;
const WRITE_ASKED: u8 = 1;
const WRITE_CANCELLED: u8 = 2;
const WRITE_DONE: u8 = 3;

/// The answer every write route makes of what its time limit found
/// ([`WriteState::timed_out`]): 503 `client-unavailable` while no write was
/// asked, 504 `client-timeout` once one was sent, and `done`, the route's own
/// success, when every write of the call landed: the save's 200 with `running`
/// true, a create's 201, a removal's or a verb's 204. The route builds `done`
/// from what it asked for, never from a read it can no longer make.
pub(crate) fn timeout_answer(timed: TimedOut, done: Response) -> Response {
    match timed {
        TimedOut::Cancelled => error(StatusCode::SERVICE_UNAVAILABLE, "client-unavailable", &[]),
        TimedOut::Asked => error(StatusCode::GATEWAY_TIMEOUT, "client-timeout", &[]),
        TimedOut::Done => done,
    }
}

/// `PUT /api/instances/{id}/server` `{"revision", "values"}`: the Server
/// form's save of the whole file, as the signed-in user. The answer is
/// `{"revision", "running"}`, the revision of the text the file holds now and
/// whether the instance's process runs; the session is read first, so with no
/// session the manager is not even connected to (401), and only a signed-in
/// request sees a body over `origin::MAX_BODY` refused by the extractor with a
/// 413 (as the boot write).
///
/// The session, the body and the bff's own check answer first, so a body
/// nothing can be made of reaches no manager. The file is then read as the
/// session's user, and its revision compared with the one the form sends:
/// another one is 409 `server-changed-elsewhere` and nothing is written, the
/// session staying signed in. The read text is what the compare and swap
/// expects, so a write that lands between the read and the write is refused
/// by the manager and answered with the same 409. The read, the write and the
/// status that answers `running` are three calls of one connect: a file the
/// manager does not have is `client-no-instance`, and a status it cannot
/// answer leaves `running` true, the save having succeeded. The 503 and the
/// 504 turn on one fact: while the write was never asked, a time limit that
/// runs out or a stream that dies is 503 `client-unavailable`, the manager
/// not reached; from the moment the write was sent its outcome is unknown,
/// and the answer is 504 `client-timeout`.
///
/// A time limit does not stop the blocking task: the handler and that task
/// share one atomic state ([`WriteState`]) and settle that fact together with a
/// compare and exchange, so the answer is the state's last word. A task parked
/// in the connect or the read when the limit runs out finds the state
/// Cancelled and sends no write: the 503 stands, the file is what it was.
///
/// `write_timeout` bounds the whole save, the read, the write and the status
/// that answers `running`: the manager's calls of a write are given the write's
/// own, longer limit (the reads of the router keep [`crate::MANAGER_TIMEOUT`],
/// the shorter one), so a manager busy writing is not cut off at the limit that
/// is right for a read. A limit that runs out once every write landed finds the
/// state Done: the file holds what the form sent, so the answer is the save's
/// 200 with `running` true, the safe answer for a state the bff cannot read,
/// and not the 504 that tells the user to look at the server and decide
/// whether the file is saved.
pub(crate) async fn save(
    connect: Connect,
    write_timeout: Duration,
    sessions: Arc<Sessions>,
    id: String,
    headers: HeaderMap,
    key: [u8; 16],
    body: Result<axum::body::Bytes, axum::extract::rejection::BytesRejection>,
) -> Response {
    let Some((session, identity)) = sessions.current(&headers) else {
        return error(StatusCode::UNAUTHORIZED, "session-required", &[]);
    };
    let body = match body {
        Ok(body) => body,
        Err(rejection) => return body_refusal(&rejection, "server-malformed"),
    };
    let Some(body) = json_body(&headers, &body) else {
        return error(StatusCode::BAD_REQUEST, "server-malformed", &[]);
    };
    let Some(sent) = body.get("revision").and_then(Value::as_str) else {
        return error(StatusCode::BAD_REQUEST, "server-malformed", &[]);
    };
    let cpus = std::thread::available_parallelism().map_or(1, |n| n.get());
    let text = match body_to_file(&body, cpus) {
        BodyAnswer::File(text) => text,
        BodyAnswer::Malformed => return error(StatusCode::BAD_REQUEST, "server-malformed", &[]),
        BodyAnswer::Refused { key, args, field } => return refused(&key, &args, &field),
    };
    let user = identity.user;
    let sent = sent.to_string();
    let written = revision(&text, &key);
    // The write's state. It moves Idle -> Asked just before the write is sent,
    // Asked -> Done once that write landed, and Idle -> Cancelled when the time
    // limit runs out first: from the moment the write is sent its outcome is
    // unknown, and every answer that cannot see it is 504, never 503; once the
    // write landed the save is done, and the answer is the 200.
    let state = Arc::new(WriteState::new());
    let mine = state.clone();
    let ask = tokio::task::spawn_blocking(move || -> Result<bool, ClientError> {
        let client = connect(Some(user.as_str()))?;
        let name = profile_file(&*client, &id)?;
        let Some(file) = client.read_file(&name)? else {
            return Err(ClientError::NoInstance);
        };
        let current = file.text;
        if revision(&current, &key) != sent {
            return Err(ClientError::Changed);
        }
        // Idle -> Asked is the only way to the write. Lost, the handler ran
        // out of time and answered 503, so this save is over and its file is
        // left as the browser was told; the value returned is never read.
        if !mine.may_write() {
            return Ok(true);
        }
        client.write_file_if(&name, &text, &current)?;
        // The write landed: what follows is a read of the state, and a limit
        // that runs out there finds the save done.
        mine.landed();
        Ok(match client.status(&id) {
            Ok(running) => runs(running),
            Err(_) => true,
        })
    });
    match tokio::time::timeout(write_timeout, ask).await {
        Ok(Ok(Ok(running))) => axum::Json(json!({
            "revision": written,
            "running": running,
        }))
        .into_response(),
        Ok(Ok(Err(failure))) => save_failure(&sessions, &session, failure, state.sent()),
        // The answer cannot see the write, and the task keeps running. Idle ->
        // Cancelled settles it with the task: won, the write was never asked
        // and the task will never ask it, so 503 `client-unavailable`, the
        // manager not reached; lost, the state is Asked, the write is sent and
        // its outcome unknown, so 504 `client-timeout`; a state that reached
        // Done is a write that landed, so the file holds what the form sent and
        // the answer is the save's 200, `running` true, the state the bff
        // cannot read.
        Err(_) => timeout_answer(
            state.timed_out(),
            axum::Json(json!({
                "revision": written,
                "running": true,
            }))
            .into_response(),
        ),
        Ok(Err(_)) => error(StatusCode::BAD_GATEWAY, "client-other", &[]),
    }
}

/// Whether the instance's process runs in this state, the one the save's and
/// the model profile switch's `running` answers with.
pub(crate) fn runs(state: InstanceState) -> bool {
    matches!(state, InstanceState::Active | InstanceState::Activating)
}

/// The 422 answer to a value the checks refused: the check's key, its values,
/// and `field`, the INI key the form marks.
fn refused(key: &str, args: &[String], field: &str) -> Response {
    (
        StatusCode::UNPROCESSABLE_ENTITY,
        axum::Json(json!({ "key": key, "args": args, "field": field })),
    )
        .into_response()
}

/// The answer to a save the manager could not do. The compare and swap
/// refused: 409 `server-changed-elsewhere`, the file is not the one the form
/// read. A content check the manager ran, its own or an address check, is 422
/// with its key, its values and the field the form marks; the manager's detail
/// stays out of the body. The manager refused the signed-in user: 403
/// `session-not-administrator` (the sign-in's key for the same fact) and the
/// session ends with the right that authorized it. The other answers are the
/// bff's news, not a refusal of the user, so the session lives on: no such
/// instance 404, anything else 502. `Unavailable` (the pipe and the bus answer
/// a broken stream or a missing reply with it) is 504 `client-timeout` when
/// `write_asked` says the state reached `WRITE_ASKED`, the write sent and its
/// outcome unknown, and 503 `client-unavailable` when it stayed Idle: the
/// connect or the read never reached a manager. Only a handler that stopped
/// waiting sets Cancelled, so the state a task hands back is Idle or Asked.
fn save_failure(
    sessions: &Arc<Sessions>,
    session: &str,
    failure: ClientError,
    write_asked: bool,
) -> Response {
    match failure {
        ClientError::Changed => error(StatusCode::CONFLICT, "server-changed-elsewhere", &[]),
        ClientError::Invalid(refusal) => refused(
            refusal.key,
            &refusal.values,
            &field_of(refusal.key, &refusal.values),
        ),
        ClientError::AddressInUse(id) => refused("client-address-in-use", &[id], "Port"),
        ClientError::UnresolvableHost(host) => {
            refused("client-unresolvable-host", &[host], "Hostname")
        }
        ClientError::NotAuthorized => {
            sessions.end(session);
            error(StatusCode::FORBIDDEN, "session-not-administrator", &[])
        }
        ClientError::NoInstance | ClientError::InvalidId | ClientError::BadFileName => {
            error(StatusCode::NOT_FOUND, "client-no-instance", &[])
        }
        ClientError::Unavailable if write_asked => {
            error(StatusCode::GATEWAY_TIMEOUT, "client-timeout", &[])
        }
        ClientError::Unavailable => {
            error(StatusCode::SERVICE_UNAVAILABLE, "client-unavailable", &[])
        }
        _ => error(StatusCode::BAD_GATEWAY, "client-other", &[]),
    }
}

#[cfg(test)]
#[path = "server_tests.rs"]
mod tests;
