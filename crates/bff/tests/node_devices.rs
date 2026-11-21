//! `GET /api/node/devices`: the node's GPU distribution list (G2,
//! `ARCHITECTURE.md` 1 "The bff route"), through `pai_bff::router_with_session`
//! with a fake manager client and a fake authenticator, never the bus, pipe,
//! PAM, LogonUser or an engine. Every request runs under a deadline, so a bff
//! that waits on the manager fails instead of hanging.
//!
//! The contract:
//! - A session is needed: with none the answer is 401 `session-required`, no
//!   `Set-Cookie`, and the manager is never connected to.
//! - It is a page read, not a poll: `Sessions::current` is asked, so the idle
//!   deadline moves (the model profile page fetches it once when it opens,
//!   never polls it).
//! - The list is asked with `DevicesAs` as the signed-in user: the connect
//!   names that user, and one call answers the page.
//! - 200 `{"probedAt": <RFC 3339 UTC>|null, "ramMib": <int>, "cpuThreads":
//!   <int>, "devices": [{"id", "name", "backend", "totalMib", "freeMib"}]}`,
//!   in the probe's order, the CPU
//!   pseudo-device already left out by the manager. A node with no probe answer
//!   is `{"probedAt": null, "devices": []}`, a CPU-only node a time with an
//!   empty list: nothing to distribute, never an error page.
//! - The manager's refusal of the signed-in user is 403
//!   `session-not-administrator` and ends that session; no manager is 503
//!   `client-unavailable`, as the other reads, and so is an answer that does
//!   not land within the route's deadline (`DEVICES_TIMEOUT`, longer than a
//!   read's for the cold probe); anything else 502 `client-other`, without the
//!   manager's detail.

use axum::body::Body;
use axum::extract::ConnectInfo;
use axum::http::{header, Method, Request, StatusCode};
use axum::Router;
use http_body_util::BodyExt;
use pai_bff::session::{AuthError, Authenticator, Identity};
use pai_bff::{Client, Connect};
use pai_core::client::{
    ClientError, DeviceInfo, InstanceState, ManagerClient, ManagerFile, NodeDevices,
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

const CLIENT: &str = "127.0.0.1:51002";

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

/// The moment of the fake's probe, and the UTC text the route answers for it.
const PROBED_AT: u64 = 1_795_166_043;
const PROBED_AT_TEXT: &str = "2026-11-20T09:14:03Z";

type Log = Arc<Mutex<Vec<String>>>;

/// The host's inventory the fake answers beside the list (R1): numbers no real
/// host has, so a document carrying them carries the fake's answer.
const RAM_MIB: u64 = 3_932_160;
const CPU_THREADS: u32 = 8;

/// The device list the fake answers: three entries in the probe's order, the
/// CUDA/Vulkan twin of one card kept, the design's own rows.
fn probed() -> NodeDevices {
    NodeDevices {
        probed_at: Some(PROBED_AT),
        ram_mib: RAM_MIB,
        cpu_threads: CPU_THREADS,
        devices: [
            (
                "ROCm0",
                "AMD Radeon AI PRO R9700",
                "ROCm",
                32624u64,
                32420u64,
            ),
            (
                "CUDA0",
                "NVIDIA GeForce RTX 4070 SUPER",
                "CUDA",
                12281u64,
                10844u64,
            ),
            (
                "Vulkan0",
                "AMD Radeon AI PRO R9700",
                "Vulkan",
                32624u64,
                32420u64,
            ),
        ]
        .iter()
        .map(|(id, name, backend, total, free)| DeviceInfo {
            id: (*id).to_string(),
            name: (*name).to_string(),
            backend: (*backend).to_string(),
            total_mib: *total,
            free_mib: *free,
        })
        .collect(),
    }
}

/// The JSON of [`probed`], as the design's page reads it.
fn listed() -> Value {
    json!({
        "probedAt": PROBED_AT_TEXT,
        "ramMib": RAM_MIB,
        "cpuThreads": CPU_THREADS,
        "devices": [
            {"id": "ROCm0", "name": "AMD Radeon AI PRO R9700", "backend": "ROCm", "totalMib": 32624, "freeMib": 32420},
            {"id": "CUDA0", "name": "NVIDIA GeForce RTX 4070 SUPER", "backend": "CUDA", "totalMib": 12281, "freeMib": 10844},
            {"id": "Vulkan0", "name": "AMD Radeon AI PRO R9700", "backend": "Vulkan", "totalMib": 32624, "freeMib": 32420},
        ]
    })
}

// ── Fakes ───────────────────────────────────────────────────────────────

/// A manager that records every call and answers the device list, or what the
/// test set. The route asks `devices` and nothing else: every other verb
/// answers what a devices read never touches.
struct FakeManager {
    log: Log,
    /// What `devices` answers.
    answer: Arc<Mutex<Result<NodeDevices, ClientError>>>,
    /// How long `devices` takes before it answers.
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
    fn devices(&self) -> Result<NodeDevices, ClientError> {
        self.record("devices".into());
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
    answer: Arc<Mutex<Result<NodeDevices, ClientError>>>,
    delay: Arc<Mutex<Duration>>,
}

impl Fixture {
    /// The list of [`probed`].
    fn probed() -> Fixture {
        Fixture {
            answer: Arc::new(Mutex::new(Ok(probed()))),
            delay: Arc::new(Mutex::new(Duration::ZERO)),
        }
    }
    /// No probe answer: no time, no device. The host's inventory is answered
    /// all the same: it is the host's, not the engine's (R1).
    fn nothing() -> Fixture {
        Fixture {
            answer: Arc::new(Mutex::new(Ok(NodeDevices {
                probed_at: None,
                ram_mib: RAM_MIB,
                cpu_threads: CPU_THREADS,
                devices: vec![],
            }))),
            delay: Arc::new(Mutex::new(Duration::ZERO)),
        }
    }
    /// A node whose probe parsed only the CPU line: a time, no device.
    fn cpu_only() -> Fixture {
        Fixture {
            answer: Arc::new(Mutex::new(Ok(NodeDevices {
                probed_at: Some(PROBED_AT),
                ram_mib: RAM_MIB,
                cpu_threads: CPU_THREADS,
                devices: vec![],
            }))),
            delay: Arc::new(Mutex::new(Duration::ZERO)),
        }
    }
    /// The answer, set by a test (a refusal, a failure).
    fn set(&self, answer: Result<NodeDevices, ClientError>) {
        *self.answer.lock().unwrap_or_else(|e| e.into_inner()) = answer;
    }
    /// How long `devices` takes.
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

    /// `GET /api/node/devices`, the route under test.
    async fn devices(&self, cookie: Option<&str>) -> Reply {
        self.get_within(DEADLINE, "/api/node/devices", cookie).await
    }

    async fn get(&self, uri: &str, cookie: Option<&str>) -> Reply {
        self.get_within(DEADLINE, uri, cookie).await
    }

    /// A `GET`, answered within `limit`: the slow-probe test gives the route
    /// more than [`DEADLINE`], because the route's own bound is the probe's.
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

/// No session: 401 `session-required`, no cookie set, and the manager is never
/// connected to. The route sits behind the gate like every read beside it.
#[tokio::test]
async fn devices_without_a_session_is_401_and_the_manager_is_not_asked() {
    let app = app(Fixture::probed());
    let rep = app.devices(None).await;
    rep.assert_session_required();
    app.assert_asked_nothing("no session");
}

/// The cookie of a session that ended is the gate's answer too, and asking it
/// asks the manager nothing.
#[tokio::test]
async fn devices_of_a_session_that_ended_is_401_and_asks_nothing() {
    let app = app(Fixture::probed());
    let cookie = app.sign_in().await;
    app.clock.advance(Duration::from_secs(30 * 60 + 1));
    let rep = app.devices(Some(&cookie)).await;
    rep.assert_session_required();
    assert_eq!(
        app.calls(),
        Vec::<String>::new(),
        "an ended session asks no manager"
    );
}

// ── The list ────────────────────────────────────────────────────────────

/// The probe's rows, in the probe's order, with the probe's moment as RFC 3339
/// UTC, asked as the signed-in user (`DevicesAs`), in one call.
#[tokio::test]
async fn devices_answers_the_probe_rows_in_the_probe_order() {
    let app = app(Fixture::probed());
    let cookie = app.sign_in().await;
    let rep = app.devices(Some(&cookie)).await;
    assert_eq!(rep.status, StatusCode::OK, "{}", rep.body);
    assert_eq!(rep.json(), listed(), "the design's document");
    assert_eq!(
        app.calls(),
        vec!["devices".to_string()],
        "one call, and the page asks nothing else"
    );
    assert_eq!(
        app.principals(),
        vec![Some("alice".to_string())],
        "the list is read as the signed-in user, the As form"
    );
}

/// No probe answer is a 200 with no time and an empty list: a node with no
/// engine, a probe that was killed or printed nothing. The page has nothing to
/// distribute, and shows no error. The inventory is in the document all the
/// same (R1): what the node is does not depend on the engine answering.
#[tokio::test]
async fn devices_of_no_probe_answer_is_no_time_and_an_empty_list() {
    let app = app(Fixture::nothing());
    let cookie = app.sign_in().await;
    let rep = app.devices(Some(&cookie)).await;
    assert_eq!(rep.status, StatusCode::OK, "{}", rep.body);
    assert_eq!(
        rep.json(),
        json!({"probedAt": null, "ramMib": RAM_MIB, "cpuThreads": CPU_THREADS, "devices": []})
    );
}

/// A node whose probe parsed only the CPU line has a time and an empty list:
/// the CPU pseudo-device is never a device to distribute over, and the time
/// says the node was read.
#[tokio::test]
async fn devices_of_a_cpu_only_node_is_a_time_and_an_empty_list() {
    let app = app(Fixture::cpu_only());
    let cookie = app.sign_in().await;
    let rep = app.devices(Some(&cookie)).await;
    assert_eq!(rep.status, StatusCode::OK, "{}", rep.body);
    assert_eq!(
        rep.json(),
        json!({"probedAt": PROBED_AT_TEXT, "ramMib": RAM_MIB, "cpuThreads": CPU_THREADS, "devices": []})
    );
}

/// The list travels as the manager answered it, ids and names included: the
/// page shows the ids a profile names that the list lacks, so the bff keeps
/// every row, adding and judging nothing.
#[tokio::test]
async fn devices_keeps_every_row_the_manager_answered() {
    let app = app(Fixture::probed());
    app.fixture.set(Ok(NodeDevices {
        probed_at: Some(PROBED_AT),
        ram_mib: 1_048_576,
        cpu_threads: 128,
        devices: vec![
            DeviceInfo {
                id: "CPU".into(),
                name: "CPU".into(),
                backend: "CPU".into(),
                total_mib: 0,
                free_mib: 0,
            },
            DeviceInfo {
                id: "ROCm0".into(),
                name: "AMD Radeon™ Graphics  XSP".into(),
                backend: "ROCm".into(),
                total_mib: 0,
                free_mib: 0,
            },
            DeviceInfo {
                id: "Vulkan9".into(),
                name: "".into(),
                backend: "Vulkan".into(),
                total_mib: 18446744073709551615,
                free_mib: 0,
            },
        ],
    }));
    let cookie = app.sign_in().await;
    let rep = app.devices(Some(&cookie)).await;
    assert_eq!(rep.status, StatusCode::OK, "{}", rep.body);
    assert_eq!(
        rep.json()["devices"],
        json!([
            {"id": "CPU", "name": "CPU", "backend": "CPU", "totalMib": 0, "freeMib": 0},
            {"id": "ROCm0", "name": "AMD Radeon™ Graphics  XSP", "backend": "ROCm", "totalMib": 0, "freeMib": 0},
            {"id": "Vulkan9", "name": "", "backend": "Vulkan", "totalMib": 18446744073709551615u64, "freeMib": 0},
        ]),
        "nothing added, dropped, sorted or judged: {}",
        rep.body
    );
    assert_eq!(
        (
            rep.json()["ramMib"].as_u64(),
            rep.json()["cpuThreads"].as_u64()
        ),
        (Some(1_048_576), Some(128)),
        "the inventory travels as the manager answered it, MiB and count: {}",
        rep.body
    );
}

// ── The refusals and the failures ───────────────────────────────────────

/// The manager refuses the signed-in user (removed from `pai`): 403
/// `session-not-administrator`, and the session ends, as every other route's
/// refusal does.
#[tokio::test]
async fn devices_refused_ends_the_session() {
    let app = app(Fixture::probed());
    app.fixture.set(Err(ClientError::NotAuthorized));
    let other = app.sign_in_as("root").await;
    let cookie = app.sign_in().await;
    let rep = app.devices(Some(&cookie)).await;
    rep.assert_error(StatusCode::FORBIDDEN, "session-not-administrator", &[]);
    assert_eq!(
        app.calls(),
        vec!["devices".to_string()],
        "the refusal is the read's own"
    );
    app.session_of(&cookie).await.assert_session_required();
    app.session_of(&other).await.assert_signed_in_as("root");
}

/// No manager is 503 `client-unavailable`, as the other reads; a manager that
/// answers over the route's deadline is the same, and the page retries.
#[tokio::test]
async fn devices_with_no_manager_is_503() {
    let app = app(Fixture::probed());
    app.fixture.set(Err(ClientError::Unavailable));
    let cookie = app.sign_in().await;
    app.devices(Some(&cookie)).await.assert_error(
        StatusCode::SERVICE_UNAVAILABLE,
        "client-unavailable",
        &[],
    );
    assert_eq!(app.calls(), vec!["devices".to_string()]);
}

/// A manager that cannot answer past the route's deadline: the page retries, so
/// the answer is the same 503 as no manager, and it comes at the deadline, not
/// when the probe finally speaks. The route's bound is the probe's (`DEVICES_
/// TIMEOUT`: 30 s of probe plus the transport's 5 s), so this test waits past
/// it, the one cost of pinning a cold probe is not cut off at a read's 5 s.
#[tokio::test]
async fn devices_answered_past_the_deadline_is_503() {
    let app = app(Fixture::probed());
    let slow = pai_bff::DEVICES_TIMEOUT + Duration::from_secs(1);
    app.fixture.set_delay(slow);
    let cookie = app.sign_in().await;
    let started = Instant::now();
    let rep = app
        .get_within(
            pai_bff::DEVICES_TIMEOUT + Duration::from_secs(15),
            "/api/node/devices",
            Some(&cookie),
        )
        .await;
    rep.assert_error(StatusCode::SERVICE_UNAVAILABLE, "client-unavailable", &[]);
    let waited = started.elapsed();
    assert!(
        waited < slow + Duration::from_secs(5),
        "the answer is at the deadline, not at the probe: {waited:?}"
    );
}

/// Anything else is 502 `client-other`, never the manager's own words: a
/// refusal is about the caller, this is about the manager.
#[tokio::test]
async fn devices_of_a_manager_that_failed_is_502_without_its_detail() {
    let app = app(Fixture::probed());
    app.fixture.set(Err(ClientError::Other(
        "gpus: cannot open /dev/dri/renderD128".into(),
    )));
    let cookie = app.sign_in().await;
    let rep = app.devices(Some(&cookie)).await;
    rep.assert_error(StatusCode::BAD_GATEWAY, "client-other", &[]);
    assert!(
        !rep.body.contains("renderD128"),
        "the manager's words stay out of the answer: {}",
        rep.body
    );
    app.session_of(&cookie).await.assert_signed_in_as("alice");
}

// ── The session's idle deadline ─────────────────────────────────────────

/// A page read, not a poll: the read moves the session's idle deadline, so a
/// profile page opened late keeps the sign-in alive. The steps are chosen to
/// tell the two gates of `session.rs` apart: after two `SHORT` steps the clock
/// is past the deadline a `peek` route leaves where it was (the sign-in's), and
/// inside the one this read moved, so only a `current` route is served there;
/// one `PAST` after the last answer the moved deadline is over, so the route
/// moved it, it did not clear it.
#[tokio::test]
async fn devices_moves_the_sessions_idle_deadline() {
    let app = app(Fixture::probed());
    let cookie = app.sign_in().await;
    app.clock.advance(SHORT);
    let rep = app.devices(Some(&cookie)).await;
    assert_eq!(rep.status, StatusCode::OK, "late: {}", rep.body);
    // Past the deadline of the sign-in, inside the one this read moved.
    app.clock.advance(SHORT);
    let rep = app.devices(Some(&cookie)).await;
    assert_eq!(
        rep.status,
        StatusCode::OK,
        "two SHORT steps are past the deadline of a read that leaves it alone: {}",
        rep.body
    );
    // Past the deadline of the read that just answered: the session ended.
    app.clock.advance(PAST);
    let rep = app.devices(Some(&cookie)).await;
    rep.assert_session_required();
    assert_eq!(
        app.calls(),
        vec!["devices".to_string(), "devices".to_string()],
        "the ended session asks the manager nothing"
    );
}

/// The route's deadline is the probe's bound plus five seconds, longer than a
/// read's: a cold probe runs up to `GPU_PROBE_TIMEOUT` (30 s) and the page must
/// not be cut off while it runs.
#[test]
fn the_devices_deadline_is_the_probe_bound_plus_five_seconds() {
    assert_eq!(
        pai_bff::DEVICES_TIMEOUT,
        Duration::from_secs(30 + 5),
        "the probe's bound, plus the transport's own"
    );
    assert!(
        pai_bff::DEVICES_TIMEOUT > pai_bff::MANAGER_TIMEOUT,
        "the cold probe is not cut off at a read's limit"
    );
}
