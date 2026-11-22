//! `GET /api/version`: the framework's version, the machine's name and its
//! operating system (slice AN4, `ARCHITECTURE.md` "About and node info"),
//! through `pai_bff::router_with_session_and_roots` with a fake manager, a fake
//! authenticator, a clock the test moves and an operating-system root of the
//! test (never the bus, pipe, PAM, LogonUser, or the real machine's `/etc`).
//!
//! The contract:
//! - 200 `{"version", "name", "os": {"name", "version", "kernel"}}`: `version`
//!   is the bff's own (the packages pin the framework to one version); `name` is
//!   the bff's machine's host name, the same value `GET /api/node` answers, never
//!   empty; `os` is what the `pai-core` host reader answers of the root the
//!   builder handed in, every field a string or `null`. The bff reads it itself:
//!   the manager is asked nothing.
//! - The read moves no session's idle deadline, like `/api/node`: About and the
//!   navigation poll it, and a tab that only polls signs itself out on time.
//! - With no live session the answer is 401 `session-required`, no `Set-Cookie`,
//!   and the manager is not connected to.
//! - The service and the other builders read the machine's own root (`/`), so
//!   the read answers the machine it runs on: in step 14 that is the director's.

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

const CLIENT: &str = "127.0.0.1:51001";

/// The revision secret these routers are built with: this route answers no
/// revision.
const REVISION_KEY: [u8; 16] = [0; 16];

/// The session idle timeout chosen for the bff (`session.rs`'s own), and a
/// step just under it.
const IDLE: Duration = Duration::from_secs(30 * 60);
const SHORT: Duration = Duration::from_secs(30 * 60 - 60);

/// The key of the gate's answer (`session.rs`).
const SESSION_REQUIRED: &str = "session-required";

/// The framework's own version, the one the packages are named after.
const FRAMEWORK: &str = env!("CARGO_PKG_VERSION");

type Log = Arc<Mutex<Vec<String>>>;

// ── Fakes ───────────────────────────────────────────────────────────────

/// A manager that records every call. The version read must never reach it: the
/// tests assert the log, the connects and the principals stay empty.
#[derive(Default)]
struct FakeManager {
    log: Log,
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
        Err(ClientError::Unavailable)
    }
    fn node_info(&self) -> Result<pai_core::client::NodeInfoReply, ClientError> {
        self.record("node_info".into());
        Err(ClientError::Unavailable)
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

/// The clock the session reads, which the test moves (the idle-deadline rule).
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

/// The authenticator: `alice` with the one password, never PAM, LogonUser or
/// the manager.
struct FakeAuth;

impl Authenticator for FakeAuth {
    fn authenticate(&self, user: &str, password: &str) -> Result<Identity, AuthError> {
        match (user, password) {
            ("alice", "correct horse") => Ok(Identity {
                user: "alice".into(),
            }),
            _ => Err(AuthError::BadCredentials),
        }
    }
}

/// A router over a manager that records the calls, a clock the test moves, and
/// the operating-system root `os_root` the version read answers.
struct App {
    router: Router,
    clock: FakeClock,
    log: Log,
    connects: Arc<AtomicUsize>,
    /// The principal each connect was asked to act for.
    principals: Arc<Mutex<Vec<Option<String>>>>,
    /// The root of the test, kept alive while the app lives.
    root: tempfile::TempDir,
}

/// The app whose version read answers the operating system of the root `root`
/// holds. The directory lives as long as the app does.
fn app_of(root: tempfile::TempDir) -> App {
    let log = Log::default();
    let connects = Arc::new(AtomicUsize::new(0));
    let principals = Arc::new(Mutex::new(Vec::new()));
    let (l, c, p) = (log.clone(), connects.clone(), principals.clone());
    let connect: Connect = Arc::new(move |principal: Option<&str>| {
        c.fetch_add(1, Ordering::SeqCst);
        p.lock()
            .unwrap_or_else(|e| e.into_inner())
            .push(principal.map(str::to_string));
        Ok(Box::new(FakeManager { log: l.clone() }) as Client)
    });
    let clock = FakeClock::new();
    let router = pai_bff::router_with_session_and_roots(
        connect,
        TIMEOUT,
        Arc::new(FakeAuth),
        clock.clock(),
        REVISION_KEY,
        None,
        root.path().to_path_buf(),
    );
    App {
        router,
        clock,
        log,
        connects,
        principals,
        root,
    }
}

/// A root whose `/etc/os-release` names `Ubuntu` `26.04`, the fixture of the AN1
/// reader's own tests.
fn ubuntu_root() -> tempfile::TempDir {
    let dir = tempfile::tempdir().expect("tempdir");
    std::fs::create_dir_all(dir.path().join("etc")).expect("dir");
    std::fs::write(
        dir.path().join("etc/os-release"),
        "PRETTY_NAME=\"Ubuntu 26.04.1 LTS\"\nNAME=\"Ubuntu\"\nVERSION_ID=\"26.04\"\nID=ubuntu\n",
    )
    .expect("os-release");
    dir
}

/// A root with no os-release at all (the file is unreadable, the reader's rule):
/// no name, no version.
fn empty_root() -> tempfile::TempDir {
    tempfile::tempdir().expect("tempdir")
}

/// The operating system the `pai-core` reader answers of `root`: the document
/// the route must carry over, field for field. The kernel is the machine's own
/// (`uname(2)`), which no root holds: the reader is the only source, so a test
/// reads the figure from it and never guesses at one.
fn os_of(root: &std::path::Path) -> Value {
    let os = pai_core::host::os(root);
    json!({
        "name": os.name,
        "version": os.version,
        "kernel": os.kernel,
    })
}

impl App {
    fn calls(&self) -> Vec<String> {
        self.log.lock().unwrap_or_else(|e| e.into_inner()).clone()
    }

    /// The principal each connect was asked to act for, in order.
    fn principals(&self) -> Vec<Option<String>> {
        self.principals
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .clone()
    }

    /// Nothing was asked of the manager: no connect, no principal, no call.
    fn assert_asked_nothing(&self, what: &str) {
        assert_eq!(self.connects.load(Ordering::SeqCst), 0, "{what}: connected");
        assert!(self.calls().is_empty(), "{what}: {:?}", self.calls());
        assert!(
            self.principals().is_empty(),
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

    /// Signed in, `GET /api/version`.
    async fn version(&self) -> Reply {
        let cookie = self.sign_in().await;
        self.get("/api/version", Some(&cookie)).await
    }
}

struct Reply {
    status: StatusCode,
    content_type: String,
    headers: Vec<(String, String)>,
    body: String,
}

impl Reply {
    fn json(&self) -> Value {
        serde_json::from_str(&self.body)
            .unwrap_or_else(|e| panic!("not JSON ({e}): {:?}", self.body))
    }

    /// The gate's answer: 401 `session-required`, and no `Set-Cookie`.
    fn assert_session_required(&self) {
        assert_eq!(self.status, StatusCode::UNAUTHORIZED, "{}", self.body);
        assert!(
            self.content_type.starts_with("application/json"),
            "content-type {:?}",
            self.content_type
        );
        assert_eq!(
            self.json(),
            json!({ "key": SESSION_REQUIRED, "args": [] }),
            "{}",
            self.body
        );
        assert!(
            self.headers.iter().all(|(n, _)| n != "set-cookie"),
            "a refused read sets a cookie: {:?}",
            self.headers
        );
    }

    /// The 200 document.
    fn document(&self) -> Value {
        assert_eq!(self.status, StatusCode::OK, "{}", self.body);
        assert!(
            self.content_type.starts_with("application/json"),
            "content-type {:?}",
            self.content_type
        );
        self.json()
    }

    /// `GET /api/session` for a session that is still signed in.
    fn assert_signed_in(&self) {
        assert_eq!(self.status, StatusCode::OK, "signed in: {}", self.body);
        assert_eq!(self.json(), json!({ "user": "alice" }), "{}", self.body);
    }
}

// ── Failure paths first ─────────────────────────────────────────────────

/// No session: 401 `session-required`, no `Set-Cookie`, the manager not
/// connected to: the gate answers before anything is read.
#[tokio::test]
async fn without_a_session_the_manager_is_not_asked() {
    let app = app_of(ubuntu_root());
    let r = app.get("/api/version", None).await;
    r.assert_session_required();
    app.assert_asked_nothing("with no session");
}

/// The read asks the manager nothing, for a signed-in reader too: the version,
/// the name and the operating system are all the bff's own machine.
#[tokio::test]
async fn the_version_read_asks_the_manager_nothing() {
    let app = app_of(ubuntu_root());
    app.version().await.assert_version_of(&app);
    app.assert_asked_nothing("signed in");
}

// ── What it serves ──────────────────────────────────────────────────────

/// The document is the three members: the framework's version, the machine's
/// name, and the operating system the host reader answers of the root the
/// builder handed in (the fixture, never the machine's `/etc`).
#[tokio::test]
async fn the_document_is_the_version_the_name_and_the_operating_system() {
    let app = app_of(ubuntu_root());
    let doc = app.version().await.document();
    assert_eq!(
        doc,
        json!({
            "version": FRAMEWORK,
            "name": pai_bff::listen::hostname(),
            "os": os_of(app.root.path()),
        }),
        "the whole document"
    );
}

/// The operating system is the fixture's os-release: the reader's own rules
/// answered `NAME` and `VERSION_ID`, so the route carries the root it was built
/// with and not the machine's. Linux only: Windows reads the registry, where a
/// root of the test answers nothing.
#[cfg(unix)]
#[tokio::test]
async fn the_operating_system_is_the_fixtures_os_release() {
    let app = app_of(ubuntu_root());
    let doc = app.version().await.document();
    assert_eq!(doc["os"]["name"], json!("Ubuntu"), "{doc}");
    assert_eq!(doc["os"]["version"], json!("26.04"), "{doc}");
}

/// A root that answers nothing (no os-release to read) is an `os` whose name and
/// version are `null`: the page has a word for an unknown system, and the read
/// is still a 200. The member is there all the same, with the kernel beside the
/// two nulls, so the page reads one object and not a hole.
#[tokio::test]
async fn a_root_that_answers_nothing_is_no_operating_system() {
    let app = app_of(empty_root());
    let doc = app.version().await.document();
    let os = doc
        .get("os")
        .and_then(Value::as_object)
        .unwrap_or_else(|| panic!("no operating system object: {doc}"));
    let mut keys: Vec<&str> = os.keys().map(String::as_str).collect();
    keys.sort();
    assert_eq!(keys, ["kernel", "name", "version"], "{doc}");
    assert_eq!(doc["os"]["name"], Value::Null, "{doc}");
    assert_eq!(doc["os"]["version"], Value::Null, "{doc}");
}

/// The name is the one `/api/node` answers, the same non-empty string: the
/// navigation takes the host name from this read and labels the node with the
/// name the node's own read carries.
#[tokio::test]
async fn the_name_is_the_one_the_node_read_answers() {
    let app = app_of(ubuntu_root());
    let cookie = app.sign_in().await;
    let version = app.get("/api/version", Some(&cookie)).await.document();
    let node = app.get("/api/node", Some(&cookie)).await.document();
    assert_eq!(version["name"], node["name"], "one name, two reads");
    assert!(
        !version["name"].as_str().expect("a string").is_empty(),
        "{version}"
    );
}

/// The read is read-only: every other verb is refused, not served.
#[tokio::test]
async fn version_refuses_another_method() {
    let app = app_of(ubuntu_root());
    let cookie = app.sign_in().await;
    for method in [Method::POST, Method::PUT, Method::DELETE] {
        let builder = Request::builder()
            .method(method.clone())
            .uri("/api/version")
            .header(header::COOKIE, &cookie);
        let request = builder.body(Body::empty()).expect("request");
        let response = tokio::time::timeout(DEADLINE, app.router.clone().oneshot(request))
            .await
            .expect("answered in time")
            .expect("infallible");
        assert_eq!(
            response.status(),
            StatusCode::METHOD_NOT_ALLOWED,
            "{method} /api/version"
        );
    }
}

// ── The session: a poll of this read signs itself out on time ───────────

/// The read is a read, not an action: it never moves the session's idle
/// deadline, which stays the sign-in's, to the second.
#[tokio::test]
async fn the_version_read_does_not_move_the_idle_deadline() {
    let alive = app_of(ubuntu_root());
    let cookie = alive.sign_in().await;
    alive.clock.advance(SHORT);
    let r = alive.get("/api/version", Some(&cookie)).await;
    assert_eq!(r.status, StatusCode::OK, "the read is answered: {}", r.body);
    alive.clock.advance(IDLE - SHORT);
    alive
        .get("/api/session", Some(&cookie))
        .await
        .assert_signed_in();

    // One second past that same deadline: the read, in the middle, moved
    // nothing.
    let gone = app_of(ubuntu_root());
    let cookie = gone.sign_in().await;
    gone.clock.advance(SHORT);
    gone.get("/api/version", Some(&cookie)).await.document();
    gone.clock.advance(IDLE - SHORT + Duration::from_secs(1));
    let r = gone.get("/api/session", Some(&cookie)).await;
    assert_eq!(r.status, StatusCode::UNAUTHORIZED, "{}", r.body);
}

impl Reply {
    /// 200 with the three members of the About read, the operating system the
    /// host reader answers of the app's root.
    fn assert_version_of(&self, app: &App) {
        assert_eq!(self.status, StatusCode::OK, "{}", self.body);
        assert!(
            self.content_type.starts_with("application/json"),
            "content-type {:?}",
            self.content_type
        );
        assert_eq!(
            self.json(),
            json!({
                "version": FRAMEWORK,
                "name": pai_bff::listen::hostname(),
                "os": os_of(app.root.path()),
            }),
            "{}",
            self.body
        );
    }
}
