// Catalogue .gguf files under a ModelsDir subdirectory for the GUI dropdowns.
//
// `list` walks `<root>/<Category::subdir()>` recursively, skipping non-first
// shards of multi-file GGUFs. `build_options` / `build_draft_options` turn a
// scan into the `(labels, values[, specs], index)` arrays a dropdown binds to;
// a `current` value missing from the scan is preserved as a "(custom)" row
// (`custom_row`) so a stale or hand-edited path never silently disappears.

use std::path::{Component, Path, PathBuf};

#[derive(Debug, Clone)]
pub struct FileOption {
    pub label: String,
    pub path: String,
}

#[derive(Copy, Clone, Debug)]
pub enum Category {
    Model,
    Mmproj,
    /// Draft / Multi-Token Prediction head GGUFs (scanned from `mtps\`).
    Mtp,
    /// DFlash block-diffusion drafter GGUFs (scanned from `dflashs\`).
    Dflash,
}

impl Category {
    pub fn is_optional(self) -> bool {
        matches!(self, Category::Mmproj | Category::Mtp | Category::Dflash)
    }

    pub fn subdir(self) -> &'static str {
        match self {
            Category::Model => "models",
            Category::Mmproj => "mmprojs",
            Category::Mtp => "mtps",
            Category::Dflash => "dflashs",
        }
    }
}

pub fn list(root_dir: &str, subdir: &str) -> Vec<FileOption> {
    // Guard the actual variable input: a blank models-dir would otherwise scan a
    // relative `./<subdir>` against the process cwd. (`subdir` is always a
    // non-empty Category::subdir(), so joining it can't produce the empty path.)
    if root_dir.trim().is_empty() {
        return Vec::new();
    }
    let root = PathBuf::from(root_dir).join(subdir);
    let mut out: Vec<FileOption> = Vec::new();
    let mut seen_paths: std::collections::HashSet<String> = std::collections::HashSet::new();
    // Visited-directory set, keyed on the canonicalized path: `is_dir()`
    // follows junctions/symlinks, so a self-referencing junction inside the
    // tree would otherwise loop the worklist forever (the GUI scans on every
    // dropdown rebuild — a hang, not just a slow scan). Canonicalize resolves
    // the link target, so revisiting an already-walked dir is a cheap skip.
    let mut visited_dirs: std::collections::HashSet<PathBuf> = std::collections::HashSet::new();
    let mut walk: Vec<PathBuf> = vec![root.clone()];

    while let Some(dir) = walk.pop() {
        if !dir.is_dir() {
            continue;
        }
        if let Ok(canon) = dir.canonicalize() {
            if !visited_dirs.insert(canon) {
                continue;
            }
        }
        let Ok(entries) = std::fs::read_dir(&dir) else {
            continue;
        };
        for e in entries.flatten() {
            let p = e.path();
            if p.is_dir() {
                walk.push(p);
                continue;
            }
            if !p.is_file() {
                continue;
            }
            let ext = p
                .extension()
                .and_then(|s| s.to_str())
                .map(|s| s.to_ascii_lowercase());
            let is_gguf = ext.as_deref() == Some("gguf");
            if !is_gguf {
                continue;
            }
            let name = match p.file_name().and_then(|s| s.to_str()) {
                Some(s) => s.to_string(),
                None => continue,
            };
            // Skip multi-shard files that aren't the first shard
            if is_multi_shard_trailer(&name) {
                continue;
            }
            let path_str = normalize(&p).to_string_lossy().into_owned();
            let key = path_str.to_ascii_lowercase();
            if seen_paths.insert(key) {
                // Use relative path from models dir as label
                let label = p
                    .strip_prefix(&root)
                    .ok()
                    .map(|r| r.to_string_lossy().to_string())
                    .unwrap_or(name);
                out.push(FileOption {
                    label,
                    path: path_str,
                });
            }
        }
    }

    out.sort_by(|a, b| {
        a.label
            .to_ascii_lowercase()
            .cmp(&b.label.to_ascii_lowercase())
    });
    out
}

/// Split a `-NNNNN-of-NNNNN` multi-shard suffix off a file stem.
/// Returns `(base, shard_counter)`, e.g. `("model", "00002")`.
pub(crate) fn split_shard_suffix(stem: &str) -> Option<(&str, &str)> {
    let (head, total) = stem.rsplit_once("-of-")?;
    if total.len() != 5 || !total.chars().all(|c| c.is_ascii_digit()) {
        return None;
    }
    let idx = head.rfind('-')?;
    let counter = &head[idx + 1..];
    if counter.len() != 5 || !counter.chars().all(|c| c.is_ascii_digit()) {
        return None;
    }
    Some((&head[..idx], counter))
}

fn is_multi_shard_trailer(name: &str) -> bool {
    // Match -NNNNN-of-NNNNN.gguf where the first number isn't 00001. The
    // extension strip must be case-INsensitive like the scan's extension
    // filter, or a `.Gguf` non-first shard slips into the dropdown.
    let stem = match name.len().checked_sub(5) {
        Some(cut) if name.is_char_boundary(cut) && name[cut..].eq_ignore_ascii_case(".gguf") => {
            &name[..cut]
        }
        _ => name,
    };
    matches!(split_shard_suffix(stem), Some((_, counter)) if counter != "00001")
}

fn normalize(p: &Path) -> PathBuf {
    let mut out = PathBuf::new();
    for c in p.components() {
        match c {
            Component::ParentDir => {
                out.pop();
            }
            Component::CurDir => {}
            other => out.push(other.as_os_str()),
        }
    }
    out
}

pub fn build_options(
    category: Category,
    scanned: Vec<FileOption>,
    current: &str,
) -> (Vec<String>, Vec<String>, i32) {
    let mut labels: Vec<String> = Vec::new();
    let mut values: Vec<String> = Vec::new();

    if category.is_optional() {
        labels.push("(none)".into());
        values.push(String::new());
    }
    for opt in scanned {
        labels.push(opt.label);
        values.push(opt.path);
    }

    let current_trim = current.trim();
    if current_trim.is_empty() {
        return (labels, values, if category.is_optional() { 0 } else { -1 });
    }

    if let Some(i) = values.iter().position(|v| paths_eq(v, current_trim)) {
        return (labels, values, i as i32);
    }

    let (label, value) = custom_row(current_trim);
    let insert_at = if category.is_optional() { 1 } else { 0 };
    labels.insert(insert_at, label);
    values.insert(insert_at, value);
    (labels, values, insert_at as i32)
}

/// The `(label, value)` pair preserving a current path that matched no scanned
/// file — shared by `build_options` and `build_draft_options` so the two
/// "(custom)" rows can't drift.
fn custom_row(current: &str) -> (String, String) {
    let basename = Path::new(current)
        .file_name()
        .map(|s| s.to_string_lossy().into_owned())
        .unwrap_or_else(|| current.to_string());
    (format!("(custom) {basename}"), current.to_string())
}

/// Build the unified draft-model dropdown that merges MTP heads (`mtps\`) and
/// DFlash drafters (`dflashs\`) into one picker. Both feed `--model-draft`, so
/// they share one combobox; the parallel `specs` vector carries the `--spec-type`
/// to apply when each row is picked ("none" for the empty row, "draft-mtp" /
/// "draft-dflash" for scanned entries). Returns `(labels, values, specs, index)`.
///
/// A `current` path that matches no scanned file is preserved as a `(custom)`
/// row keeping its existing `current_spec`, mirroring [`build_options`].
pub fn build_draft_options(
    mtp: Vec<FileOption>,
    dflash: Vec<FileOption>,
    current: &str,
    current_spec: &str,
) -> (Vec<String>, Vec<String>, Vec<String>, i32) {
    let mut labels: Vec<String> = vec!["(none)".into()];
    let mut values: Vec<String> = vec![String::new()];
    let mut specs: Vec<String> = vec!["none".into()];

    for opt in mtp {
        labels.push(format!("MTP: {}", opt.label));
        values.push(opt.path);
        specs.push("draft-mtp".into());
    }
    for opt in dflash {
        labels.push(format!("DFlash: {}", opt.label));
        values.push(opt.path);
        specs.push("draft-dflash".into());
    }

    let current_trim = current.trim();
    if current_trim.is_empty() {
        return (labels, values, specs, 0);
    }
    if let Some(i) = values.iter().position(|v| paths_eq(v, current_trim)) {
        return (labels, values, specs, i as i32);
    }

    let spec = match current_spec.trim() {
        "" => "none",
        other => other,
    };
    let (label, value) = custom_row(current_trim);
    labels.insert(1, label);
    values.insert(1, value);
    specs.insert(1, spec.to_string());
    (labels, values, specs, 1)
}

fn paths_eq(a: &str, b: &str) -> bool {
    fn norm(s: &str) -> String {
        s.trim().replace('\\', "/").to_ascii_lowercase()
    }
    norm(a) == norm(b)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn opt(label: &str, path: &str) -> FileOption {
        FileOption {
            label: label.into(),
            path: path.into(),
        }
    }

    #[test]
    fn draft_options_merge_mtp_and_dflash_with_specs() {
        let mtp = vec![opt("a-mtp.gguf", r"C:\mtps\a-mtp.gguf")];
        let dflash = vec![opt("b-dflash.gguf", r"C:\dflash\b-dflash.gguf")];
        let (labels, values, specs, idx) = build_draft_options(mtp, dflash, "", "");
        assert_eq!(
            labels,
            vec!["(none)", "MTP: a-mtp.gguf", "DFlash: b-dflash.gguf"]
        );
        assert_eq!(values[0], "");
        assert_eq!(specs, vec!["none", "draft-mtp", "draft-dflash"]);
        assert_eq!(idx, 0);
    }

    #[test]
    fn draft_options_select_current_dflash_entry() {
        let mtp = vec![opt("a-mtp.gguf", r"C:\mtps\a-mtp.gguf")];
        let dflash = vec![opt("b-dflash.gguf", r"C:\dflash\b-dflash.gguf")];
        let (_, _, specs, idx) =
            build_draft_options(mtp, dflash, r"C:/dflash/b-dflash.gguf", "draft-dflash");
        // Path matches the DFlash row (index 2) despite the slash-style difference.
        assert_eq!(idx, 2);
        assert_eq!(specs[idx as usize], "draft-dflash");
    }

    #[test]
    fn draft_options_preserve_custom_path_and_spec() {
        let (labels, values, specs, idx) =
            build_draft_options(vec![], vec![], r"D:\stray\draft.gguf", "draft-dflash");
        assert_eq!(idx, 1);
        assert_eq!(labels[1], "(custom) draft.gguf");
        assert_eq!(values[1], r"D:\stray\draft.gguf");
        assert_eq!(specs[1], "draft-dflash");
    }

    // The per-category index rules of the plain build_options — the contract an
    // agent copies when adding the next file-backed dropdown.

    #[test]
    fn build_options_required_empty_selects_nothing() {
        let (labels, _, idx) = build_options(Category::Model, vec![opt("a", r"C:\a")], "");
        assert_eq!(labels, vec!["a"]); // no (none) row for a required category
        assert_eq!(idx, -1);
    }

    #[test]
    fn build_options_optional_prepends_none_row() {
        let (labels, values, idx) = build_options(Category::Mmproj, vec![opt("a", r"C:\a")], "");
        assert_eq!(labels, vec!["(none)", "a"]);
        assert_eq!(values[0], "");
        assert_eq!(idx, 0); // empty current selects the (none) row
    }

    #[test]
    fn build_options_matches_paths_slash_and_case_insensitively() {
        let scanned = vec![opt("m.gguf", r"C:\Models\m.gguf")];
        let (_, _, idx) = build_options(Category::Model, scanned, "c:/models/M.GGUF");
        assert_eq!(idx, 0);
    }

    #[test]
    fn build_options_stale_value_inserts_after_fixed_rows() {
        // Required: the (custom) row lands at 0; optional: after the (none) row.
        let (labels, _, idx) =
            build_options(Category::Model, vec![opt("a", r"C:\a")], r"D:\gone\x.gguf");
        assert_eq!(idx, 0);
        assert_eq!(labels[0], "(custom) x.gguf");
        let (labels, _, idx) =
            build_options(Category::Mmproj, vec![opt("a", r"C:\a")], r"D:\gone\x.gguf");
        assert_eq!(idx, 1);
        assert_eq!(labels[1], "(custom) x.gguf");
    }

    // The shard gate that keeps the model dropdown from listing every shard of
    // a multi-file GGUF: only exactly-5-digit `-NNNNN-of-NNNNN` suffixes count,
    // and only the 00001 shard is listed.

    #[test]
    fn split_shard_suffix_accepts_5digit_counters_only() {
        assert_eq!(
            split_shard_suffix("model-00002-of-00003"),
            Some(("model", "00002"))
        );
        assert_eq!(split_shard_suffix("model-2-of-3"), None);
        assert_eq!(split_shard_suffix("model-000002-of-000003"), None);
        assert_eq!(split_shard_suffix("model"), None);
    }

    #[test]
    fn multi_shard_trailers_are_skipped_first_shard_kept() {
        assert!(is_multi_shard_trailer("model-00002-of-00003.gguf"));
        assert!(!is_multi_shard_trailer("model-00001-of-00003.gguf"));
        assert!(!is_multi_shard_trailer("model.gguf"));
        // The extension strip is case-insensitive like the scan's filter —
        // a mixed-case shard must not slip past the gate.
        assert!(is_multi_shard_trailer("model-00002-of-00003.Gguf"));
        assert!(is_multi_shard_trailer("model-00002-of-00003.GGUF"));
    }
}
