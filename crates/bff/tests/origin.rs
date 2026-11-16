//! The same-origin guard: `POST` and `DELETE /api/session`, the control verbs
//! `POST /api/instances/{id}/{start,stop,restart}` and the boot write
//! `PUT /api/instances/{id}/boot`, through `pai_bff::router_with_session` with
//! a fake manager client and a fake authenticator (never the real bus, pipe,
//! systemd or SCM). Each request runs under a deadline, so a bff that waits on
//! the manager fails instead of hanging.
//!
//! Why this guard exists: the session cookie is `SameSite=Strict`, which
//! compares the *site* of the page, that is its scheme, host and registrable
//! domain, and never its port. A page of another web app served by the same
//! machine on `http://localhost:<other port>` is the same site, so a browser
//! sends the cookie with its request, a body-less POST included. The bff
//! listens on loopback, so the port is the only fact that tells one local web
//! app from another, and the session alone cannot tell a request the signed-in
//! user's own page sent from one some other local page sent.
//!
//! The rule pinned here, and the payload cap the same guard shares:
//! - A state-changing request (`POST` and `DELETE /api/session`, the three
//!   control verbs, the boot write) is refused 403 `{"key":
//!   "request-cross-origin", "args": []}` when its `Origin` header is present
//!   and its host and port differ from the request's `Host` header, or when
//!   its `Sec-Fetch-Site` header is present and is neither `same-origin` nor
//!   `none`. The manager is never connected to and no session is touched: a
//!   refused sign-out leaves the session alive, a refused sign-in records no
//!   lockout failure and sets no cookie, and a refused request leaves the
//!   session's idle deadline where it was.
//! - The request passes as it does today when its `Origin` names the `Host`,
//!   when `Sec-Fetch-Site` is `same-origin` or `none`, and when it carries
//!   neither header, the shape of `curl`, a CLI and any client that sends no
//!   fetch metadata.
//! - A `GET` is never refused for these headers: reading is what the page does
//!   on its own, and `GET /api/instances` and `GET .../boot` answer without a
//!   session at all.
//! - A JSON body larger than 64 KiB on the boot write and on `POST
//!   /api/session` is refused 413 `{"key": "payload-too-large", "args": []}`
//!   before the manager is connected to: the sign-in body is the only one an
//!   anonymous caller may send, and the bff has no use for a big one. The cap
//!   is above 64 KiB and not at it, on the boot write and on the sign-in
//!   alike: a body of exactly that size is read and answered.
//! - A header that is present but is not text (a byte above `0x7F`) is a
//!   refusal, not an absent header: the bytes are on the wire, so a client
//!   sent the header, and the bff cannot read it as this bff's own origin. A
//!   browser cannot send such a value, a client built on a socket can.
//! - The same header twice is a refusal, even when the first value is this
//!   bff's own: the bff cannot tell which one the browser meant, so it takes
//!   neither.
//! - The `Host` line decides for every request, the reads and the sign-in
//!   included. The bff listens on loopback only (`listen.rs`), so a `Host`
//!   that is not a loopback name is a page that believes it sits at that name
//!   while its name resolves to this machine (a DNS rebinding). It is refused
//!   403 `{"key": "request-bad-host", "args": []}`, before the session is read
//!   and before the manager is connected to, and the sign-in is refused the
//!   same way, so a foreign name gives no answer about a password. A loopback
//!   name is `localhost`, an address in `127.0.0.0/8`, or `[::1]`, each with
//!   an optional port, in any case. A request with no `Host` at all passes:
//!   HTTP/1.0 sends none, and that is `curl`, the CLI and a raw client.
//! - A method that is not a read (`GET`, `HEAD`, `OPTIONS`) is guarded, so a
//!   `PATCH` from a foreign page is 403 `request-cross-origin` and not the
//!   route's 405. From this bff's own page a `PATCH` is still the route's to
//!   answer: the guard decides where a request came from, not which verbs
//!   exist.
//! - The IPv6 loopback is bracketed in the `Host`, so `http://[::1]:3000` is
//!   this bff's own origin against the same `Host` and not against
//!   `[::1]:9999`. An `Origin` with a user, a path, a query, a fragment or no
//!   scheme names no origin and is refused.
//!
//! `PUT /api/instances/{id}/server` (the server-file write) is a state-changing
//! route the bff does not serve yet, so nothing here pins it: when it is
//! routed, the same guard must cover it.
//!
//! What is not pinned, because the rule says nothing about it and a browser
//! never sends it: the case of an `Origin` or a `Sec-Fetch-Site` value (both
//! are lowercase on the wire), the scheme of a matching `Origin` (the bff is
//! plain HTTP on loopback, and the rule compares host and port), and which
//! check answers a request that is both cross-origin and over the cap: it is
//! refused, by either rule.

use axum::body::Body;
use axum::extract::ConnectInfo;
use axum::http::{header, HeaderName, HeaderValue, Method, Request, StatusCode};
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

/// The failed sign-ins that lock a (user, client address) pair (`session.rs`).
/// A refused sign-in is none of these, so this many of them must leave the
/// pair's own limit untouched.
const MAX_FAILURES: usize = 5;

/// The key of a request refused for where it came from.
const CROSS_ORIGIN: &str = "request-cross-origin";

/// The key of a request addressed at a name this bff is not: the bff answers
/// on loopback only (`listen.rs`), so a `Host` naming any other host is a page
/// that reached this bff under a name it does not serve (a DNS rebinding).
const BAD_HOST: &str = "request-bad-host";

/// The key of a body over [`MAX_BODY`].
const TOO_LARGE: &str = "payload-too-large";

/// The cap on a JSON body, in bytes: exactly this is accepted, one byte more
/// is refused.
const MAX_BODY: usize = 64 * 1024;

/// The host and port the bff is addressed as: the `Host` header of every
/// request that carries one.
const HOST: &str = "localhost:3000";

/// The credentials [`FakeAuth`] signs in, as the sign-in route takes them.
const CREDENTIALS: &str = r#"{"username":"alice","password":"correct horse"}"#;

/// The origin of a page served by the bff itself.
const SAME_ORIGIN: &str = "http://localhost:3000";

/// The origin of a page of another web app on the same machine: the same site,
/// another port, so a browser sends the session cookie with its request.
const OTHER_PORT: &str = "http://localhost:9999";

/// The engine every instance runs (`lib.rs`).
const ENGINE: &str = "llama.cpp";

/// The routes that change something, all of them behind a session: the three
/// control verbs and the boot write.
const VERBS: [&str; 3] = [
    "/api/instances/default/start",
    "/api/instances/default/stop",
    "/api/instances/default/restart",
];

type Log = Arc<Mutex<Vec<String>>>;

/// The principal every connect was asked for, in order: `None` is a call the
/// bff makes as its own account.
type Principals = Arc<Mutex<Vec<Option<String>>>>;

fn principals_of(p: &Principals) -> Vec<Option<String>> {
    p.lock().unwrap_or_else(|e| e.into_inner()).clone()
}

// ── Fakes ───────────────────────────────────────────────────────────────

/// A manager that accepts every call and logs it (`start default`,
/// `set_boot_enabled default true`, `boot_enabled default`, `read <name>`).
/// Its boot state starts at `boot_initial` and the write leaves it at what was
/// asked, so the read-back answers it: a `PUT` that only echoed its body is
/// told apart from a write and a read.
struct FakeManager {
    state: Arc<Mutex<bool>>,
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
        Ok(*self.state.lock().unwrap_or_else(|e| e.into_inner()))
    }
    fn set_boot_enabled(&self, id: &str, enabled: bool) -> Result<(), ClientError> {
        self.record(format!("set_boot_enabled {id} {enabled}"));
        *self.state.lock().unwrap_or_else(|e| e.into_inner()) = enabled;
        Ok(())
    }
    fn authenticate(&self, _user: &str, _password: &str) -> Result<(bool, bool), ClientError> {
        Err(ClientError::Unavailable)
    }
}

/// The accounts the fake knows: `alice` and `root`, both members of `pai`,
/// both with the password `correct horse`. It logs the user name of every
/// check it is asked for, never the password: a request refused by the guard
/// must never reach it.
struct FakeAuth {
    calls: Arc<Mutex<Vec<String>>>,
}

impl FakeAuth {
    fn asked(&self) -> Vec<String> {
        self.calls.lock().unwrap_or_else(|e| e.into_inner()).clone()
    }
}

impl Authenticator for FakeAuth {
    fn authenticate(&self, user: &str, password: &str) -> Result<Identity, AuthError> {
        self.calls
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .push(user.to_string());
        match (user, password) {
            ("alice", "correct horse") | ("root", "correct horse") => Ok(Identity {
                user: user.to_string(),
            }),
            _ => Err(AuthError::BadCredentials),
        }
    }
}

/// A clock that moves only when the test says so (the idle-deadline test).
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
    auth: Arc<FakeAuth>,
}

/// The app: a manager that accepts everything and whose boot flag starts
/// `false`.
fn new_app() -> App {
    let log = Log::default();
    let connects = Arc::new(AtomicUsize::new(0));
    let principals = Principals::default();
    let state = Arc::new(Mutex::new(false));
    let (l, c, p) = (log.clone(), connects.clone(), principals.clone());
    let connect: Connect = Arc::new(move |principal: Option<&str>| {
        c.fetch_add(1, Ordering::SeqCst);
        p.lock()
            .unwrap_or_else(|e| e.into_inner())
            .push(principal.map(str::to_string));
        Ok(Box::new(FakeManager {
            state: state.clone(),
            log: l.clone(),
        }) as Client)
    });
    let clock = FakeClock::new();
    let auth = Arc::new(FakeAuth {
        calls: Arc::new(Mutex::new(Vec::new())),
    });
    let router = pai_bff::router_with_session(connect, TIMEOUT, auth.clone(), clock.clock());
    App {
        router,
        clock,
        log,
        connects,
        principals,
        auth,
    }
}

impl App {
    fn calls(&self) -> Vec<String> {
        self.log.lock().unwrap_or_else(|e| e.into_inner()).clone()
    }

    fn asked(&self) -> Vec<String> {
        self.auth.asked()
    }

    /// Sign `alice` in and return the cookie the browser sends back. The
    /// request carries no `Origin` and no `Host`, the shape of `curl`.
    async fn sign_in(&self) -> String {
        let reply = self
            .ask(
                Method::POST,
                "/api/session",
                None,
                Some(json!({ "username": "alice", "password": "correct horse" }).to_string()),
                Some("application/json"),
                &[],
            )
            .await;
        reply.assert_ok();
        reply.set_cookie.expect("a session cookie")
    }

    /// `GET`, no fetch metadata of its own.
    async fn get(&self, uri: &str, cookie: Option<&str>) -> Reply {
        self.ask(Method::GET, uri, cookie, None, None, &[]).await
    }

    /// A control verb: no body, no fetch metadata.
    async fn post(&self, uri: &str, cookie: Option<&str>) -> Reply {
        self.ask(Method::POST, uri, cookie, None, None, &[]).await
    }

    /// `PUT` with a JSON body, no fetch metadata.
    async fn put(&self, uri: &str, body: &str, cookie: Option<&str>) -> Reply {
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

    /// A control verb sent by a browser with the headers in `extra`.
    async fn post_with(&self, uri: &str, cookie: &str, extra: &[(&str, &str)]) -> Reply {
        self.ask(Method::POST, uri, Some(cookie), None, None, extra)
            .await
    }

    /// The boot write sent by a browser with the headers in `extra`.
    async fn put_with(&self, uri: &str, cookie: &str, extra: &[(&str, &str)]) -> Reply {
        self.ask(
            Method::PUT,
            uri,
            Some(cookie),
            Some(json!({ "enabled": true }).to_string()),
            Some("application/json"),
            extra,
        )
        .await
    }

    /// `DELETE /api/session` with the headers in `extra`.
    async fn delete_session_with(&self, cookie: &str, extra: &[(&str, &str)]) -> Reply {
        self.ask(
            Method::DELETE,
            "/api/session",
            Some(cookie),
            None,
            None,
            extra,
        )
        .await
    }

    /// `POST /api/session` for `user`, sent by a browser with the headers in
    /// `extra`.
    async fn sign_in_with(&self, user: &str, extra: &[(&str, &str)]) -> Reply {
        self.ask(
            Method::POST,
            "/api/session",
            None,
            Some(json!({ "username": user, "password": "correct horse" }).to_string()),
            Some("application/json"),
            extra,
        )
        .await
    }

    /// The same, over a body the test built itself.
    async fn sign_in_body(&self, body: &str, extra: &[(&str, &str)]) -> Reply {
        self.ask(
            Method::POST,
            "/api/session",
            None,
            Some(body.to_string()),
            Some("application/json"),
            extra,
        )
        .await
    }

    /// `POST /api/session` with raw header values.
    async fn sign_in_raw(&self, body: &str, extra: &[(&str, HeaderValue)]) -> Reply {
        self.ask_raw(
            Method::POST,
            "/api/session",
            None,
            Some(body.to_string()),
            Some("application/json"),
            extra,
        )
        .await
    }

    /// The boot write with raw header values.
    async fn boot_raw(&self, cookie: &str, extra: &[(&str, HeaderValue)]) -> Reply {
        self.ask_raw(
            Method::PUT,
            "/api/instances/default/boot",
            Some(cookie),
            Some(json!({ "enabled": true }).to_string()),
            Some("application/json"),
            extra,
        )
        .await
    }

    /// A control verb, a sign-out or a sign-in with raw header values: a value
    /// a browser cannot send and any client can.
    async fn ask_raw(
        &self,
        method: Method,
        uri: &str,
        cookie: Option<&str>,
        body: Option<String>,
        content_type: Option<&str>,
        extra: &[(&str, HeaderValue)],
    ) -> Reply {
        let mut builder = Request::builder().method(method).uri(uri);
        if let Some(c) = cookie {
            builder = builder.header(header::COOKIE, c);
        }
        if let Some(ct) = content_type {
            builder = builder.header(header::CONTENT_TYPE, ct);
        }
        for (name, value) in extra {
            let name = name.parse::<HeaderName>().expect("header name");
            builder = builder.header(name, value.clone());
        }
        let body = match body {
            Some(text) => Body::from(text),
            None => Body::empty(),
        };
        let mut request = builder.body(body).expect("request");
        request
            .extensions_mut()
            .insert(ConnectInfo(CLIENT.parse::<SocketAddr>().expect("address")));
        let response = tokio::time::timeout(DEADLINE, self.router.clone().oneshot(request))
            .await
            .unwrap_or_else(|_| panic!("no answer to {uri} within {DEADLINE:?}"))
            .expect("infallible");
        let status = response.status();
        let set_cookie = response
            .headers()
            .get(header::SET_COOKIE)
            .map(|v| v.to_str().expect("ascii cookie").to_string())
            .map(|full| {
                full.split(';')
                    .next()
                    .expect("name=value")
                    .trim()
                    .to_string()
            });
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
            set_cookie,
            body: String::from_utf8(bytes.to_vec()).expect("utf-8 body"),
        }
    }

    /// The request with text header values: `Request::builder` appends a header
    /// whose name is already there, so a pair of the same name in `extra`
    /// reaches the bff as the two values a browser's request line carries.
    async fn ask(
        &self,
        method: Method,
        uri: &str,
        cookie: Option<&str>,
        body: Option<String>,
        content_type: Option<&str>,
        extra: &[(&str, &str)],
    ) -> Reply {
        let raw: Vec<(&str, HeaderValue)> = extra
            .iter()
            .map(|(name, value)| (*name, HeaderValue::from_str(value).expect("a text value")))
            .collect();
        self.ask_raw(method, uri, cookie, body, content_type, &raw)
            .await
    }

    /// Nothing was asked of the manager: no connect, no call, no principal.
    fn assert_asks_nothing(&self, what: &str) {
        assert_eq!(
            self.connects.load(Ordering::SeqCst),
            0,
            "{what}: no connect"
        );
        assert!(self.calls().is_empty(), "{what}: {:?}", self.calls());
        assert!(
            principals_of(&self.principals).is_empty(),
            "{what}: a principal was named"
        );
    }
}

struct Reply {
    status: StatusCode,
    content_type: String,
    /// The session cookie the reply set, as `name=value`, when it set one.
    set_cookie: Option<String>,
    body: String,
}

impl Reply {
    fn json(&self) -> Value {
        serde_json::from_str(&self.body)
            .unwrap_or_else(|e| panic!("not JSON ({e}): {:?}", self.body))
    }

    /// An error answer: `status` and `{"key": key, "args": []}`, nothing else.
    fn assert_error(&self, status: StatusCode, key: &str) {
        assert_eq!(self.status, status, "the body was {:?}", self.body);
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

    /// Refused for where it came from, and no session was handed to the caller.
    fn assert_cross_origin(&self) {
        self.assert_error(StatusCode::FORBIDDEN, CROSS_ORIGIN);
        assert!(
            self.set_cookie.is_none(),
            "a refused request sets no cookie: {:?}",
            self.set_cookie
        );
    }

    /// Refused for the name it was addressed at, and no session was handed to
    /// the caller: the host rule runs before the session, so the cookie is not
    /// read and a sign-in is not checked.
    fn assert_bad_host(&self) {
        self.assert_error(StatusCode::FORBIDDEN, BAD_HOST);
        assert!(
            self.set_cookie.is_none(),
            "a refused request sets no cookie: {:?}",
            self.set_cookie
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
        let value = self.json();
        let obj = value
            .as_object()
            .unwrap_or_else(|| panic!("not an object: {}", self.body));
        assert_eq!(
            obj.len(),
            1,
            "the reply carries the flag alone: {}",
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

/// The headers of a browser request from a page served at `origin` on this
/// host: the target as `Host`, the page's `Origin`.
fn from(origin: &str) -> Vec<(&'static str, &str)> {
    vec![("host", HOST), ("origin", origin)]
}

/// The headers of a browser request whose only fetch metadata is
/// `Sec-Fetch-Site: <site>`: the page's `Origin` is what a browser sends for a
/// cross-site fetch, `null`, so this is the shape that header alone has to
/// carry the rule (a sandboxed page, a schemeless page).
fn site_only(site: &str) -> Vec<(&'static str, &str)> {
    vec![("host", HOST), ("sec-fetch-site", site)]
}

/// A JSON body of exactly `n` bytes: `head` opens the object and starts a
/// string value that is filled to land on `n`, so the size, not the content,
/// is what the route sees.
fn body_of_len(n: usize, head: &str) -> String {
    assert!(n > head.len() + 2, "the body must hold its head");
    format!("{}{}\"}}", head, "x".repeat(n - head.len() - 2))
}

/// The boot body of exactly `n` bytes: the flag, plus filler.
fn boot_body_of_len(n: usize) -> String {
    body_of_len(n, r#"{"enabled":true,"pad":""#)
}

/// The sign-in body of exactly `n` bytes: a user name, plus filler. The
/// filler is a key the route ignores, so the body is well-formed JSON with no
/// password: 400 `session-malformed` once its size is accepted.
fn sign_in_body_of_len(n: usize) -> String {
    body_of_len(n, r#"{"username":"alice","pad":""#)
}

/// The sign-in body of exactly `n` bytes with the credentials in it, plus a
/// filler key the route ignores: a body the route reads, checks and answers,
/// at the size the cap is about.
fn sign_in_body_with_password_of_len(n: usize) -> String {
    body_of_len(
        n,
        r#"{"username":"alice","password":"correct horse","pad":""#,
    )
}

/// A header whose value is raw bytes: the shape of a header value a browser
/// cannot send and any client built on a socket can. `HeaderValue::from_bytes`
/// accepts every byte from `0x20` to `0xFF` except `0x7F`, so what is refused
/// here is on the wire, not an artefact of the transport, and `to_str` is what
/// the bff cannot do with it.
fn raw(name: &'static str, value: &[u8]) -> Vec<(&'static str, HeaderValue)> {
    vec![(
        name,
        HeaderValue::from_bytes(value).unwrap_or_else(|e| panic!("a header value ({e})")),
    )]
}

/// The headers of a request whose only fetch metadata is the `Host` line
/// naming `host`: the shape of `curl`, the CLI, a proxy or a script naming the
/// host it dialed.
fn host_line(host: &str) -> Vec<(&str, HeaderValue)> {
    vec![(
        "host",
        HeaderValue::from_str(host).unwrap_or_else(|e| panic!("a text host ({e})")),
    )]
}

/// The headers of a request addressed at `host` whose `Origin` names the same
/// host: the shape of a DNS rebinding, where the page's origin and the name
/// the request went out under are both the attacker's. The origin rule passes
/// such a pair, only the name rule refuses it.
fn at_host(host: &str) -> Vec<(&str, HeaderValue)> {
    let origin = format!("http://{host}");
    vec![
        (
            "host",
            HeaderValue::from_str(host).unwrap_or_else(|e| panic!("a text host ({e})")),
        ),
        (
            "origin",
            HeaderValue::from_str(&origin).unwrap_or_else(|e| panic!("a text origin ({e})")),
        ),
    ]
}

// ── Failure paths first: the refusals ───────────────────────────────────

/// A control verb or the boot write sent by a page of another web app on
/// `http://localhost:<other port>`: 403 `request-cross-origin`. The manager is
/// never connected to, and the session survives, since the guard runs before
/// the session is read: the probe after it is signed in.
#[tokio::test]
async fn a_cross_origin_action_is_refused_and_asks_nothing() {
    for uri in VERBS {
        let app = new_app();
        let cookie = app.sign_in().await;
        let extra = from(OTHER_PORT);
        app.post_with(uri, &cookie, &extra)
            .await
            .assert_cross_origin();
        app.assert_asks_nothing(uri);
        app.get("/api/session", Some(&cookie))
            .await
            .assert_signed_in_as("alice");
    }
    let app = new_app();
    let cookie = app.sign_in().await;
    let extra = from(OTHER_PORT);
    app.put_with("/api/instances/default/boot", &cookie, &extra)
        .await
        .assert_cross_origin();
    app.assert_asks_nothing("the boot write");
    app.get("/api/session", Some(&cookie))
        .await
        .assert_signed_in_as("alice");
}

/// A sign-out sent by a page of another web app on another port: 403, and the
/// session it tried to end is still signed in, so the user is not knocked out
/// by a request they never made.
#[tokio::test]
async fn a_cross_origin_sign_out_leaves_the_session_alive() {
    let app = new_app();
    let cookie = app.sign_in().await;
    // The test's own sign-in asked the authenticator: the baseline is what
    // comes after it must not grow.
    let before = app.asked();
    let extra = from(OTHER_PORT);
    app.delete_session_with(&cookie, &extra)
        .await
        .assert_cross_origin();
    app.assert_asks_nothing("sign-out");
    assert_eq!(app.asked(), before, "the authenticator was asked");
    app.get("/api/session", Some(&cookie))
        .await
        .assert_signed_in_as("alice");
}

/// A sign-in sent by a page of another web app on another port: 403, no cookie
/// set, the authenticator never asked, and the attempt counts as no failure:
/// after [`MAX_FAILURES`] of them the pair's own limit is untouched, so the
/// next real sign-in is answered, not refused 429. A refused request that fed
/// the lockout would be a way for any local page to lock a user out.
#[tokio::test]
async fn a_cross_origin_sign_in_creates_no_session_and_counts_no_failure() {
    let app = new_app();
    let extra = from(OTHER_PORT);
    for i in 0..MAX_FAILURES {
        let reply = app.sign_in_with("alice", &extra).await;
        reply.assert_cross_origin();
        assert_eq!(
            app.asked(),
            Vec::<String>::new(),
            "asked after {i} attempts"
        );
    }
    app.assert_asks_nothing("sign-in");
    app.get("/api/session", None)
        .await
        .assert_error(StatusCode::UNAUTHORIZED, "session-required");

    // The pair is not locked: the real sign-in lands, and its own cookie is
    // the only session that exists.
    let cookie = app.sign_in().await;
    app.get("/api/session", Some(&cookie))
        .await
        .assert_signed_in_as("alice");
}

/// `Sec-Fetch-Site` decides on its own: a request whose only fetch metadata is
/// one that is neither `same-origin` nor `none` is refused, whatever its
/// target. `same-site` is the value a page on another port of `localhost`
/// sends, the exact case the cookie's `SameSite` cannot see.
#[tokio::test]
async fn a_sec_fetch_site_that_is_not_same_origin_is_refused() {
    for site in ["same-site", "cross-site", "null", "sourcelite"] {
        let app = new_app();
        let cookie = app.sign_in().await;
        // The test's own sign-in asked the authenticator: the baseline is what
        // comes after it must not grow.
        let before = app.asked();
        let extra = site_only(site);
        for uri in VERBS {
            app.post_with(uri, &cookie, &extra)
                .await
                .assert_cross_origin();
        }
        app.put_with("/api/instances/default/boot", &cookie, &extra)
            .await
            .assert_cross_origin();
        app.delete_session_with(&cookie, &extra)
            .await
            .assert_cross_origin();
        app.sign_in_with("alice", &extra)
            .await
            .assert_cross_origin();
        app.assert_asks_nothing(&format!("Sec-Fetch-Site: {site}"));
        assert_eq!(app.asked(), before, "the authenticator was asked");
        app.get("/api/session", Some(&cookie))
            .await
            .assert_signed_in_as("alice");
    }
}

/// An `Origin` whose host and port are not the request's `Host` is refused,
/// whatever shape the difference comes in: another port, another host name of
/// the same machine, another site, no site at all (`null`), and a value that
/// only reads as ours until its end. The last two are the fail-closed half: a
/// value the bff cannot read as its own host and port is not ours.
#[tokio::test]
async fn an_origin_that_is_not_the_host_is_refused() {
    for origin in [
        OTHER_PORT,
        "http://127.0.0.1:3000",
        "http://localhost:3000.evil.example",
        "http://evil.example",
        "http://localhost:3000evil",
        "http://localhost:3000/",
        "null",
        "not a uri",
    ] {
        let app = new_app();
        let cookie = app.sign_in().await;
        let extra = from(origin);
        for uri in VERBS {
            app.post_with(uri, &cookie, &extra)
                .await
                .assert_cross_origin();
        }
        app.put_with("/api/instances/default/boot", &cookie, &extra)
            .await
            .assert_cross_origin();
        app.delete_session_with(&cookie, &extra)
            .await
            .assert_cross_origin();
        app.sign_in_with("alice", &extra)
            .await
            .assert_cross_origin();
        app.assert_asks_nothing(&format!("Origin: {origin}"));
        app.get("/api/session", Some(&cookie))
            .await
            .assert_signed_in_as("alice");
    }
}

/// The `Host` line of a request to the default port names no port. A page on
/// another port is then a different site by the port alone, so an `Origin`
/// that names one is refused: the host name matching is not enough.
#[tokio::test]
async fn a_host_that_names_no_port_compares_the_origin_port() {
    let app = new_app();
    let cookie = app.sign_in().await;
    let extra = [("host", "localhost"), ("origin", SAME_ORIGIN)];
    app.post_with("/api/instances/default/start", &cookie, &extra)
        .await
        .assert_cross_origin();
    app.assert_asks_nothing("the start verb on the default port");
}

// ── The requests that pass ──────────────────────────────────────────────

/// A request that carries neither `Origin` nor `Sec-Fetch-Site` is answered as
/// today: the shape of `curl`, a CLI, and every client that sends no fetch
/// metadata.
#[tokio::test]
async fn a_request_with_neither_header_passes_as_today() {
    let app = new_app();
    let cookie = app.sign_in().await;
    for verb in ["/start", "/stop", "/restart"] {
        app.post(&format!("/api/instances/default{verb}"), Some(&cookie))
            .await
            .assert_accepted();
    }
    app.put(
        "/api/instances/default/boot",
        &json!({ "enabled": true }).to_string(),
        Some(&cookie),
    )
    .await
    .assert_enabled(true);
    assert_eq!(
        app.calls(),
        vec![
            "start default",
            "stop default",
            "restart default",
            "set_boot_enabled default true",
            "boot_enabled default",
        ]
    );
    app.delete_session_with(&cookie, &[])
        .await
        .assert_accepted();
    app.get("/api/session", Some(&cookie))
        .await
        .assert_error(StatusCode::UNAUTHORIZED, "session-required");
}

/// The request of the signed-in user's own page passes: an `Origin` naming the
/// `Host`, with `Sec-Fetch-Site: same-origin` beside it, and with
/// `Sec-Fetch-Site: none` (a top-level navigation, the browser's first request
/// of a session), and the `Origin` alone.
#[tokio::test]
async fn a_same_origin_request_passes() {
    for extra in [
        from(SAME_ORIGIN),
        vec![
            ("host", HOST),
            ("origin", SAME_ORIGIN),
            ("sec-fetch-site", "same-origin"),
        ],
        vec![
            ("host", HOST),
            ("origin", SAME_ORIGIN),
            ("sec-fetch-site", "none"),
        ],
        site_only("same-origin"),
        site_only("none"),
    ] {
        let app = new_app();
        let cookie = app.sign_in().await;
        for uri in VERBS {
            app.post_with(uri, &cookie, &extra).await.assert_accepted();
        }
        app.put_with("/api/instances/default/boot", &cookie, &extra)
            .await
            .assert_enabled(true);
        app.sign_in_with("root", &extra).await.assert_ok();
        app.delete_session_with(&cookie, &extra)
            .await
            .assert_accepted();
    }
}

/// The `Origin` of a page served by the bff, whose `Host` names no port: the
/// default port is implied on both sides, so it is our own origin.
#[tokio::test]
async fn an_origin_at_the_default_port_passes() {
    let app = new_app();
    let cookie = app.sign_in().await;
    let extra = [("host", "localhost"), ("origin", "http://localhost")];
    app.post_with("/api/instances/default/start", &cookie, &extra)
        .await
        .assert_accepted();
    app.put_with("/api/instances/default/boot", &cookie, &extra)
        .await
        .assert_enabled(true);
}

/// A `GET` is never refused for these headers: the page reads the session, the
/// instances, the boot flag and the server file, and the reads that answer
/// without a session are the ones it polls. The manager is reached, so the
/// request was answered, not refused.
#[tokio::test]
async fn a_get_route_is_never_refused_for_these_headers() {
    for extra in [
        from(OTHER_PORT),
        site_only("cross-site"),
        vec![
            ("host", HOST),
            ("origin", "http://evil.example"),
            ("sec-fetch-site", "cross-site"),
        ],
    ] {
        let app = new_app();
        let cookie = app.sign_in().await;
        app.ask(Method::GET, "/api/version", None, None, None, &extra)
            .await
            .assert_ok();
        let r = app
            .ask(Method::GET, "/api/instances", None, None, None, &extra)
            .await;
        assert_eq!(
            r.json(),
            json!([{"id": "default", "engine": ENGINE, "state": "active"}]),
            "{}",
            r.body
        );
        app.ask(
            Method::GET,
            "/api/instances/default/boot",
            None,
            None,
            None,
            &extra,
        )
        .await
        .assert_enabled(false);
        app.ask(
            Method::GET,
            "/api/instances/default/server",
            Some(&cookie),
            None,
            None,
            &extra,
        )
        .await
        .assert_error(StatusCode::NOT_FOUND, "client-no-instance");
        app.ask(
            Method::GET,
            "/api/session",
            Some(&cookie),
            None,
            None,
            &extra,
        )
        .await
        .assert_signed_in_as("alice");
        assert!(
            app.connects.load(Ordering::SeqCst) > 0,
            "the reads reached the manager"
        );
        assert_eq!(
            app.calls(),
            vec![
                "list",
                "status default",
                "boot_enabled default",
                "read server@default.ini",
            ]
        );
    }
}

// ── The session is not touched ──────────────────────────────────────────

/// A refused request leaves the session's idle deadline where the sign-in put
/// it: the guard answers before the session is read, so a hostile page cannot
/// keep a session alive. The same steps as the polls in `tests/control.rs`:
/// the deadline is the sign-in's to the second.
#[tokio::test]
async fn a_refused_request_does_not_move_the_idle_deadline() {
    let alive = new_app();
    let cookie = alive.sign_in().await;
    alive.clock.advance(SHORT);
    let extra = from(OTHER_PORT);
    alive
        .post_with("/api/instances/default/start", &cookie, &extra)
        .await
        .assert_cross_origin();
    alive.clock.advance(IDLE - SHORT);
    alive
        .get("/api/session", Some(&cookie))
        .await
        .assert_signed_in_as("alice");

    // One second past the sign-in's deadline: the refused request in the
    // middle moved nothing.
    let gone = new_app();
    let cookie = gone.sign_in().await;
    gone.clock.advance(SHORT);
    let extra = from(OTHER_PORT);
    gone.post_with("/api/instances/default/start", &cookie, &extra)
        .await
        .assert_cross_origin();
    gone.clock.advance(IDLE - SHORT + Duration::from_secs(1));
    gone.get("/api/session", Some(&cookie))
        .await
        .assert_error(StatusCode::UNAUTHORIZED, "session-required");
}

// ── The payload cap ─────────────────────────────────────────────────────

/// A boot body and a sign-in body larger than [`MAX_BODY`] are refused 413
/// `payload-too-large` before the manager is connected to and before the
/// authenticator is asked. The sign-in body is the one an anonymous caller may
/// send, and neither route has any use for a big one.
#[tokio::test]
async fn a_body_over_the_cap_is_refused_before_anything() {
    let app = new_app();
    let cookie = app.sign_in().await;
    let before = app.calls();
    let big = boot_body_of_len(MAX_BODY + 1);
    assert_eq!(big.len(), MAX_BODY + 1, "the body is one over the cap");
    app.put("/api/instances/default/boot", &big, Some(&cookie))
        .await
        .assert_error(StatusCode::PAYLOAD_TOO_LARGE, TOO_LARGE);
    assert_eq!(
        app.calls(),
        before,
        "the refusal asked the manager for nothing"
    );

    let anon = new_app();
    let big = sign_in_body_of_len(MAX_BODY + 1);
    assert_eq!(big.len(), MAX_BODY + 1, "the body is one over the cap");
    anon.sign_in_body(&big, &[])
        .await
        .assert_error(StatusCode::PAYLOAD_TOO_LARGE, TOO_LARGE);
    anon.assert_asks_nothing("the sign-in body");
    assert!(anon.asked().is_empty(), "the authenticator was asked");
}

/// The cap is above [`MAX_BODY`], not at it: a body of exactly that size is
/// read and answered as today. The sign-in's filler is a key the route
/// ignores, so its own answer is 400 `session-malformed`, which is what a body
/// with no password is: the size asked for nothing.
#[tokio::test]
async fn a_body_of_exactly_the_cap_is_read() {
    let app = new_app();
    let cookie = app.sign_in().await;
    let exact = boot_body_of_len(MAX_BODY);
    assert_eq!(exact.len(), MAX_BODY, "the body is exactly the cap");
    app.put("/api/instances/default/boot", &exact, Some(&cookie))
        .await
        .assert_enabled(true);

    let anon = new_app();
    let exact = sign_in_body_of_len(MAX_BODY);
    assert_eq!(exact.len(), MAX_BODY, "the body is exactly the cap");
    anon.sign_in_body(&exact, &[])
        .await
        .assert_error(StatusCode::BAD_REQUEST, "session-malformed");
    assert!(
        anon.asked().is_empty(),
        "a body with no password is not checked"
    );
}

/// A hostile request that also carries an over-large body is refused before
/// anything is asked, whichever of the two rules answers first: the guard and
/// the cap are both checks of the request, and a request that fails either
/// must reach neither the manager nor the session.
#[tokio::test]
async fn a_hostile_request_with_a_huge_body_is_refused_before_anything() {
    let app = new_app();
    let cookie = app.sign_in().await;
    let big = boot_body_of_len(MAX_BODY + 1);
    let extra = from(OTHER_PORT);
    let reply = app
        .ask(
            Method::PUT,
            "/api/instances/default/boot",
            Some(&cookie),
            Some(big),
            Some("application/json"),
            &extra,
        )
        .await;
    assert!(
        reply.status == StatusCode::FORBIDDEN || reply.status == StatusCode::PAYLOAD_TOO_LARGE,
        "one of the two refusals: {} {}",
        reply.status,
        reply.body
    );
    assert_eq!(
        reply.json(),
        json!({ "key": if reply.status == StatusCode::FORBIDDEN { CROSS_ORIGIN } else { TOO_LARGE }, "args": [] }),
        "{}",
        reply.body
    );
    app.assert_asks_nothing("a hostile over-large boot write");
    app.get("/api/session", Some(&cookie))
        .await
        .assert_signed_in_as("alice");
}

// ── A header the bff cannot read ────────────────────────────────────────

/// A write whose `Origin` or `Sec-Fetch-Site` is present but is not text is
/// refused, not treated as an absent header: the bytes are on the wire, so a
/// client did send the header, and the bff cannot read it as this bff's own
/// origin. A value that reads as `same-origin` up to a byte the bff cannot
/// read is the fail-closed half. No `Host` is sent, so the name rule cannot
/// answer these: the key names the rule that did.
#[tokio::test]
async fn a_header_that_is_not_text_is_refused() {
    for value in [&b"caf\xc3\xa9"[..], b"\xff\xfe\xfd", b"same-origin\xff"] {
        for name in ["origin", "sec-fetch-site"] {
            let app = new_app();
            let cookie = app.sign_in().await;
            // The test's own sign-in asked the authenticator: the baseline is
            // what comes after it must not grow.
            let before = app.asked();
            let extra = raw(name, value);
            for uri in VERBS {
                app.ask_raw(Method::POST, uri, Some(&cookie), None, None, &extra)
                    .await
                    .assert_cross_origin();
            }
            app.boot_raw(&cookie, &extra).await.assert_cross_origin();
            app.ask_raw(
                Method::DELETE,
                "/api/session",
                Some(&cookie),
                None,
                None,
                &extra,
            )
            .await
            .assert_cross_origin();
            app.sign_in_raw(CREDENTIALS, &extra)
                .await
                .assert_cross_origin();
            app.assert_asks_nothing(&format!("{name}: {value:?}"));
            assert_eq!(app.asked(), before, "the authenticator was asked");
            app.get("/api/session", Some(&cookie))
                .await
                .assert_signed_in_as("alice");
        }
    }
}

/// A `Host` line that is not text names no loopback host, so the name rule
/// refuses it: the origin headers are absent, so no other rule can answer and
/// the key names the rule that did. A request addressed at bytes is not a
/// request to this bff, whatever its `Origin` says.
#[tokio::test]
async fn a_host_that_is_not_text_is_refused() {
    let app = new_app();
    let cookie = app.sign_in().await;
    let named = raw("host", b"attacker.example\xc3\xa9");
    for uri in VERBS {
        app.ask_raw(Method::POST, uri, Some(&cookie), None, None, &named)
            .await
            .assert_bad_host();
    }
    app.boot_raw(&cookie, &named).await.assert_bad_host();
    app.ask_raw(Method::GET, "/api/instances", None, None, None, &named)
        .await
        .assert_bad_host();
    app.ask_raw(Method::GET, "/api/version", None, None, None, &named)
        .await
        .assert_bad_host();
    app.assert_asks_nothing("a Host that is not text");
    app.get("/api/session", Some(&cookie))
        .await
        .assert_signed_in_as("alice");
}

// ── The same header twice ───────────────────────────────────────────────

/// Two `Origin` headers, or two `Sec-Fetch-Site` headers, are refused: the bff
/// cannot tell which one the browser meant, so it takes neither. A pair whose
/// first value is this bff's own is the half a first-value-only reading lets
/// through, and a pair whose second value is a foreign one is the half where a
/// first-value-only reading answers the attacker's request.
#[tokio::test]
async fn two_of_the_same_header_are_refused() {
    for extra in [
        vec![
            ("host", HOST),
            ("origin", SAME_ORIGIN),
            ("origin", SAME_ORIGIN),
        ],
        vec![
            ("host", HOST),
            ("origin", SAME_ORIGIN),
            ("origin", OTHER_PORT),
        ],
        vec![
            ("host", HOST),
            ("origin", OTHER_PORT),
            ("origin", SAME_ORIGIN),
        ],
        vec![
            ("host", HOST),
            ("sec-fetch-site", "same-origin"),
            ("sec-fetch-site", "same-origin"),
        ],
        vec![
            ("host", HOST),
            ("sec-fetch-site", "same-origin"),
            ("sec-fetch-site", "cross-site"),
        ],
    ] {
        let app = new_app();
        let cookie = app.sign_in().await;
        let before = app.asked();
        for uri in VERBS {
            app.post_with(uri, &cookie, &extra)
                .await
                .assert_cross_origin();
        }
        app.put_with("/api/instances/default/boot", &cookie, &extra)
            .await
            .assert_cross_origin();
        app.delete_session_with(&cookie, &extra)
            .await
            .assert_cross_origin();
        app.sign_in_with("alice", &extra)
            .await
            .assert_cross_origin();
        app.assert_asks_nothing(&format!("{extra:?}"));
        assert_eq!(app.asked(), before, "the authenticator was asked");
        app.get("/api/session", Some(&cookie))
            .await
            .assert_signed_in_as("alice");
    }
}

// ── The name the request is addressed at ────────────────────────────────

/// A `Host` that is not a loopback name is refused for every request, the
/// reads, the sign-in and the session probe included: the bff listens on
/// loopback only (`listen.rs`), so a page whose origin is `attacker.example`
/// can only reach this bff under a name that resolves to this machine (a DNS
/// rebinding), and that page is not this bff's page. The `Origin` matching the
/// `Host` is the shape of the attack: the origin rule passes every one of
/// these, only the name rule refuses them. The refusal is 403
/// `request-bad-host`, before the session is read, before the manager is
/// connected to, and before the password is checked, so a foreign name gives
/// no answer about a credential and ends no session.
#[tokio::test]
async fn a_host_that_is_not_a_loopback_name_is_refused() {
    for host in [
        "attacker.example",
        "ATTACKER.EXAMPLE",
        "attacker.example:3000",
        "localhost.evil.example",
        "localhost.example:3000",
        "127.0.0.1.evil.example",
        "local.example",
        "10.0.0.5:3000",
        "192.168.1.7",
        "0.0.0.0:8590",
    ] {
        let app = new_app();
        // The test's own sign-in sends no `Host`, the shape of `curl`: the
        // cookie it sets is the one these requests carry.
        let cookie = app.sign_in().await;
        let before = app.asked();

        // The rebinding: the page's `Origin` and the `Host` line the request
        // went out under are both the attacker's name.
        let rebinding = at_host(host);
        for uri in VERBS {
            app.ask_raw(Method::POST, uri, Some(&cookie), None, None, &rebinding)
                .await
                .assert_bad_host();
        }
        app.boot_raw(&cookie, &rebinding).await.assert_bad_host();
        app.ask_raw(
            Method::DELETE,
            "/api/session",
            Some(&cookie),
            None,
            None,
            &rebinding,
        )
        .await
        .assert_bad_host();
        app.sign_in_raw(CREDENTIALS, &rebinding)
            .await
            .assert_bad_host();
        // The reads answer nothing to a foreign name, the session-free ones
        // included.
        app.ask_raw(Method::GET, "/api/instances", None, None, None, &rebinding)
            .await
            .assert_bad_host();

        // The `Host` on its own, with no fetch metadata: `curl` naming a host
        // this bff does not serve. A name the bff does not serve is a refusal
        // whether or not a browser claims an origin.
        let named = host_line(host);
        for uri in VERBS {
            app.ask_raw(Method::POST, uri, Some(&cookie), None, None, &named)
                .await
                .assert_bad_host();
        }
        app.ask_raw(Method::GET, "/api/instances", None, None, None, &named)
            .await
            .assert_bad_host();
        app.ask_raw(Method::GET, "/api/version", None, None, None, &named)
            .await
            .assert_bad_host();
        app.ask_raw(
            Method::GET,
            "/api/session",
            Some(&cookie),
            None,
            None,
            &named,
        )
        .await
        .assert_bad_host();

        app.assert_asks_nothing(&format!("Host: {host}"));
        assert_eq!(app.asked(), before, "the authenticator was asked");
        // The session is where the sign-in left it: the guard answered before
        // the session was read.
        app.get("/api/session", Some(&cookie))
            .await
            .assert_signed_in_as("alice");
    }
}

/// A loopback `Host` is answered as today, in every spelling a browser, the
/// CLI or a proxy uses: `localhost`, an address in `127.0.0.0/8`, the IPv6
/// loopback in brackets, each with or without a port, in any case. These are
/// the names this bff is addressed as, so the signed-in user's page reads and
/// writes through them.
#[tokio::test]
async fn a_loopback_host_is_answered_as_today() {
    for host in [
        "localhost",
        "localhost:3000",
        "Localhost:3000",
        "LOCALHOST",
        "127.0.0.1",
        "127.0.0.1:3000",
        "127.0.0.53",
        "127.5.6.7:3000",
        "[::1]",
        "[::1]:3000",
    ] {
        let named = host_line(host);
        let app = new_app();
        let cookie = app.sign_in().await;
        for uri in VERBS {
            app.ask_raw(Method::POST, uri, Some(&cookie), None, None, &named)
                .await
                .assert_accepted();
        }
        app.boot_raw(&cookie, &named).await.assert_enabled(true);
        app.sign_in_raw(CREDENTIALS, &named).await.assert_ok();
        let r = app
            .ask_raw(Method::GET, "/api/instances", None, None, None, &named)
            .await;
        assert_eq!(
            r.json(),
            json!([{"id": "default", "engine": ENGINE, "state": "active"}]),
            "Host: {host}: {}",
            r.body
        );
        app.get("/api/session", Some(&cookie))
            .await
            .assert_signed_in_as("alice");
        app.ask_raw(
            Method::DELETE,
            "/api/session",
            Some(&cookie),
            None,
            None,
            &named,
        )
        .await
        .assert_accepted();
    }
}

/// A request with no `Host` line is answered as today: HTTP/1.0 (`curl -0.9`)
/// sends none, and the CLI, the tests and a raw client are that shape. There
/// is no name to check, the bff answers on loopback only, and a client that
/// sends no fetch metadata is not a browser's cross-site request.
#[tokio::test]
async fn a_request_with_no_host_is_answered_as_today() {
    let app = new_app();
    let cookie = app.sign_in().await;
    for uri in VERBS {
        app.post_with(uri, &cookie, &[]).await.assert_accepted();
    }
    app.put_with("/api/instances/default/boot", &cookie, &[])
        .await
        .assert_enabled(true);
    let r = app.get("/api/instances", None).await;
    assert_eq!(
        r.json(),
        json!([{"id": "default", "engine": ENGINE, "state": "active"}]),
        "{}",
        r.body
    );
    app.delete_session_with(&cookie, &[])
        .await
        .assert_accepted();
    app.get("/api/session", Some(&cookie))
        .await
        .assert_error(StatusCode::UNAUTHORIZED, "session-required");
}

// ── A verb that is not a read ───────────────────────────────────────────

/// A method that is not a read (`GET`, `HEAD`, `OPTIONS`) is guarded: a
/// `PATCH`, a verb the bff does not route, from a foreign page is refused 403
/// `request-cross-origin`, and is not answered with the route's 405. From this
/// bff's own page the same verb is still the route's to answer, and the reads
/// are not refused for these headers, so the guard decides where a request
/// came from and not which verbs exist.
#[tokio::test]
async fn a_method_that_is_not_a_read_is_guarded() {
    for method in [
        Method::PATCH,
        Method::from_bytes(b"EXTRA").expect("a method token"),
    ] {
        let app = new_app();
        let cookie = app.sign_in().await;
        let extra = from(OTHER_PORT);
        for uri in [
            "/api/instances/default/start",
            "/api/instances/default/boot",
            "/api/session",
        ] {
            app.ask(method.clone(), uri, Some(&cookie), None, None, &extra)
                .await
                .assert_cross_origin();
        }
        app.assert_asks_nothing(&format!("{method} from a foreign page"));
        app.get("/api/session", Some(&cookie))
            .await
            .assert_signed_in_as("alice");
    }

    // The same verbs from this bff's own page stay the route's to answer.
    let app = new_app();
    let cookie = app.sign_in().await;
    for method in [
        Method::PATCH,
        Method::from_bytes(b"EXTRA").expect("a method token"),
    ] {
        let reply = app
            .ask(
                method.clone(),
                "/api/instances/default/start",
                Some(&cookie),
                None,
                None,
                &from(SAME_ORIGIN),
            )
            .await;
        assert_eq!(
            reply.status,
            StatusCode::METHOD_NOT_ALLOWED,
            "the route answers its own page's {method}: {} {}",
            reply.status,
            reply.body
        );
    }

    // A `GET`, a `HEAD` and an `OPTIONS` are reads: a foreign `Origin` does
    // not refuse them, the rule is about where a request came from.
    for method in [Method::HEAD, Method::OPTIONS] {
        let reply = app
            .ask(
                method.clone(),
                "/api/instances",
                None,
                None,
                None,
                &from(OTHER_PORT),
            )
            .await;
        assert_ne!(
            reply.status,
            StatusCode::FORBIDDEN,
            "{method} is a read: {} {}",
            reply.status,
            reply.body
        );
    }
}

// ── The cap, at the boundary, on the sign-in ────────────────────────────

/// The sign-in's cap is above [`MAX_BODY`] and not at it, as the boot write's
/// is: a sign-in body of exactly that size is read, checked and answered with
/// the session it asks for (the filler is a key the route ignores), and one
/// byte more is the extractor's 413, before the authenticator is asked.
#[tokio::test]
async fn a_sign_in_at_exactly_the_cap_signs_in() {
    let app = new_app();
    let exact = sign_in_body_with_password_of_len(MAX_BODY);
    assert_eq!(exact.len(), MAX_BODY, "the body is exactly the cap");
    let reply = app.sign_in_body(&exact, &[]).await;
    reply.assert_ok();
    let cookie = reply.set_cookie.expect("a session cookie");
    app.get("/api/session", Some(&cookie))
        .await
        .assert_signed_in_as("alice");

    let over = new_app();
    let big = sign_in_body_with_password_of_len(MAX_BODY + 1);
    assert_eq!(big.len(), MAX_BODY + 1, "the body is one over the cap");
    over.sign_in_body(&big, &[])
        .await
        .assert_error(StatusCode::PAYLOAD_TOO_LARGE, TOO_LARGE);
    over.assert_asks_nothing("a sign-in one over the cap");
    assert!(over.asked().is_empty(), "the authenticator was asked");
}

// ── The IPv6 loopback, and an origin that is not an origin ──────────────

/// The IPv6 loopback is bracketed in the `Host`, so host and port are compared
/// whole: `http://[::1]:3000` is this bff's own origin against the same `Host`
/// and against `[::1]` (the default port on both sides), and is refused
/// against `[::1]:9999`, the port being the one fact `SameSite` never
/// compares. An `Origin` with a user, a path, a query, a fragment, no scheme
/// or no host names no origin and is refused.
#[tokio::test]
async fn an_ipv6_origin_and_host_are_compared_whole() {
    for (host, origin) in [
        ("[::1]:3000", "http://[::1]:3000"),
        ("[::1]", "http://[::1]"),
    ] {
        let extra = vec![("host", host), ("origin", origin)];
        let app = new_app();
        let cookie = app.sign_in().await;
        for uri in VERBS {
            app.post_with(uri, &cookie, &extra).await.assert_accepted();
        }
        app.put_with("/api/instances/default/boot", &cookie, &extra)
            .await
            .assert_enabled(true);
        app.sign_in_with("root", &extra).await.assert_ok();
        app.delete_session_with(&cookie, &extra)
            .await
            .assert_accepted();
    }

    for (host, origin) in [
        ("[::1]:3000", "http://[::1]:9999"),
        ("[::1]:9999", "http://[::1]:3000"),
        ("[::1]", "http://[::1]:3000"),
        ("[::1]:3000", "http://[::1]:3000/"),
        ("[::1]:3000", "http://[::1]:3000/dashboard"),
    ] {
        let extra = vec![("host", host), ("origin", origin)];
        let app = new_app();
        let cookie = app.sign_in().await;
        for uri in VERBS {
            app.post_with(uri, &cookie, &extra)
                .await
                .assert_cross_origin();
        }
        app.put_with("/api/instances/default/boot", &cookie, &extra)
            .await
            .assert_cross_origin();
        app.delete_session_with(&cookie, &extra)
            .await
            .assert_cross_origin();
        app.sign_in_with("alice", &extra)
            .await
            .assert_cross_origin();
        app.assert_asks_nothing(&format!("Host: {host}, Origin: {origin}"));
        app.get("/api/session", Some(&cookie))
            .await
            .assert_signed_in_as("alice");
    }

    for origin in [
        "http://alice@localhost:3000",
        "http://alice:secret@localhost:3000",
        "http://user@localhost:3000/dashboard",
        "http://localhost:3000/dashboard",
        "http://localhost:3000?x=1",
        "http://localhost:3000#top",
        "//localhost:3000",
        "http://",
        "http://:3000",
    ] {
        let app = new_app();
        let cookie = app.sign_in().await;
        let extra = from(origin);
        app.post_with("/api/instances/default/start", &cookie, &extra)
            .await
            .assert_cross_origin();
        app.put_with("/api/instances/default/boot", &cookie, &extra)
            .await
            .assert_cross_origin();
        app.delete_session_with(&cookie, &extra)
            .await
            .assert_cross_origin();
        app.sign_in_with("alice", &extra)
            .await
            .assert_cross_origin();
        app.assert_asks_nothing(&format!("Origin: {origin}"));
        app.get("/api/session", Some(&cookie))
            .await
            .assert_signed_in_as("alice");
    }
}
