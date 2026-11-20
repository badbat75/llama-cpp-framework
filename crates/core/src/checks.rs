//! The content checks of the files the manager writes: `server@<id>.ini`,
//! `models.ini` and `models.ini.disabled`. They work on the file text, so
//! the manager runs them before every write and the configurator can run the
//! same ones for feedback while the user types. A refusal is a strings key
//! and its values, which the client shows in its own language.

use crate::gguf_shape;
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
    "bind-all-with-others",
    "comment-markers",
    "load-on-startup-over-max",
    "load-on-startup-unlimited",
    "invalid-model-profile-id",
    "model-profile-no-model",
    "model-profile-name-taken",
    "fit-target-format",
    "override-no-pattern",
    "override-no-device",
    "moe-cache-with-tensor-split",
    "not-a-number",
    "not-one-of",
    "not-json-object",
    "min-above-max",
    "no-model-profile-named",
    "model-profile-bad-value",
    "model-profile-model-missing",
    "model-profile-over-memory",
    "server-bad-tags",
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
            "bind-all-with-others" => strings::t("bind-all-with-others"),
            "comment-markers" => strings::fill("comment-markers", &[&v(0)]),
            "load-on-startup-over-max" => {
                strings::fill("load-on-startup-over-max", &[&v(0), &v(1)])
            }
            "load-on-startup-unlimited" => strings::fill("load-on-startup-unlimited", &[&v(0)]),
            "invalid-model-profile-id" => strings::fill("invalid-model-profile-id", &[&v(0)]),
            "model-profile-no-model" => strings::fill("model-profile-no-model", &[&v(0)]),
            "model-profile-name-taken" => strings::fill("model-profile-name-taken", &[&v(0)]),
            "fit-target-format" => strings::fill("fit-target-format", &[&v(0), &v(1)]),
            "override-no-pattern" => strings::fill("override-no-pattern", &[&v(0)]),
            "override-no-device" => strings::fill("override-no-device", &[&v(0), &v(1)]),
            "moe-cache-with-tensor-split" => strings::t("moe-cache-with-tensor-split"),
            "not-a-number" => strings::fill("not-a-number", &[&v(0), &v(1)]),
            "not-one-of" => strings::fill("not-one-of", &[&v(0), &v(1), &v(2)]),
            "not-json-object" => strings::fill("not-json-object", &[&v(0)]),
            "min-above-max" => strings::fill("min-above-max", &[&v(0), &v(1), &v(2), &v(3)]),
            "no-model-profile-named" => strings::fill("no-model-profile-named", &[&v(0)]),
            "model-profile-bad-value" => strings::fill("model-profile-bad-value", &[&v(0)]),
            "model-profile-model-missing" => {
                strings::fill("model-profile-model-missing", &[&v(0), &v(1), &v(2)])
            }
            "model-profile-over-memory" => {
                strings::fill("model-profile-over-memory", &[&v(0), &v(1), &v(2), &v(3)])
            }
            "server-bad-tags" => {
                strings::fill("server-bad-tags", &[&v(0), &v(1), &v(2), &v(3), &v(4)])
            }
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

/// Every integer key of a model profile with the values it takes: the Models tab's
/// form reads each field with the same range (`form::int_fields`).
pub const MODEL_PROFILE_INT_KEYS: [(&str, RangeInclusive<i32>); 29] = [
    ("image-min-tokens", INT_POSITIVE),
    ("image-max-tokens", INT_POSITIVE),
    ("spec-draft-n-max", INT_POSITIVE),
    ("spec-draft-n-min", INT_NON_NEGATIVE),
    ("spec-draft-n-ubatch", INT_NON_NEGATIVE),
    ("moe-cache-mib", INT_NON_NEGATIVE),
    // 0 is llama.cpp's "the context from the model".
    ("ctx-size", INT_NON_NEGATIVE),
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

/// The float keys of a model profile: llama.cpp reads each with `std::stof`.
pub const MODEL_PROFILE_FLOAT_KEYS: [&str; 6] = [
    "temp",
    "top-p",
    "min-p",
    "repeat-penalty",
    "presence-penalty",
    "spec-draft-p-min",
];

/// Every type llama.cpp accepts for the KV cache, highest precision first
/// (`kv_cache_types` in common/arg.cpp).
pub const KV_CACHE_TYPES: &[&str] = &[
    "f32", "f16", "bf16", "f8_e4m3", "q8_0", "q5_1", "q5_0", "q4_1", "q4_0", "iq4_nl",
];

/// The values of `reasoning` llama.cpp takes (`is_truthy`, `is_falsey`,
/// `is_autoy` in common/arg.cpp).
const REASONING: &[&str] = &[
    "on", "enabled", "true", "1", "off", "disabled", "false", "0", "auto", "-1",
];

/// The enum keys of a model profile with every value llama.cpp takes for each: it
/// throws on any other and the model never loads (common/arg.cpp,
/// `common_reasoning_format_from_name` in common/chat.cpp).
pub const MODEL_PROFILE_ENUM_KEYS: [(&str, &[&str]); 10] = [
    ("split-mode", &["none", "layer", "row", "tensor"]),
    ("pooling", &["none", "mean", "cls", "last", "rank"]),
    ("cache-type-k", KV_CACHE_TYPES),
    ("cache-type-v", KV_CACHE_TYPES),
    ("spec-draft-type-k", KV_CACHE_TYPES),
    ("spec-draft-type-v", KV_CACHE_TYPES),
    ("reasoning", REASONING),
    (
        "reasoning-format",
        &["auto", "none", "deepseek", "deepseek-legacy"],
    ),
    ("lazy-mode", &["on", "auto", "off"]),
    ("load-mode", &crate::server_cfg::LOAD_MODES),
];

/// The entries of the comma-separated `spec-type` llama.cpp takes
/// (`common_speculative_type_from_name_map` in common/speculative.cpp).
pub const SPEC_TYPES: &[&str] = &[
    "none",
    "draft-simple",
    "draft-eagle3",
    "draft-mtp",
    "draft-dflash",
    "draft-dspark",
    "ngram-simple",
    "ngram-map-k",
    "ngram-map-k4v",
    "ngram-mod",
    "ngram-cache",
];

/// The model profile keys that bound a value from both sides: (minimum, maximum).
pub const MODEL_PROFILE_MIN_MAX_KEYS: [(&str, &str); 3] = [
    ("image-min-tokens", "image-max-tokens"),
    ("spec-draft-n-min", "spec-draft-n-max"),
    ("spec-ngram-mod-n-min", "spec-ngram-mod-n-max"),
];

/// The model profile keys the web may write, the table of "Saving a model profile from the
/// web" in `ARCHITECTURE.md`: every key `pai-app` `model_profiles::render_section` can
/// write that a control the model profile page enables sends. The bff's
/// `PUT /api/profiles/model/{id}` accepts a body key only when it is one of these, in
/// this exact spelling: llama-server knows a key only in its own case, so an
/// unlisted name is refused rather than written.
///
/// The `pai-app` test `web_key_lists_are_exactly_what_render_section_writes`
/// pins this list and [`MODEL_PROFILE_KEYS_NOT_ON_WEB`] against the writer: the keys of
/// a fully populated model profile, read by `ini::parse_sections`, are the union of
/// the two, and the two share no key.
pub const WEB_MODEL_PROFILE_KEYS: &[&str] = &[
    // Resource and context card.
    "ctx-size",
    "parallel",
    "kv-unified",
    "kv-unified-per-slot",
    "batch-size",
    "ubatch-size",
    // Embeddings card.
    "embeddings",
    "pooling",
    // KV cache card.
    "cache-type-k",
    "cache-type-v",
    "flash-attn",
    "cache-ram",
    "cache-reuse",
    "ctx-checkpoints",
    "checkpoint-min-step",
    // Sampling overrides card.
    "temp",
    "top-k",
    "top-p",
    "min-p",
    "repeat-penalty",
    "presence-penalty",
    "backend-sampling",
    // Advanced card.
    "load-on-startup",
    "chat-template-kwargs",
    // Chat and reasoning card.
    "jinja",
    "prefill-assistant",
    "reasoning",
    "reasoning-format",
    "reasoning-effort",
    "reasoning-preserve",
    "reasoning-budget",
    "reasoning-budget-message",
    "n-predict",
    // Vision card.
    "mmproj-offload",
    "mmproj-device",
    "image-min-tokens",
    "image-max-tokens",
    // Hardware card, the MoE keys of it in `MODEL_PROFILE_KEYS_NOT_ON_WEB`.
    "n-gpu-layers",
    "n-cpu-ffn",
    "op-offload",
    "repack",
    "no-host",
    "lazy-mode",
    "load-mode",
    "fit",
    "fit-ctx",
    "fit-target",
    "device-draft",
    "n-gpu-layers-draft",
    // Speculative decoding card.
    "spec-draft-n-max",
    "spec-draft-p-min",
    "spec-draft-n-min",
    "spec-draft-n-ubatch",
    "spec-draft-type-k",
    "spec-draft-type-v",
    "spec-ngram-mod-n-match",
    "spec-ngram-mod-n-min",
    "spec-ngram-mod-n-max",
    "spec-ngram-simple-size-n",
    "spec-ngram-simple-size-m",
    "spec-ngram-map-k-size-n",
    "spec-ngram-map-k-size-m",
    "spec-ngram-map-k4v-size-n",
    "spec-ngram-map-k4v-size-m",
    "spec-ngram-map-k4v-min-hits",
];

/// The model profile keys `pai-app` `model_profiles::render_section` writes that no control of
/// the model profile page enables yet, each with the slice that enables it in the
/// design's table: the Assets card's file picks, the MoE keys that need the
/// model's facts, the GPU distribution and the tensor placement. A body key
/// here is refused like an unknown one. A slice that enables one moves the key
/// to [`WEB_MODEL_PROFILE_KEYS`], in the same commit as its control.
pub const MODEL_PROFILE_KEYS_NOT_ON_WEB: &[&str] = &[
    // The Assets card's file picks, until it exists.
    "model",
    "mmproj",
    "model-draft",
    "spec-type",
    // The MoE keys: they need the model's facts.
    "n-cpu-moe",
    "moe-cache-mib",
    // The GPU distribution.
    "device",
    "split-mode",
    "tensor-split",
    // The tensor placement.
    "override-tensor",
];

/// The ports a listener can bind.
pub const PORT: RangeInclusive<i32> = 1..=65535;

/// The integer keys of server.ini the Server tab's form refuses a mistyped
/// value for (`server_form::int_fields`), with the values each takes: a port a
/// listener can bind, a `ModelsMax` of 0 or more (0 = unlimited), a margin of
/// 0 or more, one of llama.cpp's log levels.
pub const SERVER_INT_KEYS: [(&str, RangeInclusive<i32>); 4] = [
    ("Port", PORT),
    ("ModelsMax", INT_NON_NEGATIVE),
    ("ModelsMemoryMargin", INT_NON_NEGATIVE),
    ("LogVerbosity", LOG_VERBOSITY),
];

/// llama.cpp's log levels (`-lv`), as the Server tab's dropdown offers them.
pub const LOG_VERBOSITY: RangeInclusive<i32> = 0..=5;

/// The range of the server.ini integer key `key` in [`SERVER_INT_KEYS`].
pub fn server_int_range(key: &str) -> Option<RangeInclusive<i32>> {
    SERVER_INT_KEYS
        .iter()
        .find(|(k, _)| *k == key)
        .map(|(_, r)| r.clone())
}

/// The range of the model profile integer key `key` in [`MODEL_PROFILE_INT_KEYS`].
pub fn model_profile_int_range(key: &str) -> Option<RangeInclusive<i32>> {
    MODEL_PROFILE_INT_KEYS
        .iter()
        .find(|(k, _)| *k == key)
        .map(|(_, r)| r.clone())
}

/// The model profile keys whose value is free text: a `;` or `#` in one would be cut
/// off as an inline comment on reload, here and by llama-server alike.
const MODEL_PROFILE_TEXT_KEYS: [&str; 7] = [
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
pub fn valid_model_profile_id(id: &str) -> bool {
    id.chars()
        .all(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '-' | '_'))
}

/// A model profile id that can be a section name, not empty.
pub fn model_profile_id(id: &str) -> Result<(), Refusal> {
    if id.is_empty() || !valid_model_profile_id(id) {
        Err(Refusal::new("invalid-model-profile-id", &[&id]))
    } else {
        Ok(())
    }
}

/// A model profile names a model file: `model`, the value as read, not blank.
pub fn model_profile_model(id: &str, model: &str) -> Result<(), Refusal> {
    if model.trim().is_empty() {
        Err(Refusal::new("model-profile-no-model", &[&id]))
    } else {
        Ok(())
    }
}

/// The model files a model profile's `model` value names, in the order a check
/// looks for them: the one file as written, or every shard of a split model
/// when the value names one (a file stem ending `-NNNNN-of-NNNNN`, the names
/// llama.cpp writes, `{base}-{NNNNN}-of-{NNNNN}.{ext}`: `model_scan`,
/// `gguf::read_tensors`). A value that is not blank always names at least
/// itself, whatever its spelling. Empty for a blank value: the profile has no
/// file of its own (the `hf-repo` case, and [`model_profile_model`]'s to
/// refuse). The list is the whole of what the model-file check looks at, so a
/// repository in `hf-repo` adds nothing to look for.
pub fn model_files_of(model: &str) -> Vec<String> {
    let value = model.trim();
    if value.is_empty() {
        return Vec::new();
    }
    // The file name, after the last separator of either spelling.
    let cut = value
        .rfind('/')
        .map(|i| i + 1)
        .max(value.rfind('\\').map(|i| i + 1))
        .unwrap_or(0);
    let (dir, name) = value.split_at(cut);
    // The extension keeps its `.` so every shard name rebuilds byte for byte.
    let (stem, ext) = match name.rfind('.').filter(|dot| *dot > 0) {
        Some(dot) => (&name[..dot], &name[dot..]),
        None => (name, ""),
    };
    let Some((base, _)) = gguf_shape::split_shard_suffix(stem) else {
        return vec![value.to_string()];
    };
    // split_shard_suffix checked the 5 trailing digits of the total.
    let total = stem[stem.len() - 5..].parse::<usize>().unwrap_or(0);
    if total == 0 {
        return vec![value.to_string()];
    }
    (1..=total)
        .map(|i| format!("{dir}{base}-{i:05}-of-{total:05}{ext}"))
        .collect()
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

/// A bind list (`Hostname`, comma-separated) holds `0.0.0.0` alone: it covers
/// every IPv4 address, and llama.cpp's `--host` help calls overlapping
/// addresses undefined behaviour. Empty entries are ignored.
pub fn bind_list(hostname: &str) -> Result<(), Refusal> {
    let entries: Vec<&str> = hostname
        .split(',')
        .map(str::trim)
        .filter(|e| !e.is_empty())
        .collect();
    if entries.len() > 1 && entries.contains(&"0.0.0.0") {
        Err(Refusal::new("bind-all-with-others", &[]))
    } else {
        Ok(())
    }
}

/// The router does not start with more `load-on-startup` model profiles (`count`)
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

/// `text`, the value of `key`, as a finite number: `inf` and `NaN` parse as
/// floats but give a sampler that never works.
pub fn finite_number(key: &str, text: &str) -> Result<(), Refusal> {
    if ini::parse_float(text).is_some_and(f64::is_finite) {
        return Ok(());
    }
    Err(Refusal::new("not-a-number", &[&key, &text.trim()]))
}

/// `value` of `key` as one of `allowed`, exactly as llama.cpp compares it.
pub fn one_of(key: &str, value: &str, allowed: &[&str]) -> Result<(), Refusal> {
    if allowed.contains(&value) {
        return Ok(());
    }
    Err(Refusal::new(
        "not-one-of",
        &[&key, &value, &allowed.join(", ")],
    ))
}

/// Every entry of the comma-separated `spec-type` as one of [`SPEC_TYPES`].
pub fn spec_type(value: &str) -> Result<(), Refusal> {
    value
        .split(',')
        .try_for_each(|entry| one_of("spec-type", entry, SPEC_TYPES))
}

/// The most tags one server profile or instance carries.
pub const TAGS_MAX: usize = 8;

/// The least characters of a tag.
pub const TAG_MIN: usize = 1;

/// The most characters of a tag.
pub const TAG_MAX: usize = 32;

/// The `Tags` key as the list every reader reads: split on `,`, each entry
/// trimmed, an empty entry (an empty key, a doubled comma, a trailing one)
/// dropped, so the rule judges the list the file carries.
fn tags_list(raw: &str) -> Vec<String> {
    raw.split(',')
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(String::from)
        .collect()
}

/// The tags of a server profile or of an instance, as the `Tags` list is
/// written: `count` entries, each of [`TAG_MIN`] to [`TAG_MAX`] characters of
/// `a-z`, `0-9` and `-`, not the same tag twice. The web UI shows them, and
/// the manager never passes them to llama-server, so the list is checked here
/// and nowhere else. A list of none is what an off switch looks like, and is
/// accepted: it is the empty key, written by a save that cleared them.
pub fn tags_rule(tags: &[String]) -> Result<(), Refusal> {
    let limits = [
        TAGS_MAX.to_string(),
        TAG_MIN.to_string(),
        TAG_MAX.to_string(),
    ];
    let list = tags.join(",");
    let refuse = |reason: &str| {
        Refusal::new(
            "server-bad-tags",
            &[&reason, &list, &limits[0], &limits[1], &limits[2]],
        )
    };
    if tags.len() > TAGS_MAX {
        return Err(refuse("too many tags"));
    }
    let mut seen: Vec<&str> = Vec::with_capacity(tags.len());
    for tag in tags {
        let bad = if tag.is_empty() {
            "empty tag"
        } else if tag.len() > TAG_MAX {
            "too long"
        } else if !tag
            .chars()
            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-')
        {
            "bad characters"
        } else if seen.contains(&tag.as_str()) {
            "the same tag twice"
        } else {
            seen.push(tag.as_str());
            ""
        };
        if !bad.is_empty() {
            return Err(refuse(bad));
        }
    }
    Ok(())
}

/// `value` of `key` as one JSON object, which llama.cpp merges key by key.
pub fn json_object(key: &str, value: &str) -> Result<(), Refusal> {
    match serde_json::from_str::<serde_json::Value>(value) {
        Ok(v) if v.is_object() => Ok(()),
        _ => Err(Refusal::new("not-json-object", &[&key])),
    }
}

/// The value rules of one model profile with no GUI field of their own: the float,
/// enum and JSON keys and the min <= max pairs. `get` is the section's value
/// of a key; an empty enum or JSON value is the omitted key.
pub fn model_profile_values(get: impl Fn(&str) -> Option<String>) -> Result<(), Refusal> {
    let set = |key: &str| get(key).filter(|v| !v.is_empty());
    for key in MODEL_PROFILE_FLOAT_KEYS {
        if let Some(v) = get(key) {
            finite_number(key, &v)?;
        }
    }
    for (key, allowed) in &MODEL_PROFILE_ENUM_KEYS {
        if let Some(v) = set(key) {
            one_of(key, &v, allowed)?;
        }
    }
    if let Some(v) = set("spec-type") {
        spec_type(&v)?;
    }
    if let Some(v) = set("chat-template-kwargs") {
        json_object("chat-template-kwargs", &v)?;
    }
    for (min_key, max_key) in MODEL_PROFILE_MIN_MAX_KEYS {
        let (Some(min), Some(max)) = (get(min_key), get(max_key)) else {
            continue;
        };
        if let (Some(lo), Some(hi)) = (ini::parse_int(&min), ini::parse_int(&max)) {
            if lo > hi {
                return Err(Refusal::new(
                    "min-above-max",
                    &[&min_key, &min.trim(), &max_key, &max.trim()],
                ));
            }
        }
    }
    Ok(())
}

// ── Sections read raw ───────────────────────────────────────────────────

/// One section of an INI text: its id, its key lines (key, raw value: the
/// text after the first `=`, trimmed, comment markers included) and its
/// non-blank lines trimmed, which say whether it changed.
#[derive(Clone)]
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

/// `a` and `b` end to end: the sections of the two model profile files as they are
/// now, the text a key line can already be known from.
fn known_sections(a: &[RawSection], b: &[RawSection]) -> Vec<RawSection> {
    let mut out = a.to_vec();
    out.extend_from_slice(b);
    out
}

/// True when a section of `sections` with the id `id` holds the key line `key`
/// with the raw value `raw` exactly (id, key and value byte for byte, as
/// `raw_sections` stores them): a line no write of this text introduced, the
/// inline comment a byte-for-byte save moves whole ("Saving a model profile from the
/// web", decision c1).
fn known_key_line(sections: &[RawSection], id: &str, key: &str, raw: &str) -> bool {
    sections
        .iter()
        .any(|o| o.id == id && o.keys.iter().any(|(k, v)| k == key && v == raw))
}

/// The rules of one model profile section. `known` are the sections of the model profile
/// files as they are now: a key line one of them holds identically in the
/// section of the same id takes no `comment_markers`, every other rule runs on
/// the whole section.
fn model_profile_section(s: &RawSection, known: &[RawSection]) -> Result<(), Refusal> {
    model_profile_id(&s.id)?;
    model_profile_model(&s.id, &s.get("model").unwrap_or_default())?;
    for (key, raw) in &s.keys {
        if MODEL_PROFILE_TEXT_KEYS.contains(&key.as_str())
            && !known_key_line(known, &s.id, key, raw)
        {
            comment_markers(key, raw)?;
        }
    }
    fit_target(&s.get("fit-target").unwrap_or_default())?;
    override_tensor(&s.get("override-tensor").unwrap_or_default())?;
    for (key, range) in &MODEL_PROFILE_INT_KEYS {
        if let Some(v) = s.get(key) {
            whole_number(key, &v, range)?;
        }
    }
    model_profile_values(|key| s.get(key))?;
    let moe = s.get("moe-cache-mib").and_then(|v| ini::parse_int(&v));
    moe_cache_split(moe, &s.get("split-mode").unwrap_or_default())
}

/// The model profile rules on the sections of `new` that `changed` picks, the id
/// taken once in any letter case included, `known` the sections the key lines
/// can be known from. Returns whether a changed section loads on startup.
fn model_profile_sections(
    new: &[RawSection],
    known: &[RawSection],
    changed: impl Fn(&RawSection) -> bool,
) -> Result<bool, Refusal> {
    let mut flagged = false;
    for (j, s) in new.iter().enumerate() {
        let mine = changed(s);
        if mine {
            model_profile_section(s, known)?;
            flagged |= s.loads_on_startup();
        }
        let earlier = new[..j].iter().find(|o| o.id.eq_ignore_ascii_case(&s.id));
        if earlier.is_some_and(|o| mine || changed(o)) {
            return Err(Refusal::new("model-profile-name-taken", &[&s.id]));
        }
    }
    Ok(flagged)
}

fn startup_flagged(sections: &[RawSection]) -> usize {
    sections.iter().filter(|s| s.loads_on_startup()).count()
}

/// How many sections of `text` ask `load-on-startup`, counted as
/// `startup_count` judges them. `text` is one instance's
/// `models@<id>.ini`: `ModelsMax` belongs to the instance, so the count is
/// read here and the caller compares it with that instance's own.
pub fn startup_loads(text: &str) -> usize {
    startup_flagged(&raw_sections(text))
}

/// Check the text of a `server@<id>.ini`. `model_profiles` is `models.ini` as it
/// is now (empty when there is none): its `load-on-startup` model profiles must fit
/// the new `ModelsMax`.
pub fn server_file(text: &str, model_profiles: &str) -> Result<(), Refusal> {
    let sections = raw_sections(text);
    let Some(s) = sections
        .iter()
        .find(|s| s.id.eq_ignore_ascii_case("Server"))
    else {
        return startup_count(startup_flagged(&raw_sections(model_profiles)), None);
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
    bind_list(&s.get("Hostname").unwrap_or_default())?;
    tags_rule(&tags_list(s.get("Tags").as_deref().unwrap_or_default()))?;
    let models_max = s.get("ModelsMax").and_then(|v| ini::parse_int(&v));
    startup_count(startup_flagged(&raw_sections(model_profiles)), models_max)
}

/// Check the new text of `models.ini` against `old`, the file as it is now
/// (empty when there is none): the model profile rules on every section that is not
/// in `old` unchanged, and the `load-on-startup` count against `models_max`
/// (an instance's `ModelsMax`, None = llama.cpp's default). `other` is
/// `models.ini.disabled` as it is now: a key line that a section of the same
/// id (header exact) holds identically in `old` or in `other` is exempt from
/// `comment_markers`, the inline comment a byte-for-byte save moves whole
/// ("Saving a model profile from the web", decision c1).
pub fn models_file(
    new: &str,
    old: &str,
    other: &str,
    models_max: Option<i32>,
) -> Result<(), Refusal> {
    let new = raw_sections(new);
    let old = raw_sections(old);
    let known = known_sections(&old, &raw_sections(other));
    if model_profile_sections(&new, &known, |s| !unchanged_in(s, &old))? {
        startup_count(startup_flagged(&new), models_max)?;
    }
    Ok(())
}

/// Check the new text of `models.ini.disabled`: the model profile rules on every
/// section found unchanged neither in `old` (this file now) nor in `enabled`
/// (`models.ini` now), so switching off a model profile is never refused; no
/// `load-on-startup` count, llama-server never reads this file. A key line a
/// section of the same id (header exact) holds identically in `old` or in
/// `enabled` is exempt from `comment_markers`, as in [`models_file`].
pub fn models_disabled_file(new: &str, old: &str, enabled: &str) -> Result<(), Refusal> {
    let new = raw_sections(new);
    let old = raw_sections(old);
    let enabled = raw_sections(enabled);
    let known = known_sections(&old, &enabled);
    model_profile_sections(&new, &known, |s| {
        !unchanged_in(s, &old) && !unchanged_in(s, &enabled)
    })
    .map(|_| ())
}

/// Check the new text of one model profile file (`new`, the file now `old`) against
/// `other`, the other model profile file now: a section whose id `old` does not
/// hold (in any letter case) takes no id `other` holds, unless `other` holds
/// that very section unchanged (a switch writes the destination first).
/// Ids stay unique across `models.ini` and `models.ini.disabled`.
pub fn model_profile_ids_free(new: &str, old: &str, other: &str) -> Result<(), Refusal> {
    let old = raw_sections(old);
    let other = raw_sections(other);
    for s in raw_sections(new) {
        let id_in =
            |sections: &[RawSection]| sections.iter().any(|o| o.id.eq_ignore_ascii_case(&s.id));
        if !id_in(&old) && id_in(&other) && !unchanged_in(&s, &other) {
            return Err(Refusal::new("model-profile-name-taken", &[&s.id]));
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::server_cfg::{self, ServerConfig};

    /// The keys the checks refuse with, by rule.
    const EXPECTED_KEYS: [&str; 11] = [
        "not-whole-number-in-range",
        "bind-all-with-others",
        "comment-markers",
        "load-on-startup-over-max",
        "load-on-startup-unlimited",
        "invalid-model-profile-id",
        "model-profile-no-model",
        "model-profile-name-taken",
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

    /// A model profile section as the configurator writes it, from a real one
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

    /// One model profile section `[id]` with `model` and the extra lines.
    fn section(id: &str, lines: &str) -> String {
        format!("[{id}]\nmodel = /models/{id}.gguf\n{lines}")
    }

    fn model_profiles(new: &str) -> Result<(), Refusal> {
        models_file(new, "", "", None)
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

    /// The wire names (`ARCHITECTURE.md`, "Renamed error keys", `PLAN.md`
    /// step 10 item 13a sub-slice 4b): the key of a refusal is on the wire, so
    /// the renamed key resolves in BOTH strings files, carries the text it has
    /// today, and the old name is gone.
    #[test]
    fn the_renamed_keys_resolve_with_their_text() {
        let cli = crate::strings::parse(include_str!("../strings/cli.toml"));
        let gui = crate::strings::parse(include_str!("../strings/en_US.toml"));
        for (key, text) in [
            (
                "invalid-model-profile-id",
                "invalid model profile id `{0}`: use letters, digits, '.', '-', '_'",
            ),
            (
                "model-profile-no-model",
                "`{0}` names no model file: every model profile needs a `model` path",
            ),
            (
                "model-profile-name-taken",
                "a model profile named `{0}` already exists",
            ),
            ("no-model-profile-named", "no model profile named `{0}`"),
        ] {
            for (name, table) in [("cli.toml", &cli), ("en_US.toml", &gui)] {
                assert_eq!(
                    table.get(key).map(String::as_str),
                    Some(text),
                    "{key}: {name}"
                );
            }
        }
        for old in [
            "invalid-preset-id",
            "preset-no-model",
            "preset-name-taken",
            "no-preset-named",
        ] {
            assert!(!cli.contains_key(old), "{old} still in cli.toml");
            assert!(!gui.contains_key(old), "{old} still in en_US.toml");
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
            let named = table
                .get("model-profile-no-model")
                .expect("model-profile-no-model");
            assert_eq!(placeholders(named), 1, "{named:?}");
            let taken = table
                .get("model-profile-name-taken")
                .expect("model-profile-name-taken");
            assert_eq!(placeholders(taken), 1, "{taken:?}");
        }
    }

    /// The `comment-markers` text names the two characters and what they do
    /// to any value: the old wording said "use a path", wrong for
    /// `chat-template-kwargs` or `fit-target`, which are no path.
    #[test]
    fn the_comment_markers_text_names_the_characters_not_a_path() {
        const WANT: &str = "{0} contains ';' or '#', which starts a comment in the INI format and cuts the value short; use a value without those characters";
        for file in [
            include_str!("../strings/cli.toml"),
            include_str!("../strings/en_US.toml"),
        ] {
            let table = crate::strings::parse(file);
            let text = table.get("comment-markers").expect("comment-markers");
            assert_eq!(text, WANT);
            assert_eq!(placeholders(text), 1, "{text:?}");
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
            refusal("invalid-model-profile-id", &["Qwen 3 (v2) é"]),
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

    /// Every key a check refuses with has its own arm in [`Refusal::text`], so
    /// the client reads a sentence and never the key. The strings lint
    /// (`crates/config` `binding_lint`) counts a key of the language file as
    /// used only through that arm, so a key added to [`KEYS`] with no arm is a
    /// text the user never sees.
    #[test]
    fn every_check_key_has_its_own_text() {
        for &key in KEYS {
            let r = Refusal {
                key,
                values: (0..8).map(|i| format!("v{i}")).collect(),
            };
            let text = r.text();
            assert_ne!(text, key, "{key}: Refusal::text has no arm for it");
            assert!(
                !text.contains("{0}") && !text.contains("{1}"),
                "{key}: the text keeps a placeholder: {text:?}"
            );
        }
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

    /// A whole number of 0 or more: 0 is unlimited, and llama.cpp has no
    /// meaning for a negative cap (the GUI's field refuses one).
    #[test]
    fn server_models_max_must_be_a_whole_number_not_negative() {
        for bad in ["", "x", "1.5", "99999999999", "-1", "-4"] {
            assert_eq!(
                server_file(&server(&format!("ModelsMax = {bad}\n")), ""),
                Err(not_int("ModelsMax", bad, &crate::ini::INT_NON_NEGATIVE)),
                "ModelsMax = {bad:?}"
            );
        }
        for good in ["0", "1", "8"] {
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

    /// The fields with no check of the file's text stay lenient: the thread
    /// counts (their bound is the machine's CPU count, checked by the
    /// configurator, `pai_core::server_cfg::check`; the reader drops what is
    /// not a positive number), a bool the reader drops.
    #[test]
    fn server_keys_with_no_check_today_pass() {
        let text = server("Threads = x\nThreadsBatch = -3\nWebui = maybe\n");
        assert_eq!(server_file(&text, ""), Ok(()));
    }

    /// `LogVerbosity` is one of llama.cpp's six levels, 0 to 5 (the Server
    /// tab's dropdown); `-lv 7` would be taken as given.
    #[test]
    fn server_log_verbosity_is_a_level_from_0_to_5() {
        for bad in ["7", "6", "-1", "x", ""] {
            assert_eq!(
                server_file(&server(&format!("LogVerbosity = {bad}\n")), ""),
                Err(not_int("LogVerbosity", bad, &(0..=5))),
                "LogVerbosity = {bad:?}"
            );
        }
        for good in ["0", "3", "5", " 4 "] {
            assert_eq!(
                server_file(&server(&format!("LogVerbosity = {good}\n")), ""),
                Ok(()),
                "LogVerbosity = {good:?}"
            );
        }
    }

    /// The `Tags` key of a server profile obeys [`tags_rule`] like the
    /// record's tags of an instance do, so the file check refuses a list the
    /// rule refuses: every surface that writes a profile through the manager
    /// (the web's save, the GUI's, `pai server set`) refuses it, and nothing
    /// lands. The refusal is the rule's own, naming the list and the limits.
    /// The list is the one the reader reads, so blanks around an entry are
    /// trimmed before it is judged, as every other reader of the key trims it.
    #[test]
    fn the_server_file_check_refuses_a_tags_list_the_rule_refuses() {
        let want = |reason: &str, list: &str| Refusal {
            key: "server-bad-tags",
            values: vec![
                reason.to_string(),
                list.to_string(),
                TAGS_MAX.to_string(),
                TAG_MIN.to_string(),
                TAG_MAX.to_string(),
            ],
        };
        let too_long = "z".repeat(TAG_MAX + 1);
        let over = (0..=TAGS_MAX)
            .map(|i| format!("t{i}"))
            .collect::<Vec<_>>()
            .join(",");
        let cases: Vec<(String, &str, String)> = vec![
            ("Edge".into(), "bad characters", "Edge".into()),
            ("gpu edge".into(), "bad characters", "gpu edge".into()),
            ("gpu.edge".into(), "bad characters", "gpu.edge".into()),
            ("gpu,gpu".into(), "the same tag twice", "gpu,gpu".into()),
            // The space is the file's, not the tag's: trimmed, the entry is a
            // capital letter, and the list the refusal names is the trimmed one.
            ("Edge, v2".into(), "bad characters", "Edge,v2".into()),
            (
                format!("gpu,{too_long}"),
                "too long",
                format!("gpu,{too_long}"),
            ),
            (over.clone(), "too many tags", over),
        ];
        for (raw, reason, list) in cases {
            assert_eq!(
                server_file(&server(&format!("Tags = {raw}\n")), ""),
                Err(want(reason, &list)),
                "Tags = {raw:?}"
            );
        }
    }

    /// A list of the rule's own shape passes the file check: a plain list, a
    /// hand-spaced one, the longest tag, the most tags. The empty key is no
    /// tags, the way a save that cleared them writes it, and is not an empty
    /// tag.
    #[test]
    fn the_server_file_check_accepts_a_tags_list_of_the_rules_shape() {
        let at_limits = (0..TAGS_MAX)
            .map(|i| format!("{i}{}", "t".repeat(TAG_MAX - 1)))
            .collect::<Vec<_>>()
            .join(",");
        for raw in [
            "edge,v2",
            "",
            "   ",
            "edge , v2",
            "a",
            "0-9",
            &at_limits,
            "edge,v2\nPort = 8082",
        ] {
            assert_eq!(
                server_file(&server(&format!("Tags = {raw}\n")), ""),
                Ok(()),
                "Tags = {raw:?}"
            );
        }
    }

    /// `0.0.0.0` covers every IPv4 address, and llama.cpp's `--host` help says
    /// overlapping addresses are undefined behaviour: the bind list holds it
    /// alone (the GUI's checkboxes never combine it, `net_ifaces::toggle`).
    #[test]
    fn server_bind_list_never_mixes_all_interfaces_with_others() {
        for bad in [
            "0.0.0.0,127.0.0.1",
            "127.0.0.1, 0.0.0.0",
            "localhost,0.0.0.0,192.168.1.10",
        ] {
            assert_eq!(
                server_file(&server(&format!("Hostname = {bad}\n")), ""),
                Err(refusal("bind-all-with-others", &[])),
                "Hostname = {bad:?}"
            );
        }
        for good in [
            "0.0.0.0",
            " 0.0.0.0 ",
            "0.0.0.0,",
            "127.0.0.1,192.168.1.10",
            "localhost",
            "",
        ] {
            assert_eq!(
                server_file(&server(&format!("Hostname = {good}\n")), ""),
                Ok(()),
                "Hostname = {good:?}"
            );
        }
        // A commented hint is no key.
        assert_eq!(
            server_file(&server("; Hostname = 0.0.0.0,127.0.0.1\n"), ""),
            Ok(())
        );
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
    fn server_models_max_must_hold_the_model_profiles_that_load_on_startup() {
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

    // ── models.ini: one test per rule ──────────────────────────────────

    #[test]
    fn model_profiles_as_the_configurator_writes_them_pass() {
        assert_eq!(model_profiles(REAL_SECTION), Ok(()));
        let header = "; Reconstructed by hand: a comment before any section.\n\n";
        assert_eq!(model_profiles(&format!("{header}{REAL_SECTION}")), Ok(()));
        assert_eq!(model_profiles(""), Ok(()));
    }

    #[test]
    fn a_model_profile_id_must_be_a_section_name() {
        for bad in ["", "has space", "a;b", "sec]tion"] {
            assert_eq!(
                model_profiles(&section(bad, "")),
                Err(refusal("invalid-model-profile-id", &[bad])),
                "[{bad}]"
            );
        }
        for good in ["qwen3-coder.q8_0", "Swift-1.5-Qwen3.8-27B", "a_b"] {
            assert_eq!(model_profiles(&section(good, "")), Ok(()), "[{good}]");
        }
    }

    #[test]
    fn a_model_profile_must_name_its_model() {
        assert_eq!(
            model_profiles("[m]\nctx-size = 8192\n"),
            Err(refusal("model-profile-no-model", &["m"]))
        );
        assert_eq!(
            model_profiles("[m]\nmodel =\nctx-size = 8192\n"),
            Err(refusal("model-profile-no-model", &["m"]))
        );
    }

    #[test]
    fn a_model_profile_id_is_taken_once_in_any_letter_case() {
        let text = format!("{}{}", section("Qwen3-8B", ""), section("qwen3-8b", ""));
        assert_eq!(
            model_profiles(&text),
            Err(refusal("model-profile-name-taken", &["qwen3-8b"]))
        );
    }

    #[test]
    fn model_profile_free_text_fields_must_hold_no_comment_marker() {
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
                model_profiles(&text),
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
        assert_eq!(model_profiles(&clean), Ok(()));
    }

    #[test]
    fn fit_target_is_whole_mib_per_device() {
        for bad in ["1,x", "2048MiB", "1,,2", "-1"] {
            assert_eq!(
                model_profiles(&section("m", &format!("fit-target = {bad}\n"))),
                Err(refusal("fit-target-format", &[bad, "MiB"])),
                "fit-target = {bad:?}"
            );
        }
        for good in ["", "2048", "2048,1024", "2048 , 1024"] {
            assert_eq!(
                model_profiles(&section("m", &format!("fit-target = {good}\n"))),
                Ok(()),
                "fit-target = {good:?}"
            );
        }
    }

    #[test]
    fn every_override_tensor_rule_names_a_pattern_and_a_device() {
        assert_eq!(
            model_profiles(&section("m", "override-tensor = blk\\.0\\.ffn_up_exps\n")),
            Err(refusal(
                "override-no-device",
                &["1", r"blk\.0\.ffn_up_exps"]
            ))
        );
        assert_eq!(
            model_profiles(&section(
                "m",
                "override-tensor = blk\\.1\\.attn=CPU,=ROCm0\n"
            )),
            Err(refusal("override-no-pattern", &["2"]))
        );
        assert_eq!(
            model_profiles(&section(
                "m",
                "override-tensor = blk\\.1\\.attn=CPU, ffn_.*_exps=ROCm1\n"
            )),
            Ok(())
        );
    }

    #[test]
    fn the_moe_cache_is_refused_under_split_mode_tensor() {
        assert_eq!(
            model_profiles(&section("m", "moe-cache-mib = 4096\nsplit-mode = tensor\n")),
            Err(refusal(MOE_TENSOR, &[]))
        );
        assert_eq!(
            model_profiles(&section("m", "moe-cache-mib = 0\nsplit-mode = tensor\n")),
            Ok(())
        );
        for mode in ["", "layer", "row", "none"] {
            assert_eq!(
                model_profiles(&section(
                    "m",
                    &format!("moe-cache-mib = 4096\nsplit-mode = {mode}\n")
                )),
                Ok(()),
                "split-mode = {mode:?}"
            );
        }
    }

    #[test]
    fn the_model_profiles_that_load_on_startup_fit_models_max() {
        let flagged = |ids: &[&str]| -> String {
            ids.iter()
                .map(|id| section(id, "load-on-startup = true\n"))
                .collect()
        };
        assert_eq!(
            models_file(&flagged(&["a", "b", "c"]), "", "", Some(2)),
            Err(refusal("load-on-startup-over-max", &["3", "2"]))
        );
        assert_eq!(models_file(&flagged(&["a", "b"]), "", "", Some(2)), Ok(()));
        assert_eq!(
            models_file(&flagged(&["a"]), "", "", Some(0)),
            Err(refusal("load-on-startup-unlimited", &["1"]))
        );
        assert_eq!(models_file(&section("a", ""), "", "", Some(0)), Ok(()));
        let d = flagged(&["a", "b", "c", "d"]);
        assert_eq!(models_file(&d, "", "", None), Ok(()));
        assert_eq!(
            models_file(&flagged(&["a", "b", "c", "d", "e"]), "", "", None),
            Err(refusal("load-on-startup-over-max", &["5", "4"]))
        );
        // Only `true` loads on startup.
        let off = format!(
            "{}{}",
            flagged(&["a"]),
            section("b", "load-on-startup = false\n")
        );
        assert_eq!(models_file(&off, "", "", Some(1)), Ok(()));
    }

    /// The integer keys of a model profile, with the values each takes: the same
    /// ranges the Models tab's form reads them with.
    #[test]
    fn model_profile_integer_keys_hold_whole_numbers_in_their_range() {
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
            // 0 is llama.cpp's "the context from the model".
            ("ctx-size", INT_NON_NEGATIVE),
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
                    model_profiles(&section("m", &format!("{key} = {text}\n"))),
                    Err(not_int(key, text, &r)),
                    "{key} = {text:?}"
                );
            }
            for good in [*r.start(), *r.end()] {
                assert_eq!(
                    model_profiles(&section("m", &format!("{key} = {good}\n"))),
                    Ok(()),
                    "{key} = {good}"
                );
            }
        }
    }

    /// Keys the Models tab does not refuse stay lenient: a bool, and the
    /// reasoning level llama.cpp hands to the template as is.
    #[test]
    fn model_profile_keys_with_no_check_today_pass() {
        let text = section("m", "load-on-startup = yes\nreasoning-effort = thorough\n");
        assert_eq!(model_profiles(&text), Ok(()));
    }

    // The web's key list of "Saving a model profile from the web" in
    // `ARCHITECTURE.md`, pinned WITHOUT repeating it: the keys no web control
    // writes must be absent from it, and two real keys of the design's table
    // must be present. An empty list, a list of names no reader knows, and a
    // list that dropped a card of the table each fail here. The full list is
    // pinned against the configurator's writer by `pai-app`'s
    // `web_key_lists_are_exactly_what_render_section_writes`.
    #[test]
    fn web_model_profile_keys_exclude_the_ten_and_hold_the_named_ones() {
        // The ten the design keeps off the web until the slice it names.
        for key in [
            "model",
            "mmproj",
            "model-draft",
            "spec-type",
            "n-cpu-moe",
            "moe-cache-mib",
            "device",
            "split-mode",
            "tensor-split",
            "override-tensor",
        ] {
            assert!(
                !WEB_MODEL_PROFILE_KEYS.contains(&key),
                "{key} is the configurator's to write, not the web's"
            );
        }
        // One key of the sampling card and one of the context card: a list that
        // holds none of the table's keys is no list.
        for key in ["min-p", "ctx-size"] {
            assert!(
                WEB_MODEL_PROFILE_KEYS.contains(&key),
                "{key} is a key of the design's table, so the web's list must hold it"
            );
        }
    }

    // ── models.ini: only what the write changes is checked ────────────

    /// A hand-edited section already on disk does not block saving another:
    /// today a save checks only the model profile it writes.
    #[test]
    fn a_section_unchanged_from_the_file_on_disk_is_not_checked() {
        let broken = "[old]\nmodel = /models/old.gguf\nctx-size = abc\n";
        let added = format!("{broken}{}", section("new", "ctx-size = 8192\n"));
        assert_eq!(models_file(&added, broken, "", None), Ok(()));
        // The same section edited, still broken: checked.
        let edited = "[old]\nmodel = /models/old.gguf\nctx-size = abc\nparallel = 2\n";
        assert_eq!(
            models_file(edited, broken, "", None),
            Err(not_int("ctx-size", "abc", &crate::ini::INT_NON_NEGATIVE))
        );
        // A new section with a problem: checked.
        let bad_new = format!("{broken}{}", section("new", "parallel = 0\n"));
        assert_eq!(
            models_file(&bad_new, broken, "", None),
            Err(not_int("parallel", "0", &crate::ini::INT_POSITIVE))
        );
    }

    /// Only a model profile that loads on startup and is written now can push the
    /// count over: a save of another is never blocked by the ones on disk.
    #[test]
    fn the_startup_count_blocks_only_a_changed_model_profile_that_loads_on_startup() {
        let over: String = ["a", "b", "c"]
            .iter()
            .map(|id| section(id, "load-on-startup = true\n"))
            .collect();
        let plus_plain = format!("{over}{}", section("d", ""));
        assert_eq!(models_file(&plus_plain, &over, "", Some(2)), Ok(()));
        let plus_flagged = format!("{over}{}", section("d", "load-on-startup = true\n"));
        assert_eq!(
            models_file(&plus_flagged, &over, "", Some(2)),
            Err(refusal("load-on-startup-over-max", &["4", "2"]))
        );
        // Switching one on: new in models.ini, so checked.
        let two: String = ["a", "b"]
            .iter()
            .map(|id| section(id, "load-on-startup = true\n"))
            .collect();
        assert_eq!(
            models_file(&over, &two, "", Some(2)),
            Err(refusal("load-on-startup-over-max", &["3", "2"]))
        );
    }

    // ── models.ini.disabled ────────────────────────────────────────────

    #[test]
    fn a_disabled_model_profile_written_now_gets_the_model_profile_rules() {
        let bad = "[m]\nmodel = C:\\Models #1\\m.gguf\n";
        assert_eq!(
            models_disabled_file(bad, "", ""),
            Err(refusal("comment-markers", &["model"]))
        );
        assert_eq!(
            models_disabled_file(&section("m", "parallel = 0\n"), "", ""),
            Err(not_int("parallel", "0", &crate::ini::INT_POSITIVE))
        );
        let header = "; Model profiles switched OFF in pai-config.\r\n";
        assert_eq!(
            models_disabled_file(&format!("{header}{REAL_SECTION}"), "", ""),
            Ok(())
        );
    }

    #[test]
    fn disabled_model_profiles_have_no_startup_count() {
        let five: String = ["a", "b", "c", "d", "e"]
            .iter()
            .map(|id| section(id, "load-on-startup = true\n"))
            .collect();
        assert_eq!(models_disabled_file(&five, "", ""), Ok(()));
    }

    /// Switching off a hand-edited model profile llama-server refuses is how the
    /// server gets to start again: never refused.
    #[test]
    fn switching_off_a_broken_model_profile_is_never_refused() {
        let broken = "[old]\nmodel = /models/old.gguf\nctx-size = abc\n";
        assert_eq!(models_disabled_file(broken, "", broken), Ok(()));
        assert_eq!(models_disabled_file(broken, broken, ""), Ok(()));
        let edited = "[old]\nmodel = /models/old.gguf\nctx-size = abc\nparallel = 2\n";
        assert_eq!(
            models_disabled_file(edited, broken, broken),
            Err(not_int("ctx-size", "abc", &crate::ini::INT_NON_NEGATIVE))
        );
    }

    // ── An inline comment on a line the save does not touch ──────────────
    //
    // The manager's `comment_markers` of a checked section skips a key line
    // that `old` or the disabled file holds identically in the section of
    // the same id: that line is the one a byte-for-byte save moves whole,
    // and no web save can introduce it ("Saving a model profile from the web",
    // decision c1).

    /// `[a]` with an inline comment on its `model` line, as a hand edit
    /// writes it; llama-server reads the value cut at the `;`.
    const COMMENTED: &str = "[a]\nmodel = a.gguf ; Q8\nctx-size = 4096\n";

    /// The web edits `ctx-size` only: the commented `model` line stays byte
    /// for byte, so the section passes.
    #[test]
    fn inline_comment_untouched_in_the_edited_section_passes() {
        let edited = "[a]\nmodel = a.gguf ; Q8\nctx-size = 8192\n";
        assert_eq!(models_file(edited, COMMENTED, "", None), Ok(()));
    }

    /// The exemption is per key line: a comment on the line the save changes
    /// is the value the save writes with a marker in it, refused.
    #[test]
    fn inline_comment_changed_by_the_save_is_refused() {
        let edited = "[a]\nmodel = b.gguf ; Q8\nctx-size = 8192\n";
        assert_eq!(
            models_file(edited, COMMENTED, "", None),
            Err(refusal("comment-markers", &["model"]))
        );
    }

    /// A line the save adds is in no known section: its markers are refused,
    /// even beside an exempt commented line.
    #[test]
    fn inline_comment_on_an_added_line_is_refused() {
        let added = "[a]\nmodel = a.gguf ; Q8\nctx-size = 8192\nreasoning-budget-message = a;b\n";
        assert_eq!(
            models_file(added, COMMENTED, "", None),
            Err(refusal("comment-markers", &["reasoning-budget-message"]))
        );
    }

    /// The known section must carry the id: the same key line held only by
    /// another section `[b]` of `old` exempts nothing.
    #[test]
    fn inline_comment_known_only_from_another_section_is_refused() {
        let other_id = "[b]\nmodel = a.gguf ; Q8\nctx-size = 4096\n";
        let edited = "[a]\nmodel = a.gguf ; Q8\nctx-size = 8192\n";
        assert_eq!(
            models_file(edited, other_id, "", None),
            Err(refusal("comment-markers", &["model"]))
        );
    }

    /// The switch on moves the section whole: new to `models.ini`, held
    /// identically (header exact) by the disabled file, so it switches on.
    #[test]
    fn inline_comment_known_from_the_disabled_file_switches_on_ok() {
        let kept = "[kept]\nmodel = /models/kept.gguf\n";
        assert_eq!(
            models_file(&format!("{kept}{COMMENTED}"), kept, COMMENTED, None),
            Ok(())
        );
    }

    /// The same in the disabled file, from `old` (this file now) and from
    /// `enabled` (`models.ini` now): a disabled model profile edited on its own
    /// page keeps the commented line the file already holds.
    #[test]
    fn inline_comment_known_in_disabled_file_from_old_and_enabled_passes() {
        let edited = "[a]\nmodel = a.gguf ; Q8\nctx-size = 8192\n";
        assert_eq!(models_disabled_file(edited, COMMENTED, ""), Ok(()));
        assert_eq!(models_disabled_file(edited, "", COMMENTED), Ok(()));
    }

    /// A section new to both files is checked whole: the `other` file opens
    /// no door for a marker no file holds.
    #[test]
    fn inline_comment_new_in_both_files_is_refused() {
        let fresh = "[a]\nmodel = a;b.gguf\n";
        assert_eq!(
            models_file(fresh, "", "", None),
            Err(refusal("comment-markers", &["model"]))
        );
        let kept = "[kept]\nmodel = /models/kept.gguf\n";
        assert_eq!(
            models_file(fresh, "", kept, None),
            Err(refusal("comment-markers", &["model"]))
        );
    }

    /// Exactness, as the header is kept byte for byte by the save and the
    /// switch: a `[A]` header is not the known `[a]`, and a `MODEL` line is
    /// not the known `model` (llama-server knows keys in their own case).
    #[test]
    fn inline_comment_known_in_another_letter_case_is_refused() {
        let header_case = "[A]\nmodel = a.gguf ; Q8\nctx-size = 8192\n";
        assert_eq!(
            models_file(header_case, COMMENTED, "", None),
            Err(refusal("comment-markers", &["model"]))
        );
        let key_case = "[a]\nMODEL = a.gguf ; Q8\nctx-size = 8192\n";
        assert_eq!(
            models_file(key_case, COMMENTED, "", None),
            Err(refusal("model-profile-no-model", &["a"]))
        );
    }

    // ── ctx-size = 0: the context from the model ─────────────────────────

    /// llama.cpp reads `ctx-size = 0` as "the context the model was trained
    /// with" (common/arg.cpp), and the integrations treat it so: the checks
    /// take it, in both model profile files; a negative context is still refused.
    #[test]
    fn ctx_size_zero_is_the_context_from_the_model_and_writable() {
        let r = model_profile_int_range("ctx-size").expect("ctx-size is an integer key");
        assert!(r.contains(&0), "0 in {r:?}");
        assert!(!r.contains(&-1), "-1 in {r:?}");
        let zero = section("m", "ctx-size = 0\n");
        assert_eq!(models_file(&zero, "", "", None), Ok(()));
        assert_eq!(models_disabled_file(&zero, "", ""), Ok(()));
        assert_eq!(
            model_profiles(&section("m", "ctx-size = -1\n")),
            Err(not_int("ctx-size", "-1", &crate::ini::INT_NON_NEGATIVE))
        );
    }

    // ── One id across models.ini and models.ini.disabled ───────────────

    /// A model profile written into one file under an id the other file holds, in
    /// any letter case, is refused with the id as written: switching either
    /// back would land on the namesake.
    #[test]
    fn a_new_model_profile_under_an_id_the_other_file_holds_is_refused() {
        let parked = section("Qwen3.8-27B", "ctx-size = 8192\n");
        let enabled = section("Qwen3.8-27B", "ctx-size = 16384\n");
        assert_eq!(
            model_profile_ids_free(&enabled, "", &parked),
            Err(refusal("model-profile-name-taken", &["Qwen3.8-27B"]))
        );
        let other_case = section("qwen3.8-27b", "ctx-size = 16384\n");
        assert_eq!(
            model_profile_ids_free(&other_case, "", &parked),
            Err(refusal("model-profile-name-taken", &["qwen3.8-27b"]))
        );
        // Beside the model profiles already in the file.
        let kept = section("Qwen3.8-Flash-Next", "");
        assert_eq!(
            model_profile_ids_free(&format!("{kept}{enabled}"), &kept, &parked),
            Err(refusal("model-profile-name-taken", &["Qwen3.8-27B"]))
        );
    }

    /// A rename is a new id in its file: renaming onto a parked namesake is
    /// refused too.
    #[test]
    fn a_rename_onto_an_id_the_other_file_holds_is_refused() {
        let old = section("a", "");
        let renamed = section("b", "");
        let other = section("B", "ctx-size = 8192\n");
        assert_eq!(
            model_profile_ids_free(&renamed, &old, &other),
            Err(refusal("model-profile-name-taken", &["b"]))
        );
    }

    /// The switch writes the destination before it removes the source, so
    /// for a moment both files hold the section: unchanged, it is a move.
    #[test]
    fn a_switch_moving_a_model_profile_unchanged_is_not_a_namesake() {
        let moved = REAL_SECTION;
        let kept = section("Qwen3.8-Flash-Next", "");
        assert_eq!(model_profile_ids_free(moved, "", moved), Ok(()));
        assert_eq!(
            model_profile_ids_free(&format!("{kept}{moved}"), &kept, moved),
            Ok(())
        );
    }

    /// A model profile in both files already (the twin an interrupted switch
    /// leaves) can still be saved where it is, and blocks no other model profile.
    #[test]
    fn a_model_profile_already_in_both_files_still_saves() {
        let twin = section("m", "ctx-size = 8192\n");
        let edited = section("m", "ctx-size = 16384\n");
        assert_eq!(model_profile_ids_free(&edited, &twin, &twin), Ok(()));
        let added = format!("{twin}{}", section("n", ""));
        assert_eq!(model_profile_ids_free(&added, &twin, &twin), Ok(()));
        // Nothing in the other file: nothing to collide with.
        assert_eq!(model_profile_ids_free(&edited, "", ""), Ok(()));
    }

    // ── Second cut: floats, enums, JSON, min <= max ──────────────────────

    /// The refusal keys the second cut adds: each in `KEYS` (so the manager's
    /// refusal crosses the wire), in both strings files, and with a text arm
    /// that fills in its values instead of printing the key.
    #[test]
    fn the_second_cut_refusal_keys_are_known_and_worded() {
        let cli = crate::strings::parse(include_str!("../strings/cli.toml"));
        let gui = crate::strings::parse(include_str!("../strings/en_US.toml"));
        for (key, values, n) in [
            ("not-a-number", vec!["temp", "NaN"], 2),
            (
                "not-one-of",
                vec!["split-mode", "banana", "none, layer, row, tensor"],
                3,
            ),
            ("not-json-object", vec!["chat-template-kwargs"], 1),
            (
                "min-above-max",
                vec!["image-min-tokens", "2048", "image-max-tokens", "1024"],
                4,
            ),
        ] {
            assert!(KEYS.contains(&key), "{key} missing from checks::KEYS");
            for (name, table) in [("cli.toml", &cli), ("en_US.toml", &gui)] {
                let text = table
                    .get(key)
                    .unwrap_or_else(|| panic!("{key} missing from {name}"));
                assert_eq!(placeholders(text), n, "{name} {key}: {text:?}");
            }
            let r = refusal(key, &values);
            let text = r.text();
            assert!(!text.starts_with(key), "{key} has no text arm: {text:?}");
            for v in &values {
                assert!(text.contains(v), "{key}: {v:?} not in {text:?}");
            }
            assert_eq!(Refusal::decode(&r.encode()), Some(r.clone()), "{key}");
        }
    }

    /// The key of the model-file check (PLAN.md step 10 item 13, slice 9): in
    /// `KEYS`, so the manager's refusal crosses the wire; in both strings files,
    /// with the three values its rule passes (`[model profile, instance, path]`,
    /// ARCHITECTURE.md item 10 of the slices notes, the renamed key); and with a
    /// text arm that fills them in.
    #[test]
    fn the_model_file_refusal_key_is_known_and_worded() {
        let cli = crate::strings::parse(include_str!("../strings/cli.toml"));
        let gui = crate::strings::parse(include_str!("../strings/en_US.toml"));
        let key = "model-profile-model-missing";
        let values = vec!["big", "default", "/models/big-00002-of-00003.gguf"];
        assert!(KEYS.contains(&key), "{key} missing from checks::KEYS");
        for (name, table) in [("cli.toml", &cli), ("en_US.toml", &gui)] {
            let text = table
                .get(key)
                .unwrap_or_else(|| panic!("{key} missing from {name}"));
            assert_eq!(placeholders(text), 3, "{name} {key}: {text:?}");
        }
        let r = refusal(key, &values);
        let text = r.text();
        assert!(!text.starts_with(key), "{key} has no text arm: {text:?}");
        for v in &values {
            assert!(text.contains(v), "{key}: {v:?} not in {text:?}");
        }
        assert_eq!(
            Refusal::decode(&r.encode()),
            Some(r.clone()),
            "{key} round-trips on the wire"
        );
    }

    /// The key of the model profile memory check (PLAN.md step 10 item 13,
    /// slice 10b-2; ARCHITECTURE.md (b) 2, "The refusal"): in `KEYS`, so the
    /// manager's refusal crosses the wire; in both strings files, with the four
    /// values its rule passes (`[model profile, instance, needed MiB, available
    /// MiB]`); and with a text arm that fills them in.
    #[test]
    fn the_over_memory_refusal_key_is_known_and_worded() {
        let cli = crate::strings::parse(include_str!("../strings/cli.toml"));
        let gui = crate::strings::parse(include_str!("../strings/en_US.toml"));
        let key = "model-profile-over-memory";
        let values = vec!["qwen3.8-27b", "lan-2", "1446", "512"];
        assert!(KEYS.contains(&key), "{key} missing from checks::KEYS");
        for (name, table) in [("cli.toml", &cli), ("en_US.toml", &gui)] {
            let text = table
                .get(key)
                .unwrap_or_else(|| panic!("{key} missing from {name}"));
            assert_eq!(placeholders(text), 4, "{name} {key}: {text:?}");
        }
        let r = refusal(key, &values);
        let text = r.text();
        assert!(!text.starts_with(key), "{key} has no text arm: {text:?}");
        for v in &values {
            assert!(text.contains(v), "{key}: {v:?} not in {text:?}");
        }
        assert_eq!(
            Refusal::decode(&r.encode()),
            Some(r.clone()),
            "{key} round-trips on the wire"
        );
    }

    /// The memory refusal whole, byte for byte in both strings files, next
    /// step included: the window names the cards of its page, `pai` names the
    /// keys of the file. The first sentence is the web's too (the `api-` key of
    /// `fe/src/strings/en_US.json`), and the sentence `text()` renders with the
    /// four values the rule passes is the GUI's, whole.
    #[test]
    fn the_over_memory_refusal_says_the_next_step_in_both_files() {
        const GUI: &str = "`{0}` does not fit the GPU memory of instance `{1}`: it needs {2} MiB, the node has {3} MiB. To make it fit, lower its Context size, or pick a smaller K-cache type or V-cache type.";
        const CLI: &str = "`{0}` does not fit the GPU memory of instance `{1}`: it needs {2} MiB, the node has {3} MiB. To make it fit, lower its ctx-size, or pick a smaller cache-type-k or cache-type-v.";
        for (name, file, expected) in [
            ("en_US.toml", include_str!("../strings/en_US.toml"), GUI),
            ("cli.toml", include_str!("../strings/cli.toml"), CLI),
        ] {
            let table = crate::strings::parse(file);
            assert_eq!(
                table
                    .get("model-profile-over-memory")
                    .expect("model-profile-over-memory in the strings file"),
                expected,
                "{name}: model-profile-over-memory"
            );
        }
        assert_eq!(
            refusal("model-profile-over-memory", &["qwen3.8-27b", "lan-2", "1446", "512"]).text(),
            "`qwen3.8-27b` does not fit the GPU memory of instance `lan-2`: it needs 1446 MiB, the node has 512 MiB. To make it fit, lower its Context size, or pick a smaller K-cache type or V-cache type."
        );
    }

    /// The missing model file refusal whole, byte for byte in both strings
    /// files, next step included: the window names the field of its page, `pai`
    /// names the key of the file. The first sentence is the web's too (the
    /// `api-` key of `fe/src/strings/en_US.json`), and the sentence `text()`
    /// renders with the three values the rule passes is the GUI's, whole.
    #[test]
    fn the_model_missing_refusal_says_the_next_step_in_both_files() {
        const GUI: &str = "`{0}` names a model file that is not there: `{2}` is missing for instance `{1}`. Put the file at that path on the server machine, or pick another Model file for the profile.";
        const CLI: &str = "`{0}` names a model file that is not there: `{2}` is missing for instance `{1}`. Put the file at that path on the server machine, or set the profile's model to another file.";
        for (name, file, expected) in [
            ("en_US.toml", include_str!("../strings/en_US.toml"), GUI),
            ("cli.toml", include_str!("../strings/cli.toml"), CLI),
        ] {
            let table = crate::strings::parse(file);
            assert_eq!(
                table
                    .get("model-profile-model-missing")
                    .expect("model-profile-model-missing in the strings file"),
                expected,
                "{name}: model-profile-model-missing"
            );
        }
        assert_eq!(
            refusal(
                "model-profile-model-missing",
                &["big", "default", "/models/big-00002-of-00003.gguf"]
            )
            .text(),
            "`big` names a model file that is not there: `/models/big-00002-of-00003.gguf` is missing for instance `default`. Put the file at that path on the server machine, or pick another Model file for the profile."
        );
    }

    /// The files a `model` value names, the whole list the model-file check
    /// stats: the one file as written; every shard of a split model, in index
    /// order, named from the value's own directory, extension and separator,
    /// whichever shard the value happens to name; the value alone when the
    /// `-NNNNN-of-NNNNN` spelling is not there (a short counter, a total of
    /// zero, a `-of-` inside a directory name, a name with no extension);
    /// nothing for a blank value, which is `model_profile_model`'s to refuse
    /// and what a profile with only `hf-repo` leaves. Every name is the value's
    /// own spelling: a relative one is resolved by the caller, against the
    /// instance's folder, and a model profile carrying `hf-repo`, `model-url`
    /// or `docker-repo` leaves the model-file check with nothing to look at.
    #[test]
    fn model_files_of_lists_the_files_a_model_value_names() {
        // One line per file: the list reads straight, and an empty list is "".
        let names = |model: &str| model_files_of(model).join("\n");
        let shards = |dir: &str, ext: &str| {
            (1..=3)
                .map(|i| format!("{dir}big-{i:05}-of-00003{ext}"))
                .collect::<Vec<String>>()
                .join("\n")
        };

        assert_eq!(names(""), "", "a blank value names nothing");
        assert_eq!(names("   "), "", "spaces are blank");
        assert_eq!(
            names("/models/a.gguf"),
            "/models/a.gguf",
            "one file, as written"
        );
        assert_eq!(
            names("  /models/a.gguf  "),
            "/models/a.gguf",
            "the value as the readers see it, trimmed"
        );
        assert_eq!(
            names("Qwen/Qwen3-8B-Q4.gguf"),
            "Qwen/Qwen3-8B-Q4.gguf",
            "a relative value names itself"
        );
        assert_eq!(
            names("/models/no-extension"),
            "/models/no-extension",
            "a name with no extension"
        );
        assert_eq!(
            names("/models/dir-00001-of-00003/model.gguf"),
            "/models/dir-00001-of-00003/model.gguf",
            "a `-of-` in a directory name is no shard: the file name decides"
        );
        assert_eq!(
            names("/models/x-00001-of-1.gguf"),
            "/models/x-00001-of-1.gguf",
            "a counter of one digit is not llama.cpp's spelling"
        );
        assert_eq!(
            names("/models/x-1-of-00003.gguf"),
            "/models/x-1-of-00003.gguf",
            "a shard counter of one digit is not either"
        );
        assert_eq!(
            names("/models/x-00001-of-00000.gguf"),
            "/models/x-00001-of-00000.gguf",
            "a total of zero expands to nothing, so the value stands alone"
        );
        assert_eq!(
            names("/models/big-00001-of-00003.gguf"),
            shards("/models/", ".gguf"),
            "the first shard names the whole model"
        );
        assert_eq!(
            names("/models/big-00002-of-00003.gguf"),
            shards("/models/", ".gguf"),
            "a middle shard names the whole model too, in index order"
        );
        assert_eq!(
            names("Qwen/big-00001-of-00003.gguf"),
            shards("Qwen/", ".gguf"),
            "the shards of a relative value stay relative, the directory of the value kept: the check joins each onto the instance's folder"
        );
        assert_eq!(
            names("C:\\models\\big-00001-of-00003.gguf"),
            shards("C:\\models\\", ".gguf"),
            "the separator of the value is kept"
        );
        assert_eq!(
            names("/models/big-00001-of-00003.GGUF"),
            shards("/models/", ".GGUF"),
            "so is its extension, its case included"
        );
    }

    /// The float keys of a model profile (the Models tab's sampling fields and the
    /// draft confidence floor). `ini::parse_float` takes `inf` and `NaN`, which
    /// llama.cpp's `std::stof` reads as a sampler that never works; a word is
    /// no number at all. Refused in either model profile file, with the key and the
    /// value as written; a plain number of any sign passes (no GUI range).
    #[test]
    fn float_keys_refuse_infinities_nan_and_words() {
        let keys = [
            "temp",
            "top-p",
            "min-p",
            "repeat-penalty",
            "presence-penalty",
            "spec-draft-p-min",
        ];
        for key in keys {
            for bad in [
                "inf",
                "-inf",
                "INF",
                "Infinity",
                "+infinity",
                "NaN",
                "nan",
                "-NaN",
                "abc",
                "0.5x",
            ] {
                let text = section("m", &format!("{key} = {bad}\n"));
                let want = Err(refusal("not-a-number", &[key, bad]));
                assert_eq!(model_profiles(&text), want, "{key} = {bad}");
                assert_eq!(models_disabled_file(&text, "", ""), want, "{key} = {bad}");
            }
            for good in ["0", "0.95", "1", "1.1", "-1", "2.0"] {
                let text = section("m", &format!("{key} = {good}\n"));
                assert_eq!(model_profiles(&text), Ok(()), "{key} = {good}");
            }
        }
    }

    /// The enum keys: the values the Models tab's combo boxes and segmented
    /// controls offer all pass (an empty value is the omitted key and never
    /// reaches the file), and a word outside llama.cpp's own set, which
    /// `common/arg.cpp` throws on so the model never loads, is refused with the
    /// key, the value and a list of what it takes. Where llama.cpp takes more
    /// than the GUI offers (`reasoning`'s `enabled`/`true`/`1`,
    /// `reasoning-format`'s `deepseek-legacy`, `spec-type`'s `draft-eagle3` and
    /// `ngram-cache`), nothing is pinned either way.
    #[test]
    fn enum_keys_refuse_a_value_outside_their_set() {
        let cache_types = [
            "f32", "f16", "bf16", "f8_e4m3", "q8_0", "q5_1", "q5_0", "q4_1", "q4_0", "iq4_nl",
        ];
        let cases: Vec<(&str, Vec<&str>, Vec<&str>)> = vec![
            // The GUI's three entries plus `layer`, the default llama.cpp takes.
            (
                "split-mode",
                vec!["none", "layer", "row", "tensor"],
                vec!["banana", "rows", "layer,row"],
            ),
            (
                "pooling",
                vec!["none", "mean", "cls", "last", "rank"],
                vec!["max", "banana"],
            ),
            (
                "cache-type-k",
                cache_types.to_vec(),
                vec!["q6_k", "q4_k", "banana"],
            ),
            ("cache-type-v", cache_types.to_vec(), vec!["q6_k", "banana"]),
            (
                "spec-draft-type-k",
                cache_types.to_vec(),
                vec!["q6_k", "banana"],
            ),
            (
                "spec-draft-type-v",
                cache_types.to_vec(),
                vec!["q6_k", "banana"],
            ),
            (
                "reasoning",
                vec!["auto", "on", "off"],
                vec!["maybe", "banana"],
            ),
            (
                "reasoning-format",
                vec!["auto", "none", "deepseek"],
                vec!["qwen", "banana"],
            ),
            (
                "lazy-mode",
                vec!["on", "auto", "off"],
                vec!["lazy", "banana"],
            ),
            (
                "load-mode",
                vec!["auto", "none", "mmap", "mlock", "mmap+mlock", "dio"],
                vec!["no-mmap", "mmap+dio", "banana"],
            ),
        ];
        for (key, good, bad) in &cases {
            for v in good {
                let text = section("m", &format!("{key} = {v}\n"));
                assert_eq!(model_profiles(&text), Ok(()), "{key} = {v}");
            }
            for v in bad {
                let text = section("m", &format!("{key} = {v}\n"));
                for (file, got) in [
                    ("models.ini", model_profiles(&text)),
                    ("models.ini.disabled", models_disabled_file(&text, "", "")),
                ] {
                    let r = got.expect_err(&format!("{file}: {key} = {v}"));
                    assert_eq!(r.key, "not-one-of", "{file}: {key} = {v}: {r:?}");
                    assert_eq!(r.values.first().map(String::as_str), Some(*key), "{r:?}");
                    assert_eq!(r.values.get(1).map(String::as_str), Some(*v), "{r:?}");
                    let allowed = r.values.get(2).cloned().unwrap_or_default();
                    for g in good {
                        assert!(allowed.contains(g), "{key}: {g} not listed in {allowed:?}");
                    }
                }
            }
        }
    }

    /// `reasoning-effort` is handed to the chat template as is: llama.cpp takes
    /// any level, so only the GUI's own entries are pinned, as passing.
    #[test]
    fn reasoning_effort_takes_every_level_the_gui_offers() {
        for v in ["minimal", "low", "medium", "high", "xhigh", "max"] {
            let text = section("m", &format!("reasoning-effort = {v}\n"));
            assert_eq!(model_profiles(&text), Ok(()), "reasoning-effort = {v}");
        }
    }

    /// `spec-type` is a comma-separated list (`common_speculative_types_from_names`
    /// throws on an unknown entry): every entry the Models tab writes passes,
    /// alone and in the ngram + draft pairs it composes; an unknown entry is
    /// refused, named on its own.
    #[test]
    fn spec_type_refuses_an_unknown_entry_of_its_list() {
        for v in [
            "draft-mtp",
            "draft-dflash",
            "draft-dspark",
            "draft-simple",
            "ngram-simple",
            "ngram-map-k",
            "ngram-map-k4v",
            "ngram-mod",
            "ngram-map-k4v,draft-dflash",
            "ngram-mod,draft-mtp",
        ] {
            let text = section("m", &format!("spec-type = {v}\n"));
            assert_eq!(model_profiles(&text), Ok(()), "spec-type = {v}");
        }
        for (v, entry) in [
            ("draft-banana", "draft-banana"),
            ("mtp", "mtp"),
            ("ngram-map-k4v,draft-bogus", "draft-bogus"),
        ] {
            let text = section("m", &format!("spec-type = {v}\n"));
            let r = model_profiles(&text).expect_err(v);
            assert_eq!(r.key, "not-one-of", "{v}: {r:?}");
            assert_eq!(r.values.first().map(String::as_str), Some("spec-type"));
            assert_eq!(r.values.get(1).map(String::as_str), Some(entry), "{r:?}");
            let allowed = r.values.get(2).cloned().unwrap_or_default();
            assert!(allowed.contains("draft-mtp"), "{allowed:?}");
        }
    }

    /// The tags of a server profile and of an instance (the `Tags` key of
    /// `server@<id>.ini`, and the record's tags): up to [`TAGS_MAX`] of them,
    /// each of [`TAG_MIN`] to [`TAG_MAX`] characters of `a-z`, `0-9` and `-`,
    /// none twice. The web UI shows them and the manager never passes them to
    /// llama-server, so this rule is the only check they meet. An empty list
    /// is what clearing them looks like, so it passes.
    #[test]
    fn tags_are_a_short_list_of_lower_words_none_twice() {
        for list in [
            Vec::new(),
            tags("gpu"),
            tags("edge-1,v2"),
            tags("a"),
            tags(&"z".repeat(TAG_MAX)),
            (0..TAGS_MAX).map(|i| format!("t{i}")).collect(),
        ] {
            assert_eq!(tags_rule(&list), Ok(()), "{list:?}");
        }
        let long = "z".repeat(TAG_MAX + 1);
        for list in [
            vec![String::new()],
            tags("gpu").into_iter().chain([long]).collect(),
            tags("GPU"),
            tags("gpu edge"),
            tags("gpu.edge"),
            tags("gpu,edge,gpu"),
            (0..=TAGS_MAX).map(|i| format!("t{i}")).collect(),
        ] {
            let r = tags_rule(&list).expect_err(&format!("{list:?}"));
            assert_eq!(r.key, "server-bad-tags", "{list:?}: {r:?}");
            assert!(KEYS.contains(&r.key), "{} missing from checks::KEYS", r.key);
            assert_eq!(
                Refusal::decode(&r.encode()),
                Some(r.clone()),
                "wire round trip: {r:?}"
            );
        }
        // The refusal names the list it refused and the limits, so the page
        // can say what to fix.
        let r = tags_rule(&(0..=TAGS_MAX).map(|i| format!("t{i}")).collect::<Vec<_>>())
            .expect_err("one tag over the most");
        assert!(
            r.values.iter().any(|v| *v == TAGS_MAX.to_string()),
            "the refusal does not carry the most tags: {r:?}"
        );
        assert!(
            r.values.iter().any(|v| v.contains("t0")),
            "the refusal does not carry the tags it refused: {r:?}"
        );
    }

    /// The `Tags` value as the file writes it: a comma-separated list.
    fn tags(v: &str) -> Vec<String> {
        v.split(',').map(String::from).collect()
    }

    /// `chat-template-kwargs` is parsed by llama.cpp as a JSON object
    /// (`json::parse`, then merged key by key): anything else stops the load.
    #[test]
    fn chat_template_kwargs_must_be_a_json_object() {
        for good in [
            r#"{"enable_thinking":true}"#,
            r#"{ "enable_thinking" : false, "reasoning_effort": "high" }"#,
            "{}",
        ] {
            let text = section("m", &format!("chat-template-kwargs = {good}\n"));
            assert_eq!(model_profiles(&text), Ok(()), "{good}");
        }
        for bad in [
            "[1,2]",
            r#""enable_thinking""#,
            "true",
            "{bad",
            "enable_thinking=true",
            r#"{"a":1} {"b":2}"#,
        ] {
            let text = section("m", &format!("chat-template-kwargs = {bad}\n"));
            let want = Err(refusal("not-json-object", &["chat-template-kwargs"]));
            assert_eq!(model_profiles(&text), want, "{bad}");
            assert_eq!(models_disabled_file(&text, "", ""), want, "{bad}");
        }
    }

    /// The bounds that come in pairs: both set with the minimum above the
    /// maximum is refused, naming both keys and values; equal, ordered, or
    /// only one of the two set passes (the other is llama.cpp's own).
    #[test]
    fn min_above_max_pairs_are_refused() {
        for (min, max) in [
            ("image-min-tokens", "image-max-tokens"),
            ("spec-draft-n-min", "spec-draft-n-max"),
            ("spec-ngram-mod-n-min", "spec-ngram-mod-n-max"),
        ] {
            let both = |lo: &str, hi: &str| section("m", &format!("{min} = {lo}\n{max} = {hi}\n"));
            assert_eq!(
                model_profiles(&both("5", "4")),
                Err(refusal("min-above-max", &[min, "5", max, "4"])),
                "{min} > {max}"
            );
            // The order of the lines does not matter.
            let reversed = section("m", &format!("{max} = 4\n{min} = 5\n"));
            assert_eq!(
                model_profiles(&reversed),
                Err(refusal("min-above-max", &[min, "5", max, "4"])),
                "{max} first"
            );
            assert_eq!(
                models_disabled_file(&both("5", "4"), "", ""),
                Err(refusal("min-above-max", &[min, "5", max, "4"])),
                "disabled {min} > {max}"
            );
            assert_eq!(model_profiles(&both("4", "4")), Ok(()), "{min} = {max}");
            assert_eq!(model_profiles(&both("3", "4")), Ok(()), "{min} < {max}");
            assert_eq!(
                model_profiles(&section("m", &format!("{min} = 5\n"))),
                Ok(())
            );
            assert_eq!(
                model_profiles(&section("m", &format!("{max} = 4\n"))),
                Ok(())
            );
        }
    }
}
