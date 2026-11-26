//! The llama.cpp build steps every operating system shares: the release-tag
//! checkout with the out-of-tree patches, the build's own sccache daemon,
//! the web UI check (the CMake options are cmake_options.rs's). Each OS's
//! configure line is its own `build_llama.rs` (linux/, windows/, macos/).
//! docs/build-compiling.md explains the decisions the builds share.

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
    p.file_name()
        .unwrap_or_default()
        .to_string_lossy()
        .into_owned()
}

/// Clone or fetch, check out the newest release tag (vX.Y.Z), never master
/// HEAD and never a bNNNN nightly, and apply patches/llama.cpp/*.patch.
pub fn checkout(dir: &Path) -> Result<()> {
    if !dir.join("CMakeLists.txt").is_file() {
        ui::warn(format!(
            "llama.cpp not found at {}, cloning...",
            dir.display()
        ));
        run_cmd(
            cmd("git")
                .args(["clone", "https://github.com/ggerganov/llama.cpp"])
                .arg(dir),
        )?;
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
            ui::dim(format!(
                "Reverted patch for a clean checkout: {}",
                patch_name(p)
            ));
        }
        let dirty = util::output(git(dir).args(["status", "--porcelain", "--untracked-files=no"]))
            .unwrap_or_default();
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
    let Ok(text) = std::fs::read_to_string(cache) else {
        return String::new();
    };
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
        ui::head(format!(
            "sccache: {} (cache: {}, port 4227)",
            sc.display(),
            dir.display()
        ));
        let _ = succeeds(cmd(sc).arg("--stop-server"));
        if !cmd(sc)
            .arg("--start-server")
            .stdout(Stdio::null())
            .status()
            .map(|s| s.success())
            .unwrap_or(false)
        {
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
        let _ = cmd(&self.0)
            .arg("--stop-server")
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status();
    }
}

/// The oldest sccache release that may wrap nvcc: the one that carries
/// sccache PR #2809. Why nvcc is gated at all, and which release that is, is
/// in docs/build-compiling.md, "sccache".
const SCCACHE_NVCC_MIN: &str = "0.20.0";

/// Whether the sccache that was found may wrap nvcc: `sccache --version`'s
/// first line as printed ("sccache 0.20.0"). A release at or above
/// `SCCACHE_NVCC_MIN` is the gate; an older one, and a line with no readable
/// dotted version in it, keep nvcc unwrapped.
#[cfg_attr(not(target_os = "linux"), allow(dead_code))]
pub fn sccache_caches_nvcc(version_line: &str) -> bool {
    version_line
        .split_whitespace()
        .find(|word| util::is_dotted_version(word))
        .is_some_and(|v| util::vercmp(v, SCCACHE_NVCC_MIN).is_ge())
}

/// The chat UI must be IN the binary. llama.cpp's scripts/ui-assets.cmake
/// never fails: when neither the npm build (LLAMA_BUILD_UI) nor the
/// HuggingFace download (LLAMA_USE_PREBUILT_UI) produced assets it prints a
/// cmake WARNING and embeds an EMPTY UI, and a llama-server with no chat page
/// is exactly what this framework must not package (the built-in UI is the
/// frontend). Both provisioning paths write their dist under the UI's own
/// cmake dir, `ui/apps/chat`, in the build tree (a dist dir dropped into the
/// SOURCE tree is used as is, so that counts too), and only the download
/// leaves a .ui-stamp beside the dist, which is how the report knows which
/// one ran.
pub fn check_web_ui(build_dir: &Path, llama: &Path) -> Result<()> {
    let ui_dist = build_dir
        .join("ui")
        .join("apps")
        .join("chat")
        .join("dist")
        .join("index.html");
    let ui_src_dist = llama
        .join("ui")
        .join("apps")
        .join("chat")
        .join("dist")
        .join("index.html");
    let ui_stamp = build_dir
        .join("ui")
        .join("apps")
        .join("chat")
        .join(".ui-stamp");
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
        ui::dim(format!(
            "Web UI: pre-built assets found in the source tree ({})",
            ui_src_dist.display()
        ));
    }
    Ok(())
}

/// Removes CMake's CUDA identification files from every CMakeFiles/<ver>/.
#[cfg(not(target_os = "macos"))]
pub fn reset_cuda_detection(build_dir: &Path) -> Result<()> {
    let Ok(rd) = std::fs::read_dir(build_dir.join("CMakeFiles")) else {
        return Ok(());
    };
    for e in rd.flatten() {
        let name = e.file_name().to_string_lossy().into_owned();
        if !name.starts_with(|c: char| c.is_ascii_digit()) || !e.path().is_dir() {
            continue;
        }
        for f in [
            "CMakeCUDACompiler.cmake",
            "CMakeDetermineCompilerABI_CUDA.bin",
            "CompilerIdCUDA",
        ] {
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

#[cfg(test)]
mod tests {
    use super::*;

    // SC: the sccache version gate for wrapping nvcc (sccache PR #2809, in
    // the release `SCCACHE_NVCC_MIN` names; why and which release:
    // docs/build-compiling.md, "sccache"). The line is what
    // `sccache --version` prints.

    #[test]
    fn sc_caches_nvcc_0_20_1() {
        assert!(
            sccache_caches_nvcc("sccache 0.20.1"),
            "a newer patch of the same minor carries it"
        );
    }

    #[test]
    fn sc_caches_nvcc_0_21_0() {
        assert!(
            sccache_caches_nvcc("sccache 0.21.0"),
            "a newer minor carries it"
        );
    }

    #[test]
    fn sc_caches_nvcc_1_0_0() {
        assert!(
            sccache_caches_nvcc("sccache 1.0.0"),
            "a newer major carries it"
        );
    }

    #[test]
    fn sc_caches_nvcc_0_19_0_no() {
        assert!(
            !sccache_caches_nvcc("sccache 0.19.0"),
            "0.19.0 predates PR #2809"
        );
    }

    #[test]
    fn sc_caches_nvcc_0_20_0() {
        assert!(
            sccache_caches_nvcc("sccache 0.20.0"),
            "0.20.0 is the release that carries PR #2809"
        );
    }

    #[test]
    fn sc_caches_nvcc_unreadable_no() {
        for line in [
            "",
            "sccache",
            "sccache 0.20",
            "sccache dev",
            "not a version",
        ] {
            assert!(
                !sccache_caches_nvcc(line),
                "an unreadable version keeps nvcc unwrapped: {line:?}"
            );
        }
    }
    // UI2: llama.cpp v1.1.0 moved its chat UI from tools/ui to ui/apps/chat,
    // so the check looks at the release's layout. xtask always builds the
    // newest release, so the old tools/ui path counts for nothing.

    /// The chat UI's `index.html`, and the stamp only the HuggingFace
    /// download leaves, as the release lays them out.
    const UI_DIST: &str = "ui/apps/chat/dist/index.html";
    const UI_STAMP: &str = "ui/apps/chat/.ui-stamp";

    /// The layout llama.cpp v1.0.2 and earlier used.
    const OLD_UI_DIST: &str = "tools/ui/dist/index.html";

    /// A throwaway pair of trees: the cmake build dir and the llama.cpp
    /// clone, removed on drop.
    struct Trees {
        root: PathBuf,
        build: PathBuf,
        src: PathBuf,
    }

    impl Trees {
        fn new(name: &str) -> Self {
            let root =
                std::env::temp_dir().join(format!("xtask-web-ui-{name}-{}", std::process::id()));
            let _ = std::fs::remove_dir_all(&root);
            let trees = Trees {
                build: root.join("build"),
                src: root.join("llama.cpp"),
                root,
            };
            std::fs::create_dir_all(&trees.build).unwrap();
            std::fs::create_dir_all(&trees.src).unwrap();
            trees
        }

        /// Writes an `index.html` (or the stamp's content) under one tree.
        fn write(&self, dir: &Path, rel: &str, bytes: &str) {
            let p = dir.join(rel);
            std::fs::create_dir_all(p.parent().unwrap()).unwrap();
            std::fs::write(&p, bytes).unwrap();
        }

        fn in_build(&self, rel: &str) {
            self.write(&self.build, rel, "<html></html>");
        }

        fn in_src(&self, rel: &str) {
            self.write(&self.src, rel, "<html></html>");
        }
    }

    impl Drop for Trees {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.root);
        }
    }

    /// The npm build put the assets under the build dir: embedded.
    #[test]
    fn ui2_dist_in_the_build_dir_is_ok() {
        let trees = Trees::new("build-dist");
        trees.in_build(UI_DIST);
        assert!(check_web_ui(&trees.build, &trees.src).is_ok());
    }

    /// A dist dir dropped into the SOURCE tree is used as is: embedded.
    #[test]
    fn ui2_dist_in_the_source_tree_is_ok() {
        let trees = Trees::new("src-dist");
        trees.in_src(UI_DIST);
        assert!(check_web_ui(&trees.build, &trees.src).is_ok());
    }

    /// Neither tree has the assets: the build embedded an empty UI, so the
    /// package is refused, and the message names the build path it looked at.
    #[test]
    fn ui2_no_dist_is_refused_naming_the_build_path() {
        let trees = Trees::new("no-dist");
        let err = check_web_ui(&trees.build, &trees.src)
            .unwrap_err()
            .to_string();
        let want = trees
            .build
            .join("ui")
            .join("apps")
            .join("chat")
            .join("dist")
            .join("index.html");
        assert!(
            err.contains(&want.display().to_string()),
            "the refusal names the release's build path ({}, in the message): {err}",
            want.display()
        );
    }

    /// The prebuilt case: the HuggingFace download left its stamp beside the
    /// assets.
    #[test]
    fn ui2_dist_with_the_stamp_is_ok() {
        let trees = Trees::new("stamp");
        trees.in_build(UI_DIST);
        trees.in_build(UI_STAMP);
        assert!(check_web_ui(&trees.build, &trees.src).is_ok());
    }

    /// The old `tools/ui` layout, in either tree, is no proof of anything:
    /// v1.1.0 never writes it, so a dist there alone is refused.
    #[test]
    fn ui2_old_tools_ui_layout_is_not_enough() {
        for src in [false, true] {
            let trees = Trees::new(if src { "old-src" } else { "old-build" });
            if src {
                trees.in_src(OLD_UI_DIST);
            } else {
                trees.in_build(OLD_UI_DIST);
            }
            assert!(
                check_web_ui(&trees.build, &trees.src).is_err(),
                "a dist at the pre-v1.1.0 path does not count (src tree: {src})"
            );
        }
    }
}
