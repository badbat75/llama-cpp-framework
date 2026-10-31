//! The prerequisites phase's shared half (Linux, macOS): resumable,
//! sha256-checked downloads, the dist markers, sccache from its GitHub
//! release, the llama.cpp release check and the report. Each OS's legs (which
//! dists, from where) are its own `prereq.rs`.

use crate::common::dists::Dists;
use crate::common::ui;
use crate::common::util::{self, cmd, output, succeeds};
use anyhow::{bail, Result};
use serde_json::Value;
use sha2::{Digest, Sha256};
use std::path::{Path, PathBuf};

/// What the phase did, and what is left to fix by hand.
pub struct Report {
    pub lines: Vec<String>,
    pub gaps: Vec<String>,
}

impl Report {
    pub fn note(&mut self, s: impl Into<String>) {
        self.lines.push(s.into());
    }
    pub fn gap(&mut self, s: impl Into<String>) {
        self.gaps.push(s.into());
    }
}

/// A small text resource; None when it cannot be fetched.
pub fn fetch(url: &str) -> Option<String> {
    output(cmd("curl").args(["-fsSL", "--retry", "3", "--connect-timeout", "20", url]))
}

pub fn fetch_json(url: &str) -> Option<Value> {
    serde_json::from_str(&fetch(url)?).ok()
}

pub fn sha256_matches(p: &Path, want: &str) -> bool {
    let Ok(mut f) = std::fs::File::open(p) else { return false };
    let mut h = Sha256::new();
    if std::io::copy(&mut f, &mut h).is_err() {
        return false;
    }
    let got: String = h.finalize().iter().map(|b| format!("{b:02x}")).collect();
    got.eq_ignore_ascii_case(want)
}

/// Resumable download into `dest`, with an optional sha256. A file that is
/// already complete and matches is reused; one that fails the hash is deleted
/// so the next run starts clean instead of resuming garbage.
pub fn download(url: &str, dest: &Path, sha: Option<&str>) -> bool {
    if let Some(d) = dest.parent() {
        let _ = std::fs::create_dir_all(d);
    }
    if let Some(s) = sha {
        if dest.is_file() && sha256_matches(dest, s) {
            return true;
        }
    }
    let name = dest.file_name().unwrap_or_default().to_string_lossy().into_owned();
    ui::dim(format!("    downloading {name}"));
    // -C - resumes; a server answering 416 means the file is already whole.
    use std::io::IsTerminal;
    let progress = if std::io::stderr().is_terminal() { "--progress-bar" } else { "-sS" };
    let rc = cmd("curl")
        .args(["-fL", "--retry", "5", "--retry-delay", "5", "--connect-timeout", "20", "-C", "-", progress, "-o"])
        .arg(dest)
        .arg(url)
        .status()
        .ok()
        .and_then(|s| s.code())
        .unwrap_or(-1);
    // 22 with a file already on disk is the 416 case: the hash (or, for an
    // archive without one, the extract) decides whether it really is whole.
    let nonempty = std::fs::metadata(dest).map(|m| m.len() > 0).unwrap_or(false);
    if rc != 0 && !(rc == 22 && nonempty) {
        return false;
    }
    if let Some(s) = sha {
        if !sha256_matches(dest, s) {
            ui::err(format!("    sha256 mismatch on {name}, deleted"));
            let _ = std::fs::remove_file(dest);
            return false;
        }
    }
    true
}

/// Free space check before a big extract, in GiB.
pub fn need_space(dir: &Path, need_gib: u64) -> bool {
    let _ = std::fs::create_dir_all(dir);
    // POSIX output (-P), the same on GNU and BSD df: Available is field 4.
    let avail_kib: u64 = output(cmd("df").args(["-P", "-k"]).arg(dir))
        .and_then(|s| s.lines().last().and_then(|l| l.split_whitespace().nth(3)?.parse().ok()))
        .unwrap_or(0);
    let avail = avail_kib / 1_048_576;
    if avail < need_gib {
        ui::err(format!("    only {avail} GiB free under {}, {need_gib} GiB needed", dir.display()));
        return false;
    }
    true
}

/// Clears and recreates a directory (an interrupted extract of the same
/// version is redone from scratch).
#[cfg(target_os = "linux")]
pub fn fresh_dir(d: &Path) -> bool {
    util::remove_all(d).is_ok() && std::fs::create_dir_all(d).is_ok()
}

pub fn write_marker(dir: &Path, marker: &str, text: &str) -> bool {
    std::fs::write(dir.join(marker), format!("{text}\n")).is_ok()
}

pub fn tar(flags: &str, archive: &Path, dest: &Path, strip: bool) -> bool {
    let mut c = cmd("tar");
    c.arg(flags).arg(archive).arg("-C").arg(dest);
    if strip {
        c.arg("--strip-components=1");
    }
    c.status().map(|s| s.success()).unwrap_or(false)
}

pub fn or_none(s: &str) -> &str {
    if s.is_empty() {
        "none"
    } else {
        s
    }
}

// ── sccache ─────────────────────────────────────────────────────────────
pub fn sccache(dists: &Dists, r: &mut Report) {
    ui::head("  sccache");
    let bin = util::home().join(".local/bin/sccache");
    let have = if util::is_executable(&bin) {
        output(cmd(&bin).arg("--version"))
            .and_then(|s| s.split_whitespace().nth(1).map(str::to_string))
            .unwrap_or_default()
    } else {
        String::new()
    };
    // The release asset of this machine (the static musl build on Linux).
    let sc_arch = match (std::env::consts::OS, std::env::consts::ARCH) {
        ("linux", "x86_64") => Some("x86_64-unknown-linux-musl"),
        ("linux", "aarch64") => Some("aarch64-unknown-linux-musl"),
        ("macos", "aarch64") => Some("aarch64-apple-darwin"),
        _ => None,
    };
    let rel = sc_arch.and_then(|_| fetch_json(&dists.sources.sccache.release_api));
    let latest = rel
        .as_ref()
        .and_then(|j| j["tag_name"].as_str())
        .map(|t| t.trim_start_matches('v').to_string())
        .unwrap_or_default();
    if latest.is_empty() {
        r.note(format!("sccache        : {} (release lookup failed, kept)", if have.is_empty() { "not installed" } else { &have }));
    } else if !have.is_empty() && !util::version_gt(&latest, &have) {
        ui::ok(format!("    {have} is current"));
        r.note(format!("sccache        : {have} (current)"));
    } else {
        let asset = format!("sccache-v{latest}-{}.tar.gz", sc_arch.unwrap_or_default());
        let entry = rel
            .as_ref()
            .and_then(|j| j["assets"].as_array())
            .and_then(|a| a.iter().find(|x| x["name"].as_str() == Some(&asset)).cloned());
        let url = entry.as_ref().and_then(|e| e["browser_download_url"].as_str()).unwrap_or_default().to_string();
        let sha = entry
            .as_ref()
            .and_then(|e| e["digest"].as_str())
            .map(|d| d.trim_start_matches("sha256:").to_string())
            .filter(|s| !s.is_empty());
        let tmp = util::build_root().join(".sccache-download");
        let _ = util::remove_all(&tmp);
        let archive = tmp.join(&asset);
        let ok = !url.is_empty()
            && download(&url, &archive, sha.as_deref())
            && tar("-xzf", &archive, &tmp, true)
            && succeeds(cmd(tmp.join("sccache")).arg("--version"))
            && install_binary(&tmp.join("sccache"), &bin);
        if ok {
            ui::ok(format!("    {} -> {latest} ({})", or_none(&have), bin.display()));
            r.note(format!("sccache        : {} -> {latest}", or_none(&have)));
        } else {
            r.note(format!(
                "sccache        : {} (download of {latest} failed, kept)",
                if have.is_empty() { "not installed" } else { &have }
            ));
        }
        let _ = util::remove_all(&tmp);
    }
    ui::say("");
}

/// Staged beside the target and MOVED over it: a rename replaces a binary
/// that is running without disturbing the process.
fn install_binary(src: &Path, dest: &Path) -> bool {
    use std::os::unix::fs::PermissionsExt;
    let Some(dir) = dest.parent() else { return false };
    let staged = dir.join(format!("{}.new", dest.file_name().unwrap_or_default().to_string_lossy()));
    std::fs::create_dir_all(dir).is_ok()
        && std::fs::copy(src, &staged).is_ok()
        && std::fs::set_permissions(&staged, std::fs::Permissions::from_mode(0o755)).is_ok()
        && std::fs::rename(&staged, dest).is_ok()
}

// ── llama.cpp release check ─────────────────────────────────────────────
// Fetch only: the clone sits on a detached release tag and the checkout is
// the build's job. Same selection as there.
pub fn llama_check(recorded_dir: Option<&str>, r: &mut Report) {
    let dir = recorded_dir
        .filter(|d| !d.is_empty())
        .map(PathBuf::from)
        .unwrap_or_else(|| util::build_root().join("llama.cpp"));
    if !dir.join(".git").exists() || util::which("git").is_none() {
        r.note("llama.cpp      : not cloned yet (cargo run -p xtask -- build clones it)");
        return;
    }
    let git = || {
        let mut c = cmd("git");
        c.arg("-C").arg(&dir);
        c
    };
    let cur = output(git().args(["describe", "--tags", "--exact-match"])).unwrap_or_else(|| "untagged".into());
    if succeeds(git().args(["fetch", "--quiet", "origin", "--tags"])) {
        let new = util::newest_release_tag(&dir).unwrap_or_default();
        if cur == new {
            r.note(format!("llama.cpp      : {cur} (latest release)"));
        } else {
            r.note(format!("llama.cpp      : {cur}, release {new} available (run cargo run -p xtask -- build)"));
        }
    } else {
        r.note(format!("llama.cpp      : {cur} (fetch failed)"));
    }
}

/// The report, the inactive dists under each root (listed, never deleted: a
/// working rollback is the user's to keep or remove), and the gaps, which
/// fail the phase. `explicit_root` is whether --dist-root was given:
/// The configure phase has to be told the root only when it would not resolve
/// this one by itself.
pub fn finish(r: &Report, dists: &Dists, roots: &[PathBuf], explicit_root: bool, recorded_root: &str) -> Result<()> {
    // ── Report ──────────────────────────────────────────────────────────
    ui::head("  Report");
    ui::head("  ------");
    for l in &r.lines {
        ui::say(format!("  {l}"));
    }
    for root in roots {
        let n = std::fs::read_dir(root)
            .map(|rd| rd.flatten().filter(|e| e.path().join(util::OS_DIR).is_dir()).count())
            .unwrap_or(0);
        if n > 1 {
            ui::dim(format!(
                "  {} holds {n} {} dists; older ones are kept until you delete them",
                root.display(),
                util::OS_DIR
            ));
        }
    }
    ui::say("");
    if !r.gaps.is_empty() {
        ui::err(format!("  To fix by hand ({}):", r.gaps.len()));
        for g in &r.gaps {
            ui::err(format!("    - {g}"));
        }
        ui::say("");
        bail!("{} prerequisite(s) missing", r.gaps.len());
    }
    let mut configure = String::from("cargo run -p xtask -- configure");
    let root = dists.root.display().to_string();
    if explicit_root && std::env::var_os("LLAMA_FW_DIST_ROOT").map(|v| v.is_empty()).unwrap_or(true) && root != recorded_root {
        configure.push_str(&format!(" --dist-root {root}"));
    }
    ui::ok(format!("  All prerequisites in place. Next: {configure}, then cargo run -p xtask -- build"));
    ui::say("");
    Ok(())
}

