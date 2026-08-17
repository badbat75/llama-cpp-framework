//! presets.ini schema and IO for llama.cpp-framework.
//!
//! ADD A PRESET FIELD. The recurring change fans out to all of these (trace an
//! existing field like `ctx-size` as the template; kebab-case INI key ↔
//! snake_case Rust field):
//!   1. `Preset` struct field (+ doc)      : below
//!   2. `impl Default for Preset`          : below
//!   3. `Preset::from_keys`                : INI read, below
//!   4. `render_section` + `emit_*` (+ `;` comment) : INI write, below
//!   5. `PresetForm` struct                : ui/types.slint (a NUMERIC field rides
//!      as a `string`, integers included, plus a paired `<field>_default: bool`,
//!      the "omit the flag" checkbox)
//!   6. the input widget                   : ui/models_page.slint, bind two-way
//!      `<=>`: DefaultLineEdit for EVERY numeric (`input_type: InputType.number`
//!      for an integer, `decimal` for a float or a signed one), wire BOTH `value`
//!      and `default`; EnumComboBox for string dropdowns. Never a SpinBox: it edits
//!      itself on a stray mouse-wheel over the page, and a test now bans it
//!      (src\tests\binding_lint.rs `no_spinbox_widgets_anywhere`).
//!   7. `preset_to_form` + `form_to_preset` : src/form.rs (BOTH directions; a
//!      numeric goes out through `itxt`/`txt` and comes back through
//!      `ini::parse_int`/`parse_float`, deriving `<field>_default` via `is_none()`
//!      one way and `if <field>_default { None } else { … }` the other)
//!   8. FREE-TEXT field only (any value the user types freely: a filesystem
//!      path, OR raw JSON like `chat-template-kwargs`): add it to
//!      `validate_for_save`'s list below AND to the
//!      `save_validation_rejects_comment_markers_in_free_text_fields` test,
//!      because the INI format can't escape `;`/`#` (legal in Windows dirs and
//!      in JSON strings), so an unvalidated value saves fine and reloads
//!      TRUNCATED. Nothing fails if you skip this.
//!
//! Guards: the INI round-trip test in this file (`full_preset_round_trips_through_ini`)
//! and the form round-trip test in form.rs (`form_to_preset(preset_to_form(p)) == p`);
//! a field wired into one side only drops out of one of them. Give the new
//! field a NON-DEFAULT value when extending the rich fixtures: `None`/empty
//! satisfies the compiler but makes the round-trips vacuous for that field.
//! Step 8 is the one step NO test catches when skipped (round-trip fixtures use
//! clean paths), same for its widget (step 6: a forgotten widget just never
//! appears in the UI).

use std::fs;
use std::io;
use std::path::PathBuf;

use crate::ini;
use crate::paths;
use crate::server_cfg;

// ── Schema: Preset + Default ─────────────────────────────────────────────

#[derive(Debug, Clone, PartialEq)]
pub struct Preset {
    pub id: String,
    pub model: String,
    pub mmproj: String,
    /// GPU-offload the multimodal projector, the mmproj/CLIP image encoder
    /// (--mmproj-offload / --no-mmproj-offload). None/true = llama.cpp's default
    /// (offloaded). Note WHICH GPU is not this flag's business and not --device's
    /// either: the CLIP context grabs the first GPU backend it finds unless
    /// `MTMD_BACKEND_DEVICE` names one (server.ini `MmprojDevice`). Turn this off
    /// to keep the encoder on CPU entirely: it only runs on image requests, so a
    /// text-mostly workload pays nothing but the VRAM it was holding.
    pub mmproj_offload: Option<bool>,
    /// Minimum / maximum tokens a single image may take on vision models with
    /// DYNAMIC resolution (--image-min-tokens / --image-max-tokens). `None` = omit
    /// the flag → llama.cpp reads the bound from the model's metadata (its own -1
    /// sentinel). Only the mmproj/CLIP encoder reads these (`clip.cpp` folds them
    /// into `custom_image_min/max_pixels`), so they do nothing without an mmproj;
    /// the widgets are gated on one being selected. Qwen-VL warns at load that it
    /// needs at least 1024 image tokens for grounding accuracy and prints
    /// `try adding --image-min-tokens 1024` (llama.cpp #16842); this is that knob.
    pub image_min_tokens: Option<i32>,
    pub image_max_tokens: Option<i32>,
    // Speculative decoding / Multi-Token Prediction (MTP) / DFlash.
    // `model_draft` is the draft GGUF (--model-draft): an MTP head, a DFlash
    // drafter, or a small standalone draft model. `spec_type` selects the
    // speculator (--spec-type, e.g. "draft-mtp" or "draft-dflash"). Empty = unset.
    // `spec_draft_n_max` (--spec-draft-n-max) caps drafted tokens per step;
    // DFlash clamps it to the trained block_size-1 (e.g. 15).
    //
    // The GUI always writes `spec_type` rather than leaving it to llama.cpp's own
    // header sniffing (b10406/b10428: `--model-draft` with no `--spec-type` reads
    // the draft GGUF and infers one). Same source, read earlier: the picker shows
    // the verdict, `gguf::draft_spec_type` is where it comes from, and an explicit
    // key still works on builds predating the auto-detect.
    //
    // `n_gpu_layers_draft` (--n-gpu-layers-draft) and `device_draft`
    // (--device-draft) place a draft FILE, and llama.cpp reads them ONLY when one
    // is set: both live inside `if (has_dft())`, i.e. `--model-draft` given. With
    // MTP heads EMBEDDED in the main GGUF (`spec_type` set, `model_draft` empty)
    // the draft context is built against the target model itself: it runs on the
    // model's own device and both keys are silently ignored. Setting them there is
    // how a GPU ends up looking "assigned" to the draft while never drafting a
    // token. A SEPARATE MTP head file (e.g. gemma4-assistant, n_layer=0) is the
    // case where they do apply, and there, pin to ONE device: the multi-device
    // "auto" split crashes those heads.
    pub model_draft: String,
    pub spec_type: String,
    pub spec_draft_n_max: Option<i32>,
    /// KV-cache quantization for the DRAFT context's K and V
    /// (--spec-draft-type-k / --spec-draft-type-v, aliases -ctkd / -ctvd).
    /// Empty = omit the flag, the same empty↔"default" shape as `cache_type_k`
    /// below, but the default they fall back to is NOT the model's.
    ///
    /// `common_base_params_to_speculative` (common/speculative.cpp) starts from
    /// `result = params`, so the draft context inherits the model's
    /// `cache_type_k`/`_v`, and then unconditionally overwrites both with
    /// `params.speculative.draft.cache_type_k/_v`, whose own default is
    /// `GGML_TYPE_F16` (common/common.h). So a preset that runs the model on a
    /// q8_0 KV cache still drafts against an **f16** one: these two flags are
    /// the only way to reach it, and "leave them alone and it follows the model"
    /// is exactly the wrong guess.
    ///
    /// Note the contrast with the two PLACEMENT keys below. That overwrite sits
    /// OUTSIDE the `if (has_draft)` block that guards `devices`, `n_gpu_layers`
    /// and `tensor_buft_overrides`, so unlike `device_draft` /
    /// `n_gpu_layers_draft` these two DO apply to MTP heads EMBEDDED in the main
    /// GGUF, the case where the placement pair is silently ignored. Hence they
    /// ride with the speculator in `prune_inactive_draft_keys`, not with the
    /// placement pair.
    pub spec_draft_type_k: String,
    pub spec_draft_type_v: String,
    pub n_gpu_layers_draft: Option<i32>,
    pub device_draft: String,
    /// The GPUs THIS model runs on (--device): one id ("ROCm1") or a comma-
    /// separated list in split order ("ROCm1,CUDA0"). Per-preset override of
    /// server.ini Device, but a server-wide Device WINS over it at launch, since
    /// llama-server's router passes its own CLI args on top of every preset.
    /// Empty = inherit the server default. Written by the GPU distribution table
    /// (src/gpu_split.rs), never typed by hand.
    pub device: String,
    /// Multi-GPU split for THIS model. `split_mode` (--split-mode): none|layer|row.
    /// `none` keeps ONLY the first `device` entry (main_gpu) and ignores the
    /// vector; `row` splits the weight matrices row-wise by the vector (backend
    /// split buffer type, CUDA/ROCm only; Vulkan falls back to the layer cut).
    /// A server-wide SplitMode OVERRIDES this key at launch (the router's CLI
    /// wins the merge), `layer` spelled out included.
    /// `tensor_split` (--tensor-split): per-GPU weight proportions like "3,1",
    /// positional over the `device` list ABOVE, in that order. Empty tensor_split
    /// with 2+ devices = llama.cpp splits by free VRAM at load (NOT evenly).
    /// Empty = inherit the server.ini default. Identical on CUDA and HIP.
    pub split_mode: String,
    pub tensor_split: String,
    /// Per-tensor placement (--override-tensor): `<regex>=<buffer type>` rules,
    /// comma-separated, e.g. `token_embd\.weight=ROCm0`. A rule sends every tensor
    /// whose NAME matches the regex to the named device instead of the one its
    /// layer landed on, which is how you move the token-embedding table off the
    /// PINNED HOST buffer llama.cpp parks it in (`ROCm_Host`/`CUDA_Host`, i.e.
    /// Windows "Shared GPU memory") even when it reports every layer offloaded.
    /// Empty = no overrides. Written by the tensor-placement table
    /// (src/tensor_override.rs), never typed by hand: llama.cpp splits the value
    /// on `,` and at the first `=` BEFORE parsing, and neither can be escaped.
    pub override_tensor: String,
    pub ctx_size: Option<i32>,
    pub n_gpu_layers: Option<i32>,
    pub parallel: Option<i32>,
    pub batch_size: Option<i32>,
    pub ubatch_size: Option<i32>,
    /// KV-cache quantization for K and V (--cache-type-k / --cache-type-v).
    /// EMPTY = omit the flag → llama.cpp's own default, f16. Empty is not a
    /// synonym for `"f16"`: the literal pins f16 forever, the empty string follows
    /// llama.cpp. Both reach the form as the word "default" (`Options.cache_types`
    /// first entry), like `split_mode`; see src/form.rs `enum_or_default`.
    pub cache_type_k: String,
    pub cache_type_v: String,
    /// The fused flash-attention kernel (--flash-attn). llama.cpp takes
    /// `[on|off|auto]` and defaults to **auto** (on where the backend supports it,
    /// off where it doesn't), so this is a TRI-state and NOT a checkbox: `None` =
    /// omit the flag (auto), `Some(true)` = force it on, `Some(false)` = force it
    /// off. The third state is the one a bool cannot reach, and it is the default.
    /// Note `Some(false)` also takes away most of the point of a quantized K/V
    /// cache, which needs the kernel to pay off on most backends.
    pub flash_attn: Option<bool>,
    /// KV-cache RAM budget in MiB (--cache-ram): `-1` = no limit, `0` disables.
    pub cache_ram: Option<i32>,
    pub jinja: Option<bool>,
    pub reasoning: String,
    pub reasoning_format: String,
    /// How hard the model should think, as a level handed to the chat template
    /// (--reasoning-effort). Empty = omit the flag, i.e. whatever the template
    /// defaults to; the documented levels are `minimal`, `low`, `medium`, `high`,
    /// `xhigh` and `max`.
    ///
    /// Same trap as `reasoning_preserve` below, one variable fewer: it is a
    /// template kwarg with a real flag in front of it, not a knob llama.cpp acts
    /// on itself. `common/arg.cpp` stores it as
    /// `default_template_kwargs["reasoning_effort"]` (the literal `default`
    /// ERASES that entry, which is why empty means omit), and
    /// `caps_apply_reasoning_effort` (common/jinja/caps.cpp) binds it to TWO
    /// template variables at once, `reasoning_effort` and `reasoning_strength`,
    /// because templates disagree on the name. Hand-writing either one into
    /// `chat_template_kwargs` therefore sets one of the two and is a silent no-op
    /// on a template keyed to the other.
    ///
    /// A template that reads neither name ignores the value entirely, and unlike
    /// --reasoning-preserve llama-server says NOTHING about it at startup: the
    /// capability probe fills `supports_reasoning_effort` but no log line reports
    /// it, so Model info's Thinking row (`gguf::Thinking::EffortOnly`) is the only
    /// place that answers "is this a lever on this model".
    ///
    /// Needs llama.cpp **b10434** or newer. On an older llama-server the key is
    /// not merely ignored: `common_preset_context::load` throws "option
    /// 'reasoning-effort' not recognized in preset '<id>'" (`ignore_unknown_keys`
    /// is true only for the shared user config.ini, never for --models-preset),
    /// so the whole server refuses to start.
    pub reasoning_effort: String,
    /// Keep the reasoning trace of EVERY assistant turn in the history replayed to
    /// the model, not just the last one (--reasoning-preserve /
    /// --no-reasoning-preserve). `None` = the template's own default: llama.cpp
    /// passes neither flag, which is why this is a tri-state and not a checkbox.
    ///
    /// This is the ONLY supported lever, and it is NOT interchangeable with putting
    /// `preserve_thinking` into `chat_template_kwargs` by hand. The flag sets the
    /// kwarg `preserve_reasoning`, and `caps_apply_preserve_reasoning`
    /// (common/jinja/caps.cpp) expands THAT into three template variables at once
    /// (`preserve_thinking = v`, `clear_thinking = !v`, `truncate_history_thinking
    /// = !v`), because templates disagree on which name they read (LFM2.5 reads
    /// preserve_thinking, GLM-4.7 reads clear_thinking, Nemotron reads
    /// truncate_history_thinking). A hand-written `preserve_thinking` kwarg sets one
    /// of the three, so on a template keyed to either other name it is a SILENT
    /// no-op; it also misses the capability probe, which is what logs "chat template
    /// supports preserving reasoning, consider enabling it via --reasoning-preserve"
    /// (supported but off) or "…does NOT support preserving reasoning" (unsupported).
    pub reasoning_preserve: Option<bool>,
    /// Token budget for the THINKING block alone (--reasoning-budget). `None` =
    /// omit the flag → llama.cpp's own default, `-1` = unrestricted. Every integer
    /// is meaningful (`0` closes the block at once, `N > 0` is a real budget), so
    /// this takes NO `> 0` filter on the form leg, like `cache_ram`.
    ///
    /// It is not a truncation: at the limit the sampler forces the template's
    /// end-of-thinking tag (`common_sampler_reasoning_budget_force`, after
    /// injecting `reasoning_budget_message`), so the model still writes a normal
    /// answer. It only arms where the template declares thinking tags, which is
    /// where the forced sequence comes from.
    ///
    /// This is the only cap on thinking, and there is NO companion flag for the
    /// answer: `n_predict` below bounds thinking + answer together, so the answer
    /// gets `n_predict` minus the thinking actually spent. Do not copy a model
    /// card's "reasoning N / final response M" pair in as-is either: those are
    /// ceilings for the context the card assumes, and a budget larger than the
    /// context never fires, leaving a runaway generation to end by filling the KV
    /// cache instead.
    pub reasoning_budget: Option<i32>,
    /// Text injected just before the forced end-of-thinking tag when
    /// `reasoning_budget` runs out (--reasoning-budget-message), to steer the
    /// model into wrapping up rather than stopping mid-thought. Empty = omit the
    /// flag: llama.cpp closes the block with no message. FREE TEXT, hence its
    /// entry in `validate_for_save`: a `;` or `#` reloads truncated.
    pub reasoning_budget_message: String,
    /// Total tokens generated per request, thinking AND answer together
    /// (--n-predict). `None` = omit the flag → llama.cpp's default `-1`, i.e.
    /// until the context is full; `-1` stays meaningful as an explicit value, so
    /// no `> 0` filter here either.
    ///
    /// A fallback DEFAULT, never a clamp: `server-context.cpp` takes the request's
    /// own `n_predict` (OpenAI `max_tokens`) whenever it is set, in either
    /// direction, and reaches for this only when the client sends none.
    pub n_predict: Option<i32>,
    pub n_cpu_moe: Option<i32>,
    pub temp: Option<f64>,
    /// Integer sampler (--top-k): backed by an int SpinBox, not the float editor
    /// the other samplers use: a decimal field would let `40,5` slip the int parse.
    pub top_k: Option<i32>,
    pub top_p: Option<f64>,
    pub min_p: Option<f64>,
    pub repeat_penalty: Option<f64>,
    pub presence_penalty: Option<f64>,
    pub chat_template_kwargs: String,
}

/// A new preset (`new_default`) leaves EVERY tunable unset, so the model runs on
/// llama.cpp's own defaults until the user overrides something on purpose; each
/// `None`/`""` below is a key `render_section` omits, which is what the GUI shows
/// as a ticked **default** box. The framework used to seed its own opinions here
/// (32k context, 4 slots, 512-token batches, a q8_0 KV cache, forced flash-attn),
/// which read like llama.cpp's defaults but were not, and `parallel = 4` in
/// particular is not inert: pinning it turns OFF the unified KV cache that
/// llama-server's auto mode enables, quartering the context a single request may
/// use (see `integrations::effective_ctx`).
///
/// The four values that are not `None` are the ones with nothing to omit: the form
/// binds `jinja` and `mmproj-offload` to plain checkboxes and `reasoning` /
/// `reasoning-format` to dropdowns with no "unset" entry, so the key is always
/// written, and each is set to exactly what llama.cpp would have done anyway
/// (`use_jinja = true` for the server, mmproj offloaded, both reasoning knobs on
/// `auto`), which keeps writing them a no-op.
impl Default for Preset {
    fn default() -> Self {
        Self {
            id: String::new(),
            model: String::new(),
            mmproj: String::new(),
            mmproj_offload: Some(true),
            image_min_tokens: None,
            image_max_tokens: None,
            model_draft: String::new(),
            spec_type: String::new(),
            spec_draft_n_max: None,
            spec_draft_type_k: String::new(),
            spec_draft_type_v: String::new(),
            n_gpu_layers_draft: None,
            device_draft: String::new(),
            device: String::new(),
            split_mode: String::new(),
            tensor_split: String::new(),
            override_tensor: String::new(),
            ctx_size: None,
            n_gpu_layers: None,
            parallel: None,
            batch_size: None,
            ubatch_size: None,
            cache_type_k: String::new(),
            cache_type_v: String::new(),
            flash_attn: None,
            cache_ram: None,
            jinja: Some(true),
            reasoning: "auto".into(),
            reasoning_format: "auto".into(),
            reasoning_effort: String::new(),
            reasoning_preserve: None,
            reasoning_budget: None,
            reasoning_budget_message: String::new(),
            n_predict: None,
            n_cpu_moe: None,
            temp: None,
            top_k: None,
            top_p: None,
            min_p: None,
            repeat_penalty: None,
            presence_penalty: None,
            chat_template_kwargs: String::new(),
        }
    }
}

// ── Construct & parse (from_keys = INI read) ─────────────────────────────

impl Preset {
    pub fn new_default(id: String, model: String) -> Self {
        Self {
            id,
            model,
            ..Default::default()
        }
    }

    fn from_keys(id: &str, k: &std::collections::BTreeMap<String, String>) -> Self {
        let get = |key: &str| k.get(key).cloned().unwrap_or_default();
        let getb = |key: &str| k.get(key).and_then(|v| ini::parse_bool(v));
        Self {
            id: id.to_string(),
            model: get("model"),
            mmproj: get("mmproj"),
            mmproj_offload: getb("mmproj-offload"),
            image_min_tokens: k.get("image-min-tokens").and_then(|v| ini::parse_int(v)),
            image_max_tokens: k.get("image-max-tokens").and_then(|v| ini::parse_int(v)),
            model_draft: get("model-draft"),
            spec_type: get("spec-type"),
            spec_draft_n_max: k.get("spec-draft-n-max").and_then(|v| ini::parse_int(v)),
            spec_draft_type_k: get("spec-draft-type-k"),
            spec_draft_type_v: get("spec-draft-type-v"),
            n_gpu_layers_draft: k.get("n-gpu-layers-draft").and_then(|v| ini::parse_int(v)),
            device_draft: get("device-draft"),
            device: get("device"),
            split_mode: get("split-mode"),
            tensor_split: get("tensor-split"),
            override_tensor: get("override-tensor"),
            ctx_size: k.get("ctx-size").and_then(|v| ini::parse_int(v)),
            n_gpu_layers: k.get("n-gpu-layers").and_then(|v| ini::parse_int(v)),
            parallel: k.get("parallel").and_then(|v| ini::parse_int(v)),
            batch_size: k.get("batch-size").and_then(|v| ini::parse_int(v)),
            ubatch_size: k.get("ubatch-size").and_then(|v| ini::parse_int(v)),
            cache_type_k: get("cache-type-k"),
            cache_type_v: get("cache-type-v"),
            flash_attn: getb("flash-attn"),
            cache_ram: k.get("cache-ram").and_then(|v| ini::parse_int(v)),
            jinja: getb("jinja"),
            reasoning: get("reasoning"),
            reasoning_format: get("reasoning-format"),
            reasoning_effort: get("reasoning-effort"),
            reasoning_preserve: getb("reasoning-preserve"),
            reasoning_budget: k.get("reasoning-budget").and_then(|v| ini::parse_int(v)),
            reasoning_budget_message: get("reasoning-budget-message"),
            n_predict: k.get("n-predict").and_then(|v| ini::parse_int(v)),
            n_cpu_moe: k.get("n-cpu-moe").and_then(|v| ini::parse_int(v)),
            temp: k.get("temp").and_then(|v| ini::parse_float(v)),
            top_k: k.get("top-k").and_then(|v| ini::parse_int(v)),
            top_p: k.get("top-p").and_then(|v| ini::parse_float(v)),
            min_p: k.get("min-p").and_then(|v| ini::parse_float(v)),
            repeat_penalty: k.get("repeat-penalty").and_then(|v| ini::parse_float(v)),
            presence_penalty: k.get("presence-penalty").and_then(|v| ini::parse_float(v)),
            chat_template_kwargs: get("chat-template-kwargs"),
        }
    }
}

// ── Dead speculative keys (prune at the write boundary) ──────────────────

/// Drop the speculative-decoding keys this model cannot act on, returning the
/// INI names of the ones removed (for the caller's status line).
///
/// `embeds_mtp` is the model's own `<arch>.nextn_predict_layers > 0`, which the
/// caller must have actually READ (`gguf::read_model_info`). An unreadable GGUF
/// is not evidence of absence, so callers SKIP the prune there rather than pass
/// `false`: deleting a working `spec-type` because ggml-base.dll went missing
/// would be the worse failure.
///
/// Two dead sets, only the second of which announces itself:
/// - Without a draft FILE, `--device-draft` / `--n-gpu-layers-draft` are read
///   inside llama.cpp's `if (has_dft())` and silently ignored (field docs above).
/// - Without a draft file AND without embedded MTP heads there is nothing to
///   draft with at all, so `--spec-type` / `--spec-draft-n-max` and the draft
///   KV types (`--spec-draft-type-k/-v`, which DO survive embedded MTP) go too. A
///   surviving `spec-type = draft-mtp` is NOT inert: llama-server builds an MTP
///   draft context against the target model, `llama_init_from_model` rejects it
///   ("context type MTP requested but model doesn't contain MTP layers") and the
///   model fails to load entirely.
///
/// Why this is a write-boundary chore and not something the UI already prevents:
/// the widgets are `enabled: draft_active`, i.e. DISABLED, not cleared. A value
/// that arrives from somewhere other than those widgets stays in the form,
/// greyed out and unreadable, and is written back on the next save. Two ways in,
/// both ordinary: cloning a preset copies every field of its base onto a
/// different model, and re-pointing a preset at another model keeps the old
/// one's keys.
pub fn prune_inactive_draft_keys(p: &mut Preset, embeds_mtp: bool) -> Vec<&'static str> {
    let mut dropped = Vec::new();
    // A draft file makes all four live; nothing to prune.
    if !p.model_draft.is_empty() {
        return dropped;
    }
    if !embeds_mtp {
        if !p.spec_type.is_empty() {
            p.spec_type.clear();
            dropped.push("spec-type");
        }
        if p.spec_draft_n_max.take().is_some() {
            dropped.push("spec-draft-n-max");
        }
        // The draft KV types belong to THIS group, not the placement one below:
        // llama.cpp applies them whenever a draft context exists, embedded MTP
        // heads included (see the field docs), so they only die when there is no
        // speculation at all.
        if !p.spec_draft_type_k.is_empty() {
            p.spec_draft_type_k.clear();
            dropped.push("spec-draft-type-k");
        }
        if !p.spec_draft_type_v.is_empty() {
            p.spec_draft_type_v.clear();
            dropped.push("spec-draft-type-v");
        }
    }
    if p.n_gpu_layers_draft.take().is_some() {
        dropped.push("n-gpu-layers-draft");
    }
    if !p.device_draft.is_empty() {
        p.device_draft.clear();
        dropped.push("device-draft");
    }
    dropped
}

// ── File IO (load / save / delete / rename / id) ─────────────────────────

pub fn load_all() -> Vec<Preset> {
    let path = paths::presets_ini();
    ini::read_all(&path)
        .into_iter()
        .map(|s| Preset::from_keys(&s.id, &s.keys))
        .collect()
}

/// Write (replace) the preset's section in presets.ini.
///
/// Side effect: on the FIRST save, when server.ini has no `ModelsDir` yet, this
/// also seeds it, inferred from the model's path (its `models\` grandparent),
/// so the file pickers have a root to scan without a separate setup step. The
/// seeding error is intentionally ignored: a preset save must still succeed even
/// if server.ini can't be touched.
pub fn save(preset: &Preset) -> io::Result<()> {
    validate_for_save(preset)?;
    let path = paths::presets_ini();
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    let body = render_section(preset);
    ini::replace_section(&path, &preset.id, &body)?;
    // `load()` already normalizes blank to None (opt_nonblank in from_keys).
    if server_cfg::load().models_dir.is_none() {
        if let Some(models_dir) = infer_models_dir(&preset.model) {
            let _ = ini::replace_key(&paths::server_ini(), "Server", "ModelsDir", &models_dir);
        }
    }
    Ok(())
}

/// True if `id` uses only the presets.ini section-header charset (letters,
/// digits, `.`, `-`, `_`). `[`/`]`/newline break the section structure; `;`/`#`
/// get misread as an inline comment (here and by llama-server's preset reader).
/// Enforced at BOTH free-text ways into a header (`rename` and the save
/// boundary), so a hand-authored id (a future `preset set`/import, or an
/// editable-id GUI change) can't corrupt the file. Emptiness is a separate
/// check so `rename` can keep its own "…is empty" message.
fn valid_id(id: &str) -> bool {
    id.chars()
        .all(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '-' | '_'))
}

/// Save-boundary validation, pure so the unit test never touches `paths::`:
/// the id becomes a section header and the free-text fields must survive the
/// INI comment rule: a `;`/`#` in a GGUF path OR in the raw-JSON
/// `chat-template-kwargs` would silently reload truncated (here AND in
/// llama-server's own preset reader), so refuse it with the field name (the
/// cure is renaming the file / fixing the JSON). See `ini::reject_comment_markers`.
///
/// `override-tensor` gets a SECOND check on top of that one: its own grammar.
/// llama.cpp splits the value on `,` and each rule at its first `=` before it
/// parses anything, and a rule that comes out of that without a device is a
/// `throw` during arg parsing; the model simply never loads, and the reason is
/// buried in a child process's log. The table can't produce one; a hand-edited
/// INI can, which is exactly why the check lives at the save boundary too.
fn validate_for_save(preset: &Preset) -> io::Result<()> {
    if preset.id.is_empty() || !valid_id(&preset.id) {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            format!(
                "invalid preset id `{}`: use letters, digits, '.', '-', '_'",
                preset.id
            ),
        ));
    }
    for (field, value) in [
        ("model", &preset.model),
        ("mmproj", &preset.mmproj),
        ("model-draft", &preset.model_draft),
        ("chat-template-kwargs", &preset.chat_template_kwargs),
        ("override-tensor", &preset.override_tensor),
        ("reasoning-budget-message", &preset.reasoning_budget_message),
    ] {
        ini::reject_comment_markers(field, value)?;
    }
    crate::tensor_override::validate(&preset.override_tensor)
        .map_err(|e| io::Error::new(io::ErrorKind::InvalidInput, e))?;
    Ok(())
}

pub fn delete(id: &str) -> io::Result<()> {
    let path = paths::presets_ini();
    ini::delete_section(&path, id)
}

pub fn rename(old_id: &str, new_id: &str) -> io::Result<()> {
    let new = new_id.trim();
    if new.is_empty() {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "new preset id is empty",
        ));
    }
    if new == old_id {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "new preset id is unchanged",
        ));
    }
    // The rename dialog is one free-text way into a section header (the save
    // boundary is the other; see `valid_id`). Hold it to the same charset:
    // `[`/`]`/newline would corrupt the section structure, `;`/`#` gets misread
    // as a comment (here and by llama-server alike).
    if !valid_id(new) {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "preset ids may only use letters, digits, '.', '-' and '_'",
        ));
    }
    let path = paths::presets_ini();
    ini::rename_section(&path, old_id, new)
}

/// First of `base`, `base-2`, `base-3`, … that isn't already in `existing`.
/// De-conflicts an id derived by `make_id`: Clone must never overwrite an
/// existing preset when the picked model already has one.
pub(crate) fn unique_id(base: &str, existing: &[String]) -> String {
    if !existing.iter().any(|e| e == base) {
        return base.to_string();
    }
    (2..)
        .map(|n| format!("{base}-{n}"))
        .find(|cand| !existing.iter().any(|e| e == cand))
        .expect("(2..) is unbounded, so find always yields")
}

pub fn make_id(model_path: &str) -> String {
    let stem = std::path::Path::new(model_path)
        .file_stem()
        .map(|s| s.to_string_lossy().to_string())
        .unwrap_or_default();
    let stem = strip_shard_suffix(&stem);
    let mut out = String::with_capacity(stem.len());
    let mut prev_underscore = false;
    for c in stem.chars() {
        let keep = c.is_ascii_alphanumeric() || c == '.' || c == '-' || c == '_';
        if keep {
            out.push(c);
            prev_underscore = false;
        } else if !prev_underscore {
            out.push('_');
            prev_underscore = true;
        }
    }
    out.trim_matches('_').to_string()
}

fn strip_shard_suffix(stem: &str) -> String {
    match crate::model_scan::split_shard_suffix(stem) {
        Some((base, _)) => base.to_string(),
        None => stem.to_string(),
    }
}

fn infer_models_dir(model_path: &str) -> Option<String> {
    let p = PathBuf::from(model_path);
    let parent = p.parent()?;
    // Models are scanned from <ModelsDir>/models/, so when the file sits
    // directly in a `models` subdir the root is its grandparent. Otherwise
    // fall back to the file's own parent.
    let root = if parent
        .file_name()
        .and_then(|n| n.to_str())
        .is_some_and(|n| n.eq_ignore_ascii_case("models"))
    {
        parent.parent().unwrap_or(parent)
    } else {
        parent
    };
    Some(root.to_string_lossy().into_owned())
}

// ── INI write (render_section + emit_*) ──────────────────────────────────

pub fn render_section(p: &Preset) -> String {
    let mut out = String::new();
    out.push_str(&format!("[{}]\r\n", p.id));
    out.push_str("; Generated by llama-cpp-config.\r\n");
    out.push_str("; Saving this preset in llama-cpp-config rewrites this whole section;\r\n");
    out.push_str("; hand-edits to OTHER sections in this file are preserved.\r\n\r\n");

    out.push_str("; Model: local path (-m).\r\n");
    out.push_str(&format!("model = {}\r\n", p.model.trim()));
    out.push_str("\r\n; Sub-model paths\r\n");
    emit_str(&mut out, "mmproj", &p.mmproj);
    out.push_str(
        "; mmproj-offload = false keeps the image encoder on CPU. It is NOT placed by\r\n",
    );
    out.push_str("; `device`: llama.cpp puts the encoder on the first GPU backend it finds\r\n");
    out.push_str("; unless server.ini MmprojDevice (env MTMD_BACKEND_DEVICE) names one.\r\n");
    emit_bool(&mut out, "mmproj-offload", p.mmproj_offload);
    out.push_str(
        "; image-min-tokens / image-max-tokens bound the tokens ONE image takes on\r\n",
    );
    out.push_str("; DYNAMIC-resolution vision models (Qwen-VL wants >= 1024 for grounding\r\n");
    out.push_str("; accuracy). Omit = read the bound from the model; needs an mmproj.\r\n");
    emit_i32(&mut out, "image-min-tokens", p.image_min_tokens);
    emit_i32(&mut out, "image-max-tokens", p.image_max_tokens);
    emit_str(&mut out, "model-draft", &p.model_draft);

    out.push_str("\r\n; Speculative decoding / Multi-Token Prediction / DFlash\r\n");
    out.push_str("; spec-type pairs model-draft with a speculator: draft-mtp (MTP head),\r\n");
    out.push_str("; draft-dflash / draft-dspark (block-diffusion drafters; dspark is the\r\n");
    out.push_str("; one whose GGUF carries a Markov head), or draft-simple.\r\n");
    out.push_str(
        "; MTP heads embedded in the main GGUF need spec-type ALONE (no model-draft).\r\n",
    );
    emit_str(&mut out, "spec-type", &p.spec_type);
    out.push_str("; spec-draft-n-max = max drafted tokens per step. DFlash clamps this to the\r\n");
    out.push_str(
        "; model's trained block_size - 1 (e.g. 15); also applies to draft-mtp/simple.\r\n",
    );
    emit_i32(&mut out, "spec-draft-n-max", p.spec_draft_n_max);
    out.push_str("; spec-draft-type-k / -v quantize the DRAFT context's KV cache (-ctkd /\r\n");
    out.push_str("; -ctvd). Omitting them is NOT 'follow the model': llama.cpp copies\r\n");
    out.push_str("; cache-type-k/-v into the draft params and then overwrites both with the\r\n");
    out.push_str("; draft's own default, f16. Unlike the two placement keys below, these DO\r\n");
    out.push_str("; apply to MTP heads embedded in the main GGUF.\r\n");
    emit_str(&mut out, "spec-draft-type-k", &p.spec_draft_type_k);
    emit_str(&mut out, "spec-draft-type-v", &p.spec_draft_type_v);
    out.push_str("; The next two place a SEPARATE draft file and are ignored without\r\n");
    out.push_str("; model-draft: with EMBEDDED MTP heads the draft context is built against\r\n");
    out.push_str("; the target model and runs on the model's own device. With a separate MTP\r\n");
    out.push_str("; head file (e.g. gemma4-assistant, n_layer=0) pin it to ONE device with\r\n");
    out.push_str("; n-gpu-layers-draft = 99, or 0 to fall back to CPU: the multi-device auto\r\n");
    out.push_str("; split crashes those heads.\r\n");
    emit_i32(&mut out, "n-gpu-layers-draft", p.n_gpu_layers_draft);
    emit_str(&mut out, "device-draft", &p.device_draft);

    out.push_str("\r\n; Resource / context\r\n");
    emit_i32(&mut out, "ctx-size", p.ctx_size);
    emit_i32(&mut out, "n-gpu-layers", p.n_gpu_layers);
    out.push_str("; GPU distribution for this model (overrides server.ini, but a server-wide\r\n");
    out.push_str("; Device wins at launch; same on CUDA and HIP). device = the GPUs it runs\r\n");
    out.push_str("; on, e.g. ROCm1 or ROCm1,CUDA0. tensor-split = how much each one holds,\r\n");
    out.push_str("; e.g. 3,1 (positional over `device`, IN THAT ORDER). Blank tensor-split\r\n");
    out.push_str("; with 2+ devices = llama.cpp splits by free VRAM (not evenly).\r\n");
    out.push_str("; split-mode = none|layer|row. Blank = layer, or the server-wide mode, which\r\n");
    out.push_str("; also OVERRIDES this key whenever it is set (the router's CLI wins the merge).\r\n");
    out.push_str("; none = only the first device runs (tensor-split is ignored); row = weight\r\n");
    out.push_str("; matrices split row-wise by the vector (CUDA/ROCm only).\r\n");
    emit_str(&mut out, "device", &p.device);
    emit_str(&mut out, "split-mode", &p.split_mode);
    emit_str(&mut out, "tensor-split", &p.tensor_split);
    out.push_str(
        "; override-tensor sends the tensors whose NAME matches a regex to a device of\r\n",
    );
    out.push_str(
        "; their own, whatever `device` says: `<regex>=<device|CPU>`, comma-separated.\r\n",
    );
    out.push_str("; llama.cpp keeps token_embd.weight in PINNED HOST memory even when every\r\n");
    out.push_str(
        "; layer is offloaded (that is the ROCm_Host/CUDA_Host buffer in the log, and\r\n",
    );
    out.push_str(
        "; the \"Shared GPU memory\" Windows reports); token_embd\\.weight=ROCm0 moves it\r\n",
    );
    out.push_str("; onto the GPU. No escaping exists: a pattern cannot contain `,` or `=`.\r\n");
    emit_str(&mut out, "override-tensor", &p.override_tensor);
    emit_i32(&mut out, "parallel", p.parallel);
    emit_i32(&mut out, "batch-size", p.batch_size);
    emit_i32(&mut out, "ubatch-size", p.ubatch_size);

    out.push_str("\r\n; KV cache\r\n");
    out.push_str("; Omit cache-type-k/-v to get llama.cpp's own default (f16) rather than\r\n");
    out.push_str(
        "; pinning a type. flash-attn is [on|off|auto] and DEFAULTS TO AUTO (on where\r\n",
    );
    out.push_str("; the backend supports it), so omit the key for auto; flash-attn = false\r\n");
    out.push_str("; forces the kernel off, which also costs a quantized K/V cache most of its\r\n");
    out.push_str("; benefit.\r\n");
    emit_str(&mut out, "cache-type-k", &p.cache_type_k);
    emit_str(&mut out, "cache-type-v", &p.cache_type_v);
    emit_bool(&mut out, "flash-attn", p.flash_attn);

    out.push_str("\r\n; Prompt cache RAM limit in MiB (--cache-ram)\r\n");
    emit_i32(&mut out, "cache-ram", p.cache_ram);

    out.push_str("\r\n; Chat template\r\n");
    emit_bool(&mut out, "jinja", p.jinja);

    out.push_str("\r\n; Reasoning / thinking\r\n");
    emit_str(&mut out, "reasoning", &p.reasoning);
    emit_str(&mut out, "reasoning-format", &p.reasoning_format);
    out.push_str("; reasoning-effort hands the chat template a level (minimal, low, medium,\r\n");
    out.push_str("; high, xhigh, max). Omit the key = the template's own default. It is a\r\n");
    out.push_str("; template kwarg with a flag in front: llama.cpp binds it to BOTH\r\n");
    out.push_str("; reasoning_effort and reasoning_strength, so a template reading neither\r\n");
    out.push_str("; name ignores it. Needs llama.cpp b10434+; an older server refuses to\r\n");
    out.push_str("; start on the unrecognized key rather than skipping it.\r\n");
    emit_str(&mut out, "reasoning-effort", &p.reasoning_effort);
    out.push_str("; reasoning-preserve keeps the thinking of EVERY past turn in the replayed\r\n");
    out.push_str(
        "; history, not just the last one. Omit the key = the template's own default.\r\n",
    );
    out.push_str(
        "; Do NOT hand-write preserve_thinking into chat-template-kwargs instead: this\r\n",
    );
    out.push_str("; flag sets preserve_thinking, clear_thinking AND truncate_history_thinking\r\n");
    out.push_str("; together, and templates disagree on which of the three they read.\r\n");
    emit_bool(&mut out, "reasoning-preserve", p.reasoning_preserve);
    out.push_str("; reasoning-budget caps the THINKING block alone, in tokens: -1 unrestricted,\r\n");
    out.push_str("; 0 closes it at once, N > 0 is a budget. At the limit the sampler FORCES the\r\n");
    out.push_str("; template's end-of-thinking tag (after reasoning-budget-message, when set),\r\n");
    out.push_str("; so the answer is still written normally rather than truncated. Nothing caps\r\n");
    out.push_str("; the ANSWER on its own: n-predict below bounds thinking + answer together, so\r\n");
    out.push_str("; a budget bigger than the context never fires at all.\r\n");
    emit_i32(&mut out, "reasoning-budget", p.reasoning_budget);
    emit_str(
        &mut out,
        "reasoning-budget-message",
        &p.reasoning_budget_message,
    );
    out.push_str("; n-predict is the TOTAL generated per request and only a DEFAULT: the\r\n");
    out.push_str("; request's own max_tokens wins whenever the client sends one.\r\n");
    emit_i32(&mut out, "n-predict", p.n_predict);

    out.push_str("\r\n; MoE\r\n");
    emit_i32(&mut out, "n-cpu-moe", p.n_cpu_moe);

    out.push_str("\r\n; Sampling overrides\r\n");
    emit_f64(&mut out, "temp", p.temp);
    emit_i32(&mut out, "top-k", p.top_k);
    emit_f64(&mut out, "top-p", p.top_p);
    emit_f64(&mut out, "min-p", p.min_p);
    emit_f64(&mut out, "repeat-penalty", p.repeat_penalty);
    emit_f64(&mut out, "presence-penalty", p.presence_penalty);

    out.push_str("\r\n; Chat template kwargs\r\n");
    emit_str(&mut out, "chat-template-kwargs", &p.chat_template_kwargs);

    out
}

fn emit_str(out: &mut String, key: &str, val: &str) {
    // Write trimmed: the reader (ini::read_all) trims values on parse, so
    // emitting padding would break the round-trip identity (in-memory preset
    // != reloaded preset) for e.g. a path pasted with a trailing space.
    let val = val.trim();
    if !val.is_empty() {
        out.push_str(&format!("{key} = {val}\r\n"));
    }
}

fn emit_bool(out: &mut String, key: &str, val: Option<bool>) {
    if let Some(v) = val {
        out.push_str(&format!("{key} = {}\r\n", if v { "true" } else { "false" }));
    }
}

fn emit_f64(out: &mut String, key: &str, val: Option<f64>) {
    if let Some(v) = val {
        out.push_str(&format!("{key} = {v}\r\n"));
    }
}

fn emit_i32(out: &mut String, key: &str, val: Option<i32>) {
    if let Some(v) = val {
        out.push_str(&format!("{key} = {v}\r\n"));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeMap;

    // All six speculative keys set, with no draft FILE: the shape a Clone (or a
    // re-pointed model) leaves behind on a model that can't use them.
    fn preset_with_dead_draft_keys() -> Preset {
        Preset {
            model_draft: String::new(),
            spec_type: "draft-mtp".into(),
            spec_draft_n_max: Some(2),
            spec_draft_type_k: "q8_0".into(),
            spec_draft_type_v: "q8_0".into(),
            n_gpu_layers_draft: Some(99),
            device_draft: "CUDA0".into(),
            ..Default::default()
        }
    }

    // No draft file and no embedded heads: all six go, and the reported list is
    // the INI key names in file order.
    #[test]
    fn prune_drops_every_speculative_key_without_a_draft() {
        let mut p = preset_with_dead_draft_keys();
        let dropped = prune_inactive_draft_keys(&mut p, false);
        assert_eq!(
            dropped,
            vec![
                "spec-type",
                "spec-draft-n-max",
                "spec-draft-type-k",
                "spec-draft-type-v",
                "n-gpu-layers-draft",
                "device-draft"
            ]
        );
        assert_eq!(p.spec_type, "");
        assert_eq!(p.spec_draft_n_max, None);
        assert_eq!(p.spec_draft_type_k, "");
        assert_eq!(p.spec_draft_type_v, "");
        assert_eq!(p.n_gpu_layers_draft, None);
        assert_eq!(p.device_draft, "");
    }

    // Embedded MTP heads: the speculator stays (it is the whole point), and so do
    // the draft KV types; llama.cpp applies those to any draft context. Only the
    // two PLACEMENT keys die: those it reads inside `if (has_dft())`, so they'd
    // pin a draft that runs on the model's device.
    #[test]
    fn prune_keeps_the_speculator_for_embedded_mtp_heads() {
        let mut p = preset_with_dead_draft_keys();
        let dropped = prune_inactive_draft_keys(&mut p, true);
        assert_eq!(dropped, vec!["n-gpu-layers-draft", "device-draft"]);
        assert_eq!(p.spec_type, "draft-mtp");
        assert_eq!(p.spec_draft_n_max, Some(2));
        assert_eq!(p.spec_draft_type_k, "q8_0");
        assert_eq!(p.spec_draft_type_v, "q8_0");
        assert_eq!(p.n_gpu_layers_draft, None);
        assert_eq!(p.device_draft, "");
    }

    // A draft FILE makes all four live, on any model; prune must not touch them.
    #[test]
    fn prune_leaves_a_preset_with_a_draft_file_alone() {
        let mut p = Preset {
            model_draft: r"E:\mtps\head.gguf".into(),
            ..preset_with_dead_draft_keys()
        };
        let before = p.clone();
        assert!(prune_inactive_draft_keys(&mut p, false).is_empty());
        assert_eq!(p, before);
    }

    // Nothing set = nothing dropped: the status line must stay silent on the
    // overwhelmingly common preset, not announce a no-op on every save.
    #[test]
    fn prune_reports_nothing_on_a_clean_preset() {
        let mut p = Preset::default();
        assert!(prune_inactive_draft_keys(&mut p, false).is_empty());
        assert_eq!(p, Preset::default());
    }

    // Validation only: both shapes must reject BEFORE any file IO (so this
    // never touches paths::, per the src/tests/mod.rs warning).
    #[test]
    fn rename_rejects_blank_and_unchanged_ids() {
        assert!(rename("old", "  ").is_err(), "blank new id");
        assert!(rename("old", "old").is_err(), "unchanged id");
        assert!(rename("old", " old ").is_err(), "unchanged after trim");
    }

    // Free-text rename ids must stay inside make_id's charset: `[`/`]`/CR/LF
    // would corrupt the INI section structure, `;`/`#`/`=` get misparsed.
    // All rejected before any IO (per the src/tests/mod.rs warning).
    #[test]
    fn rename_rejects_hostile_ids() {
        for hostile in ["a;b", "a#b", "a[b", "a]b", "a=b", "a b", "a\nb"] {
            assert!(rename("old", hostile).is_err(), "must reject {hostile:?}");
        }
    }

    // Pure validation (no IO): a `;`/`#` in ANY free-text field (a path, or the
    // raw-JSON chat-template-kwargs) would silently reload truncated through the
    // INI comment rule, so save must refuse it.
    #[test]
    fn save_validation_rejects_comment_markers_in_free_text_fields() {
        let clean = Preset {
            id: "m".into(),
            model: r"C:\models\m.gguf".into(),
            mmproj: r"C:\models\m-mmproj.gguf".into(),
            model_draft: r"C:\models\mtps\m-mtp.gguf".into(),
            chat_template_kwargs: r#"{"enable_thinking":true}"#.into(),
            override_tensor: r"token_embd\.weight=ROCm0".into(),
            reasoning_budget_message: "Budget reached, write the final answer now.".into(),
            ..Default::default()
        };
        assert!(validate_for_save(&clean).is_ok());

        for (field, hostile) in [
            ("model", r"C:\Models #1\m.gguf"),
            ("mmproj", r"C:\a;b\m-mmproj.gguf"),
            ("model-draft", r"C:\models\m #mtp.gguf"),
            // Legal inside a JSON string, fatal to the INI reader.
            ("chat-template-kwargs", r##"{"tag":"#think"}"##),
            // Legal inside a regex (a character class), fatal to the INI reader:
            // the value reloads truncated at the `#`, silently losing the rule.
            ("override-tensor", r"blk\.[0-9#]+\.attn=CPU"),
            // Ordinary English prose reaches the model here, and prose is exactly
            // where a `;` turns up unprompted.
            (
                "reasoning-budget-message",
                "Stop now; write the final answer.",
            ),
        ] {
            let mut p = clean.clone();
            match field {
                "model" => p.model = hostile.into(),
                "mmproj" => p.mmproj = hostile.into(),
                "chat-template-kwargs" => p.chat_template_kwargs = hostile.into(),
                "override-tensor" => p.override_tensor = hostile.into(),
                "reasoning-budget-message" => p.reasoning_budget_message = hostile.into(),
                _ => p.model_draft = hostile.into(),
            }
            let err = validate_for_save(&p).expect_err(field);
            assert!(err.to_string().contains(field), "error names the field");
        }
    }

    // The id becomes a `[section]` header: reject the empty (`[]`) and the
    // structure-breaking charset at the save boundary, not only in `rename`.
    #[test]
    fn save_validation_rejects_bad_ids() {
        assert!(valid_id("qwen3-coder.q8_0"));
        assert!(!valid_id("has space"));
        assert!(!valid_id("a;b"));
        assert!(!valid_id("a]b"));

        let base = Preset {
            model: r"C:\models\m.gguf".into(),
            ..Default::default()
        };
        for bad in ["", "has space", "a;b", "sec]tion"] {
            let p = Preset {
                id: bad.into(),
                ..base.clone()
            };
            assert!(
                validate_for_save(&p).is_err(),
                "id {bad:?} must be rejected"
            );
        }
    }

    #[test]
    fn render_emits_mtp_keys_when_set() {
        let p = Preset {
            id: "m".into(),
            model: r"C:\models\m.gguf".into(),
            model_draft: r"C:\dflash\m-dflash.gguf".into(),
            spec_type: "draft-dflash".into(),
            spec_draft_n_max: Some(15),
            spec_draft_type_k: "q8_0".into(),
            spec_draft_type_v: "q5_0".into(),
            n_gpu_layers_draft: Some(99),
            device_draft: "CUDA0".into(),
            device: "CUDA0".into(),
            ..Default::default()
        };
        let ini = render_section(&p);
        assert!(ini.contains("model-draft = C:\\dflash\\m-dflash.gguf\r\n"));
        assert!(ini.contains("spec-type = draft-dflash\r\n"));
        assert!(ini.contains("spec-draft-n-max = 15\r\n"));
        assert!(ini.contains("spec-draft-type-k = q8_0\r\n"));
        assert!(ini.contains("spec-draft-type-v = q5_0\r\n"));
        assert!(ini.contains("n-gpu-layers-draft = 99\r\n"));
        assert!(ini.contains("device-draft = CUDA0\r\n"));
        assert!(ini.contains("device = CUDA0\r\n"));
    }

    #[test]
    fn render_omits_mtp_keys_when_empty() {
        let p = Preset {
            id: "m".into(),
            model: r"C:\models\m.gguf".into(),
            ..Default::default()
        };
        let ini = render_section(&p);
        // Only value lines count: the section carries a `; spec-type = …` help
        // comment that must not be mistaken for an emitted key.
        let value_lines: Vec<&str> = ini
            .lines()
            .filter(|l| !l.trim_start().starts_with(';'))
            .collect();
        assert!(!value_lines.iter().any(|l| l.starts_with("model-draft =")));
        assert!(!value_lines.iter().any(|l| l.starts_with("spec-type =")));
        assert!(!value_lines
            .iter()
            .any(|l| l.starts_with("spec-draft-n-max =")));
        assert!(!value_lines
            .iter()
            .any(|l| l.starts_with("spec-draft-type-k =")));
        assert!(!value_lines
            .iter()
            .any(|l| l.starts_with("spec-draft-type-v =")));
    }

    // Key names are pinned here; the parse-back is covered by the full round-trip
    // below (which populates split_mode/tensor_split), so no hand-rolled reparse.
    #[test]
    fn render_emits_split_keys_when_set() {
        let original = Preset {
            id: "split".into(),
            model: r"E:\m\model.gguf".into(),
            split_mode: "row".into(),
            tensor_split: "3,1".into(),
            ..Default::default()
        };
        let ini = render_section(&original);
        assert!(ini.contains("split-mode = row\r\n"));
        assert!(ini.contains("tensor-split = 3,1\r\n"));
    }

    #[test]
    fn render_omits_split_keys_when_empty() {
        let p = Preset {
            id: "m".into(),
            model: r"C:\models\m.gguf".into(),
            ..Default::default()
        };
        let ini = render_section(&p);
        let value_lines: Vec<&str> = ini
            .lines()
            .filter(|l| !l.trim_start().starts_with(';'))
            .collect();
        assert!(!value_lines.iter().any(|l| l.starts_with("split-mode =")));
        assert!(!value_lines.iter().any(|l| l.starts_with("tensor-split =")));
    }

    #[test]
    fn from_keys_parses_mtp_keys() {
        let mut k: BTreeMap<String, String> = BTreeMap::new();
        k.insert("model".into(), r"C:\models\m.gguf".into());
        k.insert("model-draft".into(), r"C:\dflash\m-dflash.gguf".into());
        k.insert("spec-type".into(), "draft-dflash".into());
        k.insert("spec-draft-n-max".into(), "15".into());
        k.insert("spec-draft-type-k".into(), "q8_0".into());
        k.insert("spec-draft-type-v".into(), "q5_0".into());
        let p = Preset::from_keys("m", &k);
        assert_eq!(p.model_draft, r"C:\dflash\m-dflash.gguf");
        assert_eq!(p.spec_type, "draft-dflash");
        assert_eq!(p.spec_draft_n_max, Some(15));
        assert_eq!(p.spec_draft_type_k, "q8_0");
        assert_eq!(p.spec_draft_type_v, "q5_0");
    }

    // The guard for step 4 of the "add a preset field" recipe: a fully-populated
    // preset must survive render_section -> (write) -> ini::read_all -> from_keys
    // unchanged. Runs through the REAL read path (ini::read_all, which strips
    // inline comments) rather than a hand-rolled `split_once('=')`, so a field
    // added to the struct/Default/from_keys but forgotten in render_section (or
    // vice-versa) fails here instead of silently not persisting.
    #[test]
    fn full_preset_round_trips_through_ini() {
        let original = Preset {
            id: "full".into(),
            model: r"E:\m\model.gguf".into(),
            mmproj: r"E:\mmprojs\clip.gguf".into(),
            mmproj_offload: Some(false),
            image_min_tokens: Some(1024),
            image_max_tokens: Some(2048),
            model_draft: r"E:\dflashs\model-dflash.gguf".into(),
            spec_type: "draft-dflash".into(),
            spec_draft_n_max: Some(15),
            // Deliberately NOT the cache_type_k/-v below: the draft cache is a
            // separate setting, and matching values would make the round-trip
            // blind to the two being crossed.
            spec_draft_type_k: "q4_0".into(),
            spec_draft_type_v: "q4_1".into(),
            n_gpu_layers_draft: Some(99),
            device_draft: "CUDA0".into(),
            device: "CUDA0,ROCm1".into(),
            split_mode: "row".into(),
            tensor_split: "3,1".into(),
            // Two rules, so the `,` that separates them AND the `=` inside each
            // one both cross the INI reader: the two characters the value's own
            // grammar is built on.
            override_tensor: r"token_embd\.weight=ROCm0,\.ffn_(up|down|gate|gate_up)_(ch|)exps=CPU"
                .into(),
            ctx_size: Some(65536),
            n_gpu_layers: Some(40),
            parallel: Some(2),
            batch_size: Some(1024),
            ubatch_size: Some(256),
            cache_type_k: "f16".into(),
            cache_type_v: "q8_0".into(),
            flash_attn: Some(false),
            cache_ram: Some(4096),
            jinja: Some(false),
            reasoning: "on".into(),
            reasoning_format: "deepseek".into(),
            reasoning_effort: "high".into(),
            // Some(false), not None: the round-trip must prove `false` survives as
            // `false` and does not collapse into "key absent" (a distinct state:
            // --no-reasoning-preserve vs. the template's own default).
            reasoning_preserve: Some(false),
            reasoning_budget: Some(16384),
            reasoning_budget_message: "Budget reached, write the final answer now.".into(),
            // Negative on purpose: `-1` is a documented value here (generate until
            // the context is full), so the minus sign has to survive render + parse.
            n_predict: Some(-1),
            n_cpu_moe: Some(12),
            temp: Some(0.7),
            top_k: Some(40),
            top_p: Some(0.95),
            min_p: Some(0.05),
            repeat_penalty: Some(1.1),
            presence_penalty: Some(0.5),
            chat_template_kwargs: r#"{"enable_thinking":true}"#.into(),
        };

        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("presets.ini");
        fs::write(&path, render_section(&original)).unwrap();

        let sections = ini::read_all(&path);
        assert_eq!(sections.len(), 1, "one section written");
        let parsed = Preset::from_keys(&sections[0].id, &sections[0].keys);
        assert_eq!(parsed, original);
    }

    // The key name is pinned here (the round-trip above proves it parses back).
    // `override-tensor` is llama.cpp's own long-arg spelling minus the dashes,
    // which is what its preset reader matches on (common/preset.cpp,
    // `get_map_key_opt`); any other spelling makes llama-server refuse the whole
    // file with "option not recognized".
    #[test]
    fn render_emits_the_override_tensor_key_when_set_and_omits_it_when_empty() {
        let p = Preset {
            id: "ot".into(),
            model: r"E:\m\model.gguf".into(),
            override_tensor: r"token_embd\.weight=ROCm0".into(),
            ..Default::default()
        };
        assert!(render_section(&p).contains("override-tensor = token_embd\\.weight=ROCm0\r\n"));

        let empty = Preset {
            override_tensor: String::new(),
            ..p
        };
        let ini = render_section(&empty);
        let value_lines: Vec<&str> = ini
            .lines()
            .filter(|l| !l.trim_start().starts_with(';'))
            .collect();
        assert!(!value_lines
            .iter()
            .any(|l| l.starts_with("override-tensor =")));
    }

    // Same reason as the test above, for the output-budget trio: these are
    // llama.cpp's long flags minus the dashes, which is what `get_map_key_opt`
    // matches on. `reasoning-budget` and `reasoning-budget-message` are
    // registered for LLAMA_EXAMPLE_SERVER and `n-predict` for every example, so
    // all three are legal in a router preset; any OTHER spelling makes
    // llama-server reject the whole presets.ini with "option not recognized",
    // taking every other model down with it.
    #[test]
    fn render_emits_the_output_budget_keys_with_llama_cpps_own_spelling() {
        let p = Preset {
            id: "budget".into(),
            model: r"E:\m\model.gguf".into(),
            reasoning_budget: Some(16384),
            reasoning_budget_message: "Wrap up now.".into(),
            // -1 is a value here, not "unset": it has to reach the file.
            n_predict: Some(-1),
            ..Default::default()
        };
        let ini = render_section(&p);
        assert!(ini.contains("reasoning-budget = 16384\r\n"));
        assert!(ini.contains("reasoning-budget-message = Wrap up now.\r\n"));
        assert!(ini.contains("n-predict = -1\r\n"));

        let unset = Preset {
            reasoning_budget: None,
            reasoning_budget_message: String::new(),
            n_predict: None,
            ..p
        };
        let value_lines: Vec<String> = render_section(&unset)
            .lines()
            .filter(|l| !l.trim_start().starts_with(';'))
            .map(|l| l.to_string())
            .collect();
        for key in ["reasoning-budget", "reasoning-budget-message", "n-predict"] {
            assert!(
                !value_lines.iter().any(|l| l.starts_with(&format!("{key} ="))),
                "{key} must be omitted when unset"
            );
        }
    }

    // The save boundary owns --override-tensor's grammar (see validate_for_save):
    // a rule llama.cpp would `throw` on must never reach the file, because there
    // the symptom is only "the model didn't load".
    #[test]
    fn save_validation_rejects_a_malformed_override_tensor_rule() {
        let base = Preset {
            id: "ot".into(),
            model: r"C:\models\m.gguf".into(),
            ..Default::default()
        };
        let ok = Preset {
            override_tensor: r"token_embd\.weight=ROCm0".into(),
            ..base.clone()
        };
        assert!(validate_for_save(&ok).is_ok());

        // A pattern with no `=<device>`: llama.cpp's parse throws "invalid value".
        let dangling = Preset {
            override_tensor: r"token_embd\.weight".into(),
            ..base
        };
        let err = validate_for_save(&dangling).expect_err("no device");
        assert!(err.to_string().contains("token_embd"), "quotes the rule");
    }

    // make_id feeds every generated preset id from an arbitrary filename:
    // shard-suffix strip → char whitelist (alnum . - _) → collapse runs of
    // anything else to one underscore → trim edge underscores.
    #[test]
    fn make_id_sanitizes_stems() {
        assert_eq!(
            make_id(r"C:\llm\models\Qwen 3 (v2)-00001-of-00003.gguf"),
            "Qwen_3_v2"
        );
        assert_eq!(
            make_id(r"C:\m\gemma-3-12b-it-Q6_K.gguf"),
            "gemma-3-12b-it-Q6_K"
        );
        assert_eq!(make_id("weird  ~~name~~ .gguf"), "weird_name");
        assert_eq!(make_id(""), "");
    }

    // infer_models_dir seeds server.ini's ModelsDir on the first save: the
    // grandparent when the file sits in a `models` dir (any case), else the parent.
    #[test]
    fn infer_models_dir_prefers_models_grandparent() {
        assert_eq!(
            infer_models_dir(r"E:\llm\models\m.gguf").as_deref(),
            Some(r"E:\llm")
        );
        assert_eq!(
            infer_models_dir(r"E:\llm\MODELS\m.gguf").as_deref(),
            Some(r"E:\llm")
        );
        assert_eq!(
            infer_models_dir(r"E:\other\m.gguf").as_deref(),
            Some(r"E:\other")
        );
    }

    #[test]
    fn unique_id_first_free_suffix() {
        let existing = vec!["m".to_string(), "m-2".to_string()];
        assert_eq!(unique_id("m", &existing), "m-3");
    }

    #[test]
    fn unique_id_base_free_returns_base() {
        assert_eq!(unique_id("m", &["other".to_string()]), "m");
        assert_eq!(unique_id("m", &[]), "m");
    }

    // The writer trims (emit_str + the model line) because the reader trims on
    // parse: padded input must still round-trip to the TRIMMED value, not
    // diverge between the in-memory preset and the reloaded one.
    #[test]
    fn padded_values_round_trip_trimmed() {
        let p = Preset {
            id: "pad".into(),
            model: "  E:\\m\\model.gguf ".into(),
            device: " CUDA0  ".into(),
            ..Default::default()
        };
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("presets.ini");
        fs::write(&path, render_section(&p)).unwrap();

        let sections = ini::read_all(&path);
        let parsed = Preset::from_keys(&sections[0].id, &sections[0].keys);
        assert_eq!(parsed.model, "E:\\m\\model.gguf");
        assert_eq!(parsed.device, "CUDA0");
    }
}
