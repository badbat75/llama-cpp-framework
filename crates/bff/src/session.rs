//! Signing in to the web UI: the authenticator seam the session routes call
//! (`POST`, `GET`, `DELETE /api/session`).

use axum::extract::{ConnectInfo, State};
use axum::http::{header, HeaderMap, StatusCode};
use axum::response::{IntoResponse, Response};
use std::collections::HashMap;
use std::net::{IpAddr, SocketAddr};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

/// Who signed in, as the authenticator names them: the account name it
/// canonicalised and whether the manager would allow them `configure`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Identity {
    pub user: String,
    pub can_configure: bool,
}

/// Why a sign-in was refused.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AuthError {
    /// No such account, or the wrong password: the two are not told apart.
    BadCredentials,
    /// The system could not check the credentials at all.
    Unavailable,
}

/// Checks an account's name and password against the operating system.
pub trait Authenticator: Send + Sync {
    fn authenticate(&self, user: &str, password: &str) -> Result<Identity, AuthError>;
}

/// The time the session routes read: `Instant::now` in the service, a
/// clock the test moves in the tests.
pub type Clock = std::sync::Arc<dyn Fn() -> std::time::Instant + Send + Sync>;

/// The session cookie's name.
const COOKIE: &str = "pai_session";

/// A session ends after this long without a request that uses it.
const IDLE: Duration = Duration::from_secs(30 * 60);

/// This many failed sign-ins for one user from one client address within
/// [`WINDOW`] lock that pair for [`WINDOW`] from the last of them.
const MAX_FAILURES: usize = 5;
const WINDOW: Duration = Duration::from_secs(60);

/// The random bytes of a session id.
const ID_BYTES: usize = 32;

/// The longest user name accepted, in bytes.
const MAX_USER_BYTES: usize = 256;

/// The session routes' state: the signed-in sessions and the failed
/// sign-ins, in memory only (a restart signs everyone out).
pub(crate) struct Sessions {
    auth: Arc<dyn Authenticator>,
    clock: Clock,
    state: Mutex<Store>,
}

#[derive(Default)]
struct Store {
    /// By session id: who, and when the session was last used.
    sessions: HashMap<String, (Identity, Instant)>,
    /// By (user name in lowercase, client address): the recent failures,
    /// the end of the lock they caused and the sign-ins still being checked.
    attempts: HashMap<(String, IpAddr), Attempts>,
}

#[derive(Default)]
struct Attempts {
    failures: Vec<Instant>,
    locked_until: Option<Instant>,
    /// Sign-ins the authenticator has not answered yet: each counts as a
    /// failure until it answers, so guesses sent at once cannot overrun the
    /// lock.
    in_flight: usize,
}

impl Attempts {
    /// Whether the entry still holds anything at `now`.
    fn live(&self, now: Instant) -> bool {
        self.in_flight > 0
            || self.locked_until.is_some_and(|until| until > now)
            || self
                .failures
                .iter()
                .any(|t| now.saturating_duration_since(*t) < WINDOW)
    }
}

/// A sign-in being checked, holding its slot in [`Attempts::in_flight`];
/// dropping it gives the slot back, also when the request is abandoned.
struct InFlight {
    sessions: Arc<Sessions>,
    key: (String, IpAddr),
}

impl Drop for InFlight {
    fn drop(&mut self) {
        let now = (self.sessions.clock)();
        let mut store = self.sessions.store();
        if let Some(attempts) = store.attempts.get_mut(&self.key) {
            attempts.in_flight = attempts.in_flight.saturating_sub(1);
            if !attempts.live(now) {
                store.attempts.remove(&self.key);
            }
        }
    }
}

impl Sessions {
    /// A poisoned lock is taken over: every write leaves the store whole.
    fn store(&self) -> std::sync::MutexGuard<'_, Store> {
        self.state.lock().unwrap_or_else(|e| e.into_inner())
    }

    /// The identity of the session the request's cookie names, its idle
    /// deadline moved; an expired session is removed.
    pub(crate) fn current(&self, headers: &HeaderMap) -> Option<(String, Identity)> {
        let id = cookie_value(headers)?;
        let now = (self.clock)();
        let mut store = self.store();
        let (identity, used) = store.sessions.get_mut(&id)?;
        if now.saturating_duration_since(*used) > IDLE {
            store.sessions.remove(&id);
            return None;
        }
        *used = now;
        let identity = identity.clone();
        Some((id, identity))
    }
}

/// No session yet, signing in through `auth` and reading the time from
/// `clock`.
pub(crate) fn sessions(auth: Arc<dyn Authenticator>, clock: Clock) -> Arc<Sessions> {
    Arc::new(Sessions {
        auth,
        clock,
        state: Mutex::new(Store::default()),
    })
}

/// `POST`, `GET` and `DELETE /api/session` over `state`.
pub(crate) fn routes(state: Arc<Sessions>) -> axum::Router {
    axum::Router::new()
        .route(
            "/api/session",
            axum::routing::post(sign_in).get(read).delete(sign_out),
        )
        .with_state(state)
}

/// `POST /api/session` `{"username", "password"}`.
async fn sign_in(
    State(s): State<Arc<Sessions>>,
    ConnectInfo(client): ConnectInfo<SocketAddr>,
    headers: HeaderMap,
    body: axum::body::Bytes,
) -> Response {
    let Some((user, password)) = credentials(&headers, &body) else {
        return error(StatusCode::BAD_REQUEST, "session-malformed", &[]);
    };
    let key = (user.to_lowercase(), client.ip());
    let now = (s.clock)();
    {
        let mut store = s.store();
        if !store.attempts.contains_key(&key) {
            store.attempts.retain(|_, a| a.live(now));
        }
        let attempts = store.attempts.entry(key.clone()).or_default();
        attempts
            .failures
            .retain(|t| now.saturating_duration_since(*t) < WINDOW);
        // Locked, or as many guesses pending as would lock it: the end of
        // the lock, or of the one the pending guesses would cause.
        let until = match attempts.locked_until.filter(|until| *until > now) {
            Some(until) => Some(until),
            None if attempts.failures.len() + attempts.in_flight >= MAX_FAILURES => {
                Some(now + WINDOW)
            }
            None => None,
        };
        if let Some(until) = until {
            let left = until - now;
            let secs = left.as_secs() + u64::from(left.subsec_nanos() > 0);
            return error(
                StatusCode::TOO_MANY_REQUESTS,
                "session-too-many-attempts",
                &[secs],
            );
        }
        attempts.in_flight += 1;
    }
    let slot = InFlight {
        sessions: s.clone(),
        key: key.clone(),
    };
    let answer = if password.is_empty() {
        Err(AuthError::BadCredentials)
    } else {
        let auth = s.auth.clone();
        let typed = user.clone();
        tokio::task::spawn_blocking(move || auth.authenticate(&typed, &password))
            .await
            .unwrap_or(Err(AuthError::Unavailable))
    };
    match answer {
        Ok(identity) => {
            let mut bytes = [0u8; ID_BYTES];
            if getrandom::fill(&mut bytes).is_err() {
                return error(StatusCode::SERVICE_UNAVAILABLE, "session-unavailable", &[]);
            }
            let id: String = bytes.iter().map(|b| format!("{b:02x}")).collect();
            let now = (s.clock)();
            let body = identity_json(&identity);
            let mut store = s.store();
            if let Some(attempts) = store.attempts.get_mut(&key) {
                attempts.failures.clear();
                attempts.locked_until = None;
            }
            store
                .sessions
                .retain(|_, (_, used)| now.saturating_duration_since(*used) <= IDLE);
            store.sessions.insert(id.clone(), (identity, now));
            (
                [(
                    header::SET_COOKIE,
                    format!("{COOKIE}={id}; HttpOnly; SameSite=Strict; Path=/"),
                )],
                body,
            )
                .into_response()
        }
        Err(AuthError::BadCredentials) => {
            let now = (s.clock)();
            let mut store = s.store();
            let attempts = store.attempts.entry(key).or_default();
            attempts
                .failures
                .retain(|t| now.saturating_duration_since(*t) < WINDOW);
            attempts.failures.push(now);
            if attempts.failures.len() >= MAX_FAILURES {
                attempts.failures.clear();
                attempts.locked_until = Some(now + WINDOW);
            }
            drop(store);
            drop(slot);
            error(StatusCode::UNAUTHORIZED, "session-bad-credentials", &[])
        }
        Err(AuthError::Unavailable) => {
            error(StatusCode::SERVICE_UNAVAILABLE, "session-unavailable", &[])
        }
    }
}

/// `GET /api/session`: who is signed in.
async fn read(State(s): State<Arc<Sessions>>, headers: HeaderMap) -> Response {
    match s.current(&headers) {
        Some((_, identity)) => identity_json(&identity).into_response(),
        None => error(StatusCode::UNAUTHORIZED, "session-required", &[]),
    }
}

/// `DELETE /api/session`: the session ended and its cookie cleared.
async fn sign_out(State(s): State<Arc<Sessions>>, headers: HeaderMap) -> Response {
    match s.current(&headers) {
        Some((id, _)) => {
            s.store().sessions.remove(&id);
            (
                StatusCode::NO_CONTENT,
                [(
                    header::SET_COOKIE,
                    format!("{COOKIE}=; HttpOnly; SameSite=Strict; Path=/; Max-Age=0"),
                )],
            )
                .into_response()
        }
        None => error(StatusCode::UNAUTHORIZED, "session-required", &[]),
    }
}

/// The user name and password of a JSON body `{"username": <non-empty
/// text>, "password": <text>}`, or None.
fn credentials(headers: &HeaderMap, body: &[u8]) -> Option<(String, String)> {
    let content_type = headers.get(header::CONTENT_TYPE)?.to_str().ok()?;
    let essence = content_type.split(';').next().unwrap_or("").trim();
    if !essence.eq_ignore_ascii_case("application/json") {
        return None;
    }
    let value: serde_json::Value = serde_json::from_slice(body).ok()?;
    let user = value.get("username")?.as_str()?;
    let password = value.get("password")?.as_str()?;
    if user.is_empty() || user.len() > MAX_USER_BYTES {
        return None;
    }
    Some((user.to_string(), password.to_string()))
}

/// The value of the session cookie among the request's cookies.
fn cookie_value(headers: &HeaderMap) -> Option<String> {
    headers
        .get_all(header::COOKIE)
        .iter()
        .filter_map(|v| v.to_str().ok())
        .flat_map(|v| v.split(';'))
        .filter_map(|pair| pair.trim().split_once('='))
        .find(|(name, _)| *name == COOKIE)
        .map(|(_, value)| value.to_string())
}

fn identity_json(identity: &Identity) -> axum::Json<serde_json::Value> {
    axum::Json(serde_json::json!({
        "user": identity.user,
        "canConfigure": identity.can_configure,
    }))
}

/// `{"key": <strings key>, "args": [...]}` with `status`.
pub(crate) fn error(status: StatusCode, key: &str, args: &[u64]) -> Response {
    (
        status,
        axum::Json(serde_json::json!({ "key": key, "args": args })),
    )
        .into_response()
}

/// The system's authenticator. The operating system's check (PAM, LogonUser)
/// is not wired yet, so every sign-in is unavailable; under the test
/// redirect (`PRIVATE_AI_DATA_ROOT`) it must stay so, never asking the real
/// system.
pub(crate) struct SystemAuthenticator;

impl Authenticator for SystemAuthenticator {
    fn authenticate(&self, _user: &str, _password: &str) -> Result<Identity, AuthError> {
        Err(AuthError::Unavailable)
    }
}
