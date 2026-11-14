//! The Windows installer workflow (`.github/workflows/installers-windows.yml`)
//! and the prerequisites script's `-Minimal` switch it runs, pinned as text:
//! a workflow runs only on GitHub, so what it must keep doing is checked here.
//! It builds the CI-only minimal llama.cpp (`xtask configure --minimal`),
//! packages both MSIs, installs them on the runner, runs the automatable
//! steps of the installer test plan (`installer/AGENTS.md`), uninstalls, and
//! uploads the MSIs and the msiexec logs. Started by hand only: every run
//! needs the user's go-ahead.

use std::path::{Path, PathBuf};

const WORKFLOW: &str = ".github/workflows/installers-windows.yml";
const PREREQ: &str = "scripts/00-install-prerequisites.ps1";

fn repo() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("xtask sits in the repo root")
        .to_path_buf()
}

fn read(rel: &str) -> String {
    std::fs::read_to_string(repo().join(rel)).unwrap_or_else(|e| panic!("{rel}: {e}"))
}

/// The workflow's lines without comment lines (YAML's and PowerShell's `#`),
/// so a comment naming a trigger or a secret does not count.
fn code_lines(text: &str) -> Vec<&str> {
    text.lines()
        .filter(|l| !l.trim_start().starts_with('#'))
        .collect()
}

fn workflow() -> String {
    code_lines(&read(WORKFLOW)).join("\n")
}

/// The keys directly under a top-level key (`on:`, `permissions:`), as the
/// trimmed lines of its indented block; an inline value (`on: x`, `on: [x]`)
/// is returned as its one line.
fn top_level_block(text: &str, key: &str) -> Vec<String> {
    let head = format!("{key}:");
    let lines = code_lines(text);
    let start = lines
        .iter()
        .position(|l| l.starts_with(&head))
        .unwrap_or_else(|| panic!("{WORKFLOW}: no top-level `{key}:`"));
    let inline = lines[start][head.len()..].trim();
    if !inline.is_empty() {
        return vec![inline.to_string()];
    }
    lines[start + 1..]
        .iter()
        .take_while(|l| l.is_empty() || l.starts_with(' '))
        .filter(|l| !l.trim().is_empty())
        .map(|l| l.trim().to_string())
        .collect()
}

/// Byte offset of the first `needle` at or after `from`, panicking with what
/// was looked for: the order checks read as "A, then B".
fn after(text: &str, from: usize, needle: &str, what: &str) -> usize {
    text[from..]
        .find(needle)
        .map(|i| from + i)
        .unwrap_or_else(|| panic!("{WORKFLOW}: {what} (`{needle}`) missing or out of order"))
}

/// Offsets (from `from`) of an msiexec switch such as `/x` or `/f`: the
/// switch after a space, a quote or a comma, followed by a space, a quote or
/// (for `/f`) its lowercase repair letters (`/fa`, `/famus`).
fn switches(text: &str, from: usize, switch: &str) -> Vec<usize> {
    text[from..]
        .match_indices(switch)
        .map(|(i, _)| from + i)
        .filter(|&i| {
            let before = text[..i].chars().next_back();
            let next = text[i + switch.len()..].chars().next();
            matches!(before, Some(' ' | '"' | '\'' | ','))
                && next.is_some_and(|c| {
                    matches!(c, ' ' | '"' | '\'' | ',')
                        || (switch == "/f" && c.is_ascii_lowercase())
                })
        })
        .collect()
}

/// Started by hand only: `workflow_dispatch` and no automatic trigger, so a
/// push, a pull request or a schedule never spends a runner on it.
#[test]
fn installers_windows_is_started_by_hand_only() {
    let on = top_level_block(&read(WORKFLOW), "on");
    let keys: Vec<String> = on
        .iter()
        .filter(|l| !l.starts_with('-'))
        .map(|l| {
            l.trim_matches(|c| c == '[' || c == ']')
                .split(':')
                .next()
                .unwrap_or_default()
                .trim()
                .to_string()
        })
        .collect();
    // The direct children only: `workflow_dispatch:` may carry inputs below
    // it, more deeply indented, which the first line alone stands for.
    assert_eq!(
        keys.first().map(String::as_str),
        Some("workflow_dispatch"),
        "on: {on:?}"
    );
    for trigger in [
        "push",
        "pull_request",
        "pull_request_target",
        "schedule",
        "workflow_run",
        "repository_dispatch",
        "release",
    ] {
        assert!(
            !on.iter().any(|l| l.starts_with(&format!("{trigger}:"))
                || l.trim_matches(|c| c == '[' || c == ']')
                    .split(',')
                    .any(|t| t.trim() == trigger)),
            "on: has the automatic trigger `{trigger}`: {on:?}"
        );
    }
}

/// Read-only token: the job builds and installs on its own runner and
/// writes nothing to the repository.
#[test]
fn installers_windows_token_reads_contents_only() {
    let perms = top_level_block(&read(WORKFLOW), "permissions");
    assert_eq!(perms, vec!["contents: read".to_string()], "permissions:");
    let text = workflow();
    assert!(
        !text.contains(": write") && !text.contains("write-all"),
        "a permission grants write somewhere in the workflow"
    );
}

/// No secret reaches the job: a public repository's workflow needs none to
/// build and install, and none can leak into an uploaded log.
#[test]
fn installers_windows_uses_no_secrets() {
    let text = workflow();
    for needle in ["secrets.", "secrets[", "github.token", "GITHUB_TOKEN"] {
        assert!(!text.contains(needle), "the workflow uses `{needle}`");
    }
}

/// Every job runs on the pinned Windows Server 2025 image, never a moving
/// `windows-latest`.
#[test]
fn installers_windows_runs_on_windows_2025() {
    let text = workflow();
    let runs_on: Vec<&str> = text
        .lines()
        .filter_map(|l| l.trim().strip_prefix("runs-on:"))
        .map(str::trim)
        .collect();
    assert!(!runs_on.is_empty(), "no runs-on:");
    for r in &runs_on {
        assert_eq!(*r, "windows-2025", "runs-on: {r}");
    }
}

/// The prerequisites script with `-Minimal`, then the cargo phases in order:
/// configure with `--minimal` (no GPU SDK: the runner has none), build both
/// products, package both MSIs, install them.
#[test]
fn installers_windows_runs_the_minimal_phases_in_order() {
    let text = workflow();
    let prereq = text
        .find("00-install-prerequisites.ps1")
        .expect("the workflow runs scripts/00-install-prerequisites.ps1");
    let prereq_line = text[prereq..].lines().next().unwrap_or_default();
    assert!(
        prereq_line.contains("-Minimal"),
        "the prerequisites script runs without -Minimal: {prereq_line}"
    );
    let configure = after(&text, prereq, "xtask -- configure", "xtask configure");
    let configure_line = text[configure..].lines().next().unwrap_or_default();
    assert!(
        configure_line.contains("--minimal"),
        "xtask configure runs without --minimal: {configure_line}"
    );
    let build = after(&text, configure, "xtask -- build", "xtask build");
    let package = after(&text, build, "xtask -- package", "xtask package");
    after(&text, package, "xtask -- install", "xtask install");
    // Both products, the default: a product argument would leave one MSI out.
    for (at, phase) in [(build, "build"), (package, "package")] {
        let line = text[at..].lines().next().unwrap_or_default();
        assert!(
            !line.contains(" llama") && !line.contains(" framework"),
            "xtask {phase} names one product: {line}"
        );
    }
}

/// WiX v7 builds nothing until its EULA is accepted once per user, and the
/// prerequisites script never accepts it on anyone's behalf: the workflow
/// does, explicitly, before the package phase.
#[test]
fn installers_windows_accepts_the_wix_eula_before_packaging() {
    let text = workflow();
    let eula = after(&text, 0, "wix eula accept wix7", "the WiX EULA acceptance");
    after(
        &text,
        eula,
        "xtask -- package",
        "xtask package after the EULA",
    );
}

/// Test plan step 2, before anything is installed: the framework alone is
/// refused with exit code 1603, the launch condition on llama.cpp.
#[test]
fn installers_windows_checks_the_framework_alone_is_refused() {
    let text = workflow();
    let package = after(&text, 0, "xtask -- package", "xtask package");
    let install = after(&text, package, "xtask -- install", "xtask install");
    let between = &text[package..install];
    assert!(
        between.contains("msiexec") && between.contains("1603"),
        "no framework-alone install expecting exit 1603 between package and install"
    );
}

/// Test plan step 2 asks for "the message in the log" and nothing changed:
/// exit 1603 alone is any fatal error (a broken package too), so the refusal
/// is proven by the launch condition's message in the verbose log, and no
/// `%ProgramFiles%\private-ai` or `%ProgramData%\private-ai` is left.
#[test]
fn installers_windows_framework_alone_shows_the_launch_message_and_leaves_no_folder() {
    let text = workflow();
    let package = after(&text, 0, "xtask -- package", "xtask package");
    let install = after(&text, package, "xtask -- install", "xtask install");
    let between = &text[package..install];
    for needle in [
        "must be installed first",
        r"ProgramFiles\private-ai",
        r"ProgramData\private-ai",
    ] {
        assert!(
            between.contains(needle),
            "the framework-alone check does not look for `{needle}`"
        );
    }
}

/// After the install, from the installed folders (each product's
/// `InstallDir` in its HKLM key): `pai.exe --version` and
/// `llama-server.exe --version` run, both services run, the engine's
/// defaults are in `%ProgramData%\llama.cpp\config`, and the web service
/// answers on its default address.
#[test]
fn installers_windows_checks_the_installed_products() {
    let text = workflow();
    let install = after(&text, 0, "xtask -- install", "xtask install");
    let rest = &text[install..];
    for needle in [
        r"Software\private-ai",
        r"Software\llama.cpp",
        "InstallDir",
        "pai.exe",
        "llama-server.exe",
        "--version",
        "private-ai-manager",
        "private-ai-bff",
        "Running",
        "ProgramData",
        "server@default.ini",
        "presets.ini",
        "127.0.0.1:8590/api/version",
    ] {
        assert!(
            rest.contains(needle),
            "no check after the install names `{needle}`"
        );
    }
}

/// A repair of the installed products (`msiexec /f...`) after the checks and
/// before the uninstall, with the services running again after it.
#[test]
fn installers_windows_repairs_before_uninstalling() {
    let text = workflow();
    let install = after(&text, 0, "xtask -- install", "xtask install");
    let repair = *switches(&text, install, "/f")
        .first()
        .expect("no msiexec /f repair after the install");
    let uninstall = *switches(&text, repair, "/x")
        .first()
        .expect("no msiexec /x uninstall after the repair");
    assert!(
        text[repair..uninstall].contains("Running"),
        "the services are not checked running again after the repair"
    );
}

/// Test plan step 7: the engine's defaults are copied back only when missing.
/// The files exist before the repair, so seeing them after it proves nothing:
/// `presets.ini` is deleted before the repair (it must come back) and
/// `server@default.ini` changed (the change must stay).
#[test]
fn installers_windows_repair_restores_a_deleted_default_and_keeps_a_changed_one() {
    let text = workflow();
    let install = after(&text, 0, "xtask -- install", "xtask install");
    let repair = *switches(&text, install, "/f")
        .first()
        .expect("no msiexec /f repair after the install");
    let step_start = text[..repair].rfind("\n      - ").unwrap_or(install);
    let before_repair = &text[step_start..repair];
    assert!(
        before_repair.contains("Remove-Item") && before_repair.contains("presets.ini"),
        "presets.ini is not deleted before the repair"
    );
    assert!(
        ["Add-Content", "Set-Content", "Out-File"]
            .iter()
            .any(|w| before_repair.contains(w))
            && before_repair.contains("server@default.ini"),
        "server@default.ini is not changed before the repair"
    );
}

/// Test plan step 12: after the framework's uninstall the engine is still
/// installed (checked before the engine's own uninstall), and after both the
/// program folders under `%ProgramFiles%` are gone.
#[test]
fn installers_windows_checks_the_engine_survives_the_framework_and_the_folders_go() {
    let text = workflow();
    let install = after(&text, 0, "xtask -- install", "xtask install");
    let uninstalls = switches(&text, install, "/x");
    assert!(uninstalls.len() >= 2, "both products need an msiexec /x");
    let between = &text[uninstalls[0]..uninstalls[1]];
    assert!(
        between.contains(r"Software\llama.cpp"),
        "the engine is not checked still installed between the two uninstalls"
    );
    let left = &text[*uninstalls.last().unwrap()..];
    for needle in [r"ProgramFiles\private-ai", r"ProgramFiles\llama.cpp"] {
        assert!(
            left.contains(needle),
            "nothing after the last uninstall checks `{needle}` is gone"
        );
    }
}

/// Test plan step 12: the framework's machine tree is "still there with its
/// files" after a silent uninstall. Nothing writes into
/// `%ProgramData%\private-ai\config` on the runner, and Windows Installer
/// removes the empty folders it created, so a file is put there before the
/// first uninstall (a `bff.ini` with its `[Bff]` section, the file that
/// folder holds) and checked still there after the last.
#[test]
fn installers_windows_keeps_a_file_in_the_framework_config_across_the_uninstall() {
    let text = workflow();
    let install = after(&text, 0, "xtask -- install", "xtask install");
    let uninstalls = switches(&text, install, "/x");
    assert!(uninstalls.len() >= 2, "both products need an msiexec /x");
    let before = &text[install..uninstalls[0]];
    assert!(
        before.contains(r"private-ai\config") && before.contains("bff.ini"),
        "no bff.ini is put into ProgramData\\private-ai\\config before the first uninstall"
    );
    assert!(
        ["Set-Content", "Add-Content", "Out-File"]
            .iter()
            .any(|w| before.contains(w)),
        "bff.ini is not written before the first uninstall"
    );
    assert!(
        before.contains("[Bff]"),
        "the bff.ini written before the first uninstall has no [Bff] section"
    );
    let left = &text[*uninstalls.last().unwrap()..];
    assert!(
        left.contains("bff.ini") && left.contains("Test-Path"),
        "nothing after the last uninstall checks bff.ini is still there"
    );
}

/// Test plan step 12: the framework's own uninstall removes what it
/// installed, checked right after it and before the engine's uninstall, so
/// a leftover is pinned on the framework and not hidden by the engine's
/// removal: its two services, `HKLM\Software\private-ai` and
/// `%ProgramFiles%\private-ai`.
#[test]
fn installers_windows_checks_the_framework_is_gone_before_the_engine_uninstall() {
    let text = workflow();
    let install = after(&text, 0, "xtask -- install", "xtask install");
    let uninstalls = switches(&text, install, "/x");
    assert!(uninstalls.len() >= 2, "both products need an msiexec /x");
    let between = &text[uninstalls[0]..uninstalls[1]];
    for needle in [
        "private-ai-manager",
        "private-ai-bff",
        r"Software\private-ai",
        r"ProgramFiles\private-ai",
    ] {
        assert!(
            between.contains(needle),
            "nothing between the framework's and the engine's uninstall checks `{needle}` is gone"
        );
    }
}

/// Both products uninstalled silently, then what is left is checked: the
/// framework's services and every instance service gone, the machine data
/// trees under `%ProgramData%` as the plan says.
#[test]
fn installers_windows_uninstalls_both_and_checks_what_is_left() {
    let text = workflow();
    let install = after(&text, 0, "xtask -- install", "xtask install");
    let uninstalls = switches(&text, install, "/x");
    assert!(
        uninstalls.len() >= 2,
        "{} msiexec /x after the install, both products need one",
        uninstalls.len()
    );
    for &x in &uninstalls {
        let line = text[x..].lines().next().unwrap_or_default();
        assert!(line.contains("/qn"), "uninstall not silent: {line}");
    }
    let left = &text[*uninstalls.last().unwrap()..];
    for needle in [
        "private-ai-manager",
        "private-ai-bff",
        "llama.cpp_",
        "ProgramData",
    ] {
        assert!(
            left.contains(needle),
            "nothing after the last uninstall checks `{needle}`"
        );
    }
}

/// Every msiexec the workflow runs itself keeps a verbose log (`/l*v`),
/// which the artifact step uploads; xtask install writes its own.
#[test]
fn installers_windows_logs_every_msiexec() {
    let text = workflow();
    let calls: Vec<&str> = text
        .lines()
        .filter(|l| l.to_ascii_lowercase().contains("msiexec"))
        .collect();
    assert!(!calls.is_empty(), "the workflow runs no msiexec of its own");
    for l in calls {
        assert!(l.contains("/l*v"), "msiexec without /l*v: {l}");
    }
}

/// The MSIs and the logs (the workflow's own and xtask install's
/// `build\windows\msi-*.log`) are uploaded whether the job passed or failed.
#[test]
fn installers_windows_uploads_msis_and_logs_always() {
    let text = workflow();
    let at = after(&text, 0, "actions/upload-artifact@", "the artifact upload");
    // The step around the `uses:` line: from the `- ` that opens it to the
    // next step or the end.
    let open = text[..at].rfind("\n      - ").unwrap_or(0);
    let close = text[at..]
        .find("\n      - ")
        .map(|i| at + i)
        .unwrap_or(text.len());
    let step = &text[open..close];
    assert!(step.contains("if: always()"), "upload not always(): {step}");
    assert!(step.contains(".msi"), "upload takes no .msi: {step}");
    assert!(step.contains(".log"), "upload takes no .log: {step}");
    assert!(
        step.contains("msi-") || text.contains(r"build\windows") || text.contains("build/windows"),
        "xtask install's logs under build/windows are not uploaded: {step}"
    );
}

/// The runner's disk is the limit of a llama.cpp + Rust build: the free
/// space is printed, so a failed run shows whether it ran out.
#[test]
fn installers_windows_reports_free_disk_space() {
    let text = workflow();
    assert!(
        ["Get-PSDrive", "Get-Volume", "fsutil volume diskfree"]
            .iter()
            .any(|n| text.contains(n)),
        "the workflow never prints the free disk space"
    );
}

/// `-Minimal` on the prerequisites script: the toolchain without the GPU SDKs,
/// for the CI-only minimal build. Nothing is looked up at AMD, NVIDIA or
/// LunarG, so no ROCm, CUDA or Vulkan dist is installed.
#[test]
fn prerequisites_script_has_a_minimal_switch_without_gpu_dists() {
    let script = read(PREREQ);
    let param_end = script.find("\n)").expect("the script's param() block");
    assert!(
        script[..param_end].contains("[switch]$Minimal"),
        "{PREREQ}: no [switch]$Minimal in param()"
    );
    let lines: Vec<&str> = script.lines().collect();
    // Each call site of a vendor lookup (not its definition) carries
    // `$Minimal` on its own line or sits inside an `if (-not $Minimal) {`.
    let gated_blocks: Vec<(usize, usize)> = lines
        .iter()
        .enumerate()
        .filter(|(_, l)| l.contains("if (-not $Minimal)") && l.trim_end().ends_with('{'))
        .map(|(start, _)| {
            let mut depth = 0i32;
            for (i, l) in lines.iter().enumerate().skip(start) {
                depth += l.matches('{').count() as i32 - l.matches('}').count() as i32;
                if depth <= 0 {
                    return (start, i);
                }
            }
            (start, lines.len())
        })
        .collect();
    for lookup in [
        "Get-RocmLatestPublished",
        "Get-CudaLatestPublished",
        "Get-VulkanLatestPublished",
    ] {
        let calls: Vec<usize> = lines
            .iter()
            .enumerate()
            .filter(|(_, l)| {
                l.contains(lookup)
                    && !l.trim_start().starts_with("function ")
                    && !l.trim_start().starts_with('#')
            })
            .map(|(i, _)| i)
            .collect();
        assert!(!calls.is_empty(), "{PREREQ}: {lookup} is never called");
        for i in calls {
            let gated =
                lines[i].contains("$Minimal") || gated_blocks.iter().any(|&(s, e)| s < i && i < e);
            assert!(
                gated,
                "{PREREQ}:{}: {lookup} runs under -Minimal: {}",
                i + 1,
                lines[i].trim()
            );
        }
    }
}

/// The value after `option` on `line`: the next whitespace-separated token,
/// its quotes removed; `None` when the option is absent.
fn option_value(line: &str, option: &str) -> Option<String> {
    let mut tokens = line.split_whitespace();
    tokens.find(|t| *t == option)?;
    tokens
        .next()
        .map(|v| v.trim_matches(|c| c == '"' || c == '\'').to_string())
}

/// The runner has no `E:` drive, the default dist root of
/// `installer/dist-sources.psd1` (run 43644178525 stopped in the
/// prerequisites step at `Join-Path $DistRoot`): every prerequisites call
/// passes `-DistRoot` and `xtask configure` passes `--dist-root`, both naming
/// the same directory under `$env:RUNNER_TEMP`.
#[test]
fn installers_windows_points_the_dist_root_at_the_runner_temp() {
    // Both options exist with these spellings.
    let script = read(PREREQ);
    let param_end = script.find("\n)").expect("the script's param() block");
    assert!(
        script[..param_end].contains("[string]$DistRoot"),
        "{PREREQ}: no [string]$DistRoot in param()"
    );
    let main = read("xtask/src/main.rs");
    let configure = main
        .find("    Configure {")
        .expect("xtask/src/main.rs: no Configure phase");
    let block_end = configure
        + main[configure..]
            .find("\n    },")
            .expect("end of Configure");
    let block = &main[configure..block_end];
    let field = block
        .find("dist_root: Option<PathBuf>")
        .expect("xtask configure has no dist_root field (--dist-root)");
    assert!(
        block[..field]
            .trim_end()
            .lines()
            .last()
            .is_some_and(|l| l.contains("#[arg(long)]")),
        "xtask configure's dist_root is not a plain --dist-root option"
    );

    let text = workflow();
    let prereq_lines: Vec<&str> = text
        .lines()
        .filter(|l| l.contains("00-install-prerequisites.ps1"))
        .collect();
    assert!(
        !prereq_lines.is_empty(),
        "the workflow never runs the prerequisites script"
    );
    let configure_line = text
        .lines()
        .find(|l| l.contains("xtask -- configure"))
        .expect("the workflow runs xtask configure");
    let root = option_value(configure_line, "--dist-root")
        .unwrap_or_else(|| panic!("xtask configure runs without --dist-root: {configure_line}"));
    assert!(
        root.starts_with("$env:RUNNER_TEMP\\") && root.len() > "$env:RUNNER_TEMP\\".len(),
        "--dist-root is not a directory under $env:RUNNER_TEMP: {root}"
    );
    for line in prereq_lines {
        assert_eq!(
            option_value(line, "-DistRoot").as_deref(),
            Some(root.as_str()),
            "the prerequisites call does not pass -DistRoot {root}: {}",
            line.trim()
        );
    }
}

/// Under `-Minimal` the script never installs or upgrades
/// `Microsoft.PowerShell`, the winget package of the shell running it: run
/// 43644571519 upgraded the step's own pwsh.exe (7.6.9 to 7.6.10) and the
/// step ended with exit code 1 right after. The entry stays in
/// `$wingetPackages` for a local run and is dropped from it, under
/// `$Minimal`, before the list is first read (the pre-state capture that
/// splits it into the packages to install and to upgrade).
#[test]
fn prerequisites_script_leaves_powershell_alone_under_minimal() {
    let script = read(PREREQ);
    let lines: Vec<&str> = script.lines().collect();
    let code = |l: &str| !l.trim_start().starts_with('#');
    let list_start = lines
        .iter()
        .position(|l| l.trim_start().starts_with("$wingetPackages = @("))
        .unwrap_or_else(|| panic!("{PREREQ}: no `$wingetPackages = @(` list"));
    let list_end = list_start
        + lines[list_start..]
            .iter()
            .position(|l| l.trim() == ")")
            .unwrap_or_else(|| panic!("{PREREQ}: the `$wingetPackages` list never closes"));
    assert!(
        lines[list_start..list_end]
            .iter()
            .any(|l| code(l) && l.contains("'Microsoft.PowerShell'")),
        "{PREREQ}: Microsoft.PowerShell left the winget list: a local run still installs it"
    );
    let first_use = list_end
        + lines[list_end..]
            .iter()
            .position(|l| code(l) && l.contains("foreach ($p in $wingetPackages)"))
            .unwrap_or_else(|| panic!("{PREREQ}: `$wingetPackages` is never iterated"));
    let dropped = lines[list_end..first_use].iter().any(|l| {
        code(l)
            && l.contains("$Minimal")
            && l.contains("$wingetPackages =")
            && l.contains("Microsoft.PowerShell")
    });
    assert!(
        dropped,
        "{PREREQ}: under -Minimal Microsoft.PowerShell is not dropped from `$wingetPackages` \
         before line {}: the script upgrades the pwsh.exe running it",
        first_use + 1
    );
}
