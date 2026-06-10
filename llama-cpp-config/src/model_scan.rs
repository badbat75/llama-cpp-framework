// Catalogue .gguf files under a ModelsDir subdirectory for the GUI dropdowns.

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
}

impl Category {
    pub fn is_optional(self) -> bool {
        matches!(self, Category::Mmproj)
    }

    pub fn subdir(self) -> &'static str {
        match self {
            Category::Model => "models",
            Category::Mmproj => "mmprojs",
        }
    }
}

pub fn list(root_dir: &str, subdir: &str) -> Vec<FileOption> {
    let dir = PathBuf::from(root_dir).join(subdir);
    let dir_str = dir.to_string_lossy();
    if dir_str.trim().is_empty() {
        return Vec::new();
    }
    let root = dir;
    let mut out: Vec<FileOption> = Vec::new();
    let mut seen_paths: std::collections::HashSet<String> = std::collections::HashSet::new();
    let mut walk: Vec<PathBuf> = vec![root.clone()];

    while let Some(dir) = walk.pop() {
        if !dir.is_dir() {
            continue;
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
                out.push(FileOption { label, path: path_str });
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
    // Match -NNNNN-of-NNNNN.gguf where the first number isn't 00001
    let stem = name
        .strip_suffix(".gguf")
        .or_else(|| name.strip_suffix(".GGUF"))
        .unwrap_or(name);
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
        return (
            labels,
            values,
            if category.is_optional() { 0 } else { -1 },
        );
    }

    if let Some(i) = values.iter().position(|v| paths_eq(v, current_trim)) {
        return (labels, values, i as i32);
    }

    let basename = Path::new(current_trim)
        .file_name()
        .map(|s| s.to_string_lossy().into_owned())
        .unwrap_or_else(|| current_trim.to_string());
    let insert_at = if category.is_optional() { 1 } else { 0 };
    labels.insert(insert_at, format!("(custom) {basename}"));
    values.insert(insert_at, current_trim.to_string());
    (labels, values, insert_at as i32)
}

fn paths_eq(a: &str, b: &str) -> bool {
    fn norm(s: &str) -> String {
        s.trim()
            .replace('\\', "/")
            .to_ascii_lowercase()
    }
    norm(a) == norm(b)
}
