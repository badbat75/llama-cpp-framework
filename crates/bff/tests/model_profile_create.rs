//! `POST /api/profiles/model` and `DELETE /api/profiles/model/{id}`: the
//! create and the delete of the model-profile pool (ARCHITECTURE.md "Create,
//! clone and delete model profiles from the web", slice MP2;
//! `fe/briefs/api-contract.md), through `pai_bff::router_with_session_and_write_timeout`
//! with a fake manager client and a fake authenticator, never the real bus,
//! pipe, PAM or LogonUser. Every request runs under a deadline, so a bff that
//! waits on the manager fails instead of hanging.
//!
//! The contract, mirroring the server-profile pool (`tests/server_profiles.rs`):
//! - Both routes sit behind a session: with none, or with the cookie of one
//!   that ended, the answer is 401 `{"key": "session-required", "args": []}`
//!   and the manager is never even connected to.
//! - `POST /api/profiles/model` `{"id", "from"}` (a clone) or `{"id",
//!   "values"}` with `model` alone, `hf-repo` alone, or `hf-repo` and
//!   `hf-file`, answers 201 `{"id"}`; the stub received `create_model_profile`
//!   with the id and the source: `from` for a clone, the pick's keys for a
//!   source, the id, `from` and every value trimmed of spaces and tabs as the
//!   save does, so a padded name reaches the manager trimmed and the 201
//!   answers the trimmed id. A body it
//!   cannot read is 400 `model-profiles-malformed` with no manager asked: not
//!   an object, a non-string, an unknown field, both or neither of `from` and
//!   `values`, a `values` key outside the three, a control character, a
//!   content type that is not JSON, a name, a source or a value that is empty
//!   once trimmed.
//! - The manager's refusals arrive as `ClientError::Invalid` and answer 422
//!   `{"key", "args", "field"}` with the field the design marks:
//!   `invalid-model-profile-id` `id`, `comment-markers` the key it names,
//!   `no-model-profile-named` (the copy source) `from`,
//!   `model-profile-no-model` and `model-profile-bad-source` `values`.
//!   `model-profile-name-taken` is 409 `{"key", "args": [id]}`. The bff judges
//!   no name, so a tab inside an id, the one control character the body's text
//!   rule allows, travels to the manager and answers the manager's 422 on `id`,
//!   never a 400 of the bff's own.
//! - `DELETE /api/profiles/model/{id}` answers 204; 404
//!   `no-model-profile-named` `[id]`; 409 `model-profile-enabled` `[id]`; 409
//!   `model-profile-assigned` `[id, instances]`; any other check the manager
//!   runs on the way is 422 `{"key", "args"}` with the refusal's own key, its
//!   values and no field (a delete has no form field to mark).
//! - A refusal of the signed-in user is 403 `session-not-administrator` and
//!   ends that session on both routes; no manager is 503
//!   `client-unavailable`; anything else 502 `client-other`, without the
//!   manager's detail.
//! - Both are writes, so the manager is given the write's own, longer limit,
//!   not the one that fits a read: a delay past `TIMEOUT` and within
//!   `WRITE_TIMEOUT` answers 201 and 204 (a route handed `timeout` is cut off
//!   at 503 there).
//!
//! The two routes are routed and served, so every assertion below is a guard
//! on the behaviour MP2 shipped, not a red waiting for it.

use axum::body::Body;
use axum::extract::ConnectInfo;
use axum::http::{header, Method, Request, StatusCode};
use axum::Router;
use http_body_util::BodyExt;
use pai_bff::session::{AuthError, Authenticator, Identity};
use pai_bff::{Client, Connect};
use pai_core::checks;
use pai_core::client::{
    ClientError, InstanceInfo, InstanceState, ManagerClient, ManagerFile, ModelProfileSource,
};
use serde_json::{json, Value};
use std::net::SocketAddr;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};
use tower::ServiceExt;

/// How long the bff waits for a read of the manager in these tests.
const TIMEOUT: Duration = Duration::from_millis(200);

/// How long the bff waits for a write of the manager in these tests: the
/// create and the removal are writes, so they run under the write's own,
/// longer limit, as the save and the switch do (`tests/model_profile_save.rs`).
const WRITE_TIMEOUT: Duration = Duration::from_millis(900);

/// How long a test waits for the bff's answer before failing.
const DEADLINE: Duration = Duration::from_secs(5);

/// The client address a sign-in is sent from (the lockout counters key on it).
const CLIENT: &str = "127.0.0.1:51002";

/// The revision secret these routers are built with; these routes hand out no
/// revision, the secret only keeps the builder whole.
const REVISION_KEY: [u8; 16] = [0; 16];

type Log = Arc<Mutex<Vec<String>>>;

/// The principal each connect was asked for.
type Principals = Arc<Mutex<Vec<Option<String>>>>;

/// What the fake manager answers for the two verbs.
#[derive(Clone)]
struct Fixture {
    /// What `create_model_profile` answers.
    create: Result<(), ClientError>,
    /// What `delete_model_profile` answers.
    delete: Result<(), ClientError>,
    /// How long either verb takes.
    delay: Duration,
}

impl Default for Fixture {
    fn default() -> Fixture {
        Fixture {
            create: Ok(()),
            delete: Ok(()),
            delay: Duration::ZERO,
        }
    }
}

/// A refusal of the manager's checks, as every transport carries it.
fn refusal(key: &'static str, values: &[&str]) -> ClientError {
    ClientError::Invalid(checks::Refusal {
        key,
        values: values.iter().map(|v| v.to_string()).collect(),
    })
}

/// A manager that knows the two verbs and logs every call: `create <id>
/// from=<from> values=<k=v,...>` (the source as its transport pair, the
/// manager's own expansion of the enum, values as it receives them) and
/// `delete-model <id>`.
struct FakeManager {
    fixture: Fixture,
    log: Log,
}

impl FakeManager {
    fn record(&self, call: String) {
        self.log
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .push(call);
    }
}

impl ManagerClient for FakeManager {
    fn list_instances(&self) -> Result<Vec<String>, ClientError> {
        Ok(vec!["default".to_string()])
    }
    fn status(&self, _id: &str) -> Result<InstanceState, ClientError> {
        Ok(InstanceState::Inactive)
    }
    fn start(&self, _id: &str) -> Result<(), ClientError> {
        Ok(())
    }
    fn stop(&self, _id: &str) -> Result<(), ClientError> {
        Ok(())
    }
    fn restart(&self, _id: &str) -> Result<(), ClientError> {
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
    fn write_file_if(
        &self,
        _name: &str,
        _contents: &str,
        _expected: &str,
    ) -> Result<(), ClientError> {
        Ok(())
    }
    fn boot_enabled(&self, _id: &str) -> Result<bool, ClientError> {
        Ok(false)
    }
    fn set_boot_enabled(&self, _id: &str, _enabled: bool) -> Result<(), ClientError> {
        Ok(())
    }
    fn authenticate(&self, _user: &str, _password: &str) -> Result<(bool, bool), ClientError> {
        Ok((true, true))
    }
    fn instances(&self) -> Result<Vec<InstanceInfo>, ClientError> {
        Ok(Vec::new())
    }
    fn create_model_profile(
        &self,
        profile: &str,
        source: &ModelProfileSource,
    ) -> Result<(), ClientError> {
        std::thread::sleep(self.fixture.delay);
        let (from, values) = source.as_source();
        let values: Vec<String> = values.iter().map(|(k, v)| format!("{k}={v}")).collect();
        self.record(format!(
            "create {profile} from={from} values={}",
            values.join(",")
        ));
        self.fixture.create.clone()
    }
    fn delete_model_profile(&self, profile: &str) -> Result<(), ClientError> {
        std::thread::sleep(self.fixture.delay);
        self.record(format!("delete-model {profile}"));
        self.fixture.delete.clone()
    }
}

fn principals_of(p: &Principals) -> Vec<Option<String>> {
    p.lock().unwrap_or_else(|e| e.into_inner()).clone()
}

/// A connect that counts the calls and records the principal each was asked
/// for, handing out `make()` every time.
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
    body: String,
}

impl Reply {
    fn json(&self) -> Value {
        serde_json::from_str(&self.body)
            .unwrap_or_else(|e| panic!("not JSON ({e}): {:?}", self.body))
    }

    /// An error answer: its status, and `{"key": key, "args": args}` as JSON.
    /// A verb the bff does not route answers a bare 405 with an empty body,
    /// so every one of these is red, not accidentally green.
    fn assert_error(&self, status: StatusCode, key: &str, args: &[&str]) {
        assert_eq!(self.status, status, "{}", self.body);
        assert_eq!(
            self.json(),
            json!({ "key": key, "args": args }),
            "{}",
            self.body
        );
    }

    /// The gate's answer: 401 `{"key": "session-required", "args": []}`.
    fn assert_session_required(&self) {
        self.assert_error(StatusCode::UNAUTHORIZED, "session-required", &[]);
    }

    /// 422 with the check's key, its args, and the field the form marks.
    fn assert_refused(&self, key: &str, args: &[&str], field: &str) {
        assert_eq!(
            self.status,
            StatusCode::UNPROCESSABLE_ENTITY,
            "{}",
            self.body
        );
        assert_eq!(
            self.json(),
            json!({ "key": key, "args": args, "field": field }),
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
}

/// A router over a fake manager, a fake authenticator and a clock, with the
/// log of the manager's calls, the count of connects and the principals each
/// was handed.
struct App {
    router: Router,
    log: Log,
    connects: Arc<AtomicUsize>,
    principals: Principals,
}

fn app_for(fixture: Fixture) -> App {
    app_for_write_timeout(fixture, WRITE_TIMEOUT)
}

/// A router with the write limit handed in, so a test can put a manager delay
/// between the read's limit and the write's.
fn app_for_write_timeout(fixture: Fixture, write_timeout: Duration) -> App {
    let log = Log::default();
    let connects = Arc::new(AtomicUsize::new(0));
    let principals = Principals::default();
    let l = log.clone();
    let connect = connect_logging(&connects, &principals, move || {
        Ok(Box::new(FakeManager {
            fixture: fixture.clone(),
            log: l.clone(),
        }) as Client)
    });
    let clock = FakeClock::new();
    let router = pai_bff::router_with_session_and_write_timeout(
        connect,
        TIMEOUT,
        write_timeout,
        Arc::new(FakeAuth),
        clock.clock(),
        REVISION_KEY,
    );
    App {
        router,
        log,
        connects,
        principals,
    }
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

    /// `GET /api/session`: the state of the session the cookie names.
    async fn session_of(&self, cookie: &str) -> Reply {
        self.get("/api/session", Some(cookie)).await
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
        self.ask_content_type(method, uri, cookie, body, Some("application/json"))
            .await
    }

    /// A request with a body and a chosen content type, `None` for no
    /// `Content-Type` header at all.
    async fn ask_content_type(
        &self,
        method: Method,
        uri: &str,
        cookie: Option<&str>,
        body: &str,
        content_type: Option<&str>,
    ) -> Reply {
        let mut builder = Request::builder().method(method).uri(uri);
        if let Some(c) = cookie {
            builder = builder.header(header::COOKIE, c);
        }
        if let Some(t) = content_type {
            builder = builder.header(header::CONTENT_TYPE, t);
        }
        let request = builder.body(Body::from(body.to_string())).expect("request");
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

    // ── The two requests, each signed in first ───────────────────────────

    /// Sign in and `POST /api/profiles/model` with `body`.
    async fn create(&self, body: Value) -> (Reply, String) {
        let cookie = self.sign_in().await;
        let reply = self
            .ask_json(
                Method::POST,
                "/api/profiles/model",
                Some(&cookie),
                &body.to_string(),
            )
            .await;
        (reply, cookie)
    }

    /// Sign in and `DELETE /api/profiles/model/{id}`.
    async fn delete(&self, id: &str) -> (Reply, String) {
        let cookie = self.sign_in().await;
        (
            self.ask(
                Method::DELETE,
                &format!("/api/profiles/model/{id}"),
                Some(&cookie),
            )
            .await,
            cookie,
        )
    }
}

// ── Failure paths first: the session gate of both routes ─────────────────

/// With no session, both routes answer 401 `session-required` and the manager
/// is never even connected to: the gate answers on its own.
#[tokio::test]
async fn both_routes_need_a_session() {
    let app = app_for(Fixture::default());
    let cookieless: Vec<(Method, String, Option<String>)> = vec![
        (
            Method::POST,
            "/api/profiles/model".into(),
            Some(json!({"id": "new", "from": "beta"}).to_string()),
        ),
        (Method::DELETE, "/api/profiles/model/beta".into(), None),
    ];
    for (method, uri, body) in cookieless {
        let reply = match body {
            Some(body) => app.ask_json(method.clone(), &uri, None, &body).await,
            None => app.ask(method.clone(), &uri, None).await,
        };
        reply.assert_session_required();
        app.assert_asked_nothing(&format!("{method} {uri} with no session"));
    }
}

/// The cookie of a session that ended is refused on both routes, and nothing
/// is asked of the manager: the pool is no hole in the sign-in page.
#[tokio::test]
async fn a_terminated_session_is_refused_on_both_routes() {
    let app = app_for(Fixture::default());
    let cookie = app.sign_in().await;
    let signed_out = app.ask(Method::DELETE, "/api/session", Some(&cookie)).await;
    assert_eq!(signed_out.status, StatusCode::NO_CONTENT, "signed out");
    for (method, uri) in [
        (Method::POST, "/api/profiles/model".to_string()),
        (Method::DELETE, "/api/profiles/model/beta".to_string()),
    ] {
        let before = app.calls().len();
        let reply = app.ask(method.clone(), &uri, Some(&cookie)).await;
        reply.assert_session_required();
        assert_eq!(
            app.calls().len(),
            before,
            "{method} {uri} asked the manager for a dead session"
        );
    }
}

// ── The successes ────────────────────────────────────────────────────────

/// `POST /api/profiles/model` `{"id", "from"}`: 201 `{"id"}` and the stub
/// received the clone: the id and the source's `from`, no values. The call is
/// made for the signed-in user, and the session lives on.
#[tokio::test]
async fn create_a_clone_answers_201_and_the_stub_received_the_source() {
    let app = app_for(Fixture::default());
    let (reply, cookie) = app.create(json!({"id": "new", "from": "beta"})).await;
    assert_eq!(reply.status, StatusCode::CREATED, "{}", reply.body);
    assert_eq!(reply.json(), json!({"id": "new"}), "{}", reply.body);
    assert_eq!(
        app.calls(),
        vec!["create new from=beta values=".to_string()],
        "the clone's call"
    );
    assert_eq!(
        principals_of(&app.principals),
        vec![Some("alice".to_string())],
        "the write acts for the signed-in user"
    );
    app.session_of(&cookie).await.assert_signed_in_as("alice");
}

/// The three sources: a `model` file, a Hugging Face repository with its
/// file, and a repository alone. Each answers 201 `{"id"}` and the stub
/// received the pick's keys.
#[tokio::test]
async fn create_from_a_pick_answers_201_with_the_pick_s_keys() {
    for (case, body, call) in [
        (
            "model",
            json!({"id": "new", "values": {"model": "/m/x.gguf"}}),
            "create new from= values=model=/m/x.gguf",
        ),
        (
            "hf-repo and hf-file",
            json!({"id": "new", "values": {"hf-repo": "r", "hf-file": "f.gguf"}}),
            "create new from= values=hf-repo=r,hf-file=f.gguf",
        ),
        (
            "hf-repo alone",
            json!({"id": "new", "values": {"hf-repo": "r"}}),
            "create new from= values=hf-repo=r",
        ),
    ] {
        let app = app_for(Fixture::default());
        let (reply, cookie) = app.create(body).await;
        assert_eq!(reply.status, StatusCode::CREATED, "{case}: {}", reply.body);
        assert_eq!(reply.json(), json!({"id": "new"}), "{case}: {}", reply.body);
        assert_eq!(app.calls(), vec![call.to_string()], "{case}: the call");
        app.session_of(&cookie).await.assert_signed_in_as("alice");
    }
}

/// The values are trimmed of spaces and tabs as the save does: the stub
/// received the trimmed text, not the padded one.
#[tokio::test]
async fn create_trims_spaces_and_tabs_off_the_values() {
    let app = app_for(Fixture::default());
    let (reply, _) = app
        .create(json!({"id": "new", "values": {"model": " \t/m/x.gguf\t "}}))
        .await;
    assert_eq!(reply.status, StatusCode::CREATED, "{}", reply.body);
    let (reply, _) = app
        .create(json!({"id": "two", "values": {"hf-repo": " r ", "hf-file": "\tf "}}))
        .await;
    assert_eq!(reply.status, StatusCode::CREATED, "{}", reply.body);
    assert_eq!(
        app.calls(),
        vec![
            "create new from= values=model=/m/x.gguf".to_string(),
            "create two from= values=hf-repo=r,hf-file=f".to_string(),
        ],
        "the trimmed values reached the manager"
    );
}

/// The name and the copy source are trimmed of spaces and tabs as the values
/// are: the stub received the trimmed text, and the 201 answers the trimmed id,
/// so a padded name is not a name of its own beside the trimmed one.
#[tokio::test]
async fn create_trims_spaces_and_tabs_off_the_id_and_from() {
    let app = app_for(Fixture::default());
    let (reply, _) = app
        .create(json!({"id": " \tnew\t ", "from": " \tbeta \t"}))
        .await;
    assert_eq!(reply.status, StatusCode::CREATED, "{}", reply.body);
    assert_eq!(
        reply.json(),
        json!({"id": "new"}),
        "the 201 names the trimmed id: {}",
        reply.body
    );
    assert_eq!(
        app.calls(),
        vec!["create new from=beta values=".to_string()],
        "the trimmed name and source reached the manager"
    );
}

/// `DELETE /api/profiles/model/{id}`: 204 with no body, the stub received the
/// delete for the signed-in user, and the session lives on.
#[tokio::test]
async fn delete_answers_204() {
    let app = app_for(Fixture::default());
    let (reply, cookie) = app.delete("beta").await;
    assert_eq!(reply.status, StatusCode::NO_CONTENT, "{}", reply.body);
    assert_eq!(reply.body, "", "204 carries no body");
    assert_eq!(app.calls(), vec!["delete-model beta".to_string()]);
    assert_eq!(
        principals_of(&app.principals),
        vec![Some("alice".to_string())],
        "the write acts for the signed-in user"
    );
    app.session_of(&cookie).await.assert_signed_in_as("alice");
}

// ── The 400s: a body the create cannot read, no manager asked ────────────

/// The bodies, each one thing broken from a well-shaped create, that are no
/// create at all: 400 `model-profiles-malformed`, and no manager call of any
/// kind.
#[tokio::test]
async fn create_body_that_is_no_create_is_400_and_reaches_no_manager() {
    let app = app_for(Fixture::default());
    let cookie = app.sign_in().await;
    for (case, body) in [
        ("no JSON at all", "{".to_string()),
        ("empty body", String::new()),
        ("not JSON", "not json at all".to_string()),
        ("an array", json!(["beta"]).to_string()),
        ("a string", json!("beta").to_string()),
        ("id a number", json!({"id": 7, "from": "beta"}).to_string()),
        ("no id beside from", json!({"from": "beta"}).to_string()),
        (
            "no id beside values",
            json!({"values": {"model": "/m/x.gguf"}}).to_string(),
        ),
        ("id empty", json!({"id": "", "from": "beta"}).to_string()),
        (
            "id spaces and tabs only",
            json!({"id": " \t ", "from": "beta"}).to_string(),
        ),
        ("from a number", json!({"id": "new", "from": 7}).to_string()),
        ("from empty", json!({"id": "new", "from": ""}).to_string()),
        (
            "from spaces and tabs only",
            json!({"id": "new", "from": " \t "}).to_string(),
        ),
        (
            "values a number",
            json!({"id": "new", "values": 7}).to_string(),
        ),
        (
            "values a string",
            json!({"id": "new", "values": "model"}).to_string(),
        ),
        (
            "a value that is no string",
            json!({"id": "new", "values": {"model": 7}}).to_string(),
        ),
        (
            "a value that is a bool",
            json!({"id": "new", "values": {"model": true}}).to_string(),
        ),
        (
            "a value empty",
            json!({"id": "new", "values": {"model": ""}}).to_string(),
        ),
        (
            "a value spaces and tabs only",
            json!({"id": "new", "values": {"model": " \t "}}).to_string(),
        ),
        (
            "hf-repo empty",
            json!({"id": "new", "values": {"hf-repo": ""}}).to_string(),
        ),
        (
            "hf-file spaces only beside hf-repo",
            json!({"id": "new", "values": {"hf-repo": "r", "hf-file": "  "}}).to_string(),
        ),
        (
            "an unknown field",
            json!({"id": "new", "from": "beta", "engine": "llama.cpp"}).to_string(),
        ),
        (
            "both from and values",
            json!({"id": "new", "from": "beta", "values": {"model": "/m/x.gguf"}}).to_string(),
        ),
        ("neither", json!({"id": "new"}).to_string()),
        (
            "values empty",
            json!({"id": "new", "values": {}}).to_string(),
        ),
        (
            "a values key outside the three",
            json!({"id": "new", "values": {"ctx-size": "8192"}}).to_string(),
        ),
        (
            "a values key in another case",
            json!({"id": "new", "values": {"Model": "/m/x.gguf"}}).to_string(),
        ),
        (
            "a key beside model",
            json!({"id": "new", "values": {"model": "/m/x.gguf", "ctx-size": "8192"}}).to_string(),
        ),
        (
            "model and hf-repo together",
            json!({"id": "new", "values": {"model": "/m/x.gguf", "hf-repo": "r"}}).to_string(),
        ),
        (
            "hf-file alone",
            json!({"id": "new", "values": {"hf-file": "f.gguf"}}).to_string(),
        ),
        (
            "a line feed in a value",
            json!({"id": "new", "values": {"model": "/m/x\ngguf"}}).to_string(),
        ),
        (
            "a carriage return in a value",
            json!({"id": "new", "values": {"model": "/m/x\rgguf"}}).to_string(),
        ),
        (
            "a NUL in a value",
            json!({"id": "new", "values": {"model": "/m/x\0gguf"}}).to_string(),
        ),
        (
            "a DEL in a value",
            json!({"id": "new", "values": {"model": "/m/x\x7fgguf"}}).to_string(),
        ),
        (
            "a line feed in the id",
            json!({"id": "ne\nw", "from": "beta"}).to_string(),
        ),
        (
            "a line feed in from",
            json!({"id": "new", "from": "be\nta"}).to_string(),
        ),
        (
            "a line feed in hf-repo",
            json!({"id": "new", "values": {"hf-repo": "r\nx"}}).to_string(),
        ),
        (
            "a line feed in hf-file",
            json!({"id": "new", "values": {"hf-repo": "r", "hf-file": "f\nf"}}).to_string(),
        ),
    ] {
        let reply = app
            .ask_json(Method::POST, "/api/profiles/model", Some(&cookie), &body)
            .await;
        reply.assert_error(StatusCode::BAD_REQUEST, "model-profiles-malformed", &[]);
        app.assert_asked_nothing(&format!("create with {case}"));
    }
}

/// The content type the body must carry: a signed-in request with a
/// well-shaped body and a type of another essence, and one with no
/// `Content-Type` at all, are the same 400 `model-profiles-malformed`, with
/// no manager asked.
#[tokio::test]
async fn create_a_body_whose_content_type_is_not_json_is_400() {
    let app = app_for(Fixture::default());
    let cookie = app.sign_in().await;
    let good = json!({"id": "new", "from": "beta"}).to_string();
    for (case, content_type) in [
        ("text/plain", Some("text/plain")),
        (
            "application/x-www-form-urlencoded",
            Some("application/x-www-form-urlencoded"),
        ),
        ("no Content-Type header", None),
    ] {
        let reply = app
            .ask_content_type(
                Method::POST,
                "/api/profiles/model",
                Some(&cookie),
                &good,
                content_type,
            )
            .await;
        reply.assert_error(StatusCode::BAD_REQUEST, "model-profiles-malformed", &[]);
        app.assert_asked_nothing(&format!("create with {case}"));
    }
}

// ── The manager's refusals: 422 with the field, 409 with the args ────────

/// Each refusal of the create arrives as the manager's and answers 422 with
/// the field the design marks, the call asked once, and the session living
/// on.
#[tokio::test]
async fn create_refusals_answer_422_with_the_field_the_design_marks() {
    for (key, args, field) in [
        ("invalid-model-profile-id", &["bad id"][..], "id"),
        ("comment-markers", &["model"], "model"),
        ("comment-markers", &["hf-repo"], "hf-repo"),
        ("no-model-profile-named", &["gamma"], "from"),
        ("model-profile-no-model", &["new"], "values"),
        ("model-profile-bad-source", &["new"], "values"),
    ] {
        let fixture = Fixture {
            create: Err(refusal(key, args)),
            ..Fixture::default()
        };
        let app = app_for(fixture);
        let (reply, cookie) = app.create(json!({"id": "new", "from": "gamma"})).await;
        reply.assert_refused(key, args, field);
        assert_eq!(
            app.calls(),
            vec!["create new from=gamma values=".to_string()],
            "{key}: the refusal is the manager's, the call was asked"
        );
        app.session_of(&cookie).await.assert_signed_in_as("alice");
    }
}

/// A tab inside the id is no name, and the bff judges no name: a tab is the one
/// control character the body's text rule allows, and trimming is only of the
/// ends, so the create travels to the manager, whose id check is the one that
/// refuses it: 422 `invalid-model-profile-id` marking `id`, with the tab inside
/// the id the stub received. A 400 here would be the bff judging a name.
#[tokio::test]
async fn create_an_id_with_a_tab_inside_is_the_managers_422_on_id() {
    let app = app_for(Fixture {
        create: Err(refusal("invalid-model-profile-id", &["a\tb"])),
        ..Fixture::default()
    });
    let (reply, cookie) = app.create(json!({"id": " a\tb ", "from": "beta"})).await;
    reply.assert_refused("invalid-model-profile-id", &["a\tb"], "id");
    assert_eq!(
        app.calls(),
        vec!["create a\tb from=beta values=".to_string()],
        "the id kept the tab inside it on the way to the manager"
    );
    app.session_of(&cookie).await.assert_signed_in_as("alice");
}

/// A name both pool files already hold: 409 `model-profile-name-taken`
/// naming it, with no `field`, and the session lives on.
#[tokio::test]
async fn create_a_taken_name_is_409_with_the_id() {
    let app = app_for(Fixture {
        create: Err(refusal("model-profile-name-taken", &["beta"])),
        ..Fixture::default()
    });
    let (reply, cookie) = app.create(json!({"id": "beta", "from": "gamma"})).await;
    reply.assert_error(StatusCode::CONFLICT, "model-profile-name-taken", &["beta"]);
    assert!(!reply.body.contains("field"), "no field: {}", reply.body);
    app.session_of(&cookie).await.assert_signed_in_as("alice");
}

/// The delete's refusals, relayed with their args: no such profile 404
/// `no-model-profile-named` `[id]`; a switched-on profile 409
/// `model-profile-enabled` `[id]`; an assigned one 409
/// `model-profile-assigned` naming the profile and the instances. Any other
/// check the manager runs on the way there is 422 with its own key and its
/// values and no field: a delete has no form field to mark. The session
/// lives on after each.
#[tokio::test]
async fn delete_refusals_answer_404_and_409_with_their_args() {
    for (key, args, status) in [
        (
            "no-model-profile-named",
            &["beta"][..],
            StatusCode::NOT_FOUND,
        ),
        ("model-profile-enabled", &["beta"], StatusCode::CONFLICT),
        (
            "model-profile-assigned",
            &["beta", "default,issac"],
            StatusCode::CONFLICT,
        ),
        // A check that is none of the delete's own three: the refusal's key and
        // values travel, with no field to mark.
        (
            "model-profile-bad-text",
            &["model"],
            StatusCode::UNPROCESSABLE_ENTITY,
        ),
    ] {
        let app = app_for(Fixture {
            delete: Err(refusal(key, args)),
            ..Fixture::default()
        });
        let (reply, cookie) = app.delete("beta").await;
        reply.assert_error(status, key, args);
        assert_eq!(
            app.calls(),
            vec!["delete-model beta".to_string()],
            "{key}: the refusal is the manager's, the call was asked"
        );
        app.session_of(&cookie).await.assert_signed_in_as("alice");
    }
}

// ── The manager refuses the user, and the answers that are the bff's news ─

/// The manager refused the signed-in user: 403 `session-not-administrator` on
/// both routes, and the session ends with the right that authorized it, so
/// the same cookie is 401 after it.
#[tokio::test]
async fn a_refusal_of_the_user_is_403_and_ends_the_session_on_both_routes() {
    for route in ["create", "delete"] {
        let fixture = Fixture {
            create: Err(ClientError::NotAuthorized),
            delete: Err(ClientError::NotAuthorized),
            ..Fixture::default()
        };
        let app = app_for(fixture);
        let (reply, cookie) = match route {
            "create" => app.create(json!({"id": "new", "from": "beta"})).await,
            _ => app.delete("beta").await,
        };
        reply.assert_error(StatusCode::FORBIDDEN, "session-not-administrator", &[]);
        let who = app.session_of(&cookie).await;
        assert_eq!(
            who.status,
            StatusCode::UNAUTHORIZED,
            "a session lived on after a refusal of the {route}: {}",
            who.body
        );
    }
}

/// No manager is 503 `client-unavailable` on both routes: nothing reached the
/// manager, so the page can be told nothing changed. A manager that takes the
/// call and does not answer in the bff's time is 504 `client-timeout`: the
/// write may have landed (slice W1, `tests/write_timeouts.rs`).
#[tokio::test]
async fn no_manager_is_unavailable_and_a_slow_one_times_out_on_both_routes() {
    let gone = Fixture {
        create: Err(ClientError::Unavailable),
        delete: Err(ClientError::Unavailable),
        ..Fixture::default()
    };
    let app = app_for(gone);
    app.create(json!({"id": "new", "from": "beta"}))
        .await
        .0
        .assert_error(StatusCode::SERVICE_UNAVAILABLE, "client-unavailable", &[]);
    app.delete("beta").await.0.assert_error(
        StatusCode::SERVICE_UNAVAILABLE,
        "client-unavailable",
        &[],
    );

    let slow = app_for(Fixture {
        delay: Duration::from_secs(2),
        ..Fixture::default()
    });
    slow.create(json!({"id": "new", "from": "beta"}))
        .await
        .0
        .assert_error(StatusCode::GATEWAY_TIMEOUT, "client-timeout", &[]);
    slow.delete("beta")
        .await
        .0
        .assert_error(StatusCode::GATEWAY_TIMEOUT, "client-timeout", &[]);
}

/// A create and a removal are writes, so the manager is given the write's own,
/// longer limit for them, as for the save and the switch: a manager that
/// answers past `TIMEOUT`, the limit that fits a read, and inside
/// `WRITE_TIMEOUT`, is not cut off. The create answers 201 and the removal 204,
/// and both calls reached the stub. A route handed `timeout` answers 503
/// `client-unavailable` on these two requests.
#[tokio::test]
async fn a_write_that_outlasts_the_read_limit_is_not_cut_off() {
    let app = app_for_write_timeout(
        Fixture {
            delay: Duration::from_millis(350),
            ..Fixture::default()
        },
        WRITE_TIMEOUT,
    );
    let (reply, cookie) = app.create(json!({"id": "new", "from": "beta"})).await;
    assert_eq!(reply.status, StatusCode::CREATED, "{}", reply.body);
    assert_eq!(reply.json(), json!({"id": "new"}), "{}", reply.body);
    let (reply, _) = app.delete("beta").await;
    assert_eq!(reply.status, StatusCode::NO_CONTENT, "{}", reply.body);
    assert_eq!(
        app.calls(),
        vec![
            "create new from=beta values=".to_string(),
            "delete-model beta".to_string()
        ],
        "both writes reached the manager"
    );
    app.session_of(&cookie).await.assert_signed_in_as("alice");
}

/// Any other failure is 502 `client-other`, with the manager's detail left
/// out of the body, and the session lives on.
#[tokio::test]
async fn any_other_failure_is_failed_without_the_detail() {
    let app = app_for(Fixture {
        create: Err(ClientError::Other("the pipe went away".into())),
        delete: Err(ClientError::Other("the pipe went away".into())),
        ..Fixture::default()
    });
    for (reply, cookie) in [
        app.create(json!({"id": "new", "from": "beta"})).await,
        app.delete("beta").await,
    ] {
        reply.assert_error(StatusCode::BAD_GATEWAY, "client-other", &[]);
        assert!(
            !reply.body.contains("pipe"),
            "the manager's detail is in the body: {}",
            reply.body
        );
        app.session_of(&cookie).await.assert_signed_in_as("alice");
    }
}
