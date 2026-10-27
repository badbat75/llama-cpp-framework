//! The model files a preset links, and deleting them together with the preset.
//!
//! A preset names up to three files: `model` (every shard of a split model),
//! `mmproj` and `model-draft` (an MTP head, a DFlash or a DSpark drafter).
//! Deleting the preset can take them too: the Models tab's Delete dialog
//! (`gui/models_tab.rs`, one tick per file) and `preset delete --delete-files`
//! (`cli.rs`). Deleting gigabytes is not undoable, so the rules are:
//!
//! - **Keep is the default.** Nothing here runs unless the user picked the
//!   files: every tick starts cleared, the CLI needs the flag.
//! - **A file another preset references is never deleted**, whichever INI file
//!   that preset sits in (`presets.ini` or `presets.ini.disabled`: callers pass
//!   `presets::load_listed`, both files). `users` names those presets, the
//!   dialog shows the file as in use and cannot tick it, and `delete` skips it
//!   unless the CLI's `--even-if-shared` asks otherwise. Paths are compared the
//!   way the pickers match them (`model_scan::paths_eq`: separators and ASCII
//!   case folded), which can only err toward "in use", the safe side.
//! - **A split model is ONE entry.** `shard_set` derives every shard's name from
//!   the `-NNNNN-of-NNNNN` suffix (the shard the preset names may be any of
//!   them), the entry deletes every shard that exists, and the in-use check
//!   expands the OTHER presets' paths the same way, so a preset naming shard 2
//!   of the set still holds it.
//! - **Empty folders go, and only inside ModelsDir's four subfolders.** A
//!   Hugging Face download lands in `<ModelsDir>\<sub>\<org>\<repo>\` (`hf.rs`),
//!   so deleting its files leaves that folder (and maybe the org's) empty.
//!   `remove_empty_dirs` walks up from each deleted file's folder, removing it
//!   while it is empty, and stops at the first one with content, at a link, and
//!   in every case BELOW the subfolder (`models`, `mmprojs`, `mtps`, `dflashs`),
//!   which is never removed itself. The bound is lexical (component by
//!   component, a `..` anywhere refuses the walk), and `fs::remove_dir` is the
//!   second guard: it fails on a folder that is not empty. A file outside those
//!   subfolders can still be deleted when the preset links it there; nothing
//!   around it is touched.
//!
//! Everything but `linked_files` (which stats the files), `delete` and
//! `remove_empty_dirs` is pure; the three filesystem ones are tested on a
//! tempdir.

use std::fs;
use std::path::{Component, Path, PathBuf};

use crate::model_scan::{self, Category};
use crate::presets::Preset;

/// Which key of the preset links the file.
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum Role {
    Model,
    Mmproj,
    Draft,
}

impl Role {
    pub const ALL: [Role; 3] = [Role::Model, Role::Mmproj, Role::Draft];

    /// The row label's head in the dialog and the CLI's lines.
    pub fn label(self) -> &'static str {
        match self {
            Role::Model => "Model",
            Role::Mmproj => "MMProj",
            Role::Draft => "Drafter",
        }
    }

    /// The preset's value for this key, as written (untrimmed).
    pub fn path_of(self, p: &Preset) -> &str {
        match self {
            Role::Model => &p.model,
            Role::Mmproj => &p.mmproj,
            Role::Draft => &p.model_draft,
        }
    }
}

/// One file (or one split model) a preset links that exists on disk.
#[derive(Clone, Debug, PartialEq)]
pub struct LinkedFile {
    pub role: Role,
    /// The path as the preset writes it.
    pub path: PathBuf,
    /// What deleting the entry removes: the shards of a split model that
    /// exist, else the one file. Never empty.
    pub files: Vec<PathBuf>,
    /// Bytes on disk, every file of `files` together.
    pub size: u64,
    /// The OTHER presets referencing any of `files`, by id: non-empty means in
    /// use, so the dialog cannot tick it and `delete` keeps it.
    pub used_by: Vec<String>,
}

impl LinkedFile {
    pub fn in_use(&self) -> bool {
        !self.used_by.is_empty()
    }

    /// `name.gguf`, or `name-00001-of-00003.gguf (3 shards)`.
    pub fn name(&self) -> String {
        let name = file_name(&self.path);
        if self.files.len() > 1 {
            format!("{name} ({} shards)", self.files.len())
        } else {
            name
        }
    }

    /// The dialog row's first line: `Model: name.gguf (2 shards)`.
    pub fn row_label(&self) -> String {
        format!("{}: {}", self.role.label(), self.name())
    }

    /// The dialog row's second line: the size, then where it is or who else
    /// uses it.
    pub fn row_detail(&self) -> String {
        let size = crate::hf::format_size(self.size);
        if self.in_use() {
            format!("{size}  ·  in use by {}: kept", bracketed(&self.used_by))
        } else {
            let dir = self
                .path
                .parent()
                .map(|d| d.to_string_lossy().into_owned())
                .unwrap_or_default();
            format!("{size}  ·  {dir}")
        }
    }
}

fn file_name(p: &Path) -> String {
    p.file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_else(|| p.to_string_lossy().into_owned())
}

/// `[a], [b]`: preset ids the way every status line writes them.
fn bracketed(ids: &[String]) -> String {
    ids.iter()
        .map(|id| format!("[{id}]"))
        .collect::<Vec<_>>()
        .join(", ")
}

/// Every shard of the split model `path` belongs to, by name only (no disk
/// read): `m-00002-of-00003.gguf` gives `m-00001-of-00003.gguf` through
/// `m-00003-of-00003.gguf`, in the folder of `path` and with its extension's
/// spelling. Anything that is not a `-NNNNN-of-NNNNN` shard is a set of one.
pub fn shard_set(path: &Path) -> Vec<PathBuf> {
    let one = || vec![path.to_path_buf()];
    let Some(name) = path.file_name().and_then(|n| n.to_str()) else {
        return one();
    };
    let Some(cut) = name.len().checked_sub(5) else {
        return one();
    };
    if !name.is_char_boundary(cut) || !name[cut..].eq_ignore_ascii_case(".gguf") {
        return one();
    }
    let (stem, ext) = (&name[..cut], &name[cut..]);
    let Some((base, _)) = model_scan::split_shard_suffix(stem) else {
        return one();
    };
    // split_shard_suffix checked the 5 trailing digits.
    let total_text = &stem[stem.len() - 5..];
    match total_text.parse::<u32>() {
        Ok(total) if total > 0 => (1..=total)
            .map(|i| path.with_file_name(format!("{base}-{i:05}-of-{total_text}{ext}")))
            .collect(),
        _ => one(),
    }
}

fn same_path(a: &Path, b: &Path) -> bool {
    model_scan::paths_eq(&a.to_string_lossy(), &b.to_string_lossy())
}

/// Every path a preset references, split models expanded to all their shards.
fn referenced(p: &Preset) -> Vec<PathBuf> {
    Role::ALL
        .iter()
        .map(|r| r.path_of(p).trim())
        .filter(|s| !s.is_empty())
        .flat_map(|s| shard_set(Path::new(s)))
        .collect()
}

/// The presets other than `target_id` (compared ignoring ASCII case, like the
/// INI layer) that reference any of `files`, in the order of `all`.
pub fn users(files: &[PathBuf], target_id: &str, all: &[Preset]) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    for p in all.iter().filter(|p| !p.id.eq_ignore_ascii_case(target_id)) {
        let hit = referenced(p)
            .iter()
            .any(|r| files.iter().any(|f| same_path(f, r)));
        if hit && !out.iter().any(|id| id.eq_ignore_ascii_case(&p.id)) {
            out.push(p.id.clone());
        }
    }
    out
}

/// The files `target` links that exist on disk, model first, then the mmproj,
/// then the drafter; a path linked under two keys is listed once. `all` is
/// every preset of both INI files (it may include `target` itself, which is
/// skipped by id), for the in-use check.
pub fn linked_files(target: &Preset, all: &[Preset]) -> Vec<LinkedFile> {
    let mut out: Vec<LinkedFile> = Vec::new();
    let mut seen: Vec<PathBuf> = Vec::new();
    for role in Role::ALL {
        let raw = role.path_of(target).trim();
        if raw.is_empty() {
            continue;
        }
        let path = PathBuf::from(raw);
        let set = shard_set(&path);
        if set.iter().any(|s| seen.iter().any(|x| same_path(x, s))) {
            continue;
        }
        seen.extend(set.iter().cloned());
        let files: Vec<PathBuf> = set.into_iter().filter(|f| f.is_file()).collect();
        if files.is_empty() {
            continue;
        }
        let size = files
            .iter()
            .filter_map(|f| fs::metadata(f).ok())
            .map(|m| m.len())
            .sum();
        let used_by = users(&files, &target.id, all);
        out.push(LinkedFile {
            role,
            path,
            files,
            size,
            used_by,
        });
    }
    out
}

/// One line saying what a confirm would remove, for the dialog under its
/// list: the ticked entries of `files` (`ticked` is parallel to it).
pub fn selection_summary(files: &[LinkedFile], ticked: &[bool]) -> String {
    let chosen: Vec<&LinkedFile> = files
        .iter()
        .zip(ticked)
        .filter(|(f, t)| **t && !f.in_use())
        .map(|(f, _)| f)
        .collect();
    if chosen.is_empty() {
        return "Only the preset is deleted: every file stays on disk.".into();
    }
    let bytes: u64 = chosen.iter().map(|f| f.size).sum();
    let names: Vec<String> = chosen.iter().map(|f| f.name()).collect();
    format!(
        "Also deletes from disk, {}: {}.",
        crate::hf::format_size(bytes),
        names.join(", ")
    )
}

/// What `delete` did.
#[derive(Debug, Default, PartialEq)]
pub struct Outcome {
    /// Each file removed, with its size.
    pub removed: Vec<(PathBuf, u64)>,
    /// Entries skipped because another preset uses them: (name, preset ids).
    pub kept: Vec<(String, Vec<String>)>,
    /// Files that could not be removed, with the error.
    pub failed: Vec<(PathBuf, String)>,
    /// The empty folders removed after the files (`remove_empty_dirs`).
    pub dirs_removed: Vec<PathBuf>,
}

/// Delete every file of `chosen`, then the folders that left empty. An entry in
/// use (`used_by`, which the caller computes against the presets as they stand
/// NOW) is kept unless `even_if_shared`. A file that fails is reported and the
/// rest still go: nothing here is all-or-nothing, since what was removed cannot
/// be put back anyway.
pub fn delete(chosen: &[LinkedFile], models_dir: &str, even_if_shared: bool) -> Outcome {
    let mut out = Outcome::default();
    for lf in chosen {
        if lf.in_use() && !even_if_shared {
            out.kept.push((lf.name(), lf.used_by.clone()));
            continue;
        }
        for f in &lf.files {
            let size = fs::metadata(f).map(|m| m.len()).unwrap_or(0);
            match fs::remove_file(f) {
                Ok(()) => out.removed.push((f.clone(), size)),
                Err(e) => out.failed.push((f.clone(), e.to_string())),
            }
        }
    }
    // After every file: two shards share one folder, and the folder is empty
    // only once both are gone.
    let removed: Vec<PathBuf> = out.removed.iter().map(|(f, _)| f.clone()).collect();
    for f in removed {
        for d in remove_empty_dirs(&f, models_dir) {
            if !out.dirs_removed.contains(&d) {
                out.dirs_removed.push(d);
            }
        }
    }
    out
}

/// Remove the folders `file` sat in while they are empty, innermost first,
/// strictly inside one of ModelsDir's four subfolders (see the module header).
/// Returns the folders removed.
pub fn remove_empty_dirs(file: &Path, models_dir: &str) -> Vec<PathBuf> {
    let mut removed = Vec::new();
    let models_dir = models_dir.trim();
    if models_dir.is_empty() {
        return removed;
    }
    let Some(start) = file.parent() else {
        return removed;
    };
    let Some(levels) = depth_inside_subfolder(start, Path::new(models_dir)) else {
        return removed;
    };
    let mut dir = start.to_path_buf();
    for _ in 0..levels {
        if !is_plain_empty_dir(&dir) || fs::remove_dir(&dir).is_err() {
            break;
        }
        removed.push(dir.clone());
        match dir.parent() {
            Some(p) => dir = p.to_path_buf(),
            None => break,
        }
    }
    removed
}

/// A real folder (not a link: removing a junction or a symlink would not even
/// look at what it points to) with nothing in it.
fn is_plain_empty_dir(dir: &Path) -> bool {
    let is_dir = fs::symlink_metadata(dir)
        .map(|m| m.is_dir() && !m.file_type().is_symlink())
        .unwrap_or(false);
    is_dir
        && fs::read_dir(dir)
            .map(|mut it| it.next().is_none())
            .unwrap_or(false)
}

/// How many levels `dir` lies below one of `<models_dir>\models`, `mmprojs`,
/// `mtps`, `dflashs` (1 for a folder directly in it), compared component by
/// component; `None` for the subfolder itself, anything outside them, and any
/// path with a `..` in it.
fn depth_inside_subfolder(dir: &Path, models_dir: &Path) -> Option<usize> {
    let parts = |p: &Path| -> Option<Vec<String>> {
        let mut out = Vec::new();
        for c in p.components() {
            match c {
                Component::CurDir => {}
                Component::ParentDir => return None,
                other => out.push(other.as_os_str().to_string_lossy().into_owned()),
            }
        }
        Some(out)
    };
    let d = parts(dir)?;
    for cat in [Category::Model, Category::Mmproj, Category::Mtp, Category::Dflash] {
        let r = parts(&models_dir.join(cat.subdir()))?;
        if d.len() > r.len() && r.iter().zip(&d).all(|(a, b)| component_eq(a, b)) {
            return Some(d.len() - r.len());
        }
    }
    None
}

#[cfg(windows)]
fn component_eq(a: &str, b: &str) -> bool {
    a.eq_ignore_ascii_case(b)
}

#[cfg(not(windows))]
fn component_eq(a: &str, b: &str) -> bool {
    a == b
}

/// The status line after a delete: `(text, is_error)`. An error only when a
/// file could not be removed; a file kept because another preset uses it is
/// the rule working, not a failure.
pub fn report(id: &str, out: &Outcome) -> (String, bool) {
    let mut parts = vec![format!("Deleted [{id}]")];
    if !out.removed.is_empty() {
        let bytes: u64 = out.removed.iter().map(|(_, s)| s).sum();
        let names: Vec<String> = out.removed.iter().map(|(f, _)| file_name(f)).collect();
        let n = out.removed.len();
        parts.push(format!(
            "removed {n} file{} ({}): {}",
            if n == 1 { "" } else { "s" },
            crate::hf::format_size(bytes),
            names.join(", ")
        ));
    }
    if !out.dirs_removed.is_empty() {
        let n = out.dirs_removed.len();
        parts.push(format!(
            "{n} empty folder{} removed",
            if n == 1 { "" } else { "s" }
        ));
    }
    for (name, ids) in &out.kept {
        parts.push(format!("kept {name}: in use by {}", bracketed(ids)));
    }
    for (f, e) in &out.failed {
        parts.push(format!("could not delete {}: {e}", f.display()));
    }
    (parts.join("; "), !out.failed.is_empty())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn preset(id: &str, model: &str, mmproj: &str, draft: &str) -> Preset {
        Preset {
            id: id.into(),
            model: model.into(),
            mmproj: mmproj.into(),
            model_draft: draft.into(),
            ..Preset::default()
        }
    }

    fn touch(p: &Path, bytes: usize) {
        fs::create_dir_all(p.parent().unwrap()).unwrap();
        fs::write(p, vec![0u8; bytes]).unwrap();
    }

    fn s(p: &Path) -> String {
        p.to_string_lossy().into_owned()
    }

    #[test]
    fn shard_set_expands_any_shard_to_the_whole_set() {
        let dir = Path::new("d");
        let set = shard_set(&dir.join("m-Q4-00002-of-00003.gguf"));
        assert_eq!(
            set,
            vec![
                dir.join("m-Q4-00001-of-00003.gguf"),
                dir.join("m-Q4-00002-of-00003.gguf"),
                dir.join("m-Q4-00003-of-00003.gguf"),
            ]
        );
        // The extension keeps its spelling.
        assert_eq!(
            shard_set(&dir.join("x-00001-of-00002.GGUF"))[1],
            dir.join("x-00002-of-00002.GGUF")
        );
    }

    #[test]
    fn shard_set_of_a_plain_file_is_the_file() {
        for name in ["m.gguf", "m-1-of-2.gguf", "m-00001-of-00002.bin", "m-00001-of-00000.gguf"] {
            let p = Path::new("d").join(name);
            assert_eq!(shard_set(&p), vec![p.clone()], "{name}");
        }
    }

    #[test]
    fn users_names_other_presets_in_either_file_and_matches_shards() {
        let all = vec![
            preset("me", r"C:\M\models\s-00001-of-00002.gguf", r"C:\M\mmprojs\p.gguf", ""),
            // Names shard 2 of the same set, in another spelling.
            preset("twin", "c:/m/models/S-00002-of-00002.gguf", "", ""),
            preset("vision", r"C:\M\models\other.gguf", r"C:\M\mmprojs\p.gguf", ""),
            preset("ME", r"C:\M\models\s-00001-of-00002.gguf", "", ""),
        ];
        let model = shard_set(Path::new(r"C:\M\models\s-00001-of-00002.gguf"));
        assert_eq!(users(&model, "me", &all), ["twin"], "the target is skipped in any case");
        assert_eq!(users(&[PathBuf::from(r"C:\M\mmprojs\p.gguf")], "me", &all), ["vision"]);
        assert!(users(&[PathBuf::from(r"C:\M\mtps\d.gguf")], "me", &all).is_empty());
    }

    #[test]
    fn linked_files_lists_what_exists_with_sizes_and_users() {
        let dir = tempfile::tempdir().unwrap();
        let m = dir.path();
        let s1 = m.join("models").join("o").join("r").join("s-00001-of-00002.gguf");
        let s2 = s1.with_file_name("s-00002-of-00002.gguf");
        let mm = m.join("mmprojs").join("o").join("r").join("mmproj.gguf");
        touch(&s1, 10);
        touch(&s2, 20);
        touch(&mm, 5);
        let target = preset("t", &s(&s1), &s(&mm), &s(&m.join("mtps").join("gone.gguf")));
        let all = vec![target.clone(), preset("other", "x.gguf", &s(&mm), "")];
        let files = linked_files(&target, &all);
        assert_eq!(files.len(), 2, "a drafter not on disk is not listed");
        assert_eq!(files[0].role, Role::Model);
        assert_eq!(files[0].files, vec![s1.clone(), s2.clone()]);
        assert_eq!(files[0].size, 30);
        assert!(!files[0].in_use());
        assert_eq!(files[0].row_label(), "Model: s-00001-of-00002.gguf (2 shards)");
        assert_eq!(files[1].role, Role::Mmproj);
        assert_eq!(files[1].used_by, ["other"]);
        assert!(files[1].row_detail().contains("in use by [other]"), "{}", files[1].row_detail());
        // A path under two keys is one entry.
        let doubled = preset("d", &s(&mm), &s(&mm), "");
        assert_eq!(linked_files(&doubled, &[]).len(), 1);
    }

    #[test]
    fn selection_summary_says_what_goes() {
        let lf = |name: &str, size: u64, used: &[&str]| LinkedFile {
            role: Role::Model,
            path: PathBuf::from(name),
            files: vec![PathBuf::from(name)],
            size,
            used_by: used.iter().map(|u| u.to_string()).collect(),
        };
        let files = vec![lf("a.gguf", 2 << 30, &[]), lf("b.gguf", 1 << 30, &["x"])];
        assert!(selection_summary(&files, &[false, false]).starts_with("Only the preset"));
        // An in-use entry never counts, even if a tick reached it.
        assert_eq!(
            selection_summary(&files, &[true, true]),
            "Also deletes from disk, 2.00 GiB: a.gguf."
        );
    }

    #[test]
    fn delete_removes_files_then_only_empty_repo_folders() {
        let dir = tempfile::tempdir().unwrap();
        let m = dir.path();
        let repo = m.join("models").join("org").join("repo");
        let s1 = repo.join("s-00001-of-00002.gguf");
        let s2 = repo.join("s-00002-of-00002.gguf");
        touch(&s1, 3);
        touch(&s2, 4);
        // The org folder holds another repo: it must stay.
        let keep = m.join("models").join("org").join("other").join("k.gguf");
        touch(&keep, 1);
        let draft = m.join("mtps").join("org2").join("repo").join("d.gguf");
        touch(&draft, 2);
        let target = preset("t", &s(&s1), "", &s(&draft));
        let files = linked_files(&target, &[]);
        let out = delete(&files, &s(m), false);
        assert_eq!(out.removed.len(), 3);
        assert!(out.failed.is_empty() && out.kept.is_empty());
        assert!(!s1.exists() && !s2.exists() && !draft.exists());
        assert!(!repo.exists(), "the emptied repo folder goes");
        assert!(keep.exists() && m.join("models").join("org").is_dir(), "a folder with content stays");
        assert!(!m.join("mtps").join("org2").exists(), "an emptied org folder goes too");
        assert!(m.join("models").is_dir() && m.join("mtps").is_dir(), "the subfolders themselves stay");
        assert_eq!(out.dirs_removed.len(), 3, "{:?}", out.dirs_removed);
        let (text, err) = report("t", &out);
        assert!(!err);
        assert!(text.starts_with("Deleted [t]; removed 3 files"), "{text}");
        assert!(text.contains("3 empty folders removed"), "{text}");
    }

    #[test]
    fn delete_keeps_shared_unless_forced_and_reports_failures() {
        let dir = tempfile::tempdir().unwrap();
        let m = dir.path();
        let mm = m.join("mmprojs").join("p.gguf");
        touch(&mm, 1);
        let shared = LinkedFile {
            role: Role::Mmproj,
            path: mm.clone(),
            files: vec![mm.clone()],
            size: 1,
            used_by: vec!["other".into()],
        };
        let out = delete(std::slice::from_ref(&shared), &s(m), false);
        assert!(mm.exists(), "in use: kept");
        assert_eq!(out.kept, vec![("p.gguf".to_string(), vec!["other".to_string()])]);
        let (text, err) = report("t", &out);
        assert!(!err && text.contains("kept p.gguf: in use by [other]"), "{text}");
        let out = delete(std::slice::from_ref(&shared), &s(m), true);
        assert!(!mm.exists() && out.removed.len() == 1, "forced");
        assert!(m.join("mmprojs").is_dir(), "a file directly in the subfolder leaves it alone");

        // A path that cannot be removed (a folder here) is reported, and the
        // next file still goes.
        let blocker = m.join("models").join("r").join("b.gguf");
        fs::create_dir_all(&blocker).unwrap();
        let fine = m.join("models").join("r").join("f.gguf");
        touch(&fine, 1);
        let entry = |p: &Path| LinkedFile {
            role: Role::Model,
            path: p.to_path_buf(),
            files: vec![p.to_path_buf()],
            size: 0,
            used_by: Vec::new(),
        };
        let out = delete(&[entry(&blocker), entry(&fine)], &s(m), false);
        assert_eq!(out.failed.len(), 1);
        assert!(!fine.exists());
        assert!(blocker.is_dir() && out.dirs_removed.is_empty(), "the folder still has content");
        let (text, err) = report("t", &out);
        assert!(err && text.contains("could not delete"), "{text}");
    }

    #[test]
    fn folder_cleanup_never_leaves_the_four_subfolders() {
        let dir = tempfile::tempdir().unwrap();
        let m = dir.path().join("M");
        // Outside ModelsDir, and in a ModelsDir folder that is not one of the
        // four: nothing is removed, empty or not.
        let outside = dir.path().join("elsewhere").join("x.gguf");
        fs::create_dir_all(outside.parent().unwrap()).unwrap();
        assert!(remove_empty_dirs(&outside, &s(&m)).is_empty());
        assert!(outside.parent().unwrap().is_dir());
        let stray = m.join("downloads").join("x.gguf");
        fs::create_dir_all(stray.parent().unwrap()).unwrap();
        assert!(remove_empty_dirs(&stray, &s(&m)).is_empty());
        assert!(stray.parent().unwrap().is_dir());
        // Directly in a subfolder: the subfolder is the bound, never removed.
        let direct = m.join("models").join("x.gguf");
        fs::create_dir_all(direct.parent().unwrap()).unwrap();
        assert!(remove_empty_dirs(&direct, &s(&m)).is_empty());
        assert!(m.join("models").is_dir());
        // A `..` is refused rather than resolved.
        let dotted = m.join("models").join("a").join("..").join("b").join("x.gguf");
        fs::create_dir_all(m.join("models").join("b")).unwrap();
        assert!(remove_empty_dirs(&dotted, &s(&m)).is_empty());
        // No ModelsDir: no cleanup.
        assert!(remove_empty_dirs(&m.join("models").join("b").join("x.gguf"), " ").is_empty());
        assert!(m.join("models").join("b").is_dir());
        // The normal case, for contrast.
        assert_eq!(
            remove_empty_dirs(&m.join("models").join("b").join("x.gguf"), &s(&m)),
            vec![m.join("models").join("b")]
        );
    }

    #[test]
    fn depth_counts_levels_below_a_subfolder() {
        let m = Path::new("root");
        assert_eq!(depth_inside_subfolder(&m.join("models").join("o").join("r"), m), Some(2));
        assert_eq!(depth_inside_subfolder(&m.join("dflashs").join("o"), m), Some(1));
        assert_eq!(depth_inside_subfolder(&m.join("models"), m), None);
        assert_eq!(depth_inside_subfolder(Path::new("other").join("models").join("o").as_path(), m), None);
    }
}
