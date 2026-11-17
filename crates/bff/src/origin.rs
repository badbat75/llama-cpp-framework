//! The same-origin guard, and the cap on the bodies the routes read.
//!
//! The bff answers plain HTTP, and the browser is the only caller that knows
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
//! reads and the sign-in included: a `Host` header that is there must name
//! this bff, and one that is not text names nothing. It names this bff when it
//! is one of this machine's names, which the service reads at its start and
//! hands the guard (`listen::host_names`: the host name, that name under
//! `.local` and under each `resolv.conf` search domain, or the `[Bff] Hosts`
//! of `bff.ini` when that sets them), or an IP literal (an IPv4 address, an
//! IPv6 one in brackets, each with an optional port), or `localhost`. One
//! trailing dot on the name is the root of the DNS tree and names nothing of
//! its own, so the guard reads `aorus.bombicci.local.` as
//! `aorus.bombicci.local` and `localhost.` as `localhost`; it takes one dot,
//! never more, so `.` and `..` and `name..` name no host and are refused. The
//! name is read in any case and its port is not the question, the port is the
//! `Origin` rule's. `localhost` and the loopback addresses (`127.0.0.0/8` and
//! `[::1]`) are answered where the listen serves the loopback, a loopback
//! address or the wildcard: a bff bound to one LAN address is not reachable at
//! the loopback at all, so there such a `Host` names a host this bff is not.
//! Every other address is answered whatever the listen.
//!
//! A request addressed at a name that is none of these is refused, and that is
//! what stops a DNS rebinding: a page of a foreign site reaches this bff only
//! once its own name resolves to this machine, and a name built on one of ours
//! (`aorus.bombicci.local.evil.example`) is not in the list. The cookie is
//! bound to the host that set it, so a rebinding carries no session and the
//! routes behind a session are safe on their own; the sign-in asks for no
//! session, so the `Host` rule is what keeps a page at a foreign name from
//! trying passwords at this bff.
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
use axum::extract::State;
use axum::http::header::{HeaderName, HOST};
use axum::http::uri::Authority;
use axum::http::{header, HeaderMap, Method, Request, StatusCode};
use axum::response::Response;
use std::net::{IpAddr, Ipv4Addr, SocketAddr};
use std::str::FromStr;
use std::sync::Arc;

/// The cap on a body the bff reads: one layer of this many bytes stands in
/// front of every route (`lib.rs`), so a body of exactly this size is read and
/// one byte more is refused. Every body the bff reads is a few dozen bytes of
/// JSON, so this is a wide margin over any legitimate one.
pub(crate) const MAX_BODY: usize = 64 * 1024;

/// The header a browser sends with the site the request came from.
const SEC_FETCH_SITE: HeaderName = HeaderName::from_static("sec-fetch-site");

/// The port this bff is on when the `Host` header names none: the service
/// answers plain HTTP (`serve.rs`).
const OURS: u16 = 80;

/// The answer to a request that did not start on this bff, and so is answered
/// as if the route were not there.
fn refusal() -> Response {
    error(StatusCode::FORBIDDEN, "request-cross-origin", &[])
}

/// The answer to a `Host` that names a host this bff is not: the bff answers
/// at its machine's names, at `localhost` and the loopback where its listen
/// serves them, and at every other address, so a name outside those is a page
/// reaching it under a name it does not serve.
fn bad_host() -> Response {
    error(StatusCode::FORBIDDEN, "request-bad-host", &[])
}

/// What the `Host` guard answers as this bff: the machine's names and the
/// address the bff is bound to. The service builds it once at its start
/// (`listen::host_names`, `listen::config_path`), a test hands it the names and
/// the listen it wants, so no test reads the real host name,
/// `/etc/resolv.conf` or `bff.ini`. Cloned into the guard's state, shared
/// between the router and every request.
#[derive(Clone)]
pub(crate) struct HostPolicy {
    /// The names answered beside `localhost` and the addresses, lowercase as
    /// `listen::host_names` gives them, none with a trailing dot: the guard
    /// reads the `Host` line in any case and takes its trailing dot off. A
    /// router that is handed none answers only `localhost` (under the listen
    /// rule) and the addresses.
    names: Arc<Vec<String>>,
    /// The address the bff listens on, which decides whether `localhost` and
    /// the loopback addresses are answered.
    listen: SocketAddr,
}

impl HostPolicy {
    /// The policy of one router: `names` beside `localhost` and the addresses,
    /// and `listen`, the address the bff is bound to.
    pub(crate) fn new(names: Vec<String>, listen: SocketAddr) -> Self {
        Self {
            names: Arc::new(names),
            listen,
        }
    }

    /// The policy of a router with no name of the machine and a loopback
    /// listen: `localhost`, the loopback addresses and every other address are
    /// answered, as they were before the machine had names to hand.
    pub(crate) fn loopback() -> Self {
        Self::new(Vec::new(), SocketAddr::from((Ipv4Addr::LOCALHOST, 0)))
    }

    /// Whether the listen serves the loopback: it is bound to a loopback
    /// address, so it is reachable there, or to the wildcard (`0.0.0.0`,
    /// `[::]`), so it is reachable at every address of the machine.
    fn serves_loopback(&self) -> bool {
        self.listen.ip().is_loopback() || self.listen.ip().is_unspecified()
    }
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
    State(policy): State<HostPolicy>,
    request: Request<axum::body::Body>,
    next: axum::middleware::Next,
) -> Response {
    let headers = request.headers();
    if !is_our_host(headers, &policy) {
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

/// Whether the `Host` line names a host this bff is, as `policy` defines it. A
/// request with no `Host` at all is answered: HTTP/1.0 sends none, and that is
/// `curl`, the CLI and a raw client, with no name to check.
fn is_our_host(headers: &HeaderMap, policy: &HostPolicy) -> bool {
    let mut hosts = headers.get_all(HOST).into_iter();
    match (hosts.next(), hosts.next()) {
        (None, _) => true,
        (Some(only), None) => only
            .to_str()
            .ok()
            .map(|text| text.to_ascii_lowercase())
            .and_then(|text| host_and_port(&text, OURS))
            .is_some_and(|(host, _)| is_our_name(&host, policy)),
        // Two `Host` lines name two hosts, at least one of them not ours.
        (Some(_), Some(_)) => false,
    }
}

/// Whether the host of a `Host` line, lowercased, is a name this bff answers
/// as: one of the machine's names, an IP literal (an IPv4 address, an IPv6 one
/// written in brackets), or `localhost`, each read with one trailing dot
/// removed: that dot is the root of the DNS tree and names nothing of its own,
/// so `aorus.bombicci.local.` is the name `aorus.bombicci.local` and
/// `localhost.` is `localhost`. One dot is all the rule takes off, so `.` and
/// `..` and a name with two of them at the end name no host and are refused:
/// the list holds no entry with a trailing dot (`listen::host_names`).
/// `localhost` and a loopback address are answered only where the listen
/// serves the loopback: the bff bound to one LAN address is not reachable
/// there. Every other address is answered whatever the listen, the bff is
/// where the address says it is. Any other name is a page that believes it
/// sits here while the name resolves to this machine.
fn is_our_name(host: &str, policy: &HostPolicy) -> bool {
    let name = host.strip_suffix('.').unwrap_or(host);
    if policy.names.iter().any(|listed| listed == name) {
        return true;
    }
    match as_address(name) {
        Some(address) => !address.is_loopback() || policy.serves_loopback(),
        // Not an address: only `localhost`, which stands for the loopback.
        None => name == "localhost" && policy.serves_loopback(),
    }
}

/// The address a `Host` line's host is, `None` when it is a name: an IPv4
/// address, an IPv6 one written in brackets on the wire (its brackets come off
/// before the parse, and an IPv6 with a zone id parses as no address).
fn as_address(host: &str) -> Option<IpAddr> {
    let text = host
        .strip_prefix('[')
        .and_then(|bracketed| bracketed.strip_suffix(']'))
        .unwrap_or(host);
    text.parse().ok()
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
