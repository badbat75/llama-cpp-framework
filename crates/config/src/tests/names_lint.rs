//! The project's names in the code of the three crates: the product is
//! "Private AI Framework", the executables `pai-config` and `pai-manager`, the
//! user and system trees `private-ai`, the registry key `Software\private-ai`
//! and the test redirect `PRIVATE_AI_DATA_ROOT`, with no fallback to the old
//! names. A plain text scan of every `.rs` and `.slint` file, `build.rs` and
//! `Cargo.toml`, whole-line comments left out (the docs say the old names
//! where they tell the history). The engine's own names stay: `llama-server`,
//! the unit `llama.cpp@<id>`, llama.cpp itself; so does the systemd service
//! `private-ai-manager.service`.

use std::path::{Path, PathBuf};

/// The old names, none of which a line of code may hold.
const OLD_NAMES: [&str; 11] = [
    "llama-cpp-config",
    "llama_cpp_config",
    "llama.cpp-framework",
    "llama.cpp Config",
    concat!("LLAMA_CPP_CONFIG", "_DATA_ROOT"),
    concat!("LLAMA_FW", "_DIST_ROOT"),
    r"Software\llama.cpp",
    r"ProgramData\llama.cpp",
    "/etc/llama.cpp",
    "/opt/llama.cpp",
    r#"join("llama.cpp")"#,
];

/// The old manager executable; `private-ai-manager.service` keeps its name.
const OLD_MANAGER: &str = "private-ai-manager";

fn files(dir: &Path, out: &mut Vec<PathBuf>) {
    for entry in std::fs::read_dir(dir).expect("read dir") {
        let path = entry.expect("dir entry").path();
        if path.is_dir() {
            files(&path, out);
        } else if path.extension().is_some_and(|e| e == "rs" || e == "slint") {
            out.push(path);
        }
    }
}

/// Every line of code (not a whole-line comment) that names an old name.
fn hits(path: &Path) -> Vec<String> {
    let text = std::fs::read_to_string(path).expect("read source");
    let toml = path.extension().is_some_and(|e| e == "toml");
    text.lines()
        .enumerate()
        .filter(|(_, line)| {
            let t = line.trim_start();
            !(t.starts_with("//") || (toml && t.starts_with('#')))
        })
        .filter(|(_, line)| {
            OLD_NAMES.iter().any(|old| line.contains(old))
                || line
                    .replace("private-ai-manager.service", "")
                    .contains(OLD_MANAGER)
        })
        .map(|(n, line)| format!("{}:{}: {}", path.display(), n + 1, line.trim()))
        .collect()
}

#[test]
fn no_code_names_the_old_project() {
    let crates = Path::new(env!("CARGO_MANIFEST_DIR")).join("..");
    let mut sources = Vec::new();
    for c in ["config", "core", "manager"] {
        let root = crates.join(c);
        files(&root.join("src"), &mut sources);
        if root.join("ui").is_dir() {
            files(&root.join("ui"), &mut sources);
        }
        for extra in ["build.rs", "Cargo.toml"] {
            if root.join(extra).is_file() {
                sources.push(root.join(extra));
            }
        }
    }
    let this = Path::new(file!()).file_name().expect("own name");
    let found: Vec<String> = sources
        .iter()
        .filter(|p| p.file_name() != Some(this))
        .flat_map(|p| hits(p))
        .collect();
    assert!(found.is_empty(), "old names in code:\n{}", found.join("\n"));
}

#[test]
fn the_scan_skips_comments_and_keeps_the_service_name() {
    let dir = tempfile::tempdir().expect("tempdir");
    let file = dir.path().join("x.rs");
    std::fs::write(
        &file,
        "// llama-cpp-config in a comment\n\
         let unit = \"private-ai-manager.service\";\n\
         let exe = \"private-ai-manager\";\n\
         let p = base.join(\"llama.cpp\");\n",
    )
    .expect("fixture");
    let found = hits(&file);
    assert_eq!(found.len(), 2, "{found:?}");
    assert!(found[0].ends_with("let exe = \"private-ai-manager\";"));
    assert!(found[1].contains("join(\"llama.cpp\")"));
}
