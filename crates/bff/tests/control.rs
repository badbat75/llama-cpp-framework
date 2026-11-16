//! `POST /api/instances/{id}/start`, `/stop` and `/restart`, and
//! `GET`/`PUT /api/instances/{id}/boot`, through `pai_bff::router_with_session`
//! with a fake manager client and a fake authenticator (never the real bus,
//! pipe, systemd or SCM). Each request runs under a deadline, so a bff that
//! waits on the manager fails instead of hanging.
//!
//! The contract (fe/briefs/api-contract.md, "Start, stop, restart, boot"):
//! - The three control routes take no body and answer 204 once the service
//!   manager has accepted the job: an empty body, nothing else. The web UI
//!   polls `GET /api/instances` for the state, so the bff sends no state back.
//! - `GET /api/instances/{id}/boot` answers 200 `{"enabled"}` and needs no
//!   sign-in: it connects as the bff's own account (no principal), and the
//!   manager's refusal of the bff is 502 `client-not-authorized`, as on
//!   `GET /api/instances`.
//! - `PUT /api/instances/{id}/boot` takes `{"enabled": <bool>}` and answers
//!   200 `{"enabled"}` read back from the manager after the write: the reply
//!   is what the read answered, never what the body asked.
//! - The control routes and `PUT .../boot` sit behind a session, so they act
//!   for the signed-in user: `Connect` is asked with the session's user as the
//!   principal, taken from the session alone, and the manager re-checks that
//!   user's right at every call. A refusal is 403 `client-not-authorized` and
//!   ends that session, as the server read does
//!   (`crates/bff/src/server.rs`).
//! - Errors are `{"key": <strings key>, "args": []}`, never the manager's
//!   detail: 401 `session-required` with no session (the manager is not even
//!   connected to), 403 `client-not-authorized`, 404 `client-no-instance` (no
//!   such instance, or the manager refuses the id), 503 `client-unavailable`
//!   (no manager, gone, or too slow), 502 `client-service-failed` with empty
//!   `args` when the service manager refused the job (the manager's
//!   `ClientError::Service`: its message stays in the bff's log), 502
//!   `client-other` for anything else.
//! - A `PUT` body that is not `{"enabled": <bool>}` is 400 and the manager is
//!   not asked. The session check runs first: a request with no session is 401
//!   whatever its body. The key is `boot-malformed`: the bff has no general
//!   bad-request key today (only the sign-in route's `session-malformed`) and
//!   the contract names none for this route, so the web UI shows it as its one
//!   generic failure, which is what it does for a malformed body.
//! - `GET /api/instances` and `GET .../boot` never move a session's idle
//!   deadline: the web UI polls them while an instance page is open, and an
//!   open tab must not keep a session alive. An action (a control verb, the
//!   boot write) does move it.

use axum::body::Body;
use axum::extract::ConnectInfo;
use axum::http::{header, Method, Request, StatusCode};
use axum::Router;
use http_body_util::BodyExt;
use pai_bff::session::{AuthError, Authenticator, Identity};
use pai_bff::{Client, Connect};
use pai_core::client::{ClientError, InstanceState, ManagerClient, ManagerFile};
use serde_json::{json, Value};
use std::net::SocketAddr;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};
use tower::ServiceExt;

/// How long the bff waits for the manager in these tests.
const TIMEOUT: Duration = Duration::from_millis(200);

/// How long a test waits for the bff's answer before failing.
const DEADLINE: Duration = Duration::from_secs(5);

const CLIENT: &str = "127.0.0.1:51000";

/// The session idle timeout chosen for the bff (`session.rs`'s own).
const IDLE: Duration = Duration::from_secs(30 * 60);

/// A step just under the idle timeout: the request that runs that far after
/// the sign-in leaves one second to the deadline the sign-in set, so the pin
/// below is to the second.
const SHORT: Duration = Duration::from_secs(30 * 60 - 60);

/// The key of a boot body the bff cannot read (see the header).
const MALFORMED: &str = "boot-malformed";

type Log = Arc<Mutex<Vec<String>>>;

/// The principal every connect was asked for, in order: `None` is a call the
/// bff makes as its own account.
type Principals = Arc<Mutex<Vec<Option<String>>>>;

/// What a verb answers.
type Unit = Result<(), ClientError>;

/// What the boot read answers.
type Flag = Result<bool, ClientError>;

fn principals_of(p: &Principals) -> Vec<Option<String>> {
    p.lock().unwrap_or_else(|e| e.into_inner()).clone()
}

// ── Fakes ───────────────────────────────────────────────────────────────

/// What the manager answers each call, and how long each takes.
#[derive(Clone)]
struct Answer {
    /// `start`, `stop`, `restart`.
    verb: Unit,
    /// `set_boot_enabled`.
    set_boot: Unit,
    /// `boot_enabled`.
    boot: Flag,
    delay: Duration,
}

impl Answer {
    /// A manager that accepts everything, at once.
    fn ok() -> Answer {
        Answer {
            verb: Ok(()),
            set_boot: Ok(()),
            boot: Ok(true),
            delay: Duration::ZERO,
        }
    }
}

/// A manager that answers from its `answer` and logs every call (`start
/// default`, `set_boot_enabled default true`, `boot_enabled default`, ...).
/// It is the client of one connect, so it knows the principal that connect was
/// handed (`as_user`) and refuses every call of the principals in `refuse`,
/// the way the real manager refuses a user who is no longer a member of `pai`.
///
/// The boot state is the `state` cell, shared by every connect of one app:
/// the write leaves it at `after_write` when that is set (a manager whose
/// write did not land the way it was asked: the unit is enabled by something
/// else, masked), else at what was asked; the read answers what the cell
/// holds. That is how the read-back of a `PUT` is told apart from an echo of
/// its body.
struct FakeManager {
    answer: Answer,
    refuse: Vec<String>,
    as_user: Option<String>,
    state: Arc<Mutex<bool>>,
    after_write: Option<bool>,
    log: Log,
}

impl FakeManager {
    fn record(&self, call: String) {
        self.log
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .push(call);
    }

    /// The manager's refusal of the principal this connect was handed, the
    /// kind every principal refusal carries.
    fn refused(&self) -> bool {
        self.refuse
            .iter()
            .any(|u| Some(u.as_str()) == self.as_user.as_deref())
    }

    /// Log, refuse, wait, answer: the shape of every verb here.
    fn act(&self, call: String, answer: &Unit) -> Result<(), ClientError> {
        self.record(call);
        if self.refused() {
            return Err(ClientError::NotAuthorized);
        }
        std::thread::sleep(self.answer.delay);
        answer.clone()
    }

    /// The boot state, read the same way.
    fn flag(&self, call: String) -> Result<bool, ClientError> {
        self.record(call);
        if self.refused() {
            return Err(ClientError::NotAuthorized);
        }
        std::thread::sleep(self.answer.delay);
        match self.answer.boot.as_ref() {
            Ok(_) => Ok(*self.state.lock().unwrap_or_else(|e| e.into_inner())),
            Err(e) => Err(e.clone()),
        }
    }
}

impl ManagerClient for FakeManager {
    fn list_instances(&self) -> Result<Vec<String>, ClientError> {
        self.record("list".into());
        Ok(vec!["default".into()])
    }
    fn status(&self, id: &str) -> Result<InstanceState, ClientError> {
        self.record(format!("status {id}"));
        Ok(InstanceState::Active)
    }
    fn start(&self, id: &str) -> Result<(), ClientError> {
        self.act(format!("start {id}"), &self.answer.verb)
    }
    fn stop(&self, id: &str) -> Result<(), ClientError> {
        self.act(format!("stop {id}"), &self.answer.verb)
    }
    fn restart(&self, id: &str) -> Result<(), ClientError> {
        self.act(format!("restart {id}"), &self.answer.verb)
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
        self.flag(format!("boot_enabled {id}"))
    }
    fn set_boot_enabled(&self, id: &str, enabled: bool) -> Result<(), ClientError> {
        self.act(
            format!("set_boot_enabled {id} {enabled}"),
            &self.answer.set_boot,
        )?;
        *self.state.lock().unwrap_or_else(|e| e.into_inner()) = self.after_write.unwrap_or(enabled);
        Ok(())
    }
    fn authenticate(&self, _user: &str, _password: &str) -> Result<(bool, bool), ClientError> {
        Err(ClientError::Unavailable)
    }
}

/// `alice` and `root`, two signed-in sessions (both members of `pai`, the
/// group sign-in admits), both with the password `correct horse`.
struct FakeAuth;

impl Authenticator for FakeAuth {
    fn authenticate(&self, user: &str, password: &str) -> Result<Identity, AuthError> {
        match (user, password) {
            ("alice", "correct horse") => Ok(Identity {
                user: "alice".into(),
            }),
            ("root", "correct horse") => Ok(Identity {
                user: "root".into(),
            }),
            _ => Err(AuthError::BadCredentials),
        }
    }
}

/// A clock that moves only when the test says so (the idle-deadline tests).
#[derive(Clone)]
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

struct App {
    router: Router,
    clock: FakeClock,
    log: Log,
    connects: Arc<AtomicUsize>,
    principals: Principals,
}

/// The app: a manager that answers `answer`, `connect` how, its boot state
/// `boot_initial`, a write leaving it at `after_write`, the principals in
/// `refuse` refused.
fn app_for(
    answer: Answer,
    connect: Result<(), ClientError>,
    boot_initial: bool,
    after_write: Option<bool>,
    refuse: Vec<String>,
) -> App {
    let log = Log::default();
    let connects = Arc::new(AtomicUsize::new(0));
    let principals = Principals::default();
    let state = Arc::new(Mutex::new(boot_initial));
    let (l, c, p) = (log.clone(), connects.clone(), principals.clone());
    let connect: Connect = Arc::new(move |principal: Option<&str>| {
        c.fetch_add(1, Ordering::SeqCst);
        p.lock()
            .unwrap_or_else(|e| e.into_inner())
            .push(principal.map(str::to_string));
        connect.clone()?;
        Ok(Box::new(FakeManager {
            answer: answer.clone(),
            refuse: refuse.clone(),
            as_user: principal.map(str::to_string),
            state: state.clone(),
            after_write,
            log: l.clone(),
        }) as Client)
    });
    let clock = FakeClock::new();
    let router = pai_bff::router_with_session(connect, TIMEOUT, Arc::new(FakeAuth), clock.clock());
    App {
        router,
        clock,
        log,
        connects,
        principals,
    }
}

/// A manager that accepts everything.
fn app() -> App {
    app_for(Answer::ok(), Ok(()), false, None, Vec::new())
}

/// `start`, `stop` and `restart` answer `verb`, the boot write answers OK.
fn app_verbs(verb: Unit) -> App {
    app_for(
        Answer {
            verb,
            ..Answer::ok()
        },
        Ok(()),
        false,
        None,
        Vec::new(),
    )
}

/// `start`, `stop`, `restart` and the boot write all answer `failure`: the
/// shape of a manager that refuses the call itself, whatever the route.
fn app_failing(failure: ClientError) -> App {
    let write = failure.clone();
    app_for(
        Answer {
            verb: Err(failure),
            set_boot: Err(write),
            ..Answer::ok()
        },
        Ok(()),
        false,
        None,
        Vec::new(),
    )
}

/// The boot write answers `set`; the read answers the state, as always.
fn app_boot_write(set: Unit) -> App {
    app_for(
        Answer {
            set_boot: set,
            ..Answer::ok()
        },
        Ok(()),
        false,
        None,
        Vec::new(),
    )
}

/// The boot read answers `boot`.
fn app_boot_read(boot: Flag) -> App {
    app_for(
        Answer {
            boot,
            ..Answer::ok()
        },
        Ok(()),
        false,
        None,
        Vec::new(),
    )
}

/// The boot read answers the state, which the write leaves at `after_write`.
fn app_boot_leaving(after_write: bool) -> App {
    app_for(Answer::ok(), Ok(()), false, Some(after_write), Vec::new())
}

/// A manager refusing every call of the principals in `refused`.
fn app_refusing(refused: &[&str]) -> App {
    app_for(
        Answer::ok(),
        Ok(()),
        false,
        None,
        refused.iter().map(|u| u.to_string()).collect(),
    )
}

/// No manager to connect to.
fn app_no_manager() -> App {
    app_for(
        Answer::ok(),
        Err(ClientError::Unavailable),
        false,
        None,
        Vec::new(),
    )
}

/// A manager that takes `delay` to answer.
fn app_slow(delay: Duration) -> App {
    app_for(
        Answer {
            delay,
            ..Answer::ok()
        },
        Ok(()),
        false,
        None,
        Vec::new(),
    )
}

impl App {
    fn calls(&self) -> Vec<String> {
        self.log.lock().unwrap_or_else(|e| e.into_inner()).clone()
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

    async fn get(&self, uri: &str, cookie: Option<&str>) -> Reply {
        self.ask(Method::GET, uri, cookie, None, None, &[]).await
    }

    /// A control call: no body at all.
    async fn post(&self, uri: &str, cookie: Option<&str>) -> Reply {
        self.ask(Method::POST, uri, cookie, None, None, &[]).await
    }

    /// A control call with an extra header, `name: value`.
    async fn post_header(&self, uri: &str, cookie: &str, name: &str, value: &str) -> Reply {
        self.ask(
            Method::POST,
            uri,
            Some(cookie),
            None,
            None,
            &[(name, value)],
        )
        .await
    }

    /// `PUT` with a JSON body.
    async fn put(&self, uri: &str, body: Value, cookie: Option<&str>) -> Reply {
        self.ask(
            Method::PUT,
            uri,
            cookie,
            Some(body.to_string()),
            Some("application/json"),
            &[],
        )
        .await
    }

    /// `PUT` with `body` as it stands, JSON content type.
    async fn put_raw(&self, uri: &str, body: &str, cookie: Option<&str>) -> Reply {
        self.ask(
            Method::PUT,
            uri,
            cookie,
            Some(body.to_string()),
            Some("application/json"),
            &[],
        )
        .await
    }

    /// `PUT` with `body` and a content type that is not JSON.
    async fn put_plain(&self, uri: &str, body: &str, cookie: Option<&str>) -> Reply {
        self.ask(
            Method::PUT,
            uri,
            cookie,
            Some(body.to_string()),
            Some("text/plain"),
            &[],
        )
        .await
    }

    async fn ask(
        &self,
        method: Method,
        uri: &str,
        cookie: Option<&str>,
        body: Option<String>,
        content_type: Option<&str>,
        extra: &[(&str, &str)],
    ) -> Reply {
        let mut builder = Request::builder().method(method).uri(uri);
        if let Some(c) = cookie {
            builder = builder.header(header::COOKIE, c);
        }
        if let Some(ct) = content_type {
            builder = builder.header(header::CONTENT_TYPE, ct);
        }
        for (name, value) in extra {
            let name = name.parse::<header::HeaderName>().expect("header name");
            builder = builder.header(name, *value);
        }
        let body = match body {
            Some(text) => Body::from(text),
            None => Body::empty(),
        };
        let request = builder.body(body).expect("request");
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
        let bytes = response
            .into_body()
            .collect()
            .await
            .expect("body")
            .to_bytes();
        Reply {
            status,
            content_type,
            body: String::from_utf8(bytes.to_vec()).expect("utf-8 body"),
        }
    }

    /// Signed in, the three control routes for `id`: each answers `r`.
    async fn control(&self, id: &str, cookie: &str) -> Vec<Reply> {
        let mut replies = Vec::new();
        for verb in ["start", "stop", "restart"] {
            replies.push(
                self.post(&format!("/api/instances/{id}/{verb}"), Some(cookie))
                    .await,
            );
        }
        replies
    }
}

struct Reply {
    status: StatusCode,
    content_type: String,
    body: String,
}

impl Reply {
    fn json(&self) -> Value {
        serde_json::from_str(&self.body)
            .unwrap_or_else(|e| panic!("not JSON ({e}): {:?}", self.body))
    }

    /// An error answer: `status` and `{"key": key, "args": []}`, nothing else.
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

    /// The accepted answer: 204, an empty body, no JSON.
    fn assert_accepted(&self) {
        assert_eq!(self.status, StatusCode::NO_CONTENT, "{}", self.body);
        assert!(self.body.is_empty(), "204 carries no body: {:?}", self.body);
    }

    /// 200, body not looked at.
    fn assert_ok(&self) {
        assert_eq!(self.status, StatusCode::OK, "{}", self.body);
    }

    /// 200 with exactly `{"enabled": <bool>}`: the flag alone.
    fn assert_enabled(&self, enabled: bool) {
        assert_eq!(self.status, StatusCode::OK, "{}", self.body);
        assert!(
            self.content_type.starts_with("application/json"),
            "content-type {:?}",
            self.content_type
        );
        let value = self.json();
        let obj = value
            .as_object()
            .unwrap_or_else(|| panic!("not an object: {}", self.body));
        assert_eq!(
            obj.len(),
            1,
            "the reply must carry the flag alone: {}",
            self.body
        );
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

// ── Failure paths first: the session ────────────────────────────────────

/// No session cookie on a control route or the boot write: 401
/// `session-required`, and the manager is not even connected to.
#[tokio::test]
async fn without_a_session_the_manager_is_never_asked() {
    for uri in [
        "/api/instances/default/start",
        "/api/instances/default/stop",
        "/api/instances/default/restart",
    ] {
        let app = app();
        app.post(uri, None)
            .await
            .assert_error(StatusCode::UNAUTHORIZED, "session-required");
        assert_eq!(app.connects.load(Ordering::SeqCst), 0, "{uri}: no connect");
        assert!(app.calls().is_empty(), "{uri}: {:?}", app.calls());
        assert!(
            principals_of(&app.principals).is_empty(),
            "{uri}: no principal named"
        );
    }
    let app = app();
    app.put(
        "/api/instances/default/boot",
        json!({ "enabled": true }),
        None,
    )
    .await
    .assert_error(StatusCode::UNAUTHORIZED, "session-required");
    assert_eq!(
        app.connects.load(Ordering::SeqCst),
        0,
        "the boot write: no connect"
    );
    assert!(app.calls().is_empty(), "{:?}", app.calls());
}

/// A cookie naming no session (made up, or from before a restart) is the same.
#[tokio::test]
async fn an_unknown_session_is_session_required() {
    let stale = format!("pai_session={}", "ab".repeat(32));
    let app = app();
    let _real = app.sign_in().await;
    for uri in [
        "/api/instances/default/start",
        "/api/instances/default/stop",
        "/api/instances/default/restart",
    ] {
        app.post(uri, Some(&stale))
            .await
            .assert_error(StatusCode::UNAUTHORIZED, "session-required");
    }
    app.put(
        "/api/instances/default/boot",
        json!({ "enabled": true }),
        Some(&stale),
    )
    .await
    .assert_error(StatusCode::UNAUTHORIZED, "session-required");
    assert_eq!(app.connects.load(Ordering::SeqCst), 0, "no connect");
    assert!(app.calls().is_empty(), "{:?}", app.calls());
}

/// The session check runs before the body is read: with no session a
/// malformed body is 401, not 400.
#[tokio::test]
async fn a_request_with_no_session_is_401_whatever_its_body() {
    let app = app();
    app.put_raw("/api/instances/default/boot", "not json", None)
        .await
        .assert_error(StatusCode::UNAUTHORIZED, "session-required");
    assert_eq!(app.connects.load(Ordering::SeqCst), 0, "no connect");
}

// ── The principal: the route acts for the signed-in user ────────────────

/// Each control route and the boot write connects with the session's user as
/// the principal, so the manager re-checks that user's own right at every
/// call: the session-free routes connect with none (the boot read, below).
#[tokio::test]
async fn an_action_connects_as_the_signed_in_user() {
    for user in ["alice", "root"] {
        let app = app();
        let cookie = app.sign_in_as(user).await;
        for verb in ["start", "stop", "restart"] {
            app.post(&format!("/api/instances/default/{verb}"), Some(&cookie))
                .await
                .assert_accepted();
        }
        app.put(
            "/api/instances/default/boot",
            json!({ "enabled": true }),
            Some(&cookie),
        )
        .await
        .assert_enabled(true);
        assert_eq!(
            app.calls(),
            [
                "start default",
                "stop default",
                "restart default",
                "set_boot_enabled default true",
                "boot_enabled default",
            ],
            "{user}: every call its own, no extra probe"
        );
        assert_eq!(
            principals_of(&app.principals),
            vec![Some(user.to_string()); 4],
            "{user}: four actions, four connects naming the session's user"
        );
        app.get("/api/session", Some(&cookie))
            .await
            .assert_signed_in_as(user);
    }
}

/// A name in the request never becomes the principal: a query parameter and a
/// header naming another user are ignored, the session's user is asked for.
#[tokio::test]
async fn a_name_in_the_request_is_never_the_principal() {
    let app = app();
    let cookie = app.sign_in_as("alice").await;
    app.post("/api/instances/default/start?user=root", Some(&cookie))
        .await
        .assert_accepted();
    app.post_header("/api/instances/default/stop", &cookie, "x-user", "root")
        .await
        .assert_accepted();
    assert_eq!(
        principals_of(&app.principals),
        vec![Some("alice".to_string()); 2],
        "the session's user, never the request's"
    );
}

// ── The manager's answers on the control routes ─────────────────────────

/// The manager refuses the call for the signed-in user: 403
/// `client-not-authorized` and that session ends at once, as the server read
/// does; the same cookie is 401 `session-required` afterwards.
#[tokio::test]
async fn a_refused_user_is_403_and_signed_out() {
    for verb in ["start", "stop", "restart"] {
        let app = app_refusing(&["alice"]);
        let cookie = app.sign_in().await;
        app.post(&format!("/api/instances/default/{verb}"), Some(&cookie))
            .await
            .assert_error(StatusCode::FORBIDDEN, "client-not-authorized");
        app.get("/api/session", Some(&cookie))
            .await
            .assert_error(StatusCode::UNAUTHORIZED, "session-required");
        assert_eq!(
            principals_of(&app.principals),
            vec![Some("alice".to_string())],
            "{verb}: the refused call, nothing after it"
        );
    }
    let app = app_refusing(&["alice"]);
    let cookie = app.sign_in().await;
    app.put(
        "/api/instances/default/boot",
        json!({ "enabled": true }),
        Some(&cookie),
    )
    .await
    .assert_error(StatusCode::FORBIDDEN, "client-not-authorized");
    app.get("/api/session", Some(&cookie))
        .await
        .assert_error(StatusCode::UNAUTHORIZED, "session-required");
    assert!(
        app.calls()
            .iter()
            .all(|c| c.starts_with("set_boot_enabled")),
        "the write is refused before anything is read back: {:?}",
        app.calls()
    );
}

/// Only the session that acted ends: a refusal for `root` signs `root` out and
/// leaves `alice` signed in and answered.
#[tokio::test]
async fn a_refusal_ends_that_session_and_no_other() {
    let app = app_refusing(&["root"]);
    let alice = app.sign_in_as("alice").await;
    let root = app.sign_in_as("root").await;
    app.post("/api/instances/default/stop", Some(&root))
        .await
        .assert_error(StatusCode::FORBIDDEN, "client-not-authorized");
    app.get("/api/session", Some(&root))
        .await
        .assert_error(StatusCode::UNAUTHORIZED, "session-required");
    app.post("/api/instances/default/stop", Some(&alice))
        .await
        .assert_accepted();
    app.get("/api/session", Some(&alice))
        .await
        .assert_signed_in_as("alice");
    assert_eq!(
        principals_of(&app.principals),
        vec![Some("root".to_string()), Some("alice".to_string())],
        "each call asked for its own session's user"
    );
}

/// No manager to connect to: 503 `client-unavailable`, and the session lives
/// on: the bff's own news, not a refusal of the user.
#[tokio::test]
async fn no_manager_is_unavailable_and_keeps_the_session() {
    let app = app_no_manager();
    let cookie = app.sign_in().await;
    for uri in [
        "/api/instances/default/start",
        "/api/instances/default/stop",
        "/api/instances/default/restart",
    ] {
        app.post(uri, Some(&cookie))
            .await
            .assert_error(StatusCode::SERVICE_UNAVAILABLE, "client-unavailable");
    }
    app.put(
        "/api/instances/default/boot",
        json!({ "enabled": true }),
        Some(&cookie),
    )
    .await
    .assert_error(StatusCode::SERVICE_UNAVAILABLE, "client-unavailable");
    assert_eq!(app.connects.load(Ordering::SeqCst), 4, "one connect each");
    app.get("/api/session", Some(&cookie))
        .await
        .assert_signed_in_as("alice");
}

/// A manager gone during the call, and one that is slower than the bff's
/// bound, are 503 too.
#[tokio::test]
async fn a_manager_gone_or_too_slow_is_unavailable() {
    let app = app_verbs(Err(ClientError::Unavailable));
    for r in app.control("default", &app.sign_in().await).await {
        r.assert_error(StatusCode::SERVICE_UNAVAILABLE, "client-unavailable");
    }
    let app = app_slow(Duration::from_secs(2));
    let cookie = app.sign_in().await;
    let asked = Instant::now();
    app.post("/api/instances/default/restart", Some(&cookie))
        .await
        .assert_error(StatusCode::SERVICE_UNAVAILABLE, "client-unavailable");
    assert!(
        asked.elapsed() < Duration::from_secs(1),
        "answered after {:?}",
        asked.elapsed()
    );
    app.put(
        "/api/instances/default/boot",
        json!({ "enabled": false }),
        Some(&cookie),
    )
    .await
    .assert_error(StatusCode::SERVICE_UNAVAILABLE, "client-unavailable");
}

/// The service manager refused the job, or the unit failed (the manager's
/// `Service`): 502 `client-service-failed` with empty `args`. The manager's
/// message stays in the bff's log: it must not reach the body. The same on
/// the boot write, where it is the write that fails (its read-back is not
/// asked).
#[tokio::test]
async fn a_service_failure_is_client_service_failed_with_no_args() {
    let detail = "Unit llama.cpp@default.service not found.";
    let app = app_failing(ClientError::Service(detail.into()));
    let cookie = app.sign_in().await;
    for uri in [
        "/api/instances/default/start",
        "/api/instances/default/boot",
    ] {
        let r = match uri {
            u if u.ends_with("boot") => app.put(u, json!({ "enabled": true }), Some(&cookie)).await,
            u => app.post(u, Some(&cookie)).await,
        };
        r.assert_error(StatusCode::BAD_GATEWAY, "client-service-failed");
        assert!(
            !r.body.contains("not found"),
            "the manager's detail must not travel: {}",
            r.body
        );
    }
}

/// Any other failure of the manager is 502 `client-other`, with empty `args`
/// and no detail in the body.
#[tokio::test]
async fn any_other_manager_failure_is_client_other() {
    for failure in [
        ClientError::Io("permission denied".into()),
        ClientError::Other("connection reset".into()),
        ClientError::BadFileName,
    ] {
        let app = app_verbs(Err(failure.clone()));
        let cookie = app.sign_in().await;
        for r in app.control("default", &cookie).await {
            r.assert_error(StatusCode::BAD_GATEWAY, "client-other");
            assert!(
                !r.body.contains("permission denied") && !r.body.contains("connection reset"),
                "the manager's detail must not travel: {}",
                r.body
            );
        }
    }
}

/// An id the manager has no instance for, by `NoInstance` or by a refused id:
/// 404 `client-no-instance`.
#[tokio::test]
async fn an_unknown_id_is_no_instance() {
    for failure in [ClientError::NoInstance, ClientError::InvalidId] {
        let app = app_failing(failure.clone());
        let cookie = app.sign_in().await;
        for verb in ["start", "stop", "restart"] {
            app.post(&format!("/api/instances/nope/{verb}"), Some(&cookie))
                .await
                .assert_error(StatusCode::NOT_FOUND, "client-no-instance");
        }
        app.put(
            "/api/instances/nope/boot",
            json!({ "enabled": true }),
            Some(&cookie),
        )
        .await
        .assert_error(StatusCode::NOT_FOUND, "client-no-instance");
        assert_eq!(
            app.calls(),
            [
                "start nope",
                "stop nope",
                "restart nope",
                "set_boot_enabled nope true",
            ],
            "asked for that id: {failure:?}"
        );
    }
}

/// An id that tries to leave its folder (`..%2Fx` decoded to `../x`) is no
/// instance: 404, the same as any id the manager refuses.
#[tokio::test]
async fn a_traversal_id_is_no_instance() {
    let app = app_failing(ClientError::InvalidId);
    let cookie = app.sign_in().await;
    for uri in [
        "/api/instances/..%2Fx/start",
        "/api/instances/..%2Fx/stop",
        "/api/instances/..%2Fx/restart",
    ] {
        app.post(uri, Some(&cookie))
            .await
            .assert_error(StatusCode::NOT_FOUND, "client-no-instance");
    }
    app.put(
        "/api/instances/..%2Fx/boot",
        json!({ "enabled": true }),
        Some(&cookie),
    )
    .await
    .assert_error(StatusCode::NOT_FOUND, "client-no-instance");
}

/// A failure that is not a refusal of the user keeps the session, on a verb
/// and on the boot write alike: 404, 502 `client-service-failed`, 502
/// `client-other`.
#[tokio::test]
async fn a_failure_that_is_not_a_refusal_keeps_the_session() {
    for (app, status, key) in [
        (
            app_failing(ClientError::NoInstance),
            StatusCode::NOT_FOUND,
            "client-no-instance",
        ),
        (
            app_failing(ClientError::Service("job failed".into())),
            StatusCode::BAD_GATEWAY,
            "client-service-failed",
        ),
        (
            app_failing(ClientError::Io("broken pipe".into())),
            StatusCode::BAD_GATEWAY,
            "client-other",
        ),
    ] {
        let cookie = app.sign_in().await;
        app.post("/api/instances/default/start", Some(&cookie))
            .await
            .assert_error(status, key);
        app.put(
            "/api/instances/default/boot",
            json!({ "enabled": true }),
            Some(&cookie),
        )
        .await
        .assert_error(status, key);
        app.get("/api/session", Some(&cookie))
            .await
            .assert_signed_in_as("alice");
    }
}

// ── `GET /api/instances/{id}/boot`: the route that needs no sign-in ─────

/// The boot state answers 200 with exactly `{"enabled"}`, for anyone: no
/// cookie, as with a signed-in one. It connects as the bff's own account, so
/// the connect is handed no principal.
#[tokio::test]
async fn the_boot_state_answers_the_flag_with_no_session() {
    for initial in [true, false] {
        let app = app_for(Answer::ok(), Ok(()), initial, None, Vec::new());
        app.get("/api/instances/default/boot", None)
            .await
            .assert_enabled(initial);
        let cookie = app.sign_in().await;
        app.get("/api/instances/default/boot", Some(&cookie))
            .await
            .assert_enabled(initial);
        assert_eq!(
            principals_of(&app.principals),
            vec![None, None],
            "the bff's own account, both times"
        );
        assert_eq!(
            app.calls(),
            ["boot_enabled default", "boot_enabled default"],
            "one read each, no probe"
        );
    }
}

/// The manager refuses the bff on this route: 502 `client-not-authorized`, the
/// same key as on `GET /api/instances`, and no session ends: the route has
/// none, and a signed-in browser's cookie survives the answer.
#[tokio::test]
async fn a_manager_refusal_on_the_boot_read_is_502_and_ends_no_session() {
    let app = app_boot_read(Err(ClientError::NotAuthorized));
    let cookie = app.sign_in().await;
    app.get("/api/instances/default/boot", None)
        .await
        .assert_error(StatusCode::BAD_GATEWAY, "client-not-authorized");
    app.get("/api/instances/default/boot", Some(&cookie))
        .await
        .assert_error(StatusCode::BAD_GATEWAY, "client-not-authorized");
    app.get("/api/session", Some(&cookie))
        .await
        .assert_signed_in_as("alice");
}

/// The boot read's other answers: 404 `client-no-instance` for no such
/// instance or a refused id, 503 `client-unavailable` for no manager or a slow
/// one, 502 `client-other` for anything else, each with empty `args`.
#[tokio::test]
async fn the_boot_read_errors() {
    for (app, status, key) in [
        (
            app_boot_read(Err(ClientError::NoInstance)),
            StatusCode::NOT_FOUND,
            "client-no-instance",
        ),
        (
            app_boot_read(Err(ClientError::InvalidId)),
            StatusCode::NOT_FOUND,
            "client-no-instance",
        ),
        (
            app_boot_read(Err(ClientError::Unavailable)),
            StatusCode::SERVICE_UNAVAILABLE,
            "client-unavailable",
        ),
        (
            app_boot_read(Err(ClientError::Io("permission denied".into()))),
            StatusCode::BAD_GATEWAY,
            "client-other",
        ),
        (
            app_boot_read(Err(ClientError::Other("reset".into()))),
            StatusCode::BAD_GATEWAY,
            "client-other",
        ),
        (
            app_boot_read(Err(ClientError::Service("unit failed".into()))),
            StatusCode::BAD_GATEWAY,
            "client-other",
        ),
    ] {
        let r = app.get("/api/instances/default/boot", None).await;
        r.assert_error(status, key);
        assert!(
            !r.body.contains("permission denied") && !r.body.contains("unit failed"),
            "the manager's detail must not travel: {}",
            r.body
        );
    }
    let app = app_no_manager();
    app.get("/api/instances/default/boot", None)
        .await
        .assert_error(StatusCode::SERVICE_UNAVAILABLE, "client-unavailable");
    assert_eq!(app.connects.load(Ordering::SeqCst), 1, "connected once");
    let app = app_slow(Duration::from_secs(2));
    let asked = Instant::now();
    app.get("/api/instances/default/boot", None)
        .await
        .assert_error(StatusCode::SERVICE_UNAVAILABLE, "client-unavailable");
    assert!(
        asked.elapsed() < Duration::from_secs(1),
        "answered after {:?}",
        asked.elapsed()
    );
}

// ── `PUT /api/instances/{id}/boot`: the write and its read-back ─────────

/// The write answers 200 `{"enabled"}` read back from the manager after the
/// write: the boot state is the cell the write left, and the reply is what the
/// read answered. Where the write does not land as asked (the manager enabled
/// the unit and `is-enabled` still says no), the reply says no, not what the
/// body asked: an echo of the body is not a read-back.
#[tokio::test]
async fn the_boot_write_answers_the_state_read_back() {
    for (asked, left) in [(true, false), (false, true), (true, true), (false, false)] {
        let app = app_boot_leaving(left);
        let cookie = app.sign_in().await;
        app.put(
            "/api/instances/default/boot",
            json!({ "enabled": asked }),
            Some(&cookie),
        )
        .await
        .assert_enabled(left);
        assert_eq!(
            app.calls(),
            [
                format!("set_boot_enabled default {asked}"),
                "boot_enabled default".to_string(),
            ],
            "the write, then the read: asked {asked}, left {left}"
        );
        assert_eq!(
            principals_of(&app.principals),
            vec![Some("alice".to_string())],
            "the write is a call for the signed-in user"
        );
    }
}

/// The plain path: the write lands, and the read-back repeats it. Both
/// directions, and the write is asked for the id in the path.
#[tokio::test]
async fn the_boot_write_lands_and_comes_back() {
    let app = app();
    let cookie = app.sign_in().await;
    app.put(
        "/api/instances/vulkan/boot",
        json!({ "enabled": true }),
        Some(&cookie),
    )
    .await
    .assert_enabled(true);
    app.put(
        "/api/instances/vulkan/boot",
        json!({ "enabled": false }),
        Some(&cookie),
    )
    .await
    .assert_enabled(false);
    assert_eq!(
        app.calls(),
        [
            "set_boot_enabled vulkan true",
            "boot_enabled vulkan",
            "set_boot_enabled vulkan false",
            "boot_enabled vulkan",
        ]
    );
}

/// The boot write's failures answer like the control routes' and end the
/// session only when the manager refused the user.
#[tokio::test]
async fn the_boot_write_errors() {
    for (app, status, key, ends_the_session) in [
        (
            app_boot_write(Err(ClientError::NoInstance)),
            StatusCode::NOT_FOUND,
            "client-no-instance",
            false,
        ),
        (
            app_boot_write(Err(ClientError::Unavailable)),
            StatusCode::SERVICE_UNAVAILABLE,
            "client-unavailable",
            false,
        ),
        (
            app_boot_write(Err(ClientError::Service("job failed".into()))),
            StatusCode::BAD_GATEWAY,
            "client-service-failed",
            false,
        ),
        (
            app_boot_write(Err(ClientError::Other("reset".into()))),
            StatusCode::BAD_GATEWAY,
            "client-other",
            false,
        ),
        (
            app_boot_write(Err(ClientError::NotAuthorized)),
            StatusCode::FORBIDDEN,
            "client-not-authorized",
            true,
        ),
    ] {
        let cookie = app.sign_in().await;
        app.put(
            "/api/instances/default/boot",
            json!({ "enabled": false }),
            Some(&cookie),
        )
        .await
        .assert_error(status, key);
        let after = app.get("/api/session", Some(&cookie)).await;
        if ends_the_session {
            after.assert_error(StatusCode::UNAUTHORIZED, "session-required");
        } else {
            after.assert_signed_in_as("alice");
        }
    }
}

/// The write succeeded and the read-back failed: the reply is the read's
/// answer, not the write's, and the manager was asked twice.
#[tokio::test]
async fn a_failing_read_back_after_a_good_write_is_answered_like_a_read() {
    let app = app_for(
        Answer {
            boot: Err(ClientError::Io("permission denied".into())),
            ..Answer::ok()
        },
        Ok(()),
        true,
        None,
        Vec::new(),
    );
    let cookie = app.sign_in().await;
    app.put(
        "/api/instances/default/boot",
        json!({ "enabled": false }),
        Some(&cookie),
    )
    .await
    .assert_error(StatusCode::BAD_GATEWAY, "client-other");
    assert_eq!(
        app.calls(),
        ["set_boot_enabled default false", "boot_enabled default"],
        "the write ran, the read failed"
    );
}

// ── A malformed body ────────────────────────────────────────────────────

/// A body that is not `{"enabled": <bool>}` is 400 with a key, and the manager
/// is not asked: an empty body, text that is not JSON, no `enabled`, `enabled`
/// not a bool, `null`.
#[tokio::test]
async fn a_malformed_boot_body_is_400_and_asks_nothing() {
    let bodies = [
        "",
        "not json",
        "{}",
        "[]",
        "null",
        "{\"enabled\":null}",
        "{\"enabled\":\"yes\"}",
        "{\"enabled\":1}",
        "{\"Enabled\":true}",
        "{\"enabled\":true,}",
    ];
    for body in bodies {
        let app = app();
        let cookie = app.sign_in().await;
        app.put_raw("/api/instances/default/boot", body, Some(&cookie))
            .await
            .assert_error(StatusCode::BAD_REQUEST, MALFORMED);
        assert_eq!(
            app.connects.load(Ordering::SeqCst),
            0,
            "no manager for {body:?}"
        );
        assert!(app.calls().is_empty(), "nothing asked for {body:?}");
    }
}

/// A body of a content type that is not JSON is the same 400, as on the
/// sign-in route: the bff reads the body itself and refuses what it cannot
/// read.
#[tokio::test]
async fn a_boot_body_that_is_not_json_is_400() {
    let app = app();
    let cookie = app.sign_in().await;
    app.put_plain("/api/instances/default/boot", "enabled=true", Some(&cookie))
        .await
        .assert_error(StatusCode::BAD_REQUEST, MALFORMED);
    assert_eq!(app.connects.load(Ordering::SeqCst), 0, "no connect");
}

/// A malformed body does not end a session: it is the request's fault, not a
/// refusal of the user.
#[tokio::test]
async fn a_malformed_boot_body_keeps_the_session() {
    let app = app();
    let cookie = app.sign_in().await;
    app.put_raw("/api/instances/default/boot", "{}", Some(&cookie))
        .await
        .assert_error(StatusCode::BAD_REQUEST, MALFORMED);
    app.get("/api/session", Some(&cookie))
        .await
        .assert_signed_in_as("alice");
}

// ── The idle deadline: the polls must not keep a session alive ──────────

/// `GET /api/instances` is the read the web UI polls every 5 s. It never
/// moves a session's idle deadline, which stays the sign-in's, to the second:
/// at the timeout the session is alive, one second later it is gone.
#[tokio::test]
async fn the_instances_read_does_not_move_the_idle_deadline() {
    let alive = app();
    let cookie = alive.sign_in().await;
    alive.clock.advance(SHORT);
    let r = alive.get("/api/instances", Some(&cookie)).await;
    assert_eq!(r.status, StatusCode::OK, "the poll is answered: {}", r.body);
    alive.clock.advance(IDLE - SHORT);
    alive
        .get("/api/session", Some(&cookie))
        .await
        .assert_signed_in_as("alice");

    // One second past that same deadline: the poll, in the middle, moved
    // nothing. A read that refreshed the session would still be alive here.
    let gone = app();
    let cookie = gone.sign_in().await;
    gone.clock.advance(SHORT);
    gone.get("/api/instances", Some(&cookie)).await.assert_ok();
    gone.clock.advance(IDLE - SHORT + Duration::from_secs(1));
    gone.get("/api/session", Some(&cookie))
        .await
        .assert_error(StatusCode::UNAUTHORIZED, "session-required");
}

/// `GET /api/instances/{id}/boot`, which the instance page reads the same
/// way, never moves it either.
#[tokio::test]
async fn the_boot_read_does_not_move_the_idle_deadline() {
    let alive = app();
    let cookie = alive.sign_in().await;
    alive.clock.advance(SHORT);
    alive
        .get("/api/instances/default/boot", Some(&cookie))
        .await
        .assert_enabled(false);
    alive.clock.advance(IDLE - SHORT);
    alive
        .get("/api/session", Some(&cookie))
        .await
        .assert_signed_in_as("alice");

    let gone = app();
    let cookie = gone.sign_in().await;
    gone.clock.advance(SHORT);
    gone.get("/api/instances/default/boot", Some(&cookie))
        .await
        .assert_enabled(false);
    gone.clock.advance(IDLE - SHORT + Duration::from_secs(1));
    gone.get("/api/session", Some(&cookie))
        .await
        .assert_error(StatusCode::UNAUTHORIZED, "session-required");
}

/// The rule above is about the reads only: an action is the user acting, so it
/// moves the deadline. Same clock steps as the two tests above, and the
/// session lives one second past where the polled one ended.
#[tokio::test]
async fn an_action_moves_the_idle_deadline() {
    let start = app();
    let cookie = start.sign_in().await;
    start.clock.advance(SHORT);
    start
        .post("/api/instances/default/start", Some(&cookie))
        .await
        .assert_accepted();
    start.clock.advance(IDLE);
    start
        .get("/api/session", Some(&cookie))
        .await
        .assert_signed_in_as("alice");

    let boot = app();
    let cookie = boot.sign_in().await;
    boot.clock.advance(SHORT);
    boot.put(
        "/api/instances/default/boot",
        json!({ "enabled": true }),
        Some(&cookie),
    )
    .await
    .assert_enabled(true);
    boot.clock.advance(IDLE);
    boot.get("/api/session", Some(&cookie))
        .await
        .assert_signed_in_as("alice");

    // One second past the action's deadline, the session is gone.
    let gone = app();
    let cookie = gone.sign_in().await;
    gone.clock.advance(SHORT);
    gone.post("/api/instances/default/stop", Some(&cookie))
        .await
        .assert_accepted();
    gone.clock.advance(IDLE + Duration::from_secs(1));
    gone.get("/api/session", Some(&cookie))
        .await
        .assert_error(StatusCode::UNAUTHORIZED, "session-required");
}
