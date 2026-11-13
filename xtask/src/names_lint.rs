//! The project's names in what the build installs and runs: the configurator
//! executable `pai-config`, the install tree `/opt/private-ai`, the desktop
//! entry's display name "Private AI Framework", the registry key
//! `Software\private-ai` and the dist root variable `PRIVATE_AI_DIST_ROOT`,
//! with no fallback to the old names. A text scan, since most of it lives in
//! code that compiles on one OS only (`windows/`, `macos/`), in the MSI source
//! or in PowerShell. The package and file names of the packages are not
//! checked here, nor the engine's own names (`llama-server`, the llama.cpp
//! clone under `build/`).

use std::path::{Path, PathBuf};

fn repo() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("xtask sits in the repo root")
        .to_path_buf()
}

fn read(rel: &str) -> String {
    std::fs::read_to_string(repo().join(rel)).unwrap_or_else(|e| panic!("{rel}: {e}"))
}

fn files(dir: &Path, ext: &[&str], out: &mut Vec<PathBuf>) {
    for entry in std::fs::read_dir(dir).expect("read dir") {
        let path = entry.expect("dir entry").path();
        if path.is_dir() {
            files(&path, ext, out);
        } else if path
            .extension()
            .is_some_and(|e| ext.iter().any(|x| e == *x))
        {
            out.push(path);
        }
    }
}

/// The prefix of every old build variable (`_DIST_ROOT`, `_ROCM`, `_CUDA`,
/// `_VULKAN_SDK`, `_PATCHES`, `_GPU_LOCK`, `_LLAMA_CPP_DIR`, `_MAINTAINER`).
const OLD_DIST_ROOT: &str = concat!("LLAMA_FW", "_");

/// Lines of xtask's code (not whole-line comments) naming an old name.
#[test]
fn xtask_code_names_the_new_executable_tree_and_variable() {
    let old_names = [
        "llama-cpp-config",
        "opt/llama.cpp",
        "llama.cpp Config",
        OLD_DIST_ROOT,
    ];
    let mut sources = Vec::new();
    files(&repo().join("xtask").join("src"), &["rs"], &mut sources);
    let this = Path::new(file!()).file_name().expect("own name");
    let mut found = Vec::new();
    for path in sources.iter().filter(|p| p.file_name() != Some(this)) {
        let text = std::fs::read_to_string(path).expect("read source");
        for (n, line) in text.lines().enumerate() {
            if line.trim_start().starts_with("//") {
                continue;
            }
            if old_names.iter().any(|old| line.contains(old)) {
                found.push(format!("{}:{}: {}", path.display(), n + 1, line.trim()));
            }
        }
    }
    assert!(
        found.is_empty(),
        "old names in xtask:\n{}",
        found.join("\n")
    );
}

/// No fallback to an old build variable: not read, not even offered in a
/// comment, by the scripts and the dist sources.
#[test]
fn scripts_and_dist_sources_name_only_the_new_dist_root_variable() {
    let mut texts = Vec::new();
    for dir in ["scripts", "installer"] {
        files(
            &repo().join(dir),
            &["ps1", "sh", "psd1", "toml"],
            &mut texts,
        );
    }
    let found: Vec<String> = texts
        .iter()
        .filter(|p| {
            std::fs::read_to_string(p)
                .expect("read")
                .contains(OLD_DIST_ROOT)
        })
        .map(|p| p.display().to_string())
        .collect();
    assert!(
        found.is_empty(),
        "{OLD_DIST_ROOT} in:\n{}",
        found.join("\n")
    );
    assert!(
        read("scripts/dists.ps1").contains("$env:PRIVATE_AI_DIST_ROOT"),
        "scripts/dists.ps1 reads PRIVATE_AI_DIST_ROOT"
    );
}

/// The MSI installs `pai-config.exe` into `private-ai`, records it under
/// `HKLM\Software\private-ai`, and shows "Private AI Framework" as the product,
/// the Apps list entry and the Start menu shortcut. (The upgrade from the old
/// product, its UpgradeCode, Uninstall key and file name, is the packages'
/// change, not checked here.)
#[test]
fn the_msi_installs_and_registers_the_new_names() {
    let wxs = read("installer/llama-cpp-framework.wxs");
    for want in [
        r#"<Package Name="Private AI Framework "#,
        r#"<Directory Id="INSTALLFOLDER" Name="private-ai">"#,
        r#"<RegistryKey Root="HKLM" Key="Software\private-ai""#,
        r#"Root="HKCU" Key="Software\private-ai""#,
        r#"<RegistryValue Name="DisplayName" Type="string" Value="Private AI Framework "#,
        r#"<Shortcut Id="ConfigShortcut" Name="Private AI Framework""#,
        r#"Target="[BINFOLDER]pai-config.exe""#,
        r#"[LocalAppDataFolder]private-ai""#,
    ] {
        assert!(wxs.contains(want), "the .wxs has no {want}");
    }
    assert!(
        !wxs.contains("llama-cpp-config"),
        "the .wxs still names the old executable"
    );
}

/// The scripts that find the installed tree read the new registry key, and the
/// MSI's process stopper knows the new executable.
#[test]
fn the_windows_scripts_find_the_new_install() {
    assert!(
        read("scripts/00-install-prerequisites.ps1").contains(r"HKLM:\Software\private-ai"),
        "00-install-prerequisites.ps1 reads HKLM:\\Software\\private-ai"
    );
    let stopper = read("installer/msi-stop-processes.ps1");
    for want in [
        r"HKLM:\Software\private-ai",
        "'pai-config.exe'",
        "'pai-config'",
    ] {
        assert!(
            stopper.contains(want),
            "msi-stop-processes.ps1 has no {want}"
        );
    }
}
