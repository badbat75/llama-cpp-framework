//! The bff's routes, without a socket: each test hands `pai_bff::router()`
//! one request through `tower::ServiceExt::oneshot`. The socket itself is
//! the executable's test (`tests/pai_bff_binary.rs`).
//!
//! Every reply carries `X-Frame-Options: DENY` and
//! `X-Content-Type-Options: nosniff`, each exactly once: the bff answers plain
//! HTTP on the address the administrator chose (`listen.rs`), so a page of
//! anywhere else must not be able to frame it, and no body may be read as a
//! type it is not. A Content-Security-Policy is agreed with the web UI first.

use axum::body::Body;
use axum::http::{header, Method, Request, StatusCode};
use http_body_util::BodyExt;
use tower::ServiceExt;

struct Reply {
    status: StatusCode,
    content_type: String,
    /// Every header of the reply, the name lowercased as `http` stores it.
    headers: Vec<(String, String)>,
    body: String,
}

async fn send(method: Method, uri: &str) -> Reply {
    let request = Request::builder()
        .method(method)
        .uri(uri)
        .body(Body::empty())
        .expect("request");
    let response = pai_bff::router()
        .oneshot(request)
        .await
        .expect("infallible");
    let status = response.status();
    let content_type = response
        .headers()
        .get(header::CONTENT_TYPE)
        .map(|v| v.to_str().expect("ascii content-type").to_string())
        .unwrap_or_default();
    let headers = response
        .headers()
        .iter()
        .map(|(name, value)| {
            (
                name.as_str().to_string(),
                String::from_utf8_lossy(value.as_bytes()).to_string(),
            )
        })
        .collect();
    let bytes = response
        .into_body()
        .collect()
        .await
        .expect("body")
        .to_bytes();
    Reply {
        status,
        content_type,
        headers,
        body: String::from_utf8(bytes.to_vec()).expect("utf-8 body"),
    }
}

impl Reply {
    /// The values the reply carries under `name`, in the order they came.
    fn values_of(&self, name: &str) -> Vec<&str> {
        self.headers
            .iter()
            .filter(|(n, _)| n == name)
            .map(|(_, v)| v.as_str())
            .collect()
    }

    /// The two headers that keep another site's page from framing this UI and
    /// from reading a body as a type it is not, each exactly once: the bff is
    /// reachable from the LAN over plain HTTP, so a page of anywhere else must
    /// not be able to show it, and no body may be sniffed into a script. No
    /// Content-Security-Policy here: that one is agreed with the web UI first.
    fn assert_security_headers(&self) {
        for (name, want) in [
            ("x-frame-options", "DENY"),
            ("x-content-type-options", "nosniff"),
        ] {
            assert_eq!(
                self.values_of(name),
                vec![want],
                "{name} exactly once: {:?}",
                self.headers
            );
        }
    }
}

// ── Failure paths first ─────────────────────────────────────────────────

/// A path the bff does not serve is a 404, under `/api/` as anywhere else.
#[tokio::test]
async fn an_unknown_path_is_not_found() {
    for uri in [
        "/no-such-page",
        "/api/no-such-call",
        "/api",
        "/api/version/extra",
    ] {
        let r = send(Method::GET, uri).await;
        assert_eq!(r.status, StatusCode::NOT_FOUND, "GET {uri}: {}", r.body);
    }
}

/// `/api/version` is read-only: another method is refused, not served.
#[tokio::test]
async fn version_refuses_another_method() {
    for method in [Method::POST, Method::PUT, Method::DELETE] {
        let r = send(method.clone(), "/api/version").await;
        assert_eq!(
            r.status,
            StatusCode::METHOD_NOT_ALLOWED,
            "{method} /api/version: {}",
            r.body
        );
    }
}

// ── What it serves ──────────────────────────────────────────────────────

/// `GET /api/version` answers JSON with the framework version, the one the
/// packages are named after (`version.workspace`).
#[tokio::test]
async fn version_answers_the_framework_version_as_json() {
    let r = send(Method::GET, "/api/version").await;
    assert_eq!(r.status, StatusCode::OK, "{}", r.body);
    assert!(
        r.content_type.starts_with("application/json"),
        "content-type {:?}",
        r.content_type
    );
    let json: serde_json::Value = serde_json::from_str(&r.body).expect("a JSON body");
    assert_eq!(
        json,
        serde_json::json!({ "version": env!("CARGO_PKG_VERSION") }),
        "{}",
        r.body
    );
}

/// `GET /` is an HTML page: the embedded web UI (`tests/fe.rs` checks which).
#[tokio::test]
async fn root_serves_an_html_page() {
    let r = send(Method::GET, "/").await;
    assert_eq!(r.status, StatusCode::OK, "{}", r.body);
    assert!(
        r.content_type.starts_with("text/html"),
        "content-type {:?}",
        r.content_type
    );
    let lower = r.body.to_ascii_lowercase();
    assert!(
        lower.contains("<!doctype html>") && lower.contains("</html>"),
        "a whole HTML document: {}",
        r.body
    );
}

// ── The headers every reply carries ─────────────────────────────────────

/// Every reply the router hands out carries the framing and the sniffing
/// header, each exactly once: the page, a JSON read, the 404 of a path the bff
/// does not serve and the 405 of a verb it does not route. The last two are the
/// router's own replies, built by no route, and a browser renders them too, so
/// whoever sets the headers runs after the router, not inside a route.
#[tokio::test]
async fn every_reply_carries_the_framing_and_sniffing_headers() {
    for (method, uri, status) in [
        (Method::GET, "/", StatusCode::OK),
        (Method::GET, "/api/version", StatusCode::OK),
        (Method::GET, "/api/no-such-call", StatusCode::NOT_FOUND),
        (Method::POST, "/api/version", StatusCode::METHOD_NOT_ALLOWED),
    ] {
        let r = send(method.clone(), uri).await;
        assert_eq!(r.status, status, "{method} {uri}: {}", r.body);
        r.assert_security_headers();
    }
}
