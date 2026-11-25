//! Minimal loopback HTTP client for the running llama-server.
//!
//! Hand-rolled rather than pulled from a crate on purpose: the whole surface is
//! a handful of calls to 127.0.0.1 with JSON bodies, and the alternative
//! (`reqwest`) drags an async runtime into a crate that has none.
//! `Connection: close` is what lets the body be read to EOF, so no
//! `Content-Length` bookkeeping is needed on the way back; chunked responses are
//! decoded anyway because cpp-httplib may pick that framing regardless.
//!
//! `request` always dials 127.0.0.1, never the configured `Hostname`: that
//! value is the address llama-server BINDS (`0.0.0.0` is a common one and not
//! connectable as a destination on every stack), while loopback reaches it
//! under every binding the framework can produce. `request_host` dials the
//! address the caller names, for a caller that asks the engine where it is
//! bound: the readiness check polls the `--host` of the launch, and maps the
//! bound-everywhere values to loopback itself.
//!
//! Two callers, with very different deadlines, which is why the timeout is
//! the caller's argument and not a constant here: `slot_save` waits out a
//! multi-GiB KV-cache dump under one deadline for the whole run, `bench::exec`
//! (in `pai-app`, through this module) waits out a cold prefill that can run
//! into minutes on a long prompt. The CONNECT timeout is shared and short: a
//! server that does not accept a loopback connection in five seconds is not
//! going to answer either.

use crate::strings;
use std::io::{Read, Write};
use std::time::Duration;
use std::time::Instant;

/// Connect (and write) timeout. Deliberately short and shared: reaching a local
/// listener is instant or never, whatever the request then costs to answer.
pub const CONNECT_TIMEOUT: Duration = Duration::from_secs(5);

/// The address that reaches llama-server under every binding the framework can
/// produce, so what every caller with no address of its own dials.
pub const LOOPBACK: &str = "127.0.0.1";

pub struct Response {
    pub status: u16,
    pub body: String,
}

/// Send one request to llama-server on the loopback interface and read the whole
/// response. `timeout` is the deadline of the WHOLE request: connect, write and
/// every read share it, so an answer that never completes ends at that instant
/// rather than restarting its clock at every byte.
pub fn request(
    port: u16,
    method: &str,
    path: &str,
    body: Option<&str>,
    timeout: Duration,
) -> Result<Response, String> {
    request_host(LOOPBACK, port, method, path, body, timeout)
}

/// The same request on the address `host` names, for a caller that asks
/// llama-server where it is bound. The caller maps what is bound everywhere
/// (`0.0.0.0`, `::`) to `LOOPBACK` first: those are bindings, no destination.
pub fn request_host(
    host: &str,
    port: u16,
    method: &str,
    path: &str,
    body: Option<&str>,
    timeout: Duration,
) -> Result<Response, String> {
    let deadline = Instant::now() + timeout;
    let connect = CONNECT_TIMEOUT.min(remaining(deadline).ok_or_else(timed_out)?);
    let mut stream = connect_host(host, port, connect)
        .map_err(|e| strings::fill("cannot-reach-server", &[&port, &e]))?;
    stream
        .set_write_timeout(Some(remaining(deadline).ok_or_else(timed_out)?))
        .map_err(err_str)?;

    let payload = body.unwrap_or("");
    // The `Host:` header of an IPv6 address carries brackets (the URL form of
    // an IP literal): written bare, its last colon reads as the port separator
    // and the server sees no address.
    let authority = if host.contains(':') {
        format!("[{host}]:{port}")
    } else {
        format!("{host}:{port}")
    };
    let mut req = format!(
        "{method} {path} HTTP/1.1\r\n\
         Host: {authority}\r\n\
         Connection: close\r\n\
         Content-Type: application/json\r\n\
         Content-Length: {}\r\n\r\n",
        payload.len()
    );
    req.push_str(payload);
    stream.write_all(req.as_bytes()).map_err(err_str)?;
    stream.flush().map_err(err_str)?;

    // The body is read in a loop, never with `read_to_end`: that gives the whole
    // body ONE read timeout, so a server that drips an answer byte by byte is
    // answered at every read and the caller waits long past its deadline. Each
    // read gets what the request has left, so the loop ends at the deadline.
    let mut raw = Vec::new();
    let mut buf = [0u8; 8 * 1024];
    loop {
        stream
            .set_read_timeout(Some(remaining(deadline).ok_or_else(timed_out)?))
            .map_err(err_str)?;
        match stream.read(&mut buf) {
            // The server has answered all it has: `Connection: close` means its
            // side ends the body.
            Ok(0) => break,
            Ok(n) => raw.extend_from_slice(&buf[..n]),
            Err(e) => match read_error_step(e.kind()) {
                // The read was cut short by something that is not the deadline;
                // the timeout set above still bounds the loop.
                ReadStep::Retry => continue,
                ReadStep::TimedOut => return Err(timed_out()),
                ReadStep::Fail => return Err(err_str(e)),
            },
        }
    }
    parse_response(&String::from_utf8_lossy(&raw))
}

/// The dial of one request: every address `host` names, tried in order, each
/// under `timeout`. A host the framework writes is an address, so this is
/// normally one try; a name resolves, and every answer gets its turn.
fn connect_host(host: &str, port: u16, timeout: Duration) -> std::io::Result<std::net::TcpStream> {
    let mut last = Err(std::io::Error::new(
        std::io::ErrorKind::AddrNotAvailable,
        "no address",
    ));
    for addr in std::net::ToSocketAddrs::to_socket_addrs(&(host, port))? {
        match std::net::TcpStream::connect_timeout(&addr, timeout) {
            Ok(stream) => return Ok(stream),
            Err(e) => last = Err(e),
        }
    }
    last
}

/// What the request has left of its deadline, `None` once it is spent. A zero
/// duration is no timeout to set (it puts the socket in non-blocking mode), so a
/// spent deadline ends the request instead of arming one more read.
fn remaining(deadline: Instant) -> Option<Duration> {
    let left = deadline.checked_duration_since(Instant::now())?;
    (!left.is_zero()).then_some(left)
}

/// What the read loop does with a failed read.
enum ReadStep {
    /// Read again: the read was cut short by something other than the deadline.
    Retry,
    /// The read timeout was reached, so the request's deadline is spent.
    TimedOut,
    /// A plain I/O failure.
    Fail,
}

/// Map a failed read to its step. A blocking read that reaches its timeout
/// answers `WouldBlock` on Unix and `TimedOut` on Windows: one event, two kinds
/// (`TcpStream::set_read_timeout`).
fn read_error_step(kind: std::io::ErrorKind) -> ReadStep {
    match kind {
        std::io::ErrorKind::Interrupted => ReadStep::Retry,
        std::io::ErrorKind::WouldBlock | std::io::ErrorKind::TimedOut => ReadStep::TimedOut,
        _ => ReadStep::Fail,
    }
}

/// The report for a request whose deadline passed before the answer arrived.
fn timed_out() -> String {
    strings::t("request-timed-out")
}

fn err_str(e: std::io::Error) -> String {
    e.to_string()
}

/// Split a raw HTTP response into its status code and decoded body.
/// Pure, so the chunked path is unit-testable without a socket.
fn parse_response(raw: &str) -> Result<Response, String> {
    let (head, body) = raw
        .split_once("\r\n\r\n")
        .ok_or_else(|| strings::t("truncated-response"))?;

    let status = head
        .lines()
        .next()
        .and_then(|l| l.split_whitespace().nth(1))
        .and_then(|c| c.parse::<u16>().ok())
        .ok_or_else(|| strings::t("unreadable-status"))?;

    let chunked = head
        .lines()
        .any(|l| l.to_ascii_lowercase().starts_with("transfer-encoding:") && l.contains("chunked"));

    let body = if chunked {
        dechunk(body)
    } else {
        body.to_string()
    };
    Ok(Response { status, body })
}

/// Decode `Transfer-Encoding: chunked` framing. A malformed stream yields what
/// was decoded so far: the callers read a status message or a JSON object out of
/// it, so a partial body degrades the reporting rather than the outcome.
fn dechunk(body: &str) -> String {
    let mut out = String::new();
    let mut rest = body;
    while let Some((size_line, after)) = rest.split_once("\r\n") {
        let size = size_line.split(';').next().unwrap_or("").trim();
        let Ok(n) = usize::from_str_radix(size, 16) else {
            break;
        };
        if n == 0 || after.len() < n {
            out.push_str(&after[..n.min(after.len())]);
            break;
        }
        out.push_str(&after[..n]);
        rest = after[n..].strip_prefix("\r\n").unwrap_or(&after[n..]);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::parse_response;
    use super::read_error_step;
    use super::request;
    use super::request_host;
    use super::ReadStep;
    use std::io::{Read, Write};
    use std::net::TcpListener;
    use std::time::{Duration, Instant};

    #[test]
    fn parses_a_plain_response() {
        let raw = "HTTP/1.1 200 OK\r\nContent-Length: 9\r\n\r\n{\"a\": 1}\n";
        let res = parse_response(raw).unwrap();
        assert_eq!(res.status, 200);
        assert!(res.body.starts_with("{\"a\": 1}"));
    }

    #[test]
    fn parses_a_chunked_response() {
        let raw = "HTTP/1.1 200 OK\r\nTransfer-Encoding: chunked\r\n\r\n\
                   4\r\n{\"a\"\r\n4\r\n: 1}\r\n0\r\n\r\n";
        let res = parse_response(raw).unwrap();
        assert_eq!(res.status, 200);
        assert_eq!(res.body, "{\"a\": 1}");
    }

    #[test]
    fn reports_a_non_200_status() {
        let raw = "HTTP/1.1 501 Not Implemented\r\nContent-Length: 2\r\n\r\n{}";
        assert_eq!(parse_response(raw).unwrap().status, 501);
    }

    #[test]
    fn truncated_response_is_an_error() {
        assert!(parse_response("HTTP/1.1 200 OK").is_err());
    }

    /// A loopback listener that answers a request with the whole reply, one
    /// byte every `gap`, and keeps the connection open. Every read is answered,
    /// so a read timeout alone never fires against it.
    fn trickling_listener(gap: Duration) -> u16 {
        let listener = TcpListener::bind("127.0.0.1:0").expect("bind a loopback port");
        let port = listener.local_addr().expect("the port").port();
        std::thread::spawn(move || {
            let Ok((mut stream, _addr)) = listener.accept() else {
                return;
            };
            // Drain the request head first, so the client is not left writing
            // into a socket nobody reads.
            let mut buf = Vec::new();
            let mut byte = [0u8; 1];
            loop {
                if stream.read(&mut byte).expect("read the request") == 0 {
                    return;
                }
                buf.push(byte[0]);
                if buf.ends_with(b"\r\n\r\n") {
                    break;
                }
            }
            let body = r#"{"data":[]}"#;
            let text = format!(
                "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\n\r\n{body}",
                body.len()
            );
            for byte in text.as_bytes() {
                // The client giving up is the only end: a broken pipe ends it.
                if stream.write_all(&[*byte]).is_err() || stream.flush().is_err() {
                    return;
                }
                std::thread::sleep(gap);
            }
        });
        port
    }

    #[test]
    fn the_timeout_bounds_the_whole_request_not_one_read() {
        // `timeout` is the request's deadline, not a per-read one: a server
        // that answers a byte every 300 ms answers every read, and a caller
        // that trusts the read timeout waits for the reply forever.
        let port = trickling_listener(Duration::from_millis(300));
        let started = Instant::now();
        let (tx, rx) = std::sync::mpsc::channel();
        std::thread::spawn(move || {
            let res = request(port, "GET", "/models", None, Duration::from_secs(1));
            let _ = tx.send(res.map(|res| res.status));
        });
        let outcome = match rx.recv_timeout(Duration::from_secs(6)) {
            Ok(outcome) => outcome,
            Err(std::sync::mpsc::RecvTimeoutError::Timeout) => panic!(
                "request did not return within 6 s of a server trickling a byte every 300 ms"
            ),
            Err(e) => panic!("the request thread panicked: {e:?}"),
        };
        let took = started.elapsed();

        assert!(
            took < Duration::from_secs(3),
            "a 1 s request ran {took:?} against a trickling server"
        );
        assert!(
            outcome.is_err(),
            "a reply that never completed is an error, got: {outcome:?}"
        );
    }

    /// A listener on `bind` that answers one request with 200 and hands back the
    /// port it got and the request head it read, so a test can read what the
    /// client wrote. `None` when `bind` is no address here (an IPv6 loopback on
    /// a host with no IPv6).
    fn head_listener(bind: &str) -> Option<(u16, std::sync::mpsc::Receiver<String>)> {
        let listener = std::net::TcpListener::bind(bind).ok()?;
        let port = listener.local_addr().ok()?.port();
        let (tx, rx) = std::sync::mpsc::channel();
        std::thread::spawn(move || {
            let Ok((mut stream, _addr)) = listener.accept() else {
                return;
            };
            // Drain the request head first, so the client is not left writing
            // into a socket nobody reads, and keep it for the caller.
            let mut head = Vec::new();
            let mut byte = [0u8; 1];
            loop {
                if stream.read(&mut byte).unwrap_or(0) == 0 {
                    return;
                }
                head.push(byte[0]);
                if head.ends_with(b"\r\n\r\n") {
                    break;
                }
            }
            let _ = tx.send(String::from_utf8_lossy(&head).to_string());
            let _ = stream.write_all(b"HTTP/1.1 200 OK\r\nContent-Length: 2\r\n\r\nok");
            let _ = stream.flush();
        });
        Some((port, rx))
    }

    /// The port of, and the head written by, the one request `request_host`
    /// sends to `host`: `None` when `bind` is no address here.
    fn head_written_to(host: &str, bind: &str) -> Option<(u16, String)> {
        let (port, rx) = head_listener(bind)?;
        let res = request_host(host, port, "GET", "/health", None, Duration::from_secs(5));
        match &res {
            Ok(res) => assert_eq!(
                res.status, 200,
                "the listener on {bind} answers the request"
            ),
            Err(why) => panic!("the request to {bind}:{port} failed: {why}"),
        }
        let head = match rx.recv_timeout(Duration::from_secs(5)) {
            Ok(head) => head,
            Err(e) => panic!("the listener never saw a request head: {e:?}"),
        };
        Some((port, head))
    }

    /// The `Host:` header of an IPv6 address carries brackets (the URL form of
    /// an IP literal): written bare, the last colon reads as the port separator
    /// and the server sees no address.
    #[test]
    fn the_host_header_brackets_an_ipv6_address() {
        let Some((port, head)) = head_written_to("::1", "[::1]:0") else {
            eprintln!("skipped: this host has no IPv6 loopback to bind");
            return;
        };
        assert!(
            head.lines().any(|l| l == format!("Host: [::1]:{port}")),
            "an IPv6 address in the Host header is bracketed, the request head is: {head:?}"
        );
    }

    /// The same header for an IPv4 address is written bare: brackets there are
    /// no address the server reads.
    #[test]
    fn the_host_header_of_an_ipv4_address_carries_no_brackets() {
        let Some((port, head)) = head_written_to("127.0.0.1", "127.0.0.1:0") else {
            eprintln!("skipped: this host has no IPv4 loopback to bind");
            return;
        };
        assert!(
            head.lines().any(|l| l == format!("Host: 127.0.0.1:{port}")),
            "an IPv4 address in the Host header is bare, the request head is: {head:?}"
        );
    }

    // The four tests below cover what a read error answers: retry
    // (`Interrupted`: a signal, not the deadline, the loop must not end on
    // it), the deadline (`WouldBlock` on Unix and `TimedOut` on Windows are
    // the same reached read timeout), or a plain failure.
    #[test]
    fn an_interrupted_read_is_retried() {
        assert!(
            matches!(
                read_error_step(std::io::ErrorKind::Interrupted),
                ReadStep::Retry
            ),
            "Interrupted must be retried, not end the request"
        );
    }

    #[test]
    fn a_wouldblock_read_is_the_deadline() {
        assert!(
            matches!(
                read_error_step(std::io::ErrorKind::WouldBlock),
                ReadStep::TimedOut
            ),
            "WouldBlock (the read timeout on Unix) must end the request as timed out"
        );
    }

    #[test]
    fn a_timedout_read_is_the_deadline() {
        // The same reached timeout on Windows answers `TimedOut`: the user
        // must get the `request-timed-out` key, not the raw OS message.
        assert!(
            matches!(
                read_error_step(std::io::ErrorKind::TimedOut),
                ReadStep::TimedOut
            ),
            "TimedOut (the read timeout on Windows) must end the request as timed out"
        );
    }

    #[test]
    fn any_other_read_error_fails_the_request() {
        for kind in [
            std::io::ErrorKind::ConnectionReset,
            std::io::ErrorKind::UnexpectedEof,
        ] {
            assert!(
                matches!(read_error_step(kind), ReadStep::Fail),
                "{kind:?} is neither the deadline nor interruptible"
            );
        }
    }
}
