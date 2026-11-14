//! The machine-independent llama.cpp CMake options, kept out of the code in
//! xtask/cmake/: common.toml (every OS: the generator and the defines) and
//! <os>.toml (linux, windows, macos; optional, only the defines), whose
//! entries follow the common ones and REPLACE the entry of the same NAME
//! rather than adding a second -D for it. common.toml's header says what
//! belongs there and what stays in each OS's build_llama.rs.

use crate::common::util;
use anyhow::{bail, Context, Result};
use serde::Deserialize;
use std::path::{Path, PathBuf};

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Common {
    generator: String,
    defines: Vec<String>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct OsOptions {
    defines: Vec<String>,
}

fn dir() -> PathBuf {
    util::repo_root().join("xtask").join("cmake")
}

fn read<T: for<'de> Deserialize<'de>>(path: &Path) -> Result<T> {
    let text =
        std::fs::read_to_string(path).with_context(|| format!("reading {}", path.display()))?;
    let value: T = toml::from_str(&text).with_context(|| format!("parsing {}", path.display()))?;
    Ok(value)
}

/// The generator and the `-D` arguments: common.toml's defines, then this
/// OS's file (when there is one) replacing by NAME, then minimal.toml for the
/// minimal build (`xtask configure --minimal`). An entry that is not
/// NAME[:TYPE]=VALUE fails here, before cmake sees it: a typo must fail as a
/// typo, not as a silently ignored entry.
pub fn cmake_options(minimal: bool) -> Result<(String, Vec<String>)> {
    load(&dir(), util::OS_DIR, minimal)
}

fn load(dir: &Path, os: &str, minimal: bool) -> Result<(String, Vec<String>)> {
    let common_path = dir.join("common.toml");
    let common: Common = read(&common_path)?;
    check(&common.defines, &common_path)?;
    let mut defines = common.defines;
    let os_path = dir.join(format!("{os}.toml"));
    let minimal_path = dir.join("minimal.toml");
    let overlays = [
        Some(os_path).filter(|p| p.is_file()),
        minimal.then_some(minimal_path),
    ];
    for path in overlays.into_iter().flatten() {
        let own: OsOptions = read(&path)?;
        check(&own.defines, &path)?;
        for d in own.defines {
            defines.retain(|c| define_name(c) != define_name(&d));
            defines.push(d);
        }
    }
    Ok((
        common.generator,
        defines.into_iter().map(|d| format!("-D{d}")).collect(),
    ))
}

fn check(defines: &[String], path: &Path) -> Result<()> {
    if defines.is_empty() && path.ends_with("common.toml") {
        bail!("{}: no defines", path.display());
    }
    for d in defines {
        if !valid_define(d) {
            bail!("{}: '{d}' is not NAME[:TYPE]=VALUE", path.display());
        }
    }
    Ok(())
}

/// NAME of a NAME[:TYPE]=VALUE entry.
fn define_name(d: &str) -> &str {
    d.split(['=', ':']).next().unwrap_or(d)
}

/// `^[A-Za-z_][A-Za-z0-9_]*(:[A-Z]+)?=.*$`
fn valid_define(d: &str) -> bool {
    let Some((key, _)) = d.split_once('=') else {
        return false;
    };
    let (name, ty) = match key.split_once(':') {
        Some((n, t)) => (n, Some(t)),
        None => (key, None),
    };
    let mut c = name.chars();
    c.next()
        .is_some_and(|c| c.is_ascii_alphabetic() || c == '_')
        && c.all(|c| c.is_ascii_alphanumeric() || c == '_')
        && ty.is_none_or(|t| !t.is_empty() && t.chars().all(|c| c.is_ascii_uppercase()))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The checked-in files load on every OS, and an OS entry replaces the
    /// common one of the same NAME.
    #[test]
    fn checked_in_files_load() {
        for os in ["linux", "windows", "macos"] {
            let (generator, defines) = load(&dir(), os, false).unwrap();
            assert_eq!(generator, "Ninja");
            assert!(defines.contains(&"-DGGML_VULKAN=ON".to_string()), "{os}");
        }
        let (_, mac) = load(&dir(), "macos", false).unwrap();
        assert!(mac.contains(&"-DGGML_CUDA=OFF".to_string()));
        assert!(!mac.contains(&"-DGGML_CUDA=ON".to_string()));
    }

    /// The Linux packages install the executables in /usr/bin and the ggml
    /// backends in /usr/lib/private-ai: ggml looks for its backends beside
    /// the executable and in the working directory, so the Linux build names
    /// that directory as its backend directory. Windows and macOS keep their
    /// backends beside the executables and set none.
    #[test]
    fn linux_loads_its_backends_from_the_private_library_dir() {
        let (_, linux) = load(&dir(), "linux", false).unwrap();
        assert!(
            linux.contains(&"-DGGML_BACKEND_DIR=/usr/lib/private-ai".to_string()),
            "{linux:?}"
        );
        for os in ["windows", "macos"] {
            let (_, defines) = load(&dir(), os, false).unwrap();
            assert!(
                !defines.iter().any(|d| d.starts_with("-DGGML_BACKEND_DIR")),
                "{os}: {defines:?}"
            );
        }
    }

    /// The GPU backends that need an SDK on the build machine.
    const SDK_BACKENDS: [&str; 3] = ["GGML_CUDA", "GGML_HIP", "GGML_VULKAN"];

    /// The minimal build (`xtask configure --minimal`, for GitHub's runners,
    /// which have none of the GPU SDKs) switches off every backend that needs
    /// one, on every OS, each named once.
    #[test]
    fn a_minimal_build_needs_no_gpu_sdk() {
        for os in ["linux", "windows", "macos"] {
            let (_, defines) = load(&dir(), os, true).unwrap();
            for name in SDK_BACKENDS {
                let off = format!("-D{name}=OFF");
                assert!(defines.contains(&off), "{os}: no {off} in {defines:?}");
                let named = defines
                    .iter()
                    .filter(|d| define_name(&d[2..]) == name)
                    .count();
                assert_eq!(named, 1, "{os}: {name} named {named} times: {defines:?}");
            }
        }
    }

    /// macOS keeps Metal (part of the system, no SDK), so its minimal build
    /// still has a GPU backend; Windows and Linux are left with the CPU.
    #[test]
    fn a_minimal_macos_build_keeps_metal() {
        let (_, mac) = load(&dir(), "macos", true).unwrap();
        assert!(mac.contains(&"-DGGML_METAL=ON".to_string()), "{mac:?}");
        for os in ["windows", "linux"] {
            let (_, defines) = load(&dir(), os, true).unwrap();
            assert!(
                !defines.iter().any(|d| d.ends_with("=ON")
                    && (d.starts_with("-DGGML_METAL")
                        || SDK_BACKENDS
                            .iter()
                            .any(|b| d.starts_with(&format!("-D{b}="))))),
                "{os}: a GPU backend is still on: {defines:?}"
            );
        }
    }

    /// Only the backends differ: every other option (the CPU variants, LTO,
    /// the release flag, the web UI, the flags) is the full build's, in the
    /// same order, with the same generator, so what CI installs is what a
    /// release builds minus the GPU backends.
    #[test]
    fn a_minimal_build_changes_only_the_backends() {
        let others = |defines: Vec<String>| -> Vec<String> {
            defines
                .into_iter()
                .filter(|d| !SDK_BACKENDS.contains(&define_name(&d[2..])))
                .collect()
        };
        for os in ["linux", "windows", "macos"] {
            let (full_gen, full) = load(&dir(), os, false).unwrap();
            let (min_gen, min) = load(&dir(), os, true).unwrap();
            assert_eq!(full_gen, min_gen, "{os}");
            assert_eq!(others(full), others(min), "{os}");
        }
    }

    /// The full build is what it was: the three SDK backends on (CUDA and HIP
    /// off on macOS, by macos.toml).
    #[test]
    fn the_full_build_keeps_its_backends() {
        for os in ["linux", "windows"] {
            let (_, defines) = load(&dir(), os, false).unwrap();
            for name in SDK_BACKENDS {
                let on = format!("-D{name}=ON");
                assert!(defines.contains(&on), "{os}: no {on}");
            }
        }
        let (_, mac) = load(&dir(), "macos", false).unwrap();
        assert!(mac.contains(&"-DGGML_VULKAN=ON".to_string()), "{mac:?}");
    }

    mod minimal_wiring;

    #[test]
    fn malformed_define_is_refused() {
        assert!(valid_define("GGML_CUDA=ON"));
        assert!(valid_define("Vulkan_LIBRARY:FILEPATH=/x"));
        assert!(!valid_define("GGML CUDA=ON"));
        assert!(!valid_define("GGML_CUDA"));
        assert!(!valid_define("X:file=1"));
    }
}
