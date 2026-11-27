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
#[path = "ini_tests.rs"]
mod tests;
