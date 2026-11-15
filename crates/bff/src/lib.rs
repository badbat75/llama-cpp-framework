//! pai-bff's library: the modules of `pai-bff` (`main.rs`), over the shared
//! core (`pai-core`).

pub mod bff;
pub mod listen;
pub mod serve;
mod server;
pub mod session;

use pai_core::client::ClientError;

/// A connection to the manager, as the bff asks it.
pub type Client = Box<dyn pai_core::client::ManagerClient + Send + Sync>;

/// How the bff connects to the manager, called for each request that asks it.
pub type Connect = std::sync::Arc<dyn Fn() -> Result<Client, ClientError> + Send + Sync>;

/// The engine every instance runs: llama.cpp, the only engine today.
const ENGINE_LLAMA_CPP: &str = "llama.cpp";

/// How long a request waits for the manager, the connect included.
const MANAGER_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(5);

/// The bff's HTTP routes: `GET /api/version`, `GET /api/instances` from the
/// system's manager, `/api/session` through the system's authenticator,
/// `GET /api/instances/{id}/server`, the web UI's page at `/` and its files under `/assets/`.
pub fn router() -> axum::Router {
    router_with_session(
        std::sync::Arc::new(system_connect),
        MANAGER_TIMEOUT,
        std::sync::Arc::new(session::SystemAuthenticator),
        std::sync::Arc::new(std::time::Instant::now),
    )
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
/// session: `GET /api/instances/{id}/server`.
pub fn router_with_session(
    connect: Connect,
    timeout: std::time::Duration,
    auth: std::sync::Arc<dyn session::Authenticator>,
    clock: session::Clock,
) -> axum::Router {
    use axum::extract::Path;
    let sessions = session::sessions(auth, clock);
    let read = {
        let (connect, sessions) = (connect.clone(), sessions.clone());
        move |Path(id): Path<String>, headers: axum::http::HeaderMap| {
            server::read(connect.clone(), timeout, sessions.clone(), id, headers)
        }
    };
    router_with(connect, timeout)
        .merge(session::routes(sessions))
        .route("/api/instances/{id}/server", axum::routing::get(read))
}

/// The manager of this system: the D-Bus service on Linux, the named pipe on
/// Windows, none on macOS; under the test redirect (`PRIVATE_AI_DATA_ROOT`)
/// none, so a test never dials the real one.
fn system_connect() -> Result<Client, ClientError> {
    if std::env::var_os("PRIVATE_AI_DATA_ROOT").is_some() {
        return Err(ClientError::Unavailable);
    }
    #[cfg(target_os = "linux")]
    {
        pai_core::client::dbus::DbusClient::connect_system().map(|c| Box::new(c) as Client)
    }
    #[cfg(windows)]
    {
        pai_core::client::pipe::connect().map(|c| Box::new(c) as Client)
    }
    #[cfg(not(any(target_os = "linux", windows)))]
    {
        Err(ClientError::Unavailable)
    }
}

/// `GET /api/instances`: `[{"id", "engine", "state"}]` in the manager's
/// order, or `{"key": <strings key>, "args": []}`, never the manager's detail.
async fn instances(connect: Connect, timeout: std::time::Duration) -> axum::response::Response {
    use axum::http::StatusCode;
    use axum::response::IntoResponse;
    let ask = tokio::task::spawn_blocking(move || -> Result<_, ClientError> {
        let client = connect()?;
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
