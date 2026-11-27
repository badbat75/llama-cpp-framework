//! The one-time move of the user's tree from the names of release 2.4 and
//! earlier to the new ones: on Linux `~/.config/llama.cpp` (or
//! `$XDG_CONFIG_HOME/llama.cpp` when that is absolute) to `paths::config_dir()`
//! and `~/.local/share/llama.cpp` to `paths::data_root()`; on Windows
//! `%LOCALAPPDATA%\llama.cpp` to `paths::data_root()`. Also the old name of the
//! login entry, which `pai-config`'s startup module moves.
//!
//! The only module allowed to name the old directories and the old login entry
//! (the names lints exempt this file, and only for those names).
//!
//! A tree moves only when the new place does not exist and the old one holds
//! something this program writes; otherwise nothing is touched. The move is a
//! rename, else a copy into a temporary sibling of the new place renamed into
//! it; the old tree is removed only once the new one is complete, and on any
//! failure the old tree is left as it was and no partial new tree remains.

use crate::paths;
use std::io;
use std::path::{Path, PathBuf};

/// The login entry's old name: the Windows HKCU Run value, and the stem of the
/// Linux XDG autostart file (`<stem>.desktop`).
pub const OLD_LOGIN_ENTRY: &str = "llama-cpp-config";

/// llama.cpp's own user config file, which llama.cpp reads from the old config
/// directory (`common/arg.cpp`): the config tree's move puts it back there,
/// never into the new tree. The data tree takes it along like any file.
pub const LLAMA_CPP_CONFIG_FILE: &str = "config.ini";

/// What a migration of one tree did.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Outcome {
    /// No old tree: nothing to do.
    NoOldTree,
    /// An old directory holding nothing this program writes: left alone.
    NotOurs,
    /// The new tree already exists: nothing moved, the old one left alone.
    NewExists,
    /// Moved with one rename.
    Renamed,
    /// Copied (the rename failed), then the old tree removed.
    Copied,
}

/// One tree's old and new place and what happened to it.
#[derive(Debug)]
pub struct TreeMove {
    pub old: PathBuf,
    pub new: PathBuf,
    pub result: io::Result<Outcome>,
}

/// The `(old, new)` places of every tree this OS had, computed from the
/// environment (the test redirect included); no IO.
pub fn old_trees() -> Vec<(PathBuf, PathBuf)> {
    #[cfg(windows)]
    {
        let new = paths::data_root();
        let old = new.parent().unwrap_or(Path::new("")).join("llama.cpp");
        vec![(old, new)]
    }
    #[cfg(not(windows))]
    {
        let home = paths::home_dir();
        let redirected = std::env::var_os("PRIVATE_AI_DATA_ROOT").is_some();
        let config_base = std::env::var_os("XDG_CONFIG_HOME")
            .filter(|_| !redirected)
            .map(PathBuf::from)
            .filter(|p| p.is_absolute())
            .unwrap_or_else(|| home.join(".config"));
        let old_data = home.join(".local").join("share").join("llama.cpp");
        let old_config = config_base.join("llama.cpp");
        // The data tree first: the redirected config dir sits inside it.
        vec![
            (old_data, paths::data_root()),
            (old_config, paths::config_dir()),
        ]
    }
}

/// What a tree holds when this program wrote it: any one is enough.
const OURS: [&str; 7] = [
    "server.ini",
    "presets.ini",
    "presets.ini.disabled",
    "settings.ini",
    "logs",
    "bench",
    "state",
];

/// The config files, which the Windows layout keeps under `config\`.
const OUR_CONFIG_FILES: [&str; 4] = [
    "server.ini",
    "presets.ini",
    "presets.ini.disabled",
    "settings.ini",
];

fn is_ours(dir: &Path) -> bool {
    OURS.iter().any(|n| dir.join(n).exists())
        || OUR_CONFIG_FILES
            .iter()
            .any(|n| dir.join("config").join(n).exists())
}

/// Copy the tree at `from` to `to`, which must not exist.
fn copy_tree(from: &Path, to: &Path) -> io::Result<()> {
    std::fs::create_dir(to)?;
    for entry in std::fs::read_dir(from)? {
        let entry = entry?;
        let target = to.join(entry.file_name());
        if entry.file_type()?.is_dir() {
            copy_tree(&entry.path(), &target)?;
        } else {
            std::fs::copy(entry.path(), &target)?;
        }
    }
    Ok(())
}

/// Migrate every tree of `old_trees()`, in an order that lets a new tree sit
/// inside another (the redirected config dir is inside the data root).
pub fn migrate_old_trees() -> Vec<TreeMove> {
    let config = paths::config_dir();
    old_trees()
        .into_iter()
        .map(|(old, new)| {
            let result = if cfg!(not(windows)) && new == config {
                migrate_config_tree(&old, &new)
            } else {
                migrate_tree(&old, &new)
            };
            TreeMove { old, new, result }
        })
        .collect()
}

/// Migrate one tree from `old` to `new` (see the module header).
pub fn migrate_tree(old: &Path, new: &Path) -> io::Result<Outcome> {
    migrate_tree_with(old, new, &|from, to| std::fs::rename(from, to))
}

/// `migrate_tree` with the rename injected, so a test can force the copy
/// fallback and its failures.
pub(crate) fn migrate_tree_with(
    old: &Path,
    new: &Path,
    rename: &dyn Fn(&Path, &Path) -> io::Result<()>,
) -> io::Result<Outcome> {
    if !old.exists() {
        return Ok(Outcome::NoOldTree);
    }
    if !old.is_dir() || !is_ours(old) {
        return Ok(Outcome::NotOurs);
    }
    if new.exists() {
        return Ok(Outcome::NewExists);
    }
    if let Some(parent) = new.parent() {
        std::fs::create_dir_all(parent)?;
    }
    if rename(old, new).is_ok() {
        return Ok(Outcome::Renamed);
    }
    let mut temp_name = std::ffi::OsString::from(".");
    temp_name.push(new.file_name().unwrap_or_default());
    temp_name.push(".migrating");
    let temp = new.with_file_name(temp_name);
    if temp.exists() {
        std::fs::remove_dir_all(&temp)?;
    }
    if let Err(e) = copy_tree(old, &temp).and_then(|()| rename(&temp, new)) {
        let _ = std::fs::remove_dir_all(&temp);
        return Err(e);
    }
    std::fs::remove_dir_all(old)?;
    Ok(Outcome::Copied)
}

/// `migrate_tree` for the config tree: after the move, a
/// `LLAMA_CPP_CONFIG_FILE` the old tree held is put back at the old path (the
/// old directory re-created holding only it), so llama.cpp still finds it. If
/// putting it back fails, the error is returned, naming where the file now is,
/// and the file stays in the new tree: never lost.
pub fn migrate_config_tree(old: &Path, new: &Path) -> io::Result<Outcome> {
    migrate_config_tree_with(old, new, &|from, to| std::fs::rename(from, to))
}

/// `migrate_config_tree` with the rename injected, as `migrate_tree_with`.
pub(crate) fn migrate_config_tree_with(
    old: &Path,
    new: &Path,
    rename: &dyn Fn(&Path, &Path) -> io::Result<()>,
) -> io::Result<Outcome> {
    let had_config_ini = old.join(LLAMA_CPP_CONFIG_FILE).is_file();
    let outcome = migrate_tree_with(old, new, rename)?;
    if had_config_ini && matches!(outcome, Outcome::Renamed | Outcome::Copied) {
        let kept = new.join(LLAMA_CPP_CONFIG_FILE);
        let back = old.join(LLAMA_CPP_CONFIG_FILE);
        let put_back = std::fs::create_dir_all(old).and_then(|()| {
            if rename(&kept, &back).is_ok() {
                return Ok(());
            }
            std::fs::copy(&kept, &back)
                .inspect_err(|_| {
                    let _ = std::fs::remove_file(&back);
                })
                .and_then(|_| std::fs::remove_file(&kept))
        });
        put_back.map_err(|e| io::Error::new(e.kind(), format!("{}: {e}", kept.display())))?;
    }
    Ok(outcome)
}

/// E2E phase for the move of the old trees under the `PRIVATE_AI_DATA_ROOT`
/// redirect, for the e2e test of this crate to call (`src/tests/mod.rs`): the
/// redirect is the whole home, so the old trees are built inside it.
#[cfg(all(test, not(windows)))]
pub(crate) fn migrate_e2e() {
    use crate::paths;
    let dir = tempfile::tempdir().expect("tempdir");
    paths::redirect_for_tests(dir.path());
    let home = dir.path();
    let old_config = home.join(".config").join("llama.cpp");
    let old_data = home.join(".local").join("share").join("llama.cpp");

    // The places, under the redirect: the old ones where 2.4 kept them in
    // this home, the new ones where `paths` puts them now.
    let trees = old_trees();
    assert_eq!(trees.len(), 2, "{trees:?}");
    assert!(
        trees.contains(&(old_config.clone(), paths::config_dir())),
        "{trees:?}"
    );
    assert!(
        trees.contains(&(old_data.clone(), paths::data_root())),
        "{trees:?}"
    );

    // No old tree: nothing happens and nothing is created.
    for m in migrate_old_trees() {
        assert_eq!(
            m.result.expect("no error"),
            Outcome::NoOldTree,
            "{:?}",
            m.old
        );
    }
    assert!(!paths::data_root().exists() && !paths::config_dir().exists());

    // A 2.4 tree: the config files and the data (logs, bench, state).
    std::fs::create_dir_all(&old_config).expect("old config");
    std::fs::write(old_config.join("server.ini"), "[Server]\nPort = 8080\n").expect("w");
    std::fs::write(old_config.join("presets.ini"), "[m]\nctx-size = 8192\n").expect("w");
    std::fs::write(
        old_config.join("settings.ini"),
        "[Settings]\nLogRotate = false\n",
    )
    .expect("w");
    std::fs::create_dir_all(old_data.join("logs")).expect("logs");
    std::fs::create_dir_all(old_data.join("bench")).expect("bench");
    std::fs::write(old_data.join("logs").join("llama-server.log"), "line\n").expect("w");
    std::fs::write(old_data.join("bench").join("bench.db"), [0u8, 1, 2, 255]).expect("w");
    // llama.cpp's own config.ini beside ours: it stays where llama.cpp reads
    // it. One in the data tree is just a file and moves along.
    std::fs::write(old_config.join(LLAMA_CPP_CONFIG_FILE), "ctx-size = 4096\n").expect("w");
    std::fs::write(old_data.join(LLAMA_CPP_CONFIG_FILE), "data\n").expect("w");

    let moves = migrate_old_trees();
    assert_eq!(moves.len(), 2);
    for m in &moves {
        assert!(
            matches!(m.result, Ok(Outcome::Renamed)),
            "{:?} -> {:?}: {:?}",
            m.old,
            m.new,
            m.result
        );
    }
    let read = |p: PathBuf| std::fs::read(&p).unwrap_or_else(|e| panic!("{p:?}: {e}"));
    assert_eq!(read(paths::server_ini()), b"[Server]\nPort = 8080\n");
    // The tree move carries every file under the name the old program wrote:
    // the model profile file of a 2.4 tree stays `presets.ini`, the name 2.4 used. It
    // is pinned by its literal path on purpose, `paths::models_ini` is the
    // current name and says nothing about a 2.4 tree.
    assert_eq!(
        read(paths::config_dir().join("presets.ini")),
        b"[m]\nctx-size = 8192\n",
        "the 2.4 preset file moves under its 2.4 name"
    );
    assert!(
        !paths::config_dir().join("models.ini").exists(),
        "the move does not rename the 2.4 file to the current name"
    );
    assert_eq!(
        read(paths::settings_ini()),
        b"[Settings]\nLogRotate = false\n"
    );
    assert_eq!(read(paths::server_log()), b"line\n");
    assert_eq!(
        read(paths::data_root().join("bench").join("bench.db")),
        [0u8, 1, 2, 255]
    );
    assert!(!old_data.exists(), "the old data tree is gone");
    assert_eq!(
        read(paths::data_root().join(LLAMA_CPP_CONFIG_FILE)),
        b"data\n",
        "the data tree takes config.ini along"
    );
    let names = |d: &Path| {
        let mut v: Vec<String> = std::fs::read_dir(d)
            .expect("read dir")
            .map(|e| e.expect("entry").file_name().to_string_lossy().into_owned())
            .collect();
        v.sort();
        v
    };
    assert_eq!(
        names(&old_config),
        [LLAMA_CPP_CONFIG_FILE],
        "the old config directory holds only llama.cpp's config.ini"
    );
    assert_eq!(
        read(old_config.join(LLAMA_CPP_CONFIG_FILE)),
        b"ctx-size = 4096\n"
    );
    assert!(!paths::config_dir().join(LLAMA_CPP_CONFIG_FILE).exists());

    // Idempotent: a second run changes nothing. The old config directory,
    // holding only llama.cpp's file, is no longer ours.
    for m in migrate_old_trees() {
        let want = if m.old == old_config {
            Outcome::NotOurs
        } else {
            Outcome::NoOldTree
        };
        assert_eq!(m.result.expect("no error"), want, "{:?}", m.old);
    }
    assert_eq!(read(paths::server_ini()), b"[Server]\nPort = 8080\n");
    assert_eq!(
        read(old_config.join(LLAMA_CPP_CONFIG_FILE)),
        b"ctx-size = 4096\n"
    );
}

#[cfg(test)]
#[path = "migrate_tests.rs"]
mod tests;
