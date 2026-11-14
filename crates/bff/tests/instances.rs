//! `GET /api/instances`: the manager's instances and the state of each,
//! through `pai_bff::router_with` and a fake manager client, never the real
//! system bus or pipe. Each test hands the router one request
//! (`tower::ServiceExt::oneshot`) under a deadline, so a bff that waits on the
//! manager fails instead of hanging.
//!
//! The answer: 200 and `[{"id": ..., "state": ...}]` in the manager's order,
//! the state as the manager names it; 503 `{"error": "unavailable"}` when no
//! manager answers (none, gone, or too slow); 502 `{"error": "not-authorized"}`
//! when the manager refuses the bff, 502 `{"error": "failed"}` on any other
//! failure, never the manager's detail.

use axum::body::Body;
use axum::http::{header, Method, Request, StatusCode};
use http_body_util::BodyExt;
use pai_bff::{Client, Connect};
use pai_core::client::{ClientError, InstanceState, ManagerClient};
use serde_json::{json, Value};
use std::collections::HashMap;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};
use tower::ServiceExt;

/// How long the bff waits for the manager in these tests.
const TIMEOUT: Duration = Duration::from_millis(200);

/// How long a test waits for the bff's answer before failing.
const DEADLINE: Duration = Duration::from_secs(5);

type Log = Arc<Mutex<Vec<String>>>;

/// A manager that answers from its fields and logs every call
/// (`list`, `status <id>`, `start <id>`, ...).
struct FakeManager {
    instances: Result<Vec<String>, ClientError>,
    states: HashMap<String, Result<InstanceState, ClientError>>,
    /// How long `list_instances` takes.
    delay: Duration,
    log: Log,
}

impl FakeManager {
    fn new(log: &Log, instances: &[(&str, InstanceState)]) -> FakeManager {
        FakeManager {
            instances: Ok(instances.iter().map(|(id, _)| id.to_string()).collect()),
            states: instances
                .iter()
                .map(|(id, s)| (id.to_string(), Ok(s.clone())))
                .collect(),
            delay: Duration::ZERO,
            log: log.clone(),
        }
    }

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
        std::thread::sleep(self.delay);
        self.instances.clone()
    }
    fn status(&self, id: &str) -> Result<InstanceState, ClientError> {
        self.record(format!("status {id}"));
        self.states
            .get(id)
            .cloned()
            .unwrap_or(Err(ClientError::NoInstance))
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
    fn read_file(&self, name: &str) -> Result<Option<String>, ClientError> {
        self.record(format!("read {name}"));
        Ok(None)
    }
    fn write_file(&self, name: &str, _contents: &str) -> Result<(), ClientError> {
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
}

/// A connect that hands out `make()` each time and counts the calls.
fn connect_with(
    count: &Arc<AtomicUsize>,
    make: impl Fn() -> Result<Client, ClientError> + Send + Sync + 'static,
) -> Connect {
    let count = count.clone();
    Arc::new(move || {
        count.fetch_add(1, Ordering::SeqCst);
        make()
    })
}

/// A connect to one fake manager, built afresh for each call by `make`.
fn connect_to(make: impl Fn() -> FakeManager + Send + Sync + 'static) -> Connect {
    connect_with(&Arc::new(AtomicUsize::new(0)), move || {
        Ok(Box::new(make()) as Client)
    })
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

    /// An error answer: its status and `{"error": key}` as JSON, nothing else.
    fn assert_error(&self, status: StatusCode, key: &str) {
        assert_eq!(self.status, status, "{}", self.body);
        assert!(
            self.content_type.starts_with("application/json"),
            "content-type {:?}",
            self.content_type
        );
        assert_eq!(self.json(), json!({ "error": key }), "{}", self.body);
    }
}

async fn send_to(router: axum::Router, method: Method, uri: &str) -> Reply {
    let request = Request::builder()
        .method(method)
        .uri(uri)
        .body(Body::empty())
        .expect("request");
    let response = tokio::time::timeout(DEADLINE, router.oneshot(request))
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

async fn get_instances(connect: Connect) -> Reply {
    send_to(
        pai_bff::router_with(connect, TIMEOUT),
        Method::GET,
        "/api/instances",
    )
    .await
}

fn calls(log: &Log) -> Vec<String> {
    log.lock().unwrap_or_else(|e| e.into_inner()).clone()
}

// ── Failure paths first ─────────────────────────────────────────────────

/// No manager to connect to (not running, no bus, no pipe): 503 with the
/// key the web UI shows its message for.
#[tokio::test]
async fn no_manager_is_unavailable() {
    let count = Arc::new(AtomicUsize::new(0));
    let r = get_instances(connect_with(&count, || Err(ClientError::Unavailable))).await;
    r.assert_error(StatusCode::SERVICE_UNAVAILABLE, "unavailable");
    assert_eq!(count.load(Ordering::SeqCst), 1, "connected once");
}

/// A manager that goes away during the call (the connection broke) is the
/// same: unavailable.
#[tokio::test]
async fn a_manager_gone_during_the_call_is_unavailable() {
    let log = Log::default();
    let l = log.clone();
    let r = get_instances(connect_to(move || {
        let mut m = FakeManager::new(&l, &[]);
        m.instances = Err(ClientError::Unavailable);
        m
    }))
    .await;
    r.assert_error(StatusCode::SERVICE_UNAVAILABLE, "unavailable");
    assert_eq!(calls(&log), ["list"]);
}

/// A manager that does not answer within the timeout: unavailable, answered
/// at the timeout, never a hang (the slow call goes on off the request).
#[tokio::test]
async fn a_manager_that_does_not_answer_in_time_is_unavailable() {
    let log = Log::default();
    let l = log.clone();
    let started = Instant::now();
    let r = get_instances(connect_to(move || {
        let mut m = FakeManager::new(&l, &[("default", InstanceState::Active)]);
        m.delay = Duration::from_secs(2);
        m
    }))
    .await;
    let took = started.elapsed();
    r.assert_error(StatusCode::SERVICE_UNAVAILABLE, "unavailable");
    assert!(
        took < Duration::from_secs(1),
        "answered after {took:?}, not at the {TIMEOUT:?} timeout"
    );
}

/// Connecting itself hangs (a busy pipe, a bus that does not answer): the
/// timeout covers the connect too.
#[tokio::test]
async fn a_connect_that_hangs_is_unavailable() {
    let count = Arc::new(AtomicUsize::new(0));
    let started = Instant::now();
    let r = get_instances(connect_with(&count, || {
        std::thread::sleep(Duration::from_secs(2));
        Err(ClientError::Unavailable)
    }))
    .await;
    let took = started.elapsed();
    r.assert_error(StatusCode::SERVICE_UNAVAILABLE, "unavailable");
    assert!(
        took < Duration::from_secs(1),
        "answered after {took:?}, not at the {TIMEOUT:?} timeout"
    );
}

/// The manager refuses the bff (the pipe's security descriptor or the bus
/// policy leaves its account out): 502 not-authorized, so the page can say
/// it is the service's access, not the manager's absence.
#[tokio::test]
async fn a_refusal_by_the_manager_is_not_authorized() {
    let r = get_instances(connect_with(&Arc::new(AtomicUsize::new(0)), || {
        Err(ClientError::NotAuthorized)
    }))
    .await;
    r.assert_error(StatusCode::BAD_GATEWAY, "not-authorized");
}

/// Any other failure is 502 failed, and the manager's detail stays in the
/// bff: the browser gets only the key.
#[tokio::test]
async fn any_other_failure_is_failed_without_the_detail() {
    let log = Log::default();
    let l = log.clone();
    let r = get_instances(connect_to(move || {
        let mut m = FakeManager::new(&l, &[]);
        m.instances = Err(ClientError::Other(
            "org.freedesktop.DBus.Error.AccessDenied: secret-detail".into(),
        ));
        m
    }))
    .await;
    r.assert_error(StatusCode::BAD_GATEWAY, "failed");
    assert!(!r.body.contains("secret-detail"), "{}", r.body);
}

/// One instance whose state cannot be read (the service manager's error)
/// fails the whole answer: no list with a hole in it.
#[tokio::test]
async fn a_state_that_cannot_be_read_fails_the_answer() {
    let log = Log::default();
    let l = log.clone();
    let r = get_instances(connect_to(move || {
        let mut m = FakeManager::new(
            &l,
            &[
                ("default", InstanceState::Active),
                ("coder", InstanceState::Inactive),
            ],
        );
        m.states.insert(
            "coder".into(),
            Err(ClientError::Service(
                "Unit llama.cpp@coder.service not loaded.".into(),
            )),
        );
        m
    }))
    .await;
    r.assert_error(StatusCode::BAD_GATEWAY, "failed");
    assert!(!r.body.contains("llama.cpp@coder"), "{}", r.body);
}

/// An instance removed between the list and its status (NoInstance) is left
/// out: it no longer exists, the others are still answered.
#[tokio::test]
async fn an_instance_removed_meanwhile_is_left_out() {
    let log = Log::default();
    let l = log.clone();
    let r = get_instances(connect_to(move || {
        let mut m = FakeManager::new(
            &l,
            &[
                ("default", InstanceState::Active),
                ("gone", InstanceState::Inactive),
            ],
        );
        m.states.insert("gone".into(), Err(ClientError::NoInstance));
        m
    }))
    .await;
    assert_eq!(r.status, StatusCode::OK, "{}", r.body);
    assert_eq!(r.json(), json!([{ "id": "default", "state": "active" }]));
}

/// The route is read-only: another method is 405 and asks the manager
/// nothing (not even a connection).
#[tokio::test]
async fn instances_refuses_another_method_without_asking_the_manager() {
    for method in [Method::POST, Method::PUT, Method::DELETE] {
        let count = Arc::new(AtomicUsize::new(0));
        let log = Log::default();
        let l = log.clone();
        let connect = connect_with(&count, move || {
            Ok(Box::new(FakeManager::new(&l, &[("default", InstanceState::Active)])) as Client)
        });
        let r = send_to(
            pai_bff::router_with(connect, TIMEOUT),
            method.clone(),
            "/api/instances",
        )
        .await;
        assert_eq!(
            r.status,
            StatusCode::METHOD_NOT_ALLOWED,
            "{method}: {}",
            r.body
        );
        assert_eq!(count.load(Ordering::SeqCst), 0, "{method}: connected");
        assert!(calls(&log).is_empty(), "{method}: {:?}", calls(&log));
    }
}

// ── What it answers ─────────────────────────────────────────────────────

/// No instances: 200 and an empty list (the page's empty state), not an
/// error.
#[tokio::test]
async fn no_instances_is_an_empty_list() {
    let log = Log::default();
    let l = log.clone();
    let r = get_instances(connect_to(move || FakeManager::new(&l, &[]))).await;
    assert_eq!(r.status, StatusCode::OK, "{}", r.body);
    assert!(
        r.content_type.starts_with("application/json"),
        "content-type {:?}",
        r.content_type
    );
    assert_eq!(r.json(), json!([]));
}

/// Each instance with its state, in the order the manager lists them, the
/// state named as the manager names it (an unknown name passed on as is).
#[tokio::test]
async fn each_instance_with_its_state_in_the_manager_s_order() {
    let log = Log::default();
    let l = log.clone();
    let r = get_instances(connect_to(move || {
        FakeManager::new(
            &l,
            &[
                ("default", InstanceState::Active),
                ("coder", InstanceState::Failed),
                ("big-model", InstanceState::Inactive),
                ("a", InstanceState::Activating),
                ("z", InstanceState::Deactivating),
                ("odd", InstanceState::Other("maintenance".into())),
            ],
        )
    }))
    .await;
    assert_eq!(r.status, StatusCode::OK, "{}", r.body);
    assert_eq!(
        r.json(),
        json!([
            { "id": "default", "state": "active" },
            { "id": "coder", "state": "failed" },
            { "id": "big-model", "state": "inactive" },
            { "id": "a", "state": "activating" },
            { "id": "z", "state": "deactivating" },
            { "id": "odd", "state": "maintenance" },
        ])
    );
}

/// Reading the instances asks only the calls the manager answers without
/// authorization: the list, then each status. Nothing starts, stops or
/// reads a file.
#[tokio::test]
async fn reading_the_instances_asks_only_the_read_only_calls() {
    let log = Log::default();
    let l = log.clone();
    let r = get_instances(connect_to(move || {
        FakeManager::new(
            &l,
            &[
                ("default", InstanceState::Active),
                ("coder", InstanceState::Inactive),
            ],
        )
    }))
    .await;
    assert_eq!(r.status, StatusCode::OK, "{}", r.body);
    assert_eq!(calls(&log), ["list", "status default", "status coder"]);
}

/// A manager that starts after the bff is seen at the next request: the
/// bff asks it anew each time, it does not keep the first failure.
#[tokio::test]
async fn a_manager_started_later_is_seen_at_the_next_request() {
    let count = Arc::new(AtomicUsize::new(0));
    let log = Log::default();
    let (c, l) = (count.clone(), log.clone());
    let connect = connect_with(&count, move || {
        // `count` was already raised for this call: the first is 1.
        if c.load(Ordering::SeqCst) == 1 {
            Err(ClientError::Unavailable)
        } else {
            Ok(Box::new(FakeManager::new(&l, &[("default", InstanceState::Active)])) as Client)
        }
    });
    let router = pai_bff::router_with(connect, TIMEOUT);
    let first = send_to(router.clone(), Method::GET, "/api/instances").await;
    first.assert_error(StatusCode::SERVICE_UNAVAILABLE, "unavailable");
    let second = send_to(router, Method::GET, "/api/instances").await;
    assert_eq!(second.status, StatusCode::OK, "{}", second.body);
    assert_eq!(
        second.json(),
        json!([{ "id": "default", "state": "active" }])
    );
    assert_eq!(count.load(Ordering::SeqCst), 2);
}

/// The other routes are unchanged under `router_with`.
#[tokio::test]
async fn version_is_still_served_beside_the_instances() {
    let r = send_to(
        pai_bff::router_with(
            connect_with(&Arc::new(AtomicUsize::new(0)), || {
                Err(ClientError::Unavailable)
            }),
            TIMEOUT,
        ),
        Method::GET,
        "/api/version",
    )
    .await;
    assert_eq!(r.status, StatusCode::OK, "{}", r.body);
    assert_eq!(r.json(), json!({ "version": env!("CARGO_PKG_VERSION") }));
}
