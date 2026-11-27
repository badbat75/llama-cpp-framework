//! The migration of the model data model (IP1, `ARCHITECTURE.md` "Instance
//! profiles" and its placement and memory amendment): today's files in, the
//! new files out. Pure code: no path is read and no file written, the manager
//! runs it and writes the texts it answers (IP2).
//!
//! One instance profile per instance (the instance's own id, its `ServerProfile`
//! the instance's), one `[<p>/<m>]` entry per model the instance's current
//! render holds, holding the placement and memory keys moved out of that model's
//! `models.ini` section; a model two instances hold gets a copy in both entries;
//! a model no render holds goes to a migration-made profile `unassigned`. The
//! pool is merged (`models.ini.disabled` appended after the switched-on
//! profiles) and every moved key leaves it. `Hostname` and `Port` move from each
//! instance's server profile into its record, as the effective values, and
//! `ServerProfile` leaves the record. There is no answer for `assignments.ini`
//! and `models.ini.disabled`: the new shape has no place for them, which is
//! their being gone.
//!
//! Idempotent: run over texts that are already migrated it answers them
//! unchanged, so the manager's start runs it every time.

use crate::{assign, checks, ini, instance_profiles, server_cfg};
use std::collections::BTreeMap;

/// The instance profile the migration makes for the models no instance's
/// render holds, of the only server profile they could have been set against.
const UNASSIGNED: &str = "unassigned";

/// The files as they are now. `instance_profiles` is `instance-profiles.ini` as
/// it stands: a crash between the profile write and the record write leaves it
/// written, and the next run finishes the job from there.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct OldFiles {
    /// `models.ini`: the pool's switched-on profiles
    pub models: String,
    /// `models.ini.disabled`: the switched-off ones
    pub models_disabled: String,
    /// `instance-profiles.ini` as it stands, empty when the file is missing
    pub instance_profiles: String,
    /// `assignments.ini`: one section per instance, one key per model profile
    pub assignments: String,
    /// `instances.ini`: the record of the instances
    pub instances: String,
    /// every `server@<id>.ini`, by its profile id
    pub server_profiles: BTreeMap<String, String>,
}

/// The files the migration answers.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct NewFiles {
    /// `models.ini`: the merged pool, every moved key out of it
    pub models: String,
    /// `instance-profiles.ini`: the profiles, and their entries
    pub instance_profiles: String,
    /// `instances.ini`: each record naming its instance profile, with the
    /// address it listens on
    pub instances: String,
    /// every `server@<id>.ini`, with the address out of it
    pub server_profiles: BTreeMap<String, String>,
}

/// The migration, over the texts. Run it on the result: it answers that
/// unchanged. The caller writes the files it answers in this order:
/// `instance-profiles.ini`, `instances.ini`, `models.ini`, every
/// `server@<id>.ini`, and only then deletes `assignments.ini` and
/// `models.ini.disabled`. The address an instance listens on is in its
/// `server@<id>.ini` alone until the record carries it, so the record goes in
/// before the server profiles give it up. Each call is safe at every crash
/// point of that order: what a run finds written is what it reads through, and
/// the next run finishes the job.
pub fn migrate_model(old: &OldFiles) -> NewFiles {
    let merged = merge_pool(&old.models, &old.models_disabled);
    let merged = strip_placement_keys(&merged);
    let migrated = record_is_migrated(&old.instances);
    NewFiles {
        models: merged,
        instance_profiles: if migrated {
            old.instance_profiles.clone()
        } else {
            instance_profile_file(old)
        },
        instances: if migrated {
            old.instances.clone()
        } else {
            record_file(old)
        },
        server_profiles: old
            .server_profiles
            .iter()
            .map(|(id, text)| {
                let mut text = text.clone();
                for key in ["Hostname", "Port"] {
                    text = ini::text::remove_key(&text, "Server", key);
                }
                (id.clone(), text)
            })
            .collect(),
    }
}

/// The two pool files as one: every section of the switched-off file the
/// switched-on one does not already hold, appended after it in the pool's own
/// line-ending style, so a run over a pool that is already merged answers it
/// byte for byte and no section is copied twice. A file with no section adds
/// nothing, and a section the pool holds is left as the pool has it: the first
/// copy is the one the render reads.
fn merge_pool(on: &str, off: &str) -> String {
    // The ending style of the file that holds text: an empty `models.ini`
    // takes the style of the file it merges from.
    let eol = if on.is_empty() {
        ini::detect_eol(off)
    } else {
        ini::detect_eol(on)
    };
    let mut out = on.to_string();
    let mut body = String::new();
    for section in ini::parse_sections(off) {
        if ini::text::section_text(&out, &section.id).is_some() {
            continue;
        }
        let Some(text) = ini::text::section_text(off, &section.id) else {
            continue;
        };
        let text = text.trim_end_matches(['\r', '\n']);
        if text.is_empty() {
            continue;
        }
        if !body.is_empty() {
            body.push_str(eol);
        }
        body.push_str(&ini::normalize_eol(text, eol));
    }
    if body.is_empty() {
        return out;
    }
    out = out.trim_end_matches(['\r', '\n']).to_string();
    if !out.is_empty() {
        out.push_str(eol);
        out.push_str(eol);
    }
    out.push_str(&body);
    out.push_str(eol);
    out
}

/// Every placement key out of every model profile section of the pool. A
/// section that holds none is left byte for byte, so a run over a pool that is
/// already stripped answers it unchanged.
fn strip_placement_keys(pool: &str) -> String {
    let mut out = pool.to_string();
    for section in ini::parse_sections(&out) {
        if section.id == "*" {
            continue;
        }
        for key in section.keys.keys() {
            if checks::placement_key(key) {
                out = ini::text::remove_key(&out, &section.id, key);
            }
        }
    }
    out
}

/// True when the record is in the new shape: every section of it names its
/// instance profile, with the `InstanceProfile` key the migration wrote in
/// place of `ServerProfile`. A record that carries an address and names no
/// profile is an OLD record and gets migrated; from a record that names its
/// profile the record answers unchanged, and so do the profiles it points at.
fn record_is_migrated(instances: &str) -> bool {
    ini::parse_sections(instances)
        .iter()
        .all(|s| s.keys.contains_key("InstanceProfile"))
}

/// True when the profiles file already has the profile `id`: its own section,
/// not an entry of it.
fn profile_is_written(text: &str, id: &str) -> bool {
    ini::text::section_text(text, id).is_some()
}

/// The instance profile's own keys, as the record of the instance `id` has
/// them: a profile of the instance's own id, naming the instance's server
/// profile.
///
/// The file as it stands is the base, and a profile it already has is left as
/// it is, entries and keys and all: the file is written whole in one rename, so
/// the profiles on disk are the list the earlier run computed, with the keys
/// that moved out of the pool. The run that finishes the job reads a pool that
/// has already lost those keys, so rewriting from it would empty the entries.
fn instance_profile_file(old: &OldFiles) -> String {
    let mut out = old.instance_profiles.clone();
    let mut held: Vec<String> = Vec::new();
    for section in ini::parse_sections(&old.instances) {
        let assignment = ini::text::section_text(&old.assignments, &section.id);
        let ids = rendered_ids(old, assignment.as_deref());
        // Every instance's render counts, written or not: an orphan is a model
        // no instance's render holds.
        for id in &ids {
            held.push(id.to_lowercase());
        }
        if profile_is_written(&old.instance_profiles, &section.id) {
            continue;
        }
        let server = section
            .keys
            .get("ServerProfile")
            .map(|s| s.trim().to_string())
            .unwrap_or_default();
        out = instance_profiles::add_profile(&out, &section.id, &server);
        for id in ids {
            out = instance_profiles::add_entry(&out, &section.id, &id);
            out = instance_profiles::set_entry_keys(
                &out,
                &section.id,
                &id,
                &moved_keys(&old.models, &old.models_disabled, &id),
            );
        }
    }
    // The models no instance's render holds, with the placement keys that fit
    // no instance profile: one profile of their own, of the only server profile
    // they could have been set against.
    let mut orphans: Vec<String> = Vec::new();
    for section in ini::parse_sections(&old.models) {
        if section.id != "*" && !held.contains(&section.id.to_lowercase()) {
            orphans.push(section.id);
        }
    }
    for section in ini::parse_sections(&old.models_disabled) {
        if !held.contains(&section.id.to_lowercase()) {
            orphans.push(section.id);
        }
    }
    let orphans: Vec<String> = orphans
        .into_iter()
        .filter(|id| !moved_keys(&old.models, &old.models_disabled, id).is_empty())
        .collect();
    if !orphans.is_empty() && !profile_is_written(&old.instance_profiles, UNASSIGNED) {
        out = instance_profiles::add_profile(&out, UNASSIGNED, "default");
        for id in orphans {
            out = instance_profiles::add_entry(&out, UNASSIGNED, &id);
            out = instance_profiles::set_entry_keys(
                &out,
                UNASSIGNED,
                &id,
                &moved_keys(&old.models, &old.models_disabled, &id),
            );
        }
    }
    out
}

/// The instance profile of an instance is the instance's own id: the record
/// names it, and carries the address its server profile listens on.
///
/// The record is rewritten, not re-rendered: its comments and the order its
/// keys sit in stay where they are, `ServerProfile` becomes `InstanceProfile`
/// with the section's own id, and the address is added only where the section
/// does not carry one.
fn record_file(old: &OldFiles) -> String {
    let mut out = old.instances.clone();
    for section in ini::parse_sections(&old.instances) {
        let id = section.id.as_str();
        let server = section
            .keys
            .get("ServerProfile")
            .map(|s| s.trim().to_string())
            .unwrap_or_default();
        out = ini::text::remove_key(&out, id, "ServerProfile");
        out = ini::text::set_key(&out, id, "InstanceProfile", id);
        // The address the instance's server profile listens on, as the
        // effective values: a profile leaving `Port` unset gives the default. A
        // profile naming NEITHER key has already had both lines moved out by an
        // earlier run, and the address sits nowhere in these inputs: the record
        // keeps what it carries and gets no made-up address.
        if section.keys.contains_key("Hostname") || section.keys.contains_key("Port") {
            continue;
        }
        let cfg = old
            .server_profiles
            .get(&server)
            .map(|text| server_cfg::from_text(text));
        if let Some(cfg) = cfg.filter(|cfg| cfg.hostname.is_some() || cfg.port.is_some()) {
            out = ini::text::set_key(&out, id, "Hostname", &cfg.hostname_or_default());
            out = ini::text::set_key(&out, id, "Port", &cfg.port_or_default().to_string());
        }
    }
    out
}

/// The model profile ids the instance's render holds, in the render's order.
fn rendered_ids(old: &OldFiles, assignment: Option<&str>) -> Vec<String> {
    ini::parse_sections(&assign::render(
        &old.models,
        &old.models_disabled,
        assignment,
    ))
    .into_iter()
    .map(|section| section.id)
    .filter(|id| id != "*")
    .collect()
}

/// The placement keys of the model profile `id`, from whichever pool file
/// holds it, in the order its section has them.
fn moved_keys(on: &str, off: &str, id: &str) -> Vec<(String, String)> {
    for pool in [on, off] {
        let keys = ini::section_keys(pool, id);
        if !keys.is_empty() {
            return keys
                .into_iter()
                .filter(|(key, _)| checks::placement_key(key))
                .collect();
        }
    }
    Vec::new()
}

#[cfg(test)]
#[path = "migrate_model_tests.rs"]
mod tests;
