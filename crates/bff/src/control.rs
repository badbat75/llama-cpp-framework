//! The control routes: `POST /api/instances/{id}/start`, `/stop` and
//! `/restart`, and `GET`/`PUT /api/instances/{id}/boot`, and the instance's
//! tags at `PUT /api/instances/{id}/tags`. The verbs answer 204
//! once the manager's job is accepted: the state right after can still be the
//! previous one, so the web UI polls `GET /api/instances` and the reply carries
//! no state. The boot flag is read by a signed-in user, as its write is, and
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
//! detail: the service manager's message stays in the bff's log.

use crate::origin::body_refusal;
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
/// one layer in front of the whole router (`lib.rs`).
pub(crate) fn routes(connect: Connect, timeout: Duration, sessions: Arc<Sessions>) -> axum::Router {
    let verb = |v: Verb| {
        let (connect, sessions) = (connect.clone(), sessions.clone());
        move |Path(id): Path<String>, headers: HeaderMap| {
            act(connect.clone(), timeout, sessions.clone(), v, id, headers)
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
                timeout,
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
                timeout,
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
/// connected to.
async fn act(
    connect: Connect,
    timeout: Duration,
    sessions: Arc<Sessions>,
    verb: Verb,
    id: String,
    headers: HeaderMap,
) -> Response {
    let Some((session, identity)) = sessions.current(&headers) else {
        return error(StatusCode::UNAUTHORIZED, "session-required", &[]);
    };
    let user = identity.user;
    let ask = tokio::task::spawn_blocking(move || verb(&connect(Some(user.as_str()))?, &id));
    match tokio::time::timeout(timeout, ask).await {
        Ok(Ok(Ok(()))) => StatusCode::NO_CONTENT.into_response(),
        Ok(Ok(Err(failure))) => action_failure(&sessions, &session, failure),
        Err(_) => error(StatusCode::SERVICE_UNAVAILABLE, "client-unavailable", &[]),
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
/// as its second step, before the manager is connected to.
async fn boot_write(
    connect: Connect,
    timeout: Duration,
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
    let ask = tokio::task::spawn_blocking(move || {
        let client = connect(Some(user.as_str()))?;
        client.set_boot_enabled(&id, enabled)?;
        client.boot_enabled(&id)
    });
    match tokio::time::timeout(timeout, ask).await {
        Ok(Ok(Ok(enabled))) => axum::Json(json!({ "enabled": enabled })).into_response(),
        Ok(Ok(Err(failure))) => action_failure(&sessions, &session, failure),
        Err(_) => error(StatusCode::SERVICE_UNAVAILABLE, "client-unavailable", &[]),
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
/// its second step, before the manager is connected to.
async fn tags_write(
    connect: Connect,
    timeout: Duration,
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
    let ask = tokio::task::spawn_blocking(move || {
        connect(Some(user.as_str()))?.set_instance_tags(&id, &tags)
    });
    match tokio::time::timeout(timeout, ask).await {
        Ok(Ok(Ok(()))) => StatusCode::NO_CONTENT.into_response(),
        Ok(Ok(Err(failure))) => tags_failure(&sessions, &session, failure),
        Err(_) => error(StatusCode::SERVICE_UNAVAILABLE, "client-unavailable", &[]),
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
/// ([`action_failure`]), plus the refusal of the list: 422 with the check's
/// key, its values and `field` "tags", the manager's own message left out of
/// the body and the session left alive.
fn tags_failure(sessions: &Arc<Sessions>, session: &str, failure: ClientError) -> Response {
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
    action_failure(sessions, session, failure)
}

/// The answer to a verb or a boot write that failed: 404 for no such instance
/// or an id the manager refuses, 503 when there is no manager, 502
/// `client-service-failed` when the service manager refused the job (its
/// message stays out of the body) and 502 `client-other` for anything else.
/// The manager refused the signed-in user: 403
/// `session-not-administrator` (the sign-in's key for the same fact), and the
/// session that acted ends with the right that authorized it. The other
/// answers are the bff's news, not a refusal of the user, so the session
/// lives on.
fn action_failure(sessions: &Arc<Sessions>, session: &str, failure: ClientError) -> Response {
    match failure {
        ClientError::NotAuthorized => {
            sessions.end(session);
            error(StatusCode::FORBIDDEN, "session-not-administrator", &[])
        }
        ClientError::NoInstance | ClientError::InvalidId => {
            error(StatusCode::NOT_FOUND, "client-no-instance", &[])
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
