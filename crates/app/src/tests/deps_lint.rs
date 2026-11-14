//! pai-app links no GUI: a headless server administered over SSH takes the
//! CLI, and the CLI takes this crate, without Slint, Skia, winit or the file
//! dialogs. Read from the workspace's Cargo.lock, which lists every package
//! cargo resolved with what it depends on (for every target, dev and build
//! dependencies included, so the walk can only over-report).

use std::path::Path;

use super::lock_walk::{gui_packages_reached, is_gui_package, manifest_dependencies, parse_lock};

fn workspace_root() -> &'static Path {
    Path::new(concat!(env!("CARGO_MANIFEST_DIR"), "/../.."))
}

#[test]
fn pai_app_reaches_no_gui_package_in_the_lock() {
    let text = std::fs::read_to_string(workspace_root().join("Cargo.lock")).expect("Cargo.lock");
    let lock = parse_lock(&text);
    let found = gui_packages_reached(&lock, "pai-app");
    assert!(
        found.is_empty(),
        "pai-app links the GUI stack:\n  {}",
        found.join("\n  ")
    );
}

#[test]
fn pai_app_manifest_names_no_gui_package() {
    let text = std::fs::read_to_string(Path::new(env!("CARGO_MANIFEST_DIR")).join("Cargo.toml"))
        .expect("crates/app/Cargo.toml");
    let deps = manifest_dependencies(&text);
    assert!(
        deps.contains(&"pai-core".to_string()),
        "the scan reads the manifest: {deps:?}"
    );
    let gui: Vec<&String> = deps.iter().filter(|d| is_gui_package(d)).collect();
    assert!(gui.is_empty(), "crates/app/Cargo.toml names {gui:?}");
}

// The scans themselves, on fixtures, so a green above is not a broken parser.

#[test]
fn lock_walk_finds_a_gui_package_however_deep() {
    let lock = parse_lock(
        r#"
[[package]]
name = "pai-app"
version = "2.4.0"
dependencies = [
 "image",
 "pai-core",
]

[[package]]
name = "pai-core"
version = "2.4.0"
dependencies = [
 "toml 1.0.0",
]

[[package]]
name = "toml"
version = "0.5.0"
dependencies = [
 "i-slint-core",
]

[[package]]
name = "toml"
version = "1.0.0"

[[package]]
name = "image"
version = "0.25.0"
source = "registry+https://github.com/rust-lang/crates.io-index"
dependencies = [
 "rfd 0.17.0 (registry+https://github.com/rust-lang/crates.io-index)",
]

[[package]]
name = "rfd"
version = "0.17.0"
source = "registry+https://github.com/rust-lang/crates.io-index"
dependencies = [
 "winit",
]

[[package]]
name = "winit"
version = "0.30.0"
"#,
    );
    // The versioned entry picks toml 1.0.0, not the 0.5.0 that would reach Slint.
    assert_eq!(
        gui_packages_reached(&lock, "pai-app"),
        vec![
            "pai-app -> image -> rfd".to_string(),
            "pai-app -> image -> rfd -> winit".to_string(),
        ],
    );
    assert_eq!(
        gui_packages_reached(&lock, "rfd"),
        vec!["rfd".to_string(), "rfd -> winit".to_string()],
    );
    assert!(gui_packages_reached(&lock, "pai-core").is_empty());
}

#[test]
fn gui_names_are_the_slint_skia_winit_and_dialog_packages() {
    for name in [
        "slint",
        "slint-build",
        "i-slint-core",
        "i-slint-backend-testing",
        "winit",
        "skia-safe",
        "skia-bindings",
        "rfd",
    ] {
        assert!(is_gui_package(name), "{name}");
    }
    for name in [
        "pai-core",
        "image",
        "rusqlite",
        "windows-sys",
        "winnow",
        "regex",
    ] {
        assert!(!is_gui_package(name), "{name}");
    }
}

#[test]
fn manifest_scan_reads_every_dependency_table() {
    let deps = manifest_dependencies(
        r#"
[package]
name = "x"

[dependencies]
pai-core = { path = "../core" }
# slint = "1.18"

[target.'cfg(target_os = "linux")'.dependencies]
rfd = { version = "0.17" }

[dev-dependencies]
i-slint-backend-testing = "1.18"

[build-dependencies.slint-build]
version = "1.18"

[features]
winit = []
"#,
    );
    assert_eq!(
        deps,
        ["pai-core", "rfd", "i-slint-backend-testing", "slint-build"],
    );
}
