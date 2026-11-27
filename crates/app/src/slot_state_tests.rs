use super::*;

/// The restore asks the probe it is given (the caller's backend, here a
/// fake manager that reports the instance gone) while the port does not
/// answer, and gives up at once with the probe's answer.
#[test]
fn restore_asks_the_probe_it_is_given() {
    use crate::server_control::fake::{self, FakeClient};
    use pai_core::client::InstanceState;
    let dir = tempfile::tempdir().expect("tempdir");
    std::fs::write(dir.path().join(state_file_name("Qwen3.8-27B")), b"dump").expect("dump");
    let port = free_port();
    let cfg = crate::server_cfg::ServerConfig {
        port: Some(i32::from(port)),
        state_dir: Some(dir.path().display().to_string()),
        ..Default::default()
    };
    let log = fake::log();
    let b = fake::backend(FakeClient::new(&log, InstanceState::Inactive));
    let (tx, rx) = std::sync::mpsc::channel();
    std::thread::spawn(move || {
        let r = restore_newest_with(&cfg, &["Qwen3.8-27B".to_string()], || b.is_running());
        let _ = tx.send(r.map(|t| t.map(|t| t.model)));
    });
    let result = match rx.recv_timeout(Duration::from_secs(20)) {
        Ok(r) => r,
        Err(std::sync::mpsc::RecvTimeoutError::Timeout) => {
            panic!("restore did not give up within 20 s")
        }
        Err(e) => panic!("the restore thread panicked: {e:?}"),
    };
    assert_eq!(result, Err(strings::fill("exited-before-answer", &[&port])));
    assert!(
        log.lock().unwrap().iter().any(|e| e == "status default"),
        "the given backend was asked: {log:?}"
    );
}

#[test]
fn file_name_is_sanitized_and_stable() {
    assert_eq!(
        state_file_name("Qwen3.8-27B-UD"),
        "Qwen3.8-27B-UD.llamastate"
    );
    // Path separators are what `fs_validate_filename` rejects outright.
    assert_eq!(state_file_name("a/b\\c"), "a_b_c.llamastate");
    // Substitution, not deletion: a name of nothing but separators still
    // yields a usable (and still distinct) file rather than collapsing.
    assert_eq!(state_file_name("///"), "___.llamastate");
    // Leading/trailing dots would make a hidden or extension-less file.
    assert_eq!(state_file_name("..x.."), "x.llamastate");
    // The empty-stem fallback is reachable only once the dots are trimmed,
    // which is the one input that would otherwise produce a bare ".llamastate".
    assert_eq!(state_file_name("..."), "model.llamastate");
    assert_eq!(state_file_name(""), "model.llamastate");
}

/// The name is the core's: the save leg in `pai_core::slot_save` writes it,
/// the restore here reads it. One rule, one owner, or a dump lands at a
/// name no start ever looks for (an id with a separator is where they
/// differed: the save took the last component, the restore the whole id).
#[test]
fn the_dump_name_is_the_cores() {
    for id in [
        "Qwen3.8-27B-UD",
        "org/model:Q4",
        r"C:\models\Qwen3.8-27B.gguf",
        "/models/Qwen3.8-27B/Qwen3.8-27B-Q4_K_M",
        ".hidden.",
        "",
    ] {
        assert_eq!(
            state_file_name(id),
            pai_core::slot_save::dump_file_name(id),
            "the name the save writes and the name the restore reads must be one name for {id:?}"
        );
    }
}

#[test]
fn formats_sizes_and_counts() {
    let t = Transfer {
        model: "M".into(),
        bytes: 8_912_896_000,
        tokens: 234_102,
    };
    assert_eq!(t.describe(), "M (234,102 tokens, 8.3 GiB)");
    assert_eq!(thousands(0), "0");
    assert_eq!(thousands(999), "999");
    assert_eq!(thousands(1000), "1,000");
    assert_eq!(gib(512 * 1024 * 1024), "512 MiB");
}

/// The save and restore replies name the same two quantities differently,
/// and swapping them is invisible (a missing key reads as 0, not an error).
/// Both shapes are copied from `server_task_result_slot_save_load::to_json`.
#[test]
fn counts_are_read_from_the_right_keys() {
    let save = serde_json::json!({
        "id_slot": 0, "filename": "m.llamastate",
        "n_saved": 234_102, "n_written": 8_912_896_000u64,
        "timings": { "save_ms": 41_000.0 }
    });
    let t = transfer_from_json("m", &save);
    assert_eq!(t.tokens, 234_102, "n_saved is a TOKEN count");
    assert_eq!(t.bytes, 8_912_896_000, "n_written is the byte count");

    let restore = serde_json::json!({
        "id_slot": 0, "filename": "m.llamastate",
        "n_restored": 234_102, "n_read": 8_912_896_000u64,
        "timings": { "restore_ms": 9_000.0 }
    });
    let t = transfer_from_json("m", &restore);
    assert_eq!(t.tokens, 234_102);
    assert_eq!(t.bytes, 8_912_896_000);

    // A reply with neither pair must degrade to zeroes, never panic.
    let t = transfer_from_json("m", &serde_json::json!({}));
    assert_eq!((t.tokens, t.bytes), (0, 0));
}

/// `loaded` is the only status with a slot worth dumping: `sleeping` has
/// released its KV cache and `loading` has not built one yet.
#[test]
fn only_loaded_models_are_listed() {
    let body = serde_json::json!({
        "object": "list",
        "data": [
            {"id": "up",       "status": {"value": "loaded"}},
            {"id": "asleep",   "status": {"value": "sleeping"}},
            {"id": "starting", "status": {"value": "loading"}},
            {"id": "down",     "status": {"value": "unloaded"}},
        ]
    })
    .to_string();
    let parsed: serde_json::Value = serde_json::from_str(&body).unwrap();
    let names: Vec<&str> = parsed["data"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|m| m["status"]["value"] == "loaded")
        .filter_map(|m| m["id"].as_str())
        .collect();
    assert_eq!(names, vec!["up"]);
}

// ── Waiting for the router after a start ─────────────────────────────

/// A port nothing listens on yet: bound to learn a free number, then
/// released, so a connect is refused as it was on 2026-10-29.
fn free_port() -> u16 {
    let l = std::net::TcpListener::bind(("127.0.0.1", 0)).unwrap();
    l.local_addr().unwrap().port()
}

/// A stand-in router that starts listening on `port` after `delay` and
/// answers its connections in order with `replies` (status, body), then
/// stops. Returns the request lines it saw.
fn fake_router(
    port: u16,
    delay: Duration,
    replies: Vec<(u16, &'static str)>,
) -> std::thread::JoinHandle<Vec<String>> {
    use std::io::{BufRead, BufReader, Read, Write};
    std::thread::spawn(move || {
        std::thread::sleep(delay);
        let l = std::net::TcpListener::bind(("127.0.0.1", port)).unwrap();
        let mut seen = Vec::new();
        for (status, body) in replies {
            let (mut s, _) = l.accept().unwrap();
            let mut r = BufReader::new(s.try_clone().unwrap());
            let mut line = String::new();
            r.read_line(&mut line).unwrap();
            seen.push(line.trim().to_string());
            let mut len = 0;
            loop {
                let mut h = String::new();
                r.read_line(&mut h).unwrap();
                if h.trim().is_empty() {
                    break;
                }
                if let Some(v) = h.to_ascii_lowercase().strip_prefix("content-length:") {
                    len = v.trim().parse().unwrap();
                }
            }
            let mut req_body = vec![0; len];
            r.read_exact(&mut req_body).unwrap();
            write!(
                s,
                "HTTP/1.1 {status} X\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                body.len()
            )
            .unwrap();
        }
        seen
    })
}

/// The 2026-10-29 failure: the router opens its port seconds after the
/// launch, then answers 503 until it is ready. The restore must wait it
/// out and only then send the slot request.
#[test]
fn restore_waits_for_a_router_that_listens_late() {
    let port = free_port();
    let began = std::time::Instant::now();
    let router = fake_router(
        port,
        Duration::from_millis(800),
        vec![
            (503, r#"{"error":{"message":"Loading model"}}"#),
            (200, r#"{"status":"ok"}"#),
            (200, r#"{"n_restored":166000,"n_read":5000000000}"#),
        ],
    );
    wait_listening(
        port,
        Duration::from_secs(10),
        Duration::from_millis(50),
        || true,
    )
    .unwrap();
    assert!(began.elapsed() >= Duration::from_millis(800));
    let t = slot_action(port, "restore", "M", "M.llamastate").unwrap();
    assert_eq!(t.tokens, 166_000);
    let seen = router.join().unwrap();
    assert_eq!(
        seen,
        vec![
            "GET /health HTTP/1.1",
            "GET /health HTTP/1.1",
            "POST /slots/0?action=restore HTTP/1.1",
        ]
    );
}

/// Nobody ever listens: the wait gives up once the timeout is past, with
/// a message that names the port.
#[test]
fn wait_gives_up_after_the_timeout() {
    let port = free_port();
    let began = std::time::Instant::now();
    let err = wait_listening(
        port,
        Duration::from_millis(300),
        Duration::from_millis(50),
        || true,
    )
    .unwrap_err();
    assert!(began.elapsed() >= Duration::from_millis(300));
    assert!(err.contains(&format!("port {port} within")), "{err}");
}

/// A server that died during startup ends the wait at once instead of
/// after the full timeout.
#[test]
fn wait_stops_when_the_server_exits() {
    let port = free_port();
    let began = std::time::Instant::now();
    let err = wait_listening(
        port,
        Duration::from_secs(60),
        Duration::from_secs(60),
        || false,
    )
    .unwrap_err();
    assert!(began.elapsed() < Duration::from_secs(10));
    assert!(err.contains("exited"), "{err}");
}

// ── The save leg: a wrapper over the core's ──────────────────────────

/// The `filename` a save request asks the server to write.
fn requested_filename(body: &str) -> Option<String> {
    serde_json::from_str::<serde_json::Value>(body)
        .ok()?
        .get("filename")?
        .as_str()
        .map(str::to_string)
}

/// The names in `dir`, sorted (a `read_dir` listing is unordered).
fn names_in(dir: &Path) -> Vec<String> {
    let mut names: Vec<String> = std::fs::read_dir(dir)
        .expect("read the state dir")
        .map(|e| e.expect("entry").file_name().to_string_lossy().into_owned())
        .collect();
    names.sort();
    names
}

/// A stand-in router that writes what it is asked to write, as llama-server
/// does: it answers `GET /models` with `models_body` and then, for each of
/// the `saves` saves that follow, writes the `filename` of the request into
/// `dir` and answers with the counts. The listener is bound before the
/// helper returns, so the caller never races it. Returns the port and the
/// request lines and bodies, in the order the router saw them.
///
/// Every accept is bounded: a caller that stops asking ends the thread with
/// what it saw, so a red is an assertion, never a hang.
fn saving_router(
    dir: &Path,
    models_body: &str,
    saves: usize,
) -> (u16, std::thread::JoinHandle<Vec<(String, String)>>) {
    use std::io::{BufRead, BufReader, Read, Write};
    use std::net::TcpStream;
    fn read_request(sock: &mut TcpStream) -> (String, String) {
        let mut r = BufReader::new(sock.try_clone().expect("clone socket"));
        let mut line = String::new();
        r.read_line(&mut line).expect("request line");
        let mut len = 0usize;
        loop {
            let mut head = String::new();
            if r.read_line(&mut head).expect("request header") == 0 {
                break;
            }
            if head.trim().is_empty() {
                break;
            }
            if let Some(v) = head.to_ascii_lowercase().strip_prefix("content-length:") {
                len = v.trim().parse().expect("content-length");
            }
        }
        let mut body = vec![0u8; len];
        if len > 0 {
            r.read_exact(&mut body).expect("request body");
        }
        (
            line.trim().to_string(),
            String::from_utf8_lossy(&body).to_string(),
        )
    }
    let l = std::net::TcpListener::bind(("127.0.0.1", 0)).expect("bind loopback");
    let port = l.local_addr().expect("local addr").port();
    l.set_nonblocking(true).expect("nonblocking listener");
    let dir = dir.to_path_buf();
    let list = models_body.to_string();
    (
        port,
        std::thread::spawn(move || {
            let deadline = std::time::Instant::now() + Duration::from_secs(20);
            let mut seen = Vec::new();
            for i in 0..=saves {
                let mut sock = loop {
                    match l.accept() {
                        Ok((sock, _)) => {
                            sock.set_nonblocking(false).expect("blocking socket");
                            sock.set_read_timeout(Some(Duration::from_secs(10)))
                                .expect("read timeout");
                            break sock;
                        }
                        Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                            if std::time::Instant::now() > deadline {
                                return seen;
                            }
                            std::thread::sleep(Duration::from_millis(20));
                        }
                        Err(e) => panic!("accept: {e}"),
                    }
                };
                let (line, body) = read_request(&mut sock);
                seen.push((line, body.clone()));
                let reply = if i == 0 {
                    (200, list.clone())
                } else {
                    if let Some(name) = requested_filename(&body) {
                        std::fs::write(dir.join(name), "the bytes llama-server wrote")
                            .expect("write the dump");
                    }
                    (
                        200,
                        r#"{"id_slot":0,"n_saved":166000,"n_written":1024}"#.to_string(),
                    )
                };
                write!(
                    sock,
                    "HTTP/1.1 {} X\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
                    reply.0,
                    reply.1.len(),
                    reply.1
                )
                .expect("reply");
            }
            seen
        }),
    )
}

/// The save the GUI and `pai` call is a thin wrapper: it hands the core's save
/// the port and the folder the config names, and the dump lands under the name
/// the restore picks.
#[test]
fn the_wrapper_saves_through_the_core_and_lands_the_dump() {
    let dir = tempfile::tempdir().expect("tempdir");
    let list = serde_json::json!({
        "object": "list",
        "data": [
            {"id": "Qwen3.8-27B", "status": {"value": "loaded"}},
            {"id": "Qwen3.8-Flash-Next", "status": {"value": "loaded"}}
        ]
    })
    .to_string();
    let (port, router) = saving_router(dir.path(), &list, 2);
    let cfg = crate::server_cfg::ServerConfig {
        port: Some(i32::from(port)),
        state_dir: Some(dir.path().display().to_string()),
        ..Default::default()
    };

    let (done, errors) = save_all(&cfg);
    assert_eq!(
        errors,
        Vec::<String>::new(),
        "the save reported: {errors:?}"
    );
    let models_saved: Vec<String> = done.iter().map(|t| t.model.clone()).collect();
    assert_eq!(
        models_saved,
        vec!["Qwen3.8-27B", "Qwen3.8-Flash-Next"],
        "one transfer per loaded model, with its counts"
    );
    assert_eq!(done[0].tokens, 166_000, "n_saved counts tokens");
    assert_eq!(done[0].bytes, 1_024, "n_written counts bytes");

    let seen = router.join().expect("the router ended");
    let (lines, bodies): (Vec<String>, Vec<String>) = seen.into_iter().unzip();
    assert_eq!(
        lines,
        vec![
            "GET /models HTTP/1.1",
            "POST /slots/0?action=save HTTP/1.1",
            "POST /slots/0?action=save HTTP/1.1",
        ],
        "the loaded models of the config's port, slot 0, the save action"
    );
    let asked: Vec<String> = bodies
        .iter()
        .map(|b| requested_filename(b).unwrap_or_default())
        .collect();
    assert_eq!(
        asked,
        vec![
            "",
            "Qwen3.8-27B.llamastate-partial",
            "Qwen3.8-Flash-Next.llamastate-partial",
        ],
        "the save asks for the partial name, so a save cut short leaves the previous dump"
    );

    assert_eq!(
        names_in(dir.path()),
        vec![
            state_file_name("Qwen3.8-27B"),
            state_file_name("Qwen3.8-Flash-Next")
        ],
        "the partial is renamed to the dump's name, nothing is left behind"
    );
    assert_eq!(
        std::fs::read_to_string(dir.path().join(state_file_name("Qwen3.8-27B")))
            .expect("the dump the save wrote"),
        "the bytes llama-server wrote",
        "the file that lands is the one the server wrote"
    );
}

/// The save leg lives in the core, so the instance's own stop (the Linux
/// unit's `ExecStop=`, the Windows service's stop) reaches the same code as
/// the clients, and the wrapper is what carries the design's one budget to
/// it: the core owns that constant, so the stop and the client cannot end
/// up with two different deadlines.
#[test]
fn the_save_leg_delegates_to_the_core() {
    let src = std::fs::read_to_string(
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src/slot_state.rs"),
    )
    .expect("read slot_state.rs");
    // Only the production half: the e2e phase and this module come after it.
    let code = src.split("\n#[cfg(test)]\n").next().expect("the code half");
    assert!(
        code.contains("slot_save::save_all"),
        "the wrapper must call the core's save: this file is where the GUI and the CLI ask for a snapshot"
    );
    assert!(
        code.contains("slot_save::SAVE_BUDGET"),
        "the wrapper hands over the core's budget, one deadline for every request of the run"
    );
    assert!(
        !code.contains(concat!("from_secs(", "240)")),
        "the budget is the core's constant, not seconds copied here"
    );
    let posts_a_save = regex::Regex::new(r#"slot_action\(\s*port,\s*"save""#).expect("regex");
    assert!(
        !posts_a_save.is_match(code),
        "the save request is the core's now: only the restore leg posts a slot action here"
    );
}

/// A save cut short leaves `<stem>.llamastate-partial` on disk. The restore
/// takes the newest dump, so the partial must never be a candidate.
#[test]
fn a_partial_is_never_a_restore_candidate() {
    let dir = tempfile::tempdir().expect("tempdir");
    std::fs::write(dir.path().join(state_file_name("Qwen3.8-27B")), b"dump").expect("dump");
    std::fs::write(
        dir.path().join("Qwen3.8-Flash-Next.llamastate-partial"),
        b"half written",
    )
    .expect("partial");

    assert_eq!(
        dumps_newest_first(dir.path()),
        vec![dir.path().join(state_file_name("Qwen3.8-27B"))],
        "the restore reads the newest complete dump, never a half-written one"
    );
}
