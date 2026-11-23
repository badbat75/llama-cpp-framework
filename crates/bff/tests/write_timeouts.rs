//! W1: one timeout answer for every bff write (ARCHITECTURE.md, slice W1),
//! through `pai_bff::router_with_session_and_write_timeout` with a fake manager
//! client and a fake authenticator (never the real bus, pipe, PAM or LogonUser).
//! Every request runs under a deadline, so a bff that waits on the manager
//! fails instead of hanging.
//!
//! The nine single-call write handlers, each one request:
//! - `model_profiles::create` `POST /api/profiles/model` and
//!   `model_profiles::remove` `DELETE /api/profiles/model/{id}`
//! - `server_profiles::create` `POST /api/profiles/server` and
//!   `server_profiles::remove` `DELETE /api/profiles/server/{id}`
//! - `instances::create` `POST /api/instances` and `instances::remove`
//!   `DELETE /api/instances/{id}`
//! - `control::act`, the three verbs `POST /api/instances/{id}/{start,stop,
//!   restart}` (one handler, all three pinned)
//! - `control::boot_write` `PUT /api/instances/{id}/boot` and
//!   `control::tags_write` `PUT /api/instances/{id}/tags`
//!
//! The rule is the server save's (`crates/bff/src/server.rs`, `WriteState`):
//! while the manager was never handed the call, the answer is 503
//! `client-unavailable`, the manager not reached; from the moment the call was
//! handed over its outcome is unknown, so the answer is 504 `client-timeout`,
//! never the 503 that tells the page nothing changed when the write may have
//! landed. Each route carries one pair of tests:
//! - a manager that RECEIVES the call and blocks past `write_timeout` answers
//!   504 `client-timeout` with `{"key": "client-timeout", "args": []}`, the
//!   session kept (the bff's own news, not a refusal of the user), and the
//!   manager's log proves the call really arrived: the 504 is not a call that
//!   never left.
//! - a connect that does not return inside the limit answers 503
//!   `client-unavailable` and asks the manager nothing. This half is green
//!   before the rule lands and must stay green: it is the guard that the new
//!   504 does not swallow the manager that was never reached.
//!
//! The reads keep 503; that is pinned in the route families' own files
//! (`tests/model_profiles.rs`, `tests/instances.rs`). Which of the two limits a
//! write runs under is pinned there too, for the model profile pool
//! (`tests/model_profile_create.rs`) and the associations
//! (`tests/instance_profiles.rs`); these tests pin only the answer, so they
//! hold under either limit.

use axum::body::Body;
use axum::extract::ConnectInfo;
use axum::http::{header, Method, Request, StatusCode};
use axum::Router;
use http_body_util::BodyExt;
use pai_bff::session::{AuthError, Authenticator, Identity};
use pai_bff::{Client, Connect};
use pai_core::client::{
    ClientError, InstanceInfo, InstanceState, ManagerClient, ManagerFile, ModelProfileSource,
};
use serde_json::{json, Value};
use std::net::SocketAddr;
use std::sync::atomic::{AtomicU64, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};
use tower::ServiceExt;

/// The limit these routers give a read.
const TIMEOUT: Duration = Duration::from_millis(200);

/// The limit they give a write: longer, as the service's pair is, so the two
/// answers of a blocked write are told apart from a cut-off read.
const WRITE_TIMEOUT: Duration = Duration::from_millis(600);

/// How long a call the test stops holds, past both limits. Bounded: a call
/// nobody releases goes on and the test fails, it does not hang.
const BLOCK: Duration = Duration::from_millis(1000);

/// How long a test waits for the bff's answer before failing.
const DEADLINE: Duration = Duration::from_secs(5);

/// The client address a sign-in is sent from (the lockout counters key on it).
const CLIENT: &str = "127.0.0.1:51007";

/// The revision secret these routers are built with; none of these routes
/// hands out a revision, the secret only keeps the builder whole.
const REVISION_KEY: [u8; 16] = [0; 16];

/// The signed-in user every request here carries.
const USER: &str = "alice";

type Log = Arc<Mutex<Vec<String>>>;

/// The cells the test moves after building a router: how long a manager call
/// takes once the fake has it, how long the connect takes, how many connects
/// there were, and every call the manager received.
#[derive(Clone, Default)]
struct Live {
    log: Log,
    call_pause_ms: Arc<AtomicU64>,
    connect_pause_ms: Arc<AtomicU64>,
    connects: Arc<AtomicUsize>,
}

impl Live {
    fn pause(&self) -> u64 {
        self.call_pause_ms.load(Ordering::SeqCst)
    }
}

/// One write route, as a request, with the line the manager's log carries when
/// the call arrives.
struct Route {
    method: Method,
    uri: &'static str,
    body: Option<&'static str>,
    call: &'static str,
}

const MODEL_CREATE: Route = Route {
    method: Method::POST,
    uri: "/api/profiles/model",
    body: Some(r#"{"id": "new", "from": "beta"}"#),
    call: "create-model new",
};

const MODEL_REMOVE: Route = Route {
    method: Method::DELETE,
    uri: "/api/profiles/model/beta",
    body: None,
    call: "delete-model beta",
};

const SERVER_CREATE: Route = Route {
    method: Method::POST,
    uri: "/api/profiles/server",
    body: Some(r#"{"engine": "llama.cpp", "id": "edge", "port": 9931}"#),
    call: "create-server edge",
};

const SERVER_REMOVE: Route = Route {
    method: Method::DELETE,
    uri: "/api/profiles/server/edge",
    body: None,
    call: "delete-server edge",
};

const INSTANCE_CREATE: Route = Route {
    method: Method::POST,
    uri: "/api/instances",
    body: Some(r#"{"engine": "llama.cpp", "id": "edge", "serverProfile": "default"}"#),
    call: "create-instance edge",
};

const INSTANCE_REMOVE: Route = Route {
    method: Method::DELETE,
    uri: "/api/instances/edge",
    body: None,
    call: "remove-instance edge",
};

const BOOT_WRITE: Route = Route {
    method: Method::PUT,
    uri: "/api/instances/default/boot",
    body: Some(r#"{"enabled": true}"#),
    call: "set_boot_enabled default true",
};

const TAGS_WRITE: Route = Route {
    method: Method::PUT,
    uri: "/api/instances/default/tags",
    body: Some(r#"{"tags": ["edge"]}"#),
    call: "set_instance_tags default",
};

/// The three control verbs: one handler (`control::act`), so its pair runs over
/// all three.
const VERBS: [Route; 3] = [
    Route {
        method: Method::POST,
        uri: "/api/instances/default/start",
        body: None,
        call: "start default",
    },
    Route {
        method: Method::POST,
        uri: "/api/instances/default/stop",
        body: None,
        call: "stop default",
    },
    Route {
        method: Method::POST,
        uri: "/api/instances/default/restart",
        body: None,
        call: "restart default",
    },
];

/// A manager that receives every write verb of the nine routes, logs it, and
/// then blocks for `Live::call_pause_ms`. The log line lands BEFORE the block:
/// a call the bff answers 504 for is provably one the manager got.
struct FakeManager {
    live: Live,
}

impl FakeManager {
    /// Received, then stopped: the log first, the sleep after.
    fn received(&self, call: &str) {
        self.live
            .log
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .push(call.to_string());
        let pause = self.live.pause();
        if pause > 0 {
            std::thread::sleep(Duration::from_millis(pause));
        }
    }
}

impl ManagerClient for FakeManager {
    fn list_instances(&self) -> Result<Vec<String>, ClientError> {
        Ok(vec!["default".to_string()])
    }
    fn status(&self, _id: &str) -> Result<InstanceState, ClientError> {
        Ok(InstanceState::Inactive)
    }
    fn start(&self, id: &str) -> Result<(), ClientError> {
        self.received(&format!("start {id}"));
        Ok(())
    }
    fn stop(&self, id: &str) -> Result<(), ClientError> {
        self.received(&format!("stop {id}"));
        Ok(())
    }
    fn restart(&self, id: &str) -> Result<(), ClientError> {
        self.received(&format!("restart {id}"));
        Ok(())
    }
    fn read_file(&self, _name: &str) -> Result<Option<ManagerFile>, ClientError> {
        Ok(None)
    }
    fn write_file(
        &self,
        _name: &str,
        _contents: &str,
        _keep_api_key: bool,
    ) -> Result<(), ClientError> {
        Ok(())
    }
    fn boot_enabled(&self, id: &str) -> Result<bool, ClientError> {
        self.received(&format!("boot_enabled {id}"));
        Ok(true)
    }
    fn set_boot_enabled(&self, id: &str, enabled: bool) -> Result<(), ClientError> {
        self.received(&format!("set_boot_enabled {id} {enabled}"));
        Ok(())
    }
    fn authenticate(&self, _user: &str, _password: &str) -> Result<(bool, bool), ClientError> {
        Ok((true, true))
    }
    fn instances(&self) -> Result<Vec<InstanceInfo>, ClientError> {
        Ok(Vec::new())
    }
    fn create_instance(
        &self,
        _engine: &str,
        id: &str,
        _server_profile: &str,
    ) -> Result<(), ClientError> {
        self.received(&format!("create-instance {id}"));
        Ok(())
    }
    fn remove_instance(&self, id: &str) -> Result<(), ClientError> {
        self.received(&format!("remove-instance {id}"));
        Ok(())
    }
    fn create_server_profile(
        &self,
        _engine: &str,
        profile: &str,
        _port: Option<u16>,
    ) -> Result<(), ClientError> {
        self.received(&format!("create-server {profile}"));
        Ok(())
    }
    fn delete_server_profile(&self, _engine: &str, profile: &str) -> Result<(), ClientError> {
        self.received(&format!("delete-server {profile}"));
        Ok(())
    }
    fn create_model_profile(
        &self,
        profile: &str,
        _source: &ModelProfileSource,
    ) -> Result<(), ClientError> {
        self.received(&format!("create-model {profile}"));
        Ok(())
    }
    fn delete_model_profile(&self, profile: &str) -> Result<(), ClientError> {
        self.received(&format!("delete-model {profile}"));
        Ok(())
    }
    fn set_instance_tags(&self, instance: &str, _tags: &[String]) -> Result<(), ClientError> {
        self.received(&format!("set_instance_tags {instance}"));
        Ok(())
    }
}

/// The authenticator these routers sign in through: one password, never PAM,
/// LogonUser or the manager.
struct FakeAuth;

impl Authenticator for FakeAuth {
    fn authenticate(&self, user: &str, password: &str) -> Result<Identity, AuthError> {
        if password == "correct horse" {
            Ok(Identity {
                user: user.to_string(),
            })
        } else {
            Err(AuthError::BadCredentials)
        }
    }
}

struct Reply {
    status: StatusCode,
    body: String,
}

impl Reply {
    fn json(&self) -> Value {
        serde_json::from_str(&self.body)
            .unwrap_or_else(|e| panic!("not JSON ({e}): {:?}", self.body))
    }

    /// An error answer: its status and `{"key": key, "args": args}`.
    fn assert_error(&self, status: StatusCode, key: &str, args: &[&str]) {
        assert_eq!(self.status, status, "{}", self.body);
        assert_eq!(
            self.json(),
            json!({ "key": key, "args": args }),
            "{}",
            self.body
        );
    }

    /// `GET /api/session` of a session that is still signed in.
    fn assert_signed_in_as(&self, user: &str) {
        assert_eq!(self.status, StatusCode::OK, "signed in: {}", self.body);
        assert_eq!(self.json(), json!({ "user": user }), "{}", self.body);
    }
}

/// A router over a manager and an authenticator that never touch the machine,
/// the write's limit `WRITE_TIMEOUT` and the read's `TIMEOUT`.
struct App {
    router: Router,
    live: Live,
}

fn app() -> App {
    let live = Live::default();
    let connect: Connect = {
        let live = live.clone();
        Arc::new(move |_principal: Option<&str>| {
            live.connects.fetch_add(1, Ordering::SeqCst);
            let pause = live.connect_pause_ms.load(Ordering::SeqCst);
            if pause > 0 {
                std::thread::sleep(Duration::from_millis(pause));
            }
            Ok(Box::new(FakeManager { live: live.clone() }) as Client)
        })
    };
    let router = pai_bff::router_with_session_and_write_timeout(
        connect,
        TIMEOUT,
        WRITE_TIMEOUT,
        Arc::new(FakeAuth),
        Arc::new(std::time::Instant::now),
        REVISION_KEY,
    );
    App { router, live }
}

impl App {
    fn calls(&self) -> Vec<String> {
        self.live
            .log
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .clone()
    }

    /// Sign in and return the cookie the browser sends back.
    async fn sign_in(&self) -> String {
        let mut request = Request::builder()
            .method(Method::POST)
            .uri("/api/session")
            .header(header::CONTENT_TYPE, "application/json")
            .body(Body::from(
                json!({ "username": USER, "password": "correct horse" }).to_string(),
            ))
            .expect("request");
        request
            .extensions_mut()
            .insert(ConnectInfo(CLIENT.parse::<SocketAddr>().expect("address")));
        let response = tokio::time::timeout(DEADLINE, self.router.clone().oneshot(request))
            .await
            .expect("sign-in answered in time")
            .expect("infallible");
        assert_eq!(response.status(), StatusCode::OK, "signed in");
        response
            .headers()
            .get(header::SET_COOKIE)
            .expect("a session cookie")
            .to_str()
            .expect("ascii cookie")
            .split(';')
            .next()
            .expect("name=value")
            .trim()
            .to_string()
    }

    /// `GET /api/session`: the state of the session the cookie names.
    async fn session_of(&self, cookie: &str) -> Reply {
        self.ask(Method::GET, "/api/session", Some(cookie), None)
            .await
    }

    /// One request of `route`, with the session's cookie.
    async fn ask_route(&self, route: &Route, cookie: &str) -> Reply {
        self.ask(route.method.clone(), route.uri, Some(cookie), route.body)
            .await
    }

    /// One request, with the session's cookie, answered under the deadline: a
    /// body of `Some` goes out as `application/json`.
    async fn ask(
        &self,
        method: Method,
        uri: &str,
        cookie: Option<&str>,
        body: Option<&str>,
    ) -> Reply {
        let mut builder = Request::builder().method(method.clone()).uri(uri);
        if body.is_some() {
            builder = builder.header(header::CONTENT_TYPE, "application/json");
        }
        if let Some(cookie) = cookie {
            builder = builder.header(header::COOKIE, cookie);
        }
        let request = builder
            .body(Body::from(body.unwrap_or("").to_string()))
            .expect("request");
        let response = tokio::time::timeout(DEADLINE, self.router.clone().oneshot(request))
            .await
            .unwrap_or_else(|_| panic!("no answer to {method} {uri} within {DEADLINE:?}"))
            .expect("infallible");
        let status = response.status();
        let bytes = response
            .into_body()
            .collect()
            .await
            .expect("body")
            .to_bytes();
        Reply {
            status,
            body: String::from_utf8(bytes.to_vec()).expect("utf-8 body"),
        }
    }

    /// The pair's 504 half: the manager receives the call and blocks past the
    /// write limit, so the answer is 504 `client-timeout`, the session lives,
    /// and the log proves the call arrived.
    async fn blocked_write_is_504(&self, route: &Route) {
        let cookie = self.sign_in().await;
        self.live
            .call_pause_ms
            .store(BLOCK.as_millis() as u64, Ordering::SeqCst);
        let started = Instant::now();
        let reply = self.ask_route(route, &cookie).await;
        reply.assert_error(StatusCode::GATEWAY_TIMEOUT, "client-timeout", &[]);
        assert!(
            started.elapsed() < DEADLINE,
            "answered after {:?}",
            started.elapsed()
        );
        assert_eq!(
            self.calls(),
            vec![route.call.to_string()],
            "{} {}: the call the manager received",
            route.method,
            route.uri
        );
        assert_eq!(
            self.live.connects.load(Ordering::SeqCst),
            1,
            "one connect, the write's own"
        );
        self.session_of(&cookie).await.assert_signed_in_as(USER);
    }

    /// The pair's 503 half: the connect does not return inside the limit, so
    /// the answer is 503 `client-unavailable` and the manager is never asked.
    async fn hung_connect_is_503(&self, route: &Route) {
        let cookie = self.sign_in().await;
        self.live
            .connect_pause_ms
            .store(BLOCK.as_millis() as u64, Ordering::SeqCst);
        let started = Instant::now();
        let reply = self.ask_route(route, &cookie).await;
        reply.assert_error(StatusCode::SERVICE_UNAVAILABLE, "client-unavailable", &[]);
        assert!(
            started.elapsed() < DEADLINE,
            "answered after {:?}",
            started.elapsed()
        );
        assert!(
            self.calls().is_empty(),
            "{} {}: the connect never returned, so nothing was asked: {:?}",
            route.method,
            route.uri,
            self.calls()
        );
    }
}

// ── The model-profile pool: `model_profiles::create` and `remove` ────────

#[tokio::test]
async fn a_model_profile_create_the_manager_never_comes_back_from_is_504() {
    app().blocked_write_is_504(&MODEL_CREATE).await;
}

#[tokio::test]
async fn a_model_profile_create_of_a_connect_that_never_returns_is_503() {
    app().hung_connect_is_503(&MODEL_CREATE).await;
}

#[tokio::test]
async fn a_model_profile_delete_the_manager_never_comes_back_from_is_504() {
    app().blocked_write_is_504(&MODEL_REMOVE).await;
}

#[tokio::test]
async fn a_model_profile_delete_of_a_connect_that_never_returns_is_503() {
    app().hung_connect_is_503(&MODEL_REMOVE).await;
}

// ── The server-profile pool: `server_profiles::create` and `remove` ──────

#[tokio::test]
async fn a_server_profile_create_the_manager_never_comes_back_from_is_504() {
    app().blocked_write_is_504(&SERVER_CREATE).await;
}

#[tokio::test]
async fn a_server_profile_create_of_a_connect_that_never_returns_is_503() {
    app().hung_connect_is_503(&SERVER_CREATE).await;
}

#[tokio::test]
async fn a_server_profile_delete_the_manager_never_comes_back_from_is_504() {
    app().blocked_write_is_504(&SERVER_REMOVE).await;
}

#[tokio::test]
async fn a_server_profile_delete_of_a_connect_that_never_returns_is_503() {
    app().hung_connect_is_503(&SERVER_REMOVE).await;
}

// ── The instances: `instances::create` and `remove` ──────────────────────

#[tokio::test]
async fn an_instance_create_the_manager_never_comes_back_from_is_504() {
    app().blocked_write_is_504(&INSTANCE_CREATE).await;
}

#[tokio::test]
async fn an_instance_create_of_a_connect_that_never_returns_is_503() {
    app().hung_connect_is_503(&INSTANCE_CREATE).await;
}

#[tokio::test]
async fn an_instance_remove_the_manager_never_comes_back_from_is_504() {
    app().blocked_write_is_504(&INSTANCE_REMOVE).await;
}

#[tokio::test]
async fn an_instance_remove_of_a_connect_that_never_returns_is_503() {
    app().hung_connect_is_503(&INSTANCE_REMOVE).await;
}

// ── The control verbs, the boot write and the tags write ─────────────────

#[tokio::test]
async fn a_control_verb_the_manager_never_comes_back_from_is_504() {
    for route in VERBS {
        app().blocked_write_is_504(&route).await;
    }
}

#[tokio::test]
async fn a_control_verb_of_a_connect_that_never_returns_is_503() {
    for route in VERBS {
        app().hung_connect_is_503(&route).await;
    }
}

#[tokio::test]
async fn a_boot_write_the_manager_never_comes_back_from_is_504() {
    app().blocked_write_is_504(&BOOT_WRITE).await;
}

#[tokio::test]
async fn a_boot_write_of_a_connect_that_never_returns_is_503() {
    app().hung_connect_is_503(&BOOT_WRITE).await;
}

#[tokio::test]
async fn a_tags_write_the_manager_never_comes_back_from_is_504() {
    app().blocked_write_is_504(&TAGS_WRITE).await;
}

#[tokio::test]
async fn a_tags_write_of_a_connect_that_never_returns_is_503() {
    app().hung_connect_is_503(&TAGS_WRITE).await;
}
