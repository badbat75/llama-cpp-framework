//! The server profile pool: `GET`/`POST /api/profiles/server` and
//! `GET`/`PUT`/`DELETE /api/profiles/server/{id}`.
//!
//! A server profile is one `server@<id>.ini` in the engine's config directory:
//! the file an instance reads, named by the profile and not by the instance
//! (`server.rs` reads it through the record, here the pool lists and edits the
//! files themselves). The list is the manager's `ServerProfiles()`, which
//! answers to any caller, so the connect is handed no principal: the session
//! gates the read, it does not choose who it acts for. Create, delete, the
//! document and the save act for the signed-in user, the session's user being
//! the principal of the connect, and the manager re-checks their membership of
//! `pai` at every call: a refusal is 403 `session-not-administrator` and ends
//! the session that acted, as on the instance's Server read (`server.rs`).
//! Every route reads the session with `Sessions::current`, so a page read or a
//! write moves the idle deadline.
//!
//! The engine is the node's one engine ([`crate::ENGINE_LLAMA_CPP`]): a
//! profile's file lives in that engine's directory, so the routes name it, and
//! the id of the path is the profile's own name.
//!
//! The document and the save are today's Server form (`server.rs`) keyed by the
//! profile: `instance` names the instance whose record names the profile, and
//! `defaults.StateDir` is that instance's folder, `null` for a profile no
//! instance runs (the folder belongs to whichever instance runs it). The save
//! answers the manager's affected answer of its write, `running` and
//! `notConverged`, and asks no state of its own.
//!
//! Every failure is `{"key": <strings key>, "args": [...]}`, never the
//! manager's detail: 503 `client-unavailable` when there is no manager or it is
//! too slow (a retried create is then 409 and a retried delete 404, so the
//! ambiguity is visible), 502 `client-other` for anything else, 404
//! `client-no-server-profile` for a name the manager refuses or a file it does
//! not have, and 409 for the two refusals of a write.

use crate::instance_pool::save_answer;
use crate::origin::body_refusal;
use crate::server::{self, timeout_answer, BodyAnswer, Owner, WriteState};
use crate::session::{error, json_body, Sessions};
use crate::{Connect, ENGINE_LLAMA_CPP};
use axum::extract::rejection::BytesRejection;
use axum::extract::Path;
use axum::http::{HeaderMap, StatusCode};
use axum::response::{IntoResponse, Response};
use pai_core::checks;
use pai_core::client::{Affected, ClientError, ManagerClient, ManagerFile, ServerProfileInfo};
use pai_core::paths;
use serde_json::{json, Value};
use std::sync::Arc;
use std::time::Duration;

/// The pool's routes, over the same seams as the instance's Server route: the
/// list and the create, and the document, the save and the delete of one
/// profile. A body is capped by the one layer in front of the whole router
/// (`lib.rs`).
pub(crate) fn routes(
    connect: Connect,
    timeout: Duration,
    write_timeout: Duration,
    sessions: Arc<Sessions>,
    key: [u8; 16],
) -> axum::Router {
    let list = {
        let (connect, sessions) = (connect.clone(), sessions.clone());
        move |headers: HeaderMap| list(connect.clone(), timeout, sessions.clone(), headers)
    };
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
    let read = {
        let (connect, sessions) = (connect.clone(), sessions.clone());
        move |Path(id): Path<String>, headers: HeaderMap| {
            document(connect.clone(), timeout, sessions.clone(), id, headers, key)
        }
    };
    let save = {
        let (connect, sessions) = (connect.clone(), sessions.clone());
        move |Path(id): Path<String>,
              headers: HeaderMap,
              body: Result<axum::body::Bytes, BytesRejection>| {
            save(
                connect.clone(),
                write_timeout,
                sessions.clone(),
                id,
                headers,
                key,
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
    axum::Router::new()
        .route(
            "/api/profiles/server",
            axum::routing::get(list).post(create),
        )
        .route(
            "/api/profiles/server/{id}",
            axum::routing::get(read).put(save).delete(remove),
        )
}

/// `GET /api/profiles/server`: the pool, the manager's rows in its order, each
/// `{id, engine, port, tags, instance}`, `instance` the id of the instance
/// using the profile, `null` when none does.
async fn list(
    connect: Connect,
    timeout: Duration,
    sessions: Arc<Sessions>,
    headers: HeaderMap,
) -> Response {
    let Some((session, _)) = sessions.current(&headers) else {
        return error(StatusCode::UNAUTHORIZED, "session-required", &[]);
    };
    let ask =
        tokio::task::spawn_blocking(move || -> Result<Vec<ServerProfileInfo>, ClientError> {
            connect(None)?.server_profiles()
        });
    match tokio::time::timeout(timeout, ask).await {
        Ok(Ok(Ok(rows))) => axum::Json(rows_json(&rows)).into_response(),
        Ok(Ok(Err(failure))) => read_failure(&sessions, &session, failure),
        Err(_) => error(StatusCode::SERVICE_UNAVAILABLE, "client-unavailable", &[]),
        Ok(Err(_)) => error(StatusCode::BAD_GATEWAY, "client-other", &[]),
    }
}

/// The pool's rows as the web UI's list.
fn rows_json(rows: &[ServerProfileInfo]) -> Value {
    rows.iter()
        .map(|row| {
            json!({
                "id": row.id,
                "engine": row.engine,
                "port": row.port,
                "tags": row.tags,
                "instance": row.instance,
            })
        })
        .collect()
}

/// `POST /api/profiles/server` `{"engine", "id", "port"}`: creates the profile
/// and answers 201 `{"id"}`. The bff reads the body first (400
/// `server-profile-malformed` for a body it cannot read, 422 naming the field the
/// form marks for a value its own checks refuse), so a body nothing can be made
/// of reaches no manager; the manager checks the same rules again, being the one
/// that writes. Its name is taken: 409 `client-server-profile-exists` naming it,
/// and the session lives on. A time limit answers 503 `client-unavailable` while
/// the call was never handed over the manager, and 504 `client-timeout` once it
/// was, the write's outcome unknown, as on the pool's save.
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
        Err(rejection) => return body_refusal(&rejection, "server-profile-malformed"),
    };
    let Some(body) = json_body(&headers, &body) else {
        return error(StatusCode::BAD_REQUEST, "server-profile-malformed", &[]);
    };
    let Some(requested) = requested(&body) else {
        return error(StatusCode::BAD_REQUEST, "server-profile-malformed", &[]);
    };
    if requested.engine != ENGINE_LLAMA_CPP {
        return refused(
            "unknown-engine",
            std::slice::from_ref(&requested.engine),
            "engine",
        );
    }
    if !paths::valid_instance_id(&requested.id) {
        return refused(
            "server-profile-bad-id",
            std::slice::from_ref(&requested.id),
            "id",
        );
    }
    let range = checks::server_int_range("Port").expect("Port has a range");
    let port = match requested.port {
        None => None,
        Some(value) => {
            let text = value.to_string();
            match checks::whole_number("Port", &text, &range) {
                Ok(()) => match u16::try_from(value) {
                    Ok(port) => Some(port),
                    Err(_) => return out_of_range(&text, &range),
                },
                Err(_) => return out_of_range(&text, &range),
            }
        }
    };
    let user = identity.user;
    let (engine, id) = (requested.engine, requested.id);
    let answer = id.clone();
    let state = Arc::new(WriteState::new());
    let mine = state.clone();
    let ask = tokio::task::spawn_blocking(move || -> Result<(), ClientError> {
        let client = connect(Some(user.as_str()))?;
        // Lost to a handler that already ran out of time, no call is sent: the
        // 503 it answered stands and no file is written.
        if !mine.may_write() {
            return Ok(());
        }
        client.create_server_profile(&engine, &id, port)?;
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

/// What a create asks for: the engine, the profile's name, and the port as a
/// whole number, `None` when the body names none (a body with no `port` and one
/// with `port: null` are both the file with no `Port` line). Any other shape,
/// and any value of a type the read never gives, is None: the web UI sends the
/// engine and the name as strings and the port as a number.
struct Requested {
    engine: String,
    id: String,
    port: Option<i64>,
}

fn requested(body: &Value) -> Option<Requested> {
    let top = body.as_object()?;
    let engine = top.get("engine")?.as_str()?.to_string();
    let id = top.get("id")?.as_str()?.to_string();
    let port = match top.get("port") {
        None | Some(Value::Null) => None,
        Some(value) => Some(value.as_i64()?),
    };
    Some(Requested { engine, id, port })
}

/// 422 `not-whole-number-in-range` for a port, with the check's own args (the
/// key, the text, the two edges) and the form's `port` control marked.
fn out_of_range(text: &str, range: &std::ops::RangeInclusive<i32>) -> Response {
    let (lo, hi) = (range.start().to_string(), range.end().to_string());
    refused(
        "not-whole-number-in-range",
        &[String::from("Port"), text.to_string(), lo, hi],
        "port",
    )
}

/// `DELETE /api/profiles/server/{id}`: 204 with no body once the manager
/// removed the file. A profile an instance's record names is 409
/// `client-server-profile-in-use` naming the profile and the instance; no such
/// profile, 404 naming it. The time limit answers 503 while the call was never
/// handed over the manager and 504 `client-timeout` once it was, as on the
/// create.
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
        client.delete_server_profile(ENGINE_LLAMA_CPP, &id)?;
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

/// `GET /api/profiles/server/{id}`: the profile's document, today's Server form
/// keyed by the profile's own file, with `instance` naming the instance whose
/// record names it. The file and the record are two calls of one connect made
/// for the session's user; a record the manager cannot read is the record's
/// own failure, never `instance: null`, which would offer a delete of a
/// profile that is in use.
async fn document(
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
    let ask = tokio::task::spawn_blocking(move || -> Result<_, ClientError> {
        let client = connect(Some(user.as_str()))?;
        let file = profile_file(&*client, &id)?;
        let instance = user_of(&*client, &id)?;
        Ok((file, instance))
    });
    match tokio::time::timeout(timeout, ask).await {
        Ok(Ok(Ok((file, instance)))) => axum::Json(server::document(
            &file,
            key,
            Owner::Profile(instance.as_deref()),
        ))
        .into_response(),
        Ok(Ok(Err(failure))) => read_failure(&sessions, &session, failure),
        Err(_) => error(StatusCode::SERVICE_UNAVAILABLE, "client-unavailable", &[]),
        Ok(Err(_)) => error(StatusCode::BAD_GATEWAY, "client-other", &[]),
    }
}

/// The manager's name of the profile `id`'s file.
fn file_name(id: &str) -> String {
    format!("server@{id}.ini")
}

/// The profile `id`'s file, read as the session's user. A name the manager
/// refuses, and a file it does not have, are [`ClientError::NoServerProfile`]
/// naming the id: the same 404 the route answers for a profile that is not
/// there.
fn profile_file(client: &dyn ManagerClient, id: &str) -> Result<ManagerFile, ClientError> {
    client
        .read_file(&file_name(id))
        .map(|file| file.ok_or(not_there(id)))
        .unwrap_or_else(|failure| match failure {
            ClientError::BadFileName | ClientError::InvalidId => Err(not_there(id)),
            other => Err(other),
        })
}

/// 404's failure: the profile `id` is not there.
fn not_there(id: &str) -> ClientError {
    ClientError::NoServerProfile(id.to_string())
}

/// The instance whose record names the profile `id`, `None` when no section
/// does. A record that cannot be read is the record's own failure.
fn user_of(client: &dyn ManagerClient, id: &str) -> Result<Option<String>, ClientError> {
    Ok(client
        .instances()?
        .into_iter()
        .find(|row| row.server_profile.as_deref() == Some(id))
        .map(|row| row.id))
}

/// `PUT /api/profiles/server/{id}` `{"revision", "values"}`: the form's save of
/// the whole profile file. The answer is `{"revision", "running",
/// "notConverged"}`: the revision of the text the file holds now, beside the
/// manager's own answer of its write, the affected answer the instance's writes
/// answer too ([`crate::instance_pool::save_answer`]). The bff lists no instance
/// and asks no state of its own: the state is the manager's read. The
/// session and the body's shape answer first, as on the instance's save
/// (`server.rs`); the file, the values and the compare-and-swap write are calls
/// of one connect made for the session's user, in that order:
/// a profile that is not there is 404 before its values are judged. While the
/// write was never asked, a time limit that runs out is 503
/// `client-unavailable`, the manager not reached; from the moment it was sent
/// its outcome is unknown and the answer is 504 `client-timeout`. A limit that
/// ran out once the write landed is the save's own 200 with the two lists empty,
/// the manager's answer never having arrived.
async fn save(
    connect: Connect,
    write_timeout: Duration,
    sessions: Arc<Sessions>,
    id: String,
    headers: HeaderMap,
    key: [u8; 16],
    body: Result<axum::body::Bytes, BytesRejection>,
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
    let prepared = server::body_to_file(&body, cpus);
    // The revision the save answers with, of the text it is going to write.
    // Only a body that gives a text reaches a write, so this is `Some` on every
    // answer that names a revision.
    let written = match &prepared {
        BodyAnswer::File(text) => Some(server::revision(text, &key)),
        _ => None,
    };
    let user = identity.user;
    let sent = sent.to_string();
    // The write's state, shared with the task, as on the instance's save: from
    // the moment the write is sent its outcome is unknown, and once it landed
    // the save is done.
    let state = Arc::new(WriteState::new());
    let mine = state.clone();
    let ask = tokio::task::spawn_blocking(move || -> Result<Outcome, ClientError> {
        let client = connect(Some(user.as_str()))?;
        let current = profile_file(&*client, &id)?.text;
        if server::revision(&current, &key) != sent {
            return Err(ClientError::Changed);
        }
        let text = match prepared {
            BodyAnswer::File(text) => text,
            refusal => return Ok(Outcome::Body(refusal)),
        };
        // Idle -> Asked is the only way to the write. Lost, the handler ran out
        // of time and answered, so this save is over; the value is never read.
        if !mine.may_write() {
            return Ok(Outcome::Saved(Affected::default()));
        }
        // The write's answer names the instances: the ones that run, and the
        // ones the write did not leave fitted.
        let touched = client.write_file_if(&file_name(&id), &text, &current)?;
        mine.landed();
        Ok(Outcome::Saved(touched))
    });
    match tokio::time::timeout(write_timeout, ask).await {
        Ok(Ok(Ok(Outcome::Saved(touched)))) => {
            json_response(save_answer(&written.unwrap_or_default(), &touched))
        }
        Ok(Ok(Ok(Outcome::Body(answer)))) => match answer {
            BodyAnswer::Malformed => error(StatusCode::BAD_REQUEST, "server-malformed", &[]),
            BodyAnswer::Refused { key, args, field } => refused(&key, &args, &field),
            // A text becomes a write, never an answer of the body: 502 is the
            // answer for a state the bff cannot name.
            BodyAnswer::File(_) => error(StatusCode::BAD_GATEWAY, "client-other", &[]),
        },
        Ok(Ok(Err(failure))) => save_failure(&sessions, &session, failure, state.sent()),
        Err(_) => timeout_answer(
            state.timed_out(),
            json_response(save_answer(
                &written.unwrap_or_default(),
                &Affected::default(),
            )),
        ),
        Ok(Err(_)) => error(StatusCode::BAD_GATEWAY, "client-other", &[]),
    }
}

/// What the save's task answers: the write landed, with the manager's answer of
/// it, or the body's own refusal, met after the profile's file was read.
enum Outcome {
    Saved(Affected),
    Body(BodyAnswer),
}

/// 200 with `doc`.
fn json_response(doc: Value) -> Response {
    (StatusCode::OK, axum::Json(doc)).into_response()
}

/// The answer to a create or a delete the manager could not do. The name is
/// taken: 409 `client-server-profile-exists` naming it. The profile is in use:
/// 409 `client-server-profile-in-use` naming the profile and the instance that
/// names it. No such profile: 404 naming it. A content check the manager ran is
/// 422 with its key, its values and the field the form marks. The manager
/// refused the signed-in user: 403, and the session ends with the right that
/// authorized it. `Unavailable` (the pipe and the bus answer a broken stream or
/// a missing reply with it) is 504 `client-timeout` when `write_asked` says the
/// shared state reached `WRITE_ASKED`, the call sent and its outcome unknown,
/// and 503 `client-unavailable` when it stayed Idle: the connect never handed
/// anything over. The rest are the bff's news and leave the session alive.
fn write_failure(
    sessions: &Arc<Sessions>,
    session: &str,
    failure: ClientError,
    write_asked: bool,
) -> Response {
    match failure {
        ClientError::ServerProfileExists(profile) => keyed(
            StatusCode::CONFLICT,
            "client-server-profile-exists",
            &[profile.as_str()],
        ),
        ClientError::ServerProfileInUse(profile, instance) => keyed(
            StatusCode::CONFLICT,
            "client-server-profile-in-use",
            &[profile.as_str(), instance.as_str()],
        ),
        ClientError::NoServerProfile(profile) => keyed(
            StatusCode::NOT_FOUND,
            "client-no-server-profile",
            &[profile.as_str()],
        ),
        ClientError::Invalid(refusal) => refused(
            refusal.key,
            &refusal.values,
            &field_of(refusal.key, &refusal.values),
        ),
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

/// The answer to a read that failed: the refusal of the signed-in user ends the
/// session (403); a profile that is not there, 404 naming it; no manager, 503;
/// anything else 502.
fn read_failure(sessions: &Arc<Sessions>, session: &str, failure: ClientError) -> Response {
    match failure {
        ClientError::NotAuthorized => {
            sessions.end(session);
            error(StatusCode::FORBIDDEN, "session-not-administrator", &[])
        }
        ClientError::NoServerProfile(profile) => keyed(
            StatusCode::NOT_FOUND,
            "client-no-server-profile",
            &[profile.as_str()],
        ),
        ClientError::Unavailable => {
            error(StatusCode::SERVICE_UNAVAILABLE, "client-unavailable", &[])
        }
        _ => error(StatusCode::BAD_GATEWAY, "client-other", &[]),
    }
}

/// The answer to a save the manager could not do, as on the instance's save:
/// the compare and swap refused is 409 `server-changed-elsewhere`. The rest are
/// the write's failures ([`write_failure`]), `Unavailable` included: 504
/// `client-timeout` when the write was sent, 503 `client-unavailable` when it
/// was not.
fn save_failure(
    sessions: &Arc<Sessions>,
    session: &str,
    failure: ClientError,
    write_asked: bool,
) -> Response {
    if matches!(failure, ClientError::Changed) {
        return error(StatusCode::CONFLICT, "server-changed-elsewhere", &[]);
    }
    write_failure(sessions, session, failure, write_asked)
}

/// The field a manager refusal marks: the key the refusal names in its first
/// value, and for the two rules that name an engine or a profile, the control of
/// that value.
fn field_of(key: &str, args: &[String]) -> String {
    match key {
        "unknown-engine" => "engine".to_string(),
        "server-profile-bad-id" => "id".to_string(),
        _ => args.first().cloned().unwrap_or_default(),
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

/// `{"key": <strings key>, "args": [...]}` with `status`, the args as text.
fn keyed(status: StatusCode, key: &str, args: &[&str]) -> Response {
    (status, axum::Json(json!({ "key": key, "args": args }))).into_response()
}
