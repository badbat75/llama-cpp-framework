//! `GET /api/node/model-files`: the node's one models folder as the
//! model-file picker's list (M3 of "Choosing the model file on the web",
//! `ARCHITECTURE.md`, amended by "One models folder for every engine" (3)),
//! relayed from the manager's `ModelFilesAs` as the signed-in user, through
//! `pai_bff::router_with_session` with a fake manager client and a fake
//! authenticator: never the bus, pipe, PAM, LogonUser, a folder or an engine.
//! Every request runs under a deadline, so a bff that waits on the manager
//! fails instead of hanging.
//!
//! The contract:
//! - A session is needed: with none the answer is 401 `session-required`, and
//!   the manager is never connected to, so the folder is never opened.
//! - It is a page read, not a poll: `Sessions::current` is asked, so the idle
//!   deadline moves (the picker asks when it opens, the page never polls it).
//! - The list is asked with `model_files` as the signed-in user: the connect
//!   names that user, and one call answers the page.
//! - 200 is the manager's reply with `"source":"local"`:
//!   `{"source": "local", "dir": "<abs>", "files": [{"path", "size", "shards",
//!   "category"}], "truncated": <bool>, "error": null | {"key", "args"}}`,
//!   every row as the manager answered it, adding and dropping nothing. The
//!   folder's keyed `error` (`models-dir-unreadable`) travels inside the 200:
//!   an unreadable folder is the picker's empty list with a text, never the
//!   route's failure.
//! - The manager's refusals relay as every `As` route's: 403
//!   `session-not-administrator` and the session ended when the manager
//!   refused the signed-in user, 503 `client-unavailable` when there is no
//!   manager or it answers past the bound, 502 `client-other` for anything
//!   else, the manager's words kept out of the body.

use axum::body::Body;
use axum::extract::ConnectInfo;
use axum::http::{header, Method, Request, StatusCode};
use axum::Router;
use http_body_util::BodyExt;
use pai_bff::session::{AuthError, Authenticator, Identity};
use pai_bff::{Client, Connect};
use pai_core::client::{
    ClientError, InstanceState, ManagerClient, ManagerFile, ModelFileRow, ModelFilesReply,
};
use serde_json::{json, Value};
use std::net::SocketAddr;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};
use tower::ServiceExt;

/// How long the bff waits for a manager read in these tests.
const TIMEOUT: Duration = Duration::from_millis(200);

/// How long a test waits for the bff's answer before failing.
const DEADLINE: Duration = Duration::from_secs(5);

const CLIENT: &str = "127.0.0.1:51003";

/// The revision secret these routers are built with.
const REVISION_KEY: [u8; 16] = [0; 16];

/// A step just under the session's idle timeout (`session.rs`'s own IDLE):
/// only a route that moved the deadline answers later than two of these.
const SHORT: Duration = Duration::from_secs(30 * 60 - 60);

/// A step just over that timeout: from the last read that moved the deadline,
/// this is where the gate answers.
const PAST: Duration = Duration::from_secs(30 * 60 + 1);

/// The key of the gate's answer (`session.rs`).
const SESSION_REQUIRED: &str = "session-required";

type Log = Arc<Mutex<Vec<String>>>;

/// The folder the fake lists: a path no real install has, so a document
/// carrying it carries the fake's answer.
const DIR: &str = "/var/lib/llama.cpp/models";

/// The listing the fake answers: a split model (first shard, three shards),
/// an mmproj, and a loose file with no category, in the manager's order.
fn listed_reply() -> ModelFilesReply {
    ModelFilesReply {
        dir: DIR.into(),
        files: vec![
            ModelFileRow {
                path: format!("{DIR}/models/o/r/x.gguf"),
                size: 4_200_000_000,
                shards: 3,
                category: Some("models".into()),
            },
            ModelFileRow {
                path: format!("{DIR}/mmprojs/mm.gguf"),
                size: 1_000_000,
                shards: 1,
                category: Some("mmprojs".into()),
            },
            ModelFileRow {
                path: format!("{DIR}/loose.gguf"),
                size: 10,
                shards: 1,
                category: None,
            },
        ],
        truncated: false,
        error: None,
    }
}

/// The JSON of [`listed_reply`], as the design's picker reads it.
fn listed() -> Value {
    json!({
        "source": "local",
        "dir": DIR,
        "files": [
            {"path": format!("{DIR}/models/o/r/x.gguf"), "size": 4_200_000_000u64, "shards": 3, "category": "models"},
            {"path": format!("{DIR}/mmprojs/mm.gguf"), "size": 1_000_000u64, "shards": 1, "category": "mmprojs"},
            {"path": format!("{DIR}/loose.gguf"), "size": 10u64, "shards": 1, "category": null},
        ],
        "truncated": false,
        "error": null,
    })
}

// ── Fakes ───────────────────────────────────────────────────────────────

/// A manager that records every call and answers the file list, or what the
/// test set. The route asks `model_files` and nothing else: every other verb
/// answers what a listing read never touches.
struct FakeManager {
    log: Log,
    /// What `model_files` answers.
    answer: Arc<Mutex<Result<ModelFilesReply, ClientError>>>,
    /// How long `model_files` takes before it answers.
    delay: Arc<Mutex<Duration>>,
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
    fn instances(&self) -> Result<Vec<pai_core::client::InstanceInfo>, ClientError> {
        self.record("instances".into());
        Ok(vec![])
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
        Ok((true, true))
    }
    fn model_files(&self) -> Result<ModelFilesReply, ClientError> {
        self.record("model_files".into());
        std::thread::sleep(*self.delay.lock().unwrap_or_else(|e| e.into_inner()));
        self.answer
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .clone()
    }
}

impl FakeManager {
    fn record(&self, call: String) {
        self.log
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .push(call);
    }
}

/// The authenticator: `alice` with the one password, and `root` too (the
/// session survives for a signed-in user the route never refuses).
struct FakeAuth;

impl Authenticator for FakeAuth {
    fn authenticate(&self, user: &str, password: &str) -> Result<Identity, AuthError> {
        if password == "correct horse" && (user == "alice" || user == "root") {
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

/// What the fake answers, shared with every manager it hands out.
struct Fixture {
    answer: Arc<Mutex<Result<ModelFilesReply, ClientError>>>,
    delay: Arc<Mutex<Duration>>,
}

impl Fixture {
    /// The listing of [`listed_reply`].
    fn listed() -> Fixture {
        Fixture {
            answer: Arc::new(Mutex::new(Ok(listed_reply()))),
            delay: Arc::new(Mutex::new(Duration::ZERO)),
        }
    }
    /// The answer, set by a test (a refusal, a failure, a folder error).
    fn set(&self, answer: Result<ModelFilesReply, ClientError>) {
        *self.answer.lock().unwrap_or_else(|e| e.into_inner()) = answer;
    }
    /// How long `model_files` takes.
    fn set_delay(&self, delay: Duration) {
        *self.delay.lock().unwrap_or_else(|e| e.into_inner()) = delay;
    }
}

/// A router over a fake manager answering from `fixture`, with the log of its
/// calls, the count of connects and the principal each was asked for.
struct App {
    router: Router,
    clock: FakeClock,
    log: Log,
    connects: Arc<AtomicUsize>,
    principals: Arc<Mutex<Vec<Option<String>>>>,
    fixture: Fixture,
}

fn app(fixture: Fixture) -> App {
    let log = Log::default();
    let connects = Arc::new(AtomicUsize::new(0));
    let principals = Arc::new(Mutex::new(Vec::new()));
    let (l, c, p) = (log.clone(), connects.clone(), principals.clone());
    let (answer, delay) = (fixture.answer.clone(), fixture.delay.clone());
    let connect: Connect = Arc::new(move |principal: Option<&str>| {
        c.fetch_add(1, Ordering::SeqCst);
        p.lock()
            .unwrap_or_else(|e| e.into_inner())
            .push(principal.map(str::to_string));
        Ok(Box::new(FakeManager {
            log: l.clone(),
            answer: answer.clone(),
            delay: delay.clone(),
        }) as Client)
    });
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
        fixture,
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

    /// An error answer: its status and `{"key", "args"}`. A route the bff does
    /// not route answers a bare 404 with an empty body, so every one of these
    /// is red, not accidentally green.
    fn assert_error(&self, status: StatusCode, key: &str, args: &[&str]) {
        assert_eq!(self.status, status, "{}", self.body);
        assert_eq!(
            self.json(),
            json!({ "key": key, "args": args }),
            "{}",
            self.body
        );
    }

    fn assert_session_required(&self) {
        self.assert_error(StatusCode::UNAUTHORIZED, SESSION_REQUIRED, &[]);
    }

    /// `GET /api/session` for a session that is still signed in.
    fn assert_signed_in_as(&self, user: &str) {
        assert_eq!(self.status, StatusCode::OK, "signed in: {}", self.body);
        assert_eq!(self.json(), json!({ "user": user }), "{}", self.body);
    }
}

impl App {
    fn calls(&self) -> Vec<String> {
        self.log.lock().unwrap_or_else(|e| e.into_inner()).clone()
    }

    fn principals(&self) -> Vec<Option<String>> {
        self.principals
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .clone()
    }

    /// Nothing was asked of the manager: no connect, no call.
    fn assert_asked_nothing(&self, what: &str) {
        assert_eq!(self.connects.load(Ordering::SeqCst), 0, "{what}: connected");
        assert!(self.calls().is_empty(), "{what}: {:?}", self.calls());
        assert!(
            self.principals().is_empty(),
            "{what}: a principal was named"
        );
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

    async fn sign_in(&self) -> String {
        self.sign_in_as("alice").await
    }

    /// `GET /api/session`: the state of the session the cookie names.
    async fn session_of(&self, cookie: &str) -> Reply {
        self.get("/api/session", Some(cookie)).await
    }

    /// `GET /api/node/model-files`, the route under test.
    async fn model_files(&self, cookie: Option<&str>) -> Reply {
        self.get("/api/node/model-files", cookie).await
    }

    async fn get(&self, uri: &str, cookie: Option<&str>) -> Reply {
        self.get_within(DEADLINE, uri, cookie).await
    }

    async fn get_within(&self, limit: Duration, uri: &str, cookie: Option<&str>) -> Reply {
        let mut builder = Request::builder().method(Method::GET).uri(uri);
        if let Some(c) = cookie {
            builder = builder.header(header::COOKIE, c);
        }
        let request = builder.body(Body::empty()).expect("request");
        let response = tokio::time::timeout(limit, self.router.clone().oneshot(request))
            .await
            .unwrap_or_else(|_| panic!("no answer to {uri} within {limit:?}"))
            .expect("infallible");
        let status = response.status();
        let bytes = tokio::time::timeout(DEADLINE, response.into_body().collect())
            .await
            .expect("body read in time")
            .expect("body readable")
            .to_bytes();
        Reply {
            status,
            body: String::from_utf8_lossy(&bytes).to_string(),
        }
    }
}

// ── The gate ────────────────────────────────────────────────────────────

/// No session: 401 `session-required`, and the manager is never connected to:
/// an anonymous visitor opens no folder through the manager.
#[tokio::test]
async fn model_files_without_a_session_is_401_and_the_manager_is_not_asked() {
    let app = app(Fixture::listed());
    let rep = app.model_files(None).await;
    rep.assert_session_required();
    app.assert_asked_nothing("no session");
}

/// The cookie of a session that ended is the gate's answer too, and asking it
/// asks the manager nothing.
#[tokio::test]
async fn model_files_of_a_session_that_ended_is_401_and_asks_nothing() {
    let app = app(Fixture::listed());
    let cookie = app.sign_in().await;
    app.clock.advance(Duration::from_secs(30 * 60 + 1));
    let rep = app.model_files(Some(&cookie)).await;
    rep.assert_session_required();
    assert_eq!(
        app.calls(),
        Vec::<String>::new(),
        "an ended session asks no manager"
    );
}

// ── The listing ─────────────────────────────────────────────────────────

/// The manager's rows, asked as the signed-in user (the `As` form), in one
/// call, with `"source":"local"` beside them: the picker reads either source.
#[tokio::test]
async fn model_files_answers_the_managers_rows_as_the_signed_in_user() {
    let app = app(Fixture::listed());
    let cookie = app.sign_in().await;
    let rep = app.model_files(Some(&cookie)).await;
    assert_eq!(rep.status, StatusCode::OK, "{}", rep.body);
    assert_eq!(rep.json(), listed(), "the design's document");
    assert_eq!(
        app.calls(),
        vec!["model_files".to_string()],
        "one call, and the page asks nothing else"
    );
    assert_eq!(
        app.principals(),
        vec![Some("alice".to_string())],
        "the list is read as the signed-in user, the As form"
    );
}

/// The reply travels as the manager answered it: the folder's keyed `error`
/// inside a 200 (an unreadable folder is the picker's empty list with a text,
/// not the route's failure), `truncated` true, a null `category`, adding and
/// judging nothing.
#[tokio::test]
async fn model_files_relay_the_rows_the_manager_answered() {
    let app = app(Fixture::listed());
    app.fixture.set(Ok(ModelFilesReply {
        dir: DIR.into(),
        files: vec![ModelFileRow {
            path: format!("{DIR}/models/x.gguf"),
            size: 0,
            shards: 1,
            category: None,
        }],
        truncated: true,
        error: Some(pai_core::client::Reason {
            key: "models-dir-unreadable".into(),
            args: vec![DIR.into()],
        }),
    }));
    let cookie = app.sign_in().await;
    let rep = app.model_files(Some(&cookie)).await;
    assert_eq!(rep.status, StatusCode::OK, "{}", rep.body);
    assert_eq!(
        rep.json(),
        json!({
            "source": "local",
            "dir": DIR,
            "files": [{"path": format!("{DIR}/models/x.gguf"), "size": 0u64, "shards": 1, "category": null}],
            "truncated": true,
            "error": {"key": "models-dir-unreadable", "args": [DIR]},
        }),
        "the folder's own error inside the 200: {}",
        rep.body
    );
}

/// An empty listing is a 200 with an empty `files`: a node with no models
/// folder content, no error page.
#[tokio::test]
async fn model_files_of_an_empty_folder_is_an_empty_list() {
    let app = app(Fixture::listed());
    app.fixture.set(Ok(ModelFilesReply {
        dir: DIR.into(),
        files: vec![],
        truncated: false,
        error: None,
    }));
    let cookie = app.sign_in().await;
    let rep = app.model_files(Some(&cookie)).await;
    assert_eq!(rep.status, StatusCode::OK, "{}", rep.body);
    assert_eq!(
        rep.json(),
        json!({"source": "local", "dir": DIR, "files": [], "truncated": false, "error": null})
    );
}

// ── The refusals and the failures ───────────────────────────────────────

/// The manager refuses the signed-in user (removed from `pai`): 403
/// `session-not-administrator`, and the session ends, as every other route's
/// refusal does.
#[tokio::test]
async fn model_files_refused_ends_the_session() {
    let app = app(Fixture::listed());
    app.fixture.set(Err(ClientError::NotAuthorized));
    let other = app.sign_in_as("root").await;
    let cookie = app.sign_in().await;
    let rep = app.model_files(Some(&cookie)).await;
    rep.assert_error(StatusCode::FORBIDDEN, "session-not-administrator", &[]);
    assert_eq!(
        app.calls(),
        vec!["model_files".to_string()],
        "the refusal is the read's own"
    );
    app.session_of(&cookie).await.assert_session_required();
    app.session_of(&other).await.assert_signed_in_as("root");
}

/// No manager is 503 `client-unavailable`, as the other reads.
#[tokio::test]
async fn model_files_with_no_manager_is_503() {
    let app = app(Fixture::listed());
    app.fixture.set(Err(ClientError::Unavailable));
    let cookie = app.sign_in().await;
    app.model_files(Some(&cookie)).await.assert_error(
        StatusCode::SERVICE_UNAVAILABLE,
        "client-unavailable",
        &[],
    );
    assert_eq!(app.calls(), vec!["model_files".to_string()]);
}

/// A manager that answers past the route's deadline: the page retries, so the
/// answer is the same 503 as no manager, and it comes at the deadline, not
/// when the walk finally speaks.
#[tokio::test]
async fn model_files_answered_past_the_deadline_is_503() {
    let app = app(Fixture::listed());
    let slow = TIMEOUT + Duration::from_secs(1);
    app.fixture.set_delay(slow);
    let cookie = app.sign_in().await;
    let started = Instant::now();
    let rep = app.model_files(Some(&cookie)).await;
    rep.assert_error(StatusCode::SERVICE_UNAVAILABLE, "client-unavailable", &[]);
    let waited = started.elapsed();
    assert!(
        waited < slow + Duration::from_secs(5),
        "the answer is at the deadline, not at the walk: {waited:?}"
    );
}

/// Anything else is 502 `client-other`, never the manager's own words: a
/// refusal is about the caller, this is about the manager.
#[tokio::test]
async fn model_files_of_a_manager_that_failed_is_502_without_its_detail() {
    let app = app(Fixture::listed());
    app.fixture.set(Err(ClientError::Other(
        "models: cannot open /var/lib/private-ai/models".into(),
    )));
    let cookie = app.sign_in().await;
    let rep = app.model_files(Some(&cookie)).await;
    rep.assert_error(StatusCode::BAD_GATEWAY, "client-other", &[]);
    assert!(
        !rep.body.contains("private-ai"),
        "the manager's words stay out of the answer: {}",
        rep.body
    );
    app.session_of(&cookie).await.assert_signed_in_as("alice");
}

// ── The session's idle deadline ─────────────────────────────────────────

/// A page read, not a poll: the read moves the session's idle deadline, so a
/// picker opened late keeps the sign-in alive. The steps are chosen to tell
/// the two gates of `session.rs` apart: after two `SHORT` steps the clock is
/// past the deadline a `peek` route leaves where it was (the sign-in's), and
/// inside the one this read moved, so only a `current` route is served there;
/// one `PAST` after the last answer the moved deadline is over, so the route
/// moved it, it did not clear it.
#[tokio::test]
async fn model_files_moves_the_sessions_idle_deadline() {
    let app = app(Fixture::listed());
    let cookie = app.sign_in().await;
    app.clock.advance(SHORT);
    let rep = app.model_files(Some(&cookie)).await;
    assert_eq!(rep.status, StatusCode::OK, "late: {}", rep.body);
    // Past the deadline of the sign-in, inside the one this read moved.
    app.clock.advance(SHORT);
    let rep = app.model_files(Some(&cookie)).await;
    assert_eq!(
        rep.status,
        StatusCode::OK,
        "two SHORT steps are past the deadline of a read that leaves it alone: {}",
        rep.body
    );
    // Past the deadline of the read that just answered: the session ended.
    app.clock.advance(PAST);
    let rep = app.model_files(Some(&cookie)).await;
    rep.assert_session_required();
    assert_eq!(
        app.calls(),
        vec!["model_files".to_string(), "model_files".to_string()],
        "the ended session asks the manager nothing"
    );
}
