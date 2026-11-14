//! What a package is named after, the same on every OS: the framework
//! version and the bundled llama.cpp release.

use crate::common::util::{self, cmd, output};
use anyhow::{bail, Context, Result};
use std::path::Path;

/// The framework version = the workspace version of the root Cargo.toml,
/// which every framework crate (pai-core, pai-manager, pai-config) takes.
pub fn framework() -> Result<String> {
    let path = util::repo_root().join("Cargo.toml");
    let manifest: toml::Table = toml::from_str(&std::fs::read_to_string(&path)?)
        .with_context(|| format!("parsing {}", path.display()))?;
    Ok(manifest
        .get("workspace")
        .and_then(|w| w.get("package"))
        .and_then(|p| p.get("version"))
        .and_then(|v| v.as_str())
        .with_context(|| {
            format!(
                "could not read [workspace.package] version from {}",
                path.display()
            )
        })?
        .to_string())
}

/// The bundled llama.cpp: its release tag (the build detaches onto it, and
/// `git describe` prefers the annotated vX.Y.Z over the lightweight bNNNN on
/// the same commit). Anything else (a hand-moved HEAD) is refused: packages
/// are built from release tags only.
pub fn llama_build(llama: &Path) -> Result<String> {
    if !llama.join(".git").exists() {
        bail!(
            "no llama.cpp clone at {}. Run cargo run -p xtask -- build first.",
            llama.display()
        );
    }
    let tag = output(cmd("git").arg("-C").arg(llama).args(["describe", "--tags"]))
        .and_then(|s| s.lines().next().map(str::to_string))
        .unwrap_or_default();
    if !util::is_release_tag(&tag) {
        bail!(
            "the llama.cpp clone at {} is not on a release tag (vX.Y.Z): packages are built from release tags only. Run cargo run -p xtask -- build to check out the latest release.",
            llama.display()
        );
    }
    Ok(tag)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;
    use std::process::Command;

    /// A throwaway llama.cpp clone: a git repo with one commit, removed on
    /// drop. Git runs with a fixed identity and no user or system config.
    struct Clone(PathBuf);

    impl Clone {
        fn new(name: &str) -> Self {
            let dir = std::env::temp_dir()
                .join(format!("xtask-llama-build-{name}-{}", std::process::id()));
            let _ = std::fs::remove_dir_all(&dir);
            std::fs::create_dir_all(&dir).unwrap();
            let clone = Clone(dir);
            clone.git(&["init", "-q"]);
            clone.git(&["commit", "-q", "--allow-empty", "-m", "one"]);
            clone
        }

        fn git(&self, args: &[&str]) {
            let ok = Command::new("git")
                .arg("-C")
                .arg(&self.0)
                .args(["-c", "user.name=t", "-c", "user.email=t@example.com"])
                .args(["-c", "commit.gpgsign=false", "-c", "tag.gpgsign=false"])
                .args(args)
                .env("GIT_CONFIG_GLOBAL", "/dev/null")
                .env("GIT_CONFIG_NOSYSTEM", "1")
                .status()
                .unwrap()
                .success();
            assert!(ok, "git {args:?}");
        }
    }

    impl Drop for Clone {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }

    /// On a release tag (annotated, as llama.cpp tags its releases, beside
    /// the lightweight build tag on the same commit), the clone is named by it.
    #[test]
    fn a_clone_on_a_release_tag_is_named_by_it() {
        let clone = Clone::new("release");
        clone.git(&["tag", "b8000"]);
        clone.git(&["tag", "-a", "v1.2.3", "-m", "v1.2.3"]);
        assert_eq!(llama_build(&clone.0).unwrap(), "v1.2.3");
    }

    /// Every OS's `xtask package` refuses a llama.cpp clone not on a release
    /// tag, and nothing else asks for its name: the clone is refused here,
    /// once, with what to do, instead of being named `b<N>` (or `b0-<sha>`)
    /// for each caller to refuse again. A build tag alone, an -rc, a commit
    /// past the release and no tag at all are each refused.
    #[test]
    fn a_clone_not_on_a_release_tag_is_refused() {
        let build_tag = Clone::new("build-tag");
        build_tag.git(&["tag", "b8000"]);
        let rc = Clone::new("rc");
        rc.git(&["tag", "-a", "v1.2.3-rc1", "-m", "rc"]);
        let past = Clone::new("past");
        past.git(&["tag", "-a", "v1.2.3", "-m", "v1.2.3"]);
        past.git(&["commit", "-q", "--allow-empty", "-m", "two"]);
        let untagged = Clone::new("untagged");
        for clone in [&build_tag, &rc, &past, &untagged] {
            let err = match llama_build(&clone.0) {
                Ok(name) => panic!("{}: named {name:?}, not refused", clone.0.display()),
                Err(e) => format!("{e:#}"),
            };
            assert!(
                err.contains("release") && err.contains("xtask -- build"),
                "{}: the refusal does not say what to do: {err}",
                clone.0.display()
            );
        }
    }

    /// No clone at all is refused as before.
    #[test]
    fn no_clone_is_refused() {
        let dir =
            std::env::temp_dir().join(format!("xtask-llama-build-none-{}", std::process::id()));
        let err = llama_build(&dir).unwrap_err().to_string();
        assert!(err.contains("no llama.cpp clone"), "{err}");
    }
}
