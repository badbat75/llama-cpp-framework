//! The macOS installer workflow (`.github/workflows/installers-macos.yml`),
//! pinned as text: a workflow runs only on GitHub, so what it must keep doing
//! is checked here, as `ci_windows.rs` does for the Windows one. It builds the
//! CI-only minimal llama.cpp (`xtask configure --minimal`: Metal only, no
//! Vulkan SDK), runs `pkgbuild` for the four `.pkg`, installs them on the
//! runner with `xtask install`, checks the installed trees, reinstalls the
//! same version, and uploads the `.pkg` files. No uninstall: the project
//! documents none for a `.pkg` (`docs/build-macos.md`). Started by hand only:
//! every run needs the user's go-ahead.

use std::path::{Path, PathBuf};

const WORKFLOW: &str = ".github/workflows/installers-macos.yml";
/// The engine's installed server, split so the names lint does not take
/// it for the old tree.
const ENGINE_SERVER_VERSION: &str = concat!("/opt/llama", ".cpp/bin/llama-server --version");

fn repo() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("xtask sits in the repo root")
        .to_path_buf()
}

fn read(rel: &str) -> String {
    std::fs::read_to_string(repo().join(rel)).unwrap_or_else(|e| panic!("{rel}: {e}"))
}

/// The workflow's lines without comment lines (YAML's and bash's `#`), so a
/// comment naming a trigger or a secret does not count.
fn code_lines(text: &str) -> Vec<&str> {
    text.lines()
        .filter(|l| !l.trim_start().starts_with('#'))
        .collect()
}

fn workflow() -> String {
    code_lines(&read(WORKFLOW)).join("\n")
}

/// The trimmed lines of the indented block under a top-level key (`on:`,
/// `permissions:`); an inline value (`on: x`) is returned as its one line.
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

/// Offsets of every `xtask install` the workflow runs.
fn installs(text: &str) -> Vec<usize> {
    text.match_indices("xtask -- install")
        .map(|(i, _)| i)
        .collect()
}

/// The step around `at`: from the `- ` that opens it to the next step or the
/// end of the text.
fn step_around(text: &str, at: usize) -> &str {
    let open = text[..at].rfind("\n      - ").unwrap_or(0);
    let close = text[at..]
        .find("\n      - ")
        .map(|i| at + i)
        .unwrap_or(text.len());
    &text[open..close]
}

/// Started by hand only: `workflow_dispatch` and no automatic trigger.
#[test]
fn installers_macos_is_started_by_hand_only() {
    let on = top_level_block(&read(WORKFLOW), "on");
    let first = on
        .first()
        .map(|l| l.split(':').next().unwrap_or_default().trim().to_string());
    assert_eq!(first.as_deref(), Some("workflow_dispatch"), "on: {on:?}");
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

/// Read-only token: the job writes nothing to the repository.
#[test]
fn installers_macos_token_reads_contents_only() {
    let perms = top_level_block(&read(WORKFLOW), "permissions");
    assert_eq!(perms, vec!["contents: read".to_string()], "permissions:");
    let text = workflow();
    assert!(
        !text.contains(": write") && !text.contains("write-all"),
        "a permission grants write somewhere in the workflow"
    );
}

/// No secret reaches the job.
#[test]
fn installers_macos_uses_no_secrets() {
    let text = workflow();
    for needle in ["secrets.", "secrets[", "github.token", "GITHUB_TOKEN"] {
        assert!(!text.contains(needle), "the workflow uses `{needle}`");
    }
}

/// The pinned arm64 image `macos-15` (Apple Silicon, the only architecture
/// `configure` accepts), never a moving `macos-latest` or an Intel image,
/// with a job timeout so a hung step cannot hold the runner for the
/// default six hours.
#[test]
fn installers_macos_runs_on_macos_15_with_a_timeout() {
    let text = workflow();
    let runs_on: Vec<&str> = text
        .lines()
        .filter_map(|l| l.trim().strip_prefix("runs-on:"))
        .map(str::trim)
        .collect();
    assert!(!runs_on.is_empty(), "no runs-on:");
    for r in &runs_on {
        assert_eq!(*r, "macos-15", "runs-on: {r}");
    }
    let timeouts: Vec<u32> = text
        .lines()
        .filter_map(|l| l.trim().strip_prefix("timeout-minutes:"))
        .map(|v| {
            v.trim()
                .parse()
                .unwrap_or_else(|_| panic!("timeout-minutes is not a number: {v}"))
        })
        .collect();
    assert_eq!(
        timeouts.len(),
        runs_on.len(),
        "a job has no timeout-minutes"
    );
    assert!(
        timeouts.iter().all(|&t| t > 0 && t < 360),
        "timeout-minutes: {timeouts:?}"
    );
}

/// The actions are pinned as in the Windows workflow: `checkout@v5` without
/// persisted credentials, `upload-artifact@v7`, and no action on a branch.
#[test]
fn installers_macos_pins_its_actions() {
    let text = workflow();
    let uses: Vec<&str> = text
        .lines()
        .filter_map(|l| {
            l.trim()
                .trim_start_matches("- ")
                .strip_prefix("uses:")
                .map(str::trim)
        })
        .collect();
    assert!(
        uses.contains(&"actions/checkout@v5"),
        "no actions/checkout@v5: {uses:?}"
    );
    assert!(
        uses.contains(&"actions/upload-artifact@v7"),
        "no actions/upload-artifact@v7: {uses:?}"
    );
    for u in &uses {
        assert!(
            u.starts_with("actions/checkout@v5") || u.starts_with("actions/upload-artifact@v7"),
            "an action outside the two pinned ones: {u}"
        );
    }
    let checkout = after(&text, 0, "actions/checkout@v5", "the checkout");
    assert!(
        step_around(&text, checkout).contains("persist-credentials: false"),
        "the checkout persists its credentials"
    );
}

/// The prerequisites script, then the cargo phases in order: configure with
/// `--minimal` (Metal only: the runner has no Vulkan SDK), build both
/// products, package both, install. The script skips the Vulkan leg
/// (`--skip vulkan`, which it passes on to `xtask prereq`): the minimal build
/// does not use it and its download needs 4 GiB.
#[test]
fn installers_macos_runs_the_minimal_phases_in_order() {
    let text = workflow();
    let prereq = after(
        &text,
        0,
        "scripts/00-install-prerequisites.sh",
        "the prerequisites script",
    );
    let prereq_line = text[prereq..].lines().next().unwrap_or_default();
    let skip = prereq_line
        .split_whitespace()
        .skip_while(|t| *t != "--skip")
        .nth(1)
        .or_else(|| {
            prereq_line
                .split_whitespace()
                .find_map(|t| t.strip_prefix("--skip="))
        })
        .unwrap_or_else(|| panic!("the prerequisites script runs without --skip: {prereq_line}"));
    assert!(
        skip.split(',').any(|leg| leg == "vulkan"),
        "the prerequisites script does not skip the Vulkan SDK: {prereq_line}"
    );
    // `xtask prereq` on macOS knows the leg by that name.
    assert!(
        read("xtask/src/macos/prereq.rs").contains("skipped(\"vulkan\")"),
        "xtask/src/macos/prereq.rs has no `vulkan` leg to skip"
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
    for (at, phase) in [(build, "build"), (package, "package")] {
        let line = text[at..].lines().next().unwrap_or_default();
        assert!(
            !line.contains(" llama") && !line.contains(" framework"),
            "xtask {phase} names one product: {line}"
        );
    }
}

/// The package step checks what `pkgbuild` wrote: the four `.pkg` of a
/// minimal build, by name (the engine's carries `-minimal`), and no
/// `llama.cpp-vulkan` one.
#[test]
fn installers_macos_checks_the_four_minimal_packages() {
    let text = workflow();
    let package = after(&text, 0, "xtask -- package", "xtask package");
    let install = after(&text, package, "xtask -- install", "xtask install");
    let between = &text[package..install];
    for needle in [
        "llama.cpp-v",
        "-minimal-arm64.pkg",
        "private-ai-framework-config-v",
        "private-ai-framework-cli-v",
        "private-ai-framework-core-v",
        "llama.cpp-vulkan",
    ] {
        assert!(
            between.contains(needle),
            "the package check between package and install does not name `{needle}`"
        );
    }
}

/// `xtask install` runs as the runner's user: it calls `sudo installer`
/// itself (`xtask/src/macos/install.rs`), and a `sudo cargo` would build as
/// root into the user's target directory.
#[test]
fn installers_macos_runs_xtask_install_without_sudo() {
    let text = workflow();
    let calls = installs(&text);
    assert!(!calls.is_empty(), "the workflow never runs xtask install");
    for at in calls {
        let line_start = text[..at].rfind('\n').map(|i| i + 1).unwrap_or(0);
        let line = &text[line_start..at];
        assert!(!line.contains("sudo"), "xtask install under sudo: {line}");
    }
}

/// After the first install: the engine's and the framework's executables
/// run from their installed prefixes, `pai` reports the root `Cargo.toml`'s
/// version, the `/usr/local/bin` links point into the prefixes, and
/// `pkgutil --pkgs` lists the four receipts.
#[test]
fn installers_macos_checks_the_installed_products() {
    let text = workflow();
    let calls = installs(&text);
    assert!(
        calls.len() >= 2,
        "the workflow needs an install and a reinstall"
    );
    let checks = &text[calls[0]..calls[1]];
    for needle in [
        ENGINE_SERVER_VERSION,
        "/opt/private-ai/bin/pai --version",
        "Cargo.toml",
        "/usr/local/bin/llama-server",
        "/usr/local/bin/pai",
        "/usr/local/bin/pai-config",
        "readlink",
        "pkgutil --pkgs",
        "org.privateai.llama.cpp",
        "org.privateai.framework.config",
        "org.privateai.framework.cli",
        "org.privateai.framework.core",
    ] {
        assert!(
            checks.contains(needle),
            "no check between the install and the reinstall names `{needle}`"
        );
    }
}

/// A reinstall of the same version, the "upgrade over the same version" of
/// `PLAN.md` step 12. The files are there before it, so seeing them after
/// proves nothing: an installed file is removed first (as root: the trees
/// are root's) and checked back after the reinstall, with both
/// `--version` checks again.
#[test]
fn installers_macos_reinstall_restores_a_removed_file() {
    let text = workflow();
    let calls = installs(&text);
    assert!(
        calls.len() >= 2,
        "the workflow needs an install and a reinstall"
    );
    let reinstall = calls[1];
    let step_start = text[..reinstall].rfind("\n      - ").unwrap_or(calls[0]);
    let before = &text[step_start..reinstall];
    let rm = before
        .lines()
        .find(|l| l.contains("sudo rm ") && (l.contains("/opt/") || l.contains("/usr/local/")))
        .unwrap_or_else(|| {
            panic!("no installed file is removed (sudo rm) in the step before the reinstall")
        });
    // The removed path: the last token of the rm line.
    let path = rm
        .split_whitespace()
        .last()
        .unwrap()
        .trim_matches(|c| c == '"' || c == '\'');
    let rest = &text[reinstall..];
    assert!(
        rest.contains(&format!("-e {path}"))
            || rest.contains(&format!("-f {path}"))
            || rest.contains(&format!("-L {path}"))
            || rest.contains(&format!("-x {path}")),
        "nothing after the reinstall checks that {path} is back"
    );
    for needle in [ENGINE_SERVER_VERSION, "/opt/private-ai/bin/pai --version"] {
        assert!(
            rest.contains(needle),
            "nothing after the reinstall runs `{needle}`"
        );
    }
}

/// The `.pkg` files are uploaded whether the job passed or failed.
#[test]
fn installers_macos_uploads_the_packages_always() {
    let text = workflow();
    let at = after(&text, 0, "actions/upload-artifact@", "the artifact upload");
    let step = step_around(&text, at);
    assert!(step.contains("if: always()"), "upload not always(): {step}");
    assert!(
        step.contains("dist/*.pkg"),
        "upload takes no dist/*.pkg: {step}"
    );
}

/// The runner's disk is the limit of a llama.cpp + Rust build: the free
/// space is printed, so a failed run shows whether it ran out.
#[test]
fn installers_macos_reports_free_disk_space() {
    let text = workflow();
    assert!(
        text.lines().any(|l| {
            let l = l.trim();
            l.strip_prefix("run:")
                .unwrap_or(l)
                .trim()
                .starts_with("df ")
        }),
        "the workflow never prints the free disk space (df)"
    );
}
