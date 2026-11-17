//! `GET /api/presets`: the preset list of the web UI's Models page, read
//! through the manager as the signed-in user.
//!
//! The list is the engine's two preset files, read over one connect made for
//! the session's user, so the manager re-checks their membership of `pai` at
//! every call: the sections of `presets.ini` (switched on, the file
//! llama-server reads) then the sections of `presets.ini.disabled` (switched
//! off), each group in its file order, and an id present in both files listed
//! once, as enabled, with the copy `presets.ini` holds: the id matched without
//! case, as the configurator's list does. A file the manager says is not there
//! is an empty group, so a machine with no preset file answers an empty list
//! and not an error.
//!
//! Every row carries the five fields the web UI reads: `{"id", "engine",
//! "engineAvailable", "enabled", "model"}`. `id` is the section as the file
//! spells it, `model` the section's `model` key as the file holds it (the
//! inline comment ended by the file's parser, `""` when the section has no
//! such key), `engine` the engine whose preset file holds the section and
//! `engineAvailable` that engine in `GET /api/instances`: llama.cpp, the only
//! engine there is.
//!
//! Every failure is `{"key": <strings key>, "args": []}`, never the manager's
//! detail: 401 `session-required` with no session (the manager is not even
//! connected to), 403 `session-not-administrator` when the manager refused the
//! signed-in user, which ends that session with the right that authorized it
//! as the server read's refusal does (`server.rs`), 503 `client-unavailable`
//! when there is no manager or it answers later than `timeout`, and 502
//! `client-other` for anything else.
//!
//! `PUT /api/presets/{id}/enabled` is the switch of one preset, the body
//! `{"enabled": <bool>}`. The section moves, unchanged, between the two files,
//! the file it LANDS IN written first; see [`set_enabled`].

use crate::origin::body_refusal;
use crate::server::{runs, WriteState};
use crate::session::{error, json_body, Sessions};
use crate::{Client, Connect};
use axum::extract::rejection::BytesRejection;
use axum::http::{HeaderMap, StatusCode};
use axum::response::{IntoResponse, Response};
use pai_core::client::{ClientError, ManagerFile};
use pai_core::{checks, ini};
use serde_json::{json, Value};
use std::sync::Arc;
use std::time::Duration;

/// The engine's file of switched-on presets, the one llama-server reads.
const PRESETS: &str = "presets.ini";

/// The engine's file of switched-off presets, never read by llama-server.
const PRESETS_DISABLED: &str = "presets.ini.disabled";

/// The header a `presets.ini.disabled` the configurator never wrote is created
/// with, so a file the switch off makes reads like one it wrote. The text is
/// the configurator's, in `crates/app/src/presets.rs`, which the bff does not
/// link: `the_disabled_header_is_the_configurators_one` in
/// `tests/presets_switch.rs` reads that file and keeps the two copies equal.
const DISABLED_FILE_HEADER: &str = "\
; Presets switched OFF in pai-config (the switch beside each preset in\r\n\
; the Models tab). Same format as presets.ini, but llama-server only reads\r\n\
; presets.ini, so nothing here is offered. Switching a preset back on moves its\r\n\
; section, unchanged, back to presets.ini.\r\n";

/// The answer to `GET /api/presets`: the list as a JSON array, or
/// `{"key", "args": []}`. The two files are read over one connect, as the
/// session's user, inside one blocking task, so the manager is asked once and
/// a refusal on the first file stops the read.
pub(crate) async fn list(
    connect: Connect,
    timeout: Duration,
    sessions: Arc<Sessions>,
    headers: HeaderMap,
) -> Response {
    let Some((session, identity)) = sessions.current(&headers) else {
        return error(StatusCode::UNAUTHORIZED, "session-required", &[]);
    };
    let user = identity.user;
    let ask = tokio::task::spawn_blocking(move || -> Result<_, ClientError> {
        let client = connect(Some(user.as_str()))?;
        let on = client.read_file(PRESETS)?;
        let off = client.read_file(PRESETS_DISABLED)?;
        Ok(rows(&on, &off))
    });
    match tokio::time::timeout(timeout, ask).await {
        Ok(Ok(Ok(rows))) => axum::Json(rows).into_response(),
        Ok(Ok(Err(ClientError::NotAuthorized))) => {
            sessions.end(&session);
            error(StatusCode::FORBIDDEN, "session-not-administrator", &[])
        }
        Ok(Ok(Err(ClientError::Unavailable))) | Err(_) => {
            error(StatusCode::SERVICE_UNAVAILABLE, "client-unavailable", &[])
        }
        Ok(Ok(Err(_))) | Ok(Err(_)) => error(StatusCode::BAD_GATEWAY, "client-other", &[]),
    }
}

/// The rows of the two files: the switched-on sections in their order, then
/// the switched-off ones in theirs, each id of `presets.ini.disabled` that
/// `presets.ini` also holds left out.
fn rows(on: &Option<ManagerFile>, off: &Option<ManagerFile>) -> Vec<Value> {
    let on = sections(on);
    let off: Vec<ini::Section> = sections(off)
        .into_iter()
        .filter(|s| !on.iter().any(|q| q.id.eq_ignore_ascii_case(&s.id)))
        .collect();
    on.into_iter()
        .map(|section| row(&section, true))
        .chain(off.into_iter().map(|section| row(&section, false)))
        .collect()
}

/// The sections of a file the manager answered: none when it says the file is
/// not there.
fn sections(file: &Option<ManagerFile>) -> Vec<ini::Section> {
    match file {
        Some(file) => ini::parse_sections(&file.text),
        None => Vec::new(),
    }
}

/// One row: the section's id and `model` key as the file holds them, the one
/// engine, and whether the section sits in the file llama-server reads.
fn row(section: &ini::Section, enabled: bool) -> Value {
    json!({
        "id": section.id,
        "engine": crate::ENGINE_LLAMA_CPP,
        "engineAvailable": true,
        "enabled": enabled,
        "model": section.keys.get("model").cloned().unwrap_or_default(),
    })
}

/// The answer of one switch.
enum Answer {
    /// The move is made, or the preset was already where it is asked for and
    /// nothing was written: the preset's state, and whether an instance is up.
    Moved { enabled: bool, running: bool },
    /// Neither preset file has a section named by the id.
    NotFound,
}

/// `PUT /api/presets/{id}/enabled` `{"enabled": <bool>}`: the on/off switch of
/// one preset, as the signed-in user. The answer is exactly `{"enabled",
/// "running"}`; with no session the manager is not even connected to, and a
/// body over `origin::MAX_BODY` is refused by the extractor before the session
/// is read (413, as the boot write).
///
/// The session and the body answer first, so a body nothing can be made of (400
/// `presets-malformed`) reaches no manager. The two files are then read once
/// each, and after the writes the file that makes a preset enabled is read
/// again, so the `enabled` the page gets is the text on the machine. The writes
/// and the instances' states are asked on the SAME
/// connect, made for the session's user, all of it in one blocking task. The id
/// is a section name, matched in any letter case as the list's ids are, never a
/// path: the two file names are the fixed ones. A name neither file holds is 404
/// `no-preset-named` ([`not_found`]), and nothing is written.
///
/// The section moves, unchanged, from the file that holds it to the other one,
/// and the file it LANDS IN is written FIRST: a section in both files reads as
/// enabled, so a move cut short leaves the preset in both, never in none. Each
/// write is the compare and swap, expecting the text this request read. A
/// disabled file that is not there is created with [`DISABLED_FILE_HEADER`] by
/// the plain write, because the compare and swap refuses a missing file. A
/// preset already where it is asked for answers 200 and writes nothing.
///
/// 409 `preset-changed-elsewhere` is the compare and swap's refusal. When it
/// answers the second write the preset sits in both files, which the next
/// switch of that preset mends. The manager's check of the moved section is 422
/// ([`field_of`]). A refusal of the signed-in user, at the connect, a read or a
/// write, is 403 and ends that session; the other answers keep it ([`switch_failure`]).
/// The 503 and the 504 follow the server file's rule, on the same shared state
/// as its save: while no write was asked, a time limit that runs out is 503
/// `client-unavailable`; from the moment a write was sent its outcome is
/// unknown, and the answer is 504 `client-timeout`.
///
/// `running` is what the page's "restart llama-server to apply" note is about:
/// the engine reads `presets.ini` at launch, so it is true when any instance is
/// up ([`running`]). The switch starts, stops and restarts nothing.
pub(crate) async fn set_enabled(
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
        Err(rejection) => return body_refusal(&rejection, "presets-malformed"),
    };
    let Some(enabled) = flag(&headers, &body) else {
        return error(StatusCode::BAD_REQUEST, "presets-malformed", &[]);
    };
    let user = identity.user;
    // The writes' state, shared with the task. From the moment the task sends
    // one, every answer that cannot see it is 504, never 503; a task whose
    // handler ran out of time with no write asked sends none at all.
    let state = Arc::new(WriteState::new());
    let mine = state.clone();
    let name = id.clone();
    let ask = tokio::task::spawn_blocking(move || switch(&connect, &user, &name, enabled, &mine));
    match tokio::time::timeout(timeout, ask).await {
        Ok(Ok(Ok(Answer::Moved { enabled, running }))) => {
            axum::Json(json!({ "enabled": enabled, "running": running })).into_response()
        }
        Ok(Ok(Ok(Answer::NotFound))) => not_found(&id),
        Ok(Ok(Err(failure))) => switch_failure(&sessions, &session, failure, state.sent()),
        // The answer cannot see the write, and the task keeps running.
        Err(_) => state.timed_out(),
        Ok(Err(_)) => error(StatusCode::BAD_GATEWAY, "client-other", &[]),
    }
}

/// The switch through the manager: one connect for `user`, both files read, the
/// section moved, the instances' states asked for `running`. The two files are
/// read once each for the move; the one that makes a preset enabled is read
/// once more after the writes, because the answer is the text on the machine
/// and not what the bff computed.
fn switch(
    connect: &Connect,
    user: &str,
    id: &str,
    enabled: bool,
    state: &WriteState,
) -> Result<Answer, ClientError> {
    let client = connect(Some(user))?;
    let on = client.read_file(PRESETS)?.map(|file| file.text);
    let off = client.read_file(PRESETS_DISABLED)?.map(|file| file.text);
    let section_on = ini::text::section_text(on.as_deref().unwrap_or(""), id);
    let section_off = ini::text::section_text(off.as_deref().unwrap_or(""), id);
    if section_on.is_none() && section_off.is_none() {
        return Ok(Answer::NotFound);
    }
    if enabled == section_on.is_some() {
        // Already where it is asked for: `presets.ini` is the home, so a
        // section in both files is on. The files are not written.
        return Ok(Answer::Moved {
            enabled,
            running: running(&client),
        });
    }
    let (source, destination, source_text, destination_text, section) = if enabled {
        (PRESETS_DISABLED, PRESETS, &off, &on, section_off)
    } else {
        (PRESETS, PRESETS_DISABLED, &on, &off, section_on)
    };
    let Some(section) = section else {
        // Unreachable: the source is the file that holds the section.
        return Ok(Answer::NotFound);
    };
    let current = source_text.as_deref().unwrap_or("");
    let landed =
        ini::text::replace_section(destination_text.as_deref().unwrap_or(""), id, &section);
    if !state.may_write() {
        // The handler ran out of time with no write asked and answered 503, so
        // this switch is over and the files stay as the browser was told. The
        // value returned is never read.
        return Ok(Answer::NotFound);
    }
    match destination_text {
        // The destination exists: the compare and swap, so a write that lands
        // between the read and this call is refused, and nothing moves.
        Some(expected) => client.write_file_if(destination, &landed, expected)?,
        // It is not there, and the compare and swap refuses a missing file: the
        // plain write creates it. A new disabled file carries the
        // configurator's header; a new `presets.ini` needs none.
        None => {
            let base = if destination == PRESETS_DISABLED {
                DISABLED_FILE_HEADER
            } else {
                ""
            };
            let created = ini::text::replace_section(base, id, &section);
            client.write_file(destination, &created, false)?;
        }
    }
    // Then the file the preset leaves, with its section taken out.
    let left = ini::text::delete_section(current, id).unwrap_or_else(|| current.to_string());
    // No time-limit check here: the state is Cancelled only when no write was
    // asked, and the destination write was asked, so the handler that ran out
    // of time answered 504 with that write in flight. This one goes on and is
    // asked, as the 504 told the browser the outcome is unknown.
    client.write_file_if(source, &left, current)?;
    // The move is not reported as the bff computed it: the file that makes a
    // preset enabled is read back after the writes, so `enabled` is the text
    // that is on the machine.
    let on = client
        .read_file(PRESETS)?
        .map(|file| file.text)
        .unwrap_or_default();
    Ok(Answer::Moved {
        enabled: ini::text::section_text(&on, id).is_some(),
        running: running(&client),
    })
}

/// Whether any instance is up, the one the switch answers as `running`: the
/// states are asked in the list's order and the asking stops at the first
/// instance that is up, and a state that cannot be read answers true, the safe
/// answer after a move that stands.
fn running(client: &Client) -> bool {
    match client.list_instances() {
        Ok(ids) => ids
            .iter()
            .any(|instance| client.status(instance).map(runs).unwrap_or(true)),
        Err(_) => true,
    }
}

/// The answer to a switch the manager could not do, `write_asked` the shared
/// state's: a write sent, so a stream that died there leaves its outcome
/// unknown, is 504 `client-timeout`, and not 503 `client-unavailable`. The
/// compare and swap found its file changed: 409 `preset-changed-elsewhere`, and
/// when it answered the second write the preset is in both files. The manager's
/// check of the moved section is 422 with its key, its values and the field to
/// mark ([`field_of`]); the manager's detail stays out of the body. The manager
/// refused the signed-in user: 403 `session-not-administrator` (the sign-in's
/// key for the same fact) and the session ends with the right that authorized
/// it. The other answers are the bff's news, not a refusal of the user, so the
/// session lives on.
fn switch_failure(
    sessions: &Arc<Sessions>,
    session: &str,
    failure: ClientError,
    write_asked: bool,
) -> Response {
    match failure {
        ClientError::Changed => error(StatusCode::CONFLICT, "preset-changed-elsewhere", &[]),
        ClientError::Invalid(refusal) => refused(
            refusal.key,
            &refusal.values,
            field_of(refusal.key, &refusal.values).as_deref(),
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

/// The field of a 422: the preset key the form marks, beside the check's key and
/// values. The load-on-startup pair counts `load-on-startup` against the lowest
/// `ModelsMax` and names no key in its values, so the pair maps to that key. A
/// check that names the key it is about marks it, as its `args[0]`. A refusal
/// about the preset's name (`invalid-preset-id`, `preset-name-taken`) has no key
/// of the section to mark, so the answer carries no field at all.
fn field_of(key: &str, args: &[String]) -> Option<String> {
    match key {
        "load-on-startup-over-max" | "load-on-startup-unlimited" => {
            Some("load-on-startup".to_string())
        }
        "invalid-preset-id" | "preset-name-taken" => None,
        _ => args.first().cloned(),
    }
}

/// The 422 answer: the check's key, its values, and `field` when there is a key
/// to mark.
fn refused(key: &str, args: &[String], field: Option<&str>) -> Response {
    let mut doc = json!({ "key": key, "args": args });
    if let Some(field) = field {
        doc["field"] = json!(field);
    }
    (StatusCode::UNPROCESSABLE_ENTITY, axum::Json(doc)).into_response()
}

/// The 404 answer: no preset file has a section named `id`. The id comes back as
/// the only arg when it is a name a preset can carry at all; a name that cannot
/// be a section header (a newline, a bracket, a path) is echoed nowhere, as the
/// id check refuses it. A run of dots is the one name the check allows and no
/// preset carries, `.` and `..` naming no preset, so it is not echoed either.
fn not_found(id: &str) -> Response {
    let echo = checks::preset_id(id).is_ok() && !id.chars().all(|c| c == '.');
    let args: Vec<&str> = if echo { vec![id] } else { Vec::new() };
    (
        StatusCode::NOT_FOUND,
        axum::Json(json!({ "key": "no-preset-named", "args": args })),
    )
        .into_response()
}

/// The `enabled` flag of a JSON body `{"enabled": <bool>}`, or None: the flag
/// alone, of the JSON type, is a bool.
fn flag(headers: &HeaderMap, body: &[u8]) -> Option<bool> {
    json_body(headers, body)?
        .get("enabled")
        .and_then(Value::as_bool)
}
