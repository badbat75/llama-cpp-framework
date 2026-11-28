//! The instance profile routes (IP4c), through `pai_bff::router_with_session`
//! with a fake manager client and a fake authenticator, never the real bus,
//! pipe, PAM or LogonUser: `GET`/`POST /api/profiles/instance`,
//! `GET`/`DELETE /api/profiles/instance/{p}`, `PUT .../{p}/server`,
//! `POST .../{p}/models` and `GET`/`PUT`/`DELETE .../{p}/models/{m}`. Every
//! request runs under a deadline, so a bff that waits on the manager fails
//! instead of hanging.
//!
//! The contract (`ARCHITECTURE.md` IP4, copied by `fe/briefs/api-contract.md`):
//! - The session, as everywhere: 401 `session-required` with no session and no
//!   connect at all; 403 `session-not-administrator` and the session ended when
//!   the manager refuses the signed-in user; 503 `client-unavailable` before the
//!   manager's call was handed over, 504 `client-timeout` after, the session
//!   kept.
//! - The wire keys are brief 26's, the bff mapping the manager's:
//!   `instance-profile-bad-id` is 422 `invalid-instance-profile-id` with `field`
//!   `id`, `InstanceProfileExists` 409 `instance-profile-name-taken` with
//!   `field` `id`, `NoInstanceProfile` 404 `no-instance-profile-named` naming
//!   the profile, `NoServerProfile` 422 `no-server-profile-named` with `field`
//!   `server`, `InstanceProfileInUse` 409 `instance-profile-in-use [instance]`.
//!   Every other keyed refusal keeps the manager's key and args: 422 for a value
//!   rule, 409 for a conflict, and `no-instance-profile-model` is the entry's 404.
//! - Every write that re-renders answers exactly `{"running": [ids],
//!   "notConverged": [{instance, key, args}]}`, read off the manager's
//!   `Affected`. The profile create answers 201 `{"id"}`, its delete 204.
//! - Only the entry's save carries a revision: a SHA-256 hex of the profile's
//!   own sections (`[<p>]` and its `[<p>/*]`), so an edit of another profile
//!   leaves it alone. The bff reads the pool, compares the hash (a stale one is
//!   409 `stale-revision`, with no write asked), hands the manager the pool text
//!   it read (the manager's own compare answers the same 409), and the save
//!   answers the NEW hash, so a second save needs no re-read. A body with no
//!   revision is 422 `model-profiles-malformed`; bytes nothing can be made of
//!   are 400; neither reaches the manager.
//! - `GET /api/profiles/instance` answers `{"rows": [{id, server, models,
//!   usedBy}]}` in pool order, `usedBy` computed by the bff from the instance
//!   rows' `instanceProfile`. The page's `{id, server, models: [{id, keys}],
//!   usedBy, revision}` and the entry's `{profile, server, model, values,
//!   revision}` come from the pool text `GetInstanceProfile` answers, read with
//!   `pai-core`'s INI reader: no file read, no new verb.

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
    ManagerFile, NotConvergedRow,
};
use pai_core::ini;
use pai_core::instance_profiles;
use serde_json::{json, Value};
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
const CLIENT: &str = "127.0.0.1:52001";

/// The revision secret these routers are built with.
const REVISION_KEY: [u8; 16] = [0; 16];

/// The key of the gate's answer (`session.rs`).
const SESSION_REQUIRED: &str = "session-required";

/// The engine every instance of these tests runs.
const ENGINE: &str = "llama.cpp";

/// The log of the manager's calls.
type Log = Arc<Mutex<Vec<String>>>;

/// The pool's text, shared so a write the fake lands shows in the next read.
type Pool = Arc<Mutex<String>>;

/// The `(profile, model, values, revision)` of the last entry save handed to
/// the fake.
type LastSave = Arc<Mutex<Option<(String, String, Vec<(String, String)>, String)>>>;

/// The text of `instance-profiles.ini`: `[<p>]` sections for `profiles`, the
/// `(id, server profile)` pairs in order, then the `[<p>/<m>]` entries of
/// `entries`, the `(profile, model, keys)` sections, in the file's own shape.
#[allow(clippy::type_complexity)]
fn pool_text(profiles: &[(&str, &str)], entries: &[(&str, &str, &[(&str, &str)])]) -> String {
    let mut text = String::new();
    for (id, server) in profiles {
        text = instance_profiles::add_profile(&text, id, server);
    }
    for (profile, model, keys) in entries {
        let owned: Vec<(String, String)> = keys
            .iter()
            .map(|(k, v)| (k.to_string(), v.to_string()))
            .collect();
        text = if owned.is_empty() {
            instance_profiles::add_entry(&text, profile, model)
        } else {
            instance_profiles::set_entry_keys(&text, profile, model, &owned)
        };
    }
    text
}

/// The pool these tests start from: `alpha` on `default`, with the entry
/// `alpha1` (two keys) and the entry `alpha2` (no key), and `beta` on `big`,
/// with the entry `beta1` (one key).
fn pool() -> String {
    pool_text(
        &[("alpha", "default"), ("beta", "big")],
        &[
            (
                "alpha",
                "alpha1",
                &[("device", "cuda0"), ("n-gpu-layers", "99")],
            ),
            ("alpha", "alpha2", &[]),
            ("beta", "beta1", &[("device", "cuda1")]),
        ],
    )
}

/// `text` without the section `section` (a text that has no such section is
/// handed back as it is).
fn drop_section(text: String, section: &str) -> String {
    let left = ini::text::delete_section(&text, section);
    left.unwrap_or(text)
}

/// `text` without the profile `id`: its `[<id>]` section and every entry of it.
fn drop_profile(text: &str, id: &str) -> String {
    let mut out = text.to_string();
    for entry in instance_profiles::parse(text)
        .into_iter()
        .filter(|p| p.id.eq_ignore_ascii_case(id))
        .flat_map(|p| p.entries)
    {
        out = drop_section(out, &format!("{id}/{}", entry.model));
    }
    drop_section(out, id)
}

/// One row of `instances`: the instance `id`, running the instance profile
/// `profile` (`None` names none).
fn row(id: &str, profile: Option<&str>) -> InstanceInfo {
    InstanceInfo {
        id: id.to_string(),
        engine: ENGINE.to_string(),
        server_profile: Some("default".to_string()),
        port: 8080,
        profile_tags: Vec::new(),
        instance_tags: Vec::new(),
        label: None,
        converged: true,
        reason: None,
        pending_restart: false,
        instance_profile: profile.map(str::to_string),
        hostname: None,
    }
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

/// What the fake manager answers.
#[derive(Clone)]
struct Fixture {
    /// The pool's text.
    pool: Pool,
    /// The record's rows, as `instances` answers them.
    rows: Vec<InstanceInfo>,
    /// What `instances` answers, ahead of `rows`.
    rows_fail: Option<ClientError>,
    /// What every instance profile verb answers.
    list: Result<Vec<InstanceProfileInfo>, ClientError>,
    profile: Result<InstanceProfileInfo, ClientError>,
    create: Result<(), ClientError>,
    remove_profile: Result<(), ClientError>,
    set_server: Result<Affected, ClientError>,
    add: Result<Affected, ClientError>,
    remove_entry: Result<Affected, ClientError>,
    set_entry: Result<Affected, ClientError>,
    /// How long every write call sleeps: the write that outlives the limit.
    write_delay: Duration,
    /// The last entry save the fake was handed.
    last_save: LastSave,
}

impl Default for Fixture {
    fn default() -> Fixture {
        Fixture {
            pool: Pool::default(),
            rows: Vec::new(),
            rows_fail: None,
            list: Ok(Vec::new()),
            profile: Ok(InstanceProfileInfo {
                id: String::new(),
                server_profile: String::new(),
                entries: Vec::new(),
                revision: String::new(),
            }),
            create: Ok(()),
            remove_profile: Ok(()),
            set_server: Ok(affected()),
            add: Ok(affected()),
            remove_entry: Ok(affected()),
            set_entry: Ok(affected()),
            write_delay: Duration::ZERO,
            last_save: LastSave::default(),
        }
    }
}

impl Fixture {
    /// The pool of `pool()`.
    fn with_pool(self) -> Fixture {
        *self.pool.lock().unwrap_or_else(|e| e.into_inner()) = pool();
        self
    }

    /// The record gains one row per `(instance, instance profile)` entry.
    fn used(mut self, entries: &[(&str, Option<&str>)]) -> Fixture {
        self.rows = entries
            .iter()
            .map(|(id, profile)| row(id, *profile))
            .collect();
        self
    }

    /// Every instance profile verb answers `fail`: the manager refuses the
    /// signed-in user on every call.
    fn refuses_everything(mut self, fail: ClientError) -> Fixture {
        self.list = Err(fail.clone());
        self.profile = Err(fail.clone());
        self.create = Err(fail.clone());
        self.remove_profile = Err(fail.clone());
        self.set_server = Err(fail.clone());
        self.add = Err(fail.clone());
        self.remove_entry = Err(fail.clone());
        self.set_entry = Err(fail);
        self
    }

    /// `instance_profiles` answers `fail`.
    fn list_fails(mut self, fail: ClientError) -> Fixture {
        self.list = Err(fail);
        self
    }

    /// `instance_profile` answers `fail`.
    fn profile_fails(mut self, fail: ClientError) -> Fixture {
        self.profile = Err(fail);
        self
    }

    /// `instances` answers `fail`.
    fn rows_fails(mut self, fail: ClientError) -> Fixture {
        self.rows_fail = Some(fail);
        self
    }

    /// `create_instance_profile` answers `fail`.
    fn create_fails(mut self, fail: ClientError) -> Fixture {
        self.create = Err(fail);
        self
    }

    /// `delete_instance_profile` answers `fail`.
    fn remove_profile_fails(mut self, fail: ClientError) -> Fixture {
        self.remove_profile = Err(fail);
        self
    }

    /// `set_instance_profile_server` answers `fail`.
    fn set_server_fails(mut self, fail: ClientError) -> Fixture {
        self.set_server = Err(fail);
        self
    }

    /// `add_instance_profile_model` answers `fail`.
    fn add_fails(mut self, fail: ClientError) -> Fixture {
        self.add = Err(fail);
        self
    }

    /// `remove_instance_profile_model` answers `fail`.
    fn remove_entry_fails(mut self, fail: ClientError) -> Fixture {
        self.remove_entry = Err(fail);
        self
    }

    /// `set_instance_profile_model` answers `fail`.
    fn set_entry_fails(mut self, fail: ClientError) -> Fixture {
        self.set_entry = Err(fail);
        self
    }

    /// Every write call sleeps `delay` before it answers.
    fn write_delay(mut self, delay: Duration) -> Fixture {
        self.write_delay = delay;
        self
    }
}

/// A manager that answers from a [`Fixture`] and logs every call
/// (`instance-profiles`, `instance-profile <p>`, `create-profile <p> <sp>`,
/// `delete-profile <p>`, `set-server <p> <sp>`, `add-model <p> <m>`,
/// `remove-model <p> <m>`, `set-entry <p> <m>`, `instances`, `read <name>`),
/// landing the writes it accepts in the pool's text.
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
                list: fixture.list.clone(),
                profile: fixture.profile.clone(),
                create: fixture.create.clone(),
                remove_profile: fixture.remove_profile.clone(),
                set_server: fixture.set_server.clone(),
                add: fixture.add.clone(),
                remove_entry: fixture.remove_entry.clone(),
                set_entry: fixture.set_entry.clone(),
                write_delay: fixture.write_delay,
                last_save: fixture.last_save.clone(),
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

    fn set_pool(&self, text: &str) {
        *self.fixture.pool.lock().unwrap_or_else(|e| e.into_inner()) = text.to_string();
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
    fn instance_profiles(&self) -> Result<Vec<InstanceProfileInfo>, ClientError> {
        self.record("instance-profiles".into());
        if let Err(fail) = &self.fixture.list {
            return Err(fail.clone());
        }
        Ok(instance_profiles::parse(&self.pool())
            .into_iter()
            .map(|p| InstanceProfileInfo {
                id: p.id,
                server_profile: p.server_profile.unwrap_or_default(),
                entries: p.entries.into_iter().map(|e| e.model).collect(),
                revision: String::new(),
            })
            .collect())
    }
    fn instance_profile(&self, profile: &str) -> Result<InstanceProfileInfo, ClientError> {
        self.record(format!("instance-profile {profile}"));
        if let Err(fail) = &self.fixture.profile {
            return Err(fail.clone());
        }
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
    fn create_instance_profile(
        &self,
        profile: &str,
        server_profile: &str,
    ) -> Result<(), ClientError> {
        self.record(format!("create-profile {profile} {server_profile}"));
        let answer = self.answered(&self.fixture.create);
        if answer.is_ok() {
            let text = instance_profiles::add_profile(&self.pool(), profile, server_profile);
            self.set_pool(&text);
        }
        answer
    }
    fn delete_instance_profile(&self, profile: &str) -> Result<(), ClientError> {
        self.record(format!("delete-profile {profile}"));
        let answer = self.answered(&self.fixture.remove_profile);
        if answer.is_ok() {
            let text = drop_profile(&self.pool(), profile);
            self.set_pool(&text);
        }
        answer
    }
    fn set_instance_profile_server(
        &self,
        profile: &str,
        server_profile: &str,
    ) -> Result<Affected, ClientError> {
        self.record(format!("set-server {profile} {server_profile}"));
        let answer = self.answered(&self.fixture.set_server);
        if answer.is_ok() {
            let text = ini::text::set_key(&self.pool(), profile, "ServerProfile", server_profile);
            self.set_pool(&text);
        }
        answer
    }
    fn add_instance_profile_model(
        &self,
        profile: &str,
        model: &str,
    ) -> Result<Affected, ClientError> {
        self.record(format!("add-model {profile} {model}"));
        let answer = self.answered(&self.fixture.add);
        if answer.is_ok() {
            let text = instance_profiles::add_entry(&self.pool(), profile, model);
            self.set_pool(&text);
        }
        answer
    }
    fn remove_instance_profile_model(
        &self,
        profile: &str,
        model: &str,
    ) -> Result<Affected, ClientError> {
        self.record(format!("remove-model {profile} {model}"));
        let answer = self.answered(&self.fixture.remove_entry);
        if answer.is_ok() {
            let section = format!("{profile}/{model}");
            let text = drop_section(self.pool(), &section);
            self.set_pool(&text);
        }
        answer
    }
    fn set_instance_profile_model(
        &self,
        profile: &str,
        model: &str,
        values: &[(String, String)],
        revision: &str,
    ) -> Result<Affected, ClientError> {
        self.record(format!("set-entry {profile} {model}"));
        *self
            .fixture
            .last_save
            .lock()
            .unwrap_or_else(|e| e.into_inner()) = Some((
            profile.to_string(),
            model.to_string(),
            values.to_vec(),
            revision.to_string(),
        ));
        let answer = self.answered(&self.fixture.set_entry);
        if answer.is_ok() {
            let text = instance_profiles::set_entry_keys(&self.pool(), profile, model, values);
            self.set_pool(&text);
        }
        answer
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

    /// An error answer: its status, and `{"key": key, "args": args}` as JSON. A
    /// route the bff does not route answers a bare 404 with an empty body, so
    /// every one of these is red, not accidentally green.
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

    /// An error answer of `status` whose key is `key`, the `args` being the
    /// manager's own: used where the contract names the key and the status.
    fn assert_key(&self, status: StatusCode, key: &str) {
        assert_eq!(self.status, status, "{}", self.body);
        assert_eq!(self.json()["key"], key, "{}", self.body);
    }

    /// An error answer of `status` with the key, its args, and the `field` the
    /// form marks: the contract names the field beside the key (the create's
    /// refusals all mark `id` or `server`).
    fn assert_error_field(&self, status: StatusCode, key: &str, args: &[&str], field: &str) {
        assert_eq!(self.status, status, "{}", self.body);
        assert_eq!(
            self.json(),
            json!({ "key": key, "args": args, "field": field }),
            "{}",
            self.body
        );
    }

    /// 422 with the check's key, its args, and the field the form marks.
    fn assert_refused(&self, key: &str, args: &[&str], field: &str) {
        self.assert_error_field(StatusCode::UNPROCESSABLE_ENTITY, key, args, field);
    }

    /// A body nothing can be made of: 400, refused as a malformed body. The
    /// contract names no key for the create, the switch and the add, so any
    /// `<what>-malformed` is accepted here, with empty args.
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

    /// The answer's `revision`, as text (empty when there is none).
    fn revision(&self) -> String {
        self.json()["revision"]
            .as_str()
            .unwrap_or_default()
            .to_string()
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
/// reads (its shared seams: the pool's text, the last read and save).
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

/// The wire revision: a SHA-256 hex, so 64 lowercase hex digits.
fn assert_hash(revision: &str, what: &str) {
    assert_eq!(revision.len(), 64, "{what}: revision {revision:?}");
    assert!(
        revision
            .chars()
            .all(|c| c.is_ascii_hexdigit() && !c.is_uppercase()),
        "{what}: revision {revision:?} is not lowercase hex"
    );
}

/// `doc` without its `revision`, to pin the rest of an answer exactly.
fn without_revision(doc: &Value) -> Value {
    let mut out = doc.clone();
    out.as_object_mut().expect("an object").remove("revision");
    out
}

impl App {
    fn calls(&self) -> Vec<String> {
        self.log.lock().unwrap_or_else(|e| e.into_inner()).clone()
    }

    /// Nothing was asked of the manager: no connect, no call. The gate answers
    /// on its own.
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
    /// bff's own account. The pool's verbs are the manager's `As` methods, so a
    /// write asked as the bff itself is a manager that checks no group.
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

    /// The pool's text as the fake holds it now.
    fn pool(&self) -> String {
        self.fixture
            .pool
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .clone()
    }

    /// The `(profile, model, values, revision)` of the last entry save.
    fn last_save(&self) -> (String, String, Vec<(String, String)>, String) {
        self.fixture
            .last_save
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .clone()
            .expect("the entry save reached the manager")
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

    /// A request whose body is bytes no JSON reader makes anything of.
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

    /// Sign in and `GET /api/profiles/instance`.
    async fn list(&self) -> (Reply, String) {
        let cookie = self.sign_in().await;
        (
            self.get("/api/profiles/instance", Some(&cookie)).await,
            cookie,
        )
    }

    /// Sign in and `POST /api/profiles/instance`.
    async fn create(&self, body: &str) -> (Reply, String) {
        let cookie = self.sign_in().await;
        (
            self.ask_json(Method::POST, "/api/profiles/instance", Some(&cookie), body)
                .await,
            cookie,
        )
    }

    /// Sign in and `GET /api/profiles/instance/{p}`.
    async fn page(&self, profile: &str) -> (Reply, String) {
        let cookie = self.sign_in().await;
        (
            self.get(&format!("/api/profiles/instance/{profile}"), Some(&cookie))
                .await,
            cookie,
        )
    }

    /// Sign in and `DELETE /api/profiles/instance/{p}`.
    async fn delete_profile(&self, profile: &str) -> (Reply, String) {
        let cookie = self.sign_in().await;
        (
            self.ask_json(
                Method::DELETE,
                &format!("/api/profiles/instance/{profile}"),
                Some(&cookie),
                "",
            )
            .await,
            cookie,
        )
    }

    /// Sign in and `PUT /api/profiles/instance/{p}/server`.
    async fn set_server(&self, profile: &str, body: &str) -> (Reply, String) {
        let cookie = self.sign_in().await;
        (
            self.ask_json(
                Method::PUT,
                &format!("/api/profiles/instance/{profile}/server"),
                Some(&cookie),
                body,
            )
            .await,
            cookie,
        )
    }

    /// Sign in and `POST /api/profiles/instance/{p}/models`.
    async fn add_model(&self, profile: &str, body: &str) -> (Reply, String) {
        let cookie = self.sign_in().await;
        (
            self.ask_json(
                Method::POST,
                &format!("/api/profiles/instance/{profile}/models"),
                Some(&cookie),
                body,
            )
            .await,
            cookie,
        )
    }

    /// Sign in and `GET /api/profiles/instance/{p}/models/{m}`.
    async fn entry(&self, profile: &str, model: &str) -> (Reply, String) {
        let cookie = self.sign_in().await;
        (
            self.get(
                &format!("/api/profiles/instance/{profile}/models/{model}"),
                Some(&cookie),
            )
            .await,
            cookie,
        )
    }

    /// Sign in and `PUT /api/profiles/instance/{p}/models/{m}`.
    async fn save_entry(&self, profile: &str, model: &str, body: &Value) -> (Reply, String) {
        let cookie = self.sign_in().await;
        (
            self.ask_json(
                Method::PUT,
                &format!("/api/profiles/instance/{profile}/models/{model}"),
                Some(&cookie),
                &body.to_string(),
            )
            .await,
            cookie,
        )
    }

    /// Sign in and `DELETE /api/profiles/instance/{p}/models/{m}`.
    async fn delete_entry(&self, profile: &str, model: &str) -> (Reply, String) {
        let cookie = self.sign_in().await;
        (
            self.ask_json(
                Method::DELETE,
                &format!("/api/profiles/instance/{profile}/models/{model}"),
                Some(&cookie),
                "",
            )
            .await,
            cookie,
        )
    }
}

/// The routes, as the session sweeps walk them: method, path, and the body a
/// write sends (a read sends none).
const ROUTES: &[(&str, &str, Option<&str>)] = &[
    ("GET", "/api/profiles/instance", None),
    (
        "POST",
        "/api/profiles/instance",
        Some(r#"{"id": "gamma", "server": "default"}"#),
    ),
    ("GET", "/api/profiles/instance/alpha", None),
    ("DELETE", "/api/profiles/instance/alpha", None),
    (
        "PUT",
        "/api/profiles/instance/alpha/server",
        Some(r#"{"server": "big"}"#),
    ),
    (
        "POST",
        "/api/profiles/instance/alpha/models",
        Some(r#"{"model": "alpha1"}"#),
    ),
    ("GET", "/api/profiles/instance/alpha/models/alpha1", None),
    (
        "PUT",
        "/api/profiles/instance/alpha/models/alpha1",
        Some(
            r#"{"values": {"device": "cuda1"}, "revision": "0000000000000000000000000000000000000000000000000000000000000000"}"#,
        ),
    ),
    ("DELETE", "/api/profiles/instance/alpha/models/alpha1", None),
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
async fn every_instance_profile_route_needs_a_session() {
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
/// every route, and that session is over, on a read and on a write alike.
#[tokio::test]
async fn a_refused_user_is_403_and_ends_the_session_on_every_route() {
    let app = app_for(
        Fixture::default()
            .with_pool()
            .used(&[("default", Some("alpha"))])
            .refuses_everything(ClientError::NotAuthorized),
    );
    for (method, uri, body) in ROUTES {
        let cookie = app.sign_in().await;
        let reply = ask_route(&app, method, uri, *body, Some(&cookie)).await;
        reply.assert_error(StatusCode::FORBIDDEN, "session-not-administrator", &[]);
        app.session_of(&cookie).await.assert_session_required();
    }
}

/// No manager to reach: a read and a write are 503 `client-unavailable`, and
/// the session lives on.
#[tokio::test]
async fn no_manager_is_503_on_a_read_and_a_write() {
    let app = app_no_manager();
    let cookie = app.sign_in().await;
    app.get("/api/profiles/instance", Some(&cookie))
        .await
        .assert_error(StatusCode::SERVICE_UNAVAILABLE, "client-unavailable", &[]);
    app.ask_json(
        Method::PUT,
        "/api/profiles/instance/alpha/server",
        Some(&cookie),
        r#"{"server": "big"}"#,
    )
    .await
    .assert_error(StatusCode::SERVICE_UNAVAILABLE, "client-unavailable", &[]);
    app.session_of(&cookie).await.assert_signed_in();
}

/// A write that was sent and outlived the write's limit is 504
/// `client-timeout`, the outcome unknown, and the session keeps living.
#[tokio::test]
async fn a_write_sent_past_its_limit_is_504_and_keeps_the_session() {
    let app = app_for_write_timeout(
        Fixture::default()
            .with_pool()
            .used(&[("default", Some("alpha"))])
            .write_delay(Duration::from_millis(400)),
        Duration::from_millis(20),
    );
    let cookie = app.sign_in().await;
    let entry = app
        .get("/api/profiles/instance/alpha/models/alpha1", Some(&cookie))
        .await;
    assert_eq!(entry.status, StatusCode::OK, "read first: {}", entry.body);
    let body = json!({ "values": { "device": "cuda1" }, "revision": entry.revision() });
    let reply = app
        .ask_json(
            Method::PUT,
            "/api/profiles/instance/alpha/models/alpha1",
            Some(&cookie),
            &body.to_string(),
        )
        .await;
    reply.assert_error(StatusCode::GATEWAY_TIMEOUT, "client-timeout", &[]);
    app.session_of(&cookie).await.assert_signed_in();
}

// ── The pool list ────────────────────────────────────────────────────────

/// `GET /api/profiles/instance` answers `{"rows": [...]}` in the pool's order,
/// each row `{id, server, models, usedBy}`: `models` the entry model ids in the
/// file's order, `usedBy` computed by the bff from the instance rows'
/// `instanceProfile`. The pool's verbs are asked as the signed-in user, the
/// pool's file is never read.
#[tokio::test]
async fn the_list_answers_rows_in_pool_order_with_used_by() {
    let app = app_for(Fixture::default().with_pool().used(&[
        ("default", Some("alpha")),
        ("two", Some("alpha")),
        ("three", None),
        ("four", Some("beta")),
    ]));
    let (reply, _) = app.list().await;
    assert_eq!(reply.status, StatusCode::OK, "{}", reply.body);
    assert_eq!(
        reply.json(),
        json!({
            "rows": [
                {
                    "id": "alpha",
                    "server": "default",
                    "models": ["alpha1", "alpha2"],
                    "usedBy": ["default", "two"]
                },
                {
                    "id": "beta",
                    "server": "big",
                    "models": ["beta1"],
                    "usedBy": ["four"]
                }
            ]
        }),
        "{}",
        reply.body
    );
    assert_eq!(
        app.actor_of("instance-profiles"),
        Some("alice".to_string()),
        "the pool's list is one of the manager's As methods, asked as the signed-in user"
    );
    app.assert_no_call(
        "read ",
        "the rows come from InstanceProfiles, not from a file read",
    );
}

/// An empty pool answers 200 with no rows, not an error.
#[tokio::test]
async fn an_empty_pool_answers_no_rows() {
    let app = app_for(Fixture::default());
    let (reply, _) = app.list().await;
    assert_eq!(reply.status, StatusCode::OK, "{}", reply.body);
    assert_eq!(reply.json(), json!({ "rows": [] }), "{}", reply.body);
}

/// The pool's list cannot be read: 502 `client-other`, the answer of a read the
/// page cannot be given a name for.
#[tokio::test]
async fn a_pool_that_cannot_be_read_is_502() {
    let app = app_for(Fixture::default().list_fails(ClientError::Io("the file went away".into())));
    let (reply, _) = app.list().await;
    reply.assert_error(StatusCode::BAD_GATEWAY, "client-other", &[]);
}

/// The record cannot be read, so no row can say who runs it: 502 `client-other`.
#[tokio::test]
async fn a_record_that_cannot_be_read_is_502_for_the_list() {
    let app = app_for(
        Fixture::default()
            .with_pool()
            .used(&[("default", Some("alpha"))])
            .rows_fails(ClientError::Io("the record went away".into())),
    );
    let (reply, _) = app.list().await;
    reply.assert_error(StatusCode::BAD_GATEWAY, "client-other", &[]);
}

// ── The profile's page ───────────────────────────────────────────────────

/// `GET /api/profiles/instance/{p}` answers `{id, server, models, usedBy,
/// revision}`: `models` one row per entry with the count of its keys (an entry
/// with none is 0, so the page says llama.cpp's defaults), `revision` a SHA-256
/// hex of the profile's own sections. The counts and the hash come from
/// `GetInstanceProfile`'s text: the pool's file is never read.
#[tokio::test]
async fn the_page_answers_its_rows_its_users_and_its_revision() {
    let app = app_for(
        Fixture::default()
            .with_pool()
            .used(&[("default", Some("alpha")), ("two", Some("beta"))]),
    );
    let (reply, _) = app.page("alpha").await;
    assert_eq!(reply.status, StatusCode::OK, "{}", reply.body);
    let doc = reply.json();
    assert_hash(&reply.revision(), "the page's revision");
    assert_eq!(
        without_revision(&doc),
        json!({
            "id": "alpha",
            "server": "default",
            "models": [{ "id": "alpha1", "keys": 2 }, { "id": "alpha2", "keys": 0 }],
            "usedBy": ["default"]
        }),
        "{}",
        reply.body
    );
    assert_eq!(
        app.actor_of("instance-profile alpha"),
        Some("alice".to_string()),
        "the profile's read is one of the manager's As methods"
    );
    app.assert_no_call(
        "read ",
        "the counts and the hash come from GetInstanceProfile's text",
    );
}

/// A profile the pool has no section for: 404 `no-instance-profile-named`.
#[tokio::test]
async fn the_page_of_a_profile_the_pool_has_not_is_404() {
    let app = app_for(Fixture::default().with_pool());
    let (reply, _) = app.page("gamma").await;
    reply.assert_error(
        StatusCode::NOT_FOUND,
        "no-instance-profile-named",
        &["gamma"],
    );
}

// ── The profile create ───────────────────────────────────────────────────

/// `POST /api/profiles/instance` `{"id", "server"}` answers 201 `{"id"}`: no
/// instance runs the new profile, so the create answers no affected answer.
#[tokio::test]
async fn the_create_answers_201_with_the_new_id() {
    let app = app_for(Fixture::default().with_pool());
    let (reply, _) = app.create(r#"{"id": "gamma", "server": "big"}"#).await;
    assert_eq!(reply.status, StatusCode::CREATED, "{}", reply.body);
    assert_eq!(reply.json(), json!({ "id": "gamma" }), "{}", reply.body);
    assert_eq!(
        app.actor_of("create-profile gamma big"),
        Some("alice".to_string()),
        "the create is asked as the signed-in user, so the manager checks the group"
    );
    assert!(
        app.pool().contains("[gamma]"),
        "the pool holds the new profile: {}",
        app.pool()
    );
}

/// The name is not one the file can hold: the manager's
/// `instance-profile-bad-id` is 422 `invalid-instance-profile-id`, marking the
/// `id` field.
#[tokio::test]
async fn a_bad_name_is_422_invalid_instance_profile_id_on_the_id_field() {
    let app = app_for(
        Fixture::default()
            .with_pool()
            .create_fails(ClientError::Invalid(Refusal {
                key: "instance-profile-bad-id",
                values: vec!["two words".to_string()],
            })),
    );
    let (reply, _) = app
        .create(r#"{"id": "two words", "server": "default"}"#)
        .await;
    reply.assert_refused("invalid-instance-profile-id", &["two words"], "id");
}

/// A profile of that name is already there: 409 `instance-profile-name-taken`
/// with the taken id as its arg (the string fills `{0}`), marking the `id`
/// field, and nothing is written.
#[tokio::test]
async fn a_taken_name_is_409_instance_profile_name_taken_on_the_id_field() {
    let app = app_for(
        Fixture::default()
            .with_pool()
            .create_fails(ClientError::InstanceProfileExists("alpha".into())),
    );
    let (reply, _) = app.create(r#"{"id": "alpha", "server": "default"}"#).await;
    reply.assert_error_field(
        StatusCode::CONFLICT,
        "instance-profile-name-taken",
        &["alpha"],
        "id",
    );
    assert_eq!(app.pool(), pool(), "a refused create writes no profile");
}

/// The server profile the create names is not on the node: 422
/// `no-server-profile-named`, marking the `server` field.
#[tokio::test]
async fn a_server_profile_the_node_has_not_is_422_on_the_server_field() {
    let app = app_for(
        Fixture::default()
            .with_pool()
            .create_fails(ClientError::NoServerProfile("missing".into())),
    );
    let (reply, _) = app.create(r#"{"id": "gamma", "server": "missing"}"#).await;
    reply.assert_refused("no-server-profile-named", &["missing"], "server");
}

/// A create body nothing can be made of is refused as a malformed body, with
/// the manager not asked at all.
#[tokio::test]
async fn a_create_body_nothing_can_be_made_of_reaches_no_manager() {
    let app = app_for(Fixture::default().with_pool());
    let cookie = app.sign_in().await;
    let reply = app
        .ask_bytes(
            Method::POST,
            "/api/profiles/instance",
            Some(&cookie),
            "not json at all",
        )
        .await;
    reply.assert_malformed();
    app.assert_no_call("create-profile", "no create for a body that is not JSON");
}

// ── The profile delete ───────────────────────────────────────────────────

/// `DELETE /api/profiles/instance/{p}` answers 204 with no body, asked as the
/// signed-in user, and the profile is gone from the pool.
#[tokio::test]
async fn the_delete_answers_204_and_removes_the_profile() {
    let app = app_for(Fixture::default().with_pool());
    let (reply, _) = app.delete_profile("beta").await;
    assert_eq!(reply.status, StatusCode::NO_CONTENT, "{}", reply.body);
    assert_eq!(reply.body, "", "204 carries no body");
    assert_eq!(
        app.actor_of("delete-profile beta"),
        Some("alice".to_string()),
        "the removal is asked as the signed-in user"
    );
    assert_eq!(
        instance_profiles::parse(&app.pool())
            .into_iter()
            .map(|p| p.id)
            .collect::<Vec<_>>(),
        vec!["alpha"],
        "the profile is gone from the pool"
    );
}

/// An instance runs the profile: 409 `instance-profile-in-use` naming the
/// instance, and the profile stays.
#[tokio::test]
async fn a_profile_an_instance_runs_is_409_in_use_naming_the_instance() {
    let app = app_for(
        Fixture::default()
            .with_pool()
            .used(&[("default", Some("alpha"))])
            .remove_profile_fails(ClientError::InstanceProfileInUse(
                "alpha".into(),
                "default".into(),
            )),
    );
    let (reply, _) = app.delete_profile("alpha").await;
    reply.assert_error(
        StatusCode::CONFLICT,
        "instance-profile-in-use",
        &["default"],
    );
    assert!(
        app.pool().contains("[alpha]"),
        "a removal that is refused leaves the profile: {}",
        app.pool()
    );
}

/// A profile that is not there: 404 `no-instance-profile-named`.
#[tokio::test]
async fn the_delete_of_a_profile_that_is_not_there_is_404() {
    let app = app_for(Fixture::default().with_pool());
    let (reply, _) = app.delete_profile("gamma").await;
    reply.assert_error(
        StatusCode::NOT_FOUND,
        "no-instance-profile-named",
        &["gamma"],
    );
}

// ── The server profile switch ────────────────────────────────────────────

/// `PUT /api/profiles/instance/{p}/server` `{"server"}` answers the affected
/// answer exactly, with no revision on the wire: the switch is one set-like
/// change, and it is asked as the signed-in user.
#[tokio::test]
async fn the_server_switch_answers_the_affected_answer() {
    let app = app_for(Fixture::default().with_pool());
    let (reply, _) = app.set_server("alpha", r#"{"server": "big"}"#).await;
    assert_eq!(reply.status, StatusCode::OK, "{}", reply.body);
    assert_eq!(reply.json(), affected_json(), "{}", reply.body);
    assert_eq!(
        app.actor_of("set-server alpha big"),
        Some("alice".to_string()),
        "the switch is asked as the signed-in user"
    );
    assert!(
        app.pool().contains("ServerProfile = big"),
        "the pool's section names the new server profile: {}",
        app.pool()
    );
}

/// The profile is not in the pool: 404 `no-instance-profile-named`.
#[tokio::test]
async fn the_switch_of_a_profile_that_is_not_there_is_404() {
    let app = app_for(
        Fixture::default()
            .with_pool()
            .set_server_fails(ClientError::NoInstanceProfile("gamma".into())),
    );
    let (reply, _) = app.set_server("gamma", r#"{"server": "big"}"#).await;
    reply.assert_error(
        StatusCode::NOT_FOUND,
        "no-instance-profile-named",
        &["gamma"],
    );
}

/// The server profile is not on the node: 422 `no-server-profile-named`,
/// marking the `server` field. An instance the new profile does not fit is no
/// refusal: it is a row of `notConverged`.
#[tokio::test]
async fn the_switch_to_a_server_profile_the_node_has_not_is_422() {
    let app = app_for(
        Fixture::default()
            .with_pool()
            .set_server_fails(ClientError::NoServerProfile("missing".into())),
    );
    let (reply, _) = app.set_server("alpha", r#"{"server": "missing"}"#).await;
    reply.assert_refused("no-server-profile-named", &["missing"], "server");
}

/// A switch body with no `server` in it is refused as a malformed body, with
/// the manager not asked.
#[tokio::test]
async fn a_switch_body_with_no_server_reaches_no_manager() {
    let app = app_for(Fixture::default().with_pool());
    let (reply, _) = app.set_server("alpha", r#"{"nope": "big"}"#).await;
    reply.assert_malformed();
    app.assert_no_call("set-server", "no switch for a body with no server");
}

// ── The entry add ────────────────────────────────────────────────────────

/// `POST /api/profiles/instance/{p}/models` `{"model"}` answers 201 with the
/// affected answer, and the pool gains the empty entry.
#[tokio::test]
async fn the_add_answers_201_with_the_affected_answer() {
    let app = app_for(Fixture::default().with_pool());
    let (reply, _) = app.add_model("alpha", r#"{"model": "gamma"}"#).await;
    assert_eq!(reply.status, StatusCode::CREATED, "{}", reply.body);
    assert_eq!(reply.json(), affected_json(), "{}", reply.body);
    assert_eq!(
        app.actor_of("add-model alpha gamma"),
        Some("alice".to_string()),
        "the add is asked as the signed-in user"
    );
    assert!(
        app.pool().contains("[alpha/gamma]"),
        "the entry is there for the next read: {}",
        app.pool()
    );
}

/// The model profile is not in the pool: 422 `no-model-profile-named`, marking
/// the `model` field.
#[tokio::test]
async fn an_add_of_a_model_the_pool_has_not_is_422_on_the_model_field() {
    let app = app_for(
        Fixture::default()
            .with_pool()
            .add_fails(ClientError::Invalid(Refusal {
                key: "no-model-profile-named",
                values: vec!["gamma".to_string()],
            })),
    );
    let (reply, _) = app.add_model("alpha", r#"{"model": "gamma"}"#).await;
    reply.assert_refused("no-model-profile-named", &["gamma"], "model");
}

/// The profile lists that model already: 409 `instance-profile-model-listed`,
/// the manager's key, and the entry is left as it is.
#[tokio::test]
async fn an_add_of_a_model_the_profile_lists_is_409() {
    let app = app_for(
        Fixture::default()
            .with_pool()
            .add_fails(ClientError::Invalid(Refusal {
                key: "instance-profile-model-listed",
                values: vec!["alpha".to_string(), "alpha1".to_string()],
            })),
    );
    let (reply, _) = app.add_model("alpha", r#"{"model": "alpha1"}"#).await;
    reply.assert_key(StatusCode::CONFLICT, "instance-profile-model-listed");
    let (page, _) = app.page("alpha").await;
    assert_eq!(
        page.json()["models"],
        json!([{ "id": "alpha1", "keys": 2 }, { "id": "alpha2", "keys": 0 }]),
        "the refused add leaves the page alone: {}",
        page.body
    );
}

/// The profile is not in the pool: 404 `no-instance-profile-named`.
#[tokio::test]
async fn the_add_to_a_profile_that_is_not_there_is_404() {
    let app = app_for(
        Fixture::default()
            .with_pool()
            .add_fails(ClientError::NoInstanceProfile("gamma".into())),
    );
    let (reply, _) = app.add_model("gamma", r#"{"model": "alpha1"}"#).await;
    reply.assert_error(
        StatusCode::NOT_FOUND,
        "no-instance-profile-named",
        &["gamma"],
    );
}

// ── The entry read ───────────────────────────────────────────────────────

/// `GET /api/profiles/instance/{p}/models/{m}` answers `{profile, server,
/// model, values, revision}`: the entry's keys as the file holds them, the
/// profile's server profile, and the same revision the page answers for the
/// profile, both read from the same text.
#[tokio::test]
async fn the_entry_read_answers_the_entry_and_the_profiles_revision() {
    let app = app_for(Fixture::default().with_pool());
    let (page, _) = app.page("alpha").await;
    let (reply, _) = app.entry("alpha", "alpha1").await;
    assert_eq!(reply.status, StatusCode::OK, "{}", reply.body);
    let doc = reply.json();
    assert_hash(&reply.revision(), "the entry's revision");
    assert_eq!(
        without_revision(&doc),
        json!({
            "profile": "alpha",
            "server": "default",
            "model": "alpha1",
            "values": { "device": "cuda0", "n-gpu-layers": "99" }
        }),
        "{}",
        reply.body
    );
    assert_eq!(
        reply.revision(),
        page.revision(),
        "the entry's hash and the page's are of the same profile's sections"
    );
    app.assert_no_call("read ", "the entry comes from GetInstanceProfile's text");
}

/// The profile lists no such entry: 404 `no-instance-profile-model`, the
/// manager's key.
#[tokio::test]
async fn the_read_of_an_entry_the_profile_lists_not_is_404() {
    let app = app_for(
        Fixture::default()
            .with_pool()
            .profile_fails(ClientError::Invalid(Refusal {
                key: "no-instance-profile-model",
                values: vec!["alpha".to_string(), "gamma".to_string()],
            })),
    );
    let (reply, _) = app.entry("alpha", "gamma").await;
    reply.assert_key(StatusCode::NOT_FOUND, "no-instance-profile-model");
}

// ── The entry save ───────────────────────────────────────────────────────

/// `PUT /api/profiles/instance/{p}/models/{m}` `{"values", "revision"}` answers
/// exactly `{revision, running, notConverged}`: the manager is handed the entry
/// whole, with the pool text the bff just read (the manager's compare guards
/// only its own window), and the answer carries the NEW hash, so a second save
/// needs no re-read.
#[tokio::test]
async fn the_entry_save_answers_the_new_revision_and_the_affected_answer() {
    let app = app_for(Fixture::default().with_pool());
    let cookie = app.sign_in().await;
    let entry = app
        .get("/api/profiles/instance/alpha/models/alpha1", Some(&cookie))
        .await;
    assert_eq!(entry.status, StatusCode::OK, "read first: {}", entry.body);
    let sent = entry.revision();
    let before = app.pool();
    let reply = app
        .ask_json(
            Method::PUT,
            "/api/profiles/instance/alpha/models/alpha1",
            Some(&cookie),
            &json!({
                "values": { "device": "cuda1", "n-gpu-layers": "9" },
                "revision": sent
            })
            .to_string(),
        )
        .await;
    assert_eq!(reply.status, StatusCode::OK, "{}", reply.body);
    let doc = reply.json();
    assert_eq!(
        doc.get("running").map(|v| v.is_array()),
        Some(true),
        "`running` is a list: {}",
        reply.body
    );
    let written = reply.revision();
    assert_hash(&written, "the revision the save answers");
    assert_ne!(written, sent, "the save answers the NEW hash");
    assert_eq!(
        doc,
        json!({
            "revision": written,
            "running": ["default", "two"],
            "notConverged": [
                { "instance": "two", "key": "over-memory", "args": ["alpha1", "2"] }
            ]
        }),
        "{}",
        reply.body
    );
    let (profile, model, mut values, revision) = app.last_save();
    assert_eq!(profile, "alpha", "the save's profile");
    assert_eq!(model, "alpha1", "the save's model");
    values.sort();
    assert_eq!(
        values,
        vec![
            ("device".to_string(), "cuda1".to_string()),
            ("n-gpu-layers".to_string(), "9".to_string())
        ],
        "the entry whole, as the page sent it"
    );
    assert_eq!(
        revision, before,
        "the manager is handed the pool text the bff read, not the hash"
    );
    let (again, _) = app.entry("alpha", "alpha1").await;
    assert_eq!(
        again.revision(),
        written,
        "a second save needs no re-read: the hash the save answers is the new one"
    );
}

/// A save whose revision is not the pool's own hash is 409 `stale-revision`,
/// read again, and no write is asked.
#[tokio::test]
async fn a_stale_revision_is_409_and_asks_no_write() {
    let app = app_for(Fixture::default().with_pool());
    let (reply, _) = app
        .save_entry(
            "alpha",
            "alpha1",
            &json!({
                "values": { "device": "cuda1" },
                "revision": "0000000000000000000000000000000000000000000000000000000000000000"
            }),
        )
        .await;
    reply.assert_error(StatusCode::CONFLICT, "stale-revision", &[]);
    app.assert_no_call("set-entry", "a stale edit writes nothing");
    assert_eq!(
        app.pool(),
        pool(),
        "a stale edit writes nothing to the pool"
    );
}

/// The pool's text moved between the bff's read and its write: the manager's
/// own compare is 409 `stale-revision` too.
#[tokio::test]
async fn a_pool_that_moved_under_the_save_is_409_stale_revision() {
    let app = app_for(
        Fixture::default()
            .with_pool()
            .set_entry_fails(ClientError::Changed),
    );
    let (entry, _) = app.entry("alpha", "alpha1").await;
    let (reply, _) = app
        .save_entry(
            "alpha",
            "alpha1",
            &json!({ "values": { "device": "cuda1" }, "revision": entry.revision() }),
        )
        .await;
    reply.assert_error(StatusCode::CONFLICT, "stale-revision", &[]);
}

/// An edit of another profile leaves a profile's hash alone: the hash is of
/// that profile's own sections, so the page and the entry of `beta` answer the
/// same hash before and after `alpha`'s entry is saved.
#[tokio::test]
async fn an_edit_of_another_profile_leaves_the_hash_alone() {
    let app = app_for(Fixture::default().with_pool());
    let cookie = app.sign_in().await;
    let beta_entry = app
        .get("/api/profiles/instance/beta/models/beta1", Some(&cookie))
        .await;
    assert_eq!(
        beta_entry.status,
        StatusCode::OK,
        "beta's entry reads: {}",
        beta_entry.body
    );
    let beta_page = app.get("/api/profiles/instance/beta", Some(&cookie)).await;
    let before = beta_entry.revision();
    assert_hash(&before, "beta's revision");
    assert_eq!(beta_page.revision(), before, "the page and the entry agree");
    let alpha_entry = app
        .get("/api/profiles/instance/alpha/models/alpha1", Some(&cookie))
        .await;
    let alpha_before = alpha_entry.revision();
    let saved = app
        .ask_json(
            Method::PUT,
            "/api/profiles/instance/alpha/models/alpha1",
            Some(&cookie),
            &json!({
                "values": { "device": "cuda1", "n-gpu-layers": "9" },
                "revision": alpha_before
            })
            .to_string(),
        )
        .await;
    assert_eq!(saved.status, StatusCode::OK, "alpha saved: {}", saved.body);
    let (page, _) = app.page("beta").await;
    assert_eq!(
        page.revision(),
        before,
        "beta's hash is untouched by an edit of alpha: {} -> {}",
        before,
        page.revision()
    );
    let (entry, _) = app.entry("beta", "beta1").await;
    assert_eq!(entry.revision(), before, "and the entry's with it");
    assert_ne!(
        saved.revision(),
        alpha_before,
        "alpha's own hash moved: {}",
        saved.body
    );
    let (alpha_page, _) = app.page("alpha").await;
    assert_eq!(
        alpha_page.revision(),
        saved.revision(),
        "and the page sees the new one"
    );
}

/// The profile lists no such entry: 404 `no-instance-profile-model`.
#[tokio::test]
async fn the_save_of_an_entry_the_profile_lists_not_is_404() {
    let app = app_for(
        Fixture::default()
            .with_pool()
            .set_entry_fails(ClientError::Invalid(Refusal {
                key: "no-instance-profile-model",
                values: vec!["alpha".to_string(), "gamma".to_string()],
            })),
    );
    let (entry, _) = app.entry("alpha", "alpha1").await;
    let (reply, _) = app
        .save_entry(
            "alpha",
            "gamma",
            &json!({ "values": { "device": "cuda1" }, "revision": entry.revision() }),
        )
        .await;
    reply.assert_key(StatusCode::NOT_FOUND, "no-instance-profile-model");
}

/// A key of the model itself is no entry key: 422 `model-profile-stay-key`,
/// naming the key and marking its field.
#[tokio::test]
async fn a_key_of_the_model_itself_is_422_model_profile_stay_key() {
    let app = app_for(
        Fixture::default()
            .with_pool()
            .set_entry_fails(ClientError::Invalid(Refusal {
                key: "model-profile-stay-key",
                values: vec!["model".to_string()],
            })),
    );
    let (entry, _) = app.entry("alpha", "alpha1").await;
    let (reply, _) = app
        .save_entry(
            "alpha",
            "alpha1",
            &json!({ "values": { "model": "/models/x.gguf" }, "revision": entry.revision() }),
        )
        .await;
    reply.assert_refused("model-profile-stay-key", &["model"], "model");
}

/// A value the entry checks refuse reaches the page as the manager's own 422:
/// the key and the args are the manager's, the status says a check refused it.
#[tokio::test]
async fn a_value_the_manager_refuses_is_422_with_its_own_key() {
    for (key, values) in [
        (
            "not-whole-number-in-range",
            vec!["n-gpu-layers", "9000", "0", "1024"],
        ),
        ("tensor-split-format", vec!["1,2,3,4,5,6,7,8,9,0,11"]),
        ("override-no-device", vec!["0=1", "*"]),
        (
            "not-one-of",
            vec!["moe-layer_freq", "sometimes", "always, never"],
        ),
    ] {
        let app = app_for(
            Fixture::default()
                .with_pool()
                .set_entry_fails(ClientError::Invalid(Refusal {
                    key,
                    values: values.iter().map(|v| v.to_string()).collect(),
                })),
        );
        let (entry, _) = app.entry("alpha", "alpha1").await;
        let (reply, _) = app
            .save_entry(
                "alpha",
                "alpha1",
                &json!({ "values": { "device": "cuda1" }, "revision": entry.revision() }),
            )
            .await;
        reply.assert_key(StatusCode::UNPROCESSABLE_ENTITY, key);
    }
}

/// A body that carries no revision is 422 `model-profiles-malformed`, and the
/// manager is asked to write nothing.
#[tokio::test]
async fn a_save_with_no_revision_is_422_malformed_and_writes_nothing() {
    let app = app_for(Fixture::default().with_pool());
    let (reply, _) = app
        .save_entry(
            "alpha",
            "alpha1",
            &json!({ "values": { "device": "cuda1" } }),
        )
        .await;
    reply.assert_error(
        StatusCode::UNPROCESSABLE_ENTITY,
        "model-profiles-malformed",
        &[],
    );
    app.assert_no_call("set-entry", "no write for a body with no revision");
    assert_eq!(app.pool(), pool(), "a body with no revision writes nothing");
}

/// A save body nothing can be made of is refused as a malformed body, with the
/// manager asked to write nothing.
#[tokio::test]
async fn a_save_body_nothing_can_be_made_of_writes_nothing() {
    let app = app_for(Fixture::default().with_pool());
    let cookie = app.sign_in().await;
    let reply = app
        .ask_bytes(
            Method::PUT,
            "/api/profiles/instance/alpha/models/alpha1",
            Some(&cookie),
            "not json at all",
        )
        .await;
    reply.assert_malformed();
    app.assert_no_call("set-entry", "no write for bytes that are not JSON");
}

// ── The entry delete ─────────────────────────────────────────────────────

/// `DELETE /api/profiles/instance/{p}/models/{m}` answers the affected answer,
/// asked as the signed-in user, and the pool loses the entry.
#[tokio::test]
async fn the_entry_delete_answers_the_affected_answer() {
    let app = app_for(Fixture::default().with_pool());
    let (reply, _) = app.delete_entry("alpha", "alpha1").await;
    assert_eq!(reply.status, StatusCode::OK, "{}", reply.body);
    assert_eq!(reply.json(), affected_json(), "{}", reply.body);
    assert_eq!(
        app.actor_of("remove-model alpha alpha1"),
        Some("alice".to_string()),
        "the entry removal is asked as the signed-in user"
    );
    let (page, _) = app.page("alpha").await;
    assert_eq!(
        page.json()["models"],
        json!([{ "id": "alpha2", "keys": 0 }]),
        "the entry is gone from the page: {}",
        page.body
    );
}

/// The profile lists no such entry: 404 `no-instance-profile-model`.
#[tokio::test]
async fn the_delete_of_an_entry_the_profile_lists_not_is_404() {
    let app = app_for(
        Fixture::default()
            .with_pool()
            .remove_entry_fails(ClientError::Invalid(Refusal {
                key: "no-instance-profile-model",
                values: vec!["alpha".to_string(), "gamma".to_string()],
            })),
    );
    let (reply, _) = app.delete_entry("alpha", "gamma").await;
    reply.assert_key(StatusCode::NOT_FOUND, "no-instance-profile-model");
    assert!(
        app.pool().contains("[alpha/alpha1]"),
        "a refused removal leaves the entries: {}",
        app.pool()
    );
}
