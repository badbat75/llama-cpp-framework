//! The project's names in the code of the three crates: the product is
//! "Private AI Framework", the executables `pai-config` and `pai-manager`, the
//! user and system trees `private-ai`, the registry key `Software\private-ai`
//! and the test redirect `PRIVATE_AI_DATA_ROOT`, with no fallback to the old
//! names (no `LLAMA_FW_` build variable either). Only the migration module
//! (`MIGRATION_MODULE`) may name the old tree and login entry it moves. A plain text scan of every `.rs` and `.slint` file, `build.rs` and
//! `Cargo.toml`, whole-line comments left out (the docs say the old names
//! where they tell the history). The engine's own names stay: `llama-server`,
//! the unit `llama.cpp@<id>`, the engine's config directory `/etc/llama.cpp`,
//! llama.cpp itself; so does the manager's
//! service name: the systemd unit `private-ai-manager.service`, the named
//! pipe `\\.\pipe\private-ai-manager` and the Windows service
//! `"private-ai-manager"` in `WINDOWS_SERVICE_MODULE`.

use std::path::{Path, PathBuf};

/// The old names, none of which a line of code may hold.
const OLD_NAMES: [&str; 10] = [
    "llama-cpp-config",
    "llama_cpp_config",
    "llama.cpp-framework",
    "llama.cpp Config",
    concat!("LLAMA_CPP_CONFIG", "_DATA_ROOT"),
    // Every old build variable (`_DIST_ROOT`, `_ROCM`, `_GPU_LOCK`, ...).
    concat!("LLAMA_FW", "_"),
    r"Software\llama.cpp",
    r"ProgramData\llama.cpp",
    "/opt/llama.cpp",
    r#"join("llama.cpp")"#,
];

/// The one file allowed to name the old user tree and login entry, relative
/// to `crates/`: the migration must find them. Only `MIGRATION_NAMES` are
/// accepted there, every other old name is still refused.
const MIGRATION_MODULE: &str = "core/src/migrate.rs";
const MIGRATION_NAMES: [&str; 2] = ["llama-cpp-config", r#"join("llama.cpp")"#];

/// The old manager executable; the manager's service keeps the name
/// (`MANAGER_SERVICE_NAMES`).
const OLD_MANAGER: &str = "private-ai-manager";

/// The service's own name, accepted anywhere: the systemd unit and the
/// Windows named pipe.
const MANAGER_SERVICE_NAMES: [&str; 2] =
    ["private-ai-manager.service", r"\pipe\private-ai-manager"];

/// The one file allowed to spell the Windows service name as a bare string,
/// relative to `crates/`; `"private-ai-manager.exe"` stays refused there.
const WINDOWS_SERVICE_MODULE: &str = "manager/src/manager/windows.rs";
const WINDOWS_SERVICE_NAME: &str = "\"private-ai-manager\"";

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
    let exempt: &[&str] = if path.ends_with(MIGRATION_MODULE) {
        &MIGRATION_NAMES
    } else {
        &[]
    };
    text.lines()
        .enumerate()
        .filter(|(_, line)| {
            let t = line.trim_start();
            !(t.starts_with("//") || (toml && t.starts_with('#')))
        })
        .filter(|(_, line)| {
            OLD_NAMES
                .iter()
                .filter(|old| !exempt.contains(old))
                .any(|old| line.contains(old))
                || manager_names_left(path, line).contains(OLD_MANAGER)
        })
        .map(|(n, line)| format!("{}:{}: {}", path.display(), n + 1, line.trim()))
        .collect()
}

/// `line` with the manager's service names it may hold taken out.
fn manager_names_left(path: &Path, line: &str) -> String {
    let mut left = line.to_string();
    for name in MANAGER_SERVICE_NAMES {
        left = left.replace(name, "");
    }
    if path.ends_with(WINDOWS_SERVICE_MODULE) {
        left = left.replace(WINDOWS_SERVICE_NAME, "");
    }
    left
}

#[test]
fn no_code_names_the_old_project() {
    let crates = Path::new(env!("CARGO_MANIFEST_DIR")).join("..");
    let mut sources = Vec::new();
    for c in ["app", "bff", "cli", "config", "core", "manager"] {
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

/// The pipe path is the service's name anywhere; the bare name only in the
/// Windows service module, and only as the whole string.
#[test]
fn the_scan_keeps_the_pipe_and_the_windows_service_name() {
    let dir = tempfile::tempdir().expect("tempdir");
    let other = dir.path().join("x.rs");
    std::fs::write(
        &other,
        "pub const PIPE_NAME: &str = r\"\\\\.\\pipe\\private-ai-manager\";\n\
         pub const MANAGER_SERVICE: &str = \"private-ai-manager\";\n",
    )
    .expect("fixture");
    let found = hits(&other);
    assert_eq!(found.len(), 1, "{found:?}");
    assert!(found[0].contains("MANAGER_SERVICE"), "{found:?}");

    let module = dir.path().join(WINDOWS_SERVICE_MODULE);
    std::fs::create_dir_all(module.parent().expect("parent")).expect("dirs");
    std::fs::write(
        &module,
        "pub const MANAGER_SERVICE: &str = \"private-ai-manager\";\n\
         let exe = \"private-ai-manager.exe\";\n\
         let cmd = Command::new(\"private-ai-manager-cli\");\n",
    )
    .expect("fixture");
    let found = hits(&module);
    assert_eq!(found.len(), 2, "{found:?}");
    assert!(found[0].contains("private-ai-manager.exe"), "{found:?}");
    assert!(found[1].contains("private-ai-manager-cli"), "{found:?}");
}

// Every old build variable is refused, not only the dist root: any
// `LLAMA_FW_` name.
#[test]
fn the_scan_refuses_every_old_build_variable() {
    let dir = tempfile::tempdir().expect("tempdir");
    let file = dir.path().join("x.rs");
    let vars = [
        "ROCM",
        "CUDA",
        "VULKAN_SDK",
        "PATCHES",
        "GPU_LOCK",
        "LLAMA_CPP_DIR",
        "MAINTAINER",
        "DIST_ROOT",
    ];
    let text: String = vars
        .iter()
        .map(|v| format!("let v = env!(\"{}_{v}\");\n", concat!("LLAMA", "_FW")))
        .collect();
    std::fs::write(&file, text).expect("fixture");
    assert_eq!(hits(&file).len(), vars.len(), "{:?}", hits(&file));
}

// The migration module, and it alone, may name the old directory and the old
// login entry, which it must find; any other old name is still refused there.
#[test]
fn only_the_migration_module_may_name_the_old_tree_and_entry() {
    let dir = tempfile::tempdir().expect("tempdir");
    let src = dir.path().join("core").join("src");
    std::fs::create_dir_all(&src).expect("src");
    let text = format!(
        "pub const OLD_LOGIN_ENTRY: &str = \"{}\";\n\
         let old = base.join(\"llama.cpp\");\n\
         let v = env!(\"{}\");\n",
        concat!("llama-cpp", "-config"),
        concat!("LLAMA", "_FW_GPU_LOCK"),
    );
    for name in ["migrate.rs", "paths.rs"] {
        std::fs::write(src.join(name), &text).expect("fixture");
    }
    let migrate = hits(&src.join("migrate.rs"));
    assert_eq!(migrate.len(), 1, "{migrate:?}");
    assert!(migrate[0].contains("_FW_GPU_LOCK"), "{migrate:?}");
    assert_eq!(hits(&src.join("paths.rs")).len(), 3);
    // The exempt file is a real one, not a name left behind by a move.
    let real = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join(MIGRATION_MODULE);
    assert!(real.is_file(), "{real:?}");
}
