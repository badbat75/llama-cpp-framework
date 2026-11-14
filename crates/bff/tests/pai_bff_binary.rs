//! The `pai-bff` executable as the service runs it. Every run gets its own
//! data root (`PRIVATE_AI_DATA_ROOT`, set on the child only, so no other test
//! races it): its `bff.ini` is `<root>/system-config/bff.ini`
//! (`paths::system_config_dir` under the redirect), never the real system
//! config. No run here binds the default port: a server run names a free
//! loopback port in its `bff.ini`, every other run must exit before binding.

use std::io::{Read, Write};
use std::net::{SocketAddr, TcpListener, TcpStream};
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::time::{Duration, Instant};

const DEADLINE: Duration = Duration::from_secs(60);

struct Run {
    code: Option<i32>,
    stdout: String,
    stderr: String,
}

fn command(root: &Path, args: &[&str]) -> Command {
    let mut c = Command::new(env!("CARGO_BIN_EXE_pai-bff"));
    c.args(args)
        .env("PRIVATE_AI_DATA_ROOT", root)
        .env(
            "DBUS_SYSTEM_BUS_ADDRESS",
            format!("unix:path={}", root.join("no-bus").display()),
        )
        // Not the repo: the page must come from the binary, not a file
        // beside the working directory.
        .current_dir(root)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    c
}

/// `pai-bff <args>` that must exit on its own, killed after the deadline.
fn pai_bff(root: &Path, args: &[&str]) -> Run {
    let mut child = command(root, args).spawn().expect("spawn pai-bff");
    let deadline = Instant::now() + DEADLINE;
    while child.try_wait().expect("try_wait").is_none() {
        if Instant::now() > deadline {
            let _ = child.kill();
            let _ = child.wait();
            panic!(
                "pai-bff {}: still running after {DEADLINE:?} (serving instead of refusing?)",
                args.join(" ")
            );
        }
        std::thread::sleep(Duration::from_millis(20));
    }
    let out = child.wait_with_output().expect("pai-bff output");
    Run {
        code: out.status.code(),
        stdout: String::from_utf8_lossy(&out.stdout).into_owned(),
        stderr: String::from_utf8_lossy(&out.stderr).into_owned(),
    }
}

/// Where the bff reads its config under the redirect.
fn bff_ini(root: &Path) -> PathBuf {
    root.join("system-config").join("bff.ini")
}

fn write_bff_ini(root: &Path, listen: &str) -> PathBuf {
    let path = bff_ini(root);
    std::fs::create_dir_all(path.parent().expect("system-config")).expect("system-config");
    std::fs::write(&path, format!("[Bff]\nListen = {listen}\n")).expect("write bff.ini");
    path
}

/// A loopback port free a moment ago.
fn free_port() -> u16 {
    TcpListener::bind("127.0.0.1:0")
        .expect("bind an ephemeral port")
        .local_addr()
        .expect("local addr")
        .port()
}

/// A text printed by the binary came out of the strings file: no `{N}` left
/// unfilled and not a bare key (`strings::fill` gives the key back when it is
/// missing).
fn assert_filled(text: &str) {
    assert!(!text.contains("{0}"), "an unfilled placeholder: {text:?}");
    let first = text.trim().trim_start_matches("Error: ");
    assert!(
        first.contains(' '),
        "a sentence, not a bare strings key: {text:?}"
    );
}

/// The child killed and reaped when the test ends, pass or fail.
struct Serving(Child);

impl Drop for Serving {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}

/// One HTTP/1.1 GET over a plain socket: (status line, headers, body).
fn get(addr: SocketAddr, path: &str) -> std::io::Result<(String, String, String)> {
    let mut s = TcpStream::connect_timeout(&addr, Duration::from_secs(2))?;
    s.set_read_timeout(Some(Duration::from_secs(10)))?;
    write!(
        s,
        "GET {path} HTTP/1.1\r\nHost: {addr}\r\nConnection: close\r\n\r\n"
    )?;
    let mut raw = String::new();
    s.read_to_string(&mut raw)?;
    let (head, body) = raw.split_once("\r\n\r\n").unwrap_or((&raw, ""));
    let (status, headers) = head.split_once("\r\n").unwrap_or((head, ""));
    Ok((
        status.to_string(),
        headers.to_ascii_lowercase(),
        body.to_string(),
    ))
}

// ── Failure paths first ─────────────────────────────────────────────────

/// An unknown argument is a usage error (exit 2) naming it, before
/// anything binds.
#[test]
fn an_unknown_argument_is_a_usage_error() {
    let dir = tempfile::tempdir().expect("tempdir");
    let r = pai_bff(dir.path(), &["--bogus"]);
    assert_eq!(r.code, Some(2), "stdout: {} stderr: {}", r.stdout, r.stderr);
    assert!(r.stderr.contains("--bogus"), "{}", r.stderr);
    assert_filled(&r.stderr);
}

/// A `Listen` that is not an address stops the service with exit 1, a
/// message naming the value and the file, and nothing served.
#[test]
fn a_bad_listen_value_is_refused() {
    let dir = tempfile::tempdir().expect("tempdir");
    let path = write_bff_ini(dir.path(), "not-an-address");
    let r = pai_bff(dir.path(), &[]);
    assert_eq!(r.code, Some(1), "stdout: {} stderr: {}", r.stdout, r.stderr);
    assert!(r.stderr.contains("not-an-address"), "{}", r.stderr);
    assert!(
        r.stderr.contains(&path.display().to_string()),
        "names {}: {}",
        path.display(),
        r.stderr
    );
    assert_filled(&r.stderr);
}

/// An address off loopback is refused (LAN access needs authentication,
/// which the bff does not have yet): exit 1, the address named.
#[test]
fn an_address_off_loopback_is_refused() {
    let dir = tempfile::tempdir().expect("tempdir");
    write_bff_ini(dir.path(), "0.0.0.0:8590");
    let r = pai_bff(dir.path(), &[]);
    assert_eq!(r.code, Some(1), "stdout: {} stderr: {}", r.stdout, r.stderr);
    assert!(r.stderr.contains("0.0.0.0:8590"), "{}", r.stderr);
    assert_filled(&r.stderr);
}

/// A port another process holds: exit 1 with the address named, not a
/// panic or a hang.
#[test]
fn a_port_in_use_is_reported() {
    let dir = tempfile::tempdir().expect("tempdir");
    let holder = TcpListener::bind("127.0.0.1:0").expect("hold a port");
    let taken = holder.local_addr().expect("local addr").to_string();
    write_bff_ini(dir.path(), &taken);
    let r = pai_bff(dir.path(), &[]);
    drop(holder);
    assert_eq!(r.code, Some(1), "stdout: {} stderr: {}", r.stdout, r.stderr);
    assert!(r.stderr.contains(&taken), "names {taken}: {}", r.stderr);
    assert!(!r.stderr.contains("panicked"), "{}", r.stderr);
    assert_filled(&r.stderr);
}

// ── What it does ────────────────────────────────────────────────────────

/// `--version` names the executable: `pai-bff <framework version>`.
#[test]
fn version_names_pai_bff() {
    let dir = tempfile::tempdir().expect("tempdir");
    let r = pai_bff(dir.path(), &["--version"]);
    assert_eq!(r.code, Some(0), "stderr: {}", r.stderr);
    assert_eq!(r.stdout, format!("pai-bff {}\n", env!("CARGO_PKG_VERSION")));
}

/// Started with a `bff.ini` naming a loopback port, it serves the router
/// there: `/api/version` and the page at `/`, out of the binary.
#[test]
fn it_serves_on_the_configured_address() {
    let dir = tempfile::tempdir().expect("tempdir");
    let addr: SocketAddr = format!("127.0.0.1:{}", free_port()).parse().expect("addr");
    write_bff_ini(dir.path(), &addr.to_string());
    let mut child = Serving(command(dir.path(), &[]).spawn().expect("spawn pai-bff"));

    let deadline = Instant::now() + DEADLINE;
    let (status, headers, body) = loop {
        if let Some(status) = child.0.try_wait().expect("try_wait") {
            let mut err = String::new();
            if let Some(mut e) = child.0.stderr.take() {
                let _ = e.read_to_string(&mut err);
            }
            panic!("pai-bff exited ({status}) instead of serving: {err}");
        }
        match get(addr, "/api/version") {
            Ok(reply) => break reply,
            Err(_) if Instant::now() < deadline => std::thread::sleep(Duration::from_millis(50)),
            Err(e) => panic!("nothing served on {addr} after {DEADLINE:?}: {e}"),
        }
    };
    assert!(status.contains(" 200 "), "{status}\n{body}");
    assert!(
        headers.contains("content-type: application/json"),
        "{headers}"
    );
    let json: serde_json::Value = serde_json::from_str(&body).expect("a JSON body");
    assert_eq!(json["version"], env!("CARGO_PKG_VERSION"), "{body}");

    let (status, headers, body) = get(addr, "/").expect("GET /");
    assert!(status.contains(" 200 "), "{status}\n{body}");
    assert!(headers.contains("content-type: text/html"), "{headers}");
    assert!(body.to_ascii_lowercase().contains("</html>"), "{body}");

    let (status, _, _) = get(addr, "/no-such-page").expect("GET /no-such-page");
    assert!(status.contains(" 404 "), "{status}");
}
