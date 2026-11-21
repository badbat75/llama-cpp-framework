//! pai-bff's library: the modules of `pai-bff` (`main.rs`), over the shared
//! core (`pai-core`).

pub mod bff;
mod control;
mod instance_profiles;
mod instances;
pub mod listen;
mod model_profiles;
pub mod node;
mod origin;
pub mod serve;
mod server;
mod server_profiles;
pub mod session;

use pai_core::client::ClientError;

/// The machine's names as the `Host` guard reads them on demand: the seam of a
/// bff whose guard keeps up with the machine (`origin::HostSource`).
pub use origin::HostSource;

/// A connection to the manager, as the bff asks it.
pub type Client = Box<dyn pai_core::client::ManagerClient + Send + Sync>;

/// How the bff connects to the manager, called for each request that asks it.
/// `principal` is the user the call acts for: a route behind a session hands
/// its session's user; the sign-in check and the reads gated by a session that
/// ask only what the manager answers of the bff's own account (the instances
/// list, an instance's boot flag) hand `None`.
pub type Connect =
    std::sync::Arc<dyn Fn(Option<&str>) -> Result<Client, ClientError> + Send + Sync>;

/// The engine every instance runs: llama.cpp, the only engine today.
const ENGINE_LLAMA_CPP: &str = "llama.cpp";

/// How long a read waits for the manager, the connect included.
pub const MANAGER_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(5);

/// How long the manager's calls of a WRITE are given: the server file's save
/// (a read, the write, the state that answers `running`), the model profile save (its
/// write and the state after it, its read keeping `timeout`) and the model profile
/// switch (every call of the move). Longer than a read's
/// [`MANAGER_TIMEOUT`], because a write goes through the manager's checks
/// before the file, and its address check may resolve a host name. An
/// estimate, until a real run measures a save and its figure replaces this.
pub const WRITE_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(15);

/// How long the device list read waits for the manager: the bound of the
/// manager's own engine probe, `GPU_PROBE_TIMEOUT` of `pai-manager`'s
/// `service.rs` (30 s, the design's), plus the transport's own 5 s. Longer than
/// a read's [`MANAGER_TIMEOUT`], because the first read of a node starts a
/// `llama-server` on it to read its cards, and the page must not be cut off
/// while that runs.
pub const DEVICES_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(30 + 5);

/// The bff's HTTP routes: `GET /api/version`, `GET /api/instances` from the
/// system's manager, `/api/session` through the system's authenticator,
/// `GET /api/instances/{id}/server` and its `PUT`, the control verbs
/// `POST /api/instances/{id}/{start,stop,restart}`, the boot flag at
/// `GET`/`PUT /api/instances/{id}/boot`, the instance's tags at
/// `PUT /api/instances/{id}/tags`, the instance writes `POST /api/instances`
/// and `DELETE /api/instances/{id}` (`instances::routes`), the web UI's page at
/// `/` and its
/// files under `/assets/`, the model-profile list at `GET /api/profiles/model`, and the
/// model profile switch at `PUT /api/profiles/model/{id}/enabled`. The revision secret is
/// drawn from the OS, and a bff that cannot draw one has no
/// revision to hand out: the service builds itself with [`try_router_at`], which
/// answers that as an error.
pub fn router() -> axum::Router {
    try_router().expect("the OS random source")
}

/// The routes of [`router`], at the address the service is bound to: the
/// revision secret drawn from the OS here, and the names the `Host` guard
/// answers beside `localhost` and the addresses: the `[Bff] Hosts` of
/// `bff.ini` when that sets them, the whole list, and otherwise this machine's
/// names, read from its host name and the search domains of its resolver
/// configuration (`listen::host_names`), which the guard reads again when a
/// `Host` asks a name it does not carry. The error is the OS's random source:
/// the bff must not start without a secret, a secret fixed in the code would
/// let anyone who reads the binary check a guess about a file they are not
/// allowed to read.
pub fn try_router_at(listen: std::net::SocketAddr) -> Result<axum::Router, std::io::Error> {
    let hosts = listen::load_hosts(&listen::config_path());
    let fixed = hosts
        .as_deref()
        .map(|hosts| listen::host_names(&listen::hostname(), &listen::resolv_conf(), Some(hosts)));
    let source: origin::HostSource = std::sync::Arc::new(|| {
        listen::host_names(&listen::hostname(), &listen::resolv_conf(), None)
    });
    system_router(origin::HostPolicy::with_source(listen, fixed, source))
}

/// The routes of [`router`], as [`try_router_at`] builds them with no name of
/// the machine and a loopback listen: `localhost` and the addresses are
/// answered, which is what a test of the routes asks for, the machine's own
/// names read by no test.
pub fn try_router() -> Result<axum::Router, std::io::Error> {
    system_router(origin::HostPolicy::loopback())
}

/// The routes over the system's manager and authenticator, the secret drawn
/// from the OS here, the guard handed `policy`.
fn system_router(policy: origin::HostPolicy) -> Result<axum::Router, std::io::Error> {
    let mut key = [0u8; 16];
    getrandom::fill(&mut key)?;
    Ok(router_with_policy(
        std::sync::Arc::new(system_connect),
        MANAGER_TIMEOUT,
        WRITE_TIMEOUT,
        std::sync::Arc::new(session::SystemAuthenticator::new(std::sync::Arc::new(
            system_connect,
        ))),
        std::sync::Arc::new(std::time::Instant::now),
        key,
        policy,
        pai_core::bff_cfg::versions_dir(),
    ))
}

/// The routes every builder starts from: the web UI's page at `/` and its
/// files under `/assets/`, open to anyone, and the two reads a session gates,
/// `GET /api/version` and `GET /api/instances`, which answer 401
/// `session-required` to a request with no live session, before the manager is
/// asked. `sessions` is the store of the builder that registers them
/// ([`router_with_policy`]): these two reads use it as the gate and never move
/// a session's idle deadline. The manager is reached through `connect`, each
/// answer that asks it given up after `timeout`.
pub(crate) fn router_with(
    connect: Connect,
    timeout: std::time::Duration,
    sessions: std::sync::Arc<session::Sessions>,
) -> axum::Router {
    use axum::extract::Path;
    use axum::routing::get;
    let version = {
        let sessions = sessions.clone();
        move |headers: axum::http::HeaderMap| version(sessions.clone(), headers)
    };
    let instances = {
        let (connect, sessions) = (connect.clone(), sessions.clone());
        move |headers: axum::http::HeaderMap| {
            instances(connect.clone(), timeout, sessions.clone(), headers)
        }
    };
    axum::Router::new()
        .route("/", get(|| async { serve("index.html") }))
        .route(
            "/assets/{*path}",
            get(|Path(path): Path<String>| async move { serve(&format!("assets/{path}")) }),
        )
        .route("/api/version", get(version))
        .route("/api/instances", get(instances))
}

/// The routes of [`router_with`] plus the session routes, signing in
/// through `auth` and reading the time from `clock`, and those that need a
/// session: `GET /api/instances/{id}/server` and its save
/// `PUT /api/instances/{id}/server`, the server profile pool
/// (`GET`/`POST /api/profiles/server`, the document, the save and the delete at
/// `/api/profiles/server/{id}`, `server_profiles::routes`), the control verbs
/// `POST /api/instances/{id}/{start,stop,restart}` and the boot write
/// `PUT /api/instances/{id}/boot`, the instance's tags
/// `PUT /api/instances/{id}/tags`, the instance create
/// `POST /api/instances` and the removal `DELETE /api/instances/{id}`
/// (`instances::routes`), the model-profile list
/// `GET /api/profiles/model`, the model profile save `PUT /api/profiles/model/{id}` and the model profile
/// switch `PUT /api/profiles/model/{id}/enabled`. The boot read
/// `GET /api/instances/{id}/boot` is gated by a session too, and reads the
/// flag as the bff's own account (`control::routes`). One sign-in's check is given
/// [`session::SIGN_IN_TIMEOUT`] to answer (the session routes' seam
/// `session::routes_with_sign_in_timeout` is what a test hands its own
/// bound to). Every route answers through the same-origin guard
/// (`origin::guard`), and every body a route reads is capped at
/// `origin::MAX_BODY` by the one layer that stands in front of the whole
/// router, so a route added later is capped before it reads anything. Every
/// reply, the guard's refusal, the cap's 413 and the router's own 404 and 405
/// included, carries the framing and the sniffing header ([`framing`]), the
/// layer outside the guard. `key`
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

/// `router_with_session` with the folder of the package version files that
/// `GET /api/node` reads handed in: [`router_with_session`] and every builder
/// of the service reads the folder of this install
/// ([`pai_core::bff_cfg::versions_dir`], `None` where the install ships none);
/// a test hands a folder of its own.
pub fn router_with_session_and_versions_dir(
    connect: Connect,
    timeout: std::time::Duration,
    auth: std::sync::Arc<dyn session::Authenticator>,
    clock: session::Clock,
    key: [u8; 16],
    versions_dir: Option<std::path::PathBuf>,
) -> axum::Router {
    router_with_policy(
        connect,
        timeout,
        timeout,
        auth,
        clock,
        key,
        origin::HostPolicy::loopback(),
        versions_dir,
    )
}

/// `router_with_session` with the manager's calls of a WRITE given their own,
/// longer limit: the server file's save, the model profile save and the model profile switch
/// run under `write_timeout`, as do the three association writes
/// (`instance_profiles.rs`); every read keeps `timeout`, and so does the boot
/// write and the control verbs (the control routes are handed `timeout`). A
/// manager busy writing is not cut off at the limit that is right for a read.
pub fn router_with_session_and_write_timeout(
    connect: Connect,
    timeout: std::time::Duration,
    write_timeout: std::time::Duration,
    auth: std::sync::Arc<dyn session::Authenticator>,
    clock: session::Clock,
    key: [u8; 16],
) -> axum::Router {
    router_with_policy(
        connect,
        timeout,
        write_timeout,
        auth,
        clock,
        key,
        origin::HostPolicy::loopback(),
        pai_core::bff_cfg::versions_dir(),
    )
}

/// The routes, the guard handed `policy`: this machine's names and the address
/// the bff is bound to. Every public builder is a call of this one with a
/// policy: the service computes its own ([`try_router_at`]),
/// [`router_with_session_and_hosts`] takes the two as arguments, the rest answer
/// `localhost` and the addresses only, at a loopback listen.
#[allow(clippy::too_many_arguments)]
fn router_with_policy(
    connect: Connect,
    timeout: std::time::Duration,
    write_timeout: std::time::Duration,
    auth: std::sync::Arc<dyn session::Authenticator>,
    clock: session::Clock,
    key: [u8; 16],
    policy: origin::HostPolicy,
    versions_dir: Option<std::path::PathBuf>,
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
    let model_profiles = {
        let (connect, sessions) = (connect.clone(), sessions.clone());
        move |headers: axum::http::HeaderMap| {
            model_profiles::list(connect.clone(), timeout, sessions.clone(), headers)
        }
    };
    let set_enabled = {
        let (connect, sessions) = (connect.clone(), sessions.clone());
        move |Path(id): Path<String>,
              headers: axum::http::HeaderMap,
              body: Result<axum::body::Bytes, axum::extract::rejection::BytesRejection>| {
            model_profiles::set_enabled(
                connect.clone(),
                write_timeout,
                sessions.clone(),
                id,
                headers,
                body,
            )
        }
    };
    let read_model_profile = {
        let (connect, sessions) = (connect.clone(), sessions.clone());
        move |Path(id): Path<String>, headers: axum::http::HeaderMap| {
            model_profiles::read(connect.clone(), timeout, sessions.clone(), id, headers, key)
        }
    };
    let save_model_profile = {
        let (connect, sessions) = (connect.clone(), sessions.clone());
        move |Path(id): Path<String>,
              headers: axum::http::HeaderMap,
              body: Result<axum::body::Bytes, axum::extract::rejection::BytesRejection>| {
            model_profiles::save(
                connect.clone(),
                timeout,
                write_timeout,
                sessions.clone(),
                id,
                headers,
                key,
                body,
            )
        }
    };
    router_with(connect.clone(), timeout, sessions.clone())
        .merge(session::routes(sessions.clone()))
        .route(
            "/api/instances/{id}/server",
            axum::routing::get(read).put(save),
        )
        .merge(server_profiles::routes(
            connect.clone(),
            timeout,
            write_timeout,
            sessions.clone(),
            key,
        ))
        .route("/api/profiles/model", axum::routing::get(model_profiles))
        .route(
            "/api/profiles/model/{id}",
            axum::routing::get(read_model_profile).put(save_model_profile),
        )
        .route(
            "/api/profiles/model/{id}/enabled",
            axum::routing::put(set_enabled),
        )
        .merge(control::routes(connect.clone(), timeout, sessions.clone()))
        .merge(instances::routes(
            connect.clone(),
            write_timeout,
            sessions.clone(),
        ))
        .merge(instance_profiles::routes(
            connect.clone(),
            timeout,
            write_timeout,
            sessions.clone(),
        ))
        .merge(node::routes(connect, sessions, versions_dir))
        .layer(axum::extract::DefaultBodyLimit::max(origin::MAX_BODY))
        .layer(axum::middleware::from_fn_with_state(policy, origin::guard))
        .layer(axum::middleware::from_fn(framing))
}

/// `router_with_session` with the names the `Host` guard answers beside
/// `localhost` and the IP literals: the whole list, as the `[Bff] Hosts` of the
/// config file gives it, so the guard has no source to re-read; and with
/// `listen`, the address the bff is bound to, which decides whether `localhost`
/// and the loopback addresses are answered.
pub fn router_with_session_and_hosts(
    connect: Connect,
    timeout: std::time::Duration,
    auth: std::sync::Arc<dyn session::Authenticator>,
    clock: session::Clock,
    key: [u8; 16],
    hosts: Vec<String>,
    listen: std::net::SocketAddr,
) -> axum::Router {
    router_with_policy(
        connect,
        timeout,
        timeout,
        auth,
        clock,
        key,
        origin::HostPolicy::new(hosts, listen),
        pai_core::bff_cfg::versions_dir(),
    )
}

/// `router_with_session_and_hosts` with a source for the machine's names, so a
/// test can change what the machine answers. `hosts` is the `[Bff] Hosts` of
/// `bff.ini` when it sets them, the whole list, and then `source` is never
/// asked; `None` is the machine's own names, which `source` answers and the
/// guard re-reads for a `Host` it does not carry.
#[allow(clippy::too_many_arguments)]
pub fn router_with_session_and_host_source(
    connect: Connect,
    timeout: std::time::Duration,
    auth: std::sync::Arc<dyn session::Authenticator>,
    clock: session::Clock,
    key: [u8; 16],
    hosts: Option<Vec<String>>,
    listen: std::net::SocketAddr,
    source: origin::HostSource,
) -> axum::Router {
    router_with_policy(
        connect,
        timeout,
        timeout,
        auth,
        clock,
        key,
        origin::HostPolicy::with_source(listen, hosts, source),
        pai_core::bff_cfg::versions_dir(),
    )
}

/// The two headers every reply carries: `X-Frame-Options: DENY` keeps a page
/// of any other site from showing this UI in a frame (clickjacking), and
/// `X-Content-Type-Options: nosniff` keeps a browser from reading a body as a
/// type it is not (MIME sniffing). The bff answers plain HTTP on the address
/// the administrator chose, so a reply says both even when no route built it.
const SECURITY_HEADERS: [(axum::http::HeaderName, axum::http::HeaderValue); 2] = [
    (
        axum::http::HeaderName::from_static("x-frame-options"),
        axum::http::HeaderValue::from_static("DENY"),
    ),
    (
        axum::http::HeaderName::from_static("x-content-type-options"),
        axum::http::HeaderValue::from_static("nosniff"),
    ),
];

/// [`SECURITY_HEADERS`] written on the way out of the whole router: the
/// outermost layer, so the guard's 403, the body cap's 413 and the 404 and 405
/// the router answers itself carry them too. `insert` replaces, so each header
/// stands exactly once whatever a route wrote.
async fn framing(
    request: axum::extract::Request,
    next: axum::middleware::Next,
) -> axum::response::Response {
    let mut response = next.run(request).await;
    for (name, value) in SECURITY_HEADERS {
        response.headers_mut().insert(name, value);
    }
    response
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

/// `GET /api/version`: `{"version": <framework version>}`, to a signed-in
/// reader. With no live session the answer is 401 `session-required` and no
/// `Set-Cookie`; the session gates the read, nothing else. The read asks the
/// manager nothing, and it leaves the session's idle deadline where it was, so
/// a page that only polls it signs itself out at the idle timeout.
async fn version(
    sessions: std::sync::Arc<session::Sessions>,
    headers: axum::http::HeaderMap,
) -> axum::response::Response {
    use axum::{http::StatusCode, response::IntoResponse};
    if sessions.peek(&headers).is_none() {
        return session::error(StatusCode::UNAUTHORIZED, "session-required", &[]);
    }
    axum::Json(serde_json::json!({ "version": env!("CARGO_PKG_VERSION") })).into_response()
}

/// `GET /api/instances`: `[{"id", "engine", "serverProfile", "state", "port",
/// "tags", "instanceTags", "boot", "removable"}]` in the manager's order, to a
/// signed-in reader, or
/// `{"key": <strings key>, "args": []}`, never the manager's detail. The
/// manager's row of the instance carries the server profile it runs, named as
/// the manager names it (`serverProfile`, `null` when the row names none: the
/// page shows which `server@<profile>.ini` the instance reads), the port and
/// the two tag lists:
/// `port` is the configured one, `tags` the union the bff builds of the two
/// lists (the server profile's first, none twice), `instanceTags` the
/// instance's own list, the one the editor saves back. `boot` is the
/// instance's own start-at-boot flag (`BootEnabled`); one the service manager
/// cannot answer is `null`. `removable` says the page may offer a remove of the
/// row: true of every id, false of the one protected id alone
/// (`instances::removable`, the id the removal refuses). With no live session
/// the answer is 401 `session-required`, the manager is not connected to, and
/// no `Set-Cookie` goes out. The session gates the read and is not its
/// principal: the list, the rows and the states are what the manager answers
/// of the bff's own account, so `connect` is handed `None`, and the idle
/// deadline stays where it was (the page polls this read). A manager with no
/// rows to answer (an install of an older protocol) leaves every row with
/// port 0 and no tags: the rows are extra, not a part of the read that can
/// fail it. So is a boot flag: one that cannot be read is `null` on that item
/// and the list is answered.
async fn instances(
    connect: Connect,
    timeout: std::time::Duration,
    sessions: std::sync::Arc<session::Sessions>,
    headers: axum::http::HeaderMap,
) -> axum::response::Response {
    use axum::http::StatusCode;
    use axum::response::IntoResponse;
    if sessions.peek(&headers).is_none() {
        return session::error(StatusCode::UNAUTHORIZED, "session-required", &[]);
    }
    let ask = tokio::task::spawn_blocking(move || -> Result<_, ClientError> {
        let client = connect(None)?;
        let ids = client.list_instances()?;
        let rows = client.instances().unwrap_or_default();
        let mut list = Vec::new();
        for id in ids {
            let row = rows.iter().find(|row| row.id == id);
            match client.status(&id) {
                Ok(state) => list.push(serde_json::json!({
                    "id": id,
                    "engine": ENGINE_LLAMA_CPP,
                    "serverProfile": row.and_then(|row| row.server_profile.clone()),
                    "state": state.name(),
                    "port": row.map_or(0, |row| row.port),
                    "tags": tags_of_row(row),
                    "instanceTags": row.map_or_else(Vec::new, |row| row.instance_tags.clone()),
                    "boot": client.boot_enabled(&id).ok(),
                    "removable": instances::removable(&id),
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

/// The tags of one row of `Instances`, as the list carries them: the server
/// profile's list first, then the instance's own, none twice. No row is no
/// tags.
fn tags_of_row(row: Option<&pai_core::client::InstanceInfo>) -> Vec<String> {
    let mut tags: Vec<String> = Vec::new();
    if let Some(row) = row {
        for tag in row.profile_tags.iter().chain(row.instance_tags.iter()) {
            if !tags.contains(tag) {
                tags.push(tag.clone());
            }
        }
    }
    tags
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
