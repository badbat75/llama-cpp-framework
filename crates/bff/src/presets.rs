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

use crate::session::{error, Sessions};
use crate::Connect;
use axum::http::{HeaderMap, StatusCode};
use axum::response::{IntoResponse, Response};
use pai_core::client::{ClientError, ManagerFile};
use pai_core::ini;
use serde_json::{json, Value};
use std::sync::Arc;
use std::time::Duration;

/// The engine's file of switched-on presets, the one llama-server reads.
const PRESETS: &str = "presets.ini";

/// The engine's file of switched-off presets, never read by llama-server.
const PRESETS_DISABLED: &str = "presets.ini.disabled";

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
