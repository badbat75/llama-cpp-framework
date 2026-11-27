//! Versioned benchmark specifications (`bench run --spec
//! standard|standard-<X.Y>|<file>`). Only a published version of `standard`
//! (`standard.versions`) is a spec run;
//! a file whose rules are not one runs the same way but records every run as
//! `spec = custom`, derived from it (`Spec::published`).
//!
//! A spec is the method of a measurement written down: which prompts, which
//! passes (temperature, repetitions, concurrency, the model profile keys a pass
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
//! each written to the model profile for the pass and put back after it, with a
//! pending marker like a sweep's own key. `Concurrency = all` is single, then
//! every concurrent scenario when the model profile has two slots or more. A prompt
//! with an image (`bench::prompt_has_image`) runs only on model profiles that load an
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
    /// `Some(None)` = the model profile's own temperature.
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
    /// `None` = the model profile's own temperature.
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
            .ok_or_else(|| strings::t("spec-no-published-version"))?;
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
    let bytes = std::fs::read(path)
        .map_err(|e| strings::fill("cannot-read-file", &[&path.display(), &e]))?;
    let text = String::from_utf8_lossy(&bytes);
    parse(&text, &crate::paths::config_dir()).map_err(|e| format!("{}: {e}", path.display()))
}

/// Parse a spec's text; `config` resolves bare prompt names.
pub fn parse(text: &str, config: &Path) -> Result<Spec, String> {
    let sections = ini::parse_sections(text);
    let head = sections
        .iter()
        .find(|s| s.id == "Spec")
        .ok_or_else(|| strings::t("no-spec-section"))?;
    let get = |k: &str| head.keys.get(k).map(|v| v.trim().to_string());
    let name = get("Name")
        .filter(|v| !v.is_empty())
        .ok_or_else(|| strings::t("spec-no-name"))?;
    if name == "custom" {
        return Err(strings::t("spec-name-reserved"));
    }
    let version = get("Version")
        .filter(|v| !v.is_empty())
        .ok_or_else(|| strings::t("spec-no-version"))?;
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
            Some(v) => {
                Some(ini::parse_float(v).ok_or_else(|| ctx(strings::fill("spec-temp", &[&v])))?)
            }
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
            if !prompts.iter().any(|(f, _)| {
                f.file_name()
                    .is_some_and(|n| n.to_string_lossy() == p.as_str())
            }) {
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
        out.push_str(&format!(
            "image-size={}\n",
            super::image_size_str(Some(size))
        ));
    }
    for (path, sha) in &spec.prompts {
        let file = path.file_name().unwrap_or_default().to_string_lossy();
        out.push_str(&format!("prompt={file}@{sha}\n"));
    }
    for p in &spec.passes {
        let overrides: Vec<String> = p
            .overrides
            .iter()
            .map(|(k, v)| format!("{k}={v}"))
            .collect();
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
pub const STANDARD_VERSIONS: &str = include_str!("../../../../benchmarks/specs/standard.versions");

/// `standard` at a published `version`: its installed file
/// (`specs\standard-<version>.ini`), refused unless it IS that version and
/// matches its digest in `standard.versions`.
pub fn standard_version(version: &str) -> Result<Spec, String> {
    let published = published_versions(STANDARD_VERSIONS);
    if !published.iter().any(|(v, _)| v == version) {
        return Err(strings::fill(
            "spec-not-published",
            &[&version, &"standard.versions"],
        ));
    }
    let path = specs_dir().join(format!("standard-{version}.ini"));
    let spec = load_file(&path)?;
    if spec.name != "standard" || spec.version != version {
        return Err(strings::fill(
            "spec-file-mismatch",
            &[&path.display(), &spec.name, &spec.version, &version],
        ));
    }
    if !spec.published {
        return Err(strings::fill(
            "spec-digest-mismatch",
            &[&path.display(), &version, &"standard.versions"],
        ));
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
                Some(dir) if dir == config || dir == crate::paths::bench_prompts_dir() => p
                    .file_name()
                    .unwrap_or_default()
                    .to_string_lossy()
                    .into_owned(),
                _ => p.to_string_lossy().into_owned(),
            };
            format!("{shown}@{sha}")
        })
        .collect();
    let mut out = format!(
        "; A benchmark spec of your own (saved by pai-config). It is not a\n\
         ; published version of standard, so every run of it records spec = custom,\n\
         ; derived from it. Format: benchmarks/README.md, \"Specs\".\n\
         [Spec]\nName = {}\nVersion = {}\nPrompts = {}\nMaxTokens = {}\n",
        spec.name,
        spec.version,
        prompts.join(", "),
        spec.max_tokens
    );
    if spec.image_size.is_some() {
        out.push_str(&format!(
            "ImageSize = {}\n",
            super::image_size_str(spec.image_size)
        ));
    }
    for p in &spec.passes {
        out.push_str(&format!(
            "\n[Pass.{}]\nTemp = {}\n",
            p.name,
            p.temp.map_or("preset".to_string(), |t| t.to_string())
        ));
        if !p.overrides.is_empty() {
            let o: Vec<String> = p
                .overrides
                .iter()
                .map(|(k, v)| format!("{k}={v}"))
                .collect();
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
/// `base` carries the model profile, key, values, apply, campaign and question; its
/// plan's method fields are replaced by the spec's.
/// `only`: the file names of the spec's prompts to run, empty = all; a
/// selection of its legs, not a change of method (each leg is recorded as the
/// spec's), so a new version can run just the legs it added.
pub fn run(spec: &Spec, base: &Opts, derive: &Derive, only: &[String]) -> Result<String, String> {
    if base.plan.mode != super::Mode::Live {
        return Err(strings::t("spec-flag-live-only"));
    }
    if base.apply != Apply::Original && spec.passes.len() * spec.prompts.len() > 1 {
        return Err(strings::t("spec-flag-no-apply"));
    }
    // Anything that is not the spec's own rules makes every run custom,
    // derived from it: a flag given beside --spec, a swept key that a pass
    // sets (the leg's value goes over the pass's), a file that is no
    // published version.
    let (mut prompts, derived) = prompts_and_derived(spec, derive, base.key.as_deref())?;
    if !only.is_empty() {
        for o in only {
            if !prompts.iter().any(|p| prompt_name(p) == *o) {
                return Err(strings::fill("only-flag-unknown", &[&o]));
            }
        }
        prompts.retain(|p| only.contains(&prompt_name(p)));
    }
    if !derived.is_empty() {
        println!(
            "{}",
            strings::fill(
                "spec-derived-notice",
                &[&spec.name, &spec.version, &derived.join(", ")]
            )
        );
    }
    let campaign = if base.campaign.is_empty() {
        default_campaign(spec)
    } else {
        base.campaign.clone()
    };
    let mut problems = Vec::new();
    for prompt in &prompts {
        if super::prompt_has_image(&prompt.1) && !sweep::has_mmproj(&base.model_profile)? {
            println!(
                "\n{}",
                strings::fill(
                    "image-prompt-skipped",
                    &[&prompt_name(prompt), &base.model_profile]
                )
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
                sweep::all_scenarios(&base.model_profile)?,
            )?;
            let restore = apply_overrides(&base.model_profile, &pass.overrides)?;
            for c in scenarios {
                println!(
                    "\n{}",
                    strings::fill(
                        "spec-leg-heading",
                        &[
                            &spec.name,
                            &spec.version,
                            &pass.name,
                            &prompt_name(prompt),
                            &c.as_str()
                        ],
                    )
                );
                let opts = Opts {
                    plan: pass_plan(spec, pass, prompt, c, &base.plan, derive, &derived),
                    campaign: campaign.clone(),
                    ..base.clone()
                };
                if let Err(e) = sweep::run(&opts) {
                    problems.push(strings::fill(
                        "spec-leg-failed",
                        &[&pass.name, &c.as_str(), &e],
                    ));
                }
            }
            if let Err(e) = restore_overrides(&base.model_profile, restore) {
                problems.push(e);
                // A key not put back is a model profile left measuring something
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
/// model profile's). A warm-prefix pass needs a prompt with an agent turn to cache up
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
    if key.is_some_and(|k| {
        spec.passes
            .iter()
            .any(|p| p.overrides.iter().any(|(o, _)| o == k))
    }) {
        derived.push("override".into());
    }
    if !spec.published {
        derived.push("spec-file".into());
    }
    Ok((prompts, derived))
}

fn prompt_name(p: &Prompt) -> String {
    match &p.0 {
        Some(f) => f
            .file_name()
            .unwrap_or_default()
            .to_string_lossy()
            .into_owned(),
        None => "--prompt".into(),
    }
}

/// The Benchmark tab's spec run: one step per prompt x pass x scenario over the
/// tab's model profiles (`base.model_profiles`, its live mode and campaign), each pass's
/// overrides written to every model profile for its steps. `Concurrency = all` runs the
/// scenarios every selected model profile supports (single alone when one has
/// `parallel` < 2). An image prompt's steps run only the selected model profiles
/// that load an mmproj, and none when no model profile does. `derive` replaces spec
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
        // An image prompt runs on the model profiles with an mmproj, and not at all
        // when none has one.
        let mut base = base.clone();
        if super::prompt_has_image(&prompt.1) {
            let mut with = Vec::new();
            for p in &base.model_profiles {
                if sweep::has_mmproj(p)? {
                    with.push(p.clone());
                }
            }
            base.model_profiles = with;
            if base.model_profiles.is_empty() {
                continue;
            }
        }
        let base = &base;
        let mut all = Concurrency::ALL.to_vec();
        for p in &base.model_profiles {
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
/// written to the model profiles for it (over a pass's own override of the same key,
/// which `tab_steps` then records as derived), value first in the label.
pub fn ab_expand(
    steps: Vec<super::exec::Step>,
    key: &str,
    values: &[String],
) -> Vec<super::exec::Step> {
    let mut out = Vec::new();
    for v in values {
        for st in &steps {
            let mut overrides: Vec<(String, String)> = st
                .overrides
                .iter()
                .filter(|(k, _)| k != key)
                .cloned()
                .collect();
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
            return Err(strings::fill(
                "spec-prompt-changed",
                &[&p.display(), &actual, &spec.name, &spec.version, pinned],
            ));
        }
        prompts.push((p.clone(), text));
    }
    Ok(prompts)
}

/// The overrides as they are written: `spec-type = none` also removes
/// `model-draft`, unless the same list sets it. llama-server loads a draft file
/// whatever the speculator is, and without `draft-mtp` an MTP head takes a KV
/// cache for every attention layer of the context, far more than with it, so a pass meant to measure no drafter
/// can fail to load on a model profile that fits with one.
fn effective_overrides(overrides: &[(String, String)]) -> Vec<(String, String)> {
    let mut out = overrides.to_vec();
    let draft_off = overrides
        .iter()
        .any(|(k, v)| k == "spec-type" && v == "none");
    if draft_off && !overrides.iter().any(|(k, _)| k == "model-draft") {
        out.push(("model-draft".to_string(), sweep::UNSET.to_string()));
    }
    out
}

/// Write each override, a pending marker first, so an interrupted run is
/// reported like an interrupted sweep. Returns what to put back.
pub(super) fn apply_overrides(
    model_profile: &str,
    overrides: &[(String, String)],
) -> Result<Vec<(String, String, PathBuf)>, String> {
    let mut restore = Vec::new();
    for (key, value) in &effective_overrides(overrides) {
        let (original, marker) = sweep::hold_key(model_profile, key)?;
        if let Err(e) = sweep::set_key(model_profile, key, value) {
            let _ = restore_overrides(model_profile, restore);
            super::interrupt::release(&marker);
            return Err(strings::fill("override-failed", &[&key, &value, &e]));
        }
        println!(
            "{}",
            strings::fill("override-for-pass", &[&key, &value, &original])
        );
        restore.push((key.clone(), original, marker));
    }
    Ok(restore)
}

pub(super) fn restore_overrides(
    model_profile: &str,
    restore: Vec<(String, String, PathBuf)>,
) -> Result<(), String> {
    for (key, original, marker) in restore.into_iter().rev() {
        sweep::set_key(model_profile, &key, &original)
            .map_err(|e| strings::fill("override-not-restored", &[&key, &original, &e]))?;
        super::interrupt::release(&marker);
    }
    Ok(())
}

#[cfg(test)]
#[path = "spec_tests.rs"]
mod tests;
