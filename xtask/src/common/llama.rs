//! The llama.cpp build steps every operating system shares: the release-tag
//! checkout with the out-of-tree patches, the build's own sccache daemon,
//! the web UI check (the CMake options are cmake_options.rs's). Each OS's
//! configure line is its own `build_llama.rs` (linux/, windows/, macos/).
//! docs/build-pipeline.md explains the decisions the builds share.

use crate::common::ui;
use crate::common::util::{self, cmd, run as run_cmd, succeeds};
use anyhow::{bail, Context, Result};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

/// The script a phase runs behind, for the messages that name it.
/// The prerequisites script of this OS, the one step that is not xtask's.
pub fn prereq_script() -> &'static str {
    if cfg!(windows) {
        "scripts\\00-install-prerequisites.ps1"
    } else {
        "scripts/00-install-prerequisites.sh"
    }
}


fn git(dir: &Path) -> Command {
    let mut c = cmd("git");
    c.arg("-C").arg(dir);
    c
}

/// A failing `git apply --check` is an ANSWER here, not an error.
fn can_apply(dir: &Path, reverse: bool, patch: &Path) -> bool {
    let mut c = git(dir);
    c.args(["apply", "--check"]);
    if reverse {
        c.arg("--reverse");
    }
    succeeds(c.arg("--").arg(patch))
}

fn patch_name(p: &Path) -> String {
    p.file_name().unwrap_or_default().to_string_lossy().into_owned()
}

/// Clone or fetch, check out the newest release tag (vX.Y.Z), never master
/// HEAD and never a bNNNN nightly, and apply patches/llama.cpp/*.patch.
pub fn checkout(dir: &Path) -> Result<()> {
    if !dir.join("CMakeLists.txt").is_file() {
        ui::warn(format!("llama.cpp not found at {}, cloning...", dir.display()));
        run_cmd(cmd("git").args(["clone", "https://github.com/ggerganov/llama.cpp"]).arg(dir))?;
    } else {
        ui::head("Fetching latest llama.cpp (branches + tags)...");
        run_cmd(git(dir).args(["fetch", "origin", "--tags"]))?;
    }

    // Out-of-tree source patches, PROVISIONAL by design, shared with the
    // Windows build: reverted before a checkout that moves HEAD, re-applied
    // onto the fresh tag, and FATAL when one applies in neither direction
    // (patches/llama.cpp/README.md).
    let patch_dir = util::repo_root().join("patches/llama.cpp");
    let mut patches: Vec<PathBuf> = std::fs::read_dir(&patch_dir)
        .map(|rd| {
            rd.flatten()
                .map(|e| e.path())
                .filter(|p| p.extension().map(|x| x == "patch").unwrap_or(false))
                .collect()
        })
        .unwrap_or_default();
    patches.sort();

    let tag = util::newest_release_tag(dir).with_context(|| {
        format!(
            "could not resolve a llama.cpp release tag (vX.Y.Z) reachable from origin/master in {}",
            dir.display()
        )
    })?;

    // Clean the tree only when the checkout actually moves HEAD, or when a
    // patch applies in neither direction (the clone holds an older revision
    // of it): rewriting a patched file on a re-run would bump its mtime and
    // cost a needless recompile. Whatever the reverts leave behind (a stale
    // patch revision, a hand edit) is stashed, never discarded: `git stash
    // list` in the clone has it back.
    let head = util::output(git(dir).args(["rev-parse", "HEAD"]));
    let target = util::output(git(dir).args(["rev-parse", &format!("{tag}^{{commit}}")]));
    let mut needs_clean = head != target;
    for p in &patches {
        if !can_apply(dir, false, p) && !can_apply(dir, true, p) {
            needs_clean = true;
        }
    }
    if needs_clean {
        for p in &patches {
            if !can_apply(dir, true, p) {
                continue;
            }
            run_cmd(git(dir).args(["apply", "--reverse", "--"]).arg(p))?;
            ui::dim(format!("Reverted patch for a clean checkout: {}", patch_name(p)));
        }
        let dirty = util::output(git(dir).args(["status", "--porcelain", "--untracked-files=no"])).unwrap_or_default();
        if !dirty.is_empty() {
            run_cmd(git(dir).args([
                "stash",
                "push",
                "--quiet",
                "--message",
                &format!("xtask build: local changes before checking out {tag}"),
            ]))?;
            ui::warn(format!(
                "Stashed local changes in the llama.cpp clone (git -C {} stash list)",
                dir.display()
            ));
        }
    }

    ui::head(format!("Checking out llama.cpp release tag: {tag}"));
    run_cmd(git(dir).args(["checkout", "--quiet", "--detach", &tag]))?;

    for p in &patches {
        let name = patch_name(p);
        if can_apply(dir, false, p) {
            run_cmd(git(dir).args(["apply", "--"]).arg(p))?;
            ui::head(format!("Applied patch: patches/llama.cpp/{name}"));
        } else if can_apply(dir, true, p) {
            ui::dim(format!("Patch already applied: patches/llama.cpp/{name}"));
        } else {
            bail!(
                "patches/llama.cpp/{name} applies in neither direction to {tag}. Upstream moved the code it touches: refresh the patch, or delete it if upstream now ships the same support. See patches/llama.cpp/README.md."
            );
        }
    }
    Ok(())
}

/// The value of a CMakeCache entry, empty when absent.
pub fn cache_get(cache: &Path, name: &str) -> String {
    let Ok(text) = std::fs::read_to_string(cache) else { return String::new() };
    let prefix = format!("{name}:");
    text.lines()
        .filter_map(|l| l.strip_prefix(&prefix))
        .find_map(|rest| rest.split_once('=').map(|(_, v)| v.to_string()))
        .unwrap_or_default()
}

/// The build's own sccache daemon: prints its stats and stops when dropped,
/// so a failed configure or build stops it too.
pub struct SccacheDaemon(PathBuf);

impl SccacheDaemon {
    /// Stops a leftover on the build's port (a killed run), then starts the
    /// build's own daemon with the settings below. A daemon already running
    /// on the default port would otherwise serve every compile with ITS cache
    /// dir and settings, which win over the client's.
    pub fn start(sc: &Path) -> Result<SccacheDaemon> {
        let dir = util::build_root().join(".sccache");
        std::env::set_var("SCCACHE_DIR", &dir);
        std::env::set_var("SCCACHE_CACHE_SIZE", "10G");
        std::env::set_var("SCCACHE_IDLE_TIMEOUT", "0");
        std::env::set_var("SCCACHE_MAX_FRAME_LENGTH", "104857600"); // 100MB: GPU multi-arch objects are large
        // Client-side mode (sccache 0.17+): cache lookup and compile run in
        // the client process, the daemon serves only shared state. Stats stay
        // server-side, so --show-stats still aggregates the whole build.
        std::env::set_var("SCCACHE_CLIENT_SIDE", "1");
        std::env::set_var("SCCACHE_SERVER_PORT", "4227");
        std::fs::create_dir_all(&dir)?;
        ui::head(format!("sccache: {} (cache: {}, port 4227)", sc.display(), dir.display()));
        let _ = succeeds(cmd(sc).arg("--stop-server"));
        if !cmd(sc).arg("--start-server").stdout(Stdio::null()).status().map(|s| s.success()).unwrap_or(false) {
            bail!("sccache could not start its server on port 4227");
        }
        Ok(SccacheDaemon(sc.to_path_buf()))
    }
}

impl Drop for SccacheDaemon {
    fn drop(&mut self) {
        ui::say("");
        ui::head("sccache stats:");
        let _ = cmd(&self.0).arg("--show-stats").status();
        let _ = cmd(&self.0).arg("--stop-server").stdout(Stdio::null()).stderr(Stdio::null()).status();
    }
}

/// The chat UI must be IN the binary. llama.cpp's scripts/ui-assets.cmake
/// never fails: when neither the npm build (LLAMA_BUILD_UI) nor the
/// HuggingFace download (LLAMA_USE_PREBUILT_UI) produced assets it prints a
/// cmake WARNING and embeds an EMPTY UI, and a llama-server with no chat page
/// is exactly what this framework must not package (the built-in UI is the
/// frontend). Both provisioning paths write tools/ui/dist under the build
/// dir (a dist dir dropped into the SOURCE tree is used as is, so that counts
/// too), and only the download leaves a .ui-stamp, which is how the report
/// knows which one ran.
pub fn check_web_ui(build_dir: &Path, llama: &Path) -> Result<()> {
    let ui_dist = build_dir.join("tools").join("ui").join("dist").join("index.html");
    let ui_src_dist = llama.join("tools").join("ui").join("dist").join("index.html");
    let ui_stamp = build_dir.join("tools").join("ui").join(".ui-stamp");
    if !ui_dist.is_file() && !ui_src_dist.is_file() {
        bail!(
            "the web UI was not embedded ({} is missing). LLAMA_BUILD_UI=ON needs node + npm on PATH (the npm failure is in the configure output above), the prebuilt fallback needs huggingface.co to be reachable. A llama-server without its chat UI is not packaged.",
            ui_dist.display()
        );
    }
    if ui_stamp.is_file() {
        let stamp: String = std::fs::read_to_string(&ui_stamp)
            .unwrap_or_default()
            .chars()
            .filter(|c| !c.is_whitespace())
            .collect();
        ui::warn(format!(
            "Web UI: PREBUILT assets ({stamp}) downloaded from HuggingFace; the npm build did not produce any"
        ));
    } else if ui_dist.is_file() {
        ui::dim("Web UI: built from the checked-out sources (npm)");
    } else {
        ui::dim(format!("Web UI: pre-built assets found in the source tree ({})", ui_src_dist.display()));
    }
    Ok(())
}

/// Removes CMake's CUDA identification files from every CMakeFiles/<ver>/.
#[cfg(not(target_os = "macos"))]
pub fn reset_cuda_detection(build_dir: &Path) -> Result<()> {
    let Ok(rd) = std::fs::read_dir(build_dir.join("CMakeFiles")) else { return Ok(()) };
    for e in rd.flatten() {
        let name = e.file_name().to_string_lossy().into_owned();
        if !name.starts_with(|c: char| c.is_ascii_digit()) || !e.path().is_dir() {
            continue;
        }
        for f in ["CMakeCUDACompiler.cmake", "CMakeDetermineCompilerABI_CUDA.bin", "CompilerIdCUDA"] {
            util::remove_all(&e.path().join(f))?;
        }
    }
    Ok(())
}

/// `<exe> --version`, indented, stderr included when `with_stderr`.
#[cfg_attr(windows, allow(dead_code))]
pub fn print_version(exe: &Path, with_stderr: bool) -> Result<()> {
    let out = cmd(exe).arg("--version").stdin(Stdio::null()).output()?;
    let mut text = String::from_utf8_lossy(&out.stdout).into_owned();
    if with_stderr {
        text.push_str(&String::from_utf8_lossy(&out.stderr));
    }
    for line in text.lines() {
        ui::say(format!("  {line}"));
    }
    if !out.status.success() {
        bail!("{} --version failed ({})", exe.display(), out.status);
    }
    Ok(())
}
