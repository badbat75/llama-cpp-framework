//! What the macOS `.pkg` files are (Apple Silicon), as pure items so Linux
//! tests them: `macos/` compiles on a Mac only. The engine and the framework
//! are packaged apart, as on Linux: llama.cpp in its own tree with its Vulkan
//! runtime, the framework's packages in theirs.

use crate::common::components::{self, Component, ENGINE_NAME};
use crate::common::configurator::NEXT_TO_SERVER;
use crate::Product;
use std::path::PathBuf;

/// The engine's prefix on the target machine.
pub const ENGINE_DIR: &str = "/opt/llama.cpp";

/// The framework's prefix on the target machine.
pub const FRAMEWORK_DIR: &str = "/opt/private-ai";

/// Where the Vulkan loader looks for driver manifests (it does not look
/// next to itself).
const ICD_DIR: &str = "/usr/local/share/vulkan/icd.d";

/// The components macOS packages: no manager, no web service, and of the
/// runtimes only Vulkan (Metal is part of the system).
const COMPONENTS: [Component; 5] = [
    Component::Config,
    Component::Cli,
    Component::Core,
    Component::Llama,
    Component::Vulkan,
];

/// The components the macOS packages take, of `product` or of both, in
/// `Component::ALL`'s order; the minimal build (Metal only) has no Vulkan.
pub fn of_product(product: Option<Product>, minimal: bool) -> Vec<Component> {
    Component::ALL
        .into_iter()
        .filter(|c| COMPONENTS.contains(c) && product.is_none_or(|p| c.product() == p))
        .filter(|c| !(minimal && *c == Component::Vulkan))
        .collect()
}

/// Where a file of the build's `bin/` is installed (absolute path):
/// pai-config and pai in the framework's `bin/`, everything else
/// (llama.cpp's executables, libraries and backends) in the engine's.
pub fn staged_path(file: &str) -> String {
    let dir = if NEXT_TO_SERVER.contains(&file) {
        FRAMEWORK_DIR
    } else {
        ENGINE_DIR
    };
    format!("{dir}/bin/{file}")
}

/// The pkg identifier of a component's `.pkg`.
pub fn identifier(c: Component) -> String {
    match c {
        Component::Llama => format!("org.privateai.{ENGINE_NAME}"),
        c if c.is_runtime() => format!("org.privateai.{ENGINE_NAME}.{}", c.suffix()),
        c => format!("org.privateai.framework.{}", c.suffix()),
    }
}

/// The version a component's `.pkg` carries (`pkgbuild --version`): the
/// engine its release, a runtime its SDK, the framework its own.
pub fn version(c: Component, fw_version: &str, llama_build: &str, sdk_version: &str) -> String {
    match c {
        Component::Llama => llama_build.trim_start_matches('v').to_string(),
        c if c.is_runtime() => sdk_version.to_string(),
        _ => fw_version.to_string(),
    }
}

/// The file name of a component's `.pkg`; the minimal build's engine
/// carries `-minimal` (never a release).
pub fn file_name(
    c: Component,
    fw_version: &str,
    llama_build: &str,
    sdk_version: &str,
    minimal: bool,
) -> String {
    let v = version(c, fw_version, llama_build, sdk_version);
    let marker = if minimal && c == Component::Llama {
        "-minimal"
    } else {
        ""
    };
    format!(
        "{}{marker}-arm64.pkg",
        components::file_stem(c, fw_version, llama_build, &v)
    )
}

/// The installed MoltenVK library, which the driver manifest names.
pub fn moltenvk_path() -> String {
    format!("{ENGINE_DIR}/lib/libMoltenVK.dylib")
}

/// The installed MoltenVK driver manifest.
pub fn icd_manifest() -> String {
    format!("{ICD_DIR}/{ENGINE_NAME}-MoltenVK_icd.json")
}

/// The order `xtask install` lays the packages down in: the engine's
/// first, then the framework's, then any other file, each in the order given.
pub fn install_order(mut pkgs: Vec<PathBuf>) -> Vec<PathBuf> {
    pkgs.sort_by_key(|p| {
        let name = p.file_name().unwrap_or_default().to_string_lossy();
        match COMPONENTS
            .iter()
            .find(|c| components::is_file_of(**c, &name))
        {
            Some(c) if c.product() == Product::Llama => 0,
            Some(_) => 1,
            None => 2,
        }
    });
    pkgs
}

/// The `pgrep -f` pattern of a program running from an installed tree
/// (an extended regular expression, as BSD pgrep reads it).
pub fn running_pattern() -> String {
    let esc = |d: &str| d.replace('.', "\\.");
    format!("^({}|{})/bin/", esc(ENGINE_DIR), esc(FRAMEWORK_DIR))
}

#[cfg(test)]
mod tests;
