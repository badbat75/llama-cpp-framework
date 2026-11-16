//! The same-origin guard, and the cap on the bodies the routes read.
//!
//! The bff answers on loopback, and the browser is the only caller that knows
//! about origins. A page served by any other service, on any port of this
//! machine, can open a request to this bff; it cannot read the answer
//! cross-origin, and it cannot set the session cookie. What it CAN do is
//! write: a `fetch` with `credentials: "include"` carries the session cookie of
//! the signed-in user, and a plain `application/json` POST from a page in the
//! same tab needs no CORS preflight, so nothing in the browser stops it. The
//! session cookie carries `SameSite=Strict`, and that is not the defence here:
//! `SameSite` compares the *site* of the page, its scheme, host and
//! registrable domain, and never its port, so a page served on
//! `http://localhost:9999` is the same site as this bff and the browser sends
//! the cookie with its request. The port is the only fact that tells this
//! bff's page from another local web app's, so the bff reads what the browser
//! says about the request:
//!
//! - `Sec-Fetch-Site`, when present, must be `same-origin` (this page) or
//!   `none` (a bookmarklet, a URL typed in).
//! - `Origin`, when present, must name this bff: the host and port of the
//!   `Host` header. A browser sends the port only when the page does not sit
//!   on the scheme's default one, and this bff answers plain HTTP, so a `Host`
//!   with no port is this bff on 80, and `http://localhost` is the origin for
//!   it.
//! - A header that is there twice, or there with bytes that are not text, is a
//!   header the bff cannot read: it takes neither value, and refuses.
//! - A request with neither header is not a browser request, so it is not a
//!   cross-site attack either: `curl` and a test's `oneshot` send neither, and
//!   pass.
//!
//! Those rules guard every method that is not a read (`GET`, `HEAD`,
//! `OPTIONS`): reading is what the page does on its own, and a read carries no
//! command. A `PATCH` from a foreign page is a refusal and not the route's
//! 405, because the guard answers where a request came from, never which
//! verbs exist.
//!
//! The name the request is addressed at is checked for every request, the
//! reads and the sign-in included: a `Host` header that is there must name a
//! loopback name (`localhost` in any case, an address in `127.0.0.0/8`,
//! `[::1]`, each with an optional port), and one that is not text names
//! nothing. A request addressed at any other name is refused, and that is what
//! stops a DNS rebinding: a page of a foreign site reaches a bff on loopback
//! only once its own name resolves to this machine, and then the `Origin` it
//! sends matches the `Host` so every rule above passes. The cookie is bound to
//! the host that set it, so a rebinding carries no session and the routes
//! behind a session are safe on their own; the sign-in asks for no session, so
//! the `Host` rule is what keeps a page at a foreign name from trying
//! passwords at this bff.
//!
//! The checks run before the session store is looked at, so a refusal never
//! touches it, never starts an idle deadline, and never sets a cookie. Each is
//! a 403 with the usual `{"key"}` body: the web UI never shows these keys,
//! because the UI is served by this bff and so always passes.
//!
//! Beside the guard sits the cap on the bodies: the sign-in reads its body on
//! the way to the password hash, and the boot write reads one as a signed-in
//! user, so both put a request's bytes in memory. The cap is one layer on the
//! whole router (`lib.rs`), so a route written later is capped too.

use crate::session::error;
use axum::extract::rejection::{BytesRejection, FailedToBufferBody};
use axum::http::header::{HeaderName, HOST};
use axum::http::uri::Authority;
use axum::http::{header, HeaderMap, Method, Request, StatusCode};
use axum::response::Response;
use std::net::{Ipv4Addr, Ipv6Addr};
use std::str::FromStr;

/// The cap on a body the bff reads: one layer of this many bytes stands in
/// front of every route (`lib.rs`), so a body of exactly this size is read and
/// one byte more is refused. Every body the bff reads is a few dozen bytes of
/// JSON, so this is a wide margin over any legitimate one.
pub(crate) const MAX_BODY: usize = 64 * 1024;

/// The header a browser sends with the site the request came from.
const SEC_FETCH_SITE: HeaderName = HeaderName::from_static("sec-fetch-site");

/// The port this bff is on when the `Host` header names none: the service
/// answers plain HTTP on loopback (`serve.rs`).
const OURS: u16 = 80;

/// The answer to a request that did not start on this bff, and so is answered
/// as if the route were not there.
fn refusal() -> Response {
    error(StatusCode::FORBIDDEN, "request-cross-origin", &[])
}

/// The answer to a `Host` that names a host this bff is not: the bff answers on
/// loopback only (`listen.rs`), so such a request is a page reaching it under a
/// name it does not serve.
fn bad_host() -> Response {
    error(StatusCode::FORBIDDEN, "request-bad-host", &[])
}

/// The answer to a body the extractor refused: over [`MAX_BODY`] a 413
/// `{"key": "payload-too-large"}`; anything else the extractor found is the
/// route's own "I cannot read you", the `malformed` key its caller hands.
pub(crate) fn body_refusal(rejection: &BytesRejection, malformed: &str) -> Response {
    if let BytesRejection::FailedToBufferBody(FailedToBufferBody::LengthLimitError(_)) = rejection {
        return error(StatusCode::PAYLOAD_TOO_LARGE, "payload-too-large", &[]);
    }
    error(StatusCode::BAD_REQUEST, malformed, &[])
}

/// The guard, for every route of the bff. A `Host` that names a host this bff
/// is not is refused for every request, the reads and the sign-in included. A
/// method that is not a read is refused in addition when it carries a
/// `Sec-Fetch-Site` that is neither `same-origin` nor `none`, an `Origin` that
/// is not this bff, or either of those headers twice or with bytes that are not
/// text.
pub(crate) async fn guard(
    request: Request<axum::body::Body>,
    next: axum::middleware::Next,
) -> Response {
    let headers = request.headers();
    if !is_loopback_host(headers) {
        return bad_host();
    }
    if is_read(request.method()) || is_this_bff(headers) {
        return next.run(request).await;
    }
    refusal()
}

/// The value of a header, lowercased, so the host comparison is the
/// case-insensitive one and the fetch metadata is read in any case: `None` when
/// the header is absent or is not text.
fn header_value(headers: &HeaderMap, name: &HeaderName) -> Option<String> {
    headers
        .get(name)
        .and_then(|value| value.to_str().ok())
        .map(|value| value.to_ascii_lowercase())
}

/// The methods that ask for nothing: a read is answered whatever the fetch
/// metadata says, because reading is what the page does on its own and carries
/// no command.
fn is_read(method: &Method) -> bool {
    method == Method::GET || method == Method::HEAD || method == Method::OPTIONS
}

/// Whether a header the guard reads is the request's one honest statement of
/// what the browser meant: at most once, and text where it is there. Twice is
/// two statements the bff cannot tell apart, and bytes that are not text are
/// a statement the bff cannot read.
fn is_honest(headers: &HeaderMap, name: &HeaderName) -> bool {
    let mut values = headers.get_all(name).into_iter();
    match values.next() {
        None => true,
        Some(only) => values.next().is_none() && only.to_str().is_ok(),
    }
}

/// Whether a request that is not a read came from this bff's own page. An
/// absent header is no refusal, only a present one that is not this bff is.
fn is_this_bff(headers: &HeaderMap) -> bool {
    for name in [&SEC_FETCH_SITE, &header::ORIGIN] {
        if !is_honest(headers, name) {
            return false;
        }
    }
    if let Some(site) = header_value(headers, &SEC_FETCH_SITE) {
        if site != "same-origin" && site != "none" {
            return false;
        }
    }
    let Some(origin) = header_value(headers, &header::ORIGIN) else {
        // No `Origin`: no browser is claiming a foreign page.
        return true;
    };
    // Both sides have to be readable: an `Origin` the bff cannot read, an
    // `Origin: null` included, is refused. A request with no `Host` names no
    // address of ours to compare against, so it is refused too.
    let host = header_value(headers, &HOST).unwrap_or_default();
    let Some(ours) = host_and_port(&host, OURS) else {
        return false;
    };
    let Some(theirs) = origin_and_port(&origin) else {
        return false;
    };
    theirs == ours
}

/// Whether the `Host` line names a host this bff is. A request with no `Host`
/// at all is answered: HTTP/1.0 sends none, and that is `curl`, the CLI and a
/// raw client, with no name to check.
fn is_loopback_host(headers: &HeaderMap) -> bool {
    let mut hosts = headers.get_all(HOST).into_iter();
    match (hosts.next(), hosts.next()) {
        (None, _) => true,
        (Some(only), None) => only
            .to_str()
            .ok()
            .map(|text| text.to_ascii_lowercase())
            .and_then(|text| host_and_port(&text, OURS))
            .is_some_and(|(host, _)| is_loopback_name(&host)),
        // Two `Host` lines name two hosts, at least one of them not ours.
        (Some(_), Some(_)) => false,
    }
}

/// Whether the host of a `Host` line, lowercased, is one of the names this bff
/// answers as: `localhost`, an address in `127.0.0.0/8`, the IPv6 loopback
/// written in brackets. Any other name is a page that believes it sits there
/// while the name resolves to this machine.
fn is_loopback_name(host: &str) -> bool {
    if host == "localhost" {
        return true;
    }
    // An IPv6 literal is bracketed on the wire; its address is what parses.
    let address = host
        .strip_prefix('[')
        .and_then(|bracketed| bracketed.strip_suffix(']'))
        .unwrap_or(host);
    address.parse::<Ipv4Addr>().is_ok_and(|v4| v4.is_loopback())
        || address.parse::<Ipv6Addr>().is_ok_and(|v6| v6.is_loopback())
}

/// The host and port an absolute origin names, the scheme's default port when
/// it names none: `http://localhost:3000` is `("localhost", 3000)`,
/// `http://localhost` is `("localhost", 80)`. What is not a plain origin, with
/// a user, a path, a query, a fragment or a port that is not a number, is
/// `None`, and the guard refuses it.
fn origin_and_port(origin: &str) -> Option<(String, u16)> {
    let (scheme, rest) = origin.split_once("://")?;
    // The default port of the scheme, and the only two a page comes from.
    let default = match scheme {
        "http" => 80,
        "https" => 443,
        _ => return None,
    };
    host_and_port(rest, default)
}

/// The host and port of an authority-form string, `default` when it names
/// none. The port is read out of the text, not out of the parsed authority, so
/// a port that is not a number is refused where the parser would drop it.
fn host_and_port(text: &str, default: u16) -> Option<(String, u16)> {
    let authority = Authority::from_str(text).ok()?;
    if authority.as_str() != text || text.contains('@') {
        // A path, a query, a fragment: the authority is the whole text.
        return None;
    }
    let host = authority.host();
    if host.is_empty() {
        return None;
    }
    // The host of a bracketed IPv6 literal keeps its brackets, so what is left
    // of the text is the port, if there is one.
    match &text[host.len()..] {
        "" | ":" => Some((host.to_string(), default)),
        port => Some((host.to_string(), port.strip_prefix(':')?.parse().ok()?)),
    }
}
