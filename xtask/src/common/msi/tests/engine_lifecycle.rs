//! The engine's life on Windows beyond a first install: its instances kept
//! running across an upgrade or repair of llama.cpp's MSI, a downgrade
//! through `xtask install` that keeps the instance services, one engine and
//! one framework file given to `xtask install`, the guard against an earlier
//! non-MSI install, and the instances' default state folder. The `.wxs`
//! conditions are evaluated, not matched as text, by a small evaluator of
//! the Windows Installer condition syntax over the property sets each
//! scenario sets. Nothing here builds or runs an MSI.

use super::two_installers::{by_id, custom, engine_data_dirs, engine_text, quiet_exec};
use super::*;
use crate::common::msi::{files_plan, removal_args};
use crate::Product;
use std::collections::HashMap;

const STOP_INSTANCES_PS1: &str = "installer/msi-stop-instances.ps1";
const START_INSTANCES_PS1: &str = "installer/msi-start-instances.ps1";
/// The public property `xtask install` passes to the engine's removal before
/// a downgrade, so the instance services stay.
const KEEP_INSTANCES: &str = "KEEPINSTANCES";

// ── A Windows Installer condition evaluator ─────────────────────────────
// The subset the two `.wxs` use: NOT, AND, OR, parentheses, a property
// alone (true when set), string and integer literals, and the comparison
// operators with their `~` (case-insensitive) forms. "Conditional Statement
// Syntax": https://learn.microsoft.com/en-us/windows/win32/msi/conditional-statement-syntax
// (an unset property is an empty string; `><` contains, `<<` starts with,
// `>>` ends with; an integer against a string that is not one is false,
// but for `<>`). XOR, EQV, IMP and feature or component states are refused,
// so a condition the evaluator cannot read fails instead of passing.

#[derive(Debug, Clone, PartialEq)]
enum Tok {
    Id(String),
    Str(String),
    Int(i64),
    Op(&'static str),
    Lp,
    Rp,
    Not,
    And,
    Or,
}

const OPS: [&str; 18] = [
    "~><", "~<>", "~>=", "~<=", "~>>", "~<<", "~=", "~>", "~<", "><", "<>", ">=", "<=", ">>", "<<",
    "=", ">", "<",
];

fn tokens(cond: &str) -> Vec<Tok> {
    let c: Vec<char> = cond.chars().collect();
    let mut out = Vec::new();
    let mut i = 0;
    while i < c.len() {
        let ch = c[i];
        if ch.is_whitespace() {
            i += 1;
        } else if ch == '(' {
            out.push(Tok::Lp);
            i += 1;
        } else if ch == ')' {
            out.push(Tok::Rp);
            i += 1;
        } else if ch == '"' {
            let end = i
                + 1
                + c[i + 1..]
                    .iter()
                    .position(|&x| x == '"')
                    .unwrap_or_else(|| panic!("unclosed string in {cond:?}"));
            out.push(Tok::Str(c[i + 1..end].iter().collect()));
            i = end + 1;
        } else if ch.is_ascii_digit() {
            let len = c[i..].iter().take_while(|x| x.is_ascii_digit()).count();
            let n: String = c[i..i + len].iter().collect();
            out.push(Tok::Int(n.parse().unwrap()));
            i += len;
        } else if ch.is_alphabetic() || ch == '_' {
            let len = c[i..]
                .iter()
                .take_while(|x| x.is_alphanumeric() || **x == '_' || **x == '.')
                .count();
            let word: String = c[i..i + len].iter().collect();
            out.push(match word.to_uppercase().as_str() {
                "NOT" => Tok::Not,
                "AND" => Tok::And,
                "OR" => Tok::Or,
                "XOR" | "EQV" | "IMP" => panic!("{word} is not supported: {cond:?}"),
                _ => Tok::Id(word),
            });
            i += len;
        } else {
            let rest: String = c[i..].iter().collect();
            let op = OPS
                .iter()
                .find(|op| rest.starts_with(**op))
                .unwrap_or_else(|| panic!("cannot read {rest:?} in {cond:?}"));
            out.push(Tok::Op(op));
            i += op.len();
        }
    }
    out
}

#[derive(Debug, Clone)]
enum Val {
    S(String),
    I(i64),
}

struct Eval<'a> {
    t: Vec<Tok>,
    i: usize,
    props: &'a HashMap<String, String>,
}

impl Eval<'_> {
    fn peek(&self) -> Option<&Tok> {
        self.t.get(self.i)
    }
    fn or(&mut self) -> bool {
        let mut v = self.and();
        while self.peek() == Some(&Tok::Or) {
            self.i += 1;
            let r = self.and();
            v = v || r;
        }
        v
    }
    fn and(&mut self) -> bool {
        let mut v = self.not();
        while self.peek() == Some(&Tok::And) {
            self.i += 1;
            let r = self.not();
            v = v && r;
        }
        v
    }
    fn not(&mut self) -> bool {
        if self.peek() == Some(&Tok::Not) {
            self.i += 1;
            !self.not()
        } else {
            self.atom()
        }
    }
    fn operand(&mut self) -> Val {
        let t = self.peek().cloned();
        self.i += 1;
        match t {
            Some(Tok::Id(name)) => Val::S(self.props.get(&name).cloned().unwrap_or_default()),
            Some(Tok::Str(s)) => Val::S(s),
            Some(Tok::Int(n)) => Val::I(n),
            other => panic!("an operand expected, found {other:?}"),
        }
    }
    fn atom(&mut self) -> bool {
        if self.peek() == Some(&Tok::Lp) {
            self.i += 1;
            let v = self.or();
            assert_eq!(self.peek(), Some(&Tok::Rp), "unbalanced parentheses");
            self.i += 1;
            return v;
        }
        let l = self.operand();
        if let Some(Tok::Op(op)) = self.peek().cloned() {
            self.i += 1;
            let r = self.operand();
            compare(&l, op, &r)
        } else {
            match l {
                Val::S(s) => !s.is_empty(),
                Val::I(n) => n != 0,
            }
        }
    }
}

fn compare(l: &Val, op: &str, r: &Val) -> bool {
    let fold = op.starts_with('~');
    let op = op.trim_start_matches('~');
    let as_int = |v: &Val| match v {
        Val::I(n) => Some(*n),
        Val::S(s) => s.trim().parse::<i64>().ok(),
    };
    let numeric = matches!(l, Val::I(_)) || matches!(r, Val::I(_));
    if numeric {
        let (Some(a), Some(b)) = (as_int(l), as_int(r)) else {
            return op == "<>";
        };
        return match op {
            "=" => a == b,
            "<>" => a != b,
            ">" => a > b,
            "<" => a < b,
            ">=" => a >= b,
            "<=" => a <= b,
            other => panic!("{other} between integers"),
        };
    }
    let text = |v: &Val| match v {
        Val::S(s) if fold => s.to_lowercase(),
        Val::S(s) => s.clone(),
        Val::I(_) => unreachable!(),
    };
    let (a, b) = (text(l), text(r));
    match op {
        "=" => a == b,
        "<>" => a != b,
        "><" => a.contains(&b),
        "<<" => a.starts_with(&b),
        ">>" => a.ends_with(&b),
        ">" => a > b,
        "<" => a < b,
        ">=" => a >= b,
        "<=" => a <= b,
        other => panic!("unknown operator {other}"),
    }
}

/// `cond` under the properties `props` (an empty condition is true, as in
/// the sequence tables).
fn holds(cond: &str, props: &[(&str, &str)]) -> bool {
    if cond.trim().is_empty() {
        return true;
    }
    let props: HashMap<String, String> = props
        .iter()
        .map(|(k, v)| (k.to_string(), v.to_string()))
        .collect();
    let mut e = Eval {
        t: tokens(cond),
        i: 0,
        props: &props,
    };
    let v = e.or();
    assert_eq!(e.i, e.t.len(), "trailing tokens in {cond:?}");
    v
}

#[test]
fn the_condition_evaluator_reads_what_the_wxs_files_write() {
    assert!(holds(r#"REMOVE~="ALL""#, &[("REMOVE", "all")]));
    assert!(!holds(r#"REMOVE="ALL""#, &[("REMOVE", "all")]));
    assert!(!holds(r#"REMOVE~="ALL""#, &[]));
    assert!(holds("NOT UPGRADINGPRODUCTCODE", &[]));
    assert!(!holds(
        "NOT UPGRADINGPRODUCTCODE",
        &[("UPGRADINGPRODUCTCODE", "{X}")]
    ));
    assert!(holds("UILevel >= 5", &[("UILevel", "5")]));
    assert!(!holds("UILevel >= 5", &[("UILevel", "3")]));
    assert!(!holds("UILevel >= 5", &[]));
    assert!(holds(
        r#"Installed OR (NOT P OR (P ~>< "msiexec"))"#,
        &[("P", "MsiExec.exe /I{X}")]
    ));
    assert!(!holds(
        r#"Installed OR (NOT P OR (P ~>< "msiexec"))"#,
        &[("P", r"C:\x\uninstall.exe")]
    ));
    assert!(holds("A AND B OR C", &[("C", "1")]));
    assert!(!holds("A AND (B OR C)", &[("C", "1")]));
    assert!(holds("NOT NOT A", &[("A", "1")]));
}

// ── The scenarios a `.wxs` condition is evaluated under ─────────────────
// Each is the property set Windows Installer has at that point: Installed
// set in maintenance and in the old package of an upgrade, REMOVE=ALL on a
// removal, UPGRADINGPRODUCTCODE in the old package that a major upgrade
// removes, WIX_UPGRADE_DETECTED (MajorUpgrade's ActionProperty) in the new
// one, REINSTALL on a repair, UILevel 5 full UI, 3 basic (/qb), 2 none (/qn).

const OLD_CODE: &str = "{0D3B5C1E-7A2F-4E9B-9C41-5E8F2A6B7C3D}";

struct Scenario {
    name: &'static str,
    props: Vec<(&'static str, &'static str)>,
}

fn scenario(name: &'static str) -> Scenario {
    let props = match name {
        "first install" => vec![("UILevel", "5")],
        "repair" => vec![
            ("Installed", "1"),
            ("REINSTALL", "ALL"),
            ("REINSTALLMODE", "omus"),
            ("UILevel", "5"),
        ],
        "upgrade: the new release installed" => {
            vec![("WIX_UPGRADE_DETECTED", OLD_CODE), ("UILevel", "5")]
        }
        "upgrade: the old release removed" => vec![
            ("Installed", "1"),
            ("REMOVE", "ALL"),
            (
                "UPGRADINGPRODUCTCODE",
                "{9A1B2C3D-4E5F-4061-8293-A4B5C6D7E8F9}",
            ),
            ("UILevel", "5"),
        ],
        "uninstall from Apps" => vec![
            ("Installed", "1"),
            ("REMOVE", "ALL"),
            ("LLAMA_CPP_UNINSTALL", "1"),
            ("UILevel", "5"),
        ],
        "silent uninstall" => vec![("Installed", "1"), ("REMOVE", "ALL"), ("UILevel", "2")],
        "downgrade: xtask install removes the newer release" => vec![
            ("Installed", "1"),
            ("REMOVE", "ALL"),
            (KEEP_INSTANCES, "1"),
            ("UILevel", "3"),
        ],
        other => panic!("no scenario {other}"),
    };
    Scenario { name, props }
}

const ALL_SCENARIOS: [&str; 7] = [
    "first install",
    "repair",
    "upgrade: the new release installed",
    "upgrade: the old release removed",
    "uninstall from Apps",
    "silent uninstall",
    "downgrade: xtask install removes the newer release",
];

/// The scenarios in which the sequence entry of custom action `id` runs.
fn runs_in(doc: &Document, id: &str) -> Vec<&'static str> {
    let cond = custom(doc, id)
        .attribute("Condition")
        .unwrap_or_default()
        .to_string();
    ALL_SCENARIOS
        .iter()
        .map(|n| scenario(n))
        .filter(|s| holds(&cond, &s.props))
        .map(|s| s.name)
        .collect()
}

/// The WixQuietExec custom action that runs the script embedded as
/// `$(var.<var>)` with powershell.exe -EncodedCommand, deferred as
/// LocalSystem and never failing the installation: its id.
fn script_action(doc: &Document, var: &str) -> String {
    let value = format!("$(var.{var})");
    let holder = elements(doc, "Property")
        .into_iter()
        .find(|p| p.attribute("Value") == Some(value.as_str()))
        .unwrap_or_else(|| panic!("a Property holding {value}"));
    assert_eq!(
        holder.attribute("Hidden"),
        Some("yes"),
        "{value}: kept out of the log"
    );
    let prop = holder.attribute("Id").unwrap();
    let (cmd, ca) = quiet_exec(doc, &format!("-EncodedCommand [{prop}]"));
    assert!(cmd.contains("powershell.exe"), "{cmd}");
    assert_eq!(ca.attribute("Execute"), Some("deferred"), "{var}");
    assert_eq!(ca.attribute("Impersonate"), Some("no"), "{var}");
    assert_eq!(ca.attribute("Return"), Some("ignore"), "{var}");
    ca.attribute("Id").unwrap().to_string()
}

/// Code lines of a PowerShell script: no blank line, no comment.
fn ps_code(rel: &str) -> String {
    let s = read(rel);
    assert!(s.is_ascii(), "{rel} is not ASCII (Windows PowerShell 5.1)");
    s.lines()
        .map(str::trim)
        .filter(|l| !l.is_empty() && !l.starts_with('#'))
        .collect::<Vec<_>>()
        .join("\n")
}

/// The file names a script spells as `llama.cpp\<name>` in a literal.
fn engine_files(code: &str) -> Vec<String> {
    code.match_indices(r"llama.cpp\")
        .map(|(i, m)| {
            code[i + m.len()..]
                .chars()
                .take_while(|c| !matches!(c, '\'' | '"' | ' ' | ')' | '\n'))
                .collect::<String>()
        })
        .filter(|n| !n.is_empty())
        .collect()
}

// ── Running instances kept across an upgrade or a repair ────────────────

/// Before an upgrade or a repair replaces llama.cpp's files (and before the
/// removal of a downgrade), the running instance services are stopped and
/// recorded: a deferred PowerShell script as LocalSystem, before RemoveFiles
/// (so before InstallFiles too). It runs in the package being removed by a
/// major upgrade (the old release; the new one's deferred actions start
/// after RemoveExistingProducts, which runs right after InstallValidate),
/// never on a first install nor on a real uninstall, whose
/// RemoveInstanceServices deletes them.
#[test]
fn running_instances_are_stopped_before_an_upgrade_or_repair_replaces_files() {
    let text = engine_text();
    let doc = Document::parse(&text).expect("llama.cpp.wxs is XML");
    let id = script_action(&doc, "StopInstancesCmd");
    let c = custom(&doc, &id);
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
    assert_eq!(
        runs_in(&doc, &id),
        [
            "repair",
            "upgrade: the old release removed",
            "downgrade: xtask install removes the newer release",
        ]
    );
}

/// After the files are in place, the recorded instances are started again:
/// on a repair, in the new release of an upgrade, and on a first install
/// (the older release a downgrade installs after the removal that recorded
/// them is a first install to Windows Installer). Never on a removal.
#[test]
fn recorded_instances_start_again_once_the_files_are_in_place() {
    let text = engine_text();
    let doc = Document::parse(&text).expect("llama.cpp.wxs is XML");
    let id = script_action(&doc, "StartInstancesCmd");
    let c = custom(&doc, &id);
    let anchored = matches!(
        (c.attribute("After"), c.attribute("Before")),
        (
            Some("InstallFiles" | "InstallServices" | "StartServices"),
            None
        ) | (None, Some("InstallFinalize"))
    );
    assert!(
        anchored,
        "after InstallFiles: {:?} {:?}",
        c.attribute("After"),
        c.attribute("Before")
    );
    assert_eq!(
        runs_in(&doc, &id),
        [
            "first install",
            "repair",
            "upgrade: the new release installed",
        ]
    );
}

/// The real uninstall alone deletes the instance services, full UI or
/// silent: not the old half of an upgrade, and not the removal a downgrade
/// makes with KEEPINSTANCES=1 (the older release finds them again).
#[test]
fn only_a_real_uninstall_deletes_the_instance_services() {
    let text = engine_text();
    let doc = Document::parse(&text).expect("llama.cpp.wxs is XML");
    let id = script_action(&doc, "RemoveInstancesCmd");
    assert_eq!(
        runs_in(&doc, &id),
        ["uninstall from Apps", "silent uninstall"]
    );
}

/// KEEPINSTANCES reaches the server side of the installation (Secure) and
/// has no value of its own: only `xtask install`'s command line sets it.
#[test]
fn keep_instances_is_a_secure_property_with_no_default() {
    let text = engine_text();
    let doc = Document::parse(&text).expect("llama.cpp.wxs is XML");
    let p = by_id(&doc, "Property", KEEP_INSTANCES);
    assert_eq!(p.attribute("Secure"), Some("yes"));
    assert_eq!(p.attribute("Value"), None);
}

/// The stopper: the instance services by their prefix, the RUNNING ones
/// recorded in a file of `%ProgramData%\llama.cpp` (spelled
/// `llama.cpp\<name>`, the root the MSI creates; the Linux packages keep
/// theirs in `/run/llama.cpp.instances`) before any is stopped, then
/// stopped; never deleted, never the framework's services, exit 0 whatever
/// happens.
#[test]
fn the_stop_script_records_the_running_instances_then_stops_them() {
    let code = ps_code(STOP_INSTANCES_PS1);
    assert!(code.contains("'llama.cpp_*'"), "{code}");
    assert!(code.contains("Running"), "only the running ones: {code}");
    assert!(code.contains("$env:ProgramData"), "{code}");
    assert!(
        !engine_files(&code).is_empty(),
        "no llama.cpp\\<file> record: {code}"
    );
    let write = ["Set-Content", "Out-File", "WriteAllLines", "WriteAllText"]
        .iter()
        .filter_map(|w| code.find(w))
        .min()
        .expect("the record is written");
    let stop = code.find("Stop-Service").expect("Stop-Service");
    assert!(write < stop, "recorded before stopped: {code}");
    assert!(!code.contains("sc.exe"), "never deletes a service: {code}");
    for other in ["private-ai-manager", "private-ai-bff"] {
        assert!(!code.contains(other), "names {other}");
    }
    assert!(code.contains("try") && code.contains("catch"), "{code}");
    assert!(code.ends_with("exit 0"), "{code}");
}

/// The starter reads the same record, starts each llama.cpp_ service in it,
/// deletes the record, and never fails the installation.
#[test]
fn the_start_script_starts_what_was_recorded_and_forgets_it() {
    let stop = ps_code(STOP_INSTANCES_PS1);
    let code = ps_code(START_INSTANCES_PS1);
    let recorded = engine_files(&stop);
    assert!(
        engine_files(&code).iter().any(|f| recorded.contains(f)),
        "not the stopper's record {recorded:?}: {code}"
    );
    assert!(code.contains("$env:ProgramData"), "{code}");
    assert!(
        ["Get-Content", "ReadAllLines", "ReadAllText"]
            .iter()
            .any(|r| code.contains(r)),
        "reads the record: {code}"
    );
    assert!(
        code.contains("llama.cpp_"),
        "only instance services: {code}"
    );
    let start = code.find("Start-Service").expect("Start-Service");
    let forget = code.find("Remove-Item").expect("the record is deleted");
    assert!(start < forget, "started, then forgotten: {code}");
    assert!(!code.contains("sc.exe"), "{code}");
    for other in ["private-ai-manager", "private-ai-bff"] {
        assert!(!code.contains(other), "names {other}");
    }
    assert!(code.contains("try") && code.contains("catch"), "{code}");
    assert!(code.ends_with("exit 0"), "{code}");
}

/// `xtask package llama` embeds both scripts as it embeds the remover.
#[test]
fn the_package_phase_embeds_the_stop_and_start_scripts() {
    let package = read("xtask/src/windows/package.rs");
    for want in [
        "StopInstancesCmd",
        r"installer\msi-stop-instances.ps1",
        "StartInstancesCmd",
        r"installer\msi-start-instances.ps1",
    ] {
        assert!(package.contains(want), "windows/package.rs has no {want}");
    }
}

// ── A downgrade through xtask install keeps the instance services ───────

/// The engine's removal before a downgrade passes KEEPINSTANCES=1; the
/// framework's, which owns no instance service, nothing more than before.
#[test]
fn the_engine_downgrade_removal_keeps_the_instances() {
    let code = "{6F1E2B3A-0C4D-4E5F-8A9B-1C2D3E4F5A6B}";
    assert_eq!(
        removal_args(Product::Llama, code),
        ["/x", code, "/qb", "KEEPINSTANCES=1"]
    );
    assert_eq!(removal_args(Product::Framework, code), ["/x", code, "/qb"]);
}

#[test]
fn the_windows_install_phase_removes_with_removal_args() {
    let install = read("xtask/src/windows/install.rs");
    assert!(
        install.contains("msi::removal_args("),
        "windows/install.rs does not use msi::removal_args"
    );
}

// ── xtask install <file>...: one engine and one framework ───────────────

fn dist(name: &str) -> PathBuf {
    Path::new("dist").join(name)
}

/// Given in either order, the engine is installed first.
#[test]
fn install_files_put_the_engine_first() {
    let engine = dist("llama.cpp-v1.0.0-x64.msi");
    let fw = dist("private-ai-framework-v2.99.1-x64.msi");
    for given in [
        vec![fw.clone(), engine.clone()],
        vec![engine.clone(), fw.clone()],
    ] {
        assert_eq!(files_plan(&given).unwrap(), [engine.clone(), fw.clone()]);
    }
}

#[test]
fn install_files_take_one_product_alone() {
    let engine = dist("llama.cpp-v0.9.0-x64.msi");
    assert_eq!(files_plan(std::slice::from_ref(&engine)).unwrap(), [engine]);
    let fw = dist("private-ai-framework-v2.99.0-x64.msi");
    assert_eq!(files_plan(std::slice::from_ref(&fw)).unwrap(), [fw]);
}

/// Two of one product are refused, naming both: which one to install is
/// the user's call.
#[test]
fn install_files_refuse_two_of_the_same_product() {
    for (a, b) in [
        ("llama.cpp-v1.0.0-x64.msi", "llama.cpp-v0.9.0-x64.msi"),
        (
            "private-ai-framework-v2.99.1-x64.msi",
            "private-ai-framework-v2.99.0-x64.msi",
        ),
    ] {
        let err = files_plan(&[dist(a), dist(b)])
            .expect_err("two of one product")
            .to_string();
        assert!(err.contains(a) && err.contains(b), "{err}");
    }
    let err = files_plan(&[
        dist("llama.cpp-v1.0.0-x64.msi"),
        dist("private-ai-framework-v2.99.1-x64.msi"),
        dist("llama.cpp-v1.0.0-x64.msi"),
    ]);
    assert!(err.is_err(), "the same engine twice: {err:?}");
}

/// A file not named as `xtask package` names an MSI is refused, naming it:
/// its product cannot be told, so neither can the order.
#[test]
fn install_files_refuse_an_unknown_name() {
    for name in [
        "setup.msi",
        "private-ai-framework-v2.98.0-v0.9.0-x64.msi",
        "llama.cpp-v1.0.0-x64.deb",
    ] {
        let err = files_plan(&[dist("llama.cpp-v1.0.0-x64.msi"), dist(name)])
            .expect_err(name)
            .to_string();
        assert!(err.contains(name), "{err}");
    }
}

/// The command line hands every file to the Windows install phase, which
/// plans them; the old one-file refusal is gone.
#[test]
fn the_windows_install_phase_takes_the_files_plan() {
    let main = read("xtask/src/main.rs");
    assert!(
        !main.contains("on Windows the package is one MSI: give one file"),
        "main.rs still refuses more than one file on Windows"
    );
    let install = read("xtask/src/windows/install.rs");
    assert!(
        install.contains("msi::files_plan("),
        "windows/install.rs does not use msi::files_plan"
    );
}

// ── The guard against an earlier non-MSI install ────────────────────────

/// What `Uninstall\llama.cpp`'s UninstallString held, by who wrote it: the
/// NSIS installer of 2.0 (its `.nsi.template` before commit 7615769), the framework's MSI of 2.1 to 2.4 (`MsiExec.exe
/// /I[ProductCode] LLAMA_UNINSTALL=1`, which this MSI removes as
/// OLDFRAMEWORKFOUND), and this MSI's own entry.
const NSIS: &str = r#""C:\Program Files\llama.cpp\bin\llama-cpp-uninstall.exe""#;
const MSI_2_4: &str = "MsiExec.exe /I{6F1E2B3A-0C4D-4E5F-8A9B-1C2D3E4F5A6B} LLAMA_UNINSTALL=1";
const MSI_OWN: &str = "MsiExec.exe /I{0D3B5C1E-7A2F-4E9B-9C41-5E8F2A6B7C3D} LLAMA_CPP_UNINSTALL=1";

/// The property AppSearch fills with `Uninstall\llama.cpp`'s
/// UninstallString (raw, 64-bit view), and the Launch conditions that read
/// it.
fn guard(doc: &Document) -> (String, Vec<String>) {
    let search = elements(doc, "RegistrySearch")
        .into_iter()
        .find(|r| {
            r.attribute("Key")
                == Some(r"Software\Microsoft\Windows\CurrentVersion\Uninstall\llama.cpp")
                && r.attribute("Name") == Some("UninstallString")
        })
        .expect(r"a RegistrySearch of Uninstall\llama.cpp UninstallString");
    assert_eq!(search.attribute("Root"), Some("HKLM"));
    assert_eq!(search.attribute("Type"), Some("raw"));
    assert_eq!(search.attribute("Bitness"), Some("always64"));
    let prop = search.parent_element().expect("its Property");
    assert_eq!(prop.tag_name().name(), "Property");
    assert_eq!(prop.attribute("Secure"), Some("yes"));
    let id = prop.attribute("Id").unwrap().to_string();
    let launches: Vec<String> = elements(doc, "Launch")
        .into_iter()
        .filter_map(|l| l.attribute("Condition"))
        .filter(|c| c.contains(id.as_str()))
        .map(str::to_string)
        .collect();
    assert!(!launches.is_empty(), "no Launch condition reads {id}");
    (id, launches)
}

/// A previous llama.cpp install that is not an MSI stops llama.cpp's MSI
/// before anything changes, as the framework's MSI does; an MSI entry
/// passes: 2.4's (removed by this MSI, so the guard must not block the
/// upgrade from 2.4) and this MSI's own (an upgrade or repackage). Never in
/// maintenance.
#[test]
fn the_engine_msi_refuses_a_previous_non_msi_install_only() {
    let text = engine_text();
    let doc = Document::parse(&text).expect("llama.cpp.wxs is XML");
    let (id, launches) = guard(&doc);
    let passes = |props: &[(&str, &str)]| launches.iter().all(|c| holds(c, props));
    assert!(passes(&[]), "no earlier install");
    assert!(passes(&[(id.as_str(), MSI_2_4)]), "an upgrade from 2.4");
    assert!(passes(&[(id.as_str(), MSI_OWN)]), "an upgrade of this MSI");
    assert!(!passes(&[(id.as_str(), NSIS)]), "a non-MSI install");
    assert!(
        passes(&[(id.as_str(), NSIS), ("Installed", "1")]),
        "maintenance is never refused"
    );
    let message = elements(&doc, "Launch")
        .into_iter()
        .find(|l| {
            l.attribute("Condition")
                .is_some_and(|c| c.contains(id.as_str()))
        })
        .and_then(|l| l.attribute("Message"))
        .unwrap_or_default()
        .to_lowercase();
    assert!(message.contains("uninstall"), "{message:?}");
}

// ── The instances' default state folder ─────────────────────────────────

/// `%ProgramData%\llama.cpp\state`, where an instance with no StateDir
/// keeps its slot snapshots (`paths::instance_state_dir`): created by the
/// MSI with models\'s ACL, the instances' services modify, users read.
#[test]
fn the_engine_state_folder_is_written_by_the_services_and_read_by_users() {
    let text = engine_text();
    let doc = Document::parse(&text).expect("llama.cpp.wxs is XML");
    let ids = engine_data_dirs(&doc);
    let state = folder_dacl(&doc, &ids["state"]);
    let models = folder_dacl(&doc, &ids["models"]);
    assert!(state.protected);
    for sid in [SYSTEM, ADMINISTRATORS, ALL_SERVICES, USERS] {
        assert_eq!(state.granted(sid), models.granted(sid), "{sid}");
    }
    assert_eq!(state.writers(), models.writers());
}

// ── Stale mentions ──────────────────────────────────────────────────────

fn rust_files(dir: &Path, out: &mut Vec<PathBuf>) {
    for e in std::fs::read_dir(dir).unwrap().flatten() {
        let p = e.path();
        if p.is_dir() {
            rust_files(&p, out);
        } else if p.extension().is_some_and(|x| x == "rs") {
            out.push(p);
        }
    }
}

/// The framework's MSI no longer writes `LlamaBuild`: no code says it does.
/// The MSI tests that pin its absence are the only ones to spell it.
#[test]
fn nothing_names_the_registry_value_the_framework_dropped() {
    let name = concat!("Llama", "Build");
    let mut files = Vec::new();
    for c in std::fs::read_dir(repo().join("crates")).unwrap().flatten() {
        if c.path().join("src").is_dir() {
            rust_files(&c.path().join("src"), &mut files);
        }
    }
    rust_files(&repo().join("xtask").join("src"), &mut files);
    let skip = repo().join("xtask/src/common/msi/tests");
    let hits: Vec<String> = files
        .iter()
        .filter(|f| !f.starts_with(&skip))
        .filter(|f| std::fs::read_to_string(f).unwrap().contains(name))
        .map(|f| f.display().to_string())
        .collect();
    assert!(hits.is_empty(), "{name} in {hits:?}");
}

/// The HIP runtime is in llama.cpp's own `bin\`, which no installer puts on
/// PATH, and the framework's `bin\` holds none: no installed folder is set
/// aside, so every PATH dir with `amdhip64*.dll` counts as a dist.
#[test]
fn the_prerequisites_report_sets_no_installed_bin_aside() {
    let text = read("scripts/00-install-prerequisites.ps1");
    let code = text
        .lines()
        .filter(|l| !l.trim_start().starts_with('#'))
        .collect::<Vec<_>>()
        .join("\n");
    assert!(
        code.contains("amdhip64*.dll"),
        "the PATH check of the HIP runtime is gone"
    );
    for stale in [
        r"HKLM:\Software\private-ai",
        r"HKLM:\Software\llama.cpp",
        "$fwBin",
    ] {
        assert!(!code.contains(stale), "the report still reads {stale}");
    }
    assert!(
        !text.contains(r"framework's bin\"),
        "a comment still calls the framework's bin\\ a holder of the HIP runtime"
    );
}
