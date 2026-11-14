//! pai-bff's library: the modules of `pai-bff` (`main.rs`), over the shared
//! core (`pai-core`).

pub mod bff;
pub mod listen;

/// The bff's HTTP routes: `GET /api/version` and the page at `/`.
pub fn router() -> axum::Router {
    use axum::response::Html;
    use axum::routing::get;
    axum::Router::new()
        .route("/", get(|| async { Html(PAGE) }))
        .route(
            "/api/version",
            get(|| async {
                axum::Json(serde_json::json!({ "version": env!("CARGO_PKG_VERSION") }))
            }),
        )
}

/// The page at `/`: a placeholder until the web UI is embedded.
const PAGE: &str = include_str!("../assets/index.html");

// Cross-cutting tests (the lock lint); the HTTP and config tests are in
// tests/, through the public API.
#[cfg(test)]
mod tests;
