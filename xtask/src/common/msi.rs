//! What the Windows package holds beyond the harvested tree: the
//! executables of the cargo workspace that `xtask package` stages into
//! `bin\`, and (in the tests) the manager's and the bff's services and the machine-wide data
//! tree under `%ProgramData%\private-ai` that `installer\private-ai-framework.wxs`
//! registers and creates. The tests read the `.wxs` as XML, so they run on
//! every OS; nothing here builds the MSI.

/// The cargo workspace's executables that `xtask package` copies from
/// cargo's release dir into `staging\bin\` on Windows.
#[cfg_attr(not(windows), allow(dead_code))]
pub const CARGO_EXES: &[&str] = &[
    "pai-config.exe",
    "pai.exe",
    "pai-manager.exe",
    "pai-bff.exe",
];

use crate::common::util;
use crate::Product;
use anyhow::bail;
use std::path::{Path, PathBuf};

/// The products `xtask package` builds an MSI of on Windows: `product`, or
/// both when None.
#[cfg_attr(not(windows), allow(dead_code))]
pub fn products(product: Option<Product>) -> Vec<Product> {
    match product {
        Some(p) => vec![p],
        None => vec![Product::Llama, Product::Framework],
    }
}

/// The MSI's file name: `llama.cpp-v<release>-<arch>.msi` for the engine
/// (`llama.cpp-v<release>-minimal-<arch>.msi` for the minimal build, never a
/// release), `private-ai-framework-v<version>-<arch>.msi` for the framework.
#[cfg_attr(not(windows), allow(dead_code))]
pub fn file_name(
    product: Product,
    framework_version: &str,
    llama_build: &str,
    arch: &str,
    minimal: bool,
) -> String {
    match product {
        Product::Llama if minimal => format!("llama.cpp-{llama_build}-minimal-{arch}.msi"),
        Product::Llama => format!("llama.cpp-{llama_build}-{arch}.msi"),
        Product::Framework => format!("private-ai-framework-v{framework_version}-{arch}.msi"),
    }
}

/// The engine's MSI ProductVersion, from its release tag (`v1.0.0` ->
/// `1.0.0`); refuses what is not a release an MSI version can hold
/// (ProductVersion: major and minor at most 255, build at most 65535).
#[cfg_attr(not(windows), allow(dead_code))]
pub fn product_version(llama_build: &str) -> anyhow::Result<String> {
    let Some(version) = llama_build
        .strip_prefix('v')
        .filter(|_| util::is_release_tag(llama_build))
    else {
        bail!("llama.cpp {llama_build:?} is not a release (vX.Y.Z): an MSI needs one");
    };
    let fields: Vec<u64> = version
        .split('.')
        .map(|f| f.parse::<u64>())
        .collect::<Result<_, _>>()?;
    if fields[0] > 255 || fields[1] > 255 || fields[2] > 65535 {
        bail!("llama.cpp {llama_build} does not fit an MSI ProductVersion (255.255.65535 at most)");
    }
    Ok(version.to_string())
}

/// Where a product is staged before its MSI is built, under `build_root`.
#[cfg_attr(not(windows), allow(dead_code))]
pub fn staging_dir(build_root: &Path, product: Product) -> PathBuf {
    build_root.join("staging").join(match product {
        Product::Llama => "llama",
        Product::Framework => "framework",
    })
}

/// The `.wxs` of a product, relative to the repo root.
#[cfg_attr(not(windows), allow(dead_code))]
pub fn wxs(product: Product) -> &'static str {
    match product {
        Product::Llama => "installer/llama.cpp.wxs",
        Product::Framework => "installer/private-ai-framework.wxs",
    }
}

/// The engine's read-only defaults on Windows, staged into
/// `share\defaults\` of llama.cpp's MSI: (file name, content).
#[cfg_attr(not(windows), allow(dead_code))]
pub fn engine_defaults() -> Vec<(&'static str, String)> {
    vec![
        (
            "server@default.ini",
            include_str!("../../../crates/core/defaults/windows/server@default.ini").to_string(),
        ),
        (
            "models.ini",
            include_str!("../../../crates/core/defaults/windows/models.ini").to_string(),
        ),
        (
            "assignments.ini",
            include_str!("../../../crates/core/defaults/windows/assignments.ini").to_string(),
        ),
        (
            "instances.ini",
            include_str!("../../../crates/core/defaults/windows/instances.ini").to_string(),
        ),
    ]
}

/// The product and version of an MSI file this package phase writes
/// (`llama.cpp-v1.0.0-x64.msi` -> engine `1.0.0`,
/// `private-ai-framework-v2.99.1-x64.msi` -> framework `2.99.1`); None for
/// any other name, the earlier two-version framework name included.
#[cfg_attr(not(windows), allow(dead_code))]
pub fn msi_product(name: &str) -> Option<(Product, String)> {
    let stem = name.strip_suffix("-x64.msi")?;
    let (product, version) = if let Some(v) = stem.strip_prefix("llama.cpp-v") {
        (Product::Llama, v.strip_suffix("-minimal").unwrap_or(v))
    } else {
        (
            Product::Framework,
            stem.strip_prefix("private-ai-framework-v")?,
        )
    };
    util::is_release_tag(&format!("v{version}")).then(|| (product, version.to_string()))
}

/// What `xtask install` installs from `dir` on Windows, in order: the
/// newest (last written) MSI of llama.cpp, then the newest of the
/// framework; a product with no MSI there is left out (the framework's MSI
/// itself refuses to install without the engine).
#[cfg_attr(not(windows), allow(dead_code))]
pub fn install_plan(dir: &Path) -> Vec<PathBuf> {
    let mut found: Vec<(Product, std::time::SystemTime, PathBuf)> = std::fs::read_dir(dir)
        .map(|rd| {
            rd.flatten()
                .filter_map(|e| {
                    let (product, _) = msi_product(&e.file_name().to_string_lossy())?;
                    let written = e.metadata().ok()?.modified().ok()?;
                    Some((product, written, e.path()))
                })
                .collect()
        })
        .unwrap_or_default();
    found.sort_by_key(|(_, written, _)| std::cmp::Reverse(*written));
    [Product::Llama, Product::Framework]
        .into_iter()
        .filter_map(|p| {
            found
                .iter()
                .find(|(q, _, _)| *q == p)
                .map(|(_, _, f)| f.clone())
        })
        .collect()
}

/// The Apps entry each product's MSI writes (64-bit view), where `xtask
/// install` reads the installed version and the ProductCode of a downgrade.
#[cfg_attr(not(windows), allow(dead_code))]
pub fn uninstall_key(product: Product) -> &'static str {
    match product {
        Product::Llama => r"HKLM\SOFTWARE\Microsoft\Windows\CurrentVersion\Uninstall\llama.cpp",
        Product::Framework => {
            r"HKLM\SOFTWARE\Microsoft\Windows\CurrentVersion\Uninstall\private-ai"
        }
    }
}

/// What `xtask install <file>...` installs on Windows, in order: at most
/// one MSI of llama.cpp and one of the framework, the engine first (the
/// framework's MSI refuses to install without it), whatever order they
/// were given in. Refuses a file not named as `xtask package` names an MSI,
/// and two files of the same product.
#[cfg_attr(not(windows), allow(dead_code))]
pub fn files_plan(files: &[PathBuf]) -> anyhow::Result<Vec<PathBuf>> {
    let mut by_product: Vec<(Product, &PathBuf)> = Vec::new();
    for f in files {
        let name = f.file_name().unwrap_or_default().to_string_lossy();
        let Some((product, _)) = msi_product(&name) else {
            bail!(
                "{name} is not named as xtask package names an MSI (llama.cpp-v<release>-x64.msi, private-ai-framework-v<version>-x64.msi)"
            );
        };
        if let Some((_, other)) = by_product.iter().find(|(p, _)| *p == product) {
            bail!(
                "two files of the same product: {} and {}; give one",
                other.display(),
                f.display()
            );
        }
        by_product.push((product, f));
    }
    Ok([Product::Llama, Product::Framework]
        .into_iter()
        .filter_map(|p| {
            by_product
                .iter()
                .find(|(q, _)| *q == p)
                .map(|(_, f)| (*f).clone())
        })
        .collect())
}

/// The msiexec arguments that remove `product`'s installed version (its
/// ProductCode `product_code`) before a downgrade, basic UI. llama.cpp's
/// adds `KEEPINSTANCES=1`: the instance services stay (and its MSI stops
/// and records the running ones), so the older release installed next
/// finds them again.
#[cfg_attr(not(windows), allow(dead_code))]
pub fn removal_args(product: Product, product_code: &str) -> Vec<String> {
    let mut args = vec!["/x".to_string(), product_code.to_string(), "/qb".into()];
    if product == Product::Llama {
        args.push("KEEPINSTANCES=1".into());
    }
    args
}

/// What `wix extension list -g` answered, read from its exit status and its
/// output (stdout and stderr together).
#[cfg_attr(not(windows), allow(dead_code))]
#[derive(Debug, PartialEq)]
pub enum WixExtensionList {
    /// WIX7015: WiX v7+ builds nothing until `wix eula accept <id>`.
    EulaNeeded,
    /// The global extensions, one per line; empty when none is installed.
    Listed(String),
    /// Any other failure, with what WiX printed.
    Failed(String),
}

/// Reads `wix extension list -g`: WiX 7 exits non-zero with no output when
/// no global extension is installed (a fresh runner), which is an empty
/// list, not a failure.
#[cfg_attr(not(windows), allow(dead_code))]
pub fn wix_extension_list(success: bool, text: &str) -> WixExtensionList {
    if success || text.trim().is_empty() {
        WixExtensionList::Listed(text.to_string())
    } else if text.contains("WIX7015") {
        WixExtensionList::EulaNeeded
    } else {
        WixExtensionList::Failed(text.to_string())
    }
}

#[cfg(test)]
#[path = "msi_tests.rs"]
mod tests;
