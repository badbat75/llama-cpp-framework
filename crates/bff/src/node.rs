//! `GET /api/node`: this host's name, the API's root and the versions the
//! packages installed (slice 13, `ARCHITECTURE.md`), and the host's own facts
//! (slice AN4, "About and node info"): the operating system and kernel, what
//! the node's Vulkan read answered, and one row per GPU with both drivers, all
//! three from the manager's `NodeInfo` asked as the signed-in user. Behind a
//! session, like `GET /api/version`, and it leaves the session's idle deadline
//! where it was, so the node page and the navigation can poll it. The version
//! files are the packages' (`xtask package`), read at each request through
//! [`pai_core::versions::installed`], so an engine upgrade shows without a
//! restart of the service.
//!
//! `GET /api/node/devices`: the node's compute devices as the manager's engine
//! probe read them (G2, `ARCHITECTURE.md`). It is a page read and not a poll:
//! the idle deadline moves, and the list is asked of the manager as the
//! signed-in user, on the probe's bound ([`crate::DEVICES_TIMEOUT`]) rather
//! than a read's, because a cold probe starts the engine on the node.
//!
//! `GET /api/node/model-files`: the node's one models folder as the model-file
//! picker's list (M3 of "Choosing the model file on the web", `ARCHITECTURE.md`,
//! amended by "One models folder for every engine" (3)). A page read like the
//! device list, asked as the signed-in user, on a read's bound: the manager
//! walks the folder, so no engine starts.

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

/// The document: `{"name", "api", "version", "engines", "os", "vulkan",
/// "gpus"}`, `engines` each `{"engine", "version", "backends"}` and each
/// backend `{"name", "version"}` (the package's suffix, the display name the
/// web UI's strings). A version of nothing is `null`; an engine with no runtime
/// has `backends: []`, and a host with no engine `engines: []`.
///
/// The last three members are the host's facts as the manager answered them
/// (`host`: [`NodeInfoReply`], the row and text bounds are the manager's, in
/// `pai-core`'s `host` and `vulkan` readers: at most 32 rows, texts of 256 and
/// 128 bytes): `os` `{"name", "version", "kernel"}`, `vulkan` the one word the
/// read answered (`ok`, `none`, `failed`), `gpus` one
/// `{"pci", "vendor", "name", "driver", "version", "vulkan"}` per GPU in the
/// manager's order, each text a string or `null` and the row's `vulkan`
/// `{"driver", "info"}` or `null`. The bff sends every field as it arrived:
/// nothing is renamed, dropped, sorted, capped or judged, so the manager's
/// bounds are the only ones and a page reads one shape. With `host` `None` (the
/// manager answered nothing) all three are `null`, which is the page's "unknown"
/// and not `gpus: []`, a node with no GPU.
pub fn document(
    name: &str,
    engines: &[pai_core::versions::Engine],
    host: Option<&pai_core::client::NodeInfoReply>,
) -> serde_json::Value {
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
        "os": host
            .map(|host| {
                serde_json::json!({
                    "name": host.os.name,
                    "version": host.os.version,
                    "kernel": host.os.kernel,
                })
            })
            .unwrap_or(serde_json::Value::Null),
        "vulkan": host
            .map(|host| serde_json::Value::String(pai_core::vulkan::word(host.vulkan).to_string()))
            .unwrap_or(serde_json::Value::Null),
        "gpus": host
            .map(|host| {
                host.gpus
                    .iter()
                    .map(gpu_row)
                    .collect::<Vec<serde_json::Value>>()
                    .into()
            })
            .unwrap_or(serde_json::Value::Null),
    })
}

/// One GPU row of [`document`], every field as the manager answered it.
fn gpu_row(gpu: &pai_core::vulkan::NodeGpu) -> serde_json::Value {
    serde_json::json!({
        "pci": gpu.pci,
        "vendor": gpu.vendor,
        "name": gpu.name,
        "driver": gpu.driver,
        "version": gpu.version,
        "vulkan": gpu
            .vulkan
            .as_ref()
            .map(|driver| serde_json::json!({ "driver": driver.driver, "info": driver.info }))
            .unwrap_or(serde_json::Value::Null),
    })
}

/// The routes of this module: `GET /api/node`, gated by `sessions` and reading
/// the version files of `versions_dir` (`None`: the install ships none) beside
/// the host's facts the manager answers as the signed-in user, and
/// `GET /api/node/devices`, gated by `sessions` and asking the device list of
/// the manager as the signed-in user.
pub(crate) fn routes(
    connect: Connect,
    timeout: std::time::Duration,
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
    let model_files = {
        let (connect, sessions) = (connect.clone(), sessions.clone());
        move |headers: HeaderMap| {
            let (connect, sessions) = (connect.clone(), sessions.clone());
            model_files(connect, sessions, timeout, headers)
        }
    };
    axum::Router::new()
        .route(
            "/api/node",
            axum::routing::get(move |headers: HeaderMap| {
                let (connect, sessions, versions_dir) =
                    (connect.clone(), sessions.clone(), versions_dir.clone());
                read(connect, sessions, versions_dir, headers)
            }),
        )
        .route("/api/node/devices", axum::routing::get(devices))
        .route("/api/node/model-files", axum::routing::get(model_files))
}

/// The answer to `GET /api/node`: 401 `session-required` with no live session,
/// the manager not asked at all; the bff's own fields and the host's facts the
/// manager answered as the session's user, on the node's bound
/// ([`crate::NODE_INFO_TIMEOUT`]: the manager's Vulkan child has a bound of its
/// own). The session gates the read and is its principal, and the poll leaves
/// the idle deadline where it was, so a page that only polls this signs itself
/// out on time.
///
/// A manager that is not there, answers past the bound, or fails, is a partial
/// 200 and not a 503 (the design's preference: the navigation's host name and
/// the versions must not vanish with the manager): the bff's own fields stand,
/// the three facts are `null` and `nodeInfo` carries the key the page has a
/// text for, `client-unavailable` when the manager is away and `client-other`
/// for anything else; the manager's words stay out of the body. With no failure
/// the member is absent. The manager refusing the signed-in user is the refusal
/// of every read that acts as the user: 403 `session-not-administrator`, and
/// the session ends.
async fn read(
    connect: Connect,
    sessions: Arc<Sessions>,
    versions_dir: Option<PathBuf>,
    headers: HeaderMap,
) -> Response<Body> {
    use axum::response::IntoResponse;
    let Some((session, identity)) = sessions.peeked(&headers) else {
        return crate::session::error(StatusCode::UNAUTHORIZED, "session-required", &[]);
    };
    let engines = versions_dir
        .as_deref()
        .map(pai_core::versions::installed)
        .unwrap_or_default();
    let name = name(&crate::listen::hostname());
    let user = identity.user;
    let ask = tokio::task::spawn_blocking(move || -> Result<_, pai_core::client::ClientError> {
        let client = connect(Some(user.as_str()))?;
        client.node_info()
    });
    match tokio::time::timeout(crate::NODE_INFO_TIMEOUT, ask).await {
        Ok(Ok(Ok(host))) => axum::Json(document(&name, &engines, Some(&host))).into_response(),
        Ok(Ok(Err(pai_core::client::ClientError::NotAuthorized))) => {
            crate::model_profiles::not_administrator(&sessions, &session)
        }
        Ok(Ok(Err(pai_core::client::ClientError::Unavailable))) | Err(_) => {
            partial(&name, &engines, "client-unavailable")
        }
        Ok(Ok(Err(_))) | Ok(Err(_)) => partial(&name, &engines, "client-other"),
    }
}

/// The partial 200 of [`read`]: the bff's own fields, no host facts, and
/// `nodeInfo` naming the failure the page has a text for.
fn partial(name: &str, engines: &[pai_core::versions::Engine], key: &str) -> Response<Body> {
    use axum::response::IntoResponse;
    let mut doc = document(name, engines, None);
    doc["nodeInfo"] = serde_json::json!({ "key": key, "args": [] });
    axum::Json(doc).into_response()
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

/// The answer to `GET /api/node/model-files`: 401 `session-required` with no
/// live session, the manager not asked at all; the node's one models folder,
/// asked as the signed-in user, on a read's bound: the manager walks a folder,
/// so no engine starts and the probe's bound is not needed. The list is read
/// through one connect and one call, in a blocking task, so the async server
/// waits on the walk without stopping. Its failures are the device list's: 403
/// `session-not-administrator` and the session ended when the manager refused
/// the signed-in user, 503 `client-unavailable` when there is no manager or it
/// answers past the bound, 502 `client-other` for anything else. It is a page
/// read and not a poll: the idle deadline moves, the picker asking when it
/// opens.
async fn model_files(
    connect: Connect,
    sessions: Arc<Sessions>,
    timeout: std::time::Duration,
    headers: HeaderMap,
) -> Response<Body> {
    use axum::response::IntoResponse;
    let Some((session, identity)) = sessions.current(&headers) else {
        return crate::session::error(StatusCode::UNAUTHORIZED, "session-required", &[]);
    };
    let user = identity.user;
    let ask = tokio::task::spawn_blocking(move || -> Result<_, pai_core::client::ClientError> {
        let client = connect(Some(user.as_str()))?;
        client.model_files()
    });
    crate::model_profiles::read_answer(
        &sessions,
        &session,
        tokio::time::timeout(timeout, ask).await,
        |reply| axum::Json(model_files_document(&reply)).into_response(),
    )
}

/// The document: `{"source": "local", "dir", "files", "truncated", "error"}`,
/// `files` one `{"path", "size", "shards", "category"}` per model file, in the
/// walk's order, `category` the first subfolder of the file (`models`,
/// `mmprojs`, `mtps`, `dflashs`) or `null` for a file of the folder itself.
/// `source` says which of the picker's two lists this is (the Hugging Face one
/// answers `"hf"`). Every row travels as the manager answered it: the bff adds,
/// drops, sorts, caps and judges nothing, so the manager's bounds are the only
/// ones. `error` is the folder's own keyed why, inside a 200: a folder that is
/// missing, is not a folder, or cannot be read is the picker's empty list with
/// a text, never this route's failure.
fn model_files_document(reply: &pai_core::client::ModelFilesReply) -> serde_json::Value {
    serde_json::json!({
        "source": "local",
        "dir": reply.dir,
        "files": reply
            .files
            .iter()
            .map(|file| {
                serde_json::json!({
                    "path": file.path,
                    "size": file.size,
                    "shards": file.shards,
                    "category": file.category,
                })
            })
            .collect::<Vec<_>>(),
        "truncated": reply.truncated,
        "error": reply
            .error
            .as_ref()
            .map(|reason| serde_json::json!({ "key": reason.key, "args": reason.args })),
    })
}

/// The document: `{"probedAt", "ramMib", "cpuThreads", "devices"}`, `probedAt`
/// the moment of the probe as UTC whole seconds (`null` when no probe answered)
/// and `devices` one `{"id", "name", "backend", "totalMib", "freeMib"}` per
/// device, in the probe's order. Every row travels as the manager answered it:
/// the bff adds, drops, sorts and judges nothing, because the ids a model
/// profile names that the list lacks are the page's to show, not an answer to
/// hide. Beside them the host's own inventory (R1), `ramMib` whole MiB of RAM
/// and `cpuThreads` logical CPUs, answered whatever the probe did: the bff
/// scales, rounds and interprets nothing, and 0 travels as the node's "no
/// figure".
fn devices_document(list: &pai_core::client::NodeDevices) -> serde_json::Value {
    serde_json::json!({
        "probedAt": list
            .probed_at
            .map(utc_text)
            .unwrap_or(serde_json::Value::Null),
        "ramMib": list.ram_mib,
        "cpuThreads": list.cpu_threads,
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
