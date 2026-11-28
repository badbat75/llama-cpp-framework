use super::*;
use crate::checks::Refusal;
use crate::client::NodeInfoReply;
use crate::host::OsInfo;
use crate::vulkan::{NodeGpu, Status};
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

/// The list one tags call sends.
fn tags() -> Vec<String> {
    vec!["edge".to_string(), "v2".to_string()]
}

const EVERY_METHOD: [(&str, Call); 18] = [
    ("list_instances", |c| c.list_instances().err()),
    ("instances", |c| c.instances().err()),
    ("devices", |c| c.devices().err()),
    ("node_info", |c| c.node_info().err()),
    ("status", |c| c.status("default").err()),
    ("start", |c| c.start("default").err()),
    ("stop", |c| c.stop("default").err()),
    ("restart", |c| c.restart("default").err()),
    ("read_file", |c| c.read_file("models.ini").err()),
    ("write_file", |c| {
        c.write_file("models.ini", "", false).err()
    }),
    ("boot_enabled", |c| c.boot_enabled("default").err()),
    ("set_boot_enabled", |c| {
        c.set_boot_enabled("default", true).err()
    }),
    ("set_model_profile", |c| {
        c.set_model_profile("default", "Qwen 3 (v2) é", "on").err()
    }),
    ("set_server_profile", |c| {
        c.set_server_profile("lan-2", "Qwen 3 (v2) é").err()
    }),
    ("set_instance_tags", |c| {
        c.set_instance_tags("lan-2", &tags()).err()
    }),
    ("set_instance_label", |c| {
        c.set_instance_label("lan-2", "Coding box").err()
    }),
    ("authenticate", |c| {
        c.authenticate("alice", "s3cret-Pw").err()
    }),
    ("version", |c| c.version().err()),
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
            (|c| c.list_instances().err()) as fn(&PipeClient<UnixStream>) -> Option<ClientError>,
        ),
        ("status", |c| c.status("default").err()),
        ("start", |c| c.start("default").err()),
        ("write_file", |c| {
            c.write_file("models.ini", "x", false).err()
        }),
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
    match client.write_file("models.ini", &huge, false) {
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

/// The list `Instances` answers, read as it is: the two tag lists kept
/// apart and in the order the manager gave them, every row kept, and a
/// manager with no instance answered an empty list.
#[test]
fn the_instances_reply_is_the_list_the_manager_answers() {
    let (client, _) = client_on(input(&[json!({"ok": [
        {"id": "default", "engine": "llama.cpp", "server_profile": "big", "port": 8080, "profile_tags": ["edge", "v2"], "instance_tags": []},
        {"id": "lan-2", "engine": "llama.cpp", "server_profile": "edge-2", "port": 8081, "profile_tags": [], "instance_tags": ["gpu"]},
        {"id": "solo", "engine": "llama.cpp", "server_profile": "", "port": 0, "profile_tags": [], "instance_tags": []},
    ]})]));
    assert_eq!(
        client.instances().expect("the list"),
        vec![
            InstanceInfo {
                id: "default".into(),
                engine: "llama.cpp".into(),
                server_profile: Some("big".into()),
                port: 8080,
                profile_tags: vec!["edge".into(), "v2".into()],
                instance_tags: vec![],
                label: None,
                converged: true,
                reason: None,
                pending_restart: false,
                // The row sent carries neither of the fields appended in
                // IP4b, and they read as none.
                instance_profile: None,
                hostname: None,
            },
            InstanceInfo {
                id: "lan-2".into(),
                engine: "llama.cpp".into(),
                server_profile: Some("edge-2".into()),
                port: 8081,
                profile_tags: vec![],
                instance_tags: vec!["gpu".into()],
                label: None,
                converged: true,
                reason: None,
                pending_restart: false,
                instance_profile: None,
                hostname: None,
            },
        ],
        "the row's fields, in the manager's order"
    );
    let (client, _) = client_on(input(&[json!({"ok": []})]));
    assert_eq!(
        client.instances().expect("no instances"),
        Vec::<InstanceInfo>::new(),
        "no instance is an empty list"
    );
}

/// The row of slice 13 carries `server_profile`, the `ServerProfile` of
/// the instance's section: every client that reads a server file needs the
/// name, because `server@<id>.ini` is not the file the instance uses. A
/// row of an older manager, which answers no such field, is not a row with
/// an empty profile: it is a reply the client cannot use, said as `Other`
/// like every row of the wrong shape, so no client reads or writes a
/// `server@.ini`.
#[test]
fn an_instances_row_without_the_server_profile_is_the_old_shape() {
    let (client, _) = client_on(input(&[json!({"ok": [
        {"id": "default", "engine": "llama.cpp", "port": 8080, "profile_tags": [], "instance_tags": []},
    ]})]));
    let got = client.instances();
    assert!(
        matches!(got, Err(ClientError::Other(_))),
        "an older manager's list, a row naming no profile, is refused: {got:?}"
    );
}

/// The `label` of slice IL1, read as the design answers it: the text the
/// manager wrote, `null` for a section with no `Label` key (the web UI
/// falls back to the engine and the port), and a row of an older manager
/// that answers no field at all is no label, not a refused list. A field
/// that is no text and not `null` is a reply of the wrong shape, said as
/// `Other` like every other one.
#[test]
fn an_instances_row_label_is_the_text_null_or_no_field() {
    let row = |label: &str| {
        format!(
            "{{\"id\": \"default\", \"engine\": \"llama.cpp\", \"server_profile\": \"big\", \
             \"port\": 8080, \"profile_tags\": [], \"instance_tags\": [], \"label\": {label}}}"
        )
    };
    for (body, want) in [
        (row("\"Coding box\""), Some("Coding box")),
        (row("null"), None),
    ] {
        let (client, _) = client_on(input(&[
            json!({"ok": [serde_json::from_str::<serde_json::Value>(&body).expect("row")]}),
        ]));
        let rows = client.instances().expect("the list");
        assert_eq!(
            rows[0].label.as_deref(),
            want,
            "the row reads {want:?} out of {body}"
        );
    }
    // The field is new: a manager that does not know it answers no field,
    // and its list is still a list, with no label on any row.
    let (client, _) = client_on(input(&[json!({"ok": [
        {"id": "default", "engine": "llama.cpp", "server_profile": "big", "port": 8080, "profile_tags": [], "instance_tags": []},
    ]})]));
    let rows = client.instances().expect("the older manager's list");
    assert_eq!(
        rows.len(),
        1,
        "a row with no label field is a row like any other"
    );
    assert_eq!(
        rows[0].label, None,
        "no field is no label, not a failure of the list"
    );
    for label in ["5", "true", "[]", "{}"] {
        let (client, _) = client_on(input(&[
            json!({"ok": [serde_json::from_str::<serde_json::Value>(&row(label)).expect("row")]}),
        ]));
        assert!(
            matches!(client.instances(), Err(ClientError::Other(_))),
            "a label that is no text: {label}"
        );
    }
}

// ── The node's devices (G2, ARCHITECTURE.md 1) ──────────────────────

/// The device list `Devices` answers, read as it is: every row kept, in the
/// manager's order, with the id, the product name, the backend, both sizes,
/// the moment of the probe, and the host's inventory R1 adds beside the
/// list: `ram_mib`, the host's total RAM in MiB, and `cpu_threads`, its
/// logical CPUs (ARCHITECTURE.md (1) "The node inventory").
#[test]
fn the_devices_reply_is_the_list_the_manager_answers() {
    let (client, sent) = client_on(input(&[json!({"ok": {
        "probed_at": 1_766_271_243u64,
        "ram_mib": 3_932_160u64,
        "cpu_threads": 8u64,
        "devices": [
            {"id": "ROCm0", "name": "AMD Radeon AI PRO R9700", "backend": "ROCm", "total_mib": 32624u64, "free_mib": 32420u64},
            {"id": "CUDA0", "name": "NVIDIA GeForce RTX 4070 SUPER", "backend": "CUDA", "total_mib": 12281u64, "free_mib": 10844u64},
            {"id": "Vulkan0", "name": "AMD Radeon AI PRO R9700", "backend": "Vulkan", "total_mib": 32624u64, "free_mib": 32420u64},
        ],
    }})]));
    assert_eq!(
        client.devices().expect("the device list"),
        NodeDevices {
            probed_at: Some(1_766_271_243),
            ram_mib: 3_932_160,
            cpu_threads: 8,
            devices: vec![
                DeviceInfo {
                    id: "ROCm0".into(),
                    name: "AMD Radeon AI PRO R9700".into(),
                    backend: "ROCm".into(),
                    total_mib: 32624,
                    free_mib: 32420,
                },
                DeviceInfo {
                    id: "CUDA0".into(),
                    name: "NVIDIA GeForce RTX 4070 SUPER".into(),
                    backend: "CUDA".into(),
                    total_mib: 12281,
                    free_mib: 10844,
                },
                DeviceInfo {
                    id: "Vulkan0".into(),
                    name: "AMD Radeon AI PRO R9700".into(),
                    backend: "Vulkan".into(),
                    total_mib: 32624,
                    free_mib: 32420,
                },
            ],
        },
        "the row's fields, in the manager's order"
    );
    assert_eq!(
        sent_frames(&sent),
        vec![json!({"method": "Devices"})],
        "the plain method, with no argument"
    );
}

/// A manager that never got a probe answered: `probed_at` 0 is no time
/// (None), and no device is an empty list. The page shows the empty list
/// with no time, never an error. The host's inventory is answered all the
/// same: it is the host's, not the engine's, so a node with no probe
/// reports what it is.
#[test]
fn a_devices_reply_of_no_probe_is_no_time_and_an_empty_list() {
    for (probed_at, devices) in [
        (0u64, json!([])),
        (
            0u64,
            json!([{"id": "CPU", "name": "CPU", "backend": "CPU", "total_mib": 0u64, "free_mib": 0u64}]),
        ),
    ] {
        let (client, _) = client_on(input(&[json!({"ok": {
            "probed_at": probed_at,
            "ram_mib": 3_932_160u64,
            "cpu_threads": 8u64,
            "devices": devices,
        }})]));
        let got = client.devices().expect("the empty device list");
        assert_eq!(got.probed_at, None, "0 is no probe ({probed_at})");
        assert_eq!(got.devices.len(), devices.as_array().unwrap().len());
        assert_eq!(
            (got.ram_mib, got.cpu_threads),
            (3_932_160, 8),
            "no probe leaves the inventory answered ({probed_at})"
        );
    }
}

/// A device row of the wrong shape is Other, like every other row: a client
/// that guessed at a missing field would show a card that is not there. The
/// inventory is read the same way: a reply with no `ram_mib`, no
/// `cpu_threads`, one of them not a number, a negative one, or a thread
/// count past a uint32, is refused (R1). The pair ships with protocol 3,
/// which is in no release, so a reply without it is an incomprehensible
/// shape, not an older manager to answer.
#[test]
fn a_devices_row_of_the_wrong_shape_is_other() {
    for row in [
        json!({"id": "ROCm0", "name": "AMD", "backend": "ROCm", "total_mib": 1u64}),
        json!({"id": "ROCm0", "name": "AMD", "total_mib": 1u64, "free_mib": 1u64}),
        json!({"name": "AMD", "backend": "ROCm", "total_mib": 1u64, "free_mib": 1u64}),
        json!({"id": "ROCm0", "name": "AMD", "backend": 1u64, "total_mib": 1u64, "free_mib": 1u64}),
        json!({"id": "ROCm0", "name": "AMD", "backend": "ROCm", "total_mib": "1", "free_mib": 1u64}),
        json!({"id": "ROCm0", "name": "AMD", "backend": "ROCm", "total_mib": -1i64, "free_mib": 1u64}),
        json!("not a row"),
    ] {
        let (client, _) = client_on(input(&[json!({"ok": {
            "probed_at": 1u64,
            "ram_mib": 1u64,
            "cpu_threads": 1u64,
            "devices": [row],
        }})]));
        let got = client.devices();
        assert!(
            matches!(got, Err(ClientError::Other(_))),
            "{row}: a row of the wrong shape is refused: {got:?}"
        );
    }
    // The reply itself of the wrong shape: not an object, no devices list,
    // a devices list that is not a list, no probed_at, and no inventory.
    for reply in [
        json!({"ok": []}),
        json!({"ok": null}),
        json!({"ok": {"devices": []}}),
        json!({"ok": {"probed_at": 1u64}}),
        json!({"ok": {"probed_at": 1u64, "devices": {}}}),
        json!({"ok": {"probed_at": "1", "devices": []}}),
        json!({"ok": {"probed_at": 1u64, "devices": [], "cpu_threads": 1u64}}),
        json!({"ok": {"probed_at": 1u64, "devices": [], "ram_mib": 1u64}}),
        json!({"ok": {"probed_at": 1u64, "devices": [], "ram_mib": "64 GiB", "cpu_threads": 1u64}}),
        json!({"ok": {"probed_at": 1u64, "devices": [], "ram_mib": -1i64, "cpu_threads": 1u64}}),
        json!({"ok": {"probed_at": 1u64, "devices": [], "ram_mib": 1u64, "cpu_threads": -1i64}}),
        json!({"ok": {"probed_at": 1u64, "devices": [], "ram_mib": 1u64, "cpu_threads": 4294967296u64}}),
    ] {
        let (client, _) = client_on(input(std::slice::from_ref(&reply)));
        let got = client.devices();
        assert!(
            matches!(got, Err(ClientError::Other(_))),
            "{reply}: a reply of the wrong shape is refused: {got:?}"
        );
    }
}

/// A client acting for the signed-in user asks `DevicesAs`, with the
/// principal: the manager reads the devices for a member of `pai`, and the
/// bff always asks this way.
#[test]
fn an_acting_client_asks_the_devices_for_the_signed_in_user() {
    let (client, sent) = client_on(input(&[json!({"ok": {
        "probed_at": 0u64,
        "ram_mib": 0u64,
        "cpu_threads": 0u64,
        "devices": [],
    }})]));
    let bound = client.acting_for("alice");
    assert_eq!(
        bound.devices().expect("the empty device list").devices,
        Vec::<DeviceInfo>::new()
    );
    assert_eq!(
        sent_frames(&sent),
        vec![json!({"method": "DevicesAs", "principal": "alice"})],
        "the As form, with the principal as given"
    );
}

/// The refusals of the devices read cross the wire like every other
/// method's: the gate's `NotAuthorized` for a caller without `control`, and
/// the same word for a `DevicesAs` principal who is no member of `pai`
/// (the bff answers that 403 and ends the session). The client's own
/// `Unavailable` is the manager gone, pinned for this method by
/// `no_reply_at_all_is_unavailable`: the wire has no kind for it, so
/// nothing here asks for one.
#[test]
fn a_devices_refusal_is_the_error_the_manager_sent() {
    let (client, _) = client_on(input(&[error_reply("NotAuthorized", "NotAuthorized")]));
    assert_eq!(
        client.devices(),
        Err(ClientError::NotAuthorized),
        "the plain form, refused by the quiet `control` check"
    );
    let (client, sent) = client_on(input(&[error_reply("NotAuthorized", "NotAuthorized")]));
    let bound = client.acting_for("alice");
    assert_eq!(
        bound.devices(),
        Err(ClientError::NotAuthorized),
        "the As form, refused for a principal outside `pai`"
    );
    assert_eq!(
        sent_frames(&sent),
        vec![json!({"method": "DevicesAs", "principal": "alice"})],
        "the refusal answers the As call"
    );
}

// ── The node's info (AN3, ARCHITECTURE.md "About and node info") ──────
//
// The document the design answers: `os` (`name`, `version`, `kernel`),
// `vulkan` (one of `ok`, `none`, `failed`) and `gpus` (rows of `pci`,
// `vendor`, `name`, `driver`, `version` and the `vulkan` pair `driver`,
// `info`), every text a string or null. Names the tests assume, the
// contract for the coder: the stream's `NodeInfo` / `NodeInfoAs` and
// `ManagerClient::node_info()`. The manager's serializer writes every key,
// so a key that is not there is the shape of no manager, not a fact the
// host left out: the one that is answered with nothing is the `null`.

/// The document of a node with two kernel GPUs, one of them carrying a
/// Vulkan device, and a third row of a Vulkan device no kernel row carries
/// (no bus id, so nothing to join it onto).
fn node_info_document() -> Value {
    json!({
        "os": {"name": "Ubuntu", "version": "26.04", "kernel": "6.14.0-24-generic"},
        "vulkan": "ok",
        "gpus": [
            {"pci": "0000:03:00.0", "vendor": "1002", "name": "AMD Radeon AI PRO R9700",
             "driver": "amdgpu", "version": "2.5.0",
             "vulkan": {"driver": "RADV", "info": "Mesa 25.2.0"}},
            {"pci": "0000:08:00.0", "vendor": "10de", "name": null, "driver": "nvidia",
             "version": null, "vulkan": null},
            {"pci": null, "vendor": null, "name": "NVIDIA GeForce RTX 4070 SUPER",
             "driver": null, "version": null,
             "vulkan": {"driver": "NVIDIA", "info": "560.35.03"}},
        ],
    })
}

/// The node's info `NodeInfo` answers, read as it is: the OS the host
/// named, the Vulkan word the child answered, and every row in the
/// manager's order with its bus id, its vendor, its name, its kernel
/// driver and version, and the Vulkan driver joined onto it. A `null` of
/// the document is a fact the host did not answer, so it arrives as `None`
/// (the page shows a hole, never a guess).
#[test]
fn the_node_info_reply_is_the_document_the_manager_answers() {
    let (client, sent) = client_on(input(&[json!({"ok": node_info_document()})]));
    assert_eq!(
        client.node_info().expect("the node's info"),
        NodeInfoReply {
            os: OsInfo {
                name: Some("Ubuntu".into()),
                version: Some("26.04".into()),
                kernel: Some("6.14.0-24-generic".into()),
            },
            vulkan: Status::Ok,
            gpus: vec![
                NodeGpu {
                    pci: Some("0000:03:00.0".into()),
                    vendor: Some("1002".into()),
                    name: Some("AMD Radeon AI PRO R9700".into()),
                    driver: Some("amdgpu".into()),
                    version: Some("2.5.0".into()),
                    vulkan: Some(crate::vulkan::Driver {
                        driver: Some("RADV".into()),
                        info: Some("Mesa 25.2.0".into()),
                    }),
                },
                NodeGpu {
                    pci: Some("0000:08:00.0".into()),
                    vendor: Some("10de".into()),
                    name: None,
                    driver: Some("nvidia".into()),
                    version: None,
                    vulkan: None,
                },
                NodeGpu {
                    pci: None,
                    vendor: None,
                    name: Some("NVIDIA GeForce RTX 4070 SUPER".into()),
                    driver: None,
                    version: None,
                    vulkan: Some(crate::vulkan::Driver {
                        driver: Some("NVIDIA".into()),
                        info: Some("560.35.03".into()),
                    }),
                },
            ],
        },
        "every field of the document, in the manager's order"
    );
    assert_eq!(
        sent_frames(&sent),
        vec![json!({"method": "NodeInfo"})],
        "the plain method, with no argument"
    );
}

/// The three words of the contract, each read as itself: a node with no
/// loader and no ICD answers `none` and no row, a read that gave nothing to
/// trust answers `failed`, and the OS of a node that reads nothing is all
/// `null`. None of them is an error the caller has to answer: the page
/// shows what the node is.
#[test]
fn the_three_vulkan_words_and_their_nulls_are_answers() {
    for (word, status) in [
        ("ok", Status::Ok),
        ("none", Status::None),
        ("failed", Status::Failed),
    ] {
        let (client, sent) = client_on(input(&[json!({"ok": {
            "os": {"name": null, "version": null, "kernel": null},
            "vulkan": word,
            "gpus": [],
        }})]));
        let info = client.node_info().expect("the empty node info");
        assert_eq!(info.vulkan, status, "`{word}` is the word of the contract");
        assert!(info.gpus.is_empty(), "`{word}` with no row");
        assert_eq!(
            info.os,
            OsInfo {
                name: None,
                version: None,
                kernel: None,
            },
            "`{word}`: a null OS field is no value, never a guessed one"
        );
        assert_eq!(
            sent_frames(&sent),
            vec![json!({"method": "NodeInfo"})],
            "`{word}` asked plainly"
        );
    }
}

/// A reply of the wrong shape is Other, like every other reply: a client
/// that guessed at a missing key would show a driver the node does not
/// have. The whole reply, the `os` object, the `vulkan` word, the `gpus`
/// list, one row and the `vulkan` pair of a row, are each judged.
#[test]
fn a_node_info_reply_of_the_wrong_shape_is_other() {
    // Positive control first: the document of the fixture IS read, so a
    // refusal below is the shape's fault and not the method's absence.
    // Like every reply in this file, it rides as the value of `ok`.
    let (client, _) = client_on(input(&[json!({"ok": node_info_document()})]));
    let good = client.node_info().expect("the fixture's document");
    assert_eq!(good.gpus.len(), 3, "the fixture has three rows");
    assert_eq!(good.vulkan, Status::Ok, "the fixture reads `ok`");
    // The whole reply.
    for reply in [
        json!({"ok": null}),
        json!({"ok": []}),
        json!({"ok": {"vulkan": "ok", "gpus": []}}),
        json!({"ok": {"os": {}, "gpus": []}}),
        json!({"ok": {"os": {}, "vulkan": "ok"}}),
        json!({"ok": {"os": {}, "vulkan": "ok", "gpus": {}}}),
    ] {
        let (client, _) = client_on(input(std::slice::from_ref(&reply)));
        let got = client.node_info();
        assert!(
            matches!(got, Err(ClientError::Other(_))),
            "{reply}: a reply of the wrong shape is refused: {got:?}"
        );
    }
    // The `os` object and the `vulkan` word.
    for os in [
        json!(null),
        json!("Ubuntu"),
        json!({"version": "26.04", "kernel": "6.14.0"}),
        json!({"name": "Ubuntu", "kernel": "6.14.0"}),
        json!({"name": "Ubuntu", "version": "26.04"}),
        json!({"name": 1, "version": "26.04", "kernel": "6.14.0"}),
        json!({"name": "Ubuntu", "version": ["26.04"], "kernel": "6.14.0"}),
    ] {
        let (client, _) = client_on(input(&[json!({"ok": {
            "os": os, "vulkan": "ok", "gpus": [],
        }})]));
        let got = client.node_info();
        assert!(
            matches!(got, Err(ClientError::Other(_))),
            "os {os}: refused: {got:?}"
        );
    }
    for vulkan in [
        json!(null),
        json!("OK"),
        json!("unknown"),
        json!(1),
        json!([]),
    ] {
        let (client, _) = client_on(input(&[json!({"ok": {
            "os": {"name": null, "version": null, "kernel": null},
            "vulkan": vulkan,
            "gpus": [],
        }})]));
        let got = client.node_info();
        assert!(
            matches!(got, Err(ClientError::Other(_))),
            "`vulkan` {vulkan}: not one of the three words, refused: {got:?}"
        );
    }
    // One row, and the `vulkan` pair of a row.
    let good_row = json!({"pci": "0000:03:00.0", "vendor": "1002", "name": null,
                         "driver": "amdgpu", "version": null, "vulkan": null});
    let keys = |v: &Value| -> Vec<String> {
        let mut k: Vec<String> = v
            .as_object()
            .expect("a row is an object")
            .keys()
            .cloned()
            .collect();
        k.sort();
        k
    };
    // Every row of the fixture carries exactly the keys this loop cuts, so
    // a cut row is a fixture row with one thing broken, not a shape the
    // manager never answers.
    for (n, fixture) in node_info_document()["gpus"]
        .as_array()
        .expect("the rows")
        .iter()
        .enumerate()
    {
        assert_eq!(
            keys(fixture),
            keys(&good_row),
            "row {n} of the fixture answers these six keys"
        );
    }
    for row in [
        json!("not a row"),
        json!(null),
        // A key of the row missing, whatever it is: the manager answers all
        // of them, so a missing one is no shape to read a row out of.
        json!({"vendor": "1002", "name": null, "driver": "amdgpu", "version": null, "vulkan": null}),
        json!({"pci": "0000:03:00.0", "name": null, "driver": "amdgpu", "version": null, "vulkan": null}),
        json!({"pci": "0000:03:00.0", "vendor": "1002", "driver": "amdgpu", "version": null, "vulkan": null}),
        json!({"pci": "0000:03:00.0", "vendor": "1002", "name": null, "version": null, "vulkan": null}),
        json!({"pci": "0000:03:00.0", "vendor": "1002", "name": null, "driver": "amdgpu", "vulkan": null}),
        json!({"pci": "0000:03:00.0", "vendor": "1002", "name": null, "driver": "amdgpu", "version": null}),
        // A text that is neither a string nor null.
        json!({"pci": 3, "vendor": "1002", "name": null, "driver": "amdgpu", "version": null, "vulkan": null}),
        json!({"pci": "0000:03:00.0", "vendor": 0x1002, "name": null, "driver": "amdgpu", "version": null, "vulkan": null}),
        json!({"pci": "0000:03:00.0", "vendor": "1002", "name": {"a": 1}, "driver": "amdgpu", "version": null, "vulkan": null}),
        // The `vulkan` of a row: null, an object of the two texts, nothing else.
        json!({"pci": "0000:03:00.0", "vendor": "1002", "name": null, "driver": "amdgpu", "version": null, "vulkan": "RADV"}),
        json!({"pci": "0000:03:00.0", "vendor": "1002", "name": null, "driver": "amdgpu", "version": null, "vulkan": {}}),
        json!({"pci": "0000:03:00.0", "vendor": "1002", "name": null, "driver": "amdgpu", "version": null, "vulkan": {"driver": "RADV"}}),
        json!({"pci": "0000:03:00.0", "vendor": "1002", "name": null, "driver": "amdgpu", "version": null, "vulkan": {"driver": "RADV", "info": 25}}),
        json!({"pci": "0000:03:00.0", "vendor": "1002", "name": null, "driver": "amdgpu", "version": null, "vulkan": []}),
    ] {
        let (client, _) = client_on(input(&[json!({"ok": {
            "os": {"name": null, "version": null, "kernel": null},
            "vulkan": "ok",
            "gpus": [row],
        }})]));
        let got = client.node_info();
        assert!(
            matches!(got, Err(ClientError::Other(_))),
            "{row}: a row of the wrong shape is refused: {got:?}"
        );
    }
}

/// A client acting for the signed-in user asks `NodeInfoAs`, with the
/// principal: the bff always asks this way, and the manager reads the node
/// for a current member of `pai`.
#[test]
fn an_acting_client_asks_the_node_info_for_the_signed_in_user() {
    let (client, sent) = client_on(input(&[json!({"ok": {
        "os": {"name": null, "version": null, "kernel": null},
        "vulkan": "none",
        "gpus": [],
    }})]));
    let bound = client.acting_for("alice");
    assert_eq!(
        bound.node_info().map(|i| i.vulkan).ok(),
        Some(Status::None),
        "the empty node info, answered to the As form"
    );
    assert_eq!(
        sent_frames(&sent),
        vec![json!({"method": "NodeInfoAs", "principal": "alice"})],
        "the As form, with the principal as given"
    );
}

/// The refusals of the node's info cross the wire like every other
/// method's: the gate's `NotAuthorized` for a caller without quiet
/// `control`, and the same word for a `NodeInfoAs` principal who is no
/// member of `pai` (the bff answers that 403 and ends the session). The
/// read may spawn a child, so a refusal is the caller's last word: the
/// client answers the error and nothing else.
#[test]
fn a_node_info_refusal_is_the_error_the_manager_sent() {
    let (client, _) = client_on(input(&[error_reply("NotAuthorized", "NotAuthorized")]));
    assert_eq!(
        client.node_info(),
        Err(ClientError::NotAuthorized),
        "the plain form, refused by the quiet `control` check"
    );
    let (client, sent) = client_on(input(&[error_reply("NotAuthorized", "NotAuthorized")]));
    let bound = client.acting_for("alice");
    assert_eq!(
        bound.node_info(),
        Err(ClientError::NotAuthorized),
        "the As form, refused for a principal outside `pai`"
    );
    assert_eq!(
        sent_frames(&sent),
        vec![json!({"method": "NodeInfoAs", "principal": "alice"})],
        "the refusal answers the As call"
    );
}

/// The model facts of one saved profile (L1, ARCHITECTURE.md (1)) cross the
/// stream as the three fields of the design: `model` the resolved absolute
/// path (null when the section names none), `info` the four counts and the
/// tensor list 2j adds (J1: `tensors` rows of `name` and `bytes`, and
/// `tensorsTruncated`, in the stream's snake_case `tensors_truncated`),
/// `reason` the keyed why. Exactly one of `info` and `reason` is there; the
/// counts are read as the manager's own `Shape` fields, the way every row of
/// this protocol is spelled. The frame a plain (not acting) client sends is
/// `ModelInfo` with the profile.
#[test]
fn the_model_info_reply_is_the_facts_or_the_reason_the_manager_answered() {
    use crate::client::{ModelFacts, ModelInfoReply, Reason, TensorRow};

    let (client, sent) = client_on(input(&[json!({"ok": {
        "model": "/var/lib/llama.cpp/models/m.gguf",
        "info": {
            "block_count": 40u64, "moe": true, "expert_count": 128u64, "expert_used": 8u64,
            "tensors": [
                {"name": "token_embd.weight", "bytes": 1048576u64},
                {"name": "blk.0.ffn_gate_exps.weight", "bytes": 2048u64},
            ],
            "tensors_truncated": false,
        },
        "reason": null,
    }})]));
    assert_eq!(
        client.model_info("moe-abs"),
        Ok(ModelInfoReply {
            model: Some("/var/lib/llama.cpp/models/m.gguf".into()),
            info: Some(ModelFacts {
                block_count: 40,
                moe: true,
                expert_count: 128,
                expert_used: 8,
                tensors: vec![
                    TensorRow {
                        name: "token_embd.weight".into(),
                        bytes: 1048576,
                    },
                    TensorRow {
                        name: "blk.0.ffn_gate_exps.weight".into(),
                        bytes: 2048,
                    },
                ],
                tensors_truncated: false,
            }),
            reason: None,
        }),
        "the facts, in the manager's order, with the path it read, and the tensor list \
         row by row"
    );
    assert_eq!(
        sent_frames(&sent),
        vec![json!({"method": "ModelInfo", "profile": "moe-abs"})],
        "the plain form, with the profile as given"
    );

    // No facts: a fetch key, with the key as the reason's only value.
    let (client, _) = client_on(input(&[json!({"ok": {
        "model": "/var/lib/llama.cpp/models/x.gguf",
        "info": null,
        "reason": {"key": "model-info-fetched", "args": ["hf-repo"]},
    }})]));
    assert_eq!(
        client.model_info("fetched").map(|r| (r.model, r.reason)),
        Ok((
            Some("/var/lib/llama.cpp/models/x.gguf".into()),
            Some(Reason {
                key: "model-info-fetched".into(),
                args: vec!["hf-repo".into()],
            })
        )),
        "the reason is data of a call that worked, not an error"
    );

    // A profile with no `model`: no path at all.
    let (client, _) = client_on(input(&[json!({"ok": {
        "model": null,
        "info": null,
        "reason": {"key": "model-profile-no-model", "args": ["no-model"]},
    }})]));
    let got = client.model_info("no-model").expect("an answer");
    assert_eq!(got.model, None);
    assert_eq!(got.info, None);
    assert_eq!(
        got.reason.map(|r| (r.key, r.args)),
        Some((
            "model-profile-no-model".to_string(),
            vec!["no-model".to_string()]
        ))
    );

    // A path with no shard of it: the reason names the path.
    let (client, _) = client_on(input(&[json!({"ok": {
        "model": "/m/x-00001-of-00002.gguf",
        "info": null,
        "reason": {"key": "model-info-missing", "args": ["/m/x-00002-of-00002.gguf"]},
    }})]));
    let got = client.model_info("shards").expect("an answer");
    assert_eq!(
        got.reason.map(|r| r.args),
        Some(vec!["/m/x-00002-of-00002.gguf".to_string()])
    );

    // Past the cap: the flag travels as the manager answered it, so the
    // page knows the list it shows is not the whole model.
    let (client, _) = client_on(input(&[json!({"ok": {
        "model": "/m/many.gguf",
        "info": {
            "block_count": 1u64, "moe": false, "expert_count": 0u64, "expert_used": 0u64,
            "tensors": [{"name": "t00000", "bytes": 4096u64}],
            "tensors_truncated": true,
        },
        "reason": null,
    }})]));
    assert_eq!(
        client.model_info("many").map(|r| {
            let facts = r.info.expect("facts");
            (facts.tensors.len(), facts.tensors_truncated)
        }),
        Ok((1, true)),
        "the flag is the manager's answer, not the client's guess"
    );
}

/// A `ModelInfo` reply of the wrong shape is `Other`, like every other
/// row: a page that read a half-reply would show 0 blocks for a model with
/// 64, the sliders on the wrong end. Every field is read strictly: `model`
/// a string or null, `info` and `reason` an object or null, exactly one of
/// them, the counts numbers that are neither negative nor text, `args` a
/// list of strings.
#[test]
fn a_model_info_reply_of_the_wrong_shape_is_other() {
    let ok = |reply: serde_json::Value| json!({"ok": reply});
    for reply in [
        // Not an object, and no fields at all.
        ok(json!([])),
        ok(json!(null)),
        ok(json!({})),
        // Both, and neither: the reply says nothing about what it read.
        json!({"ok": {
            "model": null,
            "info": {"block_count": 1u64, "moe": false, "expert_count": 0u64, "expert_used": 0u64},
            "reason": {"key": "model-info-missing", "args": []},
        }}),
        json!({"ok": {"model": "/m/x.gguf", "info": null, "reason": null}}),
        // `model` of the wrong type.
        json!({"ok": {"model": 1, "info": null, "reason": {"key": "k", "args": []}}}),
        json!({"ok": {"model": true, "info": null, "reason": {"key": "k", "args": []}}}),
        // The tensor list 2j adds (J1): the manager writes it with every
        // read, so a `tensors` that is not a list of `{name, bytes}` rows,
        // a row missing one of its two, or a `tensors_truncated` that is
        // not a bool, is the shape of no manager.
        json!({"ok": {"model": "/m/x.gguf", "reason": null, "info": {
            "block_count": 1u64, "moe": false, "expert_count": 0u64, "expert_used": 0u64,
            "tensors_truncated": false,
        }}}),
        json!({"ok": {"model": "/m/x.gguf", "reason": null, "info": {
            "block_count": 1u64, "moe": false, "expert_count": 0u64, "expert_used": 0u64,
            "tensors": "token_embd.weight", "tensors_truncated": false,
        }}}),
        json!({"ok": {"model": "/m/x.gguf", "reason": null, "info": {
            "block_count": 1u64, "moe": false, "expert_count": 0u64, "expert_used": 0u64,
            "tensors": [{"name": "token_embd.weight"}], "tensors_truncated": false,
        }}}),
        json!({"ok": {"model": "/m/x.gguf", "reason": null, "info": {
            "block_count": 1u64, "moe": false, "expert_count": 0u64, "expert_used": 0u64,
            "tensors": [{"name": "token_embd.weight", "bytes": "1024"}],
            "tensors_truncated": false,
        }}}),
        json!({"ok": {"model": "/m/x.gguf", "reason": null, "info": {
            "block_count": 1u64, "moe": false, "expert_count": 0u64, "expert_used": 0u64,
            "tensors": [],
        }}}),
        // `info` of the wrong shape: a field missing, a text count, a
        // negative one, `moe` not a bool, `info` not an object.
        json!({"ok": {"model": null, "info": {"moe": false, "expert_count": 0u64, "expert_used": 0u64}, "reason": null}}),
        json!({"ok": {"model": null, "info": {"block_count": "40", "moe": false, "expert_count": 0u64, "expert_used": 0u64}, "reason": null}}),
        json!({"ok": {"model": null, "info": {"block_count": -1i64, "moe": false, "expert_count": 0u64, "expert_used": 0u64}, "reason": null}}),
        json!({"ok": {"model": null, "info": {"block_count": 1u64, "moe": "yes", "expert_count": 0u64, "expert_used": 0u64}, "reason": null}}),
        json!({"ok": {"model": null, "info": [], "reason": null}}),
        // `reason` of the wrong shape: no key, a key that is not a string,
        // no args, args not a list, a value that is not a string.
        json!({"ok": {"model": null, "info": null, "reason": {"args": []}}}),
        json!({"ok": {"model": null, "info": null, "reason": {"key": 1, "args": []}}}),
        json!({"ok": {"model": null, "info": null, "reason": {"key": "k"}}}),
        json!({"ok": {"model": null, "info": null, "reason": {"key": "k", "args": {}}}}),
        json!({"ok": {"model": null, "info": null, "reason": {"key": "k", "args": [1]}}}),
        json!({"ok": {"model": null, "info": null, "reason": "model-info-missing"}}),
    ] {
        let (client, _) = client_on(input(std::slice::from_ref(&reply)));
        let got = client.model_info("p");
        assert!(
            matches!(got, Err(ClientError::Other(_))),
            "{reply}: a reply of the wrong shape is refused: {got:?}"
        );
    }
}

/// A client acting for the signed-in user asks `ModelInfoAs`, with the
/// principal: the bff always asks this way, and the manager reads the file
/// for a member of `pai`.
#[test]
fn an_acting_client_asks_the_model_info_for_the_signed_in_user() {
    let (client, sent) = client_on(input(&[json!({"ok": {
        "model": null,
        "info": null,
        "reason": {"key": "model-profile-no-model", "args": ["p"]},
    }})]));
    let bound = client.acting_for("alice");
    assert!(
        bound.model_info("p").is_ok(),
        "an answer of no facts is still an answer"
    );
    assert_eq!(
        sent_frames(&sent),
        vec![json!({"method": "ModelInfoAs", "principal": "alice", "profile": "p"})],
        "the As form, with the principal as given"
    );
}

/// The refusals of the model-info read cross the wire like every other
/// method's: the gate's `NotAuthorized` for a caller without `control` (and
/// for an `As` principal outside `pai`), and the keyed
/// `no-model-profile-named` for an id the pool has no section for, which is
/// the page's 404.
#[test]
fn a_model_info_refusal_is_the_error_the_manager_sent() {
    let (client, _) = client_on(input(&[error_reply("NotAuthorized", "NotAuthorized")]));
    assert_eq!(
        client.model_info("moe-abs"),
        Err(ClientError::NotAuthorized),
        "the plain form, refused by the quiet `control` check"
    );
    let (client, sent) = client_on(input(&[error_reply("NotAuthorized", "NotAuthorized")]));
    assert_eq!(
        client.acting_for("bob").model_info("moe-abs"),
        Err(ClientError::NotAuthorized),
        "the As form, refused for a principal outside `pai`"
    );
    assert_eq!(
        sent_frames(&sent),
        vec![json!({"method": "ModelInfoAs", "principal": "bob", "profile": "moe-abs"})],
        "the refusal answers the As call"
    );
    let (client, _) = client_on(input(&[error_reply(
        "Invalid",
        &Refusal {
            key: "no-model-profile-named",
            values: vec!["nope".into()],
        }
        .encode(),
    )]));
    assert_eq!(
        client.model_info("nope"),
        Err(ClientError::Invalid(Refusal {
            key: "no-model-profile-named",
            values: vec!["nope".into()],
        })),
        "the keyed refusal, with the id it names, is the page's 404"
    );
}

/// A reply that is neither a value nor an error, or the wrong value for
/// the method, is Other, never a panic.
#[test]
fn a_reply_of_the_wrong_shape_is_other() {
    let cases: [(&str, Call, Value); 16] = [
        (
            "boot_enabled",
            |c| c.boot_enabled("default").err(),
            json!({"ok": "enabled"}),
        ),
        (
            "boot_enabled",
            |c| c.boot_enabled("default").err(),
            json!({"ok": null}),
        ),
        (
            "boot_enabled",
            |c| c.boot_enabled("default").err(),
            json!({"ok": 1}),
        ),
        (
            "set_boot_enabled",
            |c| c.set_boot_enabled("default", true).err(),
            json!({"ok": true}),
        ),
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
            |c| c.read_file("models.ini").err(),
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
        (
            "instances",
            |c| c.instances().err(),
            json!({"ok": "default"}),
        ),
        (
            "instances",
            |c| c.instances().err(),
            json!({"ok": [{"id": "default", "engine": "llama.cpp"}]}),
        ),
        (
            "instances",
            |c| c.instances().err(),
            json!({"ok": [{"id": "default", "engine": "llama.cpp", "port": 8080, "profile_tags": "edge", "instance_tags": []}]}),
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
        (
            error_reply("NoServerProfile", "nope café"),
            ClientError::NoServerProfile("nope café".into()),
        ),
        (
            // The profile and the instance, split at the separator the
            // refusal's encode uses.
            error_reply("ServerProfileInUse", "lan 2 (café)\u{1f}lan-2"),
            ClientError::ServerProfileInUse("lan 2 (café)".into(), "lan-2".into()),
        ),
        (
            // A name the pair is not there for: the message is the
            // profile's own name, and nothing is split.
            error_reply("NoServerProfile", "lan-2\u{1f}lan-2"),
            ClientError::NoServerProfile("lan-2\u{1f}lan-2".into()),
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
    let cases: [(Call, Value); 17] = [
        (
            |c| c.boot_enabled("lan-2").err(),
            json!({"method": "BootEnabled", "id": "lan-2"}),
        ),
        (
            |c| c.set_boot_enabled("lan-2", true).err(),
            json!({"method": "SetBootEnabled", "id": "lan-2", "enabled": true}),
        ),
        (
            |c| c.set_boot_enabled("lan-2", false).err(),
            json!({"method": "SetBootEnabled", "id": "lan-2", "enabled": false}),
        ),
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
                c.write_file(
                    "models.ini",
                    "[Qwen 3 (v2) é]\nmodel = /models/q.gguf\n",
                    false,
                )
                .err()
            },
            json!({"method": "WriteFile", "name": "models.ini", "contents": contents, "keep_api_key": false}),
        ),
        (
            |c| {
                c.write_file("server@lan-2.ini", "[Server]\nOpencodeApiKey =\n", true)
                    .err()
            },
            json!({"method": "WriteFile", "name": "server@lan-2.ini", "contents": "[Server]\nOpencodeApiKey =\n", "keep_api_key": true}),
        ),
        (
            |c| {
                c.write_file_if(
                    "models.ini",
                    "[Qwen 3 (v2) é]\nmodel = /models/q.gguf\n",
                    "",
                )
                .err()
            },
            // The compare and swap: the file's name, the new content and
            // the content it must still hold, and no `keep_api_key`.
            json!({"method": "WriteFileIf", "name": "models.ini", "contents": contents, "expected": ""}),
        ),
        (
            |c| c.set_model_profile("lan-2", "Qwen 3 (v2) é", "off").err(),
            json!({"method": "SetModelProfile", "instance": "lan-2", "profile": "Qwen 3 (v2) é", "value": "off"}),
        ),
        (
            |c| c.set_server_profile("lan-2", "Qwen 3 (v2) é").err(),
            json!({"method": "SetServerProfile", "instance": "lan-2", "profile": "Qwen 3 (v2) é"}),
        ),
        (|c| c.instances().err(), json!({"method": "Instances"})),
        (
            |c| c.set_instance_tags("lan-2", &tags()).err(),
            json!({"method": "SetInstanceTags", "instance": "lan-2", "tags": ["edge", "v2"]}),
        ),
        (
            |c| c.set_instance_label("lan-2", "Coding box").err(),
            json!({"method": "SetInstanceLabel", "instance": "lan-2", "label": "Coding box"}),
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
            c.write_file("models.ini", "[m]\nmodel = /m.gguf\n", false)
        }),
    ] {
        let (client, _) = client_on(input(&[json!({"ok": null})]));
        assert_eq!(f(&client), Ok(()), "{method}");
    }

    for enabled in [true, false] {
        let (client, _) = client_on(input(&[json!({"ok": enabled})]));
        assert_eq!(client.boot_enabled("default"), Ok(enabled));
        let (client, _) = client_on(input(&[json!({"ok": null})]));
        assert_eq!(client.set_boot_enabled("default", enabled), Ok(()));
    }

    let text = "[Qwen3.8-27B]\nmodel = /models/Qwen3.8-27B-Q8_0.gguf\n";
    let file = |text: &str, withheld: bool| ManagerFile {
        text: text.into(),
        withheld,
    };
    let (client, _) = client_on(input(&[json!({"ok": {"text": text, "withheld": false}})]));
    assert_eq!(client.read_file("models.ini"), Ok(Some(file(text, false))));
    let (client, _) = client_on(input(&[json!({"ok": null})]));
    assert_eq!(
        client.read_file("models.ini.disabled"),
        Ok(None),
        "no such file"
    );
    let (client, _) = client_on(input(&[json!({"ok": {"text": "", "withheld": false}})]));
    assert_eq!(
        client.read_file("models.ini.disabled"),
        Ok(Some(file("", false))),
        "an empty file is not a missing one"
    );
    let blanked = "[Server]\nPort = 8080\nOpencodeApiKey =\n";
    let (client, _) = client_on(input(&[json!({"ok": {"text": blanked, "withheld": true}})]));
    assert_eq!(
        client.read_file("server@default.ini"),
        Ok(Some(file(blanked, true))),
        "the manager withheld the API key"
    );
}

/// A ReadFile reply in another shape is no file: the bare text of an
/// older manager (which would not say whether it withheld the API key),
/// an object missing a member or with a member of the wrong type.
#[test]
fn a_read_file_reply_of_another_shape_is_other() {
    for reply in [
        json!({"ok": "[Server]\nPort = 8080\n"}),
        json!({"ok": {"text": "[Server]\n"}}),
        json!({"ok": {"withheld": false}}),
        json!({"ok": {"text": "[Server]\n", "withheld": "no"}}),
        json!({"ok": {"text": 7, "withheld": false}}),
        json!({"ok": true}),
    ] {
        let (client, _) = client_on(input(std::slice::from_ref(&reply)));
        assert!(
            matches!(
                client.read_file("server@default.ini"),
                Err(ClientError::Other(_))
            ),
            "{reply}"
        );
    }
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

// ── The named pipe (Windows) ─────────────────────────────────────────

/// Pipe Names: `\\.\pipe\<name>`, the name without a backslash, at
/// most 256 characters in all.
#[test]
fn the_pipe_is_the_manager_s_local_pipe() {
    assert_eq!(PIPE_NAME, r"\\.\pipe\private-ai-manager");
    let name = PIPE_NAME.strip_prefix(r"\\.\pipe\").expect("a local pipe");
    assert!(!name.is_empty() && !name.contains('\\'), "{PIPE_NAME}");
    assert!(PIPE_NAME.len() <= 256);
}

#[test]
fn the_client_asks_to_read_and_write_data_only() {
    const FILE_GENERIC_READ: u32 = 0x0012_0089;
    const FILE_WRITE_DATA: u32 = 0x2;
    const FILE_CREATE_PIPE_INSTANCE: u32 = 0x4;
    assert_eq!(CLIENT_ACCESS, FILE_GENERIC_READ | FILE_WRITE_DATA);
    assert_eq!(CLIENT_ACCESS & FILE_CREATE_PIPE_INSTANCE, 0);
}

#[test]
fn the_client_lets_the_server_identify_it_only() {
    const SECURITY_SQOS_PRESENT: u32 = 0x0010_0000;
    const SECURITY_IDENTIFICATION: u32 = 0x0001_0000;
    assert_eq!(
        CLIENT_FLAGS,
        SECURITY_SQOS_PRESENT | SECURITY_IDENTIFICATION
    );
}

#[test]
fn no_pipe_means_no_manager() {
    // ERROR_FILE_NOT_FOUND: the manager is not running.
    assert_eq!(open_error(2), OpenError::Fail(ClientError::Unavailable));
}

/// The security descriptor keeps out everyone but SYSTEM,
/// Administrators and interactive users: a service account or a network
/// logon is refused at the open.
#[test]
fn a_refused_open_is_not_authorized() {
    assert_eq!(open_error(5), OpenError::Fail(ClientError::NotAuthorized));
}

#[test]
fn a_busy_pipe_is_waited_for() {
    assert_eq!(open_error(231), OpenError::Busy);
}

#[test]
fn any_other_open_failure_names_its_code() {
    for code in [3, 87, 1450] {
        assert_eq!(
            open_error(code),
            OpenError::Fail(ClientError::Other(format!("Win32 error {code}")))
        );
    }
}

// ── Authenticate ────────────────────────────────────────────────────

/// `authenticate` sends `{"method":"Authenticate","user","password"}`,
/// both exactly as given (nothing trimmed), one frame.
#[test]
fn authenticate_sends_its_request() {
    for (user, password) in [
        ("alice", "s3cret-Pw"),
        (" Alice ", " pässwörd \u{1f511}\"\\\n"),
        ("", ""),
    ] {
        let (client, sent) = client_on(input(&[error_reply("NotAuthorized", "NotAuthorized")]));
        assert_eq!(
            client.authenticate(user, password),
            Err(ClientError::NotAuthorized)
        );
        assert_eq!(
            sent_frames(&sent),
            vec![json!({"method": "Authenticate", "user": user, "password": password})],
            "{user:?}"
        );
    }
}

/// The reply `{"ok":{"authenticated":bool,"can_configure":bool}}` is
/// (ok, can_configure).
#[test]
fn authenticate_reads_its_reply() {
    for (ok, admin) in [(true, true), (true, false), (false, false), (false, true)] {
        let (client, _) = client_on(input(&[
            json!({"ok": {"authenticated": ok, "can_configure": admin}}),
        ]));
        assert_eq!(client.authenticate("alice", "s3cret-Pw"), Ok((ok, admin)));
    }
}

/// A reply of another shape is Other, never a sign-in: a bare boolean,
/// a member missing or of the wrong type, null, the D-Bus pair as an
/// array, the camelCase spelling.
#[test]
fn an_authenticate_reply_of_another_shape_is_other() {
    for reply in [
        json!({"ok": true}),
        json!({"ok": null}),
        json!({"ok": [true, true]}),
        json!({"ok": {"authenticated": true}}),
        json!({"ok": {"can_configure": true}}),
        json!({"ok": {"authenticated": "yes", "can_configure": false}}),
        json!({"ok": {"authenticated": true, "can_configure": 1}}),
        json!({"ok": {"ok": true, "canConfigure": true}}),
    ] {
        let (client, _) = client_on(input(std::slice::from_ref(&reply)));
        assert!(
            matches!(
                client.authenticate("alice", "s3cret-Pw"),
                Err(ClientError::Other(_))
            ),
            "{reply}"
        );
    }
}

/// No error the client gives back holds the password: an error reply
/// that does not echo it, a broken stream, a reply of the wrong shape.
#[test]
fn an_authenticate_error_never_holds_the_password() {
    for bytes in [
        input(&[error_reply("Service", "pam_start failed")]),
        input(&[json!({"ok": "nope"})]),
        Vec::new(),
        frame(b"not json"),
    ] {
        let (client, _) = client_on(bytes);
        let e = client
            .authenticate("alice", "s3cret-Pw")
            .expect_err("an error");
        assert!(!format!("{e:?}").contains("s3cret-Pw"), "{e:?}");
        assert!(!e.text().contains("s3cret-Pw"), "{e:?}");
    }
}

// ── Acting for a signed-in user: the `As` forms ─────────────────────

/// The one frame a client bound to `principal` sends for `act`, on a
/// stream answering `reply`. `act` takes `&dyn ManagerClient`, so the
/// bound client's own type stays the implementation's.
fn frame_sent_for(
    principal: &str,
    reply: Value,
    act: fn(&dyn ManagerClient) -> Option<ClientError>,
) -> Value {
    let (client, sent) = client_on(input(&[reply]));
    let bound = client.acting_for(principal);
    act(&bound);
    let frames = sent_frames(&sent);
    assert_eq!(frames.len(), 1, "one request per call");
    frames[0].clone()
}

/// One row of the frame tables below: the method's name (to name a
/// failure), the call to make on the bound client, the frame it must
/// send. Named: the bare tuple trips clippy::type_complexity.
type Case<'a> = (
    &'a str,
    fn(&dyn ManagerClient) -> Option<ClientError>,
    Value,
);

/// A client acting for `alice` sends the `As` form of every guarded
/// method: the principal is one named field of the request, beside the
/// method's own arguments, and the method's name says it carries one.
#[test]
fn an_acting_client_sends_the_as_forms() {
    let refused = error_reply("InvalidId", "InvalidId");
    let cases: [Case; 11] = [
        (
            "start",
            |c| c.start("default").err(),
            json!({"method": "StartAs", "principal": "alice", "id": "default"}),
        ),
        (
            "stop",
            |c| c.stop("lan-2").err(),
            json!({"method": "StopAs", "principal": "alice", "id": "lan-2"}),
        ),
        (
            "restart",
            |c| c.restart("lan-2").err(),
            json!({"method": "RestartAs", "principal": "alice", "id": "lan-2"}),
        ),
        (
            "read_file",
            |c| c.read_file("server@lan-2.ini").err(),
            json!({"method": "ReadFileAs", "principal": "alice", "name": "server@lan-2.ini"}),
        ),
        (
            "write_file",
            |c| {
                c.write_file("models.ini", "[m]\nmodel = /m/x.gguf\n", false)
                    .err()
            },
            json!({"method": "WriteFileAs", "principal": "alice", "name": "models.ini", "contents": "[m]\nmodel = /m/x.gguf\n", "keep_api_key": false}),
        ),
        (
            "write_file_if",
            |c| {
                c.write_file_if("models.ini", "[m]\nmodel = /m/x.gguf\n", "")
                    .err()
            },
            // The `expected` content travels beside the new one, and no
            // `keep_api_key`: the caller that holds the file's text has
            // no key to preserve.
            json!({"method": "WriteFileIfAs", "principal": "alice", "name": "models.ini", "contents": "[m]\nmodel = /m/x.gguf\n", "expected": ""}),
        ),
        (
            "set_boot_enabled",
            |c| c.set_boot_enabled("lan-2", true).err(),
            json!({"method": "SetBootEnabledAs", "principal": "alice", "id": "lan-2", "enabled": true}),
        ),
        (
            "set_model_profile",
            |c| c.set_model_profile("lan-2", "Qwen 3 (v2) é", "on").err(),
            json!({"method": "SetModelProfileAs", "principal": "alice", "instance": "lan-2", "profile": "Qwen 3 (v2) é", "value": "on"}),
        ),
        (
            "set_server_profile",
            |c| c.set_server_profile("lan-2", "Qwen 3 (v2) é").err(),
            json!({"method": "SetServerProfileAs", "principal": "alice", "instance": "lan-2", "profile": "Qwen 3 (v2) é"}),
        ),
        (
            "set_instance_tags",
            |c| c.set_instance_tags("lan-2", &tags()).err(),
            json!({"method": "SetInstanceTagsAs", "principal": "alice", "instance": "lan-2", "tags": ["edge", "v2"]}),
        ),
        (
            "set_instance_label",
            |c| c.set_instance_label("lan-2", "Coding box").err(),
            json!({"method": "SetInstanceLabelAs", "principal": "alice", "instance": "lan-2", "label": "Coding box"}),
        ),
    ];
    for (method, act, request) in cases {
        assert_eq!(
            frame_sent_for("alice", refused.clone(), act),
            request,
            "{method}"
        );
    }
}

/// The methods that need no right are unchanged by acting for a
/// principal: no principal in the request, the same frame as the
/// unbound client's.
#[test]
fn an_acting_client_sends_the_unguarded_methods_unchanged() {
    let cases: [Case; 5] = [
        (
            "list_instances",
            |c| c.list_instances().err(),
            json!({"method": "ListInstances"}),
        ),
        (
            "status",
            |c| c.status("lan-2").err(),
            json!({"method": "Status", "id": "lan-2"}),
        ),
        (
            "boot_enabled",
            |c| c.boot_enabled("lan-2").err(),
            json!({"method": "BootEnabled", "id": "lan-2"}),
        ),
        (
            "authenticate",
            |c| c.authenticate("alice", "s3cret-Pw").err(),
            json!({"method": "Authenticate", "user": "alice", "password": "s3cret-Pw"}),
        ),
        (
            "instances",
            |c| c.instances().err(),
            json!({"method": "Instances"}),
        ),
    ];
    for (method, act, request) in cases {
        assert_eq!(
            frame_sent_for("alice", error_reply("InvalidId", "InvalidId"), act),
            request,
            "{method}"
        );
    }
}

/// The principal travels exactly as given: nothing trimmed, folded,
/// shortened or dropped, the empty name and the longest one included (the
/// manager is the one that refuses a name).
#[test]
fn an_acting_client_sends_the_principal_as_given() {
    let long = "a".repeat(256);
    for principal in ["alice", " Alice ", "root", "pässwörd é", "", "a\nb", &long] {
        assert_eq!(
            frame_sent_for(principal, error_reply("InvalidId", "InvalidId"), |c| c
                .start("default")
                .err()),
            json!({"method": "StartAs", "principal": principal, "id": "default"}),
            "{principal:?}"
        );
    }
}

/// A bound client is a `ManagerClient` a service can hand to its handler
/// threads, and it shares the connection of the client it was made from:
/// every call of either, in order, has its own reply.
#[test]
fn a_bound_client_shares_the_connection() {
    fn shared<C: ManagerClient + Send + Sync>(c: C) -> C {
        c
    }
    let (client, sent) = client_on(input(&[
        json!({"ok": null}),
        json!({"ok": null}),
        json!({"ok": "inactive"}),
    ]));
    assert_eq!(client.stop("default"), Ok(()), "the client's own call");
    let bound = shared(client.acting_for("alice"));
    assert_eq!(bound.stop("default"), Ok(()), "the bound client's call");
    assert_eq!(
        bound.status("default"),
        Ok(InstanceState::Inactive),
        "an unguarded method of the bound client"
    );
    assert_eq!(
        sent_frames(&sent),
        vec![
            json!({"method": "Stop", "id": "default"}),
            json!({"method": "StopAs", "principal": "alice", "id": "default"}),
            json!({"method": "Status", "id": "default"}),
        ]
    );
}

// ── Version ─────────────────────────────────────────────────────────

/// The reply of a manager that has no method it is asked for: after it
/// the server closes the connection.
const BAD_REQUEST: &str = "org.privateai.Manager1.Error.BadRequest";

/// `version` sends `{"method":"Version"}` and reads
/// `{"ok":{"version","protocol"}}`. A protocol this client does not know
/// (a newer manager) and a reply field it does not know are read through,
/// not refused: the protocol grows with each additive change.
#[test]
fn version_sends_its_request_and_reads_its_reply() {
    for (version, protocol) in [("2.99.1", 1), ("3.0.0", 2), ("", 0)] {
        let (client, sent) = client_on(input(&[json!({
            "ok": {"version": version, "protocol": protocol, "extra": "a newer manager"},
        })]));
        assert_eq!(
            client.version(),
            Ok(Some(ManagerVersion {
                version: version.to_string(),
                protocol,
            })),
            "{version} {protocol}"
        );
        assert_eq!(
            sent_frames(&sent),
            vec![json!({"method": "Version"})],
            "{version}"
        );
    }
}

/// A manager with no `Version` method answers `BadRequest`: that is no
/// version (None), not a failure.
#[test]
fn version_is_none_against_a_manager_without_the_method() {
    let (client, sent) = client_on(input(&[json!({
        "error": {"name": BAD_REQUEST, "message": "not a request"},
    })]));
    assert_eq!(client.version(), Ok(None));
    assert_eq!(
        sent_frames(&sent),
        vec![json!({"method": "Version"})],
        "one request"
    );
}

/// Only `BadRequest` is "no such method": every other failure is the
/// failure it is, and a reply of another shape is Other.
#[test]
fn a_version_reply_of_another_shape_is_not_none() {
    for reply in [
        json!({"ok": null}),
        json!({"ok": "2.99.1"}),
        json!({"ok": [1]}),
        json!({"ok": {"version": "2.99.1"}}),
        json!({"ok": {"protocol": 1}}),
        json!({"ok": {"version": 1, "protocol": 1}}),
        json!({"ok": {"version": "2.99.1", "protocol": "1"}}),
        json!({"ok": {"version": "2.99.1", "protocol": -1}}),
        json!({"ok": {"version": "2.99.1", "protocol": 4294967296u64}}),
    ] {
        let (client, _) = client_on(input(std::slice::from_ref(&reply)));
        assert!(
            matches!(client.version(), Err(ClientError::Other(_))),
            "{reply}"
        );
    }
    for (reply, expected) in [
        (
            error_reply("NotAuthorized", "NotAuthorized"),
            ClientError::NotAuthorized,
        ),
        (
            error_reply("Service", "boom"),
            ClientError::Service("boom".into()),
        ),
    ] {
        let (client, _) = client_on(input(std::slice::from_ref(&reply)));
        assert_eq!(client.version(), Err(expected), "{reply}");
    }
    let (client, _) = client_on(Vec::new());
    assert_eq!(
        client.version(),
        Err(ClientError::Unavailable),
        "no manager"
    );
}
