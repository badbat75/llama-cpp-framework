//! `PUT /api/presets/{id}/enabled`: the preset on/off switch of the Models
//! page (PLAN.md step 10, item 11b; ARCHITECTURE.md "The switch on and off
//! (item 11b)"; `fe/briefs/api-contract.md`), through
//! `pai_bff::router_with_session` with a fake manager client and a fake
//! authenticator (never the real bus, pipe, PAM or LogonUser). Each request
//! runs under a deadline, so a bff that waits on the manager fails instead of
//! hanging.
//!
//! It is a file of its own beside `tests/presets.rs` (the list), because the
//! switch asks the manager for more than a read: `write_file_if` (the
//! compare and swap), `write_file` (only for a disabled file that does not
//! exist yet) and the state of the instances, for `running`. The fake here
//! answers those for real: a compare and swap lands only when `expected` is
//! the text the fake answered for that file in this very test, a file that is
//! not there is answered `Changed` as the real manager answers it, and each
//! write is recorded as `(name, contents, expected)` so a test can pin the
//! order of the two writes, the whole text of each and what each expected.
//! The plain writes are recorded apart from the compare and swaps, so a test
//! pins which channel the bff used for a file.
//!
//! The rule (the design's slice 5):
//! - the body is `{"enabled": true|false}`; anything else is 400
//!   `presets-malformed`, with the manager not asked;
//! - a session is needed: none, or an unknown one, is 401 `session-required`
//!   and the manager is not connected to, so nothing is written. The switch
//!   acts for the signed-in user: `Connect` is asked once, with that user as
//!   the principal; a request naming a user is never believed.
//! - the id is a section name, matched in any letter case, never a path: the
//!   two file names are fixed, so nothing comes from the URL. An id neither
//!   file holds is 404 `no-preset-named` with `args: [id]`.
//! - the preset moves, unchanged, between `presets.ini` and
//!   `presets.ini.disabled`: the file the preset LANDS IN is written FIRST,
//!   because a preset in both files reads as enabled. Switching off writes the
//!   disabled file, then `presets.ini`; switching on writes `presets.ini`,
//!   then the disabled one. Each write is a compare and swap expecting the
//!   text the bff read in this request.
//! - a disabled file that does not exist is created with the configurator's
//!   header (`DISABLED_FILE_HEADER`), as a plain `write_file`: the compare and
//!   swap refuses a missing file with `Changed`, so it cannot create it.
//! - an id that is already where it is asked for answers 200 and writes
//!   nothing.
//! - 409 `preset-changed-elsewhere` (args: none) when a compare and swap
//!   found its file changed. When it answers the SECOND write the preset is
//!   in both files: it reads enabled, and the next switch of that preset
//!   mends it.
//! - 422 `{"key", "args", "field"}` for the manager's refusal of the moved
//!   section, `field` by the design's table (`load-on-startup` for both
//!   `load-on-startup-*`, no `field` for `invalid-preset-id`), and nothing is
//!   written.
//! - 403 `session-not-administrator` (the served sibling rule: the sign-in's
//!   key for the same fact, and the session ends) when the manager refuses
//!   the read or the write for the signed-in user; 503 `client-unavailable`
//!   while the manager has not been asked to write; 504 `client-timeout` from
//!   the moment a write was asked and its outcome is unknown; 502
//!   `client-other` for anything else, never the manager's detail.
//! - every answer says `running`: true when any instance of the engine is
//!   `active` or `activating` (one `list_instances`, then the `status` of each
//!   until one answers so), and true when a status read fails after a write
//!   that landed. The bff restarts nothing.
//!
//! The time limits are the bff's own and they end the manager's task when
//! they fire (`crates/bff/src/server.rs`): a switch whose read is parked is
//! answered 503 and the task it leaves behind must not write, so the files
//! keep the text they had. A gate stops a manager call until the test
//! releases it, `GRACE` is how long a released call is given to reach a write,
//! and `GATE_LIMIT` lets an unreleased call through, so a test fails, it does
//! not hang.
//!
//! These are the bff's tests, with the manager faked: the switch against the
//! real manager, the real compare and swap and the real checks are the
//! manager's to run.

use axum::body::Body;
use axum::extract::ConnectInfo;
use axum::http::{header, Method, Request, StatusCode};
use axum::Router;
use http_body_util::BodyExt;
use pai_bff::session::{AuthError, Authenticator, Identity};
use pai_bff::{Client, Connect};
use pai_core::checks::Refusal;
use pai_core::client::{ClientError, InstanceState, ManagerClient, ManagerFile};
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

/// How long a test waits, after the manager call it stopped has returned, for
/// a blocking task that was not cut off to reach a write.
const GRACE: Duration = Duration::from_millis(500);

/// How long a stopped call in the fake holds at most. The test releases it; a
/// gate nobody releases lets the call through, so a test fails, it does not
/// hang.
const GATE_LIMIT: Duration = Duration::from_secs(5);

const CLIENT: &str = "127.0.0.1:51000";

/// The revision secret of these tests: the switch sends no revision, the
/// router just needs one.
const KEY: [u8; 16] = [0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15];

/// The names the manager knows the engine's preset files by.
const PRESETS: &str = "presets.ini";
const DISABLED: &str = "presets.ini.disabled";

/// The ids these tests switch. `Qwen` is spelled as the file spells it, so a
/// bff that matches the id case-sensitively is caught; it carries
/// `load-on-startup`, the key the manager counts against the lowest
/// `ModelsMax` (`crates/core/src/checks.rs`).
const QWEN: &str = "Qwen";
const ALPHA: &str = "alpha";
const MISTRAL: &str = "mistral-7b";

/// A `presets.ini` with two switched-on presets, in a file order that is
/// neither alphabetical nor reversed.
const FILE_ON: &str = "\
; The presets llama-server is offered (--models-preset).\r\n\
[alpha]\r\n\
model = /models/alpha.gguf\r\n\
\r\n\
[Qwen]\r\n\
model = /models/Qwen3.8-27B-Q8_0.gguf\r\n\
load-on-startup = 1\r\n\
";

/// A `presets.ini.disabled` with one switched-off preset, written as the
/// configurator writes it (its header, then the sections).
const FILE_OFF: &str = "\
; Presets switched OFF in pai-config (the switch beside each preset in\r\n\
; the Models tab). Same format as presets.ini, but llama-server only reads\r\n\
; presets.ini, so nothing here is offered. Switching a preset back on moves its\r\n\
; section, unchanged, back to presets.ini.\r\n\
[mistral-7b]\r\n\
model = /models/Mistral-7B-Instruct-v0.3-Q5_K_M.gguf\r\n\
";

/// The header a disabled file the configurator never wrote is created with,
/// as of today: the text of `DISABLED_FILE_HEADER`. The bff links no
/// `pai-app`, so the text is copied here, and
/// `the_disabled_header_is_the_configurators_one` keeps the bff from
/// carrying its own copy and the constant from being where it cannot link it.
const DISABLED_HEADER: &str = "\
; Presets switched OFF in pai-config (the switch beside each preset in\r\n\
; the Models tab). Same format as presets.ini, but llama-server only reads\r\n\
; presets.ini, so nothing here is offered. Switching a preset back on moves its\r\n\
; section, unchanged, back to presets.ini.\r\n";

// ── The texts a switch answers ──────────────────────────────────────────

/// The section of `id` as it sits in `source`, the bytes the move copies.
fn section_of(source: &str, id: &str) -> String {
    pai_core::ini::text::section_text(source, id).unwrap_or_else(|| panic!("[{id}] in {source:?}"))
}

/// The text of `destination` once the section of `source` is put into it: the
/// text of the file the preset lands in (`ini::text::replace_section`, the
/// transform the GUI's move uses).
fn landed(destination: &str, source: &str, id: &str) -> String {
    pai_core::ini::text::replace_section(destination, id, &section_of(source, id))
}

/// The text of `source` once its section is taken out of it: the text of the
/// file the preset leaves (`ini::text::delete_section`).
fn left(source: &str, id: &str) -> String {
    pai_core::ini::text::delete_section(source, id)
        .unwrap_or_else(|| panic!("[{id}] in {source:?}"))
}

/// `FILE_ON` with `Qwen` moved out of it: the `presets.ini` of the state the
/// web UI's switch starts from for the switch-on tests.
fn file_on_without_qwen() -> String {
    left(FILE_ON, QWEN)
}

/// `FILE_OFF` with `Qwen`'s section added to it: the disabled file after the
/// switch off landed.
fn file_off_with_qwen() -> String {
    landed(FILE_OFF, FILE_ON, QWEN)
}

/// The disabled file created from nothing by the switch off.
fn created_with_qwen() -> String {
    landed(DISABLED_HEADER, FILE_ON, QWEN)
}

/// `FILE_ON` with `mistral-7b`'s section added: the `presets.ini` after the
/// switch on landed.
fn file_on_with_mistral() -> String {
    landed(FILE_ON, FILE_OFF, MISTRAL)
}

/// `FILE_OFF` with `mistral-7b` moved out of it: the disabled file after the
/// switch on landed.
fn file_off_without_mistral() -> String {
    left(FILE_OFF, MISTRAL)
}

// ── Fakes ───────────────────────────────────────────────────────────────

/// What a manager call answers when the test asks it to fail: the failures a
/// switch can meet between reading the two files and writing them.
#[derive(Clone)]
enum Outcome {
    /// The compare and swap: the file no longer holds what the bff read.
    Changed,
    Unavailable,
    Other,
    NotAuthorized,
    /// The manager's check of the moved section: its key and values, as the
    /// wire sends them.
    Invalid {
        key: &'static str,
        values: Vec<String>,
    },
}

impl Outcome {
    fn into_error(self) -> ClientError {
        match self {
            Outcome::Changed => ClientError::Changed,
            Outcome::Unavailable => ClientError::Unavailable,
            Outcome::Other => ClientError::Other("the manager failed".into()),
            Outcome::NotAuthorized => ClientError::NotAuthorized,
            Outcome::Invalid { key, values } => ClientError::Invalid(Refusal { key, values }),
        }
    }
}

type Log = Arc<Mutex<Vec<String>>>;

/// The principal every connect was asked for, in order: `None` is a call the
/// bff makes as its own account.
type Principals = Arc<Mutex<Vec<Option<String>>>>;

fn principals_of(p: &Principals) -> Vec<Option<String>> {
    p.lock().unwrap_or_else(|e| e.into_inner()).clone()
}

/// The files the manager keeps live for the switch: the current text of each,
/// the compare-and-swap writes it was asked for (`name`, `contents`,
/// `expected`), the plain writes (`name`, `contents`), the principals whose
/// read or write it refuses, the failures to answer a connect, a read, a
/// write and a status with (one of them for a given write position, so a test
/// can put `Changed` on the SECOND write of a move), how long each takes, the
/// gates, and the instances with their state.
///
/// Every cell is inert by default, so a test that does not touch one is
/// untouched by it. Shared by every connect of one app, so a test can read a
/// file, change it behind the switch, refuse a call, switch and read again.
#[derive(Clone, Default)]
struct Live {
    files: Arc<Mutex<HashMap<String, String>>>,
    writes: Arc<Mutex<Vec<(String, String, String)>>>,
    plain_writes: Arc<Mutex<Vec<(String, String)>>>,
    write_refused: Arc<Mutex<Vec<String>>>,
    read_refused: Arc<Mutex<Vec<String>>>,
    /// What a coming call answers, in place of landing.
    connect_outcome: Arc<Mutex<Option<Outcome>>>,
    read_outcome: Arc<Mutex<Option<Outcome>>>,
    write_outcome: Arc<Mutex<Option<Outcome>>>,
    status_outcome: Arc<Mutex<Option<Outcome>>>,
    /// The failure of the write at one position (1, 2, ...), in place of the
    /// plain `write_outcome`: how a test answers the second write of a move
    /// with `Changed` and lets the first land.
    write_outcome_at: Arc<Mutex<Option<(usize, Outcome)>>>,
    /// How long each takes.
    connect_pause_ms: Arc<AtomicU64>,
    read_pause_ms: Arc<AtomicU64>,
    write_pause_ms: Arc<AtomicU64>,
    status_pause_ms: Arc<AtomicU64>,
    /// A call stopped inside the manager call until the test releases it (see
    /// `gate`): armed stops the next one of that kind.
    read_gate_armed: Arc<AtomicBool>,
    read_gate_released: Arc<AtomicBool>,
    reads_gated: Arc<AtomicUsize>,
    reads_released: Arc<AtomicUsize>,
    write_gate_armed: Arc<AtomicBool>,
    write_gate_released: Arc<AtomicBool>,
    writes_gated: Arc<AtomicUsize>,
    writes_released: Arc<AtomicUsize>,
    connect_gate_armed: Arc<AtomicBool>,
    connect_gate_released: Arc<AtomicBool>,
    connects_gated: Arc<AtomicUsize>,
    connects_released: Arc<AtomicUsize>,
    /// The instances, and the state of each: `running` is true when one of
    /// them is active or activating.
    instances: Arc<Mutex<Vec<String>>>,
    states: Arc<Mutex<HashMap<String, InstanceState>>>,
    /// How many writes of any kind the manager was asked for, so
    /// `write_outcome_at` counts the plain ones too.
    write_count: Arc<AtomicUsize>,
}

impl Live {
    /// The instances, in the order the manager lists them.
    fn set_instances(&self, ids: &[&str]) {
        *self.instances.lock().unwrap_or_else(|e| e.into_inner()) =
            ids.iter().map(|i| i.to_string()).collect();
    }
    fn set_state(&self, id: &str, state: InstanceState) {
        self.states
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .insert(id.to_string(), state);
    }
    /// A manager with one instance, idle: the answer for every test that does
    /// not look at `running`.
    fn idle(&self) {
        self.set_instances(&["default"]);
        self.set_state("default", InstanceState::Inactive);
    }
}

/// A call a test stops: armed for the next one, it holds there until the test
/// releases it, counting the calls that stopped in `gated` and the ones the
/// test let go in `released_calls`. This is how a test stops the switch
/// inside the connect, inside the read and inside the write with no fixed
/// sleep: the route answers past its time limit, the test then lets the
/// parked call return and watches whether the task goes on to write. A call
/// holds at most `GATE_LIMIT`, so a gate nobody releases lets the call
/// through.
///
/// The switch runs its connect, reads and writes in one `spawn_blocking`
/// task, so a parked call holds a blocking-pool thread and not the route; a
/// route parking a call on the async path (the session routes) would be
/// stopped by it, because `#[tokio::test]` runs the test on a single thread.
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
/// `Live::connect_pause_ms` and refuses with `Live::connect_outcome` when one
/// is set. Both cells are inert by default.
///
/// Set them from a test of the switch only: the switch runs its connect,
/// reads and writes in one blocking task, so a sleeping connect stops that
/// task and not the route.
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

/// A manager that answers `read_file` from the files it holds (a name it does
/// not hold is a file that does not exist), writes them under the compare and
/// swap, logs every call (`read <name>`, `write-if <principal> <name>`,
/// `write <name>`, `status <id>`, ...) and refuses the reads of the
/// principals in `refuse`, the way the real manager refuses a user who is no
/// longer a member of `pai`. It is the client of one connect, so it knows the
/// principal that connect was handed (`as_user`).
struct FakeManager {
    live: Live,
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

    /// The refusal of the read of this connect's principal: a principal in
    /// `Live::refuse` is refused every call, one in `Live::read_refused` only
    /// the reads (a refusal before the switch reaches a write).
    fn read_refused(&self) -> bool {
        self.in_refuse_list()
            || self
                .live
                .read_refused
                .lock()
                .unwrap_or_else(|e| e.into_inner())
                .contains(&self.principal())
    }

    /// The refusal of the write of this connect's principal: the member of
    /// `pai` removed between the read and the write.
    fn write_refused(&self) -> bool {
        self.in_refuse_list()
            || self
                .live
                .write_refused
                .lock()
                .unwrap_or_else(|e| e.into_inner())
                .contains(&self.principal())
    }

    /// The principal of this connect, as the manager sees it.
    fn principal(&self) -> String {
        self.as_user.clone().unwrap_or_default()
    }

    /// A principal the test put in `Live::refuse` is refused every call.
    fn in_refuse_list(&self) -> bool {
        self.refuse
            .iter()
            .any(|u| Some(u.as_str()) == self.as_user.as_deref())
    }

    /// What the manager answers the write at `position` (1-based), counting
    /// the writes asked so far: the one armed for this position wins over the
    /// one armed for every write.
    fn write_failure(&self, position: usize) -> Option<ClientError> {
        if let Some((at, outcome)) = self
            .live
            .write_outcome_at
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .clone()
        {
            if at == position {
                return Some(outcome.into_error());
            }
        }
        self.live
            .write_outcome
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .clone()
            .map(Outcome::into_error)
    }
}

impl ManagerClient for FakeManager {
    fn read_file(&self, name: &str) -> Result<Option<ManagerFile>, ClientError> {
        self.record(format!("read {name}"));
        if self.read_refused() {
            return Err(ClientError::NotAuthorized);
        }
        gate(
            &self.live.read_gate_armed,
            &self.live.read_gate_released,
            &self.live.reads_gated,
            &self.live.reads_released,
        );
        let pause = self.live.read_pause_ms.load(Ordering::SeqCst);
        if pause > 0 {
            std::thread::sleep(Duration::from_millis(pause));
        }
        if let Some(outcome) = self
            .live
            .read_outcome
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .clone()
        {
            return Err(outcome.into_error());
        }
        Ok(self
            .live
            .files
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .get(name)
            .map(|text| ManagerFile {
                text: text.clone(),
                withheld: false,
            }))
    }

    /// The compare and swap, as the real manager's is: it lands only while
    /// the file still holds the text the caller read. A file that is not
    /// there is `Changed` (that is why a missing disabled file is created
    /// with `write_file`, not with this).
    fn write_file_if(&self, name: &str, contents: &str, expected: &str) -> Result<(), ClientError> {
        let principal = self.as_user.clone().unwrap_or_default();
        self.record(format!("write-if {principal} {name}"));
        let position = self.live.write_count.fetch_add(1, Ordering::SeqCst) + 1;
        self.live
            .writes
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .push((name.to_string(), contents.to_string(), expected.to_string()));
        if self.write_refused() {
            return Err(ClientError::NotAuthorized);
        }
        if let Some(failure) = self.write_failure(position) {
            return Err(failure);
        }
        gate(
            &self.live.write_gate_armed,
            &self.live.write_gate_released,
            &self.live.writes_gated,
            &self.live.writes_released,
        );
        let pause = self.live.write_pause_ms.load(Ordering::SeqCst);
        if pause > 0 {
            std::thread::sleep(Duration::from_millis(pause));
        }
        let mut files = self.live.files.lock().unwrap_or_else(|e| e.into_inner());
        match files.get(name) {
            Some(current) if current == expected => {
                files.insert(name.to_string(), contents.to_string());
                Ok(())
            }
            // The file moved, or is not there: the manager wrote nothing.
            _ => Err(ClientError::Changed),
        }
    }

    /// The unguarded write, for the file the compare and swap cannot create.
    fn write_file(
        &self,
        name: &str,
        contents: &str,
        keep_api_key: bool,
    ) -> Result<(), ClientError> {
        let principal = self.as_user.clone().unwrap_or_default();
        self.record(format!("write {principal} {name}"));
        let position = self.live.write_count.fetch_add(1, Ordering::SeqCst) + 1;
        self.live
            .plain_writes
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .push((name.to_string(), contents.to_string()));
        assert!(
            !keep_api_key,
            "a preset file has no API key to keep: the switch has no reason to ask"
        );
        if self.write_refused() {
            return Err(ClientError::NotAuthorized);
        }
        if let Some(failure) = self.write_failure(position) {
            return Err(failure);
        }
        gate(
            &self.live.write_gate_armed,
            &self.live.write_gate_released,
            &self.live.writes_gated,
            &self.live.writes_released,
        );
        let pause = self.live.write_pause_ms.load(Ordering::SeqCst);
        if pause > 0 {
            std::thread::sleep(Duration::from_millis(pause));
        }
        self.live
            .files
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .insert(name.to_string(), contents.to_string());
        Ok(())
    }

    fn list_instances(&self) -> Result<Vec<String>, ClientError> {
        self.record("list".into());
        Ok(self
            .live
            .instances
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .clone())
    }
    fn status(&self, id: &str) -> Result<InstanceState, ClientError> {
        self.record(format!("status {id}"));
        let pause = self.live.status_pause_ms.load(Ordering::SeqCst);
        if pause > 0 {
            std::thread::sleep(Duration::from_millis(pause));
        }
        if let Some(outcome) = self
            .live
            .status_outcome
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .clone()
        {
            return Err(outcome.into_error());
        }
        Ok(self
            .live
            .states
            .lock()
            .unwrap_or_else(|e| e.into_inner())
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

/// The app: a manager holding the files, refusing the calls of `refuse`, and
/// answering every connect with `connect_result`.
fn app_for(
    files: &[(&str, &str)],
    refuse: &[&str],
    connect_result: Result<(), ClientError>,
) -> App {
    let live = Live::default();
    live.idle();
    {
        let mut held = live.files.lock().unwrap();
        for (name, text) in files {
            held.insert((*name).to_string(), (*text).to_string());
        }
    }
    let log = Log::default();
    let connects = Arc::new(AtomicUsize::new(0));
    let principals = Principals::default();
    let (l, c, p) = (log.clone(), connects.clone(), principals.clone());
    let shared = live.clone();
    let refuse: Vec<String> = refuse.iter().map(|u| u.to_string()).collect();
    let connect: Connect = Arc::new(move |principal: Option<&str>| {
        c.fetch_add(1, Ordering::SeqCst);
        p.lock()
            .unwrap_or_else(|e| e.into_inner())
            .push(principal.map(str::to_string));
        connect_result.clone()?;
        connect_hook(&shared)?;
        Ok(Box::new(FakeManager {
            live: shared.clone(),
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
        live,
    }
}

/// A router over a manager that keeps the preset files live, with the
/// manager's call log, the number of connects and the principal of each.
struct App {
    router: Router,
    log: Log,
    connects: Arc<AtomicUsize>,
    principals: Principals,
    live: Live,
}

/// A manager holding `on` as `presets.ini` and `off` as `presets.ini.disabled`:
/// the pair that shows a preset on and a preset off.
fn app_holding(on: &str, off: &str) -> App {
    app_for(&[(PRESETS, on), (DISABLED, off)], &[], Ok(()))
}

/// A manager holding `on` as `presets.ini` and no `presets.ini.disabled`: the
/// state of a machine where nothing was ever switched off, the one the switch
/// off has to create.
fn app_holding_no_disabled(on: &str) -> App {
    app_for(&[(PRESETS, on)], &[], Ok(()))
}

/// A manager holding `on` as `presets.ini`, refusing the reads of the named
/// principals: the member of `pai` removed after the sign-in.
fn app_refusing(refuse: &[&str], on: &str, off: &str) -> App {
    app_for(&[(PRESETS, on), (DISABLED, off)], refuse, Ok(()))
}

/// The manager of no preset file at all: neither file exists, so no id is
/// named by any switch.
fn app_no_files() -> App {
    app_for(&[], &[], Ok(()))
}

/// The app that cannot reach the manager at all.
fn app_no_manager() -> App {
    app_for(
        &[(PRESETS, FILE_ON), (DISABLED, FILE_OFF)],
        &[],
        Err(ClientError::Unavailable),
    )
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
        self.assert_error_with_args(status, key, &[]);
    }

    /// An error answer with its args spelled out.
    fn assert_error_with_args(&self, status: StatusCode, key: &str, args: &[&str]) {
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

    /// A 422 value refusal: `{"key", "args", "field"}`, the key the form
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

    /// A 422 refusal that names no field: `{"key", "args"}`, with no `field`
    /// member (`invalid-preset-id`, `preset-name-taken`).
    fn assert_refused_without_field(&self, key: &str, args: &[&str]) {
        assert_eq!(
            self.status,
            StatusCode::UNPROCESSABLE_ENTITY,
            "{}",
            self.body
        );
        let doc = self.json();
        assert_eq!(doc["key"], json!(key), "{}", self.body);
        assert_eq!(doc["args"], json!(args), "{}", self.body);
        assert!(
            doc.get("field").is_none(),
            "no field for {key}: {}",
            self.body
        );
    }

    /// The switch's 200: exactly `{"enabled", "running"}`, answered as JSON.
    fn assert_switched(&self, enabled: bool, running: bool) {
        assert_eq!(self.status, StatusCode::OK, "{}", self.body);
        assert!(
            self.content_type.starts_with("application/json"),
            "content-type {:?}",
            self.content_type
        );
        assert_eq!(
            self.json(),
            json!({ "enabled": enabled, "running": running }),
            "{}",
            self.body
        );
    }

    /// The 200 list: a JSON array of rows.
    fn rows(&self) -> Vec<Value> {
        assert_eq!(self.status, StatusCode::OK, "{}", self.body);
        self.json()
            .as_array()
            .unwrap_or_else(|| panic!("not a JSON array: {}", self.body))
            .clone()
    }

    /// The list's row of `id`, case-insensitively. A preset the list does not
    /// show is a failure of these tests: the switch just moved it.
    fn row_of(&self, id: &str) -> Value {
        self.rows()
            .into_iter()
            .find(|r| r["id"].as_str().is_some_and(|r| r.eq_ignore_ascii_case(id)))
            .unwrap_or_else(|| panic!("no row for {id}: {}", self.body))
    }

    /// `GET /api/session` for a session that is still signed in: `{"user"}`.
    fn assert_signed_in_as(&self, user: &str) {
        assert_eq!(self.status, StatusCode::OK, "signed in: {}", self.body);
        assert_eq!(self.json(), json!({ "user": user }), "{}", self.body);
    }
}

/// Whether `text` holds a section named `id`, in any letter case: the rule
/// the list and the switch read the files by.
fn holds(text: &str, id: &str) -> bool {
    pai_core::ini::parse_sections(text)
        .iter()
        .any(|s| s.id.eq_ignore_ascii_case(id))
}

impl App {
    /// The manager's calls, in order.
    fn calls(&self) -> Vec<String> {
        self.log.lock().unwrap_or_else(|e| e.into_inner()).clone()
    }

    /// The `write_file_if` calls the manager was asked for, in order: name,
    /// contents, the text the compare and swap expected.
    fn writes(&self) -> Vec<(String, String, String)> {
        self.live
            .writes
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .clone()
    }

    /// The `write_file` calls the manager was asked for, in order: name,
    /// contents. Kept apart from the compare and swaps, because a test pins
    /// which channel the switch used for a file.
    fn plain_writes(&self) -> Vec<(String, String)> {
        self.live
            .plain_writes
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .clone()
    }

    /// No write of any kind was asked for.
    fn no_write_asked(&self) -> bool {
        self.writes().is_empty() && self.plain_writes().is_empty()
    }

    /// The text of one preset file, as the manager holds it now: `None` for a
    /// file that is not there.
    fn file_text(&self, name: &str) -> Option<String> {
        self.live
            .files
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .get(name)
            .cloned()
    }

    /// The instances the manager lists, in order.
    fn set_instances(&self, ids: &[&str]) {
        self.live.set_instances(ids);
    }

    /// The state of one instance.
    fn set_state(&self, id: &str, state: InstanceState) {
        self.live.set_state(id, state);
    }

    /// Answer every coming connect with `outcome`: no manager to reach.
    fn set_connect_outcome(&self, outcome: Outcome) {
        *self.live.connect_outcome.lock().unwrap() = Some(outcome);
    }

    /// Answer every coming read with `outcome`: the switch stopped before it
    /// reached a write.
    fn set_read_outcome(&self, outcome: Outcome) {
        *self.live.read_outcome.lock().unwrap() = Some(outcome);
    }

    /// Answer every coming write with `outcome`, in place of landing: the
    /// file stays as it was.
    fn set_write_outcome(&self, outcome: Outcome) {
        *self.live.write_outcome.lock().unwrap() = Some(outcome);
    }

    /// Answer the write at `position` (1-based, counting the plain ones) with
    /// `outcome` and let every other write land: how a test puts `Changed` on
    /// the SECOND write of a move.
    fn set_write_outcome_at(&self, position: usize, outcome: Outcome) {
        *self.live.write_outcome_at.lock().unwrap() = Some((position, outcome));
    }

    /// Answer every coming status read with `outcome`.
    fn set_status_outcome(&self, outcome: Outcome) {
        *self.live.status_outcome.lock().unwrap() = Some(outcome);
    }

    /// Refuse the write of `user`: the member of `pai` removed between the
    /// read and the write.
    fn set_write_refused(&self, user: &str) {
        self.live
            .write_refused
            .lock()
            .unwrap()
            .push(user.to_string());
    }

    /// Refuse the reads of `user`, leaving the writes alone: the read refused
    /// for a signed-in user, before the switch reaches a write.
    fn set_read_refused(&self, user: &str) {
        self.live
            .read_refused
            .lock()
            .unwrap()
            .push(user.to_string());
    }

    /// A read that takes `ms`.
    fn set_read_pause_ms(&self, ms: u64) {
        self.live.read_pause_ms.store(ms, Ordering::SeqCst);
    }

    /// A write that takes `ms`: the manager busy at the moment the switch
    /// arrives, with a write asked.
    fn set_write_pause_ms(&self, ms: u64) {
        self.live.write_pause_ms.store(ms, Ordering::SeqCst);
    }

    /// A status read that takes `ms`: the manager busy after the writes
    /// landed.
    fn set_status_pause_ms(&self, ms: u64) {
        self.live.status_pause_ms.store(ms, Ordering::SeqCst);
    }

    /// Stop the next read inside the manager call, until `release_read_gate`
    /// lets it go (see `gate`).
    fn arm_read_gate(&self) {
        self.live.read_gate_armed.store(true, Ordering::SeqCst);
    }
    fn release_read_gate(&self) {
        self.live.read_gate_released.store(true, Ordering::SeqCst);
    }
    fn reads_gated(&self) -> usize {
        self.live.reads_gated.load(Ordering::SeqCst)
    }
    fn reads_released(&self) -> usize {
        self.live.reads_released.load(Ordering::SeqCst)
    }

    /// Stop the next write inside the manager call, until
    /// `release_write_gate` lets it go.
    fn arm_write_gate(&self) {
        self.live.write_gate_armed.store(true, Ordering::SeqCst);
    }
    fn release_write_gate(&self) {
        self.live.write_gate_released.store(true, Ordering::SeqCst);
    }
    fn writes_gated(&self) -> usize {
        self.live.writes_gated.load(Ordering::SeqCst)
    }
    fn writes_released(&self) -> usize {
        self.live.writes_released.load(Ordering::SeqCst)
    }

    /// Stop the next connect inside the manager call, until
    /// `release_connect_gate` lets it go.
    fn arm_connect_gate(&self) {
        self.live.connect_gate_armed.store(true, Ordering::SeqCst);
    }
    fn release_connect_gate(&self) {
        self.live
            .connect_gate_released
            .store(true, Ordering::SeqCst);
    }
    fn connects_gated(&self) -> usize {
        self.live.connects_gated.load(Ordering::SeqCst)
    }
    fn connects_released(&self) -> usize {
        self.live.connects_released.load(Ordering::SeqCst)
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
        let mut builder = Request::builder().method(Method::GET).uri(uri);
        if let Some(c) = cookie {
            builder = builder.header(header::COOKIE, c);
        }
        self.send(builder.body(Body::empty()).expect("request"))
            .await
    }

    /// `PUT /api/presets/{id}/enabled` with the body the web sends.
    async fn switch(&self, id: &str, body: &Value, cookie: Option<&str>) -> Reply {
        self.switch_raw(id, &body.to_string(), cookie, &[]).await
    }

    /// The switch with the body spelled out (a body that is no JSON, an empty
    /// body) and extra headers (a request from another origin).
    async fn switch_raw(
        &self,
        id: &str,
        body: &str,
        cookie: Option<&str>,
        extra: &[(&str, &str)],
    ) -> Reply {
        let mut builder = Request::builder()
            .method(Method::PUT)
            .uri(format!("/api/presets/{id}/enabled"))
            .header(header::CONTENT_TYPE, "application/json");
        if let Some(c) = cookie {
            builder = builder.header(header::COOKIE, c);
        }
        for (name, value) in extra {
            builder = builder.header(*name, *value);
        }
        self.send(builder.body(Body::from(body.to_string())).expect("request"))
            .await
    }

    /// Signed in as `alice`, the switch of `id` to `enabled`.
    async fn switch_on(&self, id: &str, enabled: bool) -> Reply {
        let cookie = self.sign_in().await;
        self.switch(id, &json!({ "enabled": enabled }), Some(&cookie))
            .await
    }

    /// Signed in as `alice`, the switch of `id` with the body `body`.
    async fn switch_on_body(&self, id: &str, body: &Value) -> Reply {
        let cookie = self.sign_in().await;
        self.switch(id, body, Some(&cookie)).await
    }

    /// Signed in as `alice`, `GET /api/presets`.
    async fn list(&self) -> Reply {
        let cookie = self.sign_in().await;
        self.get("/api/presets", Some(&cookie)).await
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
}

// ---------------------------------------------------------------------------
// The contract's body, and who may ask
// ---------------------------------------------------------------------------

/// The route at work: `{"enabled": false}` on a preset that is on answers 200
/// `{"enabled": false, "running": false}`, and the preset is off in the files:
/// its bytes sit in `presets.ini.disabled`, gone from `presets.ini`.
#[tokio::test]
async fn switch_signed_in_moves_the_preset_off_and_answers_its_state() {
    let app = app_holding(FILE_ON, FILE_OFF);
    let rep = app.switch_on(QWEN, false).await;
    rep.assert_switched(false, false);
    assert_eq!(
        app.writes(),
        vec![
            (
                DISABLED.to_string(),
                file_off_with_qwen(),
                FILE_OFF.to_string()
            ),
            (
                PRESETS.to_string(),
                file_on_without_qwen(),
                FILE_ON.to_string()
            ),
        ],
        "each write expects the text the switch read"
    );
    assert!(
        app.plain_writes().is_empty(),
        "both files exist: the compare and swap is enough"
    );
}

/// The route answers the session first: the manager is not connected to, the
/// files are not read, nothing is written. A cookie naming no session is the
/// same answer.
#[tokio::test]
async fn switch_off_without_a_session_is_401_nothing_is_read_or_written() {
    let app = app_holding(FILE_ON, FILE_OFF);
    let _real = app.sign_in().await;
    for cookie in [
        None,
        // A cookie naming no session (made up, or one from before a restart).
        Some(&format!("pai_session={}", "ab".repeat(32))[..]),
    ] {
        let rep = app.switch(QWEN, &json!({ "enabled": false }), cookie).await;
        rep.assert_error(StatusCode::UNAUTHORIZED, "session-required");
    }
    assert_eq!(app.connects.load(Ordering::SeqCst), 0, "no connect");
    assert!(app.calls().is_empty(), "{:?}", app.calls());
    assert!(app.no_write_asked(), "nothing written");
}

/// The same-origin guard the other routes serve answers before the session and
/// before the router: a signed-in request carrying a foreign `Origin` is
/// refused, the manager is not asked for anything.
#[tokio::test]
async fn switch_with_a_foreign_origin_is_refused_before_the_manager_is_read() {
    let app = app_holding(FILE_ON, FILE_OFF);
    let cookie = app.sign_in().await;
    let rep = app
        .switch_raw(
            QWEN,
            &json!({ "enabled": false }).to_string(),
            Some(&cookie),
            &[("origin", "https://evil.example")],
        )
        .await;
    rep.assert_error(StatusCode::FORBIDDEN, "request-cross-origin");
    assert!(app.calls().is_empty(), "{:?}", app.calls());
    assert!(app.no_write_asked(), "nothing written");
}

/// A body that is not an object at all (a bare string) is the same malformed
/// body as one with a member of the wrong type: 400, the manager not asked.
#[tokio::test]
async fn switch_body_not_an_object_is_400() {
    let app = app_holding(FILE_ON, FILE_OFF);
    let rep = app.switch_on_body(QWEN, &json!("off")).await;
    rep.assert_error(StatusCode::BAD_REQUEST, "presets-malformed");
    assert!(app.calls().is_empty(), "{:?}", app.calls());
    assert!(app.no_write_asked(), "nothing written");
}

/// `{"enabled": 0}`: the member is there and is not a JSON boolean, so the
/// body is malformed. No file is read, nothing is written.
#[tokio::test]
async fn switch_body_enabled_a_number_is_400_nothing_is_read() {
    let app = app_holding(FILE_ON, FILE_OFF);
    let rep = app.switch_on_body(QWEN, &json!({ "enabled": 0 })).await;
    rep.assert_error(StatusCode::BAD_REQUEST, "presets-malformed");
    assert!(app.calls().is_empty(), "{:?}", app.calls());
    assert!(app.no_write_asked(), "nothing written");
}

/// `{}`: no body is no member, and nothing is asked of the manager.
#[tokio::test]
async fn switch_body_missing_enabled_is_400() {
    let app = app_holding(FILE_ON, FILE_OFF);
    let rep = app.switch_on_body(QWEN, &json!({})).await;
    rep.assert_error(StatusCode::BAD_REQUEST, "presets-malformed");
    assert!(app.calls().is_empty(), "{:?}", app.calls());
    assert!(app.no_write_asked(), "nothing written");
}

/// A body that is no JSON at all, and an empty one: the same 400. Every case
/// is built from the well-typed body with one thing broken, so the refusal
/// cannot come from a member the test left out.
#[tokio::test]
async fn switch_body_that_is_no_json_is_400() {
    let app = app_holding(FILE_ON, FILE_OFF);
    let cookie = app.sign_in().await;
    for body in ["{", "", "not json at all", "[\"enabled\"]"] {
        let rep = app.switch_raw(QWEN, body, Some(&cookie), &[]).await;
        rep.assert_error(StatusCode::BAD_REQUEST, "presets-malformed");
    }
    assert!(app.calls().is_empty(), "{:?}", app.calls());
    assert!(app.no_write_asked(), "nothing written");
}

/// The contract's answer for a name no preset has, in either file: 404
/// `no-preset-named` with the id as its only arg, and no write.
#[tokio::test]
async fn switch_unknown_name_is_404_no_file_is_written() {
    let app = app_holding(FILE_ON, FILE_OFF);
    let rep = app.switch_on("nope", false).await;
    rep.assert_error_with_args(StatusCode::NOT_FOUND, "no-preset-named", &["nope"]);
    assert_eq!(
        app.file_text(PRESETS).as_deref(),
        Some(FILE_ON),
        "the files are as they were"
    );
    assert_eq!(
        app.file_text(DISABLED).as_deref(),
        Some(FILE_OFF),
        "the files are as they were"
    );
    assert!(app.no_write_asked(), "nothing written");
}

/// A machine with no preset file at all answers the same 404: no id is named by
/// any switch, and no file is created.
#[tokio::test]
async fn switch_with_no_preset_file_is_404_and_creates_nothing() {
    let app = app_no_files();
    let rep = app.switch_on(QWEN, false).await;
    rep.assert_error_with_args(StatusCode::NOT_FOUND, "no-preset-named", &[QWEN]);
    assert_eq!(app.file_text(PRESETS), None, "no presets.ini was created");
    assert_eq!(
        app.file_text(DISABLED),
        None,
        "no disabled file was created"
    );
    assert!(app.no_write_asked(), "nothing written");
}

/// The id on the URL is matched the way the list's ids are: the section is
/// spelled `[Qwen]`, and `qwen` finds it and moves that very section.
#[tokio::test]
async fn switch_matches_the_name_ignoring_case() {
    let app = app_holding(FILE_ON, FILE_OFF);
    let rep = app.switch_on("qwen", false).await;
    rep.assert_switched(false, false);
    assert_eq!(
        app.writes(),
        vec![
            (
                DISABLED.to_string(),
                file_off_with_qwen(),
                FILE_OFF.to_string()
            ),
            (
                PRESETS.to_string(),
                file_on_without_qwen(),
                FILE_ON.to_string()
            ),
        ],
        "the section of [Qwen] moved, spelled as the file spells it"
    );
    assert!(
        !holds(&app.file_text(PRESETS).unwrap(), QWEN),
        "gone from presets.ini"
    );
    assert!(
        holds(&app.file_text(DISABLED).unwrap(), "QWEN"),
        "in the disabled file"
    );
}

/// Every signed-in administrator is served the switch, and the manager is asked
/// for them: `root` moves the preset as `root`.
#[tokio::test]
async fn switch_is_served_to_every_signed_in_administrator() {
    let app = app_holding(FILE_ON, FILE_OFF);
    let cookie = app.sign_in_as("root").await;
    let rep = app
        .switch(QWEN, &json!({ "enabled": false }), Some(&cookie))
        .await;
    rep.assert_switched(false, false);
    assert_eq!(
        principals_of(&app.principals),
        vec![Some("root".to_string())],
        "the connect named the session's user"
    );
}

// ---------------------------------------------------------------------------
// What is read, as whom, and in which order
// ---------------------------------------------------------------------------

/// The bff is not privileged: the connect names the signed-in user, so the
/// manager re-checks their membership of `pai`, and every call of the move is
/// made as them. The move asks for exactly seven calls: both files read, the
/// enabled one first, the two writes (the destination first), then
/// `presets.ini` read back for the `enabled` of the answer, then the instances
/// for `running`.
#[tokio::test]
async fn switch_asks_exactly_seven_calls_as_the_signed_in_user() {
    let app = app_holding(FILE_ON, FILE_OFF);
    let rep = app.switch_on(QWEN, false).await;
    rep.assert_switched(false, false);
    assert_eq!(
        principals_of(&app.principals),
        vec![Some("alice".to_string())],
        "one connect, for the session's user"
    );
    assert_eq!(
        app.calls(),
        vec![
            "read presets.ini",
            "read presets.ini.disabled",
            "write-if alice presets.ini.disabled",
            "write-if alice presets.ini",
            "read presets.ini",
            "list",
            "status default",
        ],
        "no call beyond the move, its writes as the signed-in user, the enabled \
         file read back after them, then the state"
    );
}

/// A name in the request never becomes the principal: a query parameter and a
/// header naming another user are ignored, the session's user is asked for.
#[tokio::test]
async fn switch_a_name_in_the_request_is_never_the_principal() {
    let app = app_holding(FILE_ON, FILE_OFF);
    let cookie = app.sign_in_as("alice").await;
    let rep = app
        .switch_raw(
            QWEN,
            &json!({ "enabled": false }).to_string(),
            Some(&cookie),
            &[("x-user", "root"), ("x-principal", "root")],
        )
        .await;
    rep.assert_switched(false, false);
    assert_eq!(
        principals_of(&app.principals),
        vec![Some("alice".to_string())],
        "the session's user, never the request's"
    );
}

/// Off: the destination first. The preset has to appear in
/// `presets.ini.disabled` before it disappears from `presets.ini`, so a process
/// that stops in the middle leaves it in both files, never in none.
#[tokio::test]
async fn switch_off_writes_the_disabled_file_first_then_presets_ini() {
    let app = app_holding(FILE_ON, FILE_OFF);
    let rep = app.switch_on(QWEN, false).await;
    rep.assert_switched(false, false);
    let names: Vec<String> = app
        .writes()
        .iter()
        .map(|(name, _, _)| name.clone())
        .collect();
    assert_eq!(
        names,
        vec![DISABLED.to_string(), PRESETS.to_string()],
        "{:?}",
        names
    );
    assert!(
        app.plain_writes().is_empty(),
        "both files exist: no unguarded write"
    );
}

/// The text that lands in `presets.ini.disabled` is the current disabled text
/// plus the section as `presets.ini` holds it: every key, the comments, the
/// blank line. Nothing is rewritten, nothing is dropped.
#[tokio::test]
async fn switch_off_moves_the_section_bytes_to_the_disabled_file() {
    let app = app_holding(FILE_ON, FILE_OFF);
    let rep = app.switch_on(QWEN, false).await;
    rep.assert_switched(false, false);
    assert_eq!(
        app.file_text(DISABLED).as_deref(),
        Some(&file_off_with_qwen()[..])
    );
    assert_eq!(
        app.file_text(PRESETS).as_deref(),
        Some(&file_on_without_qwen()[..])
    );
    assert_eq!(
        section_of(&app.file_text(DISABLED).unwrap(), QWEN),
        section_of(FILE_ON, QWEN),
        "the section that lands must be the bytes presets.ini held"
    );
}

/// On: the same rule with the files exchanged, `presets.ini` is the destination
/// and is written first, and the section that lands is the bytes the disabled
/// file held.
#[tokio::test]
async fn switch_on_writes_presets_ini_first_then_the_disabled_file() {
    let app = app_holding(FILE_ON, FILE_OFF);
    let rep = app.switch_on(MISTRAL, true).await;
    rep.assert_switched(true, false);
    assert_eq!(
        app.writes(),
        vec![
            (
                PRESETS.to_string(),
                file_on_with_mistral(),
                FILE_ON.to_string()
            ),
            (
                DISABLED.to_string(),
                file_off_without_mistral(),
                FILE_OFF.to_string()
            ),
        ],
        "the destination first, each write expecting the text the switch read"
    );
    assert_eq!(
        section_of(&app.file_text(PRESETS).unwrap(), MISTRAL),
        section_of(FILE_OFF, MISTRAL),
        "the section that lands is the bytes the disabled file held"
    );
}

/// No `presets.ini.disabled` at all (a machine where nothing was ever switched
/// off): the switch creates it, with the configurator's header, through the
/// plain write, because the compare and swap refuses a missing file. The preset
/// follows the header, and `presets.ini` is then written as the source.
#[tokio::test]
async fn switch_creates_the_disabled_file_with_the_header_then_the_section() {
    let app = app_holding_no_disabled(FILE_ON);
    let rep = app.switch_on(QWEN, false).await;
    rep.assert_switched(false, false);
    assert_eq!(
        app.plain_writes(),
        vec![(DISABLED.to_string(), created_with_qwen())],
        "a file that is not there is created with the plain write, carrying the header"
    );
    assert_eq!(
        app.writes()
            .iter()
            .map(|(name, ..)| name.clone())
            .collect::<Vec<String>>(),
        vec![PRESETS.to_string()],
        "the compare and swap is asked only for the file that exists"
    );
    let text = app
        .file_text(DISABLED)
        .expect("the disabled file was created");
    assert!(
        text.starts_with(DISABLED_HEADER),
        "the created file starts with the header: {text}"
    );
    assert_eq!(section_of(&text, QWEN), section_of(FILE_ON, QWEN));
    assert_eq!(
        app.file_text(PRESETS).as_deref(),
        Some(&file_on_without_qwen()[..])
    );
}

/// The switch does not report what it computed alone: after the last write it
/// asks the manager for the file again, so the `enabled` it answers is the text
/// that is on the machine.
#[tokio::test]
async fn switch_a_moved_file_is_read_again_after_the_last_write() {
    let app = app_holding(FILE_ON, FILE_OFF);
    let rep = app.switch_on(QWEN, false).await;
    rep.assert_switched(false, false);
    let calls = app.calls();
    let last_write = calls
        .iter()
        .rposition(|c| c.starts_with("write"))
        .expect("the switch writes");
    let reads_after = calls[last_write + 1..]
        .iter()
        .filter(|c| c.starts_with("read"))
        .count();
    assert!(
        reads_after >= 1,
        "the file the switch moved is read back after the write: {calls:?}"
    );
}

/// The web UI shows the switch's state and the list's side by side: the two must
/// agree. The list, a separate request, reads what the switch wrote.
#[tokio::test]
async fn switch_then_list_agree_on_which_presets_are_on() {
    let app = app_holding(FILE_ON, FILE_OFF);
    let rep = app.switch_on(QWEN, false).await;
    rep.assert_switched(false, false);
    let list = app.list().await;
    assert_eq!(list.status, StatusCode::OK, "{}", list.body);
    assert_eq!(
        list.row_of(QWEN)["enabled"],
        rep.json()["enabled"],
        "the list agrees with the switch: {}",
        list.body
    );
    assert_eq!(list.row_of(ALPHA)["enabled"], json!(true), "{}", list.body);
    assert_eq!(
        list.row_of(MISTRAL)["enabled"],
        json!(false),
        "{}",
        list.body
    );
}

/// The state a half-finished move leaves: the section sits in `presets.ini` and
/// in `presets.ini.disabled`, so it reads enabled. Asking for on is already the
/// state, so nothing moves; asking for off takes it out of `presets.ini` and
/// leaves the disabled file's copy alone, and the preset ends up off.
#[tokio::test]
async fn switch_mends_a_preset_the_files_hold_in_both_places() {
    let app = app_holding(FILE_ON, &file_off_with_qwen());
    let rep = app.switch_on(QWEN, true).await;
    rep.assert_switched(true, false);
    assert!(
        app.no_write_asked(),
        "already where it is asked: {:?}",
        app.writes()
    );

    let app = app_holding(FILE_ON, &file_off_with_qwen());
    let rep = app.switch_on(QWEN, false).await;
    rep.assert_switched(false, false);
    assert_eq!(
        app.file_text(DISABLED).as_deref(),
        Some(&file_off_with_qwen()[..])
    );
    assert_eq!(
        app.file_text(PRESETS).as_deref(),
        Some(&file_on_without_qwen()[..])
    );
    assert!(
        !holds(&app.file_text(PRESETS).unwrap(), QWEN),
        "the mend leaves the section in the disabled file only"
    );
}

/// A method the contract does not list gets the router's 405, and asks the
/// manager nothing. The switch is the only method of that path.
#[tokio::test]
async fn switch_is_the_only_method_the_switch_path_serves() {
    for method in [Method::GET, Method::POST, Method::DELETE] {
        let app = app_holding(FILE_ON, FILE_OFF);
        let cookie = app.sign_in().await;
        let mut builder = Request::builder()
            .method(method.clone())
            .uri(format!("/api/presets/{QWEN}/enabled"));
        if method == Method::POST {
            builder = builder.header(header::CONTENT_TYPE, "application/json");
        }
        let request = builder
            .header(header::COOKIE, &cookie)
            .body(Body::empty())
            .expect("request");
        let rep = app.send(request).await;
        assert_eq!(
            rep.status,
            StatusCode::METHOD_NOT_ALLOWED,
            "{}: {}",
            method,
            rep.body
        );
        assert!(app.calls().is_empty(), "{method}: {:?}", app.calls());
    }
}

/// The header the switch creates a missing `presets.ini.disabled` with is the
/// configurator's `DISABLED_FILE_HEADER`, not a text the bff invented: the bff
/// links no `pai-app`, so the text is copied here and this pins the copy.
#[test]
fn the_disabled_header_is_the_configurators_one() {
    let source = std::fs::read_to_string(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../app/src/presets.rs"
    ))
    .expect("the configurator's presets.rs: it owns the disabled file's header");
    const DECL: &str = "const DISABLED_FILE_HEADER: &str = \"";
    let start = source
        .find(DECL)
        .expect("the configurator's DISABLED_FILE_HEADER")
        + DECL.len();
    let body = &source[start..start + source[start..].find("\";").expect("the header's end")];
    for line in DISABLED_HEADER.lines() {
        let line = line.trim_end_matches('\r');
        assert!(
            body.contains(line),
            "the configurator's header has no line {line:?}: {body}"
        );
    }
    assert_eq!(
        unescape(body),
        DISABLED_HEADER,
        "the header the switch creates a file with is the configurator's, byte for byte"
    );
}

/// The text of a Rust string literal, as Rust reads it: a `\` at the end of a
/// line joins the lines (and cuts the indent of the next one), `\r`, `\n`,
/// `\t`, `\"` and `\\` are the byte they name.
fn unescape(raw: &str) -> String {
    let chars: Vec<char> = raw.chars().collect();
    let mut out = String::new();
    let mut i = 0;
    while i < chars.len() {
        if chars[i] != '\\' {
            out.push(chars[i]);
            i += 1;
            continue;
        }
        match chars.get(i + 1) {
            Some('n') => out.push('\n'),
            Some('r') => out.push('\r'),
            Some('t') => out.push('\t'),
            Some('"') => out.push('"'),
            Some('\\') => out.push('\\'),
            // A line continuation: the newline and the next line's indent go.
            Some('\n') => {
                i += 2;
                while matches!(chars.get(i), Some(' ') | Some('\t')) {
                    i += 1;
                }
                continue;
            }
            _ => out.push('\\'),
        }
        i += 2;
    }
    out
}

// ---------------------------------------------------------------------------
// The id is a section name, never a path
// ---------------------------------------------------------------------------

/// The id is matched against section headers, and the two file names are the
/// fixed ones, so a URL carrying a path, a newline or a bracket is a name no
/// preset has: the same 404, no write of any kind, and no file created.
#[tokio::test]
async fn switch_a_name_that_cannot_be_a_section_is_404_and_creates_no_file() {
    for id in [
        "two%0Alines",
        "two%0Dlines",
        "with%5Bbracket%5D",
        "with%3Dequals",
        "..%2F..%2F..%2Fetc%2Fpasswd",
        "%2E%2E",
    ] {
        let app = app_no_files();
        let cookie = app.sign_in().await;
        let rep = app
            .switch_raw(
                id,
                &json!({ "enabled": false }).to_string(),
                Some(&cookie),
                &[],
            )
            .await;
        rep.assert_error(StatusCode::NOT_FOUND, "no-preset-named");
        assert!(app.no_write_asked(), "{id}: {:?}", app.calls());
        assert_eq!(
            app.file_text(PRESETS),
            None,
            "{id}: no presets.ini was created"
        );
        assert_eq!(
            app.file_text(DISABLED),
            None,
            "{id}: no disabled file was created"
        );
    }
}

// ---------------------------------------------------------------------------
// The manager's answers
// ---------------------------------------------------------------------------

/// The compare and swap of the destination found its file changed after the
/// read: 409 `preset-changed-elsewhere`, nothing moved, the session kept. The
/// web UI reloads the list.
#[tokio::test]
async fn switch_changed_on_the_first_write_is_409_and_moves_nothing() {
    let app = app_holding(FILE_ON, FILE_OFF);
    app.set_write_outcome(Outcome::Changed);
    let cookie = app.sign_in().await;

    let rep = app
        .switch(QWEN, &json!({ "enabled": false }), Some(&cookie))
        .await;

    rep.assert_error(StatusCode::CONFLICT, "preset-changed-elsewhere");
    assert_eq!(
        app.writes().len(),
        1,
        "the destination write, refused: {:?}",
        app.writes()
    );
    assert_eq!(
        app.file_text(PRESETS).as_deref(),
        Some(FILE_ON),
        "the files are as they were"
    );
    assert_eq!(
        app.file_text(DISABLED).as_deref(),
        Some(FILE_OFF),
        "the files are as they were"
    );
    app.get("/api/session", Some(&cookie))
        .await
        .assert_signed_in_as("alice");
}

/// The compare and swap of the SOURCE found its file changed, after the
/// destination write landed: 409 with the preset in BOTH files, which the list
/// reads as enabled. The next switch of that preset mends it.
#[tokio::test]
async fn switch_changed_on_the_second_write_leaves_the_preset_in_both_files_and_the_next_switch_mends_it(
) {
    let app = app_holding(FILE_ON, FILE_OFF);
    app.set_write_outcome_at(2, Outcome::Changed);

    app.switch_on(QWEN, false)
        .await
        .assert_error(StatusCode::CONFLICT, "preset-changed-elsewhere");
    assert_eq!(
        app.writes().len(),
        2,
        "the destination landed, the source was refused: {:?}",
        app.writes()
    );
    assert!(
        holds(&app.file_text(PRESETS).unwrap(), QWEN),
        "the source write was the refused one, so presets.ini still holds it"
    );
    assert!(
        holds(&app.file_text(DISABLED).unwrap(), QWEN),
        "the destination write landed: the preset is in both files"
    );

    // Asking for off from the both-files state is the repair: the copy in
    // `presets.ini` goes, the one in the disabled file stays.
    app.switch_on(QWEN, false)
        .await
        .assert_switched(false, false);
    assert!(
        !holds(&app.file_text(PRESETS).unwrap(), QWEN),
        "the next switch took the section out of presets.ini"
    );
    assert!(
        holds(&app.file_text(DISABLED).unwrap(), QWEN),
        "and left the disabled copy"
    );
}

/// Switching on a preset that carries `load-on-startup` is the manager's count
/// of those presets against the lowest `ModelsMax`, a check the bff cannot make
/// itself. Its key, its values `[count, max]` and the field the form marks pass
/// through, and the refused file stays as it was.
#[tokio::test]
async fn switch_refused_for_the_load_on_startup_count_is_422_with_the_field_and_moves_nothing() {
    let app = app_holding(&file_on_without_qwen(), &file_off_with_qwen());
    app.set_write_outcome(Outcome::Invalid {
        key: "load-on-startup-over-max",
        values: vec!["3".into(), "2".into()],
    });
    let cookie = app.sign_in().await;

    let rep = app
        .switch(QWEN, &json!({ "enabled": true }), Some(&cookie))
        .await;

    rep.assert_refused("load-on-startup-over-max", &["3", "2"], "load-on-startup");
    assert_eq!(
        app.writes().len(),
        1,
        "the check runs at the write: {:?}",
        app.writes()
    );
    assert_eq!(
        app.file_text(PRESETS).as_deref(),
        Some(&file_on_without_qwen()[..]),
        "nothing moved"
    );
    assert_eq!(
        app.file_text(DISABLED).as_deref(),
        Some(&file_off_with_qwen()[..]),
        "nothing moved"
    );
    app.get("/api/session", Some(&cookie))
        .await
        .assert_signed_in_as("alice");
}

/// A check of the moved section names the key it is about, as its first value:
/// the answer carries the check's key and values, and `field` is that key.
#[tokio::test]
async fn switch_refused_for_a_key_of_the_moved_section_is_422_on_that_field() {
    let app = app_holding(FILE_ON, FILE_OFF);
    app.set_write_outcome(Outcome::Invalid {
        key: "not-a-number",
        values: vec!["n-gpu-layers".into(), "off".into()],
    });

    app.switch_on(QWEN, false).await.assert_refused(
        "not-a-number",
        &["n-gpu-layers", "off"],
        "n-gpu-layers",
    );
    assert_eq!(
        app.file_text(PRESETS).as_deref(),
        Some(FILE_ON),
        "nothing moved"
    );
}

/// A refusal that names no key of the section (the id itself) is answered with
/// its key and values and no `field` at all: the form has nothing to mark.
#[tokio::test]
async fn switch_refused_without_a_key_to_mark_is_422_with_no_field() {
    let app = app_holding(FILE_ON, FILE_OFF);
    app.set_write_outcome(Outcome::Invalid {
        key: "invalid-preset-id",
        values: vec![QWEN.into()],
    });

    app.switch_on(QWEN, false)
        .await
        .assert_refused_without_field("invalid-preset-id", &[QWEN]);
    assert_eq!(
        app.file_text(PRESETS).as_deref(),
        Some(FILE_ON),
        "nothing moved"
    );
}

/// The signed-in user is no longer a member of `pai`, so the manager refuses the
/// read: the served sibling's 403 `session-not-administrator` (the sign-in's key
/// for the same fact) and that session ends. The switch stops at the refusal,
/// before any write.
#[tokio::test]
async fn switch_refused_at_the_read_is_403_and_ends_the_session() {
    let app = app_refusing(&["alice"], FILE_ON, FILE_OFF);
    let cookie = app.sign_in().await;

    app.switch(QWEN, &json!({ "enabled": false }), Some(&cookie))
        .await
        .assert_error(StatusCode::FORBIDDEN, "session-not-administrator");

    app.get("/api/session", Some(&cookie))
        .await
        .assert_error(StatusCode::UNAUTHORIZED, "session-required");
    assert_eq!(
        app.calls().len(),
        1,
        "the refusal ends the switch: {:?}",
        app.calls()
    );
    assert!(app.no_write_asked(), "nothing written");
    assert_eq!(
        app.file_text(PRESETS).as_deref(),
        Some(FILE_ON),
        "the files are as they were"
    );
}

/// The refusal is at the read, and it stops the switch there: the manager
/// answers the preset files with a refusal and would take the write. The write
/// is never asked, so the switch cannot report a state it did not read.
#[tokio::test]
async fn switch_refused_at_the_read_never_asks_for_a_write() {
    let app = app_holding(FILE_ON, FILE_OFF);
    app.set_read_refused("alice");
    let cookie = app.sign_in().await;

    app.switch(QWEN, &json!({ "enabled": false }), Some(&cookie))
        .await
        .assert_error(StatusCode::FORBIDDEN, "session-not-administrator");

    assert!(
        app.no_write_asked(),
        "the refusal stops the switch: {:?}",
        app.calls()
    );
    assert_eq!(app.calls().len(), 1, "one read, refused: {:?}", app.calls());
    app.get("/api/session", Some(&cookie))
        .await
        .assert_error(StatusCode::UNAUTHORIZED, "session-required");
}

/// The member of `pai` removed between the read and the write: the read is
/// answered, the write refused, so nothing moves. The same 403, and the session
/// ends: the manager refused this user's `configure`.
#[tokio::test]
async fn switch_refused_at_the_write_is_403_and_ends_the_session() {
    let app = app_holding(FILE_ON, FILE_OFF);
    app.set_write_refused("alice");
    let cookie = app.sign_in().await;

    app.switch(QWEN, &json!({ "enabled": false }), Some(&cookie))
        .await
        .assert_error(StatusCode::FORBIDDEN, "session-not-administrator");

    app.get("/api/session", Some(&cookie))
        .await
        .assert_error(StatusCode::UNAUTHORIZED, "session-required");
    assert_eq!(
        app.writes().len(),
        1,
        "the write was asked, and refused: {:?}",
        app.writes()
    );
    assert_eq!(
        app.file_text(PRESETS).as_deref(),
        Some(FILE_ON),
        "a refused write moves nothing"
    );
    assert_eq!(
        app.file_text(DISABLED).as_deref(),
        Some(FILE_OFF),
        "a refused write moves nothing"
    );
}

/// The session the bff holds is no longer accepted by the manager: the switch is
/// not made, the session ends, the files are not even read.
#[tokio::test]
async fn switch_refused_at_the_connect_is_403_and_ends_the_session() {
    let app = app_holding(FILE_ON, FILE_OFF);
    app.set_connect_outcome(Outcome::NotAuthorized);
    let cookie = app.sign_in().await;

    app.switch(QWEN, &json!({ "enabled": false }), Some(&cookie))
        .await
        .assert_error(StatusCode::FORBIDDEN, "session-not-administrator");

    app.get("/api/session", Some(&cookie))
        .await
        .assert_error(StatusCode::UNAUTHORIZED, "session-required");
    assert!(
        app.calls().is_empty(),
        "the files were not read: {:?}",
        app.calls()
    );
    assert!(app.no_write_asked(), "nothing written");
}

/// No manager to reach, and a read that answers it is not there: 503
/// `client-unavailable`, both, because no write was asked. The session is kept:
/// the manager's absence is nobody's fault and the user is still an
/// administrator.
#[tokio::test]
async fn switch_with_no_manager_or_an_unavailable_read_is_503_and_keeps_the_session() {
    for app in [app_no_manager(), {
        let app = app_holding(FILE_ON, FILE_OFF);
        app.set_read_outcome(Outcome::Unavailable);
        app
    }] {
        let cookie = app.sign_in().await;
        app.switch(QWEN, &json!({ "enabled": false }), Some(&cookie))
            .await
            .assert_error(StatusCode::SERVICE_UNAVAILABLE, "client-unavailable");
        assert!(app.no_write_asked(), "nothing written: {:?}", app.calls());
        assert_eq!(
            app.file_text(PRESETS).as_deref(),
            Some(FILE_ON),
            "the files are as they were"
        );
        app.get("/api/session", Some(&cookie))
            .await
            .assert_signed_in_as("alice");
    }
}

/// The manager is reached for the write and the stream dies there: the write may
/// have landed, so the answer is 504 `client-timeout`, never 503. That is the
/// server file's rule, carried over to the two preset files.
#[tokio::test]
async fn switch_when_the_write_answers_unavailable_is_504() {
    let app = app_holding(FILE_ON, FILE_OFF);
    app.set_write_outcome(Outcome::Unavailable);

    app.switch_on(QWEN, false)
        .await
        .assert_error(StatusCode::GATEWAY_TIMEOUT, "client-timeout");
    assert_eq!(
        app.writes().len(),
        1,
        "a write was asked: {:?}",
        app.writes()
    );
}

/// The manager answers an error that is none of the contract's: 502
/// `client-other`, at the read and at the write, with the manager's own words
/// kept out of the answer and nothing moved.
#[tokio::test]
async fn switch_when_the_manager_answers_another_error_is_502() {
    for stop in ["read", "write"] {
        let app = app_holding(FILE_ON, FILE_OFF);
        if stop == "read" {
            app.set_read_outcome(Outcome::Other);
        } else {
            app.set_write_outcome(Outcome::Other);
        }
        let cookie = app.sign_in().await;

        let rep = app
            .switch(QWEN, &json!({ "enabled": false }), Some(&cookie))
            .await;

        rep.assert_error(StatusCode::BAD_GATEWAY, "client-other");
        assert!(
            !rep.body.contains("the manager failed"),
            "{stop}: the manager's detail is not shown: {}",
            rep.body
        );
        assert_eq!(
            app.file_text(PRESETS).as_deref(),
            Some(FILE_ON),
            "{stop}: nothing moved"
        );
        assert_eq!(
            app.file_text(DISABLED).as_deref(),
            Some(FILE_OFF),
            "{stop}: nothing moved"
        );
        app.get("/api/session", Some(&cookie))
            .await
            .assert_signed_in_as("alice");
    }
}

// ---------------------------------------------------------------------------
// The time limits
// ---------------------------------------------------------------------------

/// The manager is reached and does not answer the read in time: the bff stops
/// waiting, and with no write asked the answer is 503 `client-unavailable`.
#[tokio::test]
async fn switch_with_a_read_that_is_too_slow_is_503_and_writes_nothing() {
    let app = app_holding(FILE_ON, FILE_OFF);
    app.set_read_pause_ms(TIMEOUT.as_millis() as u64 + 200);

    app.switch_on(QWEN, false)
        .await
        .assert_error(StatusCode::SERVICE_UNAVAILABLE, "client-unavailable");
    assert!(
        app.no_write_asked(),
        "the exchange stops at the read: {:?}",
        app.calls()
    );
    assert_eq!(
        app.file_text(PRESETS).as_deref(),
        Some(FILE_ON),
        "the files are as they were"
    );
}

/// A write that is still in flight when the time limit runs out: a write was
/// asked and its outcome is unknown, so 504 `client-timeout`. The bff rolls
/// nothing back.
#[tokio::test]
async fn switch_with_a_write_that_is_too_slow_is_504() {
    let app = app_holding(FILE_ON, FILE_OFF);
    app.set_write_pause_ms(TIMEOUT.as_millis() as u64 + 200);

    app.switch_on(QWEN, false)
        .await
        .assert_error(StatusCode::GATEWAY_TIMEOUT, "client-timeout");
    assert_eq!(
        app.writes().len(),
        1,
        "one write was sent: {:?}",
        app.writes()
    );
}

/// The two writes landed and the instance's state does not come back in time:
/// the move is done, and the answer is the 504 of a call that cannot be seen
/// through, with the files carrying the move.
#[tokio::test]
async fn switch_with_a_status_read_that_is_too_slow_is_504_after_the_move_landed() {
    let app = app_holding(FILE_ON, FILE_OFF);
    app.set_status_pause_ms(TIMEOUT.as_millis() as u64 + 200);

    app.switch_on(QWEN, false)
        .await
        .assert_error(StatusCode::GATEWAY_TIMEOUT, "client-timeout");
    assert_eq!(
        app.writes().len(),
        2,
        "both writes were asked: {:?}",
        app.writes()
    );
    assert_eq!(
        app.file_text(DISABLED).as_deref(),
        Some(&file_off_with_qwen()[..]),
        "the move landed"
    );
    assert_eq!(
        app.file_text(PRESETS).as_deref(),
        Some(&file_on_without_qwen()[..]),
        "the move landed"
    );
}

/// The time limit runs out while the switch is parked inside its read, and the
/// test then lets that read return. The route has answered 503, saying the
/// manager was never asked to write, so the switch is over: the task it left
/// behind must not write. A route that only stops waiting for its blocking task
/// answers the 503 and lets that task run on to the two writes.
#[tokio::test]
async fn switch_time_limit_at_the_read_writes_nothing_after_the_503() {
    let app = app_holding(FILE_ON, FILE_OFF);
    app.arm_read_gate();

    app.switch_on(QWEN, false)
        .await
        .assert_error(StatusCode::SERVICE_UNAVAILABLE, "client-unavailable");
    assert_eq!(app.reads_gated(), 1, "the switch stopped inside its read");
    assert!(app.no_write_asked(), "nothing while the read is parked");

    // Let the parked read return: the task is free to go on.
    app.release_read_gate();
    let waited = Instant::now();
    while app.reads_released() < 1 && waited.elapsed() < DEADLINE {
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
    assert_eq!(app.reads_released(), 1, "the parked read came back");

    // The read is back on the file the switch read, so a task that was not cut
    // off goes straight to its write: GRACE is the time it needs to get there.
    let grace = Instant::now();
    while app.no_write_asked() && grace.elapsed() < GRACE {
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
    assert!(
        app.no_write_asked(),
        "a switch answered 503 asked to write {} times: {:?}",
        app.writes().len() + app.plain_writes().len(),
        app.calls()
    );
    assert_eq!(
        app.file_text(PRESETS).as_deref(),
        Some(FILE_ON),
        "the files are as they were"
    );
    assert_eq!(
        app.file_text(DISABLED).as_deref(),
        Some(FILE_OFF),
        "the files are as they were"
    );
}

/// The same for a switch parked while the connection is being opened: the route
/// answers 503 before the manager is reached, and a task that is not cut off
/// opens the connection, reads both files and writes them.
#[tokio::test]
async fn switch_time_limit_at_the_connect_writes_nothing_after_the_503() {
    let app = app_holding(FILE_ON, FILE_OFF);
    app.arm_connect_gate();

    app.switch_on(QWEN, false)
        .await
        .assert_error(StatusCode::SERVICE_UNAVAILABLE, "client-unavailable");
    assert_eq!(
        app.connects_gated(),
        1,
        "the switch stopped opening its connection"
    );
    assert_eq!(
        app.reads_gated(),
        0,
        "the switch never reached a read: {:?}",
        app.calls()
    );
    assert!(app.no_write_asked(), "nothing while the connect is parked");

    app.release_connect_gate();
    let waited = Instant::now();
    while app.connects_released() < 1 && waited.elapsed() < DEADLINE {
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
    assert_eq!(app.connects_released(), 1, "the parked connect came back");

    let grace = Instant::now();
    while app.no_write_asked() && grace.elapsed() < GRACE {
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
    assert!(
        app.no_write_asked(),
        "a switch answered 503 asked to write {} times: {:?}",
        app.writes().len() + app.plain_writes().len(),
        app.calls()
    );
    assert_eq!(
        app.file_text(PRESETS).as_deref(),
        Some(FILE_ON),
        "the files are as they were"
    );
    assert_eq!(
        app.file_text(DISABLED).as_deref(),
        Some(FILE_OFF),
        "the files are as they were"
    );
}

/// The write is parked and the time limit runs out: a write was asked, so the
/// answer is 504 `client-timeout`, and it says so because the write's outcome is
/// genuinely unknown. Let the parked write go: it lands, and the file carries
/// it. The 504 stands, the bff does not undo it.
#[tokio::test]
async fn switch_time_limit_at_the_write_is_504_and_the_released_write_lands() {
    let app = app_holding(FILE_ON, FILE_OFF);
    app.arm_write_gate();

    app.switch_on(QWEN, false)
        .await
        .assert_error(StatusCode::GATEWAY_TIMEOUT, "client-timeout");
    assert_eq!(app.writes_gated(), 1, "the switch stopped inside its write");
    assert_eq!(
        app.writes().len(),
        1,
        "the write was asked, and is in flight"
    );
    assert_eq!(
        app.file_text(DISABLED).as_deref(),
        Some(FILE_OFF),
        "not landed yet"
    );

    app.release_write_gate();
    let waited = Instant::now();
    while app.writes_released() < 1 && waited.elapsed() < DEADLINE {
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
    assert_eq!(app.writes_released(), 1, "the parked write came back");

    let grace = Instant::now();
    while app.file_text(DISABLED).as_deref() != Some(&file_off_with_qwen()[..])
        && grace.elapsed() < GRACE
    {
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
    assert_eq!(
        app.file_text(DISABLED).as_deref(),
        Some(&file_off_with_qwen()[..]),
        "the write the 504 could not see through landed"
    );
}

// ---------------------------------------------------------------------------
// `running`
// ---------------------------------------------------------------------------

/// The web UI says "restart llama-server to apply" when the engine is up, so the
/// switch answers `running` true for an instance that is active and for one that
/// is starting.
#[tokio::test]
async fn switch_answers_running_true_for_an_instance_that_is_up() {
    for state in [InstanceState::Active, InstanceState::Activating] {
        let app = app_holding(FILE_ON, FILE_OFF);
        app.set_state("default", state.clone());
        app.switch_on(QWEN, false)
            .await
            .assert_switched(false, true);
    }
}

/// An instance that is not serving is not running: `inactive`, `deactivating`,
/// `failed`.
#[tokio::test]
async fn switch_answers_running_false_for_an_instance_that_is_not_up() {
    for state in [
        InstanceState::Inactive,
        InstanceState::Deactivating,
        InstanceState::Failed,
    ] {
        let app = app_holding(FILE_ON, FILE_OFF);
        app.set_state("default", state.clone());
        app.switch_on(QWEN, false)
            .await
            .assert_switched(false, false);
    }
}

/// Every instance serves the one preset file, so any instance that is up makes
/// the switch's answer `running: true`: the states are asked in the list's
/// order, and the asking stops at the first instance that is up.
#[tokio::test]
async fn switch_answers_running_true_when_any_instance_is_up() {
    let app = app_holding(FILE_ON, FILE_OFF);
    app.set_instances(&["a", "b", "c"]);
    app.set_state("a", InstanceState::Inactive);
    app.set_state("b", InstanceState::Active);
    app.set_state("c", InstanceState::Active);

    app.switch_on(QWEN, false)
        .await
        .assert_switched(false, true);

    let calls = app.calls();
    let list = calls
        .iter()
        .position(|c| c == "list")
        .expect("the instances are listed");
    assert_eq!(
        calls[list..],
        ["list", "status a", "status b"],
        "the states in the list's order, stopping at the instance that is up: {calls:?}"
    );
    assert!(
        !calls
            .iter()
            .any(|c| c.starts_with("start") || c.starts_with("stop") || c.starts_with("restart")),
        "the switch restarts nothing: {calls:?}"
    );
}

/// The move is done and the state cannot be read: `running` is true, the safe
/// answer, so the page says to restart. The switch is not undone by it.
#[tokio::test]
async fn switch_answers_running_true_when_the_state_cannot_be_read() {
    for outcome in [Outcome::Unavailable, Outcome::Other, Outcome::NotAuthorized] {
        let app = app_holding(FILE_ON, FILE_OFF);
        app.set_status_outcome(outcome);
        app.switch_on(QWEN, false)
            .await
            .assert_switched(false, true);
        assert_eq!(
            app.file_text(DISABLED).as_deref(),
            Some(&file_off_with_qwen()[..]),
            "the move stands: {:?}",
            app.calls()
        );
    }
}
