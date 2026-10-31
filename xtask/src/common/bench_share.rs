//! The benchmark method the packages install read-only beside the binaries,
//! `<root>/share/benchmarks/`: every published `standard-<X.Y>.ini` with
//! `standard.versions`, and every prompt in `benchmarks/prompts/`. The
//! configurator finds it from its own path (`<root>/bin/llama-cpp-config`,
//! `paths::bench_share_dir`) and checks each file against the digests built
//! into it, so every installed version stays runnable.

use super::{ui, util};
use anyhow::{Context, Result};
use std::path::Path;

/// Copy the method into `<root>/share/benchmarks`.
pub fn stage(root: &Path) -> Result<()> {
    let src = util::repo_root().join("benchmarks");
    let dest = root.join("share").join("benchmarks");
    let mut n = 0;
    for (dir, keep) in [
        ("specs", (|name: &str| name == "standard.versions" || (name.starts_with("standard-") && name.ends_with(".ini"))) as fn(&str) -> bool),
        ("prompts", |name: &str| name.ends_with(".txt") || name.ends_with(".json") || name == "README.md"),
    ] {
        let to = dest.join(dir);
        std::fs::create_dir_all(&to)?;
        let from = src.join(dir);
        for e in std::fs::read_dir(&from).with_context(|| format!("reading {}", from.display()))? {
            let e = e?;
            let name = e.file_name().to_string_lossy().into_owned();
            if e.file_type()?.is_file() && keep(&name) {
                std::fs::copy(e.path(), to.join(&name))
                    .with_context(|| format!("copying {} into {}", name, to.display()))?;
                n += 1;
            }
        }
    }
    ui::dim(format!("Staged the benchmark method: {n} files in {}", dest.display()));
    Ok(())
}
