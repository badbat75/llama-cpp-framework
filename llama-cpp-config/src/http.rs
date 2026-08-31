//! Minimal loopback HTTP client for the running llama-server.
//!
//! Hand-rolled rather than pulled from a crate on purpose: the whole surface is
//! a handful of calls to 127.0.0.1 with JSON bodies, and the alternative
//! (`reqwest`) drags an async runtime into a crate that has none.
//! `Connection: close` is what lets the body be read to EOF, so no
//! `Content-Length` bookkeeping is needed on the way back; chunked responses are
//! decoded anyway because cpp-httplib may pick that framing regardless.
//!
//! Always 127.0.0.1, never the configured `Hostname`: that value is the address
//! llama-server BINDS (`0.0.0.0` is a common one and not connectable as a
//! destination on every stack), while loopback reaches it under every binding
//! the framework can produce.
//!
//! Two callers, with very different read timeouts, which is why the timeout is
//! the caller's argument and not a constant here: `slot_state` waits out a
//! multi-GiB KV-cache dump, `bench::exec` waits out a cold prefill that can run
//! into minutes on a long prompt. The CONNECT timeout is shared and short: a
//! server that does not accept a loopback connection in five seconds is not
//! going to answer either.

use std::io::{Read, Write};
use std::time::Duration;

/// Connect (and write) timeout. Deliberately short and shared: reaching a local
/// listener is instant or never, whatever the request then costs to answer.
pub(crate) const CONNECT_TIMEOUT: Duration = Duration::from_secs(5);

pub(crate) struct Response {
    pub status: u16,
    pub body: String,
}

/// Send one request to llama-server on the loopback interface and read the whole
/// response. `timeout` is the READ timeout, i.e. how long the answer may take.
pub(crate) fn request(
    port: u16,
    method: &str,
    path: &str,
    body: Option<&str>,
    timeout: Duration,
) -> Result<Response, String> {
    let addr = std::net::SocketAddr::from(([127, 0, 0, 1], port));
    let mut stream = std::net::TcpStream::connect_timeout(&addr, CONNECT_TIMEOUT)
        .map_err(|e| format!("cannot reach llama-server on port {port}: {e}"))?;
    stream.set_read_timeout(Some(timeout)).map_err(err_str)?;
    stream
        .set_write_timeout(Some(CONNECT_TIMEOUT))
        .map_err(err_str)?;

    let payload = body.unwrap_or("");
    let mut req = format!(
        "{method} {path} HTTP/1.1\r\n\
         Host: 127.0.0.1:{port}\r\n\
         Connection: close\r\n\
         Content-Type: application/json\r\n\
         Content-Length: {}\r\n\r\n",
        payload.len()
    );
    req.push_str(payload);
    stream.write_all(req.as_bytes()).map_err(err_str)?;
    stream.flush().map_err(err_str)?;

    let mut raw = Vec::new();
    stream.read_to_end(&mut raw).map_err(err_str)?;
    parse_response(&String::from_utf8_lossy(&raw))
}

fn err_str(e: std::io::Error) -> String {
    e.to_string()
}

/// Split a raw HTTP response into its status code and decoded body.
/// Pure, so the chunked path is unit-testable without a socket.
fn parse_response(raw: &str) -> Result<Response, String> {
    let (head, body) = raw
        .split_once("\r\n\r\n")
        .ok_or_else(|| "truncated response from llama-server".to_string())?;

    let status = head
        .lines()
        .next()
        .and_then(|l| l.split_whitespace().nth(1))
        .and_then(|c| c.parse::<u16>().ok())
        .ok_or_else(|| "unreadable status line from llama-server".to_string())?;

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
}
