//! The packages a Linux or macOS build splits into, and which one owns each
//! staged file. One staged tree goes in, one tree per package comes out, so
//! no file can land in two packages or in none.
//!
//! - `config`: llama-cpp-config (one binary, GUI and CLI), its language files,
//!   the benchmark method, the desktop entry.
//! - `llama`: llama.cpp's executables and libraries, every ggml backend, and
//!   libomp: the dist's OpenMP runtime, which ggml-base itself links, so it
//!   comes with llama.cpp and not with ROCm.
//! - `rocm`, `cuda`, `vulkan`: the external runtimes and nothing else. ggml
//!   loads a backend at start and skips it when its runtime is missing, so
//!   each is optional, and each is versioned by its SDK: a new llama.cpp on the
//!   same SDK leaves them as they are.

use anyhow::{Context, Result};
use std::path::{Path, PathBuf};

pub const BASE_NAME: &str = "llama-cpp-framework";

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Component {
    Config,
    Llama,
    Rocm,
    Cuda,
    Vulkan,
}

impl Component {
    pub const ALL: [Component; 5] =
        [Component::Config, Component::Llama, Component::Rocm, Component::Cuda, Component::Vulkan];
    pub const RUNTIMES: [Component; 3] = [Component::Rocm, Component::Cuda, Component::Vulkan];

    pub fn suffix(self) -> &'static str {
        match self {
            Component::Config => "config",
            Component::Llama => "llama",
            Component::Rocm => "rocm",
            Component::Cuda => "cuda",
            Component::Vulkan => "vulkan",
        }
    }

    /// The package's name, which its file name starts with too.
    pub fn package(self) -> String {
        format!("{BASE_NAME}-{}", self.suffix())
    }

    pub fn is_runtime(self) -> bool {
        Component::RUNTIMES.contains(&self)
    }
}

/// The package that owns a staged file, by its path under the package root.
pub fn of(rel: &str) -> Component {
    let path = rel.trim_start_matches('/');
    let name = path.rsplit('/').next().unwrap_or(path);
    if let Some(inner) = path.strip_prefix("opt/llama.cpp/") {
        if inner == "bin/llama-cpp-config" || inner.starts_with("share/") {
            return Component::Config;
        }
        if inner.starts_with(".kpack/") {
            return Component::Rocm;
        }
        if let Some(lib) = inner.strip_prefix("lib/") {
            if lib.starts_with("llvm/lib/libomp.so") {
                return Component::Llama;
            }
            if name.starts_with("libcu") || name.starts_with("libnccl") {
                return Component::Cuda;
            }
            // macOS bundles nothing but the Vulkan SDK's dylibs.
            if name.starts_with("libvulkan") || name.ends_with(".dylib") {
                return Component::Vulkan;
            }
            return Component::Rocm;
        }
        return Component::Llama;
    }
    if path.starts_with("usr/local/share/vulkan/") {
        return Component::Vulkan;
    }
    if name == "llama-cpp-config" || path.starts_with("usr/share/") {
        return Component::Config;
    }
    Component::Llama
}

/// The root a component's tree is split into.
pub fn root(into: &Path, c: Component) -> PathBuf {
    into.join(c.suffix())
}

/// Splits the staged tree into `<into>/<suffix>/`, hard links so nothing is
/// copied and the staged tree stays whole. Returns the components that got a
/// file, in [`Component::ALL`] order.
pub fn split(stage: &Path, into: &Path) -> Result<Vec<Component>> {
    let mut present = Vec::new();
    let mut dirs = vec![stage.to_path_buf()];
    while let Some(dir) = dirs.pop() {
        for entry in std::fs::read_dir(&dir).with_context(|| format!("reading {}", dir.display()))? {
            let entry = entry?;
            let path = entry.path();
            let kind = entry.file_type()?;
            if kind.is_dir() {
                dirs.push(path);
                continue;
            }
            let rel = path.strip_prefix(stage)?;
            let component = of(&rel.to_string_lossy());
            let dest = root(into, component).join(rel);
            std::fs::create_dir_all(dest.parent().unwrap())?;
            if kind.is_symlink() {
                std::os::unix::fs::symlink(std::fs::read_link(&path)?, &dest)?;
            } else {
                std::fs::hard_link(&path, &dest).with_context(|| format!("linking {}", path.display()))?;
            }
            if !present.contains(&component) {
                present.push(component);
            }
        }
    }
    present.sort();
    Ok(present)
}

/// A package file's name without architecture and extension. llama-cpp-config and
/// llama.cpp name the framework and the llama.cpp release; a runtime names its SDK.
pub fn file_stem(c: Component, fw_version: &str, llama_build: &str, version: &str) -> String {
    if c.is_runtime() {
        format!("{}-v{version}", c.package())
    } else {
        format!("{}-v{fw_version}-{llama_build}", c.package())
    }
}

/// The newest file in `dir` of each component, for this architecture and
/// extension (`-amd64.deb`): the last one written. The single package of older
/// releases has another name and never matches.
pub fn newest_in(dir: &Path, tail: &str) -> Vec<(Component, PathBuf)> {
    let files: Vec<PathBuf> =
        std::fs::read_dir(dir).map(|rd| rd.flatten().map(|e| e.path()).collect()).unwrap_or_default();
    Component::ALL
        .iter()
        .filter_map(|c| {
            let prefix = format!("{}-v", c.package());
            files
                .iter()
                .filter(|p| {
                    let name = p.file_name().unwrap_or_default().to_string_lossy();
                    name.starts_with(&prefix) && name.ends_with(tail)
                })
                .max_by_key(|p| std::fs::metadata(p).and_then(|m| m.modified()).ok())
                .map(|p| (*c, p.clone()))
        })
        .collect()
}

/// The version of a dist, from its `<Name>/<Version>/<os>[/<arch>]` path: the
/// deepest directory that starts with a digit. None for an SDK outside the
/// layout (a `HIP_PATH` such as /opt/rocm).
pub fn dist_version(dist: &Path) -> Option<String> {
    dist.components().rev().find_map(|c| {
        let name = c.as_os_str().to_string_lossy();
        // "-" is not allowed in an rpm Version.
        name.starts_with(|ch: char| ch.is_ascii_digit()).then(|| name.replace('-', "."))
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn linux_files_go_to_their_package() {
        for (rel, want) in [
            ("opt/llama.cpp/bin/llama-cpp-config", Component::Config),
            ("opt/llama.cpp/share/strings/en_US.toml", Component::Config),
            ("opt/llama.cpp/share/benchmarks/standard-1.1.ini", Component::Config),
            ("usr/bin/llama-cpp-config", Component::Config),
            ("usr/share/applications/llama-cpp-config.desktop", Component::Config),
            ("usr/share/icons/hicolor/256x256/apps/llama-cpp-config.png", Component::Config),
            ("opt/llama.cpp/bin/llama-server", Component::Llama),
            ("opt/llama.cpp/bin/libggml-base.so.0", Component::Llama),
            ("opt/llama.cpp/bin/libggml-hip.so", Component::Llama),
            ("opt/llama.cpp/bin/libggml-cuda.so", Component::Llama),
            ("opt/llama.cpp/bin/libggml-vulkan.so", Component::Llama),
            ("opt/llama.cpp/lib/llvm/lib/libomp.so", Component::Llama),
            ("usr/bin/llama-server", Component::Llama),
            ("opt/llama.cpp/lib/libamdhip64.so.7", Component::Rocm),
            ("opt/llama.cpp/lib/llvm/lib/libLLVM.so.24.0git", Component::Rocm),
            ("opt/llama.cpp/lib/rocm_sysdeps/lib/librocm_sysdeps_numa.so.1", Component::Rocm),
            ("opt/llama.cpp/lib/rocblas/library/Kernels.so-000-gfx1201.dat", Component::Rocm),
            ("opt/llama.cpp/.kpack/blas_lib_gfx1201.kpack", Component::Rocm),
            ("opt/llama.cpp/lib/libcublas.so.13", Component::Cuda),
            ("opt/llama.cpp/lib/libcublasLt.so.13", Component::Cuda),
            ("opt/llama.cpp/lib/libcudart.so.13", Component::Cuda),
            ("opt/llama.cpp/lib/libnccl.so.2", Component::Cuda),
            ("opt/llama.cpp/lib/libvulkan.so.1", Component::Vulkan),
        ] {
            assert_eq!(of(rel), want, "{rel}");
        }
    }

    #[test]
    fn macos_files_go_to_their_package() {
        for (rel, want) in [
            ("usr/local/bin/llama-cpp-config", Component::Config),
            ("usr/local/bin/llama-server", Component::Llama),
            ("opt/llama.cpp/bin/libggml-metal.so", Component::Llama),
            ("opt/llama.cpp/lib/libvulkan.1.dylib", Component::Vulkan),
            ("opt/llama.cpp/lib/libMoltenVK.dylib", Component::Vulkan),
            ("usr/local/share/vulkan/icd.d/llama-cpp-framework-MoltenVK_icd.json", Component::Vulkan),
        ] {
            assert_eq!(of(rel), want, "{rel}");
        }
    }

    #[test]
    fn file_names_carry_the_release_or_the_sdk() {
        assert_eq!(
            file_stem(Component::Llama, "2.3.0", "v0.9.0", "2.3.0+llama.cpp.v0.9.0"),
            "llama-cpp-framework-llama-v2.3.0-v0.9.0"
        );
        assert_eq!(file_stem(Component::Rocm, "2.3.0", "v0.9.0", "10.1.0"), "llama-cpp-framework-rocm-v10.1.0");
    }

    #[test]
    fn the_newest_file_of_each_component_wins_and_the_old_package_never_matches() {
        let dir = std::env::temp_dir().join(format!("xtask-newest-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        for file in [
            "llama-cpp-framework-v2.2.0-v0.8.0-amd64.deb",
            "llama-cpp-framework-rocm-v10.0.0-amd64.deb",
            "llama-cpp-framework-rocm-v10.1.0-x86_64.rpm",
        ] {
            std::fs::write(dir.join(file), "").unwrap();
        }
        let found = newest_in(&dir, "-amd64.deb");
        assert_eq!(found.len(), 1);
        assert_eq!(found[0].0, Component::Rocm);
        assert!(found[0].1.ends_with("llama-cpp-framework-rocm-v10.0.0-amd64.deb"));
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn a_dist_version_comes_from_the_layout() {
        for (dist, want) in [
            ("/mnt/data/TheRock/10.1.0/linux", Some("10.1.0")),
            ("/mnt/data/CUDA/13.0.2/linux", Some("13.0.2")),
            ("/mnt/data/VulkanSDK/1.4.328.1/linux/x86_64", Some("1.4.328.1")),
            ("/Users/me/dists/VulkanSDK/1.4.328.1/macos/macOS", Some("1.4.328.1")),
            ("/opt/rocm", None),
        ] {
            assert_eq!(dist_version(Path::new(dist)).as_deref(), want, "{dist}");
        }
    }

    #[test]
    fn split_keeps_every_file_once_and_symlinks_as_symlinks() {
        let dir = std::env::temp_dir().join(format!("xtask-split-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let stage = dir.join("stage");
        let into = dir.join("into");
        for file in ["opt/llama.cpp/bin/llama-server", "opt/llama.cpp/lib/libcublas.so.13"] {
            let path = stage.join(file);
            std::fs::create_dir_all(path.parent().unwrap()).unwrap();
            std::fs::write(path, "x").unwrap();
        }
        std::fs::create_dir_all(stage.join("usr/bin")).unwrap();
        std::os::unix::fs::symlink("../../opt/llama.cpp/bin/llama-server", stage.join("usr/bin/llama-server")).unwrap();

        assert_eq!(split(&stage, &into).unwrap(), [Component::Llama, Component::Cuda]);

        assert!(into.join("llama/opt/llama.cpp/bin/llama-server").is_file());
        assert!(into.join("cuda/opt/llama.cpp/lib/libcublas.so.13").is_file());
        assert!(!into.join("rocm").exists());
        let link = into.join("llama/usr/bin/llama-server");
        assert_eq!(std::fs::read_link(link).unwrap(), Path::new("../../opt/llama.cpp/bin/llama-server"));
        assert!(stage.join("opt/llama.cpp/bin/llama-server").is_file(), "the staged tree stays whole");
        std::fs::remove_dir_all(&dir).unwrap();
    }
}
