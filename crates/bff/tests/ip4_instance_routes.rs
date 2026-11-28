//! The instance routes (IP4d), through
//! `pai_bff::router_with_session_and_write_timeout` with a fake manager client
//! and a fake authenticator, never the real bus, pipe, PAM or LogonUser:
//! `GET /api/instances` (the rows), `PUT /api/instances/{id}/profile`,
//! `PUT /api/instances/{id}/address` and `POST /api/instances`. Every request
//! runs under a deadline, so a bff that waits on the manager fails instead of
//! hanging.
//!
//! The contract (`ARCHITECTURE.md` IP4, copied by `fe/briefs/api-contract.md`):
//! - The session, as everywhere: 401 `session-required` with no session and no
//!   connect at all; 403 `session-not-administrator` and the session ended when
//!   the manager refuses the signed-in user; 503 `client-unavailable` before the
//!   manager's call was handed over, 504 `client-timeout` after, the session
//!   kept. The rows read answers to the bff's own account, so a manager that
//!   refuses that account is 502 `client-not-authorized` and ends no session.
//! - The rows gain `instanceProfile`, `hostname` (`null` when the record sets
//!   none), `converged`, `reason` (`{"key", "args"}` or `null`) and
//!   `pendingRestart`, the last three off the manager's row.
//! - `PUT .../profile` `{"instanceProfile"}` and `PUT .../address`
//!   `{"hostname", "port"}` (a `null` hostname clears it, both texts as the
//!   manager's strings) carry no revision on the wire: the bff reads the text
//!   the manager compares (the pool's for the switch, `instances.ini` for the
//!   address) and hands it over, so a change in that window is the manager's
//!   409 `stale-revision`. Both answer the affected answer `{"running": [ids],
//!   "notConverged": [{instance, key, args}]}`, as does every re-rendering
//!   write.
//! - `POST /api/instances` `{"engine", "id", "instanceProfile"}` answers 201
//!   `{"id", "port"}`, the port the manager picked from 8581. It supersedes the
//!   served create, whose body named a server profile and whose 201 carried the
//!   id alone.
//! - Refusals: 404 `client-no-instance` for an id the record does not name; 422
//!   `no-instance-profile-named` for a profile the pool has not got (the key in
//!   a body is a 422, per the IP4 key table); 409 `address-in-use [instance]`;
//!   422 `invalid-port` and 422 `invalid-hostname` keep the manager's keys; 409
//!   `instance-exists`; 503 `no-free-port`. A body nothing can be made of is 400
//!   `*-malformed` with the manager not connected to, the retired create's
//!   `serverProfile` key among those bodies: named at all, it is refused, never
//!   read as an instance profile.

use axum::body::Body;
use axum::extract::ConnectInfo;
use axum::http::{header, Method, Request, StatusCode};
use axum::Router;
use http_body_util::BodyExt;
use pai_bff::session::{AuthError, Authenticator, Identity};
use pai_bff::{Client, Connect};
use pai_core::checks::Refusal;
use pai_core::client::{
    Affected, ClientError, InstanceInfo, InstanceProfileInfo, InstanceState, ManagerClient,
    ManagerFile, NotConverged, NotConvergedRow,
};
use pai_core::instance_profiles;
use serde_json::{json, Value};
use std::net::SocketAddr;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};
use tower::ServiceExt;

/// How long the bff waits for the manager's reads in these tests.
const TIMEOUT: Duration = Duration::from_millis(200);

/// How long a test waits for the bff's answer before failing.
const DEADLINE: Duration = Duration::from_secs(5);

/// The client address a sign-in is sent from (the lockout counters key on it).
const CLIENT: &str = "127.0.0.1:52001";

/// The revision secret these routers are built with.
const REVISION_KEY: [u8; 16] = [0; 16];

/// The key of the gate's answer (`session.rs`).
const SESSION_REQUIRED: &str = "session-required";

/// The engine every instance of these tests runs.
const ENGINE: &str = "llama.cpp";

/// The text of `instances.ini` the fake answers: the address write hands it
/// back to the manager as the revision the manager compares.
const RECORD: &str = "[default]\nPort = 8581\n\n[two]\nHostname = 127.0.0.1\nPort = 8582\n";

/// The log of the manager's calls.
type Log = Arc<Mutex<Vec<String>>>;

/// The pool's text, shared so a read of a profile answers the text of the
/// moment.
type Pool = Arc<Mutex<String>>;

/// The `(instance, profile, revision)` of the profile switch the fake was
/// handed.
type LastSetProfile = Arc<Mutex<Option<Triple>>>;

/// The `(instance, host, port, revision)` of the address write the fake was
/// handed.
type LastSetAddress = Arc<Mutex<Option<(String, String, String, String)>>>;

/// The `(engine, id, instance profile)` of the create the fake was handed.
type LastCreate = Arc<Mutex<Option<Triple>>>;

/// Three words kept together: what a write call was handed, short of the
/// revision.
type Triple = (String, String, String);

/// The text of `instance-profiles.ini`: `[<p>]` sections for `profiles`, the
/// `(id, server profile)` pairs in order.
fn pool_text(profiles: &[(&str, &str)]) -> String {
    let mut text = String::new();
    for (id, server) in profiles {
        text = instance_profiles::add_profile(&text, id, server);
    }
    text
}

/// The pool these tests start from: `alpha` on `default` and `beta` on `big`.
fn pool() -> String {
    pool_text(&[("alpha", "default"), ("beta", "big")])
}

/// One row of `instances`: the instance `id` running the instance profile
/// `profile` on the host `hostname`, in the state named by the last three.
fn full_row(
    id: &str,
    profile: Option<&str>,
    hostname: Option<&str>,
    converged: bool,
    reason: Option<NotConverged>,
    pending_restart: bool,
) -> InstanceInfo {
    InstanceInfo {
        id: id.to_string(),
        engine: ENGINE.to_string(),
        server_profile: Some("default".to_string()),
        port: 8080,
        profile_tags: Vec::new(),
        instance_tags: Vec::new(),
        label: None,
        converged,
        reason,
        pending_restart,
        instance_profile: profile.map(str::to_string),
        hostname: hostname.map(str::to_string),
    }
}

/// One converged row running the instance profile `profile`, no host set: the
/// shape the writes and the gates run against.
fn row(id: &str, profile: Option<&str>) -> InstanceInfo {
    full_row(id, profile, None, true, None, false)
}

/// What every re-rendering write answers: two instances up, one left without
/// the pair it launched with. The wire shape is read off this.
fn affected() -> Affected {
    Affected {
        running: vec!["default".to_string(), "two".to_string()],
        not_converged: vec![NotConvergedRow {
            instance: "two".to_string(),
            key: "over-memory".to_string(),
            args: vec!["alpha1".to_string(), "2".to_string()],
        }],
    }
}

/// `affected()` as the wire answers it.
fn affected_json() -> Value {
    json!({
        "running": ["default", "two"],
        "notConverged": [{ "instance": "two", "key": "over-memory", "args": ["alpha1", "2"] }]
    })
}

/// One keyed check's refusal, as the manager answers it.
fn refusal(key: &'static str, args: &[&str]) -> ClientError {
    ClientError::Invalid(Refusal {
        key,
        values: args.iter().map(|a| a.to_string()).collect(),
    })
}

/// What the fake manager answers.
#[derive(Clone)]
struct Fixture {
    /// The pool's text.
    pool: Pool,
    /// The record's rows, as `instances` answers them.
    rows: Vec<InstanceInfo>,
    /// What the rows read answers, ahead of `rows`: both its verbs, so the
    /// read fails whichever one the bff asks first.
    rows_fail: Option<ClientError>,
    /// What `set_instance_profile` answers.
    set_profile: Result<Affected, ClientError>,
    /// What `set_instance_address` answers.
    set_address: Result<Affected, ClientError>,
    /// What `create_instance` answers: the port the manager picked.
    create: Result<u16, ClientError>,
    /// How long every write call sleeps: the write that outlives the limit.
    write_delay: Duration,
    /// The last profile switch the fake was handed.
    last_set_profile: LastSetProfile,
    /// The last address write the fake was handed.
    last_set_address: LastSetAddress,
    /// The last create the fake was handed.
    last_create: LastCreate,
}

impl Default for Fixture {
    fn default() -> Fixture {
        Fixture {
            pool: Pool::default(),
            rows: Vec::new(),
            rows_fail: None,
            set_profile: Ok(affected()),
            set_address: Ok(affected()),
            create: Ok(8581),
            write_delay: Duration::ZERO,
            last_set_profile: LastSetProfile::default(),
            last_set_address: LastSetAddress::default(),
            last_create: LastCreate::default(),
        }
    }
}

impl Fixture {
    /// The pool of `pool()`.
    fn with_pool(self) -> Fixture {
        *self.pool.lock().unwrap_or_else(|e| e.into_inner()) = pool();
        self
    }

    /// The record's rows are exactly `rows`.
    fn with_rows(mut self, rows: Vec<InstanceInfo>) -> Fixture {
        self.rows = rows;
        self
    }

    /// The record gains one converged row per `(instance, instance profile)`
    /// entry.
    fn used(mut self, entries: &[(&str, Option<&str>)]) -> Fixture {
        self.rows = entries
            .iter()
            .map(|(id, profile)| row(id, *profile))
            .collect();
        self
    }

    /// The three writes answer `fail`: on the sweep, the manager refusing the
    /// signed-in user on every one of them.
    fn refuses_writes(mut self, fail: ClientError) -> Fixture {
        self.set_profile = Err(fail.clone());
        self.set_address = Err(fail.clone());
        self.create = Err(fail);
        self
    }

    /// The rows read answers `fail`.
    fn rows_fails(mut self, fail: ClientError) -> Fixture {
        self.rows_fail = Some(fail);
        self
    }

    /// `set_instance_profile` answers `fail`.
    fn set_profile_fails(mut self, fail: ClientError) -> Fixture {
        self.set_profile = Err(fail);
        self
    }

    /// `set_instance_address` answers `fail`.
    fn set_address_fails(mut self, fail: ClientError) -> Fixture {
        self.set_address = Err(fail);
        self
    }

    /// `create_instance` answers `fail`.
    fn create_fails(mut self, fail: ClientError) -> Fixture {
        self.create = Err(fail);
        self
    }

    /// Every write call sleeps `delay` before it answers.
    fn write_delay(mut self, delay: Duration) -> Fixture {
        self.write_delay = delay;
        self
    }
}

/// A manager that answers from a [`Fixture`] and logs every call (`list`,
/// `status <id>`, `instances`, `read <name>`, `instance-profile <p>`,
/// `set-profile <i> <p>`, `set-address <i> <host> <port>`,
/// `create <engine> <id> <p>`).
struct FakeManager {
    fixture: Fixture,
    log: Log,
}

impl FakeManager {
    fn new(log: &Log, fixture: &Fixture) -> FakeManager {
        FakeManager {
            fixture: Fixture {
                pool: fixture.pool.clone(),
                rows: fixture.rows.clone(),
                rows_fail: fixture.rows_fail.clone(),
                set_profile: fixture.set_profile.clone(),
                set_address: fixture.set_address.clone(),
                create: fixture.create.clone(),
                write_delay: fixture.write_delay,
                last_set_profile: fixture.last_set_profile.clone(),
                last_set_address: fixture.last_set_address.clone(),
                last_create: fixture.last_create.clone(),
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

    fn pool(&self) -> String {
        self.fixture
            .pool
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .clone()
    }

    fn keep(&self, slot: &LastSetProfile, value: Triple) {
        *slot.lock().unwrap_or_else(|e| e.into_inner()) = Some(value);
    }

    /// The write's own delay, then its answer.
    fn answered<T: Clone>(&self, answer: &Result<T, ClientError>) -> Result<T, ClientError> {
        std::thread::sleep(self.fixture.write_delay);
        answer.clone()
    }
}

impl ManagerClient for FakeManager {
    fn list_instances(&self) -> Result<Vec<String>, ClientError> {
        self.record("list".into());
        if let Some(fail) = self.fixture.rows_fail.clone() {
            return Err(fail);
        }
        Ok(self.fixture.rows.iter().map(|r| r.id.clone()).collect())
    }
    fn status(&self, id: &str) -> Result<InstanceState, ClientError> {
        self.record(format!("status {id}"));
        Ok(InstanceState::Inactive)
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
        if name == "instances.ini" {
            return Ok(Some(ManagerFile {
                text: RECORD.to_string(),
                withheld: false,
            }));
        }
        Ok(None)
    }
    fn write_file(
        &self,
        name: &str,
        contents: &str,
        keep_api_key: bool,
    ) -> Result<(), ClientError> {
        self.record(format!("write {name} keep={keep_api_key}"));
        let _ = contents;
        Ok(())
    }
    fn boot_enabled(&self, id: &str) -> Result<bool, ClientError> {
        self.record(format!("boot {id}"));
        Ok(false)
    }
    fn set_boot_enabled(&self, id: &str, enabled: bool) -> Result<(), ClientError> {
        self.record(format!("boot {id} {enabled}"));
        Ok(())
    }
    fn authenticate(&self, _user: &str, _password: &str) -> Result<(bool, bool), ClientError> {
        Ok((true, true))
    }
    fn instances(&self) -> Result<Vec<InstanceInfo>, ClientError> {
        self.record("instances".into());
        if let Some(fail) = self.fixture.rows_fail.clone() {
            return Err(fail);
        }
        Ok(self.fixture.rows.clone())
    }
    fn instance_profile(&self, profile: &str) -> Result<InstanceProfileInfo, ClientError> {
        self.record(format!("instance-profile {profile}"));
        let text = self.pool();
        match instance_profiles::parse(&text)
            .into_iter()
            .find(|p| p.id.eq_ignore_ascii_case(profile))
        {
            Some(p) => Ok(InstanceProfileInfo {
                id: p.id,
                server_profile: p.server_profile.unwrap_or_default(),
                entries: p.entries.into_iter().map(|e| e.model).collect(),
                revision: text,
            }),
            None => Err(ClientError::NoInstanceProfile(profile.to_string())),
        }
    }
    fn set_instance_profile(
        &self,
        instance: &str,
        profile: &str,
        revision: &str,
    ) -> Result<Affected, ClientError> {
        self.record(format!("set-profile {instance} {profile}"));
        self.keep(
            &self.fixture.last_set_profile,
            (
                instance.to_string(),
                profile.to_string(),
                revision.to_string(),
            ),
        );
        self.answered(&self.fixture.set_profile)
    }
    fn set_instance_address(
        &self,
        instance: &str,
        host: &str,
        port: &str,
        revision: &str,
    ) -> Result<Affected, ClientError> {
        self.record(format!("set-address {instance} {host} {port}"));
        *self
            .fixture
            .last_set_address
            .lock()
            .unwrap_or_else(|e| e.into_inner()) = Some((
            instance.to_string(),
            host.to_string(),
            port.to_string(),
            revision.to_string(),
        ));
        self.answered(&self.fixture.set_address)
    }
    fn create_instance(
        &self,
        engine: &str,
        id: &str,
        instance_profile: &str,
    ) -> Result<u16, ClientError> {
        self.record(format!("create {engine} {id} {instance_profile}"));
        self.keep(
            &self.fixture.last_create,
            (
                engine.to_string(),
                id.to_string(),
                instance_profile.to_string(),
            ),
        );
        self.answered(&self.fixture.create)
    }
}

/// A connect that counts the calls and hands out `make(principal)` each time.
fn connect_logging(
    count: &Arc<AtomicUsize>,
    make: impl Fn(Option<&str>) -> Result<Client, ClientError> + Send + Sync + 'static,
) -> Connect {
    let count = count.clone();
    Arc::new(move |principal: Option<&str>| {
        count.fetch_add(1, Ordering::SeqCst);
        make(principal)
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

    /// The whole answer: `status` and exactly `want` as JSON. A route the bff
    /// does not route answers a bare 404 with an empty body, so every one of
    /// these is red, not accidentally green.
    fn assert_json(&self, status: StatusCode, want: &Value) {
        assert_eq!(self.status, status, "{}", self.body);
        assert_eq!(&self.json(), want, "{}", self.body);
    }

    /// An error answer: its status, and `{"key": key, "args": args}` as JSON.
    fn assert_error(&self, status: StatusCode, key: &str, args: &[&str]) {
        self.assert_json(status, &json!({ "key": key, "args": args }));
    }

    /// The gate's answer: 401 `{"key": "session-required", "args": []}`.
    fn assert_session_required(&self) {
        self.assert_error(StatusCode::UNAUTHORIZED, SESSION_REQUIRED, &[]);
    }

    /// An error answer of `status` whose key is `key`, its `args` free: used
    /// where the contract names the key and the status and not the args.
    fn assert_key(&self, status: StatusCode, key: &str) {
        assert_eq!(self.status, status, "{}", self.body);
        assert_eq!(self.json()["key"], key, "{}", self.body);
    }

    /// A body nothing can be made of: 400, refused as a malformed body. The
    /// contract names no key for these routes, so any `<what>-malformed` is
    /// accepted here, with empty args.
    fn assert_malformed(&self) {
        assert_eq!(self.status, StatusCode::BAD_REQUEST, "{}", self.body);
        let doc = self.json();
        let key = doc["key"].as_str().unwrap_or_default();
        assert!(
            key.ends_with("-malformed"),
            "not a body refusal: {}",
            self.body
        );
        assert_eq!(doc["args"], json!([]), "{}", self.body);
    }

    /// 200 `{"user": "alice"}`: the session still lives.
    fn assert_signed_in(&self) {
        assert_eq!(self.status, StatusCode::OK, "signed in: {}", self.body);
        assert_eq!(self.json(), json!({ "user": "alice" }), "{}", self.body);
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

/// The time the session routes read.
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

/// A router over a fake manager, a fake authenticator and a clock, with the log
/// of the manager's calls, the count of connects, and the fixture the fake
/// reads (its shared seams: the pool's text, the last write handed over).
struct App {
    router: Router,
    log: Log,
    connects: Arc<AtomicUsize>,
    fixture: Fixture,
}

/// The app whose manager answers from `fixture`; the fake is built again at
/// every connect.
fn app_for(fixture: Fixture) -> App {
    app_for_write_timeout(fixture, TIMEOUT)
}

/// The app whose manager answers from `fixture`, with the manager's calls of a
/// WRITE bound by `write_timeout` and the reads left at `TIMEOUT`.
fn app_for_write_timeout(fixture: Fixture, write_timeout: Duration) -> App {
    let shared = fixture.clone();
    let built = app_connecting(
        move |principal, log| {
            log.lock()
                .unwrap_or_else(|e| e.into_inner())
                .push(format!("as {}", principal.unwrap_or("-")));
            Ok(Box::new(FakeManager::new(log, &shared)) as Client)
        },
        write_timeout,
    );
    App {
        fixture,
        router: built.router,
        log: built.log,
        connects: built.connects,
    }
}

/// The app whose connect fails every time: no manager to reach.
fn app_no_manager() -> App {
    let built = app_connecting(
        move |_principal, _log| Err(ClientError::Unavailable),
        TIMEOUT,
    );
    App {
        fixture: Fixture::default(),
        router: built.router,
        log: built.log,
        connects: built.connects,
    }
}

/// A router, with the log and the connect count the caller inspects.
struct Built {
    router: Router,
    log: Log,
    connects: Arc<AtomicUsize>,
}

/// The app whose connect hands out what `make` answers. The log is the one the
/// `as <principal>` lines and the calls share.
fn app_connecting(
    make: impl Fn(Option<&str>, &Log) -> Result<Client, ClientError> + Send + Sync + 'static,
    write_timeout: Duration,
) -> Built {
    let log = Log::default();
    let connects = Arc::new(AtomicUsize::new(0));
    let l = log.clone();
    let connect = connect_logging(&connects, move |p| make(p, &l));
    let clock = FakeClock::new();
    let router = pai_bff::router_with_session_and_write_timeout(
        connect,
        TIMEOUT,
        write_timeout,
        Arc::new(FakeAuth),
        clock.clock(),
        REVISION_KEY,
    );
    Built {
        router,
        log,
        connects,
    }
}

impl App {
    fn calls(&self) -> Vec<String> {
        self.log.lock().unwrap_or_else(|e| e.into_inner()).clone()
    }

    /// Nothing was asked of the manager: no connect, no call. The gate answers
    /// on its own, and so does a body nothing can be made of.
    fn assert_asked_nothing(&self, what: &str) {
        assert_eq!(self.connects.load(Ordering::SeqCst), 0, "{what}: connected");
        assert!(self.calls().is_empty(), "{what}: {:?}", self.calls());
    }

    /// No call starts with `prefix`.
    fn assert_no_call(&self, prefix: &str, what: &str) {
        assert!(
            !self.calls().iter().any(|c| c.starts_with(prefix)),
            "{what}: {:?}",
            self.calls()
        );
    }

    /// The principal of the one call starting with `prefix`: `None` for the
    /// bff's own account. The three writes are the manager's `As` methods, so
    /// a write asked as the bff itself is a manager that checks no group.
    fn actor_of(&self, prefix: &str) -> Option<String> {
        let log = self.calls();
        let mut actor: Option<String> = None;
        let mut found: Vec<Option<String>> = Vec::new();
        for line in &log {
            if let Some(rest) = line.strip_prefix("as ") {
                actor = (rest != "-").then(|| rest.to_string());
            } else if line.starts_with(prefix) {
                found.push(actor.clone());
            }
        }
        assert!(!found.is_empty(), "no call {prefix:?}: {log:?}");
        assert_eq!(found.len(), 1, "more than one call {prefix:?}: {log:?}");
        found.into_iter().next().expect("one call")
    }

    /// The `(instance, profile, revision)` of the last profile switch.
    fn last_set_profile(&self) -> (String, String, String) {
        self.fixture
            .last_set_profile
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .clone()
            .expect("the profile switch reached the manager")
    }

    /// The `(instance, host, port, revision)` of the last address write.
    fn last_set_address(&self) -> (String, String, String, String) {
        self.fixture
            .last_set_address
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .clone()
            .expect("the address write reached the manager")
    }

    /// The `(engine, id, instance profile)` of the last create.
    fn last_create(&self) -> (String, String, String) {
        self.fixture
            .last_create
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .clone()
            .expect("the create reached the manager")
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
        let mut builder = Request::builder().method(Method::GET).uri(uri);
        if let Some(c) = cookie {
            builder = builder.header(header::COOKIE, c);
        }
        let request = builder.body(Body::empty()).expect("request");
        self.answer(request, uri).await
    }

    /// A request with a JSON body.
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

    /// A request whose body is bytes no JSON reader makes anything of, sent as
    /// `application/octet-stream`.
    async fn ask_bytes(
        &self,
        method: Method,
        uri: &str,
        cookie: Option<&str>,
        body: &str,
    ) -> Reply {
        let mut builder = Request::builder().method(method).uri(uri);
        if let Some(c) = cookie {
            builder = builder.header(header::COOKIE, c);
        }
        let request = builder
            .header(header::CONTENT_TYPE, "application/octet-stream")
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

    // ── The requests, each signed in first ───────────────────────────────

    /// Sign in and `GET /api/instances`.
    async fn rows(&self) -> (Reply, String) {
        let cookie = self.sign_in().await;
        (self.get("/api/instances", Some(&cookie)).await, cookie)
    }

    /// Sign in and `PUT /api/instances/{id}/profile`.
    async fn set_profile(&self, id: &str, body: &str) -> (Reply, String) {
        let cookie = self.sign_in().await;
        (
            self.ask_json(
                Method::PUT,
                &format!("/api/instances/{id}/profile"),
                Some(&cookie),
                body,
            )
            .await,
            cookie,
        )
    }

    /// Sign in and `PUT /api/instances/{id}/address`.
    async fn set_address(&self, id: &str, body: &str) -> (Reply, String) {
        let cookie = self.sign_in().await;
        (
            self.ask_json(
                Method::PUT,
                &format!("/api/instances/{id}/address"),
                Some(&cookie),
                body,
            )
            .await,
            cookie,
        )
    }

    /// Sign in and `POST /api/instances`.
    async fn create(&self, body: &str) -> (Reply, String) {
        let cookie = self.sign_in().await;
        (
            self.ask_json(Method::POST, "/api/instances", Some(&cookie), body)
                .await,
            cookie,
        )
    }
}

/// The routes, as the session sweeps walk them: method, path, and the body a
/// write sends (a read sends none).
const ROUTES: &[(&str, &str, Option<&str>)] = &[
    ("GET", "/api/instances", None),
    (
        "PUT",
        "/api/instances/default/profile",
        Some(r#"{"instanceProfile": "alpha"}"#),
    ),
    (
        "PUT",
        "/api/instances/default/address",
        Some(r#"{"hostname": "127.0.0.1", "port": "8581"}"#),
    ),
    (
        "POST",
        "/api/instances",
        Some(r#"{"engine": "llama.cpp", "id": "two", "instanceProfile": "alpha"}"#),
    ),
];

/// The writes, as the 403 sweep walks them: the rows read answers to the bff's
/// own account, so a refused user is its 502, not the sweep's 403.
const WRITES: &[(&str, &str, &str)] = &[
    (
        "PUT",
        "/api/instances/default/profile",
        r#"{"instanceProfile": "alpha"}"#,
    ),
    (
        "PUT",
        "/api/instances/default/address",
        r#"{"hostname": "127.0.0.1", "port": "8581"}"#,
    ),
    (
        "POST",
        "/api/instances",
        r#"{"engine": "llama.cpp", "id": "two", "instanceProfile": "alpha"}"#,
    ),
];

/// The router's answer to one of `ROUTES`, signed in when asked.
async fn ask_route(
    app: &App,
    method: &str,
    uri: &str,
    body: Option<&str>,
    cookie: Option<&str>,
) -> Reply {
    let method = Method::from_bytes(method.as_bytes()).expect("method");
    match body {
        Some(b) => app.ask_json(method, uri, cookie, b).await,
        None => app.get(uri, cookie).await,
    }
}

// ── Failure paths first: the session gate of every route ─────────────────

/// With no session, every route answers 401 `session-required` and the manager
/// is never even connected to: the gate answers on its own.
#[tokio::test]
async fn every_instance_route_needs_a_session() {
    let app = app_for(
        Fixture::default()
            .with_pool()
            .used(&[("default", Some("alpha"))]),
    );
    for (method, uri, body) in ROUTES {
        let reply = ask_route(&app, method, uri, *body, None).await;
        reply.assert_session_required();
        app.assert_asked_nothing(uri);
    }
}

/// The manager refuses the signed-in user: 403 `session-not-administrator` on
/// every write, and that session is over.
#[tokio::test]
async fn a_refused_user_is_403_and_ends_the_session_on_every_write() {
    let app = app_for(
        Fixture::default()
            .with_pool()
            .used(&[("default", Some("alpha"))])
            .refuses_writes(ClientError::NotAuthorized),
    );
    for (method, uri, body) in WRITES {
        let cookie = app.sign_in().await;
        let reply = ask_route(&app, method, uri, Some(body), Some(&cookie)).await;
        reply.assert_error(StatusCode::FORBIDDEN, "session-not-administrator", &[]);
        app.session_of(&cookie).await.assert_session_required();
    }
}

/// The rows read rides the bff's own account: a manager that refuses that
/// account is 502 `client-not-authorized` and the session lives on.
#[tokio::test]
async fn a_refused_own_account_on_the_rows_is_502_and_keeps_the_session() {
    let app = app_for(
        Fixture::default()
            .used(&[("default", Some("alpha"))])
            .rows_fails(ClientError::NotAuthorized),
    );
    let (reply, cookie) = app.rows().await;
    reply.assert_error(StatusCode::BAD_GATEWAY, "client-not-authorized", &[]);
    app.session_of(&cookie).await.assert_signed_in();
}

/// No manager to reach: the read and every write are 503 `client-unavailable`,
/// and the session lives on.
#[tokio::test]
async fn no_manager_is_503_on_the_read_and_on_every_write() {
    let app = app_no_manager();
    let cookie = app.sign_in().await;
    for (method, uri, body) in ROUTES {
        let reply = ask_route(&app, method, uri, *body, Some(&cookie)).await;
        reply.assert_error(StatusCode::SERVICE_UNAVAILABLE, "client-unavailable", &[]);
    }
    app.session_of(&cookie).await.assert_signed_in();
}

/// A write that was sent and outlived the write's limit is 504
/// `client-timeout`, the outcome unknown, and the session keeps living: on the
/// profile switch, on the address write, and on the create.
#[tokio::test]
async fn a_write_sent_past_its_limit_is_504_and_keeps_the_session() {
    let build = || {
        app_for_write_timeout(
            Fixture::default()
                .with_pool()
                .used(&[("default", Some("alpha"))])
                .write_delay(Duration::from_millis(400)),
            Duration::from_millis(20),
        )
    };
    for (method, uri, body) in WRITES {
        let app = build();
        let cookie = app.sign_in().await;
        let reply = ask_route(&app, method, uri, Some(body), Some(&cookie)).await;
        reply.assert_error(StatusCode::GATEWAY_TIMEOUT, "client-timeout", &[]);
        app.session_of(&cookie).await.assert_signed_in();
    }
}

// ── The rows ─────────────────────────────────────────────────────────────

/// Each row carries `instanceProfile`, `hostname` (`null` when the record sets
/// none), `converged`, `reason` (`{"key", "args"}` or `null`) and
/// `pendingRestart`, beside the keys it answers today: the three states are
/// the row converged, the row not converged with its keyed reason, and the row
/// that asks for a Restart.
#[tokio::test]
async fn the_rows_answer_the_new_keys_for_the_three_states() {
    let app = app_for(Fixture::default().with_rows(vec![
        full_row(
            "default",
            Some("alpha"),
            Some("127.0.0.1"),
            true,
            None,
            false,
        ),
        full_row("two", None, None, true, None, true),
        full_row(
            "three",
            Some("beta"),
            None,
            false,
            Some(NotConverged {
                key: "over-memory".to_string(),
                args: vec!["alpha1".to_string(), "2".to_string()],
            }),
            false,
        ),
    ]));
    let (reply, _) = app.rows().await;
    assert_eq!(reply.status, StatusCode::OK, "{}", reply.body);
    assert_eq!(
        reply.json(),
        json!([
            {
                "id": "default",
                "label": null,
                "engine": "llama.cpp",
                "serverProfile": "default",
                "state": "inactive",
                "port": 8080,
                "tags": [],
                "instanceTags": [],
                "boot": false,
                "removable": false,
                "instanceProfile": "alpha",
                "hostname": "127.0.0.1",
                "converged": true,
                "reason": null,
                "pendingRestart": false
            },
            {
                "id": "two",
                "label": null,
                "engine": "llama.cpp",
                "serverProfile": "default",
                "state": "inactive",
                "port": 8080,
                "tags": [],
                "instanceTags": [],
                "boot": false,
                "removable": true,
                "instanceProfile": null,
                "hostname": null,
                "converged": true,
                "reason": null,
                "pendingRestart": true
            },
            {
                "id": "three",
                "label": null,
                "engine": "llama.cpp",
                "serverProfile": "default",
                "state": "inactive",
                "port": 8080,
                "tags": [],
                "instanceTags": [],
                "boot": false,
                "removable": true,
                "instanceProfile": "beta",
                "hostname": null,
                "converged": false,
                "reason": { "key": "over-memory", "args": ["alpha1", "2"] },
                "pendingRestart": false
            }
        ]),
        "{}",
        reply.body
    );
}

// ── The instance's profile ───────────────────────────────────────────────

/// `PUT /api/instances/{id}/profile` `{"instanceProfile"}` calls the manager's
/// `SetInstanceProfile` once, as the signed-in user, handing the pool text it
/// read as the revision the manager compares (no file read), and answers the
/// affected answer exactly.
#[tokio::test]
async fn the_profile_switch_answers_the_affected_list() {
    let app = app_for(
        Fixture::default()
            .with_pool()
            .used(&[("default", Some("beta"))]),
    );
    let (reply, _) = app
        .set_profile("default", r#"{"instanceProfile": "alpha"}"#)
        .await;
    reply.assert_json(StatusCode::OK, &affected_json());
    assert_eq!(
        app.actor_of("set-profile default alpha"),
        Some("alice".to_string()),
        "the switch is one of the manager's As methods, asked as the signed-in user"
    );
    let (instance, profile, revision) = app.last_set_profile();
    assert_eq!((instance.as_str(), profile.as_str()), ("default", "alpha"));
    assert_eq!(
        revision,
        pool(),
        "the manager is handed the pool text the bff read (GetInstanceProfile's revision), \
         so its compare closes the window: no file read answers this text"
    );
}

/// A profile switch body nothing can be made of: 400, the manager not connected
/// to. The contract names no key, so any `<what>-malformed` is accepted.
#[tokio::test]
async fn a_profile_switch_body_nothing_can_be_made_of_is_400() {
    for body in [
        "not json",
        r#"[{"instanceProfile": "alpha"}]"#,
        r#"{"instanceProfile": 3}"#,
        r#"{"instanceProfile": null}"#,
        r#"{"serverProfile": "alpha"}"#,
    ] {
        let app = app_for(
            Fixture::default()
                .with_pool()
                .used(&[("default", Some("beta"))]),
        );
        let (reply, _) = app.set_profile("default", body).await;
        reply.assert_malformed();
        app.assert_asked_nothing("a refused profile body must not reach the manager");
    }
    let app = app_for(
        Fixture::default()
            .with_pool()
            .used(&[("default", Some("beta"))]),
    );
    let cookie = app.sign_in().await;
    app.ask_bytes(
        Method::PUT,
        "/api/instances/default/profile",
        Some(&cookie),
        "not json at all",
    )
    .await
    .assert_malformed();
    app.assert_asked_nothing("bytes no reader makes anything of must not reach the manager");
}

/// An id the record of the instances does not name: 404 `client-no-instance`,
/// and no write asked.
#[tokio::test]
async fn a_profile_switch_of_an_unknown_instance_is_404_with_no_write() {
    let app = app_for(
        Fixture::default()
            .with_pool()
            .used(&[("default", Some("beta"))]),
    );
    let (reply, _) = app
        .set_profile("ghost", r#"{"instanceProfile": "alpha"}"#)
        .await;
    reply.assert_error(StatusCode::NOT_FOUND, "client-no-instance", &[]);
    app.assert_no_call("set-profile", "an unknown instance writes nothing");
}

/// The manager's own 404 for a record that names no such instance answers the
/// same 404 `client-no-instance` with no args, the manager's detail staying out.
#[tokio::test]
async fn the_managers_no_instance_on_a_profile_switch_is_the_same_404() {
    let app = app_for(
        Fixture::default()
            .with_pool()
            .used(&[("default", Some("beta"))])
            .set_profile_fails(ClientError::NoInstance),
    );
    let (reply, _) = app
        .set_profile("ghost", r#"{"instanceProfile": "alpha"}"#)
        .await;
    reply.assert_error(StatusCode::NOT_FOUND, "client-no-instance", &[]);
}

/// A profile the pool has no section for: 422 `no-instance-profile-named`, the
/// key in a body (IP4's key table), with no write asked.
#[tokio::test]
async fn a_profile_the_pool_has_not_got_is_422_no_instance_profile_named() {
    let app = app_for(
        Fixture::default()
            .with_pool()
            .used(&[("default", Some("beta"))]),
    );
    let (reply, _) = app
        .set_profile("default", r#"{"instanceProfile": "gamma"}"#)
        .await;
    reply.assert_error(
        StatusCode::UNPROCESSABLE_ENTITY,
        "no-instance-profile-named",
        &["gamma"],
    );
    app.assert_no_call(
        "set-profile",
        "a profile the pool has not got writes nothing",
    );
}

/// The profile switch's conflicts and its other failure, as the page reads
/// them: 409 `address-in-use [instance]`, 409 `stale-revision` for the
/// manager's own compare of the text it was handed, 502 `client-other` with the
/// detail kept out.
#[tokio::test]
async fn the_profile_switch_answers_its_conflicts() {
    for (failure, status, key, args) in [
        (
            ClientError::AddressInUse("two".into()),
            StatusCode::CONFLICT,
            "address-in-use",
            &["two"][..],
        ),
        (
            ClientError::Changed,
            StatusCode::CONFLICT,
            "stale-revision",
            &[][..],
        ),
        (
            ClientError::Io("the write failed".into()),
            StatusCode::BAD_GATEWAY,
            "client-other",
            &[][..],
        ),
    ] {
        let app = app_for(
            Fixture::default()
                .with_pool()
                .used(&[("default", Some("beta"))])
                .set_profile_fails(failure.clone()),
        );
        let (reply, _) = app
            .set_profile("default", r#"{"instanceProfile": "alpha"}"#)
            .await;
        reply.assert_error(status, key, args);
    }
}

// ── The instance's address ───────────────────────────────────────────────

/// `PUT /api/instances/{id}/address` `{"hostname", "port"}` calls the manager's
/// `SetInstanceAddress` once, as the signed-in user, handing the `instances.ini`
/// text it read as the revision the manager compares, the two texts passed
/// through, and answers the affected answer exactly.
#[tokio::test]
async fn the_address_write_answers_the_affected_list() {
    let app = app_for(
        Fixture::default()
            .with_pool()
            .used(&[("default", Some("alpha"))]),
    );
    let (reply, _) = app
        .set_address("default", r#"{"hostname": "127.0.0.1", "port": "8582"}"#)
        .await;
    reply.assert_json(StatusCode::OK, &affected_json());
    assert_eq!(
        app.actor_of("set-address default 127.0.0.1 8582"),
        Some("alice".to_string()),
        "the address write is one of the manager's As methods, asked as the signed-in user"
    );
    let (instance, host, port, revision) = app.last_set_address();
    assert_eq!(
        (instance.as_str(), host.as_str(), port.as_str()),
        ("default", "127.0.0.1", "8582")
    );
    assert_eq!(
        revision,
        RECORD,
        "the manager is handed the instances.ini text the bff read, so its compare closes the window"
    );
}

/// A `null` hostname clears the host: the manager is handed the empty host, the
/// key's own remover.
#[tokio::test]
async fn a_null_hostname_clears_the_host() {
    let app = app_for(
        Fixture::default()
            .with_pool()
            .used(&[("default", Some("alpha"))]),
    );
    let (reply, _) = app
        .set_address("default", r#"{"hostname": null, "port": "8581"}"#)
        .await;
    reply.assert_json(StatusCode::OK, &affected_json());
    let (instance, host, port, _) = app.last_set_address();
    assert_eq!(
        (instance.as_str(), host.as_str(), port.as_str()),
        ("default", "", "8581"),
        "the empty host removes the key"
    );
}

/// An address body nothing can be made of: 400, the manager not connected to.
/// The two keys are the manager's strings, so a missing key, a `null` port and
/// a port that is not text are all refused.
#[tokio::test]
async fn an_address_body_nothing_can_be_made_of_is_400() {
    for body in [
        "not json",
        r#"[{"hostname": "h", "port": "1"}]"#,
        r#"{"hostname": "h"}"#,
        r#"{"port": "1"}"#,
        r#"{"hostname": null, "port": null}"#,
        r#"{"hostname": "h", "port": 8581}"#,
    ] {
        let app = app_for(
            Fixture::default()
                .with_pool()
                .used(&[("default", Some("alpha"))]),
        );
        let (reply, _) = app.set_address("default", body).await;
        reply.assert_malformed();
        app.assert_asked_nothing("a refused address body must not reach the manager");
    }
    let app = app_for(
        Fixture::default()
            .with_pool()
            .used(&[("default", Some("alpha"))]),
    );
    let cookie = app.sign_in().await;
    app.ask_bytes(
        Method::PUT,
        "/api/instances/default/address",
        Some(&cookie),
        "not json at all",
    )
    .await
    .assert_malformed();
    app.assert_asked_nothing("bytes no reader makes anything of must not reach the manager");
}

/// An id the record of the instances does not name: 404 `client-no-instance`,
/// and no write asked.
#[tokio::test]
async fn an_address_write_of_an_unknown_instance_is_404_with_no_write() {
    let app = app_for(
        Fixture::default()
            .with_pool()
            .used(&[("default", Some("alpha"))]),
    );
    let (reply, _) = app
        .set_address("ghost", r#"{"hostname": "127.0.0.1", "port": "8582"}"#)
        .await;
    reply.assert_error(StatusCode::NOT_FOUND, "client-no-instance", &[]);
    app.assert_no_call("set-address", "an unknown instance writes nothing");
}

/// The address write's refusals, as the page reads them: the manager's port
/// range and unresolvable host are the contract's 422 `invalid-port` and 422
/// `invalid-hostname` (the bff maps the manager's names, the args its own),
/// the taken address is 409 `address-in-use [instance]`, the manager's own
/// compare is 409 `stale-revision`, anything else 502 with the detail kept out.
#[tokio::test]
async fn the_address_write_answers_its_refusals() {
    #![allow(clippy::type_complexity)]
    for (failure, status, key, args) in [
        (
            refusal(
                "not-whole-number-in-range",
                &["Port", "99999", "1", "65535"],
            ),
            StatusCode::UNPROCESSABLE_ENTITY,
            "invalid-port",
            None,
        ),
        (
            ClientError::UnresolvableHost("nope.example".into()),
            StatusCode::UNPROCESSABLE_ENTITY,
            "invalid-hostname",
            None,
        ),
        (
            ClientError::AddressInUse("two".into()),
            StatusCode::CONFLICT,
            "address-in-use",
            Some(&["two"][..]),
        ),
        (
            ClientError::Changed,
            StatusCode::CONFLICT,
            "stale-revision",
            Some(&[][..]),
        ),
        (
            ClientError::Io("the write failed".into()),
            StatusCode::BAD_GATEWAY,
            "client-other",
            Some(&[][..]),
        ),
    ] {
        let app = app_for(
            Fixture::default()
                .with_pool()
                .used(&[("default", Some("alpha"))])
                .set_address_fails(failure.clone()),
        );
        let (reply, _) = app
            .set_address("default", r#"{"hostname": "127.0.0.1", "port": "8582"}"#)
            .await;
        match args {
            Some(args) => reply.assert_error(status, key, args),
            None => reply.assert_key(status, key),
        }
    }
}

// ── The create ───────────────────────────────────────────────────────────

/// `POST /api/instances` `{"engine", "id", "instanceProfile"}` calls the
/// manager's `CreateInstance` once, as the signed-in user, with the three words
/// of the body, and answers 201 `{"id", "port"}`: the port the manager picked
/// from 8581, so the page shows it with no second read.
#[tokio::test]
async fn the_create_answers_the_id_and_the_port_it_picked() {
    let app = app_for(
        Fixture::default()
            .with_pool()
            .used(&[("default", Some("alpha"))]),
    );
    let (reply, _) = app
        .create(r#"{"engine": "llama.cpp", "id": "two", "instanceProfile": "alpha"}"#)
        .await;
    reply.assert_json(StatusCode::CREATED, &json!({ "id": "two", "port": 8581 }));
    assert_eq!(
        app.actor_of("create llama.cpp two alpha"),
        Some("alice".to_string()),
        "the create is one of the manager's As methods, asked as the signed-in user"
    );
    assert_eq!(
        app.last_create(),
        (
            "llama.cpp".to_string(),
            "two".to_string(),
            "alpha".to_string()
        ),
        "one call, the body's three words"
    );
}

/// A create body nothing can be made of: 400 `instance-malformed`, the manager
/// not connected to. The body names the instance profile, not a server
/// profile: the served create's body is not this body.
#[tokio::test]
async fn a_create_body_nothing_can_be_made_of_is_400() {
    for body in [
        "not json",
        r#"[{"engine": "llama.cpp", "id": "two", "instanceProfile": "alpha"}]"#,
        r#"{"engine": "llama.cpp", "id": "two"}"#,
        r#"{"engine": "llama.cpp", "instanceProfile": "alpha"}"#,
        r#"{"id": "two", "instanceProfile": "alpha"}"#,
        r#"{"engine": "llama.cpp", "id": "two", "instanceProfile": 3}"#,
        r#"{"engine": "llama.cpp", "id": "two", "serverProfile": "default"}"#,
    ] {
        let app = app_for(Fixture::default().with_pool());
        let (reply, _) = app.create(body).await;
        reply.assert_malformed();
        app.assert_asked_nothing("a refused create body must not reach the manager");
    }
    let app = app_for(Fixture::default().with_pool());
    let cookie = app.sign_in().await;
    app.ask_bytes(
        Method::POST,
        "/api/instances",
        Some(&cookie),
        "not json at all",
    )
    .await
    .assert_malformed();
    app.assert_asked_nothing("bytes no reader makes anything of must not reach the manager");
}

/// The retired create named a server profile, so a body that still carries
/// `serverProfile` is refused, never read as an instance profile: not on its
/// own, and not beside the key that supersedes it.
#[tokio::test]
async fn a_create_body_that_still_names_a_server_profile_is_400() {
    for body in [
        r#"{"engine": "llama.cpp", "id": "two", "serverProfile": "default"}"#,
        r#"{"engine": "llama.cpp", "id": "two", "instanceProfile": "alpha", "serverProfile": "default"}"#,
    ] {
        let app = app_for(Fixture::default().with_pool());
        let (reply, _) = app.create(body).await;
        reply.assert_malformed();
        app.assert_asked_nothing("a body that names a server profile must reach no manager");
    }
}

/// The create's refusals, as the page reads them: a record that already has the
/// id is 409 `instance-exists [id]`, a profile the pool has not got is 422
/// `no-instance-profile-named` (the key in a body), and a node with every port
/// taken is 503 `no-free-port` naming the range's two ends.
#[tokio::test]
async fn the_create_answers_its_refusals() {
    for (failure, status, key, args) in [
        (
            refusal("instance-exists", &["two"]),
            StatusCode::CONFLICT,
            "instance-exists",
            &["two"][..],
        ),
        (
            ClientError::NoInstanceProfile("alpha".into()),
            StatusCode::UNPROCESSABLE_ENTITY,
            "no-instance-profile-named",
            &["alpha"][..],
        ),
        (
            refusal("no-free-port", &["8581", "8582"]),
            StatusCode::SERVICE_UNAVAILABLE,
            "no-free-port",
            &["8581", "8582"][..],
        ),
    ] {
        let app = app_for(Fixture::default().with_pool().create_fails(failure.clone()));
        let (reply, _) = app
            .create(r#"{"engine": "llama.cpp", "id": "two", "instanceProfile": "alpha"}"#)
            .await;
        reply.assert_error(status, key, args);
    }
}

/// The manager's detail is not the page's: a create failure that is no refusal
/// is 502 `client-other` with no args and no word of the detail.
#[tokio::test]
async fn a_create_failure_that_is_not_a_refusal_keeps_the_detail_out() {
    let app = app_for(
        Fixture::default()
            .with_pool()
            .create_fails(ClientError::Other(
                "org.freedesktop.DBus.Error.AccessDenied: secret-detail".into(),
            )),
    );
    let (reply, _) = app
        .create(r#"{"engine": "llama.cpp", "id": "two", "instanceProfile": "alpha"}"#)
        .await;
    reply.assert_error(StatusCode::BAD_GATEWAY, "client-other", &[]);
    assert!(!reply.body.contains("secret-detail"), "{}", reply.body);
}
