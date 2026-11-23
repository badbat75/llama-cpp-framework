//! The server-profile pool's routes, through `pai_bff::router_with_session`
//! with a fake manager client and a fake authenticator, never the real bus,
//! pipe, PAM or LogonUser: `GET` and `POST /api/profiles/server`, and
//! `DELETE`, `GET`, `PUT /api/profiles/server/{id}`. Each request runs under a
//! deadline, so a bff that waits on the manager fails instead of hanging.
//!
//! The contract (fe/briefs/api-contract.md "Server profile pool", and
//! ARCHITECTURE.md's slice 14):
//! - Every route sits behind a session: with none, or with the cookie of one
//!   that ended, the answer is 401 `{"key": "session-required", "args": []}`
//!   and the manager is never even connected to.
//! - `GET /api/profiles/server` is `[{"id", "engine", "port", "tags",
//!   "instance"}]` in the manager's order, `instance` the id using the profile
//!   or `null`. The bff's own account asks (no principal), as the instances
//!   list does.
//! - `POST /api/profiles/server` `{"engine", "id", "port"?}` answers 201
//!   `{"id"}`. The bff checks the body first and the manager checks again: 422
//!   `{"key","args","field"}` with `field` `engine`, `id` or `port`, 400
//!   `server-profile-malformed` for a body it cannot read (the manager not
//!   asked), 409 `client-server-profile-exists` `[id]`.
//! - `DELETE /api/profiles/server/{id}` answers 204, 404
//!   `client-no-server-profile` `[id]`, and 409
//!   `client-server-profile-in-use` `[profile, instance]`.
//! - `GET`/`PUT /api/profiles/server/{id}` are today's server document and
//!   save keyed by the profile's own file (`server@<id>.ini`), the document
//!   gaining `instance`, read from `Instances()` (whose failure is its own
//!   answer, never `null`, which would offer Delete), and `defaults.StateDir`
//!   the using instance's folder, `null` when unused; the save answers
//!   `{"revision", "running"}` with the state of the instance using it,
//!   `false` when unused, and `true` when the state is unknown: a record read
//!   (the one after the write) or a status that fails answers running, so the
//!   page never hides that a restart may be needed. A missing file or a name the manager refuses is 404
//!   `client-no-server-profile` `[id]`, a stale revision 409
//!   `server-changed-elsewhere`.
//! - A refusal of the signed-in user is 403 `session-not-administrator` and
//!   ends that session on every route; a manager that takes a write and is
//!   gone when it answers is 504 `client-timeout` (slice W2: the write crossed
//!   the hand-over, so its outcome is unknown), while a read that meets one
//!   that is gone, or a connect that hands nothing over, is 503
//!   `client-unavailable`; anything else 502 `client-other`, without the
//!   manager's detail.
//! - Every one of these reads and writes moves the session's idle deadline
//!   (`Sessions::current`), unlike the polls of `/api/instances`.
//!
//! `GET`/`PUT /api/instances/{id}/server` stay as they are (the web UI still
//! calls them): the last test pins the old document beside the new one.

use axum::body::Body;
use axum::extract::ConnectInfo;
use axum::http::{header, Method, Request, StatusCode};
use axum::Router;
use http_body_util::BodyExt;
use pai_bff::session::{AuthError, Authenticator, Identity};
use pai_bff::{Client, Connect};
use pai_core::client::{
    ClientError, InstanceInfo, InstanceState, ManagerClient, ManagerFile, ServerProfileInfo,
};
use pai_core::{checks, paths, server_cfg};
use serde_json::{json, Value};
use siphasher::sip::SipHasher;
use std::collections::HashMap;
use std::hash::Hasher;
use std::net::SocketAddr;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};
use tower::ServiceExt;

/// How long the bff waits for the manager in these tests.
const TIMEOUT: Duration = Duration::from_millis(200);

/// How long a test waits for the bff's answer before failing.
const DEADLINE: Duration = Duration::from_secs(5);

/// The client address a sign-in is sent from (the lockout counters key on it).
const CLIENT: &str = "127.0.0.1:51001";

/// The revision secret these routers are built with: the document's `revision`
/// is a keyed hash under it, so a test reads back the revision it sends.
const REVISION_KEY: [u8; 16] = [0; 16];

/// A step just under the session's idle timeout (`session.rs`'s own): a request
/// that runs that far after the sign-in is answered only by a route that moved
/// the deadline.
const SHORT: Duration = Duration::from_secs(30 * 60 - 60);

/// The key of the gate's answer (`session.rs`).
const SESSION_REQUIRED: &str = "session-required";

/// The engine every profile of these tests runs: the one engine a node has.
const ENGINE: &str = "llama.cpp";

type Log = Arc<Mutex<Vec<String>>>;

/// Where the fake leaves every `(name, text)` a write landed, shared with the
/// test (the fake is built again at each connect).
type Written = Arc<Mutex<Vec<(String, String)>>>;

/// The principal each connect was asked for.
type Principals = Arc<Mutex<Vec<Option<String>>>>;

/// The text of one profile's file, as a create writes it: the defaults with
/// that port, the directories left unset.
fn profile_text(port: i32) -> String {
    server_cfg::render_for(
        &server_cfg::ServerConfig {
            port: Some(port),
            ..Default::default()
        },
        true,
    )
}

/// One row of `server_profiles`.
fn profile(
    id: &str,
    engine: &str,
    port: u16,
    tags: &[&str],
    instance: Option<&str>,
) -> ServerProfileInfo {
    ServerProfileInfo {
        id: id.to_string(),
        engine: engine.to_string(),
        port,
        tags: tags.iter().map(|t| t.to_string()).collect(),
        instance: instance.map(str::to_string),
    }
}

/// One row of `instances`: the instance `id`, on the profile `profile`.
fn row(id: &str, profile: &str, port: u16) -> InstanceInfo {
    InstanceInfo {
        id: id.to_string(),
        engine: ENGINE.to_string(),
        server_profile: Some(profile.to_string()),
        port,
        profile_tags: Vec::new(),
        instance_tags: Vec::new(),
        label: None,
    }
}

/// What the fake manager answers. A manager that knows the pool, has one file
/// per profile of the pool, and the record of `used`.
struct Fixture {
    /// What `server_profiles` answers.
    profiles: Result<Vec<ServerProfileInfo>, ClientError>,
    /// What `create_server_profile` answers.
    create: Result<(), ClientError>,
    /// What `delete_server_profile` answers.
    delete: Result<(), ClientError>,
    /// The text of each served file, by the manager's name of it.
    files: HashMap<String, String>,
    /// What every `read_file` answers, ahead of `files`.
    read_fail: Option<ClientError>,
    /// What a write answers.
    write: Result<(), ClientError>,
    /// What `instances` answers: the record's rows.
    rows: Result<Vec<InstanceInfo>, ClientError>,
    /// The state each instance answers; `Inactive` for one it does not name.
    states: HashMap<String, InstanceState>,
    /// What every `status` answers, ahead of `states`: the state read that fails.
    status_fail: Option<ClientError>,
    /// How long `server_profiles` takes.
    delay: Duration,
    /// Where the writes that landed are left for the test to read.
    written: Option<Written>,
}

impl Default for Fixture {
    fn default() -> Fixture {
        Fixture {
            profiles: Ok(Vec::new()),
            create: Ok(()),
            delete: Ok(()),
            files: HashMap::new(),
            read_fail: None,
            write: Ok(()),
            rows: Ok(Vec::new()),
            states: HashMap::new(),
            status_fail: None,
            delay: Duration::ZERO,
            written: None,
        }
    }
}

impl Fixture {
    /// A pool with one profile per `(id, port)` entry, its file holding that
    /// port, and no instance using any of them.
    fn pool(files: &[(&str, i32)]) -> Fixture {
        let mut f = Fixture::default();
        for (id, port) in files {
            f.files
                .insert(format!("server@{id}.ini"), profile_text(*port));
            let mut rows = f.profiles.expect("the pool answers");
            rows.push(profile(id, ENGINE, *port as u16, &[], None));
            f.profiles = Ok(rows);
        }
        f
    }

    /// The record gains one section per `(instance, profile)` entry, naming
    /// that profile, and every pool profile an entry names is answered as used
    /// by it.
    fn used(mut self, entries: &[(&str, &str)]) -> Fixture {
        let mut out = Vec::new();
        for (id, profile) in entries {
            let port = self
                .profiles
                .as_ref()
                .expect("a pool")
                .iter()
                .find(|p| p.id == *profile)
                .map(|p| p.port)
                .unwrap_or(0);
            out.push(row(id, profile, port));
            if let Ok(rows) = &mut self.profiles {
                if let Some(p) = rows.iter_mut().find(|p| p.id == *profile) {
                    p.instance = Some(id.to_string());
                }
            }
        }
        self.rows = Ok(out);
        self
    }

    /// The state of `id` is `state`.
    fn state(mut self, id: &str, state: InstanceState) -> Fixture {
        self.states.insert(id.to_string(), state);
        self
    }

    /// The shared sink of the writes that landed.
    fn writing(mut self, sink: Written) -> Fixture {
        self.written = Some(sink);
        self
    }
}

/// A manager that answers from a [`Fixture`] and logs every call (`profiles`,
/// `create <engine> <id> <port>`, `delete <engine> <id>`, `read <name>`,
/// `write-if <name>`, `status <id>`, `instances`).
struct FakeManager {
    fixture: Fixture,
    log: Log,
}

impl FakeManager {
    fn new(log: &Log, fixture: &Fixture) -> FakeManager {
        FakeManager {
            fixture: Fixture {
                profiles: fixture.profiles.clone(),
                create: fixture.create.clone(),
                delete: fixture.delete.clone(),
                files: fixture.files.clone(),
                read_fail: fixture.read_fail.clone(),
                write: fixture.write.clone(),
                rows: fixture.rows.clone(),
                states: fixture.states.clone(),
                status_fail: fixture.status_fail.clone(),
                delay: fixture.delay,
                written: fixture.written.clone(),
            },
            log: log.clone(),
        }
    }

    fn record(&self, call: String) {
        self.log
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .push(call);
    }

    /// Leaves the text a write landed with, in the sink the test reads.
    fn push(&self, name: &str, contents: &str) {
        if let Some(sink) = &self.fixture.written {
            sink.lock()
                .unwrap_or_else(|e| e.into_inner())
                .push((name.to_string(), contents.to_string()));
        }
    }
}

impl ManagerClient for FakeManager {
    fn list_instances(&self) -> Result<Vec<String>, ClientError> {
        self.record("list".into());
        Ok(vec!["default".to_string()])
    }
    fn status(&self, id: &str) -> Result<InstanceState, ClientError> {
        self.record(format!("status {id}"));
        if let Some(fail) = self.fixture.status_fail.clone() {
            return Err(fail);
        }
        Ok(self
            .fixture
            .states
            .get(id)
            .cloned()
            .unwrap_or(InstanceState::Inactive))
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
        if let Some(fail) = self.fixture.read_fail.clone() {
            return Err(fail);
        }
        Ok(self.fixture.files.get(name).map(|text| ManagerFile {
            text: text.clone(),
            withheld: false,
        }))
    }
    fn write_file(
        &self,
        name: &str,
        contents: &str,
        keep_api_key: bool,
    ) -> Result<(), ClientError> {
        self.record(format!("write {name} keep={keep_api_key}"));
        self.push(name, contents);
        self.fixture.write.clone()
    }
    fn write_file_if(&self, name: &str, contents: &str, expected: &str) -> Result<(), ClientError> {
        self.record(format!("write-if {name}"));
        if self.fixture.files.get(name).map(String::as_str) != Some(expected) {
            return Err(ClientError::Changed);
        }
        self.push(name, contents);
        self.fixture.write.clone()
    }
    fn boot_enabled(&self, id: &str) -> Result<bool, ClientError> {
        self.record(format!("boot {id}"));
        Ok(false)
    }
    fn set_boot_enabled(&self, id: &str, enabled: bool) -> Result<(), ClientError> {
        self.record(format!("set-boot {id} {enabled}"));
        Ok(())
    }
    fn authenticate(&self, _user: &str, _password: &str) -> Result<(bool, bool), ClientError> {
        Ok((true, true))
    }
    fn instances(&self) -> Result<Vec<InstanceInfo>, ClientError> {
        self.record("instances".into());
        self.fixture.rows.clone()
    }
    fn set_instance_tags(&self, id: &str, tags: &[String]) -> Result<(), ClientError> {
        self.record(format!("tags {id} {}", tags.join(",")));
        Ok(())
    }
    fn create_server_profile(
        &self,
        engine: &str,
        profile: &str,
        port: Option<u16>,
    ) -> Result<(), ClientError> {
        self.record(format!(
            "create {engine} {profile} {}",
            port.map_or("-".to_string(), |p| p.to_string())
        ));
        self.fixture.create.clone()
    }
    fn delete_server_profile(&self, engine: &str, profile: &str) -> Result<(), ClientError> {
        self.record(format!("delete {engine} {profile}"));
        self.fixture.delete.clone()
    }
    fn server_profiles(&self) -> Result<Vec<ServerProfileInfo>, ClientError> {
        self.record("profiles".into());
        std::thread::sleep(self.fixture.delay);
        self.fixture.profiles.clone()
    }
}

fn principals_of(p: &Principals) -> Vec<Option<String>> {
    p.lock().unwrap_or_else(|e| e.into_inner()).clone()
}

fn written_of(w: &Written) -> Vec<(String, String)> {
    w.lock().unwrap_or_else(|e| e.into_inner()).clone()
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
    /// A route the bff does not route answers a bare 404 with an empty body,
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
        self.assert_error(StatusCode::UNAUTHORIZED, SESSION_REQUIRED, &[]);
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

    fn advance(&self, by: Duration) {
        *self.0.lock().unwrap_or_else(|e| e.into_inner()) += by;
    }
}

/// A router over a fake manager, a fake authenticator and a clock the test
/// moves, with the log of the manager's calls, the count of connects and the
/// principals each was handed.
struct App {
    router: Router,
    clock: FakeClock,
    log: Log,
    connects: Arc<AtomicUsize>,
    principals: Principals,
}

/// The app whose manager answers from `fixture`; the fake is built again at
/// every connect.
fn app_for(fixture: Fixture) -> App {
    app_connecting(move |log| Ok(Box::new(FakeManager::new(log, &fixture)) as Client))
}

/// The app whose connect hands out what `make` answers, so a connect that
/// fails, hangs, or answers differently each time is testable.
fn app_connecting(
    make: impl Fn(&Log) -> Result<Client, ClientError> + Send + Sync + 'static,
) -> App {
    let log = Log::default();
    let connects = Arc::new(AtomicUsize::new(0));
    let principals = Principals::default();
    let l = log.clone();
    let connect = connect_logging(&connects, &principals, move || make(&l));
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

    /// The create verb was never asked of the manager, as it is not for a 400
    /// or a 422 the bff answered from its own check.
    fn assert_no_create(&self, what: &str) {
        let calls = self.calls();
        assert!(
            !calls.iter().any(|c| c.starts_with("create")),
            "{what}: {calls:?}"
        );
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
        let mut builder = Request::builder().method(method).uri(uri);
        if let Some(c) = cookie {
            builder = builder.header(header::COOKIE, c);
        }
        let request = builder
            .header(header::CONTENT_TYPE, "application/json")
            .body(Body::from(body.to_string()))
            .expect("request");
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

    // ── The five requests, each signed in first ──────────────────────────

    /// Sign in and `GET /api/profiles/server`.
    async fn list(&self) -> (Reply, String) {
        let cookie = self.sign_in().await;
        (
            self.get("/api/profiles/server", Some(&cookie)).await,
            cookie,
        )
    }

    /// Sign in and `POST /api/profiles/server` with `body`.
    async fn create(&self, body: Value) -> (Reply, String) {
        let cookie = self.sign_in().await;
        let reply = self
            .ask_json(
                Method::POST,
                "/api/profiles/server",
                Some(&cookie),
                &body.to_string(),
            )
            .await;
        (reply, cookie)
    }

    /// Sign in and `DELETE /api/profiles/server/{id}`.
    async fn delete(&self, id: &str) -> (Reply, String) {
        let cookie = self.sign_in().await;
        (
            self.ask(
                Method::DELETE,
                &format!("/api/profiles/server/{id}"),
                Some(&cookie),
            )
            .await,
            cookie,
        )
    }

    /// Sign in and `GET /api/profiles/server/{id}`.
    async fn document(&self, id: &str) -> (Reply, String) {
        let cookie = self.sign_in().await;
        (
            self.get(&format!("/api/profiles/server/{id}"), Some(&cookie))
                .await,
            cookie,
        )
    }

    /// Sign in and `PUT /api/profiles/server/{id}` with `body`.
    async fn save(&self, id: &str, body: Value) -> (Reply, String) {
        let cookie = self.sign_in().await;
        let reply = self
            .ask_json(
                Method::PUT,
                &format!("/api/profiles/server/{id}"),
                Some(&cookie),
                &body.to_string(),
            )
            .await;
        (reply, cookie)
    }

    /// Reads the document of `id` and saves it back with `Port` set to `port`,
    /// as the form does: 200 answers `{"revision", "running"}`.
    async fn save_with_port(&self, id: &str, port: i64) -> (Reply, String) {
        let (doc, _) = self.document(id).await;
        assert_eq!(doc.status, StatusCode::OK, "{}", doc.body);
        let mut values = doc.json()["values"].clone();
        values
            .as_object_mut()
            .expect("the document's values")
            .insert("Port".to_string(), json!(port));
        self.save(
            id,
            json!({ "revision": doc.json()["revision"], "values": values }),
        )
        .await
    }
}

// ── Failure paths first: the session gate of every route ─────────────────

/// With no session, every route of the pool answers 401 `session-required` and
/// the manager is never even connected to: the gate answers on its own.
#[tokio::test]
async fn every_server_profile_route_needs_a_session() {
    let app =
        app_for(Fixture::pool(&[("big", 8123), ("default", 8080)]).used(&[("default", "big")]));
    let cookieless: Vec<(Method, String, Option<String>)> = vec![
        (Method::GET, "/api/profiles/server".into(), None),
        (
            Method::POST,
            "/api/profiles/server".into(),
            Some(json!({"engine": ENGINE, "id": "new", "port": 9000}).to_string()),
        ),
        (Method::DELETE, "/api/profiles/server/big".into(), None),
        (Method::GET, "/api/profiles/server/big".into(), None),
        (
            Method::PUT,
            "/api/profiles/server/big".into(),
            Some(json!({"revision": "0000000000000000", "values": {}}).to_string()),
        ),
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

/// The cookie of a session that ended is refused on every route, and nothing
/// is asked of the manager: the pool is no hole in the sign-in page.
#[tokio::test]
async fn a_terminated_session_is_refused_on_every_route() {
    let app = app_for(Fixture::pool(&[("big", 8123)]).used(&[("default", "big")]));
    let cookie = app.sign_in().await;
    let signed_out = app.ask(Method::DELETE, "/api/session", Some(&cookie)).await;
    assert_eq!(signed_out.status, StatusCode::NO_CONTENT, "signed out");
    for (method, uri) in [
        (Method::GET, "/api/profiles/server".to_string()),
        (Method::DELETE, "/api/profiles/server/big".to_string()),
        (Method::GET, "/api/profiles/server/big".to_string()),
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

/// The manager refused the signed-in user: 403 `session-not-administrator` on
/// every route, and the session ends with the right that authorized it, so the
/// same cookie is 401 after it.
#[tokio::test]
async fn a_refusal_of_the_user_is_403_and_ends_the_session_on_every_route() {
    for route in ["list", "create", "delete", "document", "save"] {
        let mut fixture = Fixture::pool(&[("big", 8123)]).used(&[("default", "big")]);
        fixture.profiles = Err(ClientError::NotAuthorized);
        fixture.create = Err(ClientError::NotAuthorized);
        fixture.delete = Err(ClientError::NotAuthorized);
        fixture.read_fail = Some(ClientError::NotAuthorized);
        let app = app_for(fixture);
        let (reply, cookie) = match route {
            "list" => app.list().await,
            "create" => {
                app.create(json!({"engine": ENGINE, "id": "new", "port": 9000}))
                    .await
            }
            "delete" => app.delete("big").await,
            "document" => app.document("big").await,
            _ => {
                let (doc, cookie) = app.document("big").await;
                assert_eq!(doc.status, StatusCode::FORBIDDEN, "{}", doc.body);
                (doc, cookie)
            }
        };
        reply.assert_error(StatusCode::FORBIDDEN, "session-not-administrator", &[]);
        let who = app.session_of(&cookie).await;
        assert_eq!(
            who.status,
            StatusCode::UNAUTHORIZED,
            "a session lived on after a refusal of {route}: {}",
            who.body
        );
    }
}

/// A manager that is gone, and one that does not answer in the bff's time, are
/// 503 `client-unavailable` on the reads. A manager that takes a write and is
/// gone when it answers is 504 `client-timeout` on the writes: the write
/// crossed the hand-over, so its outcome is unknown (slice W2,
/// `tests/write_timeouts.rs`).
#[tokio::test]
async fn a_manager_gone_on_a_read_is_503_and_lost_on_a_write_is_504() {
    let app = app_for(Fixture {
        delay: Duration::from_secs(2),
        ..Fixture::pool(&[("big", 8123)])
    });
    app.list()
        .await
        .0
        .assert_error(StatusCode::SERVICE_UNAVAILABLE, "client-unavailable", &[]);

    let mut gone = Fixture::pool(&[("big", 8123)]);
    gone.profiles = Err(ClientError::Unavailable);
    gone.read_fail = Some(ClientError::Unavailable);
    gone.create = Err(ClientError::Unavailable);
    gone.delete = Err(ClientError::Unavailable);
    let app = app_for(gone);
    let reads = [
        ("list", app.list().await.0),
        ("document", app.document("big").await.0),
    ];
    for (what, reply) in reads {
        reply.assert_error(StatusCode::SERVICE_UNAVAILABLE, "client-unavailable", &[]);
        let _ = what;
    }
    let writes = [
        (
            "create",
            app.create(json!({"engine": ENGINE, "id": "new", "port": 9000}))
                .await
                .0,
        ),
        ("delete", app.delete("big").await.0),
    ];
    for (what, reply) in writes {
        reply.assert_error(StatusCode::GATEWAY_TIMEOUT, "client-timeout", &[]);
        let _ = what;
    }
}

/// Any other failure is 502 `client-other`, with the manager's detail left out
/// of the body, and the session lives on.
#[tokio::test]
async fn any_other_failure_is_failed_without_the_detail() {
    let mut fixture = Fixture::pool(&[("big", 8123)]);
    fixture.profiles = Err(ClientError::Other("the pipe went away".into()));
    let app = app_for(fixture);
    let (reply, cookie) = app.list().await;
    reply.assert_error(StatusCode::BAD_GATEWAY, "client-other", &[]);
    assert!(
        !reply.body.contains("pipe"),
        "the manager's detail is in the body: {}",
        reply.body
    );
    app.session_of(&cookie).await.assert_signed_in_as("alice");
}

// ── The list ─────────────────────────────────────────────────────────────

/// `GET /api/profiles/server` is the manager's rows, in its order, each as
/// `{id, engine, port, tags, instance}`, `instance` the id using it or `null`.
/// The bff's own account asks: the connect is handed no principal, because
/// `ServerProfiles()` answers to any caller.
#[tokio::test]
async fn the_list_is_the_manager_s_rows_in_its_order() {
    let fixture = Fixture {
        profiles: Ok(vec![
            profile("big", ENGINE, 8123, &["gpu", "edge"], Some("default")),
            profile("small", ENGINE, 0, &[], None),
        ]),
        ..Fixture::default()
    };
    let app = app_for(fixture);
    let (reply, _) = app.list().await;
    assert_eq!(reply.status, StatusCode::OK, "{}", reply.body);
    assert_eq!(
        reply.json(),
        json!([
            {"id": "big", "engine": ENGINE, "port": 8123, "tags": ["gpu", "edge"], "instance": "default"},
            {"id": "small", "engine": ENGINE, "port": 0, "tags": [], "instance": null},
        ]),
        "{}",
        reply.body
    );
    assert_eq!(
        app.calls(),
        vec!["profiles"],
        "the list asks the pool, nothing else"
    );
    assert_eq!(
        principals_of(&app.principals),
        vec![None],
        "the pool list is the bff's own account"
    );
}

/// Every route of the pool moves the session's idle deadline: a request
/// `SHORT` after the sign-in is answered, which a read that only peeks would
/// refuse.
#[tokio::test]
async fn every_server_profile_route_moves_the_idle_deadline() {
    for route in ["list", "create", "delete", "document", "save"] {
        let app =
            app_for(Fixture::pool(&[("big", 8123), ("unused", 8124)]).used(&[("default", "big")]));
        let cookie = app.sign_in().await;
        app.clock.advance(SHORT);
        let reply = match route {
            "list" => app.get("/api/profiles/server", Some(&cookie)).await,
            "create" => {
                app.ask_json(
                    Method::POST,
                    "/api/profiles/server",
                    Some(&cookie),
                    &json!({"engine": ENGINE, "id": "fresh", "port": 9000}).to_string(),
                )
                .await
            }
            "delete" => {
                app.ask(Method::DELETE, "/api/profiles/server/unused", Some(&cookie))
                    .await
            }
            "document" => app.get("/api/profiles/server/big", Some(&cookie)).await,
            _ => {
                app.ask_json(
                    Method::PUT,
                    "/api/profiles/server/big",
                    Some(&cookie),
                    &json!({"revision": "x", "values": {}}).to_string(),
                )
                .await
            }
        };
        assert_ne!(
            reply.status,
            StatusCode::UNAUTHORIZED,
            "a {route} {SHORT:?} after the sign-in is refused: {}",
            reply.body
        );
    }
}

// ── Creating a profile ───────────────────────────────────────────────────

/// A create of a free name is 201 `{"id"}`, acts for the signed-in user, and
/// hands the manager the engine, the name and the port of the body.
#[tokio::test]
async fn a_create_answers_201_with_the_id_and_acts_for_the_user() {
    let app = app_for(Fixture::pool(&[("default", 8080)]));
    let (reply, _) = app
        .create(json!({"engine": ENGINE, "id": "big", "port": 8123}))
        .await;
    assert_eq!(reply.status, StatusCode::CREATED, "{}", reply.body);
    assert_eq!(reply.json(), json!({"id": "big"}), "{}", reply.body);
    assert_eq!(
        app.calls(),
        vec!["create llama.cpp big 8123"],
        "the verb, with the body's engine, name and port"
    );
    assert_eq!(
        principals_of(&app.principals),
        vec![Some("alice".to_string())],
        "the write acts for the signed-in user"
    );
}

/// A body with no `port`, and one with `port: null`, are a profile at the
/// defaults: the manager is handed port None, the file with no `Port` line.
#[tokio::test]
async fn a_create_with_no_port_is_the_default_file() {
    for body in [
        json!({"engine": ENGINE, "id": "big"}),
        json!({"engine": ENGINE, "id": "big", "port": null}),
    ] {
        let app = app_for(Fixture::pool(&[("default", 8080)]));
        let (reply, _) = app.create(body.clone()).await;
        assert_eq!(reply.status, StatusCode::CREATED, "{body}: {}", reply.body);
        assert_eq!(
            app.calls(),
            vec!["create llama.cpp big -"],
            "{body}: the manager is handed no port"
        );
    }
}

/// A name the engine's rule refuses, and an engine the manager does not serve,
/// are 422 from the bff's own check, with the field the form marks: the
/// manager is not asked, so nothing can be created.
#[tokio::test]
async fn a_create_the_bff_check_refuses_is_422_naming_its_field() {
    let long = "x".repeat(33);
    for (body, key, args, field) in [
        (
            json!({"engine": "vllm", "id": "big", "port": 8123}),
            "unknown-engine",
            vec!["vllm".to_string()],
            "engine",
        ),
        (
            json!({"engine": ENGINE, "id": "X", "port": 8123}),
            "server-profile-bad-id",
            vec!["X".to_string()],
            "id",
        ),
        (
            json!({"engine": ENGINE, "id": "a_b", "port": 8123}),
            "server-profile-bad-id",
            vec!["a_b".to_string()],
            "id",
        ),
        (
            json!({"engine": ENGINE, "id": "../x", "port": 8123}),
            "server-profile-bad-id",
            vec!["../x".to_string()],
            "id",
        ),
        (
            json!({"engine": ENGINE, "id": "", "port": 8123}),
            "server-profile-bad-id",
            vec![String::new()],
            "id",
        ),
        (
            json!({"engine": ENGINE, "id": &long, "port": 8123}),
            "server-profile-bad-id",
            vec![long.clone()],
            "id",
        ),
    ] {
        let app = app_for(Fixture::pool(&[("default", 8080)]));
        let (reply, _) = app.create(body.clone()).await;
        let args: Vec<&str> = args.iter().map(String::as_str).collect();
        reply.assert_refused(key, &args, field);
        app.assert_no_create(&format!("the bff refused {body}"));
    }
}

/// A port the manager's own `Port` range refuses is 422 with `field` `port`
/// (the form's control) and the check's args, and the manager is not asked.
#[tokio::test]
async fn a_create_with_a_port_out_of_range_is_422_on_port() {
    let range = checks::server_int_range("Port").expect("Port has a range");
    let (lo, hi) = (range.start().to_string(), range.end().to_string());
    for value in [json!(70000), json!(-1), json!(4294967296i64)] {
        let app = app_for(Fixture::pool(&[("default", 8080)]));
        let (reply, _) = app
            .create(json!({"engine": ENGINE, "id": "big", "port": value}))
            .await;
        let text = value.to_string();
        reply.assert_refused(
            "not-whole-number-in-range",
            &["Port", &text, &lo, &hi],
            "port",
        );
        app.assert_no_create(&format!("the bff refused port {value}"));
    }
}

/// `Port` at each edge of its range is created.
#[tokio::test]
async fn a_create_at_the_ports_edges_is_created() {
    for port in [json!(1), json!(65535)] {
        let app = app_for(Fixture::pool(&[("default", 8080)]));
        let (reply, _) = app
            .create(json!({"engine": ENGINE, "id": "big", "port": port}))
            .await;
        assert_eq!(reply.status, StatusCode::CREATED, "{port}: {}", reply.body);
    }
}

/// A body the bff cannot read is 400 `server-profile-malformed`, with the
/// manager not asked: no JSON, an object that is not the shape, an engine or an
/// id that is not a string, a port of a type the read never gives.
#[tokio::test]
async fn a_create_the_bff_cannot_read_is_400_and_asks_nothing() {
    let bodies: Vec<Value> = vec![
        json!([]),
        json!("big"),
        json!(null),
        json!({"id": "big", "port": 8123}),
        json!({"engine": ENGINE, "port": 8123}),
        json!({"engine": 7, "id": "big"}),
        json!({"engine": ENGINE, "id": 7}),
        json!({"engine": null, "id": "big"}),
        json!({"engine": ENGINE, "id": "big", "port": "8123"}),
        json!({"engine": ENGINE, "id": "big", "port": 8123.5}),
        json!({"engine": ENGINE, "id": "big", "port": true}),
        json!({"engine": ENGINE, "id": "big", "port": []}),
    ];
    for body in bodies {
        let app = app_for(Fixture::pool(&[("default", 8080)]));
        let (reply, _) = app.create(body.clone()).await;
        assert_eq!(
            reply.status,
            StatusCode::BAD_REQUEST,
            "{body}: {}",
            reply.body
        );
        assert_eq!(
            reply.json(),
            json!({"key": "server-profile-malformed", "args": []}),
            "{body}: {}",
            reply.body
        );
        app.assert_no_create(&format!("a body the bff cannot read: {body}"));
    }
}

/// The manager says the name is taken: 409 `client-server-profile-exists`
/// naming it, and the session lives on.
#[tokio::test]
async fn a_create_of_an_existing_profile_is_409_naming_it() {
    let mut fixture = Fixture::pool(&[("big", 8123)]);
    fixture.create = Err(ClientError::ServerProfileExists("big".into()));
    let app = app_for(fixture);
    let (reply, cookie) = app
        .create(json!({"engine": ENGINE, "id": "big", "port": 9000}))
        .await;
    reply.assert_error(
        StatusCode::CONFLICT,
        "client-server-profile-exists",
        &["big"],
    );
    app.session_of(&cookie).await.assert_signed_in_as("alice");
}

// ── Deleting a profile ───────────────────────────────────────────────────

/// An unused profile goes: 204 with no body, for the signed-in user, on the
/// engine the node has.
#[tokio::test]
async fn a_delete_of_an_unused_profile_is_204() {
    let app = app_for(Fixture::pool(&[("default", 8080), ("spare", 8124)]));
    let (reply, _) = app.delete("spare").await;
    assert_eq!(reply.status, StatusCode::NO_CONTENT, "{}", reply.body);
    assert!(reply.body.is_empty(), "204 with a body: {}", reply.body);
    assert_eq!(
        app.calls(),
        vec!["delete llama.cpp spare"],
        "the verb, on the one engine, naming the profile"
    );
    assert_eq!(
        principals_of(&app.principals),
        vec![Some("alice".to_string())],
        "the write acts for the signed-in user"
    );
}

/// A profile an instance's section names is refused: 409
/// `client-server-profile-in-use`, naming the profile and the instance in that
/// order.
#[tokio::test]
async fn a_delete_of_a_profile_in_use_is_409_naming_the_instance() {
    let mut fixture = Fixture::pool(&[("big", 8123)]).used(&[("default", "big")]);
    fixture.delete = Err(ClientError::ServerProfileInUse(
        "big".into(),
        "default".into(),
    ));
    let app = app_for(fixture);
    app.delete("big").await.0.assert_error(
        StatusCode::CONFLICT,
        "client-server-profile-in-use",
        &["big", "default"],
    );
}

/// No such profile: 404 `client-no-server-profile` naming the id, on the
/// delete, the document and the save alike.
#[tokio::test]
async fn a_missing_profile_is_404_naming_it_everywhere() {
    let mut fixture = Fixture::pool(&[("default", 8080)]);
    fixture.delete = Err(ClientError::NoServerProfile("gone".into()));
    let app = app_for(fixture);
    app.delete("gone").await.0.assert_error(
        StatusCode::NOT_FOUND,
        "client-no-server-profile",
        &["gone"],
    );
    let doc = app.document("gone").await.0;
    doc.assert_error(StatusCode::NOT_FOUND, "client-no-server-profile", &["gone"]);
    let saved = app
        .save("gone", json!({"revision": "0", "values": {}}))
        .await
        .0;
    saved.assert_error(StatusCode::NOT_FOUND, "client-no-server-profile", &["gone"]);
}

/// A name the manager refuses is 404 naming it: the rule that keeps a name
/// from building a path outside the engine's directory answers before any file.
#[tokio::test]
async fn a_document_of_a_name_the_manager_refuses_is_404_naming_it() {
    let mut fixture = Fixture::pool(&[("default", 8080)]);
    fixture.read_fail = Some(ClientError::InvalidId);
    let app = app_for(fixture);
    let doc = app.document("X").await.0;
    doc.assert_error(StatusCode::NOT_FOUND, "client-no-server-profile", &["X"]);
}

/// The `instance` of the document comes from the record, so a record the
/// manager cannot read is the record's own failure: never `null`, which would
/// offer a Delete of a profile that is in use.
#[tokio::test]
async fn the_document_of_an_unreadable_record_is_the_record_s_failure() {
    for (failure, status, key) in [
        (
            ClientError::Unavailable,
            StatusCode::SERVICE_UNAVAILABLE,
            "client-unavailable",
        ),
        (
            ClientError::Other("no record".into()),
            StatusCode::BAD_GATEWAY,
            "client-other",
        ),
    ] {
        let fixture = Fixture {
            rows: Err(failure),
            ..Fixture::pool(&[("big", 8123)])
        };
        let app = app_for(fixture);
        let doc = app.document("big").await.0;
        doc.assert_error(status, key, &[]);
    }
}

// ── The document and the save ────────────────────────────────────────────

/// The document of a profile an instance uses: today's server document, keyed
/// by the profile's own file, with `instance` naming that instance and
/// `defaults.StateDir` that instance's folder.
#[tokio::test]
async fn the_document_of_a_used_profile_names_its_instance() {
    let app = app_for(Fixture::pool(&[("big", 8123)]).used(&[("default", "big")]));
    let (reply, _) = app.document("big").await;
    assert_eq!(reply.status, StatusCode::OK, "{}", reply.body);
    let doc = reply.json();
    let mut keys: Vec<&str> = doc
        .as_object()
        .expect("an object")
        .keys()
        .map(String::as_str)
        .collect();
    keys.sort();
    assert_eq!(
        keys,
        [
            "defaults",
            "instance",
            "interfaces",
            "limits",
            "revision",
            "values"
        ],
        "the document's members: {}",
        reply.body
    );
    assert_eq!(doc["instance"], json!("default"), "{}", reply.body);
    assert_eq!(
        doc["defaults"]["StateDir"],
        json!(paths::instance_state_dir()
            .join("default")
            .to_string_lossy()),
        "the state folder of the instance that runs it"
    );
    assert_eq!(doc["values"]["Port"], json!(8123), "{}", reply.body);
    let calls = app.calls();
    assert!(
        calls.contains(&"read server@big.ini".to_string()) && calls.contains(&"instances".into()),
        "the profile's own file, and the record for who uses it: {calls:?}"
    );
    assert!(
        !calls.iter().any(|c| c == "read server@default.ini"),
        "the profile is keyed by its own name: {calls:?}"
    );
}

/// An unused profile's document has `instance: null` and no state folder (the
/// folder belongs to whichever instance runs it, so the document shows none).
#[tokio::test]
async fn the_document_of_an_unused_profile_has_no_instance() {
    let app = app_for(Fixture::pool(&[("spare", 8124)]).used(&[("default", "big")]));
    let (reply, _) = app.document("spare").await;
    assert_eq!(reply.status, StatusCode::OK, "{}", reply.body);
    let doc = reply.json();
    assert_eq!(doc["instance"], json!(null), "{}", reply.body);
    assert_eq!(doc["defaults"]["StateDir"], json!(null), "{}", reply.body);
    assert!(
        !app.calls().iter().any(|c| c.starts_with("status")),
        "an unused profile has no state to read: {:?}",
        app.calls()
    );
}

/// The save of a profile writes that profile's file, at `server@<id>.ini`,
/// with the text the body renders, and 200 answers the new revision and the
/// state of the instance that uses it.
#[tokio::test]
async fn the_save_writes_the_profile_s_own_file() {
    let written: Written = Arc::default();
    let app = app_for(
        Fixture::pool(&[("big", 8123)])
            .used(&[("default", "big")])
            .state("default", InstanceState::Active)
            .writing(written.clone()),
    );
    let (reply, _) = app.save_with_port("big", 9000).await;
    assert_eq!(reply.status, StatusCode::OK, "{}", reply.body);
    let answer = reply.json();
    let got = written_of(&written);
    assert_eq!(
        got.iter()
            .map(|(name, _)| name.as_str())
            .collect::<Vec<_>>(),
        vec!["server@big.ini"],
        "the profile's own file, written once"
    );
    let text = &got[0].1;
    assert!(
        text.contains("Port = 9000"),
        "the body's port is in the text: {text}"
    );
    assert_eq!(
        answer
            .as_object()
            .expect("an object")
            .keys()
            .collect::<Vec<_>>(),
        vec!["revision", "running"],
        "the save answers the revision and the state"
    );
    assert_eq!(answer["running"], json!(true), "{}", reply.body);
    assert_eq!(
        answer["revision"],
        json!(revision_of(text, &REVISION_KEY)),
        "the revision of the text that was written"
    );
    let calls = app.calls();
    assert!(
        calls.contains(&"write-if server@big.ini".to_string()),
        "the write is a compare and swap: {calls:?}"
    );
    assert!(
        !calls.iter().any(|c| c == "read server@default.ini"),
        "the instance's own file is not touched: {calls:?}"
    );
}

/// The save of an unused profile answers `running: false`: no instance runs it,
/// and the bff asks no state.
#[tokio::test]
async fn the_save_of_an_unused_profile_answers_running_false() {
    let written: Written = Arc::default();
    let app = app_for(
        Fixture::pool(&[("spare", 8124)])
            .used(&[("default", "big")])
            .writing(written.clone()),
    );
    let (reply, _) = app.save_with_port("spare", 9000).await;
    assert_eq!(reply.status, StatusCode::OK, "{}", reply.body);
    assert_eq!(reply.json()["running"], json!(false), "{}", reply.body);
    assert_eq!(
        written_of(&written)
            .iter()
            .map(|(name, _)| name.as_str())
            .collect::<Vec<_>>(),
        vec!["server@spare.ini"],
        "the unused profile's own file"
    );
    assert!(
        !app.calls().iter().any(|c| c.starts_with("status")),
        "no instance runs it, so no state is asked: {:?}",
        app.calls()
    );
}

/// The record read that fails once the write landed leaves the state unknown, so
/// the save answers 200 with `running: true`: `false` claims the instance is
/// stopped, a claim the bff cannot make, and it hides that a restart may be
/// needed. The write itself is a success, so the save is a 200, not a refusal.
#[tokio::test]
async fn the_save_answers_running_true_when_the_record_read_after_the_write_fails() {
    let written: Written = Arc::default();
    let app = app_for(Fixture {
        rows: Err(ClientError::Unavailable),
        ..Fixture::pool(&[("big", 8123)])
            .used(&[("default", "big")])
            .state("default", InstanceState::Active)
            .writing(written.clone())
    });
    // The record is the fake's, so the page's read fails too: the body is built
    // from the text the fake holds, at that text's own revision. `Tags` is an
    // array in a body the read's shape, so the values carry it.
    let cookie = app.sign_in().await;
    let reply = app
        .ask_json(
            Method::PUT,
            "/api/profiles/server/big",
            Some(&cookie),
            &json!({
                "revision": revision_of(&profile_text(8123), &REVISION_KEY),
                "values": { "Port": 9000, "Tags": [] }
            })
            .to_string(),
        )
        .await;
    assert_eq!(reply.status, StatusCode::OK, "{}", reply.body);
    assert_eq!(
        reply.json()["running"],
        json!(true),
        "a state the bff cannot read answers running: {}",
        reply.body
    );
    assert_eq!(
        written_of(&written)
            .iter()
            .map(|(name, _)| name.as_str())
            .collect::<Vec<_>>(),
        vec!["server@big.ini"],
        "the record's failure did not stop the write"
    );
    assert_eq!(
        app.calls(),
        vec![
            "read server@big.ini".to_string(),
            "write-if server@big.ini".to_string(),
            "instances".to_string()
        ],
        "the record is asked once, after the write, and no state is asked of an \
         instance the record cannot name"
    );
}

/// The state read that fails once the write landed is unknown as well: the save
/// answers 200 with `running: true`, as the instance's own save does
/// (`server.rs`'s `running_is_true_when_the_status_read_after_the_write_fails`).
/// Guard: the route answers true today, and this pins the same rule for the
/// status, beside the record's case above.
#[tokio::test]
async fn the_save_answers_running_true_when_the_state_read_after_the_write_fails() {
    let written: Written = Arc::default();
    let app = app_for(Fixture {
        status_fail: Some(ClientError::Unavailable),
        ..Fixture::pool(&[("big", 8123)])
            .used(&[("default", "big")])
            .state("default", InstanceState::Active)
            .writing(written.clone())
    });
    let (reply, _) = app.save_with_port("big", 9000).await;
    assert_eq!(reply.status, StatusCode::OK, "{}", reply.body);
    assert_eq!(
        reply.json()["running"],
        json!(true),
        "the state read failed, so the state is unknown: {}",
        reply.body
    );
    assert_eq!(
        written_of(&written)
            .iter()
            .map(|(name, _)| name.as_str())
            .collect::<Vec<_>>(),
        vec!["server@big.ini"],
        "the state's failure did not stop the write"
    );
    assert!(
        app.calls().iter().any(|c| c == "status default"),
        "the instance using the profile is the one asked: {:?}",
        app.calls()
    );
}

/// A save whose revision is no longer the file's is the document's 409
/// `server-changed-elsewhere`, and the write never reached the manager.
#[tokio::test]
async fn a_save_of_a_stale_revision_is_409() {
    let written: Written = Arc::default();
    let app = app_for(
        Fixture::pool(&[("big", 8123)])
            .used(&[("default", "big")])
            .writing(written.clone()),
    );
    let (doc, _) = app.document("big").await;
    assert_eq!(doc.status, StatusCode::OK, "{}", doc.body);
    let (reply, _) = app
        .save(
            "big",
            json!({"revision": "0000000000000000", "values": doc.json()["values"]}),
        )
        .await;
    reply.assert_error(StatusCode::CONFLICT, "server-changed-elsewhere", &[]);
    assert!(
        written_of(&written).is_empty(),
        "a stale revision wrote: {:?}",
        written_of(&written)
    );
}

/// The old route stays: `GET`/`PUT /api/instances/{id}/server` answer as
/// before, its document with no `instance`, reading the profile the record
/// names for the instance.
#[tokio::test]
async fn the_instance_server_route_still_answers_as_before() {
    let written: Written = Arc::default();
    let app = app_for(
        Fixture::pool(&[("big", 8123)])
            .used(&[("default", "big")])
            .state("default", InstanceState::Active)
            .writing(written.clone()),
    );
    let cookie = app.sign_in().await;
    let reply = app
        .get("/api/instances/default/server", Some(&cookie))
        .await;
    assert_eq!(reply.status, StatusCode::OK, "{}", reply.body);
    let doc = reply.json();
    assert!(
        doc.get("instance").is_none(),
        "the old document gained a member: {}",
        reply.body
    );
    assert_eq!(doc["values"]["Port"], json!(8123), "{}", reply.body);
    assert_eq!(
        doc["defaults"]["StateDir"],
        json!(paths::instance_state_dir()
            .join("default")
            .to_string_lossy()),
        "{}",
        reply.body
    );
    let mut values = doc["values"].clone();
    values
        .as_object_mut()
        .expect("the document's values")
        .insert("Port".to_string(), json!(9010));
    let saved = app
        .ask_json(
            Method::PUT,
            "/api/instances/default/server",
            Some(&cookie),
            &json!({"revision": doc["revision"], "values": values}).to_string(),
        )
        .await;
    assert_eq!(saved.status, StatusCode::OK, "{}", saved.body);
    assert_eq!(
        saved.json()["running"],
        json!(true),
        "the instance that uses it runs"
    );
    assert_eq!(
        written_of(&written)
            .iter()
            .map(|(name, _)| name.as_str())
            .collect::<Vec<_>>(),
        vec!["server@big.ini"],
        "the instance's profile, as the record names it"
    );
}

// ── The GPUs of the profile: `[Server] Devices` (R2) ────────────────────

/// The profile route carries the GPUs like the instance route: the document
/// gives the file's list, a save of `Devices` lands in the file, and a save of
/// another key keeps it.
#[tokio::test]
async fn the_profile_document_carries_and_keeps_the_devices() {
    let written: Written = Arc::default();
    let text = "[Server]\nPort = 8123\nDevices = CUDA0,ROCm1\n";
    let app = app_for(Fixture {
        files: HashMap::from([("server@big.ini".to_string(), text.to_string())]),
        written: Some(written.clone()),
        ..Fixture::pool(&[("big", 8123)])
    });
    let (reply, _) = app.document("big").await;
    assert_eq!(
        reply.json()["values"]["Devices"],
        json!("CUDA0,ROCm1"),
        "{}",
        reply.body
    );

    let body = |values: Value| {
        json!({
            "revision": revision_of(text, &REVISION_KEY),
            "values": values
        })
    };
    let (reply, _) = app
        .save(
            "big",
            body(json!({ "Port": 9000, "Tags": [], "Devices": "CUDA0,ROCm1" })),
        )
        .await;
    assert_eq!(reply.status, StatusCode::OK, "{}", reply.body);
    let contents = written_of(&written);
    assert_eq!(contents.len(), 1, "{contents:?}");
    assert!(
        contents[0].1.contains("Devices = CUDA0,ROCm1"),
        "the save of Port lost the GPUs: {}",
        contents[0].1
    );

    let (reply, _) = app
        .save(
            "big",
            body(json!({ "Port": 9000, "Tags": [], "Devices": "none" })),
        )
        .await;
    assert_eq!(reply.status, StatusCode::OK, "{}", reply.body);
    let contents = written_of(&written);
    assert_eq!(contents.len(), 2, "{contents:?}");
    assert!(
        contents[1].1.contains("Devices = none"),
        "`none` (CPU only) is a value the save writes: {}",
        contents[1].1
    );
}

/// The same refusal on this route: 422 `device-bad-name` naming the field
/// `Devices`, nothing written.
#[tokio::test]
async fn a_profile_save_of_devices_the_rule_refuses_names_the_field() {
    let written: Written = Arc::default();
    let text = "[Server]\nPort = 8123\n";
    let app = app_for(Fixture {
        files: HashMap::from([("server@big.ini".to_string(), text.to_string())]),
        written: Some(written.clone()),
        ..Fixture::pool(&[("big", 8123)])
    });
    app.save(
        "big",
        json!({
            "revision": revision_of(text, &REVISION_KEY),
            "values": { "Port": 8123, "Tags": [], "Devices": "ROCm0, ROCm1" }
        }),
    )
    .await
    .0
    .assert_refused("device-bad-name", &["Devices", " ROCm1"], "Devices");
    assert!(written_of(&written).is_empty(), "nothing written");
}

/// The marker rule on this route too: a `Devices` value with a `;` or a `#` is
/// 422 `comment-markers`, the field `Devices`, nothing written.
#[tokio::test]
async fn a_profile_save_of_devices_with_a_comment_marker_is_422_comment_markers() {
    for value in ["ROCm0;x", "ROCm0#y"] {
        let written: Written = Arc::default();
        let text = "[Server]\nPort = 8123\n";
        let app = app_for(Fixture {
            files: HashMap::from([("server@big.ini".to_string(), text.to_string())]),
            written: Some(written.clone()),
            ..Fixture::pool(&[("big", 8123)])
        });
        app.save(
            "big",
            json!({
                "revision": revision_of(text, &REVISION_KEY),
                "values": { "Port": 8123, "Tags": [], "Devices": value }
            }),
        )
        .await
        .0
        .assert_refused("comment-markers", &["Devices"], "Devices");
        assert!(
            written_of(&written).is_empty(),
            "{value:?}: nothing written"
        );
    }
}

/// The revision the bff answers for `text` under `key`: SipHash-2-4 of the
/// text under the router's 16 bytes, as `server.rs`'s `revision` computes it.
fn revision_of(text: &str, key: &[u8; 16]) -> String {
    let mut hasher = SipHasher::new_with_key(key);
    hasher.write(text.as_bytes());
    format!("{:016x}", hasher.finish())
}
