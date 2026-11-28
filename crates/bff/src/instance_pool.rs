//! The instance profile pool: `GET`/`POST /api/profiles/instance`,
//! `GET`/`DELETE /api/profiles/instance/{p}`, `PUT .../{p}/server`,
//! `POST .../{p}/models` and `GET`/`PUT`/`DELETE .../{p}/models/{m}`
//! (`ARCHITECTURE.md` IP4, copied by `fe/briefs/api-contract.md`).
//!
//! One instance profile is one `[<p>]` section of `instance-profiles.ini`, with
//! one `[<p>/<m>]` entry per model profile it is given. The file is the
//! manager's: every read and every write here is one of the manager's `As`
//! methods asked as the signed-in user, so the manager checks their membership
//! of `pai` at every call and a refusal is 403 `session-not-administrator`,
//! ending that session. Nothing here reads a file: the page's entry key counts
//! and the profile's revision come from the pool text `GetInstanceProfile`
//! answers, read with `pai_core::instance_profiles::parse`.
//!
//! `usedBy`, on the list's rows and on the page, is computed by the bff from
//! the instance rows' `instanceProfile` ([`used_by`]): no verb answers it, and
//! `Instances()` answers to any caller, so the row read rides the same connect
//! as the pool's read.
//!
//! Every write that re-renders answers the affected answer, read off the
//! manager's `Affected`: `{"running": [<instance id>], "notConverged":
//! [{"instance", "key", "args"}]}`, the running instances the write left
//! drifting from the pair they launched with and the instances it leaves not
//! yet fitted. The create answers 201 `{"id"}` (no instance runs a new profile,
//! so it renders nowhere) and the profile removal 204.
//!
//! Only the entry's save carries a revision on the wire: [`revision`], a
//! SHA-256 hex of the profile's OWN sections, so an edit of another profile
//! leaves it alone. The save reads the pool text, looks the profile and its
//! entry up in it (a missing one is the read's own 404, with no write asked),
//! compares the hash it was sent (a stale one is 409 `stale-revision`, with no
//! write asked), hands the manager the text it read, so the manager's own
//! compare closes the window between the read and the write, and answers the
//! NEW hash, hashed from the text the write leaves as the manager left it, so
//! a second save needs no re-read.
//!
//! The writes are bound by `write_timeout`, the reads by `timeout`, and a write
//! answers 503 `client-unavailable` while its call was never handed over the
//! manager and 504 `client-timeout` once it was sent, its outcome unknown (as
//! on the server profile pool, `server_profiles.rs`).
//!
//! Every failure is `{"key": <key>, "args": [...]}`, with `field` for the
//! refusals of a body's value, never the manager's detail. Most of the keys are
//! the manager's own; the five the contract renames are mapped by
//! [`write_failure`] ([`refused_of`]). A body nothing can be made of
//! is 400 `instance-profile-malformed`, and the entry save's body is 400
//! `model-profiles-malformed` for bytes and 422 `model-profiles-malformed` when
//! it carries no revision.

use crate::model_profiles::not_administrator;
use crate::origin::body_refusal;
use crate::server::{timeout_answer, TimedOut, WriteState};
use crate::session::{error, json_body, Sessions};
use crate::Connect;
use axum::body::Bytes;
use axum::extract::rejection::BytesRejection;
use axum::extract::Path;
use axum::http::{HeaderMap, StatusCode};
use axum::response::{IntoResponse, Response};
use axum::Router;
use pai_core::checks::Refusal;
use pai_core::client::{Affected, ClientError, InstanceInfo, InstanceProfileInfo, ManagerClient};
use pai_core::instance_profiles::{self, Profile};
use serde_json::{json, Map, Value};
use sha2::{Digest as _, Sha256};
use std::sync::{Arc, Mutex};
use std::time::Duration;

/// The pool's routes, over the same seams as the server profile pool's: the
/// list and the create, the page, the removal, the server switch, the entry
/// add, read, save and removal. A body is capped by the one layer in front of
/// the whole router (`lib.rs`).
pub(crate) fn routes(
    connect: Connect,
    timeout: Duration,
    write_timeout: Duration,
    sessions: Arc<Sessions>,
) -> Router {
    let list = {
        let (connect, sessions) = (connect.clone(), sessions.clone());
        move |headers: HeaderMap| list(connect.clone(), timeout, sessions.clone(), headers)
    };
    let create = {
        let (connect, sessions) = (connect.clone(), sessions.clone());
        move |headers: HeaderMap, body: Result<Bytes, BytesRejection>| {
            create(
                connect.clone(),
                write_timeout,
                sessions.clone(),
                headers,
                body,
            )
        }
    };
    let page = {
        let (connect, sessions) = (connect.clone(), sessions.clone());
        move |Path(id): Path<String>, headers: HeaderMap| {
            page(connect.clone(), timeout, sessions.clone(), id, headers)
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
    let set_server = {
        let (connect, sessions) = (connect.clone(), sessions.clone());
        move |Path(id): Path<String>, headers: HeaderMap, body: Result<Bytes, BytesRejection>| {
            set_server(
                connect.clone(),
                write_timeout,
                sessions.clone(),
                id,
                headers,
                body,
            )
        }
    };
    let add_model = {
        let (connect, sessions) = (connect.clone(), sessions.clone());
        move |Path(id): Path<String>, headers: HeaderMap, body: Result<Bytes, BytesRejection>| {
            add_model(
                connect.clone(),
                write_timeout,
                sessions.clone(),
                id,
                headers,
                body,
            )
        }
    };
    let entry = {
        let (connect, sessions) = (connect.clone(), sessions.clone());
        move |Path((id, model)): Path<(String, String)>, headers: HeaderMap| {
            entry(
                connect.clone(),
                timeout,
                sessions.clone(),
                id,
                model,
                headers,
            )
        }
    };
    let save_entry = {
        let (connect, sessions) = (connect.clone(), sessions.clone());
        move |Path((id, model)): Path<(String, String)>,
              headers: HeaderMap,
              body: Result<Bytes, BytesRejection>| {
            save_entry(
                connect.clone(),
                write_timeout,
                sessions.clone(),
                id,
                model,
                headers,
                body,
            )
        }
    };
    let delete_entry = {
        let (connect, sessions) = (connect.clone(), sessions.clone());
        move |Path((id, model)): Path<(String, String)>, headers: HeaderMap| {
            delete_entry(
                connect.clone(),
                write_timeout,
                sessions.clone(),
                id,
                model,
                headers,
            )
        }
    };
    Router::new()
        .route(
            "/api/profiles/instance",
            axum::routing::get(list).post(create),
        )
        .route(
            "/api/profiles/instance/{p}",
            axum::routing::get(page).delete(remove),
        )
        .route(
            "/api/profiles/instance/{p}/server",
            axum::routing::put(set_server),
        )
        .route(
            "/api/profiles/instance/{p}/models",
            axum::routing::post(add_model),
        )
        .route(
            "/api/profiles/instance/{p}/models/{m}",
            axum::routing::get(entry)
                .put(save_entry)
                .delete(delete_entry),
        )
}

// ── The reads ────────────────────────────────────────────────────────────

/// `GET /api/profiles/instance`: the pool's rows in the pool's own order, each
/// `{id, server, models, usedBy}`, `models` the entry model ids in the file's
/// order and `usedBy` the instances whose record runs the profile.
async fn list(
    connect: Connect,
    timeout: Duration,
    sessions: Arc<Sessions>,
    headers: HeaderMap,
) -> Response {
    let Some((session, identity)) = sessions.current(&headers) else {
        return error(StatusCode::UNAUTHORIZED, "session-required", &[]);
    };
    read(
        connect,
        timeout,
        &sessions,
        &session,
        identity.user,
        |client| {
            Ok(rows_json(
                &client.instance_profiles()?,
                &client.instances()?,
            ))
        },
    )
    .await
}

/// The pool's rows as the web UI's list.
fn rows_json(profiles: &[InstanceProfileInfo], rows: &[InstanceInfo]) -> Value {
    json!({
        "rows": profiles
            .iter()
            .map(|profile| json!({
                "id": profile.id,
                "server": profile.server_profile,
                "models": profile.entries,
                "usedBy": used_by(rows, &profile.id),
            }))
            .collect::<Vec<_>>()
    })
}

/// The instances that run the profile `id`: the record's rows whose
/// `instanceProfile` names it, in the record's own order. No verb answers
/// `usedBy`, the bff computes it (IP4).
fn used_by(rows: &[InstanceInfo], id: &str) -> Vec<String> {
    rows.iter()
        .filter(|row| {
            row.instance_profile
                .as_deref()
                .is_some_and(|held| held.eq_ignore_ascii_case(id))
        })
        .map(|row| row.id.clone())
        .collect()
}

/// `GET /api/profiles/instance/{p}`: the profile's page, `{id, server, models,
/// usedBy, revision}`. `models` is one row per entry with the count of its
/// keys, so the page says "llama.cpp's defaults" for 0, and `revision` the
/// hash of the profile's own sections: both read out of the pool text
/// `GetInstanceProfile` answers, the record read on the same connect for
/// `usedBy`. A profile the text has no section for is 404
/// `no-instance-profile-named`, answered from that text before the record is
/// asked, so a record failure cannot turn a missing profile into a bad gateway.
async fn page(
    connect: Connect,
    timeout: Duration,
    sessions: Arc<Sessions>,
    id: String,
    headers: HeaderMap,
) -> Response {
    let Some((session, identity)) = sessions.current(&headers) else {
        return error(StatusCode::UNAUTHORIZED, "session-required", &[]);
    };
    read(
        connect,
        timeout,
        &sessions,
        &session,
        identity.user,
        move |client| {
            let info = client.instance_profile(&id)?;
            let found = own_profile(&info.revision, &id).ok_or_else(|| not_there(&id))?;
            let rows = client.instances()?;
            Ok(json!({
                "id": found.id,
                "server": found.server_profile.clone().unwrap_or_default(),
                "models": found
                    .entries
                    .iter()
                    .map(|entry| json!({ "id": entry.model, "keys": entry.keys.len() }))
                    .collect::<Vec<_>>(),
                "usedBy": used_by(&rows, &found.id),
                "revision": revision(&info.revision, &id),
            }))
        },
    )
    .await
}

/// `GET /api/profiles/instance/{p}/models/{m}`: the entry's page, `{profile,
/// server, model, values, revision}`, the entry's keys as the file holds them
/// and the same hash the profile's page answers, both read out of one text. An
/// entry the profile lists nowhere is 404 `no-instance-profile-model`, the
/// manager's key of the same fact.
async fn entry(
    connect: Connect,
    timeout: Duration,
    sessions: Arc<Sessions>,
    id: String,
    model: String,
    headers: HeaderMap,
) -> Response {
    let Some((session, identity)) = sessions.current(&headers) else {
        return error(StatusCode::UNAUTHORIZED, "session-required", &[]);
    };
    read(
        connect,
        timeout,
        &sessions,
        &session,
        identity.user,
        move |client| {
            let info = client.instance_profile(&id)?;
            let found = own_profile(&info.revision, &id).ok_or_else(|| not_there(&id))?;
            let held = found
                .entries
                .iter()
                .find(|entry| entry.model.eq_ignore_ascii_case(&model))
                .ok_or_else(|| no_entry(&id, &model))?;
            Ok(json!({
                "profile": found.id,
                "server": found.server_profile.clone().unwrap_or_default(),
                "model": held.model,
                "values": held
                    .keys
                    .iter()
                    .map(|(key, value)| (key.clone(), json!(value)))
                    .collect::<Map<_, _>>(),
                "revision": revision(&info.revision, &id),
            }))
        },
    )
    .await
}

/// One read of the pool, as the signed-in user: `ask` is the manager's calls,
/// made on one connect under `timeout`. The session is read by the handler, so
/// with no session the manager is not even connected to.
async fn read(
    connect: Connect,
    timeout: Duration,
    sessions: &Arc<Sessions>,
    session: &str,
    user: String,
    ask: impl FnOnce(&dyn ManagerClient) -> Result<Value, ClientError> + Send + 'static,
) -> Response {
    let task = tokio::task::spawn_blocking(move || -> Result<Value, ClientError> {
        ask(connect(Some(user.as_str()))?.as_ref())
    });
    match tokio::time::timeout(timeout, task).await {
        Ok(Ok(Ok(doc))) => json_response(&doc),
        Ok(Ok(Err(failure))) => read_failure(sessions, session, failure),
        Err(_) => error(StatusCode::SERVICE_UNAVAILABLE, "client-unavailable", &[]),
        Ok(Err(_)) => error(StatusCode::BAD_GATEWAY, "client-other", &[]),
    }
}

// ── The writes ───────────────────────────────────────────────────────────

/// `POST /api/profiles/instance` `{"id", "server"}`: a new profile, answering
/// 201 `{"id"}`. The bff reads the body's shape first (400
/// `instance-profile-malformed` for a body nothing can be made of, the manager
/// not connected to); every rule of the name and of the server profile is the
/// manager's, its refusal keeping its key and its args, the keys the contract
/// renames mapped ([`refused_of`]).
async fn create(
    connect: Connect,
    write_timeout: Duration,
    sessions: Arc<Sessions>,
    headers: HeaderMap,
    body: Result<Bytes, BytesRejection>,
) -> Response {
    let Some((session, identity)) = sessions.current(&headers) else {
        return error(StatusCode::UNAUTHORIZED, "session-required", &[]);
    };
    let body = match body {
        Ok(body) => body,
        Err(rejection) => return body_refusal(&rejection, "instance-profile-malformed"),
    };
    let Some(pair) = json_body(&headers, &body).as_ref().and_then(profile_of) else {
        return error(StatusCode::BAD_REQUEST, "instance-profile-malformed", &[]);
    };
    let (id, server) = pair;
    let (ask, answer) = (id.clone(), id);
    write(
        connect,
        write_timeout,
        &sessions,
        &session,
        identity.user,
        move |client| client.create_instance_profile(&ask, &server),
        move |_, _| (StatusCode::CREATED, axum::Json(json!({ "id": answer }))).into_response(),
    )
    .await
}

/// `DELETE /api/profiles/instance/{p}`: 204 with no body once the manager
/// removed the profile. A profile an instance runs is 409
/// `instance-profile-in-use` naming the instance. A profile the pool has no
/// section for is 404 `no-instance-profile-named`, answered from the pool text
/// the removal reads first, with no write asked.
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
    let task = tokio::task::spawn_blocking(move || -> Result<(), ClientError> {
        let client = connect(Some(user.as_str()))?;
        let text = client.instance_profile(&id)?.revision;
        if own_profile(&text, &id).is_none() {
            return Err(not_there(&id));
        }
        // Lost to a handler that already ran out of time, no call is sent.
        if !mine.may_write() {
            return Ok(());
        }
        client.delete_instance_profile(&id)?;
        mine.landed();
        Ok(())
    });
    match tokio::time::timeout(write_timeout, task).await {
        Ok(Ok(Ok(()))) => StatusCode::NO_CONTENT.into_response(),
        Ok(Ok(Err(failure))) => write_failure(&sessions, &session, failure, state.sent()),
        Err(_) => timeout_answer(state.timed_out(), StatusCode::NO_CONTENT.into_response()),
        Ok(Err(_)) => error(StatusCode::BAD_GATEWAY, "client-other", &[]),
    }
}

/// `PUT /api/profiles/instance/{p}/server` `{"server"}`: the profile's server
/// profile, answering the affected answer. The switch is one set-like change,
/// so it carries no revision: the manager refuses it on its own. The instances
/// the new profile does not fit are no refusal: they are rows of
/// `notConverged`.
async fn set_server(
    connect: Connect,
    write_timeout: Duration,
    sessions: Arc<Sessions>,
    id: String,
    headers: HeaderMap,
    body: Result<Bytes, BytesRejection>,
) -> Response {
    let Some((session, identity)) = sessions.current(&headers) else {
        return error(StatusCode::UNAUTHORIZED, "session-required", &[]);
    };
    let body = match body {
        Ok(body) => body,
        Err(rejection) => return body_refusal(&rejection, "instance-profile-malformed"),
    };
    let Some(server) = json_body(&headers, &body)
        .as_ref()
        .and_then(|body| one_string(body, "server"))
    else {
        return error(StatusCode::BAD_REQUEST, "instance-profile-malformed", &[]);
    };
    write(
        connect,
        write_timeout,
        &sessions,
        &session,
        identity.user,
        move |client| client.set_instance_profile_server(&id, &server),
        |touched, _| json_response(&affected_of(&touched)),
    )
    .await
}

/// `POST /api/profiles/instance/{p}/models` `{"model"}`: an empty entry giving
/// the profile the model profile, answering 201 with the affected answer. The
/// add is a create, not a replace: the entry's keys belong to its save, which
/// is why the two are two routes.
async fn add_model(
    connect: Connect,
    write_timeout: Duration,
    sessions: Arc<Sessions>,
    id: String,
    headers: HeaderMap,
    body: Result<Bytes, BytesRejection>,
) -> Response {
    let Some((session, identity)) = sessions.current(&headers) else {
        return error(StatusCode::UNAUTHORIZED, "session-required", &[]);
    };
    let body = match body {
        Ok(body) => body,
        Err(rejection) => return body_refusal(&rejection, "instance-profile-malformed"),
    };
    let Some(model) = json_body(&headers, &body)
        .as_ref()
        .and_then(|body| one_string(body, "model"))
    else {
        return error(StatusCode::BAD_REQUEST, "instance-profile-malformed", &[]);
    };
    write(
        connect,
        write_timeout,
        &sessions,
        &session,
        identity.user,
        move |client| client.add_instance_profile_model(&id, &model),
        |touched, _| (StatusCode::CREATED, axum::Json(affected_of(&touched))).into_response(),
    )
    .await
}

/// `DELETE /api/profiles/instance/{p}/models/{m}`: the entry taken away,
/// answering the affected answer. An entry the profile lists nowhere is 404
/// `no-instance-profile-model`.
async fn delete_entry(
    connect: Connect,
    write_timeout: Duration,
    sessions: Arc<Sessions>,
    id: String,
    model: String,
    headers: HeaderMap,
) -> Response {
    let Some((session, identity)) = sessions.current(&headers) else {
        return error(StatusCode::UNAUTHORIZED, "session-required", &[]);
    };
    write(
        connect,
        write_timeout,
        &sessions,
        &session,
        identity.user,
        move |client| client.remove_instance_profile_model(&id, &model),
        |touched, _| json_response(&affected_of(&touched)),
    )
    .await
}

/// `PUT /api/profiles/instance/{p}/models/{m}` `{"values", "revision"}`: the
/// entry whole, as the page sent it, answering `{revision, running,
/// notConverged}`. The session and the body answer first, so a body nothing can
/// be made of (400 `model-profiles-malformed`) and a body with no revision (422
/// `model-profiles-malformed`) reach no manager. The pool text is read on the
/// connect of the write: the profile and its entry are looked up in it first (a
/// missing one is the same 404 the read of it answers, with no write asked),
/// then the hash of the profile's own sections is compared with the one sent: a
/// stale edit is 409 `stale-revision` with no write asked. The manager is handed
/// that same text, so the manager's own compare answers the same 409 for a pool
/// that moved between the two. The save answers the hash of the profile's
/// sections of the text the write LEFT, read back off the manager: the manager
/// rewrites the entry where it sits, and the page hashes that same text, so the
/// page can save again without reading again.
async fn save_entry(
    connect: Connect,
    write_timeout: Duration,
    sessions: Arc<Sessions>,
    id: String,
    model: String,
    headers: HeaderMap,
    body: Result<Bytes, BytesRejection>,
) -> Response {
    let Some((session, identity)) = sessions.current(&headers) else {
        return error(StatusCode::UNAUTHORIZED, "session-required", &[]);
    };
    let body = match body {
        Ok(body) => body,
        Err(rejection) => return body_refusal(&rejection, "model-profiles-malformed"),
    };
    let Some(body) = json_body(&headers, &body) else {
        return error(StatusCode::BAD_REQUEST, "model-profiles-malformed", &[]);
    };
    let Some(sent) = body.get("revision").and_then(Value::as_str) else {
        return error(
            StatusCode::UNPROCESSABLE_ENTITY,
            "model-profiles-malformed",
            &[],
        );
    };
    let Some(values) = values_of(&body) else {
        return error(
            StatusCode::UNPROCESSABLE_ENTITY,
            "model-profiles-malformed",
            &[],
        );
    };
    let user = identity.user;
    let sent = sent.to_string();
    // The hash of the text the write left, carried out of the task so a limit
    // that runs out once the write landed answers the revision of that text.
    let answered = Arc::new(Mutex::new(String::new()));
    let shared = answered.clone();
    let state = Arc::new(WriteState::new());
    let mine = state.clone();
    let task = tokio::task::spawn_blocking(move || -> Result<Value, ClientError> {
        let client = connect(Some(user.as_str()))?;
        let text = client.instance_profile(&id)?.revision;
        // The profile and its entry are looked up in the text read, before the
        // revision is compared: the revision guards a write to an entry there
        // IS one, so a missing one stays missing whatever revision is sent.
        let found = own_profile(&text, &id).ok_or_else(|| not_there(&id))?;
        if !found
            .entries
            .iter()
            .any(|entry| entry.model.eq_ignore_ascii_case(&model))
        {
            return Err(no_entry(&id, &model));
        }
        if revision(&text, &id) != sent {
            return Err(ClientError::Changed);
        }
        // Lost to a handler that already ran out of time, no call is sent: the
        // 503 it answered stands, and this value is never read.
        if !mine.may_write() {
            return Ok(save_answer("", &Affected::default()));
        }
        let touched = client.set_instance_profile_model(&id, &model, &values, &text)?;
        mine.landed();
        // The revision is hashed from the text the write left, read back: the
        // manager rewrites the entry where it sits, which is not the text the
        // bff's own writer leaves.
        let after = revision(&client.instance_profile(&id)?.revision, &id);
        *shared.lock().unwrap_or_else(|e| e.into_inner()) = after.clone();
        Ok(save_answer(&after, &touched))
    });
    match tokio::time::timeout(write_timeout, task).await {
        Ok(Ok(Ok(doc))) => json_response(&doc),
        Ok(Ok(Err(failure))) => write_failure(&sessions, &session, failure, state.sent()),
        Err(_) => timeout_answer(
            state.timed_out(),
            json_response(&save_answer(
                &answered.lock().unwrap_or_else(|e| e.into_inner()),
                &Affected::default(),
            )),
        ),
        Ok(Err(_)) => error(StatusCode::BAD_GATEWAY, "client-other", &[]),
    }
}

/// The entry save's answer: the revision it answers with, beside the affected
/// answer.
fn save_answer(revision: &str, touched: &Affected) -> Value {
    json!({
        "revision": revision,
        "running": touched.running,
        "notConverged": rows_of(touched),
    })
}

/// The affected answer, read off the manager's `Affected`: `running` the
/// running instances the write left drifting, `notConverged` the instances it
/// leaves not yet fitted, each with the manager's key and args.
fn affected_of(touched: &Affected) -> Value {
    json!({
        "running": touched.running,
        "notConverged": rows_of(touched),
    })
}

/// The `notConverged` rows of an affected answer.
fn rows_of(touched: &Affected) -> Vec<Value> {
    touched
        .not_converged
        .iter()
        .map(|row| json!({ "instance": row.instance, "key": row.key, "args": row.args }))
        .collect()
}

/// One write of the pool, as the signed-in user: `ask` is the manager's call,
/// made on one connect under `write_timeout`. The shared [`WriteState`] settles
/// with the task whether the call was ever sent: a limit that runs out first
/// leaves the task sending nothing and answers 503, and a limit that runs out
/// once the call is sent answers 504, its outcome unknown. A limit that runs out
/// once the call landed answers `success` with `true`, built from what the route
/// asked for and never from a read it can no longer make.
async fn write<T, S>(
    connect: Connect,
    write_timeout: Duration,
    sessions: &Arc<Sessions>,
    session: &str,
    user: String,
    ask: S,
    success: impl FnOnce(T, bool) -> Response,
) -> Response
where
    T: Send + Default + 'static,
    S: FnOnce(&dyn ManagerClient) -> Result<T, ClientError> + Send + 'static,
{
    let state = Arc::new(WriteState::new());
    let mine = state.clone();
    let task = tokio::task::spawn_blocking(move || -> Result<T, ClientError> {
        let client = connect(Some(user.as_str()))?;
        if !mine.may_write() {
            return Ok(T::default());
        }
        let answer = ask(client.as_ref())?;
        mine.landed();
        Ok(answer)
    });
    match tokio::time::timeout(write_timeout, task).await {
        Ok(Ok(Ok(answer))) => success(answer, true),
        Ok(Ok(Err(failure))) => write_failure(sessions, session, failure, state.sent()),
        Err(_) => {
            let timed = state.timed_out();
            let landed = matches!(timed, TimedOut::Done);
            timeout_answer(timed, success(T::default(), landed))
        }
        Ok(Err(_)) => error(StatusCode::BAD_GATEWAY, "client-other", &[]),
    }
}

// ── The pool's text ──────────────────────────────────────────────────────

/// The profile `profile` in the pool text `text`, by its id in any letter case
/// (the pool's own rule, `pai_core::instance_profiles::parse`).
fn own_profile(text: &str, profile: &str) -> Option<Profile> {
    instance_profiles::parse(text)
        .into_iter()
        .find(|found| found.id.eq_ignore_ascii_case(profile))
}

/// The profile's revision: SHA-256 in lowercase hex of its OWN sections, the
/// `[<p>]` and the `[<p>/*]` of it, so an edit of another profile of the same
/// file leaves the hash alone. The bytes are the parsed profile as the manager's
/// own parser reads it, so the text a write leaves hashes to what the write
/// answers.
fn revision(text: &str, profile: &str) -> String {
    let mut digest = Sha256::new();
    if let Some(found) = own_profile(text, profile) {
        digest.update(own_bytes(&found));
    }
    digest
        .finalize()
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
}

/// The bytes a profile's revision covers: its id, its server profile, and each
/// entry with its keys, in the file's own order, one line each.
fn own_bytes(profile: &Profile) -> String {
    let mut text = String::new();
    text.push_str(&profile.id);
    text.push('\n');
    text.push_str(profile.server_profile.as_deref().unwrap_or_default());
    text.push('\n');
    for entry in &profile.entries {
        text.push_str(&entry.model);
        text.push('\n');
        for (key, value) in &entry.keys {
            text.push_str(key);
            text.push('=');
            text.push_str(value);
            text.push('\n');
        }
    }
    text
}

// ── The bodies ───────────────────────────────────────────────────────────

/// The create's body: `{"id", "server"}`, two strings. Any other shape is none.
fn profile_of(body: &Value) -> Option<(String, String)> {
    Some((one_string(body, "id")?, one_string(body, "server")?))
}

/// The entry save's `values`: an object whose every value is a string, as
/// `(key, value)` pairs in the body's own order. Any other shape is none.
fn values_of(body: &Value) -> Option<Vec<(String, String)>> {
    let map = body.get("values")?.as_object()?;
    map.iter()
        .map(|(key, value)| Some((key.clone(), value.as_str()?.to_string())))
        .collect()
}

/// The string of a body under `key`: a body that is not an object, a missing
/// key and a value that is not a string are all none.
fn one_string(body: &Value, key: &str) -> Option<String> {
    body.get(key).and_then(Value::as_str).map(str::to_string)
}

// ── The answers ──────────────────────────────────────────────────────────

/// The answer to a read of the pool: the refusal of the signed-in user ends the
/// session (403 `session-not-administrator`); a profile the pool has not got is
/// 404 `no-instance-profile-named`, and an entry the profile lists nowhere 404
/// `no-instance-profile-model`, the manager's key; no manager, 503
/// `client-unavailable`; anything else 502 `client-other`, the session living
/// on.
fn read_failure(sessions: &Arc<Sessions>, session: &str, failure: ClientError) -> Response {
    use ClientError::*;
    match failure {
        NotAuthorized => not_administrator(sessions, session),
        NoInstanceProfile(profile) => reply(
            StatusCode::NOT_FOUND,
            "no-instance-profile-named",
            &[profile.as_str()],
            None,
        ),
        Invalid(refusal) if refusal.key == "no-instance-profile-model" => {
            refused_of(StatusCode::NOT_FOUND, &refusal)
        }
        Unavailable => error(StatusCode::SERVICE_UNAVAILABLE, "client-unavailable", &[]),
        _ => error(StatusCode::BAD_GATEWAY, "client-other", &[]),
    }
}

/// The answer to a write the manager could not do. A name taken is 409
/// `instance-profile-name-taken`, marking the `id` field; a profile an instance
/// runs is 409 `instance-profile-in-use` naming the instance; a profile the pool
/// has not got is 404 `no-instance-profile-named`; a server profile the node has
/// not got is 422 `no-server-profile-named`, marking the `server` field; the
/// compare of the entry save found its file moved, 409 `stale-revision`. Every
/// other keyed refusal keeps the manager's key and args ([`refused_of`]). The
/// manager refused the signed-in user: 403, and the session ends with the right
/// that authorized it. `Unavailable` is 504 `client-timeout` when `write_asked`
/// says the call was sent, its outcome unknown, and 503 `client-unavailable`
/// when it stayed unsent. The rest are the bff's news and leave the session
/// alive.
fn write_failure(
    sessions: &Arc<Sessions>,
    session: &str,
    failure: ClientError,
    write_asked: bool,
) -> Response {
    use ClientError::*;
    match failure {
        InstanceProfileExists(profile) => reply(
            StatusCode::CONFLICT,
            "instance-profile-name-taken",
            &[profile.as_str()],
            Some("id"),
        ),
        InstanceProfileInUse(_profile, instance) => reply(
            StatusCode::CONFLICT,
            "instance-profile-in-use",
            &[instance.as_str()],
            None,
        ),
        NoInstanceProfile(profile) => reply(
            StatusCode::NOT_FOUND,
            "no-instance-profile-named",
            &[profile.as_str()],
            None,
        ),
        NoServerProfile(server) => reply(
            StatusCode::UNPROCESSABLE_ENTITY,
            "no-server-profile-named",
            &[server.as_str()],
            Some("server"),
        ),
        Changed => reply(StatusCode::CONFLICT, "stale-revision", &[], None),
        Invalid(refusal) => refused_of(status_of(refusal.key), &refusal),
        NotAuthorized => not_administrator(sessions, session),
        Unavailable if write_asked => error(StatusCode::GATEWAY_TIMEOUT, "client-timeout", &[]),
        Unavailable => error(StatusCode::SERVICE_UNAVAILABLE, "client-unavailable", &[]),
        _ => error(StatusCode::BAD_GATEWAY, "client-other", &[]),
    }
}

/// 422 with the check's key, its values and `field` (IP4's table): the manager's
/// `instance-profile-bad-id` answers as the contract's
/// `invalid-instance-profile-id`, marking the `id` field; `no-instance-profile-model`
/// is the entry's 404; a model profile the pool has not got marks the `model`
/// field; a model the profile lists already is a conflict. Every other key keeps
/// the manager's, 422, and marks a field only when its first value IS the key
/// the form holds ([`KEY_IS_FIELD`]).
fn refused_of(status: StatusCode, refusal: &Refusal) -> Response {
    let key = if refusal.key == "instance-profile-bad-id" {
        "invalid-instance-profile-id"
    } else {
        refusal.key
    };
    let args = refusal
        .values
        .iter()
        .map(String::as_str)
        .collect::<Vec<_>>();
    let field = match status {
        StatusCode::UNPROCESSABLE_ENTITY => match refusal.key {
            "instance-profile-bad-id" => Some("id"),
            "no-model-profile-named" => Some("model"),
            key if KEY_IS_FIELD.contains(&key) => refusal.values.first().map(String::as_str),
            _ => None,
        },
        _ => None,
    };
    reply(status, key, &args, field)
}

/// The refusals whose first value is the entry key the form marks: the entry's
/// checks name the key they refused, `model-profile-stay-key` the key that
/// stays in the model profile, the whole-number, device list, float, enum and
/// JSON rules the key of the value they refused, the pair rule its lower key.
/// Every other refusal of these routes values a position, a value or a count
/// (`tensor-split-format`, the `override-tensor` rules, `bind-all-with-others`),
/// so it names no field.
const KEY_IS_FIELD: &[&str] = &[
    "device-bad-name",
    "min-above-max",
    "model-profile-stay-key",
    "not-a-number",
    "not-json-object",
    "not-one-of",
    "not-whole-number-in-range",
];

/// The status of one of the manager's keyed refusals on these routes: 404 for
/// the entry the profile lists nowhere, 409 for the model it lists already, 422
/// for a value rule.
fn status_of(key: &str) -> StatusCode {
    match key {
        "no-instance-profile-model" => StatusCode::NOT_FOUND,
        "instance-profile-model-listed" => StatusCode::CONFLICT,
        _ => StatusCode::UNPROCESSABLE_ENTITY,
    }
}

/// 404's failure: the pool has no section for the profile `id`.
fn not_there(id: &str) -> ClientError {
    ClientError::NoInstanceProfile(id.to_string())
}

/// 404's failure: the profile `id` lists the model `model` nowhere, said as the
/// manager says it.
fn no_entry(profile: &str, model: &str) -> ClientError {
    ClientError::Invalid(Refusal {
        key: "no-instance-profile-model",
        values: vec![profile.to_string(), model.to_string()],
    })
}

/// `{"key", "args"}` at `status`, with `"field"` when there is one to name.
fn reply(status: StatusCode, key: &str, args: &[&str], field: Option<&str>) -> Response {
    let mut doc = json!({ "key": key, "args": args });
    if let Some(field) = field {
        doc["field"] = json!(field);
    }
    (status, axum::Json(doc)).into_response()
}

/// 200 with `doc`.
fn json_response(doc: &Value) -> Response {
    (StatusCode::OK, axum::Json(doc.clone())).into_response()
}
