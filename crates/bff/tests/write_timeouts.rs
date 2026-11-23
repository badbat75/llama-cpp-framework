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
//!
//! W2 (the slice after W1) adds the two rules pinned in the last two sections:
//! - a manager that TAKES the call and then answers `Unavailable` (a broken
//!   stream, a missing reply) leaves the outcome unknown exactly as a call that
//!   outlasts the limit does, so it is 504 `client-timeout` with the session
//!   kept. A connect that fails handed nothing over, so it stays 503: that half
//!   is green before W2 and must stay green.
//! - the run verbs, the boot write and the tags write run under the write's
//!   limit like every other write, so a call longer than the read `TIMEOUT` and
//!   shorter than `WRITE_TIMEOUT` answers its success (204 for a verb and for
//!   the tags write, 200 with the flag for the boot write). The boot READ is a
//!   read: past the read limit it stays 503.

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
use std::sync::atomic::{AtomicBool, AtomicU64, AtomicUsize, Ordering};
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

/// How long a call the test lets land: past the read `TIMEOUT`, well inside
/// `WRITE_TIMEOUT`. A write answered under this pause is answered under the
/// write's limit and not under the read's.
const SLOW_CALL_MS: u64 = 250;

/// The boot write is two calls of one connect, so each of them is shorter: two
/// of these (440 ms) are past the read limit and inside the write's.
const SLOW_PAIR_CALL_MS: u64 = 220;

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
    /// Set after the build: every call the fake takes answers `Unavailable`.
    call_lost: Arc<AtomicBool>,
    /// Set after the build: the connect itself fails, no client comes.
    connect_fails: Arc<AtomicBool>,
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

/// The boot READ, the one read of this file's routes: it keeps the read limit,
/// so a manager slower than `TIMEOUT` is 503 for it.
const BOOT_READ: Route = Route {
    method: Method::GET,
    uri: "/api/instances/default/boot",
    body: None,
    call: "boot_enabled default",
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

    /// Received, then lost: the log first, and after the call is handed over
    /// the answer the transport gives when the stream breaks or the reply is
    /// missing, `Unavailable`. The write is already out, so its outcome is
    /// unknown.
    fn lost(&self, call: &str) -> Result<(), ClientError> {
        self.received(call);
        if self.live.call_lost.load(Ordering::SeqCst) {
            Err(ClientError::Unavailable)
        } else {
            Ok(())
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
        self.lost(&format!("start {id}"))
    }
    fn stop(&self, id: &str) -> Result<(), ClientError> {
        self.lost(&format!("stop {id}"))
    }
    fn restart(&self, id: &str) -> Result<(), ClientError> {
        self.lost(&format!("restart {id}"))
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
        self.lost(&format!("boot_enabled {id}"))?;
        Ok(true)
    }
    fn set_boot_enabled(&self, id: &str, enabled: bool) -> Result<(), ClientError> {
        self.lost(&format!("set_boot_enabled {id} {enabled}"))
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
        self.lost(&format!("create-instance {id}"))
    }
    fn remove_instance(&self, id: &str) -> Result<(), ClientError> {
        self.lost(&format!("remove-instance {id}"))
    }
    fn create_server_profile(
        &self,
        _engine: &str,
        profile: &str,
        _port: Option<u16>,
    ) -> Result<(), ClientError> {
        self.lost(&format!("create-server {profile}"))
    }
    fn delete_server_profile(&self, _engine: &str, profile: &str) -> Result<(), ClientError> {
        self.lost(&format!("delete-server {profile}"))
    }
    fn create_model_profile(
        &self,
        profile: &str,
        _source: &ModelProfileSource,
    ) -> Result<(), ClientError> {
        self.lost(&format!("create-model {profile}"))
    }
    fn delete_model_profile(&self, profile: &str) -> Result<(), ClientError> {
        self.lost(&format!("delete-model {profile}"))
    }
    fn set_instance_tags(&self, instance: &str, _tags: &[String]) -> Result<(), ClientError> {
        self.lost(&format!("set_instance_tags {instance}"))
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

    /// A success answer with no body: 204 (a run verb, the tags write).
    fn assert_no_content(&self) {
        assert_eq!(self.status, StatusCode::NO_CONTENT, "{}", self.body);
        assert_eq!(self.body, "", "204 carries no body");
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
            if live.connect_fails.load(Ordering::SeqCst) {
                return Err(ClientError::Unavailable);
            }
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

    /// W2 (b), the 504 half: the manager TAKES the call and its stream breaks,
    /// so the answer is `Unavailable` with the write already handed over. Its
    /// outcome is unknown, so the answer is 504 `client-timeout`, the session
    /// lives, and the log proves the call really crossed.
    async fn lost_after_hand_over_is_504(&self, route: &Route) {
        let cookie = self.sign_in().await;
        self.live.call_lost.store(true, Ordering::SeqCst);
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
            "{} {}: the call the manager took and lost",
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

    /// W2 (b), the 503 half: the connect fails, so nothing was ever handed
    /// over and the answer stays 503 `client-unavailable`, the session living
    /// on. Green before W2 and it must stay green: the 504 is only for a call
    /// that crossed.
    async fn failed_connect_is_503(&self, route: &Route) {
        let cookie = self.sign_in().await;
        self.live.connect_fails.store(true, Ordering::SeqCst);
        let reply = self.ask_route(route, &cookie).await;
        reply.assert_error(StatusCode::SERVICE_UNAVAILABLE, "client-unavailable", &[]);
        assert!(
            self.calls().is_empty(),
            "{} {}: the connect failed, so nothing was asked: {:?}",
            route.method,
            route.uri,
            self.calls()
        );
        assert_eq!(
            self.live.connects.load(Ordering::SeqCst),
            1,
            "one connect attempted, and failed"
        );
        self.session_of(&cookie).await.assert_signed_in_as(USER);
    }

    /// W2 (a), the success of a write that answers 204 (a run verb, the tags
    /// write): the manager takes `SLOW_CALL_MS`, longer than the read `TIMEOUT`
    /// and less than `WRITE_TIMEOUT`, so under the write's limit the write
    /// lands. Today the same call is cut off by the read limit.
    async fn write_past_the_read_limit_is_204(&self, route: &Route) {
        let cookie = self.sign_in().await;
        self.live
            .call_pause_ms
            .store(SLOW_CALL_MS, Ordering::SeqCst);
        let started = Instant::now();
        let reply = self.ask_route(route, &cookie).await;
        reply.assert_no_content();
        let elapsed = started.elapsed();
        assert!(
            elapsed >= Duration::from_millis(SLOW_CALL_MS) && elapsed < WRITE_TIMEOUT,
            "{} {}: the manager held the call {SLOW_CALL_MS} ms, past the {TIMEOUT:?} read \
             limit and inside the {WRITE_TIMEOUT:?} write's: answered after {elapsed:?}",
            route.method,
            route.uri
        );
        assert_eq!(
            self.calls(),
            vec![route.call.to_string()],
            "{} {}: the call the manager took",
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

    /// W2 (a), the boot write's success: its two calls of `SLOW_PAIR_CALL_MS`
    /// (440 ms together) are past the read limit and inside the write's, so the
    /// answer is 200 with the flag the read-back gave.
    async fn boot_write_past_the_read_limit_is_200(&self) {
        let cookie = self.sign_in().await;
        self.live
            .call_pause_ms
            .store(SLOW_PAIR_CALL_MS, Ordering::SeqCst);
        let started = Instant::now();
        let reply = self.ask_route(&BOOT_WRITE, &cookie).await;
        assert_eq!(reply.status, StatusCode::OK, "{}", reply.body);
        assert_eq!(
            reply.json(),
            json!({ "enabled": true }),
            "the flag the read-back answered"
        );
        let elapsed = started.elapsed();
        assert!(
            elapsed >= Duration::from_millis(2 * SLOW_PAIR_CALL_MS) && elapsed < WRITE_TIMEOUT,
            "the boot write and its read-back took {SLOW_PAIR_CALL_MS} ms each, past the \
             {TIMEOUT:?} read limit and inside the {WRITE_TIMEOUT:?} write's: answered after \
             {elapsed:?}"
        );
        assert_eq!(
            self.calls(),
            vec![
                "set_boot_enabled default true".to_string(),
                "boot_enabled default".to_string()
            ],
            "the write and the read-back, two calls of one connect"
        );
        assert_eq!(
            self.live.connects.load(Ordering::SeqCst),
            1,
            "one connect, the write's own"
        );
        self.session_of(&cookie).await.assert_signed_in_as(USER);
    }

    /// The boot READ's limit, which W2 (a) leaves at the read's: a manager
    /// slower than `TIMEOUT` is 503 `client-unavailable` for a read, the
    /// session left alone. Green today, and the guard that the write's limit
    /// does not reach the reads.
    async fn boot_read_past_the_read_limit_is_503(&self) {
        let cookie = self.sign_in().await;
        self.live
            .call_pause_ms
            .store(SLOW_CALL_MS, Ordering::SeqCst);
        let started = Instant::now();
        let reply = self.ask_route(&BOOT_READ, &cookie).await;
        reply.assert_error(StatusCode::SERVICE_UNAVAILABLE, "client-unavailable", &[]);
        let elapsed = started.elapsed();
        assert!(
            elapsed < WRITE_TIMEOUT,
            "a read is cut off by the read limit, not by the write's: answered after {elapsed:?}"
        );
        assert_eq!(
            self.calls(),
            vec!["boot_enabled default".to_string()],
            "the read was asked, and the read limit ran out on it"
        );
        assert_eq!(
            self.live.connects.load(Ordering::SeqCst),
            1,
            "one connect, the read's own"
        );
        self.session_of(&cookie).await.assert_signed_in_as(USER);
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

// ── W2 (b): a manager lost after the hand-over is 504, a failed connect 503 ─

#[tokio::test]
async fn a_model_profile_create_the_manager_is_lost_after_taking_is_504() {
    app().lost_after_hand_over_is_504(&MODEL_CREATE).await;
}

#[tokio::test]
async fn a_model_profile_create_of_a_connect_that_fails_is_503() {
    app().failed_connect_is_503(&MODEL_CREATE).await;
}

#[tokio::test]
async fn a_model_profile_delete_the_manager_is_lost_after_taking_is_504() {
    app().lost_after_hand_over_is_504(&MODEL_REMOVE).await;
}

#[tokio::test]
async fn a_model_profile_delete_of_a_connect_that_fails_is_503() {
    app().failed_connect_is_503(&MODEL_REMOVE).await;
}

#[tokio::test]
async fn a_server_profile_create_the_manager_is_lost_after_taking_is_504() {
    app().lost_after_hand_over_is_504(&SERVER_CREATE).await;
}

#[tokio::test]
async fn a_server_profile_create_of_a_connect_that_fails_is_503() {
    app().failed_connect_is_503(&SERVER_CREATE).await;
}

#[tokio::test]
async fn a_server_profile_delete_the_manager_is_lost_after_taking_is_504() {
    app().lost_after_hand_over_is_504(&SERVER_REMOVE).await;
}

#[tokio::test]
async fn a_server_profile_delete_of_a_connect_that_fails_is_503() {
    app().failed_connect_is_503(&SERVER_REMOVE).await;
}

#[tokio::test]
async fn an_instance_create_the_manager_is_lost_after_taking_is_504() {
    app().lost_after_hand_over_is_504(&INSTANCE_CREATE).await;
}

#[tokio::test]
async fn an_instance_create_of_a_connect_that_fails_is_503() {
    app().failed_connect_is_503(&INSTANCE_CREATE).await;
}

#[tokio::test]
async fn an_instance_remove_the_manager_is_lost_after_taking_is_504() {
    app().lost_after_hand_over_is_504(&INSTANCE_REMOVE).await;
}

#[tokio::test]
async fn an_instance_remove_of_a_connect_that_fails_is_503() {
    app().failed_connect_is_503(&INSTANCE_REMOVE).await;
}

#[tokio::test]
async fn a_control_verb_the_manager_is_lost_after_taking_is_504() {
    for route in VERBS {
        app().lost_after_hand_over_is_504(&route).await;
    }
}

#[tokio::test]
async fn a_control_verb_of_a_connect_that_fails_is_503() {
    for route in VERBS {
        app().failed_connect_is_503(&route).await;
    }
}

#[tokio::test]
async fn a_boot_write_the_manager_is_lost_after_taking_is_504() {
    app().lost_after_hand_over_is_504(&BOOT_WRITE).await;
}

#[tokio::test]
async fn a_boot_write_of_a_connect_that_fails_is_503() {
    app().failed_connect_is_503(&BOOT_WRITE).await;
}

#[tokio::test]
async fn a_tags_write_the_manager_is_lost_after_taking_is_504() {
    app().lost_after_hand_over_is_504(&TAGS_WRITE).await;
}

#[tokio::test]
async fn a_tags_write_of_a_connect_that_fails_is_503() {
    app().failed_connect_is_503(&TAGS_WRITE).await;
}

// ── W2 (a): the writes under the write limit, the boot read on the read's ───

#[tokio::test]
async fn a_control_verb_past_the_read_limit_answers_204() {
    for route in VERBS {
        app().write_past_the_read_limit_is_204(&route).await;
    }
}

#[tokio::test]
async fn a_tags_write_past_the_read_limit_answers_204() {
    app().write_past_the_read_limit_is_204(&TAGS_WRITE).await;
}

#[tokio::test]
async fn a_boot_write_past_the_read_limit_answers_200() {
    app().boot_write_past_the_read_limit_is_200().await;
}

#[tokio::test]
async fn a_boot_read_past_the_read_limit_is_503() {
    app().boot_read_past_the_read_limit_is_503().await;
}
