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

use crate::common::macos_pkg;
use crate::common::versions_share::runtime_package;
use anyhow::{Context, Result};
use std::path::{Path, PathBuf};

/// The engine's package name, and the dist version a runtime's package is
/// versioned by: the Windows package phase needs them too and does not build
/// this module, so they live in `versions_share`.
pub use crate::common::versions_share::{dist_version, ENGINE_NAME};

pub const BASE_NAME: &str = "private-ai-framework";
/// The base name of the packages up to 2.4 (the single package of 2.2 and
/// earlier, then `<base>-<component>`), which the upgrade relations name.
pub const OLD_BASE_NAME: &str = "llama-cpp-framework";
/// The folder of the installed versions, under the Linux staging root, with
/// its trailing slash: one file per engine and runtime package, named after
/// it, each written by that package and read by the web service
/// (`pai_core::bff_cfg::versions_dir`).
pub const ENGINE_VERSIONS_DIR: &str = "usr/share/private-ai/llama.cpp/versions/";

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
            c if c.is_runtime() => runtime_package(c.suffix()),
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
    // macOS: the engine's tree (its runtime in lib/), apart from the
    // framework's under /opt/private-ai.
    if let Some(inner) = path
        .strip_prefix(macos_pkg::ENGINE_DIR.trim_start_matches('/'))
        .and_then(|rest| rest.strip_prefix('/'))
    {
        return private(inner, name);
    }
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
    // The version file of an engine or runtime package, in the folder the web
    // service reads: each belongs to the package its own name names, ahead of
    // the rule that gives everything of the engine's share folder to the
    // engine. A name that is no package stays the engine's, as the rest of
    // that folder is.
    if let Some(file) = path.strip_prefix(ENGINE_VERSIONS_DIR) {
        if let Some(c) = Component::ALL.iter().find(|c| c.package() == file) {
            return *c;
        }
    }
    // The engine's instance unit (the engine owns its unit), its target (the
    // one name every instance is ordered before) with the link that pulls it
    // into the boot transaction, its shipped defaults, copied into
    // /etc/llama.cpp by its maintainer scripts, and its tmpfiles.d entry (the
    // instances' shared models folder).
    if path == "usr/lib/systemd/system/llama.cpp@.service"
        || path == "usr/lib/systemd/system/llama.cpp.target"
        || path == "usr/lib/systemd/system/multi-user.target.wants/llama.cpp.target"
        || path.starts_with("usr/share/private-ai/llama.cpp/")
        || path == "usr/lib/tmpfiles.d/llama.cpp.conf"
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

/// The owner of a file under a private tree (`/opt/llama.cpp` and
/// `/opt/private-ai` on macOS, `/usr/lib/private-ai` on Linux), by its path `inner` in that tree: the
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

/// The two products (declared beside the command line, which names them on
/// every OS).
pub use crate::Product;

impl Component {
    /// The product whose package this is.
    pub fn product(self) -> Product {
        if self == Component::Llama || self.is_runtime() {
            Product::Llama
        } else {
            Product::Framework
        }
    }
}

/// The components a package run stages and packages: those of `product`, or
/// every one when None, in `Component::ALL`'s order.
#[cfg_attr(not(target_os = "linux"), allow(dead_code))]
pub fn of_product(product: Option<Product>) -> Vec<Component> {
    Component::ALL
        .into_iter()
        .filter(|c| product.is_none_or(|p| c.product() == p))
        .collect()
}

/// The build revision of the llama.cpp package (`+pai<N>` in its version):
/// raised when anything but llama.cpp's code changes on the same release
/// (the patches, the build options, the package's scripts or files), back
/// to 1 at every new llama.cpp release.
pub const LLAMA_CPP_REVISION: u32 = 2;

/// The version of the llama.cpp package built from release `llama_build`
/// (`v1.0.0`): `1:1.0.0+pai<LLAMA_CPP_REVISION>`. The package we ship is at
/// our own build revision, so its version is the minimum we accept.
pub fn llama_cpp_version(llama_build: &str) -> String {
    llama_cpp_min(llama_build)
}

/// The lowest llama.cpp package version a framework package built against
/// release `llama_build` (`v1.0.0`) accepts: `1:1.0.0+pai<LLAMA_CPP_REVISION>`,
/// the release with the epoch (Ubuntu 26.04 ships its own `llama.cpp` at
/// `8681+dfsg-1`, which ranks above `1.0.0` without an epoch, Debian Policy
/// 5.6.12) and our build revision (an engine of an earlier revision of the
/// same release lacks what this framework was built against).
pub fn llama_cpp_min(llama_build: &str) -> String {
    format!(
        "1:{}+pai{LLAMA_CPP_REVISION}",
        llama_build.trim_start_matches('v')
    )
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

#[cfg(test)]
#[path = "components_tests.rs"]
mod tests;
