//! Versioned benchmark specifications (`bench run --spec
//! standard|standard-<X.Y>|<file>`). Only a published version of `standard`
//! (`standard.versions`) is a spec run;
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
//! Version = 1.0
//! Prompts = bench-prompt-43k-1.0.txt@96d8fb4f..., bench-prompt-code-1.0.json@...
//! MaxTokens = 0
//! ImageSize = 1344x896
//!
//! [Pass.engine]
//! Temp = 0
//! Override = spec-type=none
//! Concurrency = single
//! Reps = 3
//! ```
//!
//! Every published version is installed beside the binaries as
//! `specs\standard-<X.Y>.ini` (`paths::bench_share_dir()`) and stays there, so
//! a run can follow an older version as well as the newest; the binary carries
//! only `standard.versions`, the digest of each, so a file that no longer
//! matches its line runs as custom, never as the version it claims.
//! A bare prompt name is a shipped prompt (`paths::bench_prompts_dir()`), else
//! a file in the config directory; a path is used as is. Each
//! carries `@<sha256>` of its text (as `bench query prompts` shows it): the
//! prompts are part of the method, so a prompt that no longer matches is
//! refused before anything restarts, and changing one means a new spec
//! version pinning the new digest.
//! `Override` is a `;`-separated list of `key=value` (a value may hold commas),
//! each written to the preset for the pass and put back after it, with a
//! pending marker like a sweep's own key. `Concurrency = all` is single, then
//! every concurrent scenario when the preset has two slots or more. A prompt
//! with an image (`bench::prompt_has_image`) runs only on presets that load an
//! mmproj and is skipped on the rest, so one spec covers text-only models too.

use crate::strings;
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
    /// `Some(None)` = the image file as is.
    pub image_size: Option<Option<(u32, u32)>>,
    /// An image in place of the image prompts' own (`Some(None)` = theirs).
    pub image: Option<Option<PathBuf>>,
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
            ("image-size", self.image_size.is_some()),
            ("image", self.image.is_some()),
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
    /// `Prompts`: the file names of the spec's prompts this pass runs; empty
    /// = every prompt.
    pub prompts: Vec<String>,
    /// `Warm = prefix`: each request is an agent step with its prefix cached
    /// (`Plan::warm_prefix`).
    pub warm: bool,
}

impl Pass {
    /// Whether this pass runs the prompt in `file` (a file name).
    pub fn runs(&self, file: &str) -> bool {
        self.prompts.is_empty() || self.prompts.iter().any(|p| p == file)
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct Spec {
    pub name: String,
    pub version: String,
    pub sha256: String,
    /// Each prompt with the sha256 its text must have.
    pub prompts: Vec<(PathBuf, String)>,
    pub max_tokens: i32,
    /// `ImageSize`: the box an image prompt's image is fitted into when sent;
    /// `None` (absent, or `original`) sends the file as is.
    pub image_size: Option<(u32, u32)>,
    pub passes: Vec<Pass>,
    /// Its rules are a published version of `standard` (name, version and
    /// digest in `standard.versions`). Anything else, a file of your own
    /// included, is not a method another machine shares: its runs record
    /// `spec = custom`, derived from it, with `spec-file` among the fields.
    pub published: bool,
}

/// Where the published versions of `standard` are installed.
pub fn specs_dir() -> PathBuf {
    crate::paths::bench_share_dir().join("specs")
}

/// `standard` = its newest published version, `standard-<X.Y>` that version;
/// anything else is a file.
pub fn load(arg: &str) -> Result<Spec, String> {
    if arg == "standard" {
        let newest = published_versions(STANDARD_VERSIONS)
            .into_iter()
            .map(|(v, _)| v)
            .max_by_key(|v| version_key(v))
            .ok_or("standard.versions lists no version")?;
        return standard_version(&newest);
    }
    if let Some(v) = arg.strip_prefix("standard-") {
        return standard_version(v);
    }
    load_file(Path::new(arg))
}

/// `1.10` after `1.9`: a version's numbers, compared as numbers.
pub fn version_key(v: &str) -> Vec<u64> {
    v.split('.').map(|n| n.parse().unwrap_or(0)).collect()
}

fn load_file(path: &Path) -> Result<Spec, String> {
    let bytes = std::fs::read(path).map_err(|e| strings::fill("cannot-read-file", &[&path.display(), &e]))?;
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
        .ok_or_else(|| strings::t("no-spec-section"))?;
    let get = |k: &str| head.keys.get(k).map(|v| v.trim().to_string());
    let name = get("Name").filter(|v| !v.is_empty()).ok_or_else(|| strings::t("spec-no-name"))?;
    if name == "custom" {
        return Err(strings::t("spec-name-reserved"));
    }
    let version = get("Version").filter(|v| !v.is_empty()).ok_or_else(|| strings::t("spec-no-version"))?;
    let mut prompts = Vec::new();
    for entry in get("Prompts").unwrap_or_default().split(',').map(str::trim) {
        if entry.is_empty() {
            continue;
        }
        let (p, sha) = entry
            .rsplit_once('@')
            .filter(|(_, s)| s.len() == 64 && s.chars().all(|c| c.is_ascii_hexdigit()))
            .ok_or_else(|| strings::fill("spec-prompt-format", &[&entry]))?;
        let path = PathBuf::from(p.trim());
        let path = if path.components().count() == 1 {
            let shipped = crate::paths::bench_prompts_dir().join(&path);
            if shipped.is_file() {
                shipped
            } else {
                config.join(path)
            }
        } else {
            path
        };
        prompts.push((path, sha.to_ascii_lowercase()));
    }
    if prompts.is_empty() {
        return Err(strings::t("spec-no-prompts"));
    }
    let max_tokens = match get("MaxTokens") {
        None => 0,
        Some(v) => ini::parse_int_in(&v, &ini::INT_NON_NEGATIVE)
            .ok_or_else(|| strings::fill("spec-max-tokens", &[&v, &0]))?,
    };
    let image_size = match get("ImageSize") {
        None => None,
        Some(v) => super::parse_image_size(&v).map_err(|e| format!("ImageSize: {e}"))?,
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
                ini::parse_float(v).ok_or_else(|| ctx(strings::fill("spec-temp", &[&v])))?,
            ),
        };
        let reps = match get("Reps") {
            None => super::DEFAULT_REPS,
            Some(v) => ini::parse_int_in(&v, &ini::INT_POSITIVE)
                .ok_or_else(|| ctx(strings::fill("spec-reps", &[&v, &1])))?,
        };
        let concurrency = match get("Concurrency").as_deref() {
            None => Some(Concurrency::Single),
            Some("all") => None,
            Some(v) => Some(
                Concurrency::from_str(v)
                    .ok_or_else(|| ctx(strings::fill("spec-concurrency", &[&v])))?,
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
                .ok_or_else(|| ctx(strings::fill("spec-override-format", &[&o])))?;
            let (k, v) = (k.trim().to_string(), v.trim().to_string());
            sweep::setter(&k).map_err(|e| ctx(format!("Override `{k}`: {e}")))?;
            overrides.push((k, v));
        }
        let pass_prompts: Vec<String> = get("Prompts")
            .unwrap_or_default()
            .split(',')
            .map(str::trim)
            .filter(|p| !p.is_empty())
            .map(str::to_string)
            .collect();
        for p in &pass_prompts {
            if !prompts.iter().any(|(f, _)| f.file_name().is_some_and(|n| n.to_string_lossy() == p.as_str())) {
                return Err(ctx(strings::fill("spec-prompt-unknown", &[&p])));
            }
        }
        let warm = match get("Warm").as_deref() {
            None | Some("none") => false,
            Some("prefix") => true,
            Some(v) => return Err(ctx(strings::fill("spec-warm", &[&v]))),
        };
        passes.push(Pass {
            name: pass.to_string(),
            temp,
            reps,
            concurrency,
            overrides,
            prompts: pass_prompts,
            warm,
        });
    }
    if passes.is_empty() {
        return Err(strings::t("spec-no-pass"));
    }
    let mut spec = Spec {
        name,
        version,
        sha256: String::new(),
        prompts,
        max_tokens,
        image_size,
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
/// directory it resolves to on this machine. `image-size` only when set, so
/// a spec without one keeps the digest it had before the key existed.
pub fn canonical(spec: &Spec) -> String {
    let mut out = format!(
        "name={}\nversion={}\nmax-tokens={}\n",
        spec.name, spec.version, spec.max_tokens
    );
    if let Some(size) = spec.image_size {
        out.push_str(&format!("image-size={}\n", super::image_size_str(Some(size))));
    }
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
        // Only when set, so a spec without them keeps the digest it had
        // before the keys existed.
        if !p.prompts.is_empty() {
            out.pop();
            out.push_str(&format!(" prompts={}\n", p.prompts.join(",")));
        }
        if p.warm {
            out.pop();
            out.push_str(" warm=prefix\n");
        }
    }
    out
}

/// `standard.versions`: every published version of `standard` with its
/// digest. The only part of the method compiled in: the spec files and the
/// prompts are installed, and each is checked against this (the spec's digest
/// covers its prompt pins).
pub const STANDARD_VERSIONS: &str = include_str!("../../../benchmarks/specs/standard.versions");

/// `standard` at a published `version`: its installed file
/// (`specs\standard-<version>.ini`), refused unless it IS that version and
/// matches its digest in `standard.versions`.
pub fn standard_version(version: &str) -> Result<Spec, String> {
    let published = published_versions(STANDARD_VERSIONS);
    if !published.iter().any(|(v, _)| v == version) {
        return Err(strings::fill("spec-not-published", &[&version, &"standard.versions"]));
    }
    let path = specs_dir().join(format!("standard-{version}.ini"));
    let spec = load_file(&path)?;
    if spec.name != "standard" || spec.version != version {
        return Err(strings::fill("spec-file-mismatch", &[&path.display(), &spec.name, &spec.version, &version]));
    }
    if !spec.published {
        return Err(strings::fill("spec-digest-mismatch", &[&path.display(), &version, &"standard.versions"]));
    }
    Ok(spec)
}

/// `spec` as a spec file: what the Benchmark tab's Save as… writes. A prompt
/// in `config` or a shipped one is written as a bare name, so the file reads
/// on another machine.
pub fn to_ini(spec: &Spec, config: &Path) -> String {
    let prompts: Vec<String> = spec
        .prompts
        .iter()
        .map(|(p, sha)| {
            let shown = match p.parent() {
                Some(dir) if dir == config || dir == crate::paths::bench_prompts_dir() => p.file_name().unwrap_or_default().to_string_lossy().into_owned(),
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
    if spec.image_size.is_some() {
        out.push_str(&format!("ImageSize = {}\n", super::image_size_str(spec.image_size)));
    }
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
        if !p.prompts.is_empty() {
            out.push_str(&format!("Prompts = {}\n", p.prompts.join(", ")));
        }
        if p.warm {
            out.push_str("Warm = prefix\n");
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
/// `only`: the file names of the spec's prompts to run, empty = all; a
/// selection of its legs, not a change of method (each leg is recorded as the
/// spec's), so a new version can run just the legs it added.
pub fn run(spec: &Spec, base: &Opts, derive: &Derive, only: &[String]) -> Result<String, String> {
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
    let (mut prompts, derived) = prompts_and_derived(spec, derive, base.key.as_deref())?;
    if !only.is_empty() {
        for o in only {
            if !prompts.iter().any(|p| prompt_name(p) == *o) {
                return Err(format!("--only {o}: not one of the spec's prompts"));
            }
        }
        prompts.retain(|p| only.contains(&prompt_name(p)));
    }
    if !derived.is_empty() {
        println!(
            "Not spec {}-{} as published ({}): every run records spec = custom, derived from it.",
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
        if super::prompt_has_image(&prompt.1) && !sweep::has_mmproj(&base.preset)? {
            println!(
                "\nSkipping {}: an image prompt, and preset `{}` loads no mmproj.",
                prompt_name(prompt),
                base.preset
            );
            continue;
        }
        for pass in &spec.passes {
            if derive.prompt.is_none() && !pass.runs(&prompt_name(prompt)) {
                continue;
            }
            let scenarios = pass_scenarios(
                pass,
                &prompt.1,
                derive.concurrency.unwrap_or(pass.concurrency),
                sweep::all_scenarios(&base.preset)?,
            )?;
            let restore = apply_overrides(&base.preset, &pass.overrides)?;
            for c in scenarios {
                println!(
                    "\n=== spec {}-{} · pass {} · {} · concurrency {} ===",
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

/// The scenarios a pass runs on a prompt: its own (`chosen`), or `all` (the
/// preset's). A warm-prefix pass needs a prompt with an agent turn to cache up
/// to, and has nothing to measure in `prefill` (no slot is warm), so `all`
/// leaves that one out.
fn pass_scenarios(
    pass: &Pass,
    text: &str,
    chosen: Option<Concurrency>,
    all: Vec<Concurrency>,
) -> Result<Vec<Concurrency>, String> {
    if pass.warm && super::prompt_prefix(text).is_none() {
        return Err(strings::fill("spec-warm-needs-assistant", &[&pass.name]));
    }
    Ok(match chosen {
        Some(c) => vec![c],
        None => all
            .into_iter()
            .filter(|c| !(pass.warm && *c == Concurrency::Prefill))
            .collect(),
    })
}

/// `spec-<name>-<version>-<stamp>`: a spec run's campaign when none is named.
pub fn default_campaign(spec: &Spec) -> String {
    format!(
        "spec-{}-{}-{}",
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
        image_size: derive.image_size.unwrap_or(spec.image_size),
        image: derive.image.clone().unwrap_or(None),
        concurrency: c,
        warm_prefix: pass.warm,
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
/// `parallel` < 2). An image prompt's steps run only the selected presets
/// that load an mmproj, and none when no preset does. `derive` replaces spec
/// values in every step (Override); any at all makes every run custom, derived
/// from the spec.
pub fn tab_steps(
    spec: &Spec,
    base: &Plan,
    derive: &Derive,
    ab_key: Option<&str>,
) -> Result<Vec<super::exec::Step>, String> {
    if base.mode != super::Mode::Live {
        return Err(strings::t("spec-runs-live"));
    }
    let (prompts, derived) = prompts_and_derived(spec, derive, ab_key)?;
    let mut steps = Vec::new();
    for prompt in &prompts {
        let name = prompt_name(prompt);
        // An image prompt runs on the presets with an mmproj, and not at all
        // when none has one.
        let mut base = base.clone();
        if super::prompt_has_image(&prompt.1) {
            let mut with = Vec::new();
            for p in &base.presets {
                if sweep::has_mmproj(p)? {
                    with.push(p.clone());
                }
            }
            base.presets = with;
            if base.presets.is_empty() {
                continue;
            }
        }
        let base = &base;
        let mut all = Concurrency::ALL.to_vec();
        for p in &base.presets {
            let mine = sweep::all_scenarios(p)?;
            all.retain(|c| mine.contains(c));
        }
        for pass in &spec.passes {
            if derive.prompt.is_none() && !pass.runs(&name) {
                continue;
            }
            let scenarios = pass_scenarios(
                pass,
                &prompt.1,
                derive.concurrency.unwrap_or(pass.concurrency),
                all.clone(),
            )?;
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
        let text = super::load_prompt_file(p)?;
        let actual = super::prompt_info(&text).sha256;
        if &actual != pinned {
            return Err(strings::fill("spec-prompt-changed", &[&p.display(), &actual, &spec.name, &spec.version, pinned]));
        }
        prompts.push((p.clone(), text));
    }
    Ok(prompts)
}

/// The overrides as they are written: `spec-type = none` also removes
/// `model-draft`, unless the same list sets it. llama-server loads a draft file
/// whatever the speculator is, and without `draft-mtp` an MTP head takes a KV
/// cache for every attention layer of the context, far more than with it, so a pass meant to measure no drafter
/// can fail to load on a preset that fits with one.
fn effective_overrides(overrides: &[(String, String)]) -> Vec<(String, String)> {
    let mut out = overrides.to_vec();
    let draft_off = overrides.iter().any(|(k, v)| k == "spec-type" && v == "none");
    if draft_off && !overrides.iter().any(|(k, _)| k == "model-draft") {
        out.push(("model-draft".to_string(), sweep::UNSET.to_string()));
    }
    out
}

/// Write each override, a pending marker first, so an interrupted run is
/// reported like an interrupted sweep. Returns what to put back.
pub(super) fn apply_overrides(
    preset: &str,
    overrides: &[(String, String)],
) -> Result<Vec<(String, String, PathBuf)>, String> {
    let mut restore = Vec::new();
    for (key, value) in &effective_overrides(overrides) {
        let (original, marker) = sweep::hold_key(preset, key)?;
        if let Err(e) = sweep::set_key(preset, key, value) {
            let _ = restore_overrides(preset, restore);
            super::interrupt::release(&marker);
            return Err(strings::fill("override-failed", &[&key, &value, &e]));
        }
        println!("Override for this pass: {key} = {value} (was {original}).");
        restore.push((key.clone(), original, marker));
    }
    Ok(restore)
}

pub(super) fn restore_overrides(preset: &str, restore: Vec<(String, String, PathBuf)>) -> Result<(), String> {
    for (key, original, marker) in restore.into_iter().rev() {
        sweep::set_key(preset, &key, &original)
            .map_err(|e| strings::fill("override-not-restored", &[&key, &original, &e]))?;
        super::interrupt::release(&marker);
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
    fn spec_type_none_also_removes_the_draft_file() {
        let o = |k: &str, v: &str| (k.to_string(), v.to_string());
        assert_eq!(
            effective_overrides(&[o("spec-type", "none")]),
            vec![o("spec-type", "none"), o("model-draft", sweep::UNSET)]
        );
        // Any other speculator keeps the file, and a list that names it wins.
        assert_eq!(effective_overrides(&[o("spec-type", "draft-mtp")]), vec![o("spec-type", "draft-mtp")]);
        let named = [o("spec-type", "none"), o("model-draft", "x.gguf")];
        assert_eq!(effective_overrides(&named), named.to_vec());
    }

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
    fn a_pass_can_pick_its_prompts_and_warm_a_prefix() {
        let with = SAMPLE.replace(
            "[Pass.real]\nTemp = preset\n",
            "[Pass.real]\nTemp = preset\nPrompts = y.txt\nWarm = prefix\n",
        );
        let s = parse(&with, Path::new("/cfg")).unwrap();
        let real = &s.passes[1];
        assert_eq!(real.prompts, vec!["y.txt".to_string()]);
        assert!(real.warm && real.runs("y.txt") && !real.runs("bench-prompt-43k.txt"));
        assert!(s.passes[0].runs("anything.txt") && !s.passes[0].warm);
        // The keys enter the digest, and a spec without them keeps its own.
        assert_ne!(s.sha256, parse(SAMPLE, Path::new("/cfg")).unwrap().sha256);
        assert!(canonical(&s).contains(" prompts=y.txt warm=prefix\n"));
        assert!(!canonical(&parse(SAMPLE, Path::new("/cfg")).unwrap()).contains("prompts="));
        // Saved and read back, the same rules.
        let again = parse(&to_ini(&s, Path::new("/cfg")), Path::new("/cfg")).unwrap();
        assert_eq!(again.passes, s.passes);
        for bad in ["Prompts = other.txt", "Warm = yes"] {
            let text = SAMPLE.replace("[Pass.real]\n", &format!("[Pass.real]\n{bad}\n"));
            assert!(parse(&text, Path::new("/cfg")).is_err(), "{bad}");
        }
    }

    #[test]
    fn a_warm_pass_skips_prefill_and_needs_an_agent_turn() {
        let s = parse(&SAMPLE.replace("Concurrency = all\n", "Concurrency = all\nWarm = prefix\n"), Path::new("/cfg")).unwrap();
        let conv = r#"{"messages":[{"role":"user","content":"a"},{"role":"assistant","content":"b"},{"role":"user","content":"c"}]}"#;
        let got = pass_scenarios(&s.passes[1], conv, None, Concurrency::ALL.to_vec()).unwrap();
        assert!(!got.contains(&Concurrency::Prefill) && got.contains(&Concurrency::PrefillDecode));
        assert!(pass_scenarios(&s.passes[1], "plain text", None, Concurrency::ALL.to_vec()).is_err());
        assert_eq!(pass_scenarios(&s.passes[0], "plain text", None, Concurrency::ALL.to_vec()).unwrap().len(), 4);
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
        let std = load("standard").unwrap();
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
        let std = load("standard").unwrap();
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

    /// Every published version runs by name, the newest as `standard`; a
    /// version that is not published is refused with the reason.
    #[test]
    fn every_published_standard_version_runs() {
        let published = published_versions(STANDARD_VERSIONS);
        for (v, sha) in &published {
            let s = standard_version(v).unwrap();
            assert_eq!(&s.sha256, sha);
            assert!(s.published);
            assert_eq!(load(&format!("standard-{v}")).unwrap(), s);
        }
        let newest = published.iter().map(|(v, _)| v).max_by_key(|v| version_key(v)).unwrap();
        assert_eq!(&load("standard").unwrap().version, newest);
        assert!(standard_version("999.0").unwrap_err().contains("not a published version"));
        assert!(version_key("1.10") > version_key("1.9"));
    }

    /// Every installed `standard-<X.Y>.ini` is frozen: it is the version its
    /// name says, and its digest is that version's line in
    /// `standard.versions`, so a rule changed without a new version, or a
    /// published version edited in place, fails here. And every published
    /// version is still installed, so an older one can still be run.
    #[test]
    fn the_standard_spec_is_a_published_version() {
        let published = published_versions(STANDARD_VERSIONS);
        let mut seen = std::collections::HashSet::new();
        for (v, _) in &published {
            assert!(seen.insert(v.clone()), "version {v} is listed twice");
            assert!(
                specs_dir().join(format!("standard-{v}.ini")).is_file(),
                "standard {v} is published but standard-{v}.ini is gone: keep every version"
            );
        }
        let mut files = 0;
        for e in std::fs::read_dir(specs_dir()).unwrap().flatten() {
            let name = e.file_name().to_string_lossy().into_owned();
            let Some(v) = name.strip_prefix("standard-").and_then(|n| n.strip_suffix(".ini")) else {
                continue;
            };
            files += 1;
            let s = load_file(&e.path()).unwrap();
            assert_eq!(s.version, v, "{name} says Version {}", s.version);
            let pinned = published.iter().find(|(pv, _)| pv == v).unwrap_or_else(|| {
                panic!("{name}: {v} is not in standard.versions: add `{v} {}`", s.sha256)
            });
            assert_eq!(
                pinned.1, s.sha256,
                "{name} changed its rules but kept Version {v}: add a new standard-<X.Y>.ini \
                 and the line `<X.Y> {}`",
                s.sha256
            );
        }
        assert!(files > 0, "no standard-<X.Y>.ini in {}", specs_dir().display());
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

    /// An image prompt runs only on presets with an mmproj: with none
    /// selected, its steps are left out and the text prompt's stay.
    #[test]
    fn an_image_prompt_needs_a_preset_with_an_mmproj() {
        let dir = tempfile::tempdir().unwrap();
        let image = r#"{"messages": [{"role": "user", "content": [
            {"type": "image_url", "image_url": {"url": "data:image/jpeg;base64,AAAA"}},
            {"type": "text", "text": "Describe it."}]}]}"#;
        std::fs::write(dir.path().join("t.txt"), "hello").unwrap();
        std::fs::write(dir.path().join("i.json"), image).unwrap();
        let sha = |t: &str| super::super::prompt_info(t).sha256;
        let text = format!(
            "[Spec]\nName = t\nVersion = 2\nPrompts = t.txt@{}, i.json@{}\nMaxTokens = 0\n\n\
             [Pass.engine]\nTemp = 0\nConcurrency = single\nReps = 3\n",
            sha("hello"),
            sha(image)
        );
        let spec = parse(&text, dir.path()).unwrap();
        let base = Plan {
            mode: super::super::Mode::Live,
            ..Plan::default()
        };
        let steps = tab_steps(&spec, &base, &Derive::default(), None).unwrap();
        let labels: Vec<&str> = steps.iter().map(|s| s.label.as_str()).collect();
        assert_eq!(labels, ["engine · t.txt · single"]);
    }

    /// A/B over a key the engine pass overrides: every step once per value,
    /// the value over the pass's own, and the runs recorded as derived.
    #[test]
    fn an_ab_key_runs_every_step_per_value() {
        let spec = load("standard").unwrap();
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

    /// Every published version pins prompts that are shipped, with the
    /// digest they have: a shipped prompt edited in place fails here, not on
    /// someone's machine.
    #[test]
    fn the_standard_spec_pins_the_shipped_prompts() {
        for (v, _) in published_versions(STANDARD_VERSIONS) {
            let s = standard_version(&v).unwrap();
            assert!(!s.prompts.is_empty());
            for (path, pinned) in &s.prompts {
                assert_eq!(path.parent(), Some(crate::paths::bench_prompts_dir().as_path()));
                let text = super::super::load_prompt_file(path).unwrap();
                assert_eq!(&super::super::prompt_info(&text).sha256, pinned, "{}", path.display());
            }
        }
    }
}
