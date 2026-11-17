//! pai-bff's library: the modules of `pai-bff` (`main.rs`), over the shared
//! core (`pai-core`).

pub mod bff;
mod control;
pub mod listen;
mod origin;
mod presets;
pub mod serve;
mod server;
pub mod session;

use pai_core::client::ClientError;

/// A connection to the manager, as the bff asks it.
pub type Client = Box<dyn pai_core::client::ManagerClient + Send + Sync>;

/// How the bff connects to the manager, called for each request that asks it.
/// `principal` is the user the call acts for: a route behind a session hands
/// its session's user, the session-free routes and the sign-in check hand
/// `None` and act as the bff's own account.
pub type Connect =
    std::sync::Arc<dyn Fn(Option<&str>) -> Result<Client, ClientError> + Send + Sync>;

/// The engine every instance runs: llama.cpp, the only engine today.
const ENGINE_LLAMA_CPP: &str = "llama.cpp";

/// How long a read waits for the manager, the connect included.
pub const MANAGER_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(5);

/// How long the manager's calls of a WRITE are given: the server file's save
/// (a read, the write, the state that answers `running`) and the preset switch
/// (two reads, two writes, the read back and the state). Longer than a read's
/// [`MANAGER_TIMEOUT`], because a write goes through the manager's checks
/// before the file, and its address check may resolve a host name. An
/// estimate, until a real run measures a save and its figure replaces this.
pub const WRITE_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(15);

/// The bff's HTTP routes: `GET /api/version`, `GET /api/instances` from the
/// system's manager, `/api/session` through the system's authenticator,
/// `GET /api/instances/{id}/server` and its `PUT`, the control verbs
/// `POST /api/instances/{id}/{start,stop,restart}`, the boot flag at
/// `GET`/`PUT /api/instances/{id}/boot`, the web UI's page at `/` and its
/// files under `/assets/`, the preset list at `GET /api/presets`, and the
/// preset switch at `PUT /api/presets/{id}/enabled`. The revision secret is
/// drawn from the OS, and a bff that cannot draw one has no
/// revision to hand out: the service builds itself with [`try_router`], which
/// answers that as an error.
pub fn router() -> axum::Router {
    try_router().expect("the OS random source")
}

/// The routes of [`router`], with the revision secret drawn from the OS
/// here. The error is the OS's random source: the bff must not start
/// without a secret, a secret fixed in the code would let anyone who reads
/// the binary check a guess about a file they are not allowed to read.
pub fn try_router() -> Result<axum::Router, std::io::Error> {
    let mut key = [0u8; 16];
    getrandom::fill(&mut key)?;
    Ok(router_with_session_and_write_timeout(
        std::sync::Arc::new(system_connect),
        MANAGER_TIMEOUT,
        WRITE_TIMEOUT,
        std::sync::Arc::new(session::SystemAuthenticator::new(std::sync::Arc::new(
            system_connect,
        ))),
        std::sync::Arc::new(std::time::Instant::now),
        key,
    ))
}

/// The routes of [`router`], the manager reached through `connect`, each
/// answer that asks it given up after `timeout`.
pub fn router_with(connect: Connect, timeout: std::time::Duration) -> axum::Router {
    use axum::extract::Path;
    use axum::routing::get;
    axum::Router::new()
        .route("/", get(|| async { serve("index.html") }))
        .route(
            "/assets/{*path}",
            get(|Path(path): Path<String>| async move { serve(&format!("assets/{path}")) }),
        )
        .route(
            "/api/version",
            get(|| async {
                axum::Json(serde_json::json!({ "version": env!("CARGO_PKG_VERSION") }))
            }),
        )
        .route(
            "/api/instances",
            get(move || instances(connect.clone(), timeout)),
        )
}

/// The routes of [`router_with`] plus the session routes, signing in
/// through `auth` and reading the time from `clock`, and those that need a
/// session: `GET /api/instances/{id}/server` and its save
/// `PUT /api/instances/{id}/server`, the control verbs
/// `POST /api/instances/{id}/{start,stop,restart}` and the boot write
/// `PUT /api/instances/{id}/boot`, the preset list
/// `GET /api/presets` and the preset switch
/// `PUT /api/presets/{id}/enabled`. The boot read
/// `GET /api/instances/{id}/boot` needs none, so it is served beside them.
/// One sign-in's check is given
/// [`session::SIGN_IN_TIMEOUT`] to answer (the session routes' seam
/// `session::routes_with_sign_in_timeout` is what a test hands its own
/// bound to). Every route answers through the same-origin guard
/// (`origin::guard`), and every body a route reads is capped at
/// `origin::MAX_BODY` by the one layer that stands in front of the whole
/// router, so a route added later is capped before it reads anything. `key`
/// is this router's revision secret: the server read's `revision` is
/// SipHash-2-4 of the file text keyed by it, so one router answers the same
/// revision for the same text and another router answers another one; the
/// service draws it at each start ([`try_router`]), which leaves every open
/// form stale after a restart. The manager's calls of a WRITE are given the
/// same `timeout` as the reads: the service hands its longer write limit
/// through [`router_with_session_and_write_timeout`].
pub fn router_with_session(
    connect: Connect,
    timeout: std::time::Duration,
    auth: std::sync::Arc<dyn session::Authenticator>,
    clock: session::Clock,
    key: [u8; 16],
) -> axum::Router {
    router_with_session_and_write_timeout(connect, timeout, timeout, auth, clock, key)
}

/// `router_with_session` with the manager's calls of a WRITE given their own,
/// longer limit: the server file's save and the preset switch run under
/// `write_timeout`, every read keeps `timeout`, and so does the boot write and
/// the control verbs (the control routes are handed `timeout`). A manager busy
/// writing is not cut off at the limit that is right for a read.
pub fn router_with_session_and_write_timeout(
    connect: Connect,
    timeout: std::time::Duration,
    write_timeout: std::time::Duration,
    auth: std::sync::Arc<dyn session::Authenticator>,
    clock: session::Clock,
    key: [u8; 16],
) -> axum::Router {
    use axum::extract::Path;
    let sessions = session::sessions(auth, clock);
    let read = {
        let (connect, sessions) = (connect.clone(), sessions.clone());
        move |Path(id): Path<String>, headers: axum::http::HeaderMap| {
            server::read(connect.clone(), timeout, sessions.clone(), id, headers, key)
        }
    };
    let save = {
        let (connect, sessions) = (connect.clone(), sessions.clone());
        move |Path(id): Path<String>,
              headers: axum::http::HeaderMap,
              body: Result<axum::body::Bytes, axum::extract::rejection::BytesRejection>| {
            server::save(
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
    let presets = {
        let (connect, sessions) = (connect.clone(), sessions.clone());
        move |headers: axum::http::HeaderMap| {
            presets::list(connect.clone(), timeout, sessions.clone(), headers)
        }
    };
    let set_enabled = {
        let (connect, sessions) = (connect.clone(), sessions.clone());
        move |Path(id): Path<String>,
              headers: axum::http::HeaderMap,
              body: Result<axum::body::Bytes, axum::extract::rejection::BytesRejection>| {
            presets::set_enabled(
                connect.clone(),
                write_timeout,
                sessions.clone(),
                id,
                headers,
                body,
            )
        }
    };
    router_with(connect.clone(), timeout)
        .merge(session::routes(sessions.clone()))
        .route(
            "/api/instances/{id}/server",
            axum::routing::get(read).put(save),
        )
        .route("/api/presets", axum::routing::get(presets))
        .route("/api/presets/{id}/enabled", axum::routing::put(set_enabled))
        .merge(control::routes(connect, timeout, sessions))
        .layer(axum::extract::DefaultBodyLimit::max(origin::MAX_BODY))
        .layer(axum::middleware::from_fn(origin::guard))
}

/// The manager of this system: the D-Bus service on Linux, the named pipe on
/// Windows, none on macOS; under the test redirect (`PRIVATE_AI_DATA_ROOT`)
/// none, so a test never dials the real one. The principal is the signed-in
/// user the call acts for: the client is bound to them (`acting_for`), so
/// their guarded calls name them; with `None` the client is the bff's own,
/// as the bff's account has no right of its own.
fn system_connect(principal: Option<&str>) -> Result<Client, ClientError> {
    if std::env::var_os("PRIVATE_AI_DATA_ROOT").is_some() {
        return Err(ClientError::Unavailable);
    }
    #[cfg(target_os = "linux")]
    {
        let client = pai_core::client::dbus::DbusClient::connect_system()?;
        Ok(match principal {
            Some(user) => Box::new(client.acting_for(user)) as Client,
            None => Box::new(client) as Client,
        })
    }
    #[cfg(windows)]
    {
        let client = pai_core::client::pipe::connect()?;
        Ok(match principal {
            Some(user) => Box::new(client.acting_for(user)) as Client,
            None => Box::new(client) as Client,
        })
    }
    #[cfg(not(any(target_os = "linux", windows)))]
    {
        let _ = principal;
        Err(ClientError::Unavailable)
    }
}

/// `GET /api/instances`: `[{"id", "engine", "state"}]` in the manager's
/// order, or `{"key": <strings key>, "args": []}`, never the manager's detail.
async fn instances(connect: Connect, timeout: std::time::Duration) -> axum::response::Response {
    use axum::http::StatusCode;
    use axum::response::IntoResponse;
    let ask = tokio::task::spawn_blocking(move || -> Result<_, ClientError> {
        let client = connect(None)?;
        let mut list = Vec::new();
        for id in client.list_instances()? {
            match client.status(&id) {
                Ok(state) => list.push(serde_json::json!({
                    "id": id,
                    "engine": ENGINE_LLAMA_CPP,
                    "state": state.name(),
                })),
                Err(ClientError::NoInstance) => {}
                Err(e) => return Err(e),
            }
        }
        Ok(list)
    });
    let (status, key) = match tokio::time::timeout(timeout, ask).await {
        Ok(Ok(Ok(list))) => return axum::Json(list).into_response(),
        Ok(Ok(Err(ClientError::Unavailable))) | Err(_) => {
            (StatusCode::SERVICE_UNAVAILABLE, "client-unavailable")
        }
        Ok(Ok(Err(ClientError::NotAuthorized))) => {
            (StatusCode::BAD_GATEWAY, "client-not-authorized")
        }
        Ok(Ok(Err(_))) | Ok(Err(_)) => (StatusCode::BAD_GATEWAY, "client-other"),
    };
    (
        status,
        axum::Json(serde_json::json!({ "key": key, "args": [] })),
    )
        .into_response()
}

/// The embedded web UI: (path under its directory, bytes, content type) for
/// each file, generated by `build.rs`.
mod fe {
    include!(concat!(env!("OUT_DIR"), "/fe_files.rs"));
}

/// The embedded file at `path`, or a 404. Only a path in the table matches,
/// so `..` reaches nothing else.
fn serve(path: &str) -> axum::response::Response {
    use axum::http::{header, StatusCode};
    use axum::response::IntoResponse;
    match fe::FILES.iter().find(|(p, _, _)| *p == path) {
        Some((_, bytes, content_type)) => {
            ([(header::CONTENT_TYPE, *content_type)], *bytes).into_response()
        }
        None => StatusCode::NOT_FOUND.into_response(),
    }
}

// Cross-cutting tests (the lock lint); the HTTP and config tests are in
// tests/, through the public API.
#[cfg(test)]
mod tests;
