//! `GET /api/node`: this host's name, the API's root and the versions the
//! packages installed (slice 13, `ARCHITECTURE.md`). Behind a session, like
//! `GET /api/version`: it asks the manager nothing and leaves the session's
//! idle deadline where it was, so the About page and the navigation can poll
//! it. The version files are the packages' (`xtask package`), read at each
//! request through [`pai_core::versions::installed`], so an engine upgrade
//! shows without a restart of the service.

use crate::session::Sessions;
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
/// the version files of `versions_dir` (`None`: the install ships none).
pub(crate) fn routes(sessions: Arc<Sessions>, versions_dir: Option<PathBuf>) -> axum::Router {
    axum::Router::new().route(
        "/api/node",
        axum::routing::get(move |headers: HeaderMap| {
            let (sessions, versions_dir) = (sessions.clone(), versions_dir.clone());
            read(sessions, versions_dir, headers)
        }),
    )
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
