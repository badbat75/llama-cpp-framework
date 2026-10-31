//! Prerequisites phase on macOS, the xtask half of
//! scripts/00-install-prerequisites.sh (the script installs the Xcode Command Line
//! Tools check, the Homebrew packages and Rust first, natively). Idempotent:
//!
//!   - sccache from its GitHub release into ~/.local/bin.
//!   - the newest Vulkan SDK (LunarG), into <root>/VulkanSDK/<version>/macos,
//!     user scope: the installer app runs unattended with copy_only=1, so
//!     nothing is written to /usr/local and no sudo is needed.
//!   - llama.cpp release check and a report.
//!
//! Metal needs nothing: it is part of the system.

use super::config;
use crate::common::dists::{dist_version, Dists};
use crate::common::prereq::{download, fetch, fetch_json, finish, llama_check, need_space, or_none, sccache, write_marker, Report};
use crate::common::ui;
use crate::common::util::{self, cmd, succeeds};
use anyhow::Result;
use std::path::{Path, PathBuf};

pub fn run(dist_root: Option<PathBuf>, skip: &[String]) -> Result<()> {
    let recorded = config::load_optional().ok().flatten();
    let recorded_root = recorded.as_ref().map(|c| c.dist_root.clone()).unwrap_or_default();
    let dists = Dists::resolve(dist_root.as_deref(), Some(&recorded_root))?;
    let skipped = |leg: &str| skip.iter().any(|s| s == leg);
    let mut r = Report { lines: Vec::new(), gaps: Vec::new() };

    ui::say("");
    ui::head("  Dists and tools (xtask)");
    ui::head(format!("  dist root: {}", dists.root.display()));
    ui::say("");

    if skipped("sccache") {
        r.note("sccache        : skipped");
    } else {
        sccache(&dists, &mut r);
    }
    if skipped("vulkan") {
        r.note("Vulkan SDK     : skipped");
    } else {
        vulkan(&dists, &mut r);
    }
    llama_check(recorded.as_ref().map(|c| c.llama_cpp_dir.as_str()), &mut r);

    finish(&r, &dists, &[dists.vulkan_root()], dist_root.is_some(), &recorded_root)
}

/// The installer app's executable: the single file in <app>/Contents/MacOS.
fn installer_exe(dir: &Path) -> Option<PathBuf> {
    let app = std::fs::read_dir(dir)
        .ok()?
        .flatten()
        .map(|e| e.path())
        .find(|p| p.extension().map(|e| e == "app").unwrap_or(false))?;
    std::fs::read_dir(app.join("Contents/MacOS")).ok()?.flatten().map(|e| e.path()).find(|p| util::is_executable(p))
}

fn vulkan(dists: &Dists, r: &mut Report) {
    ui::head("  Vulkan SDK");
    let s = &dists.sources.vulkan;
    let root = dists.vulkan_root();
    let have_dir = dists.find_vulkan();
    let have = have_dir.as_deref().map(dist_version).unwrap_or_default();
    let latest: String = fetch(&s.macos.latest_url)
        .map(|t| t.chars().filter(|c| !c.is_whitespace()).collect())
        .unwrap_or_default();
    if !util::is_dotted_version(&latest) {
        r.note(format!("Vulkan SDK     : {} (LunarG unreachable, kept)", if have.is_empty() { "not installed" } else { &have }));
        if have.is_empty() {
            r.gap(format!("Vulkan: no SDK under {} and LunarG could not be reached", root.display()));
        }
        ui::say("");
        return;
    }
    if !have.is_empty() && !util::version_gt(&latest, &have) {
        ui::ok(format!("    {have} is current ({})", have_dir.unwrap().display()));
        r.note(format!("Vulkan SDK     : {have} (current)"));
        ui::say("");
        return;
    }
    let dest = root.join(&latest).join(util::OS_DIR);
    let url = s.macos.zip.replace("{version}", &latest);
    let file = util::basename(&url).to_string();
    let zip = root.join(".downloads").join(&file);
    let sha = fetch_json(&s.macos.files_json).and_then(|j| {
        j["mac"][&latest]["files"]
            .as_array()?
            .iter()
            .find(|f| f["file_name"].as_str() == Some(&file))
            .and_then(|f| f["sha"].as_str())
            .map(str::to_string)
    });
    ui::say(format!("    {} -> {latest}, into {}", or_none(&have), dest.display()));
    let kept = if have.is_empty() { "nothing" } else { &have };
    let Some(sha) = sha else {
        ui::err(format!("    no sha256 for {file} in files.json: not installing an unverified SDK"));
        r.note(format!("Vulkan SDK     : install of {latest} FAILED (no checksum), {kept} kept"));
        r.gap("Vulkan: re-run this script later");
        ui::say("");
        return;
    };
    // The zip holds the installer app; it is unpacked beside the download
    // and run unattended into the dist dir, which it creates (an interrupted
    // install of the same version is removed first).
    let unpacked = root.join(".downloads").join(format!("vulkansdk-{latest}"));
    let ok = need_space(&root, 4)
        && download(&url, &zip, Some(&sha))
        && util::remove_all(&unpacked).is_ok()
        && succeeds(cmd("ditto").args(["-x", "-k"]).arg(&zip).arg(&unpacked))
        && util::remove_all(&dest).is_ok()
        && match installer_exe(&unpacked) {
            Some(exe) => succeeds(
                cmd(exe)
                    .arg("--root")
                    .arg(&dest)
                    .args(["--accept-licenses", "--default-answer", "--confirm-command", "install", "copy_only=1"]),
            ),
            None => {
                ui::err(format!("    no installer app in {file}"));
                false
            }
        }
        && write_marker(&dest, &s.marker, &latest)
        && dists.find_vulkan().as_deref() == Some(dest.join("macOS").as_path());
    let _ = util::remove_all(&unpacked);
    if ok {
        let _ = std::fs::remove_file(&zip);
        ui::ok(format!("    installed {latest}"));
        r.note(format!("Vulkan SDK     : {} -> {latest} (re-run cargo run -p xtask -- build)", or_none(&have)));
    } else {
        r.note(format!("Vulkan SDK     : install of {latest} FAILED, {kept} kept"));
        r.gap(format!("Vulkan: re-run this script (the download resumes from {})", zip.display()));
    }
    ui::say("");
}
