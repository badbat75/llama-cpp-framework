//! One instance's profiles, through `pai_bff::router_with_session` with a fake
//! manager client and a fake authenticator, never the real bus, pipe, PAM or
//! LogonUser: `PUT /api/instances/{id}/profiles/server`, and
//! `GET`/`PUT /api/instances/{id}/profiles/model`, and
//! `PUT /api/instances/{id}/profiles/model/{profile}`. Every request runs under
//! a deadline, so a bff that waits on the manager fails instead of hanging.
//!
//! The contract (ARCHITECTURE.md's slice 15, and fe/briefs/api-contract.md for
//! the shapes):
//! - Every route sits behind a session: with none, the answer is 401
//!   `{"key": "session-required", "args": []}` and the manager is never even
//!   connected to. `NotAuthorized` is 403 `session-not-administrator` and ends
//!   that session. No manager is 503 `client-unavailable`; anything else is 502
//!   `client-other`.
//! - The writes, and the reads of `assignments.ini` and of the model profile
//!   pool, go as the signed-in user; `Instances()` and `Status` are asked by the
//!   bff's own account. The fake records the principal of every connect, so a
//!   bff that writes as itself fails a test.
//! - `PUT /api/instances/{id}/profiles/server` `{"serverProfile": string}`
//!   answers 200 `{"running"}`: `running` is true when the instance named
//!   another profile (or that read failed) and `Status` after the write is
//!   `active`/`activating`, or `Status` failed, and false when the instance is
//!   stopped or already on that profile. A body it cannot read is 400
//!   `server-profile-malformed` with the manager not asked; the manager's
//!   refusals are 404 `client-no-instance`, 404 `client-no-server-profile`
//!   `[profile]`, 409 `client-server-profile-in-use` `[profile, instance]`, and
//!   422 `client-address-in-use`, `client-unresolvable-host` and
//!   `load-on-startup-over-max` with `field` `serverProfile`.
//! - `GET /api/instances/{id}/profiles/model` answers 200 `{"all", "profiles"}`:
//!   every row of the pool's list (`id`, `engine`, `engineAvailable`, `enabled`,
//!   `model`) plus `value`, `viaAll` and `effective` as `assign::rows` gives
//!   them, in the list's order; an id the record does not name is 404
//!   `client-no-instance` and the files are not read.
//! - `PUT /api/instances/{id}/profiles/model/{profile}` `{"value"}` answers 200
//!   `{"effective", "running"}`: `effective` is the after-read's, `running` is
//!   true only when `effective` changed and the instance runs; an after-read
//!   that fails leaves the write standing (200, `effective` from the
//!   before-read and the value, `running` true). The path's `*` is 404
//!   `no-model-profile-named` and never reaches the manager; a value that is
//!   not one of the four words is 422 `model-profile-bad-value` `[value]` with
//!   `field` `value`, a body it cannot read or a non-string value 400
//!   `assignment-malformed`, both before any call. No revision, never 409.
//! - `PUT /api/instances/{id}/profiles/model` `{"all": "inherit"|"none"}`
//!   answers 200 `{"running"}` and is the manager's `SetModelProfile` with the
//!   profile `*`; another string is 422 `model-profile-bad-value` with `field`
//!   `all`.

use axum::body::Body;
use axum::extract::ConnectInfo;
use axum::http::{header, Method, Request, StatusCode};
use axum::Router;
use http_body_util::BodyExt;
use pai_bff::session::{AuthError, Authenticator, Identity};
use pai_bff::{Client, Connect};
use pai_core::checks::Refusal;
use pai_core::client::{ClientError, InstanceInfo, InstanceState, ManagerClient, ManagerFile};
use pai_core::ini;
use serde_json::{json, Value};
use std::collections::HashMap;
use std::net::SocketAddr;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};
use tower::ServiceExt;

/// How long the bff waits for the manager in these tests.
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

/// The engine's file of switched-on model profiles.
const MODELS: &str = "models.ini";

/// The engine's file of switched-off model profiles.
const MODELS_DISABLED: &str = "models.ini.disabled";

/// The engine's file of per-instance assignments.
const ASSIGNMENTS: &str = "assignments.ini";

type Log = Arc<Mutex<Vec<String>>>;

/// The principal each connect was asked for.
type Principals = Arc<Mutex<Vec<Option<String>>>>;

/// The text of each served file, shared with the fake so a write it lands is
/// visible to the next read, as it is on a machine.
type Files = Arc<Mutex<HashMap<String, String>>>;

/// The lines as a file's text, in the ending style the pool files use.
fn text_of(lines: &[&str]) -> String {
    lines.join("\r\n") + "\r\n"
}

/// `models.ini`: `alpha` and `beta`, switched on.
fn models() -> String {
    text_of(&[
        "[alpha]",
        "model = /models/alpha.gguf",
        "",
        "[beta]",
        "model = /models/beta.gguf",
    ])
}

/// `models.ini.disabled`: `gamma`, switched off.
fn models_disabled() -> String {
    text_of(&["[gamma]", "model = /models/gamma.gguf"])
}

/// `assignments.ini` with one section per `(instance, keys)` entry, `keys` the
/// lines of the section.
fn assignments(entries: &[(&str, &[&str])]) -> String {
    let mut out = String::new();
    for (id, keys) in entries {
        out.push_str(&format!("[{id}]\r\n"));
        for line in keys.iter() {
            out.push_str(line);
            out.push_str("\r\n");
        }
        out.push_str("\r\n");
    }
    out
}

/// One row of `instances`: the instance `id`, on the server profile `profile`.
fn row(id: &str, profile: &str, port: u16) -> InstanceInfo {
    InstanceInfo {
        id: id.to_string(),
        engine: ENGINE.to_string(),
        server_profile: Some(profile.to_string()),
        port,
        profile_tags: Vec::new(),
        instance_tags: Vec::new(),
        label: None,
        converged: true,
        reason: None,
        pending_restart: false,
    }
}

/// What the fake manager answers. A manager with the three files, the record of
/// the instances, and the state of each.
struct Fixture {
    /// The text of each served file.
    files: Files,
    /// What every `read_file` of that name answers, ahead of `files`.
    read_fails: HashMap<String, ClientError>,
    /// Whether a read of `assignments.ini` fails once a write landed: the
    /// after-read that cannot be made, with the write standing.
    read_after_write_fails: bool,
    /// The text `assignments.ini` answers once a write landed: the record as the
    /// manager left it, which is not what the bff computed (the manager's own
    /// rule, and another tab's write landing between).
    after_read: Option<String>,
    /// How long every write call sleeps: the write that outlasts the read's
    /// limit, and the one that outlasts the write's own.
    write_delay: Duration,
    /// What `instances` answers: the record's rows.
    rows: Result<Vec<InstanceInfo>, ClientError>,
    /// The state each instance answers; `Inactive` for one it does not name.
    states: HashMap<String, InstanceState>,
    /// What every `status` answers, ahead of `states`: the state read that fails.
    status_fail: Option<ClientError>,
    /// What `set_server_profile` answers.
    set_server: Result<(), ClientError>,
    /// What `set_model_profile` answers.
    set_model: Result<(), ClientError>,
    /// Set once a write landed, shared with every fake of the test.
    wrote: Arc<AtomicBool>,
}

impl Default for Fixture {
    fn default() -> Fixture {
        Fixture {
            files: Files::default(),
            read_fails: HashMap::new(),
            read_after_write_fails: false,
            after_read: None,
            write_delay: Duration::ZERO,
            rows: Ok(Vec::new()),
            states: HashMap::new(),
            status_fail: None,
            set_server: Ok(()),
            set_model: Ok(()),
            wrote: Arc::new(AtomicBool::new(false)),
        }
    }
}

impl Fixture {
    /// The pool of `models()` and `models_disabled()`, no assignments file.
    fn pool() -> Fixture {
        let f = Fixture::default();
        let mut files = f.files.lock().unwrap_or_else(|e| e.into_inner());
        files.insert(MODELS.to_string(), models());
        files.insert(MODELS_DISABLED.to_string(), models_disabled());
        drop(files);
        f
    }

    /// `assignments.ini` holds `text`.
    fn with_assignments(self, text: &str) -> Fixture {
        self.files
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .insert(ASSIGNMENTS.to_string(), text.to_string());
        self
    }

    /// The record gains one section per `(instance, profile)` entry.
    fn used(mut self, entries: &[(&str, &str)]) -> Fixture {
        let mut out: Vec<InstanceInfo> = Vec::new();
        for (id, profile) in entries {
            out.push(row(id, profile, 8080));
        }
        self.rows = Ok(out);
        self
    }

    /// The state of `id` is `state`.
    fn state(mut self, id: &str, state: InstanceState) -> Fixture {
        self.states.insert(id.to_string(), state);
        self
    }

    /// The record answers `rows`.
    fn rows(mut self, rows: Result<Vec<InstanceInfo>, ClientError>) -> Fixture {
        self.rows = rows;
        self
    }

    /// Every `status` answers `fail`.
    fn status_fail(mut self, fail: ClientError) -> Fixture {
        self.status_fail = Some(fail);
        self
    }

    /// `read_file` of `name` answers `fail`.
    fn read_refuses(mut self, name: &str, fail: ClientError) -> Fixture {
        self.read_fails.insert(name.to_string(), fail);
        self
    }

    /// A read of `assignments.ini` fails once a write landed.
    fn after_read_fails(mut self) -> Fixture {
        self.read_after_write_fails = true;
        self
    }

    /// A read of `assignments.ini` answers `text` once a write landed: the
    /// record as the manager left it, not as the bff computed it.
    fn after_read(mut self, text: &str) -> Fixture {
        self.after_read = Some(text.to_string());
        self
    }

    /// Every write call sleeps `delay` before it answers.
    fn write_delay(mut self, delay: Duration) -> Fixture {
        self.write_delay = delay;
        self
    }

    /// `set_server_profile` answers `fail`.
    fn set_server_refuses(mut self, fail: ClientError) -> Fixture {
        self.set_server = Err(fail);
        self
    }

    /// `set_model_profile` answers `fail`.
    fn set_model_refuses(mut self, fail: ClientError) -> Fixture {
        self.set_model = Err(fail);
        self
    }
}

/// A manager that answers from a [`Fixture`] and logs every call (`instances`,
/// `status <id>`, `read <name>`, `set-server <id> <profile>`,
/// `set-model <id> <profile> <value>`).
struct FakeManager {
    fixture: Fixture,
    log: Log,
}

impl FakeManager {
    fn new(log: &Log, fixture: &Fixture) -> FakeManager {
        FakeManager {
            fixture: Fixture {
                files: fixture.files.clone(),
                read_fails: fixture.read_fails.clone(),
                read_after_write_fails: fixture.read_after_write_fails,
                after_read: fixture.after_read.clone(),
                write_delay: fixture.write_delay,
                rows: fixture.rows.clone(),
                states: fixture.states.clone(),
                status_fail: fixture.status_fail.clone(),
                set_server: fixture.set_server.clone(),
                set_model: fixture.set_model.clone(),
                wrote: fixture.wrote.clone(),
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

    fn files(&self) -> std::sync::MutexGuard<'_, HashMap<String, String>> {
        self.fixture.files.lock().unwrap_or_else(|e| e.into_inner())
    }

    /// The write landed: the key is set in the instance's section of
    /// `assignments.ini`, as the manager does, so the after-read sees it.
    fn apply(&self, instance: &str, profile: &str, value: &str) {
        let mut files = self.files();
        let held = files
            .get(ASSIGNMENTS)
            .cloned()
            .unwrap_or_else(|| format!("[{instance}]\r\n"));
        let next = if value == "none" {
            ini::text::remove_key(&held, instance, profile)
        } else {
            ini::text::set_key(&held, instance, profile, value)
        };
        files.insert(ASSIGNMENTS.to_string(), next);
        self.fixture.wrote.store(true, Ordering::SeqCst);
    }
}

impl ManagerClient for FakeManager {
    fn list_instances(&self) -> Result<Vec<String>, ClientError> {
        self.record("list".into());
        let rows = self.fixture.rows.clone().unwrap_or_default();
        Ok(rows.iter().map(|r| r.id.clone()).collect())
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
        if let Some(fail) = self.fixture.read_fails.get(name) {
            return Err(fail.clone());
        }
        if self.fixture.read_after_write_fails
            && name == ASSIGNMENTS
            && self.fixture.wrote.load(Ordering::SeqCst)
        {
            return Err(ClientError::Io("the file went away".into()));
        }
        if let Some(text) = self.fixture.after_read.clone() {
            if name == ASSIGNMENTS && self.fixture.wrote.load(Ordering::SeqCst) {
                return Ok(Some(ManagerFile {
                    text,
                    withheld: false,
                }));
            }
        }
        Ok(self.files().get(name).map(|text| ManagerFile {
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
        self.files().insert(name.to_string(), contents.to_string());
        Ok(())
    }
    fn write_file_if(
        &self,
        name: &str,
        contents: &str,
        _expected: &str,
    ) -> Result<(), ClientError> {
        self.record(format!("write-if {name}"));
        self.files().insert(name.to_string(), contents.to_string());
        Ok(())
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
    fn server_profiles(&self) -> Result<Vec<pai_core::client::ServerProfileInfo>, ClientError> {
        self.record("profiles".into());
        Ok(Vec::new())
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
        Ok(())
    }
    fn delete_server_profile(&self, engine: &str, profile: &str) -> Result<(), ClientError> {
        self.record(format!("delete {engine} {profile}"));
        Ok(())
    }
    fn set_server_profile(&self, instance: &str, profile: &str) -> Result<(), ClientError> {
        self.record(format!("set-server {instance} {profile}"));
        std::thread::sleep(self.fixture.write_delay);
        self.fixture.set_server.clone()
    }
    fn set_model_profile(
        &self,
        instance: &str,
        profile: &str,
        value: &str,
    ) -> Result<(), ClientError> {
        self.record(format!("set-model {instance} {profile} {value}"));
        std::thread::sleep(self.fixture.write_delay);
        let answer = self.fixture.set_model.clone();
        if answer.is_ok() {
            self.apply(instance, profile, value);
        }
        answer
    }
}

fn principals_of(p: &Principals) -> Vec<Option<String>> {
    p.lock().unwrap_or_else(|e| e.into_inner()).clone()
}

/// A connect that counts the calls and records the principal each was asked
/// for, handing out `make(principal)` every time.
fn connect_logging(
    count: &Arc<AtomicUsize>,
    principals: &Principals,
    make: impl Fn(Option<&str>) -> Result<Client, ClientError> + Send + Sync + 'static,
) -> Connect {
    let (count, principals) = (count.clone(), principals.clone());
    Arc::new(move |principal: Option<&str>| {
        count.fetch_add(1, Ordering::SeqCst);
        principals
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .push(principal.map(str::to_string));
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

    /// `GET /api/session` for a session that is still signed in.
    fn assert_signed_in_as(&self, user: &str) {
        assert_eq!(self.status, StatusCode::OK, "signed in: {}", self.body);
        assert_eq!(self.json(), json!({ "user": user }), "{}", self.body);
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
    app_for_write_timeout(fixture, TIMEOUT)
}

/// The app whose manager answers from `fixture`, with the manager's calls of a
/// WRITE bound by `write_timeout` and the reads left at `TIMEOUT`: the seam a
/// test of the write's own limit uses (`router_with_session_and_write_timeout`).
fn app_for_write_timeout(fixture: Fixture, write_timeout: Duration) -> App {
    app_connecting_timeout(
        move |principal, log| {
            log.lock()
                .unwrap_or_else(|e| e.into_inner())
                .push(format!("as {}", principal.unwrap_or("-")));
            Ok(Box::new(FakeManager::new(log, &fixture)) as Client)
        },
        write_timeout,
    )
}

/// The app whose connect hands out what `make` answers, the write bound by
/// `TIMEOUT` like the read.
fn app_connecting(
    make: impl Fn(Option<&str>, &Log) -> Result<Client, ClientError> + Send + Sync + 'static,
) -> App {
    app_connecting_timeout(make, TIMEOUT)
}

/// The app whose connect hands out what `make` answers. The log is the one the
/// connect lines and the calls share.
fn app_connecting_timeout(
    make: impl Fn(Option<&str>, &Log) -> Result<Client, ClientError> + Send + Sync + 'static,
    write_timeout: Duration,
) -> App {
    let log = Log::default();
    let connects = Arc::new(AtomicUsize::new(0));
    let principals = Principals::default();
    let l = log.clone();
    let connect = connect_logging(&connects, &principals, move |p| make(p, &l));
    let clock = FakeClock::new();
    let router = pai_bff::router_with_session_and_write_timeout(
        connect,
        TIMEOUT,
        write_timeout,
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

    /// The principal of the connect that made the calls starting with `prefix`:
    /// `None` for the bff's own account. The log is read as segments opened by
    /// an `as <principal>` line.
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

    /// The principal of the one call equal to `call` (the whole recorded line).
    /// Where a prefix is a substring of a sibling call (the read of
    /// `models.ini` and of `models.ini.disabled`), the exact call names which
    /// file's read is pinned.
    fn actor_of_exact(&self, call: &str) -> Option<String> {
        let log = self.calls();
        let mut actor: Option<String> = None;
        let mut found: Vec<Option<String>> = Vec::new();
        for line in &log {
            if let Some(rest) = line.strip_prefix("as ") {
                actor = (rest != "-").then(|| rest.to_string());
            } else if line == call {
                found.push(actor.clone());
            }
        }
        assert!(!found.is_empty(), "no call {call:?}: {log:?}");
        assert_eq!(found.len(), 1, "more than one call {call:?}: {log:?}");
        found.into_iter().next().expect("one call")
    }

    /// The principal of EVERY call equal to `call`, in the order the manager was
    /// asked, and `times` says how many the route owes. A write route reads the
    /// record and reads it back for the answer, so the same line is asked twice;
    /// `actor_of` and `actor_of_exact` pin a call asked once, and this pins each
    /// ask of a call asked more than once.
    fn actors_of_exact(&self, call: &str, times: usize) -> Vec<Option<String>> {
        let log = self.calls();
        let mut actor: Option<String> = None;
        let mut found: Vec<Option<String>> = Vec::new();
        for line in &log {
            if let Some(rest) = line.strip_prefix("as ") {
                actor = (rest != "-").then(|| rest.to_string());
            } else if line == call {
                found.push(actor.clone());
            }
        }
        assert_eq!(
            found.len(),
            times,
            "{call:?} was asked {} times, the route owes {times}: {log:?}",
            found.len()
        );
        found
    }

    /// The verb was never asked of the manager.
    fn assert_no_call(&self, prefix: &str, what: &str) {
        let calls = self.calls();
        assert!(
            !calls.iter().any(|c| c.starts_with(prefix)),
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

    // ── The four requests, each signed in first ──────────────────────────

    /// Sign in and `PUT /api/instances/{id}/profiles/server` with `body`.
    async fn set_server_profile(&self, id: &str, body: Value) -> (Reply, String) {
        let cookie = self.sign_in().await;
        let reply = self
            .ask_json(
                Method::PUT,
                &format!("/api/instances/{id}/profiles/server"),
                Some(&cookie),
                &body.to_string(),
            )
            .await;
        (reply, cookie)
    }

    /// Sign in and `GET /api/instances/{id}/profiles/model`.
    async fn model_rows(&self, id: &str) -> (Reply, String) {
        let cookie = self.sign_in().await;
        (
            self.get(
                &format!("/api/instances/{id}/profiles/model"),
                Some(&cookie),
            )
            .await,
            cookie,
        )
    }

    /// Sign in and `PUT /api/instances/{id}/profiles/model/{profile}`.
    async fn set_assignment(&self, id: &str, profile: &str, body: Value) -> (Reply, String) {
        let cookie = self.sign_in().await;
        let reply = self
            .ask_json(
                Method::PUT,
                &format!("/api/instances/{id}/profiles/model/{profile}"),
                Some(&cookie),
                &body.to_string(),
            )
            .await;
        (reply, cookie)
    }

    /// Sign in and `PUT /api/instances/{id}/profiles/model`.
    async fn set_all(&self, id: &str, body: Value) -> (Reply, String) {
        let cookie = self.sign_in().await;
        let reply = self
            .ask_json(
                Method::PUT,
                &format!("/api/instances/{id}/profiles/model"),
                Some(&cookie),
                &body.to_string(),
            )
            .await;
        (reply, cookie)
    }
}

/// The URL-encoded `*`, the path segment a page sends for the whole pool.
const STAR: &str = "%2A";

// ── Failure paths first: the session gate of every route ─────────────────

/// With no session, every route answers 401 `session-required` and the manager
/// is never even connected to: the gate answers on its own.
#[tokio::test]
async fn every_instance_profile_route_needs_a_session() {
    let app = app_for(Fixture::pool().used(&[("default", "default")]));
    for (method, uri, body) in [
        (
            Method::PUT,
            "/api/instances/default/profiles/server",
            Some(r#"{"serverProfile": "big"}"#),
        ),
        (Method::GET, "/api/instances/default/profiles/model", None),
        (
            Method::PUT,
            "/api/instances/default/profiles/model/alpha",
            Some(r#"{"value": "on"}"#),
        ),
        (
            Method::PUT,
            "/api/instances/default/profiles/model",
            Some(r#"{"all": "inherit"}"#),
        ),
    ] {
        let reply = match body {
            Some(b) => app.ask_json(method.clone(), uri, None, b).await,
            None => app.get(uri, None).await,
        };
        reply.assert_session_required();
        app.assert_asked_nothing(uri);
    }
}

/// The manager refuses the signed-in user: 403 `session-not-administrator` on
/// every route, and the session is over, on a read and on a write alike.
#[tokio::test]
async fn a_refusal_of_the_signed_in_user_ends_the_session() {
    let refuse = ClientError::NotAuthorized;
    for (method, uri, body, fixture) in [
        (
            Method::PUT,
            "/api/instances/default/profiles/server",
            Some(r#"{"serverProfile": "big"}"#),
            Fixture::default()
                .used(&[("default", "default")])
                .set_server_refuses(refuse.clone()),
        ),
        (
            Method::GET,
            "/api/instances/default/profiles/model",
            None,
            Fixture::pool()
                .used(&[("default", "default")])
                .read_refuses(MODELS, refuse.clone()),
        ),
        (
            Method::PUT,
            "/api/instances/default/profiles/model/alpha",
            Some(r#"{"value": "on"}"#),
            Fixture::pool()
                .used(&[("default", "default")])
                .read_refuses(MODELS, refuse.clone()),
        ),
        (
            Method::PUT,
            "/api/instances/default/profiles/model",
            Some(r#"{"all": "inherit"}"#),
            Fixture::pool()
                .used(&[("default", "default")])
                .read_refuses(MODELS, refuse.clone()),
        ),
    ] {
        let app = app_for(fixture);
        let cookie = app.sign_in().await;
        let reply = match body {
            Some(b) => app.ask_json(method.clone(), uri, Some(&cookie), b).await,
            None => app.get(uri, Some(&cookie)).await,
        };
        reply.assert_error(StatusCode::FORBIDDEN, "session-not-administrator", &[]);
        app.session_of(&cookie).await.assert_session_required();
    }
}

/// There is no manager: 503 `client-unavailable` on every route.
#[tokio::test]
async fn no_manager_is_503_on_every_route() {
    let app = app_connecting(|_, _| Err(ClientError::Unavailable));
    let cookie = app.sign_in().await;
    for (uri, method, body) in [
        (
            "/api/instances/default/profiles/server",
            Method::PUT,
            Some(r#"{"serverProfile": "big"}"#),
        ),
        ("/api/instances/default/profiles/model", Method::GET, None),
        (
            "/api/instances/default/profiles/model/alpha",
            Method::PUT,
            Some(r#"{"value": "on"}"#),
        ),
        (
            "/api/instances/default/profiles/model",
            Method::PUT,
            Some(r#"{"all": "inherit"}"#),
        ),
    ] {
        let reply = match body {
            Some(b) => app.ask_json(method.clone(), uri, Some(&cookie), b).await,
            None => app.get(uri, Some(&cookie)).await,
        };
        reply.assert_error(StatusCode::SERVICE_UNAVAILABLE, "client-unavailable", &[]);
    }
}

/// Every one of these routes moves the session's idle deadline, as the pool's
/// do: a request that runs past the deadline is 401, one just under it is not.
#[tokio::test]
async fn a_route_read_moves_the_sessions_idle_deadline() {
    const SHORT: Duration = Duration::from_secs(30 * 60 - 60);
    let app = app_for(
        Fixture::pool()
            .used(&[("default", "default")])
            .with_assignments(&assignments(&[("default", &["* = inherit"])])),
    );
    let cookie = app.sign_in().await;
    app.clock.advance(SHORT);
    let reply = app
        .get("/api/instances/default/profiles/model", Some(&cookie))
        .await;
    assert_eq!(
        reply.status,
        StatusCode::OK,
        "the read past the old deadline is not answered: {}",
        reply.body
    );
    assert!(
        reply.json().get("profiles").is_some(),
        "the read is not the rows: {}",
        reply.body
    );
}

// ── The server profile switch ────────────────────────────────────────────

/// A running instance switched to another server profile: 200 `{"running":
/// true}`, and the write is the manager's `SetServerProfile`.
#[tokio::test]
async fn the_server_switch_answers_running_true_for_a_running_instance() {
    let app = app_for(
        Fixture::default()
            .used(&[("default", "small")])
            .state("default", InstanceState::Active),
    );
    let (reply, _) = app
        .set_server_profile("default", json!({"serverProfile": "big"}))
        .await;
    assert_eq!(reply.status, StatusCode::OK, "{}", reply.body);
    assert_eq!(reply.json(), json!({"running": true}), "{}", reply.body);
    assert!(
        app.calls().iter().any(|c| c == "set-server default big"),
        "{:?}",
        app.calls()
    );
}

/// A stopped instance: the switch stands, and `running` is false.
#[tokio::test]
async fn the_server_switch_answers_running_false_for_a_stopped_instance() {
    let app = app_for(Fixture::default().used(&[("default", "small")]));
    let (reply, _) = app
        .set_server_profile("default", json!({"serverProfile": "big"}))
        .await;
    assert_eq!(reply.status, StatusCode::OK, "{}", reply.body);
    assert_eq!(reply.json(), json!({"running": false}), "{}", reply.body);
}

/// The instance is already on that profile: `running` is false, whatever its
/// state.
#[tokio::test]
async fn the_server_switch_answers_running_false_when_already_on_it() {
    let app = app_for(
        Fixture::default()
            .used(&[("default", "big")])
            .state("default", InstanceState::Active),
    );
    let (reply, _) = app
        .set_server_profile("default", json!({"serverProfile": "big"}))
        .await;
    assert_eq!(reply.status, StatusCode::OK, "{}", reply.body);
    assert_eq!(reply.json(), json!({"running": false}), "{}", reply.body);
}

/// A switch that starts, and `Activating` is running too.
#[tokio::test]
async fn the_server_switch_answers_running_true_for_an_activating_instance() {
    let app = app_for(
        Fixture::default()
            .used(&[("default", "small")])
            .state("default", InstanceState::Activating),
    );
    let (reply, _) = app
        .set_server_profile("default", json!({"serverProfile": "big"}))
        .await;
    assert_eq!(reply.json(), json!({"running": true}), "{}", reply.body);
}

/// The state read fails after the switch: the safe answer, `running` true, so
/// the page never hides that a restart may be needed.
#[tokio::test]
async fn the_server_switch_answers_running_true_when_the_state_read_fails() {
    let app = app_for(
        Fixture::default()
            .used(&[("default", "small")])
            .status_fail(ClientError::Io("the record is gone".into())),
    );
    let (reply, _) = app
        .set_server_profile("default", json!({"serverProfile": "big"}))
        .await;
    assert_eq!(reply.status, StatusCode::OK, "{}", reply.body);
    assert_eq!(reply.json(), json!({"running": true}), "{}", reply.body);
}

/// The record read before the write fails, so the bff cannot know what the
/// instance ran: the safe answer, `running` true.
#[tokio::test]
async fn the_server_switch_answers_running_true_when_the_record_read_fails() {
    let app = app_for(
        Fixture::default()
            .rows(Err(ClientError::Io("the record is gone".into())))
            .state("default", InstanceState::Active),
    );
    let (reply, _) = app
        .set_server_profile("default", json!({"serverProfile": "big"}))
        .await;
    assert_eq!(reply.status, StatusCode::OK, "{}", reply.body);
    assert_eq!(reply.json(), json!({"running": true}), "{}", reply.body);
}

/// The switch goes as the signed-in user; the record and the state are the
/// bff's own account.
#[tokio::test]
async fn the_server_switch_asks_as_the_signed_in_user() {
    let app = app_for(
        Fixture::default()
            .used(&[("default", "small")])
            .state("default", InstanceState::Active),
    );
    let (reply, _) = app
        .set_server_profile("default", json!({"serverProfile": "big"}))
        .await;
    assert_eq!(reply.status, StatusCode::OK, "{}", reply.body);
    assert_eq!(app.actor_of("set-server "), Some("alice".to_string()));
    assert_eq!(app.actor_of("instances"), None);
    assert_eq!(app.actor_of("status default"), None);
}

/// A body the bff cannot read is 400 `server-profile-malformed`, with the
/// manager not asked at all.
#[tokio::test]
async fn a_server_switch_body_that_is_not_a_string_is_400() {
    for body in [
        json!({}),
        json!({"serverProfile": null}),
        json!({"serverProfile": 8123}),
        json!({"serverProfile": ["big"]}),
        json!({"server": "big"}),
    ] {
        let app = app_for(Fixture::default().used(&[("default", "small")]));
        let (reply, _) = app.set_server_profile("default", body.clone()).await;
        reply.assert_error(StatusCode::BAD_REQUEST, "server-profile-malformed", &[]);
        app.assert_no_call("set-server", &format!("{body}"));
    }
    let app = app_for(Fixture::default().used(&[("default", "small")]));
    let cookie = app.sign_in().await;
    let reply = app
        .ask_json(
            Method::PUT,
            "/api/instances/default/profiles/server",
            Some(&cookie),
            "not json at all",
        )
        .await;
    reply.assert_error(StatusCode::BAD_REQUEST, "server-profile-malformed", &[]);
    app.assert_no_call("set-server", "text body");
}

/// No such instance: 404 `client-no-instance`, and the write is never asked.
#[tokio::test]
async fn a_server_switch_for_an_unknown_instance_is_404() {
    let app = app_for(Fixture::default().used(&[("other", "small")]));
    let (reply, _) = app
        .set_server_profile("default", json!({"serverProfile": "big"}))
        .await;
    reply.assert_error(StatusCode::NOT_FOUND, "client-no-instance", &[]);
    app.assert_no_call("set-server", "an unknown instance");
}

/// Every refusal of the list, each with its status, key, args and field.
#[tokio::test]
async fn every_server_switch_refusal_keeps_its_status_key_args_and_field() {
    for (failure, status, key, args, field) in [
        (
            ClientError::NoInstance,
            StatusCode::NOT_FOUND,
            "client-no-instance",
            Vec::new(),
            None,
        ),
        (
            ClientError::NoServerProfile("big".into()),
            StatusCode::NOT_FOUND,
            "client-no-server-profile",
            vec!["big".to_string()],
            None,
        ),
        (
            ClientError::ServerProfileInUse("big".into(), "other".into()),
            StatusCode::CONFLICT,
            "client-server-profile-in-use",
            vec!["big".to_string(), "other".to_string()],
            None,
        ),
        (
            ClientError::AddressInUse("9010".into()),
            StatusCode::UNPROCESSABLE_ENTITY,
            "client-address-in-use",
            vec!["9010".to_string()],
            Some("serverProfile"),
        ),
        (
            ClientError::UnresolvableHost("gpu.example".into()),
            StatusCode::UNPROCESSABLE_ENTITY,
            "client-unresolvable-host",
            vec!["gpu.example".to_string()],
            Some("serverProfile"),
        ),
        (
            ClientError::Invalid(Refusal {
                key: "load-on-startup-over-max",
                values: vec!["12".to_string(), "8".to_string()],
            }),
            StatusCode::UNPROCESSABLE_ENTITY,
            "load-on-startup-over-max",
            vec!["12".to_string(), "8".to_string()],
            Some("serverProfile"),
        ),
    ] {
        let app = app_for(
            Fixture::default()
                .used(&[("default", "small")])
                .state("default", InstanceState::Active)
                .set_server_refuses(failure.clone()),
        );
        let (reply, _) = app
            .set_server_profile("default", json!({"serverProfile": "big"}))
            .await;
        match field {
            Some(field) => reply.assert_refused(
                key,
                &args.iter().map(|s| s.as_str()).collect::<Vec<_>>(),
                field,
            ),
            None => reply.assert_error(
                status,
                key,
                &args.iter().map(|s| s.as_str()).collect::<Vec<_>>(),
            ),
        }
        assert_eq!(reply.status, status, "{key}: {}", reply.body);
    }
}

/// Anything else is 502 `client-other`, without the manager's detail, and the
/// session stays signed in.
#[tokio::test]
async fn a_server_switch_the_manager_cannot_explain_is_502() {
    let app = app_for(
        Fixture::default()
            .used(&[("default", "small")])
            .set_server_refuses(ClientError::Service("the service manager said no".into())),
    );
    let (reply, cookie) = app
        .set_server_profile("default", json!({"serverProfile": "big"}))
        .await;
    reply.assert_error(StatusCode::BAD_GATEWAY, "client-other", &[]);
    assert!(!reply.body.contains("service manager"), "{}", reply.body);
    app.session_of(&cookie).await.assert_signed_in_as("alice");
}

// ── The instance's model rows ────────────────────────────────────────────

/// `GET /api/instances/{id}/profiles/model`: the pool's rows in the pool's
/// order, each with `value`, `viaAll` and `effective` as `assign::rows` gives
/// them, and the `*` of the section as `all`.
#[tokio::test]
async fn the_instance_model_rows_carry_value_via_all_and_effective() {
    let app = app_for(
        Fixture::pool()
            .used(&[("default", "default")])
            .with_assignments(&assignments(&[("default", &["* = inherit", "gamma = on"])])),
    );
    let (reply, _) = app.model_rows("default").await;
    assert_eq!(reply.status, StatusCode::OK, "{}", reply.body);
    let doc = reply.json();
    assert_eq!(doc["all"], json!("inherit"), "{}", reply.body);
    assert_eq!(
        doc["profiles"],
        json!([
            {
                "id": "alpha",
                "engine": ENGINE,
                "engineAvailable": true,
                "enabled": true,
                "model": "/models/alpha.gguf",
                "value": "none",
                "viaAll": true,
                "effective": true,
            },
            {
                "id": "beta",
                "engine": ENGINE,
                "engineAvailable": true,
                "enabled": true,
                "model": "/models/beta.gguf",
                "value": "none",
                "viaAll": true,
                "effective": true,
            },
            {
                "id": "gamma",
                "engine": ENGINE,
                "engineAvailable": true,
                "enabled": false,
                "model": "/models/gamma.gguf",
                "value": "on",
                "viaAll": false,
                "effective": true,
            },
        ]),
        "{}",
        reply.body
    );
}

/// With no `*` in the section, `all` is `none`, nothing is `viaAll`, and only
/// the explicit `on` is effective.
#[tokio::test]
async fn the_instance_model_rows_of_no_star_are_all_none() {
    let app = app_for(
        Fixture::pool()
            .used(&[("default", "default")])
            .with_assignments(&assignments(&[("default", &["beta = inherit"])])),
    );
    let (reply, _) = app.model_rows("default").await;
    assert_eq!(reply.status, StatusCode::OK, "{}", reply.body);
    let doc = reply.json();
    assert_eq!(doc["all"], json!("none"), "{}", reply.body);
    let rows: Vec<(&str, &str, bool, bool)> = doc["profiles"]
        .as_array()
        .expect("the rows")
        .iter()
        .map(|r| {
            (
                r["id"].as_str().expect("id"),
                r["value"].as_str().expect("value"),
                r["viaAll"].as_bool().expect("viaAll"),
                r["effective"].as_bool().expect("effective"),
            )
        })
        .collect();
    assert_eq!(
        rows,
        vec![
            ("alpha", "none", false, false),
            ("beta", "inherit", false, true),
            ("gamma", "none", false, false),
        ],
        "{}",
        reply.body
    );
}

/// No assignments file at all: every model profile of the pool is a row with no
/// value, effective nowhere, and `all` is `none`.
#[tokio::test]
async fn the_instance_model_rows_of_no_assignments_file_are_all_none() {
    let app = app_for(Fixture::pool().used(&[("default", "default")]));
    let (reply, _) = app.model_rows("default").await;
    assert_eq!(reply.status, StatusCode::OK, "{}", reply.body);
    let doc = reply.json();
    assert_eq!(doc["all"], json!("none"), "{}", reply.body);
    for row in doc["profiles"].as_array().expect("the rows") {
        assert_eq!(row["value"], json!("none"), "{}", reply.body);
        assert_eq!(row["viaAll"], json!(false), "{}", reply.body);
        assert_eq!(row["effective"], json!(false), "{}", reply.body);
    }
    assert_eq!(doc["profiles"].as_array().expect("the rows").len(), 3);
}

/// The rows and the assignment are read as the signed-in user; the record that
/// names the instance is the bff's own account.
#[tokio::test]
async fn the_instance_model_rows_are_read_as_the_signed_in_user() {
    let app = app_for(
        Fixture::pool()
            .used(&[("default", "default")])
            .with_assignments(&assignments(&[("default", &["* = inherit"])])),
    );
    let (reply, _) = app.model_rows("default").await;
    assert_eq!(reply.status, StatusCode::OK, "{}", reply.body);
    assert_eq!(
        app.actor_of(&format!("read {ASSIGNMENTS}")),
        Some("alice".to_string())
    );
    assert_eq!(
        app.actor_of_exact(&format!("read {MODELS}")),
        Some("alice".to_string())
    );
    assert_eq!(app.actor_of("instances"), None);
}

/// An id the record does not name: 404 `client-no-instance`, and the files are
/// not read.
#[tokio::test]
async fn the_instance_model_rows_of_an_unknown_instance_are_404() {
    let app = app_for(Fixture::pool().used(&[("other", "other")]));
    let (reply, _) = app.model_rows("default").await;
    reply.assert_error(StatusCode::NOT_FOUND, "client-no-instance", &[]);
    app.assert_no_call("read", "an unknown instance");
}

/// The assignment file cannot be read: the list fails, 502 `client-other`,
/// never an empty map.
#[tokio::test]
async fn the_instance_model_rows_of_an_unreadable_assignment_are_502() {
    let app = app_for(
        Fixture::pool()
            .used(&[("default", "default")])
            .read_refuses(ASSIGNMENTS, ClientError::Io("gone".into())),
    );
    let (reply, _) = app.model_rows("default").await;
    reply.assert_error(StatusCode::BAD_GATEWAY, "client-other", &[]);
}

// ── One assignment ───────────────────────────────────────────────────────

/// The write lands and the after-read is the answer: `gamma`, switched off in
/// the pool and covered by `* = inherit`, is set `off`. The model profile is
/// effective on neither side, so the instance is given exactly what it was
/// given before: 200 `{"effective": false, "running": false}` for a running
/// instance, and its state is not asked at all (ARCHITECTURE.md's slice 15
/// as-built: `running` is the effective set changing and the instance running,
/// nothing wider, so a key that lands under a `*` of `inherit` is no exception).
#[tokio::test]
async fn the_assignment_put_answers_the_after_reads_effective() {
    let app = app_for(
        Fixture::pool()
            .used(&[("default", "default")])
            .with_assignments(&assignments(&[("default", &["* = inherit"])]))
            .state("default", InstanceState::Active),
    );
    let (reply, _) = app
        .set_assignment("default", "gamma", json!({"value": "off"}))
        .await;
    assert_eq!(reply.status, StatusCode::OK, "{}", reply.body);
    assert_eq!(
        reply.json(),
        json!({"effective": false, "running": false}),
        "{}",
        reply.body
    );
    assert!(
        app.calls()
            .iter()
            .any(|c| c == "set-model default gamma off"),
        "{:?}",
        app.calls()
    );
    app.assert_no_call(
        "status",
        "an off under a star of inherit, the profile effective on neither side",
    );
}

/// The running case of the same key: `gamma` set `on` under the `*` of
/// `inherit` moves it off the whole pool's switch (which leaves it off, the
/// profile being in the disabled file) to an override of its own, so the set of
/// model profiles the instance is given changes: 200
/// `{"effective": true, "running": true}` for a running instance, and the
/// state is what carried that.
#[tokio::test]
async fn the_assignment_put_answers_running_true_for_an_on_under_a_star_inherit() {
    let app = app_for(
        Fixture::pool()
            .used(&[("default", "default")])
            .with_assignments(&assignments(&[("default", &["* = inherit"])]))
            .state("default", InstanceState::Active),
    );
    let (reply, _) = app
        .set_assignment("default", "gamma", json!({"value": "on"}))
        .await;
    assert_eq!(reply.status, StatusCode::OK, "{}", reply.body);
    assert_eq!(
        reply.json(),
        json!({"effective": true, "running": true}),
        "{}",
        reply.body
    );
    assert!(
        app.calls().iter().any(|c| c == "status default"),
        "the state of the instance is asked, the effective set changed: {:?}",
        app.calls()
    );
}

/// The other side of the same rule: `beta` is switched ON in the pool and
/// reaches the instance through the `*` of `inherit`, so setting it `on` pins
/// what it already was. `effective` is true on both sides, the text of
/// `assignments.ini` changes and the instance's load does not: 200
/// `{"effective": true, "running": false}` for a running instance, no state
/// asked.
#[tokio::test]
async fn the_assignment_put_answers_running_false_when_an_on_keeps_the_profile_effective() {
    let app = app_for(
        Fixture::pool()
            .used(&[("default", "default")])
            .with_assignments(&assignments(&[("default", &["* = inherit"])]))
            .state("default", InstanceState::Active),
    );
    let (reply, _) = app
        .set_assignment("default", "beta", json!({"value": "on"}))
        .await;
    assert_eq!(reply.status, StatusCode::OK, "{}", reply.body);
    assert_eq!(
        reply.json(),
        json!({"effective": true, "running": false}),
        "{}",
        reply.body
    );
    app.assert_no_call(
        "status",
        "an on that leaves the model profile effective, as it was",
    );
}

/// The same write on a stopped instance: the write stands, `running` is false.
#[tokio::test]
async fn the_assignment_put_answers_running_false_for_a_stopped_instance() {
    let app = app_for(
        Fixture::pool()
            .used(&[("default", "default")])
            .with_assignments(&assignments(&[("default", &["* = inherit"])])),
    );
    let (reply, _) = app
        .set_assignment("default", "gamma", json!({"value": "off"}))
        .await;
    assert_eq!(reply.status, StatusCode::OK, "{}", reply.body);
    assert_eq!(
        reply.json(),
        json!({"effective": false, "running": false}),
        "{}",
        reply.body
    );
}

/// The write changes nothing: `gamma` is already off under a `* = off`, so
/// `effective` is false and `running` is false on a running instance.
#[tokio::test]
async fn the_assignment_put_answers_running_false_when_effective_did_not_change() {
    let app = app_for(
        Fixture::pool()
            .used(&[("default", "default")])
            .with_assignments(&assignments(&[("default", &["* = off", "gamma = off"])]))
            .state("default", InstanceState::Active),
    );
    let (reply, _) = app
        .set_assignment("default", "gamma", json!({"value": "off"}))
        .await;
    assert_eq!(reply.status, StatusCode::OK, "{}", reply.body);
    assert_eq!(
        reply.json(),
        json!({"effective": false, "running": false}),
        "{}",
        reply.body
    );
}

/// An `on` over a model profile switched off in the pool: `effective` true, and
/// running.
#[tokio::test]
async fn the_assignment_put_answers_effective_true_for_an_on_over_the_disabled_file() {
    let app = app_for(
        Fixture::pool()
            .used(&[("default", "default")])
            .with_assignments(&assignments(&[("default", &[])]))
            .state("default", InstanceState::Active),
    );
    let (reply, _) = app
        .set_assignment("default", "gamma", json!({"value": "on"}))
        .await;
    assert_eq!(reply.status, StatusCode::OK, "{}", reply.body);
    assert_eq!(
        reply.json(),
        json!({"effective": true, "running": true}),
        "{}",
        reply.body
    );
}

/// The after-read fails with the write standing: 200 with the `effective` of
/// the before-read and the value, and `running` true, the safe answer.
#[tokio::test]
async fn the_assignment_put_survives_a_failed_after_read() {
    let app = app_for(
        Fixture::pool()
            .used(&[("default", "default")])
            .with_assignments(&assignments(&[("default", &["* = inherit"])]))
            .after_read_fails()
            .state("default", InstanceState::Active),
    );
    let (reply, cookie) = app
        .set_assignment("default", "gamma", json!({"value": "off"}))
        .await;
    assert_eq!(reply.status, StatusCode::OK, "{}", reply.body);
    assert_eq!(
        reply.json(),
        json!({"effective": false, "running": true}),
        "{}",
        reply.body
    );
    app.session_of(&cookie).await.assert_signed_in_as("alice");
}

/// Every write and every file read of the write goes under the signed-in user:
/// the record of `assignments.ini` read before the write and read back after it,
/// the pool's two files, and the write itself. The record of the instances and
/// the instance's state are the bff's own account. The key is `gamma = on`, the
/// write that changes what the instance is given, so the state read the route
/// owes for a running instance is in the log to be pinned.
#[tokio::test]
async fn the_assignment_put_asks_as_the_signed_in_user() {
    let app = app_for(
        Fixture::pool()
            .used(&[("default", "default")])
            .with_assignments(&assignments(&[("default", &["* = inherit"])]))
            .state("default", InstanceState::Active),
    );
    let (reply, _) = app
        .set_assignment("default", "gamma", json!({"value": "on"}))
        .await;
    assert_eq!(reply.status, StatusCode::OK, "{}", reply.body);
    let alice = Some("alice".to_string());
    assert_eq!(app.actor_of_exact("instances"), None);
    assert_eq!(app.actor_of_exact("set-model default gamma on"), alice);
    assert_eq!(
        app.actors_of_exact(&format!("read {ASSIGNMENTS}"), 2),
        vec![alice.clone(), alice.clone()],
        "the record is read before the write and read back after it, both as alice"
    );
    assert_eq!(app.actor_of_exact(&format!("read {MODELS}")), alice);
    assert_eq!(
        app.actor_of_exact(&format!("read {MODELS_DISABLED}")),
        alice
    );
    assert_eq!(app.actor_of_exact("status default"), None);
}

/// The path's `*` is the whole pool, which has its own route: 404
/// `no-model-profile-named`, and the manager is never asked to write.
#[tokio::test]
async fn a_star_in_the_assignment_path_is_404_and_never_reaches_the_manager() {
    for star in ["*", STAR] {
        let app = app_for(
            Fixture::pool()
                .used(&[("default", "default")])
                .with_assignments(&assignments(&[("default", &["* = inherit"])])),
        );
        let (reply, _) = app
            .set_assignment("default", star, json!({"value": "on"}))
            .await;
        reply.assert_error(StatusCode::NOT_FOUND, "no-model-profile-named", &["*"]);
        app.assert_no_call("set-model", "the star of the path");
    }
}

/// A value that is not one of the four words is 422 with the bff's own check,
/// before any call: the manager is not asked to write, and the files are not
/// read.
#[tokio::test]
async fn an_assignment_value_that_is_not_one_of_the_four_is_422_before_any_call() {
    for value in ["yes", "ON", "", "NONE", "true", "0"] {
        let app = app_for(
            Fixture::pool()
                .used(&[("default", "default")])
                .with_assignments(&assignments(&[("default", &["* = inherit"])])),
        );
        let (reply, _) = app
            .set_assignment("default", "alpha", json!({"value": value}))
            .await;
        reply.assert_refused("model-profile-bad-value", &[value], "value");
        app.assert_no_call("set-model", &format!("the value {value:?}"));
        app.assert_no_call("read", &format!("the value {value:?}"));
    }
}

/// A body it cannot read, and a value that is not a string, are 400
/// `assignment-malformed`, with the manager not asked.
#[tokio::test]
async fn an_assignment_body_that_is_not_a_string_is_400() {
    for body in [
        json!({}),
        json!({"value": null}),
        json!({"value": 1}),
        json!({"value": true}),
        json!({"value": ["on"]}),
        json!({"all": "on"}),
    ] {
        let app = app_for(
            Fixture::pool()
                .used(&[("default", "default")])
                .with_assignments(&assignments(&[("default", &["* = inherit"])])),
        );
        let (reply, _) = app.set_assignment("default", "alpha", body.clone()).await;
        reply.assert_error(StatusCode::BAD_REQUEST, "assignment-malformed", &[]);
        app.assert_no_call("set-model", &format!("{body}"));
    }
    let app = app_for(
        Fixture::pool()
            .used(&[("default", "default")])
            .with_assignments(&assignments(&[("default", &["* = inherit"])])),
    );
    let cookie = app.sign_in().await;
    let reply = app
        .ask_json(
            Method::PUT,
            "/api/instances/default/profiles/model/alpha",
            Some(&cookie),
            "not json",
        )
        .await;
    reply.assert_error(StatusCode::BAD_REQUEST, "assignment-malformed", &[]);
    app.assert_no_call("set-model", "a text body");
}

/// An id the record does not name: 404 `client-no-instance`, no write.
#[tokio::test]
async fn an_assignment_for_an_unknown_instance_is_404() {
    let app = app_for(
        Fixture::pool()
            .used(&[("other", "other")])
            .with_assignments(&assignments(&[("other", &["* = inherit"])])),
    );
    let (reply, _) = app
        .set_assignment("default", "alpha", json!({"value": "on"}))
        .await;
    reply.assert_error(StatusCode::NOT_FOUND, "client-no-instance", &[]);
    app.assert_no_call("set-model", "an unknown instance");
}

/// A model profile the pool does not hold: 404 `no-model-profile-named` naming
/// it, the manager's own refusal, and the session stays signed in.
#[tokio::test]
async fn an_assignment_of_a_model_profile_the_pool_lacks_is_404() {
    let app = app_for(
        Fixture::pool()
            .used(&[("default", "default")])
            .with_assignments(&assignments(&[("default", &[])]))
            .set_model_refuses(ClientError::Invalid(Refusal {
                key: "no-model-profile-named",
                values: vec!["zeta".to_string()],
            })),
    );
    let (reply, cookie) = app
        .set_assignment("default", "zeta", json!({"value": "on"}))
        .await;
    reply.assert_error(StatusCode::NOT_FOUND, "no-model-profile-named", &["zeta"]);
    app.session_of(&cookie).await.assert_signed_in_as("alice");
}

/// The resource refusals are 422 with the manager's args and no field.
#[tokio::test]
async fn the_assignment_resource_refusals_are_422_with_their_args_and_no_field() {
    for (failure, key, args) in [
        (
            ClientError::Invalid(Refusal {
                key: "model-profile-over-memory",
                values: vec![
                    "alpha".to_string(),
                    "default".to_string(),
                    "24000".to_string(),
                    "16000".to_string(),
                ],
            }),
            "model-profile-over-memory",
            vec!["alpha", "default", "24000", "16000"],
        ),
        (
            ClientError::Invalid(Refusal {
                key: "model-profile-model-missing",
                values: vec!["alpha".to_string()],
            }),
            "model-profile-model-missing",
            vec!["alpha"],
        ),
        (
            ClientError::Invalid(Refusal {
                key: "load-on-startup-over-max",
                values: vec!["12".to_string(), "8".to_string()],
            }),
            "load-on-startup-over-max",
            vec!["12", "8"],
        ),
    ] {
        let app = app_for(
            Fixture::pool()
                .used(&[("default", "default")])
                .with_assignments(&assignments(&[("default", &[])]))
                .set_model_refuses(failure),
        );
        let (reply, _) = app
            .set_assignment("default", "alpha", json!({"value": "on"}))
            .await;
        assert_eq!(
            reply.status,
            StatusCode::UNPROCESSABLE_ENTITY,
            "{key}: {}",
            reply.body
        );
        assert_eq!(
            reply.json(),
            json!({ "key": key, "args": args }),
            "{key}: {}",
            reply.body
        );
    }
}

/// An assignment write is never a compare and swap: no revision in the body,
/// and a 409 is not in the answers.
#[tokio::test]
async fn an_assignment_write_takes_no_revision() {
    let app = app_for(
        Fixture::pool()
            .used(&[("default", "default")])
            .with_assignments(&assignments(&[("default", &[])])),
    );
    let (reply, _) = app
        .set_assignment("default", "alpha", json!({"value": "on", "revision": "0"}))
        .await;
    assert_eq!(reply.status, StatusCode::OK, "{}", reply.body);
}

// ── The whole pool at once ───────────────────────────────────────────────

/// `PUT /api/instances/{id}/profiles/model` `{"all": "inherit"}` is the
/// manager's `SetModelProfile` with the profile `*`: the whole pool follows the
/// global switch, so `running` is true when the set of effective model profiles
/// changed on a running instance.
#[tokio::test]
async fn the_all_put_writes_the_star_and_answers_running() {
    for (all, running) in [("inherit", true), ("none", false)] {
        let app = app_for(
            Fixture::pool()
                .used(&[("default", "default")])
                .with_assignments(&assignments(&[("default", &[])]))
                .state("default", InstanceState::Active),
        );
        let (reply, _) = app.set_all("default", json!({"all": all})).await;
        assert_eq!(reply.status, StatusCode::OK, "{}", reply.body);
        assert_eq!(reply.json(), json!({"running": running}), "{}", reply.body);
        assert!(
            app.calls()
                .iter()
                .any(|c| c == &format!("set-model default * {all}")),
            "{all}: {:?}",
            app.calls()
        );
        assert_eq!(
            app.actor_of(&format!("set-model default * {all}")),
            Some("alice".to_string())
        );
    }
}

/// The whole pool on a stopped instance: the write stands, `running` is false.
#[tokio::test]
async fn the_all_put_answers_running_false_for_a_stopped_instance() {
    let app = app_for(
        Fixture::pool()
            .used(&[("default", "default")])
            .with_assignments(&assignments(&[("default", &[])])),
    );
    let (reply, _) = app.set_all("default", json!({"all": "inherit"})).await;
    assert_eq!(reply.status, StatusCode::OK, "{}", reply.body);
    assert_eq!(reply.json(), json!({"running": false}), "{}", reply.body);
}

/// A value that is not `inherit` or `none` is 422 `model-profile-bad-value`
/// with `field` `all`, before any call.
#[tokio::test]
async fn an_all_value_that_is_not_inherit_or_none_is_422_before_any_call() {
    for all in ["on", "off", "", "OFF", "yes"] {
        let app = app_for(
            Fixture::pool()
                .used(&[("default", "default")])
                .with_assignments(&assignments(&[("default", &[])])),
        );
        let (reply, _) = app.set_all("default", json!({"all": all})).await;
        reply.assert_refused("model-profile-bad-value", &[all], "all");
        app.assert_no_call("set-model", &format!("the all {all:?}"));
    }
}

/// A body it cannot read is 400 `assignment-malformed`, with the manager not
/// asked.
#[tokio::test]
async fn an_all_body_that_is_not_one_of_the_two_is_400() {
    for body in [
        json!({}),
        json!({"all": null}),
        json!({"all": 1}),
        json!({"all": ["inherit"]}),
        json!({"value": "inherit"}),
    ] {
        let app = app_for(
            Fixture::pool()
                .used(&[("default", "default")])
                .with_assignments(&assignments(&[("default", &[])])),
        );
        let (reply, _) = app.set_all("default", body.clone()).await;
        reply.assert_error(StatusCode::BAD_REQUEST, "assignment-malformed", &[]);
        app.assert_no_call("set-model", &format!("{body}"));
    }
}

/// An id the record does not name: 404 `client-no-instance`, no write.
#[tokio::test]
async fn an_all_write_for_an_unknown_instance_is_404() {
    let app = app_for(
        Fixture::pool()
            .used(&[("other", "other")])
            .with_assignments(&assignments(&[("other", &[])])),
    );
    let (reply, _) = app.set_all("other2", json!({"all": "inherit"})).await;
    reply.assert_error(StatusCode::NOT_FOUND, "client-no-instance", &[]);
    app.assert_no_call("set-model", "an unknown instance");
}

/// The manager's `NoInstance` is 404 `client-no-instance`, and its other
/// refusals keep their key and args.
#[tokio::test]
async fn the_all_put_keeps_the_managers_refusals() {
    for (failure, status, key, args) in [
        (
            ClientError::NoInstance,
            StatusCode::NOT_FOUND,
            "client-no-instance",
            Vec::new(),
        ),
        (
            ClientError::Invalid(Refusal {
                key: "model-profile-over-memory",
                values: vec![
                    "*".to_string(),
                    "default".to_string(),
                    "24000".to_string(),
                    "16000".to_string(),
                ],
            }),
            StatusCode::UNPROCESSABLE_ENTITY,
            "model-profile-over-memory",
            vec!["*", "default", "24000", "16000"],
        ),
    ] {
        let app = app_for(
            Fixture::pool()
                .used(&[("default", "default")])
                .with_assignments(&assignments(&[("default", &[])]))
                .set_model_refuses(failure),
        );
        let (reply, _) = app.set_all("default", json!({"all": "inherit"})).await;
        reply.assert_error(status, key, &args);
    }
}

// ── The reviewer's findings on slice 15: running, the write's limit, and a
//    refusal of the bff's own account ─────────────────────────────────────

/// The key lands where the model profile was already not effective, so the
/// record's text changes and nothing else does: `gamma`, switched off in the
/// pool, under a `* = off` with no key of its own, is set `off`. The write
/// stands, `effective` stays false, and `running` is false: the instance serves
/// exactly what it served before, so the page is never offered a restart.
#[tokio::test]
async fn the_assignment_put_answers_running_false_when_a_key_lands_under_a_star_off() {
    let app = app_for(
        Fixture::pool()
            .used(&[("default", "default")])
            .with_assignments(&assignments(&[("default", &["* = off"])]))
            .state("default", InstanceState::Active),
    );
    let (reply, _) = app
        .set_assignment("default", "gamma", json!({"value": "off"}))
        .await;
    assert_eq!(reply.status, StatusCode::OK, "{}", reply.body);
    assert_eq!(
        reply.json(),
        json!({"effective": false, "running": false}),
        "{}",
        reply.body
    );
    app.assert_no_call("status", "an assignment nothing changed for");
}

/// The same on an `inherit`: `gamma` is switched off in the pool and the
/// section's `*` is `off`, so `inherit` follows the pool and leaves the model
/// profile not effective, before and after. The text changes; the instance's
/// load does not, so `running` is false and no state is asked.
#[tokio::test]
async fn the_assignment_put_answers_running_false_for_an_inherit_under_a_star_off() {
    let app = app_for(
        Fixture::pool()
            .used(&[("default", "default")])
            .with_assignments(&assignments(&[("default", &["* = off"])]))
            .state("default", InstanceState::Active),
    );
    let (reply, _) = app
        .set_assignment("default", "gamma", json!({"value": "inherit"}))
        .await;
    assert_eq!(reply.status, StatusCode::OK, "{}", reply.body);
    assert_eq!(
        reply.json(),
        json!({"effective": false, "running": false}),
        "{}",
        reply.body
    );
    app.assert_no_call("status", "an inherit nothing changed for");
}

/// The whole pool's `*` goes from `inherit` to `none` on a section where every
/// model profile already has a key of its own, so the set of effective model
/// profiles is the same on both sides: `alpha` and `beta` are `on`, `gamma` is
/// `off`. The text of `assignments.ini` changes and the instance's load does
/// not, so `running` is false with no state asked.
#[tokio::test]
async fn the_all_put_answers_running_false_when_the_star_changed_nothing_effective() {
    let app = app_for(
        Fixture::pool()
            .used(&[("default", "default")])
            .with_assignments(&assignments(&[(
                "default",
                &["* = inherit", "alpha = on", "beta = on", "gamma = off"],
            )]))
            .state("default", InstanceState::Active),
    );
    let (reply, _) = app.set_all("default", json!({"all": "none"})).await;
    assert_eq!(reply.status, StatusCode::OK, "{}", reply.body);
    assert_eq!(reply.json(), json!({"running": false}), "{}", reply.body);
    app.assert_no_call("status", "a whole-pool write nothing changed for");
}

/// `effective` and `running` come from the record as the manager left it, read
/// after the write, and not from what the bff computed: `gamma`, off in the
/// pool under a `* = off`, is set `on`, and the record the manager answers
/// afterwards carries `gamma = off` (its own rule, and another tab's write
/// landing between). The answer is the record's: `effective` false, and
/// `running` false, the profile not effective on either side.
#[tokio::test]
async fn the_assignment_put_answers_the_record_the_manager_left_after_the_write() {
    let app = app_for(
        Fixture::pool()
            .used(&[("default", "default")])
            .with_assignments(&assignments(&[("default", &["* = off"])]))
            .after_read(&assignments(&[("default", &["* = off", "gamma = off"])]))
            .state("default", InstanceState::Active),
    );
    let (reply, _) = app
        .set_assignment("default", "gamma", json!({"value": "on"}))
        .await;
    assert_eq!(reply.status, StatusCode::OK, "{}", reply.body);
    assert_eq!(
        reply.json(),
        json!({"effective": false, "running": false}),
        "{}",
        reply.body
    );
    let reads = app
        .calls()
        .iter()
        .filter(|call| **call == format!("read {ASSIGNMENTS}"))
        .count();
    assert!(
        reads >= 2,
        "the record was read {reads} times, so the answer is not the after-read: {:?}",
        app.calls()
    );
}

/// The three writes are bound by the write's own limit, not the read's: a
/// manager that takes longer than the read limit (200 ms) and answers inside
/// the write limit (900 ms) is answered 200, the write standing. A route that
/// bounds its write by the read limit answers 503 `client-unavailable` here,
/// for a write that stands.
#[tokio::test]
async fn a_write_that_outlasts_the_read_limit_is_answered_at_the_write_limit() {
    for (uri, body) in [
        (
            "/api/instances/default/profiles/server",
            r#"{"serverProfile": "big"}"#,
        ),
        (
            "/api/instances/default/profiles/model/alpha",
            r#"{"value": "on"}"#,
        ),
        (
            "/api/instances/default/profiles/model",
            r#"{"all": "inherit"}"#,
        ),
    ] {
        let app = app_for_write_timeout(
            Fixture::pool()
                .used(&[("default", "default")])
                .with_assignments(&assignments(&[("default", &["* = off"])]))
                .write_delay(Duration::from_millis(350))
                .state("default", InstanceState::Active),
            Duration::from_millis(900),
        );
        let cookie = app.sign_in().await;
        let started = Instant::now();
        let reply = app.ask_json(Method::PUT, uri, Some(&cookie), body).await;
        assert_eq!(reply.status, StatusCode::OK, "{uri}: {}", reply.body);
        assert!(
            started.elapsed() >= Duration::from_millis(350),
            "{uri}: answered before the write was made"
        );
        app.session_of(&cookie).await.assert_signed_in_as("alice");
    }
}

/// A write past the write's own limit is 503 `client-unavailable` (slice 14's
/// as-built rule), answered no sooner than that limit: the route waits as long
/// as the write is allowed, and the session lives.
#[tokio::test]
async fn a_write_past_the_write_limit_is_503_and_leaves_the_session() {
    let write_timeout = Duration::from_millis(300);
    for (uri, body) in [
        (
            "/api/instances/default/profiles/server",
            r#"{"serverProfile": "big"}"#,
        ),
        (
            "/api/instances/default/profiles/model/alpha",
            r#"{"value": "on"}"#,
        ),
        (
            "/api/instances/default/profiles/model",
            r#"{"all": "inherit"}"#,
        ),
    ] {
        let app = app_for_write_timeout(
            Fixture::pool()
                .used(&[("default", "default")])
                .with_assignments(&assignments(&[("default", &["* = off"])]))
                .write_delay(Duration::from_millis(700))
                .state("default", InstanceState::Active),
            write_timeout,
        );
        let cookie = app.sign_in().await;
        let started = Instant::now();
        let reply = app.ask_json(Method::PUT, uri, Some(&cookie), body).await;
        reply.assert_error(StatusCode::SERVICE_UNAVAILABLE, "client-unavailable", &[]);
        assert!(
            started.elapsed() >= write_timeout,
            "{uri}: answered at {:?}, before the write limit of {write_timeout:?}",
            started.elapsed()
        );
        app.session_of(&cookie).await.assert_signed_in_as("alice");
    }
}

/// The record of the instances and the instance's state are asked on the bff's
/// own account, so the manager's `NotAuthorized` there is a refusal of the
/// service, not of the signed-in user: 502 `client-not-authorized`, the session
/// lives on, and nothing is written. Only a refusal of the user's own call ends
/// a session (`a_refusal_of_the_signed_in_user_ends_the_session`).
#[tokio::test]
async fn a_refusal_of_the_bffs_own_record_read_is_502_and_leaves_the_session() {
    for (method, uri, body) in [
        (
            Method::PUT,
            "/api/instances/default/profiles/server",
            Some(r#"{"serverProfile": "big"}"#),
        ),
        (Method::GET, "/api/instances/default/profiles/model", None),
        (
            Method::PUT,
            "/api/instances/default/profiles/model/alpha",
            Some(r#"{"value": "on"}"#),
        ),
        (
            Method::PUT,
            "/api/instances/default/profiles/model",
            Some(r#"{"all": "inherit"}"#),
        ),
    ] {
        let app = app_for(
            Fixture::pool()
                .used(&[("default", "default")])
                .rows(Err(ClientError::NotAuthorized))
                .with_assignments(&assignments(&[("default", &["* = off"])]))
                .state("default", InstanceState::Active),
        );
        let cookie = app.sign_in().await;
        let reply = match body {
            Some(b) => app.ask_json(method.clone(), uri, Some(&cookie), b).await,
            None => app.get(uri, Some(&cookie)).await,
        };
        reply.assert_error(StatusCode::BAD_GATEWAY, "client-not-authorized", &[]);
        app.assert_no_call("set-server", &format!("{uri} wrote a record"));
        app.assert_no_call("set-model", &format!("{uri} wrote a record"));
        app.session_of(&cookie).await.assert_signed_in_as("alice");
    }
}
