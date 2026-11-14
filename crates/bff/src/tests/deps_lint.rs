//! pai-bff links no GUI: it is a service on a machine that may have no
//! display, without Slint, Skia, winit or the file dialogs. The same lock walk
//! as pai-app's (`crates/app/src/tests/lock_walk.rs`, included by path).

use std::path::Path;

#[path = "../../../app/src/tests/lock_walk.rs"]
mod lock_walk;

use lock_walk::{gui_packages_reached, is_gui_package, manifest_dependencies, parse_lock};

#[test]
fn pai_bff_reaches_no_gui_package_in_the_lock() {
    let root = Path::new(concat!(env!("CARGO_MANIFEST_DIR"), "/../.."));
    let text = std::fs::read_to_string(root.join("Cargo.lock")).expect("Cargo.lock");
    let found = gui_packages_reached(&parse_lock(&text), "pai-bff");
    assert!(
        found.is_empty(),
        "pai-bff links the GUI stack:\n  {}",
        found.join("\n  ")
    );
}

#[test]
fn pai_bff_manifest_names_no_gui_package() {
    let text = std::fs::read_to_string(Path::new(env!("CARGO_MANIFEST_DIR")).join("Cargo.toml"))
        .expect("crates/bff/Cargo.toml");
    let deps = manifest_dependencies(&text);
    assert!(
        deps.contains(&"pai-core".to_string()),
        "the scan reads the manifest: {deps:?}"
    );
    let gui: Vec<&String> = deps.iter().filter(|d| is_gui_package(d)).collect();
    assert!(gui.is_empty(), "crates/bff/Cargo.toml names {gui:?}");
}
