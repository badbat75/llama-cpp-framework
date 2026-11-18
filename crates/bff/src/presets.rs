//! `GET /api/presets`: the preset list of the web UI's Models page, read
//! through the manager as the signed-in user.
//!
//! The list is the engine's two preset files, read over one connect made for
//! the session's user, so the manager re-checks their membership of `pai` at
//! every call: the sections of `models.ini` (switched on, the file
//! llama-server reads) then the sections of `models.ini.disabled` (switched
//! off), each group in its file order, and an id present in both files listed
//! once, as enabled, with the copy `models.ini` holds: the id matched without
//! case, as the configurator's list does. A file the manager says is not there
//! is an empty group, so a machine with no preset file answers an empty list
//! and not an error.
//!
//! llama-server's global `[*]` section is not a preset: it is the defaults
//! every preset starts from, so it is never a row, and the switch moves
//! nothing for it ([`switch`]): moved to the disabled file it would stop
//! applying to every model with no row left in the list to bring it back.
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
//! `PUT /api/presets/{id}` is the save of one preset, the body
//! `{"revision", "values"}`: the section's keys are rewritten in place in the
//! file that holds it; see [`save`].
//! `PUT /api/presets/{id}/enabled` is the switch of one preset, the body
//! `{"enabled": <bool>}`. The section moves, unchanged, between the two files,
//! the file it LANDS IN written first; see [`set_enabled`].

use crate::origin::body_refusal;
use crate::server::{revision, runs, TimedOut, WriteState};
use crate::session::{error, json_body, Sessions};
use crate::{Client, Connect};
use axum::body::Bytes;
use axum::extract::rejection::BytesRejection;
use axum::http::{HeaderMap, StatusCode};
use axum::response::{IntoResponse, Response};
use pai_core::client::{ClientError, ManagerFile};
use pai_core::{checks, ini};
use serde_json::{json, Value};
use std::sync::Arc;
use std::time::{Duration, Instant};

/// The engine's file of switched-on presets, the one llama-server reads.
const PRESETS: &str = "models.ini";

/// The engine's file of switched-off presets, never read by llama-server.
const PRESETS_DISABLED: &str = "models.ini.disabled";

/// The header a `models.ini.disabled` the configurator never wrote is created
/// with, so a file the switch off makes reads like one it wrote. The text is
/// the configurator's, in `crates/app/src/presets.rs`, which the bff does not
/// link: `the_disabled_header_is_the_configurators_one` in
/// `tests/presets_switch.rs` reads that file and keeps the two copies equal.
const DISABLED_FILE_HEADER: &str = "\
; Presets switched OFF in pai-config (the switch beside each preset in\r\n\
; the Models tab). Same format as models.ini, but llama-server only reads\r\n\
; models.ini, so nothing here is offered. Switching a preset back on moves its\r\n\
; section, unchanged, back to models.ini.\r\n";

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
    read_answer(
        &sessions,
        &session,
        tokio::time::timeout(timeout, ask).await,
        |rows| axum::Json(rows).into_response(),
    )
}

/// The answer of a read through the manager: `outcome` is the blocking task
/// under the deadline, and `answer` is what the page gets from the value the
/// task read. The list and the section read answer their failures the same way,
/// on the same rule as the server read (`server.rs`): 503 `client-unavailable`
/// when there is no manager, it went away, or it answers later than the
/// deadline; 403 `session-not-administrator` when the manager refused the
/// signed-in user, which ends that session with the right that authorized it;
/// 502 `client-other` for anything else, which keeps the session. The manager's
/// detail stays out of the body.
fn read_answer<T>(
    sessions: &Arc<Sessions>,
    session: &str,
    outcome: Result<
        Result<Result<T, ClientError>, tokio::task::JoinError>,
        tokio::time::error::Elapsed,
    >,
    answer: impl FnOnce(T) -> Response,
) -> Response {
    match outcome {
        Ok(Ok(Ok(value))) => answer(value),
        Ok(Ok(Err(ClientError::NotAuthorized))) => {
            sessions.end(session);
            error(StatusCode::FORBIDDEN, "session-not-administrator", &[])
        }
        Ok(Ok(Err(ClientError::Unavailable))) | Err(_) => {
            error(StatusCode::SERVICE_UNAVAILABLE, "client-unavailable", &[])
        }
        Ok(Ok(Err(_))) | Ok(Err(_)) => error(StatusCode::BAD_GATEWAY, "client-other", &[]),
    }
}

/// The rows of the two files: the switched-on sections in their order, then
/// the switched-off ones in theirs, each id of `models.ini.disabled` that
/// `models.ini` also holds left out, and llama-server's global section left
/// out of both.
fn rows(on: &Option<ManagerFile>, off: &Option<ManagerFile>) -> Vec<Value> {
    let on: Vec<ini::Section> = sections(on)
        .into_iter()
        .filter(|s| !is_global(&s.id))
        .collect();
    let off: Vec<ini::Section> = sections(off)
        .into_iter()
        .filter(|s| !is_global(&s.id))
        .filter(|s| !on.iter().any(|q| q.id.eq_ignore_ascii_case(&s.id)))
        .collect();
    on.into_iter()
        .map(|section| row(&section, true))
        .chain(off.into_iter().map(|section| row(&section, false)))
        .collect()
}

/// Whether a section id is llama-server's global `[*]`, the defaults every
/// preset starts from. It is not a preset: the list has no row for it, and the
/// switch has nothing to move.
fn is_global(id: &str) -> bool {
    id == "*"
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
/// "running"}`; the session is read first, so with no session the manager is
/// not even connected to (401), and only a signed-in request sees a body over
/// `origin::MAX_BODY` refused by the extractor with a 413 (as the boot write).
///
/// The session and the body answer first, so a body nothing can be made of (400
/// `presets-malformed`) reaches no manager. The two files are then read once
/// each, and after the writes the file that makes a preset enabled is read
/// again, so the `enabled` the page gets is the text on the machine. The writes
/// and the instances' states are asked on the SAME
/// connect, made for the session's user, all of it in one blocking task. The id
/// is a section name, matched in any letter case as the list's ids are, never a
/// path: the two file names are the fixed ones. A name neither file holds is 404
/// `no-preset-named` ([`not_found`]), and nothing is written; so is
/// llama-server's global `[*]`, which is not a preset.
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
/// unknown, and the answer is 504 `client-timeout`. Once BOTH writes landed the
/// move stands in the files, so a limit that runs out on the file read back
/// after them, or on the states that follow, is the switch's 200: `enabled` the
/// value the move produced, and `running` true, the safe answer for a state the
/// bff cannot read. `write_timeout` bounds every manager call of the move, the
/// two reads and the read back with them: the write's own limit, longer than
/// [`crate::MANAGER_TIMEOUT`], the limit a read is given. A read back that
/// fails answers the same as one that runs out, and a refusal of the signed-in
/// user there stays the 403 that ends the session.
///
/// `running` is what the page's "restart llama-server to apply" note is about:
/// the engine reads `models.ini` at launch, so it is true when any instance is
/// up ([`running`]). The switch starts, stops and restarts nothing.
pub(crate) async fn set_enabled(
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
        Err(rejection) => return body_refusal(&rejection, "model-profiles-malformed"),
    };
    let Some(enabled) = flag(&headers, &body) else {
        return error(StatusCode::BAD_REQUEST, "model-profiles-malformed", &[]);
    };
    let user = identity.user;
    // The writes' state, shared with the task. From the moment the task sends
    // one, every answer that cannot see it is 504, never 503; a task whose
    // handler ran out of time with no write asked sends none at all; once both
    // writes landed the state is Done and the answer is the switch's 200.
    let state = Arc::new(WriteState::new());
    let mine = state.clone();
    let name = id.clone();
    let ask = tokio::task::spawn_blocking(move || switch(&connect, &user, &name, enabled, &mine));
    match tokio::time::timeout(write_timeout, ask).await {
        Ok(Ok(Ok(Answer::Moved { enabled, running }))) => {
            axum::Json(json!({ "enabled": enabled, "running": running })).into_response()
        }
        Ok(Ok(Ok(Answer::NotFound))) => not_found(&id),
        Ok(Ok(Err(failure))) => switch_failure(&sessions, &session, failure, state.sent()),
        // The answer cannot see the write, and the task keeps running: 503 with
        // no write asked, 504 with one sent and its outcome unknown. Done is a
        // move that landed in both files, so the answer carries the `enabled`
        // the move produced and `running` true, the state the bff cannot read.
        Err(_) => match state.timed_out() {
            TimedOut::Cancelled => {
                error(StatusCode::SERVICE_UNAVAILABLE, "client-unavailable", &[])
            }
            TimedOut::Asked => error(StatusCode::GATEWAY_TIMEOUT, "client-timeout", &[]),
            TimedOut::Done => {
                axum::Json(json!({ "enabled": enabled, "running": true })).into_response()
            }
        },
        Ok(Err(_)) => error(StatusCode::BAD_GATEWAY, "client-other", &[]),
    }
}

/// The switch through the manager: one connect for `user`, both files read, the
/// section moved, the instances' states asked for `running`. The two files are
/// read once each for the move; the one that makes a preset enabled is read
/// once more after the writes, because the answer is the text on the machine
/// and not what the bff computed. llama-server's global section is answered
/// [`Answer::NotFound`] with nothing written, as the list has no row for it.
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
    if is_global(id) {
        // llama-server's global section: not a preset, no row in the list, and
        // nothing to move: switched off it would stop applying to every model.
        return Ok(Answer::NotFound);
    }
    let section_on = ini::text::section_text(on.as_deref().unwrap_or(""), id);
    let section_off = ini::text::section_text(off.as_deref().unwrap_or(""), id);
    if section_on.is_none() && section_off.is_none() {
        return Ok(Answer::NotFound);
    }
    if enabled == section_on.is_some() {
        // Already where it is asked for: `models.ini` is the home, so a
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
        // configurator's header; a new `models.ini` needs none.
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
    // The move is made: both files carry it, so a time limit that runs out on
    // the read back that follows finds the switch done, and answers its 200.
    state.landed();
    // The move is not reported as the bff computed it: the file that makes a
    // preset enabled is read back after the writes, so `enabled` is the text
    // that is on the machine. A read back that fails is not the move failing:
    // the files carry it, so the answer is the value the move produced and
    // `running` true, never the 503 of a manager that was never reached. A
    // refusal of the signed-in user is the one answer that ends the session.
    let on = match client.read_file(PRESETS) {
        Ok(file) => file.map(|file| file.text).unwrap_or_default(),
        Err(ClientError::NotAuthorized) => return Err(ClientError::NotAuthorized),
        Err(_) => {
            return Ok(Answer::Moved {
                enabled,
                running: true,
            })
        }
    };
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

/// The answer to a preset write the manager could not do, `write_asked` the
/// shared state's: a write sent, so a stream that died there leaves its outcome
/// unknown, is 504 `client-timeout`, and not 503 `client-unavailable`. The
/// compare and swap found its file changed: 409 `preset-changed-elsewhere`. The
/// manager's check of the section it was asked to write is 422 with its key, its
/// values and the field to mark ([`field_of`]); the manager's detail stays out of
/// the body. The manager
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
        ClientError::Changed => error(StatusCode::CONFLICT, "model-profile-changed-elsewhere", &[]),
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
/// values. One rule per check, so a refusal names the card's field whoever asks.
/// A check that names the key it is about marks it, as its `args[0]`. The checks
/// that name a VALUE and not a key, the load-on-startup pair (its `args` are the
/// total and `ModelsMax`) and `preset-no-model` (its `args` are the preset's
/// name), name the key their value belongs to: `load-on-startup` and `model`.
/// So does the pair `override-no-pattern` and `override-no-device`, whose
/// `args[0]` is the position of the rule inside the value, and
/// `moe-cache-with-tensor-split`, which carries no args: each marks the key of
/// its own value. A refusal about the preset's name (`invalid-preset-id`,
/// `preset-name-taken`) has no key of the section to mark, so the answer carries
/// no field at all.
fn field_of(key: &str, args: &[String]) -> Option<String> {
    match key {
        "load-on-startup-over-max" | "load-on-startup-unlimited" => {
            Some("load-on-startup".to_string())
        }
        "model-profile-no-model" => Some("model".to_string()),
        "fit-target-format" => Some("fit-target".to_string()),
        "override-no-pattern" | "override-no-device" => Some("override-tensor".to_string()),
        "moe-cache-with-tensor-split" => Some("moe-cache-mib".to_string()),
        "invalid-model-profile-id" | "model-profile-name-taken" => None,
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
        axum::Json(json!({ "key": "no-model-profile-named", "args": args })),
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

/// `GET /api/presets/{id}`: the document of one preset, the entry of the
/// contract (`fe/briefs/api-contract.md`, "the preset page"). The page shows
/// the keys and the values as the file holds them and runs no check of its own,
/// so this only reads, over one connect made for the session's user:
/// `models.ini` first, then `models.ini.disabled`, as the list reads them.
///
/// A section is matched in any letter case, as the list's ids are, and
/// llama-server's global `[*]` is never an answer: nothing in either file of
/// that name is 404 `no-preset-named` ([`not_found`]), the id echoed only when
/// it is a name a preset can carry. The failures are the list's, on the same
/// rule as the server read: 401 `session-required` with the manager not even
/// connected to, 403 `session-not-administrator` when the manager refuses the
/// signed-in user, which ends that session and stops the read, 503
/// `client-unavailable` when there is no manager, it goes away, or it answers
/// later than `timeout`, and 502 `client-other` for anything else, which keeps
/// the session.
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
    let name = id.clone();
    let ask = tokio::task::spawn_blocking(move || -> Result<_, ClientError> {
        let client = connect(Some(user.as_str()))?;
        let on = client.read_file(PRESETS)?;
        let off = client.read_file(PRESETS_DISABLED)?;
        Ok(document(&on, &off, &name, &key))
    });
    read_answer(
        &sessions,
        &session,
        tokio::time::timeout(timeout, ask).await,
        |document| match document {
            // Neither file has a section of that name: no preset, nothing to read.
            None => not_found(&id),
            Some(document) => axum::Json(document).into_response(),
        },
    )
}

/// The document of `id` in the two files, the enabled one first: a section in
/// both is the enabled copy, as the list answers the same id. The seven fields
/// the page reads: `id` as the file spells it, the list row's engine pair,
/// `enabled` for the file the section was found in, `values` the section's keys
/// and values as that file's parser reads them (verbatim: no canonicalization
/// and no check, so a preset the manager would refuse today is one the page can
/// still show), `projectUrl` the section's `; url = ` comment, read by the
/// configurator's reader in `pai-core` (`ini::project_url_of`), empty when the
/// section has none, and `revision` ([`section_revision`]).
fn document(
    on: &Option<ManagerFile>,
    off: &Option<ManagerFile>,
    id: &str,
    key: &[u8; 16],
) -> Option<Value> {
    for (file, name, enabled) in [(on, PRESETS, true), (off, PRESETS_DISABLED, false)] {
        let Some(text) = file.as_ref().map(|file| file.text.as_str()) else {
            continue;
        };
        for section in sections(file) {
            if is_global(&section.id) || !section.id.eq_ignore_ascii_case(id) {
                continue;
            }
            return Some(json!({
                "id": section.id,
                "engine": crate::ENGINE_LLAMA_CPP,
                "engineAvailable": true,
                "enabled": enabled,
                "values": section.keys,
                "projectUrl": ini::project_url_of(&section.comments),
                "revision": section_revision(name, text, &section.id, key),
            }));
        }
    }
    None
}

/// A section's revision: the server read's keyed hash (`server::revision`),
/// over the file's name, a newline, and the section's own text as the file
/// holds it: from its header through its last non-blank line, WITHOUT that last
/// line's own ending (`\n` or `\r\n`). The endings BETWEEN the lines are the
/// file's.
///
/// The file's name is in it, so the same text in the other file, the preset
/// switched off, is another preset to the page. The section's text alone is in
/// it, keys, values and comments in the file's spelling and order: a neighbour
/// added to the file leaves it where it was, and a key added to the section
/// moves it at once. The blank lines and the last line's ending are cut because
/// they belong to no section: a file whose last section has no final break, the
/// same file with that break added, and that file with a section appended below,
/// all are the same preset to the page.
fn section_revision(file: &str, text: &str, id: &str, key: &[u8; 16]) -> String {
    let section = ini::text::section_text(text, id).unwrap_or_default();
    let mut end = section.len();
    for line in section.split_inclusive('\n').rev() {
        if !line.trim_end().is_empty() {
            break;
        }
        end -= line.len();
    }
    let body = &section[..end];
    let body = body.strip_suffix("\r\n").unwrap_or(body);
    let body = body.strip_suffix('\n').unwrap_or(body);
    revision(&format!("{file}\n{body}"), key)
}

/// The section a save starts from: the file that holds it, the text of that
/// file as the manager read it (the compare and swap expects it), and the
/// connect made for the signed-in user, reused for the write. `NotFound` when
/// neither preset file has a section of that name.
enum Read {
    Found {
        client: Client,
        file: String,
        text: String,
    },
    NotFound,
}

/// The file that holds the section `id`, read as the signed-in user over one
/// connect: `models.ini` first, so a section in both files is the enabled one,
/// as the list and the section read answer the same id. A file the manager says
/// is not there holds nothing, and llama-server's global `[*]` is not a preset.
fn read_section(connect: &Connect, user: &str, id: &str) -> Result<Read, ClientError> {
    let client = connect(Some(user))?;
    for file in [PRESETS, PRESETS_DISABLED] {
        let Some(text) = client.read_file(file)?.map(|file| file.text) else {
            continue;
        };
        if !is_global(id) && ini::text::section_text(&text, id).is_some() {
            return Ok(Read::Found {
                client,
                file: file.to_string(),
                text,
            });
        }
    }
    Ok(Read::NotFound)
}

/// `PUT /api/presets/{id}`: the save of one preset, as the signed-in user. The
/// body is the object [`parse_body`] reads, the answer is exactly `{"revision",
/// "running"}`: `revision` the revision of the section as it now stands in the
/// file, so the card can go on editing, and `running` whether any instance is
/// up ([`running`]), asked only when the section lives in the file llama-server
/// reads: a preset saved in the disabled file is not offered, so it is not
/// running. The session is read first, so with no session the manager is not
/// even connected to (401), and only a signed-in request sees a body over
/// `origin::MAX_BODY` refused by the extractor with a 413 (as the boot write).
///
/// The session and the body answer first, so a body nothing can be made of (400
/// `presets-malformed`, bytes of another content type included: [`json_body`]
/// is the gate, as the server save's and the switch's) and a value no check
/// allows (422, [`field_of`]) reach no manager. The file is read next: a name
/// no file holds, and llama-server's global `[*]`, are 404 `no-preset-named`
/// ([`not_found`]) with nothing written.
/// The section's revision ([`section_revision`]) is then compared with the one
/// the body carries: a card that started from an older text is 409
/// `preset-changed-elsewhere` with nothing written, and a body that asks for the
/// text the file already holds answers 200 and writes nothing.
///
/// The write is the compare and swap, expecting the text this request read, so
/// the file is never changed under a card that read it earlier: 409
/// `preset-changed-elsewhere` when the file moved in between. The manager's
/// check of the section is 422 ([`field_of`]). A refusal of the signed-in user,
/// at the connect, the read or the write, is 403 and ends that session; the
/// other answers keep it ([`switch_failure`]).
///
/// The two limits are the server save's ([`crate::server::save`], the same
/// [`WriteState`]): the reads answer within `timeout` and a time limit that runs
/// out while no write was asked is 503 `client-unavailable`; from the moment a
/// write was sent its outcome is unknown, and the answer is 504
/// `client-timeout`, within `write_timeout`. A write only starts while the
/// answer is still wanted, so a task left behind by a deadline writes nothing.
/// Once the write landed the preset is the one the body asked for, so a limit
/// that runs out on the states that follow, and a state read that fails, are the
/// 200 with `running` true: the safe answer for a state the bff cannot read. A
/// body that changes nothing has no write to ask, so the state is settled as
/// done before the instances' state is asked, and a limit that runs out there
/// is that same 200, never the 503 that tells the page an edit was lost while
/// the file is the one the body asked for.
///
/// The save is handed both limits, where the server save is handed the longer
/// one alone: its reads answer within `timeout`, as every other read does.
#[allow(clippy::too_many_arguments)]
pub(crate) async fn save(
    connect: Connect,
    timeout: Duration,
    write_timeout: Duration,
    sessions: Arc<Sessions>,
    id: String,
    headers: HeaderMap,
    key: [u8; 16],
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
    let (sent, edits) = match parse_body(&body) {
        Ok(pair) => pair,
        Err(BodyError::Malformed) => {
            return error(StatusCode::BAD_REQUEST, "model-profiles-malformed", &[])
        }
        Err(BodyError::Refused { key, args, field }) => return refused(&key, &args, Some(&field)),
    };
    let user = identity.user;
    let name = id.clone();
    let started = Instant::now();
    let ask = tokio::task::spawn_blocking(move || read_section(&connect, &user, &name));
    let found = match tokio::time::timeout(timeout, ask).await {
        Err(_elapsed) => return error(StatusCode::SERVICE_UNAVAILABLE, "client-unavailable", &[]),
        Ok(Err(_join)) => return error(StatusCode::BAD_GATEWAY, "client-other", &[]),
        Ok(Ok(Err(failure))) => return switch_failure(&sessions, &session, failure, false),
        Ok(Ok(Ok(found))) => found,
    };
    let Read::Found { client, file, text } = found else {
        return not_found(&id);
    };
    if section_revision(&file, &text, &id, &key) != sent {
        return error(StatusCode::CONFLICT, "model-profile-changed-elsewhere", &[]);
    }
    let new_text = apply(&edits, &text, &id);
    let written = section_revision(&file, &new_text, &id, &key);
    let changed = new_text != text;
    let enabled = file == PRESETS;
    let state = Arc::new(WriteState::new());
    let mine = state.clone();
    let ask = tokio::task::spawn_blocking(move || -> Result<bool, ClientError> {
        if changed {
            if !mine.may_write() {
                return Ok(true);
            }
            client.write_file_if(&file, &new_text, &text)?;
            mine.landed();
        } else {
            // Nothing to write, the text the body asked for is the text the
            // file holds: the save is settled before the instances' state is
            // asked, so a limit that runs out there finds it done, as a write
            // that landed does.
            mine.nothing_to_write();
        }
        Ok(if enabled { running(&client) } else { false })
    });
    let left = write_timeout.saturating_sub(started.elapsed());
    match tokio::time::timeout(left, ask).await {
        Ok(Ok(Ok(running))) => saved(&written, running),
        Ok(Ok(Err(failure))) => switch_failure(&sessions, &session, failure, state.sent()),
        Ok(Err(_join)) => error(StatusCode::BAD_GATEWAY, "client-other", &[]),
        Err(_elapsed) => match state.timed_out() {
            // The write was never asked: the file is the one that was read.
            TimedOut::Cancelled => {
                error(StatusCode::SERVICE_UNAVAILABLE, "client-unavailable", &[])
            }
            TimedOut::Asked => error(StatusCode::GATEWAY_TIMEOUT, "client-timeout", &[]),
            // The write landed, or the body asked for the text the file
            // already holds and nothing was written: the preset is the one the
            // body asked for and only the instances' state stayed unknown.
            TimedOut::Done => saved(&written, true),
        },
    }
}

/// The 200 of a save: exactly `{"revision", "running"}`, the section's revision
/// as the file now holds it, and whether an instance is up.
fn saved(revision: &str, running: bool) -> Response {
    (
        StatusCode::OK,
        axum::Json(json!({ "revision": revision, "running": running })),
    )
        .into_response()
}

/// Why [`parse_body`] refuses the body of `PUT /api/presets/{id}`: the two
/// answers the save gives before it reads a file, as the server save's
/// [`crate::server::BodyAnswer`] answers them.
enum BodyError {
    /// The body is not a preset save at all: 400 `presets-malformed`.
    Malformed,
    /// A value was refused: 422 `{key, args, field}`, the check's key and
    /// values, and the INI key the form marks (the table of one rule per key).
    Refused {
        key: String,
        args: Vec<String>,
        field: String,
    },
}

/// The edits of a body, one per key of its `values` map: a `None` value is the
/// key to remove. [`parse_body`] hands them sorted by key: the map's own order
/// is not one every build can rely on (a `BTreeMap`, so sorted by key, while
/// serde_json's `preserve_order` is off, and insertion-ordered once the
/// workspace turns it on through `pai-app`), and the bytes written must not
/// depend on it. A key the body lists twice is one edit, at its last value.
type Edits = Vec<(String, Option<String>)>;

/// The whole answer of a body read on its own, the file's text handed in: the
/// new text of the file and the revision the body carried, or one of
/// [`BodyError`]'s refusals. The route answers through [`parse_body`] and
/// [`apply`], with the file read between the two; this is the shape the body's
/// own tests in `src/tests/preset_body.rs` read.
#[cfg(test)]
#[derive(Debug)]
pub(crate) enum PresetBodyAnswer {
    /// The new text of the preset file, and the revision the body carried: the
    /// save goes to the file that still holds it, and a body that changed the
    /// revision of the card is not written at all.
    Text { text: String, revision: String },
    /// The body is not a preset save at all: 400 `presets-malformed`.
    Malformed,
    /// A value was refused: 422 `{key, args, field}`, the check's key and
    /// values, and the INI key the form marks (the table of one rule per key).
    Refused {
        key: String,
        args: Vec<String>,
        field: String,
    },
}

/// The body of `PUT /api/presets/{id}`, applied to `file_text`, the text of the
/// preset file that holds the section named by `id`: the object
/// `{"revision": <the revision of the card the edit started from>, "values":
/// {"<key>": <value>, ...}}`, a `null` value removing that key. The checks of
/// [`parse_body`] answer first, then [`apply`] writes the edits: every check
/// answers before the first edit is applied, so a refused body writes nothing.
///
/// The save calls [`parse_body`] and [`apply`] with the file's text in between,
/// so a body nothing can be made of is answered before a preset file is read.
/// This is the whole of it, the tests' entry to it: it is built only under
/// `cfg(test)`, because no route calls it, only the body's own tests in
/// `src/tests/preset_body.rs` do.
#[cfg(test)]
pub(crate) fn body_to_text(body: &[u8], file_text: &str, id: &str) -> PresetBodyAnswer {
    let Ok(body) = serde_json::from_slice::<Value>(body) else {
        return PresetBodyAnswer::Malformed;
    };
    match parse_body(&body) {
        Ok((revision, edits)) => PresetBodyAnswer::Text {
            text: apply(&edits, file_text, id),
            revision,
        },
        Err(BodyError::Malformed) => PresetBodyAnswer::Malformed,
        Err(BodyError::Refused { key, args, field }) => {
            PresetBodyAnswer::Refused { key, args, field }
        }
    }
}

/// The body of `PUT /api/presets/{id}`, read and checked, the file it edits
/// looked at nowhere: the revision it carries and its edits, sorted by key
/// ([`Edits`]) whatever the order the body's `values` map came in.
///
/// Every key of `values` must be in `checks::WEB_PRESET_KEYS` (the list lives in
/// `pai-core`, pinned against `pai-app` `presets::render_section`; this crate
/// keeps no copy of it), matched in the exact spelling of the list: a key in
/// another letter case is as unknown as a name no reader knows, because
/// llama-server knows a key only in its own case. Every set value is a string
/// that is not empty after trimming spaces and tabs, holds no control character
/// other than a tab, and starts and ends with a space the reader keeps: the
/// parser (`pai_core::ini`) reads a value with `str::trim`, which knows every
/// Unicode space, so a value that starts or ends with one beyond a space or a
/// tab would be read back shorter than it went. Every set value goes through
/// the configurator's `checks::comment_markers` for its key: 422
/// `comment-markers` listing every key whose value holds a marker, with the
/// field the first of them. A value is written trimmed.
fn parse_body(body: &Value) -> Result<(String, Edits), BodyError> {
    // The shape: `{revision: string, values: object}`. The revision is only
    // carried through: this function answers no 409, the save that reads the
    // file back before it writes is the one that refuses a stale edit.
    let Some(revision) = body.get("revision").and_then(Value::as_str) else {
        return Err(BodyError::Malformed);
    };
    let Some(values) = body.get("values").and_then(Value::as_object) else {
        return Err(BodyError::Malformed);
    };
    // The keys first: one name the web may not write refuses the whole body.
    if !values
        .keys()
        .all(|key| checks::WEB_PRESET_KEYS.contains(&key.as_str()))
    {
        return Err(BodyError::Malformed);
    }
    // The values, read in full before one byte is written: a string is the text
    // to write, trimmed of the spaces and tabs no reader keeps, a `null` the key
    // to remove, and nothing else is a value at all. A control character ends
    // the line in the file, so a value holds none of them beyond a tab.
    let mut edits: Edits = Vec::with_capacity(values.len());
    let mut markers: Vec<String> = Vec::new();
    for (key, value) in values {
        match value {
            Value::Null => edits.push((key.clone(), None)),
            Value::String(text) => {
                let text = text.trim_matches([' ', '\t']);
                // A Unicode space beyond a space or a tab at one of the ends:
                // the writer keeps that byte in the file, while the parser's
                // `str::trim` reads the value without it, so the value would
                // come back shorter than it went.
                let space_at_end = |c: char| c.is_whitespace() && c != ' ' && c != '\t';
                if text.is_empty()
                    || text.chars().any(|c| c.is_control() && c != '\t')
                    || text.starts_with(space_at_end)
                    || text.ends_with(space_at_end)
                {
                    return Err(BodyError::Malformed);
                }
                // `;` and `#` end a value for every reader of the file, so a
                // value holding one reloads as less than what was written.
                if checks::comment_markers(key, text).is_err() {
                    markers.push(key.clone());
                }
                edits.push((key.clone(), Some(text.to_string())));
            }
            _ => return Err(BodyError::Malformed),
        }
    }
    if !markers.is_empty() {
        markers.sort();
        let field = markers.first().cloned().unwrap_or_default();
        return Err(BodyError::Refused {
            key: "comment-markers".to_string(),
            args: markers,
            field,
        });
    }
    // The edits go to the file sorted by key, so the text `apply` writes is the
    // same whatever order the body's `values` map came in ([`Edits`]).
    edits.sort_by(|a, b| a.0.cmp(&b.0));
    Ok((revision.to_string(), edits))
}

/// The edits of a body, applied to `file_text`: they go to the section named by
/// `id`, and to no other, in the key's own spelling and the section matched in
/// any letter case: `pai_core::ini::text::set_key` for a string (a line that
/// spells the key as given and already holds the value is left byte for byte, a
/// line that does not is rewritten whole, and a key the section holds twice is
/// collapsed onto the first occurrence) and `text::remove_key` for a `null`.
fn apply(edits: &Edits, file_text: &str, id: &str) -> String {
    let mut text = file_text.to_string();
    for (key, value) in edits {
        text = match value {
            Some(value) => ini::text::set_key(&text, id, key, value),
            None => ini::text::remove_key(&text, id, key),
        };
    }
    text
}
