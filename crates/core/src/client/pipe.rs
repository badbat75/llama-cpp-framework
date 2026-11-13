//! [`ManagerClient`] over any byte stream: the manager's local named pipe on
//! Windows, a socket pair in the tests. The protocol is OS-neutral.
//!
//! One frame per message: the length of the body as a `u32` little-endian,
//! then the body, UTF-8 JSON, at most [`MAX_FRAME`] bytes. The client sends
//! one request and reads its reply before the next. A request names the
//! method as the D-Bus interface does, its arguments as named fields:
//!
//! ```text
//! {"method":"ListInstances"}
//! {"method":"Status","id":"default"}        (also Start, Stop, Restart)
//! {"method":"ReadFile","name":"presets.ini"}
//! {"method":"WriteFile","name":"presets.ini","contents":"..."}
//! ```
//!
//! A reply is the value or an error with the same (name, message) pair the
//! D-Bus interface replies with, so [`ClientError::from_reply`] reads both:
//!
//! ```text
//! {"ok":["broken","default"]}               ListInstances
//! {"ok":"active"}                           Status
//! {"ok":"<text>"} or {"ok":null}            ReadFile (null: no such file)
//! {"ok":null}                               Start, Stop, Restart, WriteFile
//! {"error":{"name":"org.privateai.Manager1.Error.<Kind>","message":"..."}}
//! ```

use super::{ClientError, InstanceState, ManagerClient};
use serde_json::{json, Value};
use std::io::{ErrorKind, Read, Write};
use std::sync::Mutex;

/// The largest body a frame may carry, either way.
pub const MAX_FRAME: u32 = 1 << 20;

/// The manager's client over one stream. A stream that breaks (closed,
/// timed out, any I/O error) is [`ClientError::Unavailable`]; a reply that is
/// not one (bad JSON, the wrong value, over [`MAX_FRAME`]) is
/// [`ClientError::Other`].
pub struct PipeClient<S: Read + Write> {
    stream: Mutex<S>,
}

impl<S: Read + Write> PipeClient<S> {
    pub fn new(stream: S) -> Self {
        Self {
            stream: Mutex::new(stream),
        }
    }
}

impl<S: Read + Write> PipeClient<S> {
    /// Send `request`, read its reply: the value of an `ok`, or the error.
    fn call(&self, request: Value) -> Result<Value, ClientError> {
        let body = request.to_string();
        if body.len() > MAX_FRAME as usize {
            return Err(ClientError::Other(format!("{} > {MAX_FRAME}", body.len())));
        }
        let mut stream = self.stream.lock().unwrap_or_else(|e| e.into_inner());
        write_frame(&mut *stream, body.as_bytes()).map_err(|_| ClientError::Unavailable)?;
        let body = match read_frame(&mut *stream) {
            Ok(Some(body)) => body,
            Ok(None) | Err(FrameError::Io(_)) => return Err(ClientError::Unavailable),
            Err(FrameError::TooLong(len)) => {
                return Err(ClientError::Other(format!("{len} > {MAX_FRAME}")))
            }
        };
        let reply: Value =
            serde_json::from_slice(&body).map_err(|e| ClientError::Other(e.to_string()))?;
        let other = || ClientError::Other(reply.to_string());
        if let Some(error) = reply.get("error") {
            let name = error.get("name").and_then(Value::as_str);
            let message = error.get("message").and_then(Value::as_str);
            return match (name, message) {
                (Some(name), Some(message)) => Err(ClientError::from_reply(name, message)),
                _ => Err(other()),
            };
        }
        reply.get("ok").cloned().ok_or_else(other)
    }

    /// A call whose reply carries no value.
    fn call_unit(&self, request: Value) -> Result<(), ClientError> {
        match self.call(request)? {
            Value::Null => Ok(()),
            v => Err(ClientError::Other(v.to_string())),
        }
    }
}

impl<S: Read + Write> ManagerClient for PipeClient<S> {
    fn list_instances(&self) -> Result<Vec<String>, ClientError> {
        let v = self.call(json!({"method": "ListInstances"}))?;
        serde_json::from_value(v.clone()).map_err(|_| ClientError::Other(v.to_string()))
    }
    fn status(&self, id: &str) -> Result<InstanceState, ClientError> {
        match self.call(json!({"method": "Status", "id": id}))? {
            Value::String(name) => Ok(InstanceState::from_name(&name)),
            v => Err(ClientError::Other(v.to_string())),
        }
    }
    fn start(&self, id: &str) -> Result<(), ClientError> {
        self.call_unit(json!({"method": "Start", "id": id}))
    }
    fn stop(&self, id: &str) -> Result<(), ClientError> {
        self.call_unit(json!({"method": "Stop", "id": id}))
    }
    fn restart(&self, id: &str) -> Result<(), ClientError> {
        self.call_unit(json!({"method": "Restart", "id": id}))
    }
    fn read_file(&self, name: &str) -> Result<Option<String>, ClientError> {
        match self.call(json!({"method": "ReadFile", "name": name}))? {
            Value::String(text) => Ok(Some(text)),
            Value::Null => Ok(None),
            v => Err(ClientError::Other(v.to_string())),
        }
    }
    fn write_file(&self, name: &str, contents: &str) -> Result<(), ClientError> {
        self.call_unit(json!({"method": "WriteFile", "name": name, "contents": contents}))
    }
}

/// Why a frame could not be read.
#[derive(Debug)]
pub enum FrameError {
    /// The stream failed or ended inside the frame.
    Io(std::io::Error),
    /// The frame's length is over [`MAX_FRAME`]; its body was not read.
    TooLong(u32),
}

/// Read one frame's body; None when the stream ends before the frame starts.
pub fn read_frame(r: &mut impl Read) -> Result<Option<Vec<u8>>, FrameError> {
    let mut len = [0u8; 4];
    let mut got = 0;
    while got < len.len() {
        match r.read(&mut len[got..]) {
            Ok(0) if got == 0 => return Ok(None),
            Ok(0) => return Err(FrameError::Io(ErrorKind::UnexpectedEof.into())),
            Ok(n) => got += n,
            Err(e) if e.kind() == ErrorKind::Interrupted => {}
            Err(e) => return Err(FrameError::Io(e)),
        }
    }
    let len = u32::from_le_bytes(len);
    if len > MAX_FRAME {
        return Err(FrameError::TooLong(len));
    }
    let mut body = vec![0u8; len as usize];
    r.read_exact(&mut body).map_err(FrameError::Io)?;
    Ok(Some(body))
}

/// Write `body` as one frame and flush it.
pub fn write_frame(w: &mut impl Write, body: &[u8]) -> std::io::Result<()> {
    let len = u32::try_from(body.len()).map_err(|_| ErrorKind::InvalidInput)?;
    let mut frame = Vec::with_capacity(4 + body.len());
    frame.extend_from_slice(&len.to_le_bytes());
    frame.extend_from_slice(body);
    w.write_all(&frame)?;
    w.flush()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::checks::Refusal;
    use serde_json::{json, Value};
    use std::io::{Cursor, ErrorKind};
    use std::sync::Arc;

    /// A stream that answers with `input` and records what the client sends;
    /// `read_error` / `write_error` make every read / write fail with it.
    struct Script {
        input: Cursor<Vec<u8>>,
        sent: Arc<Mutex<Vec<u8>>>,
        read_error: Option<ErrorKind>,
        write_error: Option<ErrorKind>,
    }

    impl Read for Script {
        fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
            match self.read_error {
                Some(kind) => Err(kind.into()),
                None => self.input.read(buf),
            }
        }
    }

    impl Write for Script {
        fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
            match self.write_error {
                Some(kind) => Err(kind.into()),
                None => {
                    self.sent.lock().unwrap().extend_from_slice(buf);
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

    /// The frames of `replies`, one after the other.
    fn input(replies: &[Value]) -> Vec<u8> {
        replies
            .iter()
            .flat_map(|r| frame(r.to_string().as_bytes()))
            .collect()
    }

    /// A client whose stream answers with `bytes`, and what it sends.
    fn client_on(bytes: Vec<u8>) -> (PipeClient<Script>, Arc<Mutex<Vec<u8>>>) {
        let sent = Arc::new(Mutex::new(Vec::new()));
        let script = Script {
            input: Cursor::new(bytes),
            sent: sent.clone(),
            read_error: None,
            write_error: None,
        };
        (PipeClient::new(script), sent)
    }

    /// The frames in `bytes`, each body parsed as JSON; a short or
    /// non-JSON frame fails the test.
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

    fn sent_frames(sent: &Arc<Mutex<Vec<u8>>>) -> Vec<Value> {
        frames(&sent.lock().unwrap())
    }

    fn error_reply(kind: &str, message: &str) -> Value {
        json!({"error": {"name": format!("org.privateai.Manager1.Error.{kind}"), "message": message}})
    }

    /// One client method, its error when it fails.
    type Call = fn(&PipeClient<Script>) -> Option<ClientError>;

    const EVERY_METHOD: [(&str, Call); 7] = [
        ("list_instances", |c| c.list_instances().err()),
        ("status", |c| c.status("default").err()),
        ("start", |c| c.start("default").err()),
        ("stop", |c| c.stop("default").err()),
        ("restart", |c| c.restart("default").err()),
        ("read_file", |c| c.read_file("presets.ini").err()),
        ("write_file", |c| c.write_file("presets.ini", "").err()),
    ];

    // ── Failures first ──────────────────────────────────────────────────

    #[test]
    fn the_frame_cap_is_one_mebibyte() {
        assert_eq!(MAX_FRAME, 1024 * 1024);
    }

    /// The configurator holds its client as `Box<dyn ManagerClient + Send + Sync>`.
    #[test]
    fn the_client_can_be_shared_across_threads() {
        fn shared<T: Send + Sync>() {}
        shared::<PipeClient<Script>>();
        #[cfg(unix)]
        shared::<PipeClient<std::os::unix::net::UnixStream>>();
    }

    /// A manager that closed the stream before replying: no manager to ask.
    #[test]
    fn no_reply_at_all_is_unavailable() {
        for (method, f) in EVERY_METHOD {
            let (client, _) = client_on(Vec::new());
            assert_eq!(f(&client), Some(ClientError::Unavailable), "{method}");
        }
    }

    #[test]
    fn a_stream_that_ends_inside_a_frame_is_unavailable() {
        let whole = frame(json!({"ok": null}).to_string().as_bytes());
        for cut in [1, 3, 4, whole.len() - 1] {
            let (client, _) = client_on(whole[..cut].to_vec());
            assert_eq!(
                client.start("default"),
                Err(ClientError::Unavailable),
                "cut after {cut} bytes"
            );
        }
    }

    #[test]
    fn a_stream_that_fails_is_unavailable() {
        for kind in [
            ErrorKind::BrokenPipe,
            ErrorKind::ConnectionReset,
            ErrorKind::TimedOut,
            ErrorKind::WouldBlock,
        ] {
            for (method, f) in EVERY_METHOD {
                // The request cannot be sent.
                let script = Script {
                    input: Cursor::new(input(&[json!({"ok": null})])),
                    sent: Arc::new(Mutex::new(Vec::new())),
                    read_error: None,
                    write_error: Some(kind),
                };
                let client = PipeClient::new(script);
                assert_eq!(
                    f(&client),
                    Some(ClientError::Unavailable),
                    "{method}, write {kind:?}"
                );
                // The reply cannot be read (a read timeout included).
                let script = Script {
                    input: Cursor::new(Vec::new()),
                    sent: Arc::new(Mutex::new(Vec::new())),
                    read_error: Some(kind),
                    write_error: None,
                };
                let client = PipeClient::new(script);
                assert_eq!(
                    f(&client),
                    Some(ClientError::Unavailable),
                    "{method}, read {kind:?}"
                );
            }
        }
    }

    /// The other end gone: a socket pair whose server end is closed.
    #[cfg(unix)]
    #[test]
    fn a_closed_peer_is_unavailable() {
        use std::os::unix::net::UnixStream;
        for (method, call) in [
            (
                "list_instances",
                (|c| c.list_instances().err())
                    as fn(&PipeClient<UnixStream>) -> Option<ClientError>,
            ),
            ("status", |c| c.status("default").err()),
            ("start", |c| c.start("default").err()),
            ("write_file", |c| c.write_file("presets.ini", "x").err()),
        ] {
            let (server, client) = UnixStream::pair().expect("socket pair");
            client
                .set_read_timeout(Some(std::time::Duration::from_secs(10)))
                .expect("read timeout");
            drop(server);
            let client = PipeClient::new(client);
            assert_eq!(call(&client), Some(ClientError::Unavailable), "{method}");
        }
    }

    /// A request whose frame would be over the cap is refused before
    /// anything is sent: the server would answer BadRequest and hang up,
    /// which the client could read as the manager gone. The stream stays in
    /// step for the next call.
    #[test]
    fn a_request_over_the_cap_is_other_and_sends_nothing() {
        let (client, sent) = client_on(input(&[json!({"ok": null})]));
        let huge = "x".repeat(MAX_FRAME as usize);
        match client.write_file("presets.ini", &huge) {
            Err(ClientError::Other(detail)) => assert!(!detail.is_empty()),
            other => panic!("a request over the cap: {other:?}"),
        }
        assert!(sent.lock().unwrap().is_empty(), "nothing sent");
        assert_eq!(
            client.start("default"),
            Ok(()),
            "the next call has its reply"
        );
        assert_eq!(
            sent_frames(&sent),
            vec![json!({"method": "Start", "id": "default"})]
        );
    }

    /// A reply whose frame is over the cap is refused, not read.
    #[test]
    fn a_reply_over_the_cap_is_other() {
        let mut bytes = (MAX_FRAME + 1).to_le_bytes().to_vec();
        bytes.extend_from_slice(b"{\"ok\":null}");
        let (client, _) = client_on(bytes);
        match client.start("default") {
            Err(ClientError::Other(detail)) => assert!(!detail.is_empty()),
            other => panic!("a reply over the cap: {other:?}"),
        }
        let (client, _) = client_on(u32::MAX.to_le_bytes().to_vec());
        assert!(
            matches!(client.list_instances(), Err(ClientError::Other(_))),
            "a 4 GiB length"
        );
    }

    #[test]
    fn a_reply_that_is_not_json_is_other() {
        for body in [
            &b""[..],
            b"not json",
            b"{\"ok\":",
            b"\xff\xfe{\"ok\":null}",
            b"[\"ok\", null]",
        ] {
            let (client, _) = client_on(frame(body));
            assert!(
                matches!(client.start("default"), Err(ClientError::Other(_))),
                "{body:?}"
            );
        }
    }

    /// A reply that is neither a value nor an error, or the wrong value for
    /// the method, is Other, never a panic.
    #[test]
    fn a_reply_of_the_wrong_shape_is_other() {
        let cases: [(&str, Call, Value); 9] = [
            (
                "list_instances",
                |c| c.list_instances().err(),
                json!({"ok": "default"}),
            ),
            (
                "list_instances",
                |c| c.list_instances().err(),
                json!({"ok": [1, 2]}),
            ),
            (
                "list_instances",
                |c| c.list_instances().err(),
                json!({"ok": null}),
            ),
            ("status", |c| c.status("default").err(), json!({"ok": 5})),
            ("status", |c| c.status("default").err(), json!({"ok": null})),
            (
                "read_file",
                |c| c.read_file("presets.ini").err(),
                json!({"ok": true}),
            ),
            ("start", |c| c.start("default").err(), json!({})),
            (
                "start",
                |c| c.start("default").err(),
                json!({"error": "NotAuthorized"}),
            ),
            (
                "start",
                |c| c.start("default").err(),
                json!({"error": {"name": "org.privateai.Manager1.Error.NotAuthorized"}}),
            ),
        ];
        for (method, f, reply) in cases {
            let (client, _) = client_on(input(std::slice::from_ref(&reply)));
            assert!(
                matches!(f(&client), Some(ClientError::Other(_))),
                "{method} given {reply}"
            );
        }
    }

    /// Every error reply maps exactly as over D-Bus, through `from_reply`.
    #[test]
    fn error_replies_map_like_dbus() {
        let refusal = Refusal {
            key: "not-whole-number-in-range",
            values: vec!["Port".into(), "70000".into(), "1".into(), "65535".into()],
        };
        let cases = [
            (
                error_reply("NotAuthorized", "NotAuthorized"),
                ClientError::NotAuthorized,
            ),
            (
                error_reply("InvalidId", "InvalidId"),
                ClientError::InvalidId,
            ),
            (
                error_reply("NoInstance", "NoInstance"),
                ClientError::NoInstance,
            ),
            (
                error_reply("BadFileName", "BadFileName"),
                ClientError::BadFileName,
            ),
            (
                error_reply("AddressInUse", "default"),
                ClientError::AddressInUse("default".into()),
            ),
            (
                error_reply("UnresolvableHost", "nowhere.invalid"),
                ClientError::UnresolvableHost("nowhere.invalid".into()),
            ),
            (
                error_reply("Service", "llama.cpp@broken.service failed"),
                ClientError::Service("llama.cpp@broken.service failed".into()),
            ),
            (
                error_reply("Io", "permission denied"),
                ClientError::Io("permission denied".into()),
            ),
            (
                error_reply("Invalid", &refusal.encode()),
                ClientError::Invalid(refusal.clone()),
            ),
        ];
        for (reply, expected) in cases {
            for (method, f) in EVERY_METHOD {
                let (client, _) = client_on(input(std::slice::from_ref(&reply)));
                assert_eq!(f(&client), Some(expected.clone()), "{method} given {reply}");
            }
        }
        // A name this client does not know, the manager's malformed-request
        // reply among them, keeps its name and message.
        for (name, message) in [
            ("org.privateai.Manager1.Error.BadRequest", "not a request"),
            ("org.privateai.Manager1.Error.NewKind", "detail"),
        ] {
            let reply = json!({"error": {"name": name, "message": message}});
            let (client, _) = client_on(input(&[reply]));
            match client.start("default") {
                Err(ClientError::Other(detail)) => {
                    assert!(
                        detail.contains(name) && detail.contains(message),
                        "{detail:?}"
                    )
                }
                other => panic!("{name}: {other:?}"),
            }
        }
    }

    // ── The wire ────────────────────────────────────────────────────────

    /// Each method sends one frame, its request named and shaped as pinned
    /// in the module header, and reads one reply.
    #[test]
    fn every_method_sends_its_request() {
        let contents = "[Qwen 3 (v2) é]\nmodel = /models/q.gguf\n";
        let cases: [(Call, Value); 7] = [
            (
                |c| c.list_instances().err(),
                json!({"method": "ListInstances"}),
            ),
            (
                |c| c.status("lan-2").err(),
                json!({"method": "Status", "id": "lan-2"}),
            ),
            (
                |c| c.start("lan-2").err(),
                json!({"method": "Start", "id": "lan-2"}),
            ),
            (
                |c| c.stop("lan-2").err(),
                json!({"method": "Stop", "id": "lan-2"}),
            ),
            (
                |c| c.restart("lan-2").err(),
                json!({"method": "Restart", "id": "lan-2"}),
            ),
            (
                |c| c.read_file("server@lan-2.ini").err(),
                json!({"method": "ReadFile", "name": "server@lan-2.ini"}),
            ),
            (
                |c| {
                    c.write_file("presets.ini", "[Qwen 3 (v2) é]\nmodel = /models/q.gguf\n")
                        .err()
                },
                json!({"method": "WriteFile", "name": "presets.ini", "contents": contents}),
            ),
        ];
        for (f, request) in cases {
            // Any reply will do: what is pinned here is the request.
            let (client, sent) = client_on(input(&[error_reply("InvalidId", "InvalidId")]));
            assert_eq!(f(&client), Some(ClientError::InvalidId), "{request}");
            assert_eq!(sent_frames(&sent), vec![request.clone()], "{request}");
        }
    }

    #[test]
    fn every_value_reply_is_read() {
        let (client, _) = client_on(input(&[json!({"ok": ["broken", "default", "lan-2"]})]));
        assert_eq!(
            client.list_instances(),
            Ok(vec!["broken".into(), "default".into(), "lan-2".into()])
        );
        let (client, _) = client_on(input(&[json!({"ok": []})]));
        assert_eq!(client.list_instances(), Ok(vec![]));

        for (name, state) in [
            ("active", InstanceState::Active),
            ("inactive", InstanceState::Inactive),
            ("failed", InstanceState::Failed),
            ("activating", InstanceState::Activating),
            ("deactivating", InstanceState::Deactivating),
            ("maintenance", InstanceState::Other("maintenance".into())),
        ] {
            let (client, _) = client_on(input(&[json!({"ok": name})]));
            assert_eq!(client.status("default"), Ok(state), "{name}");
        }

        for (method, f) in [
            (
                "start",
                (|c| c.start("default")) as fn(&PipeClient<Script>) -> Result<(), ClientError>,
            ),
            ("stop", |c| c.stop("default")),
            ("restart", |c| c.restart("default")),
            ("write_file", |c| {
                c.write_file("presets.ini", "[m]\nmodel = /m.gguf\n")
            }),
        ] {
            let (client, _) = client_on(input(&[json!({"ok": null})]));
            assert_eq!(f(&client), Ok(()), "{method}");
        }

        let text = "[Qwen3.8-27B]\nmodel = /models/Qwen3.8-27B-Q8_0.gguf\n";
        let (client, _) = client_on(input(&[json!({"ok": text})]));
        assert_eq!(client.read_file("presets.ini"), Ok(Some(text.to_string())));
        let (client, _) = client_on(input(&[json!({"ok": null})]));
        assert_eq!(
            client.read_file("presets.ini.disabled"),
            Ok(None),
            "no such file"
        );
        let (client, _) = client_on(input(&[json!({"ok": ""})]));
        assert_eq!(
            client.read_file("presets.ini.disabled"),
            Ok(Some(String::new())),
            "an empty file is not a missing one"
        );
    }

    /// Calls in a row each read their own reply, in order, on one stream.
    #[test]
    fn calls_in_a_row_read_their_own_replies() {
        let (client, sent) = client_on(input(&[
            json!({"ok": ["default"]}),
            error_reply("NotAuthorized", "NotAuthorized"),
            json!({"ok": "inactive"}),
        ]));
        assert_eq!(client.list_instances(), Ok(vec!["default".to_string()]));
        assert_eq!(client.start("default"), Err(ClientError::NotAuthorized));
        assert_eq!(client.status("default"), Ok(InstanceState::Inactive));
        assert_eq!(
            sent_frames(&sent),
            vec![
                json!({"method": "ListInstances"}),
                json!({"method": "Start", "id": "default"}),
                json!({"method": "Status", "id": "default"}),
            ]
        );
    }
}
