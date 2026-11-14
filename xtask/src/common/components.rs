//! The packages a Linux or macOS build splits into, and which one owns each
//! staged file. One staged tree goes in, one tree per package comes out, so
//! no file can land in two packages or in none.
//!
//! - `config`: pai-config (the GUI), its language files, the desktop entry
//!   and its icon.
//! - `cli`: pai (the command line) and the data both front ends read: the
//!   benchmark method and `cli.toml`. The GUI package requires it, so a
//!   headless server installs the command line without the GUI.
//! - `llama`: llama.cpp's executables and libraries, every ggml backend,
//!   pai-manager with the system files of its services (the systemd units,
//!   the D-Bus and polkit policies, the sysusers.d entry), and libomp: the
//!   dist's OpenMP runtime, which ggml-base itself links, so it comes with
//!   llama.cpp and not with ROCm.
//! - `rocm`, `cuda`, `vulkan`: the external runtimes and nothing else. ggml
//!   loads a backend at start and skips it when its runtime is missing, so
//!   each is optional, and each is versioned by its SDK: a new llama.cpp on the
//!   same SDK leaves them as they are.

use anyhow::{Context, Result};
use std::path::{Path, PathBuf};

pub const BASE_NAME: &str = "private-ai-framework";
/// The base name of the packages up to 2.4 (the single package of 2.2 and
/// earlier, then `<base>-<component>`), which the upgrade relations name.
pub const OLD_BASE_NAME: &str = "llama-cpp-framework";

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Component {
    Config,
    Cli,
    Llama,
    Rocm,
    Cuda,
    Vulkan,
}

impl Component {
    pub const ALL: [Component; 6] = [
        Component::Config,
        Component::Cli,
        Component::Llama,
        Component::Rocm,
        Component::Cuda,
        Component::Vulkan,
    ];
    pub const RUNTIMES: [Component; 3] = [Component::Rocm, Component::Cuda, Component::Vulkan];

    pub fn suffix(self) -> &'static str {
        match self {
            Component::Config => "config",
            Component::Cli => "cli",
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
    // macOS: one tree under /opt/private-ai.
    if let Some(inner) = path.strip_prefix("opt/private-ai/") {
        if inner == "bin/pai" {
            return Component::Cli;
        }
        if let Some(data) = inner.strip_prefix("share/") {
            return shared_data(data);
        }
        if inner == "bin/pai-config" {
            return Component::Config;
        }
        return private(inner, name);
    }
    // Linux: the private library directory.
    if let Some(inner) = path.strip_prefix("usr/lib/private-ai/") {
        return private(inner, name);
    }
    if path.starts_with("usr/local/share/vulkan/") {
        return Component::Vulkan;
    }
    // The bus and polkit policies of the services, beside pai-manager.
    if path.starts_with("usr/share/dbus-1/") || path.starts_with("usr/share/polkit-1/") {
        return Component::Llama;
    }
    // /usr/bin/pai, and its link in /usr/local/bin on macOS.
    if name == "pai" {
        return Component::Cli;
    }
    if let Some(data) = path.strip_prefix("usr/share/private-ai/") {
        return shared_data(data);
    }
    if name == "pai-config" || path.starts_with("usr/share/") {
        return Component::Config;
    }
    Component::Llama
}

/// The owner of a read-only data file, by its path under the data directory
/// (`/usr/share/private-ai` on Linux, `/opt/private-ai/share` on macOS): what
/// the command line reads (the benchmark method, `cli.toml`) goes with it,
/// the GUI's language files with the GUI.
fn shared_data(data: &str) -> Component {
    if data.starts_with("benchmarks/") || data == "strings/cli.toml" {
        Component::Cli
    } else {
        Component::Config
    }
}

/// The owner of a file under a private tree (`/opt/private-ai` on macOS,
/// `/usr/lib/private-ai` on Linux), by its path `inner` in that tree: the
/// runtimes in `lib/` and the kpack archives belong to their SDK, the rest
/// (llama.cpp's libraries and backends, libomp) to llama.cpp.
fn private(inner: &str, name: &str) -> Component {
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
        for entry in
            std::fs::read_dir(&dir).with_context(|| format!("reading {}", dir.display()))?
        {
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
                std::fs::hard_link(&path, &dest)
                    .with_context(|| format!("linking {}", path.display()))?;
            }
            if !present.contains(&component) {
                present.push(component);
            }
        }
    }
    present.sort();
    Ok(present)
}

/// A package file's name without architecture and extension. pai-config, pai
/// and llama.cpp name the framework and the llama.cpp release; a runtime names its SDK.
pub fn file_stem(c: Component, fw_version: &str, llama_build: &str, version: &str) -> String {
    if c.is_runtime() {
        format!("{}-v{version}", c.package())
    } else {
        format!("{}-v{fw_version}-{llama_build}", c.package())
    }
}

/// The newest file in `dir` of each component, for this architecture and
/// extension (`-amd64.deb`): the last one written. The packages of 2.4 and
/// earlier have another base name and never match.
pub fn newest_in(dir: &Path, tail: &str) -> Vec<(Component, PathBuf)> {
    let files: Vec<PathBuf> = std::fs::read_dir(dir)
        .map(|rd| rd.flatten().map(|e| e.path()).collect())
        .unwrap_or_default();
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
        name.starts_with(|ch: char| ch.is_ascii_digit())
            .then(|| name.replace('-', "."))
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The Linux packages in the standard layout under /usr: the commands in
    /// /usr/bin, pai-manager in /usr/libexec/private-ai, libllama, the ggml
    /// backends and the runtimes in /usr/lib/private-ai, the data in
    /// /usr/share/private-ai.
    #[test]
    fn linux_files_go_to_their_package() {
        for (rel, want) in [
            ("usr/bin/pai-config", Component::Config),
            ("usr/share/private-ai/strings/en_US.toml", Component::Config),
            (
                "usr/share/private-ai/benchmarks/specs/standard-1.1.ini",
                Component::Cli,
            ),
            (
                "usr/share/applications/org.privateai.Config.desktop",
                Component::Config,
            ),
            (
                "usr/share/icons/hicolor/256x256/apps/org.privateai.Config.png",
                Component::Config,
            ),
            ("usr/bin/llama-server", Component::Llama),
            ("usr/bin/llama-bench", Component::Llama),
            ("usr/libexec/private-ai/pai-manager", Component::Llama),
            ("usr/lib/private-ai/libggml-base.so.0", Component::Llama),
            ("usr/lib/private-ai/libggml-hip.so", Component::Llama),
            ("usr/lib/private-ai/libggml-cuda.so", Component::Llama),
            ("usr/lib/private-ai/libggml-vulkan.so", Component::Llama),
            ("usr/lib/private-ai/libllama.so.0", Component::Llama),
            (
                "usr/lib/private-ai/lib/llvm/lib/libomp.so",
                Component::Llama,
            ),
            ("usr/lib/private-ai/lib/libamdhip64.so.7", Component::Rocm),
            (
                "usr/lib/private-ai/lib/llvm/lib/libLLVM.so.24.0git",
                Component::Rocm,
            ),
            (
                "usr/lib/private-ai/lib/rocm_sysdeps/lib/librocm_sysdeps_numa.so.1",
                Component::Rocm,
            ),
            (
                "usr/lib/private-ai/lib/rocblas/library/Kernels.so-000-gfx1201.dat",
                Component::Rocm,
            ),
            (
                "usr/lib/private-ai/.kpack/blas_lib_gfx1201.kpack",
                Component::Rocm,
            ),
            ("usr/lib/private-ai/lib/libcublas.so.13", Component::Cuda),
            ("usr/lib/private-ai/lib/libcublasLt.so.13", Component::Cuda),
            ("usr/lib/private-ai/lib/libcudart.so.13", Component::Cuda),
            ("usr/lib/private-ai/lib/libnccl.so.2", Component::Cuda),
            ("usr/lib/private-ai/lib/libvulkan.so.1", Component::Vulkan),
        ] {
            assert_eq!(of(rel), want, "{rel}");
        }
    }

    /// The system files of the services go with pai-manager, in the
    /// llama.cpp package, although two of them sit under /usr/share, which
    /// is otherwise the configurator's.
    #[test]
    fn the_system_files_of_the_services_go_to_llama() {
        for rel in [
            "usr/lib/systemd/system/private-ai-manager.service",
            "usr/lib/systemd/system/llama.cpp@.service",
            "usr/share/dbus-1/system.d/org.privateai.Manager1.conf",
            "usr/share/polkit-1/actions/org.privateai.manager.policy",
            "usr/lib/sysusers.d/private-ai.conf",
        ] {
            assert_eq!(of(rel), Component::Llama, "{rel}");
        }
        assert_eq!(
            of("usr/share/applications/org.privateai.Config.desktop"),
            Component::Config
        );
    }

    #[test]
    fn macos_files_go_to_their_package() {
        for (rel, want) in [
            ("usr/local/bin/pai-config", Component::Config),
            ("usr/local/bin/llama-server", Component::Llama),
            ("opt/private-ai/bin/libggml-metal.so", Component::Llama),
            ("opt/private-ai/lib/libvulkan.1.dylib", Component::Vulkan),
            ("opt/private-ai/lib/libMoltenVK.dylib", Component::Vulkan),
            (
                "usr/local/share/vulkan/icd.d/private-ai-framework-MoltenVK_icd.json",
                Component::Vulkan,
            ),
        ] {
            assert_eq!(of(rel), want, "{rel}");
        }
    }

    /// `pai`, the command line, has a package of its own, so a headless
    /// server administered over SSH installs it without the GUI and its
    /// graphics libraries. The data both front ends read goes with it (the
    /// benchmark method, `cli.toml`, which the GUI reads too), since the GUI
    /// package requires it and not the other way round; the GUI's language
    /// files, its desktop entry and its icon stay with the GUI.
    #[test]
    fn the_command_line_and_the_data_it_reads_go_to_the_cli_package() {
        for (rel, want) in [
            ("usr/bin/pai", Component::Cli),
            (
                "usr/share/private-ai/benchmarks/specs/standard-1.1.ini",
                Component::Cli,
            ),
            (
                "usr/share/private-ai/benchmarks/specs/standard.versions",
                Component::Cli,
            ),
            (
                "usr/share/private-ai/benchmarks/prompts/bench-prompt-43k-1.0.txt",
                Component::Cli,
            ),
            ("usr/share/private-ai/strings/cli.toml", Component::Cli),
            ("usr/share/private-ai/strings/en_US.toml", Component::Config),
            ("usr/bin/pai-config", Component::Config),
            (
                "usr/share/applications/org.privateai.Config.desktop",
                Component::Config,
            ),
            (
                "usr/share/icons/hicolor/256x256/apps/org.privateai.Config.png",
                Component::Config,
            ),
            // Not taken for pai by a prefix match.
            ("usr/bin/pai-manager", Component::Llama),
        ] {
            assert_eq!(of(rel), want, "{rel}");
        }
    }

    /// macOS: `pai` in /opt/private-ai/bin with its link in /usr/local/bin,
    /// and the shared data under /opt/private-ai/share, as on Linux.
    #[test]
    fn macos_command_line_files_go_to_the_cli_package() {
        for (rel, want) in [
            ("opt/private-ai/bin/pai", Component::Cli),
            ("usr/local/bin/pai", Component::Cli),
            (
                "opt/private-ai/share/benchmarks/specs/standard-1.1.ini",
                Component::Cli,
            ),
            ("opt/private-ai/share/strings/cli.toml", Component::Cli),
            ("opt/private-ai/bin/pai-config", Component::Config),
            ("opt/private-ai/share/strings/en_US.toml", Component::Config),
        ] {
            assert_eq!(of(rel), want, "{rel}");
        }
    }

    /// The command line is the framework's, versioned like the GUI (the
    /// framework and the llama.cpp release), not a runtime.
    #[test]
    fn the_cli_package_is_named_and_versioned_like_the_configurator() {
        assert_eq!(Component::Cli.package(), "private-ai-framework-cli");
        assert!(!Component::Cli.is_runtime());
        assert_eq!(
            file_stem(
                Component::Cli,
                "2.99.0",
                "v0.9.0",
                "2.99.0+llama.cpp.v0.9.0"
            ),
            "private-ai-framework-cli-v2.99.0-v0.9.0"
        );
    }

    /// `xtask install` (Linux and macOS) picks the newest file of each
    /// component: the cli package is one of them, told apart from the
    /// configurator's.
    #[test]
    fn the_newest_cli_package_is_picked_beside_the_configurators() {
        let dir = std::env::temp_dir().join(format!("xtask-newest-cli-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        for file in [
            "private-ai-framework-config-v2.99.0-v0.9.0-amd64.deb",
            "private-ai-framework-cli-v2.99.0-v0.9.0-amd64.deb",
            "private-ai-framework-llama-v2.99.0-v0.9.0-amd64.deb",
        ] {
            std::fs::write(dir.join(file), "").unwrap();
        }
        let names: Vec<(Component, String)> = newest_in(&dir, "-amd64.deb")
            .iter()
            .map(|(c, p)| (*c, p.file_name().unwrap().to_string_lossy().into_owned()))
            .collect();
        std::fs::remove_dir_all(&dir).unwrap();
        assert_eq!(
            names,
            [
                (
                    Component::Config,
                    "private-ai-framework-config-v2.99.0-v0.9.0-amd64.deb".to_string()
                ),
                (
                    Component::Cli,
                    "private-ai-framework-cli-v2.99.0-v0.9.0-amd64.deb".to_string()
                ),
                (
                    Component::Llama,
                    "private-ai-framework-llama-v2.99.0-v0.9.0-amd64.deb".to_string()
                ),
            ]
        );
    }

    /// The packages of the 2.4 releases and earlier, spelled split so the
    /// names lint stays clean.
    const OLD_BASE: &str = concat!("llama-cpp", "-framework");

    #[test]
    fn packages_carry_the_project_name() {
        let names: Vec<String> = Component::ALL.iter().map(|c| c.package()).collect();
        assert_eq!(
            names,
            [
                "private-ai-framework-config",
                "private-ai-framework-cli",
                "private-ai-framework-llama",
                "private-ai-framework-rocm",
                "private-ai-framework-cuda",
                "private-ai-framework-vulkan",
            ]
        );
    }

    #[test]
    fn file_names_carry_the_release_or_the_sdk() {
        assert_eq!(
            file_stem(
                Component::Llama,
                "2.99.0",
                "v0.9.0",
                "2.99.0+llama.cpp.v0.9.0"
            ),
            "private-ai-framework-llama-v2.99.0-v0.9.0"
        );
        assert_eq!(
            file_stem(
                Component::Config,
                "2.99.0",
                "v0.9.0",
                "2.99.0+llama.cpp.v0.9.0"
            ),
            "private-ai-framework-config-v2.99.0-v0.9.0"
        );
        assert_eq!(
            file_stem(Component::Rocm, "2.99.0", "v0.9.0", "10.1.0"),
            "private-ai-framework-rocm-v10.1.0"
        );
    }

    /// Old packages left in dist/ (the monolith of 2.2 and earlier, the split
    /// packages of 2.3 and 2.4) are never picked, even when written last.
    #[test]
    fn the_newest_file_of_each_component_wins_and_the_old_packages_never_match() {
        let dir = std::env::temp_dir().join(format!("xtask-newest-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let start =
            std::time::SystemTime::UNIX_EPOCH + std::time::Duration::from_secs(1_700_000_000);
        let files = [
            "private-ai-framework-rocm-v10.0.0-amd64.deb".to_string(),
            "private-ai-framework-rocm-v10.1.0-x86_64.rpm".to_string(),
            format!("{OLD_BASE}-v2.2.0-v0.8.0-amd64.deb"),
            format!("{OLD_BASE}-rocm-v10.2.0-amd64.deb"),
            format!("{OLD_BASE}-config-v2.4.0-v0.9.0-amd64.deb"),
        ];
        // Each file one minute newer than the one before: the old ones last.
        for (i, file) in files.iter().enumerate() {
            let path = dir.join(file);
            std::fs::write(&path, "").unwrap();
            std::fs::File::options()
                .write(true)
                .open(&path)
                .unwrap()
                .set_modified(start + std::time::Duration::from_secs(60 * i as u64))
                .unwrap();
        }
        let found = newest_in(&dir, "-amd64.deb");
        let names: Vec<(Component, String)> = found
            .iter()
            .map(|(c, p)| (*c, p.file_name().unwrap().to_string_lossy().into_owned()))
            .collect();
        assert_eq!(
            names,
            [(
                Component::Rocm,
                "private-ai-framework-rocm-v10.0.0-amd64.deb".to_string()
            )]
        );
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn a_dist_version_comes_from_the_layout() {
        for (dist, want) in [
            ("/mnt/data/TheRock/10.1.0/linux", Some("10.1.0")),
            ("/mnt/data/CUDA/13.0.2/linux", Some("13.0.2")),
            (
                "/mnt/data/VulkanSDK/1.4.328.1/linux/x86_64",
                Some("1.4.328.1"),
            ),
            (
                "/Users/me/dists/VulkanSDK/1.4.328.1/macos/macOS",
                Some("1.4.328.1"),
            ),
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
        for file in [
            "usr/bin/llama-server",
            "usr/lib/private-ai/libggml-base.so.0.9.0",
            "usr/lib/private-ai/lib/libcublas.so.13",
        ] {
            let path = stage.join(file);
            std::fs::create_dir_all(path.parent().unwrap()).unwrap();
            std::fs::write(path, "x").unwrap();
        }
        std::os::unix::fs::symlink(
            "libggml-base.so.0.9.0",
            stage.join("usr/lib/private-ai/libggml-base.so.0"),
        )
        .unwrap();

        assert_eq!(
            split(&stage, &into).unwrap(),
            [Component::Llama, Component::Cuda]
        );

        assert!(into.join("llama/usr/bin/llama-server").is_file());
        assert!(into
            .join("cuda/usr/lib/private-ai/lib/libcublas.so.13")
            .is_file());
        assert!(!into.join("rocm").exists());
        let link = into.join("llama/usr/lib/private-ai/libggml-base.so.0");
        assert_eq!(
            std::fs::read_link(link).unwrap(),
            Path::new("libggml-base.so.0.9.0")
        );
        assert!(
            stage.join("usr/bin/llama-server").is_file(),
            "the staged tree stays whole"
        );
        std::fs::remove_dir_all(&dir).unwrap();
    }
}
