//! The control routes: `POST /api/instances/{id}/start`, `/stop` and
//! `/restart`, and `GET`/`PUT /api/instances/{id}/boot`, and the instance's
//! tags at `PUT /api/instances/{id}/tags`. The verbs answer 204
//! once the manager's job is accepted: the state right after can still be the
//! previous one, so the web UI polls `GET /api/instances` and the reply carries
//! no state. A start or a restart of an instance the manager will not give a
//! job over (one with no model profile: `instance-no-model-profiles`) is 409
//! naming the id, and `stop` is never refused, asking for no job. The boot flag is read by a signed-in user, as its write is, and
//! the tags are written by one.
//!
//! The verbs, the boot read, the boot write and the tags write sit behind a
//! session. The verbs, the write and the tags
//! act for the signed-in user: the session's user is the
//! principal of the connect, taken from the session alone, and the manager
//! re-checks their right at every call. A refusal is 403 and ends the session
//! that acted, as the server read does (`server.rs`). `GET .../boot` reads the
//! session to gate the read and then connects as the bff's own account, leaving
//! the idle deadline where it was, which is why the page can poll it: the
//! refusal of that connect is of the bff's account, not of the signed-in user,
//! so it ends no session.
//!
//! Every failure is `{"key": <strings key>, "args": []}`, never the manager's
//! detail: the service manager's message stays in the bff's log. The three
//! writes run under the write's limit and the boot read under the read's. A
//! limit that runs out, or a manager that answers `Unavailable` (a broken
//! stream, a missing reply), is 503 `client-unavailable` while the call was
//! never handed over the manager, and 504 `client-timeout` once it was, the
//! write's outcome unknown (`server::WriteState`, the server file's save's
//! rule). The reads keep the 503.

use crate::origin::body_refusal;
use crate::server::{timeout_answer, WriteState};
use crate::session::{error, json_body, Sessions};
use crate::{Client, Connect};
use axum::extract::rejection::BytesRejection;
use axum::extract::Path;
use axum::http::{HeaderMap, StatusCode};
use axum::response::{IntoResponse, Response};
use pai_core::client::ClientError;
use serde_json::{json, Value};
use std::sync::Arc;
use std::time::Duration;

/// One control verb: the call its route makes on the manager.
type Verb = fn(&Client, &str) -> Result<(), ClientError>;

/// The control routes over the seams: the three verbs and the boot flag, read
/// and written, and the tags write, all behind a session. The body of the
/// routes that read one, the boot write and the tags write, is capped by the
/// one layer in front of the whole router (`lib.rs`). The three writes, a boot
/// switch (the unit's enable plus a daemon reload) and a tags write under the
/// manager's write lock among them, run under `write_timeout`; the boot read is
/// a read and keeps `timeout`.
pub(crate) fn routes(
    connect: Connect,
    timeout: Duration,
    write_timeout: Duration,
    sessions: Arc<Sessions>,
) -> axum::Router {
    let verb = |v: Verb| {
        let (connect, sessions) = (connect.clone(), sessions.clone());
        move |Path(id): Path<String>, headers: HeaderMap| {
            act(
                connect.clone(),
                write_timeout,
                sessions.clone(),
                v,
                id,
                headers,
            )
        }
    };
    let boot = {
        let (connect, sessions) = (connect.clone(), sessions.clone());
        move |Path(id): Path<String>, headers: HeaderMap| {
            boot_read(connect.clone(), timeout, sessions.clone(), id, headers)
        }
    };
    let set_boot = {
        let (connect, sessions) = (connect.clone(), sessions.clone());
        move |Path(id): Path<String>,
              headers: HeaderMap,
              body: Result<axum::body::Bytes, BytesRejection>| {
            boot_write(
                connect.clone(),
                write_timeout,
                sessions.clone(),
                id,
                headers,
                body,
            )
        }
    };
    let set_tags = {
        let (connect, sessions) = (connect.clone(), sessions.clone());
        move |Path(id): Path<String>,
              headers: HeaderMap,
              body: Result<axum::body::Bytes, BytesRejection>| {
            tags_write(
                connect.clone(),
                write_timeout,
                sessions.clone(),
                id,
                headers,
                body,
            )
        }
    };
    axum::Router::new()
        .route(
            "/api/instances/{id}/start",
            axum::routing::post(verb(start)),
        )
        .route("/api/instances/{id}/stop", axum::routing::post(verb(stop)))
        .route(
            "/api/instances/{id}/restart",
            axum::routing::post(verb(restart)),
        )
        .route(
            "/api/instances/{id}/boot",
            axum::routing::get(boot).put(set_boot),
        )
        .route("/api/instances/{id}/tags", axum::routing::put(set_tags))
}

fn start(client: &Client, id: &str) -> Result<(), ClientError> {
    client.start(id)
}

fn stop(client: &Client, id: &str) -> Result<(), ClientError> {
    client.stop(id)
}

fn restart(client: &Client, id: &str) -> Result<(), ClientError> {
    client.restart(id)
}

/// `POST /api/instances/{id}/{start,stop,restart}`: 204 with an empty body
/// once the manager took the job. With no session the manager is not even
/// connected to. The verb runs under the write's limit. A time limit answers
/// 503 `client-unavailable` while the verb was never handed over the manager,
/// and 504 `client-timeout` once it was: the job may have been taken, and a 503
/// would tell the page nothing was asked. A manager that takes the verb and
/// whose stream dies leaves that same unknown outcome, so it is answered the
/// same way ([`action_failure`]).
async fn act(
    connect: Connect,
    write_timeout: Duration,
    sessions: Arc<Sessions>,
    verb: Verb,
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
        // Lost to a handler that already ran out of time, no verb is sent: the
        // 503 it answered stands and no job is asked.
        if !mine.may_write() {
            return Ok(());
        }
        verb(&client, &id)?;
        mine.landed();
        Ok(())
    });
    match tokio::time::timeout(write_timeout, ask).await {
        Ok(Ok(Ok(()))) => StatusCode::NO_CONTENT.into_response(),
        Ok(Ok(Err(failure))) => action_failure(&sessions, &session, failure, state.sent()),
        Err(_) => timeout_answer(state.timed_out(), StatusCode::NO_CONTENT.into_response()),
        Ok(Err(_)) => error(StatusCode::BAD_GATEWAY, "client-other", &[]),
    }
}

/// `GET /api/instances/{id}/boot`: 200 `{"enabled"}`, to a signed-in reader:
/// with no live session the answer is 401 `session-required` with no
/// `Set-Cookie`, and the manager is not connected to. The connect is handed no
/// principal: the flag is what the manager answers of the bff's own account.
/// The session is read without refreshing it, so a poll of this route leaves an
/// idle deadline where it was.
async fn boot_read(
    connect: Connect,
    timeout: Duration,
    sessions: Arc<Sessions>,
    id: String,
    headers: HeaderMap,
) -> Response {
    if sessions.peek(&headers).is_none() {
        return error(StatusCode::UNAUTHORIZED, "session-required", &[]);
    }
    let ask = tokio::task::spawn_blocking(move || connect(None)?.boot_enabled(&id));
    match tokio::time::timeout(timeout, ask).await {
        Ok(Ok(Ok(enabled))) => axum::Json(json!({ "enabled": enabled })).into_response(),
        Ok(Ok(Err(failure))) => read_failure(failure),
        Err(_) => error(StatusCode::SERVICE_UNAVAILABLE, "client-unavailable", &[]),
        Ok(Err(_)) => error(StatusCode::BAD_GATEWAY, "client-other", &[]),
    }
}

/// `PUT /api/instances/{id}/boot` `{"enabled": <bool>}`: 200
/// `{"enabled"}`. The write and the read-back that answers are two calls of
/// one connect, and the reply is what the read answered, never what the body
/// asked. A body the bff cannot read is 400 and the manager is not asked; the
/// session check runs first, so with no session such a body is 401. A body
/// over `origin::MAX_BODY` is the extractor's 413, which this handler answers
/// as its second step, before the manager is connected to. The write and the
/// read-back run under the write's limit: a time limit answers 503 while the
/// write was never handed over the manager, 504 `client-timeout` once it was,
/// and 200 with the flag the body asked when the write landed and only the
/// read-back stayed out of reach. A manager that takes the write and whose
/// stream dies leaves that same unknown outcome ([`action_failure`]); a stream
/// that dies on the read-back, after the write landed, is the 503 of a read.
async fn boot_write(
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
        Err(rejection) => return body_refusal(&rejection, "boot-malformed"),
    };
    let Some(enabled) = flag(&headers, &body) else {
        return error(StatusCode::BAD_REQUEST, "boot-malformed", &[]);
    };
    let user = identity.user;
    let state = Arc::new(WriteState::new());
    let mine = state.clone();
    let ask = tokio::task::spawn_blocking(move || -> Result<bool, ClientError> {
        let client = connect(Some(user.as_str()))?;
        if !mine.may_write() {
            // The handler ran out of time before the flag was handed over and
            // answered 503: this value is never read.
            return Ok(enabled);
        }
        client.set_boot_enabled(&id, enabled)?;
        mine.landed();
        client.boot_enabled(&id)
    });
    match tokio::time::timeout(write_timeout, ask).await {
        Ok(Ok(Ok(enabled))) => axum::Json(json!({ "enabled": enabled })).into_response(),
        Ok(Ok(Err(failure))) => action_failure(&sessions, &session, failure, state.sent()),
        // The write landed and only the read-back stayed out of reach: the unit
        // is as the body asked, so the reply carries that flag.
        Err(_) => timeout_answer(
            state.timed_out(),
            axum::Json(json!({ "enabled": enabled })).into_response(),
        ),
        Ok(Err(_)) => error(StatusCode::BAD_GATEWAY, "client-other", &[]),
    }
}

/// The `enabled` flag of a JSON body `{"enabled": <bool>}`, or None: the flag
/// alone, of the JSON type, is a bool.
fn flag(headers: &HeaderMap, body: &[u8]) -> Option<bool> {
    json_body(headers, body)?
        .get("enabled")
        .and_then(Value::as_bool)
}

/// `PUT /api/instances/{id}/tags` `{"tags": [<string>, ...]}`: 204 with no
/// body once the manager wrote the list. The list is the manager's to judge: a
/// list its check refuses is 422 with the check's key, its values and `field`
/// "tags", the field the form marks, and the session of the writer lives on
/// (a refusal of a value is no refusal of the user). A body the bff cannot
/// read is 400 `tags-malformed` and the manager is not asked; the session
/// check runs first, so with no session such a body is 401. A body over
/// `origin::MAX_BODY` is the extractor's 413, which this handler answers as
/// its second step, before the manager is connected to. The write runs under
/// the write's limit: a time limit answers 503 while the write was never handed
/// over the manager and 504 `client-timeout` once it was, the list's outcome
/// unknown. A manager that takes the write and whose stream dies is the same
/// 504 ([`tags_failure`]).
async fn tags_write(
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
        Err(rejection) => return body_refusal(&rejection, "tags-malformed"),
    };
    let Some(tags) = tag_list(&headers, &body) else {
        return error(StatusCode::BAD_REQUEST, "tags-malformed", &[]);
    };
    let user = identity.user;
    let state = Arc::new(WriteState::new());
    let mine = state.clone();
    let ask = tokio::task::spawn_blocking(move || -> Result<(), ClientError> {
        let client = connect(Some(user.as_str()))?;
        if !mine.may_write() {
            return Ok(());
        }
        client.set_instance_tags(&id, &tags)?;
        mine.landed();
        Ok(())
    });
    match tokio::time::timeout(write_timeout, ask).await {
        Ok(Ok(Ok(()))) => StatusCode::NO_CONTENT.into_response(),
        Ok(Ok(Err(failure))) => tags_failure(&sessions, &session, failure, state.sent()),
        Err(_) => timeout_answer(state.timed_out(), StatusCode::NO_CONTENT.into_response()),
        Ok(Err(_)) => error(StatusCode::BAD_GATEWAY, "client-other", &[]),
    }
}

/// The `tags` of a JSON body `{"tags": [<string>, ...]}`, or None: the list
/// alone, every entry a string. An empty list is no tags, and a body of any
/// other shape (no `tags`, a list with a value that is not a string, a
/// document that is not an object, no JSON) is None.
fn tag_list(headers: &HeaderMap, body: &[u8]) -> Option<Vec<String>> {
    json_body(headers, body)?
        .get("tags")?
        .as_array()?
        .iter()
        .map(|tag| tag.as_str().map(str::to_string))
        .collect()
}

/// The answer to a tags write that failed: the verbs' failures
/// ([`action_failure`], `write_asked` the shared state's word that the write
/// was sent), plus the refusal of the list: 422 with the check's
/// key, its values and `field` "tags", the manager's own message left out of
/// the body and the session left alive.
fn tags_failure(
    sessions: &Arc<Sessions>,
    session: &str,
    failure: ClientError,
    write_asked: bool,
) -> Response {
    if let ClientError::Invalid(refusal) = failure {
        return (
            StatusCode::UNPROCESSABLE_ENTITY,
            axum::Json(json!({
                "key": refusal.key,
                "args": refusal.values,
                "field": "tags",
            })),
        )
            .into_response();
    }
    action_failure(sessions, session, failure, write_asked)
}

/// The answer to a verb or a boot write that failed: 404 for no such instance
/// or an id the manager refuses, 409 for a state the manager refuses the job
/// over (a start of an instance with no model profile: `key` and `args` as the
/// manager named them, no `field`, the page having no control of this form to
/// mark), 502 `client-service-failed` when the service manager refused the job
/// (its message stays out of the body) and 502 `client-other` for anything
/// else.
/// The manager refused the signed-in user: 403
/// `session-not-administrator` (the sign-in's key for the same fact), and the
/// session that acted ends with the right that authorized it. The other
/// answers are the bff's news, not a refusal of the user, so the session
/// lives on.
/// `Unavailable` (the pipe and the bus answer a broken stream or a missing
/// reply with it) is 504 `client-timeout` when `write_asked` says the shared
/// state reached `WRITE_ASKED`, the write sent and its outcome unknown, and 503
/// `client-unavailable` when it stayed Idle: the connect never handed anything
/// over. Only a handler that stopped waiting sets Cancelled, so the state a
/// task hands back is Idle or Asked.
fn action_failure(
    sessions: &Arc<Sessions>,
    session: &str,
    failure: ClientError,
    write_asked: bool,
) -> Response {
    match failure {
        ClientError::NotAuthorized => {
            sessions.end(session);
            error(StatusCode::FORBIDDEN, "session-not-administrator", &[])
        }
        ClientError::Invalid(refusal) => (
            StatusCode::CONFLICT,
            axum::Json(json!({ "key": refusal.key, "args": refusal.values })),
        )
            .into_response(),
        ClientError::NoInstance | ClientError::InvalidId => {
            error(StatusCode::NOT_FOUND, "client-no-instance", &[])
        }
        ClientError::Unavailable if write_asked => {
            error(StatusCode::GATEWAY_TIMEOUT, "client-timeout", &[])
        }
        ClientError::Unavailable => {
            error(StatusCode::SERVICE_UNAVAILABLE, "client-unavailable", &[])
        }
        ClientError::Service(_) => error(StatusCode::BAD_GATEWAY, "client-service-failed", &[]),
        _ => error(StatusCode::BAD_GATEWAY, "client-other", &[]),
    }
}

/// The boot read's failures, as on `GET /api/instances`: the manager's
/// refusal of the bff's own account is 502 with its key and ends no session,
/// because the refusal is of the bff's account and not of the signed-in user,
/// and a read is no action. The service manager's failure is not a read's
/// news: the read asks for no job, so it is `client-other`.
fn read_failure(failure: ClientError) -> Response {
    match failure {
        ClientError::NoInstance | ClientError::InvalidId => {
            error(StatusCode::NOT_FOUND, "client-no-instance", &[])
        }
        ClientError::Unavailable => {
            error(StatusCode::SERVICE_UNAVAILABLE, "client-unavailable", &[])
        }
        ClientError::NotAuthorized => error(StatusCode::BAD_GATEWAY, "client-not-authorized", &[]),
        _ => error(StatusCode::BAD_GATEWAY, "client-other", &[]),
    }
}
