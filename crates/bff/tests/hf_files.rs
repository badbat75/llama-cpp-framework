//! `GET /api/hf/files?repo=<id>`: the GGUF files of a Hugging Face repo for
//! the web picker (ARCHITECTURE.md "Choosing the model file on the web",
//! slice M2), through `pai_bff::router_with_session_and_hf` with a fake
//! manager client, a fake authenticator and a fake `pai_core::hf::Fetch`:
//! never the bus, the network, curl, PAM or a real repo. Every request runs
//! under a deadline, so a bff that waits on Hugging Face fails, not hangs.
//!
//! The contract (the design's sections (1), (2), (3), (5), (6) and M2's
//! done-when):
//! - A session gates the route: with none, 401 `session-required`, and
//!   nothing is fetched.
//! - The repo id is held to `parse_spec` before any network: an invalid id
//!   (and no id at all) is 400 `hf-repo-invalid` and fetches nothing.
//! - The two URLs are built from the fixed endpoint `https://huggingface.co/`.
//! - HF's 401, 403 and 404 answer one refusal, 404 `hf-repo-unavailable`
//!   (args: the repo); a 3xx answers 502 `hf-repo-moved` (args: the repo)
//!   with no second request. No token is sent (section (3)).
//! - A listing is cached per repo: a second call within 10 minutes fetches
//!   nothing. At most two HF calls run at once: a third concurrent call is
//!   429 `hf-busy`.
//! - The cache holds one spec's listing, not the repo's: the `default` is the
//!   spec's tag and file, so a tagged call leaves the bare repo's entry alone
//!   and the other way round (the M2 review's D1).
//! - The slot of a listing that fails, and of one the route gave up on, comes
//!   back: the calls after it are not 429 (the M2 review's G1).
//! - The reply is the design's document: `source`, `repo`, `revision`,
//!   `files` with `tag`, `file`, `shards`, `size`, `sha256`, `sidecars`,
//!   `value`, `default`; `value` carries `hf-repo = <repo>:<tag>` and
//!   `hf-file` only when the tag alone does not name one file.
//! - The bff's own curl runner: `-q` first, never `-L`, the config on stdin,
//!   and the config is the bff profile of `pai_core::hf::bff_curl_config`;
//!   every spec is built through `parse_spec` (the M1 review's carries).

use axum::body::Body;
use axum::extract::ConnectInfo;
use axum::http::{header, Method, Request, StatusCode};
use axum::Router;
use http_body_util::BodyExt;
use pai_bff::session::{AuthError, Authenticator, Identity};
use pai_bff::{Client, Connect};
use pai_core::checks::Refusal;
use pai_core::client::{ClientError, InstanceState, ManagerClient, ManagerFile, ModelInfoReply};
use pai_core::hf::{Fetch, HfError, Token};
use serde_json::{json, Value};
use std::collections::{HashMap, VecDeque};
use std::net::SocketAddr;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::{Arc, Condvar, Mutex};
use std::time::{Duration, Instant};
use tower::ServiceExt;

/// How long the bff waits for a manager read in these tests (the route asks
/// the manager nothing; the builder needs a bound).
const TIMEOUT: Duration = Duration::from_secs(5);

/// How long a test waits for the bff's answer before failing.
const DEADLINE: Duration = Duration::from_secs(10);

/// How long a call the route gives up on stays parked in the fake: above
/// `pai_bff::HF_TIMEOUT` (25 s), so the route's own timeout fires while the
/// call is in flight, and short enough that a failing test does not sit on
/// the runtime for a minute.
const HANG_WAKE: Duration = Duration::from_secs(40);

/// How long a test waits for the answer to a call the route gives up on at
/// `pai_bff::HF_TIMEOUT`.
const HF_DEADLINE: Duration = Duration::from_secs(45);

const CLIENT: &str = "127.0.0.1:51003";

/// The revision secret these routers are built with.
const REVISION_KEY: [u8; 16] = [0; 16];

/// The repo these tests list.
const REPO: &str = "o/r";

/// The route under test, with `repo` as the query gives it.
fn uri(repo: &str) -> String {
    format!("/api/hf/files?repo={repo}")
}

/// The commit the refs answer names (40 hex, the rule of `commit_from_refs`).
const COMMIT: &str = "0123456789abcdef0123456789abcdef01234567";

/// The refs answer of the canned listing.
fn refs_json() -> String {
    json!({"branches": [{"name": "main", "targetCommit": COMMIT}]}).to_string()
}

/// The tree answer with one file per quant tag: unique tags, one mmproj.
fn tree_unique_tags() -> String {
    json!([
        {"type": "file", "path": "model-Q4_K_M.gguf",
         "lfs": {"oid": "a1a1a1a1a1a1a1a1a1a1a1a1a1a1a1a1a1a1a1a1", "size": 1000}},
        {"type": "file", "path": "model-Q8_0.gguf",
         "lfs": {"oid": "b2b2b2b2b2b2b2b2b2b2b2b2b2b2b2b2b2b2b2b2", "size": 2000}},
        {"type": "file", "path": "model-Q4_K_M-mmproj.gguf",
         "lfs": {"oid": "c3c3c3c3c3c3c3c3c3c3c3c3c3c3c3c3c3c3c3c3", "size": 100}},
        {"type": "file", "path": "README.md", "size": 5}
    ])
    .to_string()
}

/// The tree answer with two files sharing the tag `Q4_K_M`: the tag alone
/// names no file.
fn tree_ambiguous_tag() -> String {
    json!([
        {"type": "file", "path": "a-Q4_K_M.gguf", "lfs": {"oid": "a1", "size": 100}},
        {"type": "file", "path": "nested/b-Q4_K_M.gguf", "lfs": {"oid": "b2", "size": 200}}
    ])
    .to_string()
}

/// The two URLs one listing asks, in order, from the fixed endpoint.
fn listed_urls() -> Vec<String> {
    vec![
        format!("https://huggingface.co/api/models/{REPO}/refs"),
        format!("https://huggingface.co/api/models/{REPO}/tree/{COMMIT}?recursive=true"),
    ]
}

// ── Fakes ───────────────────────────────────────────────────────────────

/// A Hugging Face that records every url and whether a token rode along,
/// answers the canned `(status, body)` list in call order, and can park its
/// calls so a test can put a third one beside two in flight. A test with
/// concurrent listings sets [`FakeFetch::set_by_url`] instead: two listings
/// interleave, so one shared FIFO would hand the second listing's refs call
/// the first listing's tree answer.
/// One url's canned answers, in call order (the name keeps the field's type
/// simple enough for the linter).
type ByUrl = Arc<Mutex<HashMap<String, VecDeque<(u16, String)>>>>;

#[derive(Clone)]
struct FakeFetch {
    log: Arc<Mutex<Vec<String>>>,
    /// Whether each call carried a token (section (3): none, ever).
    tokens: Arc<Mutex<Vec<bool>>>,
    answers: Arc<Mutex<VecDeque<(u16, String)>>>,
    /// Per-url queues set by `set_by_url`; when non-empty, `get` answers
    /// from the queue of its url, never from the FIFO.
    by_url: ByUrl,
    /// Calls parked right now.
    active: Arc<AtomicUsize>,
    parking: Arc<AtomicBool>,
    /// `(released, condvar)`: parked calls wait here until the test lets go.
    park: Arc<(Mutex<bool>, Condvar)>,
    /// Urls whose `get` never answers, until `release`: the call the route
    /// gives up on at `pai_bff::HF_TIMEOUT`, so a test can watch what happens
    /// to the concurrency slot after the route walked away.
    hang: Arc<Mutex<Vec<String>>>,
    /// The next `n` `get` calls answer a transport error, nothing fetched.
    fails: Arc<AtomicUsize>,
}

impl FakeFetch {
    fn new(answers: Vec<(u16, String)>) -> FakeFetch {
        FakeFetch {
            log: Arc::new(Mutex::new(Vec::new())),
            tokens: Arc::new(Mutex::new(Vec::new())),
            answers: Arc::new(Mutex::new(answers.into())),
            by_url: Arc::new(Mutex::new(HashMap::new())),
            active: Arc::new(AtomicUsize::new(0)),
            parking: Arc::new(AtomicBool::new(false)),
            park: Arc::new((Mutex::new(false), Condvar::new())),
            hang: Arc::new(Mutex::new(Vec::new())),
            fails: Arc::new(AtomicUsize::new(0)),
        }
    }
    /// Answer each url from its own queue, in the order given: for
    /// concurrent listings, whose calls interleave.
    fn set_by_url(&self, answers: Vec<(String, u16, String)>) {
        let mut by_url = self.by_url.lock().unwrap_or_else(|e| e.into_inner());
        for (url, status, body) in answers {
            by_url.entry(url).or_default().push_back((status, body));
        }
    }
    fn record(&self, what: String, token: Option<&Token>) {
        self.log
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .push(what);
        self.tokens
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .push(token.is_some());
    }
    fn asked(&self) -> Vec<String> {
        self.log.lock().unwrap_or_else(|e| e.into_inner()).clone()
    }
    fn with_tokens(&self) -> Vec<bool> {
        self.tokens
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .clone()
    }
    fn active(&self) -> usize {
        self.active.load(Ordering::SeqCst)
    }
    /// Park every following `get` until `release`.
    fn park(&self) {
        self.parking.store(true, Ordering::SeqCst);
    }
    /// Every `get` of these urls never answers, until `release`: the call the
    /// route gives up on at `pai_bff::HF_TIMEOUT`.
    fn hang_urls(&self, urls: &[String]) {
        *self.hang.lock().unwrap_or_else(|e| e.into_inner()) = urls.to_vec();
    }
    /// The next `n` `get` calls answer a transport error, nothing fetched.
    fn fail_next(&self, n: usize) {
        self.fails.store(n, Ordering::SeqCst);
    }
    fn release(&self) {
        let (lock, cvar) = &*self.park;
        *lock.lock().unwrap_or_else(|e| e.into_inner()) = true;
        cvar.notify_all();
    }
}

impl Fetch for FakeFetch {
    fn get(&self, url: &str, token: Option<&Token>) -> Result<(u16, String), HfError> {
        self.record(url.to_string(), token);
        if self.fails.load(Ordering::SeqCst) > 0 {
            self.fails.fetch_sub(1, Ordering::SeqCst);
            return Err(HfError::Network(
                "fake: the host was never reached".to_string(),
            ));
        }
        let hung = self
            .hang
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .iter()
            .any(|u| u == url);
        if hung {
            // Waits out the route's `HF_TIMEOUT` (25 s) and no more: the call
            // ends by itself at `HANG_WAKE`, so a test that fails leaves the
            // runtime at most that far behind.
            self.active.fetch_add(1, Ordering::SeqCst);
            let (lock, cvar) = &*self.park;
            let until = Instant::now() + HANG_WAKE;
            let mut released = lock.lock().unwrap_or_else(|e| e.into_inner());
            while !*released {
                let left = until.saturating_duration_since(Instant::now());
                if left.is_zero() {
                    break;
                }
                let (guard, _timed_out) = cvar
                    .wait_timeout(released, left)
                    .unwrap_or_else(|e| e.into_inner());
                released = guard;
            }
            self.active.fetch_sub(1, Ordering::SeqCst);
            return Ok((599, "the hung call ended on its own".to_string()));
        }
        if self.parking.load(Ordering::SeqCst) {
            self.active.fetch_add(1, Ordering::SeqCst);
            let (lock, cvar) = &*self.park;
            let mut released = lock.lock().unwrap_or_else(|e| e.into_inner());
            while !*released {
                let (guard, _timed_out) = cvar
                    .wait_timeout(released, Duration::from_secs(30))
                    .unwrap_or_else(|e| e.into_inner());
                released = guard;
            }
            self.active.fetch_sub(1, Ordering::SeqCst);
        }
        let mut by_url = self.by_url.lock().unwrap_or_else(|e| e.into_inner());
        if !by_url.is_empty() {
            return Ok(by_url
                .get_mut(url)
                .and_then(|queue| queue.pop_front())
                .unwrap_or_else(|| (599, "no canned answer for url".to_string())));
        }
        drop(by_url);
        Ok(self
            .answers
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .pop_front()
            .unwrap_or_else(|| (599, "no canned answer".to_string())))
    }
    fn get_range(
        &self,
        url: &str,
        token: Option<&Token>,
        _start: u64,
        _len: u64,
    ) -> Result<(u16, Vec<u8>), HfError> {
        self.record(format!("range {url}"), token);
        Err(HfError::Network(
            "the route lists, it does not read ranges".to_string(),
        ))
    }
}

/// A manager that records every call: this route asks it nothing, so the log
/// staying empty is part of every answer.
struct FakeManager {
    log: Arc<Mutex<Vec<String>>>,
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
        Ok((true, true))
    }
    fn model_info(&self, profile: &str) -> Result<ModelInfoReply, ClientError> {
        self.record(format!("model_info {profile}"));
        Err(ClientError::Invalid(Refusal {
            key: "no-model-profile-named",
            values: vec![profile.to_string()],
        }))
    }
}

/// The authenticator: `alice` with the one password.
struct FakeAuth;

impl Authenticator for FakeAuth {
    fn authenticate(&self, user: &str, password: &str) -> Result<Identity, AuthError> {
        if password == "correct horse" && user == "alice" {
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

/// A router over a fake fetch answering `answers`, with the fake manager's
/// log and the count of connects.
struct App {
    router: Router,
    manager_log: Arc<Mutex<Vec<String>>>,
    connects: Arc<AtomicUsize>,
    fetch: FakeFetch,
}

fn app(answers: Vec<(u16, String)>) -> App {
    let manager_log = Arc::new(Mutex::new(Vec::new()));
    let connects = Arc::new(AtomicUsize::new(0));
    let (l, c) = (manager_log.clone(), connects.clone());
    let connect: Connect = Arc::new(move |_principal: Option<&str>| {
        c.fetch_add(1, Ordering::SeqCst);
        Ok(Box::new(FakeManager { log: l.clone() }) as Client)
    });
    let fetch = FakeFetch::new(answers);
    let clock = FakeClock::new();
    let router = pai_bff::router_with_session_and_hf(
        connect,
        TIMEOUT,
        Arc::new(FakeAuth),
        clock.clock(),
        REVISION_KEY,
        Arc::new(fetch.clone()) as Arc<dyn Fetch + Send + Sync>,
    );
    App {
        router,
        manager_log,
        connects,
        fetch,
    }
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

    /// An error answer: its status and `{"key", "args"}`. A route the bff
    /// does not route answers a bare 404 with an empty body, so every one of
    /// these is red, not accidentally green.
    fn assert_error(&self, status: StatusCode, key: &str, args: &[&str]) {
        assert_eq!(self.status, status, "{}", self.body);
        assert_eq!(
            self.json(),
            json!({ "key": key, "args": args }),
            "{}",
            self.body
        );
    }
}

impl App {
    fn manager_calls(&self) -> Vec<String> {
        self.manager_log
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .clone()
    }

    /// Nothing was asked beyond `base` connects (the sign-in of the same app
    /// opens one, so the caller takes the count after signing in): no further
    /// connect, no manager call, no fetch.
    fn assert_asked_nothing(&self, base: usize, what: &str) {
        assert_eq!(
            self.connects.load(Ordering::SeqCst),
            base,
            "{what}: connected"
        );
        assert!(
            self.manager_calls().is_empty(),
            "{what}: {:?}",
            self.manager_calls()
        );
        assert!(
            self.fetch.asked().is_empty(),
            "{what}: {:?}",
            self.fetch.asked()
        );
    }

    /// Sign in and return the cookie the browser sends back.
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
        request(self.router.clone(), uri, cookie).await
    }
}

/// One `GET` of `uri` through `router`, answered within `DEADLINE` or the
/// test fails. Free so a spawned task can run it on a moved router.
async fn request(router: Router, uri: &str, cookie: Option<&str>) -> Reply {
    request_within(router, uri, cookie, DEADLINE).await
}

/// One `GET` of `uri` through `router`, answered within `deadline` or the
/// test fails.
async fn request_within(
    router: Router,
    uri: &str,
    cookie: Option<&str>,
    deadline: Duration,
) -> Reply {
    let mut builder = Request::builder().method(Method::GET).uri(uri);
    if let Some(c) = cookie {
        builder = builder.header(header::COOKIE, c);
    }
    let request = builder.body(Body::empty()).expect("request");
    let response = tokio::time::timeout(deadline, router.oneshot(request))
        .await
        .unwrap_or_else(|_| panic!("no answer to {uri} within {deadline:?}"))
        .expect("infallible");
    let status = response.status();
    let bytes = tokio::time::timeout(deadline, response.into_body().collect())
        .await
        .expect("body read in time")
        .expect("body readable")
        .to_bytes();
    Reply {
        status,
        body: String::from_utf8_lossy(&bytes).to_string(),
    }
}

/// The canned good listing: refs then tree, answering `tree`.
fn good(tree: String) -> Vec<(u16, String)> {
    vec![(200, refs_json()), (200, tree)]
}

/// The canned good listing repeated `times`: `times` listings of one repo,
/// answered in call order.
fn good_times(tree: String, times: usize) -> Vec<(u16, String)> {
    let mut all = Vec::new();
    for _ in 0..times {
        all.extend(good(tree.clone()));
    }
    all
}

/// The tag of the entry a document marks `default`.
fn default_tag(doc: &Value) -> String {
    doc["files"]
        .as_array()
        .expect("files array")
        .iter()
        .find(|entry| entry["default"] == json!(true))
        .map(|entry| entry["tag"].as_str().unwrap_or_default().to_string())
        .unwrap_or_else(|| panic!("no entry is the default: {doc}"))
}

/// The body of the function at `marker`, whole-line comments dropped: the
/// text a lint over one function of the runner's source reads.
fn fn_body(src: &str, marker: &str) -> Option<String> {
    let at = src.find(marker)?;
    let open = at + src[at..].find('{')?;
    let mut depth = 0usize;
    let mut end = None;
    for (i, c) in src[open..].char_indices() {
        match c {
            '{' => depth += 1,
            '}' => {
                depth -= 1;
                if depth == 0 {
                    end = Some(open + i + 1);
                    break;
                }
            }
            _ => {}
        }
    }
    Some(
        src[open..end?]
            .lines()
            .filter(|l| !l.trim_start().starts_with("//"))
            .collect::<Vec<_>>()
            .join("\n"),
    )
}

// ── The gate and the id rule: nothing is fetched ────────────────────────

/// No session: 401 `session-required`, and nothing is fetched: an anonymous
/// visitor cannot make the node call out (section (2)).
#[tokio::test]
async fn m2_no_session_is_401_and_fetches_nothing() {
    let app = app(good(tree_unique_tags()));
    let rep = app.get(&uri(REPO), None).await;
    rep.assert_error(StatusCode::UNAUTHORIZED, "session-required", &[]);
    app.assert_asked_nothing(0, "no session");
}

/// An id `parse_spec` refuses, and no id at all, is 400 `hf-repo-invalid`
/// before any network: the browser sends a repo id, never a URL or a host.
#[tokio::test]
async fn m2_invalid_repo_is_400_and_fetches_nothing() {
    for sent in [
        "",                         // the query carries no id
        "nosep",                    // no slash
        "a/b/c",                    // two slashes
        "org/repo..x",              // a special run
        "https://evil.example/o/r", // a URL: the endpoint is fixed, not sent
        "org/repo:evil.com/x",      // a tag that is a host
    ] {
        let app = app(good(tree_unique_tags()));
        let cookie = app.sign_in().await;
        let base = app.connects.load(Ordering::SeqCst);
        let rep = app.get(&uri(sent), Some(&cookie)).await;
        assert_eq!(
            rep.status,
            StatusCode::BAD_REQUEST,
            "{sent:?}: {}",
            rep.body
        );
        let doc = rep.json();
        assert_eq!(doc["key"], "hf-repo-invalid", "{sent:?}: {}", rep.body);
        assert!(
            doc["args"].is_array(),
            "{sent:?}: the args travel as an array: {}",
            rep.body
        );
        app.assert_asked_nothing(base, &format!("invalid repo {sent:?}"));
    }
}

// ── The answer's refusals ───────────────────────────────────────────────

/// HF's 401, 403 and 404 are one refusal, 404 `hf-repo-unavailable` with the
/// repo (section (3): a gated, private or missing repo, no token sent).
#[tokio::test]
async fn m2_hf_401_403_404_map_to_hf_repo_unavailable() {
    for status in [401u16, 403, 404] {
        let app = app(vec![(status, json!({"error": "no"}).to_string())]);
        let cookie = app.sign_in().await;
        let rep = app.get(&uri(REPO), Some(&cookie)).await;
        rep.assert_error(StatusCode::NOT_FOUND, "hf-repo-unavailable", &[REPO]);
        assert_eq!(
            app.fetch.asked(),
            vec![format!("https://huggingface.co/api/models/{REPO}/refs")],
            "{status}: the refs answer ends the listing"
        );
        assert_eq!(
            app.fetch.with_tokens(),
            vec![false],
            "{status}: the first slices send no token"
        );
    }
}

/// A 3xx answers 502 `hf-repo-moved` with the repo, and no second request
/// leaves the node: the runner follows no redirect, so no request reaches a
/// host the answer names.
#[tokio::test]
async fn m2_redirect_is_hf_repo_moved_with_no_second_request() {
    let app = app(vec![(302, String::new())]);
    let cookie = app.sign_in().await;
    let rep = app.get(&uri(REPO), Some(&cookie)).await;
    rep.assert_error(StatusCode::BAD_GATEWAY, "hf-repo-moved", &[REPO]);
    assert_eq!(
        app.fetch.asked(),
        vec![format!("https://huggingface.co/api/models/{REPO}/refs")],
        "one request, no second to a host the answer names"
    );
}

// ── The cache and the concurrency bound ─────────────────────────────────

/// A listing is pinned to a commit, so a second call within 10 minutes
/// answers from the cache and fetches nothing. The first call's two URLs
/// come from the fixed endpoint, with no token.
#[tokio::test]
async fn m2_second_call_within_ten_minutes_fetches_nothing() {
    let app = app(good(tree_unique_tags()));
    let cookie = app.sign_in().await;
    let first = app.get(&uri(REPO), Some(&cookie)).await;
    assert_eq!(first.status, StatusCode::OK, "{}", first.body);
    assert_eq!(app.fetch.asked(), listed_urls(), "the fixed endpoint");
    assert_eq!(app.fetch.with_tokens(), vec![false, false], "no token");
    let second = app.get(&uri(REPO), Some(&cookie)).await;
    assert_eq!(second.status, StatusCode::OK, "{}", second.body);
    assert_eq!(second.json(), first.json(), "the cached listing");
    assert_eq!(
        app.fetch.asked(),
        listed_urls(),
        "the second call fetched nothing"
    );
}

/// The cache is keyed by the repo, the listing's `default` by the spec's tag
/// and file, so a tagged call poisons the bare repo's entry (M2 review D1).
/// `repo=o/r:Q8_0` then `repo=o/r` answers the doc a fresh `repo=o/r`
/// answers: the bare repo's default, not the tagged one's.
#[tokio::test]
async fn m2_review_d1_a_tagged_call_leaves_the_bare_repos_listing_alone() {
    // A second app first: the doc a fresh bare call answers, to compare with.
    let fresh_app = app(good(tree_unique_tags()));
    let fresh_cookie = fresh_app.sign_in().await;
    let fresh = fresh_app.get(&uri(REPO), Some(&fresh_cookie)).await;
    assert_eq!(fresh.status, StatusCode::OK, "{}", fresh.body);
    let app = app(good_times(tree_unique_tags(), 2));
    let cookie = app.sign_in().await;
    let tagged = app.get(&uri("o/r:Q8_0"), Some(&cookie)).await;
    assert_eq!(tagged.status, StatusCode::OK, "{}", tagged.body);
    assert_eq!(
        default_tag(&tagged.json()),
        "Q8_0",
        "the tag picks its own default: {}",
        tagged.body
    );
    let bare = app.get(&uri(REPO), Some(&cookie)).await;
    assert_eq!(bare.status, StatusCode::OK, "{}", bare.body);
    assert_eq!(
        default_tag(&bare.json()),
        "Q4_K_M",
        "the bare repo keeps its own default: {}",
        bare.body
    );
    assert_eq!(
        bare.json(),
        fresh.json(),
        "the bare call after a tagged one answers the fresh bare doc"
    );
    // The cache stays keyed by repo (the design's section (6)): the second
    // call of the repo asks Hugging Face nothing.
    assert_eq!(
        app.fetch.asked(),
        listed_urls(),
        "the cached repo fetched nothing"
    );
}

/// The other way round (M2 review D1): a bare listing cached for the repo
/// must not answer a later tagged call, and a tag naming no file of a repo
/// that exists is a refusal, not the cached doc.
#[tokio::test]
async fn m2_review_d1_a_bare_repos_cache_leaves_a_tagged_call_alone() {
    let app = app(good_times(tree_unique_tags(), 3));
    let cookie = app.sign_in().await;
    let bare = app.get(&uri(REPO), Some(&cookie)).await;
    assert_eq!(bare.status, StatusCode::OK, "{}", bare.body);
    assert_eq!(default_tag(&bare.json()), "Q4_K_M", "{}", bare.body);
    let tagged = app.get(&uri("o/r:Q8_0"), Some(&cookie)).await;
    assert_eq!(tagged.status, StatusCode::OK, "{}", tagged.body);
    assert_eq!(
        default_tag(&tagged.json()),
        "Q8_0",
        "the tagged call after a bare one answers its own listing: {}",
        tagged.body
    );
    let none = app.get(&uri("o/r:NOSUCH"), Some(&cookie)).await;
    none.assert_error(StatusCode::BAD_GATEWAY, "hf-bad-answer", &[]);
}

/// The cache stays a cache: the same spec twice fetches nothing the second
/// time (M2 review D1 must not cost the cache its purpose).
#[tokio::test]
async fn m2_review_d1_a_cached_spec_still_fetches_nothing_the_second_time() {
    let app = app(good(tree_unique_tags()));
    let cookie = app.sign_in().await;
    let first = app.get(&uri("o/r:Q8_0"), Some(&cookie)).await;
    assert_eq!(first.status, StatusCode::OK, "{}", first.body);
    let second = app.get(&uri("o/r:Q8_0"), Some(&cookie)).await;
    assert_eq!(second.status, StatusCode::OK, "{}", second.body);
    assert_eq!(second.json(), first.json(), "the cached listing");
    assert_eq!(
        app.fetch.asked(),
        listed_urls(),
        "the second call fetched nothing"
    );
}

/// At most two HF calls run at once: with two listings in flight, a third
/// concurrent call is 429 `hf-busy`; the two in flight finish.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn m2_third_concurrent_call_is_429_hf_busy() {
    // The answers are keyed by url: the two listings interleave, and one
    // shared FIFO would hand the second listing's refs call the first
    // listing's tree answer.
    let app = app(Vec::new());
    app.fetch.set_by_url(vec![
        (listed_urls()[0].clone(), 200, refs_json()),
        (listed_urls()[0].clone(), 200, refs_json()),
        (listed_urls()[1].clone(), 200, tree_unique_tags()),
        (listed_urls()[1].clone(), 200, tree_unique_tags()),
    ]);
    let cookie = app.sign_in().await;
    app.fetch.park();
    let (r1, r2) = (app.router.clone(), app.router.clone());
    let (c1, c2) = (cookie.clone(), cookie.clone());
    let h1 = tokio::spawn(async move { request(r1, &uri(REPO), Some(&c1)).await });
    let h2 = tokio::spawn(async move { request(r2, &uri(REPO), Some(&c2)).await });
    let started = Instant::now();
    while app.fetch.active() < 2 && started.elapsed() < DEADLINE {
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
    assert_eq!(app.fetch.active(), 2, "two listings in flight");
    let third = app.get(&uri(REPO), Some(&cookie)).await;
    third.assert_error(StatusCode::TOO_MANY_REQUESTS, "hf-busy", &[]);
    app.fetch.release();
    for h in [h1, h2] {
        let rep = tokio::time::timeout(DEADLINE, h)
            .await
            .expect("in flight answered")
            .expect("task");
        assert_eq!(rep.status, StatusCode::OK, "{}", rep.body);
    }
    assert_eq!(
        app.fetch.asked().len(),
        4,
        "the refused call fetched nothing"
    );
}

// ── The slot comes back after a failure (M2 review G1) ──────────────────

/// A listing that fails gives its slot back: two listings that end in a
/// transport error, then two more that must not answer 429 `hf-busy`.
#[tokio::test]
async fn m2_review_g1_a_failed_listing_gives_its_slot_back() {
    let app = app(good_times(tree_unique_tags(), 2));
    let cookie = app.sign_in().await;
    app.fetch.fail_next(2);
    for repo in [REPO, "o/next0"] {
        let rep = app.get(&uri(repo), Some(&cookie)).await;
        rep.assert_error(StatusCode::GATEWAY_TIMEOUT, "hf-unreachable", &[]);
    }
    for repo in ["o/next1", "o/next2"] {
        let rep = app.get(&uri(repo), Some(&cookie)).await;
        assert_ne!(
            rep.status,
            StatusCode::TOO_MANY_REQUESTS,
            "the slots of two failed listings came back: {}",
            rep.body
        );
        assert_eq!(rep.status, StatusCode::OK, "{}", rep.body);
    }
}

/// A listing the route gave up on gives its slot back too: the fetch never
/// answers, the route walks away at `HF_TIMEOUT` with 504, and the two
/// listings it starts afterwards must not answer 429 `hf-busy`. The fetch
/// stays hung over the whole test, so a slot still held by the abandoned
/// listing shows.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn m2_review_g1_a_listing_the_route_gave_up_on_gives_its_slot_back() {
    let app = app(good_times(tree_unique_tags(), 2));
    app.fetch.hang_urls(&listed_urls());
    let cookie = app.sign_in().await;
    let (r1, r2) = (app.router.clone(), app.router.clone());
    let (c1, c2) = (cookie.clone(), cookie.clone());
    let h1 =
        tokio::spawn(async move { request_within(r1, &uri(REPO), Some(&c1), HF_DEADLINE).await });
    let h2 =
        tokio::spawn(async move { request_within(r2, &uri(REPO), Some(&c2), HF_DEADLINE).await });
    let started = Instant::now();
    while app.fetch.active() < 2 && started.elapsed() < HF_DEADLINE {
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
    assert_eq!(app.fetch.active(), 2, "two listings in flight");
    for h in [h1, h2] {
        let rep = tokio::time::timeout(HF_DEADLINE, h)
            .await
            .expect("the hung listing was given up on")
            .expect("task");
        rep.assert_error(StatusCode::GATEWAY_TIMEOUT, "hf-unreachable", &[]);
    }
    for repo in ["o/next1", "o/next2"] {
        let rep = app.get(&uri(repo), Some(&cookie)).await;
        assert_ne!(
            rep.status,
            StatusCode::TOO_MANY_REQUESTS,
            "the slot of a listing the route gave up on is back: {}",
            rep.body
        );
        assert_eq!(rep.status, StatusCode::OK, "{}", rep.body);
    }
    app.fetch.release();
}

// ── The reply's shape ───────────────────────────────────────────────────

/// The design's document for a repo with one file per tag: `source`, `repo`,
/// `revision`, and one entry per candidate with its tag, file, shards, size,
/// sha256, sidecars, value and default. `value` carries `hf-repo` with the
/// tag and no `hf-file`: the tag names one file.
#[tokio::test]
async fn m2_the_reply_is_the_designs_document_and_value_carries_no_hf_file() {
    let app = app(good(tree_unique_tags()));
    let cookie = app.sign_in().await;
    let rep = app.get(&uri(REPO), Some(&cookie)).await;
    assert_eq!(rep.status, StatusCode::OK, "{}", rep.body);
    assert_eq!(
        rep.json(),
        json!({
            "source": "hf",
            "repo": REPO,
            "revision": COMMIT,
            "files": [
                {
                    "tag": "Q4_K_M",
                    "file": "model-Q4_K_M.gguf",
                    "shards": 1,
                    "size": 1000,
                    "sha256": "a1a1a1a1a1a1a1a1a1a1a1a1a1a1a1a1a1a1a1a1",
                    "sidecars": {"mmproj": "model-Q4_K_M-mmproj.gguf"},
                    "value": {"hf-repo": "o/r:Q4_K_M"},
                    "default": true
                },
                {
                    "tag": "Q8_0",
                    "file": "model-Q8_0.gguf",
                    "shards": 1,
                    "size": 2000,
                    "sha256": "b2b2b2b2b2b2b2b2b2b2b2b2b2b2b2b2b2b2b2b2",
                    "sidecars": {"mmproj": "model-Q4_K_M-mmproj.gguf"},
                    "value": {"hf-repo": "o/r:Q8_0"},
                    "default": false
                }
            ]
        }),
        "{}",
        rep.body
    );
}

/// Two files share the tag `Q4_K_M`: the tag alone names no file, so each
/// `value` carries `hf-file` beside `hf-repo`, naming that entry's file.
#[tokio::test]
async fn m2_value_carries_hf_file_only_for_an_ambiguous_tag() {
    let app = app(good(tree_ambiguous_tag()));
    let cookie = app.sign_in().await;
    let rep = app.get(&uri(REPO), Some(&cookie)).await;
    assert_eq!(rep.status, StatusCode::OK, "{}", rep.body);
    let doc = rep.json();
    let files = doc["files"].as_array().expect("files array");
    assert_eq!(files.len(), 2, "{}", rep.body);
    for entry in files {
        let file = entry["file"].as_str().expect("file");
        assert_eq!(
            entry["value"],
            json!({"hf-repo": "o/r:Q4_K_M", "hf-file": file}),
            "the tag names no file, so the value names it: {}",
            rep.body
        );
    }
}

// ── The runner (the M1 review's carries) ────────────────────────────────

/// The bff's own curl runner builds its argv with `-q` first (no
/// `~/.curlrc`), `-sS`, the config on stdin (`-K -`), and never `-L`: a
/// redirect would send a request to a host the answer names.
#[test]
fn m2_the_runner_argv_has_q_first_and_never_location() {
    let argv = pai_bff::hf::curl_argv();
    assert_eq!(
        argv.first().map(String::as_str),
        Some("-q"),
        "the first argument disables the config file: {argv:?}"
    );
    assert!(
        !argv.iter().any(|a| a == "-L" || a == "--location"),
        "no redirect following: {argv:?}"
    );
    assert!(
        argv.windows(2).any(|w| w == ["-K", "-"]),
        "the config rides stdin: {argv:?}"
    );
    assert!(argv.iter().any(|a| a == "-sS"), "quiet: {argv:?}");
}

/// The argv `Runner::run` passes to the process is exactly `curl_argv()`: the
/// command it spawns is built by the seam `runner_command`, whose argv the
/// test reads (M2 review G2). A `-L` added to the seam reddens this; a `-L`
/// added inside `run` is caught by the seam test below.
#[test]
fn m2_review_g2_the_runner_passes_the_pinned_argv_to_the_process() {
    let cmd = pai_bff::hf::runner_command();
    let args: Vec<String> = cmd
        .get_args()
        .map(|a| a.to_string_lossy().into_owned())
        .collect();
    assert_eq!(
        args,
        pai_bff::hf::curl_argv(),
        "the argv reaching the process is the pinned one"
    );
    assert_eq!(
        cmd.get_program().to_string_lossy(),
        "curl",
        "the runner runs curl"
    );
}

/// `Runner::run` builds no command and adds no argument of its own: it spawns
/// what the seam builds, so the argv the test reads is the argv the child
/// gets (M2 review G2).
#[test]
fn m2_review_g2_the_runner_builds_its_command_through_the_seam() {
    let src =
        std::fs::read_to_string(std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src/hf.rs"))
            .expect("the runner's source");
    let body = fn_body(&src, "fn run(").expect("Runner::run is here");
    assert!(
        body.contains("runner_command"),
        "run spawns the command the seam builds: {body}"
    );
    assert!(
        !body.contains("Command::new"),
        "run builds no command of its own: {body}"
    );
    assert!(
        !body.contains(".arg"),
        "run adds no argument of its own: {body}"
    );
}

/// The runner's stdin config is the bff profile of `bff_curl_config`: fixed
/// endpoint, `proto =https`, no `location`, the bounds of section (2).
#[test]
fn m2_the_runner_uses_the_bff_profile_config() {
    let url = "https://huggingface.co/api/models/o/r/refs";
    let cfg = pai_bff::hf::curl_config(url).expect("the config builds");
    assert_eq!(
        cfg,
        pai_core::hf::bff_curl_config(url, None).expect("the bff profile"),
        "the runner sends the bff profile, nothing else"
    );
    assert!(!cfg.contains("location"), "no redirect following: {cfg}");
}

/// The bff builds every spec through `parse_spec` (which holds the id to
/// `valid_repo_id`): no `RepoSpec` is built by hand in the bff's source.
#[test]
fn m2_the_bff_builds_every_spec_through_parse_spec() {
    let src = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
    let mut text = String::new();
    for entry in std::fs::read_dir(&src).expect("the bff's src") {
        let path = entry.expect("entry").path();
        if path.extension().and_then(|e| e.to_str()) == Some("rs") {
            let body = std::fs::read_to_string(&path).expect("source readable");
            text.push_str(
                &body
                    .lines()
                    .filter(|l| !l.trim_start().starts_with("//"))
                    .collect::<Vec<_>>()
                    .join("\n"),
            );
            text.push('\n');
        }
    }
    assert!(
        text.contains("parse_spec"),
        "the bff builds its spec through parse_spec"
    );
    assert!(
        !text.contains("RepoSpec {"),
        "no spec built by hand: parse_spec holds the id rule"
    );
}
