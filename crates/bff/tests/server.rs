//! `GET /api/instances/{id}/server` and `PUT /api/instances/{id}/server`:
//! an instance's `server@<id>.ini` read and saved through the manager, for a
//! signed-in user, through `pai_bff::router_with_session` with a fake manager
//! client and a fake authenticator (never the real bus, pipe, PAM or
//! LogonUser). Each request runs under a deadline, so a bff that waits on the
//! manager fails instead of hanging.
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
//!     (fe/src/ServerForm.tsx), `null` when unset. `RocmHostMemoryGtt` (GT1) is
//!     `true`/`false` and `null` when unset, which is the ON state of the
//!     launch: the bff sends no default, the launch decides.
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
//! - The `PUT` of the form (`{"revision", "values"}`): the bff re-reads the
//!   file as the session's user, answers 409 `server-changed-elsewhere` when
//!   the body's revision is no longer the file's, runs `server_cfg::check`
//!   with this machine's CPU count, and hands the manager the whole file as
//!   `server_cfg::render_for` of the values through the compare and swap,
//!   expecting the text it read. 200 is `{"revision", "running"}`: the new
//!   file's revision, and the state of the instance read after the write
//!   (`true` also when that read fails, `false` for `inactive` and the
//!   other not-running states). 400 `server-malformed` for a body that is
//!   not the form's shape, 422 `{"key", "args", "field"}` for a refused
//!   value (the bff's own check, the manager's, `client-address-in-use`
//!   marking `Port` and `client-unresolvable-host` marking `Hostname`),
//!   404 `client-no-instance`, 502 `client-other`, and the two answers of a
//!   manager that does not answer the save: 503 `client-unavailable` for as
//!   long as it has not been asked to write (it was never reached, it refuses
//!   at the read, or the time limit runs out on the connect or on the read),
//!   504 `client-timeout` while the write was asked and its outcome is
//!   unknown (the write never comes back, or the stream dies with the
//!   manager at the write: the client reports that as `Unavailable` too, and
//!   the route must not read it as a manager that was never reached). Once the
//!   write has landed the save is done: a time limit that runs out on the
//!   status read that follows it answers the 200 with `running` true, because
//!   a 504 there tells the user to look at the server and decide whether the
//!   file is saved, when the bff knows it is. A save the time limit cut off at
//!   the connect or at the read ends with that 503: the blocking task the
//!   route left behind must not write, so the file keeps the text the form
//!   read.
//! - The manager's calls of a write run under the write's own time limit,
//!   longer than the one a read is given: a manager that is busy writing is
//!   not cut off at the limit that is right for a read (the reads keep theirs,
//!   `router_with_session_and_write_timeout` is the seam a test hands its own
//!   pair of limits through). A blank `ModelsDir` or `StateDir` is left unset
//!   in the file; the key the body names is written and never sent in an
//!   answer.
//! - GT1: `RocmHostMemoryGtt` is a switch WITH an unset state: the read sends
//!   `true`/`false`/`null` (unset is the ON state of the launch, so the bff
//!   sends no default), the save writes the key line for a bool and leaves the
//!   commented hint for `null`, and a value the manager's bool rule refuses is
//!   422 with `field` the key.
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
use pai_core::checks::Refusal;
use pai_core::client::{Affected, ClientError, InstanceState, ManagerClient, ManagerFile};
use serde_json::{json, Value};
use std::collections::HashMap;
use std::net::SocketAddr;
use std::sync::atomic::{AtomicBool, AtomicU64, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};
use tower::ServiceExt;

/// How long the bff waits for the manager in these tests.
const TIMEOUT: Duration = Duration::from_millis(200);

/// How long a test waits for the bff's answer before failing.
const DEADLINE: Duration = Duration::from_secs(5);

/// How long a test waits, after the manager call it stopped has returned, for a
/// blocking task that was not cut off to reach its write.
const GRACE: Duration = Duration::from_millis(500);

/// How long a stopped call in the fake holds at most. The test releases it; a
/// gate nobody releases lets the call through, so a test fails, it does not
/// hang.
const GATE_LIMIT: Duration = Duration::from_secs(5);

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
const KEYS: [&str; 22] = [
    "Port",
    "Hostname",
    "Threads",
    "ThreadsBatch",
    "ModelsMax",
    "ModelsMemoryMargin",
    "Devices",
    "ModelsDir",
    "RocblasUseHipblaslt",
    "RocmHostMemoryGtt",
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
    "Tags",
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
    live: Live,
}

/// What a `write_file_if` answers when the test asks it to fail: the manager's
/// own failures, the ones a save can meet between reading and writing.
#[derive(Clone)]
enum Outcome {
    /// The compare and swap: the file no longer holds what the bff read.
    Changed,
    NoInstance,
    Unavailable,
    Other,
    Invalid {
        key: &'static str,
        values: Vec<String>,
    },
    AddressInUse(String),
    UnresolvableHost(String),
}

impl Outcome {
    fn into_error(self) -> ClientError {
        match self {
            Outcome::Changed => ClientError::Changed,
            Outcome::NoInstance => ClientError::NoInstance,
            Outcome::Unavailable => ClientError::Unavailable,
            Outcome::Other => ClientError::Other("the manager failed".into()),
            Outcome::Invalid { key, values } => ClientError::Invalid(Refusal { key, values }),
            Outcome::AddressInUse(id) => ClientError::AddressInUse(id),
            Outcome::UnresolvableHost(host) => ClientError::UnresolvableHost(host),
        }
    }
}

/// The files a manager keeps live for the write tests: the current text of
/// each, the writes it was asked for (`name`, `contents`, `expected`), the
/// principals whose write it refuses (the member of `pai` removed between the
/// read and the write), the principals whose read it refuses (a read refused
/// for a signed-in user, before the route reaches the write), the failure to
/// answer a write with, how long a write takes, and whether the instance's
/// process runs. Shared by every connect of one app, so a test can read a
/// file, change it behind the form, refuse a call, and save.
///
/// The connect and the read take the same two cells as the write: the failure
/// to answer them with, and how long they take. The 503 and 504 of a save
/// turn on which call was reached, so a test has to be able to stop the save
/// inside the connect, inside the read, and inside the write.
#[derive(Clone, Default)]
struct Live {
    files: Arc<Mutex<HashMap<String, String>>>,
    writes: Arc<Mutex<Vec<(String, String, String)>>>,
    write_refused: Arc<Mutex<Vec<String>>>,
    read_refused: Arc<Mutex<Vec<String>>>,
    write_outcome: Arc<Mutex<Option<Outcome>>>,
    write_pause_ms: Arc<AtomicU64>,
    read_outcome: Arc<Mutex<Option<Outcome>>>,
    read_pause_ms: Arc<AtomicU64>,
    /// A read stopped inside the manager call until the test releases it:
    /// `read_gate_armed` stops the next read, `read_gate_released` lets it go,
    /// `reads_gated` counts the reads that stopped and `reads_released` those
    /// the test let go (see `gate`).
    read_gate_armed: Arc<AtomicBool>,
    read_gate_released: Arc<AtomicBool>,
    reads_gated: Arc<AtomicUsize>,
    reads_released: Arc<AtomicUsize>,
    connect_outcome: Arc<Mutex<Option<Outcome>>>,
    connect_pause_ms: Arc<AtomicU64>,
    /// The same gate for the connect.
    connect_gate_armed: Arc<AtomicBool>,
    connect_gate_released: Arc<AtomicBool>,
    connects_gated: Arc<AtomicUsize>,
    connects_released: Arc<AtomicUsize>,
    status_outcome: Arc<Mutex<Option<Outcome>>>,
    status_pause_ms: Arc<AtomicU64>,
    running: Arc<Mutex<bool>>,
    /// The ids the record holds, for `list_instances`: empty is the one
    /// instance `default`, every app's answer today.
    instances: Arc<Mutex<Vec<String>>>,
    /// The server profile each id's section names, for `Instances`: an id
    /// missing here names the profile of its own id, and `None` is a section
    /// that names no profile at all.
    server_profiles: Arc<Mutex<HashMap<String, Option<String>>>>,
    /// The failure to answer `Instances` with, when set: the record the bff
    /// cannot read.
    instances_outcome: Arc<Mutex<Option<Outcome>>>,
    /// The node's inventory R1 answers beside the device list, `(ram_mib,
    /// cpu_threads)` (ARCHITECTURE.md (1) "The node inventory"). `None` (the
    /// default) is a manager that answers the read no answer at all.
    devices: Arc<Mutex<Option<(u64, u32)>>>,
}

/// A call a test stops: armed for the next one, it holds there until the test
/// releases it, counting the calls that stopped in `gated` and the ones the
/// test let go in `released_calls`. This is how a test stops the save inside
/// the connect or inside the read with no fixed sleep: the route answers past
/// its time limit, the test then lets the parked call return and watches
/// whether the task goes on to write. A call holds at most `GATE_LIMIT`, so a
/// gate nobody releases lets the call through.
///
/// The save runs its connect, read and write in one `spawn_blocking` task, so a
/// parked call holds a blocking-pool thread and not the route; a route parking
/// a call on the async path (the session routes) would be stopped by it,
/// because `#[tokio::test]` runs the test on a single thread.
fn gate(
    armed: &AtomicBool,
    released: &AtomicBool,
    gated: &AtomicUsize,
    released_calls: &AtomicUsize,
) {
    if !armed.swap(false, Ordering::SeqCst) {
        return;
    }
    gated.fetch_add(1, Ordering::SeqCst);
    let deadline = Instant::now() + GATE_LIMIT;
    while !released.load(Ordering::SeqCst) && Instant::now() < deadline {
        std::thread::sleep(Duration::from_millis(5));
    }
    released_calls.fetch_add(1, Ordering::SeqCst);
}

/// The connect as the fake answers it: it takes the principal, stops for
/// `Live::connect_pause_ms`, and refuses with `Live::connect_outcome` when one is set. Both
/// cells are inert by default, so every test that does not touch them is untouched.
///
/// Set them from a test of the save only. The save runs its connect, read and write in one
/// blocking task, so a sleeping connect stops that task and not the route; a route that
/// connects on the async path (the session routes) would be stopped by a sleeping connect,
/// because `#[tokio::test]` runs the test on a single thread.
fn connect_hook(live: &Live) -> Result<(), ClientError> {
    gate(
        &live.connect_gate_armed,
        &live.connect_gate_released,
        &live.connects_gated,
        &live.connects_released,
    );
    let pause = live.connect_pause_ms.load(Ordering::SeqCst);
    if pause > 0 {
        std::thread::sleep(Duration::from_millis(pause));
    }
    let outcome = live
        .connect_outcome
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .clone();
    match outcome {
        Some(outcome) => Err(outcome.into_error()),
        None => Ok(()),
    }
}

impl FakeManager {
    fn record(&self, call: String) {
        self.log
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .push(call);
    }

    /// The refusal of the principal this connect was handed, the kind every
    /// principal refusal carries: the user is no longer a member of `pai`.
    /// A principal in `Live::read_refused` is refused the reads too, which is
    /// a refusal of a member who is still a member.
    fn refused(&self) -> bool {
        if self
            .refuse
            .iter()
            .any(|u| Some(u.as_str()) == self.as_user.as_deref())
        {
            return true;
        }
        let principal = self.as_user.clone().unwrap_or_default();
        self.live
            .read_refused
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .contains(&principal)
    }
}

impl ManagerClient for FakeManager {
    /// The node's inventory beside the device list (R1): what the test put in
    /// `live.devices`. With none the read answers no answer, the way a manager
    /// the bff cannot reach answers one.
    fn devices(&self) -> Result<pai_core::client::NodeDevices, ClientError> {
        self.record("devices".into());
        match *self.live.devices.lock().unwrap_or_else(|e| e.into_inner()) {
            Some((ram_mib, cpu_threads)) => Ok(pai_core::client::NodeDevices {
                probed_at: None,
                ram_mib,
                cpu_threads,
                devices: vec![],
            }),
            None => Err(ClientError::Unavailable),
        }
    }

    fn read_file(&self, name: &str) -> Result<Option<ManagerFile>, ClientError> {
        self.record(format!("read {name}"));
        if self.refused() {
            return Err(ClientError::NotAuthorized);
        }
        std::thread::sleep(self.delay);
        // A read stopped until the test releases it: the save parked inside it.
        gate(
            &self.live.read_gate_armed,
            &self.live.read_gate_released,
            &self.live.reads_gated,
            &self.live.reads_released,
        );
        // A read that takes past the route's time limit: the save that waits
        // on it never reaches its write.
        let pause = self.live.read_pause_ms.load(Ordering::SeqCst);
        if pause > 0 {
            std::thread::sleep(Duration::from_millis(pause));
        }
        let outcome = self
            .live
            .read_outcome
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .clone();
        if let Some(outcome) = outcome {
            return Err(outcome.into_error());
        }
        if let Some(text) = self
            .live
            .files
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .get(name)
        {
            return Ok(Some(whole(text)));
        }
        self.file.clone()
    }
    fn write_file_if(
        &self,
        name: &str,
        contents: &str,
        expected: &str,
    ) -> Result<Affected, ClientError> {
        let principal = self.as_user.clone().unwrap_or_default();
        self.record(format!("write-if {principal} {name}"));
        self.live
            .writes
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .push((name.to_string(), contents.to_string(), expected.to_string()));
        let refused = self
            .live
            .write_refused
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .contains(&principal);
        if refused {
            return Err(ClientError::NotAuthorized);
        }
        let outcome = self
            .live
            .write_outcome
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .clone();
        if let Some(outcome) = outcome {
            return Err(outcome.into_error());
        }
        // The compare and swap, as the real manager's is: the file must still
        // hold what the caller read. The pause runs inside the call, so a
        // write that never comes back is a write the bff must cut off.
        let pause = self.live.write_pause_ms.load(Ordering::SeqCst);
        if pause > 0 {
            std::thread::sleep(Duration::from_millis(pause));
        }
        let mut files = self.live.files.lock().unwrap_or_else(|e| e.into_inner());
        let current = files
            .get(name)
            .cloned()
            .unwrap_or_else(|| SHIPPED.to_string());
        if current != expected {
            return Err(ClientError::Changed);
        }
        files.insert(name.to_string(), contents.to_string());
        Ok(Affected::default())
    }
    fn list_instances(&self) -> Result<Vec<String>, ClientError> {
        self.record("list".into());
        let ids = self
            .live
            .instances
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .clone();
        Ok(if ids.is_empty() {
            vec!["default".into()]
        } else {
            ids
        })
    }
    /// The record, one row per id: the profile of each is the one the test
    /// named (`set_server_profile`), and an id nobody named carries the
    /// profile of its own name. `set_instances_outcome` fails the whole read.
    fn instances(&self) -> Result<Vec<pai_core::client::InstanceInfo>, ClientError> {
        self.record("instances".into());
        let outcome = self
            .live
            .instances_outcome
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .clone();
        if let Some(outcome) = outcome {
            return Err(outcome.into_error());
        }
        let ids = self
            .live
            .instances
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .clone();
        let ids = if ids.is_empty() {
            vec!["default".to_string()]
        } else {
            ids
        };
        let profiles = self
            .live
            .server_profiles
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .clone();
        Ok(ids
            .iter()
            .map(|id| pai_core::client::InstanceInfo {
                id: id.clone(),
                engine: "llama.cpp".to_string(),
                server_profile: profiles.get(id).cloned().unwrap_or(Some(id.clone())),
                port: 8080,
                profile_tags: Vec::new(),
                instance_tags: Vec::new(),
                label: None,
                converged: true,
                reason: None,
                pending_restart: false,
            })
            .collect())
    }
    fn status(&self, id: &str) -> Result<InstanceState, ClientError> {
        self.record(format!("status {id}"));
        let pause = self.live.status_pause_ms.load(Ordering::SeqCst);
        if pause > 0 {
            std::thread::sleep(Duration::from_millis(pause));
        }
        let outcome = self
            .live
            .status_outcome
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .clone();
        if let Some(outcome) = outcome {
            return Err(outcome.into_error());
        }
        Ok(
            if *self.live.running.lock().unwrap_or_else(|e| e.into_inner()) {
                InstanceState::Active
            } else {
                InstanceState::Inactive
            },
        )
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
    live: Live,
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
    let live = Live::default();
    let (l, c, p) = (log.clone(), connects.clone(), principals.clone());
    let shared = live.clone();
    let connect: Connect = Arc::new(move |principal: Option<&str>| {
        c.fetch_add(1, Ordering::SeqCst);
        p.lock()
            .unwrap_or_else(|e| e.into_inner())
            .push(principal.map(str::to_string));
        connect_result.clone()?;
        connect_hook(&shared)?;
        Ok(Box::new(FakeManager {
            file: file.clone(),
            delay,
            log: l.clone(),
            as_user: principal.map(str::to_string),
            refuse: refuse.clone(),
            live: shared.clone(),
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
        live,
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

/// A manager that keeps `text` as `server@default.ini` and takes writes: the
/// `PUT` tests read through it, save through it, and read again. The files,
/// the writes, the refusals and the instance state are shared cells, set
/// through the `App` methods.
fn app_writing(text: &str) -> App {
    app_writing_with_write_timeout(text, TIMEOUT)
}

/// The same manager, with the manager's calls of a write bound by
/// `write_timeout` and the reads left at `TIMEOUT`: the seam a test of the
/// write's own limit uses, so it can hand the pair it wants.
fn app_writing_with_write_timeout(text: &str, write_timeout: Duration) -> App {
    let live = Live::default();
    live.files
        .lock()
        .unwrap()
        .insert("server@default.ini".to_string(), text.to_string());
    *live.running.lock().unwrap() = true;
    let log = Log::default();
    let connects = Arc::new(AtomicUsize::new(0));
    let principals = Principals::default();
    let (l, c, p) = (log.clone(), connects.clone(), principals.clone());
    let shared = live.clone();
    let connect: Connect = Arc::new(move |principal: Option<&str>| {
        c.fetch_add(1, Ordering::SeqCst);
        p.lock()
            .unwrap_or_else(|e| e.into_inner())
            .push(principal.map(str::to_string));
        connect_hook(&shared)?;
        Ok(Box::new(FakeManager {
            file: Ok(None),
            delay: Duration::ZERO,
            log: l.clone(),
            as_user: principal.map(str::to_string),
            refuse: Vec::new(),
            live: shared.clone(),
        }) as Client)
    });
    let start = Instant::now();
    let router = pai_bff::router_with_session_and_write_timeout(
        connect,
        TIMEOUT,
        write_timeout,
        Arc::new(FakeAuth),
        Arc::new(move || start),
        KEY_A,
    );
    App {
        router,
        log,
        connects,
        principals,
        live,
    }
}

impl App {
    fn calls(&self) -> Vec<String> {
        self.log.lock().unwrap_or_else(|e| e.into_inner()).clone()
    }

    /// The `write_file_if` calls the manager was asked for: name, contents,
    /// the text the compare and swap expected.
    fn writes(&self) -> Vec<(String, String, String)> {
        self.live
            .writes
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .clone()
    }

    /// The file the manager holds now.
    fn file_text(&self) -> String {
        self.live
            .files
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .get("server@default.ini")
            .expect("the file")
            .clone()
    }

    /// Change the file behind the bff: the form read a revision of the old text.
    fn set_file(&self, text: &str) {
        self.live
            .files
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .insert("server@default.ini".to_string(), text.to_string());
    }

    /// Answer every coming write with `outcome`, the manager's own failures
    /// between the read and the write.
    fn set_write_outcome(&self, outcome: Outcome) {
        *self.live.write_outcome.lock().unwrap() = Some(outcome);
    }

    /// Answer every coming status read with `outcome`.
    fn set_status_outcome(&self, outcome: Outcome) {
        *self.live.status_outcome.lock().unwrap() = Some(outcome);
    }

    /// The record the manager holds: these ids, so the form of an instance
    /// beside `default` can be read.
    fn set_instances(&self, ids: &[&str]) {
        *self.live.instances.lock().unwrap() = ids.iter().map(|id| id.to_string()).collect();
    }

    /// The record's row of `id` names the server profile `profile` (`None`:
    /// its section names no profile): the file the bff reads and writes for
    /// that instance is `server@<profile>.ini`.
    fn set_server_profile(&self, id: &str, profile: Option<&str>) {
        self.live
            .server_profiles
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .insert(id.to_string(), profile.map(|p| p.to_string()));
    }

    /// `Instances` answers `outcome`: the record that cannot be read.
    fn set_instances_outcome(&self, outcome: Outcome) {
        *self.live.instances_outcome.lock().unwrap() = Some(outcome);
    }

    /// The manager holds `text` as `name`, beside the files it already keeps.
    fn set_file_named(&self, name: &str, text: &str) {
        self.live
            .files
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .insert(name.to_string(), text.to_string());
    }

    /// The text the manager holds as `name`, `None` when it has no such file.
    fn file_named(&self, name: &str) -> Option<String> {
        self.live
            .files
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .get(name)
            .cloned()
    }

    /// Refuse the write of `user`: the member of `pai` removed after the read.
    fn set_write_refused(&self, user: &str) {
        self.live
            .write_refused
            .lock()
            .unwrap()
            .push(user.to_string());
    }

    /// Refuse the reads of `user`, leaving the writes alone: the read refused
    /// for a signed-in user, before the route reaches the write.
    fn set_read_refused(&self, user: &str) {
        self.live
            .read_refused
            .lock()
            .unwrap()
            .push(user.to_string());
    }

    /// A write that takes `ms` milliseconds: the manager busy at the moment
    /// the save arrives.
    fn set_write_pause_ms(&self, ms: u64) {
        self.live.write_pause_ms.store(ms, Ordering::SeqCst);
    }

    /// Answer every coming read with `outcome`, the manager refusing before
    /// the save reaches its write.
    fn set_read_outcome(&self, outcome: Outcome) {
        *self.live.read_outcome.lock().unwrap() = Some(outcome);
    }

    /// A read that takes `ms` milliseconds: the time limit runs out with the
    /// write not asked.
    fn set_read_pause_ms(&self, ms: u64) {
        self.live.read_pause_ms.store(ms, Ordering::SeqCst);
    }

    /// Stop the next read inside the manager call, until `release_read_gate`
    /// lets it go. A test of the save only (see `gate`).
    fn arm_read_gate(&self) {
        self.live.read_gate_armed.store(true, Ordering::SeqCst);
    }

    /// Let the read stopped by `arm_read_gate` return.
    fn release_read_gate(&self) {
        self.live.read_gate_released.store(true, Ordering::SeqCst);
    }

    /// How many reads stopped in the gate.
    fn reads_gated(&self) -> usize {
        self.live.reads_gated.load(Ordering::SeqCst)
    }

    /// How many stopped reads the test let go, so the task that was parked in
    /// one is on its way to its write.
    fn reads_released(&self) -> usize {
        self.live.reads_released.load(Ordering::SeqCst)
    }

    /// Answer every coming connect with `outcome`, the manager not reached at
    /// all. A test of the save only (see `connect_hook`).
    fn set_connect_outcome(&self, outcome: Outcome) {
        *self.live.connect_outcome.lock().unwrap() = Some(outcome);
    }

    /// A connect that takes `ms` milliseconds: the time limit runs out before
    /// the manager is reached. A test of the save only (see `connect_hook`).
    fn set_connect_pause_ms(&self, ms: u64) {
        self.live.connect_pause_ms.store(ms, Ordering::SeqCst);
    }

    /// Stop the next connect inside the manager call, until
    /// `release_connect_gate` lets it go. A test of the save only (see `gate`).
    fn arm_connect_gate(&self) {
        self.live.connect_gate_armed.store(true, Ordering::SeqCst);
    }

    /// Let the connect stopped by `arm_connect_gate` return.
    fn release_connect_gate(&self) {
        self.live
            .connect_gate_released
            .store(true, Ordering::SeqCst);
    }

    /// How many connects stopped in the gate.
    fn connects_gated(&self) -> usize {
        self.live.connects_gated.load(Ordering::SeqCst)
    }

    /// How many stopped connects the test let go, so the task that was parked
    /// in one is on its way to its read and its write.
    fn connects_released(&self) -> usize {
        self.live.connects_released.load(Ordering::SeqCst)
    }

    /// A status read that takes `ms` milliseconds: the manager busy after the
    /// write landed.
    fn set_status_pause_ms(&self, ms: u64) {
        self.live.status_pause_ms.store(ms, Ordering::SeqCst);
    }

    /// Whether the instance's process runs.
    fn set_running(&self, running: bool) {
        *self.live.running.lock().unwrap() = running;
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
        self.send(request).await
    }

    /// `PUT /api/instances/default/server` with `body` as JSON, saving.
    async fn put(&self, cookie: Option<&str>, body: &Value) -> Reply {
        self.put_with(cookie, &body.to_string(), &[]).await
    }

    /// The same with the body as given (not JSON, oversized) and extra
    /// headers (an `Origin` from elsewhere).
    async fn put_with(&self, cookie: Option<&str>, body: &str, extra: &[(&str, &str)]) -> Reply {
        let mut builder = Request::builder()
            .method(Method::PUT)
            .uri("/api/instances/default/server")
            .header(header::CONTENT_TYPE, "application/json");
        if let Some(c) = cookie {
            builder = builder.header(header::COOKIE, c);
        }
        for (name, value) in extra {
            builder = builder.header(*name, *value);
        }
        let request = builder.body(Body::from(body.to_string())).expect("request");
        self.send(request).await
    }

    /// One request through the router, with the deadline every request runs
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

    /// A 422 value refusal: `{"key", "args", "field"}`, the INI key the form
    /// marks beside the check's key and values.
    fn assert_refused(&self, key: &str, args: &[&str], field: &str) {
        assert_eq!(
            self.status,
            StatusCode::UNPROCESSABLE_ENTITY,
            "{}",
            self.body
        );
        let doc = self.json();
        assert_eq!(doc["key"], json!(key), "{}", self.body);
        assert_eq!(doc["args"], json!(args), "{}", self.body);
        assert_eq!(doc["field"], json!(field), "{}", self.body);
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
        assert_eq!(
            app.calls(),
            ["instances", "read server@default.ini"],
            "{user}: the record, then the profile it names"
        );
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
    assert_eq!(
        app.calls(),
        ["instances"],
        "the record does not name the id, so no file is asked for"
    );
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
/// manager for the instance's own file and the node's thread width, nothing
/// else.
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
    assert_eq!(
        app.calls(),
        ["instances", "read server@default.ini"],
        "the record, then the profile it names"
    );
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
Devices = CUDA0,ROCm1
ModelsDir = /srv/models
RocblasUseHipblaslt = false
RocmHostMemoryGtt = false
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
Tags = edge,v2
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
            "Devices": "CUDA0,ROCm1",
            "ModelsDir": "/srv/models",
            "RocblasUseHipblaslt": "off",
            "RocmHostMemoryGtt": false,
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
            "Tags": ["edge", "v2"],
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

/// GT1: `RocmHostMemoryGtt` reads as a switch that has an unset state: `true`
/// and `false` as themselves, and `null` for a key the file does not set (unset
/// is the ON state of the launch: the bff sends no default of its own) and for
/// a value the configurator cannot read.
#[tokio::test]
async fn rocm_host_memory_gtt_reads_its_bool_and_unset() {
    for (text, want) in [
        ("[Server]\nRocmHostMemoryGtt = true\n", json!(true)),
        ("[Server]\nRocmHostMemoryGtt = false\n", json!(false)),
        ("[Server]\nRocmHostMemoryGtt = sometimes\n", Value::Null),
        ("[Server]\nPort = 8580\n", Value::Null),
    ] {
        let doc = app_holding(text).read_default().await.document();
        assert_eq!(doc["values"]["RocmHostMemoryGtt"], want, "{text:?}");
    }
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
        // GT1: the shipped file sets it no line of its own, so it reads unset,
        // which is the ON state of the launch.
        "RocmHostMemoryGtt",
        "CudaAllreduce",
        "Webui",
        "WebuiMcpProxy",
        "LogVerbosity",
        "SaveStateOnShutdown",
        // The shipped file sets no StateDir: every instance gets a folder of
        // its own (`paths::instance_state_dir()/<id>`), so the field is unset.
        "StateDir",
        "OpencodeBaseUrl",
        "OpencodeApiKey",
    ] {
        assert_eq!(values[key], Value::Null, "{key}");
    }
    for key in ["CudaDisableGraphs", "AttnRotDisable", "RocprofAttach"] {
        assert_eq!(values[key], json!(false), "{key}");
    }
    assert_eq!(
        values["Tags"],
        json!([]),
        "a profile with no tags reads as an empty list, not a missing key"
    );
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
RocmHostMemoryGtt = sometimes
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
        "RocmHostMemoryGtt",
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

/// The snapshot folder of the instance `id`, as the launch resolves it: the
/// engine's state root with the id joined (`paths::instance_state_dir`,
/// `launch::instance_command`).
fn state_dir_of(id: &str) -> String {
    pai_core::paths::instance_state_dir()
        .join(id)
        .to_string_lossy()
        .into_owned()
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
            "StateDir": state_dir_of("default"),
        })
    );
}

/// The state directory the form shows as the default is the folder of the
/// instance the request names, inside the engine's state root, as the
/// manager's launch passes it (`launch::instance_command` joins the id): two
/// instances never write their snapshots into one folder.
#[tokio::test]
async fn the_state_dir_default_is_the_folder_of_the_instance_of_the_request() {
    let app = app_holding("");
    app.set_instances(&["default", "gpu-1"]);
    let cookie = app.sign_in().await;
    let root = pai_core::paths::instance_state_dir();
    for id in ["default", "gpu-1"] {
        let doc = app
            .get(&format!("/api/instances/{id}/server"), Some(&cookie))
            .await
            .document();
        assert_eq!(
            doc["defaults"]["StateDir"],
            json!(state_dir_of(id)),
            "{id}: the folder of the instance, not the root"
        );
    }
    assert_ne!(
        root.join("default").to_string_lossy().into_owned(),
        root.join("gpu-1").to_string_lossy().into_owned(),
        "the two folders are different"
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

/// R1, as amended: a server document read asks the manager no `devices`. That
/// call can run the engine's probe (up to 30 s) under the 5 s read bound, so a
/// page load must never make it: the thread limits go back to the bff's own
/// CPU count, even when the manager would answer a different one.
#[tokio::test]
async fn the_document_reads_ask_the_manager_no_devices() {
    let cpus = std::thread::available_parallelism()
        .expect("the CPU count")
        .get();
    let app = app_writing(SHIPPED);
    *app.live.devices.lock().unwrap() = Some((3_932_160, 8));
    let cookie = app.sign_in().await;
    for path in [
        "/api/instances/default/server",
        "/api/profiles/server/default",
    ] {
        let doc = app.get(path, Some(&cookie)).await.document();
        assert_eq!(
            doc["limits"]["Threads"],
            json!([1, cpus]),
            "{path}: the form caps at the bff's own count, not the manager's 8"
        );
        assert_eq!(
            doc["limits"]["ThreadsBatch"],
            json!([1, cpus]),
            "{path}: the batch limit is the same width"
        );
    }
    let calls = app.calls();
    assert!(
        !calls.iter().any(|c| c == "devices"),
        "neither document read asks the manager's devices: {calls:?}"
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
/// The empty file is vector 0 of that list. The shipped file's vector is the
/// hash of its bytes: no `StateDir` line, the file ending with `ModelsDir`.
#[tokio::test]
async fn the_revision_is_the_keyed_hash_of_the_text() {
    assert!(
        !SHIPPED
            .lines()
            .any(|l| l.trim_start().starts_with("StateDir")),
        "the shipped Linux file sets no StateDir: {SHIPPED}"
    );
    assert_eq!(
        SHIPPED.lines().last(),
        Some(concat!("ModelsDir = /var/lib/llama", ".cpp/models")),
        "the shipped file ends with the models folder it names: {SHIPPED}"
    );
    let cases: [(&str, &str); 4] = [
        ("", "726fdb47dd0e0e31"),
        (SHIPPED, "109d822d77ca2d4d"),
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

// ── The GPUs of the profile: `[Server] Devices` (R2) ────────────────────

/// The document carries `Devices` like every other text key: the file's list
/// reads as one string, and a file that sets nothing answers `null` (every
/// GPU, as today).
#[tokio::test]
async fn the_document_carries_the_devices_of_the_file() {
    let doc = app_holding("[Server]\nPort = 8580\nDevices = ROCm0,ROCm1\n")
        .read_default()
        .await
        .document();
    assert_eq!(doc["values"]["Devices"], json!("ROCm0,ROCm1"), "{doc}");
    assert_eq!(
        app_holding(SHIPPED).read_default().await.document()["values"]["Devices"],
        Value::Null,
        "the shipped file sets no Devices: every GPU"
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

// ── Saving: `PUT /api/instances/{id}/server` ────────────────────────────

/// The read's `values` with these overrides: the form sends every key back as
/// the read gave it, changed where the user typed.
fn values_with(doc: &Value, overrides: &[(&str, Value)]) -> Value {
    let mut values = doc["values"].as_object().expect("values").clone();
    for (key, value) in overrides {
        values.insert(key.to_string(), value.clone());
    }
    Value::Object(values)
}

/// The form's save body for this read: `{"revision", "values"}`.
fn save_body(doc: &Value, overrides: &[(&str, Value)]) -> Value {
    json!({ "revision": doc["revision"], "values": values_with(doc, overrides) })
}

/// A save body the shape gate accepts, for a route that must be refused before
/// any file is read (a record with no profile, a record that cannot be read).
/// The shipped file's own document, read off a throwaway app: a body built by
/// hand is a second copy of the field list, and the gate refuses it the moment
/// the document gains a key.
async fn shaped_save_body() -> Value {
    save_body(&app_writing(SHIPPED).read_default().await.document(), &[])
}

/// The save's 200: exactly `{"revision", "running"}`.
fn assert_saved(reply: &Reply) -> Value {
    let doc = reply.document();
    let mut members: Vec<&str> = doc
        .as_object()
        .expect("an object")
        .keys()
        .map(String::as_str)
        .collect();
    members.sort();
    assert_eq!(members, ["revision", "running"]);
    doc
}

/// No session: 401 `session-required`, the manager not even connected to.
#[tokio::test]
async fn a_save_without_a_session_is_session_required() {
    let app = app_writing(WITH_KEY);
    let doc = app.read_default().await.document();
    let r = app.put(None, &save_body(&doc, &[])).await;
    r.assert_error(StatusCode::UNAUTHORIZED, "session-required");
    assert!(app.writes().is_empty(), "nothing written");
    assert_eq!(app.file_text(), WITH_KEY, "the file as it was");
}

/// A `PUT` from a foreign origin is refused before anything: the save writes
/// nothing and the manager is not asked beyond the form's own read.
#[tokio::test]
async fn a_save_from_a_foreign_origin_is_refused() {
    let app = app_writing(WITH_KEY);
    let cookie = app.sign_in().await;
    let connects = app.connects.load(Ordering::SeqCst);
    app.put_with(Some(&cookie), "{}", &[("origin", "https://evil.example")])
        .await
        .assert_error(StatusCode::FORBIDDEN, "request-cross-origin");
    assert_eq!(
        app.connects.load(Ordering::SeqCst),
        connects,
        "no new connect"
    );
    assert!(app.writes().is_empty(), "nothing written");
}

/// A body that is not the form's shape: no JSON, no `revision`, `values` that
/// is no object, a value not of the JSON type the read gives it. 400
/// `server-malformed`, the manager not asked beyond the form's own read.
#[tokio::test]
async fn a_body_that_is_not_the_forms_shape_is_server_malformed() {
    let app = app_writing(WITH_KEY);
    let cookie = app.sign_in().await;
    let doc = app.read_default().await.document();
    let connects = app.connects.load(Ordering::SeqCst);
    let revision = doc["revision"].as_str().expect("a revision").to_string();
    let bodies = [
        "not json at all".to_string(),
        String::new(),
        "{}".to_string(),
        json!({ "values": doc["values"] }).to_string(),
        json!({ "revision": 1, "values": doc["values"] }).to_string(),
        json!({ "revision": revision, "values": "Port" }).to_string(),
        json!({ "revision": revision, "values": { "Port": "abc" } }).to_string(),
        json!({ "revision": revision, "values": { "Webui": "true" } }).to_string(),
        json!({ "revision": revision, "values": { "RocblasUseHipblaslt": true } }).to_string(),
        // GT1: the switch is a bool, so a word and a number are not the type the
        // read gives (the value the manager's bool rule refuses is a file the
        // bff never renders: `the_managers_bool_refusal_of_rocm_host_memory_gtt…`).
        json!({ "revision": revision, "values": { "RocmHostMemoryGtt": "true" } }).to_string(),
        json!({ "revision": revision, "values": { "RocmHostMemoryGtt": 1 } }).to_string(),
        json!({ "revision": revision, "values": { "Port": 8.5 } }).to_string(),
    ];
    for body in bodies {
        let r = app.put_with(Some(&cookie), &body, &[]).await;
        r.assert_error(StatusCode::BAD_REQUEST, "server-malformed");
    }
    assert_eq!(
        app.connects.load(Ordering::SeqCst),
        connects,
        "no new connect"
    );
    assert!(app.writes().is_empty(), "nothing written");
    assert_eq!(app.file_text(), WITH_KEY, "the file as it was");
}

/// A body over the bff's size limit is 413 `payload-too-large`, before the
/// route reads it.
#[tokio::test]
async fn a_body_larger_than_the_limit_is_payload_too_large() {
    let app = app_writing(WITH_KEY);
    let cookie = app.sign_in().await;
    let doc = app.read_default().await.document();
    let connects = app.connects.load(Ordering::SeqCst);
    let body = save_body(&doc, &[("Hostname", json!("a".repeat(70 * 1024)))]);
    app.put(Some(&cookie), &body)
        .await
        .assert_error(StatusCode::PAYLOAD_TOO_LARGE, "payload-too-large");
    assert_eq!(
        app.connects.load(Ordering::SeqCst),
        connects,
        "no new connect"
    );
    assert!(app.writes().is_empty(), "nothing written");
}

/// The save on a running instance: 200 `{"revision", "running"}`, and the
/// file the manager holds afterwards is `server_cfg::render_for` of the
/// body's values. The save reads the file itself (read, compare, write): the
/// compare and swap expects exactly the text of that read, the write is the
/// manager call for the session's user, the new `revision` is the one the next
/// read answers, `running` true, and the answer sends no API key.
#[tokio::test]
async fn a_save_writes_the_file_the_body_holds_as_the_signed_in_user() {
    let app = app_writing(WITH_KEY);
    let cookie = app.sign_in().await;
    let doc = app.read_default().await.document();
    let reply = app
        .put(Some(&cookie), &save_body(&doc, &[("Port", json!(8642))]))
        .await;
    let saved = assert_saved(&reply);
    assert_eq!(saved["running"], json!(true), "{}", reply.body);

    let writes = app.writes();
    assert_eq!(writes.len(), 1, "{writes:?}");
    let (name, contents, expected) = &writes[0];
    assert_eq!(name, "server@default.ini");
    assert_eq!(expected, WITH_KEY, "the CAS expects the text the bff read");
    assert!(contents.contains("Port = 8642"), "{contents}");
    assert!(
        contents.contains("OpencodeApiKey = sk-test"),
        "the key survives the save: {contents}"
    );
    let cfg = pai_core::server_cfg::from_text(contents);
    assert_eq!(cfg.port, Some(8642));
    assert_eq!(
        pai_core::server_cfg::render_for(&cfg, true),
        *contents,
        "the file is exactly the render of the values"
    );
    assert!(!reply.body.contains("sk-test"), "no key in the answer");

    // The save reads the file itself: the text it reads is the revision it
    // compares and the `expected` of the compare and swap. So the calls are
    // the form's own read (record, file), the save's read, the write and the
    // status.
    assert_eq!(
        app.calls(),
        [
            "instances".to_string(),
            "read server@default.ini".to_string(),
            "instances".to_string(),
            "read server@default.ini".to_string(),
            "write-if alice server@default.ini".to_string(),
            "status default".to_string(),
        ],
        "the form's read, the save's read, the write, then the status for running"
    );
    assert_eq!(
        principals_of(&app.principals),
        vec![Some("alice".to_string()); 2],
        "every connect acts for the session's user"
    );

    let after = app
        .get("/api/instances/default/server", Some(&cookie))
        .await
        .document();
    assert_eq!(
        after["revision"], saved["revision"],
        "the answer's revision is the one the file has now"
    );
    assert_eq!(after["values"]["Port"], json!(8642));
    assert_eq!(after["values"]["OpencodeApiKey"], json!("sk-test"));
}

/// The save acts for the session's user, not the request's: the write is the
/// call of the signed-in `root`, and the save of `alice`'s session is `alice`'s.
/// Each form holds the revision of its own read, so `alice` reads again after
/// `root` saved: her save is fresh and lands, over his.
#[tokio::test]
async fn the_save_of_each_session_is_the_call_of_that_user() {
    let app = app_writing(WITH_KEY);
    let alice = app.sign_in_as("alice").await;
    let root = app.sign_in_as("root").await;
    let doc = app
        .get("/api/instances/default/server", Some(&root))
        .await
        .document();
    assert_saved(
        &app.put(Some(&root), &save_body(&doc, &[("Port", json!(8642))]))
            .await,
    );
    let doc = app
        .get("/api/instances/default/server", Some(&alice))
        .await
        .document();
    assert_saved(
        &app.put(Some(&alice), &save_body(&doc, &[("Port", json!(8643))]))
            .await,
    );
    let writes: Vec<String> = app
        .calls()
        .into_iter()
        .filter(|c| c.starts_with("write-if"))
        .collect();
    assert_eq!(
        writes,
        [
            "write-if root server@default.ini".to_string(),
            "write-if alice server@default.ini".to_string(),
        ],
        "each save is the call of its own session's user"
    );
    assert!(
        app.file_text().contains("Port = 8643"),
        "the last save is the one the file holds: {}",
        app.file_text()
    );
}

/// The key the body names is the key the file gets, and it never appears in
/// the save's answer.
#[tokio::test]
async fn the_saved_file_keeps_the_api_key_the_body_names() {
    let app = app_writing(WITH_KEY);
    let cookie = app.sign_in().await;
    let doc = app.read_default().await.document();
    let reply = app
        .put(
            Some(&cookie),
            &save_body(&doc, &[("OpencodeApiKey", json!("sk-rotated"))]),
        )
        .await;
    assert_saved(&reply);
    assert!(!reply.body.contains("sk-rotated"), "no key in the answer");
    let writes = app.writes();
    assert!(
        writes[0].1.contains("OpencodeApiKey = sk-rotated"),
        "{writes:?}"
    );
    let after = app
        .get("/api/instances/default/server", Some(&cookie))
        .await
        .document();
    assert_eq!(after["values"]["OpencodeApiKey"], json!("sk-rotated"));
}

/// A blank `ModelsDir` or `StateDir` is left unset in the file, on every OS:
/// the save writes no line for it (the file keeps the key's commented hint,
/// which starts with `;`), and names no folder of the bff's account.
#[tokio::test]
async fn a_blank_directory_is_left_unset_in_the_file() {
    let app = app_writing(WITH_KEY);
    let cookie = app.sign_in().await;
    let doc = app.read_default().await.document();

    app.put(
        Some(&cookie),
        &save_body(
            &doc,
            &[
                ("ModelsDir", json!("/mnt/data/models")),
                ("StateDir", json!("/var/lib/state")),
            ],
        ),
    )
    .await;
    let set = app.file_text();
    assert!(set.contains("ModelsDir = /mnt/data/models"), "{set}");
    assert!(set.contains("StateDir = /var/lib/state"), "{set}");

    let doc = app
        .get("/api/instances/default/server", Some(&cookie))
        .await
        .document();
    app.put(
        Some(&cookie),
        &save_body(&doc, &[("ModelsDir", json!("")), ("StateDir", json!(""))]),
    )
    .await;
    let cleared = app.file_text();
    for key in ["ModelsDir", "StateDir"] {
        let lines: Vec<&str> = cleared.lines().filter(|l| l.starts_with(key)).collect();
        assert!(
            lines.is_empty(),
            "{key} is left unset: the file keeps its hint commented, never a line of its own: {lines:?}"
        );
    }
    let own_models = pai_core::server_cfg::default_models_dir();
    let own_state = pai_core::slot_state::default_state_dir()
        .to_string_lossy()
        .into_owned();
    assert!(!cleared.contains(own_models.as_str()), "{cleared}");
    assert!(!cleared.contains(own_state.as_str()), "{cleared}");
}

/// GT1: the save takes `RocmHostMemoryGtt`. A bool writes the key line (the
/// launch reads `false` as "export neither HSA variable", `true` as the
/// explicit on), and `null`, the key unset, writes no line of its own: the file
/// keeps the commented hint and reloads unset, which is the ON state.
#[tokio::test]
async fn a_save_writes_rocm_host_memory_gtt_as_the_body_holds() {
    let app = app_writing(WITH_KEY);
    let cookie = app.sign_in().await;
    for (value, lit) in [(json!(true), "true"), (json!(false), "false")] {
        let doc = app
            .get("/api/instances/default/server", Some(&cookie))
            .await
            .document();
        assert_saved(
            &app.put(
                Some(&cookie),
                &save_body(&doc, &[("RocmHostMemoryGtt", value)]),
            )
            .await,
        );
        let text = app.file_text();
        assert!(
            text.lines()
                .any(|l| l == format!("RocmHostMemoryGtt = {lit}")),
            "the body's {lit}: {text}"
        );
        assert_eq!(
            pai_core::server_cfg::from_text(&text).rocm_host_memory_gtt,
            Some(lit == "true"),
            "the file reloads the value the body named"
        );
    }
    let doc = app
        .get("/api/instances/default/server", Some(&cookie))
        .await
        .document();
    assert_saved(
        &app.put(
            Some(&cookie),
            &save_body(&doc, &[("RocmHostMemoryGtt", Value::Null)]),
        )
        .await,
    );
    let text = app.file_text();
    assert!(
        !text.lines().any(|l| l.starts_with("RocmHostMemoryGtt =")),
        "unset writes no key line of its own: {text}"
    );
    assert_eq!(
        pai_core::server_cfg::from_text(&text).rocm_host_memory_gtt,
        None,
        "it reloads unset: {text}"
    );
}

/// The form's revision is stale (the file changed behind it): 409
/// `server-changed-elsewhere` and the manager is never asked to write. The
/// session stays signed in: this is not a refusal of the user.
#[tokio::test]
async fn a_stale_revision_is_409_and_the_manager_is_not_asked_to_write() {
    let app = app_writing(WITH_KEY);
    let cookie = app.sign_in().await;
    let doc = app.read_default().await.document();
    app.set_file(&format!("{WITH_KEY}# a save from elsewhere\n"));
    app.put(Some(&cookie), &save_body(&doc, &[("Port", json!(8642))]))
        .await
        .assert_error(StatusCode::CONFLICT, "server-changed-elsewhere");
    assert!(app.writes().is_empty(), "nothing written");
    app.get("/api/session", Some(&cookie))
        .await
        .assert_signed_in_as("alice");
}

/// The file changed between the bff's read and its write: the manager answers
/// the compare and swap with its refusal and the bff says the same 409.
#[tokio::test]
async fn a_file_changed_under_the_save_is_409_server_changed_elsewhere() {
    let app = app_writing(WITH_KEY);
    let cookie = app.sign_in().await;
    let doc = app.read_default().await.document();
    app.set_write_outcome(Outcome::Changed);
    app.put(Some(&cookie), &save_body(&doc, &[("Port", json!(8642))]))
        .await
        .assert_error(StatusCode::CONFLICT, "server-changed-elsewhere");
    assert_eq!(app.file_text(), WITH_KEY, "the manager wrote nothing");
}

/// `running`: true while the instance's process runs (active), false while it
/// does not (inactive): the next start reads the saved file, so a save while
/// stopped needs no restart.
#[tokio::test]
async fn running_is_true_only_while_the_process_runs() {
    let app = app_writing(WITH_KEY);
    let cookie = app.sign_in().await;
    let doc = app.read_default().await.document();
    app.set_running(false);
    let stopped = assert_saved(&app.put(Some(&cookie), &save_body(&doc, &[])).await);
    assert_eq!(stopped["running"], json!(false));

    let doc = app
        .get("/api/instances/default/server", Some(&cookie))
        .await
        .document();
    app.set_running(true);
    let running = assert_saved(&app.put(Some(&cookie), &save_body(&doc, &[])).await);
    assert_eq!(running["running"], json!(true));
}

/// A status read that fails after a good write still answers 200 with
/// `running` true: the save succeeded and the bff cannot know the state.
#[tokio::test]
async fn running_is_true_when_the_status_read_after_the_write_fails() {
    let app = app_writing(WITH_KEY);
    let cookie = app.sign_in().await;
    let doc = app.read_default().await.document();
    app.set_status_outcome(Outcome::Unavailable);
    let saved = assert_saved(&app.put(Some(&cookie), &save_body(&doc, &[])).await);
    assert_eq!(saved["running"], json!(true));
}

/// The member of `pai` removed between the read and the write: the manager
/// refuses the write, the answer is 403 `session-not-administrator` and the
/// session ends; the file is untouched.
#[tokio::test]
async fn a_write_refused_for_the_signed_in_user_ends_the_session() {
    let app = app_writing(WITH_KEY);
    let cookie = app.sign_in().await;
    let doc = app.read_default().await.document();
    app.set_write_refused("alice");
    app.put(Some(&cookie), &save_body(&doc, &[("Port", json!(8642))]))
        .await
        .assert_error(StatusCode::FORBIDDEN, "session-not-administrator");
    app.get("/api/session", Some(&cookie))
        .await
        .assert_error(StatusCode::UNAUTHORIZED, "session-required");
    assert_eq!(app.file_text(), WITH_KEY, "the file as it was");
}

/// The read refused for the signed-in user, before the route reaches the
/// write: 403 `session-not-administrator`, the session ends, and the manager
/// is never asked to write.
#[tokio::test]
async fn a_save_whose_read_is_refused_ends_the_session_and_never_writes() {
    let app = app_writing(WITH_KEY);
    let cookie = app.sign_in().await;
    let doc = app.read_default().await.document();
    app.set_read_refused("alice");
    app.put(Some(&cookie), &save_body(&doc, &[("Port", json!(8642))]))
        .await
        .assert_error(StatusCode::FORBIDDEN, "session-not-administrator");
    app.get("/api/session", Some(&cookie))
        .await
        .assert_error(StatusCode::UNAUTHORIZED, "session-required");
    assert!(app.writes().is_empty(), "no write was asked for");
    assert_eq!(app.file_text(), WITH_KEY, "the file as it was");
    let calls = app.calls();
    assert!(
        !calls.iter().any(|c| c.starts_with("write-if")),
        "the manager's calls: {calls:?}"
    );
}

/// The manager says the instance is not there: 404 `client-no-instance`.
#[tokio::test]
async fn the_managers_no_instance_is_404() {
    let app = app_writing(WITH_KEY);
    let cookie = app.sign_in().await;
    let doc = app.read_default().await.document();
    app.set_write_outcome(Outcome::NoInstance);
    app.put(Some(&cookie), &save_body(&doc, &[("Port", json!(8642))]))
        .await
        .assert_error(StatusCode::NOT_FOUND, "client-no-instance");
    assert_eq!(app.file_text(), WITH_KEY, "the file as it was");
}

// The 503 and 504 of a save turn on one thing (PLAN.md step 10 item 8): as long as the
// manager has not been asked to write, the bff reports 503 `client-unavailable`; from the
// moment the write was asked and its outcome is unknown, it reports 504 `client-timeout`. A
// stream that dies between the read and the write reaches the bff as the very same
// `ClientError::Unavailable` a manager that was never reached does, so the route cannot tell
// the two from the error alone: it has to ask whether the call was sent.
//
// The tests below set the fake's cells, or arm its gate, AFTER the form's own read, so the read that
// carries the revision number succeeds: the save stops at the connect, at the read, or at the
// write, and the answer has to place itself accordingly. The two that arm a gate release it
// afterwards, to show what a save the route answered must not do: write.

/// The time limit runs out while the bff is reading the file: the manager was never asked to
/// write, so 503 `client-unavailable`, and the session is kept (the manager's absence is no
/// refusal of the user, and it may come back).
#[tokio::test]
async fn a_time_limit_that_runs_out_at_the_read_is_503() {
    let app = app_writing(WITH_KEY);
    let cookie = app.sign_in().await;
    let doc = app.read_default().await.document();

    // Stop the save inside the read, well past the bff's own time limit.
    app.set_read_pause_ms(3 * TIMEOUT.as_millis() as u64);

    let started = Instant::now();
    app.put(Some(&cookie), &save_body(&doc, &[("Port", json!(8642))]))
        .await
        .assert_error(StatusCode::SERVICE_UNAVAILABLE, "client-unavailable");
    assert!(
        started.elapsed() < DEADLINE,
        "the bff answered within its own limit, not the request deadline"
    );
    assert_eq!(
        app.calls(),
        // The record and the form's own read (the file), then the save's:
        // the save stops at its read and is never asked to write.
        [
            "instances",
            "read server@default.ini",
            "instances",
            "read server@default.ini"
        ],
        "the save stops at the read: the manager is never asked to write"
    );
    assert!(app.writes().is_empty(), "no write may be recorded");
    assert_eq!(app.file_text(), WITH_KEY, "the file as it was");
    app.get("/api/session", Some(&cookie))
        .await
        .assert_signed_in_as("alice");
}

/// The time limit runs out before the bff even reaches the manager (here it stalls opening
/// the connection): the same 503, and nothing at all is asked of the manager.
#[tokio::test]
async fn a_time_limit_that_runs_out_before_the_manager_is_reached_is_503() {
    let app = app_writing(WITH_KEY);
    let cookie = app.sign_in().await;
    let doc = app.read_default().await.document();

    // Stop the save while the connection is being opened, past the time limit.
    app.set_connect_pause_ms(3 * TIMEOUT.as_millis() as u64);

    app.put(Some(&cookie), &save_body(&doc, &[("Port", json!(8642))]))
        .await
        .assert_error(StatusCode::SERVICE_UNAVAILABLE, "client-unavailable");
    assert_eq!(
        app.calls(),
        // Only the form's own read, the record and the file: nothing else
        // is asked of a manager that was never reached.
        vec!["instances", "read server@default.ini"],
        "nothing is asked of a manager that was never reached"
    );
    assert!(app.writes().is_empty(), "no write may be recorded");
    assert_eq!(app.file_text(), WITH_KEY, "the file as it was");
    app.get("/api/session", Some(&cookie))
        .await
        .assert_signed_in_as("alice");
}

/// The time limit runs out while the save is parked inside its read, and the test then lets
/// that read return. The route has answered 503 `client-unavailable`, saying the manager was
/// never asked to write, so the save is over: the task it left behind must not write. A route
/// that only stops waiting for the blocking task answers the 503 and lets the task run on: its
/// read comes back, the file is still the one the form read so the revision matches, and the
/// task writes the file the browser was told was not written.
#[tokio::test]
async fn a_time_limit_that_runs_out_at_the_read_writes_nothing_after_the_503() {
    let app = app_writing(WITH_KEY);
    let cookie = app.sign_in().await;
    let doc = app.read_default().await.document();

    // Park the save inside its read, past the route's time limit.
    app.arm_read_gate();
    app.put(Some(&cookie), &save_body(&doc, &[("Port", json!(8642))]))
        .await
        .assert_error(StatusCode::SERVICE_UNAVAILABLE, "client-unavailable");
    assert_eq!(app.reads_gated(), 1, "the save stopped inside its read");
    assert!(app.writes().is_empty(), "nothing while the read is parked");

    // Let the parked read return: the task is free to go on.
    app.release_read_gate();
    let waited = Instant::now();
    while app.reads_released() < 1 && waited.elapsed() < DEADLINE {
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
    assert_eq!(app.reads_released(), 1, "the parked read came back");

    // The read is back and the file is the one the form read, so a task that was not cut
    // off goes straight to its write: GRACE is the time it needs to get there.
    let grace = Instant::now();
    while app.writes().is_empty() && grace.elapsed() < GRACE {
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
    assert!(
        app.writes().is_empty(),
        "a save answered 503 asked to write {} times",
        app.writes().len()
    );
    assert!(
        !app.calls().iter().any(|c| c.starts_with("write-if")),
        "the manager's calls: {:?}",
        app.calls()
    );
    assert_eq!(app.file_text(), WITH_KEY, "the file as it was");
    app.get("/api/session", Some(&cookie))
        .await
        .assert_signed_in_as("alice");
}

/// The same for a save parked while the connection is being opened: the route answers 503
/// before the manager is reached, and a task that is not cut off opens the connection after
/// the test releases it, reads the file, finds the revision the form sent, and writes.
#[tokio::test]
async fn a_time_limit_that_runs_out_at_the_connect_writes_nothing_after_the_503() {
    let app = app_writing(WITH_KEY);
    let cookie = app.sign_in().await;
    let doc = app.read_default().await.document();

    // Park the save while its connection is being opened, past the time limit.
    app.arm_connect_gate();
    app.put(Some(&cookie), &save_body(&doc, &[("Port", json!(8642))]))
        .await
        .assert_error(StatusCode::SERVICE_UNAVAILABLE, "client-unavailable");
    assert_eq!(
        app.connects_gated(),
        1,
        "the save stopped opening its connection"
    );
    assert!(
        app.writes().is_empty(),
        "nothing while the connect is parked"
    );

    // Let the parked connect return: the task is free to read and write.
    app.release_connect_gate();
    let waited = Instant::now();
    while app.connects_released() < 1 && waited.elapsed() < DEADLINE {
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
    assert_eq!(app.connects_released(), 1, "the parked connect came back");

    // The connect is back, the read that follows it reads the unchanged file, so a task
    // that was not cut off goes on to its write.
    let grace = Instant::now();
    while app.writes().is_empty() && grace.elapsed() < GRACE {
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
    assert!(
        app.writes().is_empty(),
        "a save answered 503 asked to write {} times",
        app.writes().len()
    );
    assert!(
        !app.calls().iter().any(|c| c.starts_with("write-if")),
        "the manager's calls: {:?}",
        app.calls()
    );
    assert_eq!(app.file_text(), WITH_KEY, "the file as it was");
    app.get("/api/session", Some(&cookie))
        .await
        .assert_signed_in_as("alice");
}

/// The manager refuses with `Unavailable` where nothing has been sent yet: at the connect, and
/// at the read. 503 `client-unavailable` in both, the same answer as a manager that was never
/// reached. This stays 503 when the route learns whether it sent the call: only a write that
/// was asked turns the answer into 504.
#[tokio::test]
async fn a_manager_unavailable_before_the_write_is_503() {
    for stop in ["connect", "read"] {
        let app = app_writing(WITH_KEY);
        let cookie = app.sign_in().await;
        let doc = app.read_default().await.document();

        if stop == "connect" {
            app.set_connect_outcome(Outcome::Unavailable);
        } else {
            app.set_read_outcome(Outcome::Unavailable);
        }

        app.put(Some(&cookie), &save_body(&doc, &[("Port", json!(8642))]))
            .await
            .assert_error(StatusCode::SERVICE_UNAVAILABLE, "client-unavailable");
        assert!(
            !app.calls().iter().any(|c| c.starts_with("write-if")),
            "{stop}: the manager is never asked to write: {:?}",
            app.calls()
        );
        assert!(app.writes().is_empty(), "{stop}: no write may be recorded");
        assert_eq!(app.file_text(), WITH_KEY, "{stop}: the file as it was");
        app.get("/api/session", Some(&cookie))
            .await
            .assert_signed_in_as("alice");
    }
}

/// The write reaches the manager and the stream dies with it, before any reply: the manager
/// may have applied the change, so the answer is 504 `client-timeout`, not 503. A 503 here
/// tells the user to look at the server, decide for themselves whether the write happened and
/// repeat it, which is the one thing that must not be suggested when the write may be
/// applied.
#[tokio::test]
async fn a_manager_lost_at_the_write_is_504() {
    let app = app_writing(WITH_KEY);
    let cookie = app.sign_in().await;
    let doc = app.read_default().await.document();

    // The write is sent and the reply never comes back.
    app.set_write_outcome(Outcome::Unavailable);

    app.put(Some(&cookie), &save_body(&doc, &[("Port", json!(8642))]))
        .await
        .assert_error(StatusCode::GATEWAY_TIMEOUT, "client-timeout");
    assert_eq!(
        app.calls()
            .iter()
            .filter(|c| c.starts_with("write-if"))
            .count(),
        1,
        "the write is sent: its outcome is unknown, so the answer is 504: {:?}",
        app.calls()
    );
    app.get("/api/session", Some(&cookie))
        .await
        .assert_signed_in_as("alice");
}

/// The write landed and only the status read that follows it is too slow: the
/// file holds the new value, the save is done, so the answer is the save's 200
/// with `running` true, the safe answer for a state the bff cannot see (the
/// rule
/// `running_is_true_when_the_status_read_after_the_write_fails` pins for a
/// status read that fails). A 504 here tells the user the file may be saved
/// and to look at the server, when the write is known to have landed.
#[tokio::test]
async fn a_time_limit_that_runs_out_after_the_write_landed_is_200() {
    let app = app_writing(WITH_KEY);
    let cookie = app.sign_in().await;
    let doc = app.read_default().await.document();

    app.set_status_pause_ms(3 * TIMEOUT.as_millis() as u64);

    let saved = assert_saved(
        &app.put(Some(&cookie), &save_body(&doc, &[("Port", json!(8642))]))
            .await,
    );
    assert_eq!(
        saved["running"],
        json!(true),
        "the state is unknown after a write that stands: the safe answer: {saved}"
    );
    assert_eq!(app.writes().len(), 1, "the write was asked");
    assert!(
        app.file_text().contains("Port = 8642"),
        "the answer says the file is saved, so it must be: {}",
        app.file_text()
    );
}

/// Any other failure: 502 `client-other`, never the manager's detail.
#[tokio::test]
async fn any_other_manager_failure_is_502() {
    let app = app_writing(WITH_KEY);
    let cookie = app.sign_in().await;
    let doc = app.read_default().await.document();
    app.set_write_outcome(Outcome::Other);
    app.put(Some(&cookie), &save_body(&doc, &[("Port", json!(8642))]))
        .await
        .assert_error(StatusCode::BAD_GATEWAY, "client-other");
}

/// The bff's own check refuses a value (threads above this machine's CPU
/// count): 422 with the check's key, its values, and `field` the INI key of
/// `args[0]`. The manager is not asked to write. The thread maximum is the
/// one the read's `limits` named, in the refusal's values too: the maximum
/// itself is accepted, one above it refused.
#[tokio::test]
async fn a_value_the_bff_check_refuses_is_422_naming_its_field() {
    let app = app_writing(WITH_KEY);
    let cookie = app.sign_in().await;
    let doc = app.read_default().await.document();
    let cpus = doc["limits"]["Threads"][1].as_i64().expect("the CPU count");
    let at_max = cpus.to_string();
    let above_max = (cpus + 1).to_string();
    let connects = app.connects.load(Ordering::SeqCst);
    app.put(
        Some(&cookie),
        &save_body(&doc, &[("Threads", json!(100_000))]),
    )
    .await
    .assert_refused(
        "not-whole-number-in-range",
        &["Threads", "100000", "1", &at_max],
        "Threads",
    );
    assert_eq!(
        app.connects.load(Ordering::SeqCst),
        connects,
        "no new connect"
    );
    assert!(app.writes().is_empty(), "nothing written");
    // The range's boundary is the machine's count the read named: it is
    // accepted, one above it is refused naming Threads. The second save
    // carries the revision the first one returned, as the form does.
    let saved = assert_saved(
        &app.put(Some(&cookie), &save_body(&doc, &[("Threads", json!(cpus))]))
            .await,
    );
    let body = json!({
        "revision": saved["revision"],
        "values": values_with(&doc, &[("Threads", json!(cpus + 1))]),
    });
    app.put(Some(&cookie), &body).await.assert_refused(
        "not-whole-number-in-range",
        &["Threads", &above_max, "1", &at_max],
        "Threads",
    );
}

/// The manager's check refuses a value the bff's let through: the save passes
/// the refusal on, 422 with its key, its values and `field` the INI key.
#[tokio::test]
async fn the_managers_value_refusal_is_422_naming_its_field() {
    let app = app_writing(WITH_KEY);
    let cookie = app.sign_in().await;
    let doc = app.read_default().await.document();
    app.set_write_outcome(Outcome::Invalid {
        key: "not-whole-number-in-range",
        values: vec!["Port".into(), "70000".into(), "1".into(), "65535".into()],
    });
    app.put(Some(&cookie), &save_body(&doc, &[("Port", json!(8642))]))
        .await
        .assert_refused(
            "not-whole-number-in-range",
            &["Port", "70000", "1", "65535"],
            "Port",
        );
    assert_eq!(app.file_text(), WITH_KEY, "nothing was written");
}

/// GT1: a `RocmHostMemoryGtt` that is not a bool is refused as a value, 422,
/// with `field` naming the key the form marks. The refusal is the real one
/// `checks::server_file` gives for a hand-edited file (the manager runs it on
/// the text the bff hands the write), not a key written by hand here: the rule
/// is free to name itself, and the field must still land on the key.
#[tokio::test]
async fn the_managers_bool_refusal_of_rocm_host_memory_gtt_is_422_naming_its_field() {
    let refusal = pai_core::checks::server_file("[Server]\nRocmHostMemoryGtt = sometimes\n", "")
        .expect_err("a non-bool value is refused");
    let args: Vec<&str> = refusal.values.iter().map(String::as_str).collect();

    let app = app_writing(WITH_KEY);
    let cookie = app.sign_in().await;
    let doc = app.read_default().await.document();
    app.set_write_outcome(Outcome::Invalid {
        key: refusal.key,
        values: refusal.values.clone(),
    });
    app.put(
        Some(&cookie),
        &save_body(&doc, &[("RocmHostMemoryGtt", json!(false))]),
    )
    .await
    .assert_refused(refusal.key, &args, "RocmHostMemoryGtt");
    assert_eq!(app.file_text(), WITH_KEY, "nothing was written");
}

/// The address checks answer 422 with their client key and the field the form
/// marks: the address in use marks `Port`, the host that does not resolve
/// marks `Hostname`.
#[tokio::test]
async fn an_address_in_use_fields_the_port_and_an_unresolvable_host_the_hostname() {
    let app = app_writing(WITH_KEY);
    let cookie = app.sign_in().await;
    let doc = app.read_default().await.document();

    app.set_write_outcome(Outcome::AddressInUse("llama.cpp@zz".into()));
    app.put(Some(&cookie), &save_body(&doc, &[("Port", json!(9999))]))
        .await
        .assert_refused("client-address-in-use", &["llama.cpp@zz"], "Port");

    app.set_write_outcome(Outcome::UnresolvableHost("nope.invalid".into()));
    app.put(
        Some(&cookie),
        &save_body(&doc, &[("Hostname", json!("nope.invalid"))]),
    )
    .await
    .assert_refused("client-unresolvable-host", &["nope.invalid"], "Hostname");
    assert_eq!(app.file_text(), WITH_KEY, "nothing was written");
}

/// The write was sent and the manager does not come back: 504 `client-timeout`
/// with no args, the bff's own answer within its time limit. The file may or
/// may not be saved: the write reached the manager.
#[tokio::test]
async fn a_write_that_never_returns_is_answered_504() {
    let app = app_writing(WITH_KEY);
    let cookie = app.sign_in().await;
    let doc = app.read_default().await.document();
    app.set_write_pause_ms(3 * TIMEOUT.as_millis() as u64);
    let started = Instant::now();
    app.put(Some(&cookie), &save_body(&doc, &[("Port", json!(8642))]))
        .await
        .assert_error(StatusCode::GATEWAY_TIMEOUT, "client-timeout");
    assert!(
        started.elapsed() < DEADLINE,
        "the bff answered within its own limit, not the request deadline"
    );
    assert_eq!(app.writes().len(), 1, "the write was sent to the manager");
}

/// The manager is busy on the write for longer than the read's limit and the
/// save answers 200: the calls of a write are bound by the write's own, longer
/// limit, so a manager that takes its time writing is not cut off at the limit
/// that is right for a read. The pair production uses is the read's
/// `MANAGER_TIMEOUT` and the writes' longer one; these tests hand their own
/// pair through `router_with_session_and_write_timeout`, so the timing is
/// theirs and not the machine's.
#[tokio::test]
async fn a_save_that_outlives_the_read_limit_is_answered_under_the_write_limit() {
    let slow = TIMEOUT.as_millis() as u64 * 2;
    let write_limit = Duration::from_millis(TIMEOUT.as_millis() as u64 * 3);
    let app = app_writing_with_write_timeout(WITH_KEY, write_limit);
    let cookie = app.sign_in().await;
    let doc = app.read_default().await.document();

    app.set_write_pause_ms(slow);
    let started = Instant::now();
    let saved = assert_saved(
        &app.put(Some(&cookie), &save_body(&doc, &[("Port", json!(8642))]))
            .await,
    );
    assert!(
        started.elapsed() >= TIMEOUT,
        "the save outlived the read's limit: {:?}",
        started.elapsed()
    );
    assert_eq!(saved["running"], json!(true), "{saved}");
    assert!(
        app.file_text().contains("Port = 8642"),
        "the write landed: {}",
        app.file_text()
    );
}

/// The longer limit is the write's and not the manager's: a read of the same
/// app, taking the same time as that save, is answered 503 `client-unavailable`
/// at the read's own limit, well before the write's runs out.
#[tokio::test]
async fn a_read_that_outlives_the_read_limit_is_still_answered_at_it() {
    let slow = TIMEOUT.as_millis() as u64 * 2;
    let write_limit = Duration::from_millis(TIMEOUT.as_millis() as u64 * 3);
    let app = app_writing_with_write_timeout(WITH_KEY, write_limit);
    let cookie = app.sign_in().await;
    app.set_read_pause_ms(slow);

    let started = Instant::now();
    app.get("/api/instances/default/server", Some(&cookie))
        .await
        .assert_error(StatusCode::SERVICE_UNAVAILABLE, "client-unavailable");
    assert!(
        started.elapsed() < write_limit,
        "the read was answered at the read's limit, not carried by the write's: {:?}",
        started.elapsed()
    );
}

// ── The GPUs of the profile: saving `[Server] Devices` (R2) ─────────────

/// A save of the GPU list lands in the file verbatim, `none` (CPU only)
/// included, and the next read gives it back as one string.
#[tokio::test]
async fn a_save_of_the_devices_lands_in_the_file() {
    for value in ["ROCm0,ROCm1", "none"] {
        let app = app_writing(SHIPPED);
        let cookie = app.sign_in().await;
        let doc = app.read_default().await.document();
        let reply = app
            .put(
                Some(&cookie),
                &save_body(&doc, &[("Devices", json!(value))]),
            )
            .await;
        assert_saved(&reply);
        assert!(
            app.file_text().contains(&format!("Devices = {value}")),
            "Devices = {value:?} is not in the file: {}",
            app.file_text()
        );
        let after = app
            .get("/api/instances/default/server", Some(&cookie))
            .await
            .document();
        assert_eq!(after["values"]["Devices"], json!(value), "{value:?}");
    }
}

/// The rule's refusal, named: a list with a space inside an entry is 422
/// `device-bad-name`, the field `Devices`, and nothing is written.
#[tokio::test]
async fn a_save_of_devices_the_rule_refuses_names_the_field_and_writes_nothing() {
    let app = app_writing(SHIPPED);
    let cookie = app.sign_in().await;
    let doc = app.read_default().await.document();
    app.put(
        Some(&cookie),
        &save_body(&doc, &[("Devices", json!("ROCm0, ROCm1"))]),
    )
    .await
    .assert_refused("device-bad-name", &["Devices", " ROCm1"], "Devices");
    assert!(app.writes().is_empty(), "nothing written");
    assert_eq!(app.file_text(), SHIPPED, "the file is untouched");
}

/// The marker rule, on the same key: a `;` or `#` saved into `Devices` is the
/// INI comment marker, so the save is 422 `comment-markers` with the field
/// `Devices`, and nothing is written: `ROCm0;x` in a file reloads as `ROCm0`,
/// GPUs nobody typed.
#[tokio::test]
async fn a_save_of_devices_with_a_comment_marker_is_422_comment_markers() {
    for value in ["ROCm0;x", "ROCm0#y"] {
        let app = app_writing(SHIPPED);
        let cookie = app.sign_in().await;
        let doc = app.read_default().await.document();
        app.put(
            Some(&cookie),
            &save_body(&doc, &[("Devices", json!(value))]),
        )
        .await
        .assert_refused("comment-markers", &["Devices"], "Devices");
        assert!(app.writes().is_empty(), "{value:?}: nothing written");
        assert_eq!(app.file_text(), SHIPPED, "{value:?}: the file is untouched");
    }
}

/// The done-when clause: a save of another key keeps the GPUs. The read's
/// document carries the value, so the rendered file carries the line.
#[tokio::test]
async fn a_save_of_another_key_keeps_the_devices() {
    let text = "[Server]\nPort = 8580\nDevices = CUDA0,ROCm1\n";
    let app = app_writing(text);
    let cookie = app.sign_in().await;
    let doc = app.read_default().await.document();
    assert_saved(
        &app.put(Some(&cookie), &save_body(&doc, &[("Port", json!(8642))]))
            .await,
    );
    let contents = app.file_text();
    assert!(
        contents.contains("Devices = CUDA0,ROCm1"),
        "the save of Port lost the GPUs: {contents}"
    );
    assert_eq!(
        pai_core::server_cfg::from_text(&contents)
            .devices
            .as_deref(),
        Some("CUDA0,ROCm1"),
        "the written file still names the GPUs"
    );
}

/// Unsetting the GPUs is a save of `null`: no key in the file, every GPU.
#[tokio::test]
async fn a_save_of_no_devices_writes_no_key() {
    let text = "[Server]\nPort = 8580\nDevices = ROCm0\n";
    let app = app_writing(text);
    let cookie = app.sign_in().await;
    let doc = app.read_default().await.document();
    assert_saved(
        &app.put(Some(&cookie), &save_body(&doc, &[("Devices", Value::Null)]))
            .await,
    );
    let contents = app.file_text();
    assert!(
        !contents
            .lines()
            .any(|l| l.trim_start().starts_with("Devices")),
        "an unset Devices left a line: {contents}"
    );
    assert_eq!(
        pai_core::server_cfg::from_text(&contents).devices,
        None,
        "unset: every GPU"
    );
}

// ── The tags of the profile ─────────────────────────────────────────────

/// The tags of the profile travel in the form's document like every other
/// value: the read gives the list in its order, the save writes it back, and a
/// save of a document read unchanged keeps them. The save writes the whole
/// file from the document, so a document that carried no `Tags` would erase
/// the profile's tags.
#[tokio::test]
async fn the_tags_of_the_profile_travel_in_the_form_s_document() {
    let app = app_writing(WITH_KEY);
    let cookie = app.sign_in().await;
    let doc = app.read_default().await.document();

    app.put(
        Some(&cookie),
        &save_body(&doc, &[("Tags", json!(["edge", "v2"]))]),
    )
    .await;
    assert!(
        app.file_text().contains("Tags = edge,v2"),
        "the list as the form sent it: {}",
        app.file_text()
    );
    let after = app
        .get("/api/instances/default/server", Some(&cookie))
        .await
        .document();
    assert_eq!(
        after["values"]["Tags"],
        json!(["edge", "v2"]),
        "read back as it was written"
    );

    let app = app_writing("[Server]\nPort = 8580\nTags = edge,v2\n");
    let cookie = app.sign_in().await;
    let doc = app.read_default().await.document();
    assert_eq!(
        doc["values"]["Tags"],
        json!(["edge", "v2"]),
        "the file's list, in its order"
    );
    app.put(Some(&cookie), &save_body(&doc, &[])).await;
    assert!(
        app.file_text().contains("Tags = edge,v2"),
        "a save of the document read unchanged keeps the tags: {}",
        app.file_text()
    );
}

/// An empty list clears them: the file carries no line for `Tags` (an empty
/// key is not written).
#[tokio::test]
async fn no_tags_writes_no_line() {
    let app = app_writing("[Server]\nPort = 8580\nTags = edge,v2\n");
    let cookie = app.sign_in().await;
    let doc = app.read_default().await.document();
    app.put(Some(&cookie), &save_body(&doc, &[("Tags", json!([]))]))
        .await;
    let text = app.file_text();
    let lines: Vec<&str> = text.lines().filter(|l| l.starts_with("Tags")).collect();
    assert!(lines.is_empty(), "no tags, no line: {lines:?}\n{text}");
}

/// A `Tags` that is not a list of strings is 400 `server-malformed` and
/// nothing is written: a bare word, a number, a list with a number in it, a
/// list of lists, null.
#[tokio::test]
async fn a_tags_that_is_not_a_list_of_strings_is_server_malformed() {
    let app = app_writing(WITH_KEY);
    let cookie = app.sign_in().await;
    let doc = app.read_default().await.document();
    for tags in [
        json!("edge"),
        json!(5),
        json!(["edge", 5]),
        json!([["edge"]]),
        json!(null),
    ] {
        let r = app
            .put(Some(&cookie), &save_body(&doc, &[("Tags", tags.clone())]))
            .await;
        r.assert_error(StatusCode::BAD_REQUEST, "server-malformed");
    }
    assert!(app.writes().is_empty(), "nothing written");
    assert_eq!(app.file_text(), WITH_KEY, "the file as it was");
}

/// A `Tags` list of strings the rule refuses (a capital letter, a tag twice,
/// one with a space) is 422 `server-bad-tags` with the rule's args and `field`
/// `tags`, the form's one control for the list: the bff runs the manager's
/// rule on the body before it asks for the write, so the manager is not asked
/// and the file stays as it was.
#[tokio::test]
async fn a_tags_list_the_rule_refuses_is_422_naming_the_tags_field() {
    let app = app_writing(WITH_KEY);
    let cookie = app.sign_in().await;
    let doc = app.read_default().await.document();
    let connects = app.connects.load(Ordering::SeqCst);
    for (tags, reason, list) in [
        (json!(["Edge"]), "bad characters", "Edge"),
        (json!(["gpu", "gpu"]), "the same tag twice", "gpu,gpu"),
        (json!(["gpu edge"]), "bad characters", "gpu edge"),
    ] {
        app.put(Some(&cookie), &save_body(&doc, &[("Tags", tags)]))
            .await
            .assert_refused("server-bad-tags", &[reason, list, "8", "1", "32"], "tags");
    }
    assert_eq!(
        app.connects.load(Ordering::SeqCst),
        connects,
        "the bff's own check: the manager is not asked again"
    );
    assert!(app.writes().is_empty(), "nothing written");
    assert_eq!(app.file_text(), WITH_KEY, "the file as it was");
}

// ── The server profile: the file is the record's, never the id's (slice 13) ──

/// A profile file of its own, easy to tell from the shipped `server@default.ini`
/// the manager holds beside it: a port nothing else in this file uses.
const BIG: &str = "[Server]\nPort = 9011\n";

/// The record names the instance, and its section names a profile that is not
/// the instance's name: the read is that profile's file. The record is asked
/// first, and the file of the id is never opened.
#[tokio::test]
async fn the_read_is_the_profile_the_record_names() {
    let app = app_writing(SHIPPED);
    app.set_server_profile("default", Some("big"));
    app.set_file_named("server@big.ini", BIG);
    let doc = app.read_default().await.document();
    assert_eq!(
        doc["values"]["Port"],
        json!(9011),
        "the profile's file: {doc}"
    );
    assert_eq!(
        app.calls(),
        ["instances", "read server@big.ini"],
        "the record, then the profile's file, never server@default.ini"
    );
}

/// The save of the same instance goes to the profile's file: the compare and
/// swap names it, its text changes, and the file of the id is left as it was.
#[tokio::test]
async fn the_save_is_the_profile_the_record_names() {
    let app = app_writing(SHIPPED);
    app.set_server_profile("default", Some("big"));
    app.set_file_named("server@big.ini", WITH_KEY);
    let cookie = app.sign_in().await;
    let doc = app
        .get("/api/instances/default/server", Some(&cookie))
        .await
        .document();
    assert_eq!(doc["values"]["Port"], json!(8580), "{doc}");
    assert_saved(
        &app.put(Some(&cookie), &save_body(&doc, &[("Port", json!(8642))]))
            .await,
    );
    assert_eq!(
        app.writes()
            .iter()
            .map(|(name, _, _)| name.as_str())
            .collect::<Vec<_>>(),
        ["server@big.ini"],
        "the write names the profile's file"
    );
    let saved = app
        .file_named("server@big.ini")
        .expect("the profile's file");
    assert!(saved.contains("Port = 8642"), "{saved}");
    assert!(
        saved.contains("OpencodeApiKey = sk-test"),
        "the key of the file the save read: {saved}"
    );
    assert_eq!(
        app.file_text(),
        SHIPPED,
        "the file of the id is not touched"
    );
    let calls = app.calls();
    assert!(
        !calls.iter().any(|c| c.contains("server@default.ini")),
        "no call names the file of the id: {calls:?}"
    );
    assert_eq!(
        calls,
        [
            "instances",
            "read server@big.ini",
            "instances",
            "read server@big.ini",
            "write-if alice server@big.ini",
            "status default",
        ],
        "the form's read, then the save's own record read, read, write, status"
    );
}

/// A record that names no profile for the instance is no instance: 404
/// `client-no-instance`, and no file is read. There is no reading of a file
/// named after the instance, whatever the record says.
#[tokio::test]
async fn a_row_naming_no_profile_is_no_instance() {
    let app = app_writing(SHIPPED);
    app.set_server_profile("default", None);
    let r = app.read_default().await;
    r.assert_error(StatusCode::NOT_FOUND, "client-no-instance");
    assert_eq!(
        app.calls(),
        ["instances"],
        "the record answers, nothing is read"
    );
}

/// An id the record does not name is no instance: 404 `client-no-instance`,
/// the record asked and no file opened, so an id typed into the address bar
/// cannot reach a file the record does not name.
#[tokio::test]
async fn an_id_the_record_does_not_name_is_no_instance_and_asks_no_file() {
    let app = app_writing(SHIPPED);
    app.set_instances(&["other"]);
    let cookie = app.sign_in().await;
    app.get("/api/instances/default/server", Some(&cookie))
        .await
        .assert_error(StatusCode::NOT_FOUND, "client-no-instance");
    app.put(Some(&cookie), &shaped_save_body().await)
        .await
        .assert_error(StatusCode::NOT_FOUND, "client-no-instance");
    assert_eq!(
        app.calls(),
        ["instances", "instances"],
        "the read and the save each asked the record, and nothing else"
    );
    assert!(app.writes().is_empty(), "nothing written");
}

/// The record that cannot be read is the manager's own news, the same key as
/// on every other read: 503 `client-unavailable`, no file read, and the
/// session lives on.
#[tokio::test]
async fn a_record_that_cannot_be_read_is_unavailable_and_asks_no_file() {
    let app = app_writing(SHIPPED);
    app.set_instances_outcome(Outcome::Unavailable);
    let cookie = app.sign_in().await;
    app.get("/api/instances/default/server", Some(&cookie))
        .await
        .assert_error(StatusCode::SERVICE_UNAVAILABLE, "client-unavailable");
    app.put(Some(&cookie), &shaped_save_body().await)
        .await
        .assert_error(StatusCode::SERVICE_UNAVAILABLE, "client-unavailable");
    app.get("/api/session", Some(&cookie))
        .await
        .assert_signed_in_as("alice");
    assert_eq!(
        app.calls(),
        ["instances", "instances"],
        "the record failed, no file was read"
    );
    assert!(app.writes().is_empty(), "nothing written");
}
