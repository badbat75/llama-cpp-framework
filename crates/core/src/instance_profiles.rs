//! The instance profiles: the pool file `instance-profiles.ini`, one `[<p>]`
//! section per instance profile holding its `ServerProfile`, and one `[<p>/<m>]`
//! section per model profile the profile lists, holding that model's placement
//! and memory keys (`ARCHITECTURE.md`, "Instance profiles", the placement and
//! memory amendment). The entry sections ARE the model list, in their order: the
//! file holds no `Models` key. Pure code: texts in, texts out, no path read.
//!
//! The readers and the section edits are pure: `parse` answers the text as
//! profiles, and every edit answers a text of the same shape, in the file's
//! own line-ending style, leaving what it does not touch where it is.

use crate::ini;
use std::collections::BTreeMap;

/// The key of a `[<p>]` section that names the server profile, spelled as the
/// instance record spells its key of the same name.
const SERVER_PROFILE: &str = "ServerProfile";

/// One model entry `[<p>/<m>]`: the model profile id and the entry's keys, in
/// the file's own order. An entry with no keys is a member of the profile all
/// the same (membership is the presence of the section).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Entry {
    pub model: String,
    pub keys: Vec<(String, String)>,
}

/// One instance profile `[<p>]`: its id, its `ServerProfile` (None when the
/// section carries no such key), and its entries, in the file's own order.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Profile {
    pub id: String,
    pub server_profile: Option<String>,
    pub entries: Vec<Entry>,
}

/// The text of `instance-profiles.ini` as profiles, in the file's own order.
/// A `[<p>/<m>]` section is the entry `m` of the profile `p`, and opens `p`
/// when the file opens it nowhere, so an entry is never orphaned. A key of a
/// `[<p>]` section is read only when it is [`SERVER_PROFILE`]: the section
/// holds nothing else in this shape.
pub fn parse(text: &str) -> Vec<Profile> {
    let mut out: Vec<Profile> = Vec::new();
    // Each profile's id, lowered, and its place in `out`: one id is one
    // profile, as the section writers find one (any letter case).
    let mut places: BTreeMap<String, usize> = BTreeMap::new();
    // The section the lines read so far belong to: the profile's place, and the
    // entry its key lines feed (`None` for a `[<p>]` section).
    let mut current: Option<(usize, Option<usize>)> = None;
    for line in text.lines() {
        let t = line.trim();
        if t.is_empty() {
            continue;
        }
        if let Some(name) = t.strip_prefix('[').and_then(|rest| rest.strip_suffix(']')) {
            let name = name.trim();
            let (id, model) = match name.split_once('/') {
                Some((profile, model)) => (profile, Some(model)),
                None => (name, None),
            };
            let place = *places.entry(id.to_lowercase()).or_insert_with(|| {
                out.push(Profile {
                    id: id.to_string(),
                    server_profile: None,
                    entries: Vec::new(),
                });
                out.len() - 1
            });
            let entry = match model {
                None => None,
                Some(model) => match out[place]
                    .entries
                    .iter()
                    .position(|e| e.model.eq_ignore_ascii_case(model))
                {
                    Some(at) => Some(at),
                    None => {
                        out[place].entries.push(Entry {
                            model: model.to_string(),
                            keys: Vec::new(),
                        });
                        Some(out[place].entries.len() - 1)
                    }
                },
            };
            current = Some((place, entry));
            continue;
        }
        let Some((place, entry)) = current else {
            continue;
        };
        let Some((key, value)) = ini::key_of_line(t) else {
            continue;
        };
        match entry {
            None => {
                if key == SERVER_PROFILE {
                    out[place].server_profile = Some(value);
                }
            }
            Some(at) => out[place].entries[at].keys.push((key, value)),
        }
    }
    out
}

/// The section name of `model`'s entry under `profile`.
pub(crate) fn entry_name(profile: &str, model: &str) -> String {
    format!("{profile}/{model}")
}

/// The whole text in `eol`'s style: the section writers normalise, and a file
/// of one style stays a file of that style.
fn normalize(text: &str, eol: &'static str) -> String {
    let lf = text.replace("\r\n", "\n");
    if eol == "\r\n" {
        lf.replace('\n', "\r\n")
    } else {
        lf
    }
}

/// `text` with a section `[section]` holding `lines` appended, in the file's
/// own line-ending style `eol`, with a blank line in front when the file is
/// not empty. Every writer here answers a file `parse` reads.
fn append_section(text: &str, eol: &'static str, section: &str, lines: &[String]) -> String {
    let mut out = normalize(text, eol);
    if !out.is_empty() {
        out = out.trim_end_matches(['\r', '\n']).to_string();
        out.push_str(eol);
        out.push_str(eol);
    }
    out.push_str(&format!("[{section}]{eol}"));
    for line in lines {
        out.push_str(line);
        out.push_str(eol);
    }
    out
}

/// `text` with an instance profile `[<id>]` appended, its `ServerProfile` key
/// set to `server_profile`. A profile the text already has is left as it is:
/// its own `ServerProfile` line is its state, and a second section of the same
/// id would be a second source of it.
pub fn add_profile(text: &str, id: &str, server_profile: &str) -> String {
    if ini::text::section_text(text, id).is_some() {
        return text.to_string();
    }
    append_section(
        text,
        ini::detect_eol(text),
        id,
        &[format!("{SERVER_PROFILE} = {server_profile}")],
    )
}

/// `text` with an empty entry `[<profile>/<model>]` appended to `profile`. An
/// entry the text has is left alone, keys and all: adding a model a profile
/// already holds writes nothing.
pub fn add_entry(text: &str, profile: &str, model: &str) -> String {
    let section = entry_name(profile, model);
    if ini::text::section_text(text, &section).is_some() {
        return text.to_string();
    }
    append_section(text, ini::detect_eol(text), &section, &[])
}

/// `text` with the entry `[<profile>/<model>]` holding exactly `keys`, in that
/// order. The entry is written whole, so it moves to the end of the file, and
/// a profile that has no such entry gains one.
pub fn set_entry_keys(text: &str, profile: &str, model: &str, keys: &[(String, String)]) -> String {
    let section = entry_name(profile, model);
    let eol = ini::detect_eol(text);
    let lines: Vec<String> = keys
        .iter()
        .map(|(key, value)| format!("{key} = {value}"))
        .collect();
    let stripped = ini::text::delete_section(text, &section).unwrap_or_else(|| text.to_string());
    append_section(&stripped, eol, &section, &lines)
}

#[cfg(test)]
mod tests {
    use super::{add_entry, add_profile, parse, set_entry_keys};

    /// A pool file in its shape: two profiles, three entries, a comment, an
    /// empty entry, and a `Models` key left from the shape the design first
    /// proposed (the sections are the list, so it names nothing).
    fn fixture() -> String {
        [
            "; instance-profiles.ini",
            "[default]",
            "ServerProfile = default",
            "Models = gamma",
            "",
            "[default/alpha]",
            "; alpha on this deployment",
            "device = cuda0",
            "n-gpu-layers = 99",
            "",
            "[default/beta]",
            "",
            "[lab]",
            "ServerProfile = lab",
            "",
            "[lab/alpha]",
            "device = cpu",
            "",
        ]
        .join("\n")
    }

    /// The entry ids of a profile, in the order the parse read them.
    fn entry_models(profile: &super::Profile) -> Vec<String> {
        profile.entries.iter().map(|e| e.model.clone()).collect()
    }

    /// The entry's keys as pairs, for a readable compare.
    fn keys(entry: &super::Entry) -> Vec<(String, String)> {
        entry.keys.clone()
    }

    /// `ip1` criterion: `parse` reads the `[<p>]` section with its
    /// `ServerProfile` and the `[<p>/<m>]` entries of it, in the file's order;
    /// an empty entry is a member with no keys, a comment is no key, and the
    /// `Models` key of a profile section adds no entry.
    #[test]
    fn ip1_parse_reads_the_profile_and_entry_sections() {
        let profiles = parse(&fixture());
        let ids: Vec<String> = profiles.iter().map(|p| p.id.clone()).collect();
        assert_eq!(ids, ["default", "lab"], "the profiles are not these");
        assert_eq!(
            profiles[0].server_profile.as_deref(),
            Some("default"),
            "the [default] section's ServerProfile is not read"
        );
        assert_eq!(
            entry_models(&profiles[0]),
            ["alpha", "beta"],
            "the entries of [default] are not these, and a Models key adds none"
        );
        assert_eq!(
            keys(&profiles[0].entries[0]),
            [
                ("device".to_string(), "cuda0".to_string()),
                ("n-gpu-layers".to_string(), "99".to_string())
            ],
            "the entry's keys are not these, in the file's order"
        );
        assert!(
            profiles[0].entries[1].keys.is_empty(),
            "the empty [default/beta] entry holds keys: {:?}",
            profiles[0].entries[1]
        );
        assert_eq!(
            profiles[1].server_profile.as_deref(),
            Some("lab"),
            "the [lab] section's ServerProfile is not read"
        );
        assert_eq!(
            keys(&profiles[1].entries[0]),
            [("device".to_string(), "cpu".to_string())],
            "the same model under another profile reads its own entry"
        );
        for p in &profiles {
            assert!(
                !p.id.contains('/'),
                "the name [p/m] is read as a profile named {:?}: it splits",
                p.id
            );
        }
        assert_eq!(
            parse("[solo]\n").first().map(|p| p.id.as_str()),
            Some("solo"),
            "a profile section with no ServerProfile key is read all the same"
        );
        assert_eq!(
            parse("[solo]")
                .first()
                .and_then(|p| p.server_profile.as_deref()),
            None,
            "a profile section with no ServerProfile key reads a profile with none"
        );
        assert!(
            parse("").is_empty(),
            "an empty file reads no instance profile"
        );
    }

    /// `ip1` criterion: the section edit writes the shape `parse` reads: a
    /// profile with its `ServerProfile` line, an empty entry, an entry with
    /// keys in the order given, and a second profile that leaves the first
    /// one's entries where they are.
    #[test]
    fn ip1_the_section_edit_writes_the_shape_the_parse_reads() {
        let one = add_profile("", "default", "default");
        assert!(
            one.contains("ServerProfile = default"),
            "the profile section is not written with the key's own spelling: {one:?}"
        );
        let profiles = parse(&one);
        assert_eq!(
            profiles
                .iter()
                .map(|p| p.id.clone())
                .collect::<Vec<String>>(),
            ["default"],
            "the appended profile is not read back"
        );
        assert_eq!(
            profiles[0].server_profile.as_deref(),
            Some("default"),
            "the appended profile's ServerProfile is not read back"
        );
        assert!(
            profiles[0].entries.is_empty(),
            "a profile section alone holds entries"
        );

        let with_entry = add_entry(&one, "default", "alpha");
        let profiles = parse(&with_entry);
        assert_eq!(
            entry_models(&profiles[0]),
            ["alpha"],
            "the empty entry added is not read back"
        );
        assert!(
            profiles[0].entries[0].keys.is_empty(),
            "the entry added empty holds keys: {:?}",
            profiles[0].entries[0]
        );

        let filled = set_entry_keys(
            &with_entry,
            "default",
            "alpha",
            &[
                ("device".to_string(), "cuda0".to_string()),
                ("n-gpu-layers".to_string(), "99".to_string()),
            ],
        );
        let profiles = parse(&filled);
        assert_eq!(
            keys(&profiles[0].entries[0]),
            [
                ("device".to_string(), "cuda0".to_string()),
                ("n-gpu-layers".to_string(), "99".to_string())
            ],
            "the entry's keys are not the ones written, in that order"
        );

        let two = add_profile(&filled, "lab", "lab");
        let profiles = parse(&two);
        assert_eq!(
            profiles
                .iter()
                .map(|p| p.id.clone())
                .collect::<Vec<String>>(),
            ["default", "lab"],
            "the second profile appended is not read back after the first"
        );
        assert_eq!(
            profiles[1].server_profile.as_deref(),
            Some("lab"),
            "the second profile's ServerProfile is not read"
        );
        assert_eq!(
            entry_models(&profiles[0]),
            ["alpha"],
            "the profile appended after an entry swallowed that entry"
        );
        assert_eq!(
            keys(&profiles[0].entries[0]),
            [
                ("device".to_string(), "cuda0".to_string()),
                ("n-gpu-layers".to_string(), "99".to_string())
            ],
            "the profile appended after an entry moved that entry's keys"
        );
    }
}
