//! Versioned benchmark specifications (`bench sweep --spec <file>`).
//!
//! A spec is the method of a measurement written down: which prompts, which
//! passes (temperature, repetitions, concurrency, the preset keys a pass
//! overrides for every leg), whether the output is capped. A sweep run under
//! one records the spec's `Name`, `Version` and its digest with every run
//! (`spec`, `spec-version`, `spec-sha256`, `spec-pass`), so two runs
//! are comparable by construction when those match, and a query can keep to
//! one version. A sweep without `--spec` records `spec = custom`: its method is
//! whatever its flags said, and it is never mistaken for a spec run.
//!
//! The file is INI, like the rest of the framework's configuration:
//!
//! ```ini
//! [Spec]
//! Name = standard
//! Version = 1
//! Prompts = bench-prompt-43k.txt@96d8fb4f..., bench-prompt-110k.txt@2623e49a...
//! MaxTokens = 0
//!
//! [Pass.engine]
//! Temp = 0
//! Override = spec-type=none
//! Concurrency = single
//! Reps = 3
//! ```
//!
//! A bare prompt name is a file in the config directory, seeded from the
//! binary when it is one of the shipped prompts; a path is used as is. Each
//! carries `@<sha256>` of its text (as `bench query prompts` shows it): the
//! prompts are part of the method, so a prompt that no longer matches is
//! refused before anything restarts, and changing one means a new spec
//! version pinning the new digest.
//! `Override` is a `;`-separated list of `key=value` (a value may hold commas),
//! each written to the preset for the pass and put back after it, with a
//! pending marker like a sweep's own key. `Concurrency = all` is single, then
//! every concurrent scenario when the preset has two slots or more.

use std::path::{Path, PathBuf};

use super::sweep::{self, Apply, Opts};
use super::{Concurrency, Plan};
use crate::ini;

/// What a run records about the spec it ran under.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Tag {
    pub name: String,
    pub version: String,
    /// sha256 of the spec's canonical form (`canonical`): a spec whose rules
    /// changed without a `Version` bump still reads as another method, while
    /// a comment or layout edit does not. Written to `<name>.versions` beside
    /// the spec, one line per published version.
    pub sha256: String,
    pub pass: String,
}

/// One `[Pass.<name>]` section.
#[derive(Clone, Debug, PartialEq)]
pub struct Pass {
    pub name: String,
    /// `None` = the preset's own temperature.
    pub temp: Option<f64>,
    pub reps: i32,
    /// `None` = `all`.
    pub concurrency: Option<Concurrency>,
    pub overrides: Vec<(String, String)>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Spec {
    pub name: String,
    pub version: String,
    pub sha256: String,
    /// Each prompt with the sha256 its text must have.
    pub prompts: Vec<(PathBuf, String)>,
    pub max_tokens: i32,
    pub passes: Vec<Pass>,
}

/// The framework's own method, compiled in from the repo's
/// `benchmarks/specs/standard.ini`: `--spec standard` needs no file.
pub const STANDARD: &str = include_str!("../../../benchmarks/specs/standard.ini");

/// `standard` = the compiled-in spec; anything else is a file.
pub fn load(arg: &str) -> Result<Spec, String> {
    if arg == "standard" {
        return parse(STANDARD, &crate::paths::config_dir())
        .map_err(|e| format!("the built-in standard spec: {e}"));
    }
    load_file(Path::new(arg))
}

fn load_file(path: &Path) -> Result<Spec, String> {
    let bytes = std::fs::read(path).map_err(|e| format!("cannot read {}: {e}", path.display()))?;
    let text = String::from_utf8_lossy(&bytes);
    parse(&text, &crate::paths::config_dir())
        .map_err(|e| format!("{}: {e}", path.display()))
}

/// Parse a spec's text; `config` resolves bare prompt names.
pub fn parse(text: &str, config: &Path) -> Result<Spec, String> {
    let sections = ini::parse_sections(text);
    let head = sections
        .iter()
        .find(|s| s.id == "Spec")
        .ok_or("no [Spec] section")?;
    let get = |k: &str| head.keys.get(k).map(|v| v.trim().to_string());
    let name = get("Name").filter(|v| !v.is_empty()).ok_or("[Spec] has no Name")?;
    if name == "custom" {
        return Err("`custom` is what a sweep without --spec records: pick another Name".into());
    }
    let version = get("Version").filter(|v| !v.is_empty()).ok_or("[Spec] has no Version")?;
    let mut prompts = Vec::new();
    for entry in get("Prompts").unwrap_or_default().split(',').map(str::trim) {
        if entry.is_empty() {
            continue;
        }
        let (p, sha) = entry
            .rsplit_once('@')
            .filter(|(_, s)| s.len() == 64 && s.chars().all(|c| c.is_ascii_hexdigit()))
            .ok_or_else(|| format!("Prompts `{entry}`: <file>@<sha256 of its text>"))?;
        let path = PathBuf::from(p.trim());
        let path = if path.components().count() == 1 {
            config.join(path)
        } else {
            path
        };
        prompts.push((path, sha.to_ascii_lowercase()));
    }
    if prompts.is_empty() {
        return Err("[Spec] has no Prompts".into());
    }
    let max_tokens = match get("MaxTokens") {
        None => 0,
        Some(v) => ini::parse_int_in(&v, &ini::INT_NON_NEGATIVE)
            .ok_or_else(|| format!("MaxTokens = {v}: 0 (no cap) or more"))?,
    };
    let mut passes = Vec::new();
    for s in &sections {
        let Some(pass) = s.id.strip_prefix("Pass.") else {
            continue;
        };
        let get = |k: &str| s.keys.get(k).map(|v| v.trim().to_string());
        let ctx = |e: String| format!("[{}] {e}", s.id);
        let temp = match get("Temp").as_deref() {
            None | Some("preset") => None,
            Some(v) => Some(
                ini::parse_float(v).ok_or_else(|| ctx(format!("Temp = {v}: a number or `preset`")))?,
            ),
        };
        let reps = match get("Reps") {
            None => super::DEFAULT_REPS,
            Some(v) => ini::parse_int_in(&v, &ini::INT_POSITIVE)
                .ok_or_else(|| ctx(format!("Reps = {v}: 1 or more")))?,
        };
        let concurrency = match get("Concurrency").as_deref() {
            None => Some(Concurrency::Single),
            Some("all") => None,
            Some(v) => Some(
                Concurrency::from_str(v)
                    .ok_or_else(|| ctx(format!("Concurrency = {v}: unknown scenario")))?,
            ),
        };
        let mut overrides = Vec::new();
        for o in get("Override").unwrap_or_default().split(';') {
            let o = o.trim();
            if o.is_empty() {
                continue;
            }
            let (k, v) = o
                .split_once('=')
                .ok_or_else(|| ctx(format!("Override `{o}`: key=value")))?;
            let (k, v) = (k.trim().to_string(), v.trim().to_string());
            sweep::setter(&k).map_err(|e| ctx(format!("Override `{k}`: {e}")))?;
            overrides.push((k, v));
        }
        passes.push(Pass {
            name: pass.to_string(),
            temp,
            reps,
            concurrency,
            overrides,
        });
    }
    if passes.is_empty() {
        return Err("no [Pass.<name>] section".into());
    }
    let mut spec = Spec {
        name,
        version,
        sha256: String::new(),
        prompts,
        max_tokens,
        passes,
    };
    spec.sha256 = crate::sha256::hex(canonical(&spec).as_bytes());
    Ok(spec)
}

/// The rules of a spec, one per line, comments and layout left out: what its
/// digest covers. A prompt counts by file name and pinned digest, not by the
/// directory it resolves to on this machine.
pub fn canonical(spec: &Spec) -> String {
    let mut out = format!(
        "name={}\nversion={}\nmax-tokens={}\n",
        spec.name, spec.version, spec.max_tokens
    );
    for (path, sha) in &spec.prompts {
        let file = path.file_name().unwrap_or_default().to_string_lossy();
        out.push_str(&format!("prompt={file}@{sha}\n"));
    }
    for p in &spec.passes {
        let overrides: Vec<String> = p.overrides.iter().map(|(k, v)| format!("{k}={v}")).collect();
        out.push_str(&format!(
            "pass={} temp={} reps={} concurrency={} override={}\n",
            p.name,
            p.temp.map_or("preset".to_string(), |t| t.to_string()),
            p.reps,
            p.concurrency.map_or("all", |c| c.as_str()),
            overrides.join(";")
        ));
    }
    out
}

/// The `<version> <sha256>` lines of a `.versions` file, comments and blanks
/// skipped. Read by the test that freezes the published versions.
#[cfg(test)]
pub fn published_versions(text: &str) -> Vec<(String, String)> {
    text.lines()
        .map(str::trim)
        .filter(|l| !l.is_empty() && !l.starts_with('#'))
        .filter_map(|l| {
            let mut it = l.split_whitespace();
            Some((it.next()?.to_string(), it.next()?.to_ascii_lowercase()))
        })
        .collect()
}

/// The sweep, once per prompt, pass and scenario, all into one campaign.
/// `base` carries the preset, key, values, apply, campaign and question; its
/// plan's method fields are replaced by the spec's.
pub fn run(spec: &Spec, base: &Opts) -> Result<String, String> {
    if base.plan.mode != super::Mode::Live {
        return Err("--spec runs the live engine".into());
    }
    if base.apply != Apply::Original && spec.passes.len() * spec.prompts.len() > 1 {
        return Err("--spec with several passes or prompts leaves the preset as it was: drop --apply".into());
    }
    // A pass's override is part of the method: a swept key that a pass also
    // sets would put the leg's value over the pass's (e.g. a drafter back on
    // in a temp-0 pass).
    if let Some(p) = spec.passes.iter().find(|p| p.overrides.iter().any(|(k, _)| *k == base.key)) {
        return Err(format!(
            "--key {} is set by pass `{}` of spec {}: sweep another key, or write a spec without that override",
            base.key, p.name, spec.name
        ));
    }
    let mut prompts = Vec::new();
    for (p, pinned) in &spec.prompts {
        super::ensure_prompt_file(p);
        let text = super::load_prompt_file(p)?;
        let actual = super::prompt_info(&text).sha256;
        if &actual != pinned {
            return Err(format!(
                "{} has sha256 {actual}, spec {} v{} pins {pinned}: the prompt is part of \
                 the method, so a changed prompt is a new spec version (restore the file, \
                 or bump Version and pin the new digest)",
                p.display(),
                spec.name,
                spec.version
            ));
        }
        prompts.push((p.clone(), text));
    }
    let campaign = if base.campaign.is_empty() {
        format!(
            "spec-{}-v{}-{}",
            spec.name,
            spec.version,
            super::stamp(super::now_secs())
        )
    } else {
        base.campaign.clone()
    };
    let mut problems = Vec::new();
    for (file, text) in &prompts {
        for pass in &spec.passes {
            let scenarios = match pass.concurrency {
                Some(c) => vec![c],
                None => sweep::all_scenarios(&base.preset)?,
            };
            let restore = apply_overrides(&base.preset, &pass.overrides)?;
            for c in scenarios {
                println!(
                    "\n=== spec {} v{} · pass {} · {} · concurrency {} ===",
                    spec.name,
                    spec.version,
                    pass.name,
                    file.file_name().unwrap_or_default().to_string_lossy(),
                    c.as_str()
                );
                let opts = Opts {
                    plan: Plan {
                        prompt: text.clone(),
                        prompt_file: Some(file.clone()),
                        temp: pass.temp,
                        reps: pass.reps,
                        max_tokens: spec.max_tokens,
                        concurrency: c,
                        spec: Some(Tag {
                            name: spec.name.clone(),
                            version: spec.version.clone(),
                            sha256: spec.sha256.clone(),
                            pass: pass.name.clone(),
                        }),
                        ..base.plan.clone()
                    },
                    campaign: campaign.clone(),
                    ..base.clone()
                };
                if let Err(e) = sweep::run(&opts) {
                    problems.push(format!("pass {} concurrency {}: {e}", pass.name, c.as_str()));
                }
            }
            if let Err(e) = restore_overrides(&base.preset, restore) {
                problems.push(e);
                // A key not put back is a preset left measuring something
                // else: stop rather than run the next pass on top of it.
                return Err(problems.join("; "));
            }
        }
    }
    if problems.is_empty() {
        Ok(campaign)
    } else {
        Err(problems.join("; "))
    }
}

/// Write each override, a pending marker first, so an interrupted run is
/// reported like an interrupted sweep. Returns what to put back.
fn apply_overrides(
    preset: &str,
    overrides: &[(String, String)],
) -> Result<Vec<(String, String, PathBuf)>, String> {
    let mut restore = Vec::new();
    for (key, value) in overrides {
        let (original, marker) = sweep::hold_key(preset, key)?;
        if let Err(e) = sweep::set_key(preset, key, value) {
            let _ = restore_overrides(preset, restore);
            let _ = std::fs::remove_file(&marker);
            return Err(format!("override {key} = {value}: {e}"));
        }
        println!("Override for this pass: {key} = {value} (was {original}).");
        restore.push((key.clone(), original, marker));
    }
    Ok(restore)
}

fn restore_overrides(preset: &str, restore: Vec<(String, String, PathBuf)>) -> Result<(), String> {
    for (key, original, marker) in restore.into_iter().rev() {
        sweep::set_key(preset, &key, &original)
            .map_err(|e| format!("could not restore override {key} = {original}: {e}"))?;
        let _ = std::fs::remove_file(marker);
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    const A: &str = "96d8fb4f7d56d59ee53aa145394c8f8adb0122adbe16421391dcfd5576aaaae6";
    const SAMPLE: &str = "[Spec]\nName = standard\nVersion = 1\n\
        Prompts = bench-prompt-43k.txt@96d8fb4f7d56d59ee53aa145394c8f8adb0122adbe16421391dcfd5576aaaae6, \
        /x/y.txt@96D8FB4F7D56D59EE53AA145394C8F8ADB0122ADBE16421391DCFD5576AAAAE6\nMaxTokens = 0\n\n\
        [Pass.engine]\nTemp = 0\nOverride = spec-type=none\nReps = 3\n\n\
        [Pass.real]\nTemp = preset\nConcurrency = all\nReps = 5\n";

    #[test]
    fn a_spec_parses_its_passes_and_resolves_bare_prompt_names() {
        let s = parse(SAMPLE, Path::new("/cfg")).unwrap();
        assert_eq!((s.name.as_str(), s.version.as_str()), ("standard", "1"));
        assert_eq!(s.sha256.len(), 64);
        assert_eq!(
            s.prompts,
            vec![
                (PathBuf::from("/cfg/bench-prompt-43k.txt"), A.to_string()),
                (PathBuf::from("/x/y.txt"), A.to_string())
            ]
        );
        assert_eq!(s.max_tokens, 0);
        assert_eq!(s.passes.len(), 2);
        let e = &s.passes[0];
        assert_eq!((e.temp, e.reps, e.concurrency), (Some(0.0), 3, Some(Concurrency::Single)));
        assert_eq!(e.overrides, vec![("spec-type".to_string(), "none".to_string())]);
        let r = &s.passes[1];
        assert_eq!((r.temp, r.reps, r.concurrency), (None, 5, None));
        assert!(r.overrides.is_empty());
    }

    #[test]
    fn a_spec_refuses_what_it_cannot_run() {
        let cfg = Path::new("/cfg");
        assert!(parse("[Pass.a]\nTemp = 0\n", cfg).is_err());
        assert!(parse(&SAMPLE.replace("Name = standard", "Name = custom"), cfg).is_err());
        assert!(parse(&SAMPLE.replace("Reps = 3", "Reps = 0"), cfg).is_err());
        assert!(parse(&SAMPLE.replace("Concurrency = all", "Concurrency = many"), cfg).is_err());
        assert!(parse(&SAMPLE.replace("spec-type=none", "no-such-key=1"), cfg).is_err());
        let no_pass = format!("[Spec]\nName = a\nVersion = 1\nPrompts = p.txt@{A}\n");
        assert!(parse(&no_pass, cfg).is_err());
        let unpinned = SAMPLE.replace(&format!("bench-prompt-43k.txt@{A}"), "bench-prompt-43k.txt");
        assert!(parse(&unpinned, cfg).is_err());
    }

    /// The digest follows the rules, not the text: a comment is free, a
    /// changed repetition count is another method.
    #[test]
    fn the_digest_covers_the_rules_and_not_the_comments() {
        let cfg = Path::new("/cfg");
        let base = parse(SAMPLE, cfg).unwrap().sha256;
        let commented = format!("; a note\n{SAMPLE}");
        assert_eq!(parse(&commented, cfg).unwrap().sha256, base);
        let changed = SAMPLE.replace("Reps = 5", "Reps = 6");
        assert_ne!(parse(&changed, cfg).unwrap().sha256, base);
    }

    /// Every published version of the standard spec is frozen: its digest is
    /// in `standard.versions`, so a rule changed without a new `Version`, or a
    /// published version edited in place, fails here.
    #[test]
    fn the_standard_spec_is_a_published_version() {
        let s = parse(super::STANDARD, Path::new("/cfg")).unwrap();
        let published = published_versions(include_str!("../../../benchmarks/specs/standard.versions"));
        let mut seen = std::collections::HashSet::new();
        for (v, _) in &published {
            assert!(seen.insert(v.clone()), "version {v} is listed twice");
        }
        let pinned = published.iter().find(|(v, _)| *v == s.version).unwrap_or_else(|| {
            panic!(
                "standard.ini Version {} is not in standard.versions: add `{} {}`",
                s.version, s.version, s.sha256
            )
        });
        assert_eq!(
            pinned.1, s.sha256,
            "standard.ini changed its rules but kept Version {}: bump Version and add `<new> {}`",
            s.version, s.sha256
        );
    }

    /// The repo's own spec pins the prompts the binary ships: a shipped prompt
    /// edited without a new spec version fails here, not on someone's machine.
    #[test]
    fn the_standard_spec_pins_the_shipped_prompts() {
        let s = parse(super::STANDARD, Path::new("/cfg")).unwrap();
        assert!(!s.prompts.is_empty());
        for (path, pinned) in &s.prompts {
            let name = path.file_name().unwrap().to_str().unwrap();
            let (_, shipped) = super::super::SHIPPED_PROMPTS
                .iter()
                .find(|(f, _)| *f == name)
                .unwrap_or_else(|| panic!("{name} is not a shipped prompt"));
            assert_eq!(&super::super::prompt_info(shipped).sha256, pinned, "{name}");
        }
    }
}
