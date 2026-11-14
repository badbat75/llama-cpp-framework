//! The manager over a byte stream: the face of `pai-manager` on Windows,
//! where the configurator reaches it over a local named pipe. OS-neutral:
//! the frame format and the requests are `pai_core::client::pipe`'s (its
//! module header), the errors the same (name, message) pairs the D-Bus
//! interface replies with. The pipe itself, its security descriptor and the
//! caller's identity are the Windows side's.

pub use super::service::error_reply;
use super::service::{
    state_name, Authority, Caller, Manager, ManagerError, Resolver, ServiceControl,
};
use pai_core::client::pipe::{read_frame, write_frame, FrameError};
use serde_json::{json, Value};
use std::io::{Error, ErrorKind, Read, Write};

/// The error name of the reply to a frame that is no request (not JSON, an
/// unknown method, a missing argument, over `MAX_FRAME`); the connection
/// ends after it.
pub const BAD_REQUEST: &str = "org.privateai.Manager1.Error.BadRequest";

/// A request, its arguments read (`pai_core::client::pipe`'s header).
enum Request {
    ListInstances,
    Status(String),
    Start(String),
    Stop(String),
    Restart(String),
    ReadFile(String),
    WriteFile(String, String),
}

impl Request {
    /// The request a frame's body holds, None when it holds none.
    fn parse(body: &[u8]) -> Option<Request> {
        let v: Value = serde_json::from_slice(body).ok()?;
        let arg = |name: &str| v.get(name)?.as_str().map(str::to_string);
        Some(match v.get("method")?.as_str()? {
            "ListInstances" => Request::ListInstances,
            "Status" => Request::Status(arg("id")?),
            "Start" => Request::Start(arg("id")?),
            "Stop" => Request::Stop(arg("id")?),
            "Restart" => Request::Restart(arg("id")?),
            "ReadFile" => Request::ReadFile(arg("name")?),
            "WriteFile" => Request::WriteFile(arg("name")?, arg("contents")?),
            _ => return None,
        })
    }
}

/// The caller of a change: one with no identity is refused before anybody
/// is asked, as a D-Bus call without a sender is (`dbus::caller`).
fn changer(caller: &Caller) -> Result<&Caller, ManagerError> {
    if caller.0.is_empty() {
        Err(ManagerError::NotAuthorized)
    } else {
        Ok(caller)
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
        Request::Start(id) => changer(caller)
            .and_then(|c| manager.start(c, &id))
            .map(|()| Value::Null),
        Request::Stop(id) => changer(caller)
            .and_then(|c| manager.stop(c, &id))
            .map(|()| Value::Null),
        Request::Restart(id) => changer(caller)
            .and_then(|c| manager.restart(c, &id))
            .map(|()| Value::Null),
        Request::ReadFile(name) => manager
            .read_file(changer(caller).ok(), &name)
            .map(|text| json!(text)),
        Request::WriteFile(name, contents) => changer(caller)
            .and_then(|c| manager.write_file(c, &name, &contents))
            .map(|()| Value::Null),
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
/// empty one is refused every change, nobody asked). Ok when the stream ends
/// between frames; an error when it ends inside one, fails, or carries a
/// frame that is no request (answered with [`BAD_REQUEST`] first, the rest
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
    use super::super::service::fakes::{Call, FakeAuthority, FakeResolver, FakeService};
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
        let bodies: [&[u8]; 12] = [
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
            b"{\"method\":\"WriteFile\",\"name\":\"presets.ini\"}",
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
                json!({"method": "WriteFile", "name": "presets.ini", "contents": "[x]\nmodel = /m/x.gguf\n"}),
            ]),
        );
        assert!(result.is_ok(), "{result:?}");
        assert_eq!(replies, vec![error("NotAuthorized", "NotAuthorized"); 4]);
        assert_eq!(calls(&c), vec![]);
        assert_eq!(
            asked(&c),
            vec![
                (alice(), Action::Control),
                (alice(), Action::Control),
                (alice(), Action::Control),
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
                json!({"method": "WriteFile", "name": "presets.ini", "contents": ""}),
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
                refused,
                json!({"ok": "active"}),
            ],
            "an instance file's read is refused like a change; status needs no caller"
        );
        assert_eq!(asked(&c), vec![], "nobody asked");
        assert_eq!(calls(&c), vec![Call::Status("default".into())]);
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
                json!({"method": "WriteFile", "name": "server.ini", "contents": "x"}),
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
                    key: "invalid-preset-id",
                    values: vec!["Qwen 3 (v2) é".into()],
                }),
                "Invalid",
            ),
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
}
