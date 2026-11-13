//! The content checks of the files the manager writes: `server@<id>.ini`,
//! `presets.ini` and `presets.ini.disabled`. They work on the file text, so
//! the manager runs them before every write and the configurator can run the
//! same ones for feedback while the user types. A refusal is a strings key
//! and its values, which the client shows in its own language.

use crate::ini::{self, INT_ANY, INT_NON_NEGATIVE, INT_POSITIVE};
use crate::strings;
use std::ops::RangeInclusive;

/// Why a file was refused: a key of the strings files (`cli.toml` and
/// `en_US.toml`) and the values of its `{0}`, `{1}`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Refusal {
    pub key: &'static str,
    pub values: Vec<String>,
}

/// Every key a check refuses with: `Refusal::decode` takes no other.
pub const KEYS: &[&str] = &[
    "not-whole-number-in-range",
    "comment-markers",
    "load-on-startup-over-max",
    "load-on-startup-unlimited",
    "invalid-preset-id",
    "preset-no-model",
    "preset-name-taken",
    "fit-target-format",
    "override-no-pattern",
    "override-no-device",
    "moe-cache-with-tensor-split",
];

/// The separator of `Refusal::encode`'s fields: U+001F, the unit separator.
const SEP: char = '\u{1f}';

impl Refusal {
    fn new(key: &'static str, values: &[&dyn std::fmt::Display]) -> Refusal {
        Refusal {
            key,
            values: values.iter().map(|v| v.to_string()).collect(),
        }
    }

    /// The refusal as one string, for a D-Bus error message: the key, then
    /// each value, separated by U+001F (unit separator).
    pub fn encode(&self) -> String {
        let mut out = self.key.to_string();
        for v in &self.values {
            out.push(SEP);
            out.push_str(v);
        }
        out
    }

    /// `encode`'s string back to the refusal; None when it is not one (an
    /// empty string, a key not in [`KEYS`]).
    pub fn decode(wire: &str) -> Option<Refusal> {
        let mut parts = wire.split(SEP);
        let first = parts.next()?;
        let key = KEYS.iter().find(|k| **k == first)?;
        Some(Refusal {
            key,
            values: parts.map(str::to_string).collect(),
        })
    }

    /// The text of the key with the values filled in, in the strings file
    /// this process reads.
    ///
    /// One arm per key, each with its own values, so the strings lint
    /// (`binding_lint`) holds every text to the values its rule passes.
    pub fn text(&self) -> String {
        let v = |i: usize| self.values.get(i).map_or("", String::as_str);
        match self.key {
            "not-whole-number-in-range" => {
                strings::fill("not-whole-number-in-range", &[&v(0), &v(1), &v(2), &v(3)])
            }
            "comment-markers" => strings::fill("comment-markers", &[&v(0)]),
            "load-on-startup-over-max" => {
                strings::fill("load-on-startup-over-max", &[&v(0), &v(1)])
            }
            "load-on-startup-unlimited" => strings::fill("load-on-startup-unlimited", &[&v(0)]),
            "invalid-preset-id" => strings::fill("invalid-preset-id", &[&v(0)]),
            "preset-no-model" => strings::fill("preset-no-model", &[&v(0)]),
            "preset-name-taken" => strings::fill("preset-name-taken", &[&v(0)]),
            "fit-target-format" => strings::fill("fit-target-format", &[&v(0), &v(1)]),
            "override-no-pattern" => strings::fill("override-no-pattern", &[&v(0)]),
            "override-no-device" => strings::fill("override-no-device", &[&v(0), &v(1)]),
            "moe-cache-with-tensor-split" => strings::t("moe-cache-with-tensor-split"),
            other => other.to_string(),
        }
    }
}

// ── The rules, one function each, shared with the configurator ──────────

/// llama.cpp's `--models-max` when server.ini sets none (`common_params::models_max`).
pub const LLAMA_MODELS_MAX_DEFAULT: i32 = 4;

/// The ranges llama.cpp's argument parser accepts for the `--spec-ngram-*`
/// keys (common/arg.cpp): a lookup or draft length, a draft-count bound (0 is
/// valid) and a hit count (stored as a `uint16_t`).
pub const NGRAM_SIZE: RangeInclusive<i32> = 1..=1024;
pub const NGRAM_DRAFT: RangeInclusive<i32> = 0..=1024;
pub const NGRAM_HITS: RangeInclusive<i32> = 1..=u16::MAX as i32;

/// Every integer key of a preset with the values it takes: the Models tab's
/// form reads each field with the same range (`form::int_fields`).
pub const PRESET_INT_KEYS: [(&str, RangeInclusive<i32>); 29] = [
    ("image-min-tokens", INT_POSITIVE),
    ("image-max-tokens", INT_POSITIVE),
    ("spec-draft-n-max", INT_POSITIVE),
    ("spec-draft-n-min", INT_NON_NEGATIVE),
    ("spec-draft-n-ubatch", INT_NON_NEGATIVE),
    ("moe-cache-mib", INT_NON_NEGATIVE),
    ("ctx-size", INT_POSITIVE),
    ("parallel", INT_POSITIVE),
    ("kv-unified-per-slot", INT_POSITIVE),
    ("batch-size", INT_POSITIVE),
    ("ubatch-size", INT_POSITIVE),
    ("cache-ram", INT_ANY),
    ("cache-reuse", INT_POSITIVE),
    ("ctx-checkpoints", INT_NON_NEGATIVE),
    ("checkpoint-min-step", INT_NON_NEGATIVE),
    ("fit-ctx", INT_POSITIVE),
    ("reasoning-budget", INT_ANY),
    ("n-predict", INT_ANY),
    ("top-k", INT_ANY),
    ("spec-ngram-mod-n-match", NGRAM_SIZE),
    ("spec-ngram-mod-n-min", NGRAM_DRAFT),
    ("spec-ngram-mod-n-max", NGRAM_DRAFT),
    ("spec-ngram-simple-size-n", NGRAM_SIZE),
    ("spec-ngram-simple-size-m", NGRAM_SIZE),
    ("spec-ngram-map-k-size-n", NGRAM_SIZE),
    ("spec-ngram-map-k-size-m", NGRAM_SIZE),
    ("spec-ngram-map-k4v-size-n", NGRAM_SIZE),
    ("spec-ngram-map-k4v-size-m", NGRAM_SIZE),
    ("spec-ngram-map-k4v-min-hits", NGRAM_HITS),
];

/// The ports a listener can bind.
pub const PORT: RangeInclusive<i32> = 1..=65535;

/// The integer keys of server.ini the Server tab's form refuses a mistyped
/// value for (`server_form::int_fields`), with the values each takes: a port a
/// listener can bind, any `ModelsMax` (0 = unlimited), a margin of 0 or more.
pub const SERVER_INT_KEYS: [(&str, RangeInclusive<i32>); 3] = [
    ("Port", PORT),
    ("ModelsMax", INT_ANY),
    ("ModelsMemoryMargin", INT_NON_NEGATIVE),
];

/// The range of the server.ini integer key `key` in [`SERVER_INT_KEYS`].
pub fn server_int_range(key: &str) -> Option<RangeInclusive<i32>> {
    SERVER_INT_KEYS
        .iter()
        .find(|(k, _)| *k == key)
        .map(|(_, r)| r.clone())
}

/// The range of the preset integer key `key` in [`PRESET_INT_KEYS`].
pub fn preset_int_range(key: &str) -> Option<RangeInclusive<i32>> {
    PRESET_INT_KEYS
        .iter()
        .find(|(k, _)| *k == key)
        .map(|(_, r)| r.clone())
}

/// The preset keys whose value is free text: a `;` or `#` in one would be cut
/// off as an inline comment on reload, here and by llama-server alike.
const PRESET_TEXT_KEYS: [&str; 7] = [
    "model",
    "mmproj",
    "model-draft",
    "chat-template-kwargs",
    "override-tensor",
    "reasoning-budget-message",
    "fit-target",
];

/// The server.ini keys whose value is free text (paths, a URL, a key).
const SERVER_TEXT_KEYS: [&str; 4] = ["ModelsDir", "StateDir", "OpencodeBaseUrl", "OpencodeApiKey"];

/// `text`, the value of `key`, as a whole number in `range`.
pub fn whole_number(key: &str, text: &str, range: &RangeInclusive<i32>) -> Result<(), Refusal> {
    if ini::parse_int_in(text, range).is_some() {
        return Ok(());
    }
    Err(Refusal::new(
        "not-whole-number-in-range",
        &[&key, &text.trim(), range.start(), range.end()],
    ))
}

/// No `;` or `#` in the value of `field`: the INI format has no escaping.
pub fn comment_markers(field: &str, value: &str) -> Result<(), Refusal> {
    if value.contains([';', '#']) {
        Err(Refusal::new("comment-markers", &[&field]))
    } else {
        Ok(())
    }
}

/// True if `id` uses only the section-header charset (letters, digits, `.`,
/// `-`, `_`): `[`/`]`/newline break the section structure; `;`/`#` read as a
/// comment.
pub fn valid_preset_id(id: &str) -> bool {
    id.chars()
        .all(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '-' | '_'))
}

/// A preset id that can be a section name, not empty.
pub fn preset_id(id: &str) -> Result<(), Refusal> {
    if id.is_empty() || !valid_preset_id(id) {
        Err(Refusal::new("invalid-preset-id", &[&id]))
    } else {
        Ok(())
    }
}

/// `fit-target` as llama.cpp takes it: MiB as whole numbers, one for every
/// device or one per device separated by `,`. Empty = the key is omitted.
/// llama.cpp's `std::stoull` throws on a stray word and the model never loads.
pub fn fit_target(v: &str) -> Result<(), Refusal> {
    let v = v.trim();
    if v.is_empty()
        || v.split(',')
            .all(|n| !n.trim().is_empty() && n.trim().bytes().all(|b| b.is_ascii_digit()))
    {
        Ok(())
    } else {
        Err(Refusal::new("fit-target-format", &[&v, &"MiB"]))
    }
}

/// `override-tensor` as llama.cpp splits it: rules on `,`, each at its first
/// `=` into a pattern and a device; a rule without either is a `throw` while
/// the arguments are parsed. The position is 1-based.
pub fn override_tensor(s: &str) -> Result<(), Refusal> {
    let rules = s.split(',').map(str::trim).filter(|p| !p.is_empty());
    for (i, rule) in rules.enumerate() {
        let n = i + 1;
        let (pattern, device) = rule.split_once('=').unwrap_or((rule, ""));
        let pattern = pattern.trim();
        if pattern.is_empty() {
            return Err(Refusal::new("override-no-pattern", &[&n]));
        }
        if device.trim().is_empty() {
            return Err(Refusal::new("override-no-device", &[&n, &pattern]));
        }
    }
    Ok(())
}

/// A positive `moe-cache-mib` under `split-mode = tensor` is a model that never
/// loads: `llama_moe_cache` throws "MoE cache does not support tensor
/// parallelism" at context creation. 0 is "no cache" and passes.
pub fn moe_cache_split(moe_cache_mib: Option<i32>, split_mode: &str) -> Result<(), Refusal> {
    if moe_cache_mib.is_some_and(|v| v > 0) && split_mode.trim() == "tensor" {
        Err(Refusal::new("moe-cache-with-tensor-split", &[]))
    } else {
        Ok(())
    }
}

/// The router does not start with more `load-on-startup` presets (`count`)
/// than `models_max` (`None` = llama.cpp's default), nor with any at all under
/// `ModelsMax = 0`, which its startup check compares literally instead of
/// reading it as unlimited (`server_models::load_models`).
pub fn startup_count(count: usize, models_max: Option<i32>) -> Result<(), Refusal> {
    let max = models_max.unwrap_or(LLAMA_MODELS_MAX_DEFAULT);
    if count == 0 || (max > 0 && count <= max as usize) {
        Ok(())
    } else if max <= 0 {
        Err(Refusal::new("load-on-startup-unlimited", &[&count]))
    } else {
        Err(Refusal::new("load-on-startup-over-max", &[&count, &max]))
    }
}

// ── Sections read raw ───────────────────────────────────────────────────

/// One section of an INI text: its id, its key lines (key, raw value: the
/// text after the first `=`, trimmed, comment markers included) and its
/// non-blank lines trimmed, which say whether it changed.
struct RawSection {
    id: String,
    keys: Vec<(String, String)>,
    lines: Vec<String>,
}

impl RawSection {
    /// The value of `key` as the readers see it: the last one, its inline
    /// comment cut (`ini::parse_sections`).
    fn get(&self, key: &str) -> Option<String> {
        self.keys
            .iter()
            .rev()
            .find(|(k, _)| k == key)
            .map(|(_, v)| cut_comment(v).to_string())
    }

    fn loads_on_startup(&self) -> bool {
        self.get("load-on-startup")
            .is_some_and(|v| ini::parse_bool(&v) == Some(true))
    }
}

/// Everything from the first `;` or `#` is comment (`ini::parse_sections`).
fn cut_comment(v: &str) -> &str {
    v[..v.find([';', '#']).unwrap_or(v.len())].trim_end()
}

/// The sections of `text`, by the rules of `ini::parse_sections`; what comes
/// before the first section is no section.
fn raw_sections(text: &str) -> Vec<RawSection> {
    let mut out: Vec<RawSection> = Vec::new();
    for line in text.lines() {
        let t = line.trim();
        if t.is_empty() {
            continue;
        }
        if let Some(name) = t.strip_prefix('[').and_then(|s| s.strip_suffix(']')) {
            out.push(RawSection {
                id: name.trim().to_string(),
                keys: Vec::new(),
                lines: vec![t.to_string()],
            });
            continue;
        }
        let Some(s) = out.last_mut() else { continue };
        s.lines.push(t.to_string());
        if t.starts_with(';') || t.starts_with('#') {
            continue;
        }
        if let Some((key, value)) = t.split_once('=') {
            s.keys
                .push((key.trim().to_string(), value.trim().to_string()));
        }
    }
    out
}

/// True when `sections` holds `s` unchanged: same id, same lines.
fn unchanged_in(s: &RawSection, sections: &[RawSection]) -> bool {
    sections.iter().any(|o| o.id == s.id && o.lines == s.lines)
}

/// The rules of one preset section, on its own.
fn preset_section(s: &RawSection) -> Result<(), Refusal> {
    preset_id(&s.id)?;
    if s.get("model").is_none_or(|m| m.is_empty()) {
        return Err(Refusal::new("preset-no-model", &[&s.id]));
    }
    for (key, raw) in &s.keys {
        if PRESET_TEXT_KEYS.contains(&key.as_str()) {
            comment_markers(key, raw)?;
        }
    }
    fit_target(&s.get("fit-target").unwrap_or_default())?;
    override_tensor(&s.get("override-tensor").unwrap_or_default())?;
    for (key, range) in &PRESET_INT_KEYS {
        if let Some(v) = s.get(key) {
            whole_number(key, &v, range)?;
        }
    }
    let moe = s.get("moe-cache-mib").and_then(|v| ini::parse_int(&v));
    moe_cache_split(moe, &s.get("split-mode").unwrap_or_default())
}

/// The preset rules on the sections of `new` that `changed` picks, the id
/// taken once in any letter case included. Returns whether a changed section
/// loads on startup.
fn preset_sections(
    new: &[RawSection],
    changed: impl Fn(&RawSection) -> bool,
) -> Result<bool, Refusal> {
    let mut flagged = false;
    for (j, s) in new.iter().enumerate() {
        let mine = changed(s);
        if mine {
            preset_section(s)?;
            flagged |= s.loads_on_startup();
        }
        let earlier = new[..j].iter().find(|o| o.id.eq_ignore_ascii_case(&s.id));
        if earlier.is_some_and(|o| mine || changed(o)) {
            return Err(Refusal::new("preset-name-taken", &[&s.id]));
        }
    }
    Ok(flagged)
}

fn startup_flagged(sections: &[RawSection]) -> usize {
    sections.iter().filter(|s| s.loads_on_startup()).count()
}

/// Check the text of a `server@<id>.ini`. `presets` is `presets.ini` as it
/// is now (empty when there is none): its `load-on-startup` presets must fit
/// the new `ModelsMax`.
pub fn server_file(text: &str, presets: &str) -> Result<(), Refusal> {
    let sections = raw_sections(text);
    let Some(s) = sections
        .iter()
        .find(|s| s.id.eq_ignore_ascii_case("Server"))
    else {
        return startup_count(startup_flagged(&raw_sections(presets)), None);
    };
    for (key, range) in &SERVER_INT_KEYS {
        if let Some(v) = s.get(key) {
            whole_number(key, &v, range)?;
        }
    }
    for (key, raw) in &s.keys {
        if SERVER_TEXT_KEYS.contains(&key.as_str()) {
            comment_markers(key, raw)?;
        }
    }
    let models_max = s.get("ModelsMax").and_then(|v| ini::parse_int(&v));
    startup_count(startup_flagged(&raw_sections(presets)), models_max)
}

/// Check the new text of `presets.ini` against `old`, the file as it is now
/// (empty when there is none): the preset rules on every section that is not
/// in `old` unchanged, and the `load-on-startup` count against `models_max`
/// (an instance's `ModelsMax`, None = llama.cpp's default).
pub fn presets_file(new: &str, old: &str, models_max: Option<i32>) -> Result<(), Refusal> {
    let new = raw_sections(new);
    let old = raw_sections(old);
    if preset_sections(&new, |s| !unchanged_in(s, &old))? {
        startup_count(startup_flagged(&new), models_max)?;
    }
    Ok(())
}

/// Check the new text of `presets.ini.disabled`: the preset rules on every
/// section found unchanged neither in `old` (this file now) nor in `enabled`
/// (`presets.ini` now), so switching off a preset is never refused; no
/// `load-on-startup` count, llama-server never reads this file.
pub fn presets_disabled_file(new: &str, old: &str, enabled: &str) -> Result<(), Refusal> {
    let new = raw_sections(new);
    let old = raw_sections(old);
    let enabled = raw_sections(enabled);
    preset_sections(&new, |s| {
        !unchanged_in(s, &old) && !unchanged_in(s, &enabled)
    })
    .map(|_| ())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::server_cfg::{self, ServerConfig};

    /// The keys the checks refuse with, by rule.
    const EXPECTED_KEYS: [&str; 10] = [
        "not-whole-number-in-range",
        "comment-markers",
        "load-on-startup-over-max",
        "load-on-startup-unlimited",
        "invalid-preset-id",
        "preset-no-model",
        "preset-name-taken",
        "fit-target-format",
        "override-no-pattern",
        "override-no-device",
    ];
    const MOE_TENSOR: &str = "moe-cache-with-tensor-split";

    fn refusal(key: &'static str, values: &[&str]) -> Refusal {
        Refusal {
            key,
            values: values.iter().map(|v| v.to_string()).collect(),
        }
    }

    fn range(r: &std::ops::RangeInclusive<i32>) -> (String, String) {
        (r.start().to_string(), r.end().to_string())
    }

    fn not_int(key: &'static str, text: &str, r: &std::ops::RangeInclusive<i32>) -> Refusal {
        let (lo, hi) = range(r);
        Refusal {
            key: "not-whole-number-in-range",
            values: vec![key.into(), text.into(), lo, hi],
        }
    }

    fn server(lines: &str) -> String {
        format!("[Server]\n{lines}")
    }

    /// A preset section as the configurator writes it, from a real one
    /// (Swift-1.5-Qwen3.8-27B), the paths made generic.
    const REAL_SECTION: &str = "\
[Swift-1.5-Qwen3.8-27B]\r\n\
; Generated by pai-config.\r\n\
; The model's project page: a comment, llama-server has no such key.\r\n\
; url = https://huggingface.co/example/Swift-1.5-Qwen3.8-27B-GGUF\r\n\
; placement = measured: tensor split over two cards; a starting point to tune\r\n\
model = /models/Swift-1.5-Qwen3.8-27B-Q8_0.gguf\r\n\
mmproj = /models/mmprojs/Qwen3.8-27B-UD-mmproj-BF16.gguf\r\n\
mmproj-offload = true\r\n\
mmproj-device = ROCm1\r\n\
spec-type = ngram-map-k4v,draft-mtp\r\n\
spec-draft-n-max = 2\r\n\
spec-ngram-map-k4v-size-m = 32\r\n\
ctx-size = 425984\r\n\
device = ROCm0,ROCm1\r\n\
split-mode = tensor\r\n\
tensor-split = 1,1\r\n\
parallel = 2\r\n\
ubatch-size = 1024\r\n\
cache-type-k = f16\r\n\
cache-ram = 24576\r\n\
cache-reuse = 256\r\n\
ctx-checkpoints = 12\r\n\
reasoning-budget-message = Reasoning budget reached. Stop analysing and write the final answer now.\r\n\
fit = off\r\n\
temp = 1\r\n\
top-k = 20\r\n\
top-p = 0.95\r\n";

    /// One preset section `[id]` with `model` and the extra lines.
    fn section(id: &str, lines: &str) -> String {
        format!("[{id}]\nmodel = /models/{id}.gguf\n{lines}")
    }

    fn presets(new: &str) -> Result<(), Refusal> {
        presets_file(new, "", None)
    }

    /// Every `{N}` of `text` has a value: N < `n`, and the highest is n - 1.
    fn placeholders(text: &str) -> usize {
        let mut most = 0;
        let mut rest = text;
        while let Some((_, tail)) = rest.split_once('{') {
            if let Some((n, after)) = tail.split_once('}') {
                if let Ok(n) = n.parse::<usize>() {
                    most = most.max(n + 1);
                }
                rest = after;
            } else {
                break;
            }
        }
        most
    }

    // ── The refusal itself ──────────────────────────────────────────────

    #[test]
    fn every_refusal_key_is_known_and_in_both_strings_files() {
        let cli = crate::strings::parse(include_str!("../strings/cli.toml"));
        let gui = crate::strings::parse(include_str!("../strings/en_US.toml"));
        for key in EXPECTED_KEYS.iter().chain([&MOE_TENSOR]) {
            assert!(KEYS.contains(key), "{key} missing from checks::KEYS");
            assert!(cli.contains_key(*key), "{key} missing from cli.toml");
            assert!(gui.contains_key(*key), "{key} missing from en_US.toml");
        }
    }

    #[test]
    fn the_new_number_key_takes_the_key_the_text_and_the_bounds() {
        for file in [
            include_str!("../strings/cli.toml"),
            include_str!("../strings/en_US.toml"),
        ] {
            let table = crate::strings::parse(file);
            let text = table
                .get("not-whole-number-in-range")
                .expect("not-whole-number-in-range");
            assert_eq!(placeholders(text), 4, "{text:?}");
            let named = table.get("preset-no-model").expect("preset-no-model");
            assert_eq!(placeholders(named), 1, "{named:?}");
            let taken = table.get("preset-name-taken").expect("preset-name-taken");
            assert_eq!(placeholders(taken), 1, "{taken:?}");
        }
    }

    #[test]
    fn a_refusal_round_trips_through_its_wire_form() {
        for r in [
            refusal("comment-markers", &["ModelsDir"]),
            refusal(MOE_TENSOR, &[]),
            not_int("Port", "", &(1..=65535)),
            refusal("fit-target-format", &["2048 MiB, x", "MiB"]),
            refusal("override-no-device", &["1", r"blk\.0\.ffn_up_exps = x"]),
            refusal("invalid-preset-id", &["Qwen 3 (v2) é"]),
        ] {
            let wire = r.encode();
            assert!(wire.starts_with(r.key), "key first: {wire:?}");
            assert_eq!(Refusal::decode(&wire), Some(r.clone()), "{wire:?}");
        }
    }

    #[test]
    fn the_wire_form_is_the_key_then_the_values_split_by_the_unit_separator() {
        let r = refusal("load-on-startup-over-max", &["3", "2"]);
        assert_eq!(r.encode(), "load-on-startup-over-max\u{1f}3\u{1f}2");
        assert_eq!(refusal(MOE_TENSOR, &[]).encode(), MOE_TENSOR);
    }

    #[test]
    fn decode_refuses_what_is_not_a_refusal() {
        assert_eq!(Refusal::decode(""), None);
        assert_eq!(Refusal::decode("no-such-key\u{1f}x"), None);
        assert_eq!(Refusal::decode("AddressInUse"), None);
        assert_eq!(Refusal::decode("\u{1f}ModelsDir"), None);
    }

    #[test]
    fn the_text_fills_the_values() {
        let text = refusal("comment-markers", &["StateDir"]).text();
        assert!(text.contains("StateDir"), "{text:?}");
        assert!(
            !text.contains("{0}") && text != "comment-markers",
            "{text:?}"
        );
    }

    // ── server@<id>.ini ─────────────────────────────────────────────────

    #[test]
    fn a_server_file_as_the_configurator_writes_it_passes() {
        let rich = ServerConfig {
            port: Some(9090),
            hostname: Some("0.0.0.0".into()),
            threads: Some(12),
            threads_batch: Some(12),
            models_max: Some(2),
            models_memory_margin: Some(1024),
            models_dir: Some("/models".into()),
            cuda_allreduce: Some("internal".into()),
            rocprof_attach: true,
            webui: Some(false),
            webui_mcp_proxy: Some(false),
            log_verbosity: Some(4),
            save_state_on_shutdown: Some(false),
            state_dir: Some("/var/lib/private-ai/state".into()),
            opencode_base_url: Some("https://gw.example.com/llm".into()),
            opencode_api_key: Some("sk-abc123".into()),
            ..Default::default()
        };
        assert_eq!(server_file(&server_cfg::render(&rich), ""), Ok(()));
        // Every optional key unset: the file holds commented hints only.
        // ModelsDir and StateDir explicit: their defaults resolve through
        // `paths::`, which unit tests must not touch (src/tests/mod.rs).
        let bare = ServerConfig {
            models_dir: Some("/models".into()),
            state_dir: Some("/var/lib/private-ai/state".into()),
            ..Default::default()
        };
        assert_eq!(server_file(&server_cfg::render(&bare), ""), Ok(()));
        assert_eq!(server_file("[Server]\n", ""), Ok(()));
        assert_eq!(server_file("", ""), Ok(()));
    }

    #[test]
    fn server_port_must_be_a_port() {
        for bad in ["", "x", "0", "-1", "65536", "1.5", "99999999999"] {
            assert_eq!(
                server_file(&server(&format!("Port = {bad}\n")), ""),
                Err(not_int("Port", bad, &(1..=65535))),
                "Port = {bad:?}"
            );
        }
        for good in ["1", "8080", "65535", " 9931 "] {
            assert_eq!(
                server_file(&server(&format!("Port = {good}\n")), ""),
                Ok(()),
                "Port = {good:?}"
            );
        }
        // A commented hint is no key.
        assert_eq!(
            server_file(&server("; Port = 9931  ; TCP port\n"), ""),
            Ok(())
        );
    }

    #[test]
    fn server_models_max_must_be_a_whole_number() {
        for bad in ["", "x", "1.5", "99999999999"] {
            assert_eq!(
                server_file(&server(&format!("ModelsMax = {bad}\n")), ""),
                Err(not_int("ModelsMax", bad, &crate::ini::INT_ANY)),
                "ModelsMax = {bad:?}"
            );
        }
        // 0 is unlimited, and any i32 reads as itself.
        for good in ["0", "-1", "8"] {
            assert_eq!(
                server_file(&server(&format!("ModelsMax = {good}\n")), ""),
                Ok(()),
                "ModelsMax = {good:?}"
            );
        }
    }

    #[test]
    fn server_models_memory_margin_must_not_be_negative() {
        for bad in ["-1", "x", ""] {
            assert_eq!(
                server_file(&server(&format!("ModelsMemoryMargin = {bad}\n")), ""),
                Err(not_int(
                    "ModelsMemoryMargin",
                    bad,
                    &crate::ini::INT_NON_NEGATIVE
                )),
                "ModelsMemoryMargin = {bad:?}"
            );
        }
        for good in ["0", "2048"] {
            assert_eq!(
                server_file(&server(&format!("ModelsMemoryMargin = {good}\n")), ""),
                Ok(())
            );
        }
    }

    /// The fields the Server tab does not refuse today stay lenient: a slider
    /// value, the clamped log level, a bool the reader drops.
    #[test]
    fn server_keys_with_no_check_today_pass() {
        let text = server("Threads = x\nThreadsBatch = -3\nLogVerbosity = 7\nWebui = maybe\n");
        assert_eq!(server_file(&text, ""), Ok(()));
    }

    #[test]
    #[allow(clippy::type_complexity)]
    fn server_paths_must_hold_no_comment_marker() {
        let base = || ServerConfig {
            models_dir: Some("/models".into()),
            state_dir: Some("/var/lib/private-ai/state".into()),
            ..Default::default()
        };
        let cases: [(&str, fn(&mut ServerConfig, &str), &[&str]); 4] = [
            (
                "ModelsDir",
                |c, v| c.models_dir = Some(v.into()),
                &[r"C:\Models #1", r"C:\a;b"],
            ),
            (
                "StateDir",
                |c, v| c.state_dir = Some(v.into()),
                &[r"E:\state #1", r"E:\a;b"],
            ),
            (
                "OpencodeBaseUrl",
                |c, v| c.opencode_base_url = Some(v.into()),
                &["https://gw;bad"],
            ),
            (
                "OpencodeApiKey",
                |c, v| c.opencode_api_key = Some(v.into()),
                &["sk#bad"],
            ),
        ];
        for (field, set, hostile) in cases {
            for v in hostile {
                let mut cfg = base();
                set(&mut cfg, v);
                assert_eq!(
                    server_file(&server_cfg::render(&cfg), ""),
                    Err(refusal("comment-markers", &[field])),
                    "{field} = {v:?}"
                );
            }
        }
    }

    #[test]
    fn server_models_max_must_hold_the_presets_that_load_on_startup() {
        let two = format!(
            "{}{}{}",
            section("a", "load-on-startup = true\n"),
            section("b", "load-on-startup = true\n"),
            section("c", "load-on-startup = false\n"),
        );
        assert_eq!(
            server_file(&server("ModelsMax = 1\n"), &two),
            Err(refusal("load-on-startup-over-max", &["2", "1"]))
        );
        assert_eq!(
            server_file(&server("ModelsMax = 0\n"), &two),
            Err(refusal("load-on-startup-unlimited", &["2"]))
        );
        assert_eq!(server_file(&server("ModelsMax = 2\n"), &two), Ok(()));
        // No ModelsMax: llama.cpp's default, 4.
        let five: String = ["a", "b", "c", "d", "e"]
            .iter()
            .map(|id| section(id, "load-on-startup = true\n"))
            .collect();
        assert_eq!(
            server_file(&server(""), &five),
            Err(refusal("load-on-startup-over-max", &["5", "4"]))
        );
        let four: String = ["a", "b", "c", "d"]
            .iter()
            .map(|id| section(id, "load-on-startup = true\n"))
            .collect();
        assert_eq!(server_file(&server(""), &four), Ok(()));
        // None loading at startup: ModelsMax = 0 is fine.
        assert_eq!(
            server_file(&server("ModelsMax = 0\n"), &section("a", "")),
            Ok(())
        );
    }

    // ── presets.ini: one test per rule ──────────────────────────────────

    #[test]
    fn presets_as_the_configurator_writes_them_pass() {
        assert_eq!(presets(REAL_SECTION), Ok(()));
        let header = "; Reconstructed by hand: a comment before any section.\n\n";
        assert_eq!(presets(&format!("{header}{REAL_SECTION}")), Ok(()));
        assert_eq!(presets(""), Ok(()));
    }

    #[test]
    fn a_preset_id_must_be_a_section_name() {
        for bad in ["", "has space", "a;b", "sec]tion"] {
            assert_eq!(
                presets(&section(bad, "")),
                Err(refusal("invalid-preset-id", &[bad])),
                "[{bad}]"
            );
        }
        for good in ["qwen3-coder.q8_0", "Swift-1.5-Qwen3.8-27B", "a_b"] {
            assert_eq!(presets(&section(good, "")), Ok(()), "[{good}]");
        }
    }

    #[test]
    fn a_preset_must_name_its_model() {
        assert_eq!(
            presets("[m]\nctx-size = 8192\n"),
            Err(refusal("preset-no-model", &["m"]))
        );
        assert_eq!(
            presets("[m]\nmodel =\nctx-size = 8192\n"),
            Err(refusal("preset-no-model", &["m"]))
        );
    }

    #[test]
    fn a_preset_id_is_taken_once_in_any_letter_case() {
        let text = format!("{}{}", section("Qwen3-8B", ""), section("qwen3-8b", ""));
        assert_eq!(
            presets(&text),
            Err(refusal("preset-name-taken", &["qwen3-8b"]))
        );
    }

    #[test]
    fn preset_free_text_fields_must_hold_no_comment_marker() {
        for (field, hostile) in [
            ("model", r"C:\Models #1\m.gguf"),
            ("mmproj", r"C:\a;b\m-mmproj.gguf"),
            ("model-draft", r"C:\models\m #mtp.gguf"),
            ("chat-template-kwargs", r##"{"tag":"#think"}"##),
            ("override-tensor", r"blk\.[0-9#]+\.attn=CPU"),
            (
                "reasoning-budget-message",
                "Stop now; write the final answer.",
            ),
            // The parsed value (2048) would pass the fit-target format.
            ("fit-target", "2048 ; per GPU"),
        ] {
            let text = if field == "model" {
                format!("[m]\nmodel = {hostile}\n")
            } else {
                section("m", &format!("{field} = {hostile}\n"))
            };
            assert_eq!(
                presets(&text),
                Err(refusal("comment-markers", &[field])),
                "{field} = {hostile:?}"
            );
        }
        let clean = section(
            "m",
            "mmproj = C:\\models\\m-mmproj.gguf\n\
             model-draft = C:\\models\\mtps\\m-mtp.gguf\n\
             chat-template-kwargs = {\"enable_thinking\":true}\n\
             override-tensor = token_embd\\.weight=ROCm0\n\
             fit-target = 2048,1024\n",
        );
        assert_eq!(presets(&clean), Ok(()));
    }

    #[test]
    fn fit_target_is_whole_mib_per_device() {
        for bad in ["1,x", "2048MiB", "1,,2", "-1"] {
            assert_eq!(
                presets(&section("m", &format!("fit-target = {bad}\n"))),
                Err(refusal("fit-target-format", &[bad, "MiB"])),
                "fit-target = {bad:?}"
            );
        }
        for good in ["", "2048", "2048,1024", "2048 , 1024"] {
            assert_eq!(
                presets(&section("m", &format!("fit-target = {good}\n"))),
                Ok(()),
                "fit-target = {good:?}"
            );
        }
    }

    #[test]
    fn every_override_tensor_rule_names_a_pattern_and_a_device() {
        assert_eq!(
            presets(&section("m", "override-tensor = blk\\.0\\.ffn_up_exps\n")),
            Err(refusal(
                "override-no-device",
                &["1", r"blk\.0\.ffn_up_exps"]
            ))
        );
        assert_eq!(
            presets(&section(
                "m",
                "override-tensor = blk\\.1\\.attn=CPU,=ROCm0\n"
            )),
            Err(refusal("override-no-pattern", &["2"]))
        );
        assert_eq!(
            presets(&section(
                "m",
                "override-tensor = blk\\.1\\.attn=CPU, ffn_.*_exps=ROCm1\n"
            )),
            Ok(())
        );
    }

    #[test]
    fn the_moe_cache_is_refused_under_split_mode_tensor() {
        assert_eq!(
            presets(&section("m", "moe-cache-mib = 4096\nsplit-mode = tensor\n")),
            Err(refusal(MOE_TENSOR, &[]))
        );
        assert_eq!(
            presets(&section("m", "moe-cache-mib = 0\nsplit-mode = tensor\n")),
            Ok(())
        );
        for mode in ["", "layer", "row", "none"] {
            assert_eq!(
                presets(&section(
                    "m",
                    &format!("moe-cache-mib = 4096\nsplit-mode = {mode}\n")
                )),
                Ok(()),
                "split-mode = {mode:?}"
            );
        }
    }

    #[test]
    fn the_presets_that_load_on_startup_fit_models_max() {
        let flagged = |ids: &[&str]| -> String {
            ids.iter()
                .map(|id| section(id, "load-on-startup = true\n"))
                .collect()
        };
        assert_eq!(
            presets_file(&flagged(&["a", "b", "c"]), "", Some(2)),
            Err(refusal("load-on-startup-over-max", &["3", "2"]))
        );
        assert_eq!(presets_file(&flagged(&["a", "b"]), "", Some(2)), Ok(()));
        assert_eq!(
            presets_file(&flagged(&["a"]), "", Some(0)),
            Err(refusal("load-on-startup-unlimited", &["1"]))
        );
        assert_eq!(presets_file(&section("a", ""), "", Some(0)), Ok(()));
        let d = flagged(&["a", "b", "c", "d"]);
        assert_eq!(presets_file(&d, "", None), Ok(()));
        assert_eq!(
            presets_file(&flagged(&["a", "b", "c", "d", "e"]), "", None),
            Err(refusal("load-on-startup-over-max", &["5", "4"]))
        );
        // Only `true` loads on startup.
        let off = format!(
            "{}{}",
            flagged(&["a"]),
            section("b", "load-on-startup = false\n")
        );
        assert_eq!(presets_file(&off, "", Some(1)), Ok(()));
    }

    /// The integer keys of a preset, with the values each takes: the same
    /// ranges the Models tab's form reads them with.
    #[test]
    fn preset_integer_keys_hold_whole_numbers_in_their_range() {
        use crate::ini::{INT_ANY, INT_NON_NEGATIVE, INT_POSITIVE};
        let ngram_size = 1..=1024;
        let ngram_draft = 0..=1024;
        let ngram_hits = 1..=u16::MAX as i32;
        let keys: [(&'static str, std::ops::RangeInclusive<i32>); 29] = [
            ("image-min-tokens", INT_POSITIVE),
            ("image-max-tokens", INT_POSITIVE),
            ("spec-draft-n-max", INT_POSITIVE),
            ("spec-draft-n-min", INT_NON_NEGATIVE),
            ("spec-draft-n-ubatch", INT_NON_NEGATIVE),
            ("moe-cache-mib", INT_NON_NEGATIVE),
            ("ctx-size", INT_POSITIVE),
            ("parallel", INT_POSITIVE),
            ("kv-unified-per-slot", INT_POSITIVE),
            ("batch-size", INT_POSITIVE),
            ("ubatch-size", INT_POSITIVE),
            ("cache-ram", INT_ANY),
            ("cache-reuse", INT_POSITIVE),
            ("ctx-checkpoints", INT_NON_NEGATIVE),
            ("checkpoint-min-step", INT_NON_NEGATIVE),
            ("fit-ctx", INT_POSITIVE),
            ("reasoning-budget", INT_ANY),
            ("n-predict", INT_ANY),
            ("top-k", INT_ANY),
            ("spec-ngram-mod-n-match", ngram_size.clone()),
            ("spec-ngram-mod-n-min", ngram_draft.clone()),
            ("spec-ngram-mod-n-max", ngram_draft.clone()),
            ("spec-ngram-simple-size-n", ngram_size.clone()),
            ("spec-ngram-simple-size-m", ngram_size.clone()),
            ("spec-ngram-map-k-size-n", ngram_size.clone()),
            ("spec-ngram-map-k-size-m", ngram_size.clone()),
            ("spec-ngram-map-k4v-size-n", ngram_size.clone()),
            ("spec-ngram-map-k4v-size-m", ngram_size.clone()),
            ("spec-ngram-map-k4v-min-hits", ngram_hits.clone()),
        ];
        for (key, r) in keys {
            let mut bad: Vec<String> = vec!["".into(), "x".into(), "1.5".into()];
            bad.push("196608245760".into());
            if *r.start() > i32::MIN {
                bad.push((*r.start() as i64 - 1).to_string());
            }
            if *r.end() < i32::MAX {
                bad.push((*r.end() as i64 + 1).to_string());
            }
            for text in &bad {
                assert_eq!(
                    presets(&section("m", &format!("{key} = {text}\n"))),
                    Err(not_int(key, text, &r)),
                    "{key} = {text:?}"
                );
            }
            for good in [*r.start(), *r.end()] {
                assert_eq!(
                    presets(&section("m", &format!("{key} = {good}\n"))),
                    Ok(()),
                    "{key} = {good}"
                );
            }
        }
    }

    /// Keys the Models tab does not refuse today stay lenient: a float the
    /// reader drops, a bool, a word llama.cpp checks itself.
    #[test]
    fn preset_keys_with_no_check_today_pass() {
        let text = section(
            "m",
            "temp = warm\nload-on-startup = yes\ncache-type-k = q9\nsplit-mode = diagonal\n",
        );
        assert_eq!(presets(&text), Ok(()));
    }

    // ── presets.ini: only what the write changes is checked ────────────

    /// A hand-edited section already on disk does not block saving another:
    /// today a save checks only the preset it writes.
    #[test]
    fn a_section_unchanged_from_the_file_on_disk_is_not_checked() {
        let broken = "[old]\nmodel = /models/old.gguf\nctx-size = abc\n";
        let added = format!("{broken}{}", section("new", "ctx-size = 8192\n"));
        assert_eq!(presets_file(&added, broken, None), Ok(()));
        // The same section edited, still broken: checked.
        let edited = "[old]\nmodel = /models/old.gguf\nctx-size = abc\nparallel = 2\n";
        assert_eq!(
            presets_file(edited, broken, None),
            Err(not_int("ctx-size", "abc", &crate::ini::INT_POSITIVE))
        );
        // A new section with a problem: checked.
        let bad_new = format!("{broken}{}", section("new", "ctx-size = 0\n"));
        assert_eq!(
            presets_file(&bad_new, broken, None),
            Err(not_int("ctx-size", "0", &crate::ini::INT_POSITIVE))
        );
    }

    /// Only a preset that loads on startup and is written now can push the
    /// count over: a save of another is never blocked by the ones on disk.
    #[test]
    fn the_startup_count_blocks_only_a_changed_preset_that_loads_on_startup() {
        let over: String = ["a", "b", "c"]
            .iter()
            .map(|id| section(id, "load-on-startup = true\n"))
            .collect();
        let plus_plain = format!("{over}{}", section("d", ""));
        assert_eq!(presets_file(&plus_plain, &over, Some(2)), Ok(()));
        let plus_flagged = format!("{over}{}", section("d", "load-on-startup = true\n"));
        assert_eq!(
            presets_file(&plus_flagged, &over, Some(2)),
            Err(refusal("load-on-startup-over-max", &["4", "2"]))
        );
        // Switching one on: new in presets.ini, so checked.
        let two: String = ["a", "b"]
            .iter()
            .map(|id| section(id, "load-on-startup = true\n"))
            .collect();
        assert_eq!(
            presets_file(&over, &two, Some(2)),
            Err(refusal("load-on-startup-over-max", &["3", "2"]))
        );
    }

    // ── presets.ini.disabled ────────────────────────────────────────────

    #[test]
    fn a_disabled_preset_written_now_gets_the_preset_rules() {
        let bad = "[m]\nmodel = C:\\Models #1\\m.gguf\n";
        assert_eq!(
            presets_disabled_file(bad, "", ""),
            Err(refusal("comment-markers", &["model"]))
        );
        assert_eq!(
            presets_disabled_file(&section("m", "ctx-size = 0\n"), "", ""),
            Err(not_int("ctx-size", "0", &crate::ini::INT_POSITIVE))
        );
        let header = "; Presets switched OFF in pai-config.\r\n";
        assert_eq!(
            presets_disabled_file(&format!("{header}{REAL_SECTION}"), "", ""),
            Ok(())
        );
    }

    #[test]
    fn disabled_presets_have_no_startup_count() {
        let five: String = ["a", "b", "c", "d", "e"]
            .iter()
            .map(|id| section(id, "load-on-startup = true\n"))
            .collect();
        assert_eq!(presets_disabled_file(&five, "", ""), Ok(()));
    }

    /// Switching off a hand-edited preset llama-server refuses is how the
    /// server gets to start again: never refused.
    #[test]
    fn switching_off_a_broken_preset_is_never_refused() {
        let broken = "[old]\nmodel = /models/old.gguf\nctx-size = abc\n";
        assert_eq!(presets_disabled_file(broken, "", broken), Ok(()));
        assert_eq!(presets_disabled_file(broken, broken, ""), Ok(()));
        let edited = "[old]\nmodel = /models/old.gguf\nctx-size = abc\nparallel = 2\n";
        assert_eq!(
            presets_disabled_file(edited, broken, broken),
            Err(not_int("ctx-size", "abc", &crate::ini::INT_POSITIVE))
        );
    }
}
