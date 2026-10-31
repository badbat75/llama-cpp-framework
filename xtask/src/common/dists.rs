//! The SDK dist layout, <root>/<dir>/<version>/<os>, and installer/
//! dist-sources.toml, which names the dirs, markers and download sources.
//! A dist counts only when it sits in that layout, carries its marker AND
//! holds the files the build consumes: an interrupted install is never taken.

use crate::common::util::{self, OS_DIR};
use anyhow::{Context, Result};
use serde::Deserialize;
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

// The download sources are read by the prerequisites phase only, which
// Windows runs natively (scripts\00-install-prerequisites.ps1, dist-sources.psd1);
// macOS reads only the Vulkan and sccache ones (no ROCm, CUDA or NCCL there).
#[derive(Deserialize)]
#[cfg_attr(windows, allow(dead_code))]
pub struct Sources {
    pub dist_root: BTreeMap<String, String>,
    #[cfg_attr(target_os = "macos", allow(dead_code))]
    pub rocm: Rocm,
    #[cfg_attr(target_os = "macos", allow(dead_code))]
    pub cuda: Cuda,
    pub nccl: Nccl,
    pub vulkan: Vulkan,
    pub sccache: Sccache,
}

#[derive(Deserialize)]
#[cfg_attr(not(target_os = "linux"), allow(dead_code))]
pub struct Rocm {
    pub dir: String,
    pub marker: String,
    pub flavor: String,
    pub indexes: Vec<String>,
}

#[derive(Deserialize)]
#[cfg_attr(not(target_os = "linux"), allow(dead_code))]
pub struct Cuda {
    pub dir: String,
    pub marker: String,
    pub redist_index: String,
    pub linux: CudaOs,
}

#[derive(Deserialize)]
#[cfg_attr(not(target_os = "linux"), allow(dead_code))]
pub struct CudaOs {
    pub platform: String,
    pub components: Vec<String>,
}

#[derive(Deserialize)]
#[cfg_attr(not(target_os = "linux"), allow(dead_code))]
pub struct Nccl {
    pub dir: String,
    pub marker: String,
    pub pypi_json: String,
    pub wheel_platform: String,
}

#[derive(Deserialize)]
#[cfg_attr(windows, allow(dead_code))]
pub struct Vulkan {
    pub dir: String,
    pub marker: String,
    #[cfg_attr(not(target_os = "linux"), allow(dead_code))]
    pub linux: VulkanOs,
    #[cfg_attr(not(target_os = "macos"), allow(dead_code))]
    pub macos: VulkanMac,
}

#[derive(Deserialize)]
#[cfg_attr(not(target_os = "linux"), allow(dead_code))]
pub struct VulkanOs {
    pub latest_url: String,
    pub files_json: String,
    pub tarball: String,
}

#[derive(Deserialize)]
#[cfg_attr(not(target_os = "macos"), allow(dead_code))]
pub struct VulkanMac {
    pub latest_url: String,
    pub files_json: String,
    pub zip: String,
}

#[derive(Deserialize)]
#[cfg_attr(windows, allow(dead_code))]
pub struct Sccache {
    pub release_api: String,
}

pub fn load_sources() -> Result<Sources> {
    let path = util::repo_root().join("installer").join("dist-sources.toml");
    let text = std::fs::read_to_string(&path).with_context(|| format!("reading {}", path.display()))?;
    toml::from_str(&text).with_context(|| format!("parsing {}", path.display()))
}

/// The dists of one dist root, as the build sees them.
pub struct Dists {
    pub root: PathBuf,
    pub sources: Sources,
}

impl Dists {
    /// The dist root in use: an explicit one wins (made absolute), then
    /// LLAMA_FW_DIST_ROOT, then the one the configure phase recorded, then
    /// the default in dist-sources.toml.
    pub fn resolve(explicit: Option<&Path>, recorded: Option<&str>) -> Result<Dists> {
        let sources = load_sources()?;
        let root = if let Some(p) = explicit {
            util::absolute(p)
        } else if let Some(env) = std::env::var_os("LLAMA_FW_DIST_ROOT").filter(|v| !v.is_empty()) {
            PathBuf::from(env)
        } else if let Some(r) = recorded.filter(|r| !r.is_empty()) {
            PathBuf::from(r)
        } else {
            let default = sources
                .dist_root
                .get(OS_DIR)
                .with_context(|| format!("dist-sources.toml has no dist_root.{OS_DIR}"))?;
            match default.strip_prefix("~/") {
                Some(rest) => util::home().join(rest),
                None => PathBuf::from(default),
            }
        };
        Ok(Dists { root, sources })
    }

    #[cfg_attr(target_os = "macos", allow(dead_code))]
    pub fn rocm_root(&self) -> PathBuf {
        self.root.join(&self.sources.rocm.dir)
    }
    #[cfg_attr(target_os = "macos", allow(dead_code))]
    pub fn cuda_root(&self) -> PathBuf {
        self.root.join(&self.sources.cuda.dir)
    }
    #[cfg_attr(not(target_os = "linux"), allow(dead_code))]
    pub fn nccl_root(&self) -> PathBuf {
        self.root.join(&self.sources.nccl.dir)
    }
    pub fn vulkan_root(&self) -> PathBuf {
        self.root.join(&self.sources.vulkan.dir)
    }
}

/// The newest COMPLETE dist under a root: the highest <root>/<X.Y...>/<os>
/// that holds every file in `required` (relative to the dist dir).
pub fn newest_dist(root: &Path, required: &[&str]) -> Option<PathBuf> {
    let mut best: Option<(String, PathBuf)> = None;
    for entry in std::fs::read_dir(root).ok()?.flatten() {
        let ver = entry.file_name().to_string_lossy().into_owned();
        if !util::is_dotted_version(&ver) {
            continue;
        }
        let d = entry.path().join(OS_DIR);
        if !d.is_dir() || !required.iter().all(|rel| d.join(rel).exists()) {
            continue;
        }
        if best.as_ref().map(|(b, _)| util::version_gt(&ver, b)).unwrap_or(true) {
            best = Some((ver, d));
        }
    }
    best.map(|(_, d)| d)
}

/// <root>/<version>/<os>[/x86_64 | /macOS] -> <version> (the Vulkan SDK proper
/// sits one level down: x86_64/ on Linux, macOS/ on macOS)
#[cfg_attr(windows, allow(dead_code))]
pub fn dist_version(dir: &Path) -> String {
    let d = if dir.file_name().map(|n| n == "x86_64" || n == "macOS").unwrap_or(false) {
        dir.parent().unwrap_or(dir)
    } else {
        dir
    };
    d.parent()
        .and_then(|p| p.file_name())
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_default()
}
