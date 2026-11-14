//! Two installers on Windows: llama.cpp's own MSI (`installer/llama.cpp.wxs`:
//! the engine, its GPU runtimes and its defaults, the instances' machine
//! tree under `%ProgramData%\llama.cpp`) and the framework's MSI, which holds
//! no llama.cpp file and refuses to install without the engine at the release
//! it was built against. Read as XML and text, like the tests above: nothing
//! here builds an MSI.

use super::*;
use crate::common::msi::{
    engine_defaults, file_name, install_plan, msi_product, product_version, products, staging_dir,
    uninstall_key, wxs as wxs_of,
};
use crate::Product;

const ENGINE_WXS: &str = "installer/llama.cpp.wxs";
const REMOVE_INSTANCES_PS1: &str = "installer/msi-remove-instances.ps1";
/// The framework's identity across versions, 2.4 included (installer/AGENTS.md).
const FRAMEWORK_UPGRADE_CODE: &str = "F21D044C-04F5-478D-8F0A-A9B940DB23C3";
const DEFAULTS: [&str; 2] = ["server@default.ini", "presets.ini"];

fn engine_text() -> String {
    read(ENGINE_WXS)
}

fn package<'a, 'i>(doc: &'a Document<'i>) -> Node<'a, 'i> {
    let p = elements(doc, "Package");
    assert_eq!(p.len(), 1, "one Package");
    p[0]
}

/// The one element `tag` with `Id` = `id`.
fn by_id<'a, 'i>(doc: &'a Document<'i>, tag: &str, id: &str) -> Node<'a, 'i> {
    let found: Vec<_> = elements(doc, tag)
        .into_iter()
        .filter(|n| n.attribute("Id") == Some(id))
        .collect();
    assert_eq!(found.len(), 1, "one {tag} Id=\"{id}\"");
    found[0]
}

/// The id of the Directory reached from StandardDirectory `root` through the
/// directory names `names`.
fn dir_id(doc: &Document, root: &str, names: &[&str]) -> String {
    let mut node = elements(doc, "StandardDirectory")
        .into_iter()
        .find(|d| d.attribute("Id") == Some(root))
        .unwrap_or_else(|| panic!("StandardDirectory {root}"));
    for name in names {
        node = children(node, "Directory")
            .into_iter()
            .find(|d| d.attribute("Name") == Some(*name))
            .unwrap_or_else(|| panic!("Directory {name} under {root} {names:?}"));
    }
    node.attribute("Id").expect("Directory Id").to_string()
}

/// A GUID in the uppercase registry form WiX writes.
fn is_guid(s: &str) -> bool {
    let parts: Vec<&str> = s.split('-').collect();
    parts.iter().map(|p| p.len()).collect::<Vec<_>>() == [8, 4, 4, 4, 12]
        && parts.iter().all(|p| {
            p.chars()
                .all(|c| c.is_ascii_digit() || ('A'..='F').contains(&c))
        })
}

/// The engine MSI's UpgradeCode.
fn engine_upgrade_code() -> String {
    let text = engine_text();
    let doc = Document::parse(&text).expect("llama.cpp.wxs is XML");
    package(&doc)
        .attribute("UpgradeCode")
        .expect("UpgradeCode")
        .to_string()
}

/// The `Custom` scheduling action `id` in InstallExecuteSequence.
fn custom<'a, 'i>(doc: &'a Document<'i>, id: &str) -> Node<'a, 'i> {
    let c = elements(doc, "Custom")
        .into_iter()
        .find(|c| c.attribute("Action") == Some(id))
        .unwrap_or_else(|| panic!("Custom Action=\"{id}\" in a sequence"));
    assert_eq!(
        c.parent_element().map(|p| p.tag_name().name()),
        Some("InstallExecuteSequence"),
        "{id}"
    );
    c
}

/// The WixQuietExec custom action whose command line (its SetProperty, by
/// the action's id) contains `needle`: (SetProperty value, CustomAction).
fn quiet_exec<'a, 'i>(doc: &'a Document<'i>, needle: &str) -> (String, Node<'a, 'i>) {
    let set: Vec<_> = elements(doc, "SetProperty")
        .into_iter()
        .filter(|s| s.attribute("Value").is_some_and(|v| v.contains(needle)))
        .collect();
    assert_eq!(
        set.len(),
        1,
        "one SetProperty whose command contains {needle:?}"
    );
    let set = set[0];
    let id = set.attribute("Id").expect("SetProperty Id");
    assert_eq!(set.attribute("Sequence"), Some("execute"), "{id}");
    assert_eq!(set.attribute("Before"), Some(id), "{id}");
    let ca = by_id(doc, "CustomAction", id);
    assert_eq!(ca.attribute("BinaryRef"), Some("Wix4UtilCA_X64"), "{id}");
    assert_eq!(ca.attribute("DllEntry"), Some("WixQuietExec"), "{id}");
    (set.attribute("Value").unwrap().to_string(), ca)
}

// ── xtask: names, versions, staging ──────────────────────────────────────

/// With no product, `xtask package` builds both MSIs on Windows; with one,
/// that one alone.
#[test]
fn package_builds_both_msis_by_default() {
    let mut both = products(None);
    both.sort_by_key(|p| *p as u8);
    assert_eq!(both, [Product::Llama, Product::Framework]);
    assert_eq!(products(Some(Product::Llama)), [Product::Llama]);
    assert_eq!(products(Some(Product::Framework)), [Product::Framework]);
}

/// llama.cpp's MSI is named and versioned by the engine's release alone, no
/// `+pai<N>`; the framework's by the framework's version alone.
#[test]
fn each_msi_is_named_by_its_own_version_only() {
    assert_eq!(
        file_name(Product::Llama, "2.99.1", "v1.0.0", "x64"),
        "llama.cpp-v1.0.0-x64.msi"
    );
    assert_eq!(
        file_name(Product::Framework, "2.99.1", "v1.0.0", "x64"),
        "private-ai-framework-v2.99.1-x64.msi"
    );
    // The other product's version never leaks into a name.
    assert!(!file_name(Product::Framework, "2.99.1", "v1.0.0", "x64").contains("1.0.0"));
    assert!(!file_name(Product::Llama, "2.99.1", "v1.0.0", "x64").contains("2.99.1"));
    assert!(!file_name(Product::Llama, "2.99.1", "v1.0.0", "x64").contains("pai"));
}

/// The engine's ProductVersion is its release without the `v`.
#[test]
fn the_engine_product_version_is_its_release() {
    assert_eq!(product_version("v1.0.0").unwrap(), "1.0.0");
    assert_eq!(product_version("v0.9.12").unwrap(), "0.9.12");
}

/// What is not a release cannot be an engine MSI: a `bNNNN` build, a
/// hand-moved HEAD, or a version Windows Installer cannot hold
/// (major and minor at most 255, build at most 65535: ProductVersion).
#[test]
fn the_engine_product_version_refuses_a_non_release() {
    for bad in [
        "",
        "b1234",
        "b0-abc1234",
        "v1.0",
        "v1.0.0-rc1",
        "1.0.0+pai1",
        "vx.y.z",
        "v256.0.0",
        "v1.256.0",
        "v1.0.65536",
    ] {
        assert!(product_version(bad).is_err(), "{bad:?} accepted");
    }
    assert_eq!(product_version("v255.255.65535").unwrap(), "255.255.65535");
}

/// Each product is staged on its own, so either MSI can be built alone and
/// neither harvests the other's files.
#[test]
fn each_product_has_its_own_staging_tree() {
    let root = Path::new("/b/build/windows");
    let llama = staging_dir(root, Product::Llama);
    let framework = staging_dir(root, Product::Framework);
    assert_ne!(llama, framework);
    for d in [&llama, &framework] {
        assert!(d.starts_with(root), "{}", d.display());
        assert_ne!(d.as_path(), root);
    }
    assert!(!llama.starts_with(&framework) && !framework.starts_with(&llama));
}

/// Each product has its own `.wxs`, and both exist and parse.
#[test]
fn each_product_has_its_own_wxs() {
    assert_eq!(wxs_of(Product::Llama), ENGINE_WXS);
    assert_eq!(wxs_of(Product::Framework), WXS);
    for p in [Product::Llama, Product::Framework] {
        let text = read(wxs_of(p));
        Document::parse(&text).unwrap_or_else(|e| panic!("{}: {e}", wxs_of(p)));
    }
}

/// The engine's defaults on Windows: `server@default.ini` with the starting
/// values that hold on any machine (port, host) and an empty `presets.ini`.
/// No Linux path, and no ModelsDir or StateDir: an INI value is not
/// expanded, so `%ProgramData%` could not be written portably; the manager
/// resolves them (assumed: a manager slice of its own).
#[test]
fn the_engine_defaults_on_windows_hold_only_portable_starting_values() {
    let d = engine_defaults();
    let names: Vec<&str> = d.iter().map(|(n, _)| *n).collect();
    assert_eq!(names, DEFAULTS);
    let server = &d[0].1;
    assert!(server.is_ascii(), "{server}");
    let keys: Vec<&str> = server
        .lines()
        .map(str::trim)
        .filter(|l| !l.is_empty() && !l.starts_with(';') && !l.starts_with('#'))
        .collect();
    assert_eq!(
        keys,
        ["[Server]", "Port = 9931", "Hostname = localhost"],
        "{server}"
    );
    assert!(!server.contains("/var/lib"), "{server}");
    assert_eq!(d[1].1, "", "presets.ini is empty");
}

/// The Windows package phase uses the functions above: one MSI per product,
/// each from its own staging tree and `.wxs`, named by its own version.
#[test]
fn the_windows_package_phase_builds_each_product_apart() {
    let package = read("xtask/src/windows/package.rs");
    for call in [
        "msi::products(",
        "msi::file_name(",
        "msi::staging_dir(",
        "msi::wxs(",
        "msi::product_version(",
        "msi::engine_defaults(",
        "RemoveInstancesCmd",
        "LlamaCppMin",
    ] {
        assert!(
            package.contains(call),
            "windows/package.rs does not use {call}"
        );
    }
    assert!(
        !package.contains("private-ai-framework-v{framework_version}-{llama_build}"),
        "the framework's MSI still names the llama.cpp release"
    );
}

/// `xtask package llama|framework` is accepted on Windows and handed to the
/// package phase.
#[test]
fn windows_accepts_a_product_on_package() {
    let main = read("xtask/src/main.rs");
    assert!(
        !main.contains("on Windows the MSI holds both products"),
        "main.rs still refuses a product on Windows"
    );
    assert!(
        main.contains("windows::package::run(product"),
        "main.rs does not pass the product to windows::package::run"
    );
}

// ── llama.cpp's MSI: identity and upgrades ───────────────────────────────

/// Its own UpgradeCode (never the framework's), per machine, the
/// ProductVersion xtask passes (`Version`, the release).
#[test]
fn the_engine_msi_has_its_own_identity() {
    let text = engine_text();
    let doc = Document::parse(&text).expect("llama.cpp.wxs is XML");
    let p = package(&doc);
    let code = p.attribute("UpgradeCode").unwrap_or_default();
    assert!(is_guid(code), "UpgradeCode {code:?}");
    assert_ne!(code, FRAMEWORK_UPGRADE_CODE, "the framework's UpgradeCode");
    assert_eq!(p.attribute("Version"), Some("$(var.Version)"));
    assert_eq!(p.attribute("Scope"), Some("perMachine"));
    assert_eq!(p.attribute("ProductCode"), None, "generated per build");
    let name = p.attribute("Name").unwrap_or_default();
    assert!(name.starts_with("llama.cpp"), "Name {name:?}");
    assert!(!name.contains("pai"), "Name {name:?}");
}

/// A newer release upgrades it in place; a repackage of the same release (a
/// new `+pai<N>`) too; an older one is refused with a message.
#[test]
fn the_engine_msi_upgrades_and_refuses_downgrades() {
    let text = engine_text();
    let doc = Document::parse(&text).expect("llama.cpp.wxs is XML");
    let m = elements(&doc, "MajorUpgrade");
    assert_eq!(m.len(), 1, "one MajorUpgrade");
    assert_eq!(m[0].attribute("AllowSameVersionUpgrades"), Some("yes"));
    assert_ne!(m[0].attribute("AllowDowngrades"), Some("yes"));
    let msg = m[0].attribute("DowngradeErrorMessage").unwrap_or_default();
    assert!(msg.contains("llama.cpp"), "DowngradeErrorMessage {msg:?}");
    assert!(msg.is_ascii() && !msg.contains('\u{2014}'), "{msg:?}");
}

/// The engine installs without the framework: no launch condition names it.
#[test]
fn the_engine_msi_does_not_require_the_framework() {
    let text = engine_text();
    let doc = Document::parse(&text).expect("llama.cpp.wxs is XML");
    for l in elements(&doc, "Launch") {
        let c = l.attribute("Condition").unwrap_or_default();
        assert!(!c.to_lowercase().contains("private"), "Launch {c:?}");
    }
}

/// Release 2.4 and earlier (named `llama.cpp-framework`, the framework's
/// UpgradeCode) installed into `Program Files\llama.cpp` and registered
/// `HKLM\Software\llama.cpp`, which are now the engine's: llama.cpp's MSI
/// removes such an install before it takes them (an Upgrade row that is not
/// OnlyDetect, so RemoveExistingProducts removes what FindRelatedProducts
/// found), and only below 2.99.0: a 2.99+ framework is never touched.
/// Upgrade table: https://learn.microsoft.com/en-us/windows/win32/msi/upgrade-table
#[test]
fn the_engine_msi_removes_a_release_2_4_install_only() {
    let text = engine_text();
    let doc = Document::parse(&text).expect("llama.cpp.wxs is XML");
    let ups: Vec<_> = elements(&doc, "Upgrade")
        .into_iter()
        .filter(|u| u.attribute("Id") == Some(FRAMEWORK_UPGRADE_CODE))
        .collect();
    assert_eq!(ups.len(), 1, "one Upgrade Id=\"{FRAMEWORK_UPGRADE_CODE}\"");
    let versions = children(ups[0], "UpgradeVersion");
    assert_eq!(versions.len(), 1, "one UpgradeVersion");
    let v = versions[0];
    assert_eq!(v.attribute("Maximum"), Some("2.99.0"));
    assert_eq!(v.attribute("IncludeMaximum"), Some("no"), "2.99.0 is kept");
    assert_ne!(
        v.attribute("OnlyDetect"),
        Some("yes"),
        "2.4 must be removed"
    );
    assert!(v.attribute("Property").is_some(), "an ActionProperty");
    // Removed in full, not some features of it.
    assert_eq!(v.attribute("RemoveFeatures"), None);
}

// ── llama.cpp's MSI: layout and registration ─────────────────────────────

/// `%ProgramFiles%\llama.cpp\` with `bin\` (llama.cpp's executables and DLLs,
/// the ggml backends, the GPU runtimes, harvested from the engine's staging
/// tree), `share\` (licences, the read-only defaults) and, for ROCm, `lib\`
/// and `.kpack\` beside `bin\`.
#[test]
fn the_engine_installs_into_program_files_llama_cpp() {
    let text = engine_text();
    let doc = Document::parse(&text).expect("llama.cpp.wxs is XML");
    let root = dir_id(&doc, "ProgramFiles64Folder", &["llama.cpp"]);
    assert_eq!(root, "INSTALLFOLDER");
    let harvested: Vec<(String, String)> = elements(&doc, "Files")
        .into_iter()
        .map(|f| {
            (
                f.attribute("Directory").unwrap_or_default().to_string(),
                f.attribute("Include").unwrap_or_default().to_string(),
            )
        })
        .collect();
    for (sub, include) in [
        ("bin", r"$(var.StagingDir)\bin\**"),
        ("share", r"$(var.StagingDir)\share\**"),
        ("lib", r"$(var.StagingDir)\lib\**"),
        (".kpack", r"$(var.StagingDir)\.kpack\**"),
    ] {
        let id = dir_id(&doc, "ProgramFiles64Folder", &["llama.cpp", sub]);
        assert!(
            harvested.contains(&(id.clone(), include.to_string())),
            "{sub}: no Files Directory=\"{id}\" Include=\"{include}\" in {harvested:?}"
        );
    }
}

/// Nothing of the framework is in llama.cpp's MSI: no pai executable and no
/// service (the instances' services are the manager's, created at run time).
#[test]
fn the_engine_msi_holds_nothing_of_the_framework() {
    let text = engine_text();
    let doc = Document::parse(&text).expect("llama.cpp.wxs is XML");
    for exe in [
        "pai.exe",
        "pai-config.exe",
        "pai-manager.exe",
        "pai-bff.exe",
    ] {
        assert!(!text.contains(exe), "llama.cpp.wxs names {exe}");
    }
    assert!(
        elements(&doc, "ServiceInstall").is_empty(),
        "a ServiceInstall"
    );
    assert!(elements(&doc, "Environment").is_empty(), "a PATH entry");
}

/// `HKLM\Software\llama.cpp`: `InstallDir` (where pai-manager finds
/// llama-server) and `Version` (the release).
#[test]
fn the_engine_registers_its_folder_and_version() {
    let text = engine_text();
    let doc = Document::parse(&text).expect("llama.cpp.wxs is XML");
    let key = elements(&doc, "RegistryKey")
        .into_iter()
        .find(|k| {
            k.attribute("Root") == Some("HKLM") && k.attribute("Key") == Some(r"Software\llama.cpp")
        })
        .expect(r"RegistryKey HKLM Software\llama.cpp");
    let value = |name: &str| {
        children(key, "RegistryValue")
            .into_iter()
            .find(|v| v.attribute("Name") == Some(name))
            .unwrap_or_else(|| panic!("RegistryValue {name}"))
    };
    assert_eq!(
        value("InstallDir").attribute("Value"),
        Some("[INSTALLFOLDER]")
    );
    assert_eq!(value("InstallDir").attribute("Type"), Some("string"));
    assert_eq!(value("Version").attribute("Value"), Some("$(var.Version)"));
    assert_eq!(value("Version").attribute("Type"), Some("string"));
    let component = component_of(key).attribute("Id").expect("component Id");
    assert!(in_main_feature(&doc, component), "{component} not in Main");
}

/// Its own Apps entry, as the framework's: the MSI's own is hidden, ours
/// opens the full UI on the removal page (where the configuration question
/// can show), with a quiet uninstall for scripts.
#[test]
fn the_engine_has_its_own_apps_entry_opening_the_full_ui() {
    let text = engine_text();
    let doc = Document::parse(&text).expect("llama.cpp.wxs is XML");
    let arp = elements(&doc, "Property")
        .into_iter()
        .find(|p| p.attribute("Id") == Some("ARPSYSTEMCOMPONENT"))
        .expect("ARPSYSTEMCOMPONENT");
    assert_eq!(arp.attribute("Value"), Some("1"));
    let key = elements(&doc, "RegistryKey")
        .into_iter()
        .find(|k| {
            k.attribute("Key")
                == Some(r"Software\Microsoft\Windows\CurrentVersion\Uninstall\llama.cpp")
        })
        .expect(r"the Apps entry Uninstall\llama.cpp");
    let value = |name: &str| {
        children(key, "RegistryValue")
            .into_iter()
            .find(|v| v.attribute("Name") == Some(name))
            .and_then(|v| v.attribute("Value"))
            .unwrap_or_else(|| panic!("RegistryValue {name}"))
            .to_string()
    };
    assert_eq!(value("DisplayVersion"), "$(var.Version)");
    let uninstall = value("UninstallString");
    assert!(
        uninstall.starts_with("MsiExec.exe /I[ProductCode] "),
        "{uninstall:?}"
    );
    assert!(uninstall.ends_with("=1"), "{uninstall:?}");
    assert_eq!(
        value("QuietUninstallString"),
        "MsiExec.exe /X[ProductCode] /qn"
    );
}

// ── llama.cpp's MSI: %ProgramData%\llama.cpp ─────────────────────────────

fn engine_data_dirs(doc: &Document) -> BTreeMap<String, String> {
    data_dirs_in(doc, "llama.cpp", &["config", "logs", "models"])
}

/// The root: protected, SYSTEM and Administrators full, users read.
#[test]
fn the_engine_data_root_is_written_by_system_and_administrators_only() {
    let text = engine_text();
    let doc = Document::parse(&text).expect("llama.cpp.wxs is XML");
    let ids = engine_data_dirs(&doc);
    let d = folder_dacl(&doc, &ids[""]);
    assert!(d.protected);
    assert_eq!(d.granted(SYSTEM) & FULL, FULL);
    assert_eq!(d.granted(ADMINISTRATORS) & FULL, FULL);
    assert_eq!(d.granted(USERS) & READ, READ);
    assert_eq!(
        d.writers(),
        vec![SYSTEM.to_string(), ADMINISTRATORS.to_string()]
    );
}

/// config\: the manager (SYSTEM) and Administrators write, the instances'
/// virtual accounts read; users have no ACE (they go through the manager).
#[test]
fn the_engine_config_folder_is_read_by_the_services_and_no_user() {
    let text = engine_text();
    let doc = Document::parse(&text).expect("llama.cpp.wxs is XML");
    let ids = engine_data_dirs(&doc);
    let d = folder_dacl(&doc, &ids["config"]);
    assert!(d.protected);
    assert_eq!(d.granted(SYSTEM) & FULL, FULL);
    assert_eq!(d.granted(ADMINISTRATORS) & FULL, FULL);
    assert_eq!(d.granted(ALL_SERVICES) & READ, READ);
    assert_eq!(
        d.writers(),
        vec![SYSTEM.to_string(), ADMINISTRATORS.to_string()]
    );
    for user in [USERS, "S-1-5-11", "S-1-1-0", "S-1-5-4"] {
        assert!(!d.names(user), "config\\ names {user}");
    }
}

/// logs\ and models\: the instances' services modify (the log and its
/// `.previous.log`; models downloaded by name), users read.
#[test]
fn the_engine_logs_and_models_are_written_by_the_services_and_read_by_users() {
    let text = engine_text();
    let doc = Document::parse(&text).expect("llama.cpp.wxs is XML");
    let ids = engine_data_dirs(&doc);
    for sub in ["logs", "models"] {
        let d = folder_dacl(&doc, &ids[sub]);
        assert!(d.protected, "{sub}");
        assert_eq!(d.granted(SYSTEM) & FULL, FULL, "{sub}");
        assert_eq!(d.granted(ADMINISTRATORS) & FULL, FULL, "{sub}");
        assert_eq!(d.granted(ALL_SERVICES) & MODIFY, MODIFY, "{sub}");
        assert_eq!(d.granted(USERS) & READ, READ, "{sub}");
        let mut writers = vec![SYSTEM, ADMINISTRATORS, ALL_SERVICES];
        writers.sort();
        assert_eq!(d.writers(), writers, "{sub}");
    }
}

/// The defaults are copied into config\ only when missing, natively: each
/// a File in a component of its own in config\, `NeverOverwrite` (not
/// installed when its key file exists: what the user or the manager wrote
/// stays, on repair and upgrade too) and `Permanent` (an uninstall or the
/// old half of an upgrade leaves it), from the read-only copy the package
/// installs in `share\defaults\`. Writable: the manager rewrites it.
/// Component table (https://learn.microsoft.com/en-us/windows/win32/msi/component-table):
/// NeverOverwrite "does not install or reinstall the component if a key path
/// file ... already exists"; Permanent "does not remove the component during
/// an uninstall".
#[test]
fn the_engine_defaults_are_copied_into_config_only_when_missing() {
    let text = engine_text();
    let doc = Document::parse(&text).expect("llama.cpp.wxs is XML");
    let config = engine_data_dirs(&doc)["config"].clone();
    for name in DEFAULTS {
        let files: Vec<_> = elements(&doc, "File")
            .into_iter()
            .filter(|f| {
                f.attribute("Name")
                    .or_else(|| f.attribute("Source").and_then(|s| s.rsplit('\\').next()))
                    == Some(name)
            })
            .collect();
        assert_eq!(files.len(), 1, "one File {name}");
        let f = files[0];
        assert_eq!(
            f.attribute("Source"),
            Some(format!(r"$(var.StagingDir)\share\defaults\{name}").as_str()),
            "{name}"
        );
        assert_ne!(f.attribute("ReadOnly"), Some("yes"), "{name}");
        let c = component_of(f);
        assert_eq!(
            f.attribute("Directory")
                .or_else(|| c.attribute("Directory")),
            Some(config.as_str()),
            "{name} is not installed into %ProgramData%\\llama.cpp\\config"
        );
        assert_eq!(c.attribute("NeverOverwrite"), Some("yes"), "{name}");
        assert_eq!(c.attribute("Permanent"), Some("yes"), "{name}");
        assert_eq!(
            children(c, "File").len(),
            1,
            "{name}: a component of its own (its key path)"
        );
        let id = c.attribute("Id").expect("component Id");
        assert!(in_main_feature(&doc, id), "{id} not in Main");
    }
}

// ── llama.cpp's MSI: removal ─────────────────────────────────────────────

/// On an uninstall that is not the old half of an upgrade, the instance
/// services `llama.cpp_<id>` are stopped and deleted (deferred, as
/// LocalSystem, before the files go; never rolling the uninstall back), by
/// a PowerShell script embedded as `-EncodedCommand` (the engine does not
/// require the framework, so `pai-manager.exe` may not be there).
#[test]
fn the_engine_uninstall_stops_and_deletes_the_instance_services() {
    let text = engine_text();
    let doc = Document::parse(&text).expect("llama.cpp.wxs is XML");
    let holder = elements(&doc, "Property")
        .into_iter()
        .find(|p| p.attribute("Value") == Some("$(var.RemoveInstancesCmd)"))
        .expect("a Property holding $(var.RemoveInstancesCmd)");
    let prop = holder.attribute("Id").unwrap();
    let (cmd, ca) = quiet_exec(&doc, &format!("-EncodedCommand [{prop}]"));
    assert!(cmd.contains("powershell.exe"), "{cmd}");
    assert_eq!(ca.attribute("Execute"), Some("deferred"));
    assert_eq!(ca.attribute("Impersonate"), Some("no"));
    assert_eq!(ca.attribute("Return"), Some("ignore"));
    let c = custom(&doc, ca.attribute("Id").unwrap());
    let anchored = matches!(
        (c.attribute("After"), c.attribute("Before")),
        (
            Some("StopServices" | "DeleteServices" | "InstallInitialize"),
            None
        ) | (None, Some("DeleteServices" | "RemoveFiles"))
    );
    assert!(
        anchored,
        "before RemoveFiles: {:?} {:?}",
        c.attribute("After"),
        c.attribute("Before")
    );
    let cond = c.attribute("Condition").unwrap_or_default();
    assert!(cond.contains(r#"REMOVE~="ALL""#), "{cond:?}");
    assert!(cond.contains("NOT UPGRADINGPRODUCTCODE"), "{cond:?}");
}

/// The script: Windows PowerShell 5.1-safe text (ASCII), the instance
/// services by their prefix, stopped then deleted, never the framework's own
/// services, and exit 0 whatever happens.
#[test]
fn the_remove_instances_script_deletes_only_llama_cpp_services() {
    let s = read(REMOVE_INSTANCES_PS1);
    assert!(s.is_ascii(), "not ASCII");
    let code: Vec<&str> = s
        .lines()
        .map(str::trim)
        .filter(|l| !l.is_empty() && !l.starts_with('#'))
        .collect();
    let body = code.join("\n");
    assert!(body.contains("'llama.cpp_*'"), "{body}");
    let stop = body.find("Stop-Service").expect("Stop-Service");
    let delete = body.find("sc.exe").expect("sc.exe delete");
    assert!(body[delete..].contains("delete"), "{body}");
    assert!(stop < delete, "stopped before deleted: {body}");
    for other in ["private-ai-manager", "private-ai-bff"] {
        assert!(!body.contains(other), "names {other}");
    }
    assert!(body.contains("try") && body.contains("catch"), "{body}");
    assert_eq!(code.last(), Some(&"exit 0"), "{body}");
}

/// The removal page asks whether to remove the configuration too: a
/// checkbox, cleared by default (its property has no value), reached from
/// Remove in the maintenance wizard and shown first from the Apps entry.
#[test]
fn the_engine_removal_asks_about_the_configuration_cleared_by_default() {
    let text = engine_text();
    let doc = Document::parse(&text).expect("llama.cpp.wxs is XML");
    let checks: Vec<_> = elements(&doc, "Control")
        .into_iter()
        .filter(|c| c.attribute("Type") == Some("CheckBox"))
        .collect();
    assert_eq!(checks.len(), 1, "one CheckBox");
    let check = checks[0];
    assert_eq!(check.attribute("CheckBoxValue"), Some("1"));
    let prop = check.attribute("Property").expect("CheckBox Property");
    let p = by_id(&doc, "Property", prop);
    assert_eq!(p.attribute("Value"), None, "{prop} starts cleared");
    assert_eq!(
        p.attribute("Secure"),
        Some("yes"),
        "{prop} reaches the server side"
    );
    let dialog = check
        .ancestors()
        .find(|n| n.is_element() && n.tag_name().name() == "Dialog")
        .and_then(|d| d.attribute("Id"))
        .expect("the checkbox's Dialog");
    assert!(
        elements(&doc, "Publish").into_iter().any(|p| {
            p.attribute("Dialog") == Some("MaintenanceTypeDlg")
                && p.attribute("Control") == Some("RemoveButton")
                && p.attribute("Value") == Some(dialog)
        }),
        "Remove does not lead to {dialog}"
    );
    assert!(
        elements(&doc, "Show")
            .into_iter()
            .any(|s| s.attribute("Dialog") == Some(dialog)),
        "{dialog} is never shown first (the Apps entry's Uninstall)"
    );
    let label = check.attribute("Text").unwrap_or_default().to_lowercase();
    assert!(label.contains("configuration"), "{label:?}");
}

/// Ticked, in a full-UI removal (UILevel 5) that is not the old half of an
/// upgrade, `%ProgramData%\llama.cpp` goes, after the files (the services
/// stopped, nothing holds a log). Not impersonated: users have no access to
/// config\. A silent or basic-UI uninstall keeps it whatever the command
/// line says.
#[test]
fn the_engine_configuration_goes_only_when_ticked_in_a_full_ui_uninstall() {
    let text = engine_text();
    let doc = Document::parse(&text).expect("llama.cpp.wxs is XML");
    let root = engine_data_dirs(&doc)[""].clone();
    let (cmd, ca) = quiet_exec(&doc, &format!("[{root}]"));
    assert!(cmd.contains("rd /s /q"), "{cmd}");
    assert_eq!(ca.attribute("Execute"), Some("deferred"));
    assert_eq!(ca.attribute("Impersonate"), Some("no"));
    assert_eq!(ca.attribute("Return"), Some("ignore"));
    let c = custom(&doc, ca.attribute("Id").unwrap());
    assert_eq!(c.attribute("After"), Some("RemoveFiles"));
    let check = elements(&doc, "Control")
        .into_iter()
        .find(|c| c.attribute("Type") == Some("CheckBox"))
        .and_then(|c| c.attribute("Property"))
        .expect("the checkbox");
    let cond = c.attribute("Condition").unwrap_or_default();
    for part in [
        r#"REMOVE~="ALL""#.to_string(),
        "NOT UPGRADINGPRODUCTCODE".to_string(),
        format!(r#"{check}="1""#),
        "UILevel >= 5".to_string(),
    ] {
        assert!(cond.contains(&part), "{part} missing from {cond:?}");
    }
}

// ── The framework's MSI without llama.cpp ────────────────────────────────

/// The framework's MSI and its Apps entry carry the framework's version
/// only; no llama.cpp release anywhere.
#[test]
fn the_framework_msi_names_only_the_framework_version() {
    let text = read(WXS);
    let doc = Document::parse(&text).expect("the .wxs is XML");
    assert_eq!(
        package(&doc).attribute("Name"),
        Some("Private AI Framework v$(var.Version)")
    );
    assert!(
        !text.contains("$(var.LlamaBuild)"),
        "the .wxs names the llama.cpp release"
    );
    let names: Vec<&str> = elements(&doc, "RegistryValue")
        .into_iter()
        .filter_map(|v| v.attribute("Name"))
        .collect();
    assert!(!names.contains(&"LlamaBuild"), "{names:?}");
}

/// No llama.cpp file: no ROCm kernel stores or lib\ (the engine's), only
/// the framework's bin\ and share\.
#[test]
fn the_framework_msi_holds_no_llama_cpp_files() {
    let text = read(WXS);
    let doc = Document::parse(&text).expect("the .wxs is XML");
    for gone in ["HasKpack", "HasLib", ".kpack", "KPACKFOLDER", "LIBFOLDER"] {
        assert!(
            !text.contains(gone),
            "the framework's .wxs still has {gone}"
        );
    }
    let includes: Vec<&str> = elements(&doc, "Files")
        .into_iter()
        .filter_map(|f| f.attribute("Include"))
        .collect();
    let mut sorted = includes.clone();
    sorted.sort();
    assert_eq!(
        sorted,
        [r"$(var.StagingDir)\bin\**", r"$(var.StagingDir)\share\**"],
        "{includes:?}"
    );
}

/// The framework refuses to install without llama.cpp at the release it was
/// built against or newer. Windows Installer does not compare versions in a
/// condition (Conditional Statement Syntax), so the engine is found by its
/// UpgradeCode: FindRelatedProducts compares ProductVersion and runs before
/// LaunchConditions. Maintenance (`Installed`) is never refused.
#[test]
fn the_framework_requires_llama_cpp_at_the_built_release() {
    let engine = engine_upgrade_code();
    let text = read(WXS);
    let doc = Document::parse(&text).expect("the .wxs is XML");
    let upgrade = elements(&doc, "Upgrade")
        .into_iter()
        .find(|u| u.attribute("Id") == Some(engine.as_str()))
        .unwrap_or_else(|| panic!("Upgrade Id=\"{engine}\" (llama.cpp's UpgradeCode)"));
    let versions = children(upgrade, "UpgradeVersion");
    assert_eq!(versions.len(), 1, "one UpgradeVersion");
    let v = versions[0];
    assert_eq!(
        v.attribute("OnlyDetect"),
        Some("yes"),
        "never removes the engine"
    );
    assert_eq!(v.attribute("Minimum"), Some("$(var.LlamaCppMin)"));
    assert_ne!(v.attribute("IncludeMinimum"), Some("no"));
    assert_eq!(v.attribute("Maximum"), None, "any newer release");
    let found = v.attribute("Property").expect("Property");
    let launch: Vec<_> = elements(&doc, "Launch")
        .into_iter()
        .filter(|l| l.attribute("Condition").is_some_and(|c| c.contains(found)))
        .collect();
    assert_eq!(launch.len(), 1, "one Launch condition on {found}");
    let cond = launch[0].attribute("Condition").unwrap();
    assert!(cond.starts_with("Installed OR "), "{cond:?}");
    let msg = launch[0].attribute("Message").unwrap_or_default();
    assert!(msg.contains("llama.cpp"), "{msg:?}");
    assert!(msg.is_ascii() && !msg.contains('\u{2014}'), "{msg:?}");
    assert!(msg.len() <= 255, "ICE03: {} characters", msg.len());
}

/// The framework no longer takes a 2.4 install folder from
/// `HKLM\Software\llama.cpp\InstallDir`: that key is the engine's now (a
/// custom engine folder would become the framework's), and llama.cpp's MSI
/// removes a 2.4 install before the framework's can see it. The default is
/// the framework's own folder, or the one its own key recorded.
#[test]
fn the_framework_reads_no_engine_key_for_its_folder() {
    let text = read(WXS);
    let doc = Document::parse(&text).expect("the .wxs is XML");
    for s in elements(&doc, "RegistrySearch") {
        assert_ne!(
            s.attribute("Key"),
            Some(r"Software\llama.cpp"),
            "RegistrySearch {:?} reads the engine's key",
            s.attribute("Id")
        );
    }
    let folder = property(&doc, "INSTALLFOLDER");
    let searches: Vec<_> = children(folder, "RegistrySearch");
    assert_eq!(searches.len(), 1, "one search for the recorded folder");
    assert_eq!(searches[0].attribute("Key"), Some(r"Software\private-ai"));
    assert_eq!(searches[0].attribute("Name"), Some("InstallDir"));
    assert_eq!(
        dir_id(&doc, "ProgramFiles64Folder", &["private-ai"]),
        "INSTALLFOLDER",
        "the default stays Program Files\\private-ai"
    );
    for s in elements(&doc, "SetProperty") {
        assert_ne!(
            s.attribute("Id"),
            Some("INSTALLFOLDER"),
            "INSTALLFOLDER set from elsewhere: {:?}",
            s.attribute("Value")
        );
    }
}

// ── xtask install on Windows: the engine, then the framework ─────────────

/// The names the package phase writes are read back as product and
/// version; anything else (another arch, the earlier two-version framework
/// name, a stray file) is not a package of this phase.
#[test]
fn msi_names_read_back_as_product_and_version() {
    assert_eq!(
        msi_product("llama.cpp-v1.0.0-x64.msi"),
        Some((Product::Llama, "1.0.0".to_string()))
    );
    assert_eq!(
        msi_product("private-ai-framework-v2.99.1-x64.msi"),
        Some((Product::Framework, "2.99.1".to_string()))
    );
    for other in [
        "private-ai-framework-v2.99.0-v0.9.0-x64.msi",
        "llama.cpp-v1.0.0-arm64.msi",
        "llama.cpp-v1.0.0-x64.msi.part",
        "llama.cpp-vulkan-v1.4.0-x64.msi",
        "notes.txt",
        "",
    ] {
        assert_eq!(msi_product(other), None, "{other:?}");
    }
    // The names file_name writes are the names read back.
    for p in [Product::Llama, Product::Framework] {
        let n = file_name(p, "2.99.1", "v1.0.0", "x64");
        assert_eq!(msi_product(&n).map(|(q, _)| q), Some(p), "{n}");
    }
}

/// A file in `dir` written `age` seconds before the others.
fn msi_at(dir: &Path, name: &str, age: u64) -> PathBuf {
    let p = dir.join(name);
    std::fs::write(&p, b"").unwrap();
    let t = std::time::SystemTime::now() - std::time::Duration::from_secs(age);
    std::fs::File::options()
        .write(true)
        .open(&p)
        .unwrap()
        .set_modified(t)
        .unwrap();
    p
}

fn scratch(name: &str) -> PathBuf {
    let d = std::env::temp_dir().join(format!("xtask-msi-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&d);
    std::fs::create_dir_all(&d).unwrap();
    d
}

/// Both present: the newest MSI of llama.cpp first (the framework's MSI
/// refuses to install without it), then the newest of the framework; older
/// builds and other files are not installed.
#[test]
fn install_takes_the_newest_engine_then_the_newest_framework() {
    let d = scratch("both");
    msi_at(&d, "private-ai-framework-v2.99.0-x64.msi", 300);
    let fw = msi_at(&d, "private-ai-framework-v2.99.1-x64.msi", 10);
    msi_at(&d, "llama.cpp-v0.9.0-x64.msi", 200);
    let engine = msi_at(&d, "llama.cpp-v1.0.0-x64.msi", 100);
    msi_at(&d, "private-ai-framework-v2.98.0-v0.9.0-x64.msi", 1);
    msi_at(&d, "notes.txt", 1);
    assert_eq!(install_plan(&d), [engine, fw]);
    std::fs::remove_dir_all(&d).unwrap();
}

/// The newest is the last written, not the highest version: a rebuild of
/// an older release is what was just packaged.
#[test]
fn install_takes_the_last_written_not_the_highest_version() {
    let d = scratch("written");
    msi_at(&d, "llama.cpp-v1.0.0-x64.msi", 100);
    let older = msi_at(&d, "llama.cpp-v0.9.0-x64.msi", 10);
    assert_eq!(install_plan(&d), [older]);
    std::fs::remove_dir_all(&d).unwrap();
}

/// Only one product packaged: that one alone (the framework alone needs an
/// engine already installed, which its MSI checks); nothing: nothing.
#[test]
fn install_takes_whichever_product_is_there() {
    let d = scratch("one");
    assert!(install_plan(&d).is_empty(), "an empty dist");
    let fw = msi_at(&d, "private-ai-framework-v2.99.1-x64.msi", 10);
    assert_eq!(install_plan(&d), std::slice::from_ref(&fw));
    std::fs::remove_file(&fw).unwrap();
    let engine = msi_at(&d, "llama.cpp-v1.0.0-x64.msi", 10);
    assert_eq!(install_plan(&d), [engine]);
    std::fs::remove_dir_all(&d).unwrap();
    assert!(install_plan(&d).is_empty(), "no dist directory at all");
}

/// Each product's Apps entry, as `reg query` names it, is the key its own
/// `.wxs` writes.
#[test]
fn each_product_reads_its_own_apps_entry() {
    let base = r"HKLM\SOFTWARE\Microsoft\Windows\CurrentVersion\Uninstall\";
    assert_eq!(uninstall_key(Product::Llama), format!("{base}llama.cpp"));
    assert_eq!(
        uninstall_key(Product::Framework),
        format!("{base}private-ai")
    );
    for p in [Product::Llama, Product::Framework] {
        let text = read(wxs_of(p));
        let doc = Document::parse(&text).unwrap();
        let want = uninstall_key(p)
            .strip_prefix(r"HKLM\")
            .unwrap()
            .to_lowercase();
        assert!(
            elements(&doc, "RegistryKey").into_iter().any(|k| {
                k.attribute("Root") == Some("HKLM")
                    && k.attribute("Key").map(str::to_lowercase) == Some(want.clone())
            }),
            "{} writes no {want}",
            wxs_of(p)
        );
    }
}

/// The Windows install phase goes through the plan: every MSI of it in
/// order (the dry run prints each command), each checked against its own
/// Apps entry.
#[test]
fn the_windows_install_phase_installs_the_plan_in_order() {
    let install = read("xtask/src/windows/install.rs");
    for call in [
        "msi::install_plan(",
        "msi::msi_product(",
        "msi::uninstall_key(",
    ] {
        assert!(
            install.contains(call),
            "windows/install.rs does not use {call}"
        );
    }
    assert!(
        !install.contains(r#"const PKG_NAME: &str = "private-ai-framework";"#),
        "windows/install.rs still installs the framework's MSI alone"
    );
}

/// The process stopper no longer falls back to `HKLM\Software\llama.cpp`:
/// that key is the engine's now, and its `InstallDir` would aim the
/// framework's stopper at llama.cpp's `bin\`. A 2.4 install is removed by
/// llama.cpp's MSI (with 2.4's own stopper) before the framework's runs.
#[test]
fn the_process_stopper_reads_only_the_framework_key() {
    let stopper = read("installer/msi-stop-processes.ps1");
    let code: Vec<&str> = stopper
        .lines()
        .filter(|l| !l.trim_start().starts_with('#'))
        .collect();
    let code = code.join("\n");
    assert!(
        !code.contains(r"HKLM:\Software\llama.cpp"),
        "the stopper still reads the engine's key:\n{code}"
    );
    assert!(code.contains(r"HKLM:\Software\private-ai"), "{code}");
}
