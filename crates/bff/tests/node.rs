//! `GET /api/node`: this host's name, the API's root, the framework's version
//! and the versions of the packages that installed the engine (slice 13,
//! `ARCHITECTURE.md`), through `pai_bff::router_with_session_and_versions_dir`
//! with a fake manager, a fake authenticator and a versions directory of the
//! test (never the bus, pipe, PAM, LogonUser or a real install).
//!
//! The contract:
//! - A session is needed: with none the answer is 401 `session-required`, no
//!   `Set-Cookie`, and the manager is not connected to. The read asks the
//!   manager nothing at all, and moves no session's idle deadline, like
//!   `GET /api/version`: the About page and the navigation poll it.
//! - 200 `{"name", "api", "version", "engines"}`: `name` is the machine's host
//!   name, `localhost` when it names itself nothing; `api` is `/api`; `version`
//!   is the bff's own (the packages pin the framework to one version);
//!   `engines` is each engine installed under the versions directory,
//!   `{"engine", "version", "backends"}`, each backend `{"name", "version"}`
//!   with `name` the package's suffix (`cuda`, `rocm`, `vulkan`), sorted by
//!   name. A host with no engine has `engines: []`; an engine with no runtime
//!   `backends: []`; a version file that is empty is `null`.
//! - There is no error but 401: a versions directory that is not there, a file
//!   that cannot be read, and a file the reader's rules skip (over 128 bytes,
//!   a control character, a name that is not `llama.cpp` or
//!   `llama.cpp-<a-z0-9>+`) answer with that engine, runtime or version left
//!   out. The files are read at each request: an engine upgrade shows without
//!   a restart of the service.

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

/// The revision secret these routers are built with: this route answers no
/// revision.
const REVISION_KEY: [u8; 16] = [0; 16];

/// The session idle timeout chosen for the bff (`session.rs`'s own), and a
/// step just under it.
const IDLE: Duration = Duration::from_secs(30 * 60);
const SHORT: Duration = Duration::from_secs(30 * 60 - 60);

/// The key of the gate's answer (`session.rs`).
const SESSION_REQUIRED: &str = "session-required";

/// The framework's own version, the one `/api/version` answers and the one
/// this document carries.
const FRAMEWORK: &str = env!("CARGO_PKG_VERSION");

type Log = Arc<Mutex<Vec<String>>>;

// ── Fakes ───────────────────────────────────────────────────────────────

/// A manager that records every call and answers what no node test reads. The
/// node read must never reach it: the tests assert the log stays empty.
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
/// the versions directory `dir` (`None`: an install that ships no version
/// files).
struct App {
    router: Router,
    clock: FakeClock,
    log: Log,
    connects: Arc<AtomicUsize>,
    /// The versions directory of the test, kept alive while the app lives, so
    /// a test can write a file into it (`None`: the folder is not there).
    dir: Option<tempfile::TempDir>,
}

/// The app whose router reads the versions directory `versions_dir`
/// (`None`: an install that ships no version files).
fn app_at(versions_dir: Option<std::path::PathBuf>) -> App {
    let log = Log::default();
    let connects = Arc::new(AtomicUsize::new(0));
    let (l, c) = (log.clone(), connects.clone());
    let connect: Connect = Arc::new(move |_principal: Option<&str>| {
        c.fetch_add(1, Ordering::SeqCst);
        Ok(Box::new(FakeManager { log: l.clone() }) as Client)
    });
    let clock = FakeClock::new();
    let router = pai_bff::router_with_session_and_versions_dir(
        connect,
        TIMEOUT,
        Arc::new(FakeAuth),
        clock.clock(),
        REVISION_KEY,
        versions_dir,
    );
    App {
        router,
        clock,
        log,
        connects,
        dir: None,
    }
}

/// A versions directory holding `files`, each name with its text.
fn versions_with(files: &[(&str, &str)]) -> tempfile::TempDir {
    let dir = tempfile::tempdir().expect("tempdir");
    for (name, text) in files {
        std::fs::write(dir.path().join(name), text).expect("file");
    }
    dir
}

/// The router of a versions directory holding `files`. The directory is kept
/// by the app: a test that writes a file into it, at
/// [`App::versions_dir`], has the package install a runtime.
fn app_with(files: &[(&str, &str)]) -> App {
    let dir = versions_with(files);
    app_at(Some(dir.path().to_path_buf())).keeping(dir)
}

/// The router of a versions directory that is not there (a tree build, an
/// older package, no engine installed).
fn app_without_dir() -> App {
    let dir = tempfile::tempdir().expect("tempdir");
    let missing = dir.path().join("versions");
    assert!(!missing.exists(), "fixture: the folder is not there");
    app_at(Some(missing))
}

impl App {
    fn calls(&self) -> Vec<String> {
        self.log.lock().unwrap_or_else(|e| e.into_inner()).clone()
    }

    /// Keeps `dir` alive beside the app, so a test can write into it.
    fn keeping(self, dir: tempfile::TempDir) -> App {
        App {
            dir: Some(dir),
            ..self
        }
    }

    /// The versions directory the app reads, to install a package's file into.
    fn versions_dir(&self) -> std::path::PathBuf {
        self.dir
            .as_ref()
            .expect("an app with a versions directory")
            .path()
            .to_path_buf()
    }

    /// Nothing was asked of the manager: no connect, no call.
    fn assert_asked_nothing(&self, what: &str) {
        assert_eq!(self.connects.load(Ordering::SeqCst), 0, "{what}: connected");
        assert!(self.calls().is_empty(), "{what}: {:?}", self.calls());
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
        self.ask(Method::GET, uri, cookie).await
    }

    async fn ask(&self, method: Method, uri: &str, cookie: Option<&str>) -> Reply {
        let mut builder = Request::builder().method(method).uri(uri);
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

    /// Signed in, `GET /api/node`.
    async fn node(&self) -> Reply {
        let cookie = self.sign_in().await;
        self.get("/api/node", Some(&cookie)).await
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
}

// ── Failure paths first ─────────────────────────────────────────────────

/// No session: 401 `session-required`, no `Set-Cookie`, and the manager is not
/// connected to: the gate answers before anything is read.
#[tokio::test]
async fn without_a_session_the_manager_is_not_asked() {
    let app = app_with(&[("llama.cpp", "1:1.0.0+pai2\n")]);
    let r = app.get("/api/node", None).await;
    r.assert_session_required();
    app.assert_asked_nothing("with no session");
}

/// A cookie naming no session (made up, or from before a restart) is the same,
/// and asks the manager nothing.
#[tokio::test]
async fn an_unknown_session_is_session_required() {
    let app = app_with(&[("llama.cpp", "1:1.0.0+pai2\n")]);
    let r = app
        .get(
            "/api/node",
            Some(&format!("pai_session={}", "ab".repeat(32))),
        )
        .await;
    r.assert_session_required();
    app.assert_asked_nothing("with an unknown session");
}

/// The read is read-only: every other verb is refused, not served.
#[tokio::test]
async fn node_refuses_another_method() {
    let app = app_with(&[("llama.cpp", "1:1.0.0+pai2\n")]);
    let cookie = app.sign_in().await;
    for method in [Method::POST, Method::PUT, Method::DELETE] {
        let r = app.ask(method.clone(), "/api/node", Some(&cookie)).await;
        assert_eq!(
            r.status,
            StatusCode::METHOD_NOT_ALLOWED,
            "{method} /api/node: {}",
            r.body
        );
    }
}

// ── What it serves ──────────────────────────────────────────────────────

/// The document of a host with the engine and its three GPU runtimes, the
/// versions as the packages render them (the epoch of the engine's, the `+`
/// suffix of CUDA's), the backends sorted by name.
#[tokio::test]
async fn the_document_of_the_engine_and_its_runtimes() {
    let app = app_with(&[
        ("llama.cpp", "1:1.0.0+pai2\n"),
        ("llama.cpp-vulkan", "1.4.363.1"),
        ("llama.cpp-cuda", "13.5.0+nccl.2.32.3\n"),
        ("llama.cpp-rocm", "10.1.0\n"),
    ]);
    let doc = app.node().await.document();
    assert_eq!(
        doc,
        json!({
            "name": pai_bff::listen::hostname(),
            "api": "/api",
            "version": FRAMEWORK,
            "engines": [{
                "engine": "llama.cpp",
                "version": "1:1.0.0+pai2",
                "backends": [
                    { "name": "cuda", "version": "13.5.0+nccl.2.32.3" },
                    { "name": "rocm", "version": "10.1.0" },
                    { "name": "vulkan", "version": "1.4.363.1" },
                ],
            }],
        }),
        "the whole document"
    );
}

/// The document has the four members and no other: what the About page and the
/// navigation read is that list, nothing beside it.
#[tokio::test]
async fn the_document_has_the_four_members() {
    let app = app_with(&[("llama.cpp", "1:1.0.0+pai2\n")]);
    let doc = app.node().await.document();
    let mut members: Vec<&str> = doc
        .as_object()
        .expect("an object")
        .keys()
        .map(String::as_str)
        .collect();
    members.sort();
    assert_eq!(members, ["api", "engines", "name", "version"]);
    let engine = doc["engines"].as_array().expect("an array")[0]
        .as_object()
        .expect("an object");
    let mut keys: Vec<&str> = engine.keys().map(String::as_str).collect();
    keys.sort();
    assert_eq!(keys, ["backends", "engine", "version"]);
}

/// The name is the machine's host name: the navigation lists this host under
/// the name the machine carries.
#[tokio::test]
async fn the_name_is_the_machines_host_name() {
    let app = app_without_dir();
    let doc = app.node().await.document();
    let host = pai_bff::listen::hostname();
    assert_eq!(doc["name"], json!(host), "the name the machine carries");
    assert!(!doc["name"].as_str().expect("a string").is_empty());
}

/// A host that names itself nothing is `localhost`, the address its bff is
/// reached at: the navigation has a name to show for it.
#[test]
fn a_host_that_names_itself_nothing_is_localhost() {
    assert_eq!(pai_bff::node::name(""), "localhost");
    assert_eq!(pai_bff::node::name("node-1"), "node-1");
}

/// A CPU-only install: the engine's own file and no runtime file is
/// `backends: []`, the engine listed.
#[tokio::test]
async fn an_engine_with_no_runtime_has_no_backends() {
    let app = app_with(&[("llama.cpp", "1:1.0.0+pai2\n")]);
    let doc = app.node().await.document();
    assert_eq!(
        doc["engines"],
        json!([{ "engine": "llama.cpp", "version": "1:1.0.0+pai2", "backends": [] }])
    );
}

/// No versions directory (a tree build, an older package, no engine
/// installed): `engines: []`, and the answer is a 200: there is no error here
/// but 401.
#[tokio::test]
async fn no_versions_directory_is_no_engines() {
    let app = app_without_dir();
    let doc = app.node().await.document();
    assert_eq!(doc["engines"], json!([]), "{}", doc);
}

/// An empty directory is the same answer.
#[tokio::test]
async fn an_empty_versions_directory_is_no_engines() {
    let app = app_with(&[]);
    let doc = app.node().await.document();
    assert_eq!(doc["engines"], json!([]), "{}", doc);
}

/// A runtime file is no engine: with `llama.cpp-cuda` and no `llama.cpp`, the
/// engine is not listed, so no half-install reads as an engine with a runtime.
#[tokio::test]
async fn a_runtime_without_its_engine_is_no_engine() {
    let app = app_with(&[("llama.cpp-cuda", "13.5.0")]);
    let doc = app.node().await.document();
    assert_eq!(doc["engines"], json!([]), "{}", doc);
}

/// The files the reader's rules skip are not in the document: one over 128
/// bytes, one with a control character, one with a foreign name, and a
/// directory. The engine beside them is listed, with the one runtime that
/// reads.
#[tokio::test]
async fn the_files_the_reader_skips_are_not_in_the_document() {
    let dir = versions_with(&[
        ("llama.cpp", "1:1.0.0+pai2"),
        ("llama.cpp-cuda", "13.5.0"),
        // Over the 128 bytes a version file may hold.
        ("llama.cpp-vulkan", &"v".repeat(129)),
        // A control character in the first line.
        ("llama.cpp-rocm", "10.1.0\u{1}x"),
        // A name that is no package of this install.
        ("not-a-package", "9.9.9"),
        ("llama.cpp-OTHER", "9.9.9"),
    ]);
    std::fs::create_dir(dir.path().join("llama.cpp-nested")).expect("directory");
    let doc = app_at(Some(dir.path().to_path_buf()))
        .node()
        .await
        .document();
    assert_eq!(
        doc["engines"],
        json!([{ "engine": "llama.cpp", "version": "1:1.0.0+pai2", "backends": [
            { "name": "cuda", "version": "13.5.0" }
        ] }]),
        "only the two files that read are in the document"
    );
}

/// A version file that is empty is an entry with no version: the runtime is
/// listed, its version `null`.
#[tokio::test]
async fn an_empty_version_file_is_no_version() {
    let app = app_with(&[("llama.cpp", "1:1.0.0+pai2"), ("llama.cpp-cuda", "")]);
    let doc = app.node().await.document();
    assert_eq!(
        doc["engines"],
        json!([{ "engine": "llama.cpp", "version": "1:1.0.0+pai2", "backends": [
            { "name": "cuda", "version": null }
        ] }])
    );
}

/// The files are read at each request: a runtime installed after the session
/// signed in is in the document of the next read, with no restart of the
/// service.
#[tokio::test]
async fn a_file_added_after_the_sign_in_is_in_the_next_document() {
    let app = app_with(&[("llama.cpp", "1:1.0.0+pai2")]);
    let cookie = app.sign_in().await;
    let first = app.get("/api/node", Some(&cookie)).await.document();
    assert_eq!(first["engines"][0]["backends"], json!([]), "{}", first);

    // The package installs a runtime while the session is open.
    std::fs::write(app.versions_dir().join("llama.cpp-rocm"), "10.1.0").expect("file");
    let r = app.get("/api/node", Some(&cookie)).await.document();
    assert_eq!(
        r["engines"],
        json!([{ "engine": "llama.cpp", "version": "1:1.0.0+pai2", "backends": [
            { "name": "rocm", "version": "10.1.0" }
        ] }]),
        "the runtime the host installed since"
    );
}

// ── The session: a poll of this read signs itself out on time ───────────

/// The read is a read, not an action: it never moves the session's idle
/// deadline, which stays the sign-in's, to the second.
#[tokio::test]
async fn the_node_read_does_not_move_the_idle_deadline() {
    let alive = app_without_dir();
    let cookie = alive.sign_in().await;
    alive.clock.advance(SHORT);
    let r = alive.get("/api/node", Some(&cookie)).await;
    assert_eq!(r.status, StatusCode::OK, "the read is answered: {}", r.body);
    alive.clock.advance(IDLE - SHORT);
    alive
        .get("/api/session", Some(&cookie))
        .await
        .assert_signed_in();

    // One second past that same deadline: the read, in the middle, moved
    // nothing.
    let gone = app_without_dir();
    let cookie = gone.sign_in().await;
    gone.clock.advance(SHORT);
    gone.get("/api/node", Some(&cookie)).await.document();
    gone.clock.advance(IDLE - SHORT + Duration::from_secs(1));
    let r = gone.get("/api/session", Some(&cookie)).await;
    assert_eq!(r.status, StatusCode::UNAUTHORIZED, "{}", r.body);
}

impl Reply {
    /// `GET /api/session` for a session that is still signed in.
    fn assert_signed_in(&self) {
        assert_eq!(self.status, StatusCode::OK, "signed in: {}", self.body);
        assert_eq!(self.json(), json!({ "user": "alice" }), "{}", self.body);
    }
}
