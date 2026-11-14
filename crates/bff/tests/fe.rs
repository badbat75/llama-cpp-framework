//! The web UI the bff embeds: `fe/dist` (built by `npm run build` in `fe/`)
//! when it exists, else the fixture under `crates/bff/fe-fixture`, so
//! `cargo test -p pai-bff` never needs node. Each test reads the directory
//! the build embedded (`PAI_BFF_FE_DIR`, from `build.rs`) and checks the routes serve it, byte for byte, with
//! the content type of each file.

use axum::body::Body;
use axum::http::{header, Method, Request, StatusCode};
use http_body_util::BodyExt;
use std::path::{Path, PathBuf};
use tower::ServiceExt;

struct Reply {
    status: StatusCode,
    content_type: String,
    body: Vec<u8>,
}

async fn get(uri: &str) -> Reply {
    send(Method::GET, uri).await
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
    let body = response
        .into_body()
        .collect()
        .await
        .expect("body")
        .to_bytes()
        .to_vec();
    Reply {
        status,
        content_type,
        body,
    }
}

fn fixture() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("fe-fixture")
}

/// The directory the build embedded, absolute, as `build.rs` chose it
/// (`PRIVATE_AI_FE_DIST`, else `fe/dist`, else the fixture) and exported in
/// `PAI_BFF_FE_DIR`. Read from the build, not decided again here: a `fe/dist`
/// made after the build is not what the binary holds.
fn embedded() -> PathBuf {
    let dir = PathBuf::from(env!("PAI_BFF_FE_DIR"));
    assert!(
        dir.is_absolute(),
        "PAI_BFF_FE_DIR {} is relative",
        dir.display()
    );
    dir
}

/// The `/assets/...` paths the page names in a `src="..."` or `href="..."`.
fn asset_refs(index: &str) -> Vec<String> {
    let mut refs = Vec::new();
    for attr in ["src=\"", "href=\""] {
        for (at, _) in index.match_indices(attr) {
            let rest = &index[at + attr.len()..];
            let value = &rest[..rest.find('"').expect("closing quote")];
            if value.starts_with("/assets/") {
                refs.push(value.to_string());
            }
        }
    }
    refs
}

// ── Failure paths first ─────────────────────────────────────────────────

/// A file the build did not produce is a 404 under `/assets/` too, and a
/// path cannot climb out of it to another embedded file.
#[tokio::test]
async fn an_unknown_asset_is_not_found() {
    for uri in [
        "/assets/no-such-file.js",
        "/assets/",
        "/assets",
        "/assets/../index.html",
        "/assets/%2e%2e/index.html",
        "/fe-fixture/index.html",
    ] {
        let r = get(uri).await;
        assert_eq!(
            r.status,
            StatusCode::NOT_FOUND,
            "GET {uri}: {}",
            String::from_utf8_lossy(&r.body)
        );
    }
}

/// The page and its assets are read-only.
#[tokio::test]
async fn the_page_refuses_another_method() {
    let index = std::fs::read_to_string(embedded().join("index.html")).expect("index.html");
    let js = asset_refs(&index)
        .into_iter()
        .find(|r| r.ends_with(".js"))
        .expect("a script");
    for uri in ["/", js.as_str()] {
        let r = send(Method::POST, uri).await;
        assert_eq!(r.status, StatusCode::METHOD_NOT_ALLOWED, "POST {uri}");
    }
}

// ── What it serves ──────────────────────────────────────────────────────

/// Without `fe/dist` the fixture is embedded: it names a script and a
/// stylesheet, so both content types are checked without node.
#[test]
fn the_fixture_names_a_script_and_a_stylesheet() {
    let index = std::fs::read_to_string(fixture().join("index.html")).expect("fixture index");
    let refs = asset_refs(&index);
    assert!(refs.iter().any(|r| r.ends_with(".js")), "{refs:?}");
    assert!(refs.iter().any(|r| r.ends_with(".css")), "{refs:?}");
    for r in &refs {
        assert!(fixture().join(&r[1..]).is_file(), "fixture lacks {r}");
    }
}

/// `GET /` serves the embedded `index.html` as HTML.
#[tokio::test]
async fn root_serves_the_embedded_index() {
    let dir = embedded();
    let expected = std::fs::read(dir.join("index.html")).expect("index.html");
    let r = get("/").await;
    assert_eq!(r.status, StatusCode::OK);
    assert!(
        r.content_type.starts_with("text/html"),
        "content-type {:?}",
        r.content_type
    );
    assert_eq!(
        String::from_utf8_lossy(&r.body),
        String::from_utf8_lossy(&expected),
        "GET / is not {}",
        dir.display()
    );
}

/// Every asset the page names is served at its path, byte for byte, as
/// `text/javascript` (RFC 9239) or `text/css`.
#[tokio::test]
async fn the_assets_the_page_names_are_served_with_their_type() {
    let dir = embedded();
    let index = std::fs::read_to_string(dir.join("index.html")).expect("index.html");
    let refs = asset_refs(&index);
    assert!(
        refs.iter().any(|r| r.ends_with(".js")),
        "{} names no script: {refs:?}",
        dir.display()
    );
    for uri in &refs {
        let expected = std::fs::read(dir.join(&uri[1..])).expect("the named file");
        let r = get(uri).await;
        assert_eq!(r.status, StatusCode::OK, "GET {uri}");
        let wanted = if uri.ends_with(".js") {
            "text/javascript"
        } else if uri.ends_with(".css") {
            "text/css"
        } else {
            panic!("no content type pinned for {uri}")
        };
        assert!(
            r.content_type.starts_with(wanted),
            "GET {uri}: content-type {:?}, wanted {wanted}",
            r.content_type
        );
        assert!(r.body == expected, "GET {uri}: not the embedded bytes");
    }
}
