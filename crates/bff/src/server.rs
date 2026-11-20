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
        Ok(Ok(Ok(Some(file)))) => return axum::Json(document(&file, key, &id)).into_response(),
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

/// The 200 document of the file. `values.OpencodeApiKey` is `null` and the
/// document carries `"withheld": ["OpencodeApiKey"]` exactly when the
/// manager's `withheld` flag is set, even when the text it sent still names
/// a key: every signed-in session is a member of the administrators' group
/// (sign-in refuses anyone else 403), so the bff keeps nothing on its own.
/// `revision` is the keyed hash of the text under `key`. `id` is the instance
/// the request names: the `defaults.StateDir` shown is that instance's folder
/// (the engine's state root joined with the id, as the launch passes it),
/// never the bare root.
fn document(file: &ManagerFile, key: [u8; 16], id: &str) -> Value {
    let text = file.text.as_str();
    let mut cfg = server_cfg::from_text(text);
    if file.withheld {
        cfg.opencode_api_key = None;
    }
    let cpus = std::thread::available_parallelism().map_or(1, |n| n.get());
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
    let mut doc = json!({
        "revision": revision(text, &key),
        "values": values(&cfg),
        "defaults": {
            "Port": server_cfg::DEFAULT_PORT,
            "ModelsMax": server_cfg::DEFAULT_MODELS_MAX,
            "ModelsMemoryMargin": server_cfg::DEFAULT_MODELS_MEMORY_MARGIN,
            "OpencodeBaseUrl": base_url,
            "ModelsDir": paths::instance_models_dir().to_string_lossy(),
            "StateDir": paths::instance_state_dir().join(id).to_string_lossy(),
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
    if file.withheld {
        doc["withheld"] = json!(["OpencodeApiKey"]);
    }
    doc
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
        Err(_) => match state.timed_out() {
            TimedOut::Cancelled => {
                error(StatusCode::SERVICE_UNAVAILABLE, "client-unavailable", &[])
            }
            TimedOut::Asked => error(StatusCode::GATEWAY_TIMEOUT, "client-timeout", &[]),
            TimedOut::Done => axum::Json(json!({
                "revision": written,
                "running": true,
            }))
            .into_response(),
        },
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
mod tests {
    use super::*;

    /// The CPU count the step is handed. The test picks it, so the bound a
    /// refusal pins does not move with the machine running the test.
    const CPUS: usize = 16;

    /// A form body's revision: an opaque string this step never reads.
    const REV: &str = "0123456789abcdef";

    /// A config with a non-default value in every field, each of them a value
    /// the body hands back unchanged (a base URL already stored without the
    /// `/v1` suffix, an AllReduce mode already canonical), so the file
    /// `render_for` gives for it is the file its body must render.
    fn rich_cfg() -> ServerConfig {
        ServerConfig {
            port: Some(8580),
            hostname: Some("127.0.0.1".into()),
            threads: Some(8),
            threads_batch: Some(4),
            models_max: Some(2),
            models_memory_margin: Some(1024),
            models_dir: Some("/srv/models".into()),
            rocblas_use_hipblaslt: Some(false),
            cuda_allreduce: Some("nccl".into()),
            cuda_disable_graphs: true,
            attn_rot_disable: true,
            rocprof_attach: true,
            webui: Some(false),
            webui_mcp_proxy: Some(true),
            log_verbosity: Some(2),
            save_state_on_shutdown: Some(true),
            state_dir: Some("/srv/state".into()),
            opencode_base_url: Some("http://127.0.0.1:4096".into()),
            opencode_api_key: Some("sk-secret".into()),
            tags: vec!["edge".into(), "v2".into()],
        }
    }

    /// The body the form sends for `cfg`: every key of the read, with the JSON
    /// type the read gives it (a number, a bool, `"on"`/`"off"`, a string,
    /// `null` for an unset key), as `document` writes them.
    fn body_of(cfg: &ServerConfig) -> Value {
        json!({
            "revision": REV,
            "values": {
                "Port": cfg.port,
                "Hostname": cfg.hostname,
                "Threads": cfg.threads,
                "ThreadsBatch": cfg.threads_batch,
                "ModelsMax": cfg.models_max,
                "ModelsMemoryMargin": cfg.models_memory_margin,
                "ModelsDir": cfg.models_dir,
                "RocblasUseHipblaslt": cfg
                    .rocblas_use_hipblaslt
                    .map(|on| if on { "on" } else { "off" }),
                "CudaAllreduce": cfg.cuda_allreduce,
                "CudaDisableGraphs": cfg.cuda_disable_graphs,
                "AttnRotDisable": cfg.attn_rot_disable,
                "RocprofAttach": cfg.rocprof_attach,
                "Webui": cfg.webui,
                "WebuiMcpProxy": cfg.webui_mcp_proxy,
                "LogVerbosity": cfg.log_verbosity,
                "SaveStateOnShutdown": cfg.save_state_on_shutdown,
                "StateDir": cfg.state_dir,
                "OpencodeBaseUrl": cfg.opencode_base_url,
                "OpencodeApiKey": cfg.opencode_api_key,
                "Tags": cfg.tags,
            },
        })
    }

    /// The rich body, with the value of the one key `key` replaced by `value`:
    /// every other key keeps the rich config's own value, so the answer can
    /// only be about the key the test changed.
    fn body_with(key: &str, value: Value) -> Value {
        let mut body = body_of(&rich_cfg());
        values_of(&mut body).insert(key.to_string(), value);
        body
    }

    /// The same, for a key of the body itself (`revision`, `values`).
    fn body_top(key: &str, value: Value) -> Value {
        let mut body = body_of(&rich_cfg());
        body.as_object_mut()
            .expect("the body is an object")
            .insert(key.to_string(), value);
        body
    }

    fn values_of(body: &mut Value) -> &mut serde_json::Map<String, Value> {
        body["values"]
            .as_object_mut()
            .expect("the body's values are an object")
    }

    /// The file `body_to_file` renders, panicking on any other answer.
    fn file(body: Value) -> String {
        match body_to_file(&body, CPUS) {
            BodyAnswer::File(text) => text,
            other => panic!("expected the rendered file, got {other:?}"),
        }
    }

    /// The 422 `body_to_file` answers, as `{key, args, field}`, panicking on any
    /// other answer.
    fn refused(body: Value) -> (String, Vec<String>, String) {
        match body_to_file(&body, CPUS) {
            BodyAnswer::Refused { key, args, field } => (key, args, field),
            other => panic!("expected 422, got {other:?}"),
        }
    }

    /// The 400 `body_to_file` answers for `why`, panicking on any other answer.
    fn assert_malformed(body: Value, why: &str) {
        match body_to_file(&body, CPUS) {
            BodyAnswer::Malformed => {}
            other => panic!("expected 400 server-malformed for {why}, got {other:?}"),
        }
    }

    /// The form's whole body, well typed, is the file `render_for(cfg, true)`
    /// renders for the same config: every value lands in the config, and the
    /// text is the renderer's, never the body's.
    #[test]
    fn body_to_file_renders_the_file_render_for_gives() {
        let cfg = rich_cfg();
        let got = file(body_of(&cfg));
        assert_eq!(
            got,
            server_cfg::render_for(&cfg, true),
            "a well-typed body renders exactly render_for(cfg, true)"
        );
    }

    /// A `Port` of a type the read never gives (a JSON string, not a number) is
    /// a body the bff cannot read: 400, nothing rendered, the manager not asked.
    #[test]
    fn body_to_file_refuses_a_string_port_as_malformed() {
        assert_malformed(body_with("Port", json!("8580")), "a string Port");
    }

    /// Every body shape the bff cannot read is 400: the document it expects is
    /// `{revision: string, values: object}`, with every value the JSON type the
    /// read gives it.
    #[test]
    fn body_to_file_refuses_every_body_shape_it_cannot_read() {
        assert_malformed(json!([]), "a body that is not an object");
        assert_malformed(json!("values"), "a body that is a string");
        assert_malformed(json!(null), "a body that is null");
        let mut no_revision = body_of(&rich_cfg());
        no_revision
            .as_object_mut()
            .expect("the body")
            .remove("revision");
        assert_malformed(no_revision, "a body with no revision");
        assert_malformed(
            body_top("revision", json!(7)),
            "a revision that is not a string",
        );
        assert_malformed(body_top("revision", Value::Null), "a null revision");
        let mut no_values = body_of(&rich_cfg());
        no_values
            .as_object_mut()
            .expect("the body")
            .remove("values");
        assert_malformed(no_values, "a body with no values");
        assert_malformed(
            body_top("values", json!([])),
            "values that is not an object",
        );
        assert_malformed(body_top("values", json!("x")), "values that is a string");
        // A value of the wrong JSON type, one for each type the read gives.
        assert_malformed(
            body_with("Hostname", json!(true)),
            "a bool where a string is read",
        );
        assert_malformed(
            body_with("ModelsDir", json!(4)),
            "a number where a string is read",
        );
        assert_malformed(
            body_with("Port", json!(true)),
            "a bool where a number is read",
        );
        assert_malformed(
            body_with("Threads", json!(4.5)),
            "a fraction where a whole number is read",
        );
        assert_malformed(
            body_with("Webui", json!("on")),
            "a string where a bool is read",
        );
        assert_malformed(
            body_with("SaveStateOnShutdown", json!(1)),
            "a number where a bool is read",
        );
        assert_malformed(
            body_with("RocblasUseHipblaslt", json!(true)),
            "a bool where the tri-state's on/off string is read",
        );
    }

    /// `Threads` above this machine's CPU count is a check the bff runs itself
    /// (the manager cannot): 422 with the check's key and args, and `field` the
    /// key it refuses. The bound is the `cpus` argument, not the test's machine.
    #[test]
    fn body_to_file_refuses_threads_above_the_cpu_count() {
        let over = (CPUS + 1) as i64;
        let (key, args, field) = refused(body_with("Threads", json!(over)));
        assert_eq!(key, "not-whole-number-in-range");
        assert_eq!(field, "Threads", "the 422 marks the control it refuses");
        let want = vec![
            "Threads".to_string(),
            over.to_string(),
            "1".to_string(),
            CPUS.to_string(),
        ];
        assert_eq!(args, want, "the 422 carries the check's own args");
        let (_, _, field) = refused(body_with("ThreadsBatch", json!(over)));
        assert_eq!(field, "ThreadsBatch", "the same rule on the batch count");
        let text = file(body_with("Threads", json!(CPUS as i64)));
        assert!(
            text.contains("Threads = 16"),
            "Threads AT the CPU count is written: {text}"
        );
    }

    /// A `;` in `ModelsDir`, or a `#` in any of the free-text keys (the INI
    /// format has no escaping): 422 `comment-markers`, `field` the key the
    /// refusal names, which is its `args[0]`.
    #[test]
    fn body_to_file_refuses_comment_markers_in_a_text_value() {
        let (key, args, field) = refused(body_with("ModelsDir", json!("/srv/mod;els")));
        assert_eq!(key, "comment-markers");
        assert_eq!(
            field, "ModelsDir",
            "field is args[0], the key the refusal names"
        );
        assert_eq!(args, vec!["ModelsDir".to_string()]);
        let (key, args, field) = refused(body_with("StateDir", json!("/srv/st#ate")));
        assert_eq!(key, "comment-markers");
        assert_eq!(field, "StateDir");
        assert_eq!(args, vec!["StateDir".to_string()]);
        let (key, _, field) = refused(body_with("OpencodeApiKey", json!("sk-;secret")));
        assert_eq!(key, "comment-markers");
        assert_eq!(field, "OpencodeApiKey", "the same mapping on the API key");
    }

    /// The profile's tags obey the manager's rule (`checks::tags_rule`) on the
    /// web save too: a list the rule refuses is 422 with the rule's key and
    /// args, and `field` is the form's control, `tags` (lowercase, the one key
    /// of the list, as the decision of 2026-11-20 names it). The save writes
    /// nothing (the 422 answers before the manager is asked).
    #[test]
    fn body_to_file_refuses_a_tags_list_the_rule_refuses() {
        let want = |reason: &str, list: &str| {
            [
                reason.to_string(),
                list.to_string(),
                checks::TAGS_MAX.to_string(),
                checks::TAG_MIN.to_string(),
                checks::TAG_MAX.to_string(),
            ]
            .to_vec()
        };
        for (tags, reason, list) in [
            (json!(["Edge"]), "bad characters", "Edge".to_string()),
            (
                json!(["gpu", "gpu"]),
                "the same tag twice",
                "gpu,gpu".to_string(),
            ),
            (
                json!(["gpu edge"]),
                "bad characters",
                "gpu edge".to_string(),
            ),
            (json!([""]), "empty tag", String::new()),
        ] {
            let (key, args, field) = refused(body_with("Tags", tags));
            assert_eq!(key, "server-bad-tags", "the rule's one key");
            assert_eq!(args, want(reason, &list), "the rule's own args");
            assert_eq!(field, "tags", "the 422 marks the tags control");
        }
    }

    /// A tags list of the rule's shape is written, in the body's order; an
    /// empty list is no tags and the file carries no `Tags` key.
    #[test]
    fn body_to_file_writes_a_tags_list_of_the_rules_shape() {
        let text = file(body_with("Tags", json!(["edge", "v2"])));
        assert!(text.contains("Tags = edge,v2"), "{text}");
        let text = file(body_with("Tags", json!([])));
        assert!(!text.contains("Tags ="), "no tags writes no key: {text}");
    }

    /// A tags list holding something that is not a string is a body the bff
    /// cannot read: 400 `server-malformed`, the same answer as a value of the
    /// wrong type of any other key.
    #[test]
    fn body_to_file_refuses_a_tags_list_that_is_not_strings() {
        assert_malformed(
            body_with("Tags", json!(["gpu", 7])),
            "a number among the tags",
        );
        assert_malformed(
            body_with("Tags", json!({"a": "gpu"})),
            "an object where a list is read",
        );
        assert_malformed(body_with("Tags", json!("gpu")), "a bare string");
        assert_malformed(body_with("Tags", Value::Null), "a null tags list");
    }

    /// The base URL is stored as the GUI stores it: a trailing `/` and a
    /// trailing `/v1` are dropped, so a typed `/v1` is never appended twice.
    #[test]
    fn body_to_file_stores_the_base_url_without_its_v1_suffix() {
        let want = server_cfg::render_for(&rich_cfg(), true);
        for suffix in ["/v1", "/v1/", "/"] {
            let url = format!("http://127.0.0.1:4096{suffix}");
            let got = file(body_with("OpencodeBaseUrl", json!(url)));
            assert_eq!(got, want, "a base URL ending {suffix} is stored without it");
        }
    }

    /// A blank `ModelsDir` or `StateDir` is left unset: the file carries the
    /// commented hint, never a folder of the bff's own account (an instance
    /// whose file leaves them unset uses the engine's folders).
    #[test]
    fn body_to_file_leaves_a_blank_directory_unset() {
        let mut cfg = rich_cfg();
        cfg.models_dir = None;
        cfg.state_dir = None;
        let mut body = body_with("ModelsDir", json!("   "));
        values_of(&mut body).insert("StateDir".to_string(), json!(""));
        let got = file(body);
        assert_eq!(
            got,
            server_cfg::render_for(&cfg, true),
            "a blank ModelsDir or StateDir is left unset"
        );
        for key in ["ModelsDir", "StateDir"] {
            let named: Vec<&str> = got.lines().filter(|l| l.starts_with(key)).collect();
            assert!(
                named.is_empty(),
                "{key} stays out of the file, its hint is a comment: {named:?}"
            );
        }
    }

    /// `OpencodeApiKey: null` writes no key (the line stays a commented hint, so
    /// the file reloads with no key); a string writes that key.
    #[test]
    fn body_to_file_writes_the_api_key_only_when_the_body_carries_one() {
        let got = file(body_with("OpencodeApiKey", Value::Null));
        let lines: Vec<&str> = got
            .lines()
            .filter(|l| l.starts_with("OpencodeApiKey"))
            .collect();
        assert!(lines.is_empty(), "a null key writes no key line: {lines:?}");
        assert!(
            got.lines().any(|l| l.starts_with("; OpencodeApiKey")),
            "the key stays commented, so it reloads unset"
        );
        let set = file(body_of(&rich_cfg()));
        assert!(
            set.lines().any(|l| l == "OpencodeApiKey = sk-secret"),
            "a string sets the key"
        );
    }

    /// A key the body does not carry at all is read as the body's `null` for it: the setting
    /// is left unset, its line rendered as the commented hint. The form sends every key of the
    /// read, so this is the shape of a body written by hand: it must neither keep a setting by
    /// accident, nor clear one silently. Pinned as the code stands today: a `read_setting` that
    /// reads a missing key as "leave the file alone", which is the design's intent
    /// (ARCHITECTURE.md, the saving step), has to show up here as a deliberate change.
    #[test]
    fn body_to_file_treats_a_key_absent_from_the_body_as_unset() {
        let mut missing = body_of(&rich_cfg());
        values_of(&mut missing).remove("Port");
        let got = file(missing);
        assert_eq!(
            got,
            file(body_with("Port", Value::Null)),
            "a key the body does not carry renders as the body's null for it"
        );
        assert!(
            !got.lines().any(|l| l.starts_with("Port")),
            "a Port the body does not carry is not set: {got}"
        );
        assert!(
            got.lines().any(|l| l.starts_with("; Port")),
            "an unset Port renders the commented hint, so the file reloads at the default"
        );

        let mut missing = body_of(&rich_cfg());
        values_of(&mut missing).remove("OpencodeApiKey");
        let got = file(missing);
        assert!(
            !got.lines().any(|l| l.starts_with("OpencodeApiKey")),
            "an OpencodeApiKey the body does not carry writes no key: {got}"
        );
        assert!(
            got.lines().any(|l| l.starts_with("; OpencodeApiKey")),
            "an absent key leaves the hint commented, so the file reloads with no key"
        );
    }

    /// `0.0.0.0` bound with another address is the bff's check (the manager runs
    /// it again): 422, with `field` the design maps this key to, `Hostname`.
    #[test]
    fn body_to_file_refuses_a_bind_all_with_another_address() {
        let (key, args, field) = refused(body_with("Hostname", json!("0.0.0.0,127.0.0.1")));
        assert_eq!(key, "bind-all-with-others");
        assert_eq!(args, Vec::<String>::new(), "this refusal carries no args");
        assert_eq!(field, "Hostname", "the design maps this key to Hostname");
    }

    /// An integer out of its range is 422 with `field` the key the refusal
    /// names, as on `Threads`: the mapping is `args[0]`, not one field per key.
    #[test]
    fn body_to_file_refuses_an_integer_out_of_range_on_its_own_field() {
        for (key, value) in [
            ("Port", json!(70000)),
            ("ModelsMemoryMargin", json!(-1)),
            ("LogVerbosity", json!(6)),
        ] {
            let (refusal_key, args, field) = refused(body_with(key, value));
            assert_eq!(
                refusal_key, "not-whole-number-in-range",
                "{key} out of range"
            );
            assert_eq!(field, key, "field is args[0] of the refusal, naming {key}");
            assert_eq!(args.first().map(String::as_str), Some(key));
        }
    }

    /// The bounds of the thread check are the CPU count the step is handed,
    /// not one it computes: fed `CPUS`, it refuses `CPUS + 1` and names `CPUS`
    /// as the bound. Which count the route hands it (this machine's) is pinned
    /// by `a_value_the_bff_check_refuses_is_422_naming_its_field` in
    /// `tests/server.rs`, through the `limits` of the read the form saved.
    #[test]
    fn the_thread_bound_of_the_check_is_the_cpu_count_it_is_handed() {
        let mut cfg = rich_cfg();
        cfg.threads = Some(CPUS as i32 + 1);
        let (key, args, field) = refused(body_of(&cfg));
        assert_eq!(key, "not-whole-number-in-range");
        assert_eq!(
            args.iter().map(String::as_str).collect::<Vec<_>>(),
            vec!["Threads", &(CPUS + 1).to_string(), "1", &CPUS.to_string()],
            "the refusal's bounds are the check's own"
        );
        assert_eq!(field, "Threads");
    }

    /// The shapes that cannot name a setting, each with the body's own
    /// `revision` so the refusal is about the shape and not about a missing
    /// member: no `values`, a `values` that is a string, a `values` that is an
    /// array, and a field value that is an object. Every one is malformed, and
    /// the route refuses them before any manager call.
    #[test]
    fn body_to_file_refuses_a_body_that_cannot_name_a_setting() {
        assert_malformed(json!({ "revision": REV }), "no values member");
        assert_malformed(
            json!({ "revision": REV, "values": "Threads = 4" }),
            "values not an object",
        );
        assert_malformed(
            json!({ "revision": REV, "values": [{ "Threads": 4 }] }),
            "values an array",
        );
        assert_malformed(
            json!({ "revision": REV, "values": { "Threads": { "nested": 4 } } }),
            "a value that is an object",
        );
        assert_malformed(
            json!({ "revision": 4, "values": { "Threads": 4 } }),
            "a revision that is no string",
        );
    }

    /// A key the bff has no rule for is not a setting it writes: the file is
    /// rendered from the known fields alone, so an extra key, of any name and
    /// any type, leaves the file byte for byte what the same body without it
    /// renders. The manager only ever sees the rendered text.
    #[test]
    fn body_to_file_writes_nothing_for_a_key_that_is_not_a_known_setting() {
        let plain = body_of(&rich_cfg());
        let want = file(plain.clone());
        for (key, value) in [
            ("Threads of the future", json!(4)),
            ("", json!(4)),
            ("  Threads", json!(4)),
            ("threads", json!(4)),
            ("ModelDir", json!({ "nested": 4 })),
        ] {
            let mut body = plain.clone();
            values_of(&mut body).insert(key.to_string(), value);
            assert_eq!(file(body), want, "the key {key:?} writes nothing");
        }
    }

    /// A number that is not the whole number the read gives is malformed, and
    /// so is a float that is a whole number: the read sends an integer for a
    /// numeric key, never a float, and the body sends back what it read. The
    /// whole number itself renders.
    #[test]
    fn body_to_file_refuses_a_number_that_is_not_whole_for_an_integer_setting() {
        assert_malformed(body_with("Threads", json!(1.5)), "a fraction");
        assert_malformed(body_with("Threads", json!(4.0)), "a whole float");
        let text = file(body_with("Threads", json!(4)));
        assert!(text.contains("Threads = 4"), "{text}");
    }

    /// `null` clears a setting, and the blank and the all-blank string clear
    /// it the same way: no line of the file starts with the key (the hint of
    /// an unset one is a comment, and so starts with `;`).
    #[test]
    fn body_to_file_takes_the_blank_string_as_a_clear() {
        for blank in [Value::Null, json!(""), json!("   ")] {
            let text = file(body_with("ModelsDir", blank.clone()));
            assert!(
                !text.lines().any(|l| l.starts_with("ModelsDir")),
                "the blank {blank} leaves ModelsDir out of the file: {text}"
            );
        }
    }

    /// Every field name the web's body uses is a setting the bff knows, and
    /// all of them are there: this is what the route hands the manager for the
    /// manager's own check, and what `field_of` reads back, so a rename on one
    /// side is a break on both. The names are compared as a set: the order of
    /// a JSON object here is the bff's map's (the read's `values` and the
    /// body's are both sorted by it) and the form reads by name. Which names
    /// the read sends is pinned on the read side by
    /// `unset_keys_are_null_in_the_shipped_file` in `tests/server.rs`.
    #[test]
    fn the_values_keys_are_the_web_s_field_names() {
        let body = body_of(&rich_cfg());
        let values = body["values"]
            .as_object()
            .expect("the body carries an object");
        let mut keys: Vec<&str> = values.keys().map(String::as_str).collect();
        keys.sort();
        assert_eq!(
            keys,
            vec![
                "AttnRotDisable",
                "CudaAllreduce",
                "CudaDisableGraphs",
                "Hostname",
                "LogVerbosity",
                "ModelsDir",
                "ModelsMax",
                "ModelsMemoryMargin",
                "OpencodeApiKey",
                "OpencodeBaseUrl",
                "Port",
                "RocblasUseHipblaslt",
                "RocprofAttach",
                "SaveStateOnShutdown",
                "StateDir",
                "Tags",
                "Threads",
                "ThreadsBatch",
                "Webui",
                "WebuiMcpProxy",
            ]
        );
    }
}
