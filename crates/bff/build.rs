//! Embeds the web UI: writes `fe_files.rs` into OUT_DIR, a table of every file
//! of the chosen directory (its path, `include_bytes!` of it, its content
//! type), which `lib.rs` includes. The directory is `PRIVATE_AI_FE_DIST` when
//! set, else the repo's `fe/dist` when it holds an `index.html` (`npm run
//! build` in `fe/`, which `xtask build` runs), else `fe-fixture`, so `cargo
//! test -p pai-bff` never needs node.
//!
//! Cargo reruns a build script when a `rerun-if-changed` directory changes:
//! "it will scan the entire directory for any modifications"
//! (https://doc.rust-lang.org/cargo/reference/build-scripts.html#rerun-if-changed).
//! The book does not say what a missing path does; cargo's source does, the
//! header of `src/compiler/fingerprint/mod.rs`: "If any input files are
//! missing, or are newer than the dep-info, then the unit is dirty", so a
//! missing path would rerun the script, and rebuild the crate, at every
//! build. On the fixture the script therefore watches what a later `npm run
//! build` reads, `fe/src` and `fe/index.html` (both exist), rather than the
//! missing `fe/dist`, and never `fe/` whole, whose `node_modules` npm rewrites
//! and cargo would scan file by file. A `fe/dist` built with no change there
//! is embedded once one of those changes, or at once through
//! `PRIVATE_AI_FE_DIST`, which `xtask build` sets. Until then the tests still
//! check what was embedded: the chosen directory is exported to the crate's
//! targets as `PAI_BFF_FE_DIR` (`cargo:rustc-env`), which `tests/fe.rs` reads.
//!
//! `PRIVATE_AI_FE_DIST` must be absolute: `tests/fe.rs` reads the same value
//! with `option_env!`, unresolved.

use std::fmt::Write as _;
use std::path::{Path, PathBuf};

const ENV: &str = "PRIVATE_AI_FE_DIST";

fn main() {
    let manifest =
        PathBuf::from(std::env::var_os("CARGO_MANIFEST_DIR").expect("CARGO_MANIFEST_DIR"));
    println!("cargo:rerun-if-env-changed={ENV}");

    let fe = manifest.join("../../fe");
    let dist = fe.join("dist");
    let dir = if let Some(d) = std::env::var_os(ENV) {
        let d = PathBuf::from(d);
        assert!(
            d.is_absolute(),
            "{ENV} must be an absolute path: {}",
            d.display()
        );
        watch(&d);
        d
    } else if dist.join("index.html").is_file() {
        watch(&dist);
        dist
    } else {
        let fixture = manifest.join("fe-fixture");
        watch(&fixture);
        watch(&fe.join("src"));
        watch(&fe.join("index.html"));
        fixture
    };
    let dir = dir
        .canonicalize()
        .unwrap_or_else(|e| panic!("{}: {e}", dir.display()));
    println!("cargo:rustc-env=PAI_BFF_FE_DIR={}", dir.display());
    assert!(
        dir.join("index.html").is_file(),
        "{} holds no index.html",
        dir.display()
    );

    let mut files = Vec::new();
    collect(&dir, &dir, &mut files);
    files.sort();

    let mut code = String::from("pub static FILES: &[(&str, &[u8], &str)] = &[\n");
    for (rel, path) in &files {
        writeln!(
            code,
            "    ({rel:?}, include_bytes!({path:?}), {:?}),",
            content_type(rel)
        )
        .expect("write to a String");
    }
    code.push_str("];\n");
    let out = PathBuf::from(std::env::var_os("OUT_DIR").expect("OUT_DIR")).join("fe_files.rs");
    std::fs::write(&out, code).unwrap_or_else(|e| panic!("{}: {e}", out.display()));
}

fn watch(dir: &Path) {
    println!("cargo:rerun-if-changed={}", dir.display());
}

/// Every file under `dir`, as (its `/`-separated path under `root`, its
/// absolute path).
fn collect(root: &Path, dir: &Path, files: &mut Vec<(String, String)>) {
    let entries = std::fs::read_dir(dir).unwrap_or_else(|e| panic!("{}: {e}", dir.display()));
    for entry in entries {
        let path = entry.expect("a directory entry").path();
        if path.is_dir() {
            collect(root, &path, files);
        } else {
            let rel = path
                .strip_prefix(root)
                .expect("under the root")
                .components()
                .map(|c| c.as_os_str().to_str().expect("a UTF-8 file name"))
                .collect::<Vec<_>>()
                .join("/");
            let abs = path.to_str().expect("a UTF-8 path").to_string();
            files.push((rel, abs));
        }
    }
}

/// The content type served for a file, by extension (JavaScript as
/// `text/javascript`, RFC 9239).
fn content_type(rel: &str) -> &'static str {
    match rel.rsplit_once('.').map(|(_, ext)| ext) {
        Some("html") => "text/html; charset=utf-8",
        Some("js") => "text/javascript; charset=utf-8",
        Some("css") => "text/css; charset=utf-8",
        _ => "application/octet-stream",
    }
}
