// The render of one instance's `models@<id>.ini`: the pool (the pair
// `models.ini` and `models.ini.disabled`) cut to the model profiles one instance is
// given by its section of `assignments.ini`.
//
// The manager owns that file: it renders it after every write of a pool file,
// of an assignment and of an instance's create or remove, and renders every
// instance again at its own start. The render here is pure: texts in, text out.

use crate::ini;
use std::collections::{BTreeMap, BTreeSet};

/// The text of one instance's `models@<id>.ini`.
///
/// - `enabled`: the text of `models.ini`, the pool's switched-on model profiles
/// - `disabled`: the text of `models.ini.disabled`, the switched-off ones
/// - `assignment`: the instance's section of `assignments.ini` as
///   `ini::text::section_text` hands it (its `[<id>]` header line included),
///   or `None` when the instance has no section. One key per assigned model profile
///   id: `inherit` follows the global switch, `on` and `off` are the explicit
///   override. The key `*` assigns every model profile of the pool, and an explicit
///   key wins over it. A header of its own is optional: a bare list of key
///   lines is the assignment. A key whose value is none of those three words
///   (a hand-edited `none`) is an absent key.
///
/// The pool's global `[*]` section comes first, always (assigned or not, and
/// taken from the file that holds it), then every assigned model profile that is in
/// effect on, in the pool's own order (`enabled` first, then `disabled`), each
/// section copied byte for byte from the file that holds it. A model profile with no
/// key and no `*` is not assigned, so it is not rendered; an assigned id the
/// pool does not hold renders nothing, so an instance with no assignment
/// section is given `[*]` alone. `*` is never a model profile id, so it never becomes
/// a section of its own. One id is one model profile, so an id the two pool files
/// both hold, or written twice in one file, in any letter case, is rendered
/// once, from the first copy: `models.ini` before `models.ini.disabled`.
///
/// The file opens with the one comment line saying it is generated, in the
/// ending style of `enabled`, or of `disabled` when `enabled` is empty
/// (`ini::detect_eol`). The copied sections keep their own bytes, and a
/// section whose last line has no ending of its own, the last one of a file
/// left unterminated, is given the render's, so the section that follows it
/// opens a line of its own.
pub fn render(enabled: &str, disabled: &str, assignment: Option<&str>) -> String {
    // The ending style comes from the pool's file that holds text, so an empty
    // `models.ini` takes the style of the file that has the pool.
    let eol = if enabled.is_empty() {
        ini::detect_eol(disabled)
    } else {
        ini::detect_eol(enabled)
    };
    let keys = assigned(assignment);
    let star = keys.get("*").map(String::as_str);
    let mut out = format!("{HEADER}{eol}");
    for pool in [enabled, disabled] {
        if let Some(text) = ini::text::section_text(pool, "*") {
            push_section(&mut out, &text, eol);
            break;
        }
    }
    // The ids rendered so far, lowered: the reader finds a section ignoring the
    // letter case, so a second copy of an id is the same model profile and renders
    // nothing.
    let mut rendered: BTreeSet<String> = BTreeSet::new();
    for (in_enabled, pool) in [(true, enabled), (false, disabled)] {
        // The global switch of this file's model profiles: the file that holds them
        // (`enabled` is the switched-on half of the pool), with `on` and `off`
        // under `*` replacing it for every model profile of the pool.
        let global = match star {
            Some("on") => true,
            Some("off") => false,
            _ => in_enabled,
        };
        for id in pool_ids(pool) {
            let on = match keys.get(&id) {
                // The model profile's own key wins over `*`; `inherit` follows the
                // global switch.
                Some(value) => is_on(value, global),
                None => keys.contains_key("*") && global,
            };
            if on && rendered.insert(id.to_lowercase()) {
                if let Some(text) = ini::text::section_text(pool, &id) {
                    push_section(&mut out, &text, eol);
                }
            }
        }
    }
    out
}

/// A copied section, and the render's line ending when the section's text ends
/// in the middle of a line (the last section of a file with no final break), so
/// the section written after it opens a line of its own.
fn push_section(out: &mut String, text: &str, eol: &'static str) {
    out.push_str(text);
    if !text.ends_with('\n') {
        out.push_str(eol);
    }
}

/// The header line of the generated file. Text written into a file, not shown
/// to anyone, so it stays here rather than in a strings file.
const HEADER: &str = "; Generated by pai-manager from the model profile pool: do not edit.";

/// One assignment value as a switch: `on` and `off` as the manager writes them,
/// `inherit` as the global one. Only these three reach here, `assigned` drops
/// the rest.
fn is_on(value: &str, global: bool) -> bool {
    match value {
        "on" => true,
        "off" => false,
        _ => global,
    }
}

/// The keys of an assignment section, exact case, its values read the way
/// `ini::parse_sections` reads them (trimmed, an inline comment dropped). No
/// section, no keys. A key whose value is not one of the three words the
/// manager writes is dropped: it assigns nothing, as if it were absent.
fn assigned(assignment: Option<&str>) -> BTreeMap<String, String> {
    let Some(text) = assignment else {
        return BTreeMap::new();
    };
    // A header of its own in front of the text: `parse_sections` keeps the
    // lines before the first header nowhere, so a key list written without one
    // (a hand-edited file) reads as nothing. The section the text has, if any,
    // is the LAST one and holds its keys.
    let sections = ini::parse_sections(&format!("[keys]\n{text}"));
    let mut keys = sections
        .last()
        .map(|section| section.keys.clone())
        .unwrap_or_default();
    keys.retain(|_, value| matches!(value.as_str(), "inherit" | "on" | "off"));
    keys
}

/// The model profile ids of a pool file, in the file's own order, `[*]` aside.
fn pool_ids(pool: &str) -> Vec<String> {
    ini::parse_sections(pool)
        .into_iter()
        .map(|section| section.id)
        .filter(|id| id != "*")
        .collect()
}

/// One model profile of the pool, with what one instance's assignment makes of
/// it: the `id` as the pool spells it, `value` the explicit key of
/// `assignments.ini` (`none` when the id has no key of its own, or a word the
/// render ignores), `via_all` true when the id has no key of its own and the
/// key `*` stands in for it, and `effective` true when the render emits the
/// id's section.
pub struct AssignmentRow {
    pub id: String,
    pub value: String,
    pub via_all: bool,
    pub effective: bool,
}

/// The pool and one instance's assignment, read as the rows the web UI lists:
/// the `*` the page shows (`inherit` when the render honours the key `*`,
/// `none` when it does not) and one row per model profile of the pool, in
/// `render`'s own order.
///
/// One rule, the render's: a row is effective when `render` emits the id's
/// section, so the effective rows are exactly the rendered sections, in the
/// render's order, and a page never needs a second copy of the render. The
/// spelling of the row is the pool's, of the first file that holds the id.
pub fn rows(
    enabled: &str,
    disabled: &str,
    assignment: Option<&str>,
) -> (String, Vec<AssignmentRow>) {
    let keys = assigned(assignment);
    let star = keys.get("*").map(String::as_str);
    let all = if keys.contains_key("*") {
        "inherit"
    } else {
        "none"
    }
    .to_string();
    let mut out: Vec<AssignmentRow> = Vec::new();
    // The ids of the rows so far, lowered, with their place: one id is one
    // model profile, so a copy in the other file, or a second spelling, is not
    // a row of its own.
    let mut seen: BTreeMap<String, usize> = BTreeMap::new();
    for (in_enabled, pool) in [(true, enabled), (false, disabled)] {
        // The global switch, as `render` reads it.
        let global = match star {
            Some("on") => true,
            Some("off") => false,
            _ => in_enabled,
        };
        for id in pool_ids(pool) {
            // The id's own key, exact case, as `render` looks it up.
            let key = keys.get(&id);
            let on = match key {
                Some(value) => is_on(value, global),
                None => keys.contains_key("*") && global,
            };
            let row = AssignmentRow {
                id: id.clone(),
                value: key.cloned().unwrap_or_else(|| "none".to_string()),
                via_all: key.is_none() && keys.contains_key("*"),
                effective: on,
            };
            let lower = id.to_lowercase();
            if let Some(at) = seen.get(&lower) {
                // The row is the first copy of the id, where it sits in the
                // pool: the section `render` copies is that one. A second copy
                // the render emits makes the row effective.
                if on {
                    out[*at].effective = true;
                }
            } else {
                seen.insert(lower, out.len());
                out.push(row);
            }
        }
    }
    (all, out)
}

/// The text of one instance's `models@<id>.ini`, rendered from the instance
/// profile's shape (`ARCHITECTURE.md`, "Instance profiles", the placement and
/// memory amendment):
///
/// - `pool`: the text of `models.ini`, the one pool file
/// - `profiles`: the text of `instance-profiles.ini`
/// - `profile`: the instance profile the instance names, whose `[<p>/<m>]`
///   entry sections are the model profiles it is given, with their placement
///   and memory keys
///
/// The pool's global `[*]` section comes first, always, copied byte for byte.
/// Then every entry of `profile`, in the pool's own order: the model profile's
/// section from `pool` (bytes kept), followed by the entry's key lines. A model
/// profile the pool does not hold renders nothing, and a model profile with no
/// entry of this profile renders nothing, so a profile with no entry is given
/// `[*]` alone.
///
/// The entry wins: a key the model profile's section and the entry both name is
/// written ONCE, with the entry's value (a pool section that still carries a
/// moved key, from before the migration or from a hand edit, is the second
/// source, and a rendered section of two lines of one key is what the engine
/// reads as one).
pub fn render_profile(pool: &str, profiles: &str, profile: &str) -> String {
    let eol = ini::detect_eol(pool);
    let mut out = format!("{HEADER}{eol}");
    if let Some(text) = ini::text::section_text(pool, "*") {
        push_section(&mut out, &text, eol);
    }
    for id in pool_ids(pool) {
        let entry = format!("{profile}/{id}");
        // The entry is the list: a model profile with no entry of this
        // instance profile is not given to it, and so renders nothing.
        if ini::text::section_text(profiles, &entry).is_none() {
            continue;
        }
        let Some(text) = ini::text::section_text(pool, &id) else {
            continue;
        };
        let keys = ini::section_keys(profiles, &entry);
        let entry_keys: Vec<String> = keys.iter().map(|(key, _)| key.clone()).collect();
        let mut section = String::new();
        for line in text.split_inclusive('\n') {
            // The section's own line of a key the entry names too: dropped, so
            // the key goes into the render once, with the entry's value. Only a
            // key line counts (the `=` right after the key), never a comment.
            if !entry_keys
                .iter()
                .any(|key| ini::line_starts_with_key(line, key))
            {
                section.push_str(line);
            }
        }
        for (key, value) in keys {
            section.push_str(&format!("{key} = {value}"));
            section.push_str(eol);
        }
        push_section(&mut out, &section, eol);
    }
    out
}

#[cfg(test)]
#[path = "assign_tests.rs"]
mod tests;
