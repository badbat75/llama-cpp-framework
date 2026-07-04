// Minimal INI parser / writer that preserves comments and section order.
//
// The behavioral contract callers rely on:
// - Section lookup is CASE-INSENSITIVE and WHITESPACE-TOLERANT everywhere
//   (read_all trims each line; the writers match via find_section_header /
//   next_section_start with the same tolerance) — a hand-edited `[server]` or
//   `  [server]` header never spawns a duplicate and is never swallowed by a
//   neighbouring section's rewrite.
// - KEY lookup is asymmetric by design: reads are exact-case (`BTreeMap`
//   lookups like `keys.get("Port")`), while replace_key matches an existing
//   key case-insensitively and rewrites the line in canonical case.
// - Values are TRIMMED on parse, and an inline `; note` / `# note` tail is
//   stripped only when the marker has whitespace on BOTH sides ("a;b" stays a
//   value) — writers must therefore emit trimmed values or break round-trips.
// - Writers preserve the file's existing line endings (per line on
//   replace_key's replace path, detected once everywhere else — section
//   bodies arrive CRLF from the renderers and are converted to the file's
//   style); brand-new content defaults to CRLF.
// - `atomic_write` (sibling temp file + rename) is the canonical write path —
//   every config writer in the crate funnels through it.
// - `parse_int` / `parse_float` / `parse_bool` are the shared lenient scalar
//   parsers ("true"/"false" only for bools; anything else reads as unset).

use std::borrow::Cow;
use std::collections::BTreeMap;
use std::fs;
use std::path::Path;

#[derive(Debug, Default, Clone)]
pub struct Section {
    pub id: String,
    pub keys: BTreeMap<String, String>,
}

/// Parse all sections of an INI file. Returns sections in declaration order.
pub fn read_all(path: &Path) -> Vec<Section> {
    let Ok(text) = fs::read_to_string(path) else {
        return Vec::new();
    };
    let mut out: Vec<Section> = Vec::new();
    let mut cur: Option<Section> = None;
    for line in text.lines() {
        let t = line.trim();
        if t.is_empty() {
            continue;
        }
        if let Some(stripped) = t.strip_prefix('[') {
            if let Some(name) = stripped.strip_suffix(']') {
                if let Some(s) = cur.take() {
                    out.push(s);
                }
                cur = Some(Section {
                    id: name.trim().to_string(),
                    keys: BTreeMap::new(),
                });
                continue;
            }
        }
        if t.starts_with(';') || t.starts_with('#') {
            continue;
        }
        let Some(s) = cur.as_mut() else { continue };
        if let Some(eq) = t.find('=') {
            let key = t[..eq].trim().to_string();
            let val = t[eq + 1..].trim();
            s.keys.insert(key, strip_inline_comment(val).into_owned());
        }
    }
    if let Some(s) = cur {
        out.push(s);
    }
    out
}

fn strip_inline_comment(val: &str) -> Cow<'_, str> {
    let mut prev_was_space = false;
    for (i, c) in val.char_indices() {
        if (c == ';' || c == '#') && prev_was_space {
            let rest = &val[i + c.len_utf8()..];
            if rest.chars().next().is_some_and(char::is_whitespace) {
                return Cow::Owned(val[..i].trim_end().to_string());
            }
        }
        prev_was_space = c.is_whitespace();
    }
    Cow::Borrowed(val.trim_end())
}

/// Read only the named section's keys, or empty if not present.
pub fn read_section(path: &Path, section: &str) -> BTreeMap<String, String> {
    for s in read_all(path) {
        if s.id.eq_ignore_ascii_case(section) {
            return s.keys;
        }
    }
    BTreeMap::new()
}

/// Replace one key inside the named section.
pub fn replace_key(path: &Path, section: &str, key: &str, value: &str) -> std::io::Result<()> {
    let new_line = format!("{key} = {value}");
    let content = fs::read_to_string(path).unwrap_or_default();
    // Inserted lines follow the file's line-ending style (the replace path
    // below preserves it per line); brand-new / empty files get CRLF.
    let eol = detect_eol(&content);

    let header = format!("[{section}]");
    let Some(header_pos) = find_section_header(&content, &header) else {
        let mut out = content;
        if !out.is_empty() && !out.ends_with('\n') {
            out.push_str(eol);
        }
        if !out.is_empty() {
            out.push_str(eol);
        }
        out.push_str(&header);
        out.push_str(eol);
        out.push_str(&new_line);
        out.push_str(eol);
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)?;
        }
        return atomic_write(path, &out);
    };

    let section_start = header_pos + header.len();
    let section_start = section_start
        + content[section_start..]
            .find('\n')
            .map(|n| n + 1)
            .unwrap_or(0);
    let section_end = next_section_start(&content, section_start).unwrap_or(content.len());
    let section_body = &content[section_start..section_end];

    let mut new_body = String::new();
    let mut replaced = false;
    let lines_iter = section_body.split_inclusive('\n');
    for line in lines_iter {
        // line_starts_with_key does its own trim_start.
        if !replaced && line_starts_with_key(line, key) {
            new_body.push_str(&new_line);
            new_body.push_str(if line.ends_with("\r\n") { "\r\n" } else { "\n" });
            replaced = true;
        } else {
            new_body.push_str(line);
        }
    }
    if !replaced {
        let trimmed = new_body.trim_end_matches(['\r', '\n']);
        let tail = &new_body[trimmed.len()..];
        new_body = format!("{trimmed}{eol}{new_line}{eol}{tail}");
    }

    let mut out = String::with_capacity(content.len() + new_line.len());
    out.push_str(&content[..section_start]);
    out.push_str(&new_body);
    out.push_str(&content[section_end..]);

    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    atomic_write(path, &out)
}

/// Replace (or insert) an entire named section. The body arrives CRLF from
/// the renderers (`presets::render_section`, `ServerConfig::render`) and is
/// converted to the file's own line-ending style, so a hand-maintained
/// LF-only file doesn't gain mixed endings on every save.
pub fn replace_section(path: &Path, section_name: &str, section_body: &str) -> std::io::Result<()> {
    let header = format!("[{section_name}]");
    let content = fs::read_to_string(path).unwrap_or_default();
    let eol = detect_eol(&content);
    let body = normalize_eol(section_body.trim_end(), eol);
    let new_section = if body.is_empty() {
        String::new()
    } else {
        format!("{body}{eol}")
    };

    let Some(header_pos) = find_section_header(&content, &header) else {
        let mut out = content;
        if !out.is_empty() {
            out = out.trim_end_matches(['\r', '\n']).to_string();
            out.push_str(eol);
            out.push_str(eol);
        }
        out.push_str(&new_section);
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)?;
        }
        return atomic_write(path, &out);
    };

    let next = next_section_start(&content, header_pos + header.len()).unwrap_or(content.len());
    // Splice from the header's line start so an indented header's leading
    // whitespace is replaced along with it, not left glued to the new body.
    let before = &content[..line_start_of(&content, header_pos)];
    let after = &content[next..];
    let separator = if after.is_empty() { "" } else { eol };

    let mut out = String::with_capacity(before.len() + new_section.len() + after.len() + 4);
    out.push_str(before);
    out.push_str(&new_section);
    out.push_str(separator);
    out.push_str(after);

    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    atomic_write(path, &out)
}

/// Rename a section header in place.
pub fn rename_section(path: &Path, old: &str, new: &str) -> std::io::Result<()> {
    let old_header = format!("[{old}]");
    let new_header = format!("[{new}]");
    let content = fs::read_to_string(path)?;
    let Some(pos) = find_section_header(&content, &old_header) else {
        return Err(std::io::Error::new(
            std::io::ErrorKind::NotFound,
            format!("section [{old}] not found"),
        ));
    };
    // A case-only rename matches itself — only a *different* section counts.
    if find_section_header(&content, &new_header).is_some_and(|p| p != pos) {
        return Err(std::io::Error::new(
            std::io::ErrorKind::AlreadyExists,
            format!("section [{new}] already exists"),
        ));
    }
    let mut out = String::with_capacity(content.len() + new.len());
    out.push_str(&content[..pos]);
    out.push_str(&new_header);
    out.push_str(&content[pos + old_header.len()..]);
    atomic_write(path, &out)
}

/// Remove a section entirely.
pub fn delete_section(path: &Path, section_name: &str) -> std::io::Result<()> {
    let header = format!("[{section_name}]");
    let content = match fs::read_to_string(path) {
        Ok(c) => c,
        Err(_) => return Ok(()),
    };
    let Some(header_pos) = find_section_header(&content, &header) else {
        return Ok(());
    };
    let next = next_section_start(&content, header_pos + header.len()).unwrap_or(content.len());
    let mut out = String::with_capacity(content.len());
    out.push_str(&content[..line_start_of(&content, header_pos)]);
    out.push_str(&content[next..]);
    let tidied = out
        .replace("\r\n\r\n\r\n", "\r\n\r\n")
        .replace("\n\n\n", "\n\n");
    atomic_write(path, &tidied)
}

/// Write via a sibling temp file + rename so an interrupted write can't
/// leave a truncated config behind.
pub fn atomic_write(path: &Path, contents: &str) -> std::io::Result<()> {
    let tmp = path.with_extension(
        path.extension()
            .map(|e| format!("{}.tmp", e.to_string_lossy()))
            .unwrap_or_else(|| "tmp".to_string()),
    );
    fs::write(&tmp, contents)?;
    fs::rename(&tmp, path)
}

fn line_starts_with_key(line: &str, key: &str) -> bool {
    let line = line.trim_start();
    if line.len() < key.len() || !line.is_char_boundary(key.len()) {
        return false;
    }
    if !line[..key.len()].eq_ignore_ascii_case(key) {
        return false;
    }
    let rest = &line[key.len()..];
    let r = rest.trim_start();
    r.starts_with('=')
}

/// Byte offset of the `[` of the line that equals `header` (case-insensitive
/// and whitespace-trimmed, matching the lookup semantics of `read_all` — an
/// indented or trailing-space header is still that section, so the writers
/// must find it where the reader does).
fn find_section_header(content: &str, header: &str) -> Option<usize> {
    let mut offset = 0;
    for line in content.split_inclusive('\n') {
        if line.trim().eq_ignore_ascii_case(header) {
            // Point at the `[` itself so splicers (rename_section) can swap
            // exactly `[name]` regardless of indentation.
            return Some(offset + (line.len() - line.trim_start().len()));
        }
        offset += line.len();
    }
    None
}

/// Byte offset of the first line at/after `from` that opens a new section.
/// Tolerates leading blanks/tabs for the same reason as `find_section_header`;
/// returns the line start (indent included) so section-boundary splices keep
/// the next section's own line intact.
fn next_section_start(content: &str, from: usize) -> Option<usize> {
    let bytes = content.as_bytes();
    let mut i = from;
    while i < bytes.len() {
        if bytes[i] == b'\n' {
            let line_start = i + 1;
            let mut j = line_start;
            while matches!(bytes.get(j), Some(b' ' | b'\t')) {
                j += 1;
            }
            if bytes.get(j) == Some(&b'[') {
                return Some(line_start);
            }
        }
        i += 1;
    }
    None
}

/// Start of the line containing byte offset `pos` — used to widen a section
/// splice back over the header's indentation.
fn line_start_of(content: &str, pos: usize) -> usize {
    content[..pos].rfind('\n').map(|p| p + 1).unwrap_or(0)
}

/// The file's line-ending style: LF only when the content has newlines and no
/// CRLF; everything else — including brand-new / empty files — is CRLF.
fn detect_eol(content: &str) -> &'static str {
    if content.contains('\n') && !content.contains("\r\n") {
        "\n"
    } else {
        "\r\n"
    }
}

/// Rewrite `s` with `eol` as its line ending (input may mix CRLF and LF).
fn normalize_eol(s: &str, eol: &str) -> String {
    let lf = s.replace("\r\n", "\n");
    if eol == "\n" {
        lf
    } else {
        lf.replace('\n', "\r\n")
    }
}

pub fn parse_int(s: &str) -> Option<i32> {
    s.trim().parse().ok()
}

pub fn parse_float(s: &str) -> Option<f64> {
    s.trim().parse().ok()
}

pub fn parse_bool(s: &str) -> Option<bool> {
    match s.trim() {
        "true" => Some(true),
        "false" => Some(false),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    fn ini_file(content: &str) -> (tempfile::TempDir, PathBuf) {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("test.ini");
        fs::write(&path, content).unwrap();
        (dir, path)
    }

    #[test]
    fn read_all_parses_sections_in_order() {
        let (_d, path) = ini_file(
            "; file comment\r\n[Server]\r\nPort = 8080\r\n\r\n[alpha]\r\nmodel = a.gguf\r\n# comment\r\nctx-size = 4096\r\n",
        );
        let sections = read_all(&path);
        assert_eq!(sections.len(), 2);
        assert_eq!(sections[0].id, "Server");
        assert_eq!(sections[0].keys["Port"], "8080");
        assert_eq!(sections[1].id, "alpha");
        assert_eq!(sections[1].keys["ctx-size"], "4096");
        assert!(!sections[1].keys.contains_key("# comment"));
    }

    #[test]
    fn read_section_is_case_insensitive() {
        let (_d, path) = ini_file("[server]\r\nPort = 1234\r\n");
        assert_eq!(read_section(&path, "Server")["Port"], "1234");
    }

    #[test]
    fn strip_inline_comment_cases() {
        assert_eq!(strip_inline_comment("8080 ; note"), "8080");
        assert_eq!(strip_inline_comment("8080 # note"), "8080");
        // No whitespace around the marker → part of the value.
        assert_eq!(strip_inline_comment("a;b"), "a;b");
        assert_eq!(strip_inline_comment("x ;y"), "x ;y");
        assert_eq!(strip_inline_comment("plain  "), "plain");
    }

    #[test]
    fn replace_key_updates_existing_line() {
        let (_d, path) = ini_file("[Server]\r\nPort = 8080\r\nHostname = localhost\r\n");
        replace_key(&path, "Server", "Port", "9090").unwrap();
        let keys = read_section(&path, "Server");
        assert_eq!(keys["Port"], "9090");
        assert_eq!(keys["Hostname"], "localhost");
    }

    #[test]
    fn replace_key_appends_missing_key_to_section() {
        let (_d, path) = ini_file("[Server]\r\nPort = 8080\r\n\r\n[other]\r\nk = v\r\n");
        replace_key(&path, "Server", "ModelsDir", "C:/models").unwrap();
        assert_eq!(read_section(&path, "Server")["ModelsDir"], "C:/models");
        assert_eq!(read_section(&path, "other")["k"], "v");
    }

    #[test]
    fn replace_key_creates_missing_section_and_file() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("sub").join("new.ini");
        replace_key(&path, "Server", "Port", "8080").unwrap();
        assert_eq!(read_section(&path, "Server")["Port"], "8080");
    }

    #[test]
    fn replace_key_matches_section_case_insensitively() {
        // A hand-edited lowercase header must not produce a duplicate section.
        let (_d, path) = ini_file("[server]\r\nPort = 8080\r\n");
        replace_key(&path, "Server", "Port", "9090").unwrap();
        let sections = read_all(&path);
        assert_eq!(sections.len(), 1);
        assert_eq!(sections[0].keys["Port"], "9090");
    }

    #[test]
    fn replace_key_preserves_lf_only_files() {
        let (_d, path) = ini_file("[Server]\nPort = 8080\nHostname = localhost\n");
        replace_key(&path, "Server", "Hostname", "0.0.0.0").unwrap();
        let content = fs::read_to_string(&path).unwrap();
        assert!(content.contains("Hostname = 0.0.0.0\n"));
        assert!(!content.contains("Hostname = 0.0.0.0\r\n"));
        // The insert path too: appending a missing key must not smuggle a CRLF
        // into an LF-only file.
        replace_key(&path, "Server", "ModelsDir", "C:/models").unwrap();
        let content = fs::read_to_string(&path).unwrap();
        assert!(!content.contains('\r'), "LF-only file gained a CR");
        assert_eq!(read_section(&path, "Server")["ModelsDir"], "C:/models");
    }

    #[test]
    fn replace_section_swaps_body_and_keeps_neighbours() {
        let (_d, path) = ini_file("[a]\r\nk = 1\r\n\r\n[b]\r\nk = 2\r\n");
        replace_section(&path, "a", "[a]\r\nk = 9\r\nextra = yes\r\n").unwrap();
        let keys = read_section(&path, "a");
        assert_eq!(keys["k"], "9");
        assert_eq!(keys["extra"], "yes");
        assert_eq!(read_section(&path, "b")["k"], "2");
    }

    #[test]
    fn replace_section_appends_when_missing() {
        let (_d, path) = ini_file("[a]\r\nk = 1\r\n");
        replace_section(&path, "b", "[b]\r\nk = 2\r\n").unwrap();
        assert_eq!(read_section(&path, "b")["k"], "2");
        assert_eq!(read_section(&path, "a")["k"], "1");
    }

    #[test]
    fn replace_section_preserves_lf_only_files() {
        let (_d, path) = ini_file("[a]\nk = 1\n");
        // Both paths — replacing an existing section and appending a new one —
        // must convert the CRLF-rendered body to the file's LF-only style.
        replace_section(&path, "a", "[a]\r\nk = 9\r\n").unwrap();
        replace_section(&path, "b", "[b]\r\nk = 2\r\n").unwrap();
        let content = fs::read_to_string(&path).unwrap();
        assert!(
            !content.contains('\r'),
            "LF-only file gained a CR: {content:?}"
        );
        assert_eq!(read_section(&path, "a")["k"], "9");
        assert_eq!(read_section(&path, "b")["k"], "2");
    }

    #[test]
    fn rename_section_basic_and_conflict() {
        let (_d, path) = ini_file("[a]\r\nk = 1\r\n\r\n[b]\r\nk = 2\r\n");
        rename_section(&path, "a", "c").unwrap();
        assert_eq!(read_section(&path, "c")["k"], "1");
        assert!(rename_section(&path, "c", "b").is_err());
        assert!(rename_section(&path, "missing", "x").is_err());
    }

    #[test]
    fn rename_section_allows_case_only_rename() {
        let (_d, path) = ini_file("[qwen]\r\nk = 1\r\n");
        rename_section(&path, "qwen", "Qwen").unwrap();
        let sections = read_all(&path);
        assert_eq!(sections.len(), 1);
        assert_eq!(sections[0].id, "Qwen");
    }

    #[test]
    fn delete_section_removes_only_target() {
        let (_d, path) = ini_file("[a]\r\nk = 1\r\n\r\n[b]\r\nk = 2\r\n");
        delete_section(&path, "a").unwrap();
        let sections = read_all(&path);
        assert_eq!(sections.len(), 1);
        assert_eq!(sections[0].id, "b");
        // Deleting a missing section (or from a missing file) is a no-op.
        delete_section(&path, "nope").unwrap();
        delete_section(Path::new("does/not/exist.ini"), "a").unwrap();
    }

    // The reader trims each line before the `[` check, so a hand-indented
    // header IS a section — the writers' boundary scan must honor the same
    // rule or a neighbouring rewrite silently swallows it.
    #[test]
    fn writers_respect_indented_section_headers() {
        let (_d, path) = ini_file("[a]\r\nk = 1\r\n  [b]\r\nk = 2\r\n");

        // Rewriting [a] must stop at the indented [b], not absorb it.
        replace_section(&path, "a", "[a]\r\nk = 9\r\n").unwrap();
        assert_eq!(read_section(&path, "a")["k"], "9");
        assert_eq!(read_section(&path, "b")["k"], "2");

        // A key appended to [a] must land in [a], not fall through into [b].
        replace_key(&path, "a", "extra", "3").unwrap();
        assert_eq!(read_section(&path, "a")["extra"], "3");
        assert!(!read_section(&path, "b").contains_key("extra"));

        // The indented header itself is addressable...
        replace_section(&path, "b", "[b]\r\nk = 7\r\n").unwrap();
        assert_eq!(read_section(&path, "b")["k"], "7");

        // ...and deleting [a] leaves [b] intact.
        delete_section(&path, "a").unwrap();
        let sections = read_all(&path);
        assert_eq!(sections.len(), 1);
        assert_eq!(sections[0].id, "b");
    }
}
