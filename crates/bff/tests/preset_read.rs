//! `GET /api/profiles/model/{id}`: one model profile section of the engine's `models.ini`
//! / `models.ini.disabled`, read through the manager as the signed-in user,
//! through `pai_bff::router_with_session` with a fake manager client and a
//! fake authenticator (never the real bus, pipe, PAM or LogonUser). Each
//! request runs under a deadline, so a bff that waits on the manager fails
//! instead of hanging.
//!
//! The answer (`fe/briefs/api-contract.md`, `GET /api/profiles/model/{id}`, and the
//! section revision of ARCHITECTURE.md "Saving a model profile from the web"): 200
//! and
//! `{"id", "engine", "engineAvailable", "enabled", "values", "projectUrl",
//! "revision"}`.
//! - `id` as the file spells it: the id in the path is matched without case.
//! - `engine` is the engine whose model profile file holds the section (llama.cpp,
//!   the only engine) and `engineAvailable` is true for it: the same two
//!   fields as the list's row (`tests/presets.rs`), so the page shows one
//!   engine cell in both.
//! - `enabled`: true for a section of `models.ini`, false for one that is
//!   only in `models.ini.disabled`. A section in both files is the enabled
//!   one, as the list answers it.
//! - `values`: the section's keys as its file holds them, as strings (the
//!   value trimmed, everything from a `;` or `#` cut, the file's separators
//!   kept, a repeated key read as the last line, the key's own spelling kept);
//!   a key the section does not hold is absent, never null.
//! - `projectUrl`: the section's `; url = ` comment, and empty when the
//!   section has none. It is the configurator's reader, which lives in
//!   `pai-core` `ini` so the web and the GUI read one URL the same way
//!   (`crates/core/src/ini.rs`, the `project_url_of` tests).
//! - `revision`: the section's identity, the `If-Match` pair of the save (the
//!   `PUT` on this path): 16 lowercase hex digits, SipHash-2-4 under the
//!   router's key over `<file name> LF <the section's lines, from its header
//!   through its last non-blank line, as the file holds them, without that
//!   last line's own ending>`: the section's own text, the endings between its
//!   lines kept, its trailing blank lines and the ending of its last line cut.
//!   So a final line break added to the file's last section, and a section
//!   appended after it, leave its revision where it was. The same function as
//!   the server read's revision
//!   (`tests/server.rs`), over a section instead of a whole file, keyed on
//!   the file the section sits in. It does not move when another section of
//!   the file changes, and it is another for another file and for another key.
//! - llama-server's global `[*]` section is not a model profile, and an id that is
//!   no section is nothing to read: 404 `no-model-profile-named`, the id echoed as
//!   the only arg when it is a name a model profile can carry at all, as the list's
//!   404 answers it.
//! - A path with a segment below the section, `/api/profiles/model/alpha/*` (a raw
//!   `*` or its percent form), is not this read: it is no path of this API, so
//!   the router answers its own 404, with no body, and no manager is asked.
//! - The bff's own error answers, as its served siblings (`tests/server.rs`,
//!   `tests/presets.rs`), `{"key", "args"}` and never the manager's detail:
//!   401 `session-required` with the manager not connected to, 403
//!   `session-not-administrator` when the manager refuses the read for the
//!   signed-in user (which ends that session), 503 `client-unavailable` when
//!   the manager is not there, went away, or is too slow, 502 `client-other`
//!   for anything else, which keeps the session.
//! - One connect per request, asked for the signed-in user, and the two
//!   model profile files are read over it: no instance list, no status, no write. A
//!   name in the request never becomes the principal.
//! - `POST` and `DELETE` on the section path are not served.

use axum::body::Body;
use axum::extract::ConnectInfo;
use axum::http::{header, Method, Request, StatusCode};
use axum::Router;
use http_body_util::BodyExt;
use pai_bff::session::{AuthError, Authenticator, Identity};
use pai_bff::{Client, Connect};
use pai_core::client::{ClientError, InstanceState, ManagerClient, ManagerFile};
use serde_json::{json, Value};
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

/// The signed-in user of these tests.
const USER: &str = "alice";

/// The revision secret of the router under test, and another one: the
/// revision is keyed, so a router built with other bytes answers another one.
const KEY_A: [u8; 16] = [0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15];
const KEY_B: [u8; 16] = [
    16, 17, 18, 19, 20, 21, 22, 23, 24, 25, 26, 27, 28, 29, 30, 31,
];

/// The names the manager knows the engine's model profile files by. The revision is
/// keyed on the name, so these strings are part of the answer: the read sends
/// exactly these names to the manager.
const ON: &str = "models.ini";
const OFF: &str = "models.ini.disabled";

/// The section the tests read, as a file holds it: CRLF file, Windows
/// separators, a key spelled with an underscore, and a spelling order that is
/// not sorted, so a bff that re-renders the section hashes a text the file does
/// not hold. Its last line ends as the file's lines do: the revision covers the
/// section's lines from its header to its last non-blank line, blank lines
/// after them cut, that last line's own ending NOT hashed.
const ALPHA: &str = "[alpha]\r\nmodel = C:\\Models\\alpha.gguf\r\nn-gpu-layers = 99\r\n";

/// A `models.ini` holding `ALPHA` with a blank line and another section after
/// it, so the file the revision is read from holds a section the read does not
/// touch, and the blank line is cut from the section's text.
const FILE_ALPHA: &str =
    "[alpha]\r\nmodel = C:\\Models\\alpha.gguf\r\nn-gpu-layers = 99\r\n\r\n[Bigger]\r\nmodel = /models/b.gguf\r\n";

/// `ALPHA` with one key added: the section text changed, so its revision is
/// the pinned one below.
const ALPHA_WITH_CTX: &str =
    "[alpha]\r\nmodel = C:\\Models\\alpha.gguf\r\nn-gpu-layers = 99\r\nctx-size = 8192\r\n";

/// A section appended below `FILE_ALPHA`: the file grows, `ALPHA`'s text and
/// revision do not move, and this section has a revision of its own, over its
/// own text.
const ZETA: &str = "[Zeta]\r\nmodel = /models/z.gguf\r\nctx-size = 8192\r\n";

/// The same section in a CRLF file that ends WITHOUT a line break, as a hand
/// edit leaves it: `ALPHA`'s text minus the last line's own `\r\n`.
const ALPHA_NO_BREAK_CRLF: &str = "[alpha]\r\nmodel = C:\\Models\\alpha.gguf\r\nn-gpu-layers = 99";

/// The file `ALPHA_NO_BREAK_CRLF` with the final line break added: one byte
/// more at the end of the file, the same section.
const ALPHA_BREAK_CRLF: &str = "[alpha]\r\nmodel = C:\\Models\\alpha.gguf\r\nn-gpu-layers = 99\r\n";

/// The file `ALPHA_NO_BREAK_CRLF` with `ZETA` appended below it, verbatim:
/// `ALPHA`'s section text ends where its last key's line ends, and the section
/// below it is `ZETA`, so the read of it answers `REV_ZETA`.
const ALPHA_PLUS_ZETA_CRLF: &str = "[alpha]\r\nmodel = C:\\Models\\alpha.gguf\r\nn-gpu-layers = 99\r\n\r\n[Zeta]\r\nmodel = /models/z.gguf\r\nctx-size = 8192\r\n";

/// The same three files in LF, and the third shape of the rule: the section
/// with one key's value changed, still with no final line break.
const ALPHA_NO_BREAK_LF: &str = "[alpha]\nmodel = /models/a.gguf\nn-gpu-layers = 99";
const ALPHA_BREAK_LF: &str = "[alpha]\nmodel = /models/a.gguf\nn-gpu-layers = 99\n";
const ALPHA_PLUS_ZETA_LF: &str =
    "[alpha]\nmodel = /models/a.gguf\nn-gpu-layers = 99\n\n[Zeta]\nmodel = /models/z.gguf\n";
const ALPHA_VALUE_LF: &str = "[alpha]\nmodel = /models/a.gguf\nn-gpu-layers = 100";

/// The pinned revisions: SipHash-2-4 under the router's key over `<file
/// name> LF <the section's lines from its header through its last non-blank
/// line, without that line's own ending>`, the endings between the lines as the
/// file holds them. Computed outside the repo over the fixtures above, from the
/// published vectors `tests/server.rs` pins (the empty text, the shipped server
/// file, the file with a key, and the second key); the same code is this file's
/// `siphash_2_4`, which is pinned to those vectors by
/// `the_helper_is_the_published_siphash_2_4` and is run against the same input
/// wherever a revision is checked.
const REV_ALPHA_ON: &str = "09ee97b65e5489ad";
const REV_ALPHA_OFF: &str = "4cfe56c43964900e";
const REV_ALPHA_KEY_B: &str = "6d45850887c27fd2";
const REV_ALPHA_WITH_CTX: &str = "b22fe753f8fe10ec";

/// The digest of `ALPHA_NO_BREAK_CRLF` in `models.ini`: the section's text
/// with no final break, the bytes the rule hashes for it. It is `REV_ALPHA_ON`,
/// the same section with a final break: the last line's own ending is not in
/// the digest.
const REV_ALPHA_NO_BREAK: &str = "09ee97b65e5489ad";

/// The digest of `ALPHA_NO_BREAK_LF` in `models.ini`, and of
/// `ALPHA_VALUE_LF`: the same rule in an LF file, and the section with
/// `n-gpu-layers` changed to 100.
const REV_ALPHA_NO_BREAK_LF: &str = "c0e3488c6b9fcaa1";
const REV_ALPHA_VALUE_LF: &str = "77a8e38aaab5cd07";

/// The same hash over the whole file, and over the `[Bigger]` section that
/// sits below `ALPHA` in `FILE_ALPHA`: a revision that is one of those is
/// caught. The whole file is not a section, so its text is hashed as it is.
const REV_WHOLE_FILE: &str = "f61f68ec4c2ec783";
const REV_NEIGHBOUR: &str = "c6cf323403eefba2";

/// The same hash over `ZETA` in `models.ini`: the section both appended-file
/// tests read, so their expected value is its own and not the one of a section
/// of the file that does not hold it.
const REV_ZETA: &str = "aa296521d7feb313";

/// SipHash-2-4 of `s` under `key`, the reference implementation's own
/// init (`v0 ^= k0`, `v1 ^= k1`, `v2 ^= k0`, `v3 ^= k1` over the four
/// constants of the algorithm). The revision is keyed, so the expected value
/// is computed here and never read from the code under test.
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
/// are the file's. A final line break added to the section, and a section
/// appended after it, leave this text as it was.
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

/// The helper is the published algorithm, not a lookalike: the reference
/// implementation's own vectors for the secret `00 01 .. 0f` (Appendix A of
/// the SipHash paper, the list `tests/server.rs` pins the empty text of), for
/// the empty input, one byte, a whole block and a block with a tail. Every
/// pinned revision of this file is therefore a vector of SipHash-2-4, never a
/// value taken from the code under test.
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

/// The 64-bit FNV-1a of `text` in hex, as `tests/server.rs` computes it: the
/// same 16 hex digits, so only the value separates it from a revision.
fn fnv1a(text: &str) -> String {
    let hash = text.bytes().fold(0xcbf2_9ce4_8422_2325_u64, |h, b| {
        (h ^ u64::from(b)).wrapping_mul(0x0000_0100_0000_01b3)
    });
    format!("{hash:016x}")
}

/// The document of `ALPHA`: its id, the section's two keys as strings, no URL
/// comment, `enabled` from the file it was read from, and the revision
/// computed here.
fn alpha_document(file: &str, enabled: bool) -> Value {
    json!({
        "id": "alpha",
        "engine": "llama.cpp",
        "engineAvailable": true,
        "enabled": enabled,
        "values": { "model": "C:\\Models\\alpha.gguf", "n-gpu-layers": "99" },
        "projectUrl": "",
        "revision": revision_of(&KEY_A, file, ALPHA),
    })
}

// ── Fakes ───────────────────────────────────────────────────────────────

/// The failure a test makes the manager answer every read with: the ones a
/// model profile read can meet.
#[derive(Clone)]
enum Failure {
    /// No manager: not running, no bus, no pipe, or gone in the middle.
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

/// The model profile files the manager holds, by name: a name that is not there is a
/// file that does not exist, and `read_file` answers `None` for it.
type Files = Arc<Mutex<Vec<(String, String)>>>;

/// What every connect of one app shares: the files, the read's failure and
/// how long a read takes. Shared cells, so a test can change them after it
/// signed in.
#[derive(Clone, Default)]
struct Shared {
    files: Files,
    read_failure: Arc<Mutex<Option<Failure>>>,
    read_pause_ms: Arc<AtomicU64>,
}

/// A manager that answers `read_file` from the files it holds, logs every
/// call (`read <name>`, ...), and refuses the read for the principals in
/// `refuse`, the way the real manager refuses a user who is no longer a
/// member of `pai`. It is the client of one connect, so it knows the
/// principal that connect was handed (`as_user`).
struct FakeManager {
    shared: Shared,
    log: Arc<Mutex<Vec<String>>>,
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
        Ok(self
            .shared
            .files
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .iter()
            .find(|(held, _)| held == name)
            .map(|(_, text)| ManagerFile {
                text: text.clone(),
                withheld: false,
            }))
    }
    fn list_instances(&self) -> Result<Vec<String>, ClientError> {
        self.record("list".into());
        Ok(vec!["default".into()])
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
        contents: &str,
        _keep_api_key: bool,
    ) -> Result<(), ClientError> {
        self.record(format!("write {name} {}", contents.len()));
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
/// missing file), with the manager's call log, the number of connects, the
/// principal of each, and the revision secret `key`.
struct App {
    router: Router,
    log: Arc<Mutex<Vec<String>>>,
    connects: Arc<AtomicUsize>,
    principals: Arc<Mutex<Vec<Option<String>>>>,
    shared: Shared,
}

/// The app: a manager holding `files`, refusing the reads of `refuse`, and
/// answering every connect with `connect_result`.
fn app_for(
    files: &[(&str, &str)],
    refuse: &[&str],
    connect_result: Result<(), ClientError>,
    key: [u8; 16],
) -> App {
    let shared = Shared::default();
    {
        let mut held = shared.files.lock().unwrap();
        for (name, text) in files {
            held.push((name.to_string(), text.to_string()));
        }
    }
    let log = Arc::new(Mutex::new(Vec::new()));
    let connects = Arc::new(AtomicUsize::new(0));
    let principals = Arc::new(Mutex::new(Vec::new()));
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
        key,
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
    app_for(&[(ON, on), (OFF, off)], &[], Ok(()), KEY_A)
}

/// The same, under the other revision secret.
fn app_holding_key_b(on: &str, off: &str) -> App {
    app_for(&[(ON, on), (OFF, off)], &[], Ok(()), KEY_B)
}

/// A manager holding `on`, refusing the reads of the named principals: the
/// member of `pai` removed after the sign-in.
fn app_refusing(refuse: &[&str], on: &str, off: &str) -> App {
    app_for(&[(ON, on), (OFF, off)], refuse, Ok(()), KEY_A)
}

/// The manager of no model profile file at all: neither file exists.
fn app_no_files() -> App {
    app_for(&[], &[], Ok(()), KEY_A)
}

/// The app that cannot reach the manager at all.
fn app_no_manager() -> App {
    app_for(
        &[(ON, FILE_ALPHA)],
        &[],
        Err(ClientError::Unavailable),
        KEY_A,
    )
}

impl App {
    /// The manager's calls, in order.
    fn calls(&self) -> Vec<String> {
        self.log.lock().unwrap_or_else(|e| e.into_inner()).clone()
    }

    /// The principal of every connect, in order: `None` is a call the bff
    /// makes as its own account.
    fn principals(&self) -> Vec<Option<String>> {
        self.principals
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .clone()
    }

    /// Answer every coming read with `failure`.
    fn set_read_failure(&self, failure: Failure) {
        *self.shared.read_failure.lock().unwrap() = Some(failure);
    }

    /// A read that takes `ms` milliseconds: the manager busy while the
    /// section is being read.
    fn set_read_pause_ms(&self, ms: u64) {
        self.shared.read_pause_ms.store(ms, Ordering::SeqCst);
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

    /// `GET uri` with `cookie` when given, and extra headers (a request that
    /// tries to name the user it wants the bff to act for).
    async fn get(&self, uri: &str, cookie: Option<&str>, extra: &[(&str, &str)]) -> Reply {
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

    /// Signed in as `alice`, `GET /api/profiles/model/{id}` for the id `id`.
    async fn read(&self, id: &str) -> Reply {
        let cookie = self.sign_in_as(USER).await;
        self.get(&format!("/api/profiles/model/{id}"), Some(&cookie), &[])
            .await
    }
}

struct Reply {
    status: StatusCode,
    content_type: String,
    body: String,
}

/// The revision's shape: 16 lowercase hex digits.
fn assert_revision_shape(revision: &str) {
    assert_eq!(revision.len(), 16, "{revision}");
    assert!(
        revision
            .bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b)),
        "{revision} is not 16 lowercase hex digits"
    );
}

impl Reply {
    /// The document's `revision`.
    fn token(&self) -> String {
        self.document()["revision"]
            .as_str()
            .expect("a string revision")
            .to_string()
    }

    fn json(&self) -> Value {
        serde_json::from_str(&self.body).unwrap_or_else(|e| panic!("not JSON ({e}): {}", self.body))
    }

    /// The 200 document, answered as JSON.
    fn document(&self) -> Value {
        assert_eq!(self.status, StatusCode::OK, "{}", self.body);
        assert!(
            self.content_type.starts_with("application/json"),
            "content-type {:?}",
            self.content_type
        );
        self.json()
    }

    /// An error answer: its status, its `key`, and the `args` array, for a
    /// test that looks at the args.
    fn assert_key(&self, status: StatusCode, key: &str) -> Vec<Value> {
        assert_eq!(self.status, status, "{}", self.body);
        assert!(
            self.content_type.starts_with("application/json"),
            "content-type {:?}",
            self.content_type
        );
        let body = self.json();
        assert_eq!(body["key"], json!(key), "{}", self.body);
        body["args"]
            .as_array()
            .cloned()
            .unwrap_or_else(|| panic!("no args array: {}", self.body))
    }

    /// An error answer with no args.
    fn assert_error(&self, status: StatusCode, key: &str) {
        let args = self.assert_key(status, key);
        assert_eq!(args, Vec::<Value>::new(), "{}", self.body);
    }

    /// Nothing of the manager's own words, its paths or its addresses in the
    /// body.
    fn assert_no_manager_detail(&self) {
        let lower = self.body.to_ascii_lowercase();
        for needle in [
            "not authorized",
            "refused",
            "no such file",
            "permission denied",
            "timed out",
            "os error",
            "unix_path",
            "npipe",
            "\\\\.\\pipe",
            "private-ai-manager",
            "io::error",
            "reqwest",
            "c:\\",
            "/var/",
            "/etc/",
        ] {
            assert!(
                !lower.contains(needle),
                "the body carries a server detail ({needle}): {}",
                self.body
            );
        }
    }

    /// The session is still signed in: `GET /api/session` answers `{"user"}`.
    fn assert_signed_in_as(&self, user: &str) {
        assert_eq!(self.status, StatusCode::OK, "signed in: {}", self.body);
        assert_eq!(self.json(), json!({ "user": user }), "{}", self.body);
    }
}

// ── Failure paths first: the session ────────────────────────────────────

/// No session cookie: 401 `session-required`, and the manager is not even
/// connected to: the model profile files of a stranger are not read.
#[tokio::test]
async fn without_a_session_the_manager_is_not_asked() {
    let app = app_holding(FILE_ALPHA, "");
    let reply = app.get("/api/profiles/model/alpha", None, &[]).await;
    reply.assert_error(StatusCode::UNAUTHORIZED, "session-required");
    reply.assert_no_manager_detail();
    assert_eq!(app.connects.load(Ordering::SeqCst), 0, "no connect");
    assert!(app.calls().is_empty(), "{:?}", app.calls());
    assert!(app.principals().is_empty(), "{:?}", app.principals());
}

// ── The document ────────────────────────────────────────────────────────

/// A section of `models.ini`: the whole document, field for field, read over
/// one connect as the signed-in user, from both model profile files and nothing
/// else.
#[tokio::test]
async fn a_section_of_the_enabled_file_answers_the_document() {
    let app = app_holding(FILE_ALPHA, "");
    let doc = app.read("alpha").await.document();
    assert_eq!(doc, alpha_document(ON, true));
    assert_eq!(
        app.calls(),
        [format!("read {ON}"), format!("read {OFF}")],
        "the two preset files, the enabled one first, nothing else"
    );
    assert_eq!(app.connects.load(Ordering::SeqCst), 1, "one connect");
}

/// The fields, exactly the seven: the list row's `model` string is not there,
/// and the engine pair is not spelled another way.
#[tokio::test]
async fn the_document_has_the_pinned_fields() {
    let app = app_holding(FILE_ALPHA, "");
    let doc = app.read("alpha").await.document();
    let mut names: Vec<&String> = doc.as_object().expect("an object").keys().collect();
    names.sort();
    assert_eq!(
        names,
        [
            "enabled",
            "engine",
            "engineAvailable",
            "id",
            "projectUrl",
            "revision",
            "values"
        ],
        "{}",
        doc
    );
}

/// The id in the path is matched without case; the document answers the
/// spelling the file holds, so the page shows the model profile's own name.
#[tokio::test]
async fn the_id_is_matched_without_case_and_answered_as_the_file_spells_it() {
    let app = app_holding("[Alpha]\nmodel = /models/a.gguf\n", "");
    let doc = app.read("alpha").await.document();
    assert_eq!(doc["id"], "Alpha", "the file's spelling");
    assert_eq!(doc["enabled"], true);
    assert_eq!(doc["values"], json!({ "model": "/models/a.gguf" }));
    let same = app.read("ALPHA").await.document();
    assert_eq!(same, doc, "the same section, any case in the path");
}

/// A section in both files is the enabled one: the values and the revision of
/// `models.ini`, and `enabled` true, as the list answers the same id.
#[tokio::test]
async fn a_section_in_both_files_is_the_enabled_copy() {
    let app = app_holding(FILE_ALPHA, FILE_ALPHA);
    let doc = app.read("alpha").await.document();
    assert_eq!(doc["enabled"], true, "the enabled copy");
    assert_eq!(doc["values"], alpha_document(ON, true)["values"]);
    assert_eq!(doc["revision"], REV_ALPHA_ON, "the enabled file's revision");
    assert_ne!(
        doc["revision"],
        json!(revision_of(&KEY_A, OFF, ALPHA)),
        "not the switched-off file's"
    );
}

/// A section that is only in `models.ini.disabled` is read: `enabled` false,
/// and its revision is keyed on the file it sits in, so the same text in
/// `models.ini` is another revision.
#[tokio::test]
async fn a_section_that_is_only_disabled_is_answered_switched_off() {
    let app = app_holding("[Other]\nmodel = /models/o.gguf\n", FILE_ALPHA);
    let doc = app.read("alpha").await.document();
    assert_eq!(doc, alpha_document(OFF, false));
    assert_eq!(doc["revision"], REV_ALPHA_OFF, "keyed on the file name");
    assert_ne!(
        doc["revision"],
        json!(revision_of(&KEY_A, ON, ALPHA)),
        "one text, two files, two sections"
    );
}

/// `values` are the strings the file holds: the value trimmed, everything from
/// a `;` cut, the file's separators kept, an empty value the empty string, the
/// key spelled as the file spells it, and a key the section does not hold
/// absent from the object.
#[tokio::test]
async fn values_are_the_strings_the_file_holds() {
    let file = "[Gamma]\r\nmodel = C:\\Models\\gamma gguf.gguf ; the router reads this file\r\n   n-gpu-layers = 99   \r\nn_ctx = 4096\r\nempty-key =\r\n";
    let app = app_holding(file, "");
    let doc = app.read("Gamma").await.document();
    assert_eq!(
        doc["values"],
        json!({
            "model": "C:\\Models\\gamma gguf.gguf",
            "n-gpu-layers": "99",
            "n_ctx": "4096",
            "empty-key": "",
        }),
        "raw strings, as the file's parser reads them"
    );
    assert!(
        doc["values"].get("ctx-size").is_none(),
        "a key the section does not hold is absent, not null: {}",
        doc["values"]
    );
    assert!(
        doc["values"]["model"].is_string(),
        "a value is a string, never a number: {}",
        doc["values"]
    );
    assert_eq!(doc["projectUrl"], "", "no `; url = ` comment");
}

/// A section with no keys is a model profile the file declares with its header
/// alone: the document answers an empty `values` object.
#[tokio::test]
async fn a_section_with_no_keys_answers_empty_values() {
    let app = app_holding("[Bare]\n\n[alpha]\nmodel = /models/a.gguf\n", "");
    let doc = app.read("Bare").await.document();
    assert_eq!(doc["values"], json!({}), "an empty object");
    assert_eq!(doc["id"], "Bare");
    assert_eq!(doc["enabled"], true);
}

/// A key written twice in the section reads as the last line, as the file's
/// parser and llama-server read it.
#[tokio::test]
async fn a_repeated_key_is_the_last_line() {
    let app = app_holding(
        "[Delta]\nmodel = /models/first.gguf\nmodel = /models/last.gguf\n",
        "",
    );
    let doc = app.read("Delta").await.document();
    assert_eq!(
        doc["values"],
        json!({ "model": "/models/last.gguf" }),
        "the last line wins"
    );
}

/// `projectUrl` is the section's `; url = ` comment and only it: prose about
/// urls is prose, a word starting with `url` is not `url`, and the first url
/// comment is the one. The same answers the configurator gives, because both
/// read through the one reader in `pai-core` `ini`.
#[tokio::test]
async fn the_project_url_comes_from_the_url_comment() {
    let app = app_holding(
        "[Eps]\n; urls are listed below\n; url = https://example.org/a\n; url-ish = no\n; url = https://example.org/b\nmodel = /models/e.gguf\n",
        "",
    );
    let doc = app.read("Eps").await.document();
    assert_eq!(
        doc["projectUrl"], "https://example.org/a",
        "the first url comment"
    );
    assert_eq!(
        doc["values"],
        json!({ "model": "/models/e.gguf" }),
        "a comment is not a key"
    );
}

// ── The section is not there ────────────────────────────────────────────

/// llama-server's global `[*]` section is not a model profile: the read of `*`, of
/// its percent form and of the whole bracketed form is 404 `no-model-profile-named`,
/// with nothing echoed: `*` is not a name a model profile can carry. A path with a
/// segment below the section is a different path, pinned in
/// `a_path_below_the_section_is_the_routers_own_not_found`.
#[tokio::test]
async fn the_global_section_is_not_a_model_profile() {
    let app = app_holding(
        "[*]\nmodel = /models/global.gguf\n\n[alpha]\nmodel = /models/a.gguf\n",
        "",
    );
    for form in ["*", "%2A", "%5B%2A%5D"] {
        let reply = app.read(form).await;
        let args = reply.assert_key(StatusCode::NOT_FOUND, "no-model-profile-named");
        assert_eq!(args, Vec::<Value>::new(), "not a preset name: {form}");
    }
    // The files are read as they are for any other id: the answer comes from
    // the section list, which carries no `*`. One connect and two reads each,
    // and nothing else: no write, no status, no instance list.
    let pair = [format!("read {ON}"), format!("read {OFF}")];
    let asked: Vec<String> = pair.iter().cloned().cycle().take(6).collect();
    assert_eq!(app.calls(), asked, "the two preset files, three times");
    assert_eq!(app.connects.load(Ordering::SeqCst), 3, "one each");
}

/// A path with a segment below the section is not this read: the only path of
/// this API with a segment below a model profile is the switch,
/// `PUT /api/profiles/model/{id}/enabled`, so the router answers its own 404, with no
/// body and no JSON, and the read never starts. A percent-encoded `*` is the
/// same path: the router matches the raw path, so the id `alpha/*` is never
/// handed to a section name.
#[tokio::test]
async fn a_path_below_the_section_is_the_routers_own_not_found() {
    let app = app_holding(FILE_ALPHA, "");
    for form in ["alpha/%2A", "alpha/*"] {
        let reply = app.read(form).await;
        assert_eq!(
            reply.status,
            StatusCode::NOT_FOUND,
            "{form}: {}",
            reply.body
        );
        assert_eq!(reply.content_type, "", "{form}: not a JSON answer");
        assert_eq!(reply.body, "", "{form}: the router's bare 404");
    }
    assert_eq!(app.connects.load(Ordering::SeqCst), 0, "the read never ran");
    assert!(app.calls().is_empty(), "{:?}", app.calls());
}

/// An id that is no section is 404 `no-model-profile-named`, with the id echoed as
/// the only arg so the page can say which model profile it could not find.
#[tokio::test]
async fn an_unknown_id_is_not_found() {
    let app = app_holding(FILE_ALPHA, "");
    let reply = app.read("Zeta").await;
    let args = reply.assert_key(StatusCode::NOT_FOUND, "no-model-profile-named");
    assert_eq!(args, vec![json!("Zeta")], "the id echoed");
    reply.assert_no_manager_detail();
    assert_eq!(
        app.calls(),
        [format!("read {ON}"), format!("read {OFF}")],
        "both files were read"
    );
}

/// An id of dots is a name the checks allow and no model profile carries, so it is
/// not echoed: 404 with no args.
#[tokio::test]
async fn ids_of_dots_are_not_found() {
    let app = app_holding(FILE_ALPHA, "");
    for form in [".", "..", "%2E%2E"] {
        let reply = app.read(form).await;
        let args = reply.assert_key(StatusCode::NOT_FOUND, "no-model-profile-named");
        assert_eq!(args, Vec::<Value>::new(), "not a preset name: {form}");
    }
}

/// No model profile file at all is not an error: no file, no model profile, 404.
#[tokio::test]
async fn no_model_profile_file_answers_not_found() {
    let app = app_no_files();
    let reply = app.read("alpha").await;
    let args = reply.assert_key(StatusCode::NOT_FOUND, "no-model-profile-named");
    assert_eq!(args, vec![json!("alpha")]);
}

// ── The manager, and the session ────────────────────────────────────────

/// The manager refuses the read for this user (removed from `pai` after the
/// sign-in): 403 `session-not-administrator`, the sign-in's key for the same
/// fact, and the session ends with the right that authorized it. The refusal
/// stops the read: the second file is not asked. The other session is
/// untouched.
#[tokio::test]
async fn a_refused_user_is_answered_403_and_signed_out() {
    let app = app_refusing(&[USER], FILE_ALPHA, "");
    let cookie = app.sign_in_as(USER).await;
    let other = app.sign_in_as("root").await;
    let reply = app
        .get("/api/profiles/model/alpha", Some(&cookie), &[])
        .await;
    reply.assert_error(StatusCode::FORBIDDEN, "session-not-administrator");
    reply.assert_no_manager_detail();
    app.get("/api/session", Some(&cookie), &[])
        .await
        .assert_error(StatusCode::UNAUTHORIZED, "session-required");
    app.get("/api/session", Some(&other), &[])
        .await
        .assert_signed_in_as("root");
    assert_eq!(
        app.calls(),
        [format!("read {ON}")],
        "the refusal ends the read: {:?}",
        app.calls()
    );
}

/// No manager to connect to: 503 `client-unavailable`, the key the server
/// read answers, and none of the client's own words in the body.
#[tokio::test]
async fn no_manager_is_unavailable() {
    let app = app_no_manager();
    let reply = app.read("alpha").await;
    reply.assert_error(StatusCode::SERVICE_UNAVAILABLE, "client-unavailable");
    reply.assert_no_manager_detail();
    assert_eq!(app.connects.load(Ordering::SeqCst), 1, "connected once");
}

/// The manager is there and goes away during the read: the same 503.
#[tokio::test]
async fn a_manager_gone_during_the_read_is_unavailable() {
    let app = app_holding(FILE_ALPHA, "");
    let cookie = app.sign_in_as(USER).await;
    app.set_read_failure(Failure::Unavailable);
    let reply = app
        .get("/api/profiles/model/alpha", Some(&cookie), &[])
        .await;
    reply.assert_error(StatusCode::SERVICE_UNAVAILABLE, "client-unavailable");
    reply.assert_no_manager_detail();
}

/// A manager that answers in 2000 ms, over the bff's 200 ms limit: 503
/// `client-unavailable`, answered at the limit and not at the deadline, so
/// the page is never left waiting.
#[tokio::test]
async fn a_manager_too_slow_is_unavailable() {
    let app = app_holding(FILE_ALPHA, "");
    app.set_read_pause_ms(2000);
    let cookie = app.sign_in_as(USER).await;
    let started = Instant::now();
    app.get("/api/profiles/model/alpha", Some(&cookie), &[])
        .await
        .assert_error(StatusCode::SERVICE_UNAVAILABLE, "client-unavailable");
    let took = started.elapsed();
    assert!(
        took >= TIMEOUT && took < DEADLINE / 2,
        "answered at the timeout, not at the deadline: {took:?}"
    );
}

/// Any other failure is 502 `client-other`, the detail kept in the bff, and
/// the session is NOT ended: the failure is the manager's, not a sign of the
/// user's rights. A second read answers the same way, so the session was not
/// silently dropped.
#[tokio::test]
async fn any_other_failure_is_client_other_and_keeps_the_session() {
    let app = app_holding(FILE_ALPHA, "");
    app.set_read_failure(Failure::Other("the read failed".into()));
    let cookie = app.sign_in_as(USER).await;
    let reply = app
        .get("/api/profiles/model/alpha", Some(&cookie), &[])
        .await;
    reply.assert_error(StatusCode::BAD_GATEWAY, "client-other");
    reply.assert_no_manager_detail();
    app.get("/api/session", Some(&cookie), &[])
        .await
        .assert_signed_in_as(USER);
    app.get("/api/profiles/model/alpha", Some(&cookie), &[])
        .await
        .assert_error(StatusCode::BAD_GATEWAY, "client-other");
}

// ── The principal ───────────────────────────────────────────────────────

/// One connect per request, asked for the signed-in user, so the manager
/// re-checks that user's membership of `pai` at every read.
#[tokio::test]
async fn the_read_connects_as_the_signed_in_user() {
    for user in ["alice", "root"] {
        let app = app_holding(FILE_ALPHA, "");
        let cookie = app.sign_in_as(user).await;
        assert_eq!(
            app.get("/api/profiles/model/alpha", Some(&cookie), &[])
                .await
                .document()["id"],
            "alpha",
            "{user}"
        );
        assert_eq!(
            app.principals(),
            vec![Some(user.to_string())],
            "{user}: the principal of the connect"
        );
        assert_eq!(app.connects.load(Ordering::SeqCst), 1, "{user}");
        assert_eq!(
            app.calls(),
            [format!("read {ON}"), format!("read {OFF}")],
            "{user}: the preset files and nothing else"
        );
    }
}

/// A name in the request never becomes the principal: a query parameter and
/// headers naming another user are ignored, the session's user is asked for.
#[tokio::test]
async fn a_name_in_the_request_is_never_the_principal() {
    let app = app_holding(FILE_ALPHA, "");
    let cookie = app.sign_in_as(USER).await;
    let doc = app
        .get(
            "/api/profiles/model/alpha?user=root",
            Some(&cookie),
            &[("x-user", "root"), ("x-principal", "root")],
        )
        .await
        .document();
    assert_eq!(doc["id"], "alpha");
    assert_eq!(
        app.principals(),
        vec![Some(USER.to_string())],
        "the session's user, never the request's"
    );
}

// ── The route ───────────────────────────────────────────────────────────

/// `POST` and `DELETE` on the section path are not served: the method is not
/// routed, so the section is never read.
#[tokio::test]
async fn post_and_delete_on_the_section_path_are_not_served() {
    let app = app_holding(FILE_ALPHA, "");
    let cookie = app.sign_in_as(USER).await;
    for method in [Method::POST, Method::DELETE] {
        let request = Request::builder()
            .method(method.clone())
            .uri("/api/profiles/model/alpha")
            .header(header::COOKIE, cookie.clone())
            .header(header::CONTENT_TYPE, "application/json")
            .body(Body::from(json!({ "values": {} }).to_string()))
            .expect("request");
        let reply = app.send(request).await;
        assert_eq!(
            reply.status,
            StatusCode::METHOD_NOT_ALLOWED,
            "{method} /api/profiles/model/alpha: {}",
            reply.body
        );
    }
    assert_eq!(app.connects.load(Ordering::SeqCst), 0, "no connect");
    assert!(app.calls().is_empty(), "{:?}", app.calls());
}

// ── The revision ────────────────────────────────────────────────────────

/// `revision`: 16 lowercase hex digits, the published SipHash-2-4 under the
/// router's key over the file's name, a newline and the section's text. The
/// expected value is computed here; it is neither the hash of the whole file,
/// nor of the section text alone, nor of a neighbour, nor FNV-1a.
#[tokio::test]
async fn the_revision_is_the_keyed_hash_of_the_file_and_the_section() {
    let app = app_holding(FILE_ALPHA, "");
    let revision = app.read("alpha").await.token();
    assert_revision_shape(&revision);
    assert_eq!(revision, REV_ALPHA_ON, "the published vectors");
    assert_eq!(
        revision,
        revision_of(&KEY_A, ON, ALPHA),
        "the file's name, LF, the section's text"
    );
    assert_ne!(revision, REV_WHOLE_FILE, "not the hash of the whole file");
    assert_ne!(
        revision,
        format!("{:016x}", siphash_2_4(&KEY_A, ALPHA.as_bytes())),
        "not the hash of the section text alone: the file it sits in counts"
    );
    assert_ne!(revision, REV_NEIGHBOUR, "not a neighbour section's");
    assert_ne!(
        revision,
        fnv1a(&format!("{ON}\n{ALPHA}")),
        "not the unkeyed hash"
    );
    assert_ne!(
        revision,
        fnv1a(ALPHA),
        "not the unkeyed hash of the section"
    );
}

/// The same section answers the same revision twice through one router, and
/// a change to another section of the file does not move it: the page's
/// `If-Match` pair stays valid while someone edits a neighbour, and a section
/// appended after it leaves the one before alone. The appended section has a
/// revision of its own, over its own text and the file it sits in.
#[tokio::test]
async fn the_revision_is_stable_across_another_section() {
    let grown = format!("{FILE_ALPHA}{ZETA}");
    let one = app_holding(FILE_ALPHA, "").read("alpha").await.document();
    let twice = app_holding(FILE_ALPHA, "").read("alpha").await.document();
    let after = app_holding(&grown, "").read("alpha").await.document();
    assert_eq!(one["revision"], twice["revision"], "one router, twice");
    assert_eq!(one["revision"], REV_ALPHA_ON);
    assert_eq!(
        after["revision"], one["revision"],
        "a section added after it does not change it"
    );
    let appended = app_holding(&grown, "").read("Zeta").await.document();
    assert_eq!(
        appended["revision"], REV_ZETA,
        "the appended section has a revision of its own"
    );
    assert_eq!(
        appended["revision"],
        json!(revision_of(&KEY_A, ON, ZETA)),
        "its own text, in the file it sits in"
    );
    assert_ne!(
        appended["revision"], REV_NEIGHBOUR,
        "not the hash of [Bigger], the section above it in the same file"
    );
    assert_ne!(
        appended["revision"], one["revision"],
        "two sections of one file are two identities"
    );
}

/// The file's LAST section, with no line break after its last key (a hand
/// edit, a file something appended to, in a CRLF file): its revision is the
/// section's lines through its last non-blank line, that line's own ending not
/// hashed. So adding the final line break, and appending a section below it,
/// leave it where it was, and all three answer the pinned digest of the
/// section's text without a final break.
#[tokio::test]
async fn a_last_section_with_no_final_break_keeps_its_revision_in_a_crlf_file() {
    let no_break = app_holding(ALPHA_NO_BREAK_CRLF, "").read("alpha").await;
    let with_break = app_holding(ALPHA_BREAK_CRLF, "").read("alpha").await;
    let appended = app_holding(ALPHA_PLUS_ZETA_CRLF, "").read("alpha").await;
    for reply in [&no_break, &with_break, &appended] {
        assert_revision_shape(&reply.token());
    }
    assert_eq!(
        no_break.token(),
        REV_ALPHA_NO_BREAK,
        "the section's own text, no final break"
    );
    assert_eq!(
        no_break.token(),
        REV_ALPHA_ON,
        "the same section with a final break"
    );
    assert_eq!(
        no_break.token(),
        revision_of(&KEY_A, ON, ALPHA_NO_BREAK_CRLF),
        "the file's name, LF, the section without its last line's ending"
    );
    assert_eq!(
        with_break.token(),
        no_break.token(),
        "a final line break added to the section"
    );
    assert_eq!(
        appended.token(),
        no_break.token(),
        "a section appended after it"
    );
    let neighbour = app_holding(ALPHA_PLUS_ZETA_CRLF, "").read("Zeta").await;
    assert_eq!(
        neighbour.token(),
        REV_ZETA,
        "the appended section digests its own text"
    );
    assert_eq!(
        neighbour.token(),
        revision_of(&KEY_A, ON, ZETA),
        "the appended section is ZETA, the text REV_ZETA digests"
    );
}

/// The same rule in an LF file: the last section without a final `\n` is the
/// section it is with the break added and with a section appended below it.
#[tokio::test]
async fn a_last_section_with_no_final_break_keeps_its_revision_in_an_lf_file() {
    let no_break = app_holding(ALPHA_NO_BREAK_LF, "").read("alpha").await;
    let with_break = app_holding(ALPHA_BREAK_LF, "").read("alpha").await;
    let appended = app_holding(ALPHA_PLUS_ZETA_LF, "").read("alpha").await;
    for reply in [&no_break, &with_break, &appended] {
        assert_revision_shape(&reply.token());
    }
    assert_eq!(
        no_break.token(),
        REV_ALPHA_NO_BREAK_LF,
        "the section's own text, no final break"
    );
    assert_eq!(
        no_break.token(),
        revision_of(&KEY_A, ON, ALPHA_NO_BREAK_LF),
        "the file's name, LF, the section without its last line's ending"
    );
    assert_eq!(
        with_break.token(),
        no_break.token(),
        "a final line break added to the section"
    );
    assert_eq!(
        appended.token(),
        no_break.token(),
        "a section appended after it"
    );
}

/// The rule cuts a line's ending, not the section: a key's value changed in a
/// section that ends the file with no line break moves its revision at once, so
/// a document read before the edit is stale.
#[tokio::test]
async fn a_value_changed_in_a_section_with_no_final_break_moves_its_revision() {
    let before = app_holding(ALPHA_NO_BREAK_LF, "").read("alpha").await;
    let after = app_holding(ALPHA_VALUE_LF, "").read("alpha").await;
    assert_eq!(
        before.token(),
        REV_ALPHA_NO_BREAK_LF,
        "the text before the edit"
    );
    assert_eq!(
        after.token(),
        REV_ALPHA_VALUE_LF,
        "the text after it, no final break either"
    );
    assert_eq!(
        after.token(),
        revision_of(&KEY_A, ON, ALPHA_VALUE_LF),
        "the section's lines, its last ending cut"
    );
    assert_ne!(
        before.token(),
        after.token(),
        "one key's value changed, a stale document"
    );
}

/// A change to the section itself moves its revision at once: a save of a
/// document read before the change is stale.
#[tokio::test]
async fn the_revision_changes_with_the_section() {
    let before = app_holding(FILE_ALPHA, "").read("alpha").await.document();
    let after = app_holding(&format!("{ALPHA_WITH_CTX}\r\n"), "")
        .read("alpha")
        .await
        .document();
    assert_eq!(before["revision"], REV_ALPHA_ON);
    assert_eq!(after["revision"], REV_ALPHA_WITH_CTX, "the new text");
    assert_ne!(
        before["revision"], after["revision"],
        "one key added to the section"
    );
}

/// The revision is keyed: the same bytes through a router built with another
/// secret answer another revision, and that one is the hash of the same input
/// under the new key. The document's other fields do not move.
#[tokio::test]
async fn the_revision_changes_with_the_key_of_the_router() {
    let under_b = app_holding_key_b(FILE_ALPHA, "")
        .read("alpha")
        .await
        .document();
    assert_eq!(
        under_b["revision"], REV_ALPHA_KEY_B,
        "the secret 10 11 .. 1f"
    );
    assert_ne!(under_b["revision"], REV_ALPHA_ON, "two routers, one file");
    assert_eq!(
        under_b["values"],
        alpha_document(ON, true)["values"],
        "the values do not depend on the key"
    );
}

/// What the digest covers is the section as the file is written, not a re-write of
/// it: the save echoes the token of the bytes it wrote, so the token stands for
/// that text. Three files that parse to the same keys answer three tokens: the
/// key written twice is other text, and so are the same bytes with the other line
/// endings. A digest of a canonical re-render of the parsed keys would answer one
/// token for all three.
#[tokio::test]
async fn the_digest_covers_the_section_as_the_file_is_written() {
    let once = app_holding("[alpha]\nmodel = /models/a.gguf\n", "")
        .read("alpha")
        .await;
    let twice = app_holding(
        "[alpha]\nmodel = /models/a.gguf\nmodel = /models/a.gguf\n",
        "",
    )
    .read("alpha")
    .await;
    let crlf = app_holding("[alpha]\r\nmodel = /models/a.gguf\r\n", "")
        .read("alpha")
        .await;

    let all = [&once, &twice, &crlf];
    for reply in all {
        assert_eq!(reply.status, StatusCode::OK, "{}", reply.body);
    }

    // The three parse to the same keys: only the text of the section differs.
    let values = once.json()["values"].clone();
    assert_eq!(values, twice.json()["values"], "the key written twice");
    assert_eq!(values, crlf.json()["values"], "the other line endings");

    // And they are three different sections.
    let (a, b, c) = (once.token(), twice.token(), crlf.token());
    for token in [&a, &b, &c] {
        assert_revision_shape(token);
    }
    assert_ne!(a, b, "the key written twice is other text");
    assert_ne!(a, c, "CRLF is other text than LF");

    // The blank lines that follow the section are not part of its text.
    let blanks = app_holding("[alpha]\nmodel = /models/a.gguf\n\n\n\n", "")
        .read("alpha")
        .await;
    assert_eq!(
        blanks.token(),
        a,
        "the section's text ends with its last line"
    );
}
