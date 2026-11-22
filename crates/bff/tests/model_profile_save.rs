//! `PUT /api/profiles/model/{id}`: the save of one model profile's page (ARCHITECTURE.md
//! "Saving a model profile from the web", slice 4; `fe/briefs/api-contract.md`),
//! through `pai_bff::router_with_session_and_write_timeout` with a fake manager
//! client and a fake authenticator (never the real bus, pipe, PAM or LogonUser).
//! Every request runs under a deadline, so a bff that waits on the manager
//! fails instead of hanging.
//!
//! The file is a file of its own beside `tests/model_profiles_switch.rs` (the switch)
//! and `tests/model_profile_read.rs` (the read), because the save asks the manager for
//! a compare-and-swap write of the file that HOLDS the section, and answers the
//! revision of the section as written. The fake here keeps the two model profile files
//! live: a compare and swap lands only when `expected` is the text the fake
//! answered for that file in this very test, and every write is recorded as
//! `(name, contents, expected)` so a test pins the whole text the save computed,
//! the text it expected, and the file it chose.
//!
//! The step order is the design's ("The bff's steps for a PUT"), each failure
//! answered before the next step, so the manager call list of every case is
//! pinned:
//! - the session: none, or an unknown one, is 401 `session-required`, the
//!   manager is not connected to and nothing is written;
//! - the body: 400 `model-profiles-malformed` for a body that is not
//!   `{revision: string, values: object}`, a body whose `Content-Type` is not
//!   JSON (the header absent, or a type of another essence: the essence counts,
//!   so `application/json; charset=utf-8` is served, as every other route's
//!   body), a value that is neither a string nor
//!   `null`, a set value empty after trimming spaces and tabs or holding a
//!   control character other than a tab, a key outside `checks::WEB_MODEL_PROFILE_KEYS`
//!   in the list's exact spelling; 422 `comment-markers` with `field` the key,
//!   from the bff. All of them reach no manager at all: the body is answered
//!   before the read. A body over `origin::MAX_BODY` is the extractor's 413
//!   `payload-too-large`, also with no manager call;
//! - the section: `models.ini` read as the signed-in user, and the section
//!   matched in any letter case; else `models.ini.disabled`; else 404
//!   `no-model-profile-named`, the id echoed only when it is a name a model profile can carry.
//!   llama-server's global `[*]` is not a model profile: the same 404 with no args;
//! - the section revision: the digest of `<file name> LF <section text>` under
//!   the bff's key. A body carrying another one is 409 `model-profile-changed-elsewhere`
//!   with no write, and that includes the revision of the same section in the
//!   OTHER file and the one another bff computed;
//! - the edits, applied in key order (the map of `values` carries no order of
//!   its own every build can rely on), so two keys the section does not hold are
//!   appended in key order: `null` removes every line of the key, a string
//!   is written trimmed. A text equal to the one read is 200 with the current
//!   revision and NO write, so `{}` with a fresh revision is 200 and a stale one
//!   is 409 first;
//! - `write_file_if` with the text the save read as `expected`, as the signed-in
//!   user, one connect for the whole exchange. The manager's `Changed` is the
//!   409; its check of the edited section is 422 `{key, args, field}` with
//!   `field` by the design's table, one rule per key, and nothing is written;
//! - then `restart` and `running`, and the 200 is exactly
//!   `{"revision", "running", "restart"}`: the revision of the section as
//!   written (the one `GET /api/profiles/model/{id}` returns for the new text);
//!   `restart` the sorted ids of the instances the records of `assignments.ini`
//!   give the profile to (an explicit `on`, or `inherit` while the profile is
//!   switched on in the file it was written to) whose state is `active` or
//!   `activating`; and `running` is `restart` not empty, true with `restart:
//!   []` when a state read fails after a write that landed. A profile no
//!   instance is given is served nowhere: `restart: []`, `running: false`, and
//!   no state asked at all.
//!
//! A refusal of the signed-in user, at the connect, a read or a write, is 403
//! `session-not-administrator` and ends that session (the served sibling's rule,
//! which the contract's `client-not-authorized` line contradicts: the design
//! wins). 503 `client-unavailable` while no write was asked, and the write then
//! never happens; 504 `client-timeout` once it was; 200 with `running` true when
//! the write landed and only the state read ran out, and just the same when the
//! body needed no write at all; 502 `client-other`
//! otherwise, never the manager's own words. `write_timeout` bounds the whole
//! save, the reads with it; it is the write's own, longer limit, so a manager
//! that takes its time writing is not cut off at the limit that is right for a
//! read. A gate stops a manager call until the test releases it, `GRACE` is how
//! long a released call is given to reach a write, and `GATE_LIMIT` lets an
//! unreleased call through, so a test fails, it does not hang.
//!
//! The revision is computed in this file by an own SipHash-2-4, validated
//! against the published vectors (`the_helper_is_the_published_siphash_2_4`),
//! never read back from the code under test.

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

/// How long the bff waits for a manager read in these tests.
const TIMEOUT: Duration = Duration::from_millis(200);

/// How long a test waits for the bff's answer before failing.
const DEADLINE: Duration = Duration::from_secs(5);

/// How long a test waits, after the manager call it stopped has returned, for a
/// blocking task that was not cut off to reach a write.
const GRACE: Duration = Duration::from_millis(500);

/// How long a stopped call in the fake holds at most. The test releases it; a
/// gate nobody releases lets the call through, so a test fails, it does not
/// hang.
const GATE_LIMIT: Duration = Duration::from_secs(5);

const CLIENT: &str = "127.0.0.1:51000";

/// The revision secret of these tests, and another router's: a page read under
/// one is stale for the other, which is what a bff restart does to an open page.
const KEY_A: [u8; 16] = [0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15];
const KEY_B: [u8; 16] = [15, 14, 13, 12, 11, 10, 9, 8, 7, 6, 5, 4, 3, 2, 1, 0];

/// The names the manager knows the engine's model profile files by.
const MODELS: &str = "models.ini";
const DISABLED: &str = "models.ini.disabled";

/// The name the manager knows the assignment records by: which instance is
/// given which model profile.
const ASSIGNS: &str = "assignments.ini";

/// The body cap of `origin::MAX_BODY`, copied here (it is private to the bff):
/// the same copy `tests/origin.rs` keeps.
const MAX_BODY: usize = 64 * 1024;

/// The ids these tests save. `Qwen` is spelled as the file spells it, so a bff
/// that matches the id case-sensitively is caught.
const ALPHA: &str = "alpha";
const QWEN: &str = "Qwen";
const MISTRAL: &str = "mistral-7b";

/// A `models.ini` with two switched-on model profiles. `[alpha]` carries an inline
/// comment on a line the tests edit, a key to remove, and a blank line before
/// the next section, so the save's in-place edit has bytes to leave alone.
const FILE_ON: &str = "\
; The presets llama-server is offered (--models-preset).\r\n\
[alpha]\r\n\
model = C:\\Models\\alpha.gguf\r\n\
ctx-size = 4096 ; the context the page shows\r\n\
n-gpu-layers = 99\r\n\
\r\n\
[Qwen]\r\n\
model = /models/Qwen3.8-27B-Q8_0.gguf\r\n\
load-on-startup = 1\r\n\
temp = 0.7\r\n\
";

/// `FILE_ON` with `moe-cache-mib` set in `[alpha]`. The key is not on the web,
/// so a page can only get the pair refused by the manager's rule by writing the
/// other half of it: `split-mode = tensor` over a section that already carries
/// the cache.
const FILE_ON_MOE: &str = "\
; The presets llama-server is offered (--models-preset).\r\n\
[alpha]\r\n\
model = C:\\Models\\alpha.gguf\r\n\
ctx-size = 4096 ; the context the page shows\r\n\
n-gpu-layers = 99\r\n\
moe-cache-mib = 4096\r\n\
\r\n\
[Qwen]\r\n\
model = /models/Qwen3.8-27B-Q8_0.gguf\r\n\
load-on-startup = 1\r\n\
temp = 0.7\r\n\
";

/// A `models.ini.disabled` with one switched-off model profile, written as the
/// configurator writes it (its header, then the section).
const FILE_OFF: &str = "\
; Model profiles switched OFF in pai-config (the switch beside each model profile in\r\n\
; the Models tab). Same format as models.ini, but llama-server only reads\r\n\
; models.ini, so nothing here is offered. Switching a model profile back on moves its\r\n\
; section, unchanged, back to models.ini.\r\n\
[mistral-7b]\r\n\
model = /models/Mistral-7B-Instruct-v0.3-Q5_K_M.gguf\r\n\
ctx-size = 2048\r\n\
temp = 0.8\r\n\
";

// ── The texts a save writes ─────────────────────────────────────────────

/// `FILE_ON` with `ctx-size` set to 8192: that ONE line rewritten whole, as
/// `key = value` with its own ending, so the inline comment on it goes; every
/// other byte of the file, in both sections, as it was.
const ON_CTX_8192: &str = "\
; The presets llama-server is offered (--models-preset).\r\n\
[alpha]\r\n\
model = C:\\Models\\alpha.gguf\r\n\
ctx-size = 8192\r\n\
n-gpu-layers = 99\r\n\
\r\n\
[Qwen]\r\n\
model = /models/Qwen3.8-27B-Q8_0.gguf\r\n\
load-on-startup = 1\r\n\
temp = 0.7\r\n\
";

/// `FILE_ON` with a key the section did not hold: it lands after the section's
/// last real line, before the blank line, and moves nothing else.
const ON_TOPK_40: &str = "\
; The presets llama-server is offered (--models-preset).\r\n\
[alpha]\r\n\
model = C:\\Models\\alpha.gguf\r\n\
ctx-size = 4096 ; the context the page shows\r\n\
n-gpu-layers = 99\r\n\
top-k = 40\r\n\
\r\n\
[Qwen]\r\n\
model = /models/Qwen3.8-27B-Q8_0.gguf\r\n\
load-on-startup = 1\r\n\
temp = 0.7\r\n\
";

/// `FILE_ON` with `n-gpu-layers` removed: the line and its ending, nothing else.
const ON_NO_NGPU: &str = "\
; The presets llama-server is offered (--models-preset).\r\n\
[alpha]\r\n\
model = C:\\Models\\alpha.gguf\r\n\
ctx-size = 4096 ; the context the page shows\r\n\
\r\n\
[Qwen]\r\n\
model = /models/Qwen3.8-27B-Q8_0.gguf\r\n\
load-on-startup = 1\r\n\
temp = 0.7\r\n\
";

/// `FILE_OFF` with `temp` set to 1.0: the one line of the switched-off section.
const OFF_TEMP_1: &str = "\
; Model profiles switched OFF in pai-config (the switch beside each model profile in\r\n\
; the Models tab). Same format as models.ini, but llama-server only reads\r\n\
; models.ini, so nothing here is offered. Switching a model profile back on moves its\r\n\
; section, unchanged, back to models.ini.\r\n\
[mistral-7b]\r\n\
model = /models/Mistral-7B-Instruct-v0.3-Q5_K_M.gguf\r\n\
ctx-size = 2048\r\n\
temp = 1.0\r\n\
";

/// The section of `id` as `source` holds it, from its header through its last
/// line (`ini::text::section_text`, the text the revision hashes).
fn section_of(source: &str, id: &str) -> String {
    pai_core::ini::text::section_text(source, id).unwrap_or_else(|| panic!("[{id}] in {source:?}"))
}

// ── The revision, computed here ─────────────────────────────────────────

/// SipHash-2-4 of `s` under `key`, the reference implementation's own init
/// (`v0 ^= k0`, `v1 ^= k1`, `v2 ^= k0`, `v3 ^= k1` over the four constants of
/// the algorithm). The revision is keyed, so the expected value is computed
/// here and never read from the code under test.
fn siphash_2_4(key: &[u8; 16], s: &[u8]) -> u64 {
    let (k0, k1) = (
        u64::from_le_bytes(key[0..8].try_into().expect("8 bytes")),
        u64::from_le_bytes(key[8..16].try_into().expect("8 bytes")),
    );
    let (mut v0, mut v1, mut v2, mut v3) = (
        0x736f_6d65_7073_6575_u64 ^ k0,
        0x646f_7261_6e64_6f6d_u64 ^ k1,
        0x6c79_6765_6e65_7261_u64 ^ k0,
        0x7465_6462_7974_6573_u64 ^ k1,
    );
    let mix = |v0: &mut u64, v1: &mut u64, v2: &mut u64, v3: &mut u64| {
        *v0 = v0.wrapping_add(*v1);
        *v1 = v1.rotate_left(13);
        *v1 ^= *v0;
        *v0 = v0.rotate_left(32);
        *v2 = v2.wrapping_add(*v3);
        *v3 = v3.rotate_left(16);
        *v3 ^= *v2;
        *v0 = v0.wrapping_add(*v3);
        *v3 = v3.rotate_left(21);
        *v3 ^= *v0;
        *v2 = v2.wrapping_add(*v1);
        *v1 = v1.rotate_left(17);
        *v1 ^= *v2;
        *v2 = v2.rotate_left(32);
    };
    let mut b: u64 = (s.len() as u64) << 56;
    let mut i = 0;
    while i + 8 <= s.len() {
        let m = u64::from_le_bytes(s[i..i + 8].try_into().expect("8 bytes"));
        v3 ^= m;
        for _ in 0..2 {
            mix(&mut v0, &mut v1, &mut v2, &mut v3);
        }
        v0 ^= m;
        i += 8;
    }
    for j in 0..(s.len() - i) {
        b |= (s[i + j] as u64) << (j * 8);
    }
    v3 ^= b;
    for _ in 0..2 {
        mix(&mut v0, &mut v1, &mut v2, &mut v3);
    }
    v0 ^= b;
    v2 ^= 0xff;
    for _ in 0..4 {
        mix(&mut v0, &mut v1, &mut v2, &mut v3);
    }
    v0 ^ v1 ^ v2 ^ v3
}

/// The section's text the revision hashes, from the section as the file holds
/// it: its lines from the header through its last non-blank line, with that
/// last line's own ending (`\n` or `\r\n`) cut. The endings BETWEEN the lines
/// are the file's.
fn hashed_section(section: &str) -> &str {
    let mut end = section.len();
    for line in section.split_inclusive('\n').rev() {
        if !line.trim_end().is_empty() {
            break;
        }
        end -= line.len();
    }
    let kept = &section[..end];
    let kept = kept.strip_suffix("\r\n").unwrap_or(kept);
    kept.strip_suffix('\n').unwrap_or(kept)
}

/// The expected revision of a section: the file's name, a newline, the
/// section's lines as `hashed_section` cuts them.
fn revision_of(key: &[u8; 16], file: &str, section: &str) -> String {
    format!(
        "{:016x}",
        siphash_2_4(
            key,
            format!("{file}\n{}", hashed_section(section)).as_bytes()
        )
    )
}

/// The revision of the section `id` of the file `file` holding the text `text`.
fn rev(key: &[u8; 16], file: &str, text: &str, id: &str) -> String {
    revision_of(key, file, &section_of(text, id))
}

/// The helper is the published algorithm, not a lookalike: the reference
/// implementation's own vectors for the secret `00 01 .. 0f` (Appendix A of the
/// SipHash paper), for the empty input, one byte, a whole block and a block
/// with a tail. Every expectation of this file is therefore a vector of
/// SipHash-2-4, never a value taken from the code under test.
#[test]
fn the_helper_is_the_published_siphash_2_4() {
    let cases: [(&[u8], &str); 4] = [
        (b"", "726fdb47dd0e0e31"),
        (b"\x00", "74f839c593dc67fd"),
        (b"\x00\x01\x02\x03\x04\x05\x06\x07", "93f5f5799a932462"),
        (
            b"\x00\x01\x02\x03\x04\x05\x06\x07\x08\x09\x0a\x0b\x0c\x0d\x0e",
            "a129ca6149be45e5",
        ),
    ];
    for (text, want) in cases {
        assert_eq!(
            format!("{:016x}", siphash_2_4(&KEY_A, text)),
            want,
            "the published vector of {text:?}"
        );
    }
}

/// The revision of `[alpha]` of `FILE_ON` under this router's key: the one a
/// page read from these fixtures sends back.
fn alpha_revision() -> String {
    rev(&KEY_A, MODELS, FILE_ON, ALPHA)
}

/// The body of a save: `{revision, values}`, the contract's shape.
fn save_body(revision: &str, values: Value) -> Value {
    json!({ "revision": revision, "values": values })
}

// ── Fakes ───────────────────────────────────────────────────────────────

/// What a manager call answers when the test asks it to fail: the failures a
/// save can meet between reading the files and writing one of them.
#[derive(Clone)]
enum Outcome {
    /// The compare and swap: the file no longer holds what the bff read.
    Changed,
    Unavailable,
    Other,
    NotAuthorized,
    /// The manager's check of the edited section: its key and values, as the
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

/// The files the manager keeps live for the save: the current text of each, the
/// compare-and-swap writes it was asked for (`name`, `contents`, `expected`),
/// the plain writes (`name`, `contents`), the principals whose read or write it
/// refuses, the failures to answer a connect, a read, a write and a status, how
/// long each takes, the gates, and the instances with their state.
///
/// Every cell is inert by default, so a test that does not touch one is
/// untouched by it. Shared by every connect of one app, so a test can read a
/// file, change it behind the save, refuse a call, save and read again.
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
    /// plain `write_outcome`.
    write_outcome_at: Arc<Mutex<Option<(usize, Outcome)>>>,
    /// How long the read at one position (1, 2, ...) takes, in place of the
    /// plain `read_pause_ms`.
    read_pause_at: Arc<Mutex<Option<(usize, u64)>>>,
    /// How long the read, the write and the status read take.
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
    /// The instances, and the state of each: `restart` names those of them
    /// the profile is served on that are active or activating.
    instances: Arc<Mutex<Vec<String>>>,
    states: Arc<Mutex<HashMap<String, InstanceState>>>,
    /// How many writes of any kind the manager was asked for, so
    /// `write_outcome_at` counts the plain ones too.
    write_count: Arc<AtomicUsize>,
    /// How many reads the manager was asked for, so `read_pause_at` counts
    /// them too.
    read_count: Arc<AtomicUsize>,
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
/// test let go in `released_calls`. The save runs its connect, reads, writes
/// and status in one `spawn_blocking` task, so a parked call holds a
/// blocking-pool thread and not the route. A call holds at most `GATE_LIMIT`,
/// so a gate nobody releases lets the call through.
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

/// The connect as the fake answers it: it refuses with
/// `Live::connect_outcome` when one is set, inert by default. A refusal at the
/// connect means the manager was reached and refused the call: the stream never
/// opens, so no read, write, list or status is asked, and the file stays as it
/// was.
fn connect_hook(live: &Live) -> Result<(), ClientError> {
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
/// `status <id>`, ...) and refuses the reads and the writes of the principals
/// the test names, the way the real manager refuses a user who is no longer a
/// member of `pai`. It is the client of one connect, so it knows the principal
/// that connect was handed (`as_user`).
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

    /// The refusal of the read of this connect's principal.
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

    /// How long this read takes: the pause armed for its position (1-based,
    /// counting every read of this app) wins over the one armed for every read.
    fn read_pause(&self) -> u64 {
        let position = self.live.read_count.fetch_add(1, Ordering::SeqCst) + 1;
        let armed = *self
            .live
            .read_pause_at
            .lock()
            .unwrap_or_else(|e| e.into_inner());
        if let Some((at, ms)) = armed {
            if at == position {
                return ms;
            }
        }
        self.live.read_pause_ms.load(Ordering::SeqCst)
    }

    /// What the manager answers the write at `position` (1-based).
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
        let pause = self.read_pause();
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

    /// The compare and swap, as the real manager's is: it lands only while the
    /// file still holds the text the caller read, and the manager re-runs its
    /// checks of the edited section at this call.
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

    /// The unguarded write: a model profile save has no reason to ask for it.
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
            "a preset file has no API key to keep: the save has no reason to ask"
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

/// `alice` and `root`, two signed-in sessions (both members of `pai`, the group
/// sign-in admits), both with the password `correct horse`.
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
/// answering every connect with `connect_result`. The reads run under
/// `TIMEOUT`, the whole save under `write_timeout`, the revisions are computed
/// with `key`.
fn app_for_full(
    files: &[(&str, &str)],
    refuse: &[&str],
    connect_result: Result<(), ClientError>,
    write_timeout: Duration,
    key: [u8; 16],
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
    // The clock the session routes read, at the moment the app was built. A
    // test moves it with `App::advance`, so a test of the idle deadline is a
    // number, not a wait; every other test leaves it still.
    let start = Instant::now();
    let tick = Arc::new(AtomicU64::new(0));
    let clock = {
        let tick = tick.clone();
        Arc::new(move || start + Duration::from_millis(tick.load(Ordering::SeqCst)))
            as pai_bff::session::Clock
    };
    let router = pai_bff::router_with_session_and_write_timeout(
        connect,
        TIMEOUT,
        write_timeout,
        Arc::new(FakeAuth),
        clock,
        key,
    );
    App {
        router,
        log,
        connects,
        principals,
        live,
        tick,
    }
}

/// A router over a manager that keeps the model profile files live, with the manager's
/// call log, the number of connects, the principal of each, and this router's
/// revision key.
struct App {
    router: Router,
    log: Log,
    connects: Arc<AtomicUsize>,
    principals: Principals,
    live: Live,
    /// The clock's offset in milliseconds, moved by `App::advance`.
    tick: Arc<AtomicU64>,
}

/// The manager of the two model profile files, as `FILE_ON` and `FILE_OFF`.
fn app_holding(on: &str, off: &str) -> App {
    app_for_full(
        &[(MODELS, on), (DISABLED, off)],
        &[],
        Ok(()),
        TIMEOUT,
        KEY_A,
    )
}

/// The manager holding `FILE_ON` / `FILE_OFF` and `assignments` as
/// `assignments.ini`.
fn app_with_assignments(assignments: &str) -> App {
    app_for_full(
        &[
            (MODELS, FILE_ON),
            (DISABLED, FILE_OFF),
            (ASSIGNS, assignments),
        ],
        &[],
        Ok(()),
        TIMEOUT,
        KEY_A,
    )
}

/// The same pair, with the save bound by `write_timeout` and the reads left at
/// `TIMEOUT`: the seam a test of the write's own limit uses.
fn app_holding_write_timeout(on: &str, off: &str, write_timeout: Duration) -> App {
    app_for_full(
        &[(MODELS, on), (DISABLED, off)],
        &[],
        Ok(()),
        write_timeout,
        KEY_A,
    )
}

/// The pair plus `assignments.ini`, the save bound by `write_timeout`: the seam
/// a test of the limit needs when the restart list must have an instance in it.
fn app_with_assignments_timeout(assignments: &str, write_timeout: Duration) -> App {
    app_for_full(
        &[
            (MODELS, FILE_ON),
            (DISABLED, FILE_OFF),
            (ASSIGNS, assignments),
        ],
        &[],
        Ok(()),
        write_timeout,
        KEY_A,
    )
}

/// The same pair, under another router's revision key: a page read by the other
/// bff.
fn app_holding_key_b(on: &str, off: &str) -> App {
    app_for_full(
        &[(MODELS, on), (DISABLED, off)],
        &[],
        Ok(()),
        TIMEOUT,
        KEY_B,
    )
}

/// A manager holding `on` as `models.ini`, refusing the calls of the named
/// principals: the member of `pai` removed after the sign-in.
fn app_refusing(refuse: &[&str], on: &str, off: &str) -> App {
    app_for_full(
        &[(MODELS, on), (DISABLED, off)],
        refuse,
        Ok(()),
        TIMEOUT,
        KEY_A,
    )
}

/// The manager of no model profile file at all: neither file exists, so no id is named.
fn app_no_files() -> App {
    app_for_full(&[], &[], Ok(()), TIMEOUT, KEY_A)
}

/// The app that cannot reach the manager at all.
fn app_no_manager() -> App {
    app_for_full(
        &[(MODELS, FILE_ON), (DISABLED, FILE_OFF)],
        &[],
        Err(ClientError::Unavailable),
        TIMEOUT,
        KEY_A,
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

    /// A 422 value refusal: `{"key", "args", "field"}`, the key the form marks
    /// beside the check's key and values.
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
    /// member (`invalid-model-profile-id`, `model-profile-name-taken`).
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

    /// The save's 200: exactly `{"revision", "running", "restart"}`, answered
    /// as JSON, the revision a `GET` of the saved section returns. `restart`
    /// is the sorted list of the instances the profile is effective on (the
    /// records of `assignments.ini`) whose state is up: those are the ones the
    /// page offers to restart. `running` is `restart` not empty, or true with
    /// `restart: []` when a state could not be read.
    fn assert_saved(&self, revision: &str, running: bool, restart: &[&str]) {
        assert_eq!(self.status, StatusCode::OK, "{}", self.body);
        assert!(
            self.content_type.starts_with("application/json"),
            "content-type {:?}",
            self.content_type
        );
        assert_eq!(
            self.json(),
            json!({
                "revision": revision,
                "running": running,
                "restart": restart,
            }),
            "{}",
            self.body
        );
    }

    /// `GET /api/session` for a session that is still signed in: `{"user"}`.
    fn assert_signed_in_as(&self, user: &str) {
        assert_eq!(self.status, StatusCode::OK, "signed in: {}", self.body);
        assert_eq!(self.json(), json!({ "user": user }), "{}", self.body);
    }
}

/// The document of `GET /api/profiles/model/{id}`: its values, and its revision.
impl App {
    /// The manager's calls, in order.
    fn calls(&self) -> Vec<String> {
        self.log.lock().unwrap_or_else(|e| e.into_inner()).clone()
    }

    /// The manager's calls, sorted and joined by a space: what a save asks for,
    /// without pinning the order the route makes the calls in.
    fn sorted_calls(&self) -> String {
        let mut calls = self.calls();
        calls.sort();
        calls.join(" ")
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

    /// The `write_file` calls the manager was asked for, in order. The save has
    /// no reason to ask for one: both model profile files exist.
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

    /// The text of one model profile file, as the manager holds it now: `None` for a
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

    /// Move this app's clock `ms` forward: the session's idle deadline is
    /// measured on it, so a test can be half an hour late without waiting.
    fn advance(&self, ms: u64) {
        self.tick.fetch_add(ms, Ordering::SeqCst);
    }

    /// The state of one instance.
    fn set_state(&self, id: &str, state: InstanceState) {
        self.live.set_state(id, state);
    }

    /// Answer every coming connect with `outcome`.
    fn set_connect_outcome(&self, outcome: Outcome) {
        *self.live.connect_outcome.lock().unwrap() = Some(outcome);
    }

    /// Answer every coming read with `outcome`: the save stopped before it
    /// reached a write.
    fn set_read_outcome(&self, outcome: Outcome) {
        *self.live.read_outcome.lock().unwrap() = Some(outcome);
    }

    /// Answer every coming write with `outcome`, in place of landing: the file
    /// stays as it was.
    fn set_write_outcome(&self, outcome: Outcome) {
        *self.live.write_outcome.lock().unwrap() = Some(outcome);
    }

    /// Answer every coming status read with `outcome`.
    fn set_status_outcome(&self, outcome: Outcome) {
        *self.live.status_outcome.lock().unwrap() = Some(outcome);
    }

    /// Refuse the write of `user`: the member of `pai` removed between the read
    /// and the write.
    fn set_write_refused(&self, user: &str) {
        self.live
            .write_refused
            .lock()
            .unwrap()
            .push(user.to_string());
    }

    /// A read that takes `ms`.
    fn set_read_pause_ms(&self, ms: u64) {
        self.live.read_pause_ms.store(ms, Ordering::SeqCst);
    }

    /// A write that takes `ms`: the manager busy at the moment the save
    /// arrives, with a write asked.
    fn set_write_pause_ms(&self, ms: u64) {
        self.live.write_pause_ms.store(ms, Ordering::SeqCst);
    }

    /// A state read that takes `ms`: the manager busy when the save asks
    /// whether an instance is up.
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

    /// Stop the next write inside the manager call, until `release_write_gate`
    /// lets it go.
    fn arm_write_gate(&self) {
        self.live.write_gate_armed.store(true, Ordering::SeqCst);
    }
    fn release_write_gate(&self) {
        self.live.write_gate_released.store(true, Ordering::SeqCst);
    }
    fn writes_gated(&self) -> usize {
        self.live.writes_gated.load(Ordering::SeqCst)
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

    /// `PUT /api/profiles/model/{id}` with the body the web sends.
    async fn save(&self, id: &str, body: &Value, cookie: Option<&str>) -> Reply {
        self.save_raw(id, &body.to_string(), cookie, &[]).await
    }

    /// The save with the body spelled out (a body that is no JSON, an empty
    /// body, one over the cap) and extra headers (a request from another
    /// origin).
    async fn save_raw(
        &self,
        id: &str,
        body: &str,
        cookie: Option<&str>,
        extra: &[(&str, &str)],
    ) -> Reply {
        let mut builder = Request::builder()
            .method(Method::PUT)
            .uri(format!("/api/profiles/model/{id}"))
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

    /// The save with a `Content-Type` of its own: `None` sends the request with
    /// no `Content-Type` header at all. One header of one value, never a second
    /// one appended to the default.
    async fn save_content_type(
        &self,
        id: &str,
        body: &str,
        cookie: Option<&str>,
        content_type: Option<&str>,
    ) -> Reply {
        let mut builder = Request::builder()
            .method(Method::PUT)
            .uri(format!("/api/profiles/model/{id}"));
        if let Some(content_type) = content_type {
            builder = builder.header(header::CONTENT_TYPE, content_type);
        }
        if let Some(c) = cookie {
            builder = builder.header(header::COOKIE, c);
        }
        self.send(builder.body(Body::from(body.to_string())).expect("request"))
            .await
    }

    /// Signed in as `alice`, the save of `id` with `values` and the fresh
    /// revision of the section `id` of `FILE_ON`.
    async fn save_on(&self, id: &str, values: Value) -> Reply {
        let revision = rev(&KEY_A, MODELS, FILE_ON, id);
        self.save_on_revision(id, &revision, values).await
    }

    /// Signed in as `alice`, the save of `id` with `values` and `revision`.
    async fn save_on_revision(&self, id: &str, revision: &str, values: Value) -> Reply {
        let cookie = self.sign_in().await;
        self.save(id, &save_body(revision, values), Some(&cookie))
            .await
    }

    /// Signed in as `alice`, `GET /api/profiles/model/{id}`.
    async fn read(&self, id: &str) -> Reply {
        let cookie = self.sign_in().await;
        self.get(&format!("/api/profiles/model/{id}"), Some(&cookie))
            .await
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
// The session, and who may ask
// ---------------------------------------------------------------------------

/// The route answers the session first: the manager is not connected to, the
/// files are not read, nothing is written. A cookie naming no session is the
/// same answer.
#[tokio::test]
async fn save_without_a_session_is_401_and_the_manager_is_not_asked() {
    let app = app_holding(FILE_ON, FILE_OFF);
    let _real = app.sign_in().await;
    let body = save_body(&alpha_revision(), json!({ "ctx-size": "8192" }));
    for cookie in [
        None,
        // A cookie naming no session (made up, or one from before a restart).
        Some(&format!("pai_session={}", "ab".repeat(32))[..]),
    ] {
        let rep = app.save(ALPHA, &body, cookie).await;
        rep.assert_error(StatusCode::UNAUTHORIZED, "session-required");
    }
    assert_eq!(app.connects.load(Ordering::SeqCst), 0, "no connect");
    assert!(app.calls().is_empty(), "{:?}", app.calls());
    assert!(app.no_write_asked(), "nothing written");
    assert_eq!(app.file_text(MODELS).as_deref(), Some(FILE_ON));
}

/// Every signed-in administrator is served the save, and the manager is asked
/// for them: `root` saves the model profile as `root`.
#[tokio::test]
async fn save_is_served_to_every_signed_in_administrator() {
    let app = app_holding(FILE_ON, FILE_OFF);
    let cookie = app.sign_in_as("root").await;
    let body = save_body(&alpha_revision(), json!({ "ctx-size": "8192" }));
    app.save(ALPHA, &body, Some(&cookie)).await.assert_saved(
        &rev(&KEY_A, MODELS, ON_CTX_8192, ALPHA),
        false,
        &[],
    );
    assert_eq!(
        principals_of(&app.principals),
        vec![Some("root".to_string())],
        "the connect named the session's user"
    );
}

/// A name in the request never becomes the principal: a header naming another
/// user is ignored, the session's user is asked for.
#[tokio::test]
async fn save_a_name_in_the_request_is_never_the_principal() {
    let app = app_holding(FILE_ON, FILE_OFF);
    let cookie = app.sign_in().await;
    let body = save_body(&alpha_revision(), json!({ "ctx-size": "8192" }));
    app.save_raw(
        ALPHA,
        &body.to_string(),
        Some(&cookie),
        &[("x-user", "root")],
    )
    .await
    .assert_saved(&rev(&KEY_A, MODELS, ON_CTX_8192, ALPHA), false, &[]);
    assert_eq!(
        principals_of(&app.principals),
        vec![Some("alice".to_string())],
        "the session's user, never the request's"
    );
}

/// The same-origin guard the other routes serve answers before the session and
/// before the router: a signed-in request carrying a foreign `Origin` is
/// refused, the manager is not asked for anything.
#[tokio::test]
async fn save_with_a_foreign_origin_is_refused_before_the_manager_is_read() {
    let app = app_holding(FILE_ON, FILE_OFF);
    let cookie = app.sign_in().await;
    let body = save_body(&alpha_revision(), json!({ "ctx-size": "8192" }));
    let rep = app
        .save_raw(
            ALPHA,
            &body.to_string(),
            Some(&cookie),
            &[("origin", "https://evil.example")],
        )
        .await;
    rep.assert_error(StatusCode::FORBIDDEN, "request-cross-origin");
    assert!(app.calls().is_empty(), "{:?}", app.calls());
    assert!(app.no_write_asked(), "nothing written");
}

// ---------------------------------------------------------------------------
// The body, answered before the manager is read
// ---------------------------------------------------------------------------

/// The bodies, each built from the well-typed one with one thing broken, that
/// are no model profile save at all: 400 `model-profiles-malformed`, and no manager call of
/// any kind. The design answers the body before it reads a file.
#[tokio::test]
async fn save_body_that_is_no_model_profile_save_is_400_and_reaches_no_manager() {
    let app = app_holding(FILE_ON, FILE_OFF);
    let cookie = app.sign_in().await;
    let fresh = alpha_revision();
    let good = save_body(&fresh, json!({ "ctx-size": "8192" }));
    let mut no_revision = good.clone();
    no_revision
        .as_object_mut()
        .expect("object")
        .remove("revision");
    let mut no_values = good.clone();
    no_values.as_object_mut().expect("object").remove("values");
    for (case, body) in [
        ("no JSON at all", "{".to_string()),
        ("empty body", String::new()),
        ("not JSON", "not json at all".to_string()),
        ("an array", json!([ALPHA]).to_string()),
        (
            "revision a number",
            json!({ "revision": 7, "values": {} }).to_string(),
        ),
        ("no revision", no_revision.to_string()),
        (
            "values an array",
            json!({ "revision": fresh, "values": [] }).to_string(),
        ),
        (
            "values a string",
            json!({ "revision": fresh, "values": "ctx-size" }).to_string(),
        ),
        ("no values", no_values.to_string()),
    ] {
        let rep = app.save_raw(ALPHA, &body, Some(&cookie), &[]).await;
        rep.assert_error(StatusCode::BAD_REQUEST, "model-profiles-malformed");
        assert!(
            app.calls().is_empty(),
            "{case}: the body is answered before the read: {:?}",
            app.calls()
        );
    }
    assert!(app.no_write_asked(), "nothing written");
    assert_eq!(app.file_text(MODELS).as_deref(), Some(FILE_ON));
}

/// The content type the body must carry: the route reads the raw bytes, so the
/// type is its only sign of what the bytes are meant to be. A signed-in request
/// with the contract's body and a type of another essence, and one with no
/// `Content-Type` at all, are the same 400 `model-profiles-malformed` as a body that
/// is no save: the manager is not connected to, nothing is written, the file is
/// as it was.
#[tokio::test]
async fn save_a_body_whose_content_type_is_not_json_is_400_and_reaches_no_manager() {
    let app = app_holding(FILE_ON, FILE_OFF);
    let cookie = app.sign_in().await;
    let good = save_body(&alpha_revision(), json!({ "ctx-size": "8192" })).to_string();
    for (case, content_type) in [
        ("text/plain", Some("text/plain")),
        (
            "application/x-www-form-urlencoded",
            Some("application/x-www-form-urlencoded"),
        ),
        ("no Content-Type header", None),
    ] {
        let rep = app
            .save_content_type(ALPHA, &good, Some(&cookie), content_type)
            .await;
        rep.assert_error(StatusCode::BAD_REQUEST, "model-profiles-malformed");
        assert_eq!(
            app.connects.load(Ordering::SeqCst),
            0,
            "{case}: the body is answered before the manager is connected to"
        );
        assert!(app.calls().is_empty(), "{case}: {:?}", app.calls());
    }
    assert!(app.no_write_asked(), "nothing written: {:?}", app.writes());
    assert_eq!(app.file_text(MODELS).as_deref(), Some(FILE_ON));
}

/// The essence of the type is what counts, not the whole header: the type the
/// browsers of the page send, `application/json; charset=utf-8`, is JSON, and
/// the save is served, the file written.
#[tokio::test]
async fn save_a_json_content_type_with_a_charset_is_served() {
    let app = app_holding(FILE_ON, FILE_OFF);
    let cookie = app.sign_in().await;
    let good = save_body(&alpha_revision(), json!({ "ctx-size": "8192" })).to_string();
    app.save_content_type(
        ALPHA,
        &good,
        Some(&cookie),
        Some("application/json; charset=utf-8"),
    )
    .await
    .assert_saved(&rev(&KEY_A, MODELS, ON_CTX_8192, ALPHA), false, &[]);
    assert_eq!(
        app.file_text(MODELS).as_deref(),
        Some(ON_CTX_8192),
        "the save landed"
    );
}

/// A key the web may not write, in any spelling: the list is exact-case, and a
/// name it does not hold is as unknown as one no reader knows. 400, and the
/// manager is not asked.
#[tokio::test]
async fn save_key_outside_the_web_list_is_400_and_reaches_no_manager() {
    let app = app_holding(FILE_ON, FILE_OFF);
    for key in [
        "model",     // the Assets card, not web-editable
        "mmproj", // the same
        "moe-cache-mib",
        "override-tensor",
        "foo",            // a name no reader knows
        "CTX-SIZE",       // the right key, the wrong case
        "ctx-size-draft", // a real key, not this one
    ] {
        let rep = app.save_on(ALPHA, json!({ key: "8192" })).await;
        rep.assert_error(StatusCode::BAD_REQUEST, "model-profiles-malformed");
    }
    assert!(app.calls().is_empty(), "{:?}", app.calls());
    assert!(app.no_write_asked(), "nothing written");
    assert_eq!(app.file_text(MODELS).as_deref(), Some(FILE_ON));
}

/// A value that is no one-line text: neither a string nor `null`, empty after
/// trimming, or holding a control character other than a tab (CR and LF would
/// end the line in the file). 400, and no manager call.
#[tokio::test]
async fn save_value_that_is_no_one_line_text_is_400_and_reaches_no_manager() {
    let app = app_holding(FILE_ON, FILE_OFF);
    for (case, value) in [
        ("a number", json!(8192)),
        ("an object", json!({ "a": 1 })),
        ("a bool", json!(true)),
        ("empty", json!("")),
        ("spaces and tabs only", json!(" \t ")),
        ("a line feed", json!("8192\n99")),
        ("a carriage return", json!("8192\r99")),
        ("a NUL", json!("8192\099")),
        ("a DEL", json!("8192\x7f99")),
    ] {
        let rep = app.save_on(ALPHA, json!({ "ctx-size": value })).await;
        rep.assert_error(StatusCode::BAD_REQUEST, "model-profiles-malformed");
        assert!(
            app.calls().is_empty(),
            "{case}: the body is answered before the read: {:?}",
            app.calls()
        );
    }
    assert!(app.no_write_asked(), "nothing written");
    assert_eq!(app.file_text(MODELS).as_deref(), Some(FILE_ON));
}

/// `;` and `#` end a value for every reader of the file, so the bff refuses
/// them itself: 422 `comment-markers`, `args` every key whose value holds a
/// marker and `field` the first of them, with the manager never connected to
/// and no write asked.
#[tokio::test]
async fn save_value_with_a_comment_marker_is_422_from_the_bff_with_the_field() {
    let app = app_holding(FILE_ON, FILE_OFF);
    let rep = app
        .save_on(
            ALPHA,
            json!({ "temp": "0.7 # less", "ctx-size": "8192 ; note" }),
        )
        .await;
    rep.assert_refused("comment-markers", &["ctx-size", "temp"], "ctx-size");
    assert_eq!(
        app.connects.load(Ordering::SeqCst),
        0,
        "the bff answers this itself: {:?}",
        app.calls()
    );
    assert!(app.calls().is_empty(), "{:?}", app.calls());
    assert!(app.no_write_asked(), "nothing written");
    assert_eq!(app.file_text(MODELS).as_deref(), Some(FILE_ON));
}

/// A device-list value holding `;`/`#` is the bff's marker refusal like every
/// other key ("As built" ruling a): the web's value reloads in the file as
/// less than what was written, so the bff refuses it before the manager is
/// asked: 422 `comment-markers`, `args` and `field` `device`, no connect, no
/// write. The manager's device rule never sees a marker: it runs on the value
/// the reader cuts.
#[tokio::test]
async fn save_device_value_with_a_comment_marker_is_422_comment_markers() {
    let app = app_holding(FILE_ON, FILE_OFF);
    let rep = app.save_on(ALPHA, json!({ "device": "ROCm0;x" })).await;
    rep.assert_refused("comment-markers", &["device"], "device");
    assert_eq!(
        app.connects.load(Ordering::SeqCst),
        0,
        "the bff answers this itself: {:?}",
        app.calls()
    );
    assert!(app.calls().is_empty(), "{:?}", app.calls());
    assert!(app.no_write_asked(), "nothing written");
    assert_eq!(app.file_text(MODELS).as_deref(), Some(FILE_ON));
}

/// A body over the extractor's cap is 413 `payload-too-large`, answered before
/// the session and before any manager call.
#[tokio::test]
async fn save_body_over_the_size_limit_is_413_and_reaches_no_manager() {
    // The padding goes in a value, so the body's shape stays the contract's.
    let shell = save_body("0", json!({ "ctx-size": "" })).to_string();
    let big = save_body(
        "0",
        json!({ "ctx-size": "x".repeat(MAX_BODY + 1 - shell.len()) }),
    )
    .to_string();
    assert_eq!(big.len(), MAX_BODY + 1, "the body is one over the cap");
    let app = app_holding(FILE_ON, FILE_OFF);
    let cookie = app.sign_in().await;
    let rep = app.save_raw(ALPHA, &big, Some(&cookie), &[]).await;
    rep.assert_error(StatusCode::PAYLOAD_TOO_LARGE, "payload-too-large");
    assert_eq!(app.connects.load(Ordering::SeqCst), 0, "no connect");
    assert!(app.calls().is_empty(), "{:?}", app.calls());
    assert!(app.no_write_asked(), "nothing written");
}

/// The method the contract does not list is the router's 405, and asks the
/// manager nothing. The save and the read are the methods of that path.
#[tokio::test]
async fn save_is_the_only_write_method_the_model_profile_path_serves() {
    for method in [Method::POST, Method::DELETE, Method::PATCH] {
        let app = app_holding(FILE_ON, FILE_OFF);
        let cookie = app.sign_in().await;
        let builder = Request::builder()
            .method(method.clone())
            .uri(format!("/api/profiles/model/{ALPHA}"))
            .header(header::COOKIE, &cookie)
            .header(header::CONTENT_TYPE, "application/json");
        let request = builder.body(Body::empty()).expect("request");
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

/// A path below the section is answered by the router, not by the app: 404 with
/// no body and no content type, and no manager call. The save's id is one
/// percent-decoded segment, never a path.
#[tokio::test]
async fn save_of_a_path_below_the_section_is_the_routers_own_answer() {
    let app = app_holding(FILE_ON, FILE_OFF);
    let cookie = app.sign_in().await;
    let rep = app
        .save_raw(
            "alpha/below",
            &save_body(&alpha_revision(), json!({ "ctx-size": "8192" })).to_string(),
            Some(&cookie),
            &[],
        )
        .await;
    assert_eq!(rep.status, StatusCode::NOT_FOUND, "{}", rep.body);
    assert_eq!(rep.content_type, "", "the router's own 404 carries no JSON");
    assert_eq!(rep.body, "", "the router's own 404 carries no body");
    assert_eq!(app.connects.load(Ordering::SeqCst), 0, "no connect");
    assert!(app.no_write_asked(), "nothing written");
}

// ---------------------------------------------------------------------------
// The section: where it is, and which id is one
// ---------------------------------------------------------------------------

/// The contract's answer for a name no model profile has, in either file: 404
/// `no-model-profile-named` with the id as its only arg, and nothing written.
#[tokio::test]
async fn save_unknown_id_is_404_with_the_id_and_writes_nothing() {
    let app = app_holding(FILE_ON, FILE_OFF);
    let revision = rev(&KEY_A, MODELS, FILE_ON, ALPHA);
    let rep = app
        .save_on_revision("nope", &revision, json!({ "ctx-size": "8192" }))
        .await;
    rep.assert_error_with_args(StatusCode::NOT_FOUND, "no-model-profile-named", &["nope"]);
    assert_eq!(app.file_text(MODELS).as_deref(), Some(FILE_ON));
    assert_eq!(app.file_text(DISABLED).as_deref(), Some(FILE_OFF));
    assert!(app.no_write_asked(), "nothing written");
    assert_eq!(
        app.calls(),
        vec!["read models.ini", "read models.ini.disabled"],
        "both files are looked in, nothing else"
    );
}

/// A machine with no model profile file at all answers the same 404, and no file is
/// created by a save.
#[tokio::test]
async fn save_with_no_model_profile_file_is_404_and_creates_nothing() {
    let app = app_no_files();
    let rep = app
        .save_on_revision(ALPHA, "0000000000000000", json!({ "ctx-size": "8192" }))
        .await;
    rep.assert_error_with_args(StatusCode::NOT_FOUND, "no-model-profile-named", &[ALPHA]);
    assert_eq!(app.file_text(MODELS), None, "no models.ini was created");
    assert_eq!(
        app.file_text(DISABLED),
        None,
        "no disabled file was created"
    );
    assert!(app.no_write_asked(), "nothing written");
}

/// llama-server's global `[*]` section is not a model profile: naming it, raw,
/// url-encoded or bracketed, is the 404 of a name no model profile carries, with no
/// args (the id check refuses `*`, so the id is echoed nowhere). The section
/// stays where it is.
#[tokio::test]
async fn save_the_global_section_is_404_with_no_args() {
    let on = "\
[*]\r\n\
model = /models/global.gguf\r\n\
n-gpu-layers = 99\r\n\
\r\n\
[alpha]\r\n\
model = /models/alpha.gguf\r\n\
";
    let app = app_holding(on, FILE_OFF);
    for id in ["*", "%2A", "%5B%2A%5D"] {
        let rep = app
            .save_on_revision(id, "0000000000000000", json!({ "n-gpu-layers": "1" }))
            .await;
        rep.assert_error_with_args(StatusCode::NOT_FOUND, "no-model-profile-named", &[]);
    }
    assert_eq!(
        app.file_text(MODELS).as_deref(),
        Some(on),
        "the global section is where it was"
    );
    assert!(app.no_write_asked(), "nothing written: {:?}", app.calls());
}

/// The id is matched against section headers, and the two file names are the
/// fixed ones, so a URL carrying a path, a newline or a bracket, and a run of
/// dots no model profile carries, is a name no model profile has: the same 404 with no args,
/// no write of any kind, and no file created.
#[tokio::test]
async fn save_a_name_no_model_profile_can_carry_is_404_with_no_args() {
    for id in [
        "two%0Alines",
        "two%0Dlines",
        "with%5Bbracket%5D",
        "with%3Dequals",
        "..%2F..%2F..%2Fetc%2Fpasswd",
        "%2E%2E",
        "%2E",
    ] {
        let app = app_no_files();
        let rep = app
            .save_on_revision(id, "0000000000000000", json!({ "ctx-size": "8192" }))
            .await;
        rep.assert_error_with_args(StatusCode::NOT_FOUND, "no-model-profile-named", &[]);
        assert!(app.no_write_asked(), "{id}: {:?}", app.calls());
        assert_eq!(app.file_text(MODELS), None, "{id}: no file created");
        assert_eq!(app.file_text(DISABLED), None, "{id}: no file created");
    }
}

/// A section is found in the file that holds it: `mistral-7b` is switched off,
/// so the save writes `models.ini.disabled`, leaves `models.ini` byte for
/// byte, and answers `running: false` without asking the manager for a single
/// instance state (the engine never reads that file).
#[tokio::test]
async fn save_a_switched_off_model_profile_is_written_to_the_disabled_file_and_answers_running_false(
) {
    let app = app_holding(FILE_ON, FILE_OFF);
    app.set_state("default", InstanceState::Active);
    let revision = rev(&KEY_A, DISABLED, FILE_OFF, MISTRAL);
    let rep = app
        .save_on_revision(MISTRAL, &revision, json!({ "temp": "1.0" }))
        .await;
    rep.assert_saved(&rev(&KEY_A, DISABLED, OFF_TEMP_1, MISTRAL), false, &[]);
    assert_eq!(
        app.writes(),
        vec![(
            DISABLED.to_string(),
            OFF_TEMP_1.to_string(),
            FILE_OFF.to_string()
        )],
        "the file that holds the section, expecting the text the save read"
    );
    assert_eq!(
        app.file_text(DISABLED).as_deref(),
        Some(OFF_TEMP_1),
        "the disabled file carries the edit"
    );
    assert_eq!(
        app.file_text(MODELS).as_deref(),
        Some(FILE_ON),
        "models.ini is byte for byte what it was"
    );
    assert_eq!(
        app.calls(),
        vec![
            "read models.ini",
            "read models.ini.disabled",
            "write-if alice models.ini.disabled",
            "read assignments.ini",
        ],
        "a switched-off preset asks for no instance state (the restart reads \
         assignments.ini, never a status)"
    );
}

/// The id on the URL is matched the way the list's ids are: the section is
/// spelled `[Qwen]`, and `qwen` finds it, edits that very section and writes
/// `models.ini` with the file's own spelling of the header.
#[tokio::test]
async fn save_matches_the_id_ignoring_case() {
    let app = app_holding(FILE_ON, FILE_OFF);
    let revision = rev(&KEY_A, MODELS, FILE_ON, QWEN);
    let rep = app
        .save_on_revision("qwen", &revision, json!({ "temp": "0.2" }))
        .await;
    let wanted = "\
; The presets llama-server is offered (--models-preset).\r\n\
[alpha]\r\n\
model = C:\\Models\\alpha.gguf\r\n\
ctx-size = 4096 ; the context the page shows\r\n\
n-gpu-layers = 99\r\n\
\r\n\
[Qwen]\r\n\
model = /models/Qwen3.8-27B-Q8_0.gguf\r\n\
load-on-startup = 1\r\n\
temp = 0.2\r\n\
";
    rep.assert_saved(&rev(&KEY_A, MODELS, wanted, QWEN), false, &[]);
    assert_eq!(
        app.writes(),
        vec![(MODELS.to_string(), wanted.to_string(), FILE_ON.to_string())],
        "the section of [Qwen] edited, spelled as the file spells it"
    );
    assert_eq!(
        section_of(&app.file_text(MODELS).unwrap(), ALPHA),
        section_of(FILE_ON, ALPHA),
        "the section the save did not touch is byte for byte as it was"
    );
}

// ---------------------------------------------------------------------------
// The revision: the guard of the page
// ---------------------------------------------------------------------------

/// A fresh revision writes: one connect for the signed-in user, `models.ini`
/// read (it holds the section, so the disabled file is never opened), the file
/// written by the compare and swap with the text the save read as `expected`,
/// then the instances for `running`. The 200 is exactly `{"revision",
/// "running"}`, and that revision is the one a `GET` of the section answers
/// afterwards.
#[tokio::test]
async fn save_fresh_revision_writes_the_file_and_answers_the_revision_the_read_returns() {
    let app = app_holding(FILE_ON, FILE_OFF);
    let rep = app.save_on(ALPHA, json!({ "ctx-size": "8192" })).await;
    let written = rev(&KEY_A, MODELS, ON_CTX_8192, ALPHA);
    rep.assert_saved(&written, false, &[]);
    assert_eq!(
        app.writes(),
        vec![(
            MODELS.to_string(),
            ON_CTX_8192.to_string(),
            FILE_ON.to_string()
        )],
        "the compare and swap expects the text this save read"
    );
    assert!(
        app.plain_writes().is_empty(),
        "both files exist: the compare and swap is the only channel"
    );
    assert_eq!(
        principals_of(&app.principals),
        vec![Some("alice".to_string())],
        "one connect, for the session's user"
    );
    assert_eq!(
        app.sorted_calls(),
        "read assignments.ini read models.ini write-if alice models.ini",
        "the section is found in the first file, so the other is never read; no \
         call beyond the save, the write as the signed-in user, then the records \
         that say where the profile is served (none here, so no state is asked)"
    );
    assert_eq!(
        app.file_text(MODELS).as_deref(),
        Some(ON_CTX_8192),
        "the file carries what the save wrote"
    );
    // The page's next read answers the same revision: no 409 on the second save.
    let doc = app.read(ALPHA).await.json();
    assert_eq!(
        doc["revision"].as_str(),
        Some(written.as_str()),
        "the 200's revision is the one GET /api/profiles/model/alpha returns"
    );
    assert_eq!(
        doc["values"]["ctx-size"],
        json!("8192"),
        "the read shows the saved value"
    );
}

/// Only the edited line changes: the line is rewritten whole as `key = value`
/// (its inline comment goes, said in the design), every other line of both
/// sections keeps its bytes, its spelling and its ending, and a key the section
/// did not hold lands after its last real line.
#[tokio::test]
async fn save_the_written_text_changes_only_the_edited_lines() {
    let app = app_holding(FILE_ON, FILE_OFF);
    app.save_on(ALPHA, json!({ "ctx-size": "8192" }))
        .await
        .assert_saved(&rev(&KEY_A, MODELS, ON_CTX_8192, ALPHA), false, &[]);
    let text = app.file_text(MODELS).expect("models.ini");
    assert!(
        text.starts_with("; The presets llama-server is offered (--models-preset).\r\n"),
        "the file's own header survives: {text:?}"
    );
    assert!(
        text.contains("model = C:\\Models\\alpha.gguf\r\n"),
        "the untouched line keeps its exact spelling and ending: {text:?}"
    );
    assert!(
        !text.contains("the context the page shows"),
        "the edited line is rewritten whole: {text:?}"
    );
    assert_eq!(
        section_of(&text, QWEN),
        section_of(FILE_ON, QWEN),
        "the other section is byte for byte as it was"
    );

    // A key the section does not hold: appended after its last real line.
    let app = app_holding(FILE_ON, FILE_OFF);
    app.save_on(ALPHA, json!({ "top-k": "40" }))
        .await
        .assert_saved(&rev(&KEY_A, MODELS, ON_TOPK_40, ALPHA), false, &[]);
    assert_eq!(
        app.file_text(MODELS).as_deref(),
        Some(ON_TOPK_40),
        "the new key lands inside [alpha], not at the end of the file"
    );

    // A `null` removes every line of the key, its ending with it.
    let app = app_holding(FILE_ON, FILE_OFF);
    app.save_on(ALPHA, json!({ "n-gpu-layers": Value::Null }))
        .await
        .assert_saved(&rev(&KEY_A, MODELS, ON_NO_NGPU, ALPHA), false, &[]);
    assert_eq!(app.file_text(MODELS).as_deref(), Some(ON_NO_NGPU));
    assert!(
        !app.file_text(MODELS).unwrap().contains("n-gpu-layers = 99"),
        "the line and its ending are gone"
    );
}

/// A value the section already holds is written by nobody: `set_key`'s
/// equal-value case leaves the line byte for byte (its comment and spacing
/// kept), so the save answers 200 with the revision the body carried and asks
/// for no write at all.
#[tokio::test]
async fn save_a_value_the_file_holds_writes_nothing_and_keeps_the_line() {
    let app = app_holding(FILE_ON, FILE_OFF);
    let fresh = alpha_revision();
    let rep = app.save_on(ALPHA, json!({ "ctx-size": "4096" })).await;
    rep.assert_saved(&fresh, false, &[]);
    assert!(app.no_write_asked(), "nothing to write: {:?}", app.writes());
    assert_eq!(
        app.file_text(MODELS).as_deref(),
        Some(FILE_ON),
        "the line, its spacing and its inline comment are byte for byte as they were"
    );
}

/// `{}` with a fresh revision is the save's 200 with the current revision and
/// no write.
#[tokio::test]
async fn save_no_changes_is_200_with_the_same_revision_and_no_write() {
    let app = app_holding(FILE_ON, FILE_OFF);
    let fresh = alpha_revision();
    let rep = app.save_on(ALPHA, json!({})).await;
    rep.assert_saved(&fresh, false, &[]);
    assert!(app.no_write_asked(), "no key, no write: {:?}", app.writes());
    assert_eq!(app.file_text(MODELS).as_deref(), Some(FILE_ON));
}

/// A stale revision is 409 `model-profile-changed-elsewhere` with nothing written, the
/// session kept, and the file as it was. A body that changes nothing is
/// answered the same way: the revision is compared before the text is compared.
#[tokio::test]
async fn save_a_stale_revision_is_409_with_no_write() {
    for values in [json!({ "ctx-size": "8192" }), json!({})] {
        let app = app_holding(FILE_ON, FILE_OFF);
        // The revision of the text a first save wrote: the page is one save behind.
        let stale = rev(&KEY_A, MODELS, ON_CTX_8192, ALPHA);
        let cookie = app.sign_in().await;
        let rep = app
            .save(ALPHA, &save_body(&stale, values.clone()), Some(&cookie))
            .await;
        rep.assert_error(StatusCode::CONFLICT, "model-profile-changed-elsewhere");
        assert!(
            app.no_write_asked(),
            "the revision is compared before the write: {:?}",
            app.calls()
        );
        assert_eq!(
            app.file_text(MODELS).as_deref(),
            Some(FILE_ON),
            "the file is as it was"
        );
        app.get("/api/session", Some(&cookie))
            .await
            .assert_signed_in_as("alice");
    }
}

/// The revision carries the file's name, so the same section's text in the
/// other file is another model profile: a page that read the disabled copy is 409
/// against `models.ini`, and nothing is written.
#[tokio::test]
async fn save_a_revision_taken_from_the_other_file_is_409() {
    let app = app_holding(FILE_ON, FILE_OFF);
    // The digest of the same section text under the disabled file's name.
    let other = revision_of(&KEY_A, DISABLED, &section_of(FILE_ON, ALPHA));
    let rep = app
        .save_on_revision(ALPHA, &other, json!({ "ctx-size": "8192" }))
        .await;
    rep.assert_error(StatusCode::CONFLICT, "model-profile-changed-elsewhere");
    assert!(app.no_write_asked(), "nothing written: {:?}", app.calls());
    assert_eq!(app.file_text(MODELS).as_deref(), Some(FILE_ON));
    assert_eq!(app.file_text(DISABLED).as_deref(), Some(FILE_OFF));
}

/// A bff restart changes the key, so every open page is stale once: a revision
/// computed under another router's key is the same 409, with no write.
#[tokio::test]
async fn save_a_revision_of_another_router_is_409() {
    let app = app_holding_key_b(FILE_ON, FILE_OFF);
    let from_a = alpha_revision();
    let rep = app
        .save_on_revision(ALPHA, &from_a, json!({ "ctx-size": "8192" }))
        .await;
    rep.assert_error(StatusCode::CONFLICT, "model-profile-changed-elsewhere");
    assert!(app.no_write_asked(), "nothing written: {:?}", app.calls());
    assert_eq!(app.file_text(MODELS).as_deref(), Some(FILE_ON));
}

/// The compare and swap found its file changed after the read: 409
/// `model-profile-changed-elsewhere`, the write was asked, nothing landed, the session
/// kept.
#[tokio::test]
async fn save_changed_at_the_write_is_409_and_leaves_the_file() {
    let app = app_holding(FILE_ON, FILE_OFF);
    app.set_write_outcome(Outcome::Changed);
    let cookie = app.sign_in().await;
    let rep = app
        .save(
            ALPHA,
            &save_body(&alpha_revision(), json!({ "ctx-size": "8192" })),
            Some(&cookie),
        )
        .await;
    rep.assert_error(StatusCode::CONFLICT, "model-profile-changed-elsewhere");
    assert_eq!(
        app.writes().len(),
        1,
        "the write was asked, and refused: {:?}",
        app.writes()
    );
    assert_eq!(
        app.file_text(MODELS).as_deref(),
        Some(FILE_ON),
        "a refused compare leaves the file"
    );
    app.get("/api/session", Some(&cookie))
        .await
        .assert_signed_in_as("alice");
}

// ---------------------------------------------------------------------------
// The manager's check of the edited section: 422, one rule per field
// ---------------------------------------------------------------------------

/// The manager checks the edited section whole, so its refusal can name a key
/// the page shows read-only (`model`, `moe-cache-mib`, `override-tensor`): the
/// 422 still marks the field the design's table gives for that rule, verbatim
/// with the manager's own `args`. `field` is `args[0]` for the rules that name
/// their key; `fit-target-format`, the MoE cache pair, the load-on-startup pair,
/// `model-profile-no-model`, `model-profile-model-missing` (its args are the model
/// profile, the instance and the file) and the `override-no-*` pair name a
/// position, a count or nothing, so each maps to the key of its control; the
/// pair about the model profile's
/// name marks no field at all (the next test). The section is never written, and
/// a 422 is the manager's news about the values, not a refusal of the user: the
/// session lives, and no state is asked.
#[tokio::test]
async fn save_the_managers_check_marks_the_field_of_its_rule() {
    const CASES: &[(&str, &[&str], &str)] = &[
        // The rules that name the key they are about: `args[0]`.
        (
            "not-whole-number-in-range",
            &["ctx-size", "0", "1", "1048576"],
            "ctx-size",
        ),
        ("comment-markers", &["temp"], "temp"),
        ("not-a-number", &["temp", "abc"], "temp"),
        (
            "not-one-of",
            &["reasoning-format", "deepseek", "auto, deepseek"],
            "reasoning-format",
        ),
        (
            "not-json-object",
            &["chat-template-kwargs"],
            "chat-template-kwargs",
        ),
        (
            "min-above-max",
            &["image-min-tokens", "2048", "image-max-tokens", "1024"],
            "image-min-tokens",
        ),
        // The rules that name a value, a position or a count: the control.
        ("fit-target-format", &["12ab", "MiB"], "fit-target"),
        ("moe-cache-with-tensor-split", &[], "moe-cache-mib"),
        ("load-on-startup-over-max", &["4", "2"], "load-on-startup"),
        ("load-on-startup-unlimited", &["3"], "load-on-startup"),
        ("model-profile-no-model", &[ALPHA], "model"),
        (
            "model-profile-model-missing",
            &[ALPHA, "default", "/models/alpha.gguf"],
            "model",
        ),
        ("override-no-pattern", &["1"], "override-tensor"),
        (
            "override-no-device",
            &["2", "llama-expert"],
            "override-tensor",
        ),
    ];
    for &(key, args, field) in CASES {
        let app = app_holding(FILE_ON, FILE_OFF);
        app.set_write_outcome(Outcome::Invalid {
            key,
            values: args.iter().map(|a| a.to_string()).collect(),
        });
        let cookie = app.sign_in().await;
        let rep = app
            .save(
                ALPHA,
                &save_body(&alpha_revision(), json!({ "ctx-size": "8192" })),
                Some(&cookie),
            )
            .await;
        rep.assert_refused(key, args, field);
        assert_eq!(
            app.calls(),
            vec![format!("read {MODELS}"), format!("write-if alice {MODELS}")],
            "{key}: the answer is the write's refusal, and no state is asked"
        );
        assert_eq!(
            app.file_text(MODELS).as_deref(),
            Some(FILE_ON),
            "{key}: the manager wrote nothing"
        );
        app.get("/api/session", Some(&cookie))
            .await
            .assert_signed_in_as("alice");
    }
}

/// The two refusals about the model profile's name mark no field: there is no key of
/// the section beside them, so the answer carries no `field` member at all.
#[tokio::test]
async fn save_a_refusal_about_the_name_carries_no_field() {
    for key in ["invalid-model-profile-id", "model-profile-name-taken"] {
        let app = app_holding(FILE_ON, FILE_OFF);
        app.set_write_outcome(Outcome::Invalid {
            key,
            values: vec![ALPHA.to_string()],
        });
        let cookie = app.sign_in().await;
        let rep = app
            .save(
                ALPHA,
                &save_body(&alpha_revision(), json!({ "ctx-size": "8192" })),
                Some(&cookie),
            )
            .await;
        rep.assert_refused_without_field(key, &[ALPHA]);
        assert_eq!(app.file_text(MODELS).as_deref(), Some(FILE_ON));
        assert!(app.writes().len() == 1, "the write was asked: refused");
    }
}

/// The memory refusal (`model-profile-over-memory`, PLAN.md step 10 item 13,
/// slice 10b-2) names a model profile, an instance and two memory figures: none
/// of them is a key of the section, so the answer carries no `field` member at
/// all. The page shows the manager's sentence and marks no control; the file is
/// untouched and the session lives.
#[tokio::test]
async fn save_the_over_memory_refusal_carries_no_field() {
    let args = [ALPHA, "default", "1446", "512"];
    let app = app_holding(FILE_ON, FILE_OFF);
    app.set_write_outcome(Outcome::Invalid {
        key: "model-profile-over-memory",
        values: args.iter().map(|a| a.to_string()).collect(),
    });
    let cookie = app.sign_in().await;
    let rep = app
        .save(
            ALPHA,
            &save_body(&alpha_revision(), json!({ "ctx-size": "8192" })),
            Some(&cookie),
        )
        .await;
    rep.assert_refused_without_field("model-profile-over-memory", &args);
    assert_eq!(
        app.file_text(MODELS).as_deref(),
        Some(FILE_ON),
        "the manager wrote nothing"
    );
    app.get("/api/session", Some(&cookie))
        .await
        .assert_signed_in_as("alice");
}

// ---------------------------------------------------------------------------
// The manager refuses the signed-in user: 403, and that session ends
// ---------------------------------------------------------------------------

/// The member of `pai` removed after the sign-in is refused at the read: 403
/// `session-not-administrator`, and the session ends at the first refusal, so
/// the same cookie is 401 `session-required` afterwards. Nothing is written, and
/// the other administrator's session is untouched: the rule ends that session
/// alone.
#[tokio::test]
async fn save_a_refused_read_is_403_and_ends_that_session_alone() {
    let app = app_refusing(&["alice"], FILE_ON, FILE_OFF);
    let other = app.sign_in_as("root").await;
    let alice = app.sign_in_as("alice").await;
    let rep = app
        .save(
            ALPHA,
            &save_body(&alpha_revision(), json!({ "ctx-size": "8192" })),
            Some(&alice),
        )
        .await;
    rep.assert_error(StatusCode::FORBIDDEN, "session-not-administrator");
    assert_eq!(
        app.calls(),
        vec![format!("read {MODELS}")],
        "the read is refused, nothing else is asked"
    );
    assert_eq!(
        principals_of(&app.principals),
        vec![Some("alice".to_string())],
        "the save connects as the signed-in user"
    );
    assert!(app.no_write_asked(), "nothing written");
    assert_eq!(app.file_text(MODELS).as_deref(), Some(FILE_ON));
    app.get("/api/session", Some(&alice))
        .await
        .assert_error(StatusCode::UNAUTHORIZED, "session-required");
    app.get("/api/session", Some(&other))
        .await
        .assert_signed_in_as("root");
}

/// The same, when the removal lands between the read and the write: the write
/// was asked and refused, so 403 and the session ends. The file keeps the text
/// the save read.
#[tokio::test]
async fn save_a_refused_write_is_403_and_ends_the_session() {
    let app = app_holding(FILE_ON, FILE_OFF);
    app.set_write_refused("alice");
    let cookie = app.sign_in().await;
    let rep = app
        .save(
            ALPHA,
            &save_body(&alpha_revision(), json!({ "ctx-size": "8192" })),
            Some(&cookie),
        )
        .await;
    rep.assert_error(StatusCode::FORBIDDEN, "session-not-administrator");
    assert_eq!(
        app.calls(),
        vec![format!("read {MODELS}"), format!("write-if alice {MODELS}")],
        "the read landed, the write is refused"
    );
    assert_eq!(app.file_text(MODELS).as_deref(), Some(FILE_ON));
    app.get("/api/session", Some(&cookie))
        .await
        .assert_error(StatusCode::UNAUTHORIZED, "session-required");
}

/// A manager that refuses the connect itself answers the same 403 and ends the
/// same session, with no method called at all.
#[tokio::test]
async fn save_a_refused_connect_is_403_and_ends_the_session() {
    let app = app_holding(FILE_ON, FILE_OFF);
    app.set_connect_outcome(Outcome::NotAuthorized);
    let cookie = app.sign_in().await;
    let rep = app
        .save(
            ALPHA,
            &save_body(&alpha_revision(), json!({ "ctx-size": "8192" })),
            Some(&cookie),
        )
        .await;
    rep.assert_error(StatusCode::FORBIDDEN, "session-not-administrator");
    assert_eq!(app.calls(), Vec::<String>::new(), "no method was reached");
    assert_eq!(app.connects.load(Ordering::SeqCst), 1, "one connect");
    assert!(app.no_write_asked(), "nothing written");
    app.get("/api/session", Some(&cookie))
        .await
        .assert_error(StatusCode::UNAUTHORIZED, "session-required");
}

/// Only the manager's refusal ends a session. Every other failure of the same
/// call is the bff's news about its own connection, so the signed-in user keeps
/// their session and can ask again.
#[tokio::test]
async fn save_keeps_the_session_for_every_failure_that_is_no_refusal() {
    for (case, outcome) in [
        ("unavailable", Outcome::Unavailable),
        ("changed", Outcome::Changed),
        (
            "invalid",
            Outcome::Invalid {
                key: "not-whole-number-in-range",
                values: vec!["ctx-size".to_string(), "x".to_string()],
            },
        ),
        ("other", Outcome::Other),
    ] {
        let app = app_holding(FILE_ON, FILE_OFF);
        app.set_read_outcome(outcome);
        let cookie = app.sign_in().await;
        app.save(
            ALPHA,
            &save_body(&alpha_revision(), json!({ "ctx-size": "8192" })),
            Some(&cookie),
        )
        .await;
        app.get("/api/session", Some(&cookie))
            .await
            .assert_signed_in_as("alice");
        assert_eq!(app.file_text(MODELS).as_deref(), Some(FILE_ON), "{case}");
    }
}

// ---------------------------------------------------------------------------
// The manager unreachable, slow, or failing: 503, 504, 502
// ---------------------------------------------------------------------------

/// The rule the server file's save settled, shared here: while no write was
/// asked, a time limit or a dead stream is 503 `client-unavailable`, and the
/// task that was cut off never writes. The read is parked inside the manager
/// call, the save's limit passes, and the answer is 503; released afterwards,
/// the parked read finishes and the save stops there, with `models.ini` as it
/// was. A 503 is the bff's news, so the session lives.
#[tokio::test]
async fn save_a_parked_read_answers_503_and_the_cut_off_task_never_writes() {
    let app = app_holding(FILE_ON, FILE_OFF);
    app.arm_read_gate();
    let cookie = app.sign_in().await;
    let rep = app
        .save(
            ALPHA,
            &save_body(&alpha_revision(), json!({ "ctx-size": "8192" })),
            Some(&cookie),
        )
        .await;
    rep.assert_error(StatusCode::SERVICE_UNAVAILABLE, "client-unavailable");
    assert!(
        app.reads_gated() >= 1,
        "the read was parked in the manager call"
    );
    assert!(app.no_write_asked(), "no write was ever asked");
    app.release_read_gate();
    tokio::time::sleep(GRACE).await;
    assert!(app.reads_released() >= 1, "the parked read did finish");
    assert!(
        app.no_write_asked(),
        "a task whose answer was already sent never writes: {:?}",
        app.writes()
    );
    assert_eq!(
        app.file_text(MODELS).as_deref(),
        Some(FILE_ON),
        "the file is what it was"
    );
    app.get("/api/session", Some(&cookie))
        .await
        .assert_signed_in_as("alice");
}

/// A manager that answers the read `Unavailable`, and a manager that cannot be
/// reached at all: 503 `client-unavailable`, no write asked, the files untouched
/// and the session kept.
#[tokio::test]
async fn save_an_unreachable_manager_is_503_and_writes_nothing() {
    let app = app_holding(FILE_ON, FILE_OFF);
    app.set_read_outcome(Outcome::Unavailable);
    let cookie = app.sign_in().await;
    let rep = app
        .save(
            ALPHA,
            &save_body(&alpha_revision(), json!({ "ctx-size": "8192" })),
            Some(&cookie),
        )
        .await;
    rep.assert_error(StatusCode::SERVICE_UNAVAILABLE, "client-unavailable");
    app.get("/api/session", Some(&cookie))
        .await
        .assert_signed_in_as("alice");

    let gone = app_no_manager();
    let cookie = gone.sign_in().await;
    let rep = gone
        .save(
            ALPHA,
            &save_body(&alpha_revision(), json!({ "ctx-size": "8192" })),
            Some(&cookie),
        )
        .await;
    rep.assert_error(StatusCode::SERVICE_UNAVAILABLE, "client-unavailable");
    assert!(gone.no_write_asked(), "nothing written");
    assert_eq!(gone.file_text(MODELS).as_deref(), Some(FILE_ON));
}

/// The manager is reached for the write and answers `Unavailable` there: a
/// write was sent, so the file may hold the edit and the bff cannot know, which
/// is the 504 `client-timeout`, never 503. The server file's rule, carried to
/// the save as the switch already answers. The fake wrote nothing, so its file
/// is as it holds it, and the session lives: the bff's news about its own call
/// is no refusal of the user.
#[tokio::test]
async fn save_when_the_write_answers_unavailable_is_504() {
    let app = app_holding(FILE_ON, FILE_OFF);
    app.set_write_outcome(Outcome::Unavailable);
    let cookie = app.sign_in().await;
    let rep = app
        .save(
            ALPHA,
            &save_body(&alpha_revision(), json!({ "ctx-size": "8192" })),
            Some(&cookie),
        )
        .await;
    rep.assert_error(StatusCode::GATEWAY_TIMEOUT, "client-timeout");
    assert_eq!(
        app.calls(),
        vec![format!("read {MODELS}"), format!("write-if alice {MODELS}")],
        "the write was asked: the answer is about the write"
    );
    assert_eq!(
        app.file_text(MODELS).as_deref(),
        Some(FILE_ON),
        "the manager answered Unavailable and wrote nothing: the file is as it holds it"
    );
    app.get("/api/session", Some(&cookie))
        .await
        .assert_signed_in_as("alice");
}

/// Once the write was sent the answer flips to 504 `client-timeout`: the file
/// may or may not be saved, and the bff cannot know. The write is parked in the
/// manager call, the save's own limit passes, and the answer is 504; released,
/// the parked write lands, which is exactly the outcome the answer cannot
/// name. The session lives, and the fe's Reload shows the truth.
#[tokio::test]
async fn save_a_parked_write_answers_504_once_the_saves_limit_passes() {
    let app = app_holding_write_timeout(FILE_ON, FILE_OFF, Duration::from_millis(300));
    app.arm_write_gate();
    let cookie = app.sign_in().await;
    let rep = app
        .save(
            ALPHA,
            &save_body(&alpha_revision(), json!({ "ctx-size": "8192" })),
            Some(&cookie),
        )
        .await;
    rep.assert_error(StatusCode::GATEWAY_TIMEOUT, "client-timeout");
    assert!(app.writes_gated() >= 1, "the write was parked");
    assert_eq!(
        app.calls(),
        vec![format!("read {MODELS}"), format!("write-if alice {MODELS}")],
        "the write was asked: the answer is about the write"
    );
    app.release_write_gate();
    tokio::time::sleep(GRACE).await;
    assert_eq!(
        app.file_text(MODELS).as_deref(),
        Some(ON_CTX_8192),
        "the parked write landed after the answer: the outcome the 504 cannot name"
    );
    app.get("/api/session", Some(&cookie))
        .await
        .assert_signed_in_as("alice");
}

/// A write is a small file and at most one name lookup under the manager's
/// `writing` lock, so it runs under the save's own, longer limit and not under
/// the one that fits a read: a manager that takes its time writing is not cut
/// off, the save answers 200 and the file lands.
#[tokio::test]
async fn save_a_write_that_outlasts_the_read_limit_is_not_cut_off() {
    let app = app_holding_write_timeout(FILE_ON, FILE_OFF, Duration::from_millis(900));
    app.set_write_pause_ms(350);
    let cookie = app.sign_in().await;
    let rep = app
        .save(
            ALPHA,
            &save_body(&alpha_revision(), json!({ "ctx-size": "8192" })),
            Some(&cookie),
        )
        .await;
    rep.assert_saved(&rev(&KEY_A, MODELS, ON_CTX_8192, ALPHA), false, &[]);
    assert_eq!(
        app.file_text(MODELS).as_deref(),
        Some(ON_CTX_8192),
        "the slow write landed"
    );
    assert_eq!(app.writes().len(), 1, "one compare and swap, no retry");
}

/// The reads keep the short limit: a manager that answers a read later than
/// that is cut off at 503, before any write is asked.
#[tokio::test]
async fn save_a_read_that_outlasts_its_limit_is_cut_off_before_the_write() {
    let app = app_holding_write_timeout(FILE_ON, FILE_OFF, Duration::from_millis(900));
    app.set_read_pause_ms(350);
    let cookie = app.sign_in().await;
    let rep = app
        .save(
            ALPHA,
            &save_body(&alpha_revision(), json!({ "ctx-size": "8192" })),
            Some(&cookie),
        )
        .await;
    rep.assert_error(StatusCode::SERVICE_UNAVAILABLE, "client-unavailable");
    assert!(
        app.no_write_asked(),
        "no write was asked: {:?}",
        app.writes()
    );
    assert_eq!(app.file_text(MODELS).as_deref(), Some(FILE_ON));
}

/// A manager failure that is no refusal, no check and no lost compare is 502
/// `client-other` with empty args, at the read as at the write. The manager's
/// own words never travel to the browser: a detail of its failure is a path, a
/// unit name, a Windows message, and the page shows none of it.
#[tokio::test]
async fn save_the_managers_other_answer_is_502_without_its_words() {
    for (case, at) in [("read", "read"), ("write", "write")] {
        let app = app_holding(FILE_ON, FILE_OFF);
        match at {
            "read" => app.set_read_outcome(Outcome::Other),
            _ => app.set_write_outcome(Outcome::Other),
        }
        let cookie = app.sign_in().await;
        let rep = app
            .save(
                ALPHA,
                &save_body(&alpha_revision(), json!({ "ctx-size": "8192" })),
                Some(&cookie),
            )
            .await;
        rep.assert_error(StatusCode::BAD_GATEWAY, "client-other");
        assert!(
            !rep.body.contains("manager") && !rep.body.contains("failed"),
            "{case}: the manager's own words in the body: {}",
            rep.body
        );
        app.get("/api/session", Some(&cookie))
            .await
            .assert_signed_in_as("alice");
    }
}

// ---------------------------------------------------------------------------
// `restart` and `running`: the instances the saved profile is served on
// ---------------------------------------------------------------------------

/// The instances the profile is served on come from `assignments.ini`, not
/// from the instances the manager lists: a profile no instance is given (no
/// file, no section, a section that is `off`) is served nowhere, so the save
/// answers `restart: []` and `running: false`, and asks no state of an
/// instance that never loads it.
#[tokio::test]
async fn save_of_a_profile_no_instance_is_given_restarts_nothing() {
    let app = app_holding(FILE_ON, FILE_OFF);
    app.set_instances(&["one", "default"]);
    app.set_state("default", InstanceState::Active);
    let rep = app.save_on(ALPHA, json!({ "ctx-size": "8192" })).await;
    rep.assert_saved(&rev(&KEY_A, MODELS, ON_CTX_8192, ALPHA), false, &[]);
    assert_eq!(
        app.sorted_calls(),
        "read assignments.ini read models.ini write-if alice models.ini",
        "the records are read, no state is asked: the listed instances are not \
         the ones that serve the profile"
    );
}

/// Every instance the profile is effective on and up is named in `restart`,
/// sorted: the page offers to restart each of them. `activating` counts as up.
#[tokio::test]
async fn save_restart_names_every_up_instance_the_profile_is_effective_on() {
    let app = app_with_assignments("[one]\r\nalpha = on\r\n\r\n[default]\r\nalpha = on\r\n");
    app.set_instances(&["default", "one"]);
    app.set_state("one", InstanceState::Active);
    app.set_state("default", InstanceState::Activating);
    let rep = app.save_on(ALPHA, json!({ "ctx-size": "8192" })).await;
    rep.assert_saved(
        &rev(&KEY_A, MODELS, ON_CTX_8192, ALPHA),
        true,
        &["default", "one"],
    );
}

/// An instance where the section says `off` is left out even when it is up;
/// `inherit` follows the profile's own state in the pool, so an `on` profile
/// is effective there and lands in `restart`; an instance that is down is left
/// out. The order of `restart` is sorted, not the order the records spell.
#[tokio::test]
async fn save_restart_leaves_out_the_off_instances_and_the_down_ones() {
    let app = app_with_assignments(
        "[one]\r\nalpha = on\r\n\r\n[default]\r\nalpha = off\r\n\r\n[laptop]\r\nalpha = inherit\r\n",
    );
    app.set_instances(&["default", "laptop", "one"]);
    app.set_state("one", InstanceState::Active);
    app.set_state("default", InstanceState::Active);
    app.set_state("laptop", InstanceState::Active);
    let rep = app.save_on(ALPHA, json!({ "ctx-size": "8192" })).await;
    rep.assert_saved(
        &rev(&KEY_A, MODELS, ON_CTX_8192, ALPHA),
        true,
        &["laptop", "one"],
    );
}

/// The blind spot the records close: a switched-off profile an `on` override
/// gives to one instance IS served there, so the save of that section names
/// that instance, and not the instance that runs the pool's own state.
#[tokio::test]
async fn save_of_a_switched_off_profile_named_by_an_on_override_names_its_instance() {
    let app = app_with_assignments(
        "[coder]\r\nmistral-7b = on\r\n\r\n[default]\r\nmistral-7b = inherit\r\n",
    );
    app.set_instances(&["coder", "default"]);
    app.set_state("coder", InstanceState::Active);
    app.set_state("default", InstanceState::Active);
    let revision = rev(&KEY_A, DISABLED, FILE_OFF, MISTRAL);
    let rep = app
        .save_on_revision(MISTRAL, &revision, json!({ "temp": "1.0" }))
        .await;
    rep.assert_saved(
        &rev(&KEY_A, DISABLED, OFF_TEMP_1, MISTRAL),
        true,
        &["coder"],
    );
}

/// A state read that fails after a write that landed is still a 200, with
/// `running: true` and `restart: []`: the save is done, and the safe hint
/// costs at most a needless restart. A 200 that said `running: false` would
/// tell the page to say nothing while the instance it edited is serving.
#[tokio::test]
async fn save_running_is_true_when_the_state_read_fails_after_the_write() {
    for (case, outcome) in [
        ("unavailable", Outcome::Unavailable),
        ("other", Outcome::Other),
        ("refused", Outcome::NotAuthorized),
    ] {
        let app = app_with_assignments("[default]\r\nalpha = on\r\n");
        app.set_state("default", InstanceState::Active);
        app.set_status_outcome(outcome);
        let rep = app.save_on(ALPHA, json!({ "ctx-size": "8192" })).await;
        rep.assert_saved(&rev(&KEY_A, MODELS, ON_CTX_8192, ALPHA), true, &[]);
        assert_eq!(
            app.file_text(MODELS).as_deref(),
            Some(ON_CTX_8192),
            "{case}: the write landed"
        );
    }
}

/// A body that changes nothing has no write to wait for, so a state read that
/// outlives the save's limit cannot make the save fail: nothing needed writing
/// and nothing was written, so the answer is the 200 of a landed save, the
/// revision the file holds and `running: true` with `restart: []` (the safe
/// answer for a state the bff could not read, as for a state read that fails
/// after a write). 503 `client-unavailable` tells the page the save did not
/// happen, when it did: the page shows the edit as lost and offers a retry of
/// an edit that is saved.
///
/// `assignments.ini` gives `alpha` to `default` and `default` runs, so the save
/// has a reason to ask that state: the read that outlives the limit is asked,
/// and it is the one that times out.
#[tokio::test]
async fn save_no_changes_with_a_state_read_outliving_the_limit_is_200_running_true() {
    let app =
        app_with_assignments_timeout("[default]\r\nalpha = on\r\n", Duration::from_millis(300));
    // The instance the model profile is effective on runs, so the state read
    // this test stalls is asked: `running: true` is pinned as the safe answer
    // for a state that stayed unknown, not as one the read carried.
    app.set_state("default", InstanceState::Active);
    let fresh = alpha_revision();
    app.set_status_pause_ms(350);
    let cookie = app.sign_in().await;
    let rep = app
        .save(ALPHA, &save_body(&fresh, json!({})), Some(&cookie))
        .await;
    rep.assert_saved(&fresh, true, &[]);
    assert!(
        app.calls().iter().any(|call| call == "status default"),
        "the state of the instance the model profile is effective on is asked: {:?}",
        app.calls()
    );
    assert!(
        app.no_write_asked(),
        "a body that changes nothing writes nothing: {:?}",
        app.writes()
    );
    assert_eq!(
        app.file_text(MODELS).as_deref(),
        Some(FILE_ON),
        "the file is as it was read"
    );
    app.get("/api/session", Some(&cookie))
        .await
        .assert_signed_in_as("alice");
}

/// No `assignments.ini` at all: nothing on the machine says which instance is
/// given the model profile, so a save that changed nothing names no instance to
/// restart and asks no instance's state, running an idle manager or a busy one.
/// `restart: []` and `running: false`, and the save's limit is never spent on a
/// state of an instance the record never named (ARCHITECTURE.md's slice 15
/// as-built: no record means no instance is given the model profile).
#[tokio::test]
async fn save_no_changes_with_no_record_restarts_nothing_and_asks_no_state() {
    let app = app_holding(FILE_ON, FILE_OFF);
    // The instance is up: the point is that the save never asks, not that the
    // answer it would carry is false.
    app.set_state("default", InstanceState::Active);
    let fresh = alpha_revision();
    let cookie = app.sign_in().await;
    let rep = app
        .save(ALPHA, &save_body(&fresh, json!({})), Some(&cookie))
        .await;
    rep.assert_saved(&fresh, false, &[]);
    assert!(
        app.no_write_asked(),
        "a body that changes nothing writes nothing: {:?}",
        app.writes()
    );
    for verb in ["status", "list"] {
        assert!(
            !app.calls().iter().any(|call| call.starts_with(verb)),
            "no record, so no {verb} is asked: {:?}",
            app.calls()
        );
    }
}
// ---------------------------------------------------------------------------
// The save and the session's idle deadline
// ---------------------------------------------------------------------------

/// The save moves the session's idle deadline, as every signed-in route does.
/// The clock here is the test's: the save lands 29 minutes into a session that
/// lasts 30, and the session is read 29 minutes after that. Signed in 58 minutes
/// ago, read 29 minutes after the save: a save that left the deadline where it
/// was would answer 401.
#[tokio::test]
async fn save_moves_the_sessions_idle_deadline() {
    const MINUTE: u64 = 60_000;
    let app = app_holding(FILE_ON, FILE_OFF);
    let cookie = app.sign_in().await;
    app.advance(29 * MINUTE);
    let rep = app
        .save(
            ALPHA,
            &save_body(&alpha_revision(), json!({})),
            Some(&cookie),
        )
        .await;
    rep.assert_saved(&alpha_revision(), false, &[]);
    app.advance(29 * MINUTE);
    app.get("/api/session", Some(&cookie))
        .await
        .assert_signed_in_as("alice");
}

// ── The GPU distribution card's three keys (G1, ARCHITECTURE.md 1) ───────
//
// `device`, `split-mode` and `tensor-split` moved from
// `checks::MODEL_PROFILE_KEYS_NOT_ON_WEB` to `WEB_MODEL_PROFILE_KEYS`, so the
// page's body carries them and the save treats them as every other key: the
// body check lets them through, the text lands in the section, and the
// manager's refusal of a value answers 422 with the key it named and the
// control to mark. The save path itself is unchanged: one read, one
// compare-and-swap write, the same 200.

/// `FILE_ON` with the GPU distribution card saved on `[alpha]`: the three keys
/// the section did not hold, appended after its last real line in key order
/// (`device`, `split-mode`, `tensor-split`), each as the body gave it, and
/// nothing else moved.
const ON_GPU: &str = "\
; The presets llama-server is offered (--models-preset).\r\n\
[alpha]\r\n\
model = C:\\Models\\alpha.gguf\r\n\
ctx-size = 4096 ; the context the page shows\r\n\
n-gpu-layers = 99\r\n\
device = ROCm0,CUDA0\r\n\
split-mode = layer\r\n\
tensor-split = 3,1\r\n\
\r\n\
[Qwen]\r\n\
model = /models/Qwen3.8-27B-Q8_0.gguf\r\n\
load-on-startup = 1\r\n\
temp = 0.7\r\n\
";

/// The three keys are on the web, so a body naming them is saved: no 400 for a
/// key off the web list, the text lands as any other key's does, asked as the
/// signed-in user with one connect for the exchange.
#[tokio::test]
async fn save_the_gpu_distribution_keys_lands_in_the_section() {
    let app = app_holding(FILE_ON, FILE_OFF);
    app.save_on(
        ALPHA,
        json!({
            "device": "ROCm0,CUDA0",
            "split-mode": "layer",
            "tensor-split": "3,1",
        }),
    )
    .await
    .assert_saved(&rev(&KEY_A, MODELS, ON_GPU, ALPHA), false, &[]);
    assert_eq!(
        app.file_text(MODELS).as_deref(),
        Some(ON_GPU),
        "the three lines land inside [alpha], in key order"
    );
    assert_eq!(
        app.sorted_calls(),
        "read assignments.ini read models.ini write-if alice models.ini",
        "one read and one compare-and-swap write, then the records of where the \
         profile is served, as every save in this file"
    );
    assert_eq!(
        principals_of(&app.principals),
        vec![Some("alice".to_string())],
        "the read and the write both as the signed-in user, on one connect"
    );
    assert_eq!(
        section_of(&app.file_text(MODELS).expect("models.ini"), QWEN),
        section_of(FILE_ON, QWEN),
        "the other section is byte for byte as it was"
    );
}

/// `device`, `device-draft` and `mmproj-device` share the manager's one rule
/// (`device-bad-name`, whose args are the key and the entry it refused) and
/// `tensor-split` has its own (`tensor-split-format`, whose only arg is the
/// whole value, so the bff marks the control). The section is never written and
/// the session lives, as every 422 about values does. `moe-cache-mib` is not on
/// the web, so the page cannot send it: the last case starts from a section
/// that ALREADY holds `moe-cache-mib = 4096` (`FILE_ON_MOE`) and writes only
/// `split-mode = tensor`, and the manager's `moe-cache-with-tensor-split` comes
/// back marking `moe-cache-mib`, the key of the pair the page cannot touch.
#[tokio::test]
async fn save_a_gpu_distribution_value_the_manager_refuses_marks_its_control() {
    struct Case {
        key: &'static str,
        args: &'static [&'static str],
        /// The `models.ini` the manager holds before the save.
        file: &'static str,
        body: Value,
        field: &'static str,
    }
    let cases = [
        Case {
            key: "device-bad-name",
            args: &["device", " ROCm1"],
            file: FILE_ON,
            body: json!({ "device": "ROCm0, ROCm1" }),
            field: "device",
        },
        Case {
            key: "device-bad-name",
            args: &["device", "CPU"],
            file: FILE_ON,
            body: json!({ "device": "CPU" }),
            field: "device",
        },
        Case {
            key: "device-bad-name",
            args: &["device-draft", "a b"],
            file: FILE_ON,
            body: json!({ "device-draft": "a b" }),
            field: "device-draft",
        },
        Case {
            key: "device-bad-name",
            args: &["mmproj-device", " ROCm1"],
            file: FILE_ON,
            body: json!({ "mmproj-device": "ROCm0, ROCm1" }),
            field: "mmproj-device",
        },
        Case {
            key: "tensor-split-format",
            args: &["3,x"],
            file: FILE_ON,
            body: json!({ "tensor-split": "3,x" }),
            field: "tensor-split",
        },
        Case {
            key: "tensor-split-format",
            args: &["-1,1"],
            file: FILE_ON,
            body: json!({ "tensor-split": "-1,1" }),
            field: "tensor-split",
        },
        Case {
            key: "moe-cache-with-tensor-split",
            args: &[],
            file: FILE_ON_MOE,
            body: json!({ "split-mode": "tensor" }),
            field: "moe-cache-mib",
        },
    ];
    for case in cases {
        let app = app_holding(case.file, FILE_OFF);
        app.set_write_outcome(Outcome::Invalid {
            key: case.key,
            values: case.args.iter().map(|a| a.to_string()).collect(),
        });
        let cookie = app.sign_in().await;
        let rep = app
            .save(
                ALPHA,
                &save_body(&rev(&KEY_A, MODELS, case.file, ALPHA), case.body.clone()),
                Some(&cookie),
            )
            .await;
        rep.assert_refused(case.key, case.args, case.field);
        let writes = app.writes();
        assert_eq!(
            writes.len(),
            1,
            "{}: the refusal is the write's, so the edited text reached the manager",
            case.key
        );
        assert_eq!(writes[0].0, MODELS, "{}: the file", case.key);
        for line in case.body.as_object().expect("an object").keys() {
            assert!(
                writes[0].1.contains(&format!("{line} = ")),
                "{}: the text judged carries the edited key {line}: {:?}",
                case.key,
                writes[0].1
            );
        }
        assert_eq!(
            app.file_text(MODELS).as_deref(),
            Some(case.file),
            "{}: the manager wrote nothing",
            case.key
        );
        app.get("/api/session", Some(&cookie))
            .await
            .assert_signed_in_as("alice");
    }
}

// ── `n-cpu-moe` on the web (L3, ARCHITECTURE.md "Layers, MoE and tensor
// placement on the web model profile page", slice 3) ──────────────────────
//
// The key moved from `checks::MODEL_PROFILE_KEYS_NOT_ON_WEB` to
// `WEB_MODEL_PROFILE_KEYS` with the range rule `("n-cpu-moe", 0..=4096)` in
// `MODEL_PROFILE_INT_KEYS`, so the page's body carries it and the save treats
// it as every other key: the body check lets it through, `24` lands in the
// section, and the manager's refusal of a value out of the range answers 422
// with the range key naming `n-cpu-moe` and writes nothing.

/// `FILE_ON` with `n-cpu-moe = 24` saved on `[alpha]`: the key was not in the
/// section, so it lands after its last real line, and nothing else moved.
const ON_NCPU: &str = "\
; The presets llama-server is offered (--models-preset).\r\n\
[alpha]\r\n\
model = C:\\Models\\alpha.gguf\r\n\
ctx-size = 4096 ; the context the page shows\r\n\
n-gpu-layers = 99\r\n\
n-cpu-moe = 24\r\n\
\r\n\
[Qwen]\r\n\
model = /models/Qwen3.8-27B-Q8_0.gguf\r\n\
load-on-startup = 1\r\n\
temp = 0.7\r\n\
";

/// `n-cpu-moe` is on the web, so a body carrying `24` is saved: no 400 for a
/// key off the web list, and the line lands inside `[alpha]` as any other
/// key's does.
#[tokio::test]
async fn save_n_cpu_moe_lands_in_the_section() {
    let app = app_holding(FILE_ON, FILE_OFF);
    app.save_on(ALPHA, json!({ "n-cpu-moe": "24" }))
        .await
        .assert_saved(&rev(&KEY_A, MODELS, ON_NCPU, ALPHA), false, &[]);
    assert_eq!(
        app.file_text(MODELS).as_deref(),
        Some(ON_NCPU),
        "the line lands inside [alpha]"
    );
    assert_eq!(
        section_of(&app.file_text(MODELS).expect("models.ini"), QWEN),
        section_of(FILE_ON, QWEN),
        "the other section is byte for byte as it was"
    );
}

/// Below 0 never loads and 4096 is `shape_of`'s block cap, so the manager
/// refuses `-1` and `4097` with `not-whole-number-in-range`, whose args name
/// the key: the 422 marks the `n-cpu-moe` control and the section is never
/// written, as every 422 about values.
#[tokio::test]
async fn save_n_cpu_moe_out_of_range_is_422_naming_the_key() {
    for value in ["-1", "4097"] {
        let app = app_holding(FILE_ON, FILE_OFF);
        app.set_write_outcome(Outcome::Invalid {
            key: "not-whole-number-in-range",
            values: ["n-cpu-moe", value, "0", "4096"]
                .iter()
                .map(|a| a.to_string())
                .collect(),
        });
        let cookie = app.sign_in().await;
        let rep = app
            .save(
                ALPHA,
                &save_body(&alpha_revision(), json!({ "n-cpu-moe": value })),
                Some(&cookie),
            )
            .await;
        rep.assert_refused(
            "not-whole-number-in-range",
            &["n-cpu-moe", value, "0", "4096"],
            "n-cpu-moe",
        );
        assert_eq!(
            app.file_text(MODELS).as_deref(),
            Some(FILE_ON),
            "{value}: the refusal wrote nothing"
        );
    }
}

/// The device list a profile names is never judged against the node's probe: a
/// well-formed id the probe does not know saves, and the page flags it. So the
/// save of `device = ROCm7` on a node answering an empty list is a 200, the
/// same as any accepted value.
#[tokio::test]
async fn save_a_device_the_probe_does_not_know_is_saved() {
    let app = app_holding(FILE_ON, FILE_OFF);
    app.save_on(ALPHA, json!({ "device": "ROCm7" }))
        .await
        .assert_saved(&rev(&KEY_A, MODELS, ON_DEVICE_ROCM7, ALPHA), false, &[]);
    assert!(
        app.file_text(MODELS)
            .expect("models.ini")
            .contains("device = ROCm7\r\n"),
        "the id the probe lacks is written as the body gave it"
    );
}

/// `FILE_ON` with `device = ROCm7` appended to `[alpha]`.
const ON_DEVICE_ROCM7: &str = "\
; The presets llama-server is offered (--models-preset).\r\n\
[alpha]\r\n\
model = C:\\Models\\alpha.gguf\r\n\
ctx-size = 4096 ; the context the page shows\r\n\
n-gpu-layers = 99\r\n\
device = ROCm7\r\n\
\r\n\
[Qwen]\r\n\
model = /models/Qwen3.8-27B-Q8_0.gguf\r\n\
load-on-startup = 1\r\n\
temp = 0.7\r\n\
";

/// A section of `assignments.ini` naming an instance the manager no longer
/// lists is a stale record, not a machine: the save of a profile effective only
/// there names nothing to restart, `running` is false, and no state is asked of
/// an instance that does not exist.
#[tokio::test]
async fn save_restart_leaves_out_a_record_section_naming_no_instance() {
    let app = app_with_assignments("[default]\r\nalpha = off\r\n\r\n[ghost]\r\nalpha = on\r\n");
    app.set_instances(&["default"]);
    app.set_state("default", InstanceState::Active);
    app.set_state("ghost", InstanceState::Active);
    let rep = app.save_on(ALPHA, json!({ "ctx-size": "8192" })).await;
    rep.assert_saved(&rev(&KEY_A, MODELS, ON_CTX_8192, ALPHA), false, &[]);
    assert!(
        !app.calls().iter().any(|call| call == "status ghost"),
        "a state was asked of an instance the manager does not list: {:?}",
        app.calls()
    );
}
