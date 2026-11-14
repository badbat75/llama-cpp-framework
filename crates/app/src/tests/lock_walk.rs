//! The lock and manifest scans of the "links no GUI" lints: pai-app's
//! (`deps_lint.rs`) and pai-cli's, which includes this file by path.

use std::collections::{BTreeMap, BTreeSet, VecDeque};

/// A package of the GUI stack, by its name in Cargo.lock.
pub(crate) fn is_gui_package(name: &str) -> bool {
    name == "slint"
        || name.starts_with("slint-")
        || name.starts_with("i-slint-")
        || name == "winit"
        || name.starts_with("skia-")
        || name == "rfd"
}

/// One `[[package]]` of a lock file: its version and its dependency entries
/// (`"name"`, `"name version"` or `"name version (source)"`).
pub(crate) struct Package {
    version: String,
    deps: Vec<String>,
}

/// The packages of a lock file, keyed by name (several versions of one name
/// are kept apart by version).
pub(crate) fn parse_lock(text: &str) -> BTreeMap<String, Vec<Package>> {
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
pub(crate) fn gui_packages_reached(
    lock: &BTreeMap<String, Vec<Package>>,
    root: &str,
) -> Vec<String> {
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
pub(crate) fn manifest_dependencies(text: &str) -> Vec<String> {
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
