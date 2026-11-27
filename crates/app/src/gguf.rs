//! Describe a model in the GUI's "Model info" box: dense-vs-MoE (+ layer split),
//! layer count, trained context, GQA shape, quant, embedded MTP, the embedded
//! chat template (Jinja vs non-Jinja detection, with a raw-text preview), plus
//! the selected mmproj (clip) and draft/DFlash headers. Also classifies a draft
//! GGUF into the `--spec-type` it calls for (`draft_spec_type`).
//!
//! Every line here describes a file the model profile actually LOADS. That is a rule,
//! not an accident: the box used to also advertise the drafter whose filename
//! best matched the model, and a row that names a file nobody loaded is read as
//! a claim about the running config (see `draft_line`).
//!
//! Metadata is read with llama.cpp's OWN gguf reader: we runtime-load
//! `ggml-base.dll` (shipped next to `pai-config.exe` in `bin\`; on Linux
//! `libggml-base.so.0`, in the package's `lib/`) and call
//! `gguf_init_from_file` with `no_alloc = true`, so only the header + tensor
//! infos are read, never the multi-GB weights. No GGUF parsing is reimplemented
//! here. If the DLL can't be loaded (e.g. a bare `cargo run` with no llama.cpp
//! alongside), reads return `None` and the box shows "unavailable", and the
//! load is retried on the next read (only success is memoized; see `ffi::api`),
//! so a DLL that appears later (a finished `xtask build`) is picked up live.
//!
//! Reads are synchronous and uncached: the header parse is fast enough to run on
//! the UI thread when the model/mmproj/draft selection changes.
//!
//! Key names and the `general.file_type` enum mirror the bundled llama.cpp
//! (`src/llama-arch.cpp`, `include/llama.h`, `tools/mtmd/clip-impl.h`); the GGUF
//! value-type enum mirrors `ggml/include/gguf.h`.

use crate::strings;
use std::path::Path;

/// Separator between the parts of a Model-info box line ("arch · quant · size").
/// Single source so every line joins on the exact same glyph + spacing.
const SEP: &str = "  ·  ";

/// Read a handful of typed metadata values by key. Abstracts over the live gguf
/// context (via `ggml-base.dll`) so the field-extraction logic stays testable
/// without the DLL (see the `tests` module's map-backed source).
trait KvSource {
    fn u32(&self, key: &str) -> Option<u32>;
    fn string(&self, key: &str) -> Option<String>;
    fn boolean(&self, key: &str) -> Option<bool>;
    /// A number of any stored width (float32/float64 or any integer) as `f64`.
    /// The stored type is not a contract for the keys this reads: llama.cpp
    /// takes the `general.sampling.*` values through their STRING form
    /// (`llama_model_meta_val_str` + `strtof`, `common_init_sampler_from_model`),
    /// so a converter writing a float as an int still reaches it, and must reach
    /// us too. A float32 comes back as its shortest decimal (`f32_decimal`), so
    /// `0.6` reads `0.6` and not `0.6000000238418579`.
    fn f64(&self, key: &str) -> Option<f64>;
    /// A signed integer of any stored integer width.
    fn i64(&self, key: &str) -> Option<i64>;
    /// A tensor's `(ggml_type, size_in_bytes)` by name, from the GGUF's tensor
    /// infos, not its KV block. `None` when the tensor is absent.
    fn tensor(&self, name: &str) -> Option<(u32, u64)>;
}

/// A float32 widened through its shortest decimal form, the value a human (and
/// the converter's `generation_config.json`) wrote: `0.95f32 as f64` is
/// `0.949999988079071`, which would land in `models.ini` verbatim.
fn f32_decimal(v: f32) -> f64 {
    v.to_string().parse().unwrap_or(v as f64)
}

// ── Public descriptions ──────────────────────────────────────────────────

#[derive(Debug, Clone)]
pub struct ModelInfo {
    pub arch: String,
    pub size_label: String,
    /// Human quant name (e.g. "Q4_K_M"), or empty if `general.file_type` absent.
    pub quant: String,
    pub is_moe: bool,
    pub expert_count: u32,
    pub expert_used: u32,
    pub expert_shared_count: u32,
    /// Leading dense (non-MoE) blocks, e.g. DeepSeek-style hybrids. 0 = all MoE.
    pub leading_dense_block_count: u32,
    /// Some archs place an MoE block every N layers instead of a dense prefix.
    pub moe_every_n_layers: u32,
    pub n_layer: u32,
    pub n_ctx_train: u32,
    pub head_count: u32,
    pub head_count_kv: u32,
    /// `<arch>.embedding_length`, the model width and so `token_embd`'s row
    /// length (`ne[0]`). Read for one decision only: ggml-cuda's `GET_ROWS`
    /// kernel accepts an IQ4_NL / MXFP4 embedding table only when that width is
    /// a multiple of the 256-value super-block (`gpu_has_get_rows`). 0 if absent.
    pub n_embd: u32,
    /// `<arch>.nextn_predict_layers`; > 0 means the model embeds MTP heads.
    /// Read per FILE, which since llama.cpp v0.4.0 (#28159) is also how upstream
    /// reads it: the key moved from the ~18 per-arch loaders that bothered into
    /// `llama_model::load_hparams` for every arch, so a nextn-bearing GGUF now
    /// means MTP heads whatever its arch (the one exception being an arch that
    /// repurposes the count for a router layer, granite-switch, which llama.cpp
    /// refuses as MTP with the same "doesn't contain MTP layers" line).
    pub nextn_predict_layers: u32,
    /// `<arch>.block_size`: DFlash drafters' trained diffusion block; the
    /// `--spec-draft-n-max` ceiling is `block_size - 1`. 0 if absent.
    pub block_size: u32,
    /// Raw `tokenizer.chat_template` metadata (None when the GGUF carries
    /// none). `chat_template_line()` classifies it for the info box; the GUI's
    /// Preview button shows this raw text in a modal.
    pub chat_template: Option<String>,
    /// `token_embd.weight`'s `(ggml_type, bytes)`. Both halves drive a decision
    /// the rest of the box can't: whether an `override-tensor` rule pinning the
    /// embedding table to a GPU is a win or a catastrophe; see `embd_line`.
    /// None when the GGUF names its embedding tensor something else.
    pub embd: Option<(u32, u64)>,
    /// The sampler defaults the converter copied from the model's
    /// `generation_config.json` (`general.sampling.*`), which llama-server
    /// applies to every parameter the command line leaves unset.
    pub sampling: Sampling,
}

/// The `general.sampling.*` values a GGUF carries that a model profile has a field
/// for (`gguf-py/gguf/metadata.py` writes them; `common_init_sampler_from_model`
/// applies them). The rest of that family (`penalty_last_n`, `xtc_*`,
/// `mirostat*`, `sequence`) has no model profile key, and needs none to take effect:
/// llama-server reads them from the file whenever the launch leaves them unset.
/// `None` = the file does not say.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Sampling {
    pub temp: Option<f64>,
    pub top_k: Option<i32>,
    pub top_p: Option<f64>,
    pub min_p: Option<f64>,
    /// `general.sampling.penalty_repeat`, the model profile's `repeat-penalty`.
    pub penalty_repeat: Option<f64>,
}

impl Sampling {
    fn from_kv<S: KvSource>(s: &S) -> Sampling {
        let f = |k: &str| s.f64(&format!("general.sampling.{k}"));
        Sampling {
            temp: f("temp"),
            top_k: s
                .i64("general.sampling.top_k")
                .and_then(|v| i32::try_from(v).ok()),
            top_p: f("top_p"),
            min_p: f("min_p"),
            penalty_repeat: f("penalty_repeat"),
        }
    }
}

#[derive(Debug, Clone)]
pub struct MmprojInfo {
    pub projector_type: String,
    pub has_vision: bool,
    pub has_audio: bool,
    pub vision_block_count: u32,
    pub image_size: u32,
    pub patch_size: u32,
}

// ── Reasoning support, read off the chat template ────────────────────────

/// What the embedded chat template does about thinking. Derived by SCANNING THE
/// TEMPLATE SOURCE for the variables and tags llama.cpp's own levers act on; we
/// have no Jinja engine here, while llama.cpp answers the same question by
/// *rendering* the template twice and diffing the output
/// (`common_chat_templates_support_enable_thinking`, `jinja::caps_get`). So this
/// is an approximation, and it is one-directional on purpose: every verdict below
/// rests on a name the template must mention for the lever to reach it, so a
/// `yes` is a fact about the template, while a `no` can only ever mean "no marker
/// this build knows about", which is why the row says what it read, and the
/// llama-server load log stays the last word.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Thinking {
    /// The GGUF embeds no template at all: nothing to read.
    NoTemplate,
    /// No thinking marker anywhere: `--reasoning` has nothing to switch.
    None,
    /// Gated on `enable_thinking`: the kwarg `--reasoning on|off` writes
    /// (`common/arg.cpp`: `default_template_kwargs["enable_thinking"]`).
    Switchable,
    /// Thinks unconditionally, but takes an effort LEVEL (gpt-oss / harmony).
    /// `--reasoning off` cannot silence it; `--reasoning-effort` (b10434+) tunes
    /// it, and llama-server reports nothing about that flag at startup, which
    /// makes this verdict the only signal the user gets. See `EFFORT_VARS`.
    EffortOnly,
    /// Thinks unconditionally and exposes no switch; `--reasoning` is inert.
    Always,
}

/// The three variables `--reasoning-preserve` sets, verbatim from
/// `caps_apply_preserve_reasoning` (`common/jinja/caps.cpp`). A template that
/// mentions none of them cannot see the flag. Keep in sync with that function:
/// a variable added upstream and missing here reads as "the flag is a no-op" on
/// the very templates it was added for.
const PRESERVE_VARS: [&str; 3] = [
    "preserve_thinking",
    "clear_thinking",
    "truncate_history_thinking",
];

/// The two variables `--reasoning-effort` sets, verbatim from
/// `caps_apply_reasoning_effort` (`common/jinja/caps.cpp`), which binds ONE value
/// to both names because templates disagree on which to read. Same rule as
/// `PRESERVE_VARS`: matching only one of them reads as "the level is not a lever
/// here" on every template keyed to the other.
const EFFORT_VARS: [&str; 2] = ["reasoning_effort", "reasoning_strength"];

/// Evidence that a template handles a reasoning trace. Every entry is template
/// *mechanics* (a variable, a message field, or a trace tag), never prose, so a
/// system prompt that merely says "think step by step" is not mistaken for a
/// thinking model. Drawn from llama.cpp's bundled template corpus
/// (`models/templates/`, 60 files): the tags cover DeepSeek-R1 / Qwen / GLM /
/// Kimi (`<think>`), Mistral and Ministral-Reasoning (`[THINK]`, which also
/// carries its trace as a `'thinking'` content block), ByteDance Seed
/// (`<seed:think>`) and gpt-oss (`<|channel|>`, `message.thinking`), while
/// `reasoning_content` is the field a preserved trace travels in.
const THINKING_MARKERS: [&str; 12] = [
    "enable_thinking",
    "reasoning_content",
    "reasoning_effort",
    "reasoning_strength",
    ".thinking",
    "'thinking'",
    "\"thinking\"",
    "<think>",
    "</think>",
    "[THINK]",
    "<seed:think>",
    "<|channel|>",
];

// ── Field extraction (platform-independent, testable) ────────────────────

impl ModelInfo {
    fn from_kv<S: KvSource>(s: &S) -> Option<ModelInfo> {
        let arch = s.string("general.architecture")?;
        let a = |suffix: &str| s.u32(&format!("{arch}.{suffix}"));

        let expert_count = a("expert_count").unwrap_or(0);
        let quant = s
            .u32("general.file_type")
            .map(ftype_name)
            .unwrap_or_default();

        Some(ModelInfo {
            size_label: s.string("general.size_label").unwrap_or_default(),
            quant,
            is_moe: expert_count > 0,
            expert_count,
            expert_used: a("expert_used_count").unwrap_or(0),
            expert_shared_count: a("expert_shared_count").unwrap_or(0),
            leading_dense_block_count: a("leading_dense_block_count").unwrap_or(0),
            moe_every_n_layers: a("moe_every_n_layers").unwrap_or(0),
            n_layer: a("block_count").unwrap_or(0),
            n_ctx_train: a("context_length").unwrap_or(0),
            head_count: a("attention.head_count").unwrap_or(0),
            head_count_kv: a("attention.head_count_kv").unwrap_or(0),
            n_embd: a("embedding_length").unwrap_or(0),
            nextn_predict_layers: a("nextn_predict_layers").unwrap_or(0),
            block_size: a("block_size").unwrap_or(0),
            chat_template: s
                .string("tokenizer.chat_template")
                .filter(|t| !t.is_empty()),
            embd: s.tensor("token_embd.weight"),
            sampling: Sampling::from_kv(s),
            arch,
        })
    }

    /// Whether the embedded chat template is Jinja (`Some(true)`), a legacy
    /// non-Jinja one (`Some(false)`), or absent (`None`). The `{%`/`{{`
    /// heuristic mirrors llama.cpp's own `common_chat_verify_template`; the
    /// "Chat template" row and the model profile derivation (`hf::derive_model_profile`) share
    /// it so they cannot disagree.
    pub fn chat_template_is_jinja(&self) -> Option<bool> {
        self.chat_template
            .as_deref()
            .map(|t| t.contains("{%") || t.contains("{{"))
    }

    // ── Display strings for the GUI ──────────────────────────────────────

    pub fn kind_line(&self) -> String {
        if !self.is_moe {
            return strings::t("dense");
        }
        let mut parts = vec![strings::fill("experts-count", &[&self.expert_count])];
        if self.expert_used > 0 {
            parts.push(strings::fill("experts-active", &[&self.expert_used]));
        }
        if self.expert_shared_count > 0 {
            parts.push(strings::fill(
                "experts-shared",
                &[&self.expert_shared_count],
            ));
        }
        strings::fill("moe-kind", &[&parts.join(", ")])
    }

    /// How many transformer layers actually carry MoE expert weights: the count
    /// that sizes the `--n-cpu-moe` lever (which keeps the first N layers' experts
    /// on CPU, trading VRAM for speed). Empty for dense models, where the lever is
    /// a no-op and the FFN lever is `--n-cpu-ffn` instead (`ModelProfile::n_cpu_ffn`,
    /// sized by the plain layer count, so it needs no row of its own).
    pub fn moe_offload_line(&self) -> String {
        if !self.is_moe || self.n_layer == 0 {
            return String::new();
        }
        if self.leading_dense_block_count > 0 && self.leading_dense_block_count < self.n_layer {
            strings::fill(
                "moe-layers-after-dense",
                &[
                    &(self.n_layer - self.leading_dense_block_count),
                    &self.n_layer,
                    &self.leading_dense_block_count,
                ],
            )
        } else if self.moe_every_n_layers > 1 {
            strings::fill(
                "moe-layers-every",
                &[&self.moe_every_n_layers, &self.n_layer],
            )
        } else {
            strings::fill("moe-layers-all", &[&self.n_layer])
        }
    }

    pub fn arch_quant_line(&self) -> String {
        let mut parts: Vec<&str> = vec![&self.arch];
        if !self.quant.is_empty() {
            parts.push(&self.quant);
        }
        if !self.size_label.is_empty() {
            parts.push(&self.size_label);
        }
        parts.join(SEP)
    }

    /// The "Embeddings" row: `token_embd.weight`'s own quant and size. Facts only:
    /// the VERDICT they imply is `embd_pin_warning`, which the Tensor-placement
    /// table raises at the moment a rule actually pins the table. Both halves of
    /// this row exist because that verdict has two independent preconditions:
    ///
    /// Pinning the embedding table is the documented cure for the GBs of Windows
    /// "Shared GPU memory" a fully-offloaded model still shows (llama.cpp parks
    /// `token_embd` in a pinned host buffer even at `offloaded N/N layers`). But
    /// the cure has TWO preconditions, and violating either turns a win into a
    /// large loss, both seen on a 27B split across ROCm + CUDA:
    ///
    /// 1. **The type must have a GPU `get_rows` kernel.** Pin a table of a type
    ///    the backend cannot `GET_ROWS` and the tensor sits in VRAM the GPU cannot
    ///    read, so the lookup round-trips to the host EVERY token: decode collapsed
    ///    (the penalty tracks the table size). Prefill is untouched
    ///    (one round-trip per 2048-token batch amortizes it), which is why this
    ///    reads as "the GPU is idle" rather than "the GPU is slow". That was
    ///    on b9976, when ggml-cuda/ggml-hip whitelisted only
    ///    F32/F16/BF16/I32/Q1_0/Q4_0/Q4_1/Q5_0/Q5_1/Q8_0; **b10089** (#25962,
    ///    2026-07-22, "cuda: GET_ROWS quants", whose PR text names exactly that
    ///    round-trip as what it fixes) added every K-quant and IQ-quant, so from
    ///    the b10428 bundle on a Q4_K/Q6_K table pins fine and the warning fires
    ///    only for what is still left out: `gpu_has_get_rows` is the list. The
    ///    trap that survives is that a file's NAME does not tell you the type:
    ///    Unsloth's Q6_K_XL keeps `token_embd` at Q8_0 while its Q4_K_XL/Q5_K_XL
    ///    use Q4_K/Q5_K, and the row exists so the type is read, not guessed.
    /// 2. **The card must have room for it.** Pinning MOVES the table out of host
    ///    RAM and into VRAM, so it is a straight add to the device's footprint,
    ///    hence the size here. With the KV cache already filling the card, that
    ///    add is what tips it into paging (a Q8_0 table on a card already holding
    ///    a large f16 KV cache). Size is the number to budget against.
    pub fn embd_line(&self) -> String {
        match self.embd {
            Some((ty, bytes)) => {
                format!("{}{SEP}{}", ggml_type_name(ty), format_mib(bytes))
            }
            None => strings::t("not-available"),
        }
    }

    /// The warning the Tensor-placement table shows when this model's embedding
    /// table is pinned to a GPU it cannot be read from. Empty when the type is
    /// safe, and equally empty when it is UNKNOWN (no `token_embd.weight`, or no
    /// `ggml-base.dll` to read it with): a metadata read we couldn't do is not
    /// evidence of a problem, and crying wolf on every unreadable GGUF would train
    /// the warning away. So `!embd_pinnable()` alone must never gate this.
    pub fn embd_pin_warning(&self) -> String {
        match self.embd {
            Some((ty, _)) if !gpu_has_get_rows(ty, self.n_embd) => {
                strings::fill("embd-pin-warning", &[&ggml_type_name(ty)])
            }
            _ => String::new(),
        }
    }

    /// The warning the GPU distribution table shows under split mode `tensor`
    /// when llama.cpp's tensor parallelism does not implement this model's
    /// architecture: the load fails outright (`llama_model_create` throws
    /// `LLAMA_SPLIT_MODE_TENSOR not implemented for architecture '...'`,
    /// `src/llama-model.cpp`), and nothing the table edits can change that, so it
    /// is worth saying at the pick rather than at the next launch. Empty for
    /// every arch off the deny-list. Unlike `embd_pin_warning` there is no
    /// "unknown" here to stay quiet on: `from_kv` requires `general.architecture`,
    /// so an unreadable header never reaches this.
    pub fn sm_tensor_warning(&self) -> String {
        if arch_supports_sm_tensor(&self.arch) {
            String::new()
        } else {
            strings::fill("sm-tensor-warning", &[&self.arch])
        }
    }

    pub fn layers_ctx_line(&self) -> String {
        let mut parts: Vec<String> = Vec::new();
        if self.n_layer > 0 {
            parts.push(strings::fill("layers-count", &[&self.n_layer]));
        }
        if self.n_ctx_train > 0 {
            parts.push(strings::fill(
                "trained-ctx",
                &[&group_thousands(self.n_ctx_train as u64)],
            ));
        }
        if parts.is_empty() {
            strings::t("not-available")
        } else {
            parts.join(SEP)
        }
    }

    pub fn attn_line(&self) -> String {
        if self.head_count == 0 {
            return "n/a".to_string();
        }
        if self.head_count_kv == 0 || self.head_count_kv == self.head_count {
            return strings::fill("heads-mha", &[&self.head_count]);
        }
        if self.head_count.is_multiple_of(self.head_count_kv) {
            strings::fill(
                "heads-gqa-ratio",
                &[
                    &self.head_count,
                    &self.head_count_kv,
                    &(self.head_count / self.head_count_kv),
                ],
            )
        } else {
            strings::fill("heads-gqa", &[&self.head_count, &self.head_count_kv])
        }
    }

    /// The "Memory" row: what llama.cpp keeps of past tokens for this arch
    /// (`arch_memory`).
    pub fn memory_line(&self) -> String {
        arch_memory(&self.arch)
    }

    /// The "Thinking" row: does this model reason, and can `--reasoning on|off`
    /// switch it?
    pub fn thinking_line(&self) -> String {
        match self.thinking() {
            Thinking::NoTemplate => strings::t("no-embedded-template"),
            Thinking::None => strings::t("no-thinking-marker"),
            Thinking::Switchable => strings::t("yes"),
            Thinking::EffortOnly => strings::t("yes"),
            Thinking::Always => strings::t("yes"),
        }
    }

    /// The "Past thinking" row: is `--reasoning-preserve` a LEVER on this
    /// template, i.e. does the template read any of the three variables the flag
    /// sets?
    pub fn past_thinking_line(&self) -> String {
        match (self.thinking(), self.preserve_var()) {
            (Thinking::NoTemplate, _) => strings::t("no-embedded-template"),
            (_, Some(_)) => strings::t("yes"),
            (_, None) => strings::t("no"),
        }
    }

    /// Which of `--reasoning-preserve`'s three template variables this template
    /// reads, if any. First match wins; a template naming several (none in
    /// llama.cpp's bundled set does) is still driven by the same one flag.
    fn preserve_var(&self) -> Option<&'static str> {
        let t = self.chat_template.as_deref()?;
        PRESERVE_VARS.iter().copied().find(|v| t.contains(v))
    }

    fn thinking(&self) -> Thinking {
        let Some(t) = self.chat_template.as_deref() else {
            return Thinking::NoTemplate;
        };
        if t.contains("enable_thinking") {
            return Thinking::Switchable;
        }
        if !THINKING_MARKERS.iter().any(|m| t.contains(m)) && self.preserve_var().is_none() {
            return Thinking::None;
        }
        if EFFORT_VARS.iter().any(|v| t.contains(v)) {
            return Thinking::EffortOnly;
        }
        Thinking::Always
    }

    /// One-line status for the Model info "Chat template" row: `Jinja
    /// (embedded)` when the GGUF ships a `tokenizer.chat_template` with Jinja
    /// `{%`/`{{}` markers, `embedded (non-Jinja)` for a marker-less (older /
    /// builtin-name) template, or `none` when the metadata is absent. The
    /// `{%`/`{{}` heuristic mirrors llama.cpp's own `common_chat_verify_template`.
    pub fn chat_template_line(&self) -> String {
        match self.chat_template_is_jinja() {
            None => strings::t("none"),
            Some(true) => strings::t("template-jinja"),
            Some(false) => strings::t("template-other"),
        }
    }

    /// One-line description of a *draft* file's own header (arch, quant, layers,
    /// DFlash block_size → implied `--spec-draft-n-max` ceiling, embedded nextn).
    pub fn draft_file_line(&self) -> String {
        let mut parts = vec![self.arch.clone()];
        if !self.quant.is_empty() {
            parts.push(self.quant.clone());
        }
        parts.push(if self.n_layer > 0 {
            strings::fill("layers-count", &[&self.n_layer])
        } else {
            strings::t("layers-head")
        });
        if self.block_size > 0 {
            parts.push(strings::fill(
                "block-size",
                &[&self.block_size, &self.block_size.saturating_sub(1)],
            ));
        }
        if self.nextn_predict_layers > 0 {
            parts.push(strings::fill("nextn", &[&self.nextn_predict_layers]));
        }
        parts.join(SEP)
    }
}

impl MmprojInfo {
    fn from_kv<S: KvSource>(s: &S) -> Option<MmprojInfo> {
        let projector_type = s.string("clip.projector_type").unwrap_or_default();
        let has_vision = s.boolean("clip.has_vision_encoder").unwrap_or(false);
        let has_audio = s.boolean("clip.has_audio_encoder").unwrap_or(false);
        // Not an mmproj/clip file if none of these are present.
        if projector_type.is_empty() && !has_vision && !has_audio {
            return None;
        }
        Some(MmprojInfo {
            projector_type,
            has_vision,
            has_audio,
            vision_block_count: s.u32("clip.vision.block_count").unwrap_or(0),
            image_size: s.u32("clip.vision.image_size").unwrap_or(0),
            patch_size: s.u32("clip.vision.patch_size").unwrap_or(0),
        })
    }

    pub fn mmproj_line(&self) -> String {
        let modality = match (self.has_vision, self.has_audio) {
            (true, true) => strings::key("vision-and-audio"),
            (true, false) => strings::key("modality-vision"),
            (false, true) => strings::key("modality-audio"),
            (false, false) => strings::key("modality-projector"),
        };
        let mut parts: Vec<String> = Vec::new();
        if !self.projector_type.is_empty() {
            parts.push(self.projector_type.clone());
        }
        parts.push(strings::t(modality));
        if self.vision_block_count > 0 {
            parts.push(strings::fill("encoder-layers", &[&self.vision_block_count]));
        }
        if self.image_size > 0 {
            if self.patch_size > 0 {
                parts.push(strings::fill(
                    "image-patch",
                    &[&self.image_size, &self.patch_size],
                ));
            } else {
                parts.push(format!("{}px", self.image_size));
            }
        }
        parts.join(SEP)
    }
}

/// One-line summary of this model profile's drafting situation, for the "Draft" row of
/// the info box: the MTP/next-n layers the MODEL embeds, plus the draft file the
/// model profile points at (`selected_draft`, its `--model-draft`, empty when none).
///
/// Everything here is something that is actually LOADED, and that rule is the
/// whole design of the row. It used to also advertise the drafter whose filename
/// best matched the model in `mtps\` / `dflashs\`, which read as state and was
/// not: the same answer whether or not the model profile used that file, unchanged by
/// picking or dropping one, and identical after a restart (reported twice
/// against v1.11.2). Worse, the match was a token overlap, so a
/// `Qwen3.8-27B-UD` model profile was offered the `Qwen3.8-27B-Uncensored-…-FastMTP`
/// head on `qwen3` + `27b` alone: a drafter for a different fine-tune, which
/// llama.cpp would happily load and then reject nearly every token of. The
/// picker in the Assets card lists every drafter on disk anyway, so nothing was
/// lost with it.
pub fn draft_line(info: &ModelInfo, selected_draft: &str) -> String {
    let mut parts = Vec::new();

    if info.nextn_predict_layers > 0 {
        let n = info.nextn_predict_layers;
        parts.push(if n == 1 {
            strings::t("mtp-layers-one")
        } else {
            strings::fill("mtp-layers-many", &[&n])
        });
    }

    let selected = selected_draft.trim();
    if !selected.is_empty() {
        // The picked file's own header is the row below this one ("Draft file"),
        // so this half only has to say WHICH file is in play.
        let name = Path::new(selected)
            .file_name()
            .map(|s| s.to_string_lossy().into_owned())
            .unwrap_or_else(|| selected.to_string());
        parts.push(strings::fill("draft-file-named", &[&name]));
    }

    if parts.is_empty() {
        return strings::t("none");
    }
    parts.join(SEP)
}

// ── Reading via ggml-base.dll ────────────────────────────────────────────

/// Read a model's GGUF metadata for the info box. `None` if the file can't be
/// opened (e.g. `ggml-base.dll` isn't loadable, or it isn't a valid GGUF).
pub fn read_model_info(path: &Path) -> Option<ModelInfo> {
    let ctx = ffi::open(path)?;
    ModelInfo::from_kv(&ctx)
}

/// Read an mmproj/clip GGUF header. `None` if the file isn't a clip mmproj.
pub fn read_mmproj_info(path: &Path) -> Option<MmprojInfo> {
    let ctx = ffi::open(path)?;
    MmprojInfo::from_kv(&ctx)
}

// ── The tensor list (the Tensor placement picker) ────────────────────────

/// One tensor of a model: its GGUF name (`blk.12.ffn_up_exps.weight`) and its
/// size in the file, which is what it takes on the device it lands on.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TensorInfo {
    pub name: String,
    pub bytes: u64,
}

/// Every tensor of the model at `path`, in file order. A split model
/// (`-00001-of-00003.gguf`) keeps each shard's tensors in that shard, so every
/// shard is read; one that is missing or unreadable makes the whole list `None`
/// rather than a partial list that would read as the model's.
pub fn read_tensors(path: &Path) -> Option<Vec<TensorInfo>> {
    let mut out = ffi::open(path)?.tensors();
    let stem = path.file_stem()?.to_str()?;
    if let Some((base, "00001")) = crate::model_scan::split_shard_suffix(stem) {
        let total: u32 = stem.rsplit_once("-of-")?.1.parse().ok()?;
        let ext = path.extension()?.to_str()?;
        for i in 2..=total {
            let shard = path.with_file_name(format!("{base}-{i:05}-of-{total:05}.{ext}"));
            out.extend(ffi::open(&shard)?.tensors());
        }
    }
    (!out.is_empty()).then_some(out)
}

// ── Which speculator a draft file calls for ──────────────────────────────

/// The `--spec-type` a DRAFT GGUF asks for, read from its own header. `None`
/// when the header names no speculator this build knows (an unreadable file
/// included: no `ggml-base.dll`, or not a GGUF at all).
///
/// Mirrors llama.cpp's `common_speculative_types_from_gguf`
/// (`common/speculative.cpp`, added in b10406 and extended to MTP in b10428),
/// which llama-server now runs itself on `--model-draft` when no `--spec-type`
/// was given. Two reasons this exists anyway: the framework always WRITES
/// `spec-type` into the model profile, so upstream's auto-detect never fires; and the
/// FOLDER a drafter was dropped in (the picker's other source of truth) cannot
/// tell `draft-dspark` from `draft-dflash`, since both live in `dflashs\` and
/// only a tensor inside the file separates them.
///
/// The verdicts, in upstream's order:
/// - arch `dflash` carrying a `markov_w1.weight` tensor → `draft-dspark`
/// - arch `dflash` without it → `draft-dflash`
/// - a `blk.<block_count - 1>.nextn.eh_proj.weight` tensor → `draft-mtp`
///
/// A separate MTP head file with no blocks of its own (gemma4-assistant,
/// `block_count = 0`) matches none of them, upstream included. So `None` means
/// "the header did not say", never "no speculator", and the caller keeps the
/// folder-derived guess rather than clearing the key.
pub fn draft_spec_type(path: &Path) -> Option<&'static str> {
    let ctx = ffi::open(path)?;
    spec_type_from_kv(&ctx)
}

fn spec_type_from_kv<S: KvSource>(s: &S) -> Option<&'static str> {
    let arch = s.string("general.architecture")?;
    if arch == "dflash" {
        // The Markov head is the whole difference between the two DFlash
        // flavours: same arch, same file name shape, different speculator.
        // DFlash2 (v0.4.0, #27816: local convolution + candidate selector) is
        // NOT a third flavour here: it keeps the arch and the `draft-dflash`
        // spec type, and llama.cpp tells it apart on its own from the
        // `dflash.selector_top_k` hparam (`is_dflash2 = selector_top_k > 0`
        // inside common/speculative.cpp), so there is nothing for the key to say.
        return Some(if s.tensor("markov_w1.weight").is_some() {
            "draft-dspark"
        } else {
            "draft-dflash"
        });
    }
    // MTP heads ride the LAST block of the file, so that is the one to probe.
    // `checked_sub` where upstream wraps: a 0-block file looks up
    // `blk.4294967295.nextn.eh_proj.weight` there and finds nothing, which is
    // the same answer this returns.
    let last = s.u32(&format!("{arch}.block_count"))?.checked_sub(1)?;
    s.tensor(&format!("blk.{last}.nextn.eh_proj.weight"))
        .map(|_| "draft-mtp")
}

// The filename cross-reference that used to live here (scan `mtps\` /
// `dflashs\`, score each drafter by shared "family" tokens with the model, feed
// the winner to the Draft row) is GONE, in v1.11.3, and is not to come back in
// that shape: it put a drafter the model profile does not load in a box that otherwise
// only reports loaded things, and the score was loose enough to offer
// `Qwen3.8-27B-UD` the head built for `Qwen3.8-27B-Uncensored-…` on `qwen3` +
// `27b` alone. The Assets card's picker already lists every drafter on disk,
// which is where choosing one belongs.

// ── Small helpers ────────────────────────────────────────────────────────

/// `262144` -> `"262,144"`. Locale-free thousands grouping for readability.
fn group_thousands(n: u64) -> String {
    let s = n.to_string();
    let bytes = s.as_bytes();
    let len = bytes.len();
    let mut out = String::with_capacity(len + len / 3);
    for (i, c) in bytes.iter().enumerate() {
        if i > 0 && (len - i).is_multiple_of(3) {
            out.push(',');
        }
        out.push(*c as char);
    }
    out
}

/// `1_349_517_312` -> `"1.26 GiB"` / `715_128_832` -> `"682 MiB"`. Sizes the VRAM
/// an `override-tensor` pin would add to the target device.
fn format_mib(bytes: u64) -> String {
    let mib = bytes as f64 / (1024.0 * 1024.0);
    if mib >= 1024.0 {
        format!("{:.2} GiB", mib / 1024.0)
    } else {
        format!("{mib:.0} MiB")
    }
}

/// A `ggml_type` (ggml/include/ggml.h) by its name, from ggml itself. This is NOT
/// the `general.file_type` LLAMA_FTYPE enum that `ftype_name` maps: the two
/// disagree on nearly every value, so crossing them would silently mislabel every
/// tensor. Without ggml-base no tensor type can have been read at all, so the
/// number is only a formality.
fn ggml_type_name(t: u32) -> String {
    ffi::type_name(t).unwrap_or_else(|| format!("ggml_type {t}"))
}

/// Whether ggml-cuda / ggml-hip can run `GET_ROWS` (the embedding lookup) on a
/// tensor of this `ggml_type` with rows `n_embd` long. Mirrors the
/// `case GGML_OP_GET_ROWS` arm of `ggml_backend_cuda_device_supports_op`
/// (`ggml/src/ggml-cuda/ggml-cuda.cu`), which whitelists exactly these and
/// returns false for everything else. Verified against v0.9.0 (the whitelist
/// is unchanged since v0.5.0); ggml-hip
/// compiles the same source. ggml-vulkan's own arm is a SUPERSET of this one
/// (IQ4_NL and MXFP4 at any width, plus NVFP4, TQ1_0 and TQ2_0), so on a Vulkan
/// device this mirror errs toward a needless warning, the safe direction.
///
/// The list grew in **b10089** (#25962, 2026-07-22): every K-quant and IQ-quant
/// gained a kernel, and the two 32-value formats (IQ4_NL, MXFP4) are accepted
/// only when the row length is a multiple of the 256-value super-block the
/// kernel iterates on (`op->src[0]->ne[0] % QK_K == 0`), hence the `n_embd`
/// argument; an unknown width (0) fails that test, which errs toward the
/// warning. Still refused: Q8_1, Q8_K, the ternary TQ1_0/TQ2_0, NVFP4, F8_E4M3 and the
/// integer/F64 types nobody stores an embedding table in.
///
/// Keep this list in sync with that switch when bumping llama.cpp, and diff it
/// against the CURRENT source rather than the bundled range: this mirror sat
/// three releases behind the kernel because every range diff started after it
/// landed. A type that GAINS a kernel upstream and is missing here only costs a
/// needless warning, but a type that is listed here WITHOUT a kernel silently
/// green-lights the pin that `embd_line` exists to prevent.
fn gpu_has_get_rows(ggml_type: u32, n_embd: u32) -> bool {
    match ggml_type {
        0  // F32
        | 1  // F16
        | 2  // Q4_0
        | 3  // Q4_1
        | 6  // Q5_0
        | 7  // Q5_1
        | 8  // Q8_0
        | 10 // Q2_K
        | 11 // Q3_K
        | 12 // Q4_K
        | 13 // Q5_K
        | 14 // Q6_K
        | 16 // IQ2_XXS
        | 17 // IQ2_XS
        | 18 // IQ3_XXS
        | 19 // IQ1_S
        | 21 // IQ3_S
        | 22 // IQ2_S
        | 23 // IQ4_XS
        | 26 // I32
        | 29 // IQ1_M
        | 30 // BF16
        | 41 // Q1_0
        | 42 // Q2_0
        => true,
        // 32-value sub-blocks: the row must tile into QK_K super-blocks.
        20 | 39 => n_embd > 0 && n_embd.is_multiple_of(256), // IQ4_NL, MXFP4
        _ => false,
    }
}

/// Generated by build.rs from the llama.cpp clone (`arch_tables.rs`).
mod arch_tables {
    include!(concat!(env!("OUT_DIR"), "/arch_tables.rs"));
}

/// The memory llama.cpp builds for a `general.architecture`: a KV cache
/// ("attention"), a fixed-size recurrent state ("recurrent"), or both
/// ("hybrid"), from `llm_arch_is_recurrent` and `llm_arch_is_hybrid`, whose arches
/// get `llama_memory_recurrent` / `llama_memory_hybrid` instead of the plain KV
/// cache: a state that cannot be rewound, which is what context checkpoints exist
/// for.
fn arch_memory(arch: &str) -> String {
    strings::t(if arch.is_empty() {
        strings::key("unknown")
    } else if arch_tables::RECURRENT.contains(&arch) {
        strings::key("memory-recurrent")
    } else if arch_tables::HYBRID.contains(&arch) {
        strings::key("memory-hybrid")
    } else {
        strings::key("memory-attention")
    })
}

/// Whether llama.cpp's tensor parallelism (`--split-mode tensor`) implements this
/// `general.architecture`, from the deny-list of `llm_arch_supports_sm_tensor`:
/// every arch not named there is allowed. `hrm_text` is not denied but forced
/// MIRRORED in `llama_meta_device_get_split_state`.
fn arch_supports_sm_tensor(arch: &str) -> bool {
    !arch_tables::NO_SM_TENSOR.contains(&arch)
}

/// Map `general.file_type` (LLAMA_FTYPE) to its quant name. Falls back to
/// `"ftype N"` for values this build predates.
fn ftype_name(ft: u32) -> String {
    let name = match ft {
        0 => "F32",
        1 => "F16",
        2 => "Q4_0",
        3 => "Q4_1",
        7 => "Q8_0",
        8 => "Q5_0",
        9 => "Q5_1",
        10 => "Q2_K",
        11 => "Q3_K_S",
        12 => "Q3_K_M",
        13 => "Q3_K_L",
        14 => "Q4_K_S",
        15 => "Q4_K_M",
        16 => "Q5_K_S",
        17 => "Q5_K_M",
        18 => "Q6_K",
        19 => "IQ2_XXS",
        20 => "IQ2_XS",
        21 => "Q2_K_S",
        22 => "IQ3_XS",
        23 => "IQ3_XXS",
        24 => "IQ1_S",
        25 => "IQ4_NL",
        26 => "IQ3_S",
        27 => "IQ3_M",
        28 => "IQ2_S",
        29 => "IQ2_M",
        30 => "IQ4_XS",
        31 => "IQ1_M",
        32 => "BF16",
        36 => "TQ1_0",
        37 => "TQ2_0",
        38 => "MXFP4_MOE",
        39 => "NVFP4",
        40 => "Q1_0",
        41 => "Q2_0",
        42 => "F8_E4M3",
        _ => return format!("ftype {ft}"),
    };
    name.to_string()
}

// ── ggml-base.dll glue ───────────────────────────────────────────────────
// The GGUF reader (runtime-loaded `ggml-base.dll` on Windows; a `None` stub
// elsewhere) is split into src/gguf/ffi.rs so this module reads as pure
// metadata logic. Public surface: `ffi::open(path) -> Option<Ctx: KvSource>`.
mod ffi;

// ── Tests ────────────────────────────────────────────────────────────────

#[cfg(test)]
#[path = "gguf_tests.rs"]
mod tests;
