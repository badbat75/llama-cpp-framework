//! `GET /api/instances/{id}/server`: an instance's `server@<id>.ini` read
//! through the manager, for a signed-in user, through
//! `pai_bff::router_with_session` with a fake manager client and a fake
//! authenticator (never the real bus, pipe, PAM or LogonUser). Each request
//! runs under a deadline, so a bff that waits on the manager fails instead of
//! hanging.
//!
//! The contract (fe/briefs/api-contract.md) and the choices made here:
//! - A session is needed: none, or an unknown one, is 401 `session-required`
//!   and the manager is not asked.
//! - 200 `{"revision", "values", "defaults", "limits", "interfaces"}`:
//!   - `values`: every `[Server]` key of `pai_core::server_cfg::ServerConfig`
//!     by INI name, read as the configurator reads the file
//!     (`server_cfg::from_text`): a number for a numeric key, `true`/`false`
//!     for a switch, a string otherwise, `null` when unset (a value the
//!     configurator cannot read, like `Port = abc`, is unset too). The three
//!     plain switches with no unset state in the schema (`CudaDisableGraphs`,
//!     `AttnRotDisable`, `RocprofAttach`) are `false` when absent;
//!     `RocblasUseHipblaslt` is the select the web UI shows, `"on"`/`"off"`
//!     (fe/src/ServerForm.tsx), `null` when unset.
//!   - `defaults`: Port 9931 (`server_cfg::DEFAULT_PORT`), ModelsMax 4 and
//!     ModelsMemoryMargin 0 (the configurator's), the base URL derived from
//!     the saved Hostname and Port even when the file overrides it, ModelsDir
//!     and StateDir the instance's (`paths::instance_models_dir`,
//!     `paths::instance_state_dir`), never the bff account's.
//!   - `limits`: Port, ModelsMax, ModelsMemoryMargin the ranges the manager
//!     checks with (`checks::server_int_range`); Threads and ThreadsBatch 1
//!     to this machine's logical CPU count.
//!   - `interfaces`: `net_ifaces::interfaces()` as `{value, label}`.
//!   - `revision`: 16 lowercase hex digits, SipHash-2-4 of the whole file
//!     text under a 16-byte secret the router holds (drawn from the OS at
//!     `router()`'s build, handed in by these tests): the same for the same
//!     text through one router, another for any other text (a comment, the
//!     API key, included) and another for another secret. It is never the
//!     unkeyed hash of the text, so reading a revision is no oracle for a
//!     guess about the file.
//! - `OpencodeApiKey`: the bff sends the key to every signed-in session,
//!   since a session is a member of the administrators' group `pai` already
//!   (sign-in refuses anyone else 403): the bff has no session to keep the
//!   key from. `withheld` comes from the manager's `read_file` alone: when
//!   it says the key was withheld (the value blanked in the text it sends),
//!   `values.OpencodeApiKey` is `null` and the document gains
//!   `"withheld": ["OpencodeApiKey"]`; with nothing withheld there is no
//!   `withheld` member.
//! - Errors `{"key", "args": []}`, never the manager's detail: 404
//!   `client-no-instance` (no such file, or the manager says no such
//!   instance or refuses the id), 503 `client-unavailable` (no manager, or
//!   too slow), 403 `session-not-administrator` (the manager refused the
//!   call for the signed-in user, and the bff ended that session: the
//!   sign-in's key for the same fact), 502 `client-other` for anything else.
//!   The key `client-not-authorized` belongs to the routes that need no
//!   sign-in, where the manager refused the bff itself and no session lives
//!   to end: there it comes with 502 (`tests/instances.rs`,
//!   `tests/control.rs`).
//! - The route sits behind a session, so it acts for the signed-in user:
//!   `Connect` is asked with that user's name as the principal, taken from
//!   the session (`Sessions::current`), never from the request, so the
//!   manager re-checks the user's membership of `pai` at every call. The
//!   session-free routes connect with no principal, as sign-in does:
//!   `tests/instances.rs` and `tests/authenticator.rs` pin those.
//! - A 403 refusal for the signed-in user ends their session at once
//!   (user, 2026-11-16): the same cookie is 401 `session-required`
//!   afterwards, so the page returns to the sign-in form. Only that
//!   session ends, and no other answer ends one. The bff sends
//!   `session-not-administrator` exactly when it ends a session on a
//!   refusal, so the web UI needs no second read of `GET /api/session` to
//!   tell the two apart.

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

/// The revision secret a router is built with here, `00 01 .. 0f`: the key of
/// the published SipHash-2-4 test vectors (Appendix A of the SipHash paper,
/// the same list the `siphasher` crate's own test carries), so the revision a
/// read must answer is a vector of the algorithm and not a value computed
/// from the code under test.
const KEY_A: [u8; 16] = [0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15];

/// A second secret, `10 11 .. 1f`, for the rule that the revision depends on
/// the secret and not on the text alone.
const KEY_B: [u8; 16] = [
    16, 17, 18, 19, 20, 21, 22, 23, 24, 25, 26, 27, 28, 29, 30, 31,
];

/// The engine's shipped `server@default.ini`, the file the manager holds
/// after a fresh install on Linux.
const SHIPPED: &str = include_str!("../../core/defaults/server@default.ini");

/// Every `[Server]` key `server_cfg::from_text` reads.
const KEYS: [&str; 19] = [
    "Port",
    "Hostname",
    "Threads",
    "ThreadsBatch",
    "ModelsMax",
    "ModelsMemoryMargin",
    "ModelsDir",
    "RocblasUseHipblaslt",
    "CudaAllreduce",
    "CudaDisableGraphs",
    "AttnRotDisable",
    "RocprofAttach",
    "Webui",
    "WebuiMcpProxy",
    "LogVerbosity",
    "SaveStateOnShutdown",
    "StateDir",
    "OpencodeBaseUrl",
    "OpencodeApiKey",
];

type Log = Arc<Mutex<Vec<String>>>;

/// The principal every connect was asked for, in order: `None` is a call the
/// bff makes as its own account.
type Principals = Arc<Mutex<Vec<Option<String>>>>;

fn principals_of(p: &Principals) -> Vec<Option<String>> {
    p.lock().unwrap_or_else(|e| e.into_inner()).clone()
}

// ── Fakes ───────────────────────────────────────────────────────────────

/// A manager whose `read_file` answers `file` (for any name) after `delay`,
/// logging every call (`read <name>`, `status <id>`, ...). It is the client
/// of one connect, so it knows the principal that connect was handed
/// (`as_user`), and refuses the read for the principals in `refuse`, the way
/// the real manager refuses a user who is no longer a member of `pai`.
struct FakeManager {
    file: Result<Option<ManagerFile>, ClientError>,
    delay: Duration,
    log: Log,
    as_user: Option<String>,
    refuse: Vec<String>,
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
        if self
            .refuse
            .iter()
            .any(|u| Some(u.as_str()) == self.as_user.as_deref())
        {
            return Err(ClientError::NotAuthorized);
        }
        std::thread::sleep(self.delay);
        self.file.clone()
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

/// `alice` and `root`, two signed-in sessions (both members of `pai`, the
/// group sign-in admits: the bff keeps nothing from either), both with the
/// password `correct horse`.
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

/// A router over a manager that answers `file` after `delay`, with the
/// manager's call log, the number of connects and the principal of each.
struct App {
    router: Router,
    log: Log,
    connects: Arc<AtomicUsize>,
    principals: Principals,
}

fn app_with(
    connect_result: Result<(), ClientError>,
    file: Result<Option<ManagerFile>, ClientError>,
    delay: Duration,
) -> App {
    app_for(connect_result, file, delay, Vec::new(), KEY_A)
}

/// A manager holding `text` as every file, refusing the read for the
/// principals in `refused`: for them the call is `NotAuthorized`, as it is
/// for a user removed from `pai`.
fn app_refusing(refused: &[&str], text: &str) -> App {
    app_for(
        Ok(()),
        Ok(Some(whole(text))),
        Duration::ZERO,
        refused.iter().map(|u| u.to_string()).collect(),
        KEY_A,
    )
}

fn app_for(
    connect_result: Result<(), ClientError>,
    file: Result<Option<ManagerFile>, ClientError>,
    delay: Duration,
    refuse: Vec<String>,
    key: [u8; 16],
) -> App {
    let log = Log::default();
    let connects = Arc::new(AtomicUsize::new(0));
    let principals = Principals::default();
    let (l, c, p) = (log.clone(), connects.clone(), principals.clone());
    let connect: Connect = Arc::new(move |principal: Option<&str>| {
        c.fetch_add(1, Ordering::SeqCst);
        p.lock()
            .unwrap_or_else(|e| e.into_inner())
            .push(principal.map(str::to_string));
        connect_result.clone()?;
        Ok(Box::new(FakeManager {
            file: file.clone(),
            delay,
            log: l.clone(),
            as_user: principal.map(str::to_string),
            refuse: refuse.clone(),
        }) as Client)
    });
    let start = Instant::now();
    let router = pai_bff::router_with_session(
        connect,
        TIMEOUT,
        Arc::new(FakeAuth),
        Arc::new(move || start),
        key,
    );
    App {
        router,
        log,
        connects,
        principals,
    }
}

/// The manager's answer for a file it sends whole: nothing withheld.
fn whole(text: &str) -> ManagerFile {
    ManagerFile {
        text: text.to_string(),
        withheld: false,
    }
}

/// The manager's answer for a file whose `OpencodeApiKey` it withheld by
/// itself: the value blanked in the text it sends, the flag set.
fn withheld(text: &str) -> ManagerFile {
    ManagerFile {
        text: text.to_string(),
        withheld: true,
    }
}

/// A manager holding `text` as every file, sent whole, under the router
/// secret `key`.
fn app_holding_keyed(key: [u8; 16], text: &str) -> App {
    app_for(
        Ok(()),
        Ok(Some(whole(text))),
        Duration::ZERO,
        Vec::new(),
        key,
    )
}

/// A manager holding `text` as every file, sent whole.
fn app_holding(text: &str) -> App {
    app_holding_keyed(KEY_A, text)
}

/// A manager holding `text` as every file and reporting the key withheld.
fn app_holding_withheld(text: &str) -> App {
    app_with(Ok(()), Ok(Some(withheld(text))), Duration::ZERO)
}

fn app_reading(file: Result<Option<ManagerFile>, ClientError>) -> App {
    app_with(Ok(()), file, Duration::ZERO)
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
        assert_eq!(response.status(), StatusCode::OK, "signed in");
        let set_cookie = response
            .headers()
            .get(header::SET_COOKIE)
            .expect("a session cookie")
            .to_str()
            .expect("ascii cookie")
            .to_string();
        set_cookie
            .split(';')
            .next()
            .expect("name=value")
            .trim()
            .to_string()
    }

    /// `GET uri` with `cookie` when given.
    async fn get(&self, uri: &str, cookie: Option<&str>) -> Reply {
        self.get_with(uri, cookie, &[]).await
    }

    /// `GET uri` with `cookie` and extra headers (a request that tries to
    /// name the user it wants the bff to act for).
    async fn get_with(&self, uri: &str, cookie: Option<&str>, extra: &[(&str, &str)]) -> Reply {
        let mut builder = Request::builder().method(Method::GET).uri(uri);
        if let Some(c) = cookie {
            builder = builder.header(header::COOKIE, c);
        }
        for (name, value) in extra {
            builder = builder.header(*name, *value);
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

    /// Signed in, `GET /api/instances/default/server`.
    async fn read_default(&self) -> Reply {
        let cookie = self.sign_in().await;
        self.get("/api/instances/default/server", Some(&cookie))
            .await
    }

    /// Signed in as `user`, `GET /api/instances/default/server`.
    async fn read_default_as(&self, user: &str) -> Reply {
        let cookie = self.sign_in_as(user).await;
        self.get("/api/instances/default/server", Some(&cookie))
            .await
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

    /// An error answer: its status and `{"key": key, "args": []}`, nothing
    /// else.
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

    /// `GET /api/session` for a session that is still signed in: `{"user"}`.
    fn assert_signed_in_as(&self, user: &str) {
        assert_eq!(self.status, StatusCode::OK, "signed in: {}", self.body);
        assert_eq!(self.json(), json!({ "user": user }), "{}", self.body);
    }
}

// ── Failure paths first: the session ────────────────────────────────────

/// No session cookie: 401 `session-required`, and the manager is not even
/// connected to.
#[tokio::test]
async fn without_a_session_it_is_session_required_and_the_manager_is_not_asked() {
    let app = app_holding(SHIPPED);
    let r = app.get("/api/instances/default/server", None).await;
    r.assert_error(StatusCode::UNAUTHORIZED, "session-required");
    assert_eq!(app.connects.load(Ordering::SeqCst), 0, "no connect");
    assert!(app.calls().is_empty(), "{:?}", app.calls());
    assert!(
        principals_of(&app.principals).is_empty(),
        "no principal named: {:?}",
        principals_of(&app.principals)
    );
}

/// A cookie naming no session (made up, or from before a restart) is the
/// same.
#[tokio::test]
async fn an_unknown_session_is_session_required() {
    let app = app_holding(SHIPPED);
    let _real = app.sign_in().await;
    let r = app
        .get(
            "/api/instances/default/server",
            Some(&format!("pai_session={}", "ab".repeat(32))),
        )
        .await;
    r.assert_error(StatusCode::UNAUTHORIZED, "session-required");
    assert_eq!(app.connects.load(Ordering::SeqCst), 0, "no connect");
    assert!(app.calls().is_empty(), "{:?}", app.calls());
}

// ── The principal: the route acts for the signed-in user ────────────────

/// The connect is asked with the session's user as the principal, so the
/// manager re-checks that user's own right for every read: the session-free
/// routes connect with none (`tests/instances.rs`), this one names the user.
#[tokio::test]
async fn the_read_connects_as_the_signed_in_user() {
    for user in ["alice", "root"] {
        let app = app_holding(SHIPPED);
        let doc = app.read_default_as(user).await.document();
        assert!(doc["values"].is_object(), "{user}: {doc}");
        assert_eq!(
            principals_of(&app.principals),
            vec![Some(user.to_string())],
            "{user}: the principal of the connect"
        );
        assert_eq!(app.calls(), ["read server@default.ini"], "{user}");
    }
}

/// A name in the request never becomes the principal: a query parameter and
/// a header naming another user are ignored, the session's user is asked
/// for. The principal comes from the session and nowhere else.
#[tokio::test]
async fn a_name_in_the_request_is_never_the_principal() {
    let app = app_holding(SHIPPED);
    let cookie = app.sign_in_as("alice").await;
    let doc = app
        .get_with(
            "/api/instances/default/server?user=root",
            Some(&cookie),
            &[("x-user", "root"), ("x-principal", "root")],
        )
        .await
        .document();
    assert!(doc["values"].is_object(), "{doc}");
    assert_eq!(
        principals_of(&app.principals),
        vec![Some("alice".to_string())],
        "the session's user, never the request's"
    );
}

/// The session ends for the user the manager refused, at once: 403
/// `session-not-administrator` and the same cookie signed out.
#[tokio::test]
async fn a_refused_user_is_answered_403_and_signed_out() {
    let app = app_reading(Err(ClientError::NotAuthorized));
    let cookie = app.sign_in().await;
    app.get("/api/instances/default/server", Some(&cookie))
        .await
        .assert_error(StatusCode::FORBIDDEN, "session-not-administrator");
    app.get("/api/session", Some(&cookie))
        .await
        .assert_error(StatusCode::UNAUTHORIZED, "session-required");
    assert_eq!(
        principals_of(&app.principals),
        vec![Some("alice".to_string())],
        "the refused read, nothing after it"
    );
}

/// Only that session ends: a read refused for `root` signs `root` out and
/// leaves `alice`'s session signed in and answered.
#[tokio::test]
async fn a_refusal_ends_that_session_and_no_other() {
    let app = app_refusing(&["root"], SHIPPED);
    let alice = app.sign_in_as("alice").await;
    let root = app.sign_in_as("root").await;
    app.get("/api/instances/default/server", Some(&root))
        .await
        .assert_error(StatusCode::FORBIDDEN, "session-not-administrator");
    app.get("/api/session", Some(&root))
        .await
        .assert_error(StatusCode::UNAUTHORIZED, "session-required");
    let doc = app
        .get("/api/instances/default/server", Some(&alice))
        .await
        .document();
    assert!(doc["values"].is_object(), "alice is answered: {doc}");
    app.get("/api/session", Some(&alice))
        .await
        .assert_signed_in_as("alice");
    assert_eq!(
        principals_of(&app.principals),
        vec![Some("root".to_string()), Some("alice".to_string())],
        "each read asked for its own session's user"
    );
}

/// A manager that is not there, or too slow, is the bff's own news and not a
/// refusal of the user: the session lives on.
#[tokio::test]
async fn a_manager_that_is_unavailable_keeps_the_session() {
    let app = app_with(
        Err(ClientError::Unavailable),
        Ok(Some(whole(SHIPPED))),
        Duration::ZERO,
    );
    let cookie = app.sign_in().await;
    app.get("/api/instances/default/server", Some(&cookie))
        .await
        .assert_error(StatusCode::SERVICE_UNAVAILABLE, "client-unavailable");
    app.get("/api/session", Some(&cookie))
        .await
        .assert_signed_in_as("alice");
}

/// The other failures are no refusal of the user either: 404 (no such
/// instance) and 502 (any other failure) keep the session signed in.
#[tokio::test]
async fn a_failure_that_is_not_a_refusal_keeps_the_session() {
    for (file, status, key) in [
        (Ok(None), StatusCode::NOT_FOUND, "client-no-instance"),
        (
            Err(ClientError::Service("unit failed".into())),
            StatusCode::BAD_GATEWAY,
            "client-other",
        ),
    ] {
        let app = app_reading(file);
        let cookie = app.sign_in().await;
        app.get("/api/instances/default/server", Some(&cookie))
            .await
            .assert_error(status, key);
        app.get("/api/session", Some(&cookie))
            .await
            .assert_signed_in_as("alice");
    }
}

// ── Failure paths: the manager ──────────────────────────────────────────

/// No manager to connect to: 503 `client-unavailable`.
#[tokio::test]
async fn no_manager_is_unavailable() {
    let app = app_with(
        Err(ClientError::Unavailable),
        Ok(Some(whole(SHIPPED))),
        Duration::ZERO,
    );
    app.read_default()
        .await
        .assert_error(StatusCode::SERVICE_UNAVAILABLE, "client-unavailable");
    assert_eq!(app.connects.load(Ordering::SeqCst), 1, "connected once");
}

/// A manager gone during the read is the same.
#[tokio::test]
async fn a_manager_gone_during_the_read_is_unavailable() {
    let app = app_reading(Err(ClientError::Unavailable));
    app.read_default()
        .await
        .assert_error(StatusCode::SERVICE_UNAVAILABLE, "client-unavailable");
}

/// A manager slower than the bff's timeout: 503, answered within the
/// deadline rather than waiting for it.
#[tokio::test]
async fn a_manager_too_slow_is_unavailable() {
    let app = app_with(Ok(()), Ok(Some(whole(SHIPPED))), Duration::from_secs(2));
    let cookie = app.sign_in().await;
    let asked = Instant::now();
    let r = app
        .get("/api/instances/default/server", Some(&cookie))
        .await;
    r.assert_error(StatusCode::SERVICE_UNAVAILABLE, "client-unavailable");
    assert!(
        asked.elapsed() < Duration::from_secs(1),
        "answered after {:?}",
        asked.elapsed()
    );
}

/// The manager refuses the read: 403 `session-not-administrator` on this
/// route, since it acts for a signed-in user whose session the bff ends. The
/// session-free routes answer the refusal 502 `client-not-authorized`
/// (`tests/instances.rs`, `tests/control.rs`: the manager refused the bff).
#[tokio::test]
async fn a_manager_refusal_on_this_route_is_403_not_502() {
    let app = app_reading(Err(ClientError::NotAuthorized));
    app.read_default()
        .await
        .assert_error(StatusCode::FORBIDDEN, "session-not-administrator");
}

/// An id with no `server@<id>.ini`: 404 `client-no-instance`.
#[tokio::test]
async fn an_unknown_id_is_no_instance() {
    let app = app_reading(Ok(None));
    let cookie = app.sign_in().await;
    app.get("/api/instances/nope/server", Some(&cookie))
        .await
        .assert_error(StatusCode::NOT_FOUND, "client-no-instance");
    assert_eq!(app.calls(), ["read server@nope.ini"], "asked for its file");
}

/// An id that tries to leave the config folder (`..%2Fx`, `../x` once
/// decoded) is no instance: 404 `client-no-instance`, the same as any id
/// the manager refuses.
#[tokio::test]
async fn a_traversal_id_is_no_instance() {
    let app = app_reading(Ok(None));
    let cookie = app.sign_in().await;
    app.get("/api/instances/..%2Fx/server", Some(&cookie))
        .await
        .assert_error(StatusCode::NOT_FOUND, "client-no-instance");
}

/// The manager's own "no such instance", and its refusal of an id that
/// cannot name a file (`BadFileName`, `InvalidId`), are 404 too: in each
/// case the instance does not exist.
#[tokio::test]
async fn the_managers_no_instance_and_bad_ids_are_no_instance() {
    for error in [
        ClientError::NoInstance,
        ClientError::BadFileName,
        ClientError::InvalidId,
    ] {
        let app = app_reading(Err(error));
        let cookie = app.sign_in().await;
        app.get("/api/instances/Bad%20Id/server", Some(&cookie))
            .await
            .assert_error(StatusCode::NOT_FOUND, "client-no-instance");
    }
}

/// Any other failure: 502 `client-other` with no args, the manager's detail
/// (a path, a kind) never in the body.
#[tokio::test]
async fn any_other_failure_is_client_other_without_the_detail() {
    for (error, detail) in [
        (
            ClientError::Other("org.example.Boom: /etc/llama.cpp secret".into()),
            "secret",
        ),
        (
            ClientError::Io("PermissionDenied".into()),
            "PermissionDenied",
        ),
        (ClientError::Service("unit failed".into()), "unit failed"),
    ] {
        let app = app_reading(Err(error));
        let r = app.read_default().await;
        r.assert_error(StatusCode::BAD_GATEWAY, "client-other");
        assert!(!r.body.contains(detail), "{}", r.body);
    }
}

// ── The document ────────────────────────────────────────────────────────

/// The reply has exactly the contract's five members, and the bff asks the
/// manager for the instance's own file, nothing else.
#[tokio::test]
async fn the_document_has_exactly_the_contract_members() {
    let app = app_holding(SHIPPED);
    let doc = app.read_default().await.document();
    let mut members: Vec<&str> = doc
        .as_object()
        .expect("an object")
        .keys()
        .map(String::as_str)
        .collect();
    members.sort();
    assert_eq!(
        members,
        ["defaults", "interfaces", "limits", "revision", "values"]
    );
    assert_eq!(app.calls(), ["read server@default.ini"]);
}

/// `values` holds every server key, no other, each as typed JSON from a
/// file that sets them all (the API key included: every session is sent it).
#[tokio::test]
async fn values_are_typed_by_ini_key() {
    let text = "\
[Server]
Port = 8580
Hostname = 192.168.1.20,127.0.0.1
Threads = 12
ThreadsBatch = 16
ModelsMax = 2
ModelsMemoryMargin = 1024
ModelsDir = /srv/models
RocblasUseHipblaslt = false
CudaAllreduce = NCCL
CudaDisableGraphs = true
AttnRotDisable = true
RocprofAttach = true
Webui = false
WebuiMcpProxy = true
LogVerbosity = 3
SaveStateOnShutdown = true
StateDir = /srv/state
OpencodeBaseUrl = https://llm.example.com
OpencodeApiKey = sk-test
Unknown = 7
";
    let doc = app_holding(text).read_default().await.document();
    assert_eq!(
        doc["values"],
        json!({
            "Port": 8580,
            "Hostname": "192.168.1.20,127.0.0.1",
            "Threads": 12,
            "ThreadsBatch": 16,
            "ModelsMax": 2,
            "ModelsMemoryMargin": 1024,
            "ModelsDir": "/srv/models",
            "RocblasUseHipblaslt": "off",
            "CudaAllreduce": "nccl",
            "CudaDisableGraphs": true,
            "AttnRotDisable": true,
            "RocprofAttach": true,
            "Webui": false,
            "WebuiMcpProxy": true,
            "LogVerbosity": 3,
            "SaveStateOnShutdown": true,
            "StateDir": "/srv/state",
            "OpencodeBaseUrl": "https://llm.example.com",
            "OpencodeApiKey": "sk-test",
        })
    );
}

/// The other state of the tri-state select.
#[tokio::test]
async fn rocblas_true_reads_on() {
    let doc = app_holding("[Server]\nRocblasUseHipblaslt = true\n")
        .read_default()
        .await
        .document();
    assert_eq!(doc["values"]["RocblasUseHipblaslt"], json!("on"));
}

/// The shipped file: what it does not set is `null`, the plain switches
/// `false`; every key present.
#[tokio::test]
async fn unset_keys_are_null_in_the_shipped_file() {
    let doc = app_holding(SHIPPED).read_default().await.document();
    let values = doc["values"].as_object().expect("values object");
    let mut keys: Vec<&str> = values.keys().map(String::as_str).collect();
    keys.sort();
    let mut expected = KEYS.to_vec();
    expected.sort();
    assert_eq!(keys, expected, "exactly the server keys");
    assert_eq!(values["Port"], json!(9931));
    assert_eq!(values["Hostname"], json!("localhost"));
    assert_eq!(values["ModelsDir"], json!("/var/lib/llama.cpp/models"));
    for key in [
        "Threads",
        "ThreadsBatch",
        "ModelsMax",
        "ModelsMemoryMargin",
        "RocblasUseHipblaslt",
        "CudaAllreduce",
        "Webui",
        "WebuiMcpProxy",
        "LogVerbosity",
        "SaveStateOnShutdown",
        "OpencodeBaseUrl",
        "OpencodeApiKey",
    ] {
        assert_eq!(values[key], Value::Null, "{key}");
    }
    for key in ["CudaDisableGraphs", "AttnRotDisable", "RocprofAttach"] {
        assert_eq!(values[key], json!(false), "{key}");
    }
}

/// Values the configurator cannot read are unset, as it reads them: a word
/// for a number, an unknown AllReduce, a blank text, a non-boolean switch.
#[tokio::test]
async fn unreadable_values_are_null() {
    let text = "\
[Server]
Port = abc
Threads = 1.5
CudaAllreduce = bogus
Hostname =
ModelsDir =
Webui = maybe
RocblasUseHipblaslt = sometimes
";
    let doc = app_holding(text).read_default().await.document();
    for key in [
        "Port",
        "Threads",
        "CudaAllreduce",
        "Hostname",
        "ModelsDir",
        "Webui",
        "RocblasUseHipblaslt",
    ] {
        assert_eq!(doc["values"][key], Value::Null, "{key}");
    }
}

/// An empty file reads as every key unset, not as an error.
#[tokio::test]
async fn an_empty_file_is_every_key_unset() {
    let doc = app_holding("").read_default().await.document();
    assert_eq!(doc["values"]["Port"], Value::Null);
    assert_eq!(doc["values"]["ModelsDir"], Value::Null);
}

/// `defaults`: llama.cpp's and the configurator's numbers, the base URL from
/// the saved Hostname and Port (a loopback entry chosen, as
/// `ServerConfig::client_host`) even though the file overrides it, and the
/// instance's own folders.
#[tokio::test]
async fn defaults_are_resolved_from_the_saved_file() {
    let text = "\
[Server]
Port = 8580
Hostname = 192.168.1.20,127.0.0.1
OpencodeBaseUrl = https://llm.example.com
ModelsDir = /srv/models
StateDir = /srv/state
";
    let doc = app_holding(text).read_default().await.document();
    assert_eq!(
        doc["defaults"],
        json!({
            "Port": pai_core::server_cfg::DEFAULT_PORT,
            "ModelsMax": 4,
            "ModelsMemoryMargin": 0,
            "OpencodeBaseUrl": "http://127.0.0.1:8580",
            "ModelsDir": pai_core::paths::instance_models_dir().to_string_lossy(),
            "StateDir": pai_core::paths::instance_state_dir().to_string_lossy(),
        })
    );
}

/// With nothing saved, the base URL default is llama-server's own address.
#[tokio::test]
async fn the_base_url_default_of_an_empty_file_is_localhost_on_the_default_port() {
    let doc = app_holding("").read_default().await.document();
    assert_eq!(
        doc["defaults"]["OpencodeBaseUrl"],
        json!(format!(
            "http://localhost:{}",
            pai_core::server_cfg::DEFAULT_PORT
        ))
    );
}

/// The folder defaults are the instance's, not the folders of the account
/// the bff runs as (`server_cfg::default_models_dir`,
/// `slot_state::default_state_dir`, under its home).
#[cfg(any(target_os = "linux", windows))]
#[tokio::test]
async fn folder_defaults_are_never_the_bff_accounts() {
    let doc = app_holding("").read_default().await.document();
    let own_models = pai_core::server_cfg::default_models_dir();
    let own_state = pai_core::slot_state::default_state_dir()
        .to_string_lossy()
        .into_owned();
    assert_ne!(doc["defaults"]["ModelsDir"], json!(own_models));
    assert_ne!(doc["defaults"]["StateDir"], json!(own_state));
}

/// `limits`: the manager's own ranges for the three numbers, and 1 to this
/// machine's logical CPU count for the thread counts.
#[tokio::test]
async fn limits_are_the_checked_ranges_and_the_cpu_count() {
    let doc = app_holding(SHIPPED).read_default().await.document();
    let range = |key: &str| {
        let r = pai_core::checks::server_int_range(key).expect("a server integer key");
        json!([r.start(), r.end()])
    };
    let cpus = std::thread::available_parallelism()
        .expect("the CPU count")
        .get();
    assert_eq!(
        doc["limits"],
        json!({
            "Port": range("Port"),
            "ModelsMax": range("ModelsMax"),
            "ModelsMemoryMargin": range("ModelsMemoryMargin"),
            "Threads": [1, cpus],
            "ThreadsBatch": [1, cpus],
        })
    );
}

/// `interfaces`: this machine's, as the configurator lists them.
#[tokio::test]
async fn interfaces_are_the_machines() {
    let doc = app_holding(SHIPPED).read_default().await.document();
    let expected: Vec<Value> = pai_core::net_ifaces::interfaces()
        .into_iter()
        .map(|i| json!({ "value": i.value, "label": i.label }))
        .collect();
    assert_eq!(doc["interfaces"], Value::Array(expected));
}

/// The 64-bit FNV-1a of `text` in hex, the unkeyed revision the read used to
/// send. Recomputed here, from the code it replaces, so a test can say the
/// read no longer sends it.
fn fnv1a(text: &str) -> String {
    let hash = text.bytes().fold(0xcbf2_9ce4_8422_2325_u64, |h, b| {
        (h ^ u64::from(b)).wrapping_mul(0x0000_0100_0000_01b3)
    });
    format!("{hash:016x}")
}

/// A revision is 16 lowercase hex digits: the opaque text the web UI sends
/// back unchanged.
fn assert_revision_shape(value: &Value, what: &str) {
    let got = value
        .as_str()
        .unwrap_or_else(|| panic!("{what}: {value} is not a string"));
    assert_eq!(got.len(), 16, "{what}: {got}");
    assert!(
        got.bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b)),
        "{what}: {got} is not 16 lowercase hex digits"
    );
}

/// `revision`: 16 hex digits, the same for the same text read twice through
/// one router, another as soon as the text changes, even only in a comment,
/// and never the unkeyed FNV-1a of the text.
#[tokio::test]
async fn the_revision_follows_the_file_text() {
    let a = "[Server]\nPort = 8580\n";
    let commented = "; edited by hand\n[Server]\nPort = 8580\n";
    let other = "[Server]\nPort = 8581\n";
    let app = app_holding(a);
    let first = app.read_default().await.document();
    let again = app.read_default().await.document();
    let with_comment = app_holding(commented).read_default().await.document();
    let changed = app_holding(other).read_default().await.document();
    assert_revision_shape(&first["revision"], "the shape");
    assert_eq!(first["revision"], again["revision"], "one router, twice");
    assert_ne!(first["revision"], with_comment["revision"], "a comment");
    assert_ne!(first["revision"], changed["revision"], "a value");
    for text in [a, commented, other] {
        let doc = app_holding(text).read_default().await.document();
        assert_ne!(
            doc["revision"],
            json!(fnv1a(text)),
            "the unkeyed hash of {text:?}"
        );
    }
}

/// `revision` is SipHash-2-4 of the file text under the router's secret,
/// printed as 16 hex digits: the published vectors for the secret
/// `00 01 .. 0f`, all 64 of which the `siphasher` implementation reproduces.
/// The empty file is vector 0 of that list.
#[tokio::test]
async fn the_revision_is_the_keyed_hash_of_the_text() {
    let cases: [(&str, &str); 4] = [
        ("", "726fdb47dd0e0e31"),
        (SHIPPED, "b5c74895c1989291"),
        (WITH_KEY, "188192ef2476e7f0"),
        ("[Server]\nPort = 8580\n", "5124e870d1b0bbad"),
    ];
    for (text, want) in cases {
        let doc = app_holding_keyed(KEY_A, text)
            .read_default()
            .await
            .document();
        assert_eq!(doc["revision"], json!(want), "the text {text:?}");
    }
}

/// The secret is part of the function: the same file read through a router
/// built with another 16 bytes answers another revision, and that one is the
/// hash of the text under the new secret. A bff restart, which draws a new
/// secret, makes every open form stale, it does not make two files look
/// alike.
#[tokio::test]
async fn the_revision_depends_on_the_secret_of_the_router() {
    let under_a = app_holding_keyed(KEY_A, WITH_KEY)
        .read_default()
        .await
        .document();
    let under_b = app_holding_keyed(KEY_B, WITH_KEY)
        .read_default()
        .await
        .document();
    assert_revision_shape(&under_b["revision"], "the other router");
    assert_ne!(
        under_a["revision"], under_b["revision"],
        "two routers, one file"
    );
    assert_eq!(
        under_b["revision"],
        json!("8cb593f214fcdeb2"),
        "the secret 10 11 .. 1f"
    );
}

// ── The API key ─────────────────────────────────────────────────────────

/// A file that sets the API key, as the configurator saves it.
const WITH_KEY: &str = "[Server]\nPort = 8580\nOpencodeApiKey = sk-test\n";

/// The same file with only the key's value changed.
const WITH_OTHER_KEY: &str = "[Server]\nPort = 8580\nOpencodeApiKey = sk-other\n";

/// The same file as the manager sends it when it withheld the key: its own
/// blanking of the value (`service.rs` writes `OpencodeApiKey =`).
const KEY_BLANKED: &str = "[Server]\nPort = 8580\nOpencodeApiKey =\n";

/// Every signed-in session is sent the key as saved, with nothing withheld:
/// sign-in admits only members of `pai`, so the bff has no session to keep
/// the key from.
#[tokio::test]
async fn a_file_the_manager_did_not_withhold_sends_the_key_to_every_session() {
    for user in ["alice", "root"] {
        let r = app_holding(WITH_KEY).read_default_as(user).await;
        let doc = r.document();
        assert_eq!(doc["values"]["OpencodeApiKey"], json!("sk-test"), "{user}");
        assert!(doc.get("withheld").is_none(), "{user}: {doc}");
    }
}

/// A file whose only difference is the secret it carries is another
/// revision: the keyed hash covers the whole text, so a form opened before
/// another administrator saved a new key is refused rather than writing the
/// old key back.
#[tokio::test]
async fn a_file_differing_only_in_the_api_key_has_another_revision() {
    let with_key = app_holding(WITH_KEY).read_default().await.document();
    let with_other = app_holding(WITH_OTHER_KEY).read_default().await.document();
    assert_ne!(
        with_key["revision"], with_other["revision"],
        "the key's value alone"
    );
}

/// The manager's own flag reaches the reply: the key is `null`, the document
/// says it was withheld, and the rest of the file is sent.
#[tokio::test]
async fn the_managers_withheld_flag_yields_a_null_key_and_the_withheld_member() {
    let r = app_holding_withheld(KEY_BLANKED).read_default().await;
    let doc = r.document();
    assert_eq!(doc["values"]["OpencodeApiKey"], Value::Null, "{}", r.body);
    assert_eq!(doc["withheld"], json!(["OpencodeApiKey"]), "{}", r.body);
    assert_eq!(doc["values"]["Port"], json!(8580), "the rest is sent");
    assert!(!r.body.contains("sk-test"), "{}", r.body);
}

/// A withheld file never sends a key, for any session, even when the text
/// the manager sent still carries a value: the flag, not the session,
/// decides, and the secret stays out of the body.
#[tokio::test]
async fn a_withheld_file_never_sends_a_key_even_when_its_text_holds_one() {
    for user in ["alice", "root"] {
        let r = app_holding_withheld(WITH_KEY).read_default_as(user).await;
        let doc = r.document();
        assert_eq!(
            doc["values"]["OpencodeApiKey"],
            Value::Null,
            "{user}: {}",
            r.body
        );
        assert_eq!(
            doc["withheld"],
            json!(["OpencodeApiKey"]),
            "{user}: {}",
            r.body
        );
        assert!(!r.body.contains("sk-test"), "{user}: {}", r.body);
    }
}

/// With no key in the file nothing is withheld: `null` and no `withheld`
/// member, so the member comes from the manager's flag and never from an
/// unset value.
#[tokio::test]
async fn nothing_is_withheld_when_the_file_has_no_key() {
    let doc = app_holding("[Server]\nPort = 8580\n")
        .read_default()
        .await
        .document();
    assert_eq!(doc["values"]["OpencodeApiKey"], Value::Null);
    assert!(doc.get("withheld").is_none(), "{doc}");
}
