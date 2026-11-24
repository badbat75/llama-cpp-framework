//! `GET /api/profiles/model`: the model-profile list of the web UI's Models page, read
//! through the manager as the signed-in user.
//!
//! The list is the engine's two model profile files, read over one connect made for
//! the session's user, so the manager re-checks their membership of `pai` at
//! every call: the sections of `models.ini` (switched on, the file
//! llama-server reads) then the sections of `models.ini.disabled` (switched
//! off), each group in its file order, and an id present in both files listed
//! once, as enabled, with the copy `models.ini` holds: the id matched without
//! case, as the configurator's list does. A file the manager says is not there
//! is an empty group, so a machine with no model profile file answers an empty list
//! and not an error.
//!
//! llama-server's global `[*]` section is not a model profile: it is the defaults
//! every model profile starts from, so it is never a row, and the switch moves
//! nothing for it ([`switch`]): moved to the disabled file it would stop
//! applying to every model with no row left in the list to bring it back.
//!
//! Every row carries the six fields the web UI reads: `{"id", "engine",
//! "engineAvailable", "enabled", "model", "assignments"}`. `id` is the section as the file
//! spells it, `model` the section's `model` key as the file holds it (the
//! inline comment ended by the file's parser, `""` when the section has no
//! such key), `engine` the engine whose model profile file holds the section and
//! `engineAvailable` that engine in `GET /api/instances`: llama.cpp, the only
//! engine there is.
//!
//! `assignments` is the same answer as
//! `GET /api/instances/{id}/profiles/model` (`instance_profiles.rs`), read from
//! the instance side: one entry per instance the manager's record of
//! `assignments.ini` gives this model profile to, by a key of its own or through
//! the section's `*`, each with the `instance`, the `value` of its key, `viaAll`
//! when the `*` speaks for it, and `effective` where the render gives the model
//! profile to that instance (`pai_core::assign::rows`, one rule with the
//! render). The instances are the manager's list, in its order, and a row no
//! instance speaks of has an empty list.
//!
//! Every failure is `{"key": <strings key>, "args": []}`, never the manager's
//! detail: 401 `session-required` with no session (the manager is not even
//! connected to), 403 `session-not-administrator` when the manager refused the
//! signed-in user, which ends that session with the right that authorized it
//! as the server read's refusal does (`server.rs`), 503 `client-unavailable`
//! when there is no manager or it answers later than `timeout`, and 502
//! `client-other` for anything else.
//!
//! `PUT /api/profiles/model/{id}` is the save of one model profile, the body
//! `{"revision", "values"}`: the section's keys are rewritten in place in the
//! file that holds it; see [`save`].
//! `PUT /api/profiles/model/{id}/enabled` is the switch of one model profile, the body
//! `{"enabled": <bool>}`. The section moves, unchanged, between the two files,
//! the file it LANDS IN written first; see [`set_enabled`].
//! `GET /api/profiles/model/{id}/model-info` is the layer facts of one model
//! profile's model, relayed from the manager's `ModelInfoAs`; see
//! [`model_info`].
//! `POST /api/profiles/model` creates one model profile, a copy of one the pool
//! holds or a new one from a pick, and it is born switched off and assigned
//! nothing; see [`create`]. `DELETE /api/profiles/model/{id}` removes one the
//! pool holds switched off and assigned nothing; see [`remove`].

use crate::hf::{self, Hf};
use crate::origin::body_refusal;
use crate::server::{revision, runs, timeout_answer, WriteState};
use crate::session::{error, json_body, Sessions};
use crate::{Client, Connect};
use axum::body::Bytes;
use axum::extract::rejection::BytesRejection;
use axum::http::{HeaderMap, StatusCode};
use axum::response::{IntoResponse, Response};
use pai_core::client::{ClientError, ManagerFile, ModelInfoReply, ModelProfileSource};
use pai_core::{assign, checks, ini};
use serde_json::{json, Value};
use std::collections::{BTreeMap, BTreeSet};
use std::sync::Arc;
use std::time::{Duration, Instant};

/// The engine's file of switched-on model profiles, the one llama-server reads.
pub(crate) const MODELS: &str = "models.ini";

/// The engine's file of switched-off model profiles, never read by llama-server.
pub(crate) const MODELS_DISABLED: &str = "models.ini.disabled";

/// The manager's record of which instance is given which model profile. The bff
/// reads it for the list's rows and for the instances a move offers to restart.
const ASSIGNMENTS: &str = "assignments.ini";

/// The header a `models.ini.disabled` the configurator never wrote is created
/// with, so a file the switch off makes reads like one it wrote. The text is
/// the configurator's, in `crates/app/src/model_profiles.rs`, which the bff does not
/// link: `the_disabled_header_is_the_configurators_one` in
/// `tests/model_profiles_switch.rs` reads that file and keeps the two copies equal.
const DISABLED_FILE_HEADER: &str = "\
; Model profiles switched OFF in pai-config (the switch beside each model profile in\r\n\
; the Models tab). Same format as models.ini, but llama-server only reads\r\n\
; models.ini, so nothing here is offered. Switching a model profile back on moves its\r\n\
; section, unchanged, back to models.ini.\r\n";

/// The answer to `GET /api/profiles/model`: the list as a JSON array, or
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
        let on = client.read_file(MODELS)?;
        let off = client.read_file(MODELS_DISABLED)?;
        let assignments = client.read_file(ASSIGNMENTS)?;
        Ok(rows(&on, &off, &given(&client, &on, &off, &assignments)?))
    });
    read_answer(
        &sessions,
        &session,
        tokio::time::timeout(timeout, ask).await,
        |rows| axum::Json(rows).into_response(),
    )
}

/// The manager refused the signed-in user: 403 `session-not-administrator`, and
/// the session ends, so the page goes back to the sign-in page. Every route that
/// acts as the signed-in user gives the refusal here: [`read_answer`] and the
/// association routes ([`crate::instance_profiles`]) alike.
pub(crate) fn not_administrator(sessions: &Sessions, session: &str) -> Response {
    sessions.end(session);
    error(StatusCode::FORBIDDEN, "session-not-administrator", &[])
}

/// The answer of a read through the manager: `outcome` is the blocking task
/// under the deadline, and `answer` is what the page gets from the value the
/// task read. The list, the section read and the association routes answer
/// their failures here, on the same rule as the server read (`server.rs`):
/// 503 `client-unavailable` when there is no manager, it went away, or it
/// answers later than the deadline; 403 `session-not-administrator` when the
/// manager refused the signed-in user, ending that session ([`not_administrator`]);
/// 502 `client-other` for anything else, which keeps the session. The manager's
/// detail stays out of the body.
pub(crate) fn read_answer<T>(
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
        Ok(Ok(Err(ClientError::NotAuthorized))) => not_administrator(sessions, session),
        Ok(Ok(Err(ClientError::Unavailable))) | Err(_) => {
            error(StatusCode::SERVICE_UNAVAILABLE, "client-unavailable", &[])
        }
        Ok(Ok(Err(_))) | Ok(Err(_)) => error(StatusCode::BAD_GATEWAY, "client-other", &[]),
    }
}

/// One model profile of the pool, as the two files hold it: the `id` as the
/// first file that holds it spells it, its `model` key, and whether it sits in
/// the file llama-server reads.
pub(crate) struct PoolRow {
    pub(crate) id: String,
    pub(crate) model: String,
    pub(crate) enabled: bool,
}

/// The pool's model profiles in the two files: the switched-on sections in their
/// order, then the switched-off ones in theirs, each id of `models.ini.disabled`
/// that `models.ini` also holds left out, and llama-server's global section left
/// out of both.
pub(crate) fn pool(on: &Option<ManagerFile>, off: &Option<ManagerFile>) -> Vec<PoolRow> {
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
        .map(|section| PoolRow {
            model: model_of(&section),
            id: section.id,
            enabled: true,
        })
        .chain(off.into_iter().map(|section| PoolRow {
            model: model_of(&section),
            id: section.id,
            enabled: false,
        }))
        .collect()
}

/// The `model` key of a section, as the file holds it (`""` when it has none).
fn model_of(section: &ini::Section) -> String {
    section.keys.get("model").cloned().unwrap_or_default()
}

/// One instance's assignment of one model profile, as the list carries it under
/// that model profile: the `instance` whose section of `assignments.ini` speaks
/// of it, the `value` of its key, `viaAll` when the `*` speaks for it, and
/// `effective` where the render gives it to the instance.
struct Given {
    instance: String,
    value: String,
    via_all: bool,
    effective: bool,
}

/// Every instance's assignment of every model profile of the pool, under the
/// model profile's id as the pool spells it, lowered: the rows
/// `pai_core::assign::rows` gives for one instance, kept for the model profiles
/// that instance's section speaks of, by a key of its own or through the `*`.
/// The `value` of a model profile the section has no key of its own is the `*`
/// as that section spells it. The instances are the sections of the record that
/// the manager lists, so the rows under one model profile are in the instances'
/// own order. A record, a list, or a record read that cannot be read fails the
/// list: an answer of empty `assignments` would tell the page that no instance
/// serves anything.
fn given(
    client: &Client,
    on: &Option<ManagerFile>,
    off: &Option<ManagerFile>,
    assignments: &Option<ManagerFile>,
) -> Result<BTreeMap<String, Vec<Given>>, ClientError> {
    let record = assignments
        .clone()
        .map(|file| file.text)
        .unwrap_or_default();
    let (on, off) = (
        on.clone().map(|file| file.text).unwrap_or_default(),
        off.clone().map(|file| file.text).unwrap_or_default(),
    );
    let mut out: BTreeMap<String, Vec<Given>> = BTreeMap::new();
    // The manager's list, lowered: a record that names an instance the manager
    // does not list says nothing about a machine that no longer has it.
    let listed: BTreeSet<String> = client
        .instances()?
        .into_iter()
        .map(|instance| instance.id.to_lowercase())
        .collect();
    let mut ids = record_ids(&record);
    ids.sort_unstable_by_key(|a| a.to_lowercase());
    for instance in ids {
        if !listed.contains(&instance.to_lowercase()) {
            continue;
        }
        let section = ini::text::section_text(&record, &instance);
        // The `*` of this instance's section, as the section spells it.
        let all = section.as_deref().and_then(|text| {
            ini::parse_sections(text)
                .into_iter()
                .find_map(|section| section.keys.get("*").cloned())
        });
        for row in assign::rows(&on, &off, section.as_deref()).1 {
            if row.value == "none" && !row.via_all {
                // The section says nothing about this model profile, with the
                // `*` and without it: the row has nothing to carry.
                continue;
            }
            let value = if row.via_all {
                all.clone().unwrap_or(row.value)
            } else {
                row.value
            };
            out.entry(row.id.to_lowercase()).or_default().push(Given {
                instance: instance.clone(),
                value,
                via_all: row.via_all,
                effective: row.effective,
            });
        }
    }
    Ok(out)
}

/// The rows of the two files, in [`pool`]'s order, each with the instances
/// [`given`] lists it under it.
fn rows(
    on: &Option<ManagerFile>,
    off: &Option<ManagerFile>,
    given: &BTreeMap<String, Vec<Given>>,
) -> Vec<Value> {
    pool(on, off)
        .into_iter()
        .map(|row| {
            json!({
                "id": row.id,
                "engine": crate::ENGINE_LLAMA_CPP,
                "engineAvailable": true,
                "enabled": row.enabled,
                "model": row.model,
                "assignments": given
                    .get(&row.id.to_lowercase())
                    .map(|rows| {
                        rows.iter()
                            .map(|given| {
                                json!({
                                    "instance": given.instance,
                                    "value": given.value,
                                    "viaAll": given.via_all,
                                    "effective": given.effective,
                                })
                            })
                            .collect::<Vec<_>>()
                    })
                    .unwrap_or_default(),
            })
        })
        .collect()
}

/// Whether a section id is llama-server's global `[*]`, the defaults every
/// model profile starts from. It is not a model profile: the list has no row for it, and the
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

/// The answer of one switch.
enum Answer {
    /// The move is made, or the model profile was already where it is asked for and
    /// nothing was written: the model profile's state, the instances the page
    /// offers to restart (`running` is their list not being empty), and that
    /// list.
    Moved {
        enabled: bool,
        running: bool,
        restart: Vec<String>,
    },
    /// Neither model profile file has a section named by the id.
    NotFound,
}

/// `PUT /api/profiles/model/{id}/enabled` `{"enabled": <bool>}`: the on/off switch of
/// one model profile, as the signed-in user. The answer is exactly `{"enabled",
/// "running"}`; the session is read first, so with no session the manager is
/// not even connected to (401), and only a signed-in request sees a body over
/// `origin::MAX_BODY` refused by the extractor with a 413 (as the boot write).
///
/// The session and the body answer first, so a body nothing can be made of (400
/// `model-profiles-malformed`) reaches no manager. The two files are then read once
/// each, and after the writes the file that makes a model profile enabled is read
/// again, so the `enabled` the page gets is the text on the machine. The writes
/// and the instances' states are asked on the SAME
/// connect, made for the session's user, all of it in one blocking task. The id
/// is a section name, matched in any letter case as the list's ids are, never a
/// path: the two file names are the fixed ones. A name neither file holds is 404
/// `no-model-profile-named` ([`not_found`]), and nothing is written; so is
/// llama-server's global `[*]`, which is not a model profile.
///
/// The section moves, unchanged, from the file that holds it to the other one,
/// and the file it LANDS IN is written FIRST: a section in both files reads as
/// enabled, so a move cut short leaves the model profile in both, never in none. Each
/// write is the compare and swap, expecting the text this request read. A
/// disabled file that is not there is created with [`DISABLED_FILE_HEADER`] by
/// the plain write, because the compare and swap refuses a missing file. A
/// model profile already where it is asked for answers 200 and writes nothing.
///
/// 409 `model-profile-changed-elsewhere` is the compare and swap's refusal. When it
/// answers the second write the model profile sits in both files, which the next
/// switch of that model profile mends. The manager's check of the moved section is 422
/// ([`field_of`]). A refusal of the signed-in user, at the connect, a read or a
/// write, is 403 and ends that session; the other answers keep it ([`switch_failure`]).
/// The 503 and the 504 follow the server file's rule, on the same shared state
/// as its save: while no write was asked, a time limit that runs out is 503
/// `client-unavailable`; from the moment a write was sent its outcome is
/// unknown, and the answer is 504 `client-timeout`. Once BOTH writes landed the
/// move stands in the files, so a limit that runs out on the file read back
/// after them, or on the states that follow, is the switch's 200: `enabled` the
/// value the move produced, `running` true and no instance named, the safe
/// answer for a state the bff cannot read. `write_timeout` bounds every manager
/// call of the move, the three reads and the read back with them: the write's
/// own limit, longer than [`crate::MANAGER_TIMEOUT`], the limit a read is given.
/// A read back that fails answers the same as one that runs out, and a refusal
/// of the signed-in user there stays the 403 that ends the session.
///
/// `restart` is what the page's "restart llama-server to apply" note is about,
/// and `running` is that list not being empty: the engine reads `models.ini` at
/// launch, so the instances named are the ones the record of `assignments.ini`
/// gives this model profile to, effective in the pool as the move leaves it, and
/// up, of the instances the manager lists ([`restarted`]): a record section
/// naming an instance that is gone is a stale record, not a machine. The switch
/// starts, stops and restarts nothing.
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
        Ok(Ok(Ok(Answer::Moved {
            enabled,
            running,
            restart,
        }))) => axum::Json(json!({
            "enabled": enabled,
            "running": running,
            "restart": restart,
        }))
        .into_response(),
        Ok(Ok(Ok(Answer::NotFound))) => not_found(&id),
        Ok(Ok(Err(failure))) => switch_failure(&sessions, &session, failure, state.sent()),
        // The answer cannot see the write, and the task keeps running: 503 with
        // no write asked, 504 with one sent and its outcome unknown. Done is a
        // move that landed in both files, so the answer carries the `enabled`
        // the move produced, `running` true and no instance named: the state of
        // the instances the move serves stayed unknown, so the page says to
        // restart.
        Err(_) => timeout_answer(
            state.timed_out(),
            axum::Json(json!({
                "enabled": enabled,
                "running": true,
                "restart": [],
            }))
            .into_response(),
        ),
        Ok(Err(_)) => error(StatusCode::BAD_GATEWAY, "client-other", &[]),
    }
}

/// The switch through the manager: one connect for `user`, both files read, the
/// section moved, the instances' states asked for `running`. The two files are
/// read once each for the move; the one that makes a model profile enabled is read
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
    let on = client.read_file(MODELS)?.map(|file| file.text);
    let off = client.read_file(MODELS_DISABLED)?.map(|file| file.text);
    if is_global(id) || checks::model_profile_id(id).is_err() {
        // llama-server's global section: not a model profile, no row in the list,
        // and nothing to move: switched off it would stop applying to every model.
        // A name that cannot be a section header is the same 404, answered before
        // the records are read: no section of that name can exist anywhere.
        return Ok(Answer::NotFound);
    }
    // The record of which instance is given which model profile, read once: the
    // instances the move offers to restart are its sections, and the after side
    // of the move is computed from this same read with the switch flipped.
    let record = client
        .read_file(ASSIGNMENTS)?
        .map(|file| file.text)
        .unwrap_or_default();
    let section_on = ini::text::section_text(on.as_deref().unwrap_or(""), id);
    let section_off = ini::text::section_text(off.as_deref().unwrap_or(""), id);
    // The instances the record gives the model profile to, in the pool as it is
    // now: the before side of the move.
    let served_before = served(
        &record,
        id,
        on.as_deref().unwrap_or(""),
        off.as_deref().unwrap_or(""),
    );
    if section_on.is_none() && section_off.is_none() {
        return Ok(Answer::NotFound);
    }
    if enabled == section_on.is_some() {
        // Already where it is asked for: `models.ini` is the home, so a
        // section in both files is on. The files are not written, and the
        // instances the move offers to restart are the ones it is effective
        // on now.
        let (running, restart) = served_states(&client, &served_before);
        return Ok(Answer::Moved {
            enabled,
            running,
            restart,
        });
    }
    let (source, destination, source_text, destination_text, section) = if enabled {
        (MODELS_DISABLED, MODELS, &off, &on, section_off)
    } else {
        (MODELS, MODELS_DISABLED, &on, &off, section_on)
    };
    let Some(section) = section else {
        // Unreachable: the source is the file that holds the section.
        return Ok(Answer::NotFound);
    };
    let current = source_text.as_deref().unwrap_or("");
    let landed =
        ini::text::replace_section(destination_text.as_deref().unwrap_or(""), id, &section);
    // The text a destination that is not there is created with, and the text the
    // destination carries once the move is made: the after side of the record,
    // for the instances the move offers to restart.
    let created = ini::text::replace_section(
        if destination == MODELS_DISABLED {
            DISABLED_FILE_HEADER
        } else {
            ""
        },
        id,
        &section,
    );
    let destination_after = if destination_text.is_some() {
        landed.clone()
    } else {
        created.clone()
    };
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
            client.write_file(destination, &created, false)?;
        }
    }
    // Then the file the model profile leaves, with its section taken out.
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
    // model profile enabled is read back after the writes, so `enabled` is the text
    // that is on the machine. A read back that fails is not the move failing:
    // the files carry it, so the answer is the value the move produced and
    // `running` true with no instance named, never the 503 of a manager that was
    // never reached. A refusal of the signed-in user is the one answer that ends
    // the session.
    let on = match client.read_file(MODELS) {
        Ok(file) => file.map(|file| file.text).unwrap_or_default(),
        Err(ClientError::NotAuthorized) => return Err(ClientError::NotAuthorized),
        Err(_) => {
            return Ok(Answer::Moved {
                enabled,
                running: true,
                restart: Vec::new(),
            })
        }
    };
    // The pool as the move leaves it, for the after side of the record.
    let (on, off) = if enabled {
        (on, left.clone())
    } else {
        (left.clone(), destination_after)
    };
    let mut candidates = served_before;
    candidates.extend(served(&record, id, &on, &off));
    let (running, restart) = served_states(&client, &candidates);
    Ok(Answer::Moved {
        enabled: ini::text::section_text(&on, id).is_some(),
        running,
        restart,
    })
}

/// The instances of `record` (`assignments.ini`) the pool of `on` and `off` gives
/// `profile` to: one per section of the record, the id as the record spells it,
/// kept when `pai_core::assign::rows` makes the model profile effective on that
/// instance. An instance whose section says nothing of the model profile, or says
/// it is off, is not running it and is never offered to restart.
fn served(record: &str, profile: &str, on: &str, off: &str) -> BTreeSet<String> {
    let mut out = BTreeSet::new();
    for instance in record_ids(record) {
        let section = ini::text::section_text(record, &instance);
        let effective = assign::rows(on, off, section.as_deref())
            .1
            .into_iter()
            .any(|row| row.id.eq_ignore_ascii_case(profile) && row.effective);
        if effective {
            out.insert(instance);
        }
    }
    out
}

/// The ids of the sections of `assignments.ini`, llama-server's global `[*]`
/// aside, in order and once each in any letter case.
fn record_ids(record: &str) -> Vec<String> {
    let mut seen: Vec<String> = Vec::new();
    for section in ini::parse_sections(record) {
        if is_global(&section.id) || seen.iter().any(|id| id.eq_ignore_ascii_case(&section.id)) {
            continue;
        }
        seen.push(section.id);
    }
    seen
}

/// The states of `candidates`, the ones that are up, in their own order: the
/// instances a page offers to restart after a model profile write. `None` when
/// a state cannot be read: the page gets the safe answer, `running` true with
/// no instance named ([`served_states`]). An id the manager does not list is
/// left out before its state is asked: a section of `assignments.ini` naming an
/// instance that is gone is a stale record, not a machine, and asking its state
/// answers the service manager's refusal, not an instance to restart. The list
/// is asked only when there is a state to ask: a profile served nowhere asks
/// for nothing.
fn restarted(client: &Client, candidates: &BTreeSet<String>) -> Option<Vec<String>> {
    if candidates.is_empty() {
        return Some(Vec::new());
    }
    let known = client.list_instances().ok()?;
    let mut out = Vec::new();
    for instance in candidates {
        if !known.iter().any(|id| id.eq_ignore_ascii_case(instance)) {
            continue;
        }
        if runs(client.status(instance).ok()?) {
            out.push(instance.clone());
        }
    }
    Some(out)
}

/// `running` and `restart` for one model profile write: the instances that
/// serve it and are up, and `running` their list not being empty. A state that
/// cannot be read answers `running` true with no instance named, the safe
/// answer after a write that stands: at most it costs a needless restart.
fn served_states(client: &Client, candidates: &BTreeSet<String>) -> (bool, Vec<String>) {
    match restarted(client, candidates) {
        Some(restart) => (!restart.is_empty(), restart),
        None => (true, Vec::new()),
    }
}

/// The answer to a model profile write the manager could not do, `write_asked` the
/// shared state's: a write sent, so a stream that died there leaves its outcome
/// unknown, is 504 `client-timeout`, and not 503 `client-unavailable`. The
/// compare and swap found its file changed: 409 `model-profile-changed-elsewhere`. The
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

/// The field of a 422: the model profile key the form marks, beside the check's key and
/// values. One rule per check, so a refusal names the card's field whoever asks.
/// A check that names the key it is about marks it, as its `args[0]`. The checks
/// that name a VALUE and not a key, the load-on-startup pair (its `args` are the
/// total and `ModelsMax`) and `model-profile-no-model` and `model-profile-model-missing`
/// (their `args` are the model profile's name, the latter's with the instance and
/// the file), name the key their value belongs to: `load-on-startup` and `model`.
/// So does the `override-*` quartet (`override-no-pattern`, `override-no-device`,
/// `override-tensor-space` and `override-bad-device`: their `args[0]` is the
/// position of the rule inside the value), and
/// `moe-cache-with-tensor-split`, which carries no args, and
/// `tensor-split-format`, whose `args[0]` is the whole value: each marks the
/// key of its own value. `model-profile-device-outside` names the model profile,
/// the instance, the key and the device, so it marks `args[2]`, the key.
/// A refusal about the model profile's name (`invalid-model-profile-id`,
/// `model-profile-name-taken`), and the memory check (its `args` are the model
/// profile, an instance and two memory figures), have no key of the section to
/// mark, so the answer carries no field at all.
fn field_of(key: &str, args: &[String]) -> Option<String> {
    match key {
        "load-on-startup-over-max" | "load-on-startup-unlimited" => {
            Some("load-on-startup".to_string())
        }
        "model-profile-no-model" | "model-profile-model-missing" => Some("model".to_string()),
        "fit-target-format" => Some("fit-target".to_string()),
        "override-no-pattern"
        | "override-no-device"
        | "override-tensor-space"
        | "override-bad-device" => Some("override-tensor".to_string()),
        "tensor-split-format" => Some("tensor-split".to_string()),
        "model-profile-device-outside" => args.get(2).cloned(),
        "moe-cache-with-tensor-split" => Some("moe-cache-mib".to_string()),
        "invalid-model-profile-id" | "model-profile-name-taken" | "model-profile-over-memory" => {
            None
        }
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

/// The 404 answer: no model profile file has a section named `id`. The id comes back as
/// the only arg when it is a name a model profile can carry at all; a name that cannot
/// be a section header (a newline, a bracket, a path) is echoed nowhere, as the
/// id check refuses it. A run of dots is the one name the check allows and no
/// model profile carries, `.` and `..` naming no model profile, so it is not echoed either.
fn not_found(id: &str) -> Response {
    let echo = checks::model_profile_id(id).is_ok() && !id.chars().all(|c| c == '.');
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

/// `POST /api/profiles/model` `{"id", "from"}`, a copy of the pool's model
/// profile `from`, or `{"id", "values"}`, a new one from a pick ([`pick`]): the
/// answer is 201 `{"id"}` once the manager wrote it. The session is read first,
/// so with no session the manager is not even connected to, and the body's shape
/// answers next, as on the switch and on the server profile's create: a body
/// nothing can be made of is 400 `model-profiles-malformed` ([`requested`]) and
/// reaches no manager at all.
///
/// The write is then one call of one connect made for the session's user, under
/// `write_timeout`, the write's own limit: the manager is the one that checks
/// the name, the values and both pool files, being the one that writes, and its
/// refusals are answered by [`create_failure`]. It writes the section at the end
/// of `models.ini.disabled`: the list's row for the new model profile is
/// `enabled: false` with no assignments, and a create never overwrites a name
/// either file holds, which is what makes a retried create answer 409.
///
/// The time limit's answer turns on one fact, the server file's save's rule:
/// while the call was never handed over the manager it is 503
/// `client-unavailable`, the manager not reached; from the moment it was, the
/// write's outcome is unknown and the answer is 504 `client-timeout`, the
/// session kept. A create whose write landed while the reply stayed blind
/// answers its own 201.
pub(crate) async fn create(
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
        Err(rejection) => return body_refusal(&rejection, "model-profiles-malformed"),
    };
    let Some(requested) = requested(&headers, &body) else {
        return error(StatusCode::BAD_REQUEST, "model-profiles-malformed", &[]);
    };
    let user = identity.user;
    let Requested { id, source } = requested;
    let answer = id.clone();
    // The write's state, shared with the task: from the moment the call is
    // handed over the manager its outcome is unknown, so every answer that
    // cannot see it is 504, never the 503 that tells the page nothing changed.
    let state = Arc::new(WriteState::new());
    let mine = state.clone();
    let ask = tokio::task::spawn_blocking(move || -> Result<(), ClientError> {
        let client = connect(Some(user.as_str()))?;
        // Lost to a handler that already ran out of time, no call is sent: the
        // 503 it answered stands and the pool is what it was.
        if !mine.may_write() {
            return Ok(());
        }
        client.create_model_profile(&id, &source)?;
        mine.landed();
        Ok(())
    });
    match tokio::time::timeout(write_timeout, ask).await {
        Ok(Ok(Ok(()))) => {
            (StatusCode::CREATED, axum::Json(json!({ "id": answer }))).into_response()
        }
        Ok(Ok(Err(failure))) => create_failure(&sessions, &session, failure, state.sent()),
        // The answer cannot see the write and the task keeps running: 503 while
        // the call was never handed over, 504 once it was. A create that landed
        // answers its own 201: the section is there, only the reply's journey
        // ran out.
        Err(_) => timeout_answer(
            state.timed_out(),
            (StatusCode::CREATED, axum::Json(json!({ "id": answer }))).into_response(),
        ),
        Ok(Err(_)) => error(StatusCode::BAD_GATEWAY, "client-other", &[]),
    }
}

/// `DELETE /api/profiles/model/{id}`: 204 with no body once the manager removed
/// the section. The session is read first, so with no session the manager is not
/// even connected to, as on every route behind the sign-in page. The manager
/// judges the delete on the state under its own lock, and its refusal is the
/// answer ([`delete_failure`]): a model profile of `models.ini` is 409
/// `model-profile-enabled`, one a key of `assignments.ini` names is 409
/// `model-profile-assigned` naming the instances, and a name neither pool file
/// holds is 404 `no-model-profile-named`. A page that lists a model profile the
/// pool no longer holds is that 404; the rule is the manager's, so a stale row
/// cannot delete what it does not see. A limit that runs out is 503
/// `client-unavailable` while the call was never handed over the manager, and
/// 504 `client-timeout` once it was, as on the create.
pub(crate) async fn remove(
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
        client.delete_model_profile(&id)?;
        mine.landed();
        Ok(())
    });
    match tokio::time::timeout(write_timeout, ask).await {
        Ok(Ok(Ok(()))) => StatusCode::NO_CONTENT.into_response(),
        Ok(Ok(Err(failure))) => delete_failure(&sessions, &session, failure, state.sent()),
        Err(_) => timeout_answer(state.timed_out(), StatusCode::NO_CONTENT.into_response()),
        Ok(Err(_)) => error(StatusCode::BAD_GATEWAY, "client-other", &[]),
    }
}

/// What a create asks for: the new model profile's name and the source the
/// manager writes it from.
struct Requested {
    id: String,
    source: ModelProfileSource,
}

/// The body of `POST /api/profiles/model`, as JSON of the right content type
/// ([`json_body`]) shaped as a create: an object whose only fields are `id` and
/// exactly one of `from` and `values`; `id` and `from` text ([`text`]); `values`
/// the pick of a new model profile ([`pick`]). `None` is the 400
/// `model-profiles-malformed`, and it is the whole of the shape rule: no name
/// and no value is judged here, the manager checks those and refuses, being the
/// one that writes. So a create of a name that is taken, `*`, or `a b` reaches
/// the manager and answers 409 or 422, never a 400 of the bff's own.
fn requested(headers: &HeaderMap, body: &[u8]) -> Option<Requested> {
    let binding = json_body(headers, body)?;
    let top = binding.as_object()?;
    if !top
        .keys()
        .all(|key| key == "id" || key == "from" || key == "values")
    {
        return None;
    }
    let id = text(top.get("id")?)?;
    let source = match (top.get("from"), top.get("values")) {
        (Some(from), None) => ModelProfileSource::From(text(from)?),
        (None, Some(values)) => pick(values)?,
        _ => return None,
    };
    Some(Requested { id, source })
}

/// The pick that starts a new model profile, the `values` of a create: one
/// `model` file, or a Hugging Face `hf-repo` with its optional `hf-file`. Every
/// key of the object is one of these three in their exact spelling, and every
/// value is text ([`text`]): a value that is not refuses the whole pick, it does
/// not drop its key, so an `hf-file` with a line break in it is no create and
/// not a source of two keys. The three go in no other combination: `model`
/// beside `hf-repo`, `hf-file` alone, an empty pick, and a key of the save's
/// list (`ctx-size`, or `Model` in another letter case) name no source the
/// manager can write, so the body is no create.
fn pick(values: &Value) -> Option<ModelProfileSource> {
    let map = values.as_object()?;
    let (mut model, mut repo, mut file) = (None, None, None);
    for (key, value) in map {
        let text = text(value)?;
        match key.as_str() {
            "model" => model = Some(text),
            "hf-repo" => repo = Some(text),
            "hf-file" => file = Some(text),
            _ => return None,
        }
    }
    match (model, repo, file) {
        (Some(model), None, None) => Some(ModelProfileSource::Model(model)),
        (None, Some(repo), None) => Some(ModelProfileSource::Hf { repo, file: None }),
        (None, Some(repo), Some(file)) => Some(ModelProfileSource::Hf {
            repo,
            file: Some(file),
        }),
        _ => None,
    }
}

/// The text a create carries: a string, its spaces and tabs trimmed off the ends
/// and not empty once trimmed, holding no control character beyond a tab. A line
/// break, a NUL or a DEL would end the line the manager builds from it, the same
/// rule the save's values have ([`parse_body`]), and no reader of the file keeps
/// a space or a tab at the ends of a value, so they are trimmed here as the save
/// trims them. The name is text on the same rule: its own check, the manager's,
/// is the one that refuses a `*`, a space in the middle, a name too long.
fn text(value: &Value) -> Option<String> {
    let text = value.as_str()?;
    if text.chars().any(|c| c.is_control() && c != '\t') {
        return None;
    }
    let text = text.trim_matches([' ', '\t']);
    (!text.is_empty()).then(|| text.to_string())
}

/// The answer to a create the manager refused. Its name is taken, by both pool
/// files: 409 `model-profile-name-taken` naming it, with no field, and the
/// session lives on. A content check is 422 with its key, its values and the
/// field the form marks ([`create_field`]). The manager refused the signed-in
/// user: 403 and the session ends ([`not_administrator`]). `Unavailable` (the
/// pipe and the bus answer a broken stream or a missing reply with it) is 504
/// `client-timeout` when `write_asked` says the shared state reached
/// `WRITE_ASKED`, the create sent and its outcome unknown, and 503
/// `client-unavailable` when it stayed Idle: the connect never handed anything
/// over, so a retried create of the same name is the 409 that shows what
/// happened. Anything else is 502, the manager's detail left out.
fn create_failure(
    sessions: &Arc<Sessions>,
    session: &str,
    failure: ClientError,
    write_asked: bool,
) -> Response {
    match failure {
        ClientError::Invalid(refusal) if refusal.key == "model-profile-name-taken" => {
            let args = refusal
                .values
                .iter()
                .map(String::as_str)
                .collect::<Vec<_>>();
            keyed(StatusCode::CONFLICT, "model-profile-name-taken", &args)
        }
        ClientError::Invalid(refusal) => refused(
            refusal.key,
            &refusal.values,
            Some(&create_field(refusal.key, &refusal.values)),
        ),
        ClientError::NotAuthorized => not_administrator(sessions, session),
        ClientError::Unavailable if write_asked => {
            error(StatusCode::GATEWAY_TIMEOUT, "client-timeout", &[])
        }
        ClientError::Unavailable => {
            error(StatusCode::SERVICE_UNAVAILABLE, "client-unavailable", &[])
        }
        _ => error(StatusCode::BAD_GATEWAY, "client-other", &[]),
    }
}

/// The answer to a delete the manager refused. Every refusal of the delete names
/// the model profile it is about and has no field to mark: a name neither pool
/// file holds is 404, a model profile of `models.ini` and one an
/// `assignments.ini` key names are 409, the last naming the instances in its
/// values. Any other check the manager runs is 422 with its key and its values,
/// as a model info refusal is ([`model_info_refused`]). The refusal of the
/// signed-in user ends that session; the other answers keep it. `Unavailable` is
/// 504 `client-timeout` when `write_asked` says the delete was sent, its outcome
/// unknown, and 503 `client-unavailable` when it was not, as on the create.
fn delete_failure(
    sessions: &Arc<Sessions>,
    session: &str,
    failure: ClientError,
    write_asked: bool,
) -> Response {
    match failure {
        ClientError::Invalid(refusal) => {
            let args = refusal
                .values
                .iter()
                .map(String::as_str)
                .collect::<Vec<_>>();
            keyed(
                match refusal.key {
                    "no-model-profile-named" => StatusCode::NOT_FOUND,
                    "model-profile-enabled" | "model-profile-assigned" => StatusCode::CONFLICT,
                    _ => StatusCode::UNPROCESSABLE_ENTITY,
                },
                refusal.key,
                &args,
            )
        }
        ClientError::NotAuthorized => not_administrator(sessions, session),
        ClientError::Unavailable if write_asked => {
            error(StatusCode::GATEWAY_TIMEOUT, "client-timeout", &[])
        }
        ClientError::Unavailable => {
            error(StatusCode::SERVICE_UNAVAILABLE, "client-unavailable", &[])
        }
        _ => error(StatusCode::BAD_GATEWAY, "client-other", &[]),
    }
}

/// The field a create refusal marks: the control the form highlights. The
/// checks of a create are about the form's three fields, not about the keys of a
/// section, so this is not the save's [`field_of`]: the name's check marks `id`,
/// the copy source that is not there marks `from`, and the two rules about a
/// pick mark `values`. A check that names the key it is about,
/// `comment-markers`, marks that key, as its `args[0]`.
fn create_field(key: &str, args: &[String]) -> String {
    match key {
        "invalid-model-profile-id" => String::from("id"),
        "no-model-profile-named" => String::from("from"),
        "model-profile-no-model" | "model-profile-bad-source" => String::from("values"),
        _ => args.first().cloned().unwrap_or_default(),
    }
}

/// `GET /api/profiles/model/{id}`: the document of one model profile, the entry of the
/// contract (`fe/briefs/api-contract.md`, "the model profile page"). The page shows
/// the keys and the values as the file holds them and runs no check of its own,
/// so this only reads, over one connect made for the session's user:
/// `models.ini` first, then `models.ini.disabled`, as the list reads them.
///
/// A section is matched in any letter case, as the list's ids are, and
/// llama-server's global `[*]` is never an answer: nothing in either file of
/// that name is 404 `no-model-profile-named` ([`not_found`]), the id echoed only when
/// it is a name a model profile can carry. The failures are the list's, on the same
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
        let on = client.read_file(MODELS)?;
        let off = client.read_file(MODELS_DISABLED)?;
        Ok(document(&on, &off, &name, &key))
    });
    read_answer(
        &sessions,
        &session,
        tokio::time::timeout(timeout, ask).await,
        |document| match document {
            // Neither file has a section of that name: no model profile, nothing to read.
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
/// and no check, so a model profile the manager would refuse today is one the page can
/// still show), `projectUrl` the section's `; url = ` comment, read by the
/// configurator's reader in `pai-core` (`ini::project_url_of`), empty when the
/// section has none, and `revision` ([`section_revision`]).
fn document(
    on: &Option<ManagerFile>,
    off: &Option<ManagerFile>,
    id: &str,
    key: &[u8; 16],
) -> Option<Value> {
    for (file, name, enabled) in [(on, MODELS, true), (off, MODELS_DISABLED, false)] {
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

/// The answer to `GET /api/profiles/model/{id}/model-info`: the layer facts of
/// the model of one model profile (L2 of "Layers, MoE and tensor placement on
/// the web model profile page", `ARCHITECTURE.md`), relayed from the manager's
/// `ModelInfoAs`, asked as the signed-in user on one connect and one call, and
/// answered as [`model_info_document`]. A page read and not a poll: the session
/// gates it and is its principal, and the read moves the idle deadline
/// ([`Sessions::current`]), so a model profile page opened late keeps the
/// sign-in alive.
///
/// The failures are the list's and the section read's ([`read_answer`]) beside
/// one of the model profile reads: the manager's keyed refusal that no model
/// profile is named is answered by [`model_info_refused`] and keeps the session
/// (a typo is not a lost sign-in), 403 `session-not-administrator` ends the
/// session when the manager refuses the signed-in user, 503
/// `client-unavailable` is a manager that is not there or answers later than
/// `timeout`, and 502 `client-other` is anything else, the manager's detail
/// staying out of the body.
pub(crate) async fn model_info(
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
        let client = connect(Some(user.as_str()))?;
        Ok(match client.model_info(&id) {
            Ok(reply) => Ok(model_info_document(&reply)),
            Err(ClientError::Invalid(refusal)) => Err(model_info_refused(&refusal)),
            Err(failure) => return Err(failure),
        })
    });
    read_answer(
        &sessions,
        &session,
        tokio::time::timeout(timeout, ask).await,
        |answered| match answered {
            Ok(document) => axum::Json(document).into_response(),
            Err(answer) => answer,
        },
    )
}

/// The document of [`model_info`]: `{"model", "info", "reason"}`, the three the
/// manager answers, in camelCase: `model` the path the facts were read from,
/// resolved, `info` one `{"blockCount", "moe", "expertCount", "expertUsed",
/// "tensors", "tensorsTruncated"}` of the GGUF header's own counts and the
/// tensor list (`tensors` rows of `name` and `bytes`, in the manager's order),
/// and `reason` one `{"key", "args"}`, the why of a read that worked with no
/// facts to read. Each is `null` where the manager answered none, so the page
/// has one shape and the bff invents neither a fact nor a reason; the counts and
/// the bytes travel as numbers, `0` included. The list is relayed as the manager
/// cut it: the bff sorts, filters and caps nothing.
fn model_info_document(reply: &ModelInfoReply) -> Value {
    json!({
        "model": reply.model,
        "info": reply
            .info
            .as_ref()
            .map(|facts| {
                json!({
                    "blockCount": facts.block_count,
                    "moe": facts.moe,
                    "expertCount": facts.expert_count,
                    "expertUsed": facts.expert_used,
                    "tensors": facts
                        .tensors
                        .iter()
                        .map(|row| json!({"name": row.name, "bytes": row.bytes}))
                        .collect::<Vec<_>>(),
                    "tensorsTruncated": facts.tensors_truncated,
                })
            })
            .unwrap_or(Value::Null),
        "reason": reply
            .reason
            .as_ref()
            .map(|reason| json!({ "key": reason.key, "args": reason.args }))
            .unwrap_or(Value::Null),
    })
}

/// The answer to one of the manager's keyed refusals of a model info read: its
/// key and its values as the page's answer, 404 for `no-model-profile-named`
/// (the section read's own answer for a name no file holds, [`not_found`]) and
/// 422 for any other check the manager runs, the rule of the association routes
/// (`instance_profiles::refused`). The session lives on: a refusal of a NAME is
/// not a refusal of the signed-in user.
fn model_info_refused(refusal: &checks::Refusal) -> Response {
    let args = refusal
        .values
        .iter()
        .map(String::as_str)
        .collect::<Vec<_>>();
    keyed(
        if refusal.key == "no-model-profile-named" {
            StatusCode::NOT_FOUND
        } else {
            StatusCode::UNPROCESSABLE_ENTITY
        },
        refusal.key,
        &args,
    )
}

/// A keyed answer: `{"key", "args"}` at `status`.
fn keyed(status: StatusCode, key: &str, args: &[&str]) -> Response {
    (status, axum::Json(json!({ "key": key, "args": args }))).into_response()
}

/// A section's revision: the server read's keyed hash (`server::revision`),
/// over the file's name, a newline, and the section's own text as the file
/// holds it: from its header through its last non-blank line, WITHOUT that last
/// line's own ending (`\n` or `\r\n`). The endings BETWEEN the lines are the
/// file's.
///
/// The file's name is in it, so the same text in the other file, the model profile
/// switched off, is another model profile to the page. The section's text alone is in
/// it, keys, values and comments in the file's spelling and order: a neighbour
/// added to the file leaves it where it was, and a key added to the section
/// moves it at once. The blank lines and the last line's ending are cut because
/// they belong to no section: a file whose last section has no final break, the
/// same file with that break added, and that file with a section appended below,
/// all are the same model profile to the page.
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
/// file as the manager read it (the compare and swap expects it), and the text
/// of the other pool file, with the connect made for the signed-in user, reused
/// for the write and for the model file listing the save checks a `model`
/// against. `NotFound` when neither model profile file has a section of
/// that name.
enum Read {
    Found {
        client: Arc<Client>,
        file: String,
        text: String,
        other: String,
    },
    NotFound,
}

/// The file that holds the section `id`, read as the signed-in user over one
/// connect: `models.ini` first, so a section in both files is the enabled one,
/// as the list and the section read answer the same id. A file the manager says
/// is not there holds nothing, and llama-server's global `[*]` is not a model profile.
fn read_section(connect: &Connect, user: &str, id: &str) -> Result<Read, ClientError> {
    let client = connect(Some(user))?;
    let on = client.read_file(MODELS)?.map(|file| file.text);
    let found_on =
        !is_global(id) && ini::text::section_text(on.as_deref().unwrap_or(""), id).is_some();
    let (file, text, other) = if found_on {
        // The section is in the file llama-server reads: that is the one the
        // save edits, and the disabled file is never opened for it.
        (MODELS.to_string(), on.unwrap_or_default(), String::new())
    } else {
        let off = client.read_file(MODELS_DISABLED)?.map(|file| file.text);
        let Some(off) = off else {
            return Ok(Read::NotFound);
        };
        if is_global(id) || ini::text::section_text(&off, id).is_none() {
            return Ok(Read::NotFound);
        }
        (MODELS_DISABLED.to_string(), off, on.unwrap_or_default())
    };
    Ok(Read::Found {
        client: Arc::new(client),
        file,
        text,
        other,
    })
}

/// The three keys of the model source of the section `id` of `text`, as
/// `ini::parse_sections` reads them, in the order of the source rule: the model
/// file, the repository, the file inside it. `None` is a key the section does
/// not carry. A key the body sets is never blank (`parse_body` refuses an empty
/// value), so a key that is there names a source.
fn source_of(text: &str, id: &str) -> (Option<String>, Option<String>, Option<String>) {
    let section = ini::text::section_text(text, id).unwrap_or_default();
    let keys = ini::parse_sections(&section)
        .into_iter()
        .next()
        .map(|section| section.keys)
        .unwrap_or_default();
    (
        keys.get("model").cloned(),
        keys.get("hf-repo").cloned(),
        keys.get("hf-file").cloned(),
    )
}

/// The node's model files, as the manager walks its one models folder, asked in
/// a save over the connect its section was read with (slice M3b of "Choosing the
/// model file on the web"): the paths the picker offers, the only ones a changed
/// `model` may name. A manager that refuses the signed-in user, is gone, or
/// answers later than `timeout` is answered as every other read of the save is:
/// the answer is in `Err`, boxed, as the other routes of this crate box it.
async fn list_models(
    sessions: &Arc<Sessions>,
    session: &str,
    client: Arc<Client>,
    timeout: Duration,
) -> Result<Vec<String>, Box<Response>> {
    let ask = tokio::task::spawn_blocking(move || client.model_files());
    let reply = match tokio::time::timeout(timeout, ask).await {
        Err(_elapsed) => {
            return Err(Box::new(error(
                StatusCode::SERVICE_UNAVAILABLE,
                "client-unavailable",
                &[],
            )))
        }
        Ok(Err(_join)) => {
            return Err(Box::new(error(
                StatusCode::BAD_GATEWAY,
                "client-other",
                &[],
            )))
        }
        Ok(Ok(Err(failure))) => {
            return Err(Box::new(switch_failure(sessions, session, failure, false)))
        }
        Ok(Ok(Ok(reply))) => reply,
    };
    Ok(reply.files.into_iter().map(|row| row.path).collect())
}

/// `PUT /api/profiles/model/{id}`: the save of one model profile, as the signed-in user. The
/// body is the object [`parse_body`] reads, the answer is exactly `{"revision",
/// "running", "restart"}`: `revision` the revision of the section as it now
/// stands in the file, so the card can go on editing, and `restart` the
/// instances the page offers to restart, the ones the record of
/// `assignments.ini` gives this model profile to and that are up
/// ([`served_states`]), `running` being that list not being empty. The record
/// is read once the write stands, and a machine with no record at all has no
/// word about who serves the profile: a save that wrote nothing asks the states
/// of every instance, a save that wrote names none, and no state is asked. The
/// session is read first, so with no session the manager is not
/// even connected to (401), and only a signed-in request sees a body over
/// `origin::MAX_BODY` refused by the extractor with a 413 (as the boot write).
///
/// The session and the body answer first, so a body nothing can be made of (400
/// `model-profiles-malformed`, bytes of another content type included: [`json_body`]
/// is the gate, as the server save's and the switch's) and a value no check
/// allows (422, [`field_of`]) reach no manager. The file is read next: a name
/// no file holds, and llama-server's global `[*]`, are 404 `no-model-profile-named`
/// ([`not_found`]) with nothing written.
/// The section's revision ([`section_revision`]) is then compared with the one
/// the body carries: a card that started from an older text is 409
/// `model-profile-changed-elsewhere` with nothing written, and a body that asks for the
/// text the file already holds answers 200 and writes nothing.
///
/// The model source is checked on the section as the save leaves it (slice M3b
/// of "Choosing the model file on the web"): exactly one of `model` and
/// `hf-repo`, `hf-file` only beside a repo, else 422 `model-profile-bad-source`
/// marking `model`. A source is then checked against the node's lists, and only
/// where the body changed it: a `model` is a path the manager's walk of the
/// engine's models folder names ([`list_models`], asked in this save), the repo
/// id is one `pai_core::hf::parse_spec` holds (422 `hf-repo-invalid`, fetched
/// nothing), and an `hf-file` is a file of that repo's listing (slice M2's,
/// through its cache, [`hf::file_is_listed`]): a file the body left alone is
/// checked when the repo changed, that file having been listed in the repo the
/// section named. A value the body leaves alone beside a source it leaves alone
/// is never checked, so a source the configurator or `pai` wrote outside the
/// lists stays saveable, and the listing is not even asked for. Every one of these
/// answers before the write is asked, so the file keeps the source it had; a
/// listing that never reached Hugging Face answers M2's key at 503
/// ([`hf::Refused::save_response`]).
///
/// The write is the compare and swap, expecting the text this request read, so
/// the file is never changed under a card that read it earlier: 409
/// `model-profile-changed-elsewhere` when the file moved in between. The manager's
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
/// Once the write landed the model profile is the one the body asked for, so a limit
/// that runs out on the states that follow, and a state read that fails, are the
/// 200 with `running` true and no instance named: the safe answer for a state
/// the bff cannot read. A
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
    hf: Arc<Hf>,
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
    let Read::Found {
        client,
        file,
        text,
        other,
    } = found
    else {
        return not_found(&id);
    };
    if section_revision(&file, &text, &id, &key) != sent {
        return error(StatusCode::CONFLICT, "model-profile-changed-elsewhere", &[]);
    }
    let new_text = apply(&edits, &text, &id);
    // The model source, as the save leaves the section (slice M3b): exactly one
    // of the two, `hf-file` only beside a repo.
    let (src_model, src_repo, src_file) = source_of(&new_text, &id);
    let (was_model, was_repo, was_file) = source_of(&text, &id);
    if usize::from(src_model.is_some()) + usize::from(src_repo.is_some()) != 1
        || (src_file.is_some() && src_repo.is_none())
    {
        return refused(
            "model-profile-bad-source",
            std::slice::from_ref(&id),
            Some("model"),
        );
    }
    if src_model != was_model {
        if let Some(path) = src_model {
            let listed = match list_models(&sessions, &session, Arc::clone(&client), timeout).await
            {
                Ok(listed) => listed,
                Err(answer) => return *answer,
            };
            if !listed.contains(&path) {
                return refused("model-profile-model-not-listed", &[path], Some("model"));
            }
        }
    }
    // The repo id rule runs before any listing is fetched, so a hostile id
    // never reaches the network: it runs whenever the save has to look at the
    // source, the tag `parse_spec` cuts off being no refusal.
    let mut spec = None;
    if src_repo != was_repo || src_file != was_file {
        if let Some(sent) = &src_repo {
            match pai_core::hf::parse_spec(sent) {
                Ok(parsed) => spec = Some(parsed),
                Err(_) => {
                    return refused(
                        "hf-repo-invalid",
                        std::slice::from_ref(sent),
                        Some("hf-repo"),
                    );
                }
            }
        }
    }
    // The file of the source the save is writing, checked against that source's
    // listing: a file present in the section as the save leaves it, and the repo
    // or the file changed. A file kept as it was beside a NEW repo proves
    // nothing about the new repo: it was listed in the one the section named.
    if src_repo != was_repo || src_file != was_file {
        if let (Some(name), Some(spec)) = (src_file, spec.as_ref()) {
            match hf::file_is_listed(&hf, &spec.repo, &name).await {
                Ok(true) => {}
                Ok(false) => {
                    return refused(
                        "model-profile-file-not-listed",
                        &[spec.repo.clone(), name],
                        Some("hf-file"),
                    )
                }
                Err(refused) => return refused.save_response(),
            }
        }
    }
    let written = section_revision(&file, &new_text, &id, &key);
    let changed = new_text != text;
    // The pool as the save leaves it, for the record's rows: the file the
    // section lives in carries the new text, the other file what it has.
    let (on, off) = if file == MODELS {
        (new_text.clone(), other.clone())
    } else {
        (other.clone(), new_text.clone())
    };
    let state = Arc::new(WriteState::new());
    let mine = state.clone();
    let ask = tokio::task::spawn_blocking(move || -> Result<(bool, Vec<String>), ClientError> {
        if changed {
            if !mine.may_write() {
                return Ok((false, Vec::new()));
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
        // The record of which instance is given which model profile, read once
        // the write stands: the instances the save offers to restart are the
        // ones it gives the model profile to, effective in the pool as the save
        // leaves it. A machine with no record at all has no word about who
        // serves a profile, so the save names none, and no state is asked.
        let candidates = match client.read_file(ASSIGNMENTS)? {
            Some(record) => served(&record.text, &id, &on, &off),
            None => BTreeSet::new(),
        };
        Ok(served_states(&client, &candidates))
    });
    let left = write_timeout.saturating_sub(started.elapsed());
    match tokio::time::timeout(left, ask).await {
        Ok(Ok(Ok((running, restart)))) => saved(&written, running, restart),
        Ok(Ok(Err(failure))) => switch_failure(&sessions, &session, failure, state.sent()),
        Ok(Err(_join)) => error(StatusCode::BAD_GATEWAY, "client-other", &[]),
        // A write never asked leaves the file the one that was read (503); a
        // write asked has an unknown outcome (504). A write that landed, or a
        // body that asked for the text the file already holds, leaves the model
        // profile the one the body asked for: only the instances' state stayed
        // unknown, so `running` is true and no instance is named.
        Err(_elapsed) => timeout_answer(state.timed_out(), saved(&written, true, Vec::new())),
    }
}

/// The 200 of a save: exactly `{"revision", "running", "restart"}`, the
/// section's revision as the file now holds it, and the instances the page
/// offers to restart (`running` is that list not being empty).
fn saved(revision: &str, running: bool, restart: Vec<String>) -> Response {
    (
        StatusCode::OK,
        axum::Json(json!({
            "revision": revision,
            "running": running,
            "restart": restart,
        })),
    )
        .into_response()
}

/// Why [`parse_body`] refuses the body of `PUT /api/profiles/model/{id}`: the two
/// answers the save gives before it reads a file, as the server save's
/// [`crate::server::BodyAnswer`] answers them.
enum BodyError {
    /// The body is not a model profile save at all: 400 `model-profiles-malformed`.
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
/// own tests in `src/tests/model_profile_body.rs` read.
#[cfg(test)]
#[derive(Debug)]
pub(crate) enum ModelProfileBodyAnswer {
    /// The new text of the model profile file, and the revision the body carried: the
    /// save goes to the file that still holds it, and a body that changed the
    /// revision of the card is not written at all.
    Text { text: String, revision: String },
    /// The body is not a model profile save at all: 400 `model-profiles-malformed`.
    Malformed,
    /// A value was refused: 422 `{key, args, field}`, the check's key and
    /// values, and the INI key the form marks (the table of one rule per key).
    Refused {
        key: String,
        args: Vec<String>,
        field: String,
    },
}

/// The body of `PUT /api/profiles/model/{id}`, applied to `file_text`, the text of the
/// model profile file that holds the section named by `id`: the object
/// `{"revision": <the revision of the card the edit started from>, "values":
/// {"<key>": <value>, ...}}`, a `null` value removing that key. The checks of
/// [`parse_body`] answer first, then [`apply`] writes the edits: every check
/// answers before the first edit is applied, so a refused body writes nothing.
///
/// The save calls [`parse_body`] and [`apply`] with the file's text in between,
/// so a body nothing can be made of is answered before a model profile file is read.
/// This is the whole of it, the tests' entry to it: it is built only under
/// `cfg(test)`, because no route calls it, only the body's own tests in
/// `src/tests/model_profile_body.rs` do.
#[cfg(test)]
pub(crate) fn body_to_text(body: &[u8], file_text: &str, id: &str) -> ModelProfileBodyAnswer {
    let Ok(body) = serde_json::from_slice::<Value>(body) else {
        return ModelProfileBodyAnswer::Malformed;
    };
    match parse_body(&body) {
        Ok((revision, edits)) => ModelProfileBodyAnswer::Text {
            text: apply(&edits, file_text, id),
            revision,
        },
        Err(BodyError::Malformed) => ModelProfileBodyAnswer::Malformed,
        Err(BodyError::Refused { key, args, field }) => {
            ModelProfileBodyAnswer::Refused { key, args, field }
        }
    }
}

/// The body of `PUT /api/profiles/model/{id}`, read and checked, the file it edits
/// looked at nowhere: the revision it carries and its edits, sorted by key
/// ([`Edits`]) whatever the order the body's `values` map came in.
///
/// Every key of `values` must be in `checks::WEB_MODEL_PROFILE_KEYS` (the list lives in
/// `pai-core`, pinned against `pai-app` `model_profiles::render_section`; this crate
/// keeps no copy of it), matched in the exact spelling of the list: a key in
/// another letter case is as unknown as a name no reader knows, because
/// llama-server knows a key only in its own case. Every set value is a string
/// that is not empty after trimming spaces and tabs, holds no control character
/// other than a tab, and starts and ends with a space the reader keeps: the
/// parser (`pai_core::ini`) reads a value with `str::trim`, which knows every
/// Unicode space, so a value that starts or ends with one beyond a space or a
/// tab would be read back shorter than it went. Every set value goes through
/// the configurator's `checks::comment_markers` for its key, a device list
/// included: the manager's device rule runs on the value the reader cuts at the
/// first marker, so a marker never reaches it, and the web's value would reload
/// in the file as less than what went. 422
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
        .all(|key| checks::WEB_MODEL_PROFILE_KEYS.contains(&key.as_str()))
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
                // value holding one reloads as less than what was written. A
                // device list is no exception ("As built" (a) of the GPU
                // distribution design): the manager's `checks::device_list`
                // judges the value the reader cuts at the first marker, so it
                // never sees one, and the refusal that names an entry is the
                // one about the entry's shape.
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
