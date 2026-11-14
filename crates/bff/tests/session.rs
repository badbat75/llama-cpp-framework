//! Signing in to the web UI: `POST`, `GET` and `DELETE /api/session`
//! through `pai_bff::router_with_session`, a fake authenticator (never PAM or
//! LogonUser) and a clock the test moves. Each test hands its own router one
//! request at a time (`tower::ServiceExt::oneshot` on a clone, so the
//! sessions persist between requests) under a deadline, the client's address
//! given as the server's `ConnectInfo` would.
//!
//! The contract (fe/briefs/api-contract.md) and the choices made here:
//! - `POST /api/session` `{"username", "password"}`: 200
//!   `{"user", "canConfigure"}` from the authenticator's identity and a
//!   cookie that is HttpOnly, SameSite=Strict, Path=/, an opaque random id;
//!   no `Secure` while the bff serves plain HTTP on loopback only (a browser
//!   would not send it back over http).
//! - Wrong user or password: 401 `session-bad-credentials`, the two not told
//!   apart; an empty password is refused without asking the authenticator.
//! - A body that is not `{"username": <text>, "password": <text>}` as JSON:
//!   400 `session-malformed`.
//! - The authenticator cannot check at all: 503 `session-unavailable`.
//! - 5 failed attempts for one user from one client address within 60 s
//!   lock that pair for 60 s from the 5th: 429 `session-too-many-attempts`
//!   with the seconds left, the authenticator not asked.
//! - `GET` and `DELETE` with no session, an unknown one or an expired one:
//!   401 `session-required`.
//! - A session ends after 30 minutes without a request that uses it.

use axum::body::Body;
use axum::extract::ConnectInfo;
use axum::http::{header, Method, Request, StatusCode};
use axum::Router;
use http_body_util::BodyExt;
use pai_bff::session::{AuthError, Authenticator, Clock, Identity};
use pai_core::client::ClientError;
use serde_json::{json, Value};
use std::net::SocketAddr;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};
use tower::ServiceExt;

/// How long a test waits for the bff's answer before failing.
const DEADLINE: Duration = Duration::from_secs(5);

/// The idle timeout and the lockout chosen for the bff.
const IDLE: Duration = Duration::from_secs(30 * 60);
const MAX_FAILURES: usize = 5;
const LOCKOUT_SECS: u64 = 60;

const CLIENT: &str = "127.0.0.1:51000";

// ── Fakes ───────────────────────────────────────────────────────────────

/// One account the fake knows: what is typed, and what the authenticator
/// answers for it.
struct Account {
    typed_user: &'static str,
    password: &'static str,
    identity: Identity,
}

/// An authenticator over a fixed list of accounts. It logs the user name of
/// every call, never the password.
struct FakeAuth {
    accounts: Vec<Account>,
    unavailable: bool,
    calls: Mutex<Vec<String>>,
}

impl FakeAuth {
    fn new(accounts: Vec<Account>) -> FakeAuth {
        FakeAuth {
            accounts,
            unavailable: false,
            calls: Mutex::new(Vec::new()),
        }
    }

    fn calls(&self) -> Vec<String> {
        self.calls.lock().unwrap_or_else(|e| e.into_inner()).clone()
    }
}

impl Authenticator for FakeAuth {
    fn authenticate(&self, user: &str, password: &str) -> Result<Identity, AuthError> {
        self.calls
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .push(user.to_string());
        if self.unavailable {
            return Err(AuthError::Unavailable);
        }
        self.accounts
            .iter()
            .find(|a| a.typed_user == user && a.password == password)
            .map(|a| a.identity.clone())
            .ok_or(AuthError::BadCredentials)
    }
}

/// `alice` (no `configure`) and `root-admin` (an administrator). The struct
/// literal pins every field of `Identity`: one more (a password kept with
/// the session) would not compile.
fn accounts() -> Vec<Account> {
    vec![
        Account {
            typed_user: "alice",
            password: "correct horse",
            identity: Identity {
                user: "alice".into(),
                can_configure: false,
            },
        },
        Account {
            typed_user: "bob",
            password: "battery staple",
            identity: Identity {
                user: "bob".into(),
                can_configure: false,
            },
        },
        Account {
            typed_user: "root-admin",
            password: "s3cret-Admin!",
            identity: Identity {
                user: "root-admin".into(),
                can_configure: true,
            },
        },
    ]
}

/// A clock that moves only when the test says so.
#[derive(Clone)]
struct FakeClock(Arc<Mutex<Instant>>);

impl FakeClock {
    fn new() -> FakeClock {
        FakeClock(Arc::new(Mutex::new(Instant::now())))
    }

    fn clock(&self) -> Clock {
        let now = self.0.clone();
        Arc::new(move || *now.lock().unwrap_or_else(|e| e.into_inner()))
    }

    fn advance(&self, by: Duration) {
        *self.0.lock().unwrap_or_else(|e| e.into_inner()) += by;
    }
}

struct App {
    router: Router,
    auth: Arc<FakeAuth>,
    clock: FakeClock,
}

fn app_with(auth: FakeAuth) -> App {
    let auth = Arc::new(auth);
    let clock = FakeClock::new();
    // The session routes never ask the manager.
    let connect: pai_bff::Connect = Arc::new(|| Err(ClientError::Unavailable));
    let router = pai_bff::router_with_session(
        connect,
        Duration::from_millis(200),
        auth.clone(),
        clock.clock(),
    );
    App {
        router,
        auth,
        clock,
    }
}

fn app() -> App {
    app_with(FakeAuth::new(accounts()))
}

// ── Requests ────────────────────────────────────────────────────────────

struct Reply {
    status: StatusCode,
    content_type: String,
    set_cookies: Vec<String>,
    /// Every header, `name: value` per line, for the "no password" check.
    headers: String,
    body: String,
}

impl Reply {
    fn json(&self) -> Value {
        serde_json::from_str(&self.body)
            .unwrap_or_else(|e| panic!("not JSON ({e}): {:?}", self.body))
    }

    fn assert_error(&self, status: StatusCode, key: &str, args: Value) {
        assert_eq!(self.status, status, "{}", self.body);
        assert!(
            self.content_type.starts_with("application/json"),
            "content-type {:?}",
            self.content_type
        );
        assert_eq!(
            self.json(),
            json!({ "key": key, "args": args }),
            "{}",
            self.body
        );
    }

    /// The one session cookie this reply sets, parsed.
    fn cookie(&self) -> SetCookie {
        assert_eq!(
            self.set_cookies.len(),
            1,
            "one Set-Cookie: {:?}",
            self.set_cookies
        );
        SetCookie::parse(&self.set_cookies[0])
    }
}

/// A `Set-Cookie` value: name, value and the attributes, names lowercased.
#[derive(Debug)]
struct SetCookie {
    name: String,
    value: String,
    attrs: Vec<(String, String)>,
}

impl SetCookie {
    fn parse(raw: &str) -> SetCookie {
        let mut parts = raw.split(';').map(str::trim);
        let (name, value) = parts
            .next()
            .and_then(|p| p.split_once('='))
            .unwrap_or_else(|| panic!("no name=value in {raw:?}"));
        let attrs = parts
            .filter(|p| !p.is_empty())
            .map(|p| match p.split_once('=') {
                Some((k, v)) => (k.trim().to_ascii_lowercase(), v.trim().to_string()),
                None => (p.to_ascii_lowercase(), String::new()),
            })
            .collect();
        SetCookie {
            name: name.trim().to_string(),
            value: value.trim().to_string(),
            attrs,
        }
    }

    fn attr(&self, name: &str) -> Option<&str> {
        self.attrs
            .iter()
            .find(|(k, _)| k == name)
            .map(|(_, v)| v.as_str())
    }

    /// What the browser sends back: `name=value`.
    fn header(&self) -> String {
        format!("{}={}", self.name, self.value)
    }
}

struct Req<'a> {
    method: Method,
    cookie: Option<&'a str>,
    /// (content type, body)
    body: Option<(&'a str, String)>,
    client: &'a str,
}

impl<'a> Req<'a> {
    fn new(method: Method) -> Req<'a> {
        Req {
            method,
            cookie: None,
            body: None,
            client: CLIENT,
        }
    }
}

async fn send(router: &Router, req: Req<'_>) -> Reply {
    let mut builder = Request::builder().method(req.method).uri("/api/session");
    if let Some(cookie) = req.cookie {
        builder = builder.header(header::COOKIE, cookie);
    }
    let body = match req.body {
        Some((content_type, text)) => {
            builder = builder.header(header::CONTENT_TYPE, content_type);
            Body::from(text)
        }
        None => Body::empty(),
    };
    let mut request = builder.body(body).expect("request");
    let client: SocketAddr = req.client.parse().expect("client address");
    request.extensions_mut().insert(ConnectInfo(client));
    let response = tokio::time::timeout(DEADLINE, router.clone().oneshot(request))
        .await
        .unwrap_or_else(|_| panic!("no answer within {DEADLINE:?}"))
        .expect("infallible");
    let status = response.status();
    let headers = response.headers();
    let content_type = headers
        .get(header::CONTENT_TYPE)
        .map(|v| v.to_str().expect("ascii content-type").to_string())
        .unwrap_or_default();
    let set_cookies = headers
        .get_all(header::SET_COOKIE)
        .iter()
        .map(|v| v.to_str().expect("ascii set-cookie").to_string())
        .collect();
    let all_headers = headers
        .iter()
        .map(|(k, v)| format!("{k}: {}", String::from_utf8_lossy(v.as_bytes())))
        .collect::<Vec<_>>()
        .join("\n");
    let bytes = response
        .into_body()
        .collect()
        .await
        .expect("body")
        .to_bytes();
    Reply {
        status,
        content_type,
        set_cookies,
        headers: all_headers,
        body: String::from_utf8(bytes.to_vec()).expect("utf-8 body"),
    }
}

async fn sign_in_from(router: &Router, client: &str, user: &str, password: &str) -> Reply {
    let mut req = Req::new(Method::POST);
    req.body = Some((
        "application/json",
        json!({ "username": user, "password": password }).to_string(),
    ));
    req.client = client;
    send(router, req).await
}

async fn sign_in(router: &Router, user: &str, password: &str) -> Reply {
    sign_in_from(router, CLIENT, user, password).await
}

/// Signs `alice` in and gives back her cookie as the browser sends it.
async fn alice_cookie(router: &Router) -> String {
    let r = sign_in(router, "alice", "correct horse").await;
    assert_eq!(r.status, StatusCode::OK, "{}", r.body);
    r.cookie().header()
}

async fn get_session(router: &Router, cookie: Option<&str>) -> Reply {
    let mut req = Req::new(Method::GET);
    req.cookie = cookie;
    send(router, req).await
}

async fn delete_session(router: &Router, cookie: Option<&str>) -> Reply {
    let mut req = Req::new(Method::DELETE);
    req.cookie = cookie;
    send(router, req).await
}

// ── Failure paths first ─────────────────────────────────────────────────

/// The wrong password: 401 `session-bad-credentials`, no cookie.
#[tokio::test]
async fn a_wrong_password_is_bad_credentials_without_a_cookie() {
    let a = app();
    let r = sign_in(&a.router, "alice", "wrong").await;
    r.assert_error(
        StatusCode::UNAUTHORIZED,
        "session-bad-credentials",
        json!([]),
    );
    assert!(r.set_cookies.is_empty(), "{:?}", r.set_cookies);
    assert_eq!(a.auth.calls(), ["alice"]);
}

/// An account that does not exist answers exactly as a wrong password, so
/// the sign-in does not tell which names exist.
#[tokio::test]
async fn an_unknown_user_answers_as_a_wrong_password() {
    let a = app();
    let unknown = sign_in(&a.router, "mallory", "whatever").await;
    let wrong = sign_in(&a.router, "alice", "whatever").await;
    unknown.assert_error(
        StatusCode::UNAUTHORIZED,
        "session-bad-credentials",
        json!([]),
    );
    assert_eq!(unknown.body, wrong.body);
    assert!(unknown.set_cookies.is_empty(), "{:?}", unknown.set_cookies);
}

/// An empty password is refused before the authenticator is asked: an
/// account with no password must not sign in to a web service. The fake here
/// would accept it.
#[tokio::test]
async fn an_empty_password_is_refused_without_asking() {
    let a = app_with(FakeAuth::new(vec![Account {
        typed_user: "nopass",
        password: "",
        identity: Identity {
            user: "nopass".into(),
            can_configure: true,
        },
    }]));
    let r = sign_in(&a.router, "nopass", "").await;
    r.assert_error(
        StatusCode::UNAUTHORIZED,
        "session-bad-credentials",
        json!([]),
    );
    assert!(r.set_cookies.is_empty(), "{:?}", r.set_cookies);
    assert!(a.auth.calls().is_empty(), "asked: {:?}", a.auth.calls());
}

/// A body that is not the sign-in object: 400 `session-malformed`, the
/// authenticator not asked, no cookie. One status and key for every shape,
/// the wrong content type included.
#[tokio::test]
async fn a_malformed_body_is_refused_without_asking() {
    let a = app();
    let cases: [(&str, &str); 8] = [
        ("application/json", "not json"),
        ("application/json", ""),
        ("application/json", r#"{"username": "alice"}"#),
        ("application/json", r#"{"password": "correct horse"}"#),
        (
            "application/json",
            r#"{"username": 7, "password": "correct horse"}"#,
        ),
        ("application/json", r#"["alice", "correct horse"]"#),
        (
            "application/json",
            r#"{"username": "", "password": "correct horse"}"#,
        ),
        (
            "text/plain",
            r#"{"username": "alice", "password": "correct horse"}"#,
        ),
    ];
    for (content_type, body) in cases {
        let mut req = Req::new(Method::POST);
        req.body = Some((content_type, body.to_string()));
        let r = send(&a.router, req).await;
        assert_eq!(
            r.status,
            StatusCode::BAD_REQUEST,
            "{content_type} {body:?}: {}",
            r.body
        );
        r.assert_error(StatusCode::BAD_REQUEST, "session-malformed", json!([]));
        assert!(r.set_cookies.is_empty(), "{body:?}: {:?}", r.set_cookies);
    }
    assert!(a.auth.calls().is_empty(), "asked: {:?}", a.auth.calls());
}

/// The system cannot check credentials (PAM or LogonUser failing for
/// another reason): 503 `session-unavailable`, no cookie.
#[tokio::test]
async fn an_authenticator_that_cannot_check_is_unavailable() {
    let mut auth = FakeAuth::new(accounts());
    auth.unavailable = true;
    let a = app_with(auth);
    let r = sign_in(&a.router, "alice", "correct horse").await;
    r.assert_error(
        StatusCode::SERVICE_UNAVAILABLE,
        "session-unavailable",
        json!([]),
    );
    assert!(r.set_cookies.is_empty(), "{:?}", r.set_cookies);
}

/// After 5 failures for one user from one address, the next attempt is
/// refused with 429 and the seconds left, even with the right password,
/// and the authenticator is not asked; the lock lifts after 60 s.
#[tokio::test]
async fn repeated_failures_lock_the_user_for_a_while() {
    let a = app();
    for n in 0..MAX_FAILURES {
        let r = sign_in(&a.router, "alice", "wrong").await;
        r.assert_error(
            StatusCode::UNAUTHORIZED,
            "session-bad-credentials",
            json!([]),
        );
        assert_eq!(a.auth.calls().len(), n + 1);
    }
    let r = sign_in(&a.router, "alice", "correct horse").await;
    r.assert_error(
        StatusCode::TOO_MANY_REQUESTS,
        "session-too-many-attempts",
        json!([LOCKOUT_SECS]),
    );
    assert!(r.set_cookies.is_empty(), "{:?}", r.set_cookies);
    assert_eq!(a.auth.calls().len(), MAX_FAILURES, "asked while locked");

    a.clock.advance(Duration::from_secs(45));
    let r = sign_in(&a.router, "alice", "wrong").await;
    r.assert_error(
        StatusCode::TOO_MANY_REQUESTS,
        "session-too-many-attempts",
        json!([LOCKOUT_SECS - 45]),
    );
    assert_eq!(a.auth.calls().len(), MAX_FAILURES, "asked while locked");

    a.clock.advance(Duration::from_secs(LOCKOUT_SECS - 45 + 1));
    let r = sign_in(&a.router, "alice", "correct horse").await;
    assert_eq!(r.status, StatusCode::OK, "{}", r.body);
}

/// The lock is for one user from one address: the same user from another
/// address, and another user from the same address, still sign in.
#[tokio::test]
async fn the_lock_is_per_user_and_client_address() {
    let a = app();
    for _ in 0..MAX_FAILURES {
        sign_in(&a.router, "alice", "wrong").await;
    }
    let locked = sign_in(&a.router, "alice", "correct horse").await;
    assert_eq!(
        locked.status,
        StatusCode::TOO_MANY_REQUESTS,
        "{}",
        locked.body
    );

    // Another port of the same address is the same client.
    let other_port = sign_in_from(&a.router, "127.0.0.1:52999", "alice", "correct horse").await;
    assert_eq!(
        other_port.status,
        StatusCode::TOO_MANY_REQUESTS,
        "{}",
        other_port.body
    );

    let other_addr = sign_in_from(&a.router, "127.0.0.2:51000", "alice", "correct horse").await;
    assert_eq!(other_addr.status, StatusCode::OK, "{}", other_addr.body);

    let other_user = sign_in(&a.router, "bob", "battery staple").await;
    assert_eq!(other_user.status, StatusCode::OK, "{}", other_user.body);
}

/// Failures more than 60 s apart do not add up to a lock.
#[tokio::test]
async fn failures_outside_the_window_do_not_lock() {
    let a = app();
    for _ in 0..MAX_FAILURES - 1 {
        sign_in(&a.router, "alice", "wrong").await;
    }
    a.clock.advance(Duration::from_secs(LOCKOUT_SECS + 1));
    for _ in 0..MAX_FAILURES - 1 {
        let r = sign_in(&a.router, "alice", "wrong").await;
        r.assert_error(
            StatusCode::UNAUTHORIZED,
            "session-bad-credentials",
            json!([]),
        );
    }
    let r = sign_in(&a.router, "alice", "correct horse").await;
    assert_eq!(r.status, StatusCode::OK, "{}", r.body);
}

/// The lock cannot be dodged by changing the case of the user name: Windows
/// (LogonUser) takes `Alice` and `ALICE` for `alice`, so each spelling may not
/// get 5 attempts of its own.
#[tokio::test]
async fn the_lock_ignores_the_case_of_the_user_name() {
    let a = app();
    for _ in 0..MAX_FAILURES {
        sign_in(&a.router, "alice", "wrong").await;
    }
    for spelling in ["Alice", "ALICE"] {
        let r = sign_in(&a.router, spelling, "wrong").await;
        r.assert_error(
            StatusCode::TOO_MANY_REQUESTS,
            "session-too-many-attempts",
            json!([LOCKOUT_SECS]),
        );
    }
    assert_eq!(a.auth.calls().len(), MAX_FAILURES, "asked while locked");
}

/// An authenticator that takes a while to answer, as PAM does on a wrong
/// password, and counts its calls.
struct SlowAuth {
    calls: Mutex<usize>,
}

impl Authenticator for SlowAuth {
    fn authenticate(&self, _user: &str, _password: &str) -> Result<Identity, AuthError> {
        *self.calls.lock().unwrap_or_else(|e| e.into_inner()) += 1;
        std::thread::sleep(Duration::from_millis(200));
        Err(AuthError::BadCredentials)
    }
}

/// Guesses sent at once cannot overrun the lock: the 6th and later must not
/// reach the authenticator just because the first 5 have not answered yet.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn guesses_sent_at_once_do_not_overrun_the_lock() {
    let auth = Arc::new(SlowAuth {
        calls: Mutex::new(0),
    });
    let connect: pai_bff::Connect = Arc::new(|| Err(ClientError::Unavailable));
    let router = pai_bff::router_with_session(
        connect,
        Duration::from_millis(200),
        auth.clone(),
        FakeClock::new().clock(),
    );
    let mut tasks = tokio::task::JoinSet::new();
    for _ in 0..4 * MAX_FAILURES {
        let router = router.clone();
        tasks.spawn(async move { sign_in(&router, "alice", "wrong").await.status });
    }
    let mut statuses = Vec::new();
    while let Some(status) = tasks.join_next().await {
        statuses.push(status.expect("task"));
    }
    let calls = *auth.calls.lock().unwrap_or_else(|e| e.into_inner());
    assert!(
        calls <= MAX_FAILURES,
        "the authenticator was asked {calls} times; statuses {statuses:?}"
    );
}

/// No cookie: 401 `session-required`.
#[tokio::test]
async fn get_without_a_cookie_is_session_required() {
    let a = app();
    get_session(&a.router, None).await.assert_error(
        StatusCode::UNAUTHORIZED,
        "session-required",
        json!([]),
    );
}

/// A cookie the bff never issued (forged, or from before a restart): 401.
#[tokio::test]
async fn get_with_an_unknown_cookie_is_session_required() {
    let a = app();
    let issued = sign_in(&a.router, "alice", "correct horse").await.cookie();
    let forged = format!(
        "{}=AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA",
        issued.name
    );
    get_session(&a.router, Some(&forged)).await.assert_error(
        StatusCode::UNAUTHORIZED,
        "session-required",
        json!([]),
    );
    // The user's name is not a session.
    let by_name = format!("{}=alice", issued.name);
    get_session(&a.router, Some(&by_name)).await.assert_error(
        StatusCode::UNAUTHORIZED,
        "session-required",
        json!([]),
    );
}

/// Signing out with no session: 401 `session-required` (the route is for
/// the signed in; the fe signs out whatever it answers).
#[tokio::test]
async fn delete_without_a_session_is_session_required() {
    let a = app();
    delete_session(&a.router, None).await.assert_error(
        StatusCode::UNAUTHORIZED,
        "session-required",
        json!([]),
    );
}

/// A session unused for longer than the idle timeout has ended; every use
/// before that moves the deadline.
#[tokio::test]
async fn a_session_ends_after_the_idle_timeout() {
    let a = app();
    let cookie = alice_cookie(&a.router).await;
    let short = IDLE - Duration::from_secs(60);
    a.clock.advance(short);
    let r = get_session(&a.router, Some(&cookie)).await;
    assert_eq!(r.status, StatusCode::OK, "{}", r.body);
    a.clock.advance(short);
    let r = get_session(&a.router, Some(&cookie)).await;
    assert_eq!(
        r.status,
        StatusCode::OK,
        "used within the timeout: {}",
        r.body
    );

    a.clock.advance(IDLE + Duration::from_secs(1));
    get_session(&a.router, Some(&cookie)).await.assert_error(
        StatusCode::UNAUTHORIZED,
        "session-required",
        json!([]),
    );
    delete_session(&a.router, Some(&cookie)).await.assert_error(
        StatusCode::UNAUTHORIZED,
        "session-required",
        json!([]),
    );
}

/// The password never comes back: not in the body or headers of a sign-in
/// that succeeded or failed, nor in the session read after.
#[tokio::test]
async fn the_password_is_never_sent_back() {
    let a = app();
    let password = "correct horse";
    let failed = sign_in(&a.router, "alice", "correct horsX").await;
    assert!(!failed.body.contains("correct hors"), "{}", failed.body);
    assert!(
        !failed.headers.contains("correct hors"),
        "{}",
        failed.headers
    );
    let ok = sign_in(&a.router, "alice", password).await;
    assert_eq!(ok.status, StatusCode::OK, "{}", ok.body);
    assert!(!ok.body.contains(password), "{}", ok.body);
    assert!(!ok.headers.contains(password), "{}", ok.headers);
    assert!(!ok.headers.contains("correct%20horse"), "{}", ok.headers);
    let cookie = ok.cookie().header();
    let read = get_session(&a.router, Some(&cookie)).await;
    assert!(!read.body.contains(password), "{}", read.body);
}

// ── Signing in ──────────────────────────────────────────────────────────

/// The right password: 200 with exactly the user and `canConfigure`, and
/// the session cookie with its attributes. No `Secure`: the bff serves plain
/// HTTP on loopback, where a browser would not send a Secure cookie back
/// (to add with TLS); no `Domain`, so it stays on this host.
#[tokio::test]
async fn signing_in_answers_the_identity_and_sets_the_cookie() {
    let a = app();
    let r = sign_in(&a.router, "alice", "correct horse").await;
    assert_eq!(r.status, StatusCode::OK, "{}", r.body);
    assert!(
        r.content_type.starts_with("application/json"),
        "content-type {:?}",
        r.content_type
    );
    assert_eq!(r.json(), json!({ "user": "alice", "canConfigure": false }));

    let c = r.cookie();
    assert!(c.attr("httponly").is_some(), "HttpOnly: {c:?}");
    assert_eq!(
        c.attr("samesite").map(str::to_ascii_lowercase).as_deref(),
        Some("strict"),
        "{c:?}"
    );
    assert_eq!(c.attr("path"), Some("/"), "{c:?}");
    assert!(c.attr("secure").is_none(), "no Secure on plain HTTP: {c:?}");
    assert!(c.attr("domain").is_none(), "no Domain: {c:?}");
}

/// The cookie's value is an opaque random id: at least 128 bits in a
/// cookie-safe alphabet, not derived from the user name, different at each
/// sign-in of the same user.
#[tokio::test]
async fn the_cookie_is_an_opaque_random_id() {
    let a = app();
    let first = sign_in(&a.router, "alice", "correct horse").await.cookie();
    let second = sign_in(&a.router, "alice", "correct horse").await.cookie();
    for c in [&first, &second] {
        assert!(c.value.len() >= 22, "too short for 128 bits: {c:?}");
        assert!(
            c.value
                .chars()
                .all(|ch| ch.is_ascii_alphanumeric() || ch == '-' || ch == '_'),
            "not a plain token: {c:?}"
        );
        assert!(
            !c.value.to_ascii_lowercase().contains("alice"),
            "names the user: {c:?}"
        );
    }
    assert_eq!(first.name, second.name);
    assert_ne!(first.value, second.value, "the same id twice");
}

/// `canConfigure` is the authenticator's answer: true for an
/// administrator.
#[tokio::test]
async fn can_configure_comes_from_the_authenticator() {
    let a = app();
    let r = sign_in(&a.router, "root-admin", "s3cret-Admin!").await;
    assert_eq!(r.status, StatusCode::OK, "{}", r.body);
    assert_eq!(
        r.json(),
        json!({ "user": "root-admin", "canConfigure": true })
    );
    let cookie = r.cookie().header();
    let read = get_session(&a.router, Some(&cookie)).await;
    assert_eq!(
        read.json(),
        json!({ "user": "root-admin", "canConfigure": true })
    );
}

/// The user shown is the account the authenticator names, not what was
/// typed (Windows accepts `Alice` for `alice`).
#[tokio::test]
async fn the_user_is_the_authenticators_name_not_the_typed_one() {
    let a = app_with(FakeAuth::new(vec![Account {
        typed_user: "Alice",
        password: "correct horse",
        identity: Identity {
            user: "alice".into(),
            can_configure: false,
        },
    }]));
    let r = sign_in(&a.router, "Alice", "correct horse").await;
    assert_eq!(r.status, StatusCode::OK, "{}", r.body);
    assert_eq!(r.json(), json!({ "user": "alice", "canConfigure": false }));
}

// ── Reading and ending the session ──────────────────────────────────────

/// With the cookie, `GET` answers who is signed in; the cookie is found
/// among the browser's other cookies.
#[tokio::test]
async fn get_with_the_cookie_answers_the_identity() {
    let a = app();
    let cookie = alice_cookie(&a.router).await;
    let r = get_session(&a.router, Some(&cookie)).await;
    assert_eq!(r.status, StatusCode::OK, "{}", r.body);
    assert_eq!(r.json(), json!({ "user": "alice", "canConfigure": false }));

    let among = format!("theme=dark; {cookie}; lang=en");
    let r = get_session(&a.router, Some(&among)).await;
    assert_eq!(r.status, StatusCode::OK, "{}", r.body);
    assert_eq!(r.json(), json!({ "user": "alice", "canConfigure": false }));
}

/// Signing out: 204, the cookie cleared in the browser (same name and path,
/// expired), and the id no longer valid on the server even if replayed.
/// Another session of the same user stays.
#[tokio::test]
async fn delete_ends_the_session_and_clears_the_cookie() {
    let a = app();
    let cookie = alice_cookie(&a.router).await;
    let other = alice_cookie(&a.router).await;

    let r = delete_session(&a.router, Some(&cookie)).await;
    assert_eq!(r.status, StatusCode::NO_CONTENT, "{}", r.body);
    let cleared = r.cookie();
    assert_eq!(
        format!("{}=", cleared.name),
        cookie
            .split_once('=')
            .map(|(n, _)| format!("{n}="))
            .unwrap()
    );
    assert!(cleared.value.is_empty(), "{cleared:?}");
    assert_eq!(cleared.attr("path"), Some("/"), "{cleared:?}");
    assert_eq!(cleared.attr("max-age"), Some("0"), "{cleared:?}");

    get_session(&a.router, Some(&cookie)).await.assert_error(
        StatusCode::UNAUTHORIZED,
        "session-required",
        json!([]),
    );
    let r = get_session(&a.router, Some(&other)).await;
    assert_eq!(r.status, StatusCode::OK, "the other session: {}", r.body);
}
