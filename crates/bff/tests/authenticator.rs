//! `SystemAuthenticator`: the bff's real authenticator, tested through
//! the `ManagerClient` seam: the manager is a fake client opened through
//! the same `Connect` the bff's other routes use (never PAM, LogonUser,
//! the D-Bus bus or the named pipe). The routes' mapping of the outcomes
//! (403, 401, 503, lockout) is in `tests/session.rs`; this file pins
//! only the mapping of the manager's answers into `Identity` /
//! `AuthError`.
//!
//! The mapping (fe/briefs/api-contract.md, crates/bff/README.md):
//! - `Ok((true, true))`: signed in: the `Identity` of the typed name,
//!   `can_configure` true (the session reply keeps `canConfigure` while
//!   the web UI still reads it).
//! - `Ok((true, false))`: `AuthError::NotAdministrator`, the route's 403
//!   `session-not-administrator`.
//! - `Ok((false, _))`: `AuthError::BadCredentials`, the route's 401,
//!   whatever the second answer says: a wrong password is never a
//!   sign-in, group membership cannot rescue it. It counts for the
//!   lockout, unlike the 403 (the route tests pin that rule).
//! - `Err(ClientError)`, any variant: `AuthError::Unavailable`, the
//!   route's 503; `AuthError` carries no text, so the manager's detail
//!   cannot ride along to the browser.
//! - One client per check, as `/api/instances` opens one per request.
//! - The password travels to the manager and nowhere else: the fake
//!   records the pair it was handed, so the test pins the typed pair
//!   arrives whole (the route tests pin that no reply carries it).

use pai_bff::session::{AuthError, Authenticator, Identity, SystemAuthenticator};
use pai_bff::{Client, Connect};
use pai_core::client::{ClientError, InstanceState, ManagerClient, ManagerFile};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};

/// The (user, password) pairs the fake managers were handed, shared by
/// every client one harness opens.
type Pairs = Arc<Mutex<Vec<(String, String)>>>;

/// A manager that answers every `authenticate` with `answer`, recording
/// the pair; everything else is an answer no test reads.
struct FakeManager {
    answer: Result<(bool, bool), ClientError>,
    pairs: Pairs,
}

impl ManagerClient for FakeManager {
    fn list_instances(&self) -> Result<Vec<String>, ClientError> {
        Ok(vec![])
    }
    fn status(&self, _id: &str) -> Result<InstanceState, ClientError> {
        Ok(InstanceState::Inactive)
    }
    fn start(&self, _id: &str) -> Result<(), ClientError> {
        Ok(())
    }
    fn stop(&self, _id: &str) -> Result<(), ClientError> {
        Ok(())
    }
    fn restart(&self, _id: &str) -> Result<(), ClientError> {
        Ok(())
    }
    fn read_file(&self, _name: &str) -> Result<Option<ManagerFile>, ClientError> {
        Ok(None)
    }
    fn write_file(
        &self,
        _name: &str,
        _contents: &str,
        _keep_api_key: bool,
    ) -> Result<(), ClientError> {
        Ok(())
    }
    fn boot_enabled(&self, _id: &str) -> Result<bool, ClientError> {
        Ok(false)
    }
    fn set_boot_enabled(&self, _id: &str, _enabled: bool) -> Result<(), ClientError> {
        Ok(())
    }
    fn authenticate(&self, user: &str, password: &str) -> Result<(bool, bool), ClientError> {
        self.pairs
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .push((user.to_string(), password.to_string()));
        self.answer.clone()
    }
}

/// A `SystemAuthenticator` over a manager that answers `answer`,
/// counting the connects.
struct Harness {
    auth: SystemAuthenticator,
    pairs: Pairs,
    connects: Arc<AtomicUsize>,
}

fn harness(answer: Result<(bool, bool), ClientError>) -> Harness {
    let pairs: Pairs = Arc::new(Mutex::new(Vec::new()));
    let connects = Arc::new(AtomicUsize::new(0));
    let connect: Connect = {
        let (pairs, connects) = (pairs.clone(), connects.clone());
        Arc::new(move || {
            connects.fetch_add(1, Ordering::SeqCst);
            Ok(Box::new(FakeManager {
                answer: answer.clone(),
                pairs: pairs.clone(),
            }) as Client)
        })
    };
    Harness {
        auth: SystemAuthenticator::new(connect),
        pairs,
        connects,
    }
}

fn pairs_of(h: &Harness) -> Vec<(String, String)> {
    h.pairs.lock().unwrap_or_else(|e| e.into_inner()).clone()
}

/// `Ok((true, true))`: signed in as the typed account, an administrator.
#[test]
fn a_member_with_the_right_password_signs_in() {
    let h = harness(Ok((true, true)));
    assert_eq!(
        h.auth.authenticate("alice", "correct horse"),
        Ok(Identity {
            user: "alice".into(),
            can_configure: true,
        })
    );
    // The manager is asked the typed pair, whole; the reply tests (in
    // tests/session.rs) pin that the password is in no reply.
    assert_eq!(
        pairs_of(&h),
        vec![("alice".to_string(), "correct horse".to_string())]
    );
}

/// `Ok((true, false))`: the password was right, the account is not a
/// member of the `pai` group: NotAdministrator, the route's 403.
#[test]
fn a_right_password_without_the_group_is_not_an_administrator() {
    let h = harness(Ok((true, false)));
    assert_eq!(
        h.auth.authenticate("carol", "plain carol"),
        Err(AuthError::NotAdministrator)
    );
}

/// `Ok((false, _))`: the route's 401, whatever the second answer says:
/// the mapping never consults the group of an account whose password was
/// wrong.
#[test]
fn a_wrong_password_is_bad_credentials_whatever_the_second_answer() {
    for can_configure in [false, true] {
        let h = harness(Ok((false, can_configure)));
        assert_eq!(
            h.auth.authenticate("alice", "wrong"),
            Err(AuthError::BadCredentials),
            "second answer {can_configure}"
        );
    }
}

/// Any `Err(ClientError)` is Unavailable, the route's 503: the manager
/// unreachable, the bff's own account refused (today the manager refuses
/// it `control`), and any other failure.
#[test]
fn any_manager_failure_is_unavailable() {
    for error in [
        ClientError::Unavailable,
        ClientError::NotAuthorized,
        ClientError::InvalidId,
        ClientError::NoInstance,
        ClientError::BadFileName,
        ClientError::Io("PermissionDenied".into()),
        ClientError::Service("unit failed".into()),
        ClientError::Other("org.example.Boom: /some/path".into()),
    ] {
        let label = format!("{error:?}");
        let h = harness(Err(error));
        assert_eq!(
            h.auth.authenticate("alice", "x"),
            Err(AuthError::Unavailable),
            "{label}"
        );
    }
}

/// No manager to connect to: the same, from the connect itself (no
/// client is ever made).
#[test]
fn no_manager_to_connect_to_is_unavailable() {
    let connect: Connect = Arc::new(|| Err(ClientError::Unavailable));
    let auth = SystemAuthenticator::new(connect);
    assert_eq!(auth.authenticate("alice", "x"), Err(AuthError::Unavailable));
}

/// One client per check, as `/api/instances` opens one per request: a
/// manager started after a failed sign-in is seen by the next one.
#[test]
fn a_client_is_opened_for_each_check() {
    let h = harness(Ok((true, true)));
    for _ in 0..2 {
        h.auth
            .authenticate("alice", "correct horse")
            .expect("signed in");
    }
    assert_eq!(
        h.connects.load(Ordering::SeqCst),
        2,
        "one connect per check"
    );
}
