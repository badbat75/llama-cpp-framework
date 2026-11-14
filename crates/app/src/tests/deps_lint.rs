//! pai-app links no GUI: a headless server administered over SSH takes the
//! CLI, and the CLI takes this crate, without Slint, Skia, winit or the file
//! dialogs. Read from the workspace's Cargo.lock, which lists every package
//! cargo resolved with what it depends on (for every target, dev and build
//! dependencies included, so the walk can only over-report).

use std::collections::{BTreeMap, BTreeSet, VecDeque};
use std::path::Path;

/// A package of the GUI stack, by its name in Cargo.lock.
fn is_gui_package(name: &str) -> bool {
    name == "slint"
        || name.starts_with("slint-")
        || name.starts_with("i-slint-")
        || name == "winit"
        || name.starts_with("skia-")
        || name == "rfd"
}

/// One `[[package]]` of a lock file: its version and its dependency entries
/// (`"name"`, `"name version"` or `"name version (source)"`).
struct Package {
    version: String,
    deps: Vec<String>,
}

/// The packages of a lock file, keyed by name (several versions of one name
/// are kept apart by version).
fn parse_lock(text: &str) -> BTreeMap<String, Vec<Package>> {
    let mut out: BTreeMap<String, Vec<Package>> = BTreeMap::new();
    for block in text.split("[[package]]").skip(1) {
        let mut name = String::new();
        let mut version = String::new();
        let mut deps = Vec::new();
        let mut in_deps = false;
        for line in block.lines() {
            let line = line.trim();
            if in_deps {
                if line.starts_with(']') {
                    in_deps = false;
                } else if let Some(dep) = line.trim_end_matches(',').strip_prefix('"') {
                    deps.push(dep.trim_end_matches('"').to_string());
                }
            } else if let Some(v) = line.strip_prefix("name = ") {
                name = v.trim_matches('"').to_string();
            } else if let Some(v) = line.strip_prefix("version = ") {
                version = v.trim_matches('"').to_string();
            } else if line.starts_with("dependencies = [") {
                in_deps = !line.ends_with(']');
            }
        }
        out.entry(name).or_default().push(Package { version, deps });
    }
    out
}

/// Every GUI package reached from `root`, each with the path that reaches it.
fn gui_packages_reached(lock: &BTreeMap<String, Vec<Package>>, root: &str) -> Vec<String> {
    let mut seen = BTreeSet::new();
    let mut queue = VecDeque::from([(root.to_string(), None::<String>, root.to_string())]);
    let mut found = Vec::new();
    while let Some((name, version, path)) = queue.pop_front() {
        let versions = lock
            .get(&name)
            .unwrap_or_else(|| panic!("{name}: not in Cargo.lock (reached by {path})"));
        let pkg = match &version {
            Some(v) => versions
                .iter()
                .find(|p| &p.version == v)
                .unwrap_or_else(|| panic!("{name} {v}: not in Cargo.lock")),
            None => &versions[0],
        };
        if !seen.insert((name.clone(), pkg.version.clone())) {
            continue;
        }
        if is_gui_package(&name) {
            found.push(path.clone());
        }
        for dep in &pkg.deps {
            let mut parts = dep.split_whitespace();
            let dep_name = parts.next().expect("dependency name").to_string();
            let dep_version = parts.next().map(str::to_string);
            let dep_path = format!("{path} -> {dep_name}");
            queue.push_back((dep_name, dep_version, dep_path));
        }
    }
    found
}

/// The dependency names a manifest declares, in any table (`name = ...` under
/// a `*dependencies` table, or a `[...dependencies.name]` header).
fn manifest_dependencies(text: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut in_deps = false;
    for line in text.lines() {
        let line = line.split('#').next().unwrap_or("").trim();
        if let Some(header) = line.strip_prefix('[') {
            let header = header
                .trim_end_matches(']')
                .trim_matches(|c| c == '[' || c == ']');
            in_deps = header.ends_with("dependencies");
            if let Some((table, name)) = header.rsplit_once('.') {
                if table.ends_with("dependencies") {
                    out.push(name.trim_matches('"').to_string());
                }
            }
        } else if in_deps {
            if let Some((key, _)) = line.split_once('=') {
                out.push(key.trim().trim_matches('"').to_string());
            }
        }
    }
    out
}

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
