//! What the Windows package holds beyond the harvested tree: the
//! executables of the cargo workspace that `xtask package` stages into
//! `bin\`, and (in the tests) the manager's and the bff's services and the machine-wide data
//! tree under `%ProgramData%\private-ai` that `installer\private-ai-framework.wxs`
//! registers and creates. The tests read the `.wxs` as XML, so they run on
//! every OS; nothing here builds the MSI.

/// The cargo workspace's executables that `xtask package` copies from
/// cargo's release dir into `staging\bin\` on Windows.
#[cfg_attr(not(windows), allow(dead_code))]
pub const CARGO_EXES: &[&str] = &[
    "pai-config.exe",
    "pai.exe",
    "pai-manager.exe",
    "pai-bff.exe",
];

#[cfg(test)]
mod tests {
    use super::CARGO_EXES;
    use roxmltree::{Document, Node};
    use std::collections::BTreeMap;
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

    const WXS: &str = "installer/private-ai-framework.wxs";

    fn elements<'a, 'i>(doc: &'a Document<'i>, tag: &str) -> Vec<Node<'a, 'i>> {
        doc.descendants()
            .filter(|n| n.is_element() && n.tag_name().name() == tag)
            .collect()
    }

    fn children<'a, 'i>(node: Node<'a, 'i>, tag: &str) -> Vec<Node<'a, 'i>> {
        node.children()
            .filter(|n| n.is_element() && n.tag_name().name() == tag)
            .collect()
    }

    /// The manager's own service name, as the manager registers its
    /// dispatcher (`MANAGER_SERVICE` in the manager's Windows module).
    fn manager_service() -> String {
        let src = read("crates/manager/src/manager/windows.rs");
        let line = src
            .lines()
            .find(|l| l.starts_with("pub const MANAGER_SERVICE: &str = "))
            .expect("MANAGER_SERVICE in the manager's Windows module");
        line.split('"').nth(1).expect("a quoted name").to_string()
    }

    /// The one `ServiceInstall` of the manager.
    fn manager_service_install<'a, 'i>(doc: &'a Document<'i>) -> Node<'a, 'i> {
        let name = manager_service();
        let found: Vec<_> = elements(doc, "ServiceInstall")
            .into_iter()
            .filter(|n| n.attribute("Name") == Some(name.as_str()))
            .collect();
        assert_eq!(
            found.len(),
            1,
            "one ServiceInstall Name=\"{name}\" in the .wxs, found {}",
            found.len()
        );
        found[0]
    }

    fn component_of<'a, 'i>(node: Node<'a, 'i>) -> Node<'a, 'i> {
        node.ancestors()
            .find(|n| n.is_element() && n.tag_name().name() == "Component")
            .expect("inside a Component")
    }

    /// Whether the component `id` is installed by the feature `Main`:
    /// nested in it, referenced by it, or in a group it references.
    fn in_main_feature(doc: &Document, id: &str) -> bool {
        let Some(main) = elements(doc, "Feature")
            .into_iter()
            .find(|f| f.attribute("Id") == Some("Main"))
        else {
            return false;
        };
        let groups: Vec<&str> = main
            .descendants()
            .filter(|n| n.is_element() && n.tag_name().name() == "ComponentGroupRef")
            .filter_map(|n| n.attribute("Id"))
            .collect();
        let in_group = elements(doc, "ComponentGroup")
            .into_iter()
            .filter(|g| g.attribute("Id").is_some_and(|g| groups.contains(&g)))
            .any(|g| {
                g.descendants().any(|n| {
                    n.is_element()
                        && matches!(n.tag_name().name(), "Component" | "ComponentRef")
                        && n.attribute("Id") == Some(id)
                })
            });
        in_group
            || main.descendants().any(|n| {
                n.is_element()
                    && matches!(n.tag_name().name(), "Component" | "ComponentRef")
                    && n.attribute("Id") == Some(id)
            })
    }

    // ── SDDL, read as Windows applies it ─────────────────────────────────

    /// One ACE of a DACL: its type (`A` allow, `D` deny), flags, access mask
    /// and trustee SID.
    #[derive(Debug, Clone, PartialEq, Eq)]
    struct Ace {
        kind: String,
        flags: String,
        mask: u32,
        sid: String,
    }

    /// A DACL: whether it is protected (inherits nothing from the parent
    /// folder) and its ACEs.
    #[derive(Debug)]
    struct Dacl {
        protected: bool,
        aces: Vec<Ace>,
    }

    /// The SID behind an SDDL alias, or the SID string itself. Only the
    /// aliases that could appear here; any other one fails the test so it
    /// is looked up rather than guessed.
    /// learn.microsoft.com/windows/win32/secauthz/sid-strings
    fn sid_of(s: &str) -> String {
        if s.starts_with("S-1-") {
            return s.to_string();
        }
        match s {
            "SY" => "S-1-5-18",
            "BA" => "S-1-5-32-544",
            "BU" => "S-1-5-32-545",
            "BG" => "S-1-5-32-546",
            "PU" => "S-1-5-32-547",
            "AU" => "S-1-5-11",
            "WD" => "S-1-1-0",
            "IU" => "S-1-5-4",
            "NU" => "S-1-5-2",
            "SU" => "S-1-5-6",
            "AN" => "S-1-5-7",
            "LS" => "S-1-5-19",
            "NS" => "S-1-5-20",
            "CO" => "S-1-3-0",
            "OW" => "S-1-3-4",
            other => panic!("SDDL alias {other:?} is not known to this test"),
        }
        .to_string()
    }

    /// The access mask of an SDDL rights string: a hex number or a
    /// concatenation of two-letter rights.
    /// learn.microsoft.com/windows/win32/secauthz/ace-strings (file rights
    /// FA/FR/FW/FX, the generic rights mapped to them for files, the
    /// standard rights and the directory-service letters, which are the
    /// same low bits on a file).
    fn mask_of(rights: &str) -> u32 {
        if let Some(hex) = rights
            .strip_prefix("0x")
            .or_else(|| rights.strip_prefix("0X"))
        {
            return u32::from_str_radix(hex, 16).expect("hex rights");
        }
        assert!(rights.len().is_multiple_of(2), "rights {rights:?}");
        (0..rights.len())
            .step_by(2)
            .map(|i| match &rights[i..i + 2] {
                "FA" | "GA" => 0x001F_01FF,
                "FR" | "GR" => 0x0012_0089,
                "FW" | "GW" => 0x0012_0116,
                "FX" | "GX" => 0x0012_00A0,
                "SD" => 0x0001_0000,
                "RC" => 0x0002_0000,
                "WD" => 0x0004_0000,
                "WO" => 0x0008_0000,
                "CC" => 0x1,
                "DC" => 0x2,
                "LC" => 0x4,
                "SW" => 0x8,
                "RP" => 0x10,
                "WP" => 0x20,
                "DT" => 0x40,
                "LO" => 0x80,
                "CR" => 0x100,
                other => panic!("SDDL right {other:?} is not known to this test"),
            })
            .fold(0, |a, b| a | b)
    }

    fn dacl(sddl: &str) -> Dacl {
        let d = sddl
            .find("D:")
            .unwrap_or_else(|| panic!("no DACL in {sddl:?}"));
        let rest = &sddl[d + 2..];
        let open = rest.find('(').unwrap_or(rest.len());
        // A SACL after the DACL would start with S:, never inside the flags.
        let flags = &rest[..open];
        let mut aces = Vec::new();
        let mut body = &rest[open..];
        while let Some(stripped) = body.strip_prefix('(') {
            let close = stripped.find(')').expect("ACE closed");
            let fields: Vec<&str> = stripped[..close].split(';').collect();
            assert_eq!(fields.len(), 6, "ACE {:?}", &stripped[..close]);
            aces.push(Ace {
                kind: fields[0].to_string(),
                flags: fields[1].to_string(),
                mask: mask_of(fields[2]),
                sid: sid_of(fields[5]),
            });
            body = &stripped[close + 1..];
        }
        Dacl {
            protected: flags.contains('P'),
            aces,
        }
    }

    const SYSTEM: &str = "S-1-5-18";
    const ADMINISTRATORS: &str = "S-1-5-32-544";
    const USERS: &str = "S-1-5-32-545";
    /// `NT SERVICE\ALL SERVICES`: every service process, the instances'
    /// virtual accounts `NT SERVICE\llama.cpp_<id>` included.
    const ALL_SERVICES: &str = "S-1-5-80-0";
    /// Full control (`FA`).
    const FULL: u32 = 0x001F_01FF;
    /// Read and execute: `FILE_GENERIC_READ | FILE_GENERIC_EXECUTE`.
    const READ: u32 = 0x0012_00A9;
    /// Modify: read, write, execute and delete, no `WRITE_DAC` /
    /// `WRITE_OWNER` / `FILE_DELETE_CHILD`.
    const MODIFY: u32 = 0x0013_01BF;
    /// Every bit that changes a file or folder: write data / add file,
    /// append / add subdirectory, write EA, delete child, write attributes,
    /// delete, write DAC, write owner.
    const WRITE_BITS: u32 =
        0x2 | 0x4 | 0x10 | 0x40 | 0x100 | 0x0001_0000 | 0x0004_0000 | 0x0008_0000;

    impl Dacl {
        /// What `sid` is allowed on this folder and on everything created in
        /// it: the allow ACEs that apply to the folder (not inherit-only) and
        /// are inherited by files and subfolders (OI and CI).
        fn granted(&self, sid: &str) -> u32 {
            self.aces
                .iter()
                .filter(|a| a.kind == "A" && a.sid == sid)
                .filter(|a| {
                    a.flags.contains("OI") && a.flags.contains("CI") && !a.flags.contains("IO")
                })
                .fold(0, |m, a| m | a.mask)
        }

        /// The trustees any allow ACE gives a write bit to.
        fn writers(&self) -> Vec<String> {
            let mut w: Vec<String> = self
                .aces
                .iter()
                .filter(|a| a.kind == "A" && a.mask & WRITE_BITS != 0)
                .map(|a| a.sid.clone())
                .collect();
            w.sort();
            w.dedup();
            w
        }

        fn names(&self, sid: &str) -> bool {
            self.aces.iter().any(|a| a.sid == sid)
        }
    }

    #[test]
    fn sddl_reads_aliases_hex_and_protection() {
        let d =
            dacl("O:SYG:SYD:PAI(A;OICI;FA;;;SY)(A;OICI;0x1301bf;;;S-1-5-80-0)(A;OICI;FRFX;;;BU)");
        assert!(d.protected);
        assert_eq!(d.granted(SYSTEM), FULL);
        assert_eq!(d.granted(ALL_SERVICES), MODIFY);
        assert_eq!(d.granted(USERS), READ);
        assert_eq!(
            d.writers(),
            vec![SYSTEM.to_string(), ALL_SERVICES.to_string()]
        );
        assert!(!dacl("D:AI(A;OICI;FA;;;SY)").protected);
        // Inherit-only does not reach the folder itself.
        assert_eq!(dacl("D:P(A;OICIIO;FA;;;BU)").granted(USERS), 0);
    }

    // ── Staging: the manager beside the configurator ─────────────────────

    /// `pai-manager.exe` is installed in `bin\` beside `pai-config.exe`:
    /// the manager's service runs it (`serve`), and so do the instances'
    /// services (`run-instance <id>`), by the path of the running manager.
    #[test]
    fn cargo_exes_stage_the_manager_beside_the_configurator() {
        for exe in ["pai-config.exe", "pai-manager.exe"] {
            assert!(CARGO_EXES.contains(&exe), "{exe} not in {CARGO_EXES:?}");
        }
    }

    /// The Windows package phase stages the list above, not one
    /// hard-coded configurator.
    #[test]
    fn the_windows_package_stages_every_cargo_exe() {
        let package = read("xtask/src/windows/package.rs");
        assert!(
            package.contains("CARGO_EXES"),
            "xtask/src/windows/package.rs does not stage msi::CARGO_EXES"
        );
        assert!(
            !package.contains(r#"r"cargo-target\release\pai-config.exe""#),
            "xtask/src/windows/package.rs still stages pai-config.exe alone"
        );
    }

    /// Already true: the build phase compiles the manager with the
    /// configurator on every OS, so it is in cargo's release dir for the
    /// package phase.
    #[test]
    fn the_build_compiles_the_manager_with_the_configurator() {
        let build = read("xtask/src/common/configurator.rs");
        assert!(
            build.contains(r#""pai-manager""#),
            "common/configurator.rs no longer builds pai-manager"
        );
    }

    // ── The manager's executable and its service ─────────────────────────

    /// Windows Installer needs the service's executable as the key path of
    /// the component that installs the service (ServiceInstall table,
    /// `Component_`): `pai-manager.exe` is left out of the harvest of `bin\`
    /// and declared in that component instead, never twice.
    #[test]
    fn the_manager_exe_is_not_harvested() {
        let text = read(WXS);
        let doc = Document::parse(&text).expect("the .wxs is XML");
        let bin = elements(&doc, "Files")
            .into_iter()
            .find(|f| f.attribute("Directory") == Some("BINFOLDER"))
            .expect("the harvest of bin\\");
        let excluded: Vec<&str> = children(bin, "Exclude")
            .into_iter()
            .filter_map(|e| e.attribute("Files"))
            .collect();
        assert!(
            excluded
                .iter()
                .any(|f| f.ends_with(r"\bin\pai-manager.exe")),
            "bin\\pai-manager.exe is still harvested: Exclude {excluded:?}"
        );
    }

    #[test]
    fn the_manager_exe_is_the_key_path_of_its_service_component() {
        let text = read(WXS);
        let doc = Document::parse(&text).expect("the .wxs is XML");
        let files: Vec<_> = elements(&doc, "File")
            .into_iter()
            .filter(|f| {
                f.attribute("Source")
                    .is_some_and(|s| s.ends_with(r"\bin\pai-manager.exe"))
            })
            .collect();
        assert_eq!(files.len(), 1, "one File for bin\\pai-manager.exe");
        let file = files[0];
        assert!(
            file.attribute("Source")
                .is_some_and(|s| s.starts_with("$(var.StagingDir)")),
            "{:?}",
            file.attribute("Source")
        );
        assert_eq!(file.attribute("KeyPath"), Some("yes"));
        let component = component_of(file);
        assert_eq!(
            file.attribute("Directory")
                .or_else(|| component.attribute("Directory")),
            Some("BINFOLDER")
        );
        let install = manager_service_install(&doc);
        assert_eq!(
            component_of(install).attribute("Id"),
            component.attribute("Id"),
            "the ServiceInstall is not in pai-manager.exe's component"
        );
    }

    /// LocalSystem (the manager creates, starts and deletes the instances'
    /// services and owns `%ProgramData%\private-ai\config`), its own
    /// process, started at boot, `pai-manager serve`. No password: the
    /// StartName of LocalSystem is null (ServiceInstall table).
    #[test]
    fn the_manager_service_runs_serve_as_local_system_at_boot() {
        let text = read(WXS);
        let doc = Document::parse(&text).expect("the .wxs is XML");
        let s = manager_service_install(&doc);
        assert_eq!(s.attribute("Type"), Some("ownProcess"));
        assert_eq!(s.attribute("Start"), Some("auto"));
        assert_eq!(s.attribute("ErrorControl"), Some("normal"));
        assert_eq!(s.attribute("Arguments"), Some("serve"));
        assert!(
            matches!(s.attribute("Account"), None | Some("LocalSystem")),
            "Account {:?}",
            s.attribute("Account")
        );
        assert_eq!(s.attribute("Password"), None);
        assert_ne!(s.attribute("Interactive"), Some("yes"));
        // A service that did not install fails the install (Vital, yes by default).
        assert_ne!(s.attribute("Vital"), Some("no"));
    }

    /// The names Services (services.msc) shows: system text in English,
    /// the display name the Linux unit's Description.
    #[test]
    fn the_manager_service_has_an_english_display_name_and_description() {
        let text = read(WXS);
        let doc = Document::parse(&text).expect("the .wxs is XML");
        let s = manager_service_install(&doc);
        assert_eq!(
            s.attribute("DisplayName"),
            Some("Private AI Framework manager")
        );
        let description = s.attribute("Description").unwrap_or_default();
        assert!(description.len() > 20, "Description {description:?}");
        assert!(description.is_ascii(), "Description {description:?}");
        assert!(
            !description.contains('\u{2014}'),
            "em-dash in {description:?}"
        );
        assert_ne!(s.attribute("EraseDescription"), Some("yes"));
    }

    /// The counterpart of the Linux unit's `Restart=on-failure`: a crashed
    /// manager comes back, or the pipe is gone until the next boot.
    #[test]
    fn the_manager_service_restarts_after_a_failure() {
        let text = read(WXS);
        let doc = Document::parse(&text).expect("the .wxs is XML");
        let s = manager_service_install(&doc);
        let name = manager_service();
        let actions: Vec<_> = children(s, "ServiceConfigFailureActions")
            .into_iter()
            .chain(
                elements(&doc, "ServiceConfigFailureActions")
                    .into_iter()
                    .filter(|a| a.attribute("ServiceName") == Some(name.as_str())),
            )
            .collect();
        assert_eq!(
            actions.len(),
            1,
            "one ServiceConfigFailureActions for {name}"
        );
        let a = actions[0];
        assert_eq!(a.attribute("OnInstall"), Some("yes"));
        assert_eq!(a.attribute("OnReinstall"), Some("yes"));
        let failures = children(a, "Failure");
        let first = failures.first().expect("a first Failure");
        assert_eq!(first.attribute("Action"), Some("restartService"));
        let delay: u64 = first
            .attribute("Delay")
            .and_then(|d| d.parse().ok())
            .expect("a Delay in milliseconds");
        assert!((1_000..=60_000).contains(&delay), "Delay {delay} ms");
    }

    /// Started at the end of the install, stopped before its files are
    /// replaced or removed (install covers the new product of an upgrade and
    /// a repair; uninstall the old product of an upgrade), deleted on
    /// uninstall. The SCM stops it: nothing kills it.
    #[test]
    fn the_manager_service_starts_on_install_stops_on_both_and_goes_on_uninstall() {
        let text = read(WXS);
        let doc = Document::parse(&text).expect("the .wxs is XML");
        let name = manager_service();
        let controls: Vec<_> = elements(&doc, "ServiceControl")
            .into_iter()
            .filter(|c| c.attribute("Name") == Some(name.as_str()))
            .collect();
        assert_eq!(controls.len(), 1, "one ServiceControl Name=\"{name}\"");
        let c = controls[0];
        assert_eq!(c.attribute("Start"), Some("install"));
        assert_eq!(c.attribute("Stop"), Some("both"));
        assert_eq!(c.attribute("Remove"), Some("uninstall"));
        assert_ne!(c.attribute("Wait"), Some("no"));
        let install = manager_service_install(&doc);
        assert_eq!(
            component_of(c).attribute("Id"),
            component_of(install).attribute("Id")
        );
    }

    #[test]
    fn the_manager_service_is_part_of_the_main_feature() {
        let text = read(WXS);
        let doc = Document::parse(&text).expect("the .wxs is XML");
        let component = component_of(manager_service_install(&doc));
        let id = component.attribute("Id").expect("the component has an Id");
        assert!(in_main_feature(&doc, id), "component {id} is not in Main");
    }

    // ── Removing the instances' services ─────────────────────────────────

    /// The SCM services `llama.cpp_<id>` are the manager's, created at run
    /// time, so Windows Installer does not know them: `pai-manager
    /// uninstall-services` stops and deletes them (deferred, as LocalSystem)
    /// whenever the manager's files are removed or repaired, after the
    /// manager has stopped and before the files go. That includes the old
    /// product's removal in an upgrade: the new manager recreates them from
    /// the instances on disk when it starts (and starts those set to start
    /// at boot), so the config and the instances are kept. A failure never
    /// rolls the uninstall back.
    #[test]
    fn the_instance_services_are_removed_before_the_files() {
        let text = read(WXS);
        let doc = Document::parse(&text).expect("the .wxs is XML");
        let set: Vec<_> = elements(&doc, "SetProperty")
            .into_iter()
            .filter(|s| {
                s.attribute("Value").is_some_and(|v| {
                    v.contains(r#"[BINFOLDER]pai-manager.exe" uninstall-services"#)
                })
            })
            .collect();
        assert_eq!(
            set.len(),
            1,
            "one SetProperty running pai-manager.exe uninstall-services"
        );
        let set = set[0];
        let id = set.attribute("Id").expect("SetProperty Id");
        assert_eq!(set.attribute("Sequence"), Some("execute"));
        assert_eq!(set.attribute("Before"), Some(id));
        let ca = elements(&doc, "CustomAction")
            .into_iter()
            .find(|c| c.attribute("Id") == Some(id))
            .unwrap_or_else(|| panic!("CustomAction {id}"));
        assert_eq!(ca.attribute("BinaryRef"), Some("Wix4UtilCA_X64"));
        assert_eq!(ca.attribute("DllEntry"), Some("WixQuietExec"));
        assert_eq!(ca.attribute("Execute"), Some("deferred"));
        assert_eq!(ca.attribute("Impersonate"), Some("no"));
        assert_eq!(ca.attribute("Return"), Some("ignore"));
        let custom = elements(&doc, "Custom")
            .into_iter()
            .find(|c| c.attribute("Action") == Some(id))
            .unwrap_or_else(|| panic!("Custom Action=\"{id}\" in a sequence"));
        assert_eq!(
            custom.parent_element().map(|p| p.tag_name().name()),
            Some("InstallExecuteSequence")
        );
        let anchored = matches!(
            (custom.attribute("After"), custom.attribute("Before")),
            (Some("StopServices" | "DeleteServices"), None)
                | (None, Some("DeleteServices" | "RemoveFiles"))
        );
        assert!(
            anchored,
            "{id} must run after StopServices and before RemoveFiles: After {:?} Before {:?}",
            custom.attribute("After"),
            custom.attribute("Before")
        );
        let condition = custom.attribute("Condition").unwrap_or_default();
        assert!(
            condition.contains(r#"REMOVE~="ALL""#),
            "Condition {condition:?}"
        );
        assert!(condition.contains("REINSTALL"), "Condition {condition:?}");
        assert!(
            !condition.contains("UPGRADINGPRODUCTCODE"),
            "the old product of an upgrade removes them too: {condition:?}"
        );
    }

    // ── pai, the command line ────────────────────────────────────────────

    /// `pai.exe`, the console command line, is staged into `bin\` beside
    /// the GUI and the manager.
    #[test]
    fn cargo_exes_stage_the_command_line_beside_the_configurator() {
        assert!(
            CARGO_EXES.contains(&"pai.exe"),
            "pai.exe not in {CARGO_EXES:?}"
        );
    }

    /// Already true, kept so: `pai.exe` is installed by the harvest of
    /// `bin\` (it installs no service, so it is not left out of it the way
    /// `pai-manager.exe` is), and nowhere else.
    #[test]
    fn the_command_line_is_harvested_into_bin() {
        let text = read(WXS);
        let doc = Document::parse(&text).expect("the .wxs is XML");
        let bin = elements(&doc, "Files")
            .into_iter()
            .find(|f| f.attribute("Directory") == Some("BINFOLDER"))
            .expect("the harvest of bin\\");
        assert!(
            !children(bin, "Exclude").into_iter().any(|e| e
                .attribute("Files")
                .is_some_and(|f| f.ends_with(r"\bin\pai.exe"))),
            "bin\\pai.exe is left out of the harvest"
        );
        assert!(
            !elements(&doc, "File").into_iter().any(|f| f
                .attribute("Source")
                .is_some_and(|s| s.ends_with(r"\bin\pai.exe"))),
            "bin\\pai.exe is declared beside the harvest"
        );
    }

    /// Already true, kept so: `bin\` (where `pai` now is) is appended to
    /// the machine PATH by a feature that is on by default, and removed
    /// again on uninstall (not Permanent), so `pai` runs from any console.
    #[test]
    fn bin_is_appended_to_the_machine_path_until_uninstall() {
        let text = read(WXS);
        let doc = Document::parse(&text).expect("the .wxs is XML");
        let path: Vec<_> = elements(&doc, "Environment")
            .into_iter()
            .filter(|e| e.attribute("Name") == Some("PATH"))
            .collect();
        assert_eq!(path.len(), 1, "one Environment Name=\"PATH\"");
        let env = path[0];
        for (attr, want) in [
            ("Value", "[BINFOLDER]"),
            ("Part", "last"),
            ("Action", "set"),
            ("System", "yes"),
        ] {
            assert_eq!(env.attribute(attr), Some(want), "{attr}");
        }
        assert_ne!(env.attribute("Permanent"), Some("yes"));
        let component = component_of(env).attribute("Id").expect("component id");
        let feature = elements(&doc, "Feature")
            .into_iter()
            .find(|f| {
                children(*f, "ComponentRef")
                    .into_iter()
                    .any(|r| r.attribute("Id") == Some(component))
            })
            .expect("a feature installs the PATH component");
        assert_eq!(feature.attribute("Level"), Some("1"), "on by default");
    }

    /// The graceful stop runs `pai.exe control stop-and-close` (the
    /// conversation snapshot, then llama-server, then the GUI's close), and
    /// falls back to `pai-config.exe` when there is no `pai.exe` (an older
    /// install, whose GUI binary still had the command), then to the 2.4
    /// configurator: tried in that order, each only when the one before is
    /// missing.
    #[test]
    fn the_graceful_stop_runs_pai_then_an_older_installs_configurator() {
        let stopper = read("installer/msi-stop-processes.ps1");
        let old = concat!("llama-cpp", "-config.exe");
        let assignments: Vec<&str> = stopper
            .lines()
            .map(str::trim)
            .filter(|l| l.contains("$exe = Join-Path $bin "))
            .collect();
        let want = [
            "$exe = Join-Path $bin 'pai.exe'".to_string(),
            "if (-not (Test-Path -LiteralPath $exe)) { $exe = Join-Path $bin 'pai-config.exe' }"
                .to_string(),
            format!("if (-not (Test-Path -LiteralPath $exe)) {{ $exe = Join-Path $bin '{old}' }}"),
        ];
        assert_eq!(assignments, want, "{stopper}");
        let start = stopper
            .lines()
            .find(|l| l.contains("Start-Process"))
            .expect("the graceful stop's Start-Process");
        assert!(
            start.contains("-FilePath $exe -ArgumentList 'control', 'stop-and-close'"),
            "{start}"
        );
    }

    /// A `pai` running from the installed `bin\` (a benchmark in a console)
    /// holds `pai.exe` like the GUI holds its exe: it is stopped with them.
    #[test]
    fn the_process_stopper_stops_a_running_pai() {
        let stopper = read("installer/msi-stop-processes.ps1");
        let get = stopper
            .lines()
            .find(|l| l.contains("Get-Process -Name"))
            .expect("the stopper's Get-Process");
        let names: Vec<&str> = get
            .split("-Name")
            .nth(1)
            .and_then(|r| r.split("-ErrorAction").next())
            .unwrap_or_default()
            .split(',')
            .map(|n| n.trim().trim_matches('\''))
            .collect();
        assert!(names.contains(&"pai"), "{get}");
        assert!(names.contains(&"pai-config"), "{get}");
    }

    /// Already true, kept so: the process stopper stops the configurator
    /// and llama-server, never the manager, whose service the SCM stops
    /// (ServiceControl); and it stays immediate, before InstallValidate.
    #[test]
    fn the_process_stopper_leaves_the_manager_to_the_scm() {
        let stopper = read("installer/msi-stop-processes.ps1");
        let get = stopper
            .lines()
            .find(|l| l.contains("Get-Process -Name"))
            .expect("the stopper's Get-Process");
        assert!(!get.contains("pai-manager"), "{get}");
        let text = read(WXS);
        let doc = Document::parse(&text).expect("the .wxs is XML");
        let ca = elements(&doc, "CustomAction")
            .into_iter()
            .find(|c| c.attribute("Id") == Some("StopProcesses"))
            .expect("StopProcesses");
        assert_eq!(ca.attribute("Execute"), Some("immediate"));
        let custom = elements(&doc, "Custom")
            .into_iter()
            .find(|c| c.attribute("Action") == Some("StopProcesses"))
            .expect("StopProcesses sequenced");
        assert_eq!(custom.attribute("Before"), Some("InstallValidate"));
    }

    // ── pai-bff, the web service ─────────────────────────────────────────

    /// The bff's SCM service, the Linux unit's name without `.service`.
    const BFF_SERVICE: &str = "private-ai-bff";

    /// The one `ServiceInstall` of the bff.
    fn bff_service_install<'a, 'i>(doc: &'a Document<'i>) -> Node<'a, 'i> {
        let found: Vec<_> = elements(doc, "ServiceInstall")
            .into_iter()
            .filter(|n| n.attribute("Name") == Some(BFF_SERVICE))
            .collect();
        assert_eq!(
            found.len(),
            1,
            "one ServiceInstall Name=\"{BFF_SERVICE}\" in the .wxs, found {}",
            found.len()
        );
        found[0]
    }

    /// `pai-bff.exe` is staged into `bin\` from cargo's release dir, beside
    /// the manager.
    #[test]
    fn cargo_exes_stage_the_bff_beside_the_manager() {
        assert!(
            CARGO_EXES.contains(&"pai-bff.exe"),
            "pai-bff.exe not in {CARGO_EXES:?}"
        );
    }

    /// As for the manager: the service's executable is the key path of the
    /// component that installs the service, so `pai-bff.exe` is left out of
    /// the harvest of `bin\` and declared once, there.
    #[test]
    fn the_bff_exe_is_not_harvested() {
        let text = read(WXS);
        let doc = Document::parse(&text).expect("the .wxs is XML");
        let bin = elements(&doc, "Files")
            .into_iter()
            .find(|f| f.attribute("Directory") == Some("BINFOLDER"))
            .expect("the harvest of bin\\");
        let excluded: Vec<&str> = children(bin, "Exclude")
            .into_iter()
            .filter_map(|e| e.attribute("Files"))
            .collect();
        assert!(
            excluded.iter().any(|f| f.ends_with(r"\bin\pai-bff.exe")),
            "bin\\pai-bff.exe is still harvested: Exclude {excluded:?}"
        );
    }

    #[test]
    fn the_bff_exe_is_the_key_path_of_its_own_service_component() {
        let text = read(WXS);
        let doc = Document::parse(&text).expect("the .wxs is XML");
        let files: Vec<_> = elements(&doc, "File")
            .into_iter()
            .filter(|f| {
                f.attribute("Source")
                    .is_some_and(|s| s.ends_with(r"\bin\pai-bff.exe"))
            })
            .collect();
        assert_eq!(files.len(), 1, "one File for bin\\pai-bff.exe");
        let file = files[0];
        assert!(
            file.attribute("Source")
                .is_some_and(|s| s.starts_with("$(var.StagingDir)")),
            "{:?}",
            file.attribute("Source")
        );
        assert_eq!(file.attribute("KeyPath"), Some("yes"));
        let component = component_of(file);
        assert_eq!(
            file.attribute("Directory")
                .or_else(|| component.attribute("Directory")),
            Some("BINFOLDER")
        );
        let install = bff_service_install(&doc);
        assert_eq!(
            component_of(install).attribute("Id"),
            component.attribute("Id"),
            "the ServiceInstall is not in pai-bff.exe's component"
        );
        assert_ne!(
            component.attribute("Id"),
            component_of(manager_service_install(&doc)).attribute("Id"),
            "the bff has a component of its own, not the manager's"
        );
    }

    /// Its own process, started at boot, `pai-bff service` (the entry that
    /// hands the process to the service dispatcher), as its virtual account
    /// `NT SERVICE\private-ai-bff`, the way the instances run as `NT
    /// SERVICE\llama.cpp_<id>`: it then carries the same `NT SERVICE\ALL
    /// SERVICES` read on `config\` (where `bff.ini` is) that the instances
    /// rely on. CreateService (lpServiceStartName): "If the service is
    /// configured to use a virtual account, specify the name as NT
    /// SERVICE\ServiceName"; lpPassword: "If the account name ... is the name
    /// of a managed service account or virtual account name, the lpPassword
    /// parameter must be NULL". The ServiceInstall table's StartName and
    /// Password are CreateService's parameters.
    #[test]
    fn the_bff_service_runs_service_as_its_virtual_account_at_boot() {
        let text = read(WXS);
        let doc = Document::parse(&text).expect("the .wxs is XML");
        let s = bff_service_install(&doc);
        assert_eq!(s.attribute("Type"), Some("ownProcess"));
        assert_eq!(s.attribute("Start"), Some("auto"));
        assert_eq!(s.attribute("ErrorControl"), Some("normal"));
        assert_eq!(s.attribute("Arguments"), Some("service"));
        assert_eq!(
            s.attribute("Account"),
            Some(format!(r"NT SERVICE\{BFF_SERVICE}").as_str())
        );
        assert_eq!(s.attribute("Password"), None, "no password: NULL");
        assert_ne!(s.attribute("Interactive"), Some("yes"));
        assert_ne!(s.attribute("Vital"), Some("no"));
    }

    /// The names Services shows, in English: the display name the Linux
    /// unit's Description (linux/system_files.rs).
    #[test]
    fn the_bff_service_has_an_english_display_name_and_description() {
        let text = read(WXS);
        let doc = Document::parse(&text).expect("the .wxs is XML");
        let s = bff_service_install(&doc);
        assert_eq!(
            s.attribute("DisplayName"),
            Some("Private AI Framework web service")
        );
        let description = s.attribute("Description").unwrap_or_default();
        assert!(description.len() > 20, "Description {description:?}");
        assert!(description.is_ascii(), "Description {description:?}");
        assert!(
            !description.contains('\u{2014}'),
            "em-dash in {description:?}"
        );
        assert_ne!(s.attribute("EraseDescription"), Some("yes"));
    }

    /// The counterpart of the Linux unit's `Restart=on-failure`.
    #[test]
    fn the_bff_service_restarts_after_a_failure() {
        let text = read(WXS);
        let doc = Document::parse(&text).expect("the .wxs is XML");
        let s = bff_service_install(&doc);
        let actions: Vec<_> = children(s, "ServiceConfigFailureActions")
            .into_iter()
            .chain(
                elements(&doc, "ServiceConfigFailureActions")
                    .into_iter()
                    .filter(|a| a.attribute("ServiceName") == Some(BFF_SERVICE)),
            )
            .collect();
        assert_eq!(
            actions.len(),
            1,
            "one ServiceConfigFailureActions for {BFF_SERVICE}"
        );
        let a = actions[0];
        assert_eq!(a.attribute("OnInstall"), Some("yes"));
        assert_eq!(a.attribute("OnReinstall"), Some("yes"));
        let failures = children(a, "Failure");
        let first = failures.first().expect("a first Failure");
        assert_eq!(first.attribute("Action"), Some("restartService"));
        let delay: u64 = first
            .attribute("Delay")
            .and_then(|d| d.parse().ok())
            .expect("a Delay in milliseconds");
        assert!((1_000..=60_000).contains(&delay), "Delay {delay} ms");
    }

    /// Started at the end of the install, stopped before its files are
    /// replaced or removed, deleted on uninstall, by the SCM.
    #[test]
    fn the_bff_service_starts_on_install_stops_on_both_and_goes_on_uninstall() {
        let text = read(WXS);
        let doc = Document::parse(&text).expect("the .wxs is XML");
        let controls: Vec<_> = elements(&doc, "ServiceControl")
            .into_iter()
            .filter(|c| c.attribute("Name") == Some(BFF_SERVICE))
            .collect();
        assert_eq!(
            controls.len(),
            1,
            "one ServiceControl Name=\"{BFF_SERVICE}\""
        );
        let c = controls[0];
        assert_eq!(c.attribute("Start"), Some("install"));
        assert_eq!(c.attribute("Stop"), Some("both"));
        assert_eq!(c.attribute("Remove"), Some("uninstall"));
        assert_ne!(c.attribute("Wait"), Some("no"));
        assert_eq!(
            component_of(c).attribute("Id"),
            component_of(bff_service_install(&doc)).attribute("Id")
        );
    }

    #[test]
    fn the_bff_service_is_part_of_the_main_feature() {
        let text = read(WXS);
        let doc = Document::parse(&text).expect("the .wxs is XML");
        let component = component_of(bff_service_install(&doc));
        let id = component.attribute("Id").expect("the component has an Id");
        assert!(in_main_feature(&doc, id), "component {id} is not in Main");
    }

    /// The SCM stops the bff (ServiceControl), never the process stopper.
    #[test]
    fn the_process_stopper_leaves_the_bff_to_the_scm() {
        let stopper = read("installer/msi-stop-processes.ps1");
        assert!(!stopper.contains("pai-bff"), "the stopper names pai-bff");
    }

    // ── %ProgramData%\private-ai ─────────────────────────────────────────

    /// The ids of `%ProgramData%\private-ai` (key "") and of its `config`,
    /// `logs` and `models` folders.
    fn data_dirs(doc: &Document) -> BTreeMap<String, String> {
        let common = elements(doc, "StandardDirectory")
            .into_iter()
            .find(|d| d.attribute("Id") == Some("CommonAppDataFolder"))
            .expect("StandardDirectory CommonAppDataFolder (%ProgramData%)");
        let root = children(common, "Directory")
            .into_iter()
            .find(|d| d.attribute("Name") == Some("private-ai"))
            .expect("Directory Name=\"private-ai\" under CommonAppDataFolder");
        let mut ids = BTreeMap::new();
        ids.insert(String::new(), root.attribute("Id").expect("Id").to_string());
        for name in ["config", "logs", "models"] {
            let dir = children(root, "Directory")
                .into_iter()
                .find(|d| d.attribute("Name") == Some(name))
                .unwrap_or_else(|| {
                    panic!("Directory Name=\"{name}\" under %ProgramData%\\private-ai")
                });
            ids.insert(
                name.to_string(),
                dir.attribute("Id").expect("Id").to_string(),
            );
        }
        ids
    }

    /// The DACL the MSI sets on folder `id` (a CreateFolder with one
    /// PermissionEx, MsiLockPermissionsEx), in a component of `Main`.
    fn folder_dacl(doc: &Document, id: &str) -> Dacl {
        let folders: Vec<_> = elements(doc, "CreateFolder")
            .into_iter()
            .filter(|f| {
                f.attribute("Directory")
                    .or_else(|| component_of(*f).attribute("Directory"))
                    == Some(id)
            })
            .collect();
        assert_eq!(folders.len(), 1, "one CreateFolder for {id}");
        let folder = folders[0];
        let component = component_of(folder).attribute("Id").expect("component Id");
        assert!(
            in_main_feature(doc, component),
            "component {component} is not in Main"
        );
        let perms = children(folder, "PermissionEx");
        assert_eq!(perms.len(), 1, "one PermissionEx on {id}");
        assert_eq!(
            perms[0].attribute("Condition"),
            None,
            "{id}: always applied"
        );
        dacl(perms[0].attribute("Sddl").expect("Sddl"))
    }

    #[test]
    fn the_machine_data_tree_is_under_programdata() {
        let text = read(WXS);
        let doc = Document::parse(&text).expect("the .wxs is XML");
        let ids = data_dirs(&doc);
        assert_eq!(ids.len(), 4, "{ids:?}");
    }

    /// Protected (nothing inherited from `%ProgramData%`, whose default ACL
    /// lets any user create files), SYSTEM and Administrators in full, users
    /// read (they browse to logs\ and models\), nobody else writes.
    #[test]
    fn the_data_root_is_written_by_system_and_administrators_only() {
        let text = read(WXS);
        let doc = Document::parse(&text).expect("the .wxs is XML");
        let ids = data_dirs(&doc);
        let d = folder_dacl(&doc, &ids[""]);
        assert!(d.protected, "the root's DACL inherits from %ProgramData%");
        assert_eq!(d.granted(SYSTEM) & FULL, FULL);
        assert_eq!(d.granted(ADMINISTRATORS) & FULL, FULL);
        assert_eq!(d.granted(USERS) & READ, READ);
        assert_eq!(
            d.writers(),
            vec![SYSTEM.to_string(), ADMINISTRATORS.to_string()]
        );
    }

    /// config\: the manager (SYSTEM) writes, Administrators too, the
    /// instances' virtual accounts read their `server@<id>.ini` and
    /// `presets.ini`; users have no ACE at all: they read and write through
    /// the manager, and `OpencodeApiKey` is in there.
    #[test]
    fn the_config_folder_is_read_by_the_services_and_no_user() {
        let text = read(WXS);
        let doc = Document::parse(&text).expect("the .wxs is XML");
        let ids = data_dirs(&doc);
        let d = folder_dacl(&doc, &ids["config"]);
        assert!(d.protected, "config\\ inherits from its parent");
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

    /// logs\: each instance's service creates and rotates its
    /// `llama-server@<id>.log` (a rename to `.previous.log`: modify), users
    /// read it in the configurator's Log window.
    #[test]
    fn the_logs_folder_is_written_by_the_services_and_read_by_users() {
        let text = read(WXS);
        let doc = Document::parse(&text).expect("the .wxs is XML");
        let ids = data_dirs(&doc);
        let d = folder_dacl(&doc, &ids["logs"]);
        assert!(d.protected, "logs\\ inherits from its parent");
        assert_eq!(d.granted(SYSTEM) & FULL, FULL);
        assert_eq!(d.granted(ADMINISTRATORS) & FULL, FULL);
        assert_eq!(d.granted(ALL_SERVICES) & MODIFY, MODIFY);
        assert_eq!(d.granted(USERS) & READ, READ);
        let mut writers = vec![SYSTEM, ADMINISTRATORS, ALL_SERVICES];
        writers.sort();
        assert_eq!(d.writers(), writers);
    }

    /// models\: a default ModelsDir the instances can read, and write as
    /// `LLAMA_CACHE` (a model downloaded by name); users read, an
    /// administrator adds models.
    #[test]
    fn the_models_folder_is_written_by_the_services_and_read_by_users() {
        let text = read(WXS);
        let doc = Document::parse(&text).expect("the .wxs is XML");
        let ids = data_dirs(&doc);
        let d = folder_dacl(&doc, &ids["models"]);
        assert!(d.protected, "models\\ inherits from its parent");
        assert_eq!(d.granted(SYSTEM) & FULL, FULL);
        assert_eq!(d.granted(ADMINISTRATORS) & FULL, FULL);
        assert_eq!(d.granted(ALL_SERVICES) & MODIFY, MODIFY);
        assert_eq!(d.granted(USERS) & READ, READ);
        let mut writers = vec![SYSTEM, ADMINISTRATORS, ALL_SERVICES];
        writers.sort();
        assert_eq!(d.writers(), writers);
    }

    /// The one `Property` with this id.
    fn property<'a, 'i>(doc: &'a Document<'i>, id: &str) -> Node<'a, 'i> {
        let found: Vec<_> = elements(doc, "Property")
            .into_iter()
            .filter(|p| p.attribute("Id") == Some(id))
            .collect();
        assert_eq!(found.len(), 1, "one Property {id}");
        found[0]
    }

    /// The one `Control` with this id.
    fn control<'a, 'i>(doc: &'a Document<'i>, id: &str) -> Node<'a, 'i> {
        let found: Vec<_> = elements(doc, "Control")
            .into_iter()
            .filter(|c| c.attribute("Id") == Some(id))
            .collect();
        assert_eq!(found.len(), 1, "one Control {id}");
        found[0]
    }

    /// Under the manager the per-user tree never holds a `config\server.ini`
    /// of this install, so the removal dialog's offer is keyed on the tree
    /// itself: a DirectorySearch with no FileSearch below it sets its
    /// property to the folder's path when the folder exists (AppSearch: a
    /// signature not in the Signature table is a directory search).
    #[test]
    fn the_user_data_offer_is_found_from_the_per_user_tree_itself() {
        let text = read(WXS);
        let doc = Document::parse(&text).expect("the .wxs is XML");
        let found = property(&doc, "LLAMA_USERDATA_FOUND");
        assert_eq!(found.attribute("Secure"), Some("yes"));
        let searches: Vec<_> = found.children().filter(|n| n.is_element()).collect();
        assert_eq!(searches.len(), 1, "one search under the property");
        let search = searches[0];
        assert_eq!(search.tag_name().name(), "DirectorySearch");
        assert_eq!(
            search.attribute("Path"),
            Some("[LocalAppDataFolder]private-ai")
        );
        assert_eq!(search.attribute("Depth"), Some("0"));
        assert!(
            !search.descendants().skip(1).any(|n| n.is_element()),
            "nothing searched below the tree: the folder itself is the signature"
        );
        for n in found.descendants().filter(|n| n.is_element()) {
            for a in n.attributes() {
                assert!(
                    !a.value().to_lowercase().contains("server.ini"),
                    "{} {}={:?} names server.ini",
                    n.tag_name().name(),
                    a.name(),
                    a.value()
                );
            }
        }
    }

    /// The removal dialog names what the per-user tree holds under the
    /// manager (settings, benchmark results, logs and an earlier install's
    /// server settings and presets), never that the configuration lives
    /// there, and keeps the folder and the "kept unless ticked" promise.
    #[test]
    fn the_user_data_text_names_what_the_tree_holds_under_the_manager() {
        let text = read(WXS);
        let doc = Document::parse(&text).expect("the .wxs is XML");
        let ctl = control(&doc, "UserDataText");
        assert_eq!(ctl.attribute("ShowCondition"), Some("LLAMA_USERDATA_FOUND"));
        let body = ctl.attribute("Text").expect("UserDataText has a Text");
        let lower = body.to_lowercase();
        assert!(
            body.contains("\r\n[PRIVATE_AI_USERDATA_DIR]\r\n"),
            "{body:?}"
        );
        assert!(
            body.contains("They are kept unless you tick the box below"),
            "{body:?}"
        );
        for word in [
            "settings",
            "benchmark results",
            "logs",
            "earlier install",
            "presets",
        ] {
            assert!(lower.contains(word), "{body:?} names {word}");
        }
        assert!(!lower.contains("configuration"), "{body:?}");
        let check = control(&doc, "DeleteUserData");
        assert_eq!(
            check.attribute("ShowCondition"),
            Some("LLAMA_USERDATA_FOUND")
        );
        assert_eq!(check.attribute("Property"), Some("LLAMA_DELETEUSERDATA"));
    }

    /// The machine's config, logs and models outlive an upgrade and an
    /// uninstall: Windows Installer removes a CreateFolder folder only when
    /// it is empty, and nothing else in the package deletes them.
    #[test]
    fn nothing_removes_the_machine_data() {
        let text = read(WXS);
        let doc = Document::parse(&text).expect("the .wxs is XML");
        let ids: Vec<String> = data_dirs(&doc).into_values().collect();
        for tag in ["RemoveFolder", "RemoveFile", "RemoveFolderEx"] {
            for e in elements(&doc, tag) {
                let target = e
                    .attribute("Directory")
                    .or_else(|| e.attribute("Property"))
                    .or_else(|| component_of(e).attribute("Directory"))
                    .unwrap_or_default();
                assert!(!ids.iter().any(|id| id == target), "{tag} on {target}");
            }
        }
        for s in elements(&doc, "SetProperty") {
            let value = s.attribute("Value").unwrap_or_default();
            assert!(
                !value.contains("CommonAppDataFolder")
                    && !ids.iter().any(|id| value.contains(&format!("[{id}]"))),
                "SetProperty {:?} names the machine data: {value}",
                s.attribute("Id")
            );
        }
    }
}
