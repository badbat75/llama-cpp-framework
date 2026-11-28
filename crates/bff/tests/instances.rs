//! `GET /api/instances`: the manager's instances and the state of each, and
//! `GET /api/version`, through `pai_bff::router_with_session` with a fake
//! manager client and a fake authenticator, never the real system bus, pipe,
//! PAM or LogonUser. Each test hands the router one request
//! (`tower::ServiceExt::oneshot`) under a deadline, so a bff that waits on the
//! manager fails instead of hanging.
//!
//! The answer (fe/briefs/api-contract.md): 200 and
//! `[{"id": ..., "label": ..., "engine": "llama.cpp", "serverProfile": ..., "state": ..., "port": ..., "tags":
//! [...], "instanceTags": [...], "instanceProfile": ..., "hostname": ...,
//! "converged": ..., "reason": ..., "pendingRestart": ...}]` in the
//! manager's order, the state as the
//! manager names it, `port` the configured one (0 when the manager answers
//! no row for the instance), `tags` the union the bff builds of the two
//! lists the manager answers for the instance (the server profile's first,
//! then its own, none twice), `instanceTags` the instance's own list only
//! (saving it back must not copy the profile's tags into the instance), and
//! the five keys IP4d added (the row's `instanceProfile` and `hostname`, each
//! `null` when the record names none, and the row's three states: `converged`,
//! its `reason` as `{"key", "args"}` or `null`, and `pendingRestart`);
//! errors in the common
//! shape `{"key": <strings key>, "args": [...]}`: 503 `client-unavailable`
//! when no manager answers (none, gone, or too slow), 502
//! `client-not-authorized` when the manager refuses the bff, 502
//! `client-other` with no args on any other failure, never the manager's
//! detail.
//!
//! `PUT /api/instances/{id}/tags` `{"tags": [<string>, ...]}` replaces the
//! tags of one instance and answers 204 with no body. It acts for the
//! signed-in user, as the verbs do, and the manager is the one that judges
//! the list: a list its check refuses is 422 `{"key": "server-bad-tags",
//! "args": [...], "field": "tags"}`, no such instance 404
//! `client-no-instance`, a body the bff cannot read 400 `tags-malformed`
//! with the manager not asked, a refusal of the user 403
//! `session-not-administrator` with the session ended.
//!
//! Both reads sit behind a session, as every other read (`PLAN.md` 13a: a
//! visitor who has not signed in sees only the sign-in page). With no session,
//! and with the cookie of a session that ended, the answer is 401
//! `{"key": "session-required", "args": []}`: no `Set-Cookie` (a refused read
//! hands out no session) and the manager is never connected to. A verb the
//! route does not route is the router's 405, which answers before the session
//! is read.
//!
//! A signed-in read still acts as the bff's own account: `Connect` is handed
//! no principal (`None`), because these reads ask only what the manager
//! answers without authorization. The session gates the read, it does not
//! choose who it acts for. Neither read moves the session's idle deadline, so
//! a page that only polls them still ends its session.

use axum::body::Body;
use axum::extract::ConnectInfo;
use axum::http::{header, Method, Request, StatusCode};
use axum::Router;
use http_body_util::BodyExt;
use pai_bff::session::{AuthError, Authenticator, Identity};
use pai_bff::{Client, Connect};
use pai_core::checks::Refusal;
use pai_core::client::{ClientError, InstanceInfo, InstanceState, ManagerClient, ManagerFile};
use serde_json::{json, Value};
use std::collections::HashMap;
use std::net::SocketAddr;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};
use tower::ServiceExt;

/// How long the bff waits for the manager in these tests.
const TIMEOUT: Duration = Duration::from_millis(200);

/// How long a test waits for the bff's answer before failing.
const DEADLINE: Duration = Duration::from_secs(5);

/// The client address a sign-in is sent from (the lockout counters key on it).
const CLIENT: &str = "127.0.0.1:51000";

/// The revision secret these routers are built with: the routes tested here
/// never read it.
const REVISION_KEY: [u8; 16] = [0; 16];

/// The session idle timeout chosen for the bff (`session.rs`'s own).
const IDLE: Duration = Duration::from_secs(30 * 60);

/// A step just under the idle timeout: the read that runs that far after the
/// sign-in leaves one second to the deadline the sign-in set.
const SHORT: Duration = Duration::from_secs(30 * 60 - 60);

/// The key of the gate's answer (`session.rs`).
const SESSION_REQUIRED: &str = "session-required";

/// The port the fake's rows carry: the bff answers it as `port`, it is the
/// manager's row that picks it (one row below carries another).
const PORT: u16 = 8080;

type Log = Arc<Mutex<Vec<String>>>;

/// A manager that answers from its fields and logs every call
/// (`list`, `status <id>`, `start <id>`, ...).
struct FakeManager {
    instances: Result<Vec<String>, ClientError>,
    states: HashMap<String, Result<InstanceState, ClientError>>,
    /// What `create_instance` answers (slice 16).
    create: Result<(), ClientError>,
    /// What `remove_instance` answers (slice 16).
    remove: Result<(), ClientError>,
    /// How long `list_instances` takes.
    delay: Duration,
    /// What `boot_enabled` answers, for the tests that read the boot flag.
    boot: Result<bool, ClientError>,
    /// What `boot_enabled` answers for one id, ahead of `boot`: the list reads
    /// the flag of every instance, so a test of the list needs answers that
    /// differ per instance (one on, one off, one that cannot be read).
    boots: HashMap<String, Result<bool, ClientError>>,
    /// What `instances` answers: the rows, each with its two tag lists. The
    /// builder leaves it the failure a client that does not know the verb
    /// answers, so a fake that says nothing about tags reads as a manager
    /// that has none.
    rows: Result<Vec<InstanceInfo>, ClientError>,
    /// What `set_instance_tags` answers.
    tags: Result<(), ClientError>,
    log: Log,
}

impl FakeManager {
    fn new(log: &Log, instances: &[(&str, InstanceState)]) -> FakeManager {
        let owned = instances
            .iter()
            .map(|(id, s)| (id.to_string(), s.clone()))
            .collect::<Vec<_>>();
        FakeManager::listing(log, &owned)
    }

    /// The same, from names the caller owns: the app builders hand these to a
    /// `'static` closure.
    fn listing(log: &Log, instances: &[(String, InstanceState)]) -> FakeManager {
        FakeManager {
            instances: Ok(instances
                .iter()
                .map(|(id, _)| id.clone())
                .collect::<Vec<String>>()),
            states: instances
                .iter()
                .map(|(id, s)| (id.clone(), Ok(s.clone())))
                .collect(),
            create: Ok(()),
            remove: Ok(()),
            delay: Duration::ZERO,
            boot: Ok(false),
            boots: HashMap::new(),
            rows: Err(ClientError::Other("no Instances verb".into())),
            tags: Ok(()),
            log: log.clone(),
        }
    }

    /// A row of `rows`: the id, the engine, the server profile (the id's own,
    /// as the record names it by default), the port and the two tag lists.
    fn row(id: &str, port: u16, profile_tags: &[&str], instance_tags: &[&str]) -> InstanceInfo {
        InstanceInfo {
            id: id.to_string(),
            engine: "llama.cpp".to_string(),
            server_profile: Some(id.to_string()),
            port,
            profile_tags: profile_tags.iter().map(|t| t.to_string()).collect(),
            instance_tags: instance_tags.iter().map(|t| t.to_string()).collect(),
            label: None,
            converged: true,
            reason: None,
            pending_restart: false,
            instance_profile: None,
            hostname: None,
        }
    }

    fn record(&self, call: String) {
        self.log
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .push(call);
    }
}

impl ManagerClient for FakeManager {
    fn list_instances(&self) -> Result<Vec<String>, ClientError> {
        self.record("list".into());
        std::thread::sleep(self.delay);
        self.instances.clone()
    }
    fn status(&self, id: &str) -> Result<InstanceState, ClientError> {
        self.record(format!("status {id}"));
        self.states
            .get(id)
            .cloned()
            .unwrap_or(Err(ClientError::NoInstance))
    }
    fn start(&self, id: &str) -> Result<(), ClientError> {
        self.record(format!("start {id}"));
        Ok(())
    }
    fn stop(&self, id: &str) -> Result<(), ClientError> {
        self.record(format!("stop {id}"));
        Ok(())
    }
    fn restart(&self, id: &str) -> Result<(), ClientError> {
        self.record(format!("restart {id}"));
        Ok(())
    }
    fn create_instance(
        &self,
        engine: &str,
        id: &str,
        instance_profile: &str,
    ) -> Result<u16, ClientError> {
        self.record(format!("create {engine} {id} {instance_profile}"));
        self.create.clone().map(|()| 8581)
    }
    fn remove_instance(&self, id: &str) -> Result<(), ClientError> {
        self.record(format!("remove {id}"));
        self.remove.clone()
    }
    fn read_file(&self, name: &str) -> Result<Option<ManagerFile>, ClientError> {
        self.record(format!("read {name}"));
        Ok(None)
    }
    fn write_file(
        &self,
        name: &str,
        _contents: &str,
        _keep_api_key: bool,
    ) -> Result<(), ClientError> {
        self.record(format!("write {name}"));
        Ok(())
    }
    fn boot_enabled(&self, id: &str) -> Result<bool, ClientError> {
        self.record(format!("boot_enabled {id}"));
        self.boots
            .get(id)
            .cloned()
            .unwrap_or_else(|| self.boot.clone())
    }
    fn set_boot_enabled(&self, id: &str, enabled: bool) -> Result<(), ClientError> {
        self.record(format!("set_boot_enabled {id} {enabled}"));
        Ok(())
    }
    fn authenticate(&self, _user: &str, _password: &str) -> Result<(bool, bool), ClientError> {
        Err(ClientError::Unavailable)
    }
    fn instances(&self) -> Result<Vec<InstanceInfo>, ClientError> {
        self.record("instances".into());
        self.rows.clone()
    }
    fn set_instance_tags(&self, id: &str, tags: &[String]) -> Result<(), ClientError> {
        // The empty list logs as no list at all: no trailing space (the
        // join of no tags is "", the log names the instance alone).
        let list = tags.join(",");
        self.record(if list.is_empty() {
            format!("set_instance_tags {id}")
        } else {
            format!("set_instance_tags {id} {list}")
        });
        self.tags.clone()
    }
}

/// The principal every connect was asked for, in order: `None` is a call the
/// bff makes as its own account.
type Principals = Arc<Mutex<Vec<Option<String>>>>;

fn principals_of(p: &Principals) -> Vec<Option<String>> {
    p.lock().unwrap_or_else(|e| e.into_inner()).clone()
}

/// A connect that hands out `make()` each time, counts the calls and records
/// the principal each was asked for.
fn connect_logging(
    count: &Arc<AtomicUsize>,
    principals: &Principals,
    make: impl Fn() -> Result<Client, ClientError> + Send + Sync + 'static,
) -> Connect {
    let (count, principals) = (count.clone(), principals.clone());
    Arc::new(move |principal: Option<&str>| {
        count.fetch_add(1, Ordering::SeqCst);
        principals
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .push(principal.map(str::to_string));
        make()
    })
}

struct Reply {
    status: StatusCode,
    content_type: String,
    /// Every header of the reply, the name lowercased as `http` stores it.
    headers: Vec<(String, String)>,
    body: String,
}

impl Reply {
    fn json(&self) -> Value {
        serde_json::from_str(&self.body)
            .unwrap_or_else(|e| panic!("not JSON ({e}): {:?}", self.body))
    }

    /// The values the reply carries under `name`, in the order they came.
    fn values_of(&self, name: &str) -> Vec<&str> {
        self.headers
            .iter()
            .filter(|(n, _)| n == name)
            .map(|(_, v)| v.as_str())
            .collect()
    }

    /// An error answer: its status and `{"key": key, "args": []}` as JSON,
    /// nothing else.
    fn assert_error(&self, status: StatusCode, key: &str) {
        assert_eq!(self.status, status, "{}", self.body);
        assert!(
            self.content_type.starts_with("application/json"),
            "content-type {:?}",
            self.content_type
        );
        assert_eq!(
            self.json(),
            json!({ "key": key, "args": [] }),
            "{}",
            self.body
        );
    }

    /// The gate's answer: 401 `{"key": "session-required", "args": []}`, and
    /// no `Set-Cookie`: a read refused for want of a session hands out no
    /// session. The framing and the sniffing header ride it like every reply.
    fn assert_session_required(&self) {
        self.assert_error(StatusCode::UNAUTHORIZED, SESSION_REQUIRED);
        assert!(
            self.values_of("set-cookie").is_empty(),
            "a refused read sets a cookie: {:?}",
            self.headers
        );
        for (name, want) in [
            ("x-frame-options", "DENY"),
            ("x-content-type-options", "nosniff"),
        ] {
            assert_eq!(
                self.values_of(name),
                vec![want],
                "{name} exactly once: {:?}",
                self.headers
            );
        }
    }

    /// 200 with the framework's version, the machine's name and its operating
    /// system (AN4). The version is the one the packages are named after
    /// (`version.workspace`); the two host facts are the machine's own, read
    /// from `/` (the fixture of a known document is `tests/version.rs`).
    fn assert_version(&self) {
        assert_eq!(self.status, StatusCode::OK, "{}", self.body);
        assert!(
            self.content_type.starts_with("application/json"),
            "content-type {:?}",
            self.content_type
        );
        let os = pai_core::host::os(std::path::Path::new("/"));
        assert_eq!(
            self.json(),
            json!({
                "version": env!("CARGO_PKG_VERSION"),
                "name": pai_bff::listen::hostname(),
                "os": {
                    "name": os.name,
                    "version": os.version,
                    "kernel": os.kernel,
                },
            }),
            "{}",
            self.body
        );
    }

    /// 200 with exactly `{"enabled": <bool>}`: the flag alone.
    fn assert_enabled(&self, enabled: bool) {
        assert_eq!(self.status, StatusCode::OK, "{}", self.body);
        let value = self.json();
        let obj = value
            .as_object()
            .unwrap_or_else(|| panic!("not an object: {}", self.body));
        assert_eq!(obj.len(), 1, "the flag alone: {}", self.body);
        assert_eq!(
            obj.get("enabled").and_then(Value::as_bool),
            Some(enabled),
            "{}",
            self.body
        );
    }

    /// `GET /api/session` for a session that is still signed in.
    fn assert_signed_in_as(&self, user: &str) {
        assert_eq!(self.status, StatusCode::OK, "signed in: {}", self.body);
        assert_eq!(self.json(), json!({ "user": user }), "{}", self.body);
    }
}

/// The authenticator these routers sign in through: any name with the one
/// password, never PAM, LogonUser or the manager.
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

/// The time the session routes read, moved by the test.
struct FakeClock(Arc<Mutex<Instant>>);

impl FakeClock {
    fn new() -> FakeClock {
        FakeClock(Arc::new(Mutex::new(Instant::now())))
    }

    fn clock(&self) -> pai_bff::session::Clock {
        let now = self.0.clone();
        Arc::new(move || *now.lock().unwrap_or_else(|e| e.into_inner()))
    }

    fn advance(&self, by: Duration) {
        *self.0.lock().unwrap_or_else(|e| e.into_inner()) += by;
    }
}

/// A router over a fake manager, a fake authenticator and a clock the test
/// moves, with the log of the manager's calls, the count of connects and the
/// principals each was handed.
struct App {
    router: Router,
    clock: FakeClock,
    log: Log,
    connects: Arc<AtomicUsize>,
    principals: Principals,
}

/// The app over a manager built by `make`, which is handed the app's call log
/// and is called again for each connect.
fn app_at(make: impl Fn(&Log) -> FakeManager + Send + Sync + 'static) -> App {
    app_connecting(move |log| Ok(Box::new(make(log)) as Client))
}

/// The app whose connect hands out what `make` answers, so a connect that
/// fails, hangs, or answers differently each time is testable.
fn app_connecting(
    make: impl Fn(&Log) -> Result<Client, ClientError> + Send + Sync + 'static,
) -> App {
    let log = Log::default();
    let connects = Arc::new(AtomicUsize::new(0));
    let principals = Principals::default();
    let l = log.clone();
    let connect = connect_logging(&connects, &principals, move || make(&l));
    let clock = FakeClock::new();
    let router = pai_bff::router_with_session(
        connect,
        TIMEOUT,
        Arc::new(FakeAuth),
        clock.clock(),
        REVISION_KEY,
    );
    App {
        router,
        clock,
        log,
        connects,
        principals,
    }
}

/// The app whose manager lists `instances` with these states.
fn app_listing(instances: &[(&str, InstanceState)]) -> App {
    let owned = instances
        .iter()
        .map(|(id, s)| (id.to_string(), s.clone()))
        .collect::<Vec<_>>();
    app_at(move |l| FakeManager::listing(l, &owned))
}

impl App {
    fn calls(&self) -> Vec<String> {
        self.log.lock().unwrap_or_else(|e| e.into_inner()).clone()
    }

    /// Nothing was asked of the manager: no connect, no call, no principal
    /// named. The gate answers on its own.
    fn assert_asked_nothing(&self, what: &str) {
        assert_eq!(self.connects.load(Ordering::SeqCst), 0, "{what}: connected");
        assert!(self.calls().is_empty(), "{what}: {:?}", self.calls());
        assert!(
            principals_of(&self.principals).is_empty(),
            "{what}: a principal was named"
        );
    }

    /// Sign `alice` in and return the cookie the browser sends back.
    async fn sign_in(&self) -> String {
        self.sign_in_as("alice").await
    }

    /// Sign `user` in and return the cookie the browser sends back.
    async fn sign_in_as(&self, user: &str) -> String {
        let mut request = Request::builder()
            .method(Method::POST)
            .uri("/api/session")
            .header(header::CONTENT_TYPE, "application/json")
            .body(Body::from(
                json!({ "username": user, "password": "correct horse" }).to_string(),
            ))
            .expect("request");
        request
            .extensions_mut()
            .insert(ConnectInfo(CLIENT.parse::<SocketAddr>().expect("address")));
        let response = tokio::time::timeout(DEADLINE, self.router.clone().oneshot(request))
            .await
            .expect("sign-in answered in time")
            .expect("infallible");
        assert_eq!(response.status(), StatusCode::OK, "signed in as {user}");
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

    /// `DELETE /api/session`: the session ends. The cookie the reply carries
    /// clears the name, so the browser drops it; the test keeps the old one,
    /// the shape of a tab that signs out in another tab.
    async fn sign_out(&self, cookie: &str) -> Reply {
        let r = self.get("/api/session", Some(cookie)).await;
        let deleted = self.ask(Method::DELETE, "/api/session", Some(cookie)).await;
        assert_eq!(r.status, StatusCode::OK, "signed in first: {}", r.body);
        assert_eq!(deleted.status, StatusCode::NO_CONTENT, "{}", deleted.body);
        deleted
    }

    async fn get(&self, uri: &str, cookie: Option<&str>) -> Reply {
        self.ask(Method::GET, uri, cookie).await
    }

    async fn ask(&self, method: Method, uri: &str, cookie: Option<&str>) -> Reply {
        let mut builder = Request::builder().method(method).uri(uri);
        if let Some(c) = cookie {
            builder = builder.header(header::COOKIE, c);
        }
        let request = builder.body(Body::empty()).expect("request");
        self.answer(request, uri).await
    }

    /// A request with a body sent as `application/json` (the tags write, and
    /// the bodies that are not one).
    async fn ask_json(&self, method: Method, uri: &str, cookie: Option<&str>, body: &str) -> Reply {
        let mut builder = Request::builder().method(method).uri(uri);
        if let Some(c) = cookie {
            builder = builder.header(header::COOKIE, c);
        }
        let request = builder
            .header(header::CONTENT_TYPE, "application/json")
            .body(Body::from(body.to_string()))
            .expect("request");
        self.answer(request, uri).await
    }

    /// The router's answer to `request`, read into a `Reply` under the
    /// deadline: a bff that waits on the manager fails, it does not hang.
    async fn answer(&self, request: Request<Body>, uri: &str) -> Reply {
        let response = tokio::time::timeout(DEADLINE, self.router.clone().oneshot(request))
            .await
            .unwrap_or_else(|_| panic!("no answer to {uri} within {DEADLINE:?}"))
            .expect("infallible");
        let status = response.status();
        let content_type = response
            .headers()
            .get(header::CONTENT_TYPE)
            .map(|v| v.to_str().expect("ascii content-type").to_string())
            .unwrap_or_default();
        let headers = response
            .headers()
            .iter()
            .map(|(name, value)| {
                (
                    name.as_str().to_string(),
                    String::from_utf8_lossy(value.as_bytes()).to_string(),
                )
            })
            .collect();
        let bytes = response
            .into_body()
            .collect()
            .await
            .expect("body")
            .to_bytes();
        Reply {
            status,
            content_type,
            headers,
            body: String::from_utf8(bytes.to_vec()).expect("utf-8 body"),
        }
    }

    /// Sign in and `PUT /api/instances/{id}/tags` with `body`.
    async fn put_tags(&self, id: &str, body: &str) -> (Reply, String) {
        let cookie = self.sign_in().await;
        let r = self
            .ask_json(
                Method::PUT,
                &format!("/api/instances/{id}/tags"),
                Some(&cookie),
                body,
            )
            .await;
        (r, cookie)
    }

    /// Sign in and read `GET /api/instances` with that session.
    async fn instances(&self) -> Reply {
        let cookie = self.sign_in().await;
        self.get("/api/instances", Some(&cookie)).await
    }

    /// Sign in and read `GET /api/version` with that session.
    async fn version(&self) -> Reply {
        let cookie = self.sign_in().await;
        self.get("/api/version", Some(&cookie)).await
    }
}

// ── Failure paths first: the session ────────────────────────────────────

/// With no session, `GET /api/instances` and `GET /api/version` answer 401
/// `session-required` with no `Set-Cookie`, and the manager is never even
/// connected to: the gate answers on its own.
#[tokio::test]
async fn the_instances_and_version_reads_need_a_session() {
    for uri in ["/api/instances", "/api/version"] {
        let app = app_listing(&[("default", InstanceState::Active)]);
        app.get(uri, None).await.assert_session_required();
        app.assert_asked_nothing(&format!("GET {uri} with no session"));
    }
}

/// A cookie of a session that ended (the tab signed out, the manager refused
/// the user, the idle timeout passed) is refused on the same reads, and asks
/// the manager nothing.
#[tokio::test]
async fn a_terminated_session_is_refused_on_the_reads() {
    let app = app_listing(&[("default", InstanceState::Active)]);
    let cookie = app.sign_in().await;
    app.sign_out(&cookie).await;
    for uri in ["/api/instances", "/api/version"] {
        app.get(uri, Some(&cookie)).await.assert_session_required();
    }
    app.assert_asked_nothing("the reads of a cookie of an ended session");
}

/// A verb the route does not route is the router's 405, not the gate's 401:
/// the routing answers before the session is read, and the manager is asked
/// nothing.
#[tokio::test]
async fn instances_refuses_another_method_without_asking_the_manager() {
    // The two writes of slice 16 (`POST /api/instances`,
    // `DELETE /api/instances/{id}`) are routed, so only the verbs no route
    // answers are here.
    for (method, uri) in [
        (Method::POST, "/api/version"),
        (Method::PUT, "/api/version"),
        (Method::DELETE, "/api/version"),
        (Method::PUT, "/api/instances"),
        (Method::DELETE, "/api/instances"),
    ] {
        let app = app_listing(&[("default", InstanceState::Active)]);
        let r = app.ask(method.clone(), uri, None).await;
        assert_eq!(
            r.status,
            StatusCode::METHOD_NOT_ALLOWED,
            "{method} {uri}: {}",
            r.body
        );
        app.assert_asked_nothing(&format!("{method} {uri}"));
    }
}

/// No manager to connect to (not running, no bus, no pipe): 503 with the
/// key the web UI shows its message for.
#[tokio::test]
async fn no_manager_is_unavailable() {
    let app = app_connecting(|_| Err(ClientError::Unavailable));
    let r = app.instances().await;
    r.assert_error(StatusCode::SERVICE_UNAVAILABLE, "client-unavailable");
    assert_eq!(app.connects.load(Ordering::SeqCst), 1, "connected once");
}

/// A manager that goes away during the call (the connection broke) is the
/// same: unavailable.
#[tokio::test]
async fn a_manager_gone_during_the_call_is_unavailable() {
    let app = app_at(|l| FakeManager {
        instances: Err(ClientError::Unavailable),
        ..FakeManager::new(l, &[])
    });
    let r = app.instances().await;
    r.assert_error(StatusCode::SERVICE_UNAVAILABLE, "client-unavailable");
    assert_eq!(app.calls(), ["list"]);
}

/// A manager that does not answer within the timeout: unavailable, answered
/// at the timeout, never a hang (the slow call goes on off the request).
#[tokio::test]
async fn a_manager_that_does_not_answer_in_time_is_unavailable() {
    let app = app_at(|l| FakeManager {
        delay: Duration::from_secs(2),
        ..FakeManager::new(l, &[("default", InstanceState::Active)])
    });
    let started = Instant::now();
    let r = app.instances().await;
    let took = started.elapsed();
    r.assert_error(StatusCode::SERVICE_UNAVAILABLE, "client-unavailable");
    assert!(
        took < Duration::from_secs(1),
        "answered after {took:?}, not at the {TIMEOUT:?} timeout"
    );
}

/// Connecting itself hangs (a busy pipe, a bus that does not answer): the
/// timeout covers the connect too.
#[tokio::test]
async fn a_connect_that_hangs_is_unavailable() {
    let app = app_connecting(|_| {
        std::thread::sleep(Duration::from_secs(2));
        Err(ClientError::Unavailable)
    });
    let started = Instant::now();
    let r = app.instances().await;
    let took = started.elapsed();
    r.assert_error(StatusCode::SERVICE_UNAVAILABLE, "client-unavailable");
    assert!(
        took < Duration::from_secs(1),
        "answered after {took:?}, not at the {TIMEOUT:?} timeout"
    );
}

/// The manager refuses the bff (the pipe's security descriptor or the bus
/// policy leaves its account out): 502 client-not-authorized, so the page can
/// say it is the service's access, not the manager's absence. The session of
/// the reader is not a refusal of that user, so it lives on.
#[tokio::test]
async fn a_refusal_by_the_manager_is_not_authorized() {
    let app = app_connecting(|_| Err(ClientError::NotAuthorized));
    let cookie = app.sign_in().await;
    app.get("/api/instances", Some(&cookie))
        .await
        .assert_error(StatusCode::BAD_GATEWAY, "client-not-authorized");
    app.get("/api/session", Some(&cookie))
        .await
        .assert_signed_in_as("alice");
}

/// Any other failure is 502 `client-other` with no args: the manager's
/// detail (which `client-other`'s `{0}` would carry) stays in the bff.
#[tokio::test]
async fn any_other_failure_is_failed_without_the_detail() {
    let app = app_at(|l| FakeManager {
        instances: Err(ClientError::Other(
            "org.freedesktop.DBus.Error.AccessDenied: secret-detail".into(),
        )),
        ..FakeManager::new(l, &[])
    });
    let r = app.instances().await;
    r.assert_error(StatusCode::BAD_GATEWAY, "client-other");
    assert!(!r.body.contains("secret-detail"), "{}", r.body);
}

/// One instance whose state cannot be read (the service manager's error)
/// fails the whole answer: no list with a hole in it.
#[tokio::test]
async fn a_state_that_cannot_be_read_fails_the_answer() {
    let app = app_at(|l| {
        let mut m = FakeManager::new(
            l,
            &[
                ("default", InstanceState::Active),
                ("coder", InstanceState::Inactive),
            ],
        );
        m.states.insert(
            "coder".into(),
            Err(ClientError::Service(
                "Unit llama.cpp@coder.service not loaded.".into(),
            )),
        );
        m
    });
    let r = app.instances().await;
    r.assert_error(StatusCode::BAD_GATEWAY, "client-other");
    assert!(!r.body.contains("llama.cpp@coder"), "{}", r.body);
}

/// An instance removed between the list and its status (NoInstance) is left
/// out: it no longer exists, the others are still answered.
#[tokio::test]
async fn an_instance_removed_meanwhile_is_left_out() {
    let app = app_at(|l| {
        let mut m = FakeManager::new(
            l,
            &[
                ("default", InstanceState::Active),
                ("gone", InstanceState::Inactive),
            ],
        );
        m.states.insert("gone".into(), Err(ClientError::NoInstance));
        m
    });
    let r = app.instances().await;
    assert_eq!(r.status, StatusCode::OK, "{}", r.body);
    assert_eq!(
        r.json(),
        json!([{ "id": "default", "label": null, "engine": "llama.cpp", "serverProfile": null, "state": "active", "port": 0, "tags": [], "instanceTags": [], "boot": false, "removable": false, "instanceProfile": null, "hostname": null, "converged": true, "reason": null, "pendingRestart": false }]),
        "no rows: port 0, no tags"
    );
}

// ── What it answers ─────────────────────────────────────────────────────

/// No instances: 200 and an empty list (the page's empty state), not an
/// error.
#[tokio::test]
async fn no_instances_is_an_empty_list() {
    let r = app_listing(&[]).instances().await;
    assert_eq!(r.status, StatusCode::OK, "{}", r.body);
    assert!(
        r.content_type.starts_with("application/json"),
        "content-type {:?}",
        r.content_type
    );
    assert_eq!(r.json(), json!([]));
}

/// Each instance with its state, in the order the manager lists them, the
/// state named as the manager names it (an unknown name passed on as is).
#[tokio::test]
async fn each_instance_with_its_state_in_the_manager_s_order() {
    let r = app_listing(&[
        ("default", InstanceState::Active),
        ("coder", InstanceState::Failed),
        ("big-model", InstanceState::Inactive),
        ("a", InstanceState::Activating),
        ("z", InstanceState::Deactivating),
        ("odd", InstanceState::Other("maintenance".into())),
    ])
    .instances()
    .await;
    assert_eq!(r.status, StatusCode::OK, "{}", r.body);
    assert_eq!(
        r.json(),
        json!([
            { "id": "default", "label": null, "engine": "llama.cpp", "serverProfile": null, "state": "active", "port": 0, "tags": [], "instanceTags": [], "boot": false, "removable": false, "instanceProfile": null, "hostname": null, "converged": true, "reason": null, "pendingRestart": false },
            { "id": "coder", "label": null, "engine": "llama.cpp", "serverProfile": null, "state": "failed", "port": 0, "tags": [], "instanceTags": [], "boot": false, "removable": true, "instanceProfile": null, "hostname": null, "converged": true, "reason": null, "pendingRestart": false },
            { "id": "big-model", "label": null, "engine": "llama.cpp", "serverProfile": null, "state": "inactive", "port": 0, "tags": [], "instanceTags": [], "boot": false, "removable": true, "instanceProfile": null, "hostname": null, "converged": true, "reason": null, "pendingRestart": false },
            { "id": "a", "label": null, "engine": "llama.cpp", "serverProfile": null, "state": "activating", "port": 0, "tags": [], "instanceTags": [], "boot": false, "removable": true, "instanceProfile": null, "hostname": null, "converged": true, "reason": null, "pendingRestart": false },
            { "id": "z", "label": null, "engine": "llama.cpp", "serverProfile": null, "state": "deactivating", "port": 0, "tags": [], "instanceTags": [], "boot": false, "removable": true, "instanceProfile": null, "hostname": null, "converged": true, "reason": null, "pendingRestart": false },
            { "id": "odd", "label": null, "engine": "llama.cpp", "serverProfile": null, "state": "maintenance", "port": 0, "tags": [], "instanceTags": [], "boot": false, "removable": true, "instanceProfile": null, "hostname": null, "converged": true, "reason": null, "pendingRestart": false },
        ])
    );
}

/// The list carries each instance's own boot flag (`boot`): the page's switch
/// reads it from the list, so the list asks `BootEnabled` for every instance,
/// as the manager answers it of the bff's own account. Each instance keeps its
/// own answer: one on and one off is not one flag for the host.
#[tokio::test]
async fn each_instance_carries_its_own_boot_flag() {
    let app = app_at(|l| {
        let mut m = FakeManager::new(
            l,
            &[
                ("default", InstanceState::Active),
                ("coder", InstanceState::Inactive),
            ],
        );
        m.boots.insert("default".into(), Ok(true));
        m.boots.insert("coder".into(), Ok(false));
        m
    });
    let r = app.instances().await;
    assert_eq!(r.status, StatusCode::OK, "{}", r.body);
    assert_eq!(
        r.json(),
        json!([
            { "id": "default", "label": null, "engine": "llama.cpp", "serverProfile": null, "state": "active", "port": 0, "tags": [], "instanceTags": [], "boot": true, "removable": false, "instanceProfile": null, "hostname": null, "converged": true, "reason": null, "pendingRestart": false },
            { "id": "coder", "label": null, "engine": "llama.cpp", "serverProfile": null, "state": "inactive", "port": 0, "tags": [], "instanceTags": [], "boot": false, "removable": true, "instanceProfile": null, "hostname": null, "converged": true, "reason": null, "pendingRestart": false },
        ])
    );
    assert_eq!(
        app.calls(),
        [
            "list",
            "instances",
            "status default",
            "boot_enabled default",
            "status coder",
            "boot_enabled coder",
        ],
        "the flag of each instance is read once, beside its state"
    );
}

/// One boot flag that cannot be read is `null` on that one item and the list is
/// still answered: the flag is extra, like the row's port and tags, and a host
/// whose systemd cannot be asked still shows its instances.
#[tokio::test]
async fn a_boot_flag_that_cannot_be_read_is_null_and_the_list_answers() {
    let app = app_at(|l| {
        let mut m = FakeManager::new(
            l,
            &[
                ("default", InstanceState::Active),
                ("coder", InstanceState::Inactive),
            ],
        );
        m.boots.insert(
            "coder".into(),
            Err(ClientError::Service(
                "Unit llama.cpp@coder.service not loaded.".into(),
            )),
        );
        m
    });
    let r = app.instances().await;
    assert_eq!(r.status, StatusCode::OK, "{}", r.body);
    assert_eq!(
        r.json(),
        json!([
            { "id": "default", "label": null, "engine": "llama.cpp", "serverProfile": null, "state": "active", "port": 0, "tags": [], "instanceTags": [], "boot": false, "removable": false, "instanceProfile": null, "hostname": null, "converged": true, "reason": null, "pendingRestart": false },
            { "id": "coder", "label": null, "engine": "llama.cpp", "serverProfile": null, "state": "inactive", "port": 0, "tags": [], "instanceTags": [], "boot": null, "removable": true, "instanceProfile": null, "hostname": null, "converged": true, "reason": null, "pendingRestart": false },
        ]),
        "the instance whose flag is unanswerable is listed, its flag null"
    );
}

/// Reading the instances asks only the calls the manager answers without
/// authorization: the list, the rows for the tags, then each status and each
/// boot flag, the rows asked once for the whole list. Nothing starts, stops or
/// reads a file.
#[tokio::test]
async fn reading_the_instances_asks_only_the_read_only_calls() {
    let app = app_listing(&[
        ("default", InstanceState::Active),
        ("coder", InstanceState::Inactive),
    ]);
    let r = app.instances().await;
    assert_eq!(r.status, StatusCode::OK, "{}", r.body);
    assert_eq!(
        app.calls(),
        [
            "list",
            "instances",
            "status default",
            "boot_enabled default",
            "status coder",
            "boot_enabled coder",
        ]
    );
}

/// The signed-in read acts as the bff's own account: the connect is asked
/// with no principal, so the manager answers it with the bff's rights (the
/// reads it asks need none). The session gates the read, it does not choose
/// who it acts for. A route that acts for the user names them instead
/// (`tests/server.rs`).
#[tokio::test]
async fn the_instances_route_connects_without_a_principal() {
    let app = app_listing(&[("default", InstanceState::Active)]);
    let r = app.instances().await;
    assert_eq!(r.status, StatusCode::OK, "{}", r.body);
    assert_eq!(
        principals_of(&app.principals),
        vec![None],
        "the bff acts as itself here"
    );
}

/// A manager that starts after the bff is seen at the next request: the
/// bff asks it anew each time, it does not keep the first failure.
#[tokio::test]
async fn a_manager_started_later_is_seen_at_the_next_request() {
    let asked = Arc::new(AtomicUsize::new(0));
    let seen = asked.clone();
    let app = app_at(move |l| {
        // `seen` counts the connects: this closure runs once for each.
        if seen.fetch_add(1, Ordering::SeqCst) == 0 {
            FakeManager {
                instances: Err(ClientError::Unavailable),
                ..FakeManager::new(l, &[])
            }
        } else {
            FakeManager::new(l, &[("default", InstanceState::Active)])
        }
    });
    let cookie = app.sign_in().await;
    let first = app.get("/api/instances", Some(&cookie)).await;
    first.assert_error(StatusCode::SERVICE_UNAVAILABLE, "client-unavailable");
    let second = app.get("/api/instances", Some(&cookie)).await;
    assert_eq!(second.status, StatusCode::OK, "{}", second.body);
    assert_eq!(
        second.json(),
        json!([{ "id": "default", "label": null, "engine": "llama.cpp", "serverProfile": null, "state": "active", "port": 0, "tags": [], "instanceTags": [], "boot": false, "removable": false, "instanceProfile": null, "hostname": null, "converged": true, "reason": null, "pendingRestart": false }])
    );
    assert_eq!(asked.load(Ordering::SeqCst), 2, "asked anew each time");
    assert_eq!(app.connects.load(Ordering::SeqCst), 2);
}

/// `GET /api/version` answers the framework version to a signed-in reader;
/// the same route with no session is 401 (`the_instances_and_version_reads_need_a_session`).
#[tokio::test]
async fn version_is_served_to_a_signed_in_reader() {
    let app = app_connecting(|_| Err(ClientError::Unavailable));
    app.version().await.assert_version();
    assert_eq!(
        app.connects.load(Ordering::SeqCst),
        0,
        "the version read asks the manager nothing"
    );
}

/// The other routes are unchanged beside these two: the model-profile list
/// and the control verbs keep their own session rule (`tests/model_profiles.rs`,
/// `tests/control.rs`), and the page at `/` is open to anyone.
#[tokio::test]
async fn the_page_stays_open_beside_the_gated_reads() {
    let app = app_connecting(|_| Err(ClientError::Unavailable));
    let r = app.get("/", None).await;
    assert_eq!(r.status, StatusCode::OK, "{}", r.body);
    assert!(
        r.content_type.starts_with("text/html"),
        "content-type {:?}",
        r.content_type
    );
    app.assert_asked_nothing("GET / with no session");
}

// ── The idle deadline: a session read only through these reads ends ─────

/// `GET /api/version` is a read, not an action: it never moves a session's
/// idle deadline, which stays the sign-in's, to the second: at the timeout the
/// session is alive, one second later it is gone.
#[tokio::test]
async fn the_version_read_does_not_move_the_idle_deadline() {
    let alive = app_listing(&[("default", InstanceState::Active)]);
    let cookie = alive.sign_in().await;
    alive.clock.advance(SHORT);
    let r = alive.get("/api/version", Some(&cookie)).await;
    assert_eq!(r.status, StatusCode::OK, "the read is answered: {}", r.body);
    alive.clock.advance(IDLE - SHORT);
    alive
        .get("/api/session", Some(&cookie))
        .await
        .assert_signed_in_as("alice");

    // One second past that same deadline: the read, in the middle, moved
    // nothing. A read that refreshed the session would still be alive here.
    let gone = app_listing(&[("default", InstanceState::Active)]);
    let cookie = gone.sign_in().await;
    gone.clock.advance(SHORT);
    gone.get("/api/version", Some(&cookie))
        .await
        .assert_version();
    gone.clock.advance(IDLE - SHORT + Duration::from_secs(1));
    gone.get("/api/session", Some(&cookie))
        .await
        .assert_error(StatusCode::UNAUTHORIZED, SESSION_REQUIRED);
}

/// A tab that only ever reads these three routes (the version, the instances,
/// one instance's boot flag) is a session that ends: the page left open keeps
/// its reads and still signs itself out at the idle timeout.
#[tokio::test]
async fn a_session_read_only_through_the_gated_reads_still_expires() {
    let app = app_listing(&[("default", InstanceState::Active)]);
    let cookie = app.sign_in().await;
    app.clock.advance(SHORT);
    app.get("/api/version", Some(&cookie))
        .await
        .assert_version();
    let r = app.get("/api/instances", Some(&cookie)).await;
    assert_eq!(
        r.json(),
        json!([{ "id": "default", "label": null, "engine": "llama.cpp", "serverProfile": null, "state": "active", "port": 0, "tags": [], "instanceTags": [], "boot": false, "removable": false, "instanceProfile": null, "hostname": null, "converged": true, "reason": null, "pendingRestart": false }]),
        "{}",
        r.body
    );
    app.get("/api/instances/default/boot", Some(&cookie))
        .await
        .assert_enabled(false);
    app.clock.advance(IDLE - SHORT + Duration::from_secs(1));
    app.get("/api/session", Some(&cookie))
        .await
        .assert_error(StatusCode::UNAUTHORIZED, SESSION_REQUIRED);
    // The session is gone: the same reads now answer 401.
    app.get("/api/instances", Some(&cookie))
        .await
        .assert_session_required();
}

// ── The tags of the rows ────────────────────────────────────────────────

/// The app whose manager answers `rows` from `Instances` and `tags` from
/// `SetInstanceTags`, beside the ids and states of `ids`.
fn app_tags(
    ids: &[(&str, InstanceState)],
    rows: Result<Vec<InstanceInfo>, ClientError>,
    tags: Result<(), ClientError>,
) -> App {
    let owned = ids
        .iter()
        .map(|(id, s)| (id.to_string(), s.clone()))
        .collect::<Vec<_>>();
    app_at(move |l| FakeManager {
        rows: rows.clone(),
        tags: tags.clone(),
        ..FakeManager::listing(l, &owned)
    })
}

/// The row's server profile is on the item, named as the manager names it:
/// the page shows which `server@<id>.ini` the instance runs, and a manager
/// that names none (a row from a manager that knows no profile) answers
/// `null`, it does not fall back to the instance's id.
#[tokio::test]
async fn the_row_carries_the_server_profile_the_manager_names() {
    let mut big = FakeManager::row("big", PORT, &[], &[]);
    big.server_profile = Some("big-model".into());
    let mut plain = FakeManager::row("plain", PORT, &[], &[]);
    plain.server_profile = None;
    let app = app_tags(
        &[
            ("big", InstanceState::Active),
            ("plain", InstanceState::Inactive),
        ],
        Ok(vec![big, plain]),
        Ok(()),
    );
    let r = app.instances().await;
    assert_eq!(r.status, StatusCode::OK, "{}", r.body);
    assert_eq!(
        r.json(),
        json!([
            { "id": "big", "label": null, "engine": "llama.cpp", "serverProfile": "big-model", "state": "active", "port": 8080, "tags": [], "instanceTags": [], "boot": false, "removable": true, "instanceProfile": null, "hostname": null, "converged": true, "reason": null, "pendingRestart": false },
            { "id": "plain", "label": null, "engine": "llama.cpp", "serverProfile": null, "state": "inactive", "port": 8080, "tags": [], "instanceTags": [], "boot": false, "removable": true, "instanceProfile": null, "hostname": null, "converged": true, "reason": null, "pendingRestart": false },
        ]),
        "the profile the row names, and `null` where the row names none"
    );
}

/// The row of the list keeps its id, engine and state and gains `port`,
/// `tags` and `instanceTags`: the tags are the union, the server profile's
/// list first, then the instance's own, none twice; the port is the row's.
/// An instance the rows do not mention is answered with port 0 and no tags.
#[tokio::test]
async fn the_rows_carry_the_tags_of_the_profile_then_their_own_none_twice() {
    let rows = Ok(vec![
        FakeManager::row("default", PORT, &["edge", "v2"], &["v2", "gpu"]),
        FakeManager::row("coder", 18123, &[], &["bench"]),
        FakeManager::row("gone", PORT, &["old"], &[]),
    ]);
    let app = app_tags(
        &[
            ("default", InstanceState::Active),
            ("coder", InstanceState::Inactive),
        ],
        rows,
        Ok(()),
    );
    let r = app.instances().await;
    assert_eq!(r.status, StatusCode::OK, "{}", r.body);
    assert_eq!(
        r.json(),
        json!([
            { "id": "default", "label": null, "engine": "llama.cpp", "serverProfile": "default", "state": "active", "port": 8080, "tags": ["edge", "v2", "gpu"], "instanceTags": ["v2", "gpu"], "boot": false, "removable": false, "instanceProfile": null, "hostname": null, "converged": true, "reason": null, "pendingRestart": false },
            { "id": "coder", "label": null, "engine": "llama.cpp", "serverProfile": "coder", "state": "inactive", "port": 18123, "tags": ["bench"], "instanceTags": ["bench"], "boot": false, "removable": true, "instanceProfile": null, "hostname": null, "converged": true, "reason": null, "pendingRestart": false },
        ]),
        "the union, the profile's tags first, `v2` once, in the list's order; the port the row carries, not a constant"
    );
}

/// Profile and instance both carry tags, one of them shared: `tags` is the
/// union shown on the page (the profile's first, the shared one once), and
/// `instanceTags` is the instance's own list alone, in its own order. The
/// profile-only tags NEVER appear in `instanceTags`: the editor saves that
/// list back, it must not copy the profile's tags into the instance.
#[tokio::test]
async fn the_shown_tags_are_the_union_the_editable_ones_the_instance_s_own() {
    let rows = Ok(vec![FakeManager::row(
        "default",
        PORT,
        &["edge", "v2", "gpu"],
        &["v2", "lab"],
    )]);
    let app = app_tags(&[("default", InstanceState::Active)], rows, Ok(()));
    let r = app.instances().await;
    assert_eq!(r.status, StatusCode::OK, "{}", r.body);
    assert_eq!(
        r.json(),
        json!([{
            "id": "default",
            "label": null,
            "engine": "llama.cpp",
            "serverProfile": "default",
            "state": "active",
            "port": 8080,
            "tags": ["edge", "v2", "gpu", "lab"],
            "instanceTags": ["v2", "lab"],
            "boot": false,
            "removable": false,
            "instanceProfile": null,
            "hostname": null,
            "converged": true,
            "reason": null,
            "pendingRestart": false,
        }]),
        "the union, then the instance's own list: `edge` and `gpu` are the profile's, they stay out of `instanceTags`"
    );
}

/// A manager with no `Instances` (an install of an older protocol): the rows
/// are answered with no tags, the read still 200: the tags are extra, not a
/// part of the read that can fail it.
#[tokio::test]
async fn a_manager_that_answers_no_rows_shows_no_tags() {
    let app = app_tags(
        &[("default", InstanceState::Active)],
        Err(ClientError::Other("no Instances verb".into())),
        Ok(()),
    );
    let r = app.instances().await;
    assert_eq!(r.status, StatusCode::OK, "{}", r.body);
    assert_eq!(
        r.json(),
        json!([{ "id": "default", "label": null, "engine": "llama.cpp", "serverProfile": null, "state": "active", "port": 0, "tags": [], "instanceTags": [], "boot": false, "removable": false, "instanceProfile": null, "hostname": null, "converged": true, "reason": null, "pendingRestart": false }]),
        "no rows: port 0, no tags"
    );
    assert!(
        !r.body.contains("Instances verb"),
        "the client's detail stays out of the body: {}",
        r.body
    );
}

/// A row whose lists are empty is answered with `tags: []` and
/// `instanceTags: []`, both present, and the row's port: the page renders no
/// chips for it, it does not read a missing field as a failure.
#[tokio::test]
async fn an_instance_with_no_tags_carries_an_empty_list() {
    let app = app_tags(
        &[("default", InstanceState::Active)],
        Ok(vec![FakeManager::row("default", PORT, &[], &[])]),
        Ok(()),
    );
    let r = app.instances().await;
    assert_eq!(r.json()[0].get("tags"), Some(&json!([])), "{}", r.body);
    assert_eq!(
        r.json()[0].get("instanceTags"),
        Some(&json!([])),
        "present, not missing: {}",
        r.body
    );
    assert_eq!(
        r.json()[0].get("port"),
        Some(&json!(8080)),
        "present, not missing: {}",
        r.body
    );
}

// ── The tags write ──────────────────────────────────────────────────────

/// With no session, and with the cookie of a session that ended, the write is
/// 401 `session-required` with no `Set-Cookie`, and the manager is never
/// connected to.
#[tokio::test]
async fn the_tags_write_needs_a_session() {
    let app = app_tags(&[("default", InstanceState::Active)], Ok(vec![]), Ok(()));
    app.ask_json(
        Method::PUT,
        "/api/instances/default/tags",
        None,
        r#"{"tags": ["edge"]}"#,
    )
    .await
    .assert_session_required();
    app.assert_asked_nothing("PUT /api/instances/default/tags with no session");

    let cookie = app.sign_in().await;
    app.sign_out(&cookie).await;
    app.ask_json(
        Method::PUT,
        "/api/instances/default/tags",
        Some(&cookie),
        r#"{"tags": ["edge"]}"#,
    )
    .await
    .assert_session_required();
    app.assert_asked_nothing("the write of a cookie of an ended session");
}

/// The write asks the manager for the instance with the list the body carried,
/// as the signed-in user, and answers 204 with no body.
#[tokio::test]
async fn the_tags_write_sends_the_list_and_answers_no_content() {
    let app = app_tags(&[("default", InstanceState::Active)], Ok(vec![]), Ok(()));
    let (r, _cookie) = app.put_tags("default", r#"{"tags": ["edge", "v2"]}"#).await;
    assert_eq!(r.status, StatusCode::NO_CONTENT, "{}", r.body);
    assert!(r.body.is_empty(), "204 with a body: {:?}", r.body);
    assert_eq!(
        app.calls(),
        ["set_instance_tags default edge,v2"],
        "the list, in the body's order, of that instance"
    );
    assert_eq!(
        principals_of(&app.principals),
        vec![Some("alice".to_string())],
        "the write acts for the signed-in user, as the verbs do"
    );
}

/// An empty list is a write of no tags, not a malformed body.
#[tokio::test]
async fn an_empty_list_clears_the_tags() {
    let app = app_tags(&[("default", InstanceState::Active)], Ok(vec![]), Ok(()));
    let (r, _) = app.put_tags("default", r#"{"tags": []}"#).await;
    assert_eq!(r.status, StatusCode::NO_CONTENT, "{}", r.body);
    assert_eq!(app.calls(), ["set_instance_tags default"]);
}

/// The manager's check refuses the list: 422 with the check's key, its values
/// and `field` "tags", the field the form marks. The manager's own message
/// stays out of the body, and the session of the writer lives on: a refusal
/// of a value is no refusal of the user.
#[tokio::test]
async fn a_refused_list_is_422_with_the_checks_key() {
    let refusal = ClientError::Invalid(Refusal {
        key: "server-bad-tags",
        values: vec![
            "bad characters".into(),
            "Edge".into(),
            "8".into(),
            "1".into(),
            "32".into(),
        ],
    });
    let app = app_tags(
        &[("default", InstanceState::Active)],
        Ok(vec![]),
        Err(refusal),
    );
    let (r, cookie) = app.put_tags("default", r#"{"tags": ["Edge"]}"#).await;
    assert_eq!(r.status, StatusCode::UNPROCESSABLE_ENTITY, "{}", r.body);
    assert_eq!(
        r.json(),
        json!({
            "key": "server-bad-tags",
            "args": ["bad characters", "Edge", "8", "1", "32"],
            "field": "tags",
        })
    );
    app.get("/api/session", Some(&cookie))
        .await
        .assert_signed_in_as("alice");
}

/// No such instance, and an id the manager refuses: 404 `client-no-instance`,
/// the session of the writer living on.
#[tokio::test]
async fn the_tags_of_no_instance_are_404() {
    for failure in [ClientError::NoInstance, ClientError::InvalidId] {
        let app = app_tags(
            &[("default", InstanceState::Active)],
            Ok(vec![]),
            Err(failure.clone()),
        );
        let (r, cookie) = app.put_tags("lan-9", r#"{"tags": ["edge"]}"#).await;
        r.assert_error(StatusCode::NOT_FOUND, "client-no-instance");
        app.get("/api/session", Some(&cookie))
            .await
            .assert_signed_in_as("alice");
    }
}

/// A body the bff cannot read is 400 `tags-malformed` and the manager is not
/// asked: no `tags` field, a value that is not a list, an entry that is not a
/// string, a document that is not an object, and no JSON at all. The session
/// check runs first, so with no session such a body is 401.
#[tokio::test]
async fn a_body_that_is_not_a_list_of_strings_is_400_and_asks_nothing() {
    for body in [
        r#"{}"#,
        r#"{"tags": "edge"}"#,
        r#"{"tags": null}"#,
        r#"{"tags": ["edge", 5]}"#,
        r#"{"tags": [null]}"#,
        r#"{"tags": [["edge"]]}"#,
        r#"["edge"]"#,
        r#"tags=edge"#,
        "",
    ] {
        let app = app_tags(&[("default", InstanceState::Active)], Ok(vec![]), Ok(()));
        let (r, _) = app.put_tags("default", body).await;
        r.assert_error(StatusCode::BAD_REQUEST, "tags-malformed");
        app.assert_asked_nothing(&format!("the body {body:?}"));
    }
    let app = app_tags(&[("default", InstanceState::Active)], Ok(vec![]), Ok(()));
    app.ask_json(Method::PUT, "/api/instances/default/tags", None, r#"{}"#)
        .await
        .assert_session_required();
    app.assert_asked_nothing("a malformed body with no session");
}

/// The manager refuses the signed-in user (they left the `pai` group since
/// they signed in): 403 `session-not-administrator` and the session ends, as
/// on the verbs.
#[tokio::test]
async fn a_refusal_of_the_signed_in_user_ends_the_session() {
    let app = app_tags(
        &[("default", InstanceState::Active)],
        Ok(vec![]),
        Err(ClientError::NotAuthorized),
    );
    let (r, cookie) = app.put_tags("default", r#"{"tags": ["edge"]}"#).await;
    r.assert_error(StatusCode::FORBIDDEN, "session-not-administrator");
    app.get("/api/instances", Some(&cookie))
        .await
        .assert_session_required();
}

/// No manager, and a manager that refuses the connect of the signed-in user:
/// 503 `client-unavailable` and 403 `session-not-administrator` with the
/// session ended, as on the verbs: the connect acts as the user, so a refusal
/// of it is a refusal of them.
#[tokio::test]
async fn the_tags_write_of_no_manager_is_unavailable() {
    let app = app_connecting(|_| Err(ClientError::Unavailable));
    let cookie = app.sign_in().await;
    app.ask_json(
        Method::PUT,
        "/api/instances/default/tags",
        Some(&cookie),
        r#"{"tags": ["edge"]}"#,
    )
    .await
    .assert_error(StatusCode::SERVICE_UNAVAILABLE, "client-unavailable");
}

/// The route is the write: `GET` and `DELETE` on it are the router's 405,
/// which answers before the session is read and asks the manager nothing.
#[tokio::test]
async fn the_tags_route_answers_only_the_write() {
    for method in [Method::GET, Method::DELETE, Method::POST] {
        let app = app_tags(&[("default", InstanceState::Active)], Ok(vec![]), Ok(()));
        let r = app
            .ask_json(method.clone(), "/api/instances/default/tags", None, "{}")
            .await;
        assert_eq!(r.status, StatusCode::METHOD_NOT_ALLOWED, "{method}");
        app.assert_asked_nothing(&format!("{method} /api/instances/default/tags"));
    }
}

// ── Creating and removing an instance (slice 16, the body IP4d) ──────────
// `POST /api/instances` `{"engine", "id", "instanceProfile"}` answers 201
// `{"id", "port"}` once the manager created it, the port the manager picked,
// `DELETE /api/instances/{id}`
// answers 204. Both sit behind the session gate like every other write: with
// no session 401 `session-required` and the manager is asked nothing; a
// refusal of the signed-in user 403 `session-not-administrator` ending the
// session; a manager that took the write and is gone when it answers 504
// `client-timeout` (slice W2: the write crossed the hand-over, so its outcome
// is unknown; a connect that hands nothing over stays 503
// `client-unavailable`); anything else 502. Each
// refusal of the manager keeps its key and its args in the body, the
// manager's own message staying out of it: 409 `instance-exists`,
// `instance-protected` and `instance-running` naming the id, 404
// `client-no-instance`, 422 `no-instance-profile-named` naming the bind the
// body asks for, 404 `client-no-server-profile` and 409
// `client-server-profile-in-use` naming the server profile the instance
// profile names one step behind, 409 `address-in-use` naming the instance
// holding the address, 422 `invalid-hostname` for a host that does not
// resolve, and 503 `no-free-port` when the node has no address left. The id
// and the engine the bff itself refuses are 422 with the check's key and the
// form's field, and a body it cannot read, the retired `serverProfile` key
// among them, is 400 `instance-malformed` with the manager not asked.
// `GET /api/instances` items carry `removable` besides
// `boot`: true for every id that is not the protected `default`.

/// The app whose manager answers `create_instance` with `create` and
/// `remove_instance` with `remove`, listing `lan-2` stopped and the default.
fn app_crud(create: Result<(), ClientError>, remove: Result<(), ClientError>) -> App {
    app_at(move |l| FakeManager {
        create: create.clone(),
        remove: remove.clone(),
        ..FakeManager::new(
            l,
            &[
                ("default", InstanceState::Active),
                ("lan-2", InstanceState::Inactive),
            ],
        )
    })
}

/// `POST /api/instances` with `body`, under `cookie` (none for a visitor).
async fn create(app: &App, cookie: Option<&str>, body: &str) -> Reply {
    app.ask_json(Method::POST, "/api/instances", cookie, body)
        .await
}

/// The error body of a refusal: the key, its args, and the field to mark when
/// the answer names one.
fn args_doc(key: &str, args: &[&str], field: Option<&str>) -> Value {
    let mut doc = json!({ "key": key, "args": args });
    if let Some(field) = field {
        doc["field"] = json!(field);
    }
    doc
}

/// The body a create sends: the engine, the id, the instance profile to bind
/// (IP4d: the server profile the body named before is retired, a body carrying
/// it at all is refused).
const CREATE_BODY: &str = r#"{"engine": "llama.cpp", "id": "lan-2", "instanceProfile": "free"}"#;

/// The whole call of a create: the route calls `create_instance` as the
/// signed-in user, once, with the three words of the body, and answers 201
/// with the id it created and the port the manager picked (the fake answers
/// 8581).
#[tokio::test]
async fn an_instance_is_created_through_the_manager() {
    let app = app_crud(Ok(()), Ok(()));
    let cookie = app.sign_in().await;
    let r = create(&app, Some(&cookie), CREATE_BODY).await;
    assert_eq!(r.status, StatusCode::CREATED, "{}", r.body);
    assert_eq!(
        r.json(),
        json!({ "id": "lan-2", "port": 8581 }),
        "{}",
        r.body
    );
    assert_eq!(
        app.calls(),
        ["create llama.cpp lan-2 free"],
        "one call, the body's three words"
    );
    assert_eq!(
        principals_of(&app.principals),
        [Some("alice".to_string())],
        "as the signed-in user"
    );
}

/// The whole call of a remove: `DELETE /api/instances/{id}` calls
/// `remove_instance` once with the id of the path and answers 204 with no body.
#[tokio::test]
async fn an_instance_is_removed_through_the_manager() {
    let app = app_crud(Ok(()), Ok(()));
    let cookie = app.sign_in().await;
    let r = app
        .ask(Method::DELETE, "/api/instances/lan-2", Some(&cookie))
        .await;
    assert_eq!(r.status, StatusCode::NO_CONTENT, "{}", r.body);
    assert_eq!(r.body, "", "204 carries no body");
    assert_eq!(app.calls(), ["remove lan-2"]);
    assert_eq!(
        principals_of(&app.principals),
        [Some("alice".to_string())],
        "as the signed-in user"
    );
}

/// A visitor: both writes are 401 `session-required` with no cookie set, and
/// the manager is not connected to at all: the gate answers on its own, so a
/// visitor creates and removes nothing.
#[tokio::test]
async fn the_instance_crud_without_a_session_is_refused_and_asks_nothing() {
    let app = app_crud(Ok(()), Ok(()));
    create(&app, None, CREATE_BODY)
        .await
        .assert_session_required();
    app.assert_asked_nothing("POST /api/instances with no session");

    let app = app_crud(Ok(()), Ok(()));
    app.ask(Method::DELETE, "/api/instances/lan-2", None)
        .await
        .assert_session_required();
    app.assert_asked_nothing("DELETE /api/instances/lan-2 with no session");
}

/// Every answer a create's refusal takes, as the body the page reads: the
/// manager's key, its args and the field to mark, its message staying out; a
/// manager that took the create and was gone when it answered 504
/// `client-timeout`, the write's outcome being unknown (slice W2,
/// `tests/write_timeouts.rs`); anything else 502 `client-other` with no args.
#[tokio::test]
async fn the_instance_create_answers_the_refusals() {
    let refusal = |key: &'static str, value: &str| {
        ClientError::Invalid(Refusal {
            key,
            values: vec![value.to_string()],
        })
    };
    for (failure, status, key, args, field) in [
        (
            refusal("unknown-engine", "vllm"),
            StatusCode::UNPROCESSABLE_ENTITY,
            "unknown-engine",
            &["vllm"][..],
            Some("engine"),
        ),
        (
            refusal("instance-bad-id", "A"),
            StatusCode::UNPROCESSABLE_ENTITY,
            "instance-bad-id",
            &["A"][..],
            Some("id"),
        ),
        (
            refusal("instance-exists", "lan-2"),
            StatusCode::CONFLICT,
            "instance-exists",
            &["lan-2"][..],
            None,
        ),
        (
            ClientError::NoServerProfile("free".into()),
            StatusCode::NOT_FOUND,
            "client-no-server-profile",
            &["free"][..],
            None,
        ),
        (
            ClientError::ServerProfileInUse("free".into(), "default".into()),
            StatusCode::CONFLICT,
            "client-server-profile-in-use",
            &["free", "default"][..],
            None,
        ),
        (
            ClientError::AddressInUse("lan-2".into()),
            StatusCode::CONFLICT,
            "address-in-use",
            &["lan-2"][..],
            None,
        ),
        (
            ClientError::UnresolvableHost("ns.example".into()),
            StatusCode::UNPROCESSABLE_ENTITY,
            "invalid-hostname",
            &["ns.example"][..],
            None,
        ),
        (
            ClientError::Unavailable,
            StatusCode::GATEWAY_TIMEOUT,
            "client-timeout",
            &[][..],
            None,
        ),
    ] {
        let app = app_crud(Err(failure), Ok(()));
        let cookie = app.sign_in().await;
        let r = create(&app, Some(&cookie), CREATE_BODY).await;
        assert_eq!(
            r.json(),
            args_doc(key, args, field),
            "{status} {key}: {}",
            r.body
        );
        assert_eq!(r.status, status, "{key}: {}", r.body);
    }
}

/// The manager's detail is not the page's: any other failure is 502
/// `client-other` with no args and no word of the manager's message.
#[tokio::test]
async fn a_create_failure_that_is_not_a_refusal_keeps_the_detail_out() {
    let app = app_crud(
        Err(ClientError::Other(
            "org.freedesktop.DBus.Error.AccessDenied: secret-detail".into(),
        )),
        Ok(()),
    );
    let cookie = app.sign_in().await;
    let r = create(&app, Some(&cookie), CREATE_BODY).await;
    r.assert_error(StatusCode::BAD_GATEWAY, "client-other");
    assert!(!r.body.contains("secret-detail"), "{}", r.body);
}

/// Every answer a remove's refusal takes: the protected id and a running one
/// 409 with the id, no such instance and an id the rule refuses 404
/// `client-no-instance`, a manager that took the remove and was gone when it
/// answered 504 `client-timeout` (slice W2, `tests/write_timeouts.rs`), the
/// service manager's failure 502 `client-service-failed` with its message kept
/// out.
#[tokio::test]
async fn the_instance_remove_answers_the_refusals() {
    let refusal = |key: &'static str, value: &str| {
        ClientError::Invalid(Refusal {
            key,
            values: vec![value.to_string()],
        })
    };
    for (failure, status, key, args) in [
        (
            refusal("instance-protected", "default"),
            StatusCode::CONFLICT,
            "instance-protected",
            &["default"][..],
        ),
        (
            refusal("instance-running", "lan-2"),
            StatusCode::CONFLICT,
            "instance-running",
            &["lan-2"][..],
        ),
        (
            ClientError::NoInstance,
            StatusCode::NOT_FOUND,
            "client-no-instance",
            &[][..],
        ),
        (
            ClientError::InvalidId,
            StatusCode::NOT_FOUND,
            "client-no-instance",
            &[][..],
        ),
        (
            ClientError::Unavailable,
            StatusCode::GATEWAY_TIMEOUT,
            "client-timeout",
            &[][..],
        ),
    ] {
        let app = app_crud(Ok(()), Err(failure));
        let cookie = app.sign_in().await;
        let r = app
            .ask(Method::DELETE, "/api/instances/lan-2", Some(&cookie))
            .await;
        assert_eq!(r.status, status, "{key}: {}", r.body);
        assert_eq!(r.json(), json!({ "key": key, "args": args }), "{}", r.body);
    }
}

/// A remove the service manager refuses is the service's failure, not the
/// manager's: 502 `client-service-failed`, the message staying in the bff. The
/// record stays, so the page's next list still shows the instance.
#[tokio::test]
async fn a_remove_the_service_manager_refuses_is_a_failed_service() {
    let app = app_crud(Ok(()), Err(ClientError::Service("secret-detail".into())));
    let cookie = app.sign_in().await;
    let r = app
        .ask(Method::DELETE, "/api/instances/lan-2", Some(&cookie))
        .await;
    r.assert_error(StatusCode::BAD_GATEWAY, "client-service-failed");
    assert!(!r.body.contains("secret-detail"), "{}", r.body);
}

/// A caller the manager will not let write: 403 `session-not-administrator`,
/// and the session ends, as on every other write of the signed-in user.
#[tokio::test]
async fn a_refusal_of_the_signed_in_user_ends_the_session_on_the_instance_crud() {
    for (method, uri, body) in [
        (Method::POST, "/api/instances", Some(CREATE_BODY)),
        (Method::DELETE, "/api/instances/lan-2", None),
    ] {
        let refused = ClientError::NotAuthorized;
        let app = if body.is_some() {
            app_crud(Err(refused), Ok(()))
        } else {
            app_crud(Ok(()), Err(refused))
        };
        let cookie = app.sign_in().await;
        let r = match body {
            Some(body) => create(&app, Some(&cookie), body).await,
            None => app.ask(method.clone(), uri, Some(&cookie)).await,
        };
        r.assert_error(StatusCode::FORBIDDEN, "session-not-administrator");
        app.get("/api/instances", Some(&cookie))
            .await
            .assert_session_required();
    }
}

/// A body the bff cannot read is its own 400 `instance-malformed`, before the
/// manager is connected to: the create reads three words (the engine, the id,
/// the instance profile, IP4d), and a body naming none of them, or a word of
/// the wrong type (`null`, a list, a number), is one it cannot read. The last
/// body is the retired key: a body naming `serverProfile` at all is refused,
/// never read as the instance profile that supersedes it.
#[tokio::test]
async fn a_create_body_the_bff_cannot_read_is_refused_before_the_manager() {
    for body in [
        "not json",
        "[]",
        "{}",
        r#"{"id": "lan-2", "instanceProfile": "free"}"#,
        r#"{"engine": "llama.cpp", "instanceProfile": "free"}"#,
        r#"{"engine": "llama.cpp", "id": "lan-2"}"#,
        r#"{"engine": "llama.cpp", "id": null, "instanceProfile": "free"}"#,
        r#"{"engine": "llama.cpp", "id": 5, "instanceProfile": "free"}"#,
        r#"{"engine": 5, "id": "lan-2", "instanceProfile": "free"}"#,
        r#"{"engine": "llama.cpp", "id": "lan-2", "instanceProfile": null}"#,
        r#"{"engine": "llama.cpp", "id": "lan-2", "instanceProfile": 5}"#,
        r#"{"engine": "llama.cpp", "id": "lan-2", "instanceProfile": ["free"]}"#,
        r#"{"engine": "llama.cpp", "id": "lan-2", "instanceProfile": "free", "serverProfile": "free"}"#,
    ] {
        let app = app_crud(Ok(()), Ok(()));
        let cookie = app.sign_in().await;
        let r = create(&app, Some(&cookie), body).await;
        r.assert_error(StatusCode::BAD_REQUEST, "instance-malformed");
        app.assert_asked_nothing(&format!("create on {body}"));
    }
}

/// The list says which row the page may offer a remove of: `removable` is true
/// of every id and false of the protected `default` alone, whatever its state.
/// Additive: the keys of every row before it are unchanged.
#[tokio::test]
async fn the_instance_list_carries_removable() {
    let app = app_listing(&[
        ("default", InstanceState::Active),
        ("lan-2", InstanceState::Inactive),
    ]);
    let cookie = app.sign_in().await;
    let r = app.get("/api/instances", Some(&cookie)).await;
    assert_eq!(
        r.json(),
        json!([
            { "id": "default", "label": null, "engine": "llama.cpp", "serverProfile": null, "state": "active", "port": 0, "tags": [], "instanceTags": [], "boot": false, "removable": false, "instanceProfile": null, "hostname": null, "converged": true, "reason": null, "pendingRestart": false },
            { "id": "lan-2", "label": null, "engine": "llama.cpp", "serverProfile": null, "state": "inactive", "port": 0, "tags": [], "instanceTags": [], "boot": false, "removable": true, "instanceProfile": null, "hostname": null, "converged": true, "reason": null, "pendingRestart": false }
        ]),
        "{}",
        r.body
    );
}

/// The two writes are actions, not reads: each moves the session's idle
/// deadline, which starts again from the call, to the second: at the call's own
/// timeout the session is alive, one second later it is gone (a read would
/// expire at the sign-in's deadline, well before).
#[tokio::test]
async fn the_instance_crud_moves_the_idle_deadline() {
    for (method, uri, body) in [
        (Method::POST, "/api/instances", Some(CREATE_BODY)),
        (Method::DELETE, "/api/instances/lan-2", None),
    ] {
        let app = app_crud(Ok(()), Ok(()));
        let cookie = app.sign_in().await;
        app.clock.advance(SHORT);
        let r = match body {
            Some(body) => app.ask_json(method.clone(), uri, Some(&cookie), body).await,
            None => app.ask(method.clone(), uri, Some(&cookie)).await,
        };
        assert!(r.status.is_success(), "{method} {uri}: {}", r.body);
        app.clock.advance(IDLE);
        app.get("/api/session", Some(&cookie))
            .await
            .assert_signed_in_as("alice");

        // One second past the deadline from the call: the write moved it, and
        // only that.
        let app = app_crud(Ok(()), Ok(()));
        let cookie = app.sign_in().await;
        app.clock.advance(SHORT);
        match body {
            Some(body) => app.ask_json(method.clone(), uri, Some(&cookie), body).await,
            None => app.ask(method.clone(), uri, Some(&cookie)).await,
        };
        app.clock.advance(IDLE + Duration::from_secs(1));
        app.get("/api/session", Some(&cookie))
            .await
            .assert_error(StatusCode::UNAUTHORIZED, SESSION_REQUIRED);
    }
}

/// The two routes answer their own verb and nothing else: `PUT` the list, and
/// the verbs the page uses on an id, are the router's 405, answered before the
/// session is read, asking the manager nothing.
#[tokio::test]
async fn the_instance_crud_routes_answer_only_their_method() {
    for (method, uri) in [
        (Method::PUT, "/api/instances"),
        (Method::DELETE, "/api/version"),
        (Method::POST, "/api/instances/lan-2"),
        (Method::PUT, "/api/instances/lan-2"),
    ] {
        let app = app_crud(Ok(()), Ok(()));
        let cookie = app.sign_in().await;
        let r = app.ask(method.clone(), uri, Some(&cookie)).await;
        assert_eq!(
            r.status,
            StatusCode::METHOD_NOT_ALLOWED,
            "{method} {uri}: {}",
            r.body
        );
        app.assert_asked_nothing(&format!("{method} {uri}"));
    }
}

/// A 405 answer carries the framing headers like every other reply.
#[tokio::test]
async fn a_405_on_the_instance_crud_carries_the_framing() {
    let app = app_crud(Ok(()), Ok(()));
    let r = app.ask(Method::PUT, "/api/instances", None).await;
    assert_eq!(r.status, StatusCode::METHOD_NOT_ALLOWED, "{}", r.body);
    assert_eq!(r.values_of("x-frame-options"), ["DENY"]);
    assert_eq!(r.values_of("x-content-type-options"), ["nosniff"]);
}
