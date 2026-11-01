//! The configurator's text files (each language, and `cli.toml`, the messages a CLI
//! command shares with the GUI), installed read-only beside the binaries:
//! `<root>/share/strings/`, from `llama-cpp-config/strings/`. The configurator
//! finds them from its own path (`<root>/bin/llama-cpp-config`,
//! `paths::strings_dir`) and reads them at startup.

use super::{ui, util};
use anyhow::{Context, Result};
use std::path::Path;

/// Copy every `*.toml` of the language folder into `<root>/share/strings`.
pub fn stage(root: &Path) -> Result<()> {
    let from = util::repo_root().join("llama-cpp-config").join("strings");
    let to = root.join("share").join("strings");
    std::fs::create_dir_all(&to)?;
    let mut n = 0;
    for e in std::fs::read_dir(&from).with_context(|| format!("reading {}", from.display()))? {
        let e = e?;
        let name = e.file_name().to_string_lossy().into_owned();
        if e.file_type()?.is_file() && name.ends_with(".toml") {
            std::fs::copy(e.path(), to.join(&name))
                .with_context(|| format!("copying {} into {}", name, to.display()))?;
            n += 1;
        }
    }
    ui::dim(format!("Staged the language files: {n} in {}", to.display()));
    Ok(())
}
