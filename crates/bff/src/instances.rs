//! Creating and removing an instance: `POST /api/instances` and
//! `DELETE /api/instances/{id}`, the two writes beside the list read
//! (`lib.rs` answers `GET /api/instances`).
//!
//! Both write served files, so both sit behind a session and act for the
//! signed-in user: the session's user is the principal of the connect and the
//! manager re-checks their membership of `pai` at every call, a refusal of them
//! being 403 `session-not-administrator` and ending the session that acted, as
//! on the server profile pool's create (`server_profiles.rs`). Each reads the
//! session with `Sessions::current`, so the call moves the idle deadline. The
//! manager's calls are given `write_timeout`: a create runs the address check
//! of the server profile it binds, which resolves a host name.
//!
//! The manager holds every rule, so the bff reads only the shape of the body:
//! three strings, a body it cannot read being 400 `instance-malformed` with the
//! manager not connected to. What the manager answers keeps its key and its
//! args in `{"key": <key>, "args": [...]}`, never the manager's detail, and
//! `field` names the control the form marks when the refusal is of one of the
//! body's values.

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
use pai_core::client::ClientError;
use serde_json::{json, Value};
use std::sync::Arc;
use std::time::Duration;

/// The id no remove takes away, the one the engine's install writes and every
/// host has: the same id the manager protects (`service.rs`).
const PROTECTED_INSTANCE: &str = "default";

/// The instance create and removal, over the same seams as the pool's: both
/// writes bound by `write_timeout`. A body is capped by the one layer in front
/// of the whole router (`lib.rs`).
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
    Router::new()
        .route("/api/instances", axum::routing::post(create))
        .route("/api/instances/{id}", axum::routing::delete(remove))
}

/// `POST /api/instances` `{"engine", "id", "serverProfile"}`: creates the
/// instance and answers 201 `{"id"}` with the id the body sent. Every rule is
/// the manager's: its refusal of the engine, of the id, of a name taken, of a
/// server profile that is not there, of one another instance runs, and of an
/// address in use keeps its key in the body ([`write_failure`]). The time limit
/// answers 503 `client-unavailable` while the call was never handed over the
/// manager, and 504 `client-timeout` once it was, the create's outcome unknown,
/// as on the server file's save.
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
    let state = Arc::new(WriteState::new());
    let mine = state.clone();
    let ask = tokio::task::spawn_blocking(move || -> Result<(), ClientError> {
        let client = connect(Some(user.as_str()))?;
        // Lost to a handler that already ran out of time, no call is sent: the
        // 503 it answered stands and no instance is created.
        if !mine.may_write() {
            return Ok(());
        }
        // IP4b: the create takes the instance profile and answers the port the
        // manager proposed. The route's body and its answer are IP4d's work:
        // today it names a server profile and answers the id alone, so the port
        // is dropped here.
        client.create_instance(&requested.engine, &requested.id, &requested.server_profile)?;
        mine.landed();
        Ok(())
    });
    match tokio::time::timeout(write_timeout, ask).await {
        Ok(Ok(Ok(()))) => {
            (StatusCode::CREATED, axum::Json(json!({ "id": answer }))).into_response()
        }
        Ok(Ok(Err(failure))) => write_failure(&sessions, &session, failure, state.sent()),
        Err(_) => timeout_answer(
            state.timed_out(),
            (StatusCode::CREATED, axum::Json(json!({ "id": answer }))).into_response(),
        ),
        Ok(Err(_)) => error(StatusCode::BAD_GATEWAY, "client-other", &[]),
    }
}

/// What a create asks for: the engine, the id and the server profile to bind,
/// each its own string. Any other shape, and a body naming none of the three,
/// is None: the page sends the three words of its form, and the manager is the
/// one that judges them.
struct Requested {
    engine: String,
    id: String,
    server_profile: String,
}

fn requested(body: &Value) -> Option<Requested> {
    let top = body.as_object()?;
    Some(Requested {
        engine: top.get("engine")?.as_str()?.to_string(),
        id: top.get("id")?.as_str()?.to_string(),
        server_profile: top.get("serverProfile")?.as_str()?.to_string(),
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

/// `removable` of one row of the list (`lib.rs`): the page offers a remove of
/// every instance except [`PROTECTED_INSTANCE`], whatever its state (whether it
/// runs is `state`, and the removal refuses a running one anyway).
pub(crate) fn removable(id: &str) -> bool {
    id != PROTECTED_INSTANCE
}

/// The answer to a create or a removal the manager could not do. A content
/// check it ran keeps its key and its values ([`refusal_answer`]). No server
/// profile of that name: 404 naming it; one another instance runs: 409 naming
/// the profile and that instance; an address in use and a host that does not
/// resolve: 422 marking the form's server profile control. No such instance, and
/// an id the rule refuses, 404 `client-no-instance`; the service manager's
/// refusal 502 `client-service-failed`, its message out of the body. The manager
/// refused the signed-in user: 403, and the session ends with the right that
/// authorized it. `Unavailable` (the pipe and the bus answer a broken stream or a
/// missing reply with it) is 504 `client-timeout` when `write_asked` says the
/// shared state reached `WRITE_ASKED`, the call sent and its outcome unknown, and
/// 503 `client-unavailable` when it stayed Idle: the connect never handed
/// anything over. The rest are the bff's news, and leave the session alive.
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
        ClientError::AddressInUse(instance) => refused(
            "client-address-in-use",
            std::slice::from_ref(&instance),
            "serverProfile",
        ),
        ClientError::UnresolvableHost(host) => refused(
            "client-unresolvable-host",
            std::slice::from_ref(&host),
            "serverProfile",
        ),
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
/// one, it runs), and 422 with the key and its values alone for any other check
/// the manager ran, the page having no control of the form to mark.
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
