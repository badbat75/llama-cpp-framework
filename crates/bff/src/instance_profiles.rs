//! The association routes: the server profile one instance runs on, and the
//! model profiles it is given.
//!
//! `PUT /api/instances/{id}/profiles/server` `{"serverProfile": <name>}` is the
//! manager's `SetServerProfile`: the instance's record names the
//! `server@<name>.ini` it runs, and nothing is started, stopped or restarted.
//! The answer is `{"running": <bool>}`, the instance's state read after the
//! write: the page offers to restart an instance that is up, its record having
//! changed under it. A record that already names that profile answers false
//! with no state asked: nothing changed.
//!
//! `GET /api/instances/{id}/profiles/model` is the model profile pool seen from
//! one instance: the pool's rows in the pool's own order, each with the three
//! fields its section of `assignments.ini` gives it (`value`, `viaAll`,
//! `effective`, one rule with the render, `pai_core::assign::rows`), and `all`,
//! the `*` of that section as the page shows it.
//! `PUT /api/instances/{id}/profiles/model/{profile}` `{"value": <word>}` writes
//! one key of that section and answers `{"effective", "running"}`;
//! `PUT /api/instances/{id}/profiles/model` `{"all": <inherit|none>}` writes the
//! `*` of it, the whole pool, and answers `{"running"}`. The path's `*` is the
//! whole pool, which has that route: it is 404 `no-model-profile-named`, and the
//! manager is never asked to write it.
//!
//! The record that names the instance, and the state the answer is about, are
//! what the manager answers of the bff's own account, so their connect is handed
//! no principal. The files, the assignment writes and the server profile write
//! act for the signed-in user, the session's user being the principal of their
//! connect and the manager re-checking their membership of `pai` at every call.
//! Every route reads the session with `Sessions::current`, so a page read and a
//! write keep the idle deadline.
//!
//! `running` after a model profile write is the instance's state when the write
//! changed the model profiles the instance is given: the record is read back
//! after the write, so the answer is the machine's and not what the bff
//! computed, and the set of model profile ids the instance is given (`effective`
//! in `pai_core::assign::rows`) is compared on both sides of the write ([`load_changed`]).
//! A write that changes the text of `assignments.ini` and nothing the instance
//! is given answers false with no state asked. The read back is the one read the
//! bff computes: the write stands, its failure answers from the text the write
//! would have left (`ini::text::set_key` and `remove_key`, the manager's own
//! rule). A state that cannot be read answers true, the safe answer for a write
//! that stands, and so does a record that cannot be read.
//!
//! The three writes are bound by the write's own limit (`write_timeout`), the
//! reads by `timeout`: a manager that is slow writing is not cut off at the
//! limit that is right for a read.
//!
//! Every failure is `{"key": <strings key>, "args": [...]}`, never the
//! manager's detail: 401 `session-required` with no session (the manager is not
//! even connected to), 400 `server-profile-malformed` and `assignment-malformed`
//! for a body nothing can be made of, 422 `model-profile-bad-value` for a word
//! these routes do not write, 403 `session-not-administrator` when the manager
//! refused the signed-in user, which ends that session, 502
//! `client-not-authorized` when it refused the bff's own account (the record of
//! the instances, the state), which ends no session, 503 `client-unavailable`
//! when there is no manager or it answers later than the route's limit, 404
//! `client-no-instance` for an id the record does not name, 404
//! `client-no-server-profile` and 409 `client-server-profile-in-use` for the two
//! refusals of the server profile write, the manager's own refusals with their
//! key, their args and, for the server profile write, the field `serverProfile`,
//! and 502 `client-other` for anything else.

use crate::model_profiles::{not_administrator, pool, read_answer, MODELS, MODELS_DISABLED};
use crate::origin::body_refusal;
use crate::server::runs;
use crate::session::{error, json_body, Sessions};
use crate::{Client, Connect, ENGINE_LLAMA_CPP};
use axum::body::Bytes;
use axum::extract::rejection::BytesRejection;
use axum::extract::Path;
use axum::http::{HeaderMap, StatusCode};
use axum::response::{IntoResponse, Response};
use pai_core::client::{ClientError, ManagerFile};
use pai_core::{assign, ini};
use serde_json::{json, Value};
use std::collections::BTreeSet;
use std::sync::Arc;
use std::time::Duration;

/// The manager's record of which instance is given which model profile.
const ASSIGNMENTS: &str = "assignments.ini";

/// The key that stands for every model profile of the pool, never a model
/// profile id of its own.
const STAR: &str = "*";

/// The field the page marks for a refusal of the server profile write.
const SERVER_PROFILE_FIELD: &str = "serverProfile";

/// The words the assignment route writes, and the two of the whole pool's `*`.
const VALUES: [&str; 4] = ["on", "off", "none", "inherit"];
const ALLS: [&str; 2] = ["inherit", "none"];

/// The association routes, over the same seams as the instance's Server route.
/// The three writes are bound by `write_timeout`, the rows read by `timeout`. A
/// body is capped by the one layer in front of the whole router (`lib.rs`).
pub(crate) fn routes(
    connect: Connect,
    timeout: Duration,
    write_timeout: Duration,
    sessions: Arc<Sessions>,
) -> axum::Router {
    let server_switch = {
        let (connect, sessions) = (connect.clone(), sessions.clone());
        move |Path(id): Path<String>, headers: HeaderMap, body: Result<Bytes, BytesRejection>| {
            set_server_profile(
                connect.clone(),
                write_timeout,
                sessions.clone(),
                id,
                headers,
                body,
            )
        }
    };
    let rows = {
        let (connect, sessions) = (connect.clone(), sessions.clone());
        move |Path(id): Path<String>, headers: HeaderMap| {
            model_rows(connect.clone(), timeout, sessions.clone(), id, headers)
        }
    };
    let set_all = {
        let (connect, sessions) = (connect.clone(), sessions.clone());
        move |Path(id): Path<String>, headers: HeaderMap, body: Result<Bytes, BytesRejection>| {
            set_all(
                connect.clone(),
                write_timeout,
                sessions.clone(),
                id,
                headers,
                body,
            )
        }
    };
    let set_one = {
        let (connect, sessions) = (connect.clone(), sessions.clone());
        move |Path((id, profile)): Path<(String, String)>,
              headers: HeaderMap,
              body: Result<Bytes, BytesRejection>| {
            set_assignment(
                connect.clone(),
                write_timeout,
                sessions.clone(),
                id,
                profile,
                headers,
                body,
            )
        }
    };
    axum::Router::new()
        .route(
            "/api/instances/{id}/profiles/server",
            axum::routing::put(server_switch),
        )
        .route(
            "/api/instances/{id}/profiles/model",
            axum::routing::get(rows).put(set_all),
        )
        .route(
            "/api/instances/{id}/profiles/model/{profile}",
            axum::routing::put(set_one),
        )
}

/// `PUT /api/instances/{id}/profiles/server` `{"serverProfile": <name>}`: the
/// instance's server profile, as the signed-in user. The session and the body
/// answer first, so a body nothing can be made of (400
/// `server-profile-malformed`) reaches no manager. The record that names the
/// instance answers first too: an id it does not name is 404 with nothing
/// written, and a record the manager will not answer for the bff is 502 with
/// nothing written. The state that carries `running` is asked after the write.
async fn set_server_profile(
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
        Err(rejection) => return body_refusal(&rejection, "server-profile-malformed"),
    };
    let Some(profile) = one_string(&json_body(&headers, &body), "serverProfile") else {
        return error(StatusCode::BAD_REQUEST, "server-profile-malformed", &[]);
    };
    let user = identity.user;
    let ask = tokio::task::spawn_blocking(move || -> Result<_, ClientError> {
        Ok(write_server_profile(&connect, &user, &id, &profile))
    });
    answer(
        &sessions,
        &session,
        tokio::time::timeout(write_timeout, ask).await,
    )
}

/// `GET /api/instances/{id}/profiles/model`: the pool's rows as one instance's
/// section of `assignments.ini` makes them. The record that names the instance
/// is read before the files, so an id it does not name is 404 with no file read.
async fn model_rows(
    connect: Connect,
    timeout: Duration,
    sessions: Arc<Sessions>,
    id: String,
    headers: HeaderMap,
) -> Response {
    let Some((session, identity)) = sessions.current(&headers) else {
        return error(StatusCode::UNAUTHORIZED, "session-required", &[]);
    };
    let user = identity.user;
    let ask = tokio::task::spawn_blocking(move || -> Result<_, ClientError> {
        Ok(read_rows(&connect, &user, &id))
    });
    answer(
        &sessions,
        &session,
        tokio::time::timeout(timeout, ask).await,
    )
}

/// `PUT /api/instances/{id}/profiles/model/{profile}` `{"value": <word>}`: one
/// key of one instance's section. The body and the word answer first, so a body
/// nothing can be made of (400 `assignment-malformed`) and a word that is not
/// one of the four (422 `model-profile-bad-value`, field `value`) reach no
/// manager, and so does the path's `*` (404 `no-model-profile-named`).
async fn set_assignment(
    connect: Connect,
    write_timeout: Duration,
    sessions: Arc<Sessions>,
    id: String,
    profile: String,
    headers: HeaderMap,
    body: Result<Bytes, BytesRejection>,
) -> Response {
    let Some((session, identity)) = sessions.current(&headers) else {
        return error(StatusCode::UNAUTHORIZED, "session-required", &[]);
    };
    let body = match body {
        Ok(body) => body,
        Err(rejection) => return body_refusal(&rejection, "assignment-malformed"),
    };
    let Some(value) = one_string(&json_body(&headers, &body), "value") else {
        return error(StatusCode::BAD_REQUEST, "assignment-malformed", &[]);
    };
    if !VALUES.contains(&value.as_str()) {
        return refused_value(&value, "value");
    }
    if profile == STAR {
        return reply(
            StatusCode::NOT_FOUND,
            "no-model-profile-named",
            &[profile.as_str()],
            None,
        );
    }
    let user = identity.user;
    let ask = tokio::task::spawn_blocking(move || -> Result<_, ClientError> {
        Ok(write_assignment(&connect, &user, &id, &profile, &value))
    });
    answer(
        &sessions,
        &session,
        tokio::time::timeout(write_timeout, ask).await,
    )
}

/// `PUT /api/instances/{id}/profiles/model` `{"all": <inherit|none>}`: the `*`
/// of one instance's section, the whole pool. The body and the word answer
/// first, as the per-profile route's do, and `none` writes as much as `inherit`
/// does: it takes the `*` away, so every model profile of the pool is given to
/// the instance by its own key alone.
async fn set_all(
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
        Err(rejection) => return body_refusal(&rejection, "assignment-malformed"),
    };
    let Some(all) = one_string(&json_body(&headers, &body), "all") else {
        return error(StatusCode::BAD_REQUEST, "assignment-malformed", &[]);
    };
    if !ALLS.contains(&all.as_str()) {
        return refused_value(&all, "all");
    }
    let user = identity.user;
    let ask = tokio::task::spawn_blocking(move || -> Result<_, ClientError> {
        Ok(write_all(&connect, &user, &id, &all))
    });
    answer(
        &sessions,
        &session,
        tokio::time::timeout(write_timeout, ask).await,
    )
}

/// The answer of one route: the blocking task under the deadline, its own
/// refusal handed through as the response it built, and the manager's failures
/// answered on the list's rule ([`read_answer`]): 503 `client-unavailable`, 403
/// `session-not-administrator` ending the session, 502 `client-other`.
fn answer(sessions: &Arc<Sessions>, session: &str, outcome: Outcome) -> Response {
    read_answer(sessions, session, outcome, |inner| match inner {
        Ok(value) => axum::Json(value).into_response(),
        Err(Failure::Answer(response)) => *response,
        Err(Failure::User) => not_administrator(sessions, session),
    })
}

/// What one route's blocking body computes: the page's JSON, or why it cannot
/// answer.
type Answered = Result<Value, Failure>;

/// That, as it comes back through the deadline and the task: the body's answer,
/// the manager's failure, the task's panic, the deadline.
type Outcome = Result<
    Result<Result<Answered, ClientError>, tokio::task::JoinError>,
    tokio::time::error::Elapsed,
>;

/// The record and the state on a connect of the bff's own account, the write as
/// `user`: the instance's server profile. A record that already names
/// `profile` answers false, nothing changing; a record that cannot be read
/// answers the safe true with no state asked, and so an id it does not name is
/// 404 with nothing written. A record the manager will not answer for the bff is
/// 502, the service refusing the service, with nothing written.
fn write_server_profile(
    connect: &Connect,
    user: &str,
    id: &str,
    profile: &str,
) -> Result<Value, Failure> {
    let client = connect(None).map_err(|failure| own_refused(&failure))?;
    let row = match client.instances() {
        Ok(rows) => rows
            .into_iter()
            .find(|row| row.id.eq_ignore_ascii_case(id))
            .map(|row| row.server_profile),
        Err(failure) if matches!(failure, ClientError::NotAuthorized) => {
            return Err(own_refused(&failure))
        }
        Err(_) => {
            // The record of the instances cannot be read, so the bff cannot know
            // what the instance ran: the write stands, and the answer is the
            // safe true, with no state asked.
            let client = connect(Some(user)).map_err(|failure| refused(&failure, None))?;
            client
                .set_server_profile(id, profile)
                .map_err(|failure| refused(&failure, Some(SERVER_PROFILE_FIELD)))?;
            return Ok(json!({ "running": true }));
        }
    };
    let Some(server_profile) = row else {
        return Err(Failure::Answer(Box::new(no_instance())));
    };
    connect(Some(user))
        .map_err(|failure| refused(&failure, None))?
        .set_server_profile(id, profile)
        .map_err(|failure| refused(&failure, Some(SERVER_PROFILE_FIELD)))?;
    Ok(json!({
        "running": !server_profile
            .is_some_and(|held| held.eq_ignore_ascii_case(profile))
            && state_of(connect, id),
    }))
}

/// The instance's state, on the bff's own connect, asked once the write stands:
/// true for an instance that is up, and true when the state cannot be read, the
/// safe answer for a write that stands.
fn state_of(connect: &Connect, id: &str) -> bool {
    connect(None)
        .and_then(|admin| admin.status(id))
        .map(runs)
        .unwrap_or(true)
}

/// The pool's rows as one instance sees them: its section of `assignments.ini`
/// over the pool, and the `*` of that section as `all`.
fn read_rows(connect: &Connect, user: &str, id: &str) -> Result<Value, Failure> {
    let (record, on, off) = assign_target(connect, user, id)?;
    let section = ini::text::section_text(&record, id);
    let (all, given) = assign::rows(&text_of(&on), &text_of(&off), section.as_deref());
    let profiles = pool(&on, &off)
        .into_iter()
        .map(|row| {
            let assignment = given
                .iter()
                .find(|given| given.id.eq_ignore_ascii_case(&row.id));
            json!({
                "id": row.id,
                "engine": ENGINE_LLAMA_CPP,
                "engineAvailable": true,
                "enabled": row.enabled,
                "model": row.model,
                "value": assignment.map(|a| a.value.as_str()).unwrap_or("none"),
                "viaAll": assignment.map(|a| a.via_all).unwrap_or(false),
                "effective": assignment.map(|a| a.effective).unwrap_or(false),
            })
        })
        .collect::<Vec<_>>();
    Ok(json!({ "all": all, "profiles": profiles }))
}

/// One key of one instance's section, written, and the answer: `effective` is
/// what the record leaves the model profile on, as the manager left it, and
/// `running` is the instance's state, asked only when the write changed what the
/// instance is given.
fn write_assignment(
    connect: &Connect,
    user: &str,
    id: &str,
    profile: &str,
    value: &str,
) -> Result<Value, Failure> {
    let (client, record, on, off) = assign_files(connect, user, id)?;
    client
        .set_model_profile(id, profile, value)
        .map_err(|failure| refused(&failure, None))?;
    let (after, read_back) = read_back(&client, &record, id, profile, value)?;
    let (on, off) = (text_of(&on), text_of(&off));
    let section = ini::text::section_text(&after, id);
    let effective = assign::rows(&on, &off, section.as_deref())
        .1
        .into_iter()
        .find(|row| row.id.eq_ignore_ascii_case(profile))
        .is_some_and(|row| row.effective);
    // A record that cannot be read back leaves the write standing with its after
    // side unknown: the safe true, as a state that cannot be read is.
    let running =
        !read_back || (load_changed(&on, &off, &record, &after, id) && state_of(connect, id));
    Ok(json!({ "effective": effective, "running": running }))
}

/// The `*` of one instance's section, written, and the answer: `running` is the
/// instance's state, asked only when the write changed the model profiles the
/// instance is given. The whole pool's two words are never an override of a
/// profile of their own, so the change is the set alone.
fn write_all(connect: &Connect, user: &str, id: &str, all: &str) -> Result<Value, Failure> {
    let (client, record, on, off) = assign_files(connect, user, id)?;
    client
        .set_model_profile(id, STAR, all)
        .map_err(|failure| refused(&failure, None))?;
    let (after, read_back) = read_back(&client, &record, id, STAR, all)?;
    let (on, off) = (text_of(&on), text_of(&off));
    let running =
        !read_back || (load_changed(&on, &off, &record, &after, id) && state_of(connect, id));
    Ok(json!({ "running": running }))
}

/// `assignments.ini` as the manager left it after the write, and whether it came
/// from the manager: the answer is the record on the machine and not what the bff
/// computed. The write stands, so a read back that fails (and is not the manager
/// refusing the signed-in user) answers from the text the write would have left.
fn read_back(
    client: &Client,
    before: &str,
    id: &str,
    profile: &str,
    value: &str,
) -> Result<(String, bool), Failure> {
    match read(client, ASSIGNMENTS) {
        Ok(file) => Ok((text_of(&file), true)),
        Err(Failure::User) => Err(Failure::User),
        Err(_) => Ok((written(before, id, profile, value), false)),
    }
}

/// The record and the pool's two files, for a read of one instance's section:
/// the same three reads as [`assign_files`], with no client to write with.
fn assign_target(
    connect: &Connect,
    user: &str,
    id: &str,
) -> Result<(String, Option<ManagerFile>, Option<ManagerFile>), Failure> {
    let (_client, record, on, off) = assign_files(connect, user, id)?;
    Ok((record, on, off))
}

/// The record of `assignments.ini` and the pool's two files, read as `user`,
/// with the record of the instances, read on the connect of the bff's own
/// account, checking the id first: 404 `client-no-instance` for an id that
/// record does not name, asked before anything is read or written.
fn assign_files(
    connect: &Connect,
    user: &str,
    id: &str,
) -> Result<(Client, String, Option<ManagerFile>, Option<ManagerFile>), Failure> {
    let known = connect(None)
        .map_err(|failure| own_refused(&failure))?
        .instances()
        .map_err(|failure| own_refused(&failure))?
        .iter()
        .any(|row| row.id.eq_ignore_ascii_case(id));
    if !known {
        return Err(Failure::Answer(Box::new(no_instance())));
    }
    let client = connect(Some(user)).map_err(|failure| refused(&failure, None))?;
    let record = text_of(&read(&client, ASSIGNMENTS)?);
    let on = read(&client, MODELS)?;
    let off = read(&client, MODELS_DISABLED)?;
    Ok((client, record, on, off))
}

/// The text of `assignments.ini` as the manager leaves it after writing the key
/// `profile` of the section `id` to `value`: the manager's own rule, the key set
/// to the word, and `none` taking the key away.
fn written(before: &str, id: &str, profile: &str, value: &str) -> String {
    if value == "none" {
        ini::text::remove_key(before, id, profile)
    } else {
        ini::text::set_key(before, id, profile, value)
    }
}

/// The write changed what the instance is given: the set of model profiles
/// `pai_core::assign::rows` marks `effective` for its section differs on the two
/// sides of the write, and the instance's state is asked only then. A key that
/// lands where the model profile already stood (an `off` under a `*` of `off`, an
/// `inherit` nothing resolves through, an `on` or `off` under a `*` of `inherit`
/// that leaves the model profile on neither side) changes the text of
/// `assignments.ini` and not what the instance loads, so the page is never
/// offered a restart for nothing: the render emits the same sections.
fn load_changed(on: &str, off: &str, before: &str, after: &str, id: &str) -> bool {
    served_ids(on, off, before, id) != served_ids(on, off, after, id)
}

/// The model profiles the record `record` gives the instance `id`: the ids
/// `pai_core::assign::rows` marks `effective` for that section, the same rule
/// the rows and the render use.
fn served_ids(on: &str, off: &str, record: &str, id: &str) -> BTreeSet<String> {
    assign::rows(on, off, ini::text::section_text(record, id).as_deref())
        .1
        .into_iter()
        .filter(|row| row.effective)
        .map(|row| row.id)
        .collect()
}

/// The text of a file the manager answered: empty when it says the file is not
/// there.
fn text_of(file: &Option<ManagerFile>) -> String {
    file.clone().map(|file| file.text).unwrap_or_default()
}

/// One file through the manager, its failure the route's answer.
fn read(client: &Client, name: &str) -> Result<Option<ManagerFile>, Failure> {
    client
        .read_file(name)
        .map_err(|failure| refused(&failure, None))
}

/// The one string of a body under `key`: the object `{"<key>": <string>}`. A
/// body that is not an object, a missing key and a value that is not a string
/// are all none.
fn one_string(body: &Option<Value>, key: &str) -> Option<String> {
    body.as_ref()?
        .get(key)
        .and_then(Value::as_str)
        .map(str::to_string)
}

/// 404 for an id the record of the instances does not name.
fn no_instance() -> Response {
    error(StatusCode::NOT_FOUND, "client-no-instance", &[])
}

/// 422 for a word a route does not write, naming the field the page marks.
fn refused_value(value: &str, field: &str) -> Response {
    reply(
        StatusCode::UNPROCESSABLE_ENTITY,
        "model-profile-bad-value",
        &[value],
        Some(field),
    )
}

/// The manager's refusal of one call of these routes, as the page's answer, the
/// manager's detail staying out of the body. `field` is the field the page
/// marks, which only the server profile write has. The refusal of the
/// signed-in user is not answered here: it goes to [`read_answer`] as the
/// manager's failure, which ends the session ([`crate::model_profiles::not_administrator`]).
fn refused(failure: &ClientError, field: Option<&str>) -> Failure {
    use ClientError::*;
    let answer = match failure {
        NotAuthorized => return Failure::User,
        Unavailable => reply(
            StatusCode::SERVICE_UNAVAILABLE,
            "client-unavailable",
            &[],
            None,
        ),
        NoInstance => no_instance(),
        NoServerProfile(profile) => reply(
            StatusCode::NOT_FOUND,
            "client-no-server-profile",
            &[profile.as_str()],
            None,
        ),
        ServerProfileInUse(profile, instance) => reply(
            StatusCode::CONFLICT,
            "client-server-profile-in-use",
            &[profile.as_str(), instance.as_str()],
            None,
        ),
        AddressInUse(port) => reply(
            StatusCode::UNPROCESSABLE_ENTITY,
            "client-address-in-use",
            &[port.as_str()],
            field,
        ),
        UnresolvableHost(host) => reply(
            StatusCode::UNPROCESSABLE_ENTITY,
            "client-unresolvable-host",
            &[host.as_str()],
            field,
        ),
        Invalid(refusal) => {
            let args = refusal
                .values
                .iter()
                .map(String::as_str)
                .collect::<Vec<_>>();
            let status = if refusal.key == "no-model-profile-named" {
                StatusCode::NOT_FOUND
            } else {
                StatusCode::UNPROCESSABLE_ENTITY
            };
            reply(status, refusal.key, &args, field)
        }
        _ => reply(StatusCode::BAD_GATEWAY, "client-other", &[], None),
    };
    Failure::Answer(Box::new(answer))
}

/// Why one call of these routes failed, before it is answered: the answer the
/// page gets ([`refused`]), or the manager's refusal of the signed-in user,
/// which [`read_answer`] answers on the rule every route shares. The response
/// is boxed: it is a body, and these calls carry the reason beside a `Value`.
enum Failure {
    Answer(Box<Response>),
    User,
}

/// The answer to a failure of a call the bff makes on its OWN account (the
/// record of the instances, [`state_of`]'s state): the manager refused the
/// service, not the signed-in user, so 502 `client-not-authorized` and the
/// session lives on. Only a refusal of the user's own call ends a session
/// ([`refused`]).
fn own_refused(failure: &ClientError) -> Failure {
    use ClientError::*;
    let answer = match failure {
        NotAuthorized => reply(StatusCode::BAD_GATEWAY, "client-not-authorized", &[], None),
        Unavailable => reply(
            StatusCode::SERVICE_UNAVAILABLE,
            "client-unavailable",
            &[],
            None,
        ),
        _ => reply(StatusCode::BAD_GATEWAY, "client-other", &[], None),
    };
    Failure::Answer(Box::new(answer))
}

/// `{"key", "args"}` at `status`, with `"field"` when there is one to name.
fn reply(status: StatusCode, key: &str, args: &[&str], field: Option<&str>) -> Response {
    let mut doc = json!({ "key": key, "args": args });
    if let Some(field) = field {
        doc["field"] = json!(field);
    }
    (status, axum::Json(doc)).into_response()
}
