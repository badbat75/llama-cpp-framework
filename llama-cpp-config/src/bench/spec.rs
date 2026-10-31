//! Versioned benchmark specifications (`bench sweep --spec standard|<file>`).
//! Only a published version of `standard` (`standard.versions`) is a spec run;
//! a file whose rules are not one runs the same way but records every run as
//! `spec = custom`, derived from it (`Spec::published`).
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
    /// The fields a tab run with Override took from the Workload card instead
    /// of the spec (`Derive::fields`). Non-empty: the run is NOT of the spec
    /// and records `spec = custom`, with `spec-derived-from` naming it.
    pub derived: Vec<String>,
}

/// A prompt's file (`None` for an inline `--prompt`) and text.
pub type Prompt = (Option<PathBuf>, String);

/// What a run changes of a spec (the tab's Override, or a flag beside
/// `--spec`): each `Some` replaces the spec's value in every step.
#[derive(Clone, Debug, Default)]
pub struct Derive {
    /// Prompts in place of the spec's list.
    pub prompt: Option<Vec<Prompt>>,
    /// `Some(None)` = the preset's own temperature.
    pub temp: Option<Option<f64>>,
    pub max_tokens: Option<i32>,
    pub reps: Option<i32>,
    /// `Some(None)` = all.
    pub concurrency: Option<Option<Concurrency>>,
}

impl Derive {
    /// The overridden fields, in the card's order: what `Tag::derived` records.
    pub fn fields(&self) -> Vec<String> {
        [
            ("prompt", self.prompt.is_some()),
            ("temp", self.temp.is_some()),
            ("max-tokens", self.max_tokens.is_some()),
            ("concurrency", self.concurrency.is_some()),
            ("reps", self.reps.is_some()),
        ]
        .into_iter()
        .filter(|(_, on)| *on)
        .map(|(f, _)| f.to_string())
        .collect()
    }
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
    /// Its rules are a published version of `standard` (name, version and
    /// digest in `standard.versions`). Anything else, a file of your own
    /// included, is not a method another machine shares: its runs record
    /// `spec = custom`, derived from it, with `spec-file` among the fields.
    pub published: bool,
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
        published: false,
    };
    spec.sha256 = crate::sha256::hex(canonical(&spec).as_bytes());
    spec.published = spec.name == "standard"
        && published_versions(STANDARD_VERSIONS)
            .iter()
            .any(|(v, sha)| *v == spec.version && *sha == spec.sha256);
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

/// `standard.versions`: every published version of `standard` with its digest.
pub const STANDARD_VERSIONS: &str = include_str!("../../../benchmarks/specs/standard.versions");

/// `standard` at a published `version`: the compiled-in rules, refused unless
/// they are that version and match its digest in `standard.versions`. The
/// Benchmark tab's only specs (besides custom).
pub fn standard_version(version: &str) -> Result<Spec, String> {
    let published = published_versions(STANDARD_VERSIONS);
    let Some((_, pinned)) = published.iter().find(|(v, _)| v == version) else {
        return Err(format!("standard v{version} is not a published version (standard.versions)"));
    };
    let spec = load("standard")?;
    if spec.version != version {
        return Err(format!(
            "this build carries the rules of standard v{} only, not v{version}",
            spec.version
        ));
    }
    if &spec.sha256 != pinned {
        return Err(format!(
            "the built-in standard v{version} does not match its digest in standard.versions"
        ));
    }
    Ok(spec)
}

/// `spec` as a spec file: what the Benchmark tab's Save as… writes. A prompt
/// in `config` is written as a bare name, so the file reads on another
/// machine.
pub fn to_ini(spec: &Spec, config: &Path) -> String {
    let prompts: Vec<String> = spec
        .prompts
        .iter()
        .map(|(p, sha)| {
            let shown = match p.parent() {
                Some(dir) if dir == config => p.file_name().unwrap_or_default().to_string_lossy().into_owned(),
                _ => p.to_string_lossy().into_owned(),
            };
            format!("{shown}@{sha}")
        })
        .collect();
    let mut out = format!(
        "; A benchmark spec of your own (saved by llama-cpp-config). It is not a\n\
         ; published version of standard, so every run of it records spec = custom,\n\
         ; derived from it. Format: benchmarks/README.md, \"Specs\".\n\
         [Spec]\nName = {}\nVersion = {}\nPrompts = {}\nMaxTokens = {}\n",
        spec.name,
        spec.version,
        prompts.join(", "),
        spec.max_tokens
    );
    for p in &spec.passes {
        out.push_str(&format!(
            "\n[Pass.{}]\nTemp = {}\n",
            p.name,
            p.temp.map_or("preset".to_string(), |t| t.to_string())
        ));
        if !p.overrides.is_empty() {
            let o: Vec<String> = p.overrides.iter().map(|(k, v)| format!("{k}={v}")).collect();
            out.push_str(&format!("Override = {}\n", o.join("; ")));
        }
        out.push_str(&format!(
            "Concurrency = {}\nReps = {}\n",
            p.concurrency.map_or("all", |c| c.as_str()),
            p.reps
        ));
    }
    out
}

/// The `<version> <sha256>` lines of a `.versions` file, comments and blanks
/// skipped.
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
pub fn run(spec: &Spec, base: &Opts, derive: &Derive) -> Result<String, String> {
    if base.plan.mode != super::Mode::Live {
        return Err("--spec runs the live engine".into());
    }
    if base.apply != Apply::Original && spec.passes.len() * spec.prompts.len() > 1 {
        return Err("--spec with several passes or prompts leaves the preset as it was: drop --apply".into());
    }
    // Anything that is not the spec's own rules makes every run custom,
    // derived from it: a flag given beside --spec, a swept key that a pass
    // sets (the leg's value goes over the pass's), a file that is no
    // published version.
    let (prompts, derived) = prompts_and_derived(spec, derive, Some(&base.key))?;
    if !derived.is_empty() {
        println!(
            "Not spec {} v{} as published ({}): every run records spec = custom, derived from it.",
            spec.name,
            spec.version,
            derived.join(", ")
        );
    }
    let campaign = if base.campaign.is_empty() {
        default_campaign(spec)
    } else {
        base.campaign.clone()
    };
    let mut problems = Vec::new();
    for prompt in &prompts {
        for pass in &spec.passes {
            let scenarios = match derive.concurrency.unwrap_or(pass.concurrency) {
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
                    prompt_name(prompt),
                    c.as_str()
                );
                let opts = Opts {
                    plan: pass_plan(spec, pass, prompt, c, &base.plan, derive, &derived),
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

/// `spec-<name>-v<version>-<stamp>`: a spec run's campaign when none is named.
pub fn default_campaign(spec: &Spec) -> String {
    format!(
        "spec-{}-v{}-{}",
        spec.name,
        spec.version,
        super::stamp(super::now_secs())
    )
}

/// The plan of one pass over one prompt and one scenario: `base` with the
/// spec's prompt, temperature, repetitions, cap, concurrency and identity,
/// then `derive` over them, `derived` recorded as what changed.
fn pass_plan(
    spec: &Spec,
    pass: &Pass,
    prompt: &Prompt,
    c: Concurrency,
    base: &Plan,
    derive: &Derive,
    derived: &[String],
) -> Plan {
    Plan {
        prompt: prompt.1.clone(),
        prompt_file: prompt.0.clone(),
        temp: derive.temp.unwrap_or(pass.temp),
        reps: derive.reps.unwrap_or(pass.reps),
        max_tokens: derive.max_tokens.unwrap_or(spec.max_tokens),
        concurrency: c,
        spec: Some(Tag {
            name: spec.name.clone(),
            version: spec.version.clone(),
            sha256: spec.sha256.clone(),
            pass: pass.name.clone(),
            derived: derived.to_vec(),
        }),
        ..base.clone()
    }
}

/// The prompts a run sends (`derive`'s one, or the spec's, checked against
/// their pins) and what it records as changed from the spec: `derive`'s
/// fields, `override` when the swept `key` is one a pass sets (the leg's value
/// wins over the pass's), and `spec-file` for rules that are no published
/// standard version. Empty: the run IS of the spec.
fn prompts_and_derived(
    spec: &Spec,
    derive: &Derive,
    key: Option<&str>,
) -> Result<(Vec<Prompt>, Vec<String>), String> {
    let prompts = match &derive.prompt {
        Some(p) => p.clone(),
        None => checked_prompts(spec)?
            .into_iter()
            .map(|(f, t)| (Some(f), t))
            .collect(),
    };
    let mut derived = derive.fields();
    if key.is_some_and(|k| spec.passes.iter().any(|p| p.overrides.iter().any(|(o, _)| o == k))) {
        derived.push("override".into());
    }
    if !spec.published {
        derived.push("spec-file".into());
    }
    Ok((prompts, derived))
}

fn prompt_name(p: &Prompt) -> String {
    match &p.0 {
        Some(f) => f.file_name().unwrap_or_default().to_string_lossy().into_owned(),
        None => "--prompt".into(),
    }
}

/// The Benchmark tab's spec run: one step per prompt x pass x scenario over the
/// tab's presets (`base.presets`, its live mode and campaign), each pass's
/// overrides written to every preset for its steps. `Concurrency = all` runs the
/// scenarios every selected preset supports (single alone when one has
/// `parallel` < 2). `derive` replaces spec values in every step (Override);
/// any at all makes every run custom, derived from the spec.
pub fn tab_steps(
    spec: &Spec,
    base: &Plan,
    derive: &Derive,
    ab_key: Option<&str>,
) -> Result<Vec<super::exec::Step>, String> {
    if base.mode != super::Mode::Live {
        return Err("a spec runs the live engine: pick live".into());
    }
    let (prompts, derived) = prompts_and_derived(spec, derive, ab_key)?;
    let mut all = Concurrency::ALL.to_vec();
    for p in &base.presets {
        let mine = sweep::all_scenarios(p)?;
        all.retain(|c| mine.contains(c));
    }
    let mut steps = Vec::new();
    for prompt in &prompts {
        let name = prompt_name(prompt);
        for pass in &spec.passes {
            let scenarios = match derive.concurrency.unwrap_or(pass.concurrency) {
                Some(c) => vec![c],
                None => all.clone(),
            };
            for c in scenarios {
                steps.push(super::exec::Step {
                    label: format!("{} · {} · {}", pass.name, name, c.as_str()),
                    overrides: pass.overrides.clone(),
                    plan: pass_plan(spec, pass, prompt, c, base, derive, &derived),
                });
            }
        }
    }
    Ok(steps)
}

/// The Benchmark tab's A/B: every step once per value of `key`, that value
/// written to the presets for it (over a pass's own override of the same key,
/// which `tab_steps` then records as derived), value first in the label.
pub fn ab_expand(steps: Vec<super::exec::Step>, key: &str, values: &[String]) -> Vec<super::exec::Step> {
    let mut out = Vec::new();
    for v in values {
        for st in &steps {
            let mut overrides: Vec<(String, String)> =
                st.overrides.iter().filter(|(k, _)| k != key).cloned().collect();
            overrides.push((key.to_string(), v.clone()));
            let label = if st.label.is_empty() {
                format!("{key}={v}")
            } else {
                format!("{key}={v} · {}", st.label)
            };
            out.push(super::exec::Step {
                label,
                overrides,
                plan: st.plan.clone(),
            });
        }
    }
    out
}

/// Each prompt with its text, after checking it against its pin.
fn checked_prompts(spec: &Spec) -> Result<Vec<(PathBuf, String)>, String> {
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
    Ok(prompts)
}

/// Write each override, a pending marker first, so an interrupted run is
/// reported like an interrupted sweep. Returns what to put back.
pub(super) fn apply_overrides(
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

pub(super) fn restore_overrides(preset: &str, restore: Vec<(String, String, PathBuf)>) -> Result<(), String> {
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
    /// Anything that is not the spec's own rules is recorded: a changed
    /// field, a swept key a pass sets, rules that are no published version.
    #[test]
    fn a_run_off_the_spec_says_what_it_changed() {
        // The shipped prompts, seeded by name into a config of its own.
        let dir = tempfile::tempdir().unwrap();
        let std = parse(super::STANDARD, dir.path()).unwrap();
        let none = Derive::default();
        let (_, d) = prompts_and_derived(&std, &none, Some("split-mode")).unwrap();
        assert!(d.is_empty(), "standard as published, a key no pass sets");
        let (_, d) = prompts_and_derived(&std, &none, Some("spec-type")).unwrap();
        assert_eq!(d, ["override"]);
        let temp = Derive {
            temp: Some(None),
            prompt: Some(vec![(None, "hi".into())]),
            ..Derive::default()
        };
        let (prompts, d) = prompts_and_derived(&std, &temp, None).unwrap();
        assert_eq!(prompts, [(None, "hi".to_string())]);
        assert_eq!(d, ["prompt", "temp"]);
    }

    /// What Save as… writes reads back as the same rules.
    #[test]
    fn a_saved_spec_reads_back_as_the_same_rules() {
        let cfg = Path::new("/cfg");
        let std = parse(super::STANDARD, cfg).unwrap();
        let mine = Spec {
            name: "mine".into(),
            ..std.clone()
        };
        let back = parse(&to_ini(&mine, cfg), cfg).unwrap();
        assert_eq!(back.passes, mine.passes);
        assert_eq!(back.prompts, mine.prompts);
        assert_eq!((back.max_tokens, back.name.as_str()), (mine.max_tokens, "mine"));
        assert!(!back.published);
        // Named standard with the same rules, it IS the published version.
        assert!(parse(&to_ini(&std, cfg), cfg).unwrap().published);
    }

    /// The tab picks standard by version: the compiled one runs, any other
    /// (published or not) is refused with the reason.
    #[test]
    fn only_the_compiled_published_standard_version_runs() {
        let s = parse(super::STANDARD, Path::new("/cfg")).unwrap();
        assert_eq!(standard_version(&s.version).unwrap().sha256, s.sha256);
        assert!(s.published, "the compiled standard is a published version");
        assert!(standard_version("999").unwrap_err().contains("not a published version"));
    }

    #[test]
    fn the_standard_spec_is_a_published_version() {
        let s = parse(super::STANDARD, Path::new("/cfg")).unwrap();
        let published = published_versions(STANDARD_VERSIONS);
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

    /// The tab's steps: prompt x pass x scenario, each pass's overrides on its
    /// steps only, the plan at the pass's rules and tagged with it.
    #[test]
    fn the_tab_runs_one_step_per_prompt_pass_and_scenario() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("p.txt"), "hello").unwrap();
        let sha = super::super::prompt_info("hello").sha256;
        let text = format!(
            "[Spec]\nName = t\nVersion = 2\nPrompts = p.txt@{sha}\nMaxTokens = 0\n\n\
             [Pass.engine]\nTemp = 0\nOverride = spec-type=none\nConcurrency = single\nReps = 3\n\n\
             [Pass.real]\nTemp = preset\nConcurrency = all\nReps = 5\n"
        );
        let spec = parse(&text, dir.path()).unwrap();
        let base = Plan {
            mode: super::super::Mode::Live,
            ..Plan::default()
        };
        let steps = tab_steps(&spec, &base, &Derive::default(), None).unwrap();
        let labels: Vec<&str> = steps.iter().map(|s| s.label.as_str()).collect();
        assert_eq!(
            labels,
            [
                "engine · p.txt · single",
                "real · p.txt · single",
                "real · p.txt · prefill",
                "real · p.txt · decode",
                "real · p.txt · prefill+decode",
            ]
        );
        assert_eq!(steps[0].overrides, [("spec-type".to_string(), "none".to_string())]);
        assert!(steps[1..].iter().all(|s| s.overrides.is_empty()));
        assert_eq!((steps[0].plan.temp, steps[0].plan.reps), (Some(0.0), 3));
        assert_eq!((steps[1].plan.temp, steps[1].plan.reps), (None, 5));
        assert_eq!(steps[0].plan.prompt, "hello");
        assert_eq!(steps[4].plan.spec.as_ref().map(|t| t.pass.as_str()), Some("real"));
        // A spec file that is no published standard version runs, but every
        // run of it is custom, derived from it.
        assert!(!spec.published);
        assert_eq!(steps[0].plan.spec.as_ref().unwrap().derived, ["spec-file"]);
        let synthetic = Plan {
            mode: super::super::Mode::Synthetic,
            ..Plan::default()
        };
        assert!(tab_steps(&spec, &synthetic, &Derive::default(), None).is_err());

        // Override: the changed fields replace the spec's in every step, and
        // every run is recorded as derived from it.
        let derive = Derive {
            reps: Some(2),
            concurrency: Some(Some(Concurrency::Single)),
            ..Derive::default()
        };
        let steps = tab_steps(&spec, &base, &derive, None).unwrap();
        assert_eq!(steps.len(), 2, "one scenario per pass");
        assert!(steps.iter().all(|s| s.plan.reps == 2));
        assert_eq!(steps[0].plan.temp, Some(0.0), "the pass's temperature stays");
        assert_eq!(
            steps[1].plan.spec.as_ref().unwrap().derived,
            ["concurrency", "reps", "spec-file"],
            "a spec that is no published standard version is a derivation too"
        );
    }

    /// A/B over a key the engine pass overrides: every step once per value,
    /// the value over the pass's own, and the runs recorded as derived.
    #[test]
    fn an_ab_key_runs_every_step_per_value() {
        let dir = tempfile::tempdir().unwrap();
        let spec = parse(super::STANDARD, dir.path()).unwrap();
        let base = Plan {
            mode: super::super::Mode::Live,
            presets: vec![],
            ..Plan::default()
        };
        let derive = Derive {
            concurrency: Some(Some(Concurrency::Single)),
            ..Derive::default()
        };
        let steps = tab_steps(&spec, &base, &derive, Some("spec-type")).unwrap();
        let n = steps.len();
        let values = vec!["none".to_string(), "draft-mtp".to_string()];
        let ab = ab_expand(steps, "spec-type", &values);
        assert_eq!(ab.len(), 2 * n);
        assert!(ab[0].label.starts_with("spec-type=none · "));
        assert_eq!(ab[n].overrides, [("spec-type".to_string(), "draft-mtp".to_string())]);
        assert!(ab[n].plan.spec.as_ref().unwrap().derived.contains(&"override".to_string()));
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
