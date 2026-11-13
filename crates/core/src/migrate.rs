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
    assert_eq!(read(paths::presets_ini()), b"[m]\nctx-size = 8192\n");
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
mod tests {
    use super::{
        migrate_config_tree, migrate_config_tree_with, migrate_tree, migrate_tree_with, Outcome,
        LLAMA_CPP_CONFIG_FILE,
    };
    use std::io;
    use std::path::{Path, PathBuf};

    /// Every path under a tree, relative, with a file's bytes.
    type Snapshot = Vec<(PathBuf, Option<Vec<u8>>)>;

    /// Every file under `dir`, relative, with its bytes, sorted; directories
    /// listed with no bytes, so an empty one counts too.
    fn snapshot(dir: &Path) -> Snapshot {
        fn walk(root: &Path, dir: &Path, out: &mut Snapshot) {
            for e in std::fs::read_dir(dir).expect("read dir") {
                let p = e.expect("entry").path();
                let rel = p.strip_prefix(root).expect("under root").to_path_buf();
                if p.is_dir() {
                    out.push((rel, None));
                    walk(root, &p, out);
                } else {
                    out.push((rel, Some(std::fs::read(&p).expect("read"))));
                }
            }
        }
        let mut out = Vec::new();
        walk(dir, dir, &mut out);
        out.sort();
        out
    }

    fn names(dir: &Path) -> Vec<String> {
        let mut v: Vec<String> = std::fs::read_dir(dir)
            .expect("read dir")
            .map(|e| e.expect("entry").file_name().to_string_lossy().into_owned())
            .collect();
        v.sort();
        v
    }

    /// An old tree as 2.4 left it: config files, a log, a binary bench.db and
    /// a nested state directory.
    fn old_tree(root: &Path) -> PathBuf {
        let old = root.join("llama.cpp");
        std::fs::create_dir_all(old.join("logs")).expect("logs");
        std::fs::create_dir_all(old.join("bench")).expect("bench");
        std::fs::create_dir_all(old.join("state").join("nested")).expect("state");
        std::fs::write(old.join("server.ini"), "[Server]\nPort = 8080\n").expect("w");
        std::fs::write(
            old.join("presets.ini"),
            "[Qwen3.8-27B]\nctx-size = 172032\n",
        )
        .expect("w");
        std::fs::write(old.join("logs").join("llama-server.log"), "boot\n").expect("w");
        std::fs::write(old.join("bench").join("bench.db"), [0u8, 159, 146, 150, 0]).expect("w");
        std::fs::write(
            old.join("state").join("nested").join("m.llamastate"),
            vec![7u8; 4096],
        )
        .expect("w");
        old
    }

    /// A rename that fails for `from == failing`, like a move across
    /// filesystems, and renames anything else for real.
    fn failing_for(failing: Vec<PathBuf>) -> impl Fn(&Path, &Path) -> io::Result<()> {
        move |from: &Path, to: &Path| {
            if failing.iter().any(|f| f == from) {
                Err(io::Error::new(
                    io::ErrorKind::CrossesDevices,
                    "test: other filesystem",
                ))
            } else {
                std::fs::rename(from, to)
            }
        }
    }

    // ── Failure paths ───────────────────────────────────────────────────

    // The new tree already exists: nothing moves, nothing merges, the old
    // tree stays as it was.
    #[test]
    fn migrate_tree_never_merges_into_an_existing_new_tree() {
        let tmp = tempfile::tempdir().expect("tempdir");
        let old = old_tree(tmp.path());
        let new = tmp.path().join("private-ai");
        std::fs::create_dir(&new).expect("new");
        std::fs::write(new.join("server.ini"), "[Server]\nPort = 9090\n").expect("w");
        let before = snapshot(&old);

        assert_eq!(
            migrate_tree(&old, &new).expect("no error"),
            Outcome::NewExists
        );
        assert_eq!(snapshot(&old), before, "the old tree is untouched");
        assert_eq!(
            names(&new),
            ["server.ini"],
            "nothing merged into the new tree"
        );
        assert_eq!(
            std::fs::read_to_string(new.join("server.ini")).expect("r"),
            "[Server]\nPort = 9090\n"
        );
    }

    // A `llama.cpp` directory of someone else's (llama.cpp's own config.ini,
    // models, a `config` folder with nothing of ours): never touched.
    #[test]
    fn migrate_tree_leaves_a_foreign_directory_alone() {
        let tmp = tempfile::tempdir().expect("tempdir");
        let old = tmp.path().join("llama.cpp");
        std::fs::create_dir_all(old.join("models")).expect("models");
        std::fs::create_dir_all(old.join("config")).expect("config");
        std::fs::write(old.join("config.ini"), "ctx-size = 4096\n").expect("w");
        std::fs::write(old.join("models").join("x.gguf"), "GGUF").expect("w");
        std::fs::write(old.join("config").join("notes.txt"), "mine").expect("w");
        let new = tmp.path().join("private-ai");
        let before = snapshot(&old);

        assert_eq!(
            migrate_tree(&old, &new).expect("no error"),
            Outcome::NotOurs
        );
        assert_eq!(snapshot(&old), before);
        assert!(!new.exists(), "no new tree for a foreign directory");
        assert_eq!(names(tmp.path()), ["llama.cpp"]);
    }

    // An empty old directory holds nothing of ours either.
    #[test]
    fn migrate_tree_leaves_an_empty_old_directory_alone() {
        let tmp = tempfile::tempdir().expect("tempdir");
        let old = tmp.path().join("llama.cpp");
        std::fs::create_dir(&old).expect("old");
        let new = tmp.path().join("private-ai");
        assert_eq!(
            migrate_tree(&old, &new).expect("no error"),
            Outcome::NotOurs
        );
        assert!(old.is_dir() && !new.exists());
    }

    // No old tree: nothing happens, nothing is created.
    #[test]
    fn migrate_tree_without_an_old_tree_does_nothing() {
        let tmp = tempfile::tempdir().expect("tempdir");
        let new = tmp.path().join("private-ai");
        assert_eq!(
            migrate_tree(&tmp.path().join("llama.cpp"), &new).expect("no error"),
            Outcome::NoOldTree
        );
        assert!(names(tmp.path()).is_empty());
    }

    // The rename fails and so does the copy, on a file it cannot read: the
    // error comes back with its kind, the old tree is exactly as it was, and
    // neither the new tree nor a temporary one is left.
    #[cfg(unix)]
    #[test]
    fn a_failed_copy_leaves_the_old_tree_and_no_new_or_temp_tree() {
        use std::os::unix::fs::PermissionsExt;
        let tmp = tempfile::tempdir().expect("tempdir");
        let old = old_tree(tmp.path());
        let new = tmp.path().join("private-ai");
        let locked = old.join("state").join("nested").join("m.llamastate");
        let before = snapshot(&old);
        std::fs::set_permissions(&locked, std::fs::Permissions::from_mode(0o000)).expect("chmod");
        if std::fs::read(&locked).is_ok() {
            // Root reads anything: the failure cannot be provoked this way.
            std::fs::set_permissions(&locked, std::fs::Permissions::from_mode(0o644))
                .expect("chmod back");
            eprintln!("skipped: running as root, a mode-000 file is still readable");
            return;
        }

        let result = migrate_tree_with(&old, &new, &failing_for(vec![old.clone()]));
        std::fs::set_permissions(&locked, std::fs::Permissions::from_mode(0o644))
            .expect("chmod back");

        let err = result.expect_err("the copy must fail");
        assert_eq!(err.kind(), io::ErrorKind::PermissionDenied, "{err}");
        assert_eq!(snapshot(&old), before, "the old tree is exactly as it was");
        assert_eq!(
            names(tmp.path()),
            ["llama.cpp"],
            "no new or temporary tree left"
        );
    }

    // The copy succeeds but the last rename, temporary tree into place, fails:
    // the temporary tree is removed and the old one kept.
    #[test]
    fn a_failed_final_rename_leaves_the_old_tree_and_no_temp_tree() {
        let tmp = tempfile::tempdir().expect("tempdir");
        let old = old_tree(tmp.path());
        let new = tmp.path().join("private-ai");
        let before = snapshot(&old);
        let always = |_: &Path, _: &Path| -> io::Result<()> {
            Err(io::Error::new(
                io::ErrorKind::CrossesDevices,
                "test: every rename fails",
            ))
        };

        let err = migrate_tree_with(&old, &new, &always).expect_err("the move must fail");
        assert_eq!(err.kind(), io::ErrorKind::CrossesDevices, "{err}");
        assert_eq!(snapshot(&old), before);
        assert_eq!(
            names(tmp.path()),
            ["llama.cpp"],
            "no new or temporary tree left"
        );
    }

    // ── The move ────────────────────────────────────────────────────────

    // One rename: every file and nested directory is in the new place, byte
    // for byte, and the old tree is gone.
    #[test]
    fn migrate_tree_moves_the_whole_tree() {
        let tmp = tempfile::tempdir().expect("tempdir");
        let old = old_tree(tmp.path());
        let new = tmp.path().join("private-ai");
        let before = snapshot(&old);

        assert_eq!(migrate_tree(&old, &new).expect("moved"), Outcome::Renamed);
        assert_eq!(snapshot(&new), before);
        assert!(!old.exists(), "the old tree is gone");
    }

    // The rename fails (another filesystem): the tree is copied, the copy
    // renamed into place, the old tree removed; no temporary tree is left.
    #[test]
    fn migrate_tree_falls_back_to_a_copy_when_the_rename_fails() {
        let tmp = tempfile::tempdir().expect("tempdir");
        let old = old_tree(tmp.path());
        let new = tmp.path().join("private-ai");
        let before = snapshot(&old);

        assert_eq!(
            migrate_tree_with(&old, &new, &failing_for(vec![old.clone()])).expect("copied"),
            Outcome::Copied
        );
        assert_eq!(snapshot(&new), before, "the same contents");
        assert!(
            !old.exists(),
            "the old tree is removed once the new one is complete"
        );
        assert_eq!(names(tmp.path()), ["private-ai"], "no temporary tree left");
    }

    // A new place whose parent does not exist yet (the redirected config dir
    // sits inside a data root that may not exist): the parent is created.
    #[test]
    fn migrate_tree_creates_the_parent_of_the_new_place() {
        let tmp = tempfile::tempdir().expect("tempdir");
        let old = old_tree(tmp.path());
        let new = tmp.path().join("private-ai").join("config");
        assert_eq!(migrate_tree(&old, &new).expect("moved"), Outcome::Renamed);
        assert!(new.join("server.ini").is_file());
    }

    // Each thing this program writes is enough to call the tree ours,
    // including the Windows layout's `config\` folder.
    #[test]
    fn any_file_or_folder_of_ours_makes_the_tree_ours() {
        for entry in [
            "server.ini",
            "presets.ini",
            "presets.ini.disabled",
            "settings.ini",
            "logs/",
            "bench/",
            "state/",
            "config/server.ini",
            "config/settings.ini",
        ] {
            let tmp = tempfile::tempdir().expect("tempdir");
            let old = tmp.path().join("llama.cpp");
            let path = old.join(entry.trim_end_matches('/'));
            if entry.ends_with('/') {
                std::fs::create_dir_all(&path).expect("dir");
            } else {
                std::fs::create_dir_all(path.parent().expect("parent")).expect("dir");
                std::fs::write(&path, "x").expect("w");
            }
            std::fs::write(old.join("config.ini"), "foreign").expect("w");
            let new = tmp.path().join("private-ai");
            assert_eq!(
                migrate_tree(&old, &new).expect("moved"),
                Outcome::Renamed,
                "{entry}"
            );
            assert!(new.join(entry.trim_end_matches('/')).exists(), "{entry}");
        }
    }

    // Idempotent: the second run finds no old tree and leaves the new one.
    #[test]
    fn a_second_migration_is_a_no_op() {
        let tmp = tempfile::tempdir().expect("tempdir");
        let old = old_tree(tmp.path());
        let new = tmp.path().join("private-ai");
        migrate_tree(&old, &new).expect("first run");
        let after_first = snapshot(&new);
        assert_eq!(
            migrate_tree(&old, &new).expect("second run"),
            Outcome::NoOldTree
        );
        assert_eq!(snapshot(&new), after_first);
    }

    // ── llama.cpp's own config.ini in the config tree ───────────────────

    /// The old config tree with llama.cpp's own `config.ini` beside ours.
    fn old_config_tree(root: &Path) -> (PathBuf, Snapshot) {
        let old = old_tree(root);
        let ours = snapshot(&old);
        std::fs::write(old.join(LLAMA_CPP_CONFIG_FILE), "ctx-size = 4096\n").expect("w");
        (old, ours)
    }

    fn assert_config_ini_put_back(old: &Path, new: &Path, ours: &Snapshot) {
        assert_eq!(
            &snapshot(new),
            ours,
            "the new tree is ours, without config.ini"
        );
        assert_eq!(
            names(old),
            [LLAMA_CPP_CONFIG_FILE],
            "the old dir holds only it"
        );
        assert_eq!(
            std::fs::read_to_string(old.join(LLAMA_CPP_CONFIG_FILE)).expect("put back"),
            "ctx-size = 4096\n"
        );
    }

    // The rename path: everything moves, then config.ini goes back to the
    // old directory, re-created for it.
    #[test]
    fn the_config_tree_leaves_llama_cpps_config_ini_where_it_reads_it() {
        let tmp = tempfile::tempdir().expect("tempdir");
        let (old, ours) = old_config_tree(tmp.path());
        let new = tmp.path().join("private-ai");
        assert_eq!(
            migrate_config_tree(&old, &new).expect("moved"),
            Outcome::Renamed
        );
        assert_config_ini_put_back(&old, &new, &ours);
        assert_eq!(
            names(tmp.path()),
            ["llama.cpp", "private-ai"],
            "no temp left"
        );
    }

    // The copy path: the same, with every rename out of the old tree failing
    // as across filesystems (the put-back crosses them too).
    #[test]
    fn the_copied_config_tree_leaves_llama_cpps_config_ini_too() {
        let tmp = tempfile::tempdir().expect("tempdir");
        let (old, ours) = old_config_tree(tmp.path());
        let new = tmp.path().join("private-ai");
        let put_back_from = new.join(LLAMA_CPP_CONFIG_FILE);
        let rename = failing_for(vec![old.clone(), put_back_from]);
        assert_eq!(
            migrate_config_tree_with(&old, &new, &rename).expect("copied"),
            Outcome::Copied
        );
        assert_config_ini_put_back(&old, &new, &ours);
        assert_eq!(
            names(tmp.path()),
            ["llama.cpp", "private-ai"],
            "no temp left"
        );
    }

    // A config tree with no config.ini: no old directory is re-created.
    #[test]
    fn the_config_tree_without_config_ini_leaves_no_old_directory() {
        let tmp = tempfile::tempdir().expect("tempdir");
        let old = old_tree(tmp.path());
        let new = tmp.path().join("private-ai");
        assert_eq!(
            migrate_config_tree(&old, &new).expect("moved"),
            Outcome::Renamed
        );
        assert!(!old.exists());
    }

    // The data tree has no such rule: its config.ini is a file like any other.
    #[test]
    fn the_data_tree_takes_config_ini_along() {
        let tmp = tempfile::tempdir().expect("tempdir");
        let (old, _) = old_config_tree(tmp.path());
        let new = tmp.path().join("private-ai");
        assert_eq!(migrate_tree(&old, &new).expect("moved"), Outcome::Renamed);
        assert!(!old.exists());
        assert!(new.join(LLAMA_CPP_CONFIG_FILE).is_file());
    }

    // Putting config.ini back fails (the old directory cannot be re-created:
    // its parent turns read-only right after the move). The error comes back,
    // naming where the file now is, and the file is in the new tree, not lost.
    #[cfg(unix)]
    #[test]
    fn a_failed_put_back_keeps_config_ini_in_the_new_tree_and_reports_it() {
        use std::os::unix::fs::PermissionsExt;
        let tmp = tempfile::tempdir().expect("tempdir");
        let root = tmp.path().to_path_buf();
        let (old, _) = old_config_tree(&root);
        let new = root.join("private-ai");
        let set_mode = |mode| {
            std::fs::set_permissions(&root, std::fs::Permissions::from_mode(mode)).expect("chmod")
        };
        // Probe: does a read-only directory refuse a new entry (not as root)?
        let probe = root.join("probe");
        std::fs::create_dir(&probe).expect("probe");
        std::fs::set_permissions(&probe, std::fs::Permissions::from_mode(0o555)).expect("chmod");
        let refuses = std::fs::create_dir(probe.join("x")).is_err();
        std::fs::set_permissions(&probe, std::fs::Permissions::from_mode(0o755)).expect("chmod");
        std::fs::remove_dir_all(&probe).expect("probe gone");
        if !refuses {
            eprintln!("skipped: running as root, a mode-555 directory is still writable");
            return;
        }
        let old_moved = old.clone();
        let rename = move |from: &Path, to: &Path| -> io::Result<()> {
            std::fs::rename(from, to)?;
            if from == old_moved {
                std::fs::set_permissions(
                    old_moved.parent().expect("parent"),
                    std::fs::Permissions::from_mode(0o555),
                )?;
            }
            Ok(())
        };

        let result = migrate_config_tree_with(&old, &new, &rename);
        set_mode(0o755);

        let err = result.expect_err("the put-back must fail");
        assert_eq!(err.kind(), io::ErrorKind::PermissionDenied, "{err}");
        let kept = new.join(LLAMA_CPP_CONFIG_FILE);
        assert!(
            err.to_string().contains(&kept.display().to_string()),
            "the error says where config.ini now is: {err}"
        );
        assert_eq!(
            std::fs::read_to_string(&kept).expect("kept in the new tree"),
            "ctx-size = 4096\n"
        );
        assert!(new.join("server.ini").is_file(), "the move itself stands");
    }
}
