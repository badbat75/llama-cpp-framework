//! `GET /api/profiles/model/{id}/model-info`: the layer facts of one model
//! profile's model (L2 of "Layers, MoE and tensor placement on the web model
//! profile page", `ARCHITECTURE.md` 1 "The bff route"), relayed from the
//! manager's `ModelInfoAs` as the signed-in user, through
//! `pai_bff::router_with_session` with a fake manager client and a fake
//! authenticator: never the bus, pipe, PAM, LogonUser, a GGUF file or an
//! engine. Every request runs under a deadline, so a bff that waits on the
//! manager fails instead of hanging.
//!
//! The contract:
//! - A session is needed: with none the answer is 401 `session-required`, and
//!   the manager is never connected to, so nothing is read.
//! - The read is asked as the signed-in user (`ModelInfoAs`, the connect
//!   naming that user), with the id of the path as the page spelled it, and one
//!   call answers the page.
//! - 200 is the manager's reply, in camelCase, exactly three fields:
//!   `{"model": <abs path>|null, "info": {"blockCount", "moe", "expertCount",
//!   "expertUsed"}|null, "reason": {"key", "args"}|null}`. `model` is set only
//!   with `info`; every reason row answers `model` null, with the path, when
//!   there is one, as the reason's arg. The counts are numbers, `0` included.
//! - The manager's refusal of the signed-in user is 403
//!   `session-not-administrator` and ends that session, as every other `As`
//!   read; an id with no section is 404 `no-model-profile-named` `[id]`; no
//!   manager, and an answer over the read's deadline, are 503
//!   `client-unavailable`; anything else is 502 `client-other`, with the
//!   manager's detail kept out of the body.
//! - It moves the session's idle deadline: the page asks it when it opens and
//!   after a save that changes `model`, and a page opened late keeps the
//!   sign-in alive.

use axum::body::Body;
use axum::extract::ConnectInfo;
use axum::http::{header, Method, Request, StatusCode};
use axum::Router;
use http_body_util::BodyExt;
use pai_bff::session::{AuthError, Authenticator, Identity};
use pai_bff::{Client, Connect};
use pai_core::checks::Refusal;
use pai_core::client::{
    ClientError, InstanceState, ManagerClient, ManagerFile, ModelFacts, ModelInfoReply, Reason,
};
use serde_json::{json, Value};
use std::net::SocketAddr;
use std::sync::atomic::{AtomicU64, AtomicUsize, Ordering};
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

// ── The manager's answers ───────────────────────────────────────────────

/// The facts of a Mixture of Experts model: the counts the GGUF header spells,
/// and the resolved path the manager read them from.
fn moe_facts() -> ModelInfoReply {
    ModelInfoReply {
        model: Some("/var/lib/llama.cpp/models/Qwen3-27B-Q8_0.gguf".into()),
        info: Some(ModelFacts {
            block_count: 64,
            moe: true,
            expert_count: 128,
            expert_used: 8,
        }),
        reason: None,
    }
}

/// The document of [`moe_facts`], in the camelCase of the design's reply.
fn moe_facts_document() -> Value {
    json!({
        "model": "/var/lib/llama.cpp/models/Qwen3-27B-Q8_0.gguf",
        "info": {"blockCount": 64, "moe": true, "expertCount": 128, "expertUsed": 8},
        "reason": null,
    })
}

/// Why a read that worked has no facts: the key and its args, with no model
/// and no info beside them.
fn reason(key: &str, args: &[&str]) -> ModelInfoReply {
    ModelInfoReply {
        model: None,
        info: None,
        reason: Some(Reason {
            key: key.to_string(),
            args: args.iter().map(|a| a.to_string()).collect(),
        }),
    }
}

/// The document of [`reason`].
fn reason_document(key: &str, args: &[&str]) -> Value {
    json!({"model": null, "info": null, "reason": {"key": key, "args": args}})
}

// ── Fakes ───────────────────────────────────────────────────────────────

/// A manager that records every call and answers `model_info` with what the
/// test set, after the delay it set. The route asks `model_info` and nothing
/// else: every other verb answers what a model info read never touches.
struct FakeManager {
    log: Log,
    /// What `model_info` answers.
    answer: Arc<Mutex<Result<ModelInfoReply, ClientError>>>,
    /// How long `model_info` takes before it answers, in milliseconds.
    pause_ms: Arc<AtomicU64>,
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
    fn model_info(&self, profile: &str) -> Result<ModelInfoReply, ClientError> {
        self.record(format!("model_info {profile}"));
        let pause = self.pause_ms.load(Ordering::SeqCst);
        if pause > 0 {
            // The read runs in the route's blocking task, so this holds that
            // task and not the route.
            std::thread::sleep(Duration::from_millis(pause));
        }
        self.answer
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .clone()
    }
}

/// The authenticator: `alice` with the one password, and `root` too (so a test
/// can show a refusal ends one session and not every session).
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
    answer: Arc<Mutex<Result<ModelInfoReply, ClientError>>>,
    pause_ms: Arc<AtomicU64>,
}

impl Fixture {
    /// The facts of [`moe_facts`].
    fn facts() -> Fixture {
        Fixture {
            answer: Arc::new(Mutex::new(Ok(moe_facts()))),
            pause_ms: Arc::new(AtomicU64::new(0)),
        }
    }
    /// The answer, set by a test (a reason, a refusal, a failure).
    fn set(&self, answer: Result<ModelInfoReply, ClientError>) {
        *self.answer.lock().unwrap_or_else(|e| e.into_inner()) = answer;
    }
    /// How long `model_info` takes, in milliseconds.
    fn set_pause_ms(&self, ms: u64) {
        self.pause_ms.store(ms, Ordering::SeqCst);
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
    let (answer, pause_ms) = (fixture.answer.clone(), fixture.pause_ms.clone());
    let connect: Connect = Arc::new(move |principal: Option<&str>| {
        c.fetch_add(1, Ordering::SeqCst);
        p.lock()
            .unwrap_or_else(|e| e.into_inner())
            .push(principal.map(str::to_string));
        Ok(Box::new(FakeManager {
            log: l.clone(),
            answer: answer.clone(),
            pause_ms: pause_ms.clone(),
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

    /// None of the manager's own words in the body.
    fn assert_no_manager_detail(&self) {
        assert!(
            !self.body.contains("renderD128"),
            "the manager's words stay out of the answer: {}",
            self.body
        );
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

    /// `GET /api/profiles/model/{id}/model-info`, the route under test, with
    /// `id` sent as the page spells it (`encoded` is the percent-encoding of a
    /// name with a space).
    async fn model_info(&self, id: &str, cookie: Option<&str>) -> Reply {
        self.get(&format!("/api/profiles/model/{id}/model-info"), cookie)
            .await
    }

    async fn get(&self, uri: &str, cookie: Option<&str>) -> Reply {
        let mut builder = Request::builder().method(Method::GET).uri(uri);
        if let Some(c) = cookie {
            builder = builder.header(header::COOKIE, c);
        }
        let request = builder.body(Body::empty()).expect("request");
        let response = tokio::time::timeout(DEADLINE, self.router.clone().oneshot(request))
            .await
            .unwrap_or_else(|_| panic!("no answer to {uri} within {DEADLINE:?}"))
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
/// a visitor reads no model file, and no GGUF header.
#[tokio::test]
async fn model_info_without_a_session_is_401_and_the_manager_is_not_asked() {
    let app = app(Fixture::facts());
    let rep = app.model_info("alpha", None).await;
    rep.assert_session_required();
    app.assert_asked_nothing("no session");
}

/// The cookie of a session that ended asks nothing.
#[tokio::test]
async fn model_info_of_a_session_that_ended_is_401_and_asks_nothing() {
    let app = app(Fixture::facts());
    let cookie = app.sign_in().await;
    app.clock.advance(PAST);
    let rep = app.model_info("alpha", Some(&cookie)).await;
    rep.assert_session_required();
    assert_eq!(
        app.calls(),
        Vec::<String>::new(),
        "an ended session asks no manager"
    );
}

// ── The three shapes of the reply ───────────────────────────────────────

/// The facts row: the manager's `model` and the four counts, camelCase, with
/// `reason` null, asked as the signed-in user in one call and nothing else.
#[tokio::test]
async fn the_facts_row_is_relayed_in_camel_case() {
    let app = app(Fixture::facts());
    let cookie = app.sign_in().await;
    let rep = app.model_info("Qwen3-27B", Some(&cookie)).await;
    assert_eq!(rep.status, StatusCode::OK, "{}", rep.body);
    assert_eq!(rep.json(), moe_facts_document(), "the design's document");
    assert_eq!(
        app.calls(),
        vec!["model_info Qwen3-27B".to_string()],
        "one call, and the page asks nothing else"
    );
    assert_eq!(
        app.principals(),
        vec![Some("alice".to_string())],
        "the facts are read as the signed-in user, the As form"
    );
}

/// A dense model has no experts: `moe` false and both expert counts `0` travel
/// as numbers, not as null and not left out, so the page hides the expert
/// slider and shows a real 0 on the others.
#[tokio::test]
async fn a_dense_model_answers_moe_false_and_zero_experts_as_numbers() {
    let app = app(Fixture::facts());
    app.fixture.set(Ok(ModelInfoReply {
        model: Some("/models/llama-8b.f16.gguf".into()),
        info: Some(ModelFacts {
            block_count: 32,
            moe: false,
            expert_count: 0,
            expert_used: 0,
        }),
        reason: None,
    }));
    let cookie = app.sign_in().await;
    let rep = app.model_info("llama-8b", Some(&cookie)).await;
    assert_eq!(rep.status, StatusCode::OK, "{}", rep.body);
    assert_eq!(
        rep.json(),
        json!({
            "model": "/models/llama-8b.f16.gguf",
            "info": {"blockCount": 32, "moe": false, "expertCount": 0, "expertUsed": 0},
            "reason": null,
        }),
        "{}",
        rep.body
    );
}

/// A reason row answers no model and no info: the page shows the reason and no
/// sliders. Each of the four keys travels with its own args, a path included.
#[tokio::test]
async fn every_reason_key_is_relayed_with_its_args_and_no_model() {
    for (key, args) in [
        ("model-info-fetched", vec!["hf-repo"]),
        ("model-profile-no-model", vec!["alpha"]),
        (
            "model-info-missing",
            vec!["/var/lib/llama.cpp/models/alpha-00003-of-00042.gguf"],
        ),
        (
            "model-info-unreadable",
            vec!["/var/lib/llama.cpp/models/a b/ünreadable.gguf"],
        ),
    ] {
        let app = app(Fixture::facts());
        app.fixture.set(Ok(reason(key, &args)));
        let cookie = app.sign_in().await;
        let rep = app.model_info("alpha", Some(&cookie)).await;
        assert_eq!(rep.status, StatusCode::OK, "{key}: {}", rep.body);
        assert_eq!(
            rep.json(),
            reason_document(key, &args),
            "{key}: {}",
            rep.body
        );
    }
}

/// A read that answered nothing at all, with no reason either, is relayed as
/// the three nulls: the page has no facts and no reason to show, and the bff
/// invents neither.
#[tokio::test]
async fn an_empty_reply_is_three_nulls() {
    let app = app(Fixture::facts());
    app.fixture.set(Ok(ModelInfoReply {
        model: None,
        info: None,
        reason: None,
    }));
    let cookie = app.sign_in().await;
    let rep = app.model_info("alpha", Some(&cookie)).await;
    assert_eq!(rep.status, StatusCode::OK, "{}", rep.body);
    assert_eq!(
        rep.json(),
        json!({"model": null, "info": null, "reason": null}),
        "{}",
        rep.body
    );
}

/// The reply carries exactly the three fields, and the counts exactly the
/// four: the bff adds nothing, renames nothing and judges nothing, so a shape
/// the fe reads today keeps being the shape it reads.
#[tokio::test]
async fn the_reply_is_the_managers_fields_and_nothing_else() {
    let app = app(Fixture::facts());
    let cookie = app.sign_in().await;
    let rep = app.model_info("Qwen3-27B", Some(&cookie)).await;
    let doc = rep.json();
    let mut keys: Vec<&str> = doc
        .as_object()
        .expect("object")
        .keys()
        .map(|k| k.as_ref())
        .collect();
    keys.sort();
    assert_eq!(keys, ["info", "model", "reason"], "{}", rep.body);
    let mut info: Vec<&str> = doc["info"]
        .as_object()
        .expect("info object")
        .keys()
        .map(|k| k.as_ref())
        .collect();
    info.sort();
    assert_eq!(
        info,
        ["blockCount", "expertCount", "expertUsed", "moe"],
        "{}",
        rep.body
    );
}

/// The id of the path is what the manager is asked for, as the page spelled it
/// (a name with a space, percent-encoded on the wire, decoded here), and a
/// name in another letter case is sent unchanged: the manager matches it
/// without case, the bff judges nothing.
#[tokio::test]
async fn the_id_of_the_path_is_the_id_the_manager_is_asked_for() {
    for (sent, asked) in [
        ("Alpha%20One", "Alpha One"),
        ("ALPHA", "ALPHA"),
        ("qwen3.8-27b", "qwen3.8-27b"),
    ] {
        let app = app(Fixture::facts());
        let cookie = app.sign_in().await;
        let rep = app.model_info(sent, Some(&cookie)).await;
        assert_eq!(rep.status, StatusCode::OK, "{sent}: {}", rep.body);
        assert_eq!(
            app.calls(),
            vec![format!("model_info {asked}")],
            "the id travels as the page spelled it, decoded once"
        );
    }
}

// ── The refusals and the failures ───────────────────────────────────────

/// No section of that name, and `*`: 404 `no-model-profile-named` with the id,
/// as every other `As` read answers that keyed refusal, and the session lives
/// on: a typo is not a lost sign-in.
#[tokio::test]
async fn an_unknown_model_profile_is_404_and_the_session_lives_on() {
    for (id, asked) in [("gamma", "gamma"), ("*", "*")] {
        let app = app(Fixture::facts());
        app.fixture.set(Err(ClientError::Invalid(Refusal {
            key: "no-model-profile-named",
            values: vec![asked.to_string()],
        })));
        let cookie = app.sign_in().await;
        let rep = app.model_info(id, Some(&cookie)).await;
        rep.assert_error(
            StatusCode::NOT_FOUND,
            "no-model-profile-named",
            &[asked.to_string().as_str()],
        );
        assert_eq!(app.calls(), vec![format!("model_info {asked}")]);
        app.session_of(&cookie).await.assert_signed_in_as("alice");
    }
}

/// The manager refuses the signed-in user (removed from `pai`): 403
/// `session-not-administrator`, and the session ends, as every other route's
/// refusal does.
#[tokio::test]
async fn model_info_refused_ends_the_session() {
    let app = app(Fixture::facts());
    app.fixture.set(Err(ClientError::NotAuthorized));
    let other = app.sign_in_as("root").await;
    let cookie = app.sign_in().await;
    let rep = app.model_info("alpha", Some(&cookie)).await;
    rep.assert_error(StatusCode::FORBIDDEN, "session-not-administrator", &[]);
    assert_eq!(
        app.calls(),
        vec!["model_info alpha".to_string()],
        "the refusal is the read's own"
    );
    app.session_of(&cookie).await.assert_session_required();
    app.session_of(&other).await.assert_signed_in_as("root");
}

/// No manager is 503 `client-unavailable`, as the other reads.
#[tokio::test]
async fn model_info_with_no_manager_is_503() {
    let app = app(Fixture::facts());
    app.fixture.set(Err(ClientError::Unavailable));
    let cookie = app.sign_in().await;
    let rep = app.model_info("alpha", Some(&cookie)).await;
    rep.assert_error(StatusCode::SERVICE_UNAVAILABLE, "client-unavailable", &[]);
    rep.assert_no_manager_detail();
    assert_eq!(app.calls(), vec!["model_info alpha".to_string()]);
}

/// A manager that answers over the read's deadline is the same 503, answered
/// at the limit and not at the deadline, so the page is never left waiting for
/// a header read.
#[tokio::test]
async fn model_info_answered_past_the_deadline_is_503() {
    let app = app(Fixture::facts());
    let pause = Duration::from_millis(2000);
    app.fixture.set_pause_ms(pause.as_millis() as u64);
    let cookie = app.sign_in().await;
    let started = Instant::now();
    let rep = app.model_info("alpha", Some(&cookie)).await;
    rep.assert_error(StatusCode::SERVICE_UNAVAILABLE, "client-unavailable", &[]);
    let took = started.elapsed();
    assert!(
        took >= TIMEOUT && took < pause,
        "answered at the read's limit, not at the deadline nor when the manager spoke: {took:?}"
    );
}

/// Anything else is 502 `client-other`, never the manager's own words, and the
/// session lives on: the failure is the manager's, not a sign about the user.
#[tokio::test]
async fn model_info_of_a_manager_that_failed_is_502_without_its_detail() {
    let app = app(Fixture::facts());
    app.fixture.set(Err(ClientError::Other(
        "model: cannot open /dev/dri/renderD128: Input/output error".into(),
    )));
    let cookie = app.sign_in().await;
    let rep = app.model_info("alpha", Some(&cookie)).await;
    rep.assert_error(StatusCode::BAD_GATEWAY, "client-other", &[]);
    rep.assert_no_manager_detail();
    app.session_of(&cookie).await.assert_signed_in_as("alice");
}

// ── The session's idle deadline ─────────────────────────────────────────

/// A page read, not a poll: the read moves the session's idle deadline, so a
/// model profile page opened late keeps the sign-in alive. The steps tell the
/// two gates of `session.rs` apart: after two `SHORT` steps the clock is past
/// the deadline a `peek` route leaves where it was (the sign-in's), and inside
/// the one this read moved, so only a `current` route is served there; one
/// `PAST` after the last answer the moved deadline is over, so the route moved
/// it, it did not clear it.
#[tokio::test]
async fn model_info_moves_the_sessions_idle_deadline() {
    let app = app(Fixture::facts());
    let cookie = app.sign_in().await;
    app.clock.advance(SHORT);
    let rep = app.model_info("alpha", Some(&cookie)).await;
    assert_eq!(rep.status, StatusCode::OK, "late: {}", rep.body);
    // Past the deadline of the sign-in, inside the one this read moved.
    app.clock.advance(SHORT);
    let rep = app.model_info("alpha", Some(&cookie)).await;
    assert_eq!(
        rep.status,
        StatusCode::OK,
        "two SHORT steps are past the deadline of a read that leaves it alone: {}",
        rep.body
    );
    // Past the deadline of the read that just answered: the session ended.
    app.clock.advance(PAST);
    let rep = app.model_info("alpha", Some(&cookie)).await;
    rep.assert_session_required();
    assert_eq!(
        app.calls(),
        vec![
            "model_info alpha".to_string(),
            "model_info alpha".to_string()
        ],
        "the ended session asks the manager nothing"
    );
}
