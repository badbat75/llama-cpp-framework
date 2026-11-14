//! pai-cli links no GUI: `pai` is what a headless server administered over
//! SSH runs, without Slint, Skia, winit or the file dialogs. The same lock walk
//! as pai-app's (`crates/app/src/tests/lock_walk.rs`, included by path).

use std::path::Path;

#[path = "../../../app/src/tests/lock_walk.rs"]
mod lock_walk;

use lock_walk::{gui_packages_reached, is_gui_package, manifest_dependencies, parse_lock};

#[test]
fn pai_cli_reaches_no_gui_package_in_the_lock() {
    let root = Path::new(concat!(env!("CARGO_MANIFEST_DIR"), "/../.."));
    let text = std::fs::read_to_string(root.join("Cargo.lock")).expect("Cargo.lock");
    let found = gui_packages_reached(&parse_lock(&text), "pai-cli");
    assert!(
        found.is_empty(),
        "pai-cli links the GUI stack:\n  {}",
        found.join("\n  ")
    );
}

#[test]
fn pai_cli_manifest_names_no_gui_package() {
    let text = std::fs::read_to_string(Path::new(env!("CARGO_MANIFEST_DIR")).join("Cargo.toml"))
        .expect("crates/cli/Cargo.toml");
    let deps = manifest_dependencies(&text);
    assert!(
        deps.contains(&"pai-app".to_string()),
        "the scan reads the manifest: {deps:?}"
    );
    let gui: Vec<&String> = deps.iter().filter(|d| is_gui_package(d)).collect();
    assert!(gui.is_empty(), "crates/cli/Cargo.toml names {gui:?}");
}
