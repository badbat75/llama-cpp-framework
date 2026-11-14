//! The packages a Linux or macOS build splits into, and which one owns each
//! staged file. One staged tree goes in, one tree per package comes out, so
//! no file can land in two packages or in none.
//!
//! - `config`: pai-config (the GUI), its language files, the desktop entry
//!   and its icon.
//! - `cli`: pai (the command line) and the benchmark method, which both
//!   front ends read. The GUI package requires it, so a headless server
//!   installs the command line without the GUI.
//! - `bff`: pai-bff, the web service, and its systemd unit (Linux only).
//! - `core`: pai-manager with the system files of its service (its systemd
//!   unit, the D-Bus and polkit policies, the sysusers.d entry) and
//!   `cli.toml` (the texts every program prints); every other framework
//!   package requires this one.
//! - `llama`: the engine, package `llama.cpp`: llama.cpp's executables and
//!   libraries, every ggml backend, and libomp: the dist's OpenMP runtime,
//!   which ggml-base itself links, so it comes with llama.cpp and not with
//!   ROCm; and its instance unit `llama.cpp@.service`. Nothing else of the
//!   framework.
//! - `rocm`, `cuda`, `vulkan`: the external runtimes and nothing else, the
//!   engine's (`llama.cpp-<sdk>`). ggml
//!   loads a backend at start and skips it when its runtime is missing, so
//!   each is optional, and each is versioned by its SDK: a new llama.cpp on the
//!   same SDK leaves them as they are.

use anyhow::{Context, Result};
use std::path::{Path, PathBuf};

pub const BASE_NAME: &str = "private-ai-framework";
/// The base name of the packages up to 2.4 (the single package of 2.2 and
/// earlier, then `<base>-<component>`), which the upgrade relations name.
pub const OLD_BASE_NAME: &str = "llama-cpp-framework";
/// The engine's package name, which its runtimes' names start with too.
pub const ENGINE_NAME: &str = "llama.cpp";

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Component {
    Config,
    Cli,
    Bff,
    Core,
    Llama,
    Rocm,
    Cuda,
    Vulkan,
}

impl Component {
    pub const ALL: [Component; 8] = [
        Component::Config,
        Component::Cli,
        Component::Bff,
        Component::Core,
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
            Component::Bff => "bff",
            Component::Core => "core",
            Component::Llama => "llama",
            Component::Rocm => "rocm",
            Component::Cuda => "cuda",
            Component::Vulkan => "vulkan",
        }
    }

    /// The package's name, which its file name starts with too: the
    /// engine is named after itself (`llama.cpp`) and its runtimes after it,
    /// the framework's packages after the project.
    pub fn package(self) -> String {
        match self {
            Component::Llama => ENGINE_NAME.to_string(),
            c if c.is_runtime() => format!("{ENGINE_NAME}-{}", c.suffix()),
            c => format!("{BASE_NAME}-{}", c.suffix()),
        }
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
    // The web service, its unit and the sysusers.d entry of its user.
    if path == "usr/libexec/private-ai/pai-bff"
        || path == "usr/lib/systemd/system/private-ai-bff.service"
        || path == "usr/lib/sysusers.d/private-ai-bff.conf"
    {
        return Component::Bff;
    }
    // The engine's instance unit (the engine owns its unit) and its shipped
    // defaults, copied into /etc/llama.cpp by its maintainer scripts.
    if path == "usr/lib/systemd/system/llama.cpp@.service"
        || path.starts_with("usr/share/private-ai/llama.cpp/")
    {
        return Component::Llama;
    }
    // pai-manager and the system files of its services.
    if path == "usr/libexec/private-ai/pai-manager"
        || path.starts_with("usr/lib/systemd/")
        || path.starts_with("usr/lib/sysusers.d/")
        || path.starts_with("usr/share/dbus-1/")
        || path.starts_with("usr/share/polkit-1/")
    {
        return Component::Core;
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
/// (`/usr/share/private-ai` on Linux, `/opt/private-ai/share` on macOS): the
/// benchmark method goes with the command line, `cli.toml` (read by pai,
/// pai-manager and pai-bff) with the core, which every other framework
/// package requires, the GUI's language files with the GUI.
fn shared_data(data: &str) -> Component {
    if data == "strings/cli.toml" {
        Component::Core
    } else if data.starts_with("benchmarks/") {
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

/// The build revision of the llama.cpp package (`+pai<N>` in its version):
/// raised when the patches or the build options change on the same
/// llama.cpp release.
pub const LLAMA_CPP_REVISION: u32 = 1;

/// The version of the llama.cpp package built from release `llama_build`
/// (`v1.0.0`): `1:1.0.0+pai<LLAMA_CPP_REVISION>`.
pub fn llama_cpp_version(llama_build: &str) -> String {
    format!("{}+pai{LLAMA_CPP_REVISION}", llama_cpp_min(llama_build))
}

/// The lowest llama.cpp package version a framework package built against
/// release `llama_build` (`v1.0.0`) accepts: `1:1.0.0`.
pub fn llama_cpp_min(llama_build: &str) -> String {
    format!("1:{}", llama_build.trim_start_matches('v'))
}

/// A package file's name without architecture and extension. A framework
/// package names the framework version, the engine its own `version` without
/// the epoch (no `:` in a file name), a runtime its SDK.
pub fn file_stem(c: Component, fw_version: &str, _llama_build: &str, version: &str) -> String {
    if c.is_runtime() {
        format!("{}-v{version}", c.package())
    } else if c == Component::Llama {
        let bare = version.split_once(':').map_or(version, |(_, v)| v);
        format!("{}-v{bare}", c.package())
    } else {
        format!("{}-v{fw_version}", c.package())
    }
}

/// Whether `name` is a package file of `c`: `<package>-v` and a version,
/// which starts with a digit, so `llama.cpp-v...` never takes
/// `llama.cpp-vulkan-v...`.
pub fn is_file_of(c: Component, name: &str) -> bool {
    name.strip_prefix(&format!("{}-v", c.package()))
        .is_some_and(|rest| rest.starts_with(|ch: char| ch.is_ascii_digit()))
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
            files
                .iter()
                .filter(|p| {
                    let name = p.file_name().unwrap_or_default().to_string_lossy();
                    is_file_of(*c, &name) && name.ends_with(tail)
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
            ("usr/libexec/private-ai/pai-manager", Component::Core),
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

    // ── The framework's core and the engine ─────────────────────────────

    /// `private-ai-framework-core` holds what the framework put in the
    /// llama.cpp package before: pai-manager, the system files of its
    /// service, and `cli.toml`, the texts pai-manager, pai and pai-bff
    /// print. The instance unit `llama.cpp@.service` is the engine's, the
    /// bff's unit and the sysusers.d entry of its user stay with pai-bff,
    /// the desktop entry with the GUI, although they sit beside these.
    #[test]
    fn core_holds_the_manager_its_system_files_and_cli_toml() {
        for rel in [
            "usr/libexec/private-ai/pai-manager",
            "usr/lib/systemd/system/private-ai-manager.service",
            "usr/share/dbus-1/system.d/org.privateai.Manager1.conf",
            "usr/share/polkit-1/actions/org.privateai.manager.policy",
            "usr/lib/sysusers.d/private-ai.conf",
            "usr/share/private-ai/strings/cli.toml",
        ] {
            assert_eq!(of(rel), Component::Core, "{rel}");
        }
        for (rel, want) in [
            (
                "usr/lib/systemd/system/llama.cpp@.service",
                Component::Llama,
            ),
            (
                "usr/lib/systemd/system/private-ai-bff.service",
                Component::Bff,
            ),
            ("usr/lib/sysusers.d/private-ai-bff.conf", Component::Bff),
            (
                "usr/share/applications/org.privateai.Config.desktop",
                Component::Config,
            ),
            ("usr/share/private-ai/strings/en_US.toml", Component::Config),
        ] {
            assert_eq!(of(rel), want, "{rel}");
        }
    }

    /// The engine's package holds llama.cpp's executables and libraries,
    /// every ggml backend and libomp, and its instance unit
    /// `llama.cpp@.service` (the engine owns its unit), and nothing else of
    /// the framework: no program, other unit, policy or data file.
    #[test]
    fn the_engine_holds_llama_cpp_and_nothing_of_the_framework() {
        for rel in [
            "usr/bin/llama-server",
            "usr/bin/llama-bench",
            "usr/bin/llama",
            "usr/lib/private-ai/libllama.so.1",
            "usr/lib/private-ai/libggml-base.so.1",
            "usr/lib/private-ai/libggml-cpu-zen4.so",
            "usr/lib/private-ai/libggml-hip.so",
            "usr/lib/private-ai/libmtmd.so.1",
            "usr/lib/private-ai/libllama-server-impl.so",
            "usr/lib/private-ai/lib/llvm/lib/libomp.so",
            "usr/lib/systemd/system/llama.cpp@.service",
            // The engine's defaults, copied into /etc/llama.cpp when missing.
            "usr/share/private-ai/llama.cpp/server@default.ini",
            "usr/share/private-ai/llama.cpp/presets.ini",
        ] {
            assert_eq!(of(rel), Component::Llama, "{rel}");
        }
        for rel in [
            "usr/bin/pai-config",
            "usr/bin/pai",
            "usr/libexec/private-ai/pai-manager",
            "usr/libexec/private-ai/pai-bff",
            "usr/lib/systemd/system/private-ai-manager.service",
            "usr/lib/systemd/system/private-ai-bff.service",
            "usr/share/dbus-1/system.d/org.privateai.Manager1.conf",
            "usr/share/polkit-1/actions/org.privateai.manager.policy",
            "usr/lib/sysusers.d/private-ai.conf",
            "usr/lib/sysusers.d/private-ai-bff.conf",
            "usr/share/private-ai/strings/cli.toml",
            "usr/share/private-ai/strings/en_US.toml",
            "usr/share/private-ai/benchmarks/specs/standard-1.1.ini",
            "usr/share/applications/org.privateai.Config.desktop",
        ] {
            assert_ne!(of(rel), Component::Llama, "{rel}");
        }
    }

    /// The engine is named after itself, `llama.cpp`, and its runtimes
    /// belong to it (`llama.cpp-rocm`, `-cuda`, `-vulkan`); the framework's
    /// packages, `core` among them, carry the project name.
    #[test]
    fn packages_carry_the_project_or_the_engine_name() {
        let names: Vec<String> = Component::ALL.iter().map(|c| c.package()).collect();
        assert_eq!(
            names,
            [
                "private-ai-framework-config",
                "private-ai-framework-cli",
                "private-ai-framework-bff",
                "private-ai-framework-core",
                "llama.cpp",
                "llama.cpp-rocm",
                "llama.cpp-cuda",
                "llama.cpp-vulkan",
            ]
        );
        assert!(!Component::Core.is_runtime());
        assert!(!Component::Llama.is_runtime());
    }

    /// The engine's version is `1:<release>+pai<N>`: the llama.cpp release
    /// without its `v`, `+pai<N>` our build revision ([`LLAMA_CPP_REVISION`],
    /// checked in, starting at 1), and the epoch `1:` because Ubuntu 26.04
    /// ships its own `llama.cpp` at `8681+dfsg-1`, which would rank above
    /// `1.0.0` without it (Debian Policy 5.6.12: the epoch is compared
    /// first). The framework requires at least `1:<release>`, so any build
    /// revision of that release, or a later release, satisfies it.
    #[test]
    fn the_engine_version_carries_the_epoch_and_the_build_revision() {
        assert_eq!(LLAMA_CPP_REVISION, 1);
        assert_eq!(llama_cpp_version("v1.0.0"), "1:1.0.0+pai1");
        assert_eq!(llama_cpp_version("v1.2.10"), "1:1.2.10+pai1");
        assert_eq!(llama_cpp_min("v1.0.0"), "1:1.0.0");
        assert_eq!(llama_cpp_min("v1.2.10"), "1:1.2.10");
    }

    /// File names: a framework package names the framework version alone
    /// (no llama.cpp tag), the engine its own version without the epoch
    /// (a `:` has no place in a file name; dpkg-name leaves the epoch out
    /// too), a runtime its SDK. The same stem for the `.deb` and the `.rpm`.
    #[test]
    fn file_names_carry_the_framework_the_engine_or_the_sdk_version() {
        for (c, version, want) in [
            (
                Component::Config,
                "2.99.1",
                "private-ai-framework-config-v2.99.1",
            ),
            (Component::Cli, "2.99.1", "private-ai-framework-cli-v2.99.1"),
            (Component::Bff, "2.99.1", "private-ai-framework-bff-v2.99.1"),
            (
                Component::Core,
                "2.99.1",
                "private-ai-framework-core-v2.99.1",
            ),
            (Component::Llama, "1:1.0.0+pai1", "llama.cpp-v1.0.0+pai1"),
            (Component::Rocm, "10.1.0", "llama.cpp-rocm-v10.1.0"),
            (
                Component::Cuda,
                "13.5.0+nccl.2.32.3",
                "llama.cpp-cuda-v13.5.0+nccl.2.32.3",
            ),
            (
                Component::Vulkan,
                "1.4.363.1",
                "llama.cpp-vulkan-v1.4.363.1",
            ),
        ] {
            assert_eq!(file_stem(c, "2.99.1", "v1.0.0", version), want, "{c:?}");
        }
    }

    /// `xtask install` (Linux and macOS) picks the newest file of each
    /// package: the engine's `llama.cpp-v...` is told apart from its
    /// runtimes' `llama.cpp-rocm-v...`, and `core` from the others.
    #[test]
    fn the_newest_engine_core_and_runtime_are_picked_apart() {
        let dir = std::env::temp_dir().join(format!("xtask-newest-engine-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        for file in [
            "private-ai-framework-core-v2.99.1-x86_64.rpm",
            "llama.cpp-v1.0.0+pai1-x86_64.rpm",
            "llama.cpp-rocm-v10.1.0-x86_64.rpm",
        ] {
            std::fs::write(dir.join(file), "").unwrap();
        }
        let found: Vec<(Component, String)> = newest_in(&dir, "-x86_64.rpm")
            .iter()
            .map(|(c, p)| (*c, p.file_name().unwrap().to_string_lossy().into_owned()))
            .collect();
        std::fs::remove_dir_all(&dir).unwrap();
        assert_eq!(
            found,
            [
                (
                    Component::Core,
                    "private-ai-framework-core-v2.99.1-x86_64.rpm".to_string()
                ),
                (
                    Component::Llama,
                    "llama.cpp-v1.0.0+pai1-x86_64.rpm".to_string()
                ),
                (
                    Component::Rocm,
                    "llama.cpp-rocm-v10.1.0-x86_64.rpm".to_string()
                ),
            ]
        );
    }

    /// The real dist/ of the split, both formats side by side: each
    /// package gets its own file. The engine's prefix `llama.cpp-v` is also
    /// the start of `llama.cpp-vulkan-v...`, so a match on `<package>-v`
    /// alone hands the Vulkan runtime to the engine's slot (seen with
    /// `xtask install --dry-run` on the real dist/, which listed the Vulkan
    /// file twice and never the engine). The version after `-v` starts with
    /// a digit. macOS's `.pkg` finder is this same function.
    #[test]
    fn each_package_gets_its_own_file_when_the_engine_prefix_starts_a_runtime_name() {
        let dir = std::env::temp_dir().join(format!("xtask-newest-real-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let stems = [
            (Component::Llama, "llama.cpp-v1.0.0+pai1"),
            (Component::Rocm, "llama.cpp-rocm-v10.1.0"),
            (Component::Cuda, "llama.cpp-cuda-v13.5.0+nccl.2.32.3"),
            (Component::Vulkan, "llama.cpp-vulkan-v1.4.363.1"),
        ];
        // The engine first, so it is the oldest: newest-by-time cannot hide
        // the bug.
        let start =
            std::time::SystemTime::UNIX_EPOCH + std::time::Duration::from_secs(1_700_000_000);
        let mut n = 0;
        for (_, stem) in &stems {
            for tail in ["-amd64.deb", "-x86_64.rpm"] {
                let path = dir.join(format!("{stem}{tail}"));
                std::fs::write(&path, "").unwrap();
                std::fs::File::options()
                    .write(true)
                    .open(&path)
                    .unwrap()
                    .set_modified(start + std::time::Duration::from_secs(60 * n))
                    .unwrap();
                n += 1;
            }
        }
        let picked = |tail: &str| -> Vec<(Component, String)> {
            newest_in(&dir, tail)
                .iter()
                .map(|(c, p)| (*c, p.file_name().unwrap().to_string_lossy().into_owned()))
                .collect()
        };
        let (deb, rpm) = (picked("-amd64.deb"), picked("-x86_64.rpm"));
        std::fs::remove_dir_all(&dir).unwrap();
        for (tail, got) in [("-amd64.deb", deb), ("-x86_64.rpm", rpm)] {
            let want: Vec<(Component, String)> = stems
                .iter()
                .map(|(c, stem)| (*c, format!("{stem}{tail}")))
                .collect();
            assert_eq!(got, want, "{tail}");
        }
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
    /// graphics libraries. The benchmark method, which the GUI reads too,
    /// goes with it, since the GUI package requires it and not the other way
    /// round; `cli.toml` goes to the core package (below); the GUI's
    /// language files, its desktop entry and its icon stay with the GUI.
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
            ("usr/share/private-ai/strings/cli.toml", Component::Core),
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
        ] {
            assert_eq!(of(rel), want, "{rel}");
        }
        // Not taken for pai by a prefix match.
        assert_ne!(of("usr/bin/pai-manager"), Component::Cli);
    }

    /// macOS: `pai` in /opt/private-ai/bin with its link in /usr/local/bin,
    /// and the benchmark method under /opt/private-ai/share, as on Linux.
    /// (Where macOS puts `cli.toml` comes with its own split of the `.pkg`.)
    #[test]
    fn macos_command_line_files_go_to_the_cli_package() {
        for (rel, want) in [
            ("opt/private-ai/bin/pai", Component::Cli),
            ("usr/local/bin/pai", Component::Cli),
            (
                "opt/private-ai/share/benchmarks/specs/standard-1.1.ini",
                Component::Cli,
            ),
            ("opt/private-ai/bin/pai-config", Component::Config),
            ("opt/private-ai/share/strings/en_US.toml", Component::Config),
        ] {
            assert_eq!(of(rel), want, "{rel}");
        }
    }

    /// The command line is the framework's, versioned like the GUI (the
    /// framework version alone), not a runtime.
    #[test]
    fn the_cli_package_is_named_and_versioned_like_the_configurator() {
        assert_eq!(Component::Cli.package(), "private-ai-framework-cli");
        assert!(!Component::Cli.is_runtime());
        assert_eq!(
            file_stem(Component::Cli, "2.99.1", "v1.0.0", "2.99.1"),
            "private-ai-framework-cli-v2.99.1"
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
            "private-ai-framework-config-v2.99.1-amd64.deb",
            "private-ai-framework-cli-v2.99.1-amd64.deb",
            "llama.cpp-v1.0.0+pai1-amd64.deb",
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
                    "private-ai-framework-config-v2.99.1-amd64.deb".to_string()
                ),
                (
                    Component::Cli,
                    "private-ai-framework-cli-v2.99.1-amd64.deb".to_string()
                ),
                (
                    Component::Llama,
                    "llama.cpp-v1.0.0+pai1-amd64.deb".to_string()
                ),
            ]
        );
    }

    // ── pai-bff, the web service, in a package of its own ───────────────

    /// pai-bff has a package of its own, `private-ai-framework-bff`: it is
    /// a service, so it sits beside pai-manager in /usr/libexec/private-ai
    /// (no user command), and its unit and the sysusers.d entry of its
    /// static user go with it, not with the manager's in the core package.
    /// A desktop install and a headless server each choose it apart from
    /// the GUI and the command line.
    #[test]
    fn pai_bff_and_its_unit_go_to_the_bff_package() {
        for (rel, want) in [
            ("usr/libexec/private-ai/pai-bff", Component::Bff),
            (
                "usr/lib/systemd/system/private-ai-bff.service",
                Component::Bff,
            ),
            // The sysusers.d entry of the bff's static user goes with it.
            ("usr/lib/sysusers.d/private-ai-bff.conf", Component::Bff),
            // The manager and its files go to the core package.
            ("usr/libexec/private-ai/pai-manager", Component::Core),
            (
                "usr/lib/systemd/system/private-ai-manager.service",
                Component::Core,
            ),
            (
                "usr/lib/systemd/system/llama.cpp@.service",
                Component::Llama,
            ),
            ("usr/lib/sysusers.d/private-ai.conf", Component::Core),
            ("usr/bin/pai", Component::Cli),
            ("usr/bin/pai-config", Component::Config),
        ] {
            assert_eq!(of(rel), want, "{rel}");
        }
    }

    /// The bff is the framework's, versioned like the GUI and the command
    /// line (the framework version alone), not a runtime, and one of the
    /// packages every install phase considers.
    #[test]
    fn the_bff_package_is_named_and_versioned_like_the_configurator() {
        assert_eq!(Component::Bff.package(), "private-ai-framework-bff");
        assert!(!Component::Bff.is_runtime());
        assert!(Component::ALL.contains(&Component::Bff));
        assert_eq!(
            file_stem(Component::Bff, "2.99.1", "v1.0.0", "2.99.1"),
            "private-ai-framework-bff-v2.99.1"
        );
    }

    /// `xtask install` picks the newest file of each component: the bff's
    /// is one of them, told apart from the others by its name.
    #[test]
    fn the_newest_bff_package_is_picked_beside_the_others() {
        let dir = std::env::temp_dir().join(format!("xtask-newest-bff-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        for file in [
            "private-ai-framework-cli-v2.99.1-x86_64.rpm",
            "private-ai-framework-bff-v2.99.1-x86_64.rpm",
            "llama.cpp-v1.0.0+pai1-x86_64.rpm",
        ] {
            std::fs::write(dir.join(file), "").unwrap();
        }
        let found: Vec<(Component, String)> = newest_in(&dir, "-x86_64.rpm")
            .iter()
            .map(|(c, p)| (*c, p.file_name().unwrap().to_string_lossy().into_owned()))
            .collect();
        std::fs::remove_dir_all(&dir).unwrap();
        assert!(
            found.contains(&(
                Component::Bff,
                "private-ai-framework-bff-v2.99.1-x86_64.rpm".to_string()
            )),
            "{found:?}"
        );
        assert_eq!(found.len(), 3, "{found:?}");
    }

    /// `cli.toml` holds every text the programs print: pai, but also
    /// pai-manager and pai-bff, which do not require the command line's
    /// package. It goes to the core package, which every other framework
    /// package requires; the GUI's language file stays with the GUI, the
    /// benchmark method with the command line.
    #[test]
    fn cli_toml_goes_to_core_which_every_framework_package_requires() {
        for (rel, want) in [
            ("usr/share/private-ai/strings/cli.toml", Component::Core),
            ("usr/share/private-ai/strings/en_US.toml", Component::Config),
            ("opt/private-ai/share/strings/en_US.toml", Component::Config),
            (
                "usr/share/private-ai/benchmarks/specs/standard-1.1.ini",
                Component::Cli,
            ),
            (
                "opt/private-ai/share/benchmarks/specs/standard-1.1.ini",
                Component::Cli,
            ),
        ] {
            assert_eq!(of(rel), want, "{rel}");
        }
    }

    /// macOS is left out, like the manager: no service there. The macOS
    /// package phase stages the build's bin/, where the build copies only
    /// `NEXT_TO_SERVER` (pai-config, pai), and names neither pai-bff nor its
    /// component; its install phase neither. Already true, kept so.
    #[test]
    fn no_macos_package_carries_pai_bff() {
        use crate::common::configurator::NEXT_TO_SERVER;
        assert!(
            !NEXT_TO_SERVER.contains(&"pai-bff"),
            "pai-bff copied next to llama-server: {NEXT_TO_SERVER:?}"
        );
        for (name, text) in [
            ("macos/package.rs", include_str!("../macos/package.rs")),
            ("macos/install.rs", include_str!("../macos/install.rs")),
        ] {
            assert!(!text.contains("pai-bff"), "{name} names pai-bff");
            assert!(
                !text.contains("Component::Bff"),
                "{name} names Component::Bff"
            );
        }
    }

    /// The packages of the 2.4 releases and earlier, spelled split so the
    /// names lint stays clean.
    const OLD_BASE: &str = concat!("llama-cpp", "-framework");

    /// Old packages left in dist/ (the monolith of 2.2 and earlier, the split
    /// packages of 2.3 and 2.4, and the never-published 2.99.x runtime
    /// `private-ai-framework-rocm`) are never picked, even when written last.
    #[test]
    fn the_newest_file_of_each_component_wins_and_the_old_packages_never_match() {
        let dir = std::env::temp_dir().join(format!("xtask-newest-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let start =
            std::time::SystemTime::UNIX_EPOCH + std::time::Duration::from_secs(1_700_000_000);
        let files = [
            "llama.cpp-rocm-v10.0.0-amd64.deb".to_string(),
            "llama.cpp-rocm-v10.1.0-x86_64.rpm".to_string(),
            format!("{OLD_BASE}-v2.2.0-v0.8.0-amd64.deb"),
            format!("{OLD_BASE}-rocm-v10.2.0-amd64.deb"),
            format!("{OLD_BASE}-config-v2.4.0-v0.9.0-amd64.deb"),
            "private-ai-framework-rocm-v10.2.0-amd64.deb".to_string(),
            "private-ai-framework-llama-v2.99.0-v1.0.0-amd64.deb".to_string(),
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
                "llama.cpp-rocm-v10.0.0-amd64.deb".to_string()
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
