//! Prerequisites phase, the xtask half of scripts/00-install-prerequisites.sh (the
//! script installs the apt packages and Rust first, natively: this crate
//! cannot build before they exist). Idempotent, safe to re-run any time:
//!
//!   - sccache from its GitHub release into ~/.local/bin.
//!   - the newest STABLE ROCm/TheRock dist, CUDA Toolkit, NCCL (optional, the
//!     one matching that toolkit's CUDA major) and Vulkan SDK, each into
//!     <root>/<version>/linux (installer/dist-sources.toml says where each is
//!     looked up and installed; nothing is pinned). User scope, no root: the
//!     roots must be writable by the current user.
//!   - llama.cpp release check and a report.
//!
//! A source that cannot be reached keeps what is installed and the report
//! says so; nothing is ever downgraded or deleted. Downloads resume: an
//! interrupted run picks the partial file up from <root>/.downloads.

use super::config;
use crate::common::dists::{dist_version, Dists};
use crate::common::prereq::{
    download, fetch, fetch_json, finish, fresh_dir, llama_check, need_space, or_none, sccache, tar, write_marker, Report,
};
use crate::common::ui;
use crate::common::util::{self, cmd, run as run_cmd};
use anyhow::Result;
use std::path::PathBuf;

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
    if skipped("rocm") {
        r.note("ROCm (TheRock) : skipped");
    } else {
        rocm(&dists, &mut r);
    }
    if skipped("cuda") {
        r.note("CUDA Toolkit   : skipped");
    } else {
        cuda(&dists, &mut r);
    }
    if skipped("nccl") {
        r.note("NCCL           : skipped");
    } else {
        nccl(&dists, &mut r);
    }
    if skipped("vulkan") {
        r.note("Vulkan SDK     : skipped");
    } else {
        vulkan(&dists, &mut r);
    }
    llama_check(recorded.as_ref().map(|c| c.llama_cpp_dir.as_str()), &mut r);

    finish(&r, &dists, &[dists.rocm_root(), dists.cuda_root(), dists.nccl_root(), dists.vulkan_root()], dist_root.is_some(), &recorded_root)
}

// ── ROCm / HIP (TheRock dist) ───────────────────────────────────────────
/// (version, url) of the newest stable in the indexes.
fn rocm_latest(dists: &Dists) -> Option<(String, String)> {
    let s = &dists.sources.rocm;
    let prefix = format!("therock-dist-{}-{}-", util::OS_DIR, s.flavor);
    let mut best: Option<(String, String)> = None;
    for idx in &s.indexes {
        let Some(page) = fetch(idx) else { continue };
        for (ver, name) in util::scan_versions(&page, &prefix, ".tar.gz") {
            if best.as_ref().map(|(b, _)| util::version_gt(&ver, b)).unwrap_or(true) {
                best = Some((ver, format!("{idx}{name}")));
            }
        }
    }
    best
}

fn rocm(dists: &Dists, r: &mut Report) {
    ui::head("  ROCm / HIP (TheRock dist)");
    let root = dists.rocm_root();
    let have_dir = dists.find_rocm();
    let have = have_dir.as_deref().map(dist_version).unwrap_or_default();
    match rocm_latest(dists) {
        None => {
            r.note(format!("ROCm (TheRock) : {} (indexes unreachable, kept)", if have.is_empty() { "not installed" } else { &have }));
            if have.is_empty() {
                r.gap(format!("ROCm: no dist under {} and the AMD indexes could not be read", root.display()));
            }
        }
        Some((latest, _)) if !have.is_empty() && !util::version_gt(&latest, &have) => {
            ui::ok(format!("    {have} is current ({})", have_dir.unwrap().display()));
            r.note(format!("ROCm (TheRock) : {have} (current)"));
        }
        Some((latest, url)) => {
            let dest = root.join(&latest).join(util::OS_DIR);
            let tarball = root.join(".downloads").join(util::basename(&url));
            ui::say(format!("    {} -> {latest}, into {}", or_none(&have), dest.display()));
            // A tree without the marker is an interrupted extract of this
            // same version: cleared and redone. A complete one never reaches
            // this point.
            let ok = need_space(&root, 45)
                && download(&url, &tarball, None)
                && fresh_dir(&dest)
                && {
                    ui::dim("    extracting (several minutes)");
                    tar("-xzf", &tarball, &dest, false)
                }
                && write_marker(&dest, &dists.sources.rocm.marker, &latest)
                && dists.find_rocm().as_deref() == Some(dest.as_path());
            if ok {
                let _ = std::fs::remove_file(&tarball);
                ui::ok(format!("    installed {latest}"));
                r.note(format!("ROCm (TheRock) : {} -> {latest} (re-run cargo run -p xtask -- configure, then cargo run -p xtask -- build)", or_none(&have)));
            } else {
                r.note(format!("ROCm (TheRock) : install of {latest} FAILED, {} kept", if have.is_empty() { "nothing" } else { &have }));
                r.gap(format!("ROCm: re-run this script (the download resumes from {})", tarball.display()));
            }
        }
    }
    ui::say("");
}

// ── CUDA Toolkit (redist archives) ──────────────────────────────────────
fn cuda(dists: &Dists, r: &mut Report) {
    ui::head("  CUDA Toolkit");
    let s = &dists.sources.cuda;
    let root = dists.cuda_root();
    let have_dir = dists.find_cuda();
    let have = have_dir.as_deref().map(dist_version).unwrap_or_default();
    let latest = fetch(&s.redist_index)
        .map(|page| util::scan_versions(&page, "redistrib_", ".json"))
        .and_then(|v| v.into_iter().map(|(ver, _)| ver).max_by(|a, b| util::vercmp(a, b)))
        .unwrap_or_default();
    if latest.is_empty() {
        r.note(format!("CUDA Toolkit   : {} (redist index unreachable, kept)", if have.is_empty() { "not installed" } else { &have }));
        if have.is_empty() {
            r.gap(format!("CUDA: no toolkit under {} and the NVIDIA redist index could not be read", root.display()));
        }
    } else if !have.is_empty() && !util::version_gt(&latest, &have) {
        ui::ok(format!("    {have} is current ({})", have_dir.unwrap().display()));
        r.note(format!("CUDA Toolkit   : {have} (current)"));
    } else {
        let dest = root.join(&latest).join(util::OS_DIR);
        let cache = root.join(".downloads").join(&latest);
        ui::say(format!("    {} -> {latest}, into {}", or_none(&have), dest.display()));
        let manifest = fetch_json(&format!("{}redistrib_{latest}.json", s.redist_index));
        let mut ok = manifest.is_some() && need_space(&root, 8) && fresh_dir(&dest);
        if ok {
            let manifest = manifest.unwrap();
            for comp in &s.linux.components {
                let entry = &manifest[comp][&s.linux.platform];
                let (Some(rel), Some(sha)) = (entry["relative_path"].as_str(), entry["sha256"].as_str()) else {
                    ui::err(format!("    component {comp} is not in redistrib_{latest}.json for {}", s.linux.platform));
                    ok = false;
                    break;
                };
                // Every archive unpacks as <name>-archive/{bin,include,lib,...};
                // stripping that one level merges them into a single toolkit
                // prefix.
                let archive = cache.join(util::basename(rel));
                if !(download(&format!("{}{rel}", s.redist_index), &archive, Some(sha)) && tar("-xJf", &archive, &dest, true)) {
                    ok = false;
                    break;
                }
            }
        }
        // The redist archives carry lib/, while nvcc links its own runtime
        // from <toolkit>/lib64 (the layout of the .run installer): alias it.
        ok = ok
            && {
                let _ = util::remove_all(&dest.join("lib64"));
                std::os::unix::fs::symlink("lib", dest.join("lib64")).is_ok()
            }
            && write_marker(&dest, &s.marker, &latest)
            && dists.find_cuda().as_deref() == Some(dest.as_path());
        if ok {
            let _ = util::remove_all(&cache);
            ui::ok(format!("    installed {latest}"));
            r.note(format!("CUDA Toolkit   : {} -> {latest} (re-run cargo run -p xtask -- build)", or_none(&have)));
        } else {
            r.note(format!("CUDA Toolkit   : install of {latest} FAILED, {} kept", if have.is_empty() { "nothing" } else { &have }));
            r.gap(format!("CUDA: re-run this script (finished downloads are reused from {})", cache.display()));
        }
    }
    ui::say("");
}

// ── NCCL (PyPI wheel) ───────────────────────────────────────────────────
// After the CUDA leg on purpose: the wheel is chosen by the CUDA major of the
// toolkit that is installed NOW. A dist built for another major (its marker
// says which) does not count as installed.
fn nccl(dists: &Dists, r: &mut Report) {
    ui::head("  NCCL");
    let s = &dists.sources.nccl;
    let root = dists.nccl_root();
    let Some(cuda_dir) = dists.find_cuda() else {
        r.note("NCCL           : no CUDA Toolkit to match, nothing done");
        ui::say("");
        return;
    };
    let cuda_ver = dist_version(&cuda_dir);
    let major = cuda_ver.split('.').next().unwrap_or_default().to_string();
    let have_dir = dists.find_nccl();
    let have = have_dir
        .as_deref()
        .filter(|d| {
            std::fs::read_to_string(d.join(&s.marker))
                .map(|m| m.trim_end().ends_with(&format!(" cu{major}")))
                .unwrap_or(false)
        })
        .map(dist_version)
        .unwrap_or_default();
    let meta = fetch_json(&s.pypi_json.replace("{major}", &major));
    let latest = meta.as_ref().and_then(|m| m["info"]["version"].as_str()).unwrap_or_default().to_string();
    let suffix = format!("_{}.whl", s.wheel_platform);
    let wheel = meta.as_ref().and_then(|m| m["urls"].as_array()).and_then(|urls| {
        urls.iter().find(|u| {
            u["packagetype"].as_str() == Some("bdist_wheel")
                && u["filename"].as_str().map(|f| f.ends_with(&suffix)).unwrap_or(false)
        })
    });
    let url = wheel.and_then(|w| w["url"].as_str()).unwrap_or_default().to_string();
    let sha = wheel.and_then(|w| w["digests"]["sha256"].as_str()).unwrap_or_default().to_string();
    if !util::is_dotted_version(&latest) || !url.starts_with("http") {
        r.note(format!(
            "NCCL           : {} (PyPI unreachable or no cu{major} wheel, kept)",
            if have.is_empty() { "not installed" } else { &have }
        ));
    } else if !have.is_empty() && !util::version_gt(&latest, &have) {
        ui::ok(format!("    {have} is current ({})", have_dir.unwrap().display()));
        r.note(format!("NCCL           : {have} (current)"));
    } else {
        let dest = root.join(&latest).join(util::OS_DIR);
        let whl = root.join(".downloads").join(util::basename(&url));
        let tmp = root.join(".downloads").join(format!("unpack-{latest}"));
        ui::say(format!("    {} -> {latest} (cu{major}), into {}", or_none(&have), dest.display()));
        // The wheel keeps everything under nvidia/nccl/{lib,include}; only
        // that subtree is a dist. libnccl.so is the name the linker asks for
        // and the wheel does not carry: aliased to the soname.
        let unpacked = tmp.join("nvidia/nccl");
        let ok = need_space(&root, 2)
            && download(&url, &whl, (!sha.is_empty()).then_some(sha.as_str()))
            && fresh_dir(&tmp)
            && fresh_dir(&dest)
            && run_cmd(cmd("unzip").arg("-q").arg(&whl).arg("-d").arg(&tmp)).is_ok()
            && unpacked.join("lib/libnccl.so.2").is_file()
            && std::fs::rename(unpacked.join("lib"), dest.join("lib")).is_ok()
            && std::fs::rename(unpacked.join("include"), dest.join("include")).is_ok()
            && std::os::unix::fs::symlink("libnccl.so.2", dest.join("lib/libnccl.so")).is_ok()
            && write_marker(&dest, &s.marker, &format!("{latest} cu{major}"))
            && dists.find_nccl().as_deref() == Some(dest.as_path());
        let _ = util::remove_all(&tmp);
        if ok {
            let _ = std::fs::remove_file(&whl);
            ui::ok(format!("    installed {latest}"));
            r.note(format!("NCCL           : {} -> {latest} (re-run cargo run -p xtask -- build)", or_none(&have)));
        } else {
            // Not a gap: the build goes on without it, and tensor mode
            // between CUDA cards falls back to the internal AllReduce.
            r.note(format!(
                "NCCL           : install of {latest} FAILED, {} kept (optional)",
                if have.is_empty() { "nothing" } else { &have }
            ));
        }
    }
    ui::say("");
}

// ── Vulkan SDK ──────────────────────────────────────────────────────────
fn vulkan(dists: &Dists, r: &mut Report) {
    ui::head("  Vulkan SDK");
    let s = &dists.sources.vulkan;
    let root = dists.vulkan_root();
    let have_dir = dists.find_vulkan();
    let have = have_dir.as_deref().map(dist_version).unwrap_or_default();
    let latest: String = fetch(&s.linux.latest_url)
        .map(|t| t.chars().filter(|c| !c.is_whitespace()).collect())
        .unwrap_or_default();
    if !util::is_dotted_version(&latest) {
        r.note(format!("Vulkan SDK     : {} (LunarG unreachable, kept)", if have.is_empty() { "not installed" } else { &have }));
        if have.is_empty() {
            r.gap(format!("Vulkan: no SDK under {} and LunarG could not be reached", root.display()));
        }
    } else if !have.is_empty() && !util::version_gt(&latest, &have) {
        ui::ok(format!("    {have} is current ({})", have_dir.unwrap().display()));
        r.note(format!("Vulkan SDK     : {have} (current)"));
    } else {
        let dest = root.join(&latest).join(util::OS_DIR);
        let url = s.linux.tarball.replace("{version}", &latest);
        let file = util::basename(&url).to_string();
        let tarball = root.join(".downloads").join(&file);
        let sha = fetch_json(&s.linux.files_json).and_then(|j| {
            j["linux"][&latest]["files"]
                .as_array()?
                .iter()
                .find(|f| f["file_name"].as_str() == Some(&file))
                .and_then(|f| f["sha"].as_str())
                .map(str::to_string)
        });
        ui::say(format!("    {} -> {latest}, into {}", or_none(&have), dest.display()));
        let kept = if have.is_empty() { "nothing" } else { &have };
        match sha {
            None => {
                ui::err(format!("    no sha256 for {file} in files.json: not installing an unverified SDK"));
                r.note(format!("Vulkan SDK     : install of {latest} FAILED (no checksum), {kept} kept"));
                r.gap("Vulkan: re-run this script later");
            }
            Some(sha) => {
                let ok = need_space(&root, 4)
                    && download(&url, &tarball, Some(&sha))
                    && fresh_dir(&dest)
                    && tar("-xJf", &tarball, &dest, true)
                    && write_marker(&dest, &s.marker, &latest)
                    && dists.find_vulkan().as_deref() == Some(dest.join("x86_64").as_path());
                if ok {
                    let _ = std::fs::remove_file(&tarball);
                    ui::ok(format!("    installed {latest}"));
                    r.note(format!("Vulkan SDK     : {} -> {latest} (re-run cargo run -p xtask -- build)", or_none(&have)));
                } else {
                    r.note(format!("Vulkan SDK     : install of {latest} FAILED, {kept} kept"));
                    r.gap(format!("Vulkan: re-run this script (the download resumes from {})", tarball.display()));
                }
            }
        }
    }
    ui::say("");
}
