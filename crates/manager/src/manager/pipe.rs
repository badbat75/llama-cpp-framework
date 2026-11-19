//! The manager over a byte stream: the face of `pai-manager` on Windows,
//! where the configurator reaches it over a local named pipe. OS-neutral:
//! the frame format and the requests are `pai_core::client::pipe`'s (its
//! module header), the errors the same (name, message) pairs the D-Bus
//! interface replies with. The guarded methods answer in their `As` form
//! too, with the principal the caller vouches for; `Version` answers the
//! framework version and [`PROTOCOL`] to anybody. The pipe itself, its
//! security descriptor and the caller's identity are the Windows side's.

pub use super::service::error_reply;
use super::service::{
    state_name, Authority, Caller, Manager, ManagerError, Principal, Requester, Resolver,
    ServiceControl,
};
use pai_core::client::pipe::{read_frame, write_frame, FrameError};
use serde_json::{json, Value};
use std::io::{Error, ErrorKind, Read, Write};

/// The error name of the reply to a frame that is no request (not JSON, an
/// unknown method, a missing argument, over `MAX_FRAME`); the connection
/// ends after it.
pub const BAD_REQUEST: &str = "org.privateai.Manager1.Error.BadRequest";

/// The protocol version this manager answers with, the number `Version`
/// reports beside the framework version. 2 is the `As` methods; 3 adds
/// `SetModelProfile`, the assignment verb, with its `As` form.
pub const PROTOCOL: u32 = 3;

/// The suffix of the `As` form of a guarded method, the one that carries a
/// `principal`.
const AS_SUFFIX: &str = "As";

/// A request, its arguments read (`pai_core::client::pipe`'s header). The
/// guarded methods carry the principal the caller vouches for, None when
/// the call is the caller's own.
enum Request {
    ListInstances,
    Status(String),
    Version,
    Start(String, Option<Principal>),
    Stop(String, Option<Principal>),
    Restart(String, Option<Principal>),
    ReadFile(String, Option<Principal>),
    WriteFile(String, String, bool, Option<Principal>),
    WriteFileIf(String, String, String, Option<Principal>),
    BootEnabled(String),
    SetBootEnabled(String, bool, Option<Principal>),
    SetModelProfile(String, String, String, Option<Principal>),
    Authenticate(String, String),
}

impl Request {
    /// The request a frame's body holds, None when it holds none.
    fn parse(body: &[u8]) -> Option<Request> {
        let v: Value = serde_json::from_slice(body).ok()?;
        let method = v.get("method")?.as_str()?;
        // The principal travels only on the `As` form, and always: a method
        // and a field that do not agree is no request. Every other unknown
        // field stays ignored, as a client of an older protocol expects.
        let as_form = method.ends_with(AS_SUFFIX);
        if v.get("principal").is_some() != as_form {
            return None;
        }
        let arg = |name: &str| v.get(name)?.as_str().map(str::to_string);
        // A missing principal, or one that is not a string, is no request (the
        // `?` at the use); present, it is the `Some` the `As` request carries.
        let principal = || Some(Principal(v.get("principal")?.as_str()?.to_string()));
        Some(match method {
            "ListInstances" => Request::ListInstances,
            "Status" => Request::Status(arg("id")?),
            "Version" => Request::Version,
            "Start" => Request::Start(arg("id")?, None),
            "StartAs" => Request::Start(arg("id")?, principal().map(Some)?),
            "Stop" => Request::Stop(arg("id")?, None),
            "StopAs" => Request::Stop(arg("id")?, principal().map(Some)?),
            "Restart" => Request::Restart(arg("id")?, None),
            "RestartAs" => Request::Restart(arg("id")?, principal().map(Some)?),
            "ReadFile" => Request::ReadFile(arg("name")?, None),
            "ReadFileAs" => Request::ReadFile(arg("name")?, principal().map(Some)?),
            "WriteFile" => Request::WriteFile(
                arg("name")?,
                arg("contents")?,
                v.get("keep_api_key")?.as_bool()?,
                None,
            ),
            "WriteFileAs" => Request::WriteFile(
                arg("name")?,
                arg("contents")?,
                v.get("keep_api_key")?.as_bool()?,
                principal().map(Some)?,
            ),
            // The compare and swap names no `keep_api_key`: the caller holds
            // the file's own text, key line included, and sends it back.
            "WriteFileIf" => {
                Request::WriteFileIf(arg("name")?, arg("contents")?, arg("expected")?, None)
            }
            "WriteFileIfAs" => Request::WriteFileIf(
                arg("name")?,
                arg("contents")?,
                arg("expected")?,
                principal().map(Some)?,
            ),
            "BootEnabled" => Request::BootEnabled(arg("id")?),
            "SetBootEnabled" => {
                Request::SetBootEnabled(arg("id")?, v.get("enabled")?.as_bool()?, None)
            }
            "SetBootEnabledAs" => Request::SetBootEnabled(
                arg("id")?,
                v.get("enabled")?.as_bool()?,
                principal().map(Some)?,
            ),
            // The assignment verb: instance, model profile and value, the
            // names the method's own (the frame's fields are camelCase, as
            // every other method's).
            "SetModelProfile" => {
                Request::SetModelProfile(arg("instance")?, arg("profile")?, arg("value")?, None)
            }
            "SetModelProfileAs" => Request::SetModelProfile(
                arg("instance")?,
                arg("profile")?,
                arg("value")?,
                principal().map(Some)?,
            ),
            "Authenticate" => Request::Authenticate(arg("user")?, arg("password")?),
            _ => return None,
        })
    }
}

/// The requester of a guarded call: the caller, and the principal the
/// request vouches for when it is an `As` method. A caller with no identity
/// and no principal is refused before anybody is asked, as a D-Bus call
/// without a sender is (`dbus::caller`); with a principal, the caller's
/// identity is the core's business (only the bff vouches,
/// `Manager::check_principal`).
fn requester(caller: &Caller, principal: Option<Principal>) -> Result<Requester, ManagerError> {
    if principal.is_none() && caller.0.is_empty() {
        Err(ManagerError::NotAuthorized)
    } else {
        Ok(Requester {
            caller: caller.clone(),
            principal,
        })
    }
}

/// The reply to `request`, run through `manager` as `caller`.
fn answer<S, A, R>(manager: &Manager<S, A, R>, caller: &Caller, request: Request) -> Value
where
    S: ServiceControl,
    A: Authority,
    R: Resolver,
{
    let result = match request {
        Request::ListInstances => Ok(json!(manager.list_instances())),
        Request::Status(id) => manager.status(&id).map(|s| json!(state_name(s))),
        // The version asks nothing of anybody: every caller gets it, the
        // empty one included.
        Request::Version => Ok(json!({"version": env!("CARGO_PKG_VERSION"), "protocol": PROTOCOL})),
        Request::Start(id, principal) => requester(caller, principal)
            .and_then(|who| manager.start(&who, &id))
            .map(|()| Value::Null),
        Request::Stop(id, principal) => requester(caller, principal)
            .and_then(|who| manager.stop(&who, &id))
            .map(|()| Value::Null),
        Request::Restart(id, principal) => requester(caller, principal)
            .and_then(|who| manager.restart(&who, &id))
            .map(|()| Value::Null),
        Request::ReadFile(name, principal) => manager
            .read_file(requester(caller, principal).ok().as_ref(), &name)
            .map(|file| json!(file.map(|f| json!({"text": f.text, "withheld": f.withheld})))),
        Request::WriteFile(name, contents, keep, principal) => requester(caller, principal)
            .and_then(|who| manager.write_file(&who, &name, &contents, keep))
            .map(|()| Value::Null),
        // The compare and swap: `expected` is the text the caller read, and
        // the core answers `Changed` when the file no longer holds it.
        Request::WriteFileIf(name, contents, expected, principal) => requester(caller, principal)
            .and_then(|who| manager.write_file_if(&who, &name, &contents, &expected))
            .map(|()| Value::Null),
        Request::BootEnabled(id) => manager.boot_enabled(&id).map(|on| json!(on)),
        Request::SetBootEnabled(id, enabled, principal) => requester(caller, principal)
            .and_then(|who| manager.set_boot_enabled(&who, &id, enabled))
            .map(|()| Value::Null),
        // The assignment of one model profile to one instance.
        Request::SetModelProfile(instance, profile, value, principal) => {
            requester(caller, principal)
                .and_then(|who| manager.set_model_profile(&who, &instance, &profile, &value))
                .map(|()| Value::Null)
        }
        // The empty caller too goes to the core, which answers only the bff.
        Request::Authenticate(user, password) => manager
            .authenticate(caller, &user, &password)
            .map(|(ok, admin)| json!({"authenticated": ok, "can_configure": admin})),
    };
    match result {
        Ok(value) => json!({"ok": value}),
        Err(e) => error_value(error_reply(&e)),
    }
}

fn error_value((name, message): (String, String)) -> Value {
    json!({"error": {"name": name, "message": message}})
}

/// Answer BadRequest, then end the connection with an error.
fn bad_request(stream: &mut impl Write) -> std::io::Result<()> {
    let reply = error_value((BAD_REQUEST.to_string(), "BadRequest".to_string()));
    write_frame(stream, reply.to_string().as_bytes())?;
    Err(Error::new(ErrorKind::InvalidData, BAD_REQUEST))
}

/// Serve one connection: read requests until the stream ends, answer each
/// through `manager` in order, as `caller` (who opened the connection; an
/// empty one is refused its own changes, nobody asked; an `As` request goes
/// to the core, which gates the principal on the caller). Ok when the stream
/// ends between frames; an error when it ends inside one, fails, or carries
/// a frame that is no request (answered with [`BAD_REQUEST`] first, the rest
/// of the stream left unread). Never panics on what it reads.
pub fn serve_stream<S, A, R>(
    manager: &Manager<S, A, R>,
    caller: Caller,
    mut stream: impl Read + Write,
) -> std::io::Result<()>
where
    S: ServiceControl,
    A: Authority,
    R: Resolver,
{
    loop {
        let body = match read_frame(&mut stream) {
            Ok(Some(body)) => body,
            Ok(None) => return Ok(()),
            Err(FrameError::Io(e)) => return Err(e),
            Err(FrameError::TooLong(_)) => return bad_request(&mut stream),
        };
        let Some(request) = Request::parse(&body) else {
            return bad_request(&mut stream);
        };
        let reply = answer(manager, &caller, request);
        write_frame(&mut stream, reply.to_string().as_bytes())?;
    }
}

/// E2E-free tests: nothing here reaches `paths::` (the requests are refused
/// before the core reads a file, or ask only a fake), so they are plain
/// `#[test]`s. The round trips through the files are the e2e phase in
/// `src/tests/pipe_client.rs`.
#[cfg(test)]
mod tests {
    use super::super::service::fakes::{
        Call, FakeAuthority, FakeGroup, FakePasswords, FakeResolver, FakeService,
    };
    use super::super::service::{Action, ServiceError, UnitState};
    use super::*;
    use pai_core::client::pipe::MAX_FRAME;
    use serde_json::{json, Value};
    use std::io::{Cursor, ErrorKind};
    use std::sync::{Arc, Mutex};

    type Core = Manager<FakeService, FakeAuthority, FakeResolver>;

    const PREFIX: &str = "org.privateai.Manager1.Error.";

    fn core_in(state: UnitState, allow: bool) -> Core {
        let mut service = FakeService::new();
        service.state = state;
        Manager::new(service, FakeAuthority::new(allow), FakeResolver::lan())
            .with_group(crate::manager::service::fakes::FakeGroup::new())
    }

    fn core(allow: bool) -> Core {
        core_in(UnitState::Active, allow)
    }

    fn calls(c: &Core) -> Vec<Call> {
        c.service().calls.lock().unwrap().clone()
    }

    fn asked(c: &Core) -> Vec<(Caller, Action)> {
        c.authority().asked.lock().unwrap().clone()
    }

    /// The caller a connection was opened by: an opaque string to the core
    /// (here shaped like a Windows user SID).
    fn alice() -> Caller {
        Caller("S-1-5-21-3623811015-3361044348-30300820-1013".into())
    }

    /// A client's side of a connection: what it sent (`input`, read by the
    /// server) and what the server wrote back; `write_error` makes every
    /// write fail.
    struct Script {
        input: Cursor<Vec<u8>>,
        replies: Arc<Mutex<Vec<u8>>>,
        write_error: Option<ErrorKind>,
    }

    impl Read for Script {
        fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
            self.input.read(buf)
        }
    }

    impl Write for Script {
        fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
            match self.write_error {
                Some(kind) => Err(kind.into()),
                None => {
                    self.replies.lock().unwrap().extend_from_slice(buf);
                    Ok(buf.len())
                }
            }
        }
        fn flush(&mut self) -> std::io::Result<()> {
            match self.write_error {
                Some(kind) => Err(kind.into()),
                None => Ok(()),
            }
        }
    }

    /// One frame: the body's length, u32 little-endian, then the body.
    fn frame(body: &[u8]) -> Vec<u8> {
        let mut out = (body.len() as u32).to_le_bytes().to_vec();
        out.extend_from_slice(body);
        out
    }

    fn requests(values: &[Value]) -> Vec<u8> {
        values
            .iter()
            .flat_map(|v| frame(v.to_string().as_bytes()))
            .collect()
    }

    /// The frames in `bytes`, each body parsed as JSON; a short or non-JSON
    /// frame fails the test.
    fn frames(bytes: &[u8]) -> Vec<Value> {
        let mut out = Vec::new();
        let mut rest = bytes;
        while !rest.is_empty() {
            assert!(rest.len() >= 4, "a short length prefix: {rest:?}");
            let len = u32::from_le_bytes(rest[..4].try_into().unwrap()) as usize;
            assert!(rest.len() >= 4 + len, "a short body: {rest:?}");
            let body = std::str::from_utf8(&rest[4..4 + len]).expect("a UTF-8 body");
            out.push(serde_json::from_str(body).expect("a JSON body"));
            rest = &rest[4 + len..];
        }
        out
    }

    /// Serve `input` to its end as `caller`: the result and the replies.
    fn serve(c: &Core, caller: Caller, input: Vec<u8>) -> (std::io::Result<()>, Vec<Value>) {
        let replies = Arc::new(Mutex::new(Vec::new()));
        let script = Script {
            input: Cursor::new(input),
            replies: replies.clone(),
            write_error: None,
        };
        let result = serve_stream(c, caller, script);
        let bytes = replies.lock().unwrap().clone();
        (result, frames(&bytes))
    }

    fn error(kind: &str, message: &str) -> Value {
        json!({"error": {"name": format!("{PREFIX}{kind}"), "message": message}})
    }

    /// The reply is the BadRequest error, with a message.
    fn assert_bad_request(reply: &Value, what: &str) {
        assert_eq!(reply["error"]["name"], BAD_REQUEST, "{what}: {reply}");
        let message = reply["error"]["message"].as_str().unwrap_or_default();
        assert!(!message.is_empty(), "{what}: {reply}");
    }

    // ── Failures first ──────────────────────────────────────────────────

    #[test]
    fn the_bad_request_name_is_under_the_prefix() {
        assert_eq!(BAD_REQUEST, "org.privateai.Manager1.Error.BadRequest");
    }

    /// A client that connects and leaves: nothing answered, nothing asked.
    #[test]
    fn a_stream_that_ends_at_once_is_a_clean_end() {
        let c = core(true);
        let (result, replies) = serve(&c, alice(), Vec::new());
        assert!(result.is_ok(), "{result:?}");
        assert_eq!(replies, Vec::<Value>::new());
        assert_eq!(calls(&c), vec![]);
        assert_eq!(asked(&c), vec![]);
    }

    /// A stream that ends inside a frame: the request never arrived, so it
    /// is neither answered nor run.
    #[test]
    fn a_stream_that_ends_inside_a_frame_is_an_error() {
        let whole = frame(
            json!({"method": "Start", "id": "default"})
                .to_string()
                .as_bytes(),
        );
        for cut in [1, 3, 4, 5, whole.len() - 1] {
            let c = core(true);
            let (result, replies) = serve(&c, alice(), whole[..cut].to_vec());
            assert!(result.is_err(), "cut after {cut} bytes");
            assert_eq!(replies, Vec::<Value>::new(), "cut after {cut} bytes");
            assert_eq!(calls(&c), vec![], "cut after {cut} bytes");
            assert_eq!(asked(&c), vec![], "cut after {cut} bytes");
        }
        // After a whole request, the truncated next one ends the connection
        // and the first is still answered.
        let c = core(true);
        let mut input = requests(&[json!({"method": "Status", "id": "default"})]);
        input.extend_from_slice(&whole[..whole.len() - 1]);
        let (result, replies) = serve(&c, alice(), input);
        assert!(result.is_err());
        assert_eq!(replies, vec![json!({"ok": "active"})]);
        assert_eq!(calls(&c), vec![Call::Status("default".into())]);
    }

    /// A frame that is no request is answered BadRequest and ends the
    /// connection: the requests after it are never read, the core never
    /// reached.
    #[test]
    fn a_malformed_request_is_answered_and_ends_the_connection() {
        let bodies: [&[u8]; 21] = [
            b"",
            b"not json",
            b"{\"method\":",
            b"\xff\xfe{\"method\":\"ListInstances\"}",
            b"[\"Start\",\"default\"]",
            b"\"ListInstances\"",
            b"{\"id\":\"default\"}",
            b"{\"method\":\"Reload\",\"id\":\"default\"}",
            b"{\"method\":\"start\",\"id\":\"default\"}",
            b"{\"method\":\"Start\"}",
            b"{\"method\":\"Start\",\"id\":5}",
            b"{\"method\":\"WriteFile\",\"name\":\"models.ini\"}",
            // `keep_api_key` is required, a bool, as SetBootEnabled's `enabled`.
            b"{\"method\":\"WriteFile\",\"name\":\"models.ini\",\"contents\":\"\"}",
            b"{\"method\":\"WriteFile\",\"name\":\"models.ini\",\"contents\":\"\",\"keep_api_key\":\"no\"}",
            // `WriteFileIf` requires the content the file must still hold:
            // missing, a number, null, is the same BadRequest. A frame with no
            // `expected` is refused, never treated as an empty expectation.
            b"{\"method\":\"WriteFileIf\",\"name\":\"models.ini\",\"contents\":\"x\"}",
            b"{\"method\":\"WriteFileIf\",\"name\":\"models.ini\",\"contents\":\"x\",\"expected\":42}",
            b"{\"method\":\"WriteFileIf\",\"name\":\"models.ini\",\"contents\":\"x\",\"expected\":null}",
            // The name and the new content are arguments of both spellings.
            b"{\"method\":\"WriteFileIf\",\"contents\":\"x\",\"expected\":\"\"}",
            b"{\"method\":\"WriteFileIf\",\"name\":\"models.ini\",\"expected\":\"\"}",
            b"{\"method\":\"WriteFileIfAs\",\"principal\":\"alice\",\"name\":\"models.ini\",\"contents\":\"x\"}",
            b"{\"method\":\"WriteFileIfAs\",\"name\":\"models.ini\",\"contents\":\"x\",\"expected\":\"\"}",
        ];
        for body in bodies {
            let c = core(true);
            let mut input = frame(body);
            input.extend(requests(&[json!({"method": "Start", "id": "default"})]));
            let (result, replies) = serve(&c, alice(), input);
            let what = String::from_utf8_lossy(body).into_owned();
            assert!(result.is_err(), "{what}");
            assert_eq!(replies.len(), 1, "{what}: only the BadRequest: {replies:?}");
            assert_bad_request(&replies[0], &what);
            assert_eq!(calls(&c), vec![], "{what}");
            assert_eq!(asked(&c), vec![], "{what}");
        }
    }

    /// A frame over the cap is refused from its length alone: answered
    /// BadRequest, its body and everything after it left unread.
    #[test]
    fn a_frame_over_the_cap_is_answered_and_ends_the_connection() {
        let start = json!({"method": "Start", "id": "default"}).to_string();
        for len in [MAX_FRAME + 1, u32::MAX] {
            let c = core(true);
            let mut input = len.to_le_bytes().to_vec();
            input.extend_from_slice(start.as_bytes());
            input.extend(requests(&[json!({"method": "Start", "id": "default"})]));
            let (result, replies) = serve(&c, alice(), input);
            assert!(result.is_err(), "length {len}");
            assert_eq!(replies.len(), 1, "length {len}: {replies:?}");
            assert_bad_request(&replies[0], &format!("length {len}"));
            assert_eq!(calls(&c), vec![], "length {len}");
            assert_eq!(asked(&c), vec![], "length {len}");
        }
    }

    /// A frame of exactly the cap is read and answered.
    #[test]
    fn a_frame_at_the_cap_is_served() {
        let shell = json!({"method": "ReadFile", "name": ""}).to_string();
        let name = "x".repeat(MAX_FRAME as usize - shell.len());
        let body = json!({"method": "ReadFile", "name": name}).to_string();
        assert_eq!(body.len(), MAX_FRAME as usize);
        let c = core(true);
        let (result, replies) = serve(&c, alice(), frame(body.as_bytes()));
        assert!(result.is_ok(), "{result:?}");
        assert_eq!(replies, vec![error("BadFileName", "BadFileName")]);
    }

    /// A client gone before its reply: the server stops with an error, no
    /// panic.
    #[test]
    fn a_reply_that_cannot_be_written_ends_the_connection() {
        let c = core(true);
        let script = Script {
            input: Cursor::new(requests(&[
                json!({"method": "Status", "id": "default"}),
                json!({"method": "Stop", "id": "default"}),
            ])),
            replies: Arc::new(Mutex::new(Vec::new())),
            write_error: Some(ErrorKind::BrokenPipe),
        };
        let result = serve_stream(&c, alice(), script);
        assert!(result.is_err());
        assert_eq!(
            calls(&c),
            vec![Call::Status("default".into())],
            "the request after the failed reply is not run"
        );
    }

    /// Every change asks the authority about the connection's caller, per
    /// action; a refused caller changes nothing.
    #[test]
    fn a_refused_caller_is_asked_with_its_own_identity_and_nothing_is_done() {
        let c = core(false);
        let (result, replies) = serve(
            &c,
            alice(),
            requests(&[
                json!({"method": "Start", "id": "default"}),
                json!({"method": "Stop", "id": "default"}),
                json!({"method": "Restart", "id": "default"}),
                json!({"method": "WriteFile", "name": "models.ini", "contents": "[x]\nmodel = /m/x.gguf\n", "keep_api_key": false}),
                // The compare-and-swap write is a guarded call like the plain
                // one: refused for a caller without the right, no path
                // resolved, nothing written.
                json!({"method": "WriteFileIf", "name": "server.ini", "contents": "x", "expected": ""}),
            ]),
        );
        assert!(result.is_ok(), "{result:?}");
        assert_eq!(replies, vec![error("NotAuthorized", "NotAuthorized"); 5]);
        assert_eq!(calls(&c), vec![]);
        assert_eq!(
            asked(&c),
            vec![
                (alice(), Action::Control),
                (alice(), Action::Control),
                (alice(), Action::Control),
                (alice(), Action::Configure),
                (alice(), Action::Configure),
            ]
        );
    }

    /// An instance file can hold a secret (`OpencodeApiKey`): reading one
    /// asks the authority for control as the connection's caller, and a
    /// refused caller gets NotAuthorized without the file being read (here
    /// before any path is resolved: this test never reaches `paths::`), an
    /// instance that does not exist included.
    #[test]
    fn reading_an_instance_file_asks_control_and_a_refused_caller_reads_nothing() {
        let c = core(false);
        let (result, replies) = serve(
            &c,
            alice(),
            requests(&[
                json!({"method": "ReadFile", "name": "server@default.ini"}),
                json!({"method": "ReadFile", "name": "server@nope.ini"}),
            ]),
        );
        assert!(result.is_ok(), "{result:?}");
        assert_eq!(replies, vec![error("NotAuthorized", "NotAuthorized"); 2]);
        assert_eq!(asked(&c), vec![(alice(), Action::Control); 2]);
        assert_eq!(calls(&c), vec![]);
    }

    /// A connection with no identity (the D-Bus call with no sender) is
    /// refused every change before anybody is asked, even by an authority
    /// that would allow it.
    #[test]
    fn an_empty_caller_is_refused_every_change_unasked() {
        let c = core(true);
        let (result, replies) = serve(
            &c,
            Caller(String::new()),
            requests(&[
                json!({"method": "Start", "id": "default"}),
                json!({"method": "Stop", "id": "default"}),
                json!({"method": "Restart", "id": "default"}),
                json!({"method": "WriteFile", "name": "models.ini", "contents": "", "keep_api_key": false}),
                json!({"method": "WriteFileIf", "name": "models.ini", "contents": "", "expected": ""}),
                json!({"method": "ReadFile", "name": "server@default.ini"}),
                json!({"method": "Status", "id": "default"}),
            ]),
        );
        assert!(result.is_ok(), "{result:?}");
        let refused = error("NotAuthorized", "NotAuthorized");
        assert_eq!(
            replies,
            vec![
                refused.clone(),
                refused.clone(),
                refused.clone(),
                refused.clone(),
                refused.clone(),
                refused,
                json!({"ok": "active"}),
            ],
            "an instance file's read is refused like a change; status needs no caller"
        );
        assert_eq!(asked(&c), vec![], "nobody asked");
        assert_eq!(calls(&c), vec![Call::Status("default".into())]);
    }

    // ── Start at boot ───────────────────────────────────────────────────
    // Refused before the core resolves a path: plain tests. The round trips
    // are in `src/tests/pipe_client.rs`.

    /// `SetBootEnabled` asks the authority for configure as the
    /// connection's caller; a refused caller changes nothing.
    #[test]
    fn the_start_at_boot_change_asks_configure_and_a_refused_caller_changes_nothing() {
        let c = core(false);
        let (result, replies) = serve(
            &c,
            alice(),
            requests(&[
                json!({"method": "SetBootEnabled", "id": "default", "enabled": true}),
                json!({"method": "SetBootEnabled", "id": "default", "enabled": false}),
            ]),
        );
        assert!(result.is_ok(), "{result:?}");
        assert_eq!(replies, vec![error("NotAuthorized", "NotAuthorized"); 2]);
        assert_eq!(asked(&c), vec![(alice(), Action::Configure); 2]);
        assert_eq!(calls(&c), vec![]);
    }

    /// A connection with no identity is refused the change unasked; the
    /// read needs no caller (here an invalid id, refused as one).
    #[test]
    fn an_empty_caller_is_refused_the_start_at_boot_change_unasked() {
        let c = core(true);
        let (result, replies) = serve(
            &c,
            Caller(String::new()),
            requests(&[
                json!({"method": "SetBootEnabled", "id": "default", "enabled": true}),
                json!({"method": "BootEnabled", "id": "../x"}),
            ]),
        );
        assert!(result.is_ok(), "{result:?}");
        assert_eq!(
            replies,
            vec![
                error("NotAuthorized", "NotAuthorized"),
                error("InvalidId", "InvalidId"),
            ]
        );
        assert_eq!(asked(&c), vec![]);
        assert_eq!(calls(&c), vec![]);
    }

    /// An invalid id is refused either way, the change after its
    /// authorization.
    #[test]
    fn the_start_at_boot_of_an_invalid_id_is_refused() {
        let c = core(true);
        let (result, replies) = serve(
            &c,
            alice(),
            requests(&[
                json!({"method": "BootEnabled", "id": "Default"}),
                json!({"method": "SetBootEnabled", "id": "a_b", "enabled": true}),
            ]),
        );
        assert!(result.is_ok(), "{result:?}");
        assert_eq!(replies, vec![error("InvalidId", "InvalidId"); 2]);
        assert_eq!(asked(&c), vec![(alice(), Action::Configure)]);
        assert_eq!(calls(&c), vec![]);
    }

    /// `enabled` is a JSON boolean and required; `id` too.
    #[test]
    fn a_malformed_start_at_boot_request_is_a_bad_request() {
        for body in [
            json!({"method": "BootEnabled"}),
            json!({"method": "BootEnabled", "id": 5}),
            json!({"method": "SetBootEnabled", "id": "default"}),
            json!({"method": "SetBootEnabled", "id": "default", "enabled": "yes"}),
            json!({"method": "SetBootEnabled", "id": "default", "enabled": 1}),
            json!({"method": "SetBootEnabled", "enabled": true}),
        ] {
            let c = core(true);
            let (result, replies) = serve(&c, alice(), requests(std::slice::from_ref(&body)));
            assert!(result.is_err(), "{body}");
            assert_eq!(replies.len(), 1, "{body}: {replies:?}");
            assert_bad_request(&replies[0], &body.to_string());
            assert_eq!(asked(&c), vec![], "{body}");
            assert_eq!(calls(&c), vec![], "{body}");
        }
    }

    // ── The wire ────────────────────────────────────────────────────────

    /// The raw replies, as pinned in `pai_core::client::pipe`'s header:
    /// requests on one connection answered in order.
    #[test]
    fn replies_are_the_pinned_frames_in_order() {
        let c = core(true);
        let (result, replies) = serve(
            &c,
            alice(),
            requests(&[
                json!({"method": "Status", "id": "default"}),
                json!({"method": "Stop", "id": "default"}),
                json!({"method": "Status", "id": "../x"}),
                json!({"method": "ReadFile", "name": "/etc/passwd"}),
                json!({"method": "WriteFile", "name": "server.ini", "contents": "x", "keep_api_key": false}),
            ]),
        );
        assert!(result.is_ok(), "{result:?}");
        assert_eq!(
            replies,
            vec![
                json!({"ok": "active"}),
                json!({"ok": null}),
                error("InvalidId", "InvalidId"),
                error("BadFileName", "BadFileName"),
                error("BadFileName", "BadFileName"),
            ]
        );
        assert_eq!(
            calls(&c),
            vec![Call::Status("default".into()), Call::Stop("default".into())]
        );
        assert_eq!(
            asked(&c),
            vec![(alice(), Action::Control), (alice(), Action::Configure)]
        );
    }

    #[test]
    fn status_names_every_state_as_dbus_does() {
        for (state, name) in [
            (UnitState::Active, "active"),
            (UnitState::Inactive, "inactive"),
            (UnitState::Failed, "failed"),
            (UnitState::Activating, "activating"),
            (UnitState::Deactivating, "deactivating"),
        ] {
            let c = core_in(state, true);
            let (result, replies) = serve(
                &c,
                alice(),
                requests(&[json!({"method": "Status", "id": "default"})]),
            );
            assert!(result.is_ok(), "{state:?}: {result:?}");
            assert_eq!(replies, vec![json!({"ok": name})], "{state:?}");
        }
    }

    /// The service manager's failure comes back with its own message.
    #[test]
    fn a_service_failure_carries_its_message() {
        let mut service = FakeService::new();
        service.failing = vec!["default".into()];
        let c = Manager::new(service, FakeAuthority::new(true), FakeResolver::lan())
            .with_group(crate::manager::service::fakes::FakeGroup::new());
        let (result, replies) = serve(
            &c,
            alice(),
            requests(&[
                json!({"method": "Stop", "id": "default"}),
                json!({"method": "Status", "id": "default"}),
            ]),
        );
        assert!(result.is_ok(), "{result:?}");
        assert_eq!(
            replies,
            vec![
                error("Service", "llama.cpp@default.service failed"),
                error("Service", "llama.cpp@default.service failed"),
            ]
        );
    }

    fn every_error() -> Vec<(ManagerError, &'static str)> {
        vec![
            (ManagerError::NotAuthorized, "NotAuthorized"),
            (ManagerError::InvalidId, "InvalidId"),
            (ManagerError::NoInstance, "NoInstance"),
            (ManagerError::BadFileName, "BadFileName"),
            (ManagerError::AddressInUse("default".into()), "AddressInUse"),
            (
                ManagerError::UnresolvableHost("gpu-box.lan".into()),
                "UnresolvableHost",
            ),
            (
                ManagerError::Service(ServiceError(
                    "Unit llama.cpp@default.service not found.".into(),
                )),
                "Service",
            ),
            (ManagerError::Io(ErrorKind::PermissionDenied), "Io"),
            (
                ManagerError::Invalid(pai_core::checks::Refusal {
                    key: "invalid-model-profile-id",
                    values: vec!["Qwen 3 (v2) é".into()],
                }),
                "Invalid",
            ),
            (ManagerError::Changed, "Changed"),
        ]
    }

    /// Each error has its name under the prefix and a message the client's
    /// `from_reply` reads back to the same error.
    #[test]
    fn every_error_has_its_name_and_reads_back() {
        use pai_core::client::ClientError;
        for (e, kind) in every_error() {
            let (name, message) = error_reply(&e);
            assert_eq!(name, format!("{PREFIX}{kind}"), "{e:?}");
            let expected = match &e {
                ManagerError::NotAuthorized => ClientError::NotAuthorized,
                ManagerError::InvalidId => ClientError::InvalidId,
                ManagerError::NoInstance => ClientError::NoInstance,
                ManagerError::BadFileName => ClientError::BadFileName,
                ManagerError::AddressInUse(id) => ClientError::AddressInUse(id.clone()),
                ManagerError::UnresolvableHost(h) => ClientError::UnresolvableHost(h.clone()),
                ManagerError::Service(s) => ClientError::Service(s.0.clone()),
                ManagerError::Io(k) => ClientError::Io(k.to_string()),
                ManagerError::Invalid(r) => ClientError::Invalid(r.clone()),
                ManagerError::Changed => ClientError::Changed,
            };
            assert_eq!(ClientError::from_reply(&name, &message), expected, "{e:?}");
        }
    }

    /// The very pair the D-Bus interface replies with, so both transports
    /// map identically.
    #[cfg(target_os = "linux")]
    #[test]
    fn every_error_is_the_dbus_pair() {
        for (e, _) in every_error() {
            assert_eq!(
                error_reply(&e),
                super::super::dbus::error_reply(&e),
                "{e:?}"
            );
        }
    }

    // ── Authenticate ────────────────────────────────────────────────────

    /// The bff's account on Windows, `NT SERVICE\private-ai-bff`, by its
    /// service SID (the one `windows::PIPE_SDDL` grants).
    fn bff() -> Caller {
        Caller("S-1-5-80-4088784262-1422927641-1363593751-33154763-2531830098".into())
    }

    /// A core taking [`bff`] for the bff's account over the fake accounts
    /// (alice an administrator, bob not), and its checker's log.
    fn auth_core(
        passwords: super::super::service::fakes::FakePasswords,
    ) -> (Core, crate::manager::service::fakes::Checked) {
        let checked = passwords.checked.clone();
        let core = Manager::new(
            FakeService::new(),
            FakeAuthority::new(true).bff(&bff()),
            FakeResolver::lan(),
        )
        .with_group(crate::manager::service::fakes::FakeGroup::new())
        .with_passwords(passwords);
        (core, checked)
    }

    fn auth(user: &str, password: &str) -> Value {
        json!({"method": "Authenticate", "user": user, "password": password})
    }

    /// `{"method":"Authenticate","user","password"}` from the bff:
    /// `{"ok":{"authenticated":bool,"can_configure":bool}}`, in the
    /// snake_case of `keep_api_key`.
    #[test]
    fn the_bff_authenticates_over_the_pipe() {
        let (c, checked) = auth_core(super::super::service::fakes::FakePasswords::new());
        let (result, replies) = serve(
            &c,
            bff(),
            requests(&[
                auth("alice", "s3cret-Pw"),
                auth("bob", "hunter2!"),
                auth("alice", "wrong"),
                auth("mallory", "s3cret-Pw"),
                auth("", "s3cret-Pw"),
                auth("alice", ""),
            ]),
        );
        assert!(result.is_ok(), "{result:?}");
        let answer =
            |ok: bool, admin: bool| json!({"ok": {"authenticated": ok, "can_configure": admin}});
        assert_eq!(
            replies,
            vec![
                answer(true, true),
                answer(true, false),
                answer(false, false),
                answer(false, false),
                answer(false, false),
                answer(false, false),
            ]
        );
        assert_eq!(checked.lock().unwrap().len(), 4, "empty ones never checked");
        assert_eq!(calls(&c), vec![]);
        assert_eq!(asked(&c), vec![], "polkit-like rights are not asked");
    }

    /// Any other caller, a Windows user or an administrator, or none:
    /// NotAuthorized, no password checked, the connection kept.
    #[test]
    fn only_the_bff_may_authenticate_over_the_pipe() {
        for caller in [alice(), Caller("S-1-5-18".into()), Caller(String::new())] {
            let (c, checked) = auth_core(super::super::service::fakes::FakePasswords::new());
            let (result, replies) = serve(
                &c,
                caller.clone(),
                requests(&[
                    auth("alice", "s3cret-Pw"),
                    json!({"method": "Status", "id": "default"}),
                ]),
            );
            assert!(result.is_ok(), "{caller:?}: {result:?}");
            assert_eq!(
                replies,
                vec![
                    error("NotAuthorized", "NotAuthorized"),
                    json!({"ok": "active"})
                ],
                "{caller:?}"
            );
            assert_eq!(*checked.lock().unwrap(), vec![], "{caller:?}");
        }
    }

    /// A request missing its user or password, or with one not a string,
    /// is BadRequest: the connection ends and no password is checked. The
    /// reply never echoes the password.
    #[test]
    fn a_malformed_authenticate_is_a_bad_request() {
        for request in [
            json!({"method": "Authenticate", "user": "alice"}),
            json!({"method": "Authenticate", "password": "s3cret-Pw"}),
            json!({"method": "Authenticate"}),
            json!({"method": "Authenticate", "user": "alice", "password": 7}),
            json!({"method": "Authenticate", "user": null, "password": "s3cret-Pw"}),
            json!({"method": "Authenticate", "user": ["alice"], "password": "s3cret-Pw"}),
        ] {
            let (c, checked) = auth_core(super::super::service::fakes::FakePasswords::new());
            let (result, replies) = serve(&c, bff(), requests(std::slice::from_ref(&request)));
            assert!(result.is_err(), "{request}");
            assert_eq!(replies.len(), 1, "{request}");
            assert_bad_request(&replies[0], &request.to_string());
            assert!(!replies[0].to_string().contains("s3cret-Pw"), "{request}");
            assert_eq!(*checked.lock().unwrap(), vec![], "{request}");
        }
    }

    /// A failing checker: the Service error with its message, never the
    /// password, and the connection kept.
    #[test]
    fn a_failing_checker_answers_service_over_the_pipe() {
        let failure = "LogonUserW: ERROR_NO_LOGON_SERVERS (1311)";
        let (c, _) = auth_core(super::super::service::fakes::FakePasswords::failing(
            failure,
        ));
        let (result, replies) = serve(&c, bff(), requests(&[auth("alice", "s3cret-Pw")]));
        assert!(result.is_ok(), "{result:?}");
        assert_eq!(replies, vec![error("Service", failure)]);
        assert!(!replies[0].to_string().contains("s3cret-Pw"));
    }

    // ── Acting for a signed-in user: the `As` methods ───────────────────
    // Every call here is refused before a path is resolved, or asks a fake
    // only, so these never reach `paths::`.

    /// The accounts of [`principal_core`], with the log of the membership
    /// questions its calls ask.
    type Admins = Arc<Mutex<Vec<String>>>;

    /// A core taking [`bff`] for the bff's account over `passwords` (alice a
    /// member of `pai`, bob not) and polkit that would allow the caller
    /// everything, plus the log of the membership questions.
    fn principal_core(passwords: FakePasswords) -> (Core, Admins) {
        let admins = passwords.asked_admin.clone();
        let core = Manager::new(
            FakeService::new(),
            FakeAuthority::new(true).bff(&bff()),
            FakeResolver::lan(),
        )
        .with_group(FakeGroup::new())
        .with_passwords(passwords);
        (core, admins)
    }

    fn asked_admin(admins: &Admins) -> Vec<String> {
        admins.lock().unwrap().clone()
    }

    fn asked_bff(c: &Core) -> Vec<Caller> {
        c.authority().asked_bff.lock().unwrap().clone()
    }

    /// An `As` method from the bff reaches the core as a request for that
    /// principal: the membership of the name is asked once per call, polkit
    /// is not (the right is the pair), and the action runs.
    #[test]
    fn an_as_method_from_the_bff_reaches_the_core_with_the_principal() {
        let (c, admins) = principal_core(FakePasswords::new());
        let (result, replies) = serve(
            &c,
            bff(),
            requests(&[
                json!({"method": "StopAs", "principal": "alice", "id": "default"}),
                json!({"method": "SetBootEnabledAs", "principal": "alice", "id": "../x", "enabled": true}),
                json!({"method": "ReadFileAs", "principal": "alice", "name": "server.ini"}),
                json!({"method": "WriteFileAs", "principal": "alice", "name": "server.ini", "contents": "x", "keep_api_key": false}),
                json!({"method": "WriteFileIfAs", "principal": "alice", "name": "server.ini", "contents": "x", "expected": ""}),
            ]),
        );
        assert!(result.is_ok(), "{result:?}");
        assert_eq!(
            replies,
            vec![
                json!({"ok": null}),
                error("InvalidId", "InvalidId"),
                error("BadFileName", "BadFileName"),
                error("BadFileName", "BadFileName"),
                error("BadFileName", "BadFileName"),
            ]
        );
        assert_eq!(calls(&c), vec![Call::Stop("default".into())]);
        assert_eq!(
            asked_admin(&admins),
            vec!["alice".to_string(); 5],
            "the membership of the name the request vouches for, once per call"
        );
        assert_eq!(
            asked(&c),
            vec![],
            "polkit asked, so the call ran as the bff's own, not for the principal"
        );
        assert_eq!(asked_bff(&c), vec![bff(); 5]);
    }

    /// A caller that is not the bff naming a principal: NotAuthorized for
    /// every `As` method, the membership never looked up, nothing asked and
    /// nothing done, the connection kept. Refused, never run as the caller's
    /// own (the caller polkit would allow everything here).
    #[test]
    fn an_as_method_from_a_caller_that_is_not_the_bff_is_refused_unasked() {
        for caller in [alice(), Caller("S-1-5-18".into()), Caller(String::new())] {
            let (c, admins) = principal_core(FakePasswords::new());
            let mut expected = vec![error("NotAuthorized", "NotAuthorized"); 7];
            expected.push(json!({"ok": "active"}));
            let (result, replies) = serve(
                &c,
                caller.clone(),
                requests(&[
                    json!({"method": "StartAs", "principal": "alice", "id": "default"}),
                    json!({"method": "StopAs", "principal": "alice", "id": "default"}),
                    json!({"method": "RestartAs", "principal": "alice", "id": "default"}),
                    json!({"method": "ReadFileAs", "principal": "alice", "name": "server@default.ini"}),
                    json!({"method": "WriteFileAs", "principal": "alice", "name": "models.ini", "contents": "", "keep_api_key": false}),
                    json!({"method": "WriteFileIfAs", "principal": "alice", "name": "models.ini", "contents": "", "expected": ""}),
                    json!({"method": "SetBootEnabledAs", "principal": "alice", "id": "default", "enabled": true}),
                    json!({"method": "Status", "id": "default"}),
                ]),
            );
            assert!(result.is_ok(), "{caller:?}: {result:?}");
            assert_eq!(replies, expected, "{caller:?}");
            assert_eq!(
                asked_admin(&admins),
                Vec::<String>::new(),
                "{caller:?}: a membership looked up for a caller that is not the bff"
            );
            assert_eq!(
                asked(&c),
                vec![],
                "{caller:?}: polkit asked, so the call ran as the caller's own"
            );
            assert_eq!(
                calls(&c),
                vec![Call::Status("default".into())],
                "{caller:?}: something beside the Status probe reached the service manager"
            );
            // A connection with no identity is refused as any other; whether
            // the face asks `is_bff` for it, as it does for Authenticate, is
            // its own choice, so only a named caller's log is pinned.
            if !caller.0.is_empty() {
                assert_eq!(
                    asked_bff(&c),
                    vec![caller.clone(); 7],
                    "{caller:?}: the caller is checked first, once per call"
                );
            }
        }
    }

    /// The bff naming an account that is no member of `pai` (bob, an ordinary
    /// account) or is unknown: NotAuthorized, the membership asked for that
    /// name, nothing done.
    #[test]
    fn an_as_method_for_a_principal_who_is_not_a_member_is_refused() {
        for user in ["bob", "mallory"] {
            let (c, admins) = principal_core(FakePasswords::new());
            let (result, replies) = serve(
                &c,
                bff(),
                requests(&[
                    json!({"method": "StopAs", "principal": user, "id": "default"}),
                    json!({"method": "SetBootEnabledAs", "principal": user, "id": "../x", "enabled": true}),
                    json!({"method": "WriteFileIfAs", "principal": user, "name": "server.ini", "contents": "x", "expected": ""}),
                ]),
            );
            assert!(result.is_ok(), "{user}: {result:?}");
            assert_eq!(
                replies,
                vec![error("NotAuthorized", "NotAuthorized"); 3],
                "{user}"
            );
            assert_eq!(
                asked_admin(&admins),
                vec![user.to_string(); 3],
                "{user}: the membership asked for the name sent"
            );
            assert_eq!(asked(&c), vec![], "{user}: polkit asked");
            assert_eq!(
                calls(&c),
                vec![],
                "{user}: something reached the service manager"
            );
        }
    }

    /// An `As` method that carries no principal is no request: the connection
    /// ends with BadRequest, the core never reached. A principal has exactly
    /// one spelling, and it is required.
    #[test]
    fn an_as_method_without_a_principal_is_a_bad_request() {
        for request in [
            json!({"method": "StartAs", "id": "default"}),
            json!({"method": "StopAs", "id": "default"}),
            json!({"method": "RestartAs", "id": "default"}),
            json!({"method": "ReadFileAs", "name": "models.ini"}),
            json!({"method": "WriteFileAs", "name": "models.ini", "contents": "", "keep_api_key": false}),
            json!({"method": "WriteFileIfAs", "name": "models.ini", "contents": "", "expected": ""}),
            json!({"method": "SetBootEnabledAs", "id": "default", "enabled": true}),
            json!({"method": "StartAs", "principal": 5, "id": "default"}),
            json!({"method": "WriteFileIfAs", "principal": 5, "name": "models.ini", "contents": "", "expected": ""}),
            json!({"method": "StartAs", "principal": null, "id": "default"}),
            json!({"method": "StartAs", "principal": ["alice"], "id": "default"}),
            json!({"method": "StartAs", "principal": {"name": "alice"}, "id": "default"}),
            // The `As` methods take the same arguments as the plain ones.
            json!({"method": "StartAs", "principal": "alice"}),
            json!({"method": "ReadFileAs", "principal": "alice"}),
            json!({"method": "WriteFileAs", "principal": "alice", "name": "models.ini"}),
            json!({"method": "WriteFileIfAs", "principal": "alice", "name": "models.ini", "contents": ""}),
            json!({"method": "WriteFileIfAs", "principal": "alice", "name": "models.ini", "expected": ""}),
            json!({"method": "SetBootEnabledAs", "principal": "alice", "id": "default"}),
            json!({"method": "SetBootEnabledAs", "principal": "alice", "id": "default", "enabled": "yes"}),
            // The name says it carries a principal: the plain spelling is
            // unknown with one, and the `As` spelling is unknown without.
            json!({"method": "startAs", "principal": "alice", "id": "default"}),
        ] {
            let (c, admins) = principal_core(FakePasswords::new());
            let (result, replies) = serve(&c, bff(), requests(std::slice::from_ref(&request)));
            assert!(result.is_err(), "{request}");
            assert_eq!(replies.len(), 1, "{request}: {replies:?}");
            assert_bad_request(&replies[0], &request.to_string());
            assert_eq!(asked(&c), vec![], "{request}");
            assert_eq!(asked_admin(&admins), Vec::<String>::new(), "{request}");
            assert_eq!(
                asked_bff(&c),
                Vec::<Caller>::new(),
                "{request}: the core was reached"
            );
            assert_eq!(calls(&c), vec![], "{request}");
        }
    }

    /// A `principal` field on a method that takes none is no request: the
    /// principal travels only on an `As` method, so a client that puts it
    /// elsewhere is refused, never run as the caller's own call.
    #[test]
    fn a_principal_on_a_method_that_takes_none_is_a_bad_request() {
        for request in [
            json!({"method": "ListInstances", "principal": "alice"}),
            json!({"method": "Status", "id": "default", "principal": "alice"}),
            json!({"method": "Start", "id": "default", "principal": "alice"}),
            json!({"method": "Stop", "id": "default", "principal": "alice"}),
            json!({"method": "Restart", "id": "default", "principal": "alice"}),
            json!({"method": "ReadFile", "name": "models.ini", "principal": "alice"}),
            json!({"method": "WriteFile", "name": "models.ini", "contents": "", "keep_api_key": false, "principal": "alice"}),
            json!({"method": "WriteFileIf", "name": "models.ini", "contents": "", "expected": "", "principal": "alice"}),
            json!({"method": "BootEnabled", "id": "default", "principal": "alice"}),
            json!({"method": "SetBootEnabled", "id": "default", "enabled": true, "principal": "alice"}),
            json!({"method": "Authenticate", "user": "alice", "password": "s3cret-Pw", "principal": "alice"}),
            json!({"method": "Version", "principal": "alice"}),
            json!({"method": "Start", "id": "default", "principal": 5}),
            json!({"method": "WriteFileIf", "name": "models.ini", "contents": "", "expected": "", "principal": 5}),
            json!({"method": "Start", "id": "default", "principal": null}),
        ] {
            let (c, admins) = principal_core(FakePasswords::new());
            let (result, replies) = serve(&c, bff(), requests(std::slice::from_ref(&request)));
            assert!(result.is_err(), "{request}");
            assert_eq!(replies.len(), 1, "{request}: {replies:?}");
            assert_bad_request(&replies[0], &request.to_string());
            assert_eq!(asked(&c), vec![], "{request}");
            assert_eq!(asked_admin(&admins), Vec::<String>::new(), "{request}");
            assert_eq!(
                asked_bff(&c),
                Vec::<Caller>::new(),
                "{request}: the core was reached"
            );
            assert_eq!(calls(&c), vec![], "{request}");
        }
    }

    /// The rule is the `principal` field alone: every other field the parser
    /// does not know stays ignored, as it is today, so a client of an older
    /// protocol is not refused for what it sends.
    #[test]
    fn a_field_the_manager_does_not_know_is_still_ignored() {
        let (c, admins) = principal_core(FakePasswords::new());
        let (result, replies) = serve(
            &c,
            alice(),
            requests(&[
                json!({"method": "Status", "id": "default", "Principal": "alice"}),
                json!({"method": "Version", "id": "default"}),
                json!({"method": "BootEnabled", "id": "../x", "nonsense": [1, 2]}),
                // `WriteFileIf` carries no `keep_api_key`: the compare and
                // swap needs no key to preserve, so the field is one the
                // manager does not know and the call runs (the name is the
                // core's to refuse, here before any path is resolved).
                json!({"method": "WriteFileIf", "name": "server.ini", "contents": "x", "expected": "", "keep_api_key": true, "nonsense": [1, 2]}),
            ]),
        );
        assert!(result.is_ok(), "{result:?}");
        assert_eq!(
            replies,
            vec![
                json!({"ok": "active"}),
                json!({"ok": {"version": env!("CARGO_PKG_VERSION"), "protocol": 3}}),
                error("InvalidId", "InvalidId"),
                error("BadFileName", "BadFileName"),
            ]
        );
        assert_eq!(calls(&c), vec![Call::Status("default".into())]);
        assert_eq!(asked_admin(&admins), Vec::<String>::new());
    }

    /// An empty `principal` is a principal: the face sends it, and the core
    /// refuses the name. BadRequest is for a request the manager cannot read,
    /// not for a name it refuses, so the connection is kept.
    #[test]
    fn an_empty_principal_reaches_the_core_and_is_refused_by_the_name() {
        let (c, admins) = principal_core(FakePasswords::new());
        let (result, replies) = serve(
            &c,
            bff(),
            requests(&[
                json!({"method": "StopAs", "principal": "", "id": "default"}),
                json!({"method": "WriteFileIfAs", "principal": "", "name": "server.ini", "contents": "x", "expected": ""}),
                json!({"method": "Status", "id": "default"}),
            ]),
        );
        assert!(result.is_ok(), "{result:?}");
        assert_eq!(
            replies,
            vec![
                error("NotAuthorized", "NotAuthorized"),
                error("NotAuthorized", "NotAuthorized"),
                json!({"ok": "active"}),
            ]
        );
        assert_eq!(
            asked_admin(&admins),
            Vec::<String>::new(),
            "the name is refused before any account is looked up"
        );
        assert_eq!(
            calls(&c),
            vec![Call::Status("default".into())],
            "the refused call ran nothing: only the Status probe reached the service manager"
        );
    }

    // ── Version ─────────────────────────────────────────────────────────

    /// `{"method":"Version"}` answers the framework version and the protocol
    /// version, to anyone (the empty caller included), asking no right and
    /// running nothing.
    #[test]
    fn version_answers_the_framework_version_and_the_protocol() {
        for caller in [alice(), bff(), Caller(String::new())] {
            let (c, admins) = principal_core(FakePasswords::new());
            let (result, replies) = serve(
                &c,
                caller.clone(),
                requests(&[
                    json!({"method": "Version"}),
                    json!({"method": "Version", "id": "default"}),
                    json!({"method": "Status", "id": "default"}),
                ]),
            );
            let version = json!({"ok": {"version": env!("CARGO_PKG_VERSION"), "protocol": 3}});
            assert!(result.is_ok(), "{caller:?}: {result:?}");
            assert_eq!(
                replies,
                vec![version.clone(), version, json!({"ok": "active"})],
                "{caller:?}"
            );
            assert_eq!(
                calls(&c),
                vec![Call::Status("default".into())],
                "{caller:?}"
            );
            assert_eq!(
                asked(&c),
                vec![],
                "{caller:?}: a right was asked for the version"
            );
            assert_eq!(asked_bff(&c), Vec::<Caller>::new(), "{caller:?}");
            assert_eq!(asked_admin(&admins), Vec::<String>::new(), "{caller:?}");
        }
    }

    /// The whole protocol at once: pai-core's client, bound to a principal
    /// and asking for the version, against this server over a Unix socket
    /// pair (the named pipe's OS-neutral half, never a real pipe). The client
    /// end has a read deadline: a server that panics or hangs fails the test,
    /// it does not hang.
    #[cfg(unix)]
    #[test]
    fn an_acting_client_round_trips_over_a_socket_pair() {
        use pai_core::client::pipe::PipeClient;
        use pai_core::client::{ClientError, InstanceState, ManagerClient, ManagerVersion};
        use std::os::unix::net::UnixStream;
        use std::time::Duration;

        const DEADLINE: Duration = Duration::from_secs(10);

        let (core, admins) = principal_core(FakePasswords::new());
        let core = Arc::new(core);
        let (server_end, client_end) = UnixStream::pair().expect("socket pair");
        client_end
            .set_read_timeout(Some(DEADLINE))
            .expect("read timeout");
        let served = {
            let core = core.clone();
            let caller = bff();
            std::thread::spawn(move || serve_stream(&core, caller, server_end))
        };
        let client = PipeClient::new(client_end);
        {
            // A block: a bound client that borrows `client` must be gone
            // before the connection is used again.
            let bound = client.acting_for("alice");
            assert_eq!(
                bound.stop("default"),
                Ok(()),
                "the bff stops an instance for a member"
            );
            assert_eq!(
                bound.status("default"),
                Ok(InstanceState::Active),
                "an unguarded method of a bound client"
            );
            // The compare-and-swap write goes over the `As` form for the
            // member: the gate runs before any path is resolved, so a name
            // the core refuses answers BadFileName. The round trip against a
            // data root is in `src/tests/pipe_client.rs`.
            assert_eq!(
                bound.write_file_if("server.ini", "x", ""),
                Err(ClientError::BadFileName),
                "the compare-and-swap write of a bound client, for the member"
            );
        }
        assert_eq!(
            client.version(),
            Ok(Some(ManagerVersion {
                version: env!("CARGO_PKG_VERSION").to_string(),
                protocol: 3,
            })),
            "the version this manager answers"
        );
        assert_eq!(
            calls(&core),
            vec![Call::Stop("default".into()), Call::Status("default".into())],
            "the bound stop ran once for the member, the status probe recorded its own call, nothing else"
        );
        assert_eq!(
            asked_admin(&admins),
            vec!["alice".to_string(); 2],
            "the membership of the name, once per bound call"
        );
        assert_eq!(
            asked(&core),
            vec![],
            "polkit asked, so the call ran as the bff's own"
        );
        drop(client);
        let served = served.join().expect("the server thread panicked");
        assert!(served.is_ok(), "{served:?}");
    }
}
