//! The instance's own writes: `POST /api/instances`,
//! `DELETE /api/instances/{id}`, `PUT /api/instances/{id}/profile` and
//! `PUT /api/instances/{id}/address`, the four writes beside the list read
//! (`lib.rs` answers `GET /api/instances`).
//!
//! All four write served files, so all four sit behind a session and act for
//! the signed-in user: the session's user is the principal of the connect and
//! the manager re-checks their membership of `pai` at every call, a refusal of
//! them being 403 `session-not-administrator` and ending the session that
//! acted, as on the server profile pool's create (`server_profiles.rs`). Each
//! reads the session with `Sessions::current`, so the call moves the idle
//! deadline. The manager's calls are given `write_timeout`: a create runs the
//! address check of the instance profile it binds, which resolves a host name.
//!
//! The manager holds every rule, so the bff reads only the shape of the body:
//! three strings for the create, one for the profile switch, two for the
//! address, a body it cannot read being 400 with the manager not connected to.
//! The create's retired key is one of those bodies: a body naming
//! [`SERVER_PROFILE`] at all is refused, never read as the instance profile that
//! supersedes it. What the manager answers keeps its key and its args in
//! `{"key": <key>, "args": [...]}`, never the manager's detail, and `field`
//! names the control the form marks when the refusal is of one of the create's
//! body's values.
//!
//! The create takes the instance profile the record binds and answers
//! 201 `{"id", "port"}`: the manager picks the address from the range, so the
//! page shows it with no second read, the instance being created inactive.
//!
//! The profile switch and the address write carry no revision on the wire:
//! the bff reads the text the manager compares and hands it over, so the
//! manager's compare closes the window between the page reading and writing
//! (IP4, `ARCHITECTURE.md`, `fe/briefs/api-contract.md`). The profile switch
//! hands the instance profile pool's text, read with the one verb that answers
//! one profile (`instance_profile`, as on the pool's own routes,
//! `instance_pool.rs`), and reads no file: the id of the path is checked
//! against the record of the instances the same connect answers. The address
//! write hands the `instances.ini` text it read. Both answer the affected
//! answer, `{"running": [ids], "notConverged": [{instance, key, args}]}`, read
//! off the manager's `Affected` ([`crate::instance_pool::affected_of`]).

use crate::instance_pool::affected_of;
use crate::origin::body_refusal;
use crate::server::{timeout_answer, WriteState};
use crate::session::{error, json_body, Sessions};
use crate::Connect;
use axum::extract::rejection::BytesRejection;
use axum::extract::Path;
use axum::http::{HeaderMap, StatusCode};
use axum::response::{IntoResponse, Response};
use axum::Router;
use pai_core::checks::Refusal;
use pai_core::client::{Affected, ClientError, ManagerClient};
use serde_json::{json, Value};
use std::sync::{Arc, Mutex};
use std::time::Duration;

/// The record of the instances, whose text the address write hands the manager
/// as the revision it compares.
const INSTANCES: &str = "instances.ini";

/// The id no remove takes away, the one the engine's install writes and every
/// host has: the same id the manager protects (`service.rs`).
const PROTECTED_INSTANCE: &str = "default";

/// The key the retired create named, before an instance was bound by an
/// instance profile: a create body that still carries it is refused, never read
/// as the instance profile that supersedes it (IP4, `ARCHITECTURE.md`).
const SERVER_PROFILE: &str = "serverProfile";

/// The instance create and removal, its profile switch and its address write,
/// over the same seams as the pool's: all four writes bound by `write_timeout`.
/// A body is capped by the one layer in front of the whole router (`lib.rs`).
pub(crate) fn routes(connect: Connect, write_timeout: Duration, sessions: Arc<Sessions>) -> Router {
    let create = {
        let (connect, sessions) = (connect.clone(), sessions.clone());
        move |headers: HeaderMap, body: Result<axum::body::Bytes, BytesRejection>| {
            create(
                connect.clone(),
                write_timeout,
                sessions.clone(),
                headers,
                body,
            )
        }
    };
    let remove = {
        let (connect, sessions) = (connect.clone(), sessions.clone());
        move |Path(id): Path<String>, headers: HeaderMap| {
            remove(
                connect.clone(),
                write_timeout,
                sessions.clone(),
                id,
                headers,
            )
        }
    };
    let set_profile = {
        let (connect, sessions) = (connect.clone(), sessions.clone());
        move |Path(id): Path<String>,
              headers: HeaderMap,
              body: Result<axum::body::Bytes, BytesRejection>| {
            set_profile(
                connect.clone(),
                write_timeout,
                sessions.clone(),
                id,
                headers,
                body,
            )
        }
    };
    let set_address = {
        let (connect, sessions) = (connect.clone(), sessions.clone());
        move |Path(id): Path<String>,
              headers: HeaderMap,
              body: Result<axum::body::Bytes, BytesRejection>| {
            set_address(
                connect.clone(),
                write_timeout,
                sessions.clone(),
                id,
                headers,
                body,
            )
        }
    };
    Router::new()
        .route("/api/instances", axum::routing::post(create))
        .route("/api/instances/{id}", axum::routing::delete(remove))
        .route(
            "/api/instances/{id}/profile",
            axum::routing::put(set_profile),
        )
        .route(
            "/api/instances/{id}/address",
            axum::routing::put(set_address),
        )
}

/// `POST /api/instances` `{"engine", "id", "instanceProfile"}`: creates the
/// instance and answers 201 `{"id", "port"}` with the id the body sent and the
/// port the manager picked from the range, so the page shows the address with no
/// second read. A body that still names [`SERVER_PROFILE`] is one it cannot
/// read: 400 `instance-malformed`, the manager not connected to. Every rule is
/// the manager's: its refusal of the engine, of the
/// id, of a name taken, of an instance profile that is not there, and of a node
/// with no address left keeps its key in the body ([`write_failure`]). The time
/// limit answers 503 `client-unavailable` while the call was never handed over
/// the manager, and 504 `client-timeout` once it was, the create's outcome
/// unknown, as on the server file's save.
async fn create(
    connect: Connect,
    write_timeout: Duration,
    sessions: Arc<Sessions>,
    headers: HeaderMap,
    body: Result<axum::body::Bytes, BytesRejection>,
) -> Response {
    let Some((session, identity)) = sessions.current(&headers) else {
        return error(StatusCode::UNAUTHORIZED, "session-required", &[]);
    };
    let body = match body {
        Ok(body) => body,
        Err(rejection) => return body_refusal(&rejection, "instance-malformed"),
    };
    let Some(requested) = json_body(&headers, &body).as_ref().and_then(requested) else {
        return error(StatusCode::BAD_REQUEST, "instance-malformed", &[]);
    };
    let user = identity.user;
    let answer = requested.id.clone();
    // The port the manager picked, carried out of the task so a limit that runs
    // out once the create landed answers the address it gave, as the pool's
    // entry save carries its revision out (`instance_pool.rs`).
    let answered = Arc::new(Mutex::new(0u16));
    let picked = answered.clone();
    let state = Arc::new(WriteState::new());
    let mine = state.clone();
    let ask = tokio::task::spawn_blocking(move || -> Result<u16, ClientError> {
        let client = connect(Some(user.as_str()))?;
        // Lost to a handler that already ran out of time, no call is sent: the
        // 503 it answered stands and no instance is created.
        if !mine.may_write() {
            return Ok(0);
        }
        let port = client.create_instance(
            &requested.engine,
            &requested.id,
            &requested.instance_profile,
        )?;
        mine.landed();
        *picked.lock().unwrap_or_else(|e| e.into_inner()) = port;
        Ok(port)
    });
    match tokio::time::timeout(write_timeout, ask).await {
        Ok(Ok(Ok(port))) => created(&answer, port),
        Ok(Ok(Err(failure))) => write_failure(&sessions, &session, failure, state.sent()),
        Err(_) => timeout_answer(
            state.timed_out(),
            created(&answer, *answered.lock().unwrap_or_else(|e| e.into_inner())),
        ),
        Ok(Err(_)) => error(StatusCode::BAD_GATEWAY, "client-other", &[]),
    }
}

/// 201 with the id the create took and the port the manager gave it.
fn created(id: &str, port: u16) -> Response {
    (
        StatusCode::CREATED,
        axum::Json(json!({ "id": id, "port": port })),
    )
        .into_response()
}

/// What a create asks for: the engine, the id and the instance profile to bind,
/// each its own string. Any other shape, a body naming none of the three, and a
/// body that still carries [`SERVER_PROFILE`], are None: the page sends the
/// three words of its form, and the manager is the one that judges them.
struct Requested {
    engine: String,
    id: String,
    instance_profile: String,
}

fn requested(body: &Value) -> Option<Requested> {
    let top = body.as_object()?;
    if top.contains_key(SERVER_PROFILE) {
        return None;
    }
    Some(Requested {
        engine: top.get("engine")?.as_str()?.to_string(),
        id: top.get("id")?.as_str()?.to_string(),
        instance_profile: top.get("instanceProfile")?.as_str()?.to_string(),
    })
}

/// `DELETE /api/instances/{id}`: 204 with no body once the manager took the
/// instance away. The protected id and a running one are 409 naming the id, an
/// id the record does not name is 404 `client-no-instance`, and the service
/// manager's own refusal is 502 `client-service-failed`, its message staying in
/// the bff. The time limit answers 503 while the call was never handed over the
/// manager, and 504 `client-timeout` once it was, as on the create.
async fn remove(
    connect: Connect,
    write_timeout: Duration,
    sessions: Arc<Sessions>,
    id: String,
    headers: HeaderMap,
) -> Response {
    let Some((session, identity)) = sessions.current(&headers) else {
        return error(StatusCode::UNAUTHORIZED, "session-required", &[]);
    };
    let user = identity.user;
    let state = Arc::new(WriteState::new());
    let mine = state.clone();
    let ask = tokio::task::spawn_blocking(move || -> Result<(), ClientError> {
        let client = connect(Some(user.as_str()))?;
        if !mine.may_write() {
            return Ok(());
        }
        client.remove_instance(&id)?;
        mine.landed();
        Ok(())
    });
    match tokio::time::timeout(write_timeout, ask).await {
        Ok(Ok(Ok(()))) => StatusCode::NO_CONTENT.into_response(),
        Ok(Ok(Err(failure))) => write_failure(&sessions, &session, failure, state.sent()),
        Err(_) => timeout_answer(state.timed_out(), StatusCode::NO_CONTENT.into_response()),
        Ok(Err(_)) => error(StatusCode::BAD_GATEWAY, "client-other", &[]),
    }
}

/// `PUT /api/instances/{id}/profile` `{"instanceProfile": <name>}`: the
/// instance profile the record binds, as the signed-in user. The session and
/// the body answer first, so a body nothing can be made of (400
/// `profile-malformed`) reaches no manager. The record of the instances answers
/// the id next: an id it does not name is 404 with nothing written. The
/// revision is the pool's text read with the one verb that answers one profile,
/// handed over so the manager's compare covers the window the read leaves; a
/// profile the pool has no section for is 422 `no-instance-profile-named`, with
/// no write asked.
async fn set_profile(
    connect: Connect,
    write_timeout: Duration,
    sessions: Arc<Sessions>,
    id: String,
    headers: HeaderMap,
    body: Result<axum::body::Bytes, BytesRejection>,
) -> Response {
    let Some((session, identity)) = sessions.current(&headers) else {
        return error(StatusCode::UNAUTHORIZED, "session-required", &[]);
    };
    let body = match body {
        Ok(body) => body,
        Err(rejection) => return body_refusal(&rejection, "profile-malformed"),
    };
    let Some(profile) = one_string(&json_body(&headers, &body), "instanceProfile") else {
        return error(StatusCode::BAD_REQUEST, "profile-malformed", &[]);
    };
    let text_profile = profile.clone();
    write_affected(
        connect,
        write_timeout,
        &sessions,
        &session,
        identity.user,
        id,
        move |client| Ok(client.instance_profile(&text_profile)?.revision),
        move |client, id, text| client.set_instance_profile(id, &profile, text),
    )
    .await
}

/// `PUT /api/instances/{id}/address` `{"hostname", "port"}`: the address of the
/// record's section, as the signed-in user. Both words are the manager's
/// strings, a `null` hostname being the empty host that takes the key away. The
/// session and the body answer first (400 `address-malformed`), the record of
/// the instances answers the id next, and the revision is the `instances.ini`
/// text read here, handed over so the manager's compare covers the record it is
/// about to change. The port's range and a host that resolves are the manager's:
/// its refusals are 422 `invalid-port` and 422 `invalid-hostname`.
async fn set_address(
    connect: Connect,
    write_timeout: Duration,
    sessions: Arc<Sessions>,
    id: String,
    headers: HeaderMap,
    body: Result<axum::body::Bytes, BytesRejection>,
) -> Response {
    let Some((session, identity)) = sessions.current(&headers) else {
        return error(StatusCode::UNAUTHORIZED, "session-required", &[]);
    };
    let body = match body {
        Ok(body) => body,
        Err(rejection) => return body_refusal(&rejection, "address-malformed"),
    };
    let Some((host, port)) = address_of(&json_body(&headers, &body)) else {
        return error(StatusCode::BAD_REQUEST, "address-malformed", &[]);
    };
    write_affected(
        connect,
        write_timeout,
        &sessions,
        &session,
        identity.user,
        id,
        |client| {
            client
                .read_file(INSTANCES)
                .map(|file| file.map(|file| file.text).unwrap_or_default())
        },
        move |client, id, text| client.set_instance_address(id, &host, &port, text),
    )
    .await
}

/// One of the two writes that carry no revision of their own, as the signed-in
/// user under `write_timeout`: the record of the instances answers the id first,
/// matched without case, an id it does not name being 404 `client-no-instance`
/// with nothing written, and the record's own spelling of the id is what the
/// manager is handed; then `text` reads the text the manager compares, and
/// `write` hands it over with the write, the manager's compare closing the
/// window between this read and the write. The answer is the affected answer.
/// The shared [`WriteState`] settles with the task whether the call was ever
/// sent: a limit that runs out
/// first leaves the task sending nothing and answers 503, and a limit that runs
/// out once the call is sent answers 504, its outcome unknown, as on the create.
#[allow(clippy::too_many_arguments)]
async fn write_affected(
    connect: Connect,
    write_timeout: Duration,
    sessions: &Arc<Sessions>,
    session: &str,
    user: String,
    id: String,
    text: impl FnOnce(&dyn ManagerClient) -> Result<String, ClientError> + Send + 'static,
    write: impl FnOnce(&dyn ManagerClient, &str, &str) -> Result<Affected, ClientError> + Send + 'static,
) -> Response {
    let state = Arc::new(WriteState::new());
    let mine = state.clone();
    let ask = tokio::task::spawn_blocking(move || -> Result<Value, ClientError> {
        let client = connect(Some(user.as_str()))?;
        // The record's spelling of the id, not the path's: the manager's verbs
        // read `instances.ini` by the name it wrote, and a call carrying the
        // path's spelling acts on no instance.
        let Some(known) = client
            .list_instances()?
            .into_iter()
            .find(|known| known.eq_ignore_ascii_case(&id))
        else {
            return Err(ClientError::NoInstance);
        };
        let text = text(client.as_ref())?;
        // Lost to a handler that already ran out of time, no call is sent: the
        // 503 it answered stands and nothing is written.
        if !mine.may_write() {
            return Ok(affected_of(&Affected::default()));
        }
        let touched = write(client.as_ref(), &known, &text)?;
        mine.landed();
        Ok(affected_of(&touched))
    });
    match tokio::time::timeout(write_timeout, ask).await {
        Ok(Ok(Ok(doc))) => json_response(&doc),
        Ok(Ok(Err(failure))) => write_failure(sessions, session, failure, state.sent()),
        Err(_) => timeout_answer(
            state.timed_out(),
            json_response(&affected_of(&Affected::default())),
        ),
        Ok(Err(_)) => error(StatusCode::BAD_GATEWAY, "client-other", &[]),
    }
}

/// The address body's two words, as the manager takes them: the host and the
/// port, both strings, a `null` hostname being the empty host the manager's INI
/// method removes the key with. A body that is not an object, a missing key, a
/// port that is not a string, and a hostname that is neither a string nor
/// `null`, are all none.
fn address_of(body: &Option<Value>) -> Option<(String, String)> {
    let top = body.as_ref()?.as_object()?;
    let host = match top.get("hostname")? {
        Value::Null => String::new(),
        Value::String(host) => host.clone(),
        _ => return None,
    };
    Some((host, top.get("port")?.as_str()?.to_string()))
}

/// The one string of a body under `key`: a body that is not an object, a
/// missing key and a value that is not a string are all none.
fn one_string(body: &Option<Value>, key: &str) -> Option<String> {
    body.as_ref()?
        .get(key)
        .and_then(Value::as_str)
        .map(str::to_string)
}

/// 200 with `doc`.
fn json_response(doc: &Value) -> Response {
    (StatusCode::OK, axum::Json(doc.clone())).into_response()
}

/// `removable` of one row of the list (`lib.rs`): the page offers a remove of
/// every instance except [`PROTECTED_INSTANCE`], whatever its state (whether it
/// runs is `state`, and the removal refuses a running one anyway).
pub(crate) fn removable(id: &str) -> bool {
    id != PROTECTED_INSTANCE
}

/// The answer to one of these writes the manager could not do. A content check
/// it ran keeps its key and its values ([`refusal_answer`]). No server profile
/// of that name: 404 naming it; one another instance runs: 409 naming the
/// profile and that instance; an instance profile the pool has no section for:
/// 422 `no-instance-profile-named` naming it; an address another instance holds:
/// 409 `address-in-use` naming that instance; a host that does not resolve: 422
/// `invalid-hostname`; the file the write compares moved under it: 409
/// `stale-revision`. No such instance, and an id the rule refuses, 404
/// `client-no-instance`; the service manager's refusal 502
/// `client-service-failed`, its message out of the body. The manager refused the
/// signed-in user: 403, and the session ends with the right that authorized it.
/// `Unavailable` (the pipe and the bus answer a broken stream or a missing reply
/// with it) is 504 `client-timeout` when `write_asked` says the shared state
/// reached `WRITE_ASKED`, the call sent and its outcome unknown, and 503
/// `client-unavailable` when it stayed Idle: the connect never handed anything
/// over. The rest are the bff's news, and leave the session alive.
fn write_failure(
    sessions: &Arc<Sessions>,
    session: &str,
    failure: ClientError,
    write_asked: bool,
) -> Response {
    match failure {
        ClientError::Invalid(refusal) => refusal_answer(refusal),
        ClientError::NoServerProfile(profile) => keyed(
            StatusCode::NOT_FOUND,
            "client-no-server-profile",
            &[profile.as_str()],
        ),
        ClientError::ServerProfileInUse(profile, instance) => keyed(
            StatusCode::CONFLICT,
            "client-server-profile-in-use",
            &[profile.as_str(), instance.as_str()],
        ),
        ClientError::NoInstanceProfile(profile) => keyed(
            StatusCode::UNPROCESSABLE_ENTITY,
            "no-instance-profile-named",
            &[profile.as_str()],
        ),
        ClientError::AddressInUse(instance) => {
            keyed(StatusCode::CONFLICT, "address-in-use", &[instance.as_str()])
        }
        ClientError::UnresolvableHost(host) => keyed(
            StatusCode::UNPROCESSABLE_ENTITY,
            "invalid-hostname",
            &[host.as_str()],
        ),
        ClientError::Changed => keyed(StatusCode::CONFLICT, "stale-revision", &[]),
        ClientError::NoInstance | ClientError::InvalidId => {
            error(StatusCode::NOT_FOUND, "client-no-instance", &[])
        }
        ClientError::Service(_) => error(StatusCode::BAD_GATEWAY, "client-service-failed", &[]),
        ClientError::NotAuthorized => {
            sessions.end(session);
            error(StatusCode::FORBIDDEN, "session-not-administrator", &[])
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

/// The answer to one of the manager's refusals: 422 with `field` for the two
/// words of the create's body the form marks (the engine and the id), 409 for
/// the three rules about the id itself (its name is taken, it is the protected
/// one, it runs), 503 `no-free-port` when the node has no address left for the
/// create, 422 `invalid-port` for the manager's range rule (the key the contract
/// names, the manager's values kept), and 422 with the key and its values alone
/// for any other check the manager ran, the page having no control of the form
/// to mark.
fn refusal_answer(refusal: Refusal) -> Response {
    let args = refusal
        .values
        .iter()
        .map(String::as_str)
        .collect::<Vec<&str>>();
    match refusal.key {
        "unknown-engine" => refused(refusal.key, &refusal.values, "engine"),
        "instance-bad-id" => refused(refusal.key, &refusal.values, "id"),
        "instance-exists" | "instance-protected" | "instance-running" => {
            keyed(StatusCode::CONFLICT, refusal.key, &args)
        }
        "no-free-port" => keyed(StatusCode::SERVICE_UNAVAILABLE, refusal.key, &args),
        "not-whole-number-in-range" => {
            keyed(StatusCode::UNPROCESSABLE_ENTITY, "invalid-port", &args)
        }
        _ => (
            StatusCode::UNPROCESSABLE_ENTITY,
            axum::Json(json!({ "key": refusal.key, "args": refusal.values })),
        )
            .into_response(),
    }
}

/// 422 with the check's key, its values and `field`, the control the form marks.
fn refused(key: &str, args: &[String], field: &str) -> Response {
    (
        StatusCode::UNPROCESSABLE_ENTITY,
        axum::Json(json!({ "key": key, "args": args, "field": field })),
    )
        .into_response()
}

/// `{"key": <key>, "args": [...]}` with `status`.
fn keyed(status: StatusCode, key: &str, args: &[&str]) -> Response {
    (status, axum::Json(json!({ "key": key, "args": args }))).into_response()
}
