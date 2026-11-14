//! The project's names in what the build installs and runs: the configurator
//! executable `pai-config`, the install tree (`/opt/private-ai` on macOS), the desktop
//! entry's display name "Private AI Framework", the registry key
//! `Software\private-ai` and the dist root variable `PRIVATE_AI_DIST_ROOT`,
//! with no fallback to the old names; the packages `private-ai-framework-*`
//! and the MSI, with the few places that name the old ones for the upgrade
//! from 2.4. A text scan, since most of it lives in code that compiles on one
//! OS only (`windows/`, `macos/`), in the MSI source or in PowerShell. The
//! engine's own names (`llama-server`, the llama.cpp clone under `build/`)
//! are not checked here.

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

/// The base name of the packages up to 2.4 (the monolith, then
/// `<base>-<component>`).
const OLD_PACKAGES: &str = concat!("llama-cpp", "-framework");

/// The one line of xtask that may spell the old package name: the constant
/// the `.deb` and `.rpm` relations of the upgrade from 2.4 are built from.
const OLD_PACKAGES_SOURCE: (&str, &str) = (
    "xtask/src/common/components.rs",
    concat!(
        r#"pub const OLD_BASE_NAME: &str = "llama-cpp"#,
        r#"-framework";"#
    ),
);

/// The MSI source, renamed with the packages.
const WXS: &str = "installer/private-ai-framework.wxs";

/// llama.cpp's own MSI: `Software\llama.cpp` and `Uninstall\llama.cpp` are
/// its own registration (the names 2.4 used, which it removes first).
const ENGINE_WXS: &str = "installer/llama.cpp.wxs";

/// The one installer file that may name the old executable: the process
/// stopper, which must stop a 2.4 configurator still running while the
/// upgrade replaces it.
const OLD_PRODUCT_STOPPER: &str = "installer/msi-stop-processes.ps1";

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
/// the Apps list entry and the Start menu shortcut.
#[test]
fn the_msi_installs_and_registers_the_new_names() {
    let wxs = read(WXS);
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

/// The MSI's process stopper reads the new registry key and knows the new
/// executable. (The prerequisites script no longer reads the installed tree:
/// `the_prerequisites_report_sets_no_installed_bin_aside` in the MSI tests.)
#[test]
fn the_windows_scripts_find_the_new_install() {
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

/// Every line of xtask, comments included (they name the package files), says
/// `private-ai-framework`: the old package name is spelled once, in the
/// constant the upgrade relations come from.
#[test]
fn xtask_names_the_old_packages_only_in_their_constant() {
    let mut sources = Vec::new();
    files(&repo().join("xtask").join("src"), &["rs"], &mut sources);
    let this = Path::new(file!()).file_name().expect("own name");
    let (exempt_file, exempt_line) = OLD_PACKAGES_SOURCE;
    let mut found = Vec::new();
    let mut exempt_seen = 0;
    for path in sources.iter().filter(|p| p.file_name() != Some(this)) {
        let text = std::fs::read_to_string(path).expect("read source");
        let rel = path
            .strip_prefix(repo())
            .expect("under the repo")
            .to_string_lossy()
            .replace('\\', "/");
        for (n, line) in text.lines().enumerate() {
            if !line.contains(OLD_PACKAGES) {
                continue;
            }
            if rel == exempt_file && line.trim() == exempt_line {
                exempt_seen += 1;
                continue;
            }
            found.push(format!("{rel}:{}: {}", n + 1, line.trim()));
        }
    }
    assert!(
        found.is_empty(),
        "old package name in xtask:\n{}",
        found.join("\n")
    );
    assert_eq!(
        exempt_seen, 1,
        "{exempt_file} defines the old package name once: {exempt_line}"
    );
    assert!(
        read("xtask/src/common/components.rs")
            .contains(r#"pub const BASE_NAME: &str = "private-ai-framework";"#),
        "components.rs names the packages private-ai-framework"
    );
}

/// The scripts and the installer sources never name the old packages, and
/// the old product's names (its registry keys, its executable) only where the
/// upgrade from 2.4 needs them: the MSI only searches the old keys, never
/// writes them, and only the process stopper names the old executable.
#[test]
fn scripts_and_installer_name_the_old_product_only_for_the_upgrade() {
    let mut texts = Vec::new();
    for dir in ["scripts", "installer"] {
        files(
            &repo().join(dir),
            &["ps1", "sh", "psd1", "toml", "wxs"],
            &mut texts,
        );
    }
    let old_exe = concat!("llama-cpp", "-config");
    let mut found = Vec::new();
    for path in &texts {
        let rel = path
            .strip_prefix(repo())
            .expect("under the repo")
            .to_string_lossy()
            .replace('\\', "/");
        let text = std::fs::read_to_string(path).expect("read");
        for (n, line) in text.lines().enumerate() {
            let at = format!("{rel}:{}: {}", n + 1, line.trim());
            if line.contains(OLD_PACKAGES) {
                found.push(at.clone());
            }
            if line.contains(old_exe) && rel != OLD_PRODUCT_STOPPER {
                found.push(at.clone());
            }
            // llama.cpp's own MSI registers under its own name, which was
            // 2.4's: there it is the engine's key, not the old product's.
            let old_key = line.contains(r#"\llama.cpp""#) && line.contains("Key=");
            if old_key && rel != ENGINE_WXS && !line.trim_start().starts_with("<RegistrySearch ") {
                found.push(at);
            }
        }
    }
    assert!(
        found.is_empty(),
        "old product names outside the upgrade's places:\n{}",
        found.join("\n")
    );
    assert!(
        !repo().join("installer/llama-cpp-framework.wxs").exists(),
        "the old .wxs is renamed to {WXS}"
    );
}

/// The start of every element `<tag ...>` in the .wxs, up to its first `>`
/// (attribute values escape theirs).
fn elements<'a>(wxs: &'a str, tag: &str) -> Vec<&'a str> {
    let open = format!("<{tag} ");
    let mut out = Vec::new();
    let mut rest = wxs;
    while let Some(i) = rest.find(&open) {
        let from = &rest[i..];
        let end = from.find('>').map_or(from.len(), |j| j + 1);
        out.push(&from[..end]);
        rest = &from[end..];
    }
    out
}

/// One attribute's value in an element.
fn attr<'a>(element: &'a str, name: &str) -> Option<&'a str> {
    let key = format!(" {name}=\"");
    let from = element.find(&key)? + key.len();
    let len = element[from..].find('"')?;
    Some(&element[from..from + len])
}

/// The `<Property Id=...>` holding the RegistrySearch of `key` / `name`.
fn property_searching(wxs: &str, key: &str, name: &str) -> Option<String> {
    let search = elements(wxs, "RegistrySearch")
        .into_iter()
        .find(|e| attr(e, "Key") == Some(key) && attr(e, "Name") == Some(name))?;
    let at = wxs.find(search)?;
    let property = elements(&wxs[..at], "Property").into_iter().last()?;
    attr(property, "Id").map(str::to_string)
}

/// The upgrade from 2.4 on Windows: the MSI keeps the UpgradeCode every
/// version shares (so MajorUpgrade removes the old product), and writes its
/// own Apps entry under the new name.
#[test]
fn the_msi_keeps_its_upgrade_code_and_writes_the_new_apps_entry() {
    let wxs = read(WXS);
    let package = elements(&wxs, "Package");
    assert_eq!(package.len(), 1, "one <Package>");
    assert_eq!(
        attr(package[0], "UpgradeCode"),
        Some("F21D044C-04F5-478D-8F0A-A9B940DB23C3"),
        "the UpgradeCode never changes"
    );
    assert_eq!(elements(&wxs, "MajorUpgrade").len(), 1, "MajorUpgrade kept");
    let written: Vec<&str> = elements(&wxs, "RegistryKey")
        .into_iter()
        .filter_map(|e| attr(e, "Key"))
        .collect();
    assert!(
        written.contains(&r"Software\Microsoft\Windows\CurrentVersion\Uninstall\private-ai"),
        "the Apps entry is Uninstall\\private-ai: {written:?}"
    );
    assert!(
        !written.iter().any(|k| k.ends_with(r"\llama.cpp")),
        "no old key written: {written:?}"
    );
}

/// The guard against an installation that is not an MSI looks at the Apps
/// entry of the old name (where those installs registered) and of the new
/// one: either, when present with an UninstallString that is not msiexec's,
/// stops the install.
#[test]
fn the_msi_guard_checks_the_old_and_the_new_apps_entry() {
    let wxs = read(WXS);
    // The guard is the launch condition on the Apps entries; the other one
    // requires llama.cpp's MSI.
    let guard = property_searching(
        &wxs,
        r"Software\Microsoft\Windows\CurrentVersion\Uninstall\private-ai",
        "UninstallString",
    )
    .expect("a Property searches the new Apps entry");
    let launch: Vec<&str> = elements(&wxs, "Launch")
        .into_iter()
        .filter(|l| attr(l, "Condition").is_some_and(|c| c.contains(&guard)))
        .collect();
    assert_eq!(launch.len(), 1, "one guard launch condition");
    let condition = attr(launch[0], "Condition").expect("Condition");
    for entry in ["llama.cpp", "private-ai"] {
        let key = format!(r"Software\Microsoft\Windows\CurrentVersion\Uninstall\{entry}");
        let property = property_searching(&wxs, &key, "UninstallString")
            .unwrap_or_else(|| panic!("no Property searches {key} UninstallString"));
        for want in [
            format!("NOT {property}"),
            format!("({property} ~&gt;&lt; &quot;msiexec&quot;)"),
        ] {
            assert!(
                condition.contains(&want),
                "the launch condition has no {want}: {condition}"
            );
        }
    }
    assert!(condition.starts_with("Installed OR "), "{condition}");
}

/// The install directory: the one recorded under `Software\private-ai`,
/// else the default `private-ai` under Program Files. No carry-over of a
/// 2.4 folder from `Software\llama.cpp`, which is llama.cpp's own key now
/// (`the_framework_reads_no_engine_key_for_its_folder` in common/msi.rs's
/// tests pins the rest).
#[test]
fn the_msi_install_dir_follows_the_new_key_only() {
    let wxs = read(WXS);
    assert_eq!(
        property_searching(&wxs, r"Software\private-ai", "InstallDir").as_deref(),
        Some("INSTALLFOLDER"),
        "INSTALLFOLDER comes from HKLM\\Software\\private-ai InstallDir"
    );
    assert_eq!(
        property_searching(&wxs, r"Software\llama.cpp", "InstallDir"),
        None,
        "the framework reads the engine's key"
    );
    assert!(
        wxs.contains(r#"<Directory Id="INSTALLFOLDER" Name="private-ai">"#),
        "the default directory is private-ai"
    );
}

/// The process stopper reads only the framework's key: `Software\llama.cpp`
/// is llama.cpp's own now, and a 2.4 install is removed by llama.cpp's MSI
/// (with 2.4's own stopper) before the framework's MSI runs. The 2.4
/// executable's name may stay in its list (`OLD_PRODUCT_STOPPER`).
#[test]
fn the_process_stopper_no_longer_reads_the_old_key() {
    let stopper = read(OLD_PRODUCT_STOPPER);
    assert!(
        stopper.contains("private-ai-framework.wxs"),
        "msi-stop-processes.ps1 names its .wxs"
    );
    assert!(
        !stopper.contains(r"HKLM:\Software\llama.cpp"),
        "msi-stop-processes.ps1 still reads the engine's key"
    );
    assert!(
        !stopper.contains("settles"),
        "msi-stop-processes.ps1 still defers the old names to a later change"
    );
}

/// What only a Windows or macOS build runs, pinned as text: the MSI's file
/// name and source, the Windows install's package prefix and Apps entry, the
/// macOS package identifier and MoltenVK manifest.
#[test]
fn the_windows_and_macos_phases_name_the_new_packages() {
    let package = read("xtask/src/windows/package.rs");
    for want in ["msi::file_name(", "msi::wxs("] {
        assert!(package.contains(want), "windows/package.rs has no {want}");
    }
    let msi = read("xtask/src/common/msi.rs");
    for want in [
        r#""private-ai-framework-v{framework_version}-{arch}.msi""#,
        r#""installer/private-ai-framework.wxs""#,
        r#"CurrentVersion\Uninstall\private-ai"#,
    ] {
        assert!(msi.contains(want), "common/msi.rs has no {want}");
    }
    let install = read("xtask/src/windows/install.rs");
    assert!(
        install.contains("msi::uninstall_key("),
        "windows/install.rs reads no product's Apps entry through msi::uninstall_key"
    );
    let macos = read("xtask/src/macos/package.rs");
    for want in [
        r#"pub const PKG_ID: &str = "org.privateai.framework";"#,
        r#"const ICD_NAME: &str = "private-ai-framework-MoltenVK_icd.json";"#,
    ] {
        assert!(macos.contains(want), "macos/package.rs has no {want}");
    }
}

/// The helper scripts under `tools/` (comments included: a header says
/// where a script reads from) name neither the old product nor its old user
/// tree. The engine's own names (`llama-server`, llama.cpp's log lines) are
/// not old names and stay.
#[test]
fn tools_scripts_name_the_new_product_and_user_tree() {
    let mut texts = Vec::new();
    files(&repo().join("tools"), &["ps1", "sh"], &mut texts);
    assert!(
        texts.iter().any(|p| p.ends_with("report-memory.ps1")),
        "the scan reaches tools/report-memory.ps1"
    );
    let old_names = [
        OLD_PACKAGES,
        OLD_DIST_ROOT,
        concat!("llama.cpp", "-framework"),
        concat!("llama-cpp", "-config"),
        concat!("llama.cpp", " Config"),
        concat!("opt/", "llama.cpp"),
        concat!(".local/share/", "llama.cpp"),
        concat!(".config/", "llama.cpp"),
    ];
    let old_tree = concat!("llama", r".cpp\");
    let mut found = Vec::new();
    for path in &texts {
        let rel = path
            .strip_prefix(repo())
            .expect("under the repo")
            .to_string_lossy()
            .replace('\\', "/");
        let text = std::fs::read_to_string(path).expect("read");
        for (n, line) in text.lines().enumerate() {
            let windows_tree = line.contains("LOCALAPPDATA") && line.contains(old_tree);
            if windows_tree || old_names.iter().any(|old| line.contains(old)) {
                found.push(format!("{rel}:{}: {}", n + 1, line.trim()));
            }
        }
    }
    assert!(
        found.is_empty(),
        "old product names or user tree in tools/:\n{}",
        found.join("\n")
    );
    let report = read("tools/report-memory.ps1");
    assert!(
        report.contains(r"Join-Path $env:LOCALAPPDATA 'private-ai\logs\llama-server.log'"),
        "report-memory.ps1 defaults -LogPath to %LOCALAPPDATA%\\private-ai\\logs\\llama-server.log"
    );
}

/// The bench.db schema's comments name the configurator by its new name.
/// Only comments outside the `CREATE` statements change: SQLite stores each
/// statement's text from `CREATE` on, so the committed bench.db's schema
/// (`sqlite_master`) does not hold these lines.
#[test]
fn the_bench_schema_names_the_new_configurator() {
    let schema = read("benchmarks/schema.sql");
    let old_exe = concat!("llama-cpp", "-config");
    let found: Vec<String> = schema
        .lines()
        .enumerate()
        .filter(|(_, l)| l.contains(old_exe) || l.contains(OLD_PACKAGES))
        .map(|(n, l)| format!("benchmarks/schema.sql:{}: {}", n + 1, l.trim()))
        .collect();
    assert!(
        found.is_empty(),
        "old names in the schema:\n{}",
        found.join("\n")
    );
    assert!(
        schema.contains("pai-config"),
        "schema.sql names pai-config where it names the configurator"
    );
}
