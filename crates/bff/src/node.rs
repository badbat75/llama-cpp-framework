//! `GET /api/node`: this host's name, the API's root and the versions the
//! packages installed (slice 13, `ARCHITECTURE.md`). Behind a session, like
//! `GET /api/version`: it asks the manager nothing and leaves the session's
//! idle deadline where it was, so the About page and the navigation can poll
//! it. The version files are the packages' (`xtask package`), read at each
//! request through [`pai_core::versions::installed`], so an engine upgrade
//! shows without a restart of the service.
//!
//! `GET /api/node/devices`: the node's compute devices as the manager's engine
//! probe read them (G2, `ARCHITECTURE.md`). It is a page read and not a poll:
//! the idle deadline moves, and the list is asked of the manager as the
//! signed-in user, on the probe's bound ([`crate::DEVICES_TIMEOUT`]) rather
//! than a read's, because a cold probe starts the engine on the node.

use crate::session::Sessions;
use crate::Connect;
use axum::body::Body;
use axum::http::{HeaderMap, StatusCode};
use axum::response::Response;
use std::path::PathBuf;
use std::sync::Arc;

/// The node's name from the system's host name: the name the navigation
/// shows for this host. An empty host name (a machine that names itself
/// nothing) is `localhost`, the address the bff is reached at.
pub fn name(host: &str) -> String {
    if host.is_empty() {
        "localhost".to_string()
    } else {
        host.to_string()
    }
}

/// The document: `{"name", "api", "version", "engines"}`, `engines` each
/// `{"engine", "version", "backends"}` and each backend
/// `{"name", "version"}` (the package's suffix, the display name the web UI's
/// strings). A version of nothing is `null`; an engine with no runtime has
/// `backends: []`, and a host with no engine `engines: []`.
pub fn document(name: &str, engines: &[pai_core::versions::Engine]) -> serde_json::Value {
    let engines: Vec<serde_json::Value> = engines
        .iter()
        .map(|engine| {
            serde_json::json!({
                "engine": engine.engine,
                "version": engine.version,
                "backends": engine
                    .backends
                    .iter()
                    .map(|b| serde_json::json!({ "name": b.name, "version": b.version }))
                    .collect::<Vec<_>>(),
            })
        })
        .collect();
    serde_json::json!({
        "name": name,
        "api": "/api",
        "version": env!("CARGO_PKG_VERSION"),
        "engines": engines,
    })
}

/// The routes of this module: `GET /api/node`, gated by `sessions` and reading
/// the version files of `versions_dir` (`None`: the install ships none), and
/// `GET /api/node/devices`, gated by `sessions` and asking the device list of
/// the manager as the signed-in user.
pub(crate) fn routes(
    connect: Connect,
    sessions: Arc<Sessions>,
    versions_dir: Option<PathBuf>,
) -> axum::Router {
    let devices = {
        let (connect, sessions) = (connect.clone(), sessions.clone());
        move |headers: HeaderMap| {
            let (connect, sessions) = (connect.clone(), sessions.clone());
            devices(connect, sessions, headers)
        }
    };
    axum::Router::new()
        .route(
            "/api/node",
            axum::routing::get(move |headers: HeaderMap| {
                let (sessions, versions_dir) = (sessions.clone(), versions_dir.clone());
                read(sessions, versions_dir, headers)
            }),
        )
        .route("/api/node/devices", axum::routing::get(devices))
}

/// The answer to `GET /api/node`: 401 `session-required` with no live session,
/// the manager not asked, the session's idle deadline unmoved.
async fn read(
    sessions: Arc<Sessions>,
    versions_dir: Option<PathBuf>,
    headers: HeaderMap,
) -> Response<Body> {
    use axum::response::IntoResponse;
    if sessions.peek(&headers).is_none() {
        return crate::session::error(StatusCode::UNAUTHORIZED, "session-required", &[]);
    }
    let engines = versions_dir
        .as_deref()
        .map(pai_core::versions::installed)
        .unwrap_or_default();
    axum::Json(document(&name(&crate::listen::hostname()), &engines)).into_response()
}

/// The answer to `GET /api/node/devices`: 401 `session-required` with no live
/// session, the manager not asked at all; the device list, asked as the
/// signed-in user, on the probe's bound. The list is read through one connect
/// and one call, in a blocking task, so the async server waits on the probe
/// without stopping. Its failures are the model profile reads': 403
/// `session-not-administrator` and the session ended when the manager refused
/// the signed-in user, 503 `client-unavailable` when there is no manager or it
/// answers past the bound, 502 `client-other` for anything else.
async fn devices(connect: Connect, sessions: Arc<Sessions>, headers: HeaderMap) -> Response<Body> {
    use axum::response::IntoResponse;
    let Some((session, identity)) = sessions.current(&headers) else {
        return crate::session::error(StatusCode::UNAUTHORIZED, "session-required", &[]);
    };
    let user = identity.user;
    let ask = tokio::task::spawn_blocking(move || -> Result<_, pai_core::client::ClientError> {
        let client = connect(Some(user.as_str()))?;
        client.devices()
    });
    crate::model_profiles::read_answer(
        &sessions,
        &session,
        tokio::time::timeout(crate::DEVICES_TIMEOUT, ask).await,
        |list| axum::Json(devices_document(&list)).into_response(),
    )
}

/// The document: `{"probedAt", "devices"}`, `probedAt` the moment of the probe
/// as UTC whole seconds (`null` when no probe answered) and `devices` one
/// `{"id", "name", "backend", "totalMib", "freeMib"}` per device, in the
/// probe's order. Every row travels as the manager answered it: the bff adds,
/// drops, sorts and judges nothing, because the ids a model profile names that
/// the list lacks are the page's to show, not an answer to hide.
fn devices_document(list: &pai_core::client::NodeDevices) -> serde_json::Value {
    serde_json::json!({
        "probedAt": list
            .probed_at
            .map(utc_text)
            .unwrap_or(serde_json::Value::Null),
        "devices": list
            .devices
            .iter()
            .map(|device| {
                serde_json::json!({
                    "id": device.id,
                    "name": device.name,
                    "backend": device.backend,
                    "totalMib": device.total_mib,
                    "freeMib": device.free_mib,
                })
            })
            .collect::<Vec<_>>(),
    })
}

/// The moment of the probe as the clock writes it, `2026-11-20T09:14:03Z`: UTC
/// and whole seconds, the second the node was read, so the page can say how old
/// the memory of a card is. A count of seconds no civil calendar reaches is no
/// moment to show, and answers `null` like no probe.
fn utc_text(secs: u64) -> serde_json::Value {
    let Ok(secs) = i64::try_from(secs) else {
        return serde_json::Value::Null;
    };
    let (days, seconds) = (secs.div_euclid(86_400), secs.rem_euclid(86_400));
    let (year, month, day) = civil_from_days(days);
    serde_json::Value::String(format!(
        "{year:04}-{month:02}-{day:02}T{:02}:{:02}:{:02}Z",
        seconds / 3600,
        (seconds % 3600) / 60,
        seconds % 60
    ))
}

/// The year, month and day of a count of days from 1970-01-01, counting
/// backwards into the negative, on the proleptic Gregorian calendar (the
/// civil-calendar half of Howard Hinnant's `DaysToCivil` algorithm, the one
/// every UTC clock of this size writes).
fn civil_from_days(days: i64) -> (i64, u32, u32) {
    let shifted = days + 719_468;
    let era = if shifted >= 0 {
        shifted
    } else {
        shifted - 146_096
    } / 146_097;
    let day_of_era = (shifted - era * 146_097) as u64;
    let year_of_era =
        (day_of_era - day_of_era / 1460 + day_of_era / 36_524 - day_of_era / 146_096) / 365;
    let year = year_of_era as i64 + era * 400;
    let day_of_year = day_of_era - (365 * year_of_era + year_of_era / 4 - year_of_era / 100);
    let month_of_year = (5 * day_of_year + 2) / 153;
    let day = day_of_year - (153 * month_of_year + 2) / 5 + 1;
    let month = if month_of_year < 10 {
        month_of_year + 3
    } else {
        month_of_year - 9
    };
    (year + i64::from(month <= 2), month as u32, day as u32)
}
