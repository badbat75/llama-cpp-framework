//! The versions the packages installed, one file per package in the folder
//! the web service reads: `share\versions\` in llama.cpp's staging tree on
//! Windows, `/usr/share/private-ai/llama.cpp/versions/` in the Linux ones
//! (`linux::package` writes those). Each file is named after the package it
//! carries and holds that package's version on one line; an empty line is a
//! package whose version is not known. The reader and the rule are
//! `pai_core::versions`, the decisions `ARCHITECTURE.md` (slice 13).
//!
//! The names a version file carries, and the dist version read out of the
//! dist layout, live here too: every package phase writes them the same way,
//! and `components`, the Unix packages' module, does not build on Windows.

use anyhow::Result;
use std::path::Path;

/// The engine's package name, which its runtimes' names start with too.
pub const ENGINE_NAME: &str = "llama.cpp";

/// The package name of one of the engine's GPU runtimes: the engine's name
/// with the SDK's suffix (`llama.cpp-rocm`), which is the name of that
/// runtime's version file.
pub fn runtime_package(sdk: &str) -> String {
    format!("{ENGINE_NAME}-{sdk}")
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

/// The engine's version and the runtimes bundled with it, each as one file in
/// `dir` (made when missing): `dir/<engine>` with `engine_version`, and
/// `dir/<package>` with the dist version of the runtime it was bundled from,
/// `None` for a dist outside the layout. A runtime the bundling took nothing
/// from is not in `runtimes` and gets no file.
#[cfg_attr(not(windows), allow(dead_code))]
pub fn stage(
    dir: &Path,
    engine: &str,
    engine_version: &str,
    runtimes: &[(&str, Option<&str>)],
) -> Result<()> {
    std::fs::create_dir_all(dir)?;
    std::fs::write(dir.join(engine), format!("{engine_version}\n"))?;
    for (package, version) in runtimes {
        std::fs::write(
            dir.join(package),
            format!("{}\n", version.unwrap_or_default()),
        )?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    /// The signature the Windows staging calls: the folder, the engine's
    /// package name and version, the bundled runtimes with their dist
    /// versions.
    #[allow(clippy::type_complexity)]
    const _: fn(&Path, &str, &str, &[(&str, Option<&str>)]) -> Result<()> = stage;

    /// `name`'s scratch directory, removed and not made: the folder under
    /// test is the function's to create.
    fn scratch(name: &str) -> PathBuf {
        let dir =
            std::env::temp_dir().join(format!("xtask-versions-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        dir
    }

    /// Every file of `dir`, as `name=content`, sorted.
    fn files(dir: &Path) -> Vec<String> {
        let mut out: Vec<String> = std::fs::read_dir(dir)
            .expect("the versions folder")
            .flatten()
            .map(|e| {
                format!(
                    "{}={}",
                    e.file_name().to_string_lossy(),
                    std::fs::read_to_string(e.path()).expect("the version file")
                )
            })
            .collect();
        out.sort();
        out
    }

    /// The engine and every runtime the bundling took a library from get one
    /// file each, holding the version on one line; an empty line is a dist
    /// outside the layout, whose version is not known.
    #[test]
    fn one_file_per_package_with_its_version() {
        let dir = scratch("one-line").join("versions");
        stage(
            &dir,
            "llama.cpp",
            "1:1.0.0+pai2",
            &[
                ("llama.cpp-cuda", Some("13.5.0+nccl.2.32.3")),
                ("llama.cpp-rocm", None),
            ],
        )
        .expect("stage");
        assert!(dir.is_dir(), "the folder is made when missing");
        assert_eq!(
            files(&dir),
            vec![
                "llama.cpp-cuda=13.5.0+nccl.2.32.3\n",
                "llama.cpp-rocm=\n",
                "llama.cpp=1:1.0.0+pai2\n",
            ],
            "named after each package, its version alone on the line"
        );
    }

    /// A CPU-only engine: its own file and nothing else.
    #[test]
    fn an_engine_with_no_bundled_runtime_is_one_file() {
        let dir = scratch("cpu-only").join("versions");
        stage(&dir, "llama.cpp", "1:1.0.0+pai2", &[]).expect("stage");
        assert_eq!(files(&dir), vec!["llama.cpp=1:1.0.0+pai2\n"]);
    }

    /// The names the call site passes are the packages' own: the engine's
    /// file is the engine's package name and each runtime's is
    /// `llama.cpp-<sdk>`, which is what the reader names its entries after.
    /// `components` is the Unix packages' module, so this is a Unix test.
    #[cfg(unix)]
    #[test]
    fn the_files_are_named_after_the_packages() {
        use crate::common::components;
        let dir = scratch("names").join("versions");
        let names: Vec<String> = components::Component::RUNTIMES
            .iter()
            .map(|c| c.package())
            .collect();
        let runtimes: Vec<(&str, Option<&str>)> = names
            .iter()
            .map(|name| (name.as_str(), Some("1.0.0")))
            .collect();
        stage(&dir, components::ENGINE_NAME, "1:1.0.0+pai2", &runtimes).expect("stage");
        assert_eq!(
            files(&dir),
            vec![
                "llama.cpp-cuda=1.0.0\n",
                "llama.cpp-rocm=1.0.0\n",
                "llama.cpp-vulkan=1.0.0\n",
                "llama.cpp=1:1.0.0+pai2\n",
            ]
        );
    }
}
