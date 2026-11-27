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
    std::fs::set_permissions(&locked, std::fs::Permissions::from_mode(0o644)).expect("chmod back");

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
