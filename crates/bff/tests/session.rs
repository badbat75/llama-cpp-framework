//! Signing in to the web UI: `POST`, `GET` and `DELETE /api/session`
//! through `pai_bff::router_with_session`, a fake authenticator (never PAM or
//! LogonUser) and a clock the test moves. Each test hands its own router one
//! request at a time (`tower::ServiceExt::oneshot` on a clone, so the
//! sessions persist between requests) under a deadline, the client's address
//! given as the server's `ConnectInfo` would. The tests that bound one check
//! build the session routes through
//! `pai_bff::session::routes_with_sign_in_timeout`, handing the bound the
//! service keeps as a const a short value.
//!
//! The contract (fe/briefs/api-contract.md) and the choices made here:
//! - `POST /api/session` `{"username", "password"}`: 200
//!   `{"user"}` (the whole reply: the web UI no longer reads a role from
//!   it) and a cookie that is HttpOnly, SameSite=Strict, Path=/, an opaque random id;
//!   no `Secure` while the bff serves plain HTTP on loopback only (a browser
//!   would not send it back over http).
//! - Wrong user or password: 401 `session-bad-credentials`, the two not told
//!   apart; an empty password is refused without asking the authenticator.
//! - A body that is not `{"username": <text>, "password": <text>}` as JSON:
//!   400 `session-malformed`.
//! - The authenticator cannot check at all: 503 `session-unavailable`.
//! - The password is right but the account is not an administrator (not a
//!   member of the `pai` group, the manager's second answer): 403
//!   `session-not-administrator`, answered only after the password check,
//!   and the 429 check runs first. A refusal is no credential failure, so it
//!   adds nothing to the failures: refusals are counted in a list of their
//!   own, per (user name in lowercase, client address) within the same
//!   window; refusals plus the checks in flight reaching the limit makes the
//!   next request 429, and the count expires after the window.
//! - One check is bounded by the sign-in timeout: a check that has not
//!   answered by the bound is 503 `session-unavailable`, counts as no
//!   lockout failure and frees its slot, so the request after it is checked
//!   and not refused 429.
//! - A check that cannot answer: 503, also no lockout failure (it is the
//!   bff's failure, not the caller's guess).
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
use std::path::Path;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};
use tower::ServiceExt;

/// How long a test waits for the bff's answer before failing.
const DEADLINE: Duration = Duration::from_secs(5);

/// The session idle timeout and the lockout chosen for the bff.
const IDLE: Duration = Duration::from_secs(30 * 60);
const MAX_FAILURES: usize = 5;
const LOCKOUT_SECS: u64 = 60;

/// The sign-in timeout handed to the session routes in the tests that bound
/// one check: short so the test runs fast (the service keeps its own as a
/// const, pinned by `one_check_is_bounded_by_a_10_second_const`).
const SIGN_IN_TIMEOUT: Duration = Duration::from_millis(20);

/// How long the `sleepy` account's check blocks: 20 times the bound, so a
/// bounded check answers long before the fake's answer lands, and an
/// unbounded one is late by an order of magnitude.
const HANG: Duration = Duration::from_millis(400);

const CLIENT: &str = "127.0.0.1:51000";

/// The revision secret these routers are built with: the session routes
/// never read it.
const REVISION_KEY: [u8; 16] = [0; 16];

// ── Fakes ───────────────────────────────────────────────────────────────

/// One account the fake knows: what is typed, what the fake answers for it
/// and how long the check blocks before answering. The answer is the
/// manager's `(ok, can_configure)` as `SystemAuthenticator` maps it:
/// `Ok(Identity)` for a member with the right password, `NotAdministrator`
/// for a right password of a non-member, `Unavailable` when the manager
/// cannot be asked.
struct Account {
    typed_user: &'static str,
    password: &'static str,
    answer: Result<Identity, AuthError>,
    /// How long `authenticate` sleeps for this account before answering:
    /// zero answers at once, `HANG` stands for a manager that hangs.
    hang: Duration,
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

    /// How many calls were made for `user`.
    fn calls_for(&self, user: &str) -> usize {
        self.calls().iter().filter(|u| *u == user).count()
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
        let account = self
            .accounts
            .iter()
            .find(|a| a.typed_user == user && a.password == password);
        let hang = account.map_or(Duration::ZERO, |a| a.hang);
        if hang > Duration::ZERO {
            std::thread::sleep(hang);
        }
        account.map_or_else(|| Err(AuthError::BadCredentials), |a| a.answer.clone())
    }
}

/// A right password of a non-member (the account `carol`, and the spellings
/// of it a Windows logon takes for the same name).
fn non_member(typed_user: &'static str) -> Account {
    Account {
        typed_user,
        password: "plain carol",
        answer: Err(AuthError::NotAdministrator),
        hang: Duration::ZERO,
    }
}

/// `alice`, `bob` and `root-admin` (all members of `pai`, the group sign-in
/// admits: every session is an administrator's), all with right passwords;
/// `carol`, a right password of a non-member (`NotAdministrator`); `dave`,
/// whose check cannot answer (`Unavailable`); `sleepy`, whose check answers
/// only after the sign-in bound (`HANG`).
/// The struct literal pins every field of `Identity`: one more (a password
/// kept with the session) would not compile.
fn accounts() -> Vec<Account> {
    vec![
        Account {
            typed_user: "alice",
            password: "correct horse",
            answer: Ok(Identity {
                user: "alice".into(),
            }),
            hang: Duration::ZERO,
        },
        Account {
            typed_user: "bob",
            password: "battery staple",
            answer: Ok(Identity { user: "bob".into() }),
            hang: Duration::ZERO,
        },
        Account {
            typed_user: "root-admin",
            password: "s3cret-Admin!",
            answer: Ok(Identity {
                user: "root-admin".into(),
            }),
            hang: Duration::ZERO,
        },
        non_member("carol"),
        Account {
            typed_user: "dave",
            password: "right dave",
            answer: Err(AuthError::Unavailable),
            hang: Duration::ZERO,
        },
        Account {
            typed_user: "sleepy",
            password: "right sleepy",
            // A member with the right password: the check reaches 200, it
            // only never arrives in time.
            answer: Ok(Identity {
                user: "sleepy".into(),
            }),
            hang: HANG,
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
    let connect: pai_bff::Connect = Arc::new(|_| Err(ClientError::Unavailable));
    let router = pai_bff::router_with_session(
        connect,
        Duration::from_millis(200),
        auth.clone(),
        clock.clock(),
        REVISION_KEY,
    );
    App {
        router,
        auth,
        clock,
    }
}

/// The session routes with one check bounded by `sign_in_timeout` (the
/// service keeps its bound as a const, too long for a test to wait out).
fn app_with_sign_in_timeout(auth: FakeAuth, sign_in_timeout: Duration) -> App {
    let auth = Arc::new(auth);
    let clock = FakeClock::new();
    let router =
        pai_bff::session::routes_with_sign_in_timeout(auth.clone(), clock.clock(), sign_in_timeout);
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

    /// The success body of the session routes: exactly `{"user": <name>}`,
    /// one key, no other (fe/briefs/api-contract.md: the session reply is
    /// `{"user"}`, the web UI no longer reads a role from it).
    fn assert_user_reply(&self, user: &str) {
        let value = self.json();
        let obj = value
            .as_object()
            .unwrap_or_else(|| panic!("not an object: {}", self.body));
        assert_eq!(
            obj.len(),
            1,
            "the reply must carry the user alone: {}",
            self.body
        );
        assert_eq!(
            obj.get("user").and_then(Value::as_str),
            Some(user),
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
        answer: Ok(Identity {
            user: "nopass".into(),
        }),
        hang: Duration::ZERO,
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

// ── The administrator check (the manager's second answer) ──────────────

/// The password is right and the account is not a member of the `pai`
/// group: 403 `session-not-administrator` with empty args, no cookie, the
/// authenticator asked exactly once (the refusal comes only after the
/// password check), the password nowhere in the reply.
#[tokio::test]
async fn a_right_password_from_a_non_administrator_is_forbidden() {
    let a = app();
    let r = sign_in(&a.router, "carol", "plain carol").await;
    r.assert_error(
        StatusCode::FORBIDDEN,
        "session-not-administrator",
        json!([]),
    );
    assert!(r.set_cookies.is_empty(), "{:?}", r.set_cookies);
    assert_eq!(a.auth.calls(), ["carol"], "asked once, after the check");
    assert!(!r.body.contains("plain carol"), "{}", r.body);
    assert!(!r.headers.contains("plain carol"), "{}", r.headers);
}

/// A non-administrator account with an empty password gets the plain
/// credential refusal, not the 403: the administrator answer is reached
/// only after a right password (the empty password is refused without
/// asking, as for any account).
#[tokio::test]
async fn an_empty_password_never_reaches_the_administrator_answer() {
    let a = app();
    sign_in(&a.router, "carol", "").await.assert_error(
        StatusCode::UNAUTHORIZED,
        "session-bad-credentials",
        json!([]),
    );
    assert!(a.auth.calls().is_empty(), "asked: {:?}", a.auth.calls());
}

/// The 429 check comes first: a locked pair signing in with the right
/// password of a non-administrator account is refused 429, the
/// authenticator not asked: the 403 is not reached while locked.
#[tokio::test]
async fn the_lockout_is_checked_before_the_administrator_answer() {
    let a = app();
    for _ in 0..MAX_FAILURES {
        sign_in(&a.router, "carol", "wrong").await;
    }
    let r = sign_in(&a.router, "carol", "plain carol").await;
    r.assert_error(
        StatusCode::TOO_MANY_REQUESTS,
        "session-too-many-attempts",
        json!([LOCKOUT_SECS]),
    );
    assert_eq!(a.auth.calls().len(), MAX_FAILURES, "asked while locked");
}

/// A right password refused 403 is no credential failure: the failures
/// counter keeps counting only the bad passwords, the refusals have a
/// counter of their own. 4 failures and 4 refusals: the next bad password
/// is answered 401 (asked: it is the 5th failure, and it locks), it is not
/// refused 429 by the refusals.
#[tokio::test]
async fn a_non_administrator_refusal_adds_nothing_to_the_credential_failures() {
    let a = app();
    for _ in 0..MAX_FAILURES - 1 {
        sign_in(&a.router, "carol", "wrong").await.assert_error(
            StatusCode::UNAUTHORIZED,
            "session-bad-credentials",
            json!([]),
        );
    }
    for _ in 0..MAX_FAILURES - 1 {
        sign_in(&a.router, "carol", "plain carol")
            .await
            .assert_error(
                StatusCode::FORBIDDEN,
                "session-not-administrator",
                json!([]),
            );
    }
    // The 5th credential failure: asked and answered 401, and it locks.
    sign_in(&a.router, "carol", "wrong").await.assert_error(
        StatusCode::UNAUTHORIZED,
        "session-bad-credentials",
        json!([]),
    );
    sign_in(&a.router, "carol", "plain carol")
        .await
        .assert_error(
            StatusCode::TOO_MANY_REQUESTS,
            "session-too-many-attempts",
            json!([LOCKOUT_SECS]),
        );
    assert_eq!(
        a.auth.calls().len(),
        2 * (MAX_FAILURES - 1) + 1,
        "asked while locked: {:?}",
        a.auth.calls()
    );
}

/// The refusals' own counter: 5 right passwords of a non-member from one
/// address, each asked and answered 403, and the 6th request is refused 429
/// without asking, even with the right password. The count is kept for the
/// lowercased name, as the failures' is: changing the case of it (Windows
/// takes `Carol` for `carol`) moves nothing.
#[tokio::test]
async fn five_right_password_refusals_lock_the_pair() {
    let mut list = accounts();
    list.push(non_member("Carol"));
    list.push(non_member("CAROL"));
    let a = app_with(FakeAuth::new(list));
    for spelling in ["carol", "Carol", "CAROL", "carol", "CAROL"] {
        sign_in(&a.router, spelling, "plain carol")
            .await
            .assert_error(
                StatusCode::FORBIDDEN,
                "session-not-administrator",
                json!([]),
            );
    }
    assert_eq!(a.auth.calls().len(), MAX_FAILURES, "asked once each");
    sign_in(&a.router, "Carol", "plain carol")
        .await
        .assert_error(
            StatusCode::TOO_MANY_REQUESTS,
            "session-too-many-attempts",
            json!([LOCKOUT_SECS]),
        );
    assert_eq!(a.auth.calls().len(), MAX_FAILURES, "asked while locked");

    // The refusals expire with the window: the pair is checked again.
    a.clock.advance(Duration::from_secs(LOCKOUT_SECS + 1));
    sign_in(&a.router, "carol", "plain carol")
        .await
        .assert_error(
            StatusCode::FORBIDDEN,
            "session-not-administrator",
            json!([]),
        );
}

/// The refusals' count is for the same window as the failures: refusals
/// farther apart than the window do not add up. 4 refusals, the clock moves
/// past the window, then 5 refusals: every one is asked and answered 403
/// (the first 4 left the count, else the 5th would be refused 429), and
/// only the request after the 5th is 429.
#[tokio::test]
async fn refusals_outside_the_window_do_not_lock() {
    let a = app();
    for _ in 0..MAX_FAILURES - 1 {
        sign_in(&a.router, "carol", "plain carol")
            .await
            .assert_error(
                StatusCode::FORBIDDEN,
                "session-not-administrator",
                json!([]),
            );
    }
    a.clock.advance(Duration::from_secs(LOCKOUT_SECS + 1));
    for _ in 0..MAX_FAILURES {
        sign_in(&a.router, "carol", "plain carol")
            .await
            .assert_error(
                StatusCode::FORBIDDEN,
                "session-not-administrator",
                json!([]),
            );
    }
    sign_in(&a.router, "carol", "plain carol")
        .await
        .assert_error(
            StatusCode::TOO_MANY_REQUESTS,
            "session-too-many-attempts",
            json!([LOCKOUT_SECS]),
        );
    assert_eq!(
        a.auth.calls().len(),
        (MAX_FAILURES - 1) + MAX_FAILURES,
        "asked while locked"
    );
}

/// The two counters are apart: 4 credential failures and 4 refusals are 4
/// and 4, never 8. The 5th refusal is asked and answered 403 (the failures
/// are no part of its count), and only with the refusals at 5 is the next
/// request refused 429.
#[tokio::test]
async fn refusals_lock_the_pair_on_their_own_count() {
    let a = app();
    for _ in 0..MAX_FAILURES - 1 {
        sign_in(&a.router, "carol", "wrong").await.assert_error(
            StatusCode::UNAUTHORIZED,
            "session-bad-credentials",
            json!([]),
        );
    }
    for _ in 0..MAX_FAILURES - 1 {
        sign_in(&a.router, "carol", "plain carol")
            .await
            .assert_error(
                StatusCode::FORBIDDEN,
                "session-not-administrator",
                json!([]),
            );
    }
    sign_in(&a.router, "carol", "plain carol")
        .await
        .assert_error(
            StatusCode::FORBIDDEN,
            "session-not-administrator",
            json!([]),
        );
    sign_in(&a.router, "carol", "plain carol")
        .await
        .assert_error(
            StatusCode::TOO_MANY_REQUESTS,
            "session-too-many-attempts",
            json!([LOCKOUT_SECS]),
        );
    assert_eq!(
        a.auth.calls().len(),
        2 * (MAX_FAILURES - 1) + 1,
        "asked while locked"
    );
}

/// The counter is for one typed user from one client address: refusals that
/// reach the limit for `carol` leave every other name of the same address
/// signing in, a real administrator (the manager's `(true, true)`) included,
/// and the same name from another address is its own pair: asked, answered
/// 403 (a 403 answers only after the check).
#[tokio::test]
async fn refusals_for_one_name_do_not_lock_another() {
    let a = app();
    for _ in 0..MAX_FAILURES {
        sign_in(&a.router, "carol", "plain carol")
            .await
            .assert_error(
                StatusCode::FORBIDDEN,
                "session-not-administrator",
                json!([]),
            );
    }
    sign_in(&a.router, "carol", "plain carol")
        .await
        .assert_error(
            StatusCode::TOO_MANY_REQUESTS,
            "session-too-many-attempts",
            json!([LOCKOUT_SECS]),
        );
    assert_eq!(
        a.auth.calls_for("carol"),
        MAX_FAILURES,
        "asked while locked: {:?}",
        a.auth.calls()
    );

    let admin = sign_in(&a.router, "root-admin", "s3cret-Admin!").await;
    assert_eq!(admin.status, StatusCode::OK, "{}", admin.body);
    admin.assert_user_reply("root-admin");
    let member = sign_in(&a.router, "alice", "correct horse").await;
    assert_eq!(member.status, StatusCode::OK, "{}", member.body);

    // The same name from another address is another pair.
    sign_in_from(&a.router, "127.0.0.2:51000", "carol", "plain carol")
        .await
        .assert_error(
            StatusCode::FORBIDDEN,
            "session-not-administrator",
            json!([]),
        );
    // The 403 answers only after the check, so this fresh pair is asked
    // once: the locked pair's count stays at the limit, it is the new
    // pair's own first refusal.
    assert_eq!(
        a.auth.calls_for("carol"),
        MAX_FAILURES + 1,
        "the other address is its own pair, asked once: {:?}",
        a.auth.calls()
    );
}

/// A check that cannot answer is 503 and counts as no lockout failure:
/// five of them between credential failures add no lock (the 503 is the
/// bff's failure, not the caller's guess).
#[tokio::test]
async fn an_unavailable_check_does_not_count_toward_the_lockout() {
    let a = app();
    for _ in 0..MAX_FAILURES - 1 {
        sign_in(&a.router, "dave", "wrong").await;
    }
    for _ in 0..MAX_FAILURES {
        sign_in(&a.router, "dave", "right dave").await.assert_error(
            StatusCode::SERVICE_UNAVAILABLE,
            "session-unavailable",
            json!([]),
        );
    }
    sign_in(&a.router, "dave", "wrong").await.assert_error(
        StatusCode::UNAUTHORIZED,
        "session-bad-credentials",
        json!([]),
    );
    sign_in(&a.router, "dave", "right dave").await.assert_error(
        StatusCode::TOO_MANY_REQUESTS,
        "session-too-many-attempts",
        json!([LOCKOUT_SECS]),
    );
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
    let connect: pai_bff::Connect = Arc::new(|_| Err(ClientError::Unavailable));
    let router = pai_bff::router_with_session(
        connect,
        Duration::from_millis(200),
        auth.clone(),
        FakeClock::new().clock(),
        REVISION_KEY,
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

/// Right passwords of a non-administrator sent at once: the refusals are
/// counted with the checks in flight, so the burst asks at most
/// [`MAX_FAILURES`] times, every request that was not asked is refused 429,
/// and the refusals the burst recorded keep the pair out of the request
/// after it.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn non_administrator_guesses_sent_at_once_ask_at_most_the_limit() {
    let a = app();
    let burst = 4 * MAX_FAILURES;
    let mut tasks = tokio::task::JoinSet::new();
    for _ in 0..burst {
        let router = a.router.clone();
        tasks.spawn(async move { sign_in(&router, "carol", "plain carol").await.status });
    }
    let mut statuses = Vec::new();
    while let Some(status) = tasks.join_next().await {
        statuses.push(status.expect("task"));
    }
    let calls = a.auth.calls().len();
    assert!(
        (1..=MAX_FAILURES).contains(&calls),
        "the authenticator was asked {calls} times; statuses {statuses:?}"
    );
    let forbidden = statuses
        .iter()
        .filter(|s| **s == StatusCode::FORBIDDEN)
        .count();
    let locked = statuses
        .iter()
        .filter(|s| **s == StatusCode::TOO_MANY_REQUESTS)
        .count();
    assert_eq!(
        forbidden, calls,
        "every check asked answers 403: {statuses:?}"
    );
    assert_eq!(
        forbidden + locked,
        burst,
        "the rest are refused 429: {statuses:?}"
    );

    // The refusals stand on their own once the checks that were in flight
    // are gone: the pair is refused, and not asked.
    sign_in(&a.router, "carol", "plain carol")
        .await
        .assert_error(
            StatusCode::TOO_MANY_REQUESTS,
            "session-too-many-attempts",
            json!([LOCKOUT_SECS]),
        );
    assert_eq!(a.auth.calls().len(), calls, "asked while locked");
}

// ── The bound on one check (the sign-in timeout) ────────────────────────

/// A check that has not answered by the sign-in timeout: 503
/// `session-unavailable`, no cookie, and the reply comes at the bound, not
/// when the check finally finishes (the fake answers 200 at `HANG`, 20 times
/// the bound handed to the router).
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_check_that_outlives_the_sign_in_timeout_is_unavailable() {
    let a = app_with_sign_in_timeout(FakeAuth::new(accounts()), SIGN_IN_TIMEOUT);
    let start = Instant::now();
    let r = sign_in(&a.router, "sleepy", "right sleepy").await;
    let took = start.elapsed();
    r.assert_error(
        StatusCode::SERVICE_UNAVAILABLE,
        "session-unavailable",
        json!([]),
    );
    assert!(r.set_cookies.is_empty(), "{:?}", r.set_cookies);
    assert!(
        took < HANG / 2,
        "the reply came after {took:?}: the request waited for the answer \
         ({HANG:?}) instead of answering at the bound ({SIGN_IN_TIMEOUT:?})"
    );
    assert_eq!(a.auth.calls_for("sleepy"), 1, "asked once");
}

/// A check that times out is the bff's failure and not the caller's guess:
/// it adds nothing to the lockout, and it gives its slot back.
/// [`MAX_FAILURES`] of them, and the request after them is still asked and
/// answered 503, not refused 429.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_timed_out_check_does_not_count_toward_the_lockout() {
    let a = app_with_sign_in_timeout(FakeAuth::new(accounts()), SIGN_IN_TIMEOUT);
    for n in 0..MAX_FAILURES {
        sign_in(&a.router, "sleepy", "right sleepy")
            .await
            .assert_error(
                StatusCode::SERVICE_UNAVAILABLE,
                "session-unavailable",
                json!([]),
            );
        assert!(
            a.auth.calls_for("sleepy") > n,
            "the request was not checked: {:?}",
            a.auth.calls()
        );
    }
    sign_in(&a.router, "sleepy", "right sleepy")
        .await
        .assert_error(
            StatusCode::SERVICE_UNAVAILABLE,
            "session-unavailable",
            json!([]),
        );
    assert_eq!(
        a.auth.calls_for("sleepy"),
        MAX_FAILURES + 1,
        "checked, not refused 429"
    );
}

/// The bound the service uses for one check: a `SIGN_IN_TIMEOUT` const of
/// 10 s, declared in `src/` and named there again (its value cannot be
/// waited out in a test; the two tests above pin the effect with a short
/// bound handed to `session::routes_with_sign_in_timeout`).
#[test]
fn one_check_is_bounded_by_a_10_second_const() {
    let mut declared = false;
    let mut mentions = 0;
    for file in ["src/session.rs", "src/lib.rs"] {
        let text = std::fs::read_to_string(Path::new(env!("CARGO_MANIFEST_DIR")).join(file))
            .unwrap_or_else(|e| panic!("{file}: {e}"));
        for line in text.lines() {
            if line.contains("const SIGN_IN_TIMEOUT") {
                assert!(
                    line.contains("from_secs(10)"),
                    "{file}: the bound is not 10 s: {line}"
                );
                declared = true;
            }
            if line.contains("SIGN_IN_TIMEOUT") {
                mentions += 1;
            }
        }
    }
    assert!(
        declared,
        "no `const SIGN_IN_TIMEOUT` of 10 s in src/session.rs or src/lib.rs"
    );
    assert!(
        mentions >= 2,
        "SIGN_IN_TIMEOUT is declared and handed to the session routes nowhere: \
         {mentions} mentions"
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

/// The right password: 200 with exactly `{"user": <name>}` (the reply
/// carries the user alone, no role: fe/briefs/api-contract.md), and
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
    r.assert_user_reply("alice");

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

/// A real administrator signs in, and the reply of both routes is the
/// user alone: the web UI no longer reads a role from the session reply,
/// so an administrator's sign-in and session read answer exactly
/// `{"user"}`, with no key carrying the role.
#[tokio::test]
async fn an_administrators_reply_is_the_user_alone() {
    let a = app();
    let r = sign_in(&a.router, "root-admin", "s3cret-Admin!").await;
    assert_eq!(r.status, StatusCode::OK, "{}", r.body);
    r.assert_user_reply("root-admin");
    let cookie = r.cookie().header();
    let read = get_session(&a.router, Some(&cookie)).await;
    assert_eq!(read.status, StatusCode::OK, "{}", read.body);
    read.assert_user_reply("root-admin");
}

/// The user shown is the account the authenticator names, not what was
/// typed (Windows accepts `Alice` for `alice`).
#[tokio::test]
async fn the_user_is_the_authenticators_name_not_the_typed_one() {
    let a = app_with(FakeAuth::new(vec![Account {
        typed_user: "Alice",
        password: "correct horse",
        answer: Ok(Identity {
            user: "alice".into(),
        }),
        hang: Duration::ZERO,
    }]));
    let r = sign_in(&a.router, "Alice", "correct horse").await;
    assert_eq!(r.status, StatusCode::OK, "{}", r.body);
    r.assert_user_reply("alice");
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
    r.assert_user_reply("alice");

    let among = format!("theme=dark; {cookie}; lang=en");
    let r = get_session(&a.router, Some(&among)).await;
    assert_eq!(r.status, StatusCode::OK, "{}", r.body);
    r.assert_user_reply("alice");
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
