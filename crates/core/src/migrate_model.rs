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
mod tests {
    use super::{migrate_model, OldFiles};
    use crate::{assign, checks, ini, instance_profiles};
    use std::collections::BTreeMap;

    /// The fixture's instance ids.
    const INSTANCES: [&str; 2] = ["default", "lab"];

    /// `models.ini`: `alpha` with its keys interleaved (the moved ones sitting
    /// between the ones that stay, so the render's key ORDER changes and only
    /// the key and value SET is owed), and `beta` with nothing that moves. The
    /// global section carries a placement key of its own: `[*]` is exempt from
    /// the strip, the render copies it into every instance's file.
    fn pool_on() -> String {
        [
            "; models.ini, the pool's switched-on profiles",
            "[*]",
            "temp = 0.7",
            "n-gpu-layers = 99",
            "",
            "[alpha]",
            "; the alpha profile",
            "model = /models/alpha.gguf",
            "ctx-size = 8192",
            "device = cuda0",
            "n-gpu-layers = 99",
            "load-on-startup = true",
            "cache-type-k = f16",
            "cache-type-v = q8_0",
            "temp = 0.2",
            "",
            "[beta]",
            "model = /models/beta.gguf",
            "temp = 1.0",
            "",
        ]
        .join("\n")
    }

    /// `models.ini.disabled`: `gamma`, switched off, held by `lab` through an
    /// explicit `on`, and `delta`, switched off and held by nobody.
    fn pool_off() -> String {
        [
            "; models.ini.disabled, the switched-off profiles",
            "[gamma]",
            "; the gamma profile, switched off",
            "model = /models/gamma.gguf",
            "tensor-split = 1,1",
            "moe-cache-mib = 512",
            "",
            "[delta]",
            "model = /models/delta.gguf",
            "n-cpu-moe = 8",
            "device = cpu",
            "",
        ]
        .join("\n")
    }

    /// `assignments.ini`: `default` takes the pool by `* = inherit` and turns
    /// `beta` off; `lab` names three profiles, so `beta` reaches it with
    /// nothing that moves, `gamma` from the switched-off file, and `delta`
    /// reaches nobody.
    fn assignments() -> String {
        [
            "[default]",
            "* = inherit",
            "beta = off",
            "",
            "[lab]",
            "alpha = on",
            "beta = on",
            "gamma = on",
            "",
        ]
        .join("\n")
    }

    /// `instances.ini`: `default` with a label, `lab` with nothing beside its
    /// profile.
    fn instances() -> String {
        [
            "[default]",
            "ServerProfile = default",
            "Label = primary",
            "",
            "[lab]",
            "ServerProfile = lab",
            "",
        ]
        .join("\n")
    }

    /// The server profiles: `default` with an address, `lab` with a hostname
    /// and no `Port` (the effective one is `server_cfg::DEFAULT_PORT`).
    fn servers() -> BTreeMap<String, String> {
        let mut out = BTreeMap::new();
        out.insert(
            "default".to_string(),
            [
                "[Server]",
                "Hostname = 127.0.0.1",
                "Port = 8581",
                "ModelsMax = 4",
                "Devices = cuda0, cuda1",
                "",
            ]
            .join("\n"),
        );
        out.insert(
            "lab".to_string(),
            ["[Server]", "Hostname = 0.0.0.0", "ModelsMax = 2", ""].join("\n"),
        );
        out
    }

    /// The fixture: nothing migrated yet.
    fn fixture() -> OldFiles {
        OldFiles {
            models: pool_on(),
            models_disabled: pool_off(),
            instance_profiles: String::new(),
            assignments: assignments(),
            instances: instances(),
            server_profiles: servers(),
        }
    }

    /// The same files with the answer in them, AS THE MANAGER FINDS THEM after
    /// a crash between the writes: the merged `models.ini` is on disk and the
    /// file it was merged FROM, `models.ini.disabled`, is still there beside
    /// it, and `assignments.ini` is still there. The rerun must answer the same
    /// files, so it faces those leftovers, not a cleaned-up tree.
    fn migrated_again(new: &super::NewFiles) -> OldFiles {
        OldFiles {
            models: new.models.clone(),
            models_disabled: pool_off(),
            instance_profiles: new.instance_profiles.clone(),
            assignments: assignments(),
            instances: new.instances.clone(),
            server_profiles: new.server_profiles.clone(),
        }
    }

    /// A text as sections: the ids, in the file's order.
    fn ids(text: &str) -> Vec<String> {
        ini::parse_sections(text)
            .into_iter()
            .map(|s| s.id)
            .collect()
    }

    /// A text as sections: the id and the keys and values the reader reads.
    fn sections(text: &str) -> Vec<(String, BTreeMap<String, String>)> {
        ini::parse_sections(text)
            .into_iter()
            .map(|s| (s.id, s.keys))
            .collect()
    }

    /// The section `id` of `text`, as `assign::render` takes an assignment
    /// section (its header line included).
    fn section_of(text: &str, id: &str) -> Option<String> {
        ini::text::section_text(text, id).map(|s| s.trim_end().to_string())
    }

    /// A map of pairs, for a readable compare with a section's keys.
    fn map(pairs: &[(&str, &str)]) -> BTreeMap<String, String> {
        pairs
            .iter()
            .map(|(k, v)| (k.to_string(), v.to_string()))
            .collect()
    }

    /// The migrated `instance-profiles.ini` read as profiles.
    fn profiles(text: &str) -> Vec<instance_profiles::Profile> {
        instance_profiles::parse(text)
    }

    /// The profile `id` of the text, or None.
    fn profile(text: &str, id: &str) -> Option<instance_profiles::Profile> {
        profiles(text).into_iter().find(|p| p.id == id)
    }

    /// The entry of `id` for the model `model`, or None.
    fn entry(text: &str, id: &str, model: &str) -> Option<Vec<(String, String)>> {
        profile(text, id)
            .and_then(|p| p.entries.into_iter().find(|e| e.model == model))
            .map(|e| e.keys)
    }

    /// The instance profile the record says the instance `id` names.
    fn profile_of_record(instances: &str, id: &str) -> Option<String> {
        ini::parse_sections(instances)
            .into_iter()
            .find(|s| s.id == id)
            .and_then(|s| s.keys.get("InstanceProfile").cloned())
    }

    /// `ip1` criterion: one instance profile per instance, its `ServerProfile`
    /// the instance's, one entry per model that instance's current render
    /// holds, holding the model's moved keys; a model two instances hold is
    /// copied into both entries; a model no render holds goes to `unassigned`,
    /// whose server profile is `default`; no profile section carries a `Models`
    /// key.
    #[test]
    fn ip1_the_migration_writes_one_instance_profile_per_instance_with_the_moved_keys() {
        let new = migrate_model(&fixture());
        let mut ids: Vec<String> = profiles(&new.instance_profiles)
            .iter()
            .map(|p| p.id.clone())
            .collect();
        ids.sort();
        assert_eq!(
            ids,
            ["default", "lab", "unassigned"],
            "the instance profiles are not these: {:?}",
            new.instance_profiles
        );
        assert_eq!(
            profile(&new.instance_profiles, "default")
                .and_then(|p| p.server_profile)
                .as_deref(),
            Some("default"),
            "[default]'s ServerProfile is not the instance's: {:?}",
            new.instance_profiles
        );
        assert_eq!(
            profile(&new.instance_profiles, "lab")
                .and_then(|p| p.server_profile)
                .as_deref(),
            Some("lab"),
            "[lab]'s ServerProfile is not the instance's: {:?}",
            new.instance_profiles
        );
        assert_eq!(
            profile(&new.instance_profiles, "unassigned")
                .and_then(|p| p.server_profile)
                .as_deref(),
            Some("default"),
            "[unassigned]'s ServerProfile is not the only server profile a switched-off \
             model could have been set against: {:?}",
            new.instance_profiles
        );
        let alpha = map(&[
            ("ctx-size", "8192"),
            ("device", "cuda0"),
            ("n-gpu-layers", "99"),
            ("load-on-startup", "true"),
            ("cache-type-k", "f16"),
            ("cache-type-v", "q8_0"),
        ]);
        let as_map = |keys: &Option<Vec<(String, String)>>| -> BTreeMap<String, String> {
            keys.iter().flat_map(|v| v.clone()).collect()
        };
        assert_eq!(
            as_map(&entry(&new.instance_profiles, "default", "alpha")),
            alpha,
            "[default/alpha] does not hold the moved keys of alpha's section"
        );
        assert_eq!(
            as_map(&entry(&new.instance_profiles, "lab", "alpha")),
            alpha,
            "[lab/alpha] does not hold a COPY of the moved keys (a model two instances \
             hold is in both entries)"
        );
        assert_eq!(
            entry(&new.instance_profiles, "lab", "beta").map(|k| k.len()),
            Some(0),
            "[lab/beta] is not an entry of no keys (beta reaches lab, and holds nothing \
             that moves): {:?}",
            entry(&new.instance_profiles, "lab", "beta")
        );
        assert_eq!(
            as_map(&entry(&new.instance_profiles, "lab", "gamma")),
            map(&[("tensor-split", "1,1"), ("moe-cache-mib", "512")]),
            "[lab/gamma] does not hold gamma's moved keys (gamma reaches lab from the \
             switched-off file)"
        );
        assert_eq!(
            as_map(&entry(&new.instance_profiles, "unassigned", "delta")),
            map(&[("n-cpu-moe", "8"), ("device", "cpu")]),
            "[unassigned/delta] does not hold the moved keys of the model no render holds"
        );
        assert_eq!(
            profile(&new.instance_profiles, "default")
                .map(|p| p.entries.len())
                .unwrap_or_default(),
            1,
            "[default] holds an entry for a model its render does not hold: {:?}",
            new.instance_profiles
        );
        let mut lab: Vec<String> = profile(&new.instance_profiles, "lab")
            .map(|p| p.entries.iter().map(|e| e.model.clone()).collect())
            .unwrap_or_default();
        lab.sort();
        assert_eq!(
            lab,
            ["alpha", "beta", "gamma"],
            "[lab]'s entries are not these: {:?}",
            new.instance_profiles
        );
        // The entry sections are the list: the `Models` key the design first
        // proposed is written nowhere.
        for s in ini::parse_sections(&new.instance_profiles) {
            assert!(
                !s.keys.contains_key("Models"),
                "the section [{}] carries a Models key: {:?}",
                s.id,
                s.keys
            );
        }
    }

    /// `ip1` criterion: the pool is merged, the switched-off profiles after the
    /// switched-on ones, and every moved key leaves every model profile
    /// section, the keys that stay kept.
    #[test]
    fn ip1_the_migration_merges_the_pool_and_leaves_the_moved_keys_out_of_it() {
        let new = migrate_model(&fixture());
        assert_eq!(
            ids(&new.models),
            ["*", "alpha", "beta", "gamma", "delta"],
            "the merged pool is not these sections, in this order: {:?}",
            new.models
        );
        let sections = sections(&new.models);
        let keys_of = |id: &str| -> BTreeMap<String, String> {
            sections
                .iter()
                .find(|(s, _)| s == id)
                .map(|(_, k)| k.clone())
                .unwrap_or_default()
        };
        assert_eq!(
            keys_of("alpha"),
            map(&[("model", "/models/alpha.gguf"), ("temp", "0.2")]),
            "alpha's section is not the keys that stay (a moved key is left in it?)"
        );
        assert_eq!(
            keys_of("beta"),
            map(&[("model", "/models/beta.gguf"), ("temp", "1.0")]),
            "beta's section is not the keys it had"
        );
        assert_eq!(
            keys_of("gamma"),
            map(&[("model", "/models/gamma.gguf")]),
            "gamma's section is not the keys that stay"
        );
        assert_eq!(
            keys_of("delta"),
            map(&[("model", "/models/delta.gguf")]),
            "delta's section is not the keys that stay"
        );
        // The global section keeps its placement key: the strip leaves `[*]`
        // alone (the render copies it into every instance's file, where the
        // key belongs), and the placement check leaves it alone too.
        assert_eq!(
            keys_of("*"),
            map(&[("temp", "0.7"), ("n-gpu-layers", "99")]),
            "the pool's global section [*] is not kept as it was ([*] is exempt from \
             the strip): {:?}",
            new.models
        );
        assert_eq!(
            checks::model_profile_placement(&new.models),
            Ok(()),
            "the migrated pool is refused by the model profile placement check"
        );
    }

    /// `ip1` criterion: for every instance, the render of the new files parses
    /// to the same keys and values per model section as the render of the old
    /// ones, in the same section order.
    #[test]
    fn ip1_the_migration_gives_every_instance_the_render_it_had() {
        let old = fixture();
        let new = migrate_model(&old);
        for id in INSTANCES {
            let before = assign::render(
                &old.models,
                &old.models_disabled,
                section_of(&old.assignments, id).as_deref(),
            );
            let profile = profile_of_record(&new.instances, id)
                .unwrap_or_else(|| panic!("[{id}] names no instance profile: {:?}", new.instances));
            let after = assign::render_profile(&new.models, &new.instance_profiles, &profile);
            let (want, got) = (sections(&before), sections(&after));
            assert_eq!(
                want.iter().map(|(s, _)| s.clone()).collect::<Vec<_>>(),
                got.iter().map(|(s, _)| s.clone()).collect::<Vec<_>>(),
                "instance {id}: the rendered sections are not the same, in the same order\n\
                 before:\n{before}\nafter:\n{after}"
            );
            for (id_want, keys_want) in &want {
                let keys_got = got
                    .iter()
                    .find(|(s, _)| s == id_want)
                    .map(|(_, k)| k.clone())
                    .unwrap_or_default();
                assert_eq!(
                    keys_want, &keys_got,
                    "instance {id}: the section [{id_want}] does not parse to the same keys \
                     and values\nbefore:\n{before}\nafter:\n{after}"
                );
            }
        }
    }

    /// `ip1` criterion: the address moves into the record as the effective
    /// value (a server profile leaving `Port` unset gives 9931), `Hostname`
    /// with it, `ServerProfile` out, and the record's other keys kept.
    #[test]
    fn ip1_the_migration_moves_the_effective_address_into_the_record() {
        let new = migrate_model(&fixture());
        let record = sections(&new.instances);
        let keys_of = |id: &str| -> BTreeMap<String, String> {
            record
                .iter()
                .find(|(s, _)| s == id)
                .map(|(_, k)| k.clone())
                .unwrap_or_default()
        };
        assert_eq!(
            keys_of("default"),
            map(&[
                ("InstanceProfile", "default"),
                ("Label", "primary"),
                ("Hostname", "127.0.0.1"),
                ("Port", "8581"),
            ]),
            "the record of default is not its instance profile, its label and its address: \
             {:?}",
            new.instances
        );
        assert_eq!(
            keys_of("lab"),
            map(&[
                ("InstanceProfile", "lab"),
                ("Hostname", "0.0.0.0"),
                ("Port", "9931"),
            ]),
            "a server profile leaving Port unset gives the record the effective port \
             (server_cfg::DEFAULT_PORT): {:?}",
            new.instances
        );
        // The server profiles lose the address and keep the rest.
        let servers = &new.server_profiles;
        assert_eq!(
            servers.keys().cloned().collect::<Vec<_>>(),
            ["default", "lab"],
            "a server profile of the fixture is not answered"
        );
        assert_eq!(
            sections(
                servers
                    .get("default")
                    .map(String::as_str)
                    .unwrap_or_default()
            ),
            vec![(
                "Server".to_string(),
                map(&[("ModelsMax", "4"), ("Devices", "cuda0, cuda1")])
            )],
            "server@default.ini is not its own keys with the address out of it"
        );
        assert_eq!(
            sections(servers.get("lab").map(String::as_str).unwrap_or_default()),
            vec![("Server".to_string(), map(&[("ModelsMax", "2")]))],
            "server@lab.ini is not its own keys with the address out of it"
        );
    }

    /// `ip1` criterion (review D3, D2b): a second run over the files the first
    /// one answered, as they sit on disk with the leftovers of a crash between
    /// the writes (`models.ini` merged and stripped, `models.ini.disabled`
    /// still there, `assignments.ini` still there): it changes nothing, and in
    /// particular it does not merge the switched-off file in a second time, so
    /// no section is duplicated.
    #[test]
    fn ip1_the_migration_a_second_run_changes_nothing() {
        let new = migrate_model(&fixture());
        let again = migrate_model(&migrated_again(&new));
        assert_eq!(
            ids(&again.models),
            ids(&new.models),
            "the rerun merged the models.ini.disabled that is still on disk into the \
             merged models.ini, duplicating its sections: {:?}",
            again.models
        );
        assert_eq!(
            again.models, new.models,
            "the second run rewrote the pool (the switched-off sections merged in twice?)"
        );
        assert_eq!(
            again.instance_profiles, new.instance_profiles,
            "the second run rewrote the instance profiles (an entry duplicated?)"
        );
        assert_eq!(
            again.instances, new.instances,
            "the second run rewrote the record"
        );
        assert_eq!(
            again.server_profiles, new.server_profiles,
            "the second run rewrote a server profile"
        );
    }

    /// `ip1` criterion: a crash point between the profile write and the record
    /// write: the profiles are on disk with their entries, the record and the
    /// pool are still the old ones. The next run completes to the same result.
    #[test]
    fn ip1_the_migration_completes_a_crash_point_where_the_record_is_not_written() {
        let new = migrate_model(&fixture());
        let crash = OldFiles {
            models: pool_on(),
            models_disabled: pool_off(),
            instance_profiles: new.instance_profiles.clone(),
            assignments: assignments(),
            instances: instances(),
            server_profiles: servers(),
        };
        let done = migrate_model(&crash);
        let as_map = |keys: &[(String, String)]| -> BTreeMap<String, String> {
            keys.iter().cloned().collect()
        };
        assert_eq!(
            as_map(
                entry(&done.instance_profiles, "default", "alpha")
                    .unwrap_or_default()
                    .as_slice()
            ),
            map(&[
                ("ctx-size", "8192"),
                ("device", "cuda0"),
                ("n-gpu-layers", "99"),
                ("load-on-startup", "true"),
                ("cache-type-k", "f16"),
                ("cache-type-v", "q8_0"),
            ]),
            "the run that finished the job left [default/alpha] without the moved keys: {:?}",
            done.instance_profiles
        );
        assert_eq!(
            profile_of_record(&done.instances, "lab").as_deref(),
            Some("lab"),
            "the run that finished the job left [lab] naming no instance profile: {:?}",
            done.instances
        );
        assert_eq!(
            sections(&done.instances)
                .iter()
                .find(|(s, _)| s == "lab")
                .map(|(_, k)| k.get("Port").cloned())
                .unwrap_or_default(),
            Some("9931".to_string()),
            "the run that finished the job left [lab] without the effective port: {:?}",
            done.instances
        );
        assert_eq!(
            checks::model_profile_placement(&done.models),
            Ok(()),
            "the run that finished the job left a moved key in the pool"
        );
        assert_eq!(
            done.instance_profiles, new.instance_profiles,
            "the run that finished the job did not answer the profiles the clean run answers"
        );
        assert_eq!(
            done.models, new.models,
            "the run that finished the job did not answer the pool the clean run answers"
        );
        assert_eq!(
            done.instances, new.instances,
            "the run that finished the job did not answer the record the clean run answers"
        );
        assert_eq!(
            done.server_profiles, new.server_profiles,
            "the run that finished the job did not answer the server profiles the clean \
             run answers"
        );
    }

    /// The entry of `id` for the model `model`, as a map, for a readable compare.
    fn entry_map(text: &str, id: &str, model: &str) -> BTreeMap<String, String> {
        entry(text, id, model)
            .unwrap_or_default()
            .into_iter()
            .collect()
    }

    /// The `Port` the record gives the instance `id`.
    fn port_of_record(instances: &str, id: &str) -> Option<String> {
        sections(instances)
            .into_iter()
            .find(|(s, _)| s == id)
            .and_then(|(_, k)| k.get("Port").cloned())
    }

    /// The keys the fixture's `alpha` moves (the ones interleaved in its
    /// section, the K and V cache types of it).
    fn alpha_moved() -> BTreeMap<String, String> {
        map(&[
            ("ctx-size", "8192"),
            ("device", "cuda0"),
            ("n-gpu-layers", "99"),
            ("load-on-startup", "true"),
            ("cache-type-k", "f16"),
            ("cache-type-v", "q8_0"),
        ])
    }

    /// `ip1` criterion (review D2a): a crash point further on than the one the
    /// first test takes: `instance-profiles.ini` is written with its entries,
    /// `models.ini` is the merged pool already stripped of the moved keys, the
    /// server profiles are untouched, and `instances.ini` is still the old
    /// record. The run that finishes the job keeps the entries that are
    /// already written (the stripped pool can no longer supply their keys: an
    /// existing entry wins), and answers the clean run's files.
    #[test]
    fn ip1_the_migration_completes_a_crash_point_where_the_pool_is_already_stripped() {
        let new = migrate_model(&fixture());
        let crash = OldFiles {
            models: new.models.clone(),
            models_disabled: String::new(),
            instance_profiles: new.instance_profiles.clone(),
            assignments: assignments(),
            instances: instances(),
            server_profiles: servers(),
        };
        let done = migrate_model(&crash);
        assert_eq!(
            entry_map(&done.instance_profiles, "default", "alpha"),
            alpha_moved(),
            "the run that finished the job rebuilt the profile file from the stripped pool, \
             so the entry [default/alpha] it had already written lost its keys: {:?}",
            done.instance_profiles
        );
        assert_eq!(
            done.instance_profiles, new.instance_profiles,
            "the run that finished the job did not answer the profiles the clean run answers \
             (an entry already on disk must win over the stripped pool)"
        );
        assert_eq!(
            done.models, new.models,
            "the run that finished the job did not answer the pool the clean run answers"
        );
        assert_eq!(
            done.instances, new.instances,
            "the run that finished the job did not answer the record the clean run answers"
        );
        assert_eq!(
            done.server_profiles, new.server_profiles,
            "the run that finished the job did not answer the server profiles the clean \
             run answers"
        );
    }

    /// `ip1` criterion (review D2a): the crash point where the server profiles
    /// have already lost `Hostname` and `Port` while `instances.ini` is still
    /// the old record, the pool is stripped and the profiles are written. The
    /// entries already on disk win, and the run does not invent an address:
    /// a server profile whose `Port` line the migration already moved out
    /// gives the record no 9931, the fallback of a profile that never had one.
    ///
    /// The record comes back no byte for byte the clean run's HERE, because this
    /// input leaves the address nowhere: it is the strip of the server profiles
    /// happening BEFORE the record is written, an order the manager does not
    /// follow (the record is its second write, the server profiles lose the
    /// address at the fourth). The record pins are the entry keys, and that no
    /// address is made up. At the crash points the fixed order can leave on disk
    /// the server profiles still carry the address, and there the record does
    /// come back byte for byte (the test below).
    #[test]
    fn ip1_the_migration_completes_a_crash_point_where_the_address_is_already_stripped() {
        let new = migrate_model(&fixture());
        let crash = OldFiles {
            models: new.models.clone(),
            models_disabled: String::new(),
            instance_profiles: new.instance_profiles.clone(),
            assignments: assignments(),
            instances: instances(),
            server_profiles: new.server_profiles.clone(),
        };
        let done = migrate_model(&crash);
        assert_eq!(
            entry_map(&done.instance_profiles, "default", "alpha"),
            alpha_moved(),
            "the run that finished the job left the entry [default/alpha] of the profiles \
             file that was already written without its keys: {:?}",
            done.instance_profiles
        );
        assert_eq!(
            done.instance_profiles, new.instance_profiles,
            "the run that finished the job rewrote the profiles that were already written"
        );
        assert_eq!(
            done.models, new.models,
            "the run that finished the job did not answer the pool the clean run answers"
        );
        assert_eq!(
            done.server_profiles, new.server_profiles,
            "the run that finished the job rewrote a server profile that was already stripped"
        );
        assert_eq!(
            profile_of_record(&done.instances, "default").as_deref(),
            Some("default"),
            "the run that finished the job left [default] naming no instance profile: {:?}",
            done.instances
        );
        assert_ne!(
            port_of_record(&done.instances, "default").as_deref(),
            Some("9931"),
            "the run that finished the job gave [default] the 9931 fallback of a server \
             profile whose Port line the migration had already moved out: {:?}",
            done.instances
        );
        let record = sections(&done.instances)
            .into_iter()
            .find(|(s, _)| s == "default")
            .map(|(_, k)| k)
            .unwrap_or_default();
        assert!(
            !(record.contains_key("Hostname") && record.contains_key("Port")),
            "the run that finished the job wrote an address into [default] that sits \
             nowhere in these inputs: the server profile had already lost both of its lines: \
             {:?}",
            done.instances
        );
    }

    /// `ip1` criterion (the write order): the crash point right after the
    /// record is written. The manager writes `instance-profiles.ini`, then
    /// `instances.ini`, then the stripped `models.ini`, then the stripped
    /// `server@<id>.ini`, then deletes `assignments.ini` and
    /// `models.ini.disabled`: so on disk sit the new profiles and the new
    /// record (with its address and its `InstanceProfile`), the pool the strip
    /// has not touched, the server profiles still carrying the address, and the
    /// two files the last step removes. The run that finishes the job answers
    /// the record and the profiles unchanged (the record names its profile, so
    /// the run takes it as migrated) and finishes the strip of the pool and of
    /// the server profiles. The record comes back byte for byte: the address it
    /// carries sits in the server profile this crash point has not stripped,
    /// and in the record itself.
    #[test]
    fn ip1_the_migration_completes_a_crash_point_where_the_record_is_written() {
        let new = migrate_model(&fixture());
        let crash = OldFiles {
            models: pool_on(),
            models_disabled: pool_off(),
            instance_profiles: new.instance_profiles.clone(),
            assignments: assignments(),
            instances: new.instances.clone(),
            server_profiles: servers(),
        };
        let done = migrate_model(&crash);
        assert_eq!(
            done.instances, new.instances,
            "the run that finished the job rewrote the record that was already written: {:?}",
            done.instances
        );
        assert_eq!(
            profile_of_record(&done.instances, "default").as_deref(),
            Some("default"),
            "the run that finished the job left [default] naming no instance profile: {:?}",
            done.instances
        );
        assert_eq!(
            port_of_record(&done.instances, "default").as_deref(),
            Some("8581"),
            "the run that finished the job lost the address of the record that was already \
             written: {:?}",
            done.instances
        );
        assert_eq!(
            done.instance_profiles, new.instance_profiles,
            "the run that finished the job rewrote the profiles that were already written: {:?}",
            done.instance_profiles
        );
        assert_eq!(
            checks::model_profile_placement(&done.models),
            Ok(()),
            "the run that finished the job left a moved key in the pool it had not stripped: {:?}",
            done.models
        );
        assert_eq!(
            done.models, new.models,
            "the run that finished the job did not answer the pool the clean run answers (the \
             models.ini.disabled still on disk merged in twice?): {:?}",
            done.models
        );
        assert_eq!(
            sections(
                done.server_profiles
                    .get("default")
                    .map(String::as_str)
                    .unwrap_or_default()
            ),
            vec![(
                "Server".to_string(),
                map(&[("ModelsMax", "4"), ("Devices", "cuda0, cuda1")])
            )],
            "the run that finished the job left the address in the server profile it had not \
             stripped: {:?}",
            done.server_profiles
        );
        assert_eq!(
            done.server_profiles, new.server_profiles,
            "the run that finished the job did not answer the server profiles the clean run \
             answers"
        );
    }

    /// `ip1` criterion (review D4): a record that carries an old `Hostname` and
    /// `Port` and names no server profile and no instance profile is an OLD
    /// record, not a migrated one: the run migrates it instead of answering it
    /// unchanged, so it names its instance profile, keeps the address it
    /// carries, and the profile pool is written beside it.
    #[test]
    fn ip1_a_record_with_an_address_and_no_profile_is_not_counted_migrated() {
        let old = OldFiles {
            instances: [
                "[default]",
                "Hostname = 127.0.0.1",
                "Port = 8581",
                "Label = primary",
                "",
            ]
            .join("\n"),
            ..fixture()
        };
        let new = migrate_model(&old);
        assert_eq!(
            sections(&new.instances)
                .into_iter()
                .find(|(s, _)| s == "default")
                .map(|(_, k)| k)
                .unwrap_or_default(),
            map(&[
                ("InstanceProfile", "default"),
                ("Label", "primary"),
                ("Hostname", "127.0.0.1"),
                ("Port", "8581"),
            ]),
            "a record carrying Hostname and Port and no ServerProfile and no InstanceProfile \
             was counted as migrated and answered unchanged: {:?}",
            new.instances
        );
        assert!(
            profile(&new.instance_profiles, "default").is_some(),
            "the profile pool was answered as it came (the record was counted as migrated): \
             {:?}",
            new.instance_profiles
        );
    }

    /// `ip1` criterion (review D4): the record is rewritten, not re-rendered:
    /// the file's comments, each section's own comment and the order the file
    /// has its keys in are kept; only `ServerProfile` becomes `InstanceProfile`
    /// and the address is added.
    #[test]
    fn ip1_the_record_keeps_the_comments_and_the_key_order() {
        let old = OldFiles {
            instances: [
                "; the record of the instances",
                "; pai-manager owns this file",
                "",
                "[default]",
                "; the primary instance",
                "Zebra = keep-me",
                "Label = primary",
                "ServerProfile = default",
                "",
            ]
            .join("\n"),
            ..fixture()
        };
        let new = migrate_model(&old);
        assert!(
            new.instances.contains("; the record of the instances"),
            "the record's file comments are gone: {:?}",
            new.instances
        );
        let body = ini::text::section_text(&new.instances, "default").unwrap_or_default();
        assert!(
            body.contains("; the primary instance"),
            "the section's own comment is gone: {:?}",
            new.instances
        );
        let at = |needle: &str| {
            body.find(needle)
                .unwrap_or_else(|| panic!("the record lost `{needle}`: {:?}", new.instances))
        };
        assert_eq!(
            profile_of_record(&new.instances, "default").as_deref(),
            Some("default"),
            "the record does not name its instance profile: {:?}",
            new.instances
        );
        assert!(
            !body.contains("ServerProfile"),
            "the record still names a server profile: {:?}",
            new.instances
        );
        assert!(
            at("Zebra = keep-me") < at("Label = primary"),
            "the record's keys are written in the reader's sorted order, not the order the \
             file has them: {:?}",
            new.instances
        );
    }
}
