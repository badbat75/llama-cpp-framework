//! Minimal INI parser / writer that preserves comments and section order.
//!
//! The behavioral contract callers rely on:
//! - Section lookup is CASE-INSENSITIVE and WHITESPACE-TOLERANT everywhere
//!   (read_all trims each line; the writers match via find_section_header /
//!   next_section_start with the same tolerance): a hand-edited `[server]` or
//!   `  [server]` header never spawns a duplicate and is never swallowed by a
//!   neighbouring section's rewrite.
//! - KEY lookup is asymmetric by design: reads are exact-case (`BTreeMap`
//!   lookups like `keys.get("Port")`), while replace_key matches an existing
//!   key case-insensitively and rewrites the line in canonical case.
//! - Values are TRIMMED on parse, and everything from the FIRST `;` or `#` on
//!   is stripped as an inline comment, spaced or not ("a;b" reads as "a").
//!   This mirrors llama-server's own model profile parser (common/preset.cpp PEG:
//!   `eol-start ::= ws ([;#] / newline / EOF)`), which models.ini must obey:
//!   `;`/`#` simply cannot occur inside a value, and a writer that emits one
//!   produces a value llama-server would truncate anyway. Writers must also
//!   emit trimmed values or break round-trips.
//! - Writers preserve the file's existing line endings (per line on the
//!   replace_key and set_key replace paths, detected once everywhere else; a
//!   rewritten line that has no ending of its own, the file's last line, stays
//!   unterminated on set_key); section bodies arrive CRLF from the renderers
//!   and are converted to the file's style; brand-new content defaults to CRLF.
//! - `set_key` returns the text byte for byte when the key already reads as
//!   the value it is asked to write (it sits once in the section, its value
//!   read as `parse_sections` reads it), so a caller that writes only when the
//!   text changes writes nothing; `replace_key` always rewrites.
//! - `atomic_write` (sibling temp file + FSYNC + rename) is the canonical write
//!   path: every config writer in the crate funnels through it. The sync is not
//!   belt-and-braces, it is the difference between atomicity and DURABILITY, and
//!   this file learned the difference the hard way (see the function).
//! - `parse_int` / `parse_float` / `parse_bool` are the shared lenient scalar
//!   parsers ("true"/"false" only for bools; anything else reads as unset).
//!   Lenient is right for READING a file and wrong for SAVING a form: a form's
//!   integer field goes through `int_problem` first, so a mistyped number is
//!   refused instead of being written as an absent key.

use crate::strings;
use std::collections::BTreeMap;
use std::fs;
use std::ops::RangeInclusive;
use std::path::Path;

// ── Types & readers ──────────────────────────────────────────────────────

#[derive(Debug, Default, Clone)]
pub struct Section {
    pub id: String,
    pub keys: BTreeMap<String, String>,
    /// The section's whole-line comments, trimmed, marker included, in file
    /// order. They carry data llama-server must never see as a key (it refuses
    /// a model profile holding an unknown one): `model_profiles::project_url_of` reads one.
    pub comments: Vec<String>,
}

/// Parse all sections of an INI file. Returns sections in declaration order.
pub fn read_all(path: &Path) -> Vec<Section> {
    let Ok(text) = fs::read_to_string(path) else {
        return Vec::new();
    };
    parse_sections(&text)
}

/// `read_all` over text already in memory.
pub fn parse_sections(text: &str) -> Vec<Section> {
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
                    ..Default::default()
                });
                continue;
            }
        }
        if t.starts_with(';') || t.starts_with('#') {
            if let Some(s) = cur.as_mut() {
                s.comments.push(t.to_string());
            }
            continue;
        }
        let Some(s) = cur.as_mut() else { continue };
        if let Some((key, val)) = key_of_line(t) {
            s.keys.insert(key, val);
        }
    }
    if let Some(s) = cur {
        out.push(s);
    }
    out
}

/// The named section's key lines, in the order the file has them, each read as
/// `parse_sections` reads it; nothing when the file has no such section.
/// `parse_sections` puts a section's keys in a map, sorted: a writer that
/// keeps the order the file has (a section moved into a render) uses this. The
/// section is found as the writers find one, so `[*]` is named by `*`.
pub fn section_keys(text: &str, section: &str) -> Vec<(String, String)> {
    let mut out = Vec::new();
    let mut inside = false;
    for line in text.lines() {
        let t = line.trim();
        if let Some(name) = t.strip_prefix('[').and_then(|rest| rest.strip_suffix(']')) {
            inside = name.trim().eq_ignore_ascii_case(section);
            continue;
        }
        if inside {
            if let Some(pair) = key_of_line(t) {
                out.push(pair);
            }
        }
    }
    out
}

// Everything from the first `;` or `#` is comment, spaced or not: the exact
// rule of llama-server's model profile PEG (see the header contract). Being MORE
// lenient here (e.g. requiring whitespace around the marker) would make the
// GUI show a value llama-server truncates.
fn strip_inline_comment(val: &str) -> &str {
    let end = val.find([';', '#']).unwrap_or(val.len());
    val[..end].trim_end()
}

/// One TRIMMED line as its key and value, exactly as `parse_sections` reads
/// one: no comment, no section header, a line with an `=`, the key and the
/// value trimmed, everything from the first `;`/`#` of the value dropped.
/// `None` for anything else, so every reader of key lines answers the same.
pub(crate) fn key_of_line(line: &str) -> Option<(String, String)> {
    let t = line.trim();
    if t.starts_with(';') || t.starts_with('#') {
        return None;
    }
    let eq = t.find('=')?;
    Some((
        t[..eq].trim().to_string(),
        strip_inline_comment(t[eq + 1..].trim()).to_string(),
    ))
}

/// The value of a key line exactly as `parse_sections` reads it, so a writer
/// that asks "does this line already say that?" answers the way the reader
/// does. `None` for a line with no `=`.
fn value_of_line(line: &str) -> Option<&str> {
    let t = line.trim();
    let eq = t.find('=')?;
    Some(strip_inline_comment(t[eq + 1..].trim()))
}

/// Save-boundary guard for the comment rule above: the format has NO escaping,
/// so a value containing `;` or `#` (legal in Windows paths, e.g.
/// `C:\Models #1`) would write fine and silently reload truncated, by this
/// parser and by llama-server's own model profile reader alike. Callers that persist
/// path-valued fields (`server_cfg::save`, `model_profiles::save`) reject such values
/// here instead, so the user gets an error naming the field rather than a
/// config that quietly points somewhere else.
pub fn reject_comment_markers(field: &str, value: &str) -> std::io::Result<()> {
    crate::checks::comment_markers(field, value)
        .map_err(|r| std::io::Error::new(std::io::ErrorKind::InvalidInput, r.text()))
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

// ── Writers (atomic, section-preserving) ─────────────────────────────────
//
// Each writer is a text transform in `text` (what a caller that reads and
// writes through someone else, the manager, uses) and a file wrapper here.

/// Replace one key inside the named section.
pub fn replace_key(path: &Path, section: &str, key: &str, value: &str) -> std::io::Result<()> {
    let content = fs::read_to_string(path).unwrap_or_default();
    let out = text::replace_key(&content, section, key, value);
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    atomic_write(path, &out)
}

/// Replace (or insert) an entire named section: `text::replace_section`.
pub fn replace_section(path: &Path, section_name: &str, section_body: &str) -> std::io::Result<()> {
    let content = fs::read_to_string(path).unwrap_or_default();
    let out = text::replace_section(&content, section_name, section_body);
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    atomic_write(path, &out)
}

/// Rename a section header in place.
pub fn rename_section(path: &Path, old: &str, new: &str) -> std::io::Result<()> {
    let content = fs::read_to_string(path)?;
    atomic_write(path, &text::rename_section(&content, old, new)?)
}

/// The named section's raw text (`text::section_text`), or `None` when the
/// file or the section is missing.
pub fn section_text(path: &Path, section_name: &str) -> Option<String> {
    let content = fs::read_to_string(path).ok()?;
    text::section_text(&content, section_name)
}

/// The value of the first `url` comment among a section's comments, or empty.
/// The configurator's reader, so the web and the GUI read one URL the same way
/// (`crates/bff/tests/model_profile_read.rs`).
pub fn project_url_of(comments: &[String]) -> String {
    comment_value(comments, "url")
}

/// The value of the first `<word> = value` comment among a section's comments,
/// or empty. Read leniently (`;` or `#`, any spacing around `=`), so a
/// hand-written line counts; `word` must be the whole word, so `; urls ...` is
/// prose. What a configurator stores in a comment of `server.ini` or
/// `models.ini` (`crates/app/src/model_profiles.rs` writes them).
pub fn comment_value(comments: &[String], word: &str) -> String {
    comments
        .iter()
        .find_map(|c| {
            let rest = c
                .strip_prefix([';', '#'])?
                .trim_start()
                .strip_prefix(word)?;
            Some(rest.trim_start().strip_prefix('=')?.trim().to_string())
        })
        .unwrap_or_default()
}

/// Remove a section entirely; a missing file or section is left alone.
pub fn delete_section(path: &Path, section_name: &str) -> std::io::Result<()> {
    let Ok(content) = fs::read_to_string(path) else {
        return Ok(());
    };
    match text::delete_section(&content, section_name) {
        Some(out) => atomic_write(path, &out),
        None => Ok(()),
    }
}

/// The writers as transforms of a file's text: each takes the content and
/// returns the content to write.
pub mod text {
    use super::{detect_eol, find_section_header, line_start_of, next_section_start};
    use super::{line_spells_key, line_starts_with_key, normalize_eol, strings, value_of_line};

    /// The line ending a rewritten key line gets: the one the line already has,
    /// so a CRLF file keeps CRLF per line; an unterminated final line has none
    /// to preserve, so it takes the file's detected style, never a hardcoded
    /// `\n` that would mix endings into a CRLF file.
    fn line_eol(line: &str, eol: &'static str) -> &'static str {
        if line.ends_with("\r\n") {
            "\r\n"
        } else if line.ends_with('\n') {
            "\n"
        } else {
            eol
        }
    }

    /// Where a section's body starts (the first byte after its header's line
    /// ending) and whether that header has no line ending of its own because it
    /// is the file's last line: a writer splicing a body in front of it adds one
    /// there, so an appended key starts on its own line instead of glued to the
    /// `]`.
    fn body_start_of(content: &str, header_end: usize) -> (usize, bool) {
        match content[header_end..].find('\n') {
            Some(n) => (header_end + n + 1, false),
            None => (content.len(), true),
        }
    }

    /// `content` with one key replaced inside the named section (inserted,
    /// with the section when that is missing too).
    pub fn replace_key(content: &str, section: &str, key: &str, value: &str) -> String {
        let new_line = format!("{key} = {value}");
        // Inserted lines follow the file's line-ending style (the replace path
        // below preserves it per line); brand-new / empty files get CRLF.
        let eol = detect_eol(content);

        let header = format!("[{section}]");
        let Some((_, header_end)) = find_section_header(content, section) else {
            let mut out = content.to_string();
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
            return out;
        };

        // Body starts after the header's newline; a header that is the file's
        // last line (no terminator) gets one added at the splice below so the
        // appended key starts on its own line instead of glued to the `]`.
        let (section_start, header_needs_eol) = body_start_of(content, header_end);
        // Scan from header_end (like replace_section / delete_section), NOT from
        // section_start: next_section_start only sees a header on a line AFTER a
        // '\n' at/beyond `from`, so scanning from the body start skips a
        // neighbouring header sitting exactly there (empty body): the key would
        // splice into THAT section instead.
        let section_end = next_section_start(content, header_end).unwrap_or(content.len());
        let section_body = &content[section_start..section_end];

        let mut new_body = String::new();
        let mut replaced = false;
        let lines_iter = section_body.split_inclusive('\n');
        for line in lines_iter {
            // line_starts_with_key does its own trim_start.
            if !replaced && line_starts_with_key(line, key) {
                new_body.push_str(&new_line);
                new_body.push_str(line_eol(line, eol));
                replaced = true;
            } else {
                new_body.push_str(line);
            }
        }
        if !replaced {
            let trimmed = new_body.trim_end_matches(['\r', '\n']);
            let tail = &new_body[trimmed.len()..];
            // `tail` still holds the last real line's terminator (plus any blank
            // separator lines before the next section), so don't add another `eol`
            // in front of it: that duplicated the terminator, growing one blank
            // line per appended key.
            new_body = if trimmed.is_empty() {
                format!("{new_line}{eol}{tail}")
            } else {
                format!("{trimmed}{eol}{new_line}{tail}")
            };
        }

        let mut out = String::with_capacity(content.len() + new_line.len());
        out.push_str(&content[..section_start]);
        if header_needs_eol {
            out.push_str(eol);
        }
        out.push_str(&new_body);
        out.push_str(&content[section_end..]);
        out
    }

    /// `content` with an entire named section replaced (or appended). The
    /// body arrives CRLF from the renderers (`model_profiles::render_section`,
    /// `ServerConfig::render`) and is converted to the file's own line-ending
    /// style, so a hand-maintained LF-only file doesn't gain mixed endings on
    /// every save.
    pub fn replace_section(content: &str, section_name: &str, section_body: &str) -> String {
        let eol = detect_eol(content);
        let body = normalize_eol(section_body.trim_end(), eol);
        let new_section = if body.is_empty() {
            String::new()
        } else {
            format!("{body}{eol}")
        };

        let Some((header_pos, header_end)) = find_section_header(content, section_name) else {
            let mut out = content.to_string();
            if !out.is_empty() {
                out = out.trim_end_matches(['\r', '\n']).to_string();
                out.push_str(eol);
                out.push_str(eol);
            }
            out.push_str(&new_section);
            return out;
        };

        let next = next_section_start(content, header_end).unwrap_or(content.len());
        // Splice from the header's line start so an indented header's leading
        // whitespace is replaced along with it, not left glued to the new body.
        let before = &content[..line_start_of(content, header_pos)];
        let after = &content[next..];
        let separator = if after.is_empty() { "" } else { eol };

        let mut out = String::with_capacity(before.len() + new_section.len() + after.len() + 4);
        out.push_str(before);
        out.push_str(&new_section);
        out.push_str(separator);
        out.push_str(after);
        out
    }

    /// `content` with a section header renamed in place: `NotFound` when
    /// there is no `old`, `AlreadyExists` when another section is `new`.
    pub fn rename_section(content: &str, old: &str, new: &str) -> std::io::Result<String> {
        let new_header = format!("[{new}]");
        let Some((pos, end)) = find_section_header(content, old) else {
            return Err(std::io::Error::new(
                std::io::ErrorKind::NotFound,
                strings::fill("section-not-found", &[&old]),
            ));
        };
        // A case-only rename matches itself; only a *different* section counts.
        if find_section_header(content, new).is_some_and(|(p, _)| p != pos) {
            return Err(std::io::Error::new(
                std::io::ErrorKind::AlreadyExists,
                strings::fill("section-exists", &[&new]),
            ));
        }
        let mut out = String::with_capacity(content.len() + new.len());
        out.push_str(&content[..pos]);
        out.push_str(&new_header);
        out.push_str(&content[end..]);
        Ok(out)
    }

    /// The named section's raw text, header line included, exactly as it sits
    /// in `content` (comments, key order, hand-edits), up to the next section's
    /// header; or `None` when the section is missing. The unit `delete_section`
    /// removes, so the pair moves a section between files without re-rendering it.
    pub fn section_text(content: &str, section_name: &str) -> Option<String> {
        let (header_pos, header_end) = find_section_header(content, section_name)?;
        let next = next_section_start(content, header_end).unwrap_or(content.len());
        Some(content[line_start_of(content, header_pos)..next].to_string())
    }

    /// `content` without the named section, or `None` when it has none.
    pub fn delete_section(content: &str, section_name: &str) -> Option<String> {
        let (header_pos, header_end) = find_section_header(content, section_name)?;
        let next = next_section_start(content, header_end).unwrap_or(content.len());
        let mut out = String::with_capacity(content.len());
        out.push_str(&content[..line_start_of(content, header_pos)]);
        out.push_str(&content[next..]);
        Some(
            out.replace("\r\n\r\n\r\n", "\r\n\r\n")
                .replace("\n\n\n", "\n\n"),
        )
    }

    /// `content` with every line of `key` inside the named section dropped, its
    /// line ending with it, every other byte kept. A key line counts the way
    /// `replace_key` counts one (any letter case, never a comment line, never a
    /// longer key such as `ctx-size-draft`), and only inside the named section:
    /// the same key in another section stays. An absent section or an absent
    /// key leaves the text equal.
    pub fn remove_key(content: &str, section: &str, key: &str) -> String {
        let Some((_, header_end)) = find_section_header(content, section) else {
            return content.to_string();
        };
        // Scan from the header's end (as replace_key does), so the section's
        // span ends at the neighbour header sitting on the very next line.
        let body_end = next_section_start(content, header_end).unwrap_or(content.len());
        let mut out = String::with_capacity(content.len());
        out.push_str(&content[..header_end]);
        for line in content[header_end..body_end].split_inclusive('\n') {
            if !line_starts_with_key(line, key) {
                out.push_str(line);
            }
        }
        out.push_str(&content[body_end..]);
        out
    }

    /// `content` with `key` set to `value` inside the named section. The named
    /// section must exist: an absent one leaves the text equal (the caller
    /// answers a model profile that is not there, it does not create one).
    ///
    /// A key the section holds exactly once, spelled in the exact case given,
    /// whose value read the way `parse_sections` reads it (the text after the
    /// first `=`, trimmed, with everything from the first `;`/`#` dropped as an
    /// inline comment) equals `value`, already has that value: the text comes
    /// back byte for byte, so the line keeps its indentation, its spacing and
    /// its inline comment, and an unterminated final line stays unterminated. A
    /// line that spells the key in ANOTHER case is rewritten even when its
    /// value is equal: llama-server knows a key only in its own case and
    /// refuses a model profile that spells it otherwise (`common/preset.cpp`), so
    /// leaving such a line alone would answer a save with success over a model profile
    /// that does not start. When the value differs the line is rewritten whole
    /// as `key = value`, the value as given, and keeps its own line ending: an
    /// unterminated final line stays unterminated. A key occurring more than
    /// once is always collapsed onto the first occurrence, even when every copy
    /// already holds the value and every copy spells the key as given, and a
    /// key the section does not have is appended where `replace_key`
    /// appends it.
    pub fn set_key(content: &str, section: &str, key: &str, value: &str) -> String {
        let Some((_, header_end)) = find_section_header(content, section) else {
            return content.to_string();
        };
        let eol = detect_eol(content);
        let new_line = format!("{key} = {value}");
        // The body starts after the header's own line ending and ends at the
        // next header's line, the way replace_key scopes it: an appended key
        // lands in this section, never past the next one.
        let (body_start, header_needs_eol) = body_start_of(content, header_end);
        let body_end = next_section_start(content, header_end).unwrap_or(content.len());

        // The equal-value check: one line of the key, spelling it in the exact
        // case given and read as the reader reads it, is nothing to write, and
        // the caller writes no file at all. A line in another case is not: the
        // rewrite below puts the spelling the engine knows in its place.
        let mut lines = content[body_start..body_end]
            .split_inclusive('\n')
            .filter(|line| line_starts_with_key(line, key));
        if let Some(line) = lines.next() {
            if lines.next().is_none()
                && line_spells_key(line, key)
                && value_of_line(line) == Some(value)
            {
                return content.to_string();
            }
        }

        let mut new_body = String::new();
        let mut set = false;
        for line in content[body_start..body_end].split_inclusive('\n') {
            if !line_starts_with_key(line, key) {
                new_body.push_str(line);
            } else if !set {
                new_body.push_str(&new_line);
                // The line's OWN ending, and nothing when it has none: the
                // rewrite of an unterminated final line leaves it unterminated.
                new_body.push_str(&line[line.trim_end_matches(['\r', '\n']).len()..]);
                set = true;
            }
        }
        if !set {
            // No such key here: append it after the section's last real line,
            // `tail` keeping the body's trailing blank lines, so the separator
            // in front of the next header is neither lost nor doubled.
            let trimmed = new_body.trim_end_matches(['\r', '\n']);
            let tail = &new_body[trimmed.len()..];
            new_body = if trimmed.is_empty() {
                format!("{new_line}{eol}{tail}")
            } else {
                format!("{trimmed}{eol}{new_line}{tail}")
            };
        }

        let mut out = String::with_capacity(content.len() + new_line.len());
        out.push_str(&content[..body_start]);
        if header_needs_eol {
            out.push_str(eol);
        }
        out.push_str(&new_body);
        out.push_str(&content[body_end..]);
        out
    }
}

/// Write via a sibling temp file + rename so an interrupted write can't
/// leave a truncated config behind.
pub fn atomic_write(path: &Path, contents: &str) -> std::io::Result<()> {
    let tmp = path.with_extension(
        path.extension()
            .map(|e| format!("{}.tmp", e.to_string_lossy()))
            .unwrap_or_else(|| "tmp".to_string()),
    );
    // Write, then FLUSH TO THE DISK, then rename. The sync is the whole point of
    // this being three steps instead of one: `rename` is a metadata operation,
    // and the filesystem is free to commit it while the temp file's data blocks
    // are still in the page cache. Lose power in that window and the rename
    // survives while the bytes do not, which leaves the config file with the
    // right SIZE and nothing but zeros inside.
    //
    // That is not hypothetical. On 2026-09-02 a 0x116 bugcheck (the AMD driver
    // fault this machine is prone to) landed while a sweep was restoring one
    // model profile key, and models.ini came back as 27,669 bytes of NUL: every model
    // model profile on the machine, gone, while server.ini and settings.ini (not being
    // written at that moment) were untouched. `fs::write` + `rename` had made
    // the update atomic, which was never the property at risk.
    {
        let mut file = fs::File::create(&tmp)?;
        std::io::Write::write_all(&mut file, contents.as_bytes())?;
        file.sync_all()?;
    }
    fs::rename(&tmp, path)
}

// ── Internal: section-header scanning & EOL ──────────────────────────────

/// True when `line` is a key line naming `key`, in any letter case (the `=`
/// right after the key, blanks aside, never a comment line).
pub(crate) fn line_starts_with_key(line: &str, key: &str) -> bool {
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

/// True when `line` spells `key` in the exact case given (the leading blanks
/// and tabs of an indented line aside), where `line_starts_with_key` accepts
/// any letter case. Every reader of the line takes it as the key; the writer
/// that leaves an equal value alone must not, because the engine refuses a
/// model profile holding a key in a case of its own.
fn line_spells_key(line: &str, key: &str) -> bool {
    line.trim_start().starts_with(key)
}

/// Byte span (`[` ..= `]`, exclusive end) of the header line whose NAME equals
/// `section`, parsed exactly the way `read_all` parses a header (trim the
/// line, strip `[`/`]`, trim the name, compare case-insensitively), so the
/// writers find a section wherever the reader does: an indented, trailing-space
/// or internally-spaced `[ name ]` header is still that section. Returning the
/// real span (not `"[name]".len()`) keeps splicers correct when the on-disk
/// header is longer than the canonical one.
fn find_section_header(content: &str, section: &str) -> Option<(usize, usize)> {
    let mut offset = 0;
    for line in content.split_inclusive('\n') {
        let t = line.trim();
        if let Some(name) = t.strip_prefix('[').and_then(|s| s.strip_suffix(']')) {
            if name.trim().eq_ignore_ascii_case(section) {
                let start = offset + (line.len() - line.trim_start().len());
                return Some((start, start + t.len()));
            }
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

/// Start of the line containing byte offset `pos`, used to widen a section
/// splice back over the header's indentation.
fn line_start_of(content: &str, pos: usize) -> usize {
    content[..pos].rfind('\n').map(|p| p + 1).unwrap_or(0)
}

/// The file's line-ending style: LF only when the content has newlines and no
/// CRLF; everything else, including brand-new / empty files, is CRLF. Also
/// what the render of a generated file (`assign::render`) writes its header in.
pub fn detect_eol(content: &str) -> &'static str {
    if content.contains('\n') && !content.contains("\r\n") {
        "\n"
    } else {
        "\r\n"
    }
}

/// Rewrite `s` with `eol` as its line ending (input may mix CRLF and LF).
pub fn normalize_eol(s: &str, eol: &str) -> String {
    let lf = s.replace("\r\n", "\n");
    if eol == "\n" {
        lf
    } else {
        lf.replace('\n', "\r\n")
    }
}

// ── Value parsers ────────────────────────────────────────────────────────

pub fn parse_int(s: &str) -> Option<i32> {
    s.trim().parse().ok()
}

/// Every value an `i32` field can hold (`--cache-ram`'s `-1`, `--top-k`'s `0`).
pub const INT_ANY: RangeInclusive<i32> = i32::MIN..=i32::MAX;
/// Counts and sizes where `0` and below mean nothing (`--ctx-size`, `--parallel`).
pub const INT_POSITIVE: RangeInclusive<i32> = 1..=i32::MAX;
/// Lengths where `0` is a real value (llama-bench's `-n 0`, a depth of 0).
pub const INT_NON_NEGATIVE: RangeInclusive<i32> = 0..=i32::MAX;

/// `parse_int` restricted to `range`: what an integer FIELD of a form stands
/// for, or `None` when its text is blank, not a number, past `i32`, or outside
/// the range. Pairs with `int_problem`, which words the same verdict.
pub fn parse_int_in(s: &str, range: &RangeInclusive<i32>) -> Option<i32> {
    parse_int(s).filter(|v| range.contains(v))
}

/// Why `s` is not an integer in `range`, worded for the status line, or `None`
/// when it is one. Every form that writes a config or starts a run asks this of
/// each integer field it reads BEFORE converting, because the conversions are
/// lenient on purpose (an unparseable field reads as unset, or as a default) and
/// "unset" is a real instruction: a model profile saved with an unparseable context
/// has no `ctx-size` at all, and llama-server then loads the model's TRAINED
/// context. Met on 2026-09-17: a context edit saved from the Models tab reached
/// disk with no `ctx-size`, and the card paged at that context. The field is
/// digits-only, so the text cannot have held a separator; a number typed next to
/// the one already there (`196608245760`, past `i32`) is one way to that save.
pub fn int_problem(label: &str, s: &str, range: &RangeInclusive<i32>) -> Option<String> {
    if parse_int_in(s, range).is_some() {
        return None;
    }
    let wanted = strings::fill("whole-number-range", &[&range.start(), &range.end()]);
    Some(match s.trim() {
        "" => strings::fill("number-empty", &[&label, &wanted]),
        t => strings::fill("number-not", &[&label, &t, &wanted]),
    })
}

pub fn parse_float(s: &str) -> Option<f64> {
    // Accept comma as the decimal separator (e.g. "0,5" on a comma-decimal
    // keyboard layout like Italian/German/French) by normalizing to the
    // period that Rust's f64 parser (and llama.cpp's CLI) expect. Domain
    // values are small sampling knobs (0.0–2.0), so a thousands-separator
    // reading never applies; a plain replace is safe.
    s.trim().replace(',', ".").parse().ok()
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
        // llama-server's PEG cuts at the FIRST marker even unspaced: we must
        // read exactly what it would read, not keep a longer value it won't.
        assert_eq!(strip_inline_comment("a;b"), "a");
        assert_eq!(strip_inline_comment("x ;y"), "x");
        assert_eq!(strip_inline_comment("q8#0"), "q8");
        assert_eq!(strip_inline_comment("plain  "), "plain");
    }

    // The refusal names the field and the text, and it is exactly the complement
    // of `parse_int_in`: overflow, blank, garbage, a fraction and an out-of-range
    // value are refused; a padded number inside the range is not.
    #[test]
    fn int_problem_refuses_exactly_what_parse_int_in_rejects() {
        let cases = [
            ("196608245760", &INT_POSITIVE, false),
            ("", &INT_POSITIVE, false),
            ("  ", &INT_ANY, false),
            ("abc", &INT_ANY, false),
            ("1.5", &INT_ANY, false),
            ("0", &INT_POSITIVE, false),
            ("-1", &INT_NON_NEGATIVE, false),
            ("0", &INT_NON_NEGATIVE, true),
            ("-1", &INT_ANY, true),
            (" 245760 ", &INT_POSITIVE, true),
        ];
        for (text, range, ok) in cases {
            assert_eq!(parse_int_in(text, range).is_some(), ok, "{text:?}");
            assert_eq!(
                int_problem("ctx-size", text, range).is_none(),
                ok,
                "{text:?}"
            );
        }
        assert_eq!(
            int_problem("ctx-size", "196608245760", &INT_POSITIVE).unwrap(),
            "ctx-size = `196608245760` is not a whole number from 1 to 2147483647"
        );
        assert_eq!(
            int_problem("Port", " ", &(1..=65535)).unwrap(),
            "Port is empty, expected a whole number from 1 to 65535"
        );
    }

    #[test]
    fn parse_float_accepts_dot_or_comma_separator() {
        // Period: the canonical form llama-server's CLI expects.
        assert_eq!(parse_float("0.5"), Some(0.5));
        assert_eq!(parse_float("1.25"), Some(1.25));
        // Comma: the natural separator on a comma-decimal keyboard
        // (Italian/German/French). Must normalize to the same value so the
        // sampling knobs are editable on those layouts.
        assert_eq!(parse_float("0,5"), Some(0.5));
        assert_eq!(parse_float("1,25"), Some(1.25));
        // Surrounding whitespace is tolerated, blank/garbage yields None.
        assert_eq!(parse_float("  0,7  "), Some(0.7));
        assert_eq!(parse_float(""), None);
        assert_eq!(parse_float("abc"), None);
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
        // Exact content: the new line slots in after the last real line, and
        // the single blank separator line stays single (the append path once
        // duplicated the terminator, growing a blank line per appended key).
        assert_eq!(
            fs::read_to_string(&path).unwrap(),
            "[Server]\r\nPort = 8080\r\nModelsDir = C:/models\r\n\r\n[other]\r\nk = v\r\n"
        );
    }

    // next_section_start only recognizes a header AFTER a '\n' at/beyond
    // `from`, so scanning from the body start missed a neighbouring header
    // sitting exactly there: appending to an empty-bodied section spliced the
    // key into the NEXT section instead (and read_section still saw nothing).
    #[test]
    fn replace_key_respects_empty_section_body() {
        let (_d, path) = ini_file("[Server]\r\n[other]\r\nk = 2\r\n");
        replace_key(&path, "Server", "ModelsDir", "C:/models").unwrap();
        assert_eq!(
            fs::read_to_string(&path).unwrap(),
            "[Server]\r\nModelsDir = C:/models\r\n[other]\r\nk = 2\r\n"
        );
    }

    // A header that is the file's last line (no terminator) used to have the
    // key glued straight onto the `]`, destroying the header line.
    #[test]
    fn replace_key_handles_header_without_trailing_newline() {
        let (_d, path) = ini_file("[Server]");
        replace_key(&path, "Server", "Port", "8080").unwrap();
        assert_eq!(
            fs::read_to_string(&path).unwrap(),
            "[Server]\r\nPort = 8080\r\n"
        );
    }

    // Replacing an UNTERMINATED final line has no ending to preserve: it
    // must fall back to the file's detected EOL, not hardcode `\n` and mix
    // endings into a CRLF file.
    #[test]
    fn replace_key_keeps_crlf_when_replacing_unterminated_last_line() {
        let (_d, path) = ini_file("[Server]\r\nModelsDir = C:/old");
        replace_key(&path, "Server", "ModelsDir", "C:/new").unwrap();
        assert_eq!(
            fs::read_to_string(&path).unwrap(),
            "[Server]\r\nModelsDir = C:/new\r\n"
        );
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
        // Both paths (replacing an existing section and appending a new one)
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

    // The move primitive behind a model profile's on/off switch: the text must come out
    // exactly as written (a hand comment and an unknown key included), stop at
    // the next header, and splice into another file as that same section.
    #[test]
    fn section_text_moves_a_section_verbatim() {
        let (_d, path) =
            ini_file("[a]\r\n; mine\r\nk = 1\r\nodd-key = x\r\n\r\n  [B]\r\nk = 2\r\n");
        let a = section_text(&path, "A").expect("case-insensitive lookup");
        assert_eq!(a, "[a]\r\n; mine\r\nk = 1\r\nodd-key = x\r\n\r\n");
        // An indented header comes out whole, indentation included.
        assert_eq!(section_text(&path, "b").unwrap(), "  [B]\r\nk = 2\r\n");
        assert!(section_text(&path, "nope").is_none());
        assert!(section_text(Path::new("does/not/exist.ini"), "a").is_none());

        let (_d2, other) = ini_file("[c]\r\nk = 3\r\n");
        replace_section(&other, "a", &a).unwrap();
        let moved = fs::read_to_string(&other).unwrap();
        assert!(
            moved.contains("[a]\r\n; mine\r\nk = 1\r\nodd-key = x\r\n"),
            "{moved}"
        );
        assert_eq!(read_section(&other, "a")["odd-key"], "x");
        assert_eq!(read_section(&other, "c")["k"], "3");
    }

    // The reader trims each line before the `[` check, so a hand-indented
    // header IS a section: the writers' boundary scan must honor the same
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

    #[test]
    fn reject_comment_markers_matches_the_strip_rule() {
        assert!(reject_comment_markers("f", r"C:\Models\plain").is_ok());
        for hostile in [r"C:\Models #1", r"C:\a;b", "x#y"] {
            let err = reject_comment_markers("ModelsDir", hostile).expect_err(hostile);
            assert!(
                err.to_string().contains("ModelsDir"),
                "error names the field"
            );
            // The guard exists precisely because the reader would truncate it.
            assert_ne!(strip_inline_comment(hostile), hostile);
        }
    }

    // read_all trims the NAME inside the brackets, so a hand-edited `[ a ]`
    // lists as section `a`. The writers must find it there too: matching the
    // whole line against `[a]` missed it, so replace_section appended a fresh
    // duplicate `[a]` and every later read returned the stale first copy
    // ("edits never stick").
    #[test]
    fn writers_respect_internal_header_whitespace() {
        let (_d, path) = ini_file("[ a ]\r\nk = 1\r\n[b]\r\nk = 2\r\n");
        assert_eq!(read_section(&path, "a")["k"], "1");

        // Rewrite must hit the existing `[ a ]`, never spawn a second [a]…
        replace_section(&path, "a", "[a]\r\nk = 9\r\n").unwrap();
        let sections = read_all(&path);
        assert_eq!(sections.iter().filter(|s| s.id == "a").count(), 1);
        assert_eq!(read_section(&path, "a")["k"], "9");

        // …replace_key must land inside it…
        let (_d2, path2) = ini_file("[ a ]\r\nk = 1\r\n[b]\r\nk = 2\r\n");
        replace_key(&path2, "a", "extra", "3").unwrap();
        assert_eq!(read_section(&path2, "a")["extra"], "3");
        assert!(!read_section(&path2, "b").contains_key("extra"));

        // …rename must swap the REAL `[ a ]` span (no dangling ` ]`)…
        let (_d3, path3) = ini_file("[ a ]\r\nk = 1\r\n");
        rename_section(&path3, "a", "c").unwrap();
        assert_eq!(
            fs::read_to_string(&path3).unwrap(),
            "[c]\r\nk = 1\r\n",
            "rename must replace the actual header text"
        );

        // …and delete must remove it.
        let (_d4, path4) = ini_file("[ a ]\r\nk = 1\r\n[b]\r\nk = 2\r\n");
        delete_section(&path4, "a").unwrap();
        let sections = read_all(&path4);
        assert_eq!(sections.len(), 1);
        assert_eq!(sections[0].id, "b");
    }

    /// The `; url = ` reader, with the cases the configurator has always
    /// pinned (`crates/app/src/model_profiles.rs`): the value of the first url
    /// comment, and empty for anything else. The web's section read answers
    /// this same value, so the reader is in `pai-core` and both callers call
    /// it (`crates/bff/tests/model_profile_read.rs`).
    #[test]
    fn project_url_of_reads_the_url_comment() {
        assert_eq!(
            project_url_of(&["; url = https://a.b/c".into()]),
            "https://a.b/c"
        );
        // `#` and no spacing around `=`: a hand-written line counts, and a
        // fragment is part of the URL.
        assert_eq!(
            project_url_of(&["#url=https://a.b/c#x".into()]),
            "https://a.b/c#x"
        );
        // Prose about urls is prose; a word starting with `url` is not `url`.
        assert_eq!(
            project_url_of(&["; urls are listed below".into(), "; Model: x".into()]),
            ""
        );
        assert_eq!(project_url_of(&["; url-ish = no".into()]), "");
        // The first url comment is the one, as the configurator shows it.
        assert_eq!(
            project_url_of(&["; url = first".into(), "; url = second".into()]),
            "first"
        );
        // No comment, no URL: the empty string, never a missing field.
        assert_eq!(project_url_of(&[]), "");
    }

    /// The reader takes the comments `read_all` collects, so a model profile file
    /// read end to end answers its URL: the plumbing the section read uses.
    #[test]
    fn project_url_of_takes_the_comments_the_parser_collects() {
        let (_d, path) = ini_file(
            "; the file\r\n[alpha]\r\n; the router reads this file at launch\r\n; url = https://a.b/c\r\nmodel = a.gguf\r\n\r\n[b]\r\nmodel = b.gguf\r\n",
        );
        let sections = read_all(&path);
        assert_eq!(sections.len(), 2);
        assert_eq!(
            project_url_of(&sections[0].comments),
            "https://a.b/c",
            "a prose comment before it is skipped"
        );
        assert_eq!(
            project_url_of(&sections[1].comments),
            "",
            "a section without the comment"
        );
    }

    // ── The model profile save's in-place edits (text::set_key / text::remove_key) ──
    //
    // The web's save changes only the edited lines and keeps the rest of the
    // file byte for byte: `set_key` rewrites one line, `remove_key` drops the
    // key's lines, and a key written more than once collapses to the one line
    // every reader reads.

    /// A file of these lines, each one terminated by `eol`.
    fn file_of(lines: &[&str], eol: &str) -> String {
        let mut out = String::new();
        for line in lines {
            out.push_str(line);
            out.push_str(eol);
        }
        out
    }

    /// The lines of `text` the writers count as `key`, the way `replace_key`
    /// counts them.
    fn key_line_count(text: &str, key: &str) -> usize {
        text.lines()
            .filter(|l| line_starts_with_key(l, key))
            .count()
    }

    #[test]
    fn remove_key_drops_every_line_of_the_key() {
        let text = file_of(
            &[
                "[alpha]",
                "; tuned by hand",
                "ctx-size = 4096",
                "CTX-SIZE = 8192",
                "   ctx-size = 123",
                "ctx-size=7 ; note",
                "; ctx-size = 9",
                "ctx-size-draft = 512",
                "other = 1",
                "",
                "[beta]",
                "ctx-size = 42",
            ],
            "\r\n",
        );
        let out = text::remove_key(&text, "alpha", "ctx-size");
        assert_eq!(
            out,
            file_of(
                &[
                    "[alpha]",
                    "; tuned by hand",
                    "; ctx-size = 9",
                    "ctx-size-draft = 512",
                    "other = 1",
                    "",
                    "[beta]",
                    "ctx-size = 42",
                ],
                "\r\n",
            ),
            "every line of the key goes (any case, indented, unspaced around the \
             `=`) with its ending, and the comment naming it, the longer key, the \
             blank separator and the other section's own line all stay"
        );
        let sections = parse_sections(&out);
        assert!(!sections[0].keys.contains_key("ctx-size"));
        assert_eq!(sections[1].keys["ctx-size"], "42");
        assert_eq!(key_line_count(&out, "ctx-size"), 1);
    }

    #[test]
    fn remove_key_keeps_the_files_line_ending_style() {
        for eol in ["\n", "\r\n"] {
            let text = file_of(
                &["[alpha]", "ctx-size = 4096", "ctx-size = 8192", "other = 1"],
                eol,
            );
            let out = text::remove_key(&text, "alpha", "ctx-size");
            assert_eq!(out, file_of(&["[alpha]", "other = 1"], eol));
            if eol == "\n" {
                assert!(!out.contains('\r'), "an LF-only file gained a CR");
            }
        }
    }

    #[test]
    fn remove_key_of_an_absent_key_or_section_leaves_the_text_equal() {
        let text = file_of(
            &[
                "[alpha]",
                "; tuned by hand",
                "other = 1",
                "",
                "[beta]",
                "ctx-size = 42",
            ],
            "\r\n",
        );
        assert_eq!(
            text::remove_key(&text, "alpha", "ctx-size"),
            text,
            "the key is only in the other section"
        );
        assert_eq!(
            text::remove_key(&text, "gamma", "ctx-size"),
            text,
            "no such section"
        );
        assert_eq!(text::remove_key("", "alpha", "ctx-size"), "", "empty text");
    }

    // The file's last line has no ending of its own: the key's line goes with
    // its ending, and the section header is never touched.
    #[test]
    fn remove_key_of_the_last_line_of_the_last_section() {
        for eol in ["\n", "\r\n"] {
            let with_break = file_of(&["[alpha]", "k = 1", "ctx-size = 4096"], eol);
            assert_eq!(
                text::remove_key(&with_break, "alpha", "ctx-size"),
                file_of(&["[alpha]", "k = 1"], eol),
                "the file keeps a final break"
            );
            let without = with_break.strip_suffix(eol).unwrap();
            assert_eq!(
                text::remove_key(without, "alpha", "ctx-size"),
                file_of(&["[alpha]", "k = 1"], eol),
                "a final line with no break of its own"
            );
            assert_eq!(
                text::remove_key(
                    &file_of(&["[alpha]", "ctx-size = 4096"], eol),
                    "alpha",
                    "ctx-size"
                ),
                format!("[alpha]{eol}"),
                "the header stays"
            );
        }
    }

    // `read_all` trims the name inside the brackets and the line around them,
    // so an indented `[ Alpha ]` IS the section the reader lists: the writers
    // edit it there, and stop at the next (also indented) header.
    #[test]
    fn remove_key_takes_the_section_the_reader_finds() {
        let text = file_of(
            &[
                "  [ Alpha ]",
                "ctx-size = 4096",
                "k = 1",
                "  [beta]",
                "ctx-size = 42",
            ],
            "\r\n",
        );
        assert_eq!(
            text::remove_key(&text, "alpha", "ctx-size"),
            file_of(
                &["  [ Alpha ]", "k = 1", "  [beta]", "ctx-size = 42"],
                "\r\n"
            )
        );
    }

    #[test]
    fn remove_key_edits_only_the_first_of_a_repeated_section() {
        // Two `[a]` headers are not mended here (the manager refuses the write
        // with `model-profile-name-taken`), so the second copy is left as it is.
        let text = file_of(&["[a]", "ctx-size = 1", "[A]", "ctx-size = 2"], "\r\n");
        assert_eq!(
            text::remove_key(&text, "a", "ctx-size"),
            file_of(&["[a]", "[A]", "ctx-size = 2"], "\r\n")
        );
    }

    #[test]
    fn set_key_rewrites_only_the_edited_line() {
        let text = file_of(
            &[
                "[alpha]",
                "; keep me",
                "  Port   =   8080   ; hand note",
                "Host = localhost",
                "",
                "[beta]",
                "Port = 1",
            ],
            "\r\n",
        );
        assert_eq!(
            text::set_key(&text, "alpha", "Port", "9090"),
            file_of(
                &[
                    "[alpha]",
                    "; keep me",
                    "Port = 9090",
                    "Host = localhost",
                    "",
                    "[beta]",
                    "Port = 1",
                ],
                "\r\n",
            ),
            "the edited line is rewritten whole as `key = value` (its indentation, \
             its spacing and its inline comment go with it, as the design says), \
             its own ending stays, the comment line above stays, and the same key \
             in the other section is untouched"
        );
    }

    #[test]
    fn set_key_keeps_the_files_line_ending_style() {
        // Replacing an existing key and appending a new one in an LF-only file.
        let text = file_of(&["[alpha]", "Port = 8080", "", "[beta]", "k = 2"], "\n");
        let out = text::set_key(&text, "alpha", "Port", "9090");
        let out = text::set_key(&out, "alpha", "n-cpu-moe", "8");
        assert_eq!(
            out,
            file_of(
                &[
                    "[alpha]",
                    "Port = 9090",
                    "n-cpu-moe = 8",
                    "",
                    "[beta]",
                    "k = 2"
                ],
                "\n"
            )
        );
        // A rewritten final line keeps its OWN ending: an unterminated last
        // line stays unterminated, the rewrite only replaces the line's text.
        assert_eq!(
            text::set_key("[a]\r\nPort = 1", "a", "Port", "9"),
            "[a]\r\nPort = 9"
        );
        assert_eq!(
            text::set_key("[a]\nPort = 1", "a", "Port", "9"),
            "[a]\nPort = 9"
        );
    }

    // The readers take the LAST line of a key (llama-server the same), so the
    // page shows 3 here; after the edit exactly one line stands, at the first's
    // place, carrying the written value. A line of the key in another letter
    // case is an occurrence and goes too.
    #[test]
    fn set_key_collapses_a_repeated_key_onto_the_first() {
        let text = file_of(
            &[
                "[alpha]",
                "; note above",
                "ctx-size = 1",
                "CTX-SIZE = 2",
                "ctx-size = 3 ; last",
                "other = 9",
            ],
            "\r\n",
        );
        let out = text::set_key(&text, "alpha", "ctx-size", "4096");
        assert_eq!(
            out,
            file_of(
                &["[alpha]", "; note above", "ctx-size = 4096", "other = 9"],
                "\r\n"
            ),
            "the first keeps its place, every other line of the key goes"
        );
        assert_eq!(key_line_count(&out, "ctx-size"), 1);
        assert_eq!(parse_sections(&out)[0].keys["ctx-size"], "4096");
        // The only line in another case is the one that is set, and it takes the
        // spelling the caller asked for (the readers key on the exact name).
        assert_eq!(
            text::set_key("[alpha]\r\nCTX-SIZE = 1\r\n", "alpha", "ctx-size", "9"),
            "[alpha]\r\nctx-size = 9\r\n"
        );
    }

    #[test]
    fn set_key_appends_a_new_key_inside_the_section() {
        // The new line sits after the section's last real line, before its
        // trailing blank lines, and never past the next section's header.
        let text = file_of(&["[alpha]", "k = 1", "", "[beta]", "k = 2"], "\r\n");
        assert_eq!(
            text::set_key(&text, "alpha", "n-cpu-moe", "8"),
            file_of(
                &["[alpha]", "k = 1", "n-cpu-moe = 8", "", "[beta]", "k = 2"],
                "\r\n"
            )
        );
        // An empty body: the key lands in the section, not in the next one.
        assert_eq!(
            text::set_key("[alpha]\r\n[beta]\r\nk = 2\r\n", "alpha", "n", "9"),
            "[alpha]\r\nn = 9\r\n[beta]\r\nk = 2\r\n"
        );
    }

    // A section header sitting on the file's LAST line has no line ending of
    // its own. An absent key appended under it must start on its own line,
    // never glued to the `]` (the header's terminator is added, as
    // `replace_key` does), and the reader must find the key in that section.
    #[test]
    fn set_key_of_an_absent_key_under_a_last_line_header() {
        // An LF file: the break in front of the key, and the one closing it,
        // are the file's LF, no CR smuggled in.
        let text = format!("{}[Sec]", file_of(&["[alpha]", "k = 1"], "\n"));
        let out = text::set_key(&text, "Sec", "n-cpu-moe", "8");
        assert_eq!(
            out,
            file_of(&["[alpha]", "k = 1", "[Sec]", "n-cpu-moe = 8"], "\n"),
            "the header line keeps its own line, the key follows on the next"
        );
        let sections = parse_sections(&out);
        assert_eq!(sections.last().expect("the Sec section").id, "Sec");
        assert_eq!(
            sections.last().expect("the Sec section").keys["n-cpu-moe"],
            "8",
            "the key is read in the section it was written to"
        );

        // The same header as the file's last line, in a CRLF file (every other
        // line CRLF): the break between the header and the appended key is the
        // file's CRLF, and the key's own line takes it as the file's ending
        // (the append path's rule, the one the LF case above pins).
        let text = format!("{}[Sec]", file_of(&["[alpha]", "k = 1"], "\r\n"));
        let out = text::set_key(&text, "Sec", "n-cpu-moe", "8");
        assert_eq!(
            out,
            file_of(&["[alpha]", "k = 1", "[Sec]", "n-cpu-moe = 8"], "\r\n"),
            "the appended key is separated from the header by the file's CRLF"
        );
        let sections = parse_sections(&out);
        assert_eq!(sections.last().expect("the Sec section").id, "Sec");
        assert_eq!(
            sections.last().expect("the Sec section").keys["n-cpu-moe"],
            "8",
            "the key is read in the section it was written to"
        );

        // A file whose only line is the header has no line break at all, so
        // nothing to detect a style from: the writer still puts a break of its
        // default style between the header and the key. Which break (CRLF, as
        // `detect_eol` answers for content with no newline, the way
        // `replace_key` does, or LF) is not pinned here, only that one is
        // there and the header survives.
        const HEADER: &str = "[Sec]";
        let out = text::set_key(HEADER, "Sec", "n-cpu-moe", "8");
        assert!(out.starts_with(HEADER), "the header line: {out:?}");
        let rest = &out[HEADER.len()..];
        assert!(
            rest.starts_with('\n') || rest.starts_with("\r\n"),
            "no line break between the header and the key: {out:?}"
        );
        assert_eq!(
            rest.trim_matches(['\r', '\n']),
            "n-cpu-moe = 8",
            "the key line, between the breaks: {out:?}"
        );
        assert_eq!(key_line_count(&out, "n-cpu-moe"), 1);
        let sections = parse_sections(&out);
        assert_eq!(sections.len(), 1, "one section: {out:?}");
        assert_eq!(sections[0].id, "Sec", "{out:?}");
        assert_eq!(sections[0].keys["n-cpu-moe"], "8", "{out:?}");
    }

    #[test]
    fn set_key_leaves_an_absent_section_untouched() {
        // The route answers a model profile that is in neither file before it edits
        // anything, so this transform creates no section (`replace_key` does).
        let text = file_of(&["[alpha]", "k = 1"], "\r\n");
        assert_eq!(
            text::set_key(&text, "beta", "k", "2"),
            text,
            "byte for byte"
        );
        assert_eq!(text::set_key("", "beta", "k", "2"), "", "empty text");
    }

    #[test]
    fn set_key_of_the_value_the_key_already_has_is_the_text_unchanged() {
        for eol in ["\n", "\r\n"] {
            let text = file_of(&["[alpha]", "ctx-size = 4096", ""], eol);
            assert_eq!(
                text::set_key(&text, "alpha", "ctx-size", "4096"),
                text,
                "the route writes nothing when the text comes back equal"
            );
        }
    }

    #[test]
    fn set_key_writes_the_value_as_it_is_given() {
        // Trimming the value and refusing `;`/`#` are the route's rules; this
        // transform checks nothing, so the value it is handed is the value that
        // lands after `key =`.
        assert_eq!(
            text::set_key("[a]\r\nPort = 1\r\n", "a", "Port", " 9 "),
            "[a]\r\nPort =  9 \r\n"
        );
        assert_eq!(
            text::set_key("[a]\r\nPort = 1\r\n", "a", "Port", "C:/Models #1"),
            "[a]\r\nPort = C:/Models #1\r\n"
        );
    }

    // "Already has that value" is compared the way the parser READS the line:
    // the value with any inline comment cut at the first `;`/`#` and trimmed.
    // When the key sits exactly once in the section, spells the key in the
    // exact case given, and that reading equals the value given, the text comes
    // back byte for byte: the line is NOT canonicalised, its indentation,
    // spacing and inline comment all stay. A line spelling the key in ANOTHER
    // case is rewritten, because llama-server refuses that spelling
    // (common/preset.cpp): leaving it would answer a save with 200 over a
    // model profile that does not start.
    #[test]
    fn set_key_of_an_equal_value_returns_the_text_byte_for_byte() {
        for eol in ["\n", "\r\n"] {
            let canonical = file_of(&["[alpha]", "Port = 9090", "Host = localhost"], eol);
            assert_eq!(
                text::set_key(&canonical, "alpha", "Port", "9090"),
                canonical,
                "the canonical line is not rewritten at all"
            );

            let commented = file_of(
                &["[alpha]", "Port = 9090 ; hand note", "Host = localhost"],
                eol,
            );
            assert_eq!(
                text::set_key(&commented, "alpha", "Port", "9090"),
                commented,
                "the value read is `9090`, the inline comment stays"
            );

            let spaced = file_of(
                &[
                    "[alpha]",
                    "  Port\t=  9090  ; hand note",
                    "Host = localhost",
                ],
                eol,
            );
            assert_eq!(
                text::set_key(&spaced, "alpha", "Port", "9090"),
                spaced,
                "indentation, tabs and spacing are kept, not canonicalised"
            );
        }
        // The file's last line has no ending of its own: the text, ending
        // included, comes back as it was.
        let last = "[alpha]\nPort = 9090";
        assert_eq!(
            text::set_key(last, "alpha", "Port", "9090"),
            last,
            "the unterminated final line stays unterminated"
        );
    }

    // The equal-value case of the sibling test is for a line that spells the
    // key AS IT IS GIVEN: `CTX-SIZE = 4096` set to `ctx-size` `4096` is
    // rewritten as `ctx-size = 4096`, at the same place, with the line's own
    // ending. llama-server knows a key only in its own case and refuses a
    // model profile holding another spelling, so an untouched line here is a save that
    // answers 200 over a model profile that will not start.
    #[test]
    fn set_key_of_an_equal_value_rewrites_a_key_spelled_in_another_case() {
        for eol in ["\n", "\r\n"] {
            let upper = file_of(&["[alpha]", "CTX-SIZE = 4096", "host = localhost"], eol);
            assert_eq!(
                text::set_key(&upper, "alpha", "ctx-size", "4096"),
                file_of(&["[alpha]", "ctx-size = 4096", "host = localhost"], eol),
                "the line is rewritten in the spelling given, and the neighbour \
                 line keeps its own ({eol:?})"
            );

            // The line keeps its own ending: an unterminated last line stays
            // unterminated.
            assert_eq!(
                text::set_key("[alpha]\nCTX-SIZE = 4096", "alpha", "ctx-size", "4096"),
                "[alpha]\nctx-size = 4096",
                "the rewrite of an unterminated final line leaves it unterminated"
            );
        }
    }

    // When the value DOES differ, the line is rewritten whole as `key = value`
    // (the decision the sibling test pins: only an equal value is untouched),
    // and the rewritten line keeps its own ending.
    #[test]
    fn set_key_of_a_changed_value_keeps_the_last_lines_own_ending() {
        let text = file_of(
            &["[alpha]", "Port = 9090 ; hand note", "Host = localhost"],
            "\n",
        );
        assert_eq!(
            text::set_key(&text, "alpha", "Port", "8080"),
            file_of(&["[alpha]", "Port = 8080", "Host = localhost"], "\n"),
            "a changed value rewrites the line whole, comment included"
        );
        assert_eq!(
            text::set_key("[a]\nPort = 1 ; note", "a", "Port", "9"),
            "[a]\nPort = 9",
            "the unterminated final line stays unterminated"
        );
    }

    // A key that occurs more than once is collapsed onto the first occurrence
    // even when every copy already holds the value: "nothing to write" is only
    // true of a key that sits exactly once in the section.
    #[test]
    fn set_key_collapses_a_repeated_key_even_when_the_values_match() {
        let text = file_of(
            &[
                "[alpha]",
                "ctx-size = 4096",
                "CTX-SIZE = 4096 ; note",
                "other = 9",
            ],
            "\r\n",
        );
        let out = text::set_key(&text, "alpha", "ctx-size", "4096");
        assert_eq!(
            out,
            file_of(&["[alpha]", "ctx-size = 4096", "other = 9"], "\r\n"),
            "one line stands, at the first's place"
        );
        assert_eq!(key_line_count(&out, "ctx-size"), 1);
    }

    // Blank and comment lines are not lines of the key: a collapse keeps every
    // one of them, wherever it sits.
    #[test]
    fn set_key_keeps_the_blank_and_comment_lines_between_duplicates() {
        let text = file_of(
            &[
                "[alpha]",
                "ctx-size = 1",
                "",
                "; a note between the two",
                "ctx-size = 2",
                "other = 9",
            ],
            "\r\n",
        );
        assert_eq!(
            text::set_key(&text, "alpha", "ctx-size", "4096"),
            file_of(
                &[
                    "[alpha]",
                    "ctx-size = 4096",
                    "",
                    "; a note between the two",
                    "other = 9",
                ],
                "\r\n",
            )
        );
    }

    // A key is matched as a whole name, not as a prefix: `keyboard` is not a
    // line of `key` (the same rule as `ctx-size-draft`).
    #[test]
    fn set_key_and_remove_key_of_key_leave_a_longer_key_alone() {
        let text = file_of(&["[alpha]", "keyboard = 1", "other = 2"], "\r\n");
        assert_eq!(
            text::set_key(&text, "alpha", "key", "9"),
            file_of(&["[alpha]", "keyboard = 1", "other = 2", "key = 9"], "\r\n"),
            "the new line lands after the section's last real line, `keyboard` untouched"
        );
        assert_eq!(
            text::remove_key(&text, "alpha", "key"),
            text,
            "nothing to drop: the section has no `key`"
        );
    }

    // The key and value spans are byte counts into a line: every name and
    // value here is multi-byte UTF-8.
    #[test]
    fn set_key_and_remove_key_in_a_section_with_non_ascii_text() {
        let text = file_of(
            &[
                "[café]",
                ";注释",
                "名字 = Café",
                "ctx-size = 4096",
                "notes = Ω 日本語",
            ],
            "\r\n",
        );
        assert_eq!(
            text::remove_key(&text, "café", "名字"),
            file_of(
                &["[café]", ";注释", "ctx-size = 4096", "notes = Ω 日本語"],
                "\r\n"
            ),
            "the non-ASCII key's line goes, its neighbour stays"
        );
        assert_eq!(
            text::set_key(&text, "café", "ctx-size", "8192"),
            file_of(
                &[
                    "[café]",
                    ";注释",
                    "名字 = Café",
                    "ctx-size = 8192",
                    "notes = Ω 日本語"
                ],
                "\r\n",
            ),
            "the multi-byte value three lines away is not touched"
        );
        assert_eq!(
            text::set_key(&text, "café", "名字", "日本語"),
            file_of(
                &[
                    "[café]",
                    ";注释",
                    "名字 = 日本語",
                    "ctx-size = 4096",
                    "notes = Ω 日本語"
                ],
                "\r\n",
            ),
            "a non-ASCII key and value are rewritten as given"
        );
        // The equal value is byte for byte even with a multi-byte value.
        assert_eq!(
            text::set_key(&text, "café", "notes", "Ω 日本語"),
            text,
            "the section name and the value are both kept as they are"
        );
        // A key whose byte length lands INSIDE a multi-byte character of a
        // line is not that line's key: the writers count bytes into a line, so
        // this is the shape that would panic on a slice.
        let edges = file_of(&["[café]", "aΩ = 1", "ab = 2"], "\r\n");
        assert_eq!(
            text::remove_key(&edges, "café", "ab"),
            file_of(&["[café]", "aΩ = 1"], "\r\n"),
            "only the real line goes"
        );
        assert_eq!(
            text::set_key(&edges, "café", "ab", "3"),
            file_of(&["[café]", "aΩ = 1", "ab = 3"], "\r\n"),
            "the neighbour line is not read as `ab`"
        );
    }

    // The reader parses `[*]` as a section named `*` (llama-server's global
    // section), so the writers edit it like any other.
    #[test]
    fn set_key_and_remove_key_in_the_star_section() {
        let text = file_of(&["[*]", "use-jinja = true", "", "[alpha]", "k = 1"], "\r\n");
        assert_eq!(
            text::set_key(&text, "*", "use-jinja", "false"),
            file_of(
                &["[*]", "use-jinja = false", "", "[alpha]", "k = 1"],
                "\r\n"
            )
        );
        assert_eq!(
            text::remove_key(&text, "*", "use-jinja"),
            file_of(&["[*]", "", "[alpha]", "k = 1"], "\r\n")
        );
        assert_eq!(
            text::set_key(&text, "alpha", "k", "1"),
            text,
            "the guard: the equal value in the real section comes back untouched"
        );
    }

    // The writers take the FIRST section the reader finds; two headers of one
    // name are not mended here (the manager refuses the write).
    #[test]
    fn set_key_edits_only_the_first_of_a_repeated_section() {
        let text = file_of(&["[a]", "k = 1", "[A]", "k = 2"], "\r\n");
        assert_eq!(
            text::set_key(&text, "a", "k", "9"),
            file_of(&["[a]", "k = 9", "[A]", "k = 2"], "\r\n")
        );
    }

    // The body of a section with no keys: the new line lands between the two
    // headers. The file's last line has no ending of its own, so an appended
    // key must not be glued to that header: it starts on its own line and
    // closes with the file's ending.
    #[test]
    fn set_key_on_a_last_line_header_with_an_empty_body() {
        assert_eq!(
            text::set_key("[a]\n[b]", "a", "k", "1"),
            "[a]\nk = 1\n[b]",
            "the key is inside `a`, the next header keeps its line"
        );
        assert_eq!(
            text::set_key("[a]\n[b]", "b", "k", "1"),
            "[a]\n[b]\nk = 1\n",
            "the key is inside `b`, not glued to its header"
        );
        assert_eq!(
            parse_sections(&text::set_key("[a]\n[b]", "b", "k", "1"))
                .last()
                .expect("b")
                .keys["k"],
            "1",
            "the reader finds the key in `b`"
        );
    }
}
