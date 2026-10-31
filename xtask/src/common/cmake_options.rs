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
    let text = std::fs::read_to_string(path).with_context(|| format!("reading {}", path.display()))?;
    let value: T = toml::from_str(&text).with_context(|| format!("parsing {}", path.display()))?;
    Ok(value)
}

/// The generator and the `-D` arguments: common.toml's defines, then this
/// OS's file (when there is one) replacing by NAME. An entry that is not
/// NAME[:TYPE]=VALUE fails here, before cmake sees it: a typo must fail as a
/// typo, not as a silently ignored entry.
pub fn cmake_options() -> Result<(String, Vec<String>)> {
    load(&dir(), util::OS_DIR)
}

fn load(dir: &Path, os: &str) -> Result<(String, Vec<String>)> {
    let common_path = dir.join("common.toml");
    let common: Common = read(&common_path)?;
    check(&common.defines, &common_path)?;
    let mut defines = common.defines;
    let os_path = dir.join(format!("{os}.toml"));
    if os_path.is_file() {
        let own: OsOptions = read(&os_path)?;
        check(&own.defines, &os_path)?;
        for d in own.defines {
            defines.retain(|c| define_name(c) != define_name(&d));
            defines.push(d);
        }
    }
    Ok((common.generator, defines.into_iter().map(|d| format!("-D{d}")).collect()))
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
    let Some((key, _)) = d.split_once('=') else { return false };
    let (name, ty) = match key.split_once(':') {
        Some((n, t)) => (n, Some(t)),
        None => (key, None),
    };
    let mut c = name.chars();
    c.next().is_some_and(|c| c.is_ascii_alphabetic() || c == '_')
        && c.all(|c| c.is_ascii_alphanumeric() || c == '_')
        && ty.map_or(true, |t| !t.is_empty() && t.chars().all(|c| c.is_ascii_uppercase()))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The checked-in files load on every OS, and an OS entry replaces the
    /// common one of the same NAME.
    #[test]
    fn checked_in_files_load() {
        for os in ["linux", "windows", "macos"] {
            let (generator, defines) = load(&dir(), os).unwrap();
            assert_eq!(generator, "Ninja");
            assert!(defines.contains(&"-DGGML_VULKAN=ON".to_string()), "{os}");
        }
        let (_, mac) = load(&dir(), "macos").unwrap();
        assert!(mac.contains(&"-DGGML_CUDA=OFF".to_string()));
        assert!(!mac.contains(&"-DGGML_CUDA=ON".to_string()));
    }

    #[test]
    fn malformed_define_is_refused() {
        assert!(valid_define("GGML_CUDA=ON"));
        assert!(valid_define("Vulkan_LIBRARY:FILEPATH=/x"));
        assert!(!valid_define("GGML CUDA=ON"));
        assert!(!valid_define("GGML_CUDA"));
        assert!(!valid_define("X:file=1"));
    }
}
