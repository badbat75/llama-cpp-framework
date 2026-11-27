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
    "model-profile-bad-source",
    "model-profile-bad-text",
    "model-profile-enabled",
    "model-profile-assigned",
    "model-profile-placement-key",
    "model-profile-stay-key",
    "fit-target-format",
    "override-no-pattern",
    "override-no-device",
    "override-tensor-space",
    "override-bad-device",
    "moe-cache-with-tensor-split",
    "not-a-number",
    "not-one-of",
    "not-json-object",
    "min-above-max",
    "no-model-profile-named",
    "model-profile-bad-value",
    "model-profile-model-missing",
    "model-profile-over-memory",
    "model-profile-device-outside",
    "server-bad-tags",
    "unknown-engine",
    "server-profile-bad-id",
    "device-bad-name",
    "tensor-split-format",
    "instance-bad-id",
    "instance-exists",
    "instance-protected",
    "instance-running",
    "instance-no-model-profiles",
    "instance-bad-label",
    "instance-label-taken",
    "instance-profile-bad-id",
    "instance-profile-exists",
    "instance-profile-in-use",
    "instance-profile-model-listed",
    "no-instance-profile-model",
    "no-free-port",
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
            "model-profile-bad-source" => strings::fill("model-profile-bad-source", &[&v(0)]),
            "model-profile-bad-text" => strings::fill("model-profile-bad-text", &[&v(0)]),
            "model-profile-enabled" => strings::fill("model-profile-enabled", &[&v(0)]),
            "model-profile-assigned" => strings::fill("model-profile-assigned", &[&v(0), &v(1)]),
            "model-profile-placement-key" => strings::fill("model-profile-placement-key", &[&v(0)]),
            "model-profile-stay-key" => strings::fill("model-profile-stay-key", &[&v(0)]),
            "fit-target-format" => strings::fill("fit-target-format", &[&v(0), &v(1)]),
            "override-no-pattern" => strings::fill("override-no-pattern", &[&v(0)]),
            "override-no-device" => strings::fill("override-no-device", &[&v(0), &v(1)]),
            "override-bad-device" => strings::fill("override-bad-device", &[&v(0), &v(1)]),
            "override-tensor-space" => strings::fill("override-tensor-space", &[&v(0)]),
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
            "model-profile-device-outside" => strings::fill(
                "model-profile-device-outside",
                &[&v(0), &v(1), &v(2), &v(3)],
            ),
            "server-bad-tags" => {
                strings::fill("server-bad-tags", &[&v(0), &v(1), &v(2), &v(3), &v(4)])
            }
            "unknown-engine" => strings::fill("unknown-engine", &[&v(0)]),
            "server-profile-bad-id" => strings::fill("server-profile-bad-id", &[&v(0)]),
            "device-bad-name" => strings::fill("device-bad-name", &[&v(0), &v(1)]),
            "tensor-split-format" => strings::fill("tensor-split-format", &[&v(0)]),
            "instance-bad-id" => strings::fill("instance-bad-id", &[&v(0)]),
            "instance-exists" => strings::fill("instance-exists", &[&v(0)]),
            "instance-protected" => strings::fill("instance-protected", &[&v(0)]),
            "instance-running" => strings::fill("instance-running", &[&v(0)]),
            "instance-no-model-profiles" => strings::fill("instance-no-model-profiles", &[&v(0)]),
            "instance-bad-label" => strings::fill("instance-bad-label", &[&v(0)]),
            "instance-label-taken" => strings::fill("instance-label-taken", &[&v(0), &v(1)]),
            "instance-profile-bad-id" => strings::fill("instance-profile-bad-id", &[&v(0)]),
            "instance-profile-exists" => strings::fill("instance-profile-exists", &[&v(0)]),
            "instance-profile-in-use" => strings::fill("instance-profile-in-use", &[&v(0), &v(1)]),
            "instance-profile-model-listed" => {
                strings::fill("instance-profile-model-listed", &[&v(0), &v(1)])
            }
            "no-instance-profile-model" => {
                strings::fill("no-instance-profile-model", &[&v(0), &v(1)])
            }
            "no-free-port" => strings::fill("no-free-port", &[&v(0), &v(1)]),
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
pub const MODEL_PROFILE_INT_KEYS: [(&str, RangeInclusive<i32>); 30] = [
    ("image-min-tokens", INT_POSITIVE),
    ("image-max-tokens", INT_POSITIVE),
    ("spec-draft-n-max", INT_POSITIVE),
    ("spec-draft-n-min", INT_NON_NEGATIVE),
    ("spec-draft-n-ubatch", INT_NON_NEGATIVE),
    ("moe-cache-mib", INT_NON_NEGATIVE),
    // 0 is "no layer's experts on the CPU"; the top is `gguf_shape::shape_of`'s
    // block cap, so no real model is refused and a loop value is.
    ("n-cpu-moe", 0..=4096),
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
    // The model source: the file pick of the node's models folder, and the
    // repository and the file inside it ("Choosing the model file on the web").
    "model",
    "hf-repo",
    "hf-file",
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
    "n-cpu-moe",
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
    // The GPU distribution card.
    "device",
    "split-mode",
    "tensor-split",
    // The tensor placement card.
    "override-tensor",
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
/// design's table: the Assets card's file picks and the MoE keys that need the
/// model's facts. A body
/// key here is refused like an unknown one. A slice that enables one moves the key
/// to [`WEB_MODEL_PROFILE_KEYS`], in the same commit as its control.
pub const MODEL_PROFILE_KEYS_NOT_ON_WEB: &[&str] = &[
    // The Assets card's file picks, until it exists.
    "mmproj",
    "model-draft",
    "spec-type",
    // The MoE keys: they need the model's facts.
    "moe-cache-mib",
];

/// The model profile keys that MOVE into the instance profile's entry
/// `[<p>/<m>]` (`ARCHITECTURE.md`, "Instance profiles", the placement and
/// memory amendment): a key that says where the weights go, or is sized by the
/// memory of the hardware. The model profile keeps the file, the sampling and
/// the behaviour, so it becomes hardware-independent. Every key of the model
/// profile key set is in this list or in [`MODEL_PROFILE_STAY_KEYS`], never in
/// both.
pub const PLACEMENT_KEYS: &[&str] = &[
    // Tensor split card: the GPUs and how the tensors spread over them.
    "device",
    "split-mode",
    "tensor-split",
    "override-tensor",
    // That card's MoE keys: how many layers sit on a GPU, and the expert cache.
    "n-gpu-layers",
    "n-cpu-moe",
    "n-cpu-ffn",
    "moe-cache-mib",
    "op-offload",
    "repack",
    "no-host",
    "lazy-mode",
    "load-mode",
    // Memory fit card: the target the load is fitted to.
    "fit",
    "fit-ctx",
    "fit-target",
    // The draft model and the vision projector: which GPU carries them.
    "device-draft",
    "n-gpu-layers-draft",
    "mmproj-device",
    "mmproj-offload",
    // Resource and context card: the size of the context is the memory the pair
    // must find, and the batches are scaled by it.
    "ctx-size",
    "parallel",
    "kv-unified",
    "kv-unified-per-slot",
    "batch-size",
    "ubatch-size",
    // KV cache card: both cache types, sized by the memory of the pair like the
    // context is, the size of the cache, and the context checkpoints.
    "cache-type-k",
    "cache-type-v",
    "flash-attn",
    "cache-ram",
    "ctx-checkpoints",
    "checkpoint-min-step",
    // Speculation card: the draft runs on a GPU of the pair, and its ubatch and
    // cache types are sized by that GPU.
    "spec-draft-n-ubatch",
    "spec-draft-type-k",
    "spec-draft-type-v",
    // Advanced card: the models that sit in memory from the start, counted over
    // the pair (the load-on-startup memory check, section E).
    "load-on-startup",
];

/// The model profile keys that STAY in the model profile
/// ([`PLACEMENT_KEYS`] is the list that moves).
pub const MODEL_PROFILE_STAY_KEYS: &[&str] = &[
    // The model source: the file, and the repository and the file in it.
    "model",
    "hf-repo",
    "hf-file",
    // The vision projector file: what is loaded, not where.
    "mmproj",
    // The draft model file: what is loaded, not where.
    "model-draft",
    // Sampling overrides card: the model's voice, the same in every instance.
    "temp",
    "top-k",
    "top-p",
    "min-p",
    "repeat-penalty",
    "presence-penalty",
    "backend-sampling",
    // Chat and reasoning card.
    "n-predict",
    "jinja",
    "chat-template-kwargs",
    "prefill-assistant",
    "reasoning",
    "reasoning-format",
    "reasoning-effort",
    "reasoning-preserve",
    "reasoning-budget",
    "reasoning-budget-message",
    // Embeddings card: a mode of the model.
    "embeddings",
    "pooling",
    // KV cache card: how much of the cache is kept for the next request, a
    // reuse chunk and not a size.
    "cache-reuse",
    // Vision card: the tokens an image takes, read from the model.
    "image-min-tokens",
    "image-max-tokens",
    // Speculation card: the method and its bounds, not the draft's memory.
    "spec-type",
    "spec-draft-n-max",
    "spec-draft-n-min",
    "spec-draft-p-min",
    // The n-gram speculators' tuning: every key of that family.
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
/// off as an inline comment on reload, here and by llama-server alike. `hf-repo`
/// and `hf-file` carry a repository and a file name, so they are here too.
const MODEL_PROFILE_TEXT_KEYS: [&str; 9] = [
    "model",
    "mmproj",
    "model-draft",
    "chat-template-kwargs",
    "override-tensor",
    "reasoning-budget-message",
    "fit-target",
    "hf-repo",
    "hf-file",
];

/// The server.ini keys whose value is free text (paths, a URL, a key, the GPU
/// list: its own rule judges a shape no marker can ride in, so the marker rule
/// has to run on the line as written).
const SERVER_TEXT_KEYS: [&str; 5] = [
    "ModelsDir",
    "StateDir",
    "OpencodeBaseUrl",
    "OpencodeApiKey",
    "Devices",
];

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

/// A model profile names its model with ONE source: `model`, a file of the
/// node, not blank, or `hf_repo`, the Hugging Face pick the web writes and the
/// engine downloads, not blank. The save's rule, the one the two pickers
/// answer to. The whole-file rule is [`model_profile_has_model`], which takes
/// every [`FETCH_KEYS`] key: `model-url` and `docker-repo` name a model to the
/// engine and are not fields of the configurator's schema, so a save never
/// sees one.
pub fn model_profile_source(id: &str, model: &str, hf_repo: &str) -> Result<(), Refusal> {
    if model.trim().is_empty() && hf_repo.trim().is_empty() {
        Err(Refusal::new("model-profile-no-model", &[&id]))
    } else {
        Ok(())
    }
}

/// The keys whose value replaces the file with one the engine downloads:
/// `hf-repo` fetches it from a repository (`hf-file` names the file inside it,
/// and is no model of its own), `model-url` and `docker-repo` replace the value
/// with a URL and an image (llama.cpp `common/arg.cpp`). A profile holding one
/// of these names its model without a `model` line, and its file is not on the
/// node: the manager's model-file check skips it (`service.rs`).
pub const FETCH_KEYS: [&str; 3] = ["hf-repo", "model-url", "docker-repo"];

/// A model profile names its model: a not-blank `model`, or a not-blank key of
/// [`FETCH_KEYS`] (the Hugging Face pick the web writes has no file of the
/// node). Blank is no name: `model =` alone, and `hf-file` on its own, are
/// refused.
fn model_profile_has_model(s: &RawSection) -> bool {
    let named = |key: &str| s.get(key).is_some_and(|v| !v.trim().is_empty());
    named("model") || FETCH_KEYS.iter().any(|key| named(key))
}

/// The model files a model profile's `model` value names, in the order a check
/// looks for them: the one file as written, or every shard of a split model
/// when the value names one (a file stem ending `-NNNNN-of-NNNNN`, the names
/// llama.cpp writes, `{base}-{NNNNN}-of-{NNNNN}.{ext}`: `model_scan`,
/// `gguf::read_tensors`). A value that is not blank always names at least
/// itself, whatever its spelling. Empty for a blank value: the profile has no
/// file of its own, the `hf-repo` case a save accepts through
/// [`model_profile_source`]. The list is the whole of what the model-file
/// check looks at, so a repository in `hf-repo` adds nothing to look for.
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
///
/// J1 of "Layers, MoE and tensor placement on the web model profile page"
/// (ARCHITECTURE.md 5) adds what the web editor is judged on, and only on a key
/// line the write changed (`changed`, the exemption [`model_profile_rules]`
/// carries for `comment_markers`): a space at the ends of a pattern or of a
/// device (`override-tensor-space`, the position of the rule: llama.cpp keeps
/// the space, so the pattern matches no tensor and
/// `ggml_backend_dev_by_name` looks a name with a space in it up and finds no
/// device), and the device's shape (`override-bad-device`: a device id,
/// `[A-Za-z0-9_]+`, `CPU` only in the spelling llama.cpp uses, the device a card
/// name and not a description as `--device` takes). The pattern is the caller's:
/// llama.cpp compiles it as a regex, and a pattern it cannot read is its answer
/// at load time, not this rule's.
pub fn override_tensor(s: &str, changed: bool) -> Result<(), Refusal> {
    for (i, raw) in s.split(',').filter(|r| !r.trim().is_empty()).enumerate() {
        let n = i + 1;
        let rule = raw.trim();
        let (raw_pattern, raw_device) = rule.split_once('=').unwrap_or((rule, ""));
        let pattern = raw_pattern.trim();
        if pattern.is_empty() {
            return Err(Refusal::new("override-no-pattern", &[&n]));
        }
        let device = raw_device.trim();
        if device.is_empty() {
            return Err(Refusal::new("override-no-device", &[&n, &pattern]));
        }
        if changed {
            // The two parts as the rule's own text has them, against what the
            // reader keeps: a space at the ends of one is in the pattern, or in
            // the name `ggml_backend_dev_by_name` looks up.
            let (raw_pattern, raw_device) = raw.split_once('=').unwrap_or((raw, ""));
            if raw_pattern != pattern || raw_device != device {
                return Err(Refusal::new("override-tensor-space", &[&n]));
            }
            let id = device
                .chars()
                .all(|c| c.is_ascii_alphanumeric() || c == '_');
            if !id || (device.eq_ignore_ascii_case("cpu") && device != "CPU") {
                return Err(Refusal::new("override-bad-device", &[&n, &device]));
            }
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

/// The keys whose value is a list of device ids: one rule for all three.
pub const DEVICE_LIST_KEYS: [&str; 3] = ["device", "device-draft", "mmproj-device"];

/// One device list (a model profile's `device`, `device-draft`,
/// `mmproj-device`, and the server profile's `Devices`, the key `--device`
/// takes) as llama.cpp reads it: `parse_device_list` in `common/arg.cpp`
/// splits the value at `,`, keeps each entry byte for byte (it trims nothing),
/// and throws on an empty entry, on an entry naming the CPU device and on
/// `none` beside another name, so the model never loads. `value` is the value
/// the reader of the file sees (the preset reader `common/preset.cpp` drops
/// the spaces after `=`, ends the value at the spaces before a
/// `;`/`#`/newline and cuts it at the first `;` or `#`, which is
/// [`RawSection::get`]): an entry keeps the spaces inside the value, and
/// neither marker can ride in one, the value ending at the first. A line
/// carrying one is refused by [`comment_markers`], never here (this rule reads
/// `ROCm0;x` as one entry of the right shape): `Devices` is in the free-text
/// server keys for it, and a body value with a marker is the bff's 422
/// `comment-markers` for every key. An `=` does ride in an entry, and is
/// refused here: llama.cpp looks that name up and finds no device. The names
/// are judged by their shape only, never against the node's probe: an id the
/// probe does not answer is a value llama.cpp takes. Refused with
/// `device-bad-name`, naming the key and the entry.
pub fn device_list(key: &str, value: &str) -> Result<(), Refusal> {
    if value.is_empty() {
        return Ok(());
    }
    let entries = value.split(',').collect::<Vec<_>>();
    for entry in entries.iter() {
        let bad = entry.is_empty()
            || entry.chars().any(char::is_whitespace)
            || entry.contains('=')
            || entry.eq_ignore_ascii_case("CPU")
            // `none` beside another name: llama.cpp looks a device named
            // "none" up and finds none.
            || (entries.len() > 1 && entry.eq_ignore_ascii_case("none"));
        if bad {
            return Err(Refusal::new("device-bad-name", &[&key, &entry]));
        }
    }
    Ok(())
}

/// The first GPU a model profile section names (the entries of `device-draft`
/// and `mmproj-device`, and the device of each `override-tensor` rule, `CPU`
/// and `none` naming none) that lies outside the server profile's `Devices`
/// (L4, ARCHITECTURE.md "Layers, MoE and tensor placement on the web model
/// profile page" (3), slice 3b: the device bound):
/// the key and the device, compared case-insensitively; `devices` empty (the
/// key unset) is every GPU, `none` is none of them. Not judged against the
/// node's probe: an id the probe does not answer is a value llama.cpp takes
/// ([`device_list`]'s rule), and the key's own value rules run on it there.
/// `section` is one model profile's key lines, in the order the reader sees
/// them: a caller judging llama.cpp's `[*]` cascade puts the global section's
/// lines before the section's own, and the later line of a key is the one the
/// model profile uses.
pub fn devices_outside(section: &str, devices: &str) -> Option<(String, String)> {
    if devices.trim().is_empty() {
        return None;
    }
    let allowed: Vec<String> = devices
        .split(',')
        .map(|entry| entry.trim().to_lowercase())
        .collect();
    for key in &DEVICE_LIST_KEYS[1..] {
        for entry in last_value(section, key)
            .unwrap_or_default()
            .split(',')
            .map(str::trim)
        {
            if outside_bound(entry, &allowed) {
                return Some((key.to_string(), entry.to_string()));
            }
        }
    }
    for rule in last_value(section, "override-tensor")
        .unwrap_or_default()
        .split(',')
    {
        // llama.cpp splits the value at `,` and each rule at its first `=`: the
        // device is what follows, and a rule without an `=` names none (it
        // throws while the arguments are parsed).
        let device = rule.split_once('=').map_or("", |(_, d)| d.trim());
        if outside_bound(device, &allowed) {
            return Some(("override-tensor".to_string(), device.to_string()));
        }
    }
    None
}

/// True for a device id the bound `allowed` (the `Devices` entries, lowercased)
/// does not name: `CPU` and `none` name no GPU and llama.cpp refuses an empty
/// entry (`device_list`), so neither is judged; a GPU id is compared
/// case-insensitively, as `ggml_backend_dev_by_name` matches.
fn outside_bound(entry: &str, allowed: &[String]) -> bool {
    !entry.is_empty()
        && !entry.eq_ignore_ascii_case("CPU")
        && !entry.eq_ignore_ascii_case("none")
        && !allowed.contains(&entry.to_lowercase())
}

/// The value of the LAST line of `text` that sets `key`, as the reader sees it
/// ([`cut_comment`]), None when no line does. The last line is the one a reader
/// keeps, so a text carrying a cascaded value before the section's own reads
/// the section's.
fn last_value(text: &str, key: &str) -> Option<String> {
    let mut out = None;
    for line in text.lines() {
        let t = line.trim();
        if t.is_empty() || t.starts_with(';') || t.starts_with('#') || t.starts_with('[') {
            continue;
        }
        if let Some((k, v)) = t.split_once('=') {
            if k.trim() == key {
                out = Some(cut_comment(v.trim()).to_string());
            }
        }
    }
    out
}

/// The `tensor-split` weights as llama.cpp parses them (`--tensor-split` in
/// `common/arg.cpp`): the value it reads (see [`device_list`]) split at every
/// run of `,` and `/` (`split_weights`), at most 15 entries (it throws at
/// `llama_max_devices()`, 16), each read with `std::stof` so a finite number of
/// 0 or more; the number rule here is stricter than `std::stof`, which reads
/// `3x` as 3 (a preference: a negative, `inf`, `nan` or a trailing letter is
/// never a weight the configurator writes). Refused with `tensor-split-format`,
/// naming the value.
pub fn tensor_split(value: &str) -> Result<(), Refusal> {
    if value.is_empty() {
        return Ok(());
    }
    let entries = split_weights(value);
    let taken = entries.len() <= TENSOR_SPLIT_MAX
        && entries
            .iter()
            .all(|e| e.parse::<f64>().is_ok_and(|n| n.is_finite() && n >= 0.0));
    if taken {
        return Ok(());
    }
    Err(Refusal::new("tensor-split-format", &[&value]))
}

/// The pieces of `value` between the runs of `,` and `/`, as the engine's
/// `sregex_token_iterator` with `-1` yields them (the regex `[,/]+` of
/// `--tensor-split`): one run is one separator, a run at the start leaves an
/// empty first piece (`std::stof` throws on it) and a run at the end ends the
/// list, so `3,,1`, `3//1` and `3,1,` are the two weights 3 and 1.
fn split_weights(value: &str) -> Vec<&str> {
    let mut out: Vec<&str> = Vec::new();
    let mut start = 0;
    let mut in_run = false;
    for (at, c) in value.char_indices() {
        if c == ',' || c == '/' {
            if !in_run {
                out.push(&value[start..at]);
                in_run = true;
            }
        } else if in_run {
            start = at;
            in_run = false;
        }
    }
    if !in_run {
        out.push(&value[start..]);
    }
    out
}

/// The most entries of `tensor-split`: llama.cpp throws at one more.
pub const TENSOR_SPLIT_MAX: usize = 15;

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

/// `value` of `key` as a bool the readers accept (`ini::parse_bool`: `true` or
/// `false`, spaces around allowed). The refusal is [`one_of`]'s, naming the key
/// in its first value, which is the control the web form marks
/// (`bff::server::field_of`).
pub fn bool_value(key: &str, value: &str) -> Result<(), Refusal> {
    if ini::parse_bool(value).is_some() {
        return Ok(());
    }
    Err(Refusal::new(
        "not-one-of",
        &[&key, &value.trim(), &"true, false"],
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

/// The most characters of one instance's label (the `Label` key of its
/// `instances.ini` section).
pub const LABEL_MAX: usize = 64;

/// One instance's label: spaces and tabs trimmed at both ends, then empty
/// (the caller removes the key) or 1 to [`LABEL_MAX`] characters with no
/// control character and no `;` or `#` ([`comment_markers`]: the INI reader
/// cuts a value at one of those). The key lives on one line of
/// `instances.ini`, so a control character is a second line. Each refusal
/// carries the trimmed text, the one the file would have held.
pub fn instance_label(text: &str) -> Result<(), Refusal> {
    let label = text.trim_matches([' ', '\t']);
    if label.is_empty() {
        return Ok(());
    }
    if label.contains([';', '#']) {
        return Err(Refusal::new("comment-markers", &[&label]));
    }
    if label.chars().count() > LABEL_MAX || label.chars().any(char::is_control) {
        return Err(Refusal::new("instance-bad-label", &[&label]));
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
    /// comment cut (`ini::parse_sections`). The value a rule that refuses what
    /// llama.cpp reads judges (`device`, `tensor-split`): the preset reader
    /// (`common/preset.cpp`) drops the spaces after `=`, ends the value at the
    /// spaces before a `;`/`#`/newline and cuts it at the first `;` or `#`.
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
/// section of the same id takes no `comment_markers`, and the same exemption
/// keeps `override_tensor`'s two web rules off a rule line the write did not
/// touch; every other rule runs on the whole section.
fn model_profile_section(s: &RawSection, known: &[RawSection]) -> Result<(), Refusal> {
    model_profile_id(&s.id)?;
    if !model_profile_has_model(s) {
        return Err(Refusal::new("model-profile-no-model", &[&s.id]));
    }
    for (key, raw) in &s.keys {
        if MODEL_PROFILE_TEXT_KEYS.contains(&key.as_str())
            && !known_key_line(known, &s.id, key, raw)
        {
            comment_markers(key, raw)?;
        }
    }
    fit_target(&s.get("fit-target").unwrap_or_default())?;
    let override_rules = s.get("override-tensor").unwrap_or_default();
    override_tensor(
        &override_rules,
        !known_key_line(known, &s.id, "override-tensor", &override_rules),
    )?;
    for key in DEVICE_LIST_KEYS {
        if let Some(v) = s.get(key) {
            device_list(key, &v)?;
        }
    }
    if let Some(v) = s.get("tensor-split") {
        tensor_split(&v)?;
    }
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
    // The server profile's GPUs, by the list rule's shape only (never against
    // the probe: a card can vanish for a while and must not block a save).
    if let Some(v) = s.get("Devices") {
        device_list("Devices", &v)?;
    }
    // The GTT switch is a bool in the file, and the launch reads it for every
    // instance: a word the reader drops silently leaves a file that says one
    // thing and a launch that does another (the runtime's own defaults return),
    // so refuse it here, where every surface that writes the file is checked.
    if let Some(v) = s.get("RocmHostMemoryGtt") {
        bool_value("RocmHostMemoryGtt", &v)?;
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

/// Check the text of a model profile file (`models.ini`) for a placement or
/// memory key of [`PLACEMENT_KEYS`] left in a model profile section: after the
/// migration those keys live in the instance profile's entry `[<p>/<m>]`, so a
/// model profile carrying one is two sources of truth for where the weights go.
/// The refusal names the key (`model-profile-placement-key`).
pub fn model_profile_placement(text: &str) -> Result<(), Refusal> {
    for section in ini::parse_sections(text) {
        if section.id == "*" {
            continue;
        }
        if let Some((key, _)) = section.keys.iter().find(|(k, _)| placement_key(k)) {
            return Err(Refusal::new("model-profile-placement-key", &[key]));
        }
    }
    Ok(())
}

/// True when `key` decides where a model's memory sits, so that it belongs to
/// an instance profile's entry and not to the model profile.
pub fn placement_key(key: &str) -> bool {
    PLACEMENT_KEYS.contains(&key)
}

#[cfg(test)]
#[path = "checks_tests.rs"]
mod tests;
