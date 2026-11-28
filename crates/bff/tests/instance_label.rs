//! IL2: `PUT /api/instances/{id}/label` `{"label": <string>}` and the
//! `label` of the rows of `GET /api/instances`, through
//! `pai_bff::router_with_session` with a fake manager client and a fake
//! authenticator, never the real bus, pipe, PAM or LogonUser. Each test hands
//! the router one request (`tower::ServiceExt::oneshot`) under a deadline, so
//! a bff that waits on the manager fails instead of hanging.
//!
//! The write (ARCHITECTURE.md, slice IL2) answers 204 with no body once the
//! manager wrote the text. It acts for the signed-in user, as the tags write
//! does (`tests/instances.rs`), and the manager is the one that judges the
//! text; the text travels trimmed of the spaces and tabs of its ends, as the
//! design's Done-when says, and the fake's log proves what arrived. The
//! answers:
//! - 422 `{"key", "args", "field": "label"}` for a text the manager's rule
//!   refuses (`instance-bad-label`, `comment-markers`), the session living on.
//! - 409 `{"key": "instance-label-taken", "args": [<label>, <other id>]}`, the
//!   label another instance of the node already carries, no `field`.
//! - 404 `client-no-instance` for an id the record lacks, and for an id the
//!   manager refuses.
//! - 400 `instance-malformed` for a body that is not `{"label": <string>}`,
//!   the manager not asked; 413 `payload-too-large` for a body over
//!   `origin::MAX_BODY`, which the handler answers as its second step, after
//!   the session check.
//! - 403 `session-not-administrator` with the session ended, when the manager
//!   refuses the signed-in user; 401 `session-required` with no `Set-Cookie`,
//!   asking nothing, with no session.
//! - 503 `client-unavailable` while the call never reached the manager, 504
//!   `client-timeout` once it did: both pinned in `tests/write_timeouts.rs`.
//!
//! `GET /api/instances` rows gain `label`: the text of the manager's row, or
//! `null` when the row carries none, as it is `null` for an instance the rows
//! do not mention. The rows' other keys are `tests/instances.rs`.

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
use std::time::Duration;
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

/// The key of the gate's answer (`session.rs`).
const SESSION_REQUIRED: &str = "session-required";

/// The body cap of `origin::MAX_BODY`, copied here (it is private to the bff).
const MAX_BODY: usize = 64 * 1024;

/// The port the fake's rows carry.
const PORT: u16 = 8080;

/// The separator the fake's log line puts between the id and the text it
/// received, so an assertion tells a leading and a trailing space from none:
/// the same unit separator the manager's refusals use on the wire.
const SEP: char = '\u{1f}';

type Log = Arc<Mutex<Vec<String>>>;

/// A manager that answers from its fields and logs every call
/// (`list`, `instances`, `status <id>`, `set_instance_label <id><sep><text>`).
struct FakeManager {
    instances: Result<Vec<String>, ClientError>,
    states: HashMap<String, Result<InstanceState, ClientError>>,
    /// What `instances` answers: the rows, each with its label.
    rows: Result<Vec<InstanceInfo>, ClientError>,
    /// What `set_instance_label` answers.
    label: Result<(), ClientError>,
    log: Log,
}

impl FakeManager {
    fn new(log: &Log, ids: &[String]) -> FakeManager {
        FakeManager {
            instances: Ok(ids.to_vec()),
            states: ids
                .iter()
                .map(|id| (id.clone(), Ok(InstanceState::Inactive)))
                .collect(),
            rows: Err(ClientError::Other("no Instances verb".into())),
            label: Ok(()),
            log: log.clone(),
        }
    }

    /// A row of `rows`: the id, the engine, its own server profile, the port,
    /// no tags and the label `label`.
    fn row(id: &str, label: Option<&str>) -> InstanceInfo {
        InstanceInfo {
            id: id.to_string(),
            engine: "llama.cpp".to_string(),
            server_profile: Some(id.to_string()),
            port: PORT,
            profile_tags: Vec::new(),
            instance_tags: Vec::new(),
            label: label.map(str::to_string),
            converged: true,
            reason: None,
            pending_restart: false,
        }
    }

    fn record(&self, call: String) {
        self.log
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .push(call);
    }

    /// The line the log carries for a label write: the verb, the id, the text.
    fn label_call(&self, id: &str, label: &str) -> String {
        format!("set_instance_label {id}{SEP}{label}")
    }
}

impl ManagerClient for FakeManager {
    fn list_instances(&self) -> Result<Vec<String>, ClientError> {
        self.record("list".into());
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
        Ok(false)
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
    fn set_instance_label(&self, id: &str, label: &str) -> Result<(), ClientError> {
        self.record(self.label_call(id, label));
        self.label.clone()
    }
}

/// The principal every connect was asked for, in order: `None` is a call the
/// bff makes as its own account.
type Principals = Arc<Mutex<Vec<Option<String>>>>;

fn principals_of(p: &Principals) -> Vec<Option<String>> {
    p.lock().unwrap_or_else(|e| e.into_inner()).clone()
}

struct Reply {
    status: StatusCode,
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

    /// An error answer: its status and `{"key": key, "args": []}` as JSON.
    fn assert_error(&self, status: StatusCode, key: &str) {
        assert_eq!(self.status, status, "{}", self.body);
        assert_eq!(
            self.json(),
            json!({ "key": key, "args": [] }),
            "{}",
            self.body
        );
    }

    /// The gate's answer: 401 `session-required`, and no `Set-Cookie`.
    fn assert_session_required(&self) {
        self.assert_error(StatusCode::UNAUTHORIZED, SESSION_REQUIRED);
        assert!(
            self.values_of("set-cookie").is_empty(),
            "a refused write sets a cookie: {:?}",
            self.headers
        );
    }

    /// 204 with no body: the write landed.
    fn assert_no_content(&self) {
        assert_eq!(self.status, StatusCode::NO_CONTENT, "{}", self.body);
        assert_eq!(self.body, "", "204 carries no body");
    }

    /// 422 `{"key", "args", "field": "label"}`: the manager's check of the
    /// text, its message out, the label's control named.
    fn assert_422_label(&self, key: &str, args: &[&str]) {
        assert_eq!(
            self.status,
            StatusCode::UNPROCESSABLE_ENTITY,
            "{}",
            self.body
        );
        assert_eq!(
            self.json(),
            json!({ "key": key, "args": args, "field": "label" }),
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

/// A router over a fake manager, a fake authenticator and the real clock, with
/// the log of the manager's calls, the count of connects and the principals
/// each was handed.
struct App {
    router: Router,
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
/// fails is testable.
fn app_connecting(
    make: impl Fn(&Log) -> Result<Client, ClientError> + Send + Sync + 'static,
) -> App {
    let log = Log::default();
    let connects = Arc::new(AtomicUsize::new(0));
    let principals = Principals::default();
    let l = log.clone();
    let connect: Connect = {
        let (connects, principals) = (connects.clone(), principals.clone());
        Arc::new(move |principal: Option<&str>| {
            connects.fetch_add(1, Ordering::SeqCst);
            principals
                .lock()
                .unwrap_or_else(|e| e.into_inner())
                .push(principal.map(str::to_string));
            make(&l)
        })
    };
    let router = pai_bff::router_with_session(
        connect,
        TIMEOUT,
        Arc::new(FakeAuth),
        Arc::new(std::time::Instant::now),
        REVISION_KEY,
    );
    App {
        router,
        log,
        connects,
        principals,
    }
}

/// The app whose manager lists `ids` stopped, answers `rows` from `Instances`
/// and answers `set_instance_label` with `label`.
fn app_label(
    ids: &[&str],
    rows: Result<Vec<InstanceInfo>, ClientError>,
    label: Result<(), ClientError>,
) -> App {
    let owned = ids.iter().map(|id| id.to_string()).collect::<Vec<_>>();
    app_at(move |l| FakeManager {
        rows: rows.clone(),
        label: label.clone(),
        ..FakeManager::new(l, &owned)
    })
}

/// The app whose manager lists the two instances and answers no rows: enough
/// for a write, whose answer never reads the list.
fn app_writing(label: Result<(), ClientError>) -> App {
    app_label(
        &["default", "coder"],
        Err(ClientError::Other("no Instances verb".into())),
        label,
    )
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
        let mut request = Request::builder()
            .method(Method::POST)
            .uri("/api/session")
            .header(header::CONTENT_TYPE, "application/json")
            .body(Body::from(
                json!({ "username": "alice", "password": "correct horse" }).to_string(),
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

    /// `DELETE /api/session`: the session ends.
    async fn sign_out(&self, cookie: &str) {
        let deleted = self.ask(Method::DELETE, "/api/session", Some(cookie)).await;
        assert_eq!(deleted.status, StatusCode::NO_CONTENT, "{}", deleted.body);
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

    /// A request with a body sent as `application/json`.
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
            headers,
            body: String::from_utf8(bytes.to_vec()).expect("utf-8 body"),
        }
    }

    /// Sign in and `PUT /api/instances/{id}/label` with `body`.
    async fn put_label(&self, id: &str, body: &str) -> (Reply, String) {
        let cookie = self.sign_in().await;
        let r = self
            .ask_json(
                Method::PUT,
                &format!("/api/instances/{id}/label"),
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
}

/// The label the tests write, and the body that carries it.
const LABEL: &str = "Coding box";
const LABEL_BODY: &str = r#"{"label": "Coding box"}"#;

// ── Failure paths first: the session, the body, the refusals ─────────────

/// With no session, and with the cookie of a session that ended, the write is
/// 401 `session-required` with no `Set-Cookie`, and the manager is never
/// connected to: the gate answers on its own, so a visitor labels nothing.
#[tokio::test]
async fn the_label_write_needs_a_session() {
    let app = app_writing(Ok(()));
    app.ask_json(
        Method::PUT,
        "/api/instances/default/label",
        None,
        LABEL_BODY,
    )
    .await
    .assert_session_required();
    app.assert_asked_nothing("PUT /api/instances/default/label with no session");

    let app = app_writing(Ok(()));
    let cookie = app.sign_in().await;
    app.sign_out(&cookie).await;
    app.ask_json(
        Method::PUT,
        "/api/instances/default/label",
        Some(&cookie),
        LABEL_BODY,
    )
    .await
    .assert_session_required();
    app.assert_asked_nothing("the write of a cookie of an ended session");
}

/// A body the bff cannot read is 400 `instance-malformed` and the manager is
/// not asked: no `label`, a value that is not a string, a document that is not
/// an object, and no JSON at all. The session check runs first, so with no
/// session such a body is 401.
#[tokio::test]
async fn a_body_that_is_not_one_string_is_400_and_asks_nothing() {
    for body in [
        r#"{}"#,
        r#"{"label": null}"#,
        r#"{"label": 5}"#,
        r#"{"label": true}"#,
        r#"{"label": ["Coding box"]}"#,
        r#"{"label": {}}"#,
        r#"{"labelled": "Coding box"}"#,
        r#"["Coding box"]"#,
        r#""Coding box""#,
        "label=Coding box",
        "",
    ] {
        let app = app_writing(Ok(()));
        let (r, _) = app.put_label("default", body).await;
        r.assert_error(StatusCode::BAD_REQUEST, "instance-malformed");
        app.assert_asked_nothing(&format!("the label body {body:?}"));
    }
    let app = app_writing(Ok(()));
    app.ask_json(Method::PUT, "/api/instances/default/label", None, r#"{}"#)
        .await
        .assert_session_required();
    app.assert_asked_nothing("a malformed body with no session");
}

/// A body over `origin::MAX_BODY` is the extractor's 413 `payload-too-large`,
/// answered before the manager is connected to. The session check comes first:
/// the same body from a visitor is 401, not 413.
#[tokio::test]
async fn a_body_over_the_cap_is_413_after_the_session_check() {
    let filler = "x".repeat(MAX_BODY + 1 - r#"{"label": ""}"#.len());
    let body = format!(r#"{{"label": "{filler}"}}"#);
    assert_eq!(body.len(), MAX_BODY + 1, "the body is one over the cap");

    let app = app_writing(Ok(()));
    let (r, _) = app.put_label("default", &body).await;
    r.assert_error(StatusCode::PAYLOAD_TOO_LARGE, "payload-too-large");
    app.assert_asked_nothing("the label body over the cap");

    let anon = app_writing(Ok(()));
    anon.ask_json(Method::PUT, "/api/instances/default/label", None, &body)
        .await
        .assert_session_required();
    anon.assert_asked_nothing("the body over the cap with no session");
}

/// The manager's check refuses the text: 422 with the check's key, its values
/// and `field` `label`, the control the form marks. The manager's own message
/// stays out of the body, and the session of the writer lives on: a refusal of
/// a value is no refusal of the user.
#[tokio::test]
async fn a_text_the_rule_refuses_is_422_with_the_label_field() {
    for (key, value) in [
        ("instance-bad-label", "x".repeat(65)),
        ("instance-bad-label", "two\nlines".to_string()),
        ("comment-markers", "a;b".to_string()),
        ("comment-markers", "a#b".to_string()),
    ] {
        let app = app_writing(Err(ClientError::Invalid(Refusal {
            key,
            values: vec![value.clone()],
        })));
        let (r, cookie) = app
            .put_label("default", &json!({ "label": value }).to_string())
            .await;
        r.assert_422_label(key, &[value.as_str()]);
        assert!(
            !r.body.contains("org.freedesktop"),
            "the manager's detail stays out: {}",
            r.body
        );
        app.get("/api/session", Some(&cookie))
            .await
            .assert_signed_in_as("alice");
    }
}

/// A label another instance of the node already carries is 409
/// `instance-label-taken` with the label and the other instance's id, no
/// `field`: the page has no control to mark, the clash is between two rows.
/// The session lives on.
#[tokio::test]
async fn a_label_another_instance_holds_is_409_with_its_args() {
    let app = app_writing(Err(ClientError::Invalid(Refusal {
        key: "instance-label-taken",
        values: vec![LABEL.to_string(), "coder".to_string()],
    })));
    let (r, cookie) = app.put_label("default", LABEL_BODY).await;
    assert_eq!(r.status, StatusCode::CONFLICT, "{}", r.body);
    assert_eq!(
        r.json(),
        json!({ "key": "instance-label-taken", "args": [LABEL, "coder"] }),
        "{}",
        r.body
    );
    app.get("/api/session", Some(&cookie))
        .await
        .assert_signed_in_as("alice");
}

/// No such instance, and an id the manager refuses: 404 `client-no-instance`,
/// the session of the writer living on.
#[tokio::test]
async fn the_label_of_no_instance_is_404() {
    for failure in [ClientError::NoInstance, ClientError::InvalidId] {
        let app = app_writing(Err(failure.clone()));
        let (r, cookie) = app.put_label("lan-9", LABEL_BODY).await;
        r.assert_error(StatusCode::NOT_FOUND, "client-no-instance");
        app.get("/api/session", Some(&cookie))
            .await
            .assert_signed_in_as("alice");
    }
}

/// The manager refuses the signed-in user (they left the `pai` group since
/// they signed in): 403 `session-not-administrator` and the session ends, as
/// on the tags write.
#[tokio::test]
async fn a_refusal_of_the_signed_in_user_ends_the_session() {
    let app = app_writing(Err(ClientError::NotAuthorized));
    let (r, cookie) = app.put_label("default", LABEL_BODY).await;
    r.assert_error(StatusCode::FORBIDDEN, "session-not-administrator");
    app.get("/api/instances", Some(&cookie))
        .await
        .assert_session_required();
}

/// No manager to connect to (not running, no bus, no pipe): 503
/// `client-unavailable`, the session living on. The two halves of the time
/// limit, 503 before the hand-over and 504 after it, are
/// `tests/write_timeouts.rs`.
#[tokio::test]
async fn the_label_write_of_no_manager_is_unavailable() {
    let app = app_connecting(|_| Err(ClientError::Unavailable));
    let cookie = app.sign_in().await;
    let r = app
        .ask_json(
            Method::PUT,
            "/api/instances/default/label",
            Some(&cookie),
            LABEL_BODY,
        )
        .await;
    r.assert_error(StatusCode::SERVICE_UNAVAILABLE, "client-unavailable");
    app.get("/api/session", Some(&cookie))
        .await
        .assert_signed_in_as("alice");
}

/// Any other failure is 502 `client-other` with no args, the manager's detail
/// staying in the bff.
#[tokio::test]
async fn any_other_failure_is_failed_without_the_detail() {
    let app = app_writing(Err(ClientError::Other(
        "org.freedesktop.DBus.Error.AccessDenied: secret-detail".into(),
    )));
    let (r, _) = app.put_label("default", LABEL_BODY).await;
    r.assert_error(StatusCode::BAD_GATEWAY, "client-other");
    assert!(!r.body.contains("secret-detail"), "{}", r.body);
}

/// The route is the write: `GET`, `POST` and `DELETE` on it are the router's
/// 405, which answers before the session is read and asks the manager nothing.
#[tokio::test]
async fn the_label_route_answers_only_the_write() {
    for method in [Method::GET, Method::POST, Method::DELETE] {
        let app = app_writing(Ok(()));
        let r = app
            .ask_json(
                method.clone(),
                "/api/instances/default/label",
                None,
                LABEL_BODY,
            )
            .await;
        assert_eq!(r.status, StatusCode::METHOD_NOT_ALLOWED, "{method}");
        app.assert_asked_nothing(&format!("{method} /api/instances/default/label"));
    }
}

// ── What it answers ─────────────────────────────────────────────────────

/// The write asks the manager for the instance with the text the body carried,
/// as the signed-in user, once, and answers 204 with no body.
#[tokio::test]
async fn the_label_write_sends_the_text_and_answers_no_content() {
    let app = app_writing(Ok(()));
    let (r, _) = app.put_label("default", LABEL_BODY).await;
    r.assert_no_content();
    assert_eq!(
        app.calls(),
        [app_log_call("default", LABEL)],
        "the text of the body, for that instance"
    );
    assert_eq!(
        principals_of(&app.principals),
        vec![Some("alice".to_string())],
        "the write acts for the signed-in user, as the tags write does"
    );
}

/// The text reaches the manager trimmed of the spaces and tabs of its ends:
/// the design's Done-when says the manager received the trimmed label. The
/// manager's rule trims as well (`pai-core` `checks::instance_label`, and
/// `set_instance_label` before it writes `Label`), so a space the page leaves
/// at the edges changes nothing on the way. A label of nothing but spaces is
/// the empty text, which the manager reads as "no label".
#[tokio::test]
async fn the_label_write_sends_the_trimmed_text() {
    for (body, text) in [
        (r#"{"label": "Coding box"}"#, "Coding box"),
        (r#"{"label": "  Coding box\t"}"#, "Coding box"),
        (r#"{"label": "\tCoding "}"#, "Coding"),
        (r#"{"label": "   "}"#, ""),
    ] {
        let app = app_writing(Ok(()));
        let (r, _) = app.put_label("default", body).await;
        r.assert_no_content();
        assert_eq!(
            app.calls(),
            [app_log_call("default", text)],
            "the trimmed text of {body}"
        );
    }
}

/// An empty label is a write, not a malformed body: the text the manager takes
/// as "no label" (it removes the `Label` line), so it travels as the empty
/// string and answers 204.
#[tokio::test]
async fn an_empty_label_clears_it() {
    let app = app_writing(Ok(()));
    let (r, _) = app.put_label("default", r#"{"label": ""}"#).await;
    r.assert_no_content();
    assert_eq!(app.calls(), [app_log_call("default", "")]);
}

/// The line the fake's log carries for a write of `id` with the text `label`.
fn app_log_call(id: &str, label: &str) -> String {
    format!("set_instance_label {id}{SEP}{label}")
}

// ── The list gains the label ────────────────────────────────────────────

/// A row of `Instances` with a label puts it on the item as a string; a row
/// that carries none, and an instance the rows do not mention at all, are
/// `null`: the page falls back to the engine and the port. The write is what
/// sets a label, so the same text lands here.
#[tokio::test]
async fn the_list_carries_the_label_of_the_row() {
    let app = app_label(
        &["default", "coder", "gone"],
        Ok(vec![
            FakeManager::row("default", Some(LABEL)),
            FakeManager::row("coder", None),
        ]),
        Ok(()),
    );
    let r = app.instances().await;
    assert_eq!(r.status, StatusCode::OK, "{}", r.body);
    let rows = r.json();
    let of = |id: &str| {
        rows.as_array()
            .unwrap_or_else(|| panic!("not a list: {}", r.body))
            .iter()
            .find(|row| row.get("id") == Some(&json!(id)))
            .unwrap_or_else(|| panic!("no row for {id}: {}", r.body))
            .get("label")
            .cloned()
    };
    assert_eq!(of("default"), Some(json!(LABEL)), "the row's text");
    assert_eq!(of("coder"), Some(json!(null)), "a row with no label");
    assert_eq!(of("gone"), Some(json!(null)), "an instance with no row");
}

/// The key is present on every row, so the page never reads a missing field:
/// a manager with no `Instances` verb at all (an older protocol) answers the
/// list with `label: null` on each item, not with the read failing.
#[tokio::test]
async fn every_row_carries_the_key_even_with_no_rows() {
    let app = app_label(
        &["default", "coder"],
        Err(ClientError::Other("no Instances verb".into())),
        Ok(()),
    );
    let r = app.instances().await;
    assert_eq!(r.status, StatusCode::OK, "{}", r.body);
    let rows = r.json();
    let list = rows.as_array().expect("a list");
    assert_eq!(list.len(), 2, "both instances: {}", r.body);
    for row in list {
        assert_eq!(
            row.get("label"),
            Some(&json!(null)),
            "present, not missing: {}",
            r.body
        );
    }
}
