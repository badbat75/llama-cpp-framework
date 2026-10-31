//! What a package is named after, the same on every OS: the framework
//! version and the bundled llama.cpp release.

use crate::common::ui;
use crate::common::util::{self, cmd, output};
use anyhow::{bail, Context, Result};
use std::path::Path;

/// The framework version = the llama-cpp-config crate version: the
/// configurator and the framework are versioned together.
pub fn framework() -> Result<String> {
    let path = util::repo_root().join("llama-cpp-config").join("Cargo.toml");
    let manifest: toml::Table = toml::from_str(&std::fs::read_to_string(&path)?)
        .with_context(|| format!("parsing {}", path.display()))?;
    Ok(manifest
        .get("package")
        .and_then(|p| p.get("version"))
        .and_then(|v| v.as_str())
        .with_context(|| format!("could not read [package] version from {}", path.display()))?
        .to_string())
}

/// The bundled llama.cpp: its release tag (the build detaches onto it, and
/// `git describe` prefers the annotated vX.Y.Z over the lightweight bNNNN on
/// the same commit). Anything else (a hand-moved HEAD) is named by build
/// number, shaped bNNNN, so it cannot be mistaken for a release.
pub fn llama_build(llama: &Path) -> Result<String> {
    if !llama.join(".git").exists() {
        bail!("no llama.cpp clone at {}. Run cargo run -p xtask -- build first.", llama.display());
    }
    let git = |args: &[&str]| output(cmd("git").arg("-C").arg(llama).args(args));
    let tag = git(&["describe", "--tags"]).and_then(|s| s.lines().next().map(str::to_string)).unwrap_or_default();
    if util::is_release_tag(&tag) {
        return Ok(tag);
    }
    ui::warn("llama.cpp checkout is not on a release tag; naming it by build number");
    Ok(match git(&["rev-list", "--count", "HEAD"]).filter(|s| !s.is_empty()) {
        Some(n) => format!("b{n}"),
        None => format!("b0-{}", git(&["rev-parse", "--short", "HEAD"]).unwrap_or_default()),
    })
}
