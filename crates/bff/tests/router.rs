//! The bff's routes, without a socket: each test hands `pai_bff::router()`
//! one request through `tower::ServiceExt::oneshot`. The socket itself is
//! the executable's test (`tests/pai_bff_binary.rs`).

use axum::body::Body;
use axum::http::{header, Method, Request, StatusCode};
use http_body_util::BodyExt;
use tower::ServiceExt;

struct Reply {
    status: StatusCode,
    content_type: String,
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
    let bytes = response
        .into_body()
        .collect()
        .await
        .expect("body")
        .to_bytes();
    Reply {
        status,
        content_type,
        body: String::from_utf8(bytes.to_vec()).expect("utf-8 body"),
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

/// `GET /` is an HTML page (the placeholder until the web UI is embedded).
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
