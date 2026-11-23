//! `GET /api/profiles/model`: the model-profile list of the Models page, read through the
//! manager as the signed-in user, through `pai_bff::router_with_session` with
//! a fake manager client and a fake authenticator (never the real bus, pipe,
//! PAM or LogonUser). Each request runs under a deadline, so a bff that waits
//! on the manager fails instead of hanging.
//!
//! The answer (fe/briefs/api-contract.md, `GET /api/profiles/model`): 200 and
//! `[{"id", "engine", "engineAvailable", "enabled", "model", "assignments"}]`,
//! one entry per section of the engine's `models.ini` (enabled true) and
//! `models.ini.disabled` (enabled false), read as the session's user:
//! - `id` as the file spells it, `model` the section's `model` key as the
//!   model profile file holds it (no path resolution, no existence check, the
//!   server machine's separators kept), `""` when the section has no `model`
//!   key or it is empty. Every row carries a string `model`: a row without
//!   one is a malformed reply for the web UI.
//! - `engine` is the id of the engine whose model profile file holds the section:
//!   llama.cpp for both files, the only engine today. `engineAvailable` is
//!   true for every row while llama.cpp is the only engine: there is no
//!   engines route to check it against, so the row simply says the engine
//!   that owns the model profile file is installed.
//! - `assignments` is what the records of `assignments.ini` say about the
//!   profile: `[{"instance", "value", "viaAll", "effective"}]`, sorted by
//!   instance, for every instance the manager lists (`Instances()`) that has an
//!   explicit key or is covered by the record's `*`. A section naming no listed
//!   instance is not listed, and a profile no instance is given has `[]`.
//!   `assignments.ini` unreadable, or the instance list unreadable, fails the
//!   list: an answer of empty lists would tell the page no instance serves
//!   anything, on a machine where the records are simply unreadable.
//! - The order is the GUI's (`model_profiles::load_listed`): the switched-on
//!   model profiles, then the switched-off ones, each group in file order; an id in
//!   both files is listed once, as on (the id matched without case, the row
//!   the copy from `models.ini`). The web UI does not sort.
//! - llama-server's global `[*]` section is not a model profile: it is the defaults
//!   every model profile starts from, and it is never a row. The page switches
//!   model profiles, and the global section is nothing a user switches off.
//! - A missing file is an empty group, not an error: no `models.ini.disabled`
//!   means no switched-off model profile, no `models.ini` means no model profile at all.
//! - Errors `{"key", "args": []}`, never the manager's detail: 401
//!   `session-required` (the manager not asked), 403
//!   `session-not-administrator` when the manager refused the call for the
//!   signed-in user (the sign-in's key for the same fact, and the session
//!   ends with the right that authorized it, as the server read's refusal
//!   since the session rule), 503 `client-unavailable` (no manager, or too
//!   slow), 502 `client-other` for anything else.
//! - The list is the same for any signed-in user: every session is a member
//!   of the administrators' group `pai` (sign-in refuses anyone else 403), so
//!   the route serves it to all of them and asks the manager as each one, who
//!   re-checks their membership at every call.

use axum::body::Body;
use axum::extract::ConnectInfo;
use axum::http::{header, Method, Request, StatusCode};
use axum::Router;
use http_body_util::BodyExt;
use pai_bff::session::{AuthError, Authenticator, Identity};
use pai_bff::{Client, Connect};
use pai_core::client::{ClientError, InstanceInfo, InstanceState, ManagerClient, ManagerFile};
use serde_json::{json, Value};
use std::collections::HashMap;
use std::net::SocketAddr;
use std::sync::atomic::{AtomicU64, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};
use tower::ServiceExt;

/// How long the bff waits for the manager in these tests.
const TIMEOUT: Duration = Duration::from_millis(200);

/// How long a test waits for the bff's answer before failing.
const DEADLINE: Duration = Duration::from_secs(5);

const CLIENT: &str = "127.0.0.1:51000";

/// The revision secret the router is built with. The model-profile list carries no
/// revision, so it is inert here; the route just needs one.
const KEY: [u8; 16] = [0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15];

/// The names the manager knows the engine's model profile files by.
const ON: &str = "models.ini";
const OFF: &str = "models.ini.disabled";

/// A `models.ini` with three switched-on model profiles, in an order that is
/// neither alphabetical nor reversed, so a bff that sorts the list is caught.
/// `alpha`'s model has an inline comment after it: the file's parser ends the
/// value at the marker, and that is the value llama-server reads.
const FILE_ON: &str = "\
; The presets llama-server is offered (--models-preset).\r\n\
[zz-zama-70b]\r\n\
model = /models/Zamba-70B-Q4_K_M.gguf\r\n\
alias = Zama\r\n\
\r\n\
[Qwen3.8-27B]\r\n\
model = /models/Qwen3.8-27B-Q8_0.gguf\r\n\
ctx-size = 262144\r\n\
\r\n\
[alpha]\r\n\
; the router reads this file at launch\r\n\
model = C:\\Models\\alpha.gguf ; kept short for now\r\n\
n-gpu-layers = 99\r\n\
";

/// A `models.ini.disabled` with two switched-off model profiles, again in file
/// order and with ids whose case the file spells.
const FILE_OFF: &str = "\
; Presets switched OFF (llama-server never reads this file).\r\n\
[mistral-7b]\r\n\
model = /models/Mistral-7B-Instruct-v0.3-Q5_K_M.gguf\r\n\
\r\n\
[Bigger]\r\n\
model = /models/DeepSeek-R1-Distill-Qwen-32B-Q4_K_M.gguf\r\n\
n-gpu-layers = 4\r\n\
";

/// The six fields of a row, `engine` and `engineAvailable` fixed to the only
/// engine there is today, and `assignments` empty: these tests' manager lists
/// no instance, so no profile is served anywhere.
fn row(id: &str, enabled: bool, model: &str) -> Value {
    json!({
        "id": id,
        "engine": "llama.cpp",
        "engineAvailable": true,
        "enabled": enabled,
        "model": model,
        "assignments": [],
    })
}

/// One entry of a row's `assignments`: the instance, the value its section of
/// `assignments.ini` carries (the record's `*` when the instance has no key of
/// its own, spelled as the record spells it), `viaAll` true when it comes from
/// that `*`, and whether the profile is effective on it.
fn assignment(instance: &str, value: &str, via_all: bool, effective: bool) -> Value {
    json!({
        "instance": instance,
        "value": value,
        "viaAll": via_all,
        "effective": effective,
    })
}

type Log = Arc<Mutex<Vec<String>>>;

/// The files the manager holds, by name. A name that is not there is a file
/// that does not exist: the manager answers `read_file` with `None`.
type Files = Arc<Mutex<HashMap<String, String>>>;

/// The principal every connect was asked for, in order: `None` is a call the
/// bff makes as its own account.
type Principals = Arc<Mutex<Vec<Option<String>>>>;

fn principals_of(p: &Principals) -> Vec<Option<String>> {
    p.lock().unwrap_or_else(|e| e.into_inner()).clone()
}

// ── Fakes ───────────────────────────────────────────────────────────────

/// The failure a test makes the manager answer every coming read with: the
/// ones a model profile file read can meet.
#[derive(Clone)]
enum Failure {
    /// No manager: not running, no bus, no pipe.
    Unavailable,
    /// Any other failure, with a detail that must stay in the bff.
    Other(String),
}

impl Failure {
    fn into_error(self) -> ClientError {
        match self {
            Failure::Unavailable => ClientError::Unavailable,
            Failure::Other(detail) => ClientError::Other(detail),
        }
    }
}

/// What every connect of one app shares: the files, the read's failure and
/// how long a read takes. Shared cells so a test can change them after it
/// signed in.
#[derive(Clone, Default)]
struct Shared {
    files: Files,
    read_failure: Arc<Mutex<Option<Failure>>>,
    read_pause_ms: Arc<AtomicU64>,
    /// What `instances` answers: the rows a row's `assignments` are built from.
    /// Empty by default: these tests' machine runs no instance, so no profile
    /// is served and every list is empty.
    rows: Arc<Mutex<Vec<InstanceInfo>>>,
    /// The failure `instances` answers with, in place of `rows`.
    rows_failure: Arc<Mutex<Option<Failure>>>,
    /// The one file a read fails on, in place of the plain `read_failure`: how
    /// a test makes `assignments.ini` unreadable and leaves the pool files
    /// readable, which is the case that tempts a bff to answer empty lists.
    fail_read_of: Arc<Mutex<Option<String>>>,
}

/// A manager that answers `read_file` from the files it holds, logs every
/// call (`read <name>`, `status <id>`, ...), and refuses the read for the
/// principals in `refuse`, the way the real manager refuses a user who is no
/// longer a member of `pai`. It is the client of one connect, so it knows the
/// principal that connect was handed (`as_user`).
struct FakeManager {
    shared: Shared,
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

    fn refused(&self) -> bool {
        self.refuse
            .iter()
            .any(|u| Some(u.as_str()) == self.as_user.as_deref())
    }
}

impl ManagerClient for FakeManager {
    fn read_file(&self, name: &str) -> Result<Option<ManagerFile>, ClientError> {
        self.record(format!("read {name}"));
        if self.refused() {
            return Err(ClientError::NotAuthorized);
        }
        let pause = self.shared.read_pause_ms.load(Ordering::SeqCst);
        if pause > 0 {
            // The read runs in the route's blocking task, so this holds that
            // task and not the route.
            std::thread::sleep(Duration::from_millis(pause));
        }
        let failure = self
            .shared
            .read_failure
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .clone();
        if let Some(failure) = failure {
            return Err(failure.into_error());
        }
        if self
            .shared
            .fail_read_of
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .as_deref()
            == Some(name)
        {
            return Err(ClientError::Other("the records are unreadable".into()));
        }
        Ok(self
            .shared
            .files
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .get(name)
            .map(|text| ManagerFile {
                text: text.clone(),
                withheld: false,
            }))
    }
    fn list_instances(&self) -> Result<Vec<String>, ClientError> {
        self.record("list".into());
        Ok(vec!["default".into()])
    }
    /// The rows behind a row's `assignments`: the instances, each with the
    /// engine and the server profile its record names by default.
    fn instances(&self) -> Result<Vec<InstanceInfo>, ClientError> {
        self.record("instances".into());
        let failure = self
            .shared
            .rows_failure
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .clone();
        if let Some(failure) = failure {
            return Err(failure.into_error());
        }
        Ok(self
            .shared
            .rows
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .clone())
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
/// group sign-in admits), both with the password `correct horse`.
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

/// A router over a manager holding `files` (a name it does not hold is a
/// missing file), with the manager's call log, the number of connects and the
/// principal of each.
struct App {
    router: Router,
    log: Log,
    connects: Arc<AtomicUsize>,
    principals: Principals,
    shared: Shared,
}

/// The app: a manager holding `files`, refusing the reads of `refuse`, and
/// answering every connect with `connect_result`.
fn app_for(
    files: &[(&str, &str)],
    refuse: &[&str],
    connect_result: Result<(), ClientError>,
) -> App {
    let shared = Shared::default();
    {
        let mut held = shared.files.lock().unwrap();
        for (name, text) in files {
            held.insert((*name).to_string(), (*text).to_string());
        }
    }
    let log = Log::default();
    let connects = Arc::new(AtomicUsize::new(0));
    let principals = Principals::default();
    let (l, c, p) = (log.clone(), connects.clone(), principals.clone());
    let refuse: Vec<String> = refuse.iter().map(|u| u.to_string()).collect();
    let shared_of_connect = shared.clone();
    let connect: Connect = Arc::new(move |principal: Option<&str>| {
        c.fetch_add(1, Ordering::SeqCst);
        p.lock()
            .unwrap_or_else(|e| e.into_inner())
            .push(principal.map(str::to_string));
        connect_result.clone()?;
        Ok(Box::new(FakeManager {
            shared: shared_of_connect.clone(),
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
        KEY,
    );
    App {
        router,
        log,
        connects,
        principals,
        shared,
    }
}

/// A manager holding `on` as `models.ini` and `off` as `models.ini.disabled`.
fn app_holding(on: &str, off: &str) -> App {
    app_for(&[(ON, on), (OFF, off)], &[], Ok(()))
}

/// The manager holding the pool `POOL_ON` / `POOL_OFF` and `assignments` as
/// `assignments.ini`: the state of a machine whose records hand profiles to
/// instances. The instance list is empty until the test sets it.
fn app_pool(assignments: &str) -> App {
    app_for(
        &[(ON, POOL_ON), (OFF, POOL_OFF), (ASSIGNS, assignments)],
        &[],
        Ok(()),
    )
}

/// One instance's row: the id, the engine, the id's own server profile, port 0
/// and no tags.
fn info(id: &str) -> InstanceInfo {
    InstanceInfo {
        id: id.to_string(),
        engine: "llama.cpp".to_string(),
        server_profile: Some(id.to_string()),
        port: 0,
        profile_tags: Vec::new(),
        instance_tags: Vec::new(),
    }
}

/// The pool the assignment tests read: `alpha` switched on, `mistral-7b`
/// switched off.
const POOL_ON: &str = "[alpha]\r\nmodel = /models/alpha.gguf\r\n\r\n";
const POOL_OFF: &str = "[mistral-7b]\r\nmodel = /models/mistral.gguf\r\n\r\n";

/// The name the manager knows the assignment records by.
const ASSIGNS: &str = "assignments.ini";

/// A manager holding `on` as `models.ini` and no `models.ini.disabled`.
fn app_holding_no_disabled(on: &str) -> App {
    app_for(&[(ON, on)], &[], Ok(()))
}

/// A manager holding `on`, refusing the reads of the named principals: the
/// member of `pai` removed after the sign-in.
fn app_refusing(refuse: &[&str], on: &str, off: &str) -> App {
    app_for(&[(ON, on), (OFF, off)], refuse, Ok(()))
}

/// A manager that answers every read with `failure`, holding both files.
fn app_read_failing(failure: Failure) -> App {
    let app = app_holding(FILE_ON, FILE_OFF);
    app.set_read_failure(failure);
    app
}

/// The manager of no model profile file at all: neither file exists.
fn app_no_files() -> App {
    app_for(&[], &[], Ok(()))
}

/// The app that cannot reach the manager at all.
fn app_no_manager() -> App {
    app_for(
        &[(ON, FILE_ON), (OFF, FILE_OFF)],
        &[],
        Err(ClientError::Unavailable),
    )
}

impl App {
    /// The manager's calls, in order.
    fn calls(&self) -> Vec<String> {
        self.log.lock().unwrap_or_else(|e| e.into_inner()).clone()
    }

    /// The manager's calls, sorted and joined by a space: what the read asks
    /// for, without pinning the order the route makes the calls in.
    fn sorted_calls(&self) -> String {
        let mut calls = self.calls();
        calls.sort();
        calls.join(" ")
    }

    /// The manager answers `instances` with one row per id (its engine, its
    /// own server profile, no tags): the instances the records may name.
    fn set_rows(&self, ids: &[&str]) {
        *self.shared.rows.lock().unwrap() = ids.iter().map(|id| info(id)).collect();
    }

    /// The manager fails `instances` with `failure`.
    fn set_rows_failure(&self, failure: Failure) {
        *self.shared.rows_failure.lock().unwrap() = Some(failure);
    }

    /// The read of `name` fails, every other read answers.
    fn set_read_fails_of(&self, name: &str) {
        *self.shared.fail_read_of.lock().unwrap() = Some(name.to_string());
    }

    /// Answer every coming read with `failure`.
    fn set_read_failure(&self, failure: Failure) {
        *self.shared.read_failure.lock().unwrap() = Some(failure);
    }

    /// A read that takes `ms` milliseconds: the manager busy while the list is
    /// being read.
    fn set_read_pause_ms(&self, ms: u64) {
        self.shared.read_pause_ms.store(ms, Ordering::SeqCst);
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

    /// `GET uri` with `cookie` and extra headers (a request that tries to name
    /// the user it wants the bff to act for).
    async fn get_with(&self, uri: &str, cookie: Option<&str>, extra: &[(&str, &str)]) -> Reply {
        let mut builder = Request::builder().method(Method::GET).uri(uri);
        if let Some(c) = cookie {
            builder = builder.header(header::COOKIE, c);
        }
        for (name, value) in extra {
            builder = builder.header(*name, *value);
        }
        let request = builder.body(Body::empty()).expect("request");
        self.send(request).await
    }

    /// One request through the router, under the deadline every request runs
    /// under: a bff that never answers fails the test instead of hanging it.
    async fn send(&self, request: Request<Body>) -> Reply {
        let uri = request.uri().clone();
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

    /// Signed in as `alice`, `GET /api/profiles/model`.
    async fn list(&self) -> Reply {
        let cookie = self.sign_in().await;
        self.get("/api/profiles/model", Some(&cookie)).await
    }

    /// Signed in as `user`, `GET /api/profiles/model`.
    async fn list_as(&self, user: &str) -> Reply {
        let cookie = self.sign_in_as(user).await;
        self.get("/api/profiles/model", Some(&cookie)).await
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

    /// The 200 list: a JSON array, answered as JSON.
    fn rows(&self) -> Vec<Value> {
        assert_eq!(self.status, StatusCode::OK, "{}", self.body);
        assert!(
            self.content_type.starts_with("application/json"),
            "content-type {:?}",
            self.content_type
        );
        self.json()
            .as_array()
            .unwrap_or_else(|| panic!("not a JSON array: {}", self.body))
            .clone()
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
    let app = app_holding(FILE_ON, FILE_OFF);
    let r = app.get("/api/profiles/model", None).await;
    r.assert_error(StatusCode::UNAUTHORIZED, "session-required");
    assert_eq!(app.connects.load(Ordering::SeqCst), 0, "no connect");
    assert!(app.calls().is_empty(), "{:?}", app.calls());
    assert!(
        principals_of(&app.principals).is_empty(),
        "no principal named: {:?}",
        principals_of(&app.principals)
    );
}

/// A cookie naming no session (made up, or from before a restart) is the same.
#[tokio::test]
async fn an_unknown_session_is_session_required() {
    let app = app_holding(FILE_ON, FILE_OFF);
    let _real = app.sign_in().await;
    let r = app
        .get(
            "/api/profiles/model",
            Some(&format!("pai_session={}", "ab".repeat(32))),
        )
        .await;
    r.assert_error(StatusCode::UNAUTHORIZED, "session-required");
    assert_eq!(app.connects.load(Ordering::SeqCst), 0, "no connect");
    assert!(app.calls().is_empty(), "{:?}", app.calls());
}

// ── The principal: the route acts for the signed-in user ────────────────

/// The connect is asked with the session's user as the principal, so the
/// manager re-checks that user's membership of `pai`: the session-free routes
/// connect with none (`tests/instances.rs`), this one names the user. Both
/// model profile files are read over that one connection.
#[tokio::test]
async fn the_read_connects_as_the_signed_in_user() {
    for user in ["alice", "root"] {
        let app = app_holding(FILE_ON, FILE_OFF);
        assert_eq!(app.list_as(user).await.rows().len(), 5, "{user}");
        assert_eq!(
            principals_of(&app.principals),
            vec![Some(user.to_string())],
            "{user}: the principal of the connect"
        );
        assert_eq!(
            app.sorted_calls(),
            "instances read assignments.ini read models.ini read models.ini.disabled"
        );
    }
}

/// The list is served to every signed-in user, with no more rights than the
/// read of the model profile files the manager already gives any caller.
#[tokio::test]
async fn every_signed_in_user_is_served_the_same_list() {
    let app = app_holding(FILE_ON, FILE_OFF);
    let as_alice = app.list_as("alice").await.rows();
    let as_root = app.list_as("root").await.rows();
    assert_eq!(as_alice, as_root, "the list is not per user");
    assert_eq!(
        principals_of(&app.principals),
        vec![Some("alice".to_string()), Some("root".to_string())],
        "each read asked for its own session's user"
    );
}

/// A name in the request never becomes the principal: a query parameter and a
/// header naming another user are ignored, the session's user is asked for.
#[tokio::test]
async fn a_name_in_the_request_is_never_the_principal() {
    let app = app_holding(FILE_ON, FILE_OFF);
    let cookie = app.sign_in_as("alice").await;
    let rows = app
        .get_with(
            "/api/profiles/model?user=root",
            Some(&cookie),
            &[("x-user", "root"), ("x-principal", "root")],
        )
        .await
        .rows();
    assert_eq!(rows.len(), 5, "{:?}", rows);
    assert_eq!(
        principals_of(&app.principals),
        vec![Some("alice".to_string())],
        "the session's user, never the request's"
    );
}

/// The read asks the manager the two pool files, the records of
/// `assignments.ini` (the rows' `assignments`) and the instance list (the ids
/// those records name), and nothing else: no status, no write.
#[tokio::test]
async fn the_read_asks_the_pool_files_the_assignments_and_the_instances() {
    let app = app_holding(FILE_ON, FILE_OFF);
    assert_eq!(app.list().await.rows().len(), 5);
    assert_eq!(
        app.sorted_calls(),
        "instances read assignments.ini read models.ini read models.ini.disabled",
        "the three reads and the instance list, nothing beside them"
    );
}

/// The session ends for the user the manager refused, at once: 403
/// `session-not-administrator` (the sign-in's key for the same fact, not the
/// `client-not-authorized` of the session-free routes) and the same cookie
/// signed out. The refusal stops the route: the second file is not read.
#[tokio::test]
async fn a_refused_user_is_answered_403_and_signed_out() {
    let app = app_refusing(&["alice"], FILE_ON, FILE_OFF);
    let cookie = app.sign_in().await;
    app.get("/api/profiles/model", Some(&cookie))
        .await
        .assert_error(StatusCode::FORBIDDEN, "session-not-administrator");
    app.get("/api/session", Some(&cookie))
        .await
        .assert_error(StatusCode::UNAUTHORIZED, "session-required");
    assert_eq!(
        app.calls().len(),
        1,
        "the refusal ends the read: {:?}",
        app.calls()
    );
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
    let app = app_refusing(&["root"], FILE_ON, FILE_OFF);
    let alice = app.sign_in_as("alice").await;
    let root = app.sign_in_as("root").await;
    app.get("/api/profiles/model", Some(&root))
        .await
        .assert_error(StatusCode::FORBIDDEN, "session-not-administrator");
    app.get("/api/session", Some(&root))
        .await
        .assert_error(StatusCode::UNAUTHORIZED, "session-required");
    let rows = app.get("/api/profiles/model", Some(&alice)).await.rows();
    assert_eq!(rows.len(), 5, "alice is answered: {rows:?}");
    app.get("/api/session", Some(&alice))
        .await
        .assert_signed_in_as("alice");
}

// ── Failure paths: the manager ──────────────────────────────────────────

/// No manager to connect to: 503 `client-unavailable`.
#[tokio::test]
async fn no_manager_is_unavailable() {
    let app = app_no_manager();
    app.list()
        .await
        .assert_error(StatusCode::SERVICE_UNAVAILABLE, "client-unavailable");
    assert_eq!(app.connects.load(Ordering::SeqCst), 1, "connected once");
}

/// A manager gone during the read is the same.
#[tokio::test]
async fn a_manager_gone_during_the_read_is_unavailable() {
    let app = app_read_failing(Failure::Unavailable);
    app.list()
        .await
        .assert_error(StatusCode::SERVICE_UNAVAILABLE, "client-unavailable");
}

/// A manager slower than the bff's timeout: 503, answered at the timeout and
/// not at the deadline, so the page is never left waiting.
#[tokio::test]
async fn a_manager_too_slow_is_unavailable() {
    let app = app_holding(FILE_ON, FILE_OFF);
    app.set_read_pause_ms(2000);
    let cookie = app.sign_in().await;
    let started = Instant::now();
    app.get("/api/profiles/model", Some(&cookie))
        .await
        .assert_error(StatusCode::SERVICE_UNAVAILABLE, "client-unavailable");
    let took = started.elapsed();
    assert!(
        took < Duration::from_secs(1),
        "answered after {took:?}, not at the {TIMEOUT:?} timeout"
    );
}

/// Any other failure is 502 `client-other` with no args: the manager's detail
/// (which `client-other`'s `{0}` would carry) stays in the bff.
#[tokio::test]
async fn any_other_failure_is_failed_without_the_detail() {
    let app = app_read_failing(Failure::Other(
        "org.freedesktop.DBus.Error.AccessDenied: secret-detail".into(),
    ));
    let r = app.list().await;
    r.assert_error(StatusCode::BAD_GATEWAY, "client-other");
    assert!(!r.body.contains("secret-detail"), "{}", r.body);
}

/// A 503 and a 502 are the bff's news and not a refusal of the user: the
/// session lives on.
#[tokio::test]
async fn a_failure_that_is_not_a_refusal_keeps_the_session() {
    for (failure, status, key) in [
        (
            Failure::Unavailable,
            StatusCode::SERVICE_UNAVAILABLE,
            "client-unavailable",
        ),
        (
            Failure::Other("the manager failed".into()),
            StatusCode::BAD_GATEWAY,
            "client-other",
        ),
    ] {
        let app = app_read_failing(failure);
        let cookie = app.sign_in().await;
        app.get("/api/profiles/model", Some(&cookie))
            .await
            .assert_error(status, key);
        app.get("/api/session", Some(&cookie))
            .await
            .assert_signed_in_as("alice");
    }
}

/// The list and the create are the methods of this path (MP2's
/// `POST /api/profiles/model`, pinned in `model_profile_create.rs`). Any other
/// method is 405 and asks the manager nothing (not even a connection), signed
/// in or not.
#[tokio::test]
async fn model_profiles_refuse_another_method_without_asking_the_manager() {
    for method in [Method::PUT, Method::PATCH, Method::DELETE] {
        let app = app_holding(FILE_ON, FILE_OFF);
        let cookie = app.sign_in().await;
        let request = Request::builder()
            .method(method.clone())
            .uri("/api/profiles/model")
            .header(header::COOKIE, cookie)
            .body(Body::empty())
            .expect("request");
        let r = app.send(request).await;
        assert_eq!(
            r.status,
            StatusCode::METHOD_NOT_ALLOWED,
            "{method} /api/profiles/model: {}",
            r.body
        );
        assert_eq!(
            app.connects.load(Ordering::SeqCst),
            0,
            "{method}: the route connected"
        );
        assert!(app.calls().is_empty(), "{method}: {:?}", app.calls());
    }
}

/// The routes moved to `/api/profiles/model...`, so the model profile paths are paths
/// the router does not route: its own 404, with no body, no content type and no
/// manager call, signed in like anywhere else. A reply of the app (`{"key",
/// "args"}`) here would mean a model profile route is still served.
#[tokio::test]
async fn the_old_model_profile_paths_are_the_routers_404_with_no_body() {
    let app = app_holding(FILE_ON, FILE_OFF);
    let cookie = app.sign_in().await;
    for (method, uri) in [
        (Method::GET, "/api/presets"),
        (Method::GET, "/api/presets/alpha"),
        (Method::PUT, "/api/presets/alpha"),
        (Method::PUT, "/api/presets/alpha/enabled"),
    ] {
        let request = Request::builder()
            .method(method.clone())
            .uri(uri)
            .header(header::COOKIE, cookie.clone())
            .body(Body::empty())
            .expect("request");
        let r = app.send(request).await;
        assert_eq!(r.status, StatusCode::NOT_FOUND, "{method} {uri}");
        assert_eq!(r.content_type, "", "{method} {uri}: content type");
        assert_eq!(r.body, "", "{method} {uri}: body");
    }
    assert_eq!(app.connects.load(Ordering::SeqCst), 0, "no connect");
    assert!(app.calls().is_empty(), "{:?}", app.calls());
    // The session is untouched: an unknown path is not a refusal.
    app.get("/api/session", Some(&cookie))
        .await
        .assert_signed_in_as("alice");
}

// ── What it answers ─────────────────────────────────────────────────────

/// Both files, every row with its six fields, in the GUI's order: the
/// switched-on model profiles in their file order, then the switched-off ones in
/// theirs, each id as the file spells it and each model as the file holds it.
#[tokio::test]
async fn both_files_answer_the_list_in_the_guis_order() {
    let app = app_holding(FILE_ON, FILE_OFF);
    assert_eq!(
        app.list().await.rows(),
        vec![
            row("zz-zama-70b", true, "/models/Zamba-70B-Q4_K_M.gguf"),
            row("Qwen3.8-27B", true, "/models/Qwen3.8-27B-Q8_0.gguf"),
            row("alpha", true, "C:\\Models\\alpha.gguf"),
            row(
                "mistral-7b",
                false,
                "/models/Mistral-7B-Instruct-v0.3-Q5_K_M.gguf"
            ),
            row(
                "Bigger",
                false,
                "/models/DeepSeek-R1-Distill-Qwen-32B-Q4_K_M.gguf"
            ),
        ]
    );
}

/// A row carries exactly the six fields of the contract, with the types the
/// web UI reads: `engine` and `engineAvailable` for every row of both files
/// (llama.cpp is the only engine, and it is available), `model` a string even
/// for a section that has none, and `assignments` an array.
#[tokio::test]
async fn every_row_carries_exactly_the_six_fields() {
    let app = app_holding(FILE_ON, FILE_OFF);
    let rows = app.list().await.rows();
    assert!(!rows.is_empty(), "{rows:?}");
    for row in rows {
        let obj = row.as_object().expect("a JSON object");
        let mut names: Vec<&str> = obj.keys().map(String::as_str).collect();
        names.sort_unstable();
        assert_eq!(
            names,
            [
                "assignments",
                "enabled",
                "engine",
                "engineAvailable",
                "id",
                "model"
            ],
            "{row}"
        );
        assert!(row["id"].is_string(), "{row}");
        assert_eq!(row["engine"], json!("llama.cpp"), "{row}");
        assert_eq!(row["engineAvailable"], json!(true), "{row}");
        assert!(row["enabled"].is_boolean(), "{row}");
        assert!(
            row["model"].is_string(),
            "a model the web UI can read: {row}"
        );
        assert!(
            row["assignments"].is_array(),
            "a list the web UI can read: {row}"
        );
    }
}

/// `model` is the section's key as the file holds it: the machine's separators
/// and spaces kept, an inline comment ended by the file's parser, `""` when the
/// key is absent, empty, or the section has no keys at all.
#[tokio::test]
async fn the_model_is_the_raw_value_and_empty_when_the_key_is_absent() {
    let on = "\
[plain]\r\n\
model = /models/plain.gguf\r\n\
\r\n\
[spaced]\r\n\
   model   =   /models/spaced.gguf   \r\n\
\r\n\
[commented]\r\n\
model = /models/commented.gguf ; a note\r\n\
\r\n\
[windows]\r\n\
model = C:\\Models\\alpha.gguf\r\n\
\r\n\
[no-key]\r\n\
n-gpu-layers = 8\r\n\
\r\n\
[empty-value]\r\n\
model =\r\n\
\r\n\
[bare]\r\n\
";
    let app = app_holding(on, "");
    let rows = app.list().await.rows();
    let models: Vec<&str> = rows
        .iter()
        .map(|row| row["model"].as_str().expect("a string model"))
        .collect();
    assert_eq!(
        models,
        [
            "/models/plain.gguf",
            "/models/spaced.gguf",
            "/models/commented.gguf",
            "C:\\Models\\alpha.gguf",
            "",
            "",
            "",
        ],
        "{rows:?}"
    );
}

/// An id in both files is listed once, as enabled, with the section
/// `models.ini` holds (the copy llama-server serves): the id matched without
/// case, as the GUI's list does.
#[tokio::test]
async fn an_id_in_both_files_is_listed_once_as_enabled() {
    let on = "[Qwen3-32B]\r\nmodel = /models/on.gguf\r\n";
    let off = "[qwen3-32b]\r\nmodel = /models/off.gguf\r\n\
[other]\r\nmodel = /models/other.gguf\r\n";
    let app = app_holding(on, off);
    assert_eq!(
        app.list().await.rows(),
        vec![
            row("Qwen3-32B", true, "/models/on.gguf"),
            row("other", false, "/models/other.gguf"),
        ]
    );
}

/// llama-server's global `[*]` section, the defaults every model profile starts from,
/// is not a model profile: it is never a row, in either file, and the model profiles beside
/// it keep their place and their order.
#[tokio::test]
async fn the_global_section_is_not_a_model_profile() {
    let on = "\
[*]\r\n\
model = /models/global.gguf\r\n\
n-gpu-layers = 99\r\n\
\r\n\
[alpha]\r\n\
model = /models/alpha.gguf\r\n\
";
    let off = "\
[*]\r\n\
ctx-size = 4096\r\n\
\r\n\
[mistral-7b]\r\n\
model = /models/Mistral-7B-Q5_K_M.gguf\r\n\
";
    let app = app_holding(on, off);
    assert_eq!(
        app.list().await.rows(),
        vec![
            row("alpha", true, "/models/alpha.gguf"),
            row("mistral-7b", false, "/models/Mistral-7B-Q5_K_M.gguf"),
        ]
    );
}

/// No `models.ini.disabled` is not an error: the switched-on model profiles alone,
/// and the bff still asked the manager for the file (it answers it is not
/// there).
#[tokio::test]
async fn a_missing_disabled_file_is_not_an_error() {
    let app = app_holding_no_disabled(FILE_ON);
    assert_eq!(
        app.list().await.rows(),
        vec![
            row("zz-zama-70b", true, "/models/Zamba-70B-Q4_K_M.gguf"),
            row("Qwen3.8-27B", true, "/models/Qwen3.8-27B-Q8_0.gguf"),
            row("alpha", true, "C:\\Models\\alpha.gguf"),
        ]
    );
    assert_eq!(
        app.sorted_calls(),
        "instances read assignments.ini read models.ini read models.ini.disabled"
    );
}

/// Neither file exists: 200 and an empty list (the page's empty state), not an
/// error: a missing file is an empty group.
#[tokio::test]
async fn no_model_profile_file_at_all_is_an_empty_list() {
    let app = app_no_files();
    assert_eq!(app.list().await.rows(), Vec::<Value>::new());
    assert_eq!(
        app.sorted_calls(),
        "instances read assignments.ini read models.ini read models.ini.disabled"
    );
}

/// Both files there and no section in either: 200 and an empty list, with a
/// comment-only file and a blank one.
#[tokio::test]
async fn files_with_no_sections_answer_an_empty_list() {
    let app = app_holding("; nothing switched on\r\n", "\r\n");
    assert_eq!(app.list().await.rows(), Vec::<Value>::new());
}

/// The other routes are unchanged beside the new one: `GET /api/version`
/// answers the framework's version, the machine's name and its operating system
/// to a signed-in reader.
#[tokio::test]
async fn version_is_still_served_beside_the_model_profiles() {
    let app = app_no_manager();
    let cookie = app.sign_in().await;
    let r = app.get("/api/version", Some(&cookie)).await;
    assert_eq!(r.status, StatusCode::OK, "{}", r.body);
    let os = pai_core::host::os(std::path::Path::new("/"));
    assert_eq!(
        r.json(),
        json!({
            "version": env!("CARGO_PKG_VERSION"),
            "name": pai_bff::listen::hostname(),
            "os": { "name": os.name, "version": os.version, "kernel": os.kernel },
        })
    );
}

// ── `assignments`: what the records say about each profile ──────────────

/// A row's `assignments` is what the records of `assignments.ini` say about
/// that profile, one entry per instance the manager lists that the records
/// name it to, sorted by instance: `alpha` is given `on` by `default` and
/// `inherit`ed by `lab`, so it is served on both; `mistral-7b` is switched off
/// in the pool and `default` says `off` about it, so it is served nowhere.
#[tokio::test]
async fn a_row_names_the_instances_the_records_give_the_profile_to() {
    let app =
        app_pool("[default]\r\nalpha = on\r\nmistral-7b = off\r\n\r\n[lab]\r\nalpha = inherit\r\n");
    app.set_rows(&["default", "lab"]);
    let rows = app.list().await.rows();
    assert_eq!(rows.len(), 2, "{rows:?}");
    assert_eq!(rows[0]["id"], "alpha", "the switched-on file first");
    assert_eq!(
        rows[0]["assignments"],
        json!([
            assignment("default", "on", false, true),
            assignment("lab", "inherit", false, true),
        ]),
        "{}",
        rows[0]
    );
    assert_eq!(
        rows[1]["assignments"],
        json!([assignment("default", "off", false, false)]),
        "{}",
        rows[1]
    );
}

/// The record's `*` stands in for an instance that has no key of its own,
/// spelled as the record spells it with `viaAll` true; an explicit key wins
/// over it and is not `viaAll`. An instance with no section of its own is in no
/// list: the records say nothing about it.
#[tokio::test]
async fn the_star_record_stands_in_for_an_instance_with_no_key_of_its_own() {
    let app = app_pool("[default]\r\n* = inherit\r\nalpha = off\r\n\r\n[lab]\r\nalpha = on\r\n");
    app.set_rows(&["default", "lab"]);
    let rows = app.list().await.rows();
    assert_eq!(
        rows[0]["assignments"],
        json!([
            assignment("default", "off", false, false),
            assignment("lab", "on", false, true),
        ]),
        "the explicit key of {} wins over its record's *",
        rows[0]["id"]
    );
    assert_eq!(
        rows[1]["assignments"],
        json!([assignment("default", "inherit", true, false)]),
        "{}: * covers it, and a switched-off pool profile inherited is not served",
        rows[1]
    );
}

/// A profile no instance is given is a row with an empty list: the page shows it
/// as served nowhere. An id the records name that the pool does not hold adds
/// no row: the list is the pool's.
#[tokio::test]
async fn a_profile_no_instance_is_given_is_a_row_with_an_empty_list() {
    let app = app_pool("[default]\r\ngamma = on\r\n");
    app.set_rows(&["default", "lab"]);
    let rows = app.list().await.rows();
    assert_eq!(
        rows.iter()
            .map(|r| r["id"].as_str().unwrap())
            .collect::<Vec<_>>(),
        ["alpha", "mistral-7b"],
        "a record naming a profile the pool lacks adds no row"
    );
    for row in rows {
        assert_eq!(row["assignments"], json!([]), "{}", row);
    }
}

/// The records unreadable, or the instance list unreadable, fails the list: an
/// answer of empty `assignments` would tell the page that no instance serves
/// anything, on a machine where the records are simply unreachable. The pool
/// files readable is no help: the same keys as a pool read that fails.
#[tokio::test]
async fn the_records_unreadable_fail_the_list_rather_than_answer_empty_lists() {
    let app = app_pool("[default]\r\nalpha = on\r\n");
    app.set_rows(&["default"]);
    app.set_read_fails_of(ASSIGNS);
    app.list()
        .await
        .assert_error(StatusCode::BAD_GATEWAY, "client-other");

    for (failure, status, key) in [
        (
            Failure::Unavailable,
            StatusCode::SERVICE_UNAVAILABLE,
            "client-unavailable",
        ),
        (
            Failure::Other("no rows".into()),
            StatusCode::BAD_GATEWAY,
            "client-other",
        ),
    ] {
        let app = app_pool("[default]\r\nalpha = on\r\n");
        app.set_rows(&["default"]);
        app.set_rows_failure(failure);
        app.list().await.assert_error(status, key);
    }
}
