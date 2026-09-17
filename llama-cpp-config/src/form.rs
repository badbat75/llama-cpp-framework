//! Conversion between the Slint edit form (`PresetForm`) and the `presets::Preset`
//! schema. Kept out of `gui.rs` (which only shuttles the whole `PresetForm` around,
//! never per-field) so adding a preset field touches small files. This is step 7
//! (both directions below) of the 7-step fan-out; the full checklist (including
//! the `ui/types.slint` and `ui/models_page.slint` edits it's easy to forget)
//! lives at the top of `presets.rs`.

use std::ops::RangeInclusive;

use slint::SharedString;

use crate::gui::PresetForm;
use crate::{ini, presets};

/// The preset's string value, or the schema default when it's empty, so the
/// form's text defaults track `Preset::default()` instead of being re-hardcoded.
fn str_or(val: &str, default: &str) -> SharedString {
    SharedString::from(if val.is_empty() { default } else { val })
}

/// An `Option<bool>` as the word the tri-state `SegmentedControl`s show: the
/// widget for a flag whose "unset" is a THIRD instruction rather than the absence
/// of one (`--flash-attn`, whose own default is `auto`; `--reasoning-preserve`,
/// whose default was the template's own behaviour up to llama.cpp v0.3.0 and is
/// ON since v0.4.0). Pairs with `tri_bool` on the way back. Not a bool anywhere:
/// `Some(false)` (pass the negative flag) and `None` (pass nothing) are different
/// instructions to llama.cpp.
///
/// Shared with the SERVER form (`server_form.rs`, `rocblas_use_hipblaslt`), whose
/// third state is "never export the env var"; same shape, same widget, so the
/// pair lives here once rather than being re-derived per form.
pub(crate) fn tri_state(v: Option<bool>) -> SharedString {
    match v {
        Some(true) => "on",
        Some(false) => "off",
        None => "default",
    }
    .into()
}

/// The form spelling → `Option<bool>`, the inverse of `tri_state`. Anything that
/// isn't an explicit on/off is `None`: the natural simplification
/// `Some(s == "on")` collapses "default" into an explicit off, which is a real
/// flag (`--no-flash-attn` / `--no-reasoning-preserve`) with real consequences.
pub(crate) fn tri_bool(s: &str) -> Option<bool> {
    match s {
        "on" => Some(true),
        "off" => Some(false),
        _ => None,
    }
}

/// An enum-valued string field where empty means "omit the flag": carried to the
/// form as the word "default", which is the first entry of the widget's option
/// list (`Options.cache_types`, `Options.split_modes`). Pairs with `enum_or_empty`.
fn enum_or_default(val: &str) -> SharedString {
    SharedString::from(if val.is_empty() { "default" } else { val })
}

/// The form spelling → the INI value, the inverse of `enum_or_default`: the
/// "default" entry (and an empty string) collapse to "", which `render_section`'s
/// `emit_str` then drops from the file entirely.
fn enum_or_empty(val: &str) -> String {
    match val {
        "" | "default" => String::new(),
        other => other.to_string(),
    }
}

/// An optional float (no schema default) as its decimal string, or "" when
/// unset: the blank-able text a `DefaultLineEdit` shows for the sampling
/// overrides (temp / top-p / min-p / repeat- + presence-penalty). Pairs with
/// `ini::parse_float` on the way back.
fn txt(v: Option<f64>) -> SharedString {
    v.map(|n| n.to_string()).unwrap_or_default().into()
}

/// An optional INT as the text its `DefaultLineEdit` shows, falling back to
/// `hint` when the key is unset, the integer twin of `txt`, and the reason the
/// integers are strings on the form at all: they left their `SpinBox` in v1.5.0
/// because Slint's spins itself on a stray mouse-wheel over the page (the full
/// story is on the component, ui/components.slint).
///
/// Unlike `txt` this is never blank: an unset integer still SHOWS its hint, in a
/// disabled field, so unticking "default" starts from a sensible number rather
/// than an empty box. The `*_default` checkbox (not the text) is what carries
/// "unset" back to the preset.
fn itxt(v: Option<i32>, hint: i32) -> SharedString {
    v.unwrap_or(hint).to_string().into()
}

/// "All layers on GPU" sentinel for the `--n-gpu-layers*` sliders: any value
/// above a real block count. The single Rust home (the form fallbacks here,
/// `apply_draft_pick` in gui/models_tab.rs). Mirrors `Options.all_layers` in
/// ui/components.slint; the equality is asserted in the e2e test
/// (src/tests/ui_bindings.rs), so a drift fails the suite instead of shipping
/// two different sentinels.
pub(crate) const ALL_LAYERS: i32 = 99;

/// Where a numeric field parks while its **default** box is ticked: the value the
/// user takes over at the moment they untick it. These are llama.cpp's OWN
/// defaults (`common_params`, common/common.h @ b9995), so unticking a box and
/// saving reproduces what was already running instead of quietly changing it.
///
/// They can't be read off `Preset::default()` any more: a new preset now leaves
/// every one of these keys UNSET (that is what "the model runs on llama.cpp's
/// defaults" means), so the schema has no number left to lend. `--ctx-size` has
/// none to mirror in the first place: its default is `0` = "the context the model
/// was trained with", so the box parks on a conservative 32k rather than on a `0`
/// that would read as broken. The tooltips in ui/models_page.slint name the real
/// default per field, and they are the text these numbers must not contradict.
const HINT_CTX_SIZE: i32 = 32768;
const HINT_PARALLEL: i32 = 4;
const HINT_BATCH_SIZE: i32 = 2048;
const HINT_UBATCH_SIZE: i32 = 512;
const HINT_CACHE_RAM: i32 = 8192;
const HINT_TOP_K: i32 = 40;
const HINT_SPEC_DRAFT_N_MAX: i32 = 3;
// --image-min/max-tokens have no fixed llama.cpp numeric default (their -1 = read
// from the model), so the box parks on a sane value the way --ctx-size does: the
// 1024 llama.cpp itself suggests for Qwen-VL grounding, and a matching upper bound.
const HINT_IMAGE_MIN_TOKENS: i32 = 1024;
const HINT_IMAGE_MAX_TOKENS: i32 = 2048;
// --reasoning-budget and --n-predict both default to `-1` in llama.cpp (no
// budget / generate until the context is full), and unlike --ctx-size's `0` that
// number is not a broken-looking placeholder but a value worth keeping, so
// neither gets an invented parking number. Both widgets take `decimal` input for
// it, the only kind that lets a `-` be typed at all (see `cache_ram`).
const HINT_REASONING_BUDGET: i32 = -1;
const HINT_N_PREDICT: i32 = -1;

pub fn preset_to_form(p: &presets::Preset) -> PresetForm {
    // String/bool domain defaults are pulled from `Preset::default()` so the form
    // and the INI can't drift apart. The literals that remain are UI-only choices
    // with no counterpart in `Preset`: slider fallback positions while a flag is
    // "auto"/"default" (ALL_LAYERS / 0 / the HINT_* values above), and
    // empty→sentinel labels ("none" / "default").
    let d = presets::Preset::default();
    PresetForm {
        id: p.id.clone().into(),
        model: p.model.clone().into(),
        mmproj: p.mmproj.clone().into(),
        mmproj_offload: p.mmproj_offload.or(d.mmproj_offload).unwrap_or_default(),
        image_min_tokens: itxt(p.image_min_tokens, HINT_IMAGE_MIN_TOKENS),
        image_min_tokens_default: p.image_min_tokens.is_none(),
        image_max_tokens: itxt(p.image_max_tokens, HINT_IMAGE_MAX_TOKENS),
        image_max_tokens_default: p.image_max_tokens.is_none(),
        model_draft: p.model_draft.clone().into(),
        spec_type: if p.spec_type.is_empty() {
            "none".into()
        } else {
            p.spec_type.clone().into()
        },
        spec_draft_n_max: itxt(p.spec_draft_n_max, HINT_SPEC_DRAFT_N_MAX),
        spec_draft_n_max_default: p.spec_draft_n_max.is_none(),
        spec_draft_type_k: enum_or_default(&p.spec_draft_type_k),
        spec_draft_type_v: enum_or_default(&p.spec_draft_type_v),
        n_gpu_layers_draft: p.n_gpu_layers_draft.unwrap_or(ALL_LAYERS),
        n_gpu_layers_draft_auto: p.n_gpu_layers_draft.is_none(),
        device_draft: p.device_draft.clone().into(),
        device: p.device.clone().into(),
        split_mode: if p.split_mode.is_empty() {
            "default".into()
        } else {
            p.split_mode.clone().into()
        },
        tensor_split: p.tensor_split.clone().into(),
        override_tensor: p.override_tensor.clone().into(),
        ctx_size: itxt(p.ctx_size, HINT_CTX_SIZE),
        ctx_size_default: p.ctx_size.is_none(),
        n_gpu_layers: p.n_gpu_layers.unwrap_or(ALL_LAYERS),
        n_gpu_layers_auto: p.n_gpu_layers.is_none(),
        parallel: itxt(p.parallel, HINT_PARALLEL),
        parallel_default: p.parallel.is_none(),
        batch_size: itxt(p.batch_size, HINT_BATCH_SIZE),
        batch_size_default: p.batch_size.is_none(),
        ubatch_size: itxt(p.ubatch_size, HINT_UBATCH_SIZE),
        ubatch_size_default: p.ubatch_size.is_none(),
        // The KV-cache trio carries its "unset" into the WIDGET (the "default"
        // entry / pill) instead of parking on a hint the way the numeric fields
        // above do, because here the displayed value IS the saved one. An omitted
        // cache-type-k once fell back to the schema (which then said `q8_0`), so it
        // displayed q8_0 and got WRITTEN BACK as q8_0 on the next save of any
        // unrelated field, quietly turning llama.cpp's f16 into q8_0 on a preset
        // nobody had touched. Empty ↔ "default" here, like `split_mode`.
        cache_type_k: enum_or_default(&p.cache_type_k),
        cache_type_v: enum_or_default(&p.cache_type_v),
        flash_attn: tri_state(p.flash_attn),
        cache_ram: itxt(p.cache_ram, HINT_CACHE_RAM),
        cache_ram_default: p.cache_ram.is_none(),
        jinja: p.jinja.or(d.jinja).unwrap_or_default(),
        reasoning: str_or(&p.reasoning, &d.reasoning),
        reasoning_format: str_or(&p.reasoning_format, &d.reasoning_format),
        // Not `str_or`: an empty reasoning-effort has no schema default to fall
        // back to, it IS the "omit the flag" state, carried to the combo as the
        // word "default" exactly like the cache types above.
        reasoning_effort: enum_or_default(&p.reasoning_effort),
        // Tri-state, so it deliberately does NOT fall back to `d` the way the
        // fields above do: `None` is not "unset, show the default" here, it IS a
        // value: "pass no flag, take llama.cpp's default" (on since v0.4.0),
        // distinct from an explicit off.
        reasoning_preserve: tri_state(p.reasoning_preserve),
        reasoning_budget: itxt(p.reasoning_budget, HINT_REASONING_BUDGET),
        reasoning_budget_default: p.reasoning_budget.is_none(),
        reasoning_budget_message: p.reasoning_budget_message.clone().into(),
        n_predict: itxt(p.n_predict, HINT_N_PREDICT),
        n_predict_default: p.n_predict.is_none(),
        n_cpu_moe: p.n_cpu_moe.unwrap_or(0),
        n_cpu_moe_auto: p.n_cpu_moe.is_none(),
        n_cpu_ffn: p.n_cpu_ffn.unwrap_or(0),
        n_cpu_ffn_auto: p.n_cpu_ffn.is_none(),
        temp: txt(p.temp),
        temp_default: p.temp.is_none(),
        top_k: itxt(p.top_k, HINT_TOP_K),
        top_k_default: p.top_k.is_none(),
        top_p: txt(p.top_p),
        top_p_default: p.top_p.is_none(),
        min_p: txt(p.min_p),
        min_p_default: p.min_p.is_none(),
        repeat_penalty: txt(p.repeat_penalty),
        repeat_penalty_default: p.repeat_penalty.is_none(),
        presence_penalty: txt(p.presence_penalty),
        presence_penalty_default: p.presence_penalty.is_none(),
        // Tri-state like reasoning_preserve: None is "omit the flag", a value
        // of its own, so no fallback to `d`.
        backend_sampling: tri_state(p.backend_sampling),
        chat_template_kwargs: p.chat_template_kwargs.clone().into(),
    }
}

pub fn form_to_preset(f: &PresetForm) -> presets::Preset {
    presets::Preset {
        id: f.id.to_string(),
        model: f.model.to_string(),
        mmproj: f.mmproj.to_string(),
        mmproj_offload: Some(f.mmproj_offload),
        // Vision-token bounds: positive only (llama.cpp's -1 = "read from model" is
        // exactly our omit-the-flag/None), so `> 0` collapses 0 or a stray sign to
        // unset, same rule as ctx-size above.
        image_min_tokens: if f.image_min_tokens_default {
            None
        } else {
            ini::parse_int_in(f.image_min_tokens.as_str(), &ini::INT_POSITIVE)
        },
        image_max_tokens: if f.image_max_tokens_default {
            None
        } else {
            ini::parse_int_in(f.image_max_tokens.as_str(), &ini::INT_POSITIVE)
        },
        model_draft: f.model_draft.to_string(),
        spec_type: match f.spec_type.as_str() {
            "" | "none" => String::new(),
            other => other.to_string(),
        },
        // The integer fields are TEXT on the form (see `itxt`), so each one is
        // re-parsed here, inside the range that was the SpinBox's `minimum`
        // (`INT_POSITIVE`) or none (`INT_ANY`). Text outside it reads as unset,
        // which is only safe because the Save button asks `invalid_numbers` first
        // and refuses: the lenient reading is for `prune_inactive_draft_fields`,
        // which runs while the user is still typing. A range changed here must
        // change in `int_fields` too (a test holds the two together).
        spec_draft_n_max: if f.spec_draft_n_max_default {
            None
        } else {
            ini::parse_int_in(f.spec_draft_n_max.as_str(), &ini::INT_POSITIVE)
        },
        spec_draft_type_k: enum_or_empty(f.spec_draft_type_k.as_str()),
        spec_draft_type_v: enum_or_empty(f.spec_draft_type_v.as_str()),
        n_gpu_layers_draft: if f.n_gpu_layers_draft_auto {
            None
        } else {
            Some(f.n_gpu_layers_draft)
        },
        device_draft: f.device_draft.to_string(),
        device: f.device.to_string(),
        split_mode: match f.split_mode.as_str() {
            "" | "default" => String::new(),
            other => other.to_string(),
        },
        tensor_split: f.tensor_split.to_string(),
        override_tensor: f.override_tensor.to_string(),
        ctx_size: if f.ctx_size_default {
            None
        } else {
            ini::parse_int_in(f.ctx_size.as_str(), &ini::INT_POSITIVE)
        },
        n_gpu_layers: if f.n_gpu_layers_auto {
            None
        } else {
            Some(f.n_gpu_layers)
        },
        parallel: if f.parallel_default {
            None
        } else {
            ini::parse_int_in(f.parallel.as_str(), &ini::INT_POSITIVE)
        },
        batch_size: if f.batch_size_default {
            None
        } else {
            ini::parse_int_in(f.batch_size.as_str(), &ini::INT_POSITIVE)
        },
        ubatch_size: if f.ubatch_size_default {
            None
        } else {
            ini::parse_int_in(f.ubatch_size.as_str(), &ini::INT_POSITIVE)
        },
        cache_type_k: enum_or_empty(f.cache_type_k.as_str()),
        cache_type_v: enum_or_empty(f.cache_type_v.as_str()),
        flash_attn: tri_bool(f.flash_attn.as_str()),
        // Any integer is meaningful to --cache-ram (0 disables, -1 = no
        // limit), matching the hint and `Preset::from_keys`, so NO `> 0` filter
        // here, and its field is the one integer taking `decimal` input (the only
        // `input_type` that lets a `-` be typed at all).
        cache_ram: if f.cache_ram_default {
            None
        } else {
            ini::parse_int_in(f.cache_ram.as_str(), &ini::INT_ANY)
        },
        jinja: Some(f.jinja),
        reasoning: f.reasoning.to_string(),
        reasoning_format: f.reasoning_format.to_string(),
        reasoning_effort: enum_or_empty(f.reasoning_effort.as_str()),
        // "default" → None: omit the key, let the template decide.
        reasoning_preserve: tri_bool(f.reasoning_preserve.as_str()),
        // Every integer is meaningful to both of these (-1 = unrestricted / until
        // the context is full, 0 = close the thinking block at once), so NO `> 0`
        // filter, the same rule --cache-ram follows above.
        reasoning_budget: if f.reasoning_budget_default {
            None
        } else {
            ini::parse_int_in(f.reasoning_budget.as_str(), &ini::INT_ANY)
        },
        reasoning_budget_message: f.reasoning_budget_message.to_string(),
        n_predict: if f.n_predict_default {
            None
        } else {
            ini::parse_int_in(f.n_predict.as_str(), &ini::INT_ANY)
        },
        n_cpu_moe: if f.n_cpu_moe_auto {
            None
        } else {
            Some(f.n_cpu_moe)
        },
        n_cpu_ffn: if f.n_cpu_ffn_auto {
            None
        } else {
            Some(f.n_cpu_ffn)
        },
        temp: if f.temp_default {
            None
        } else {
            ini::parse_float(f.temp.as_str())
        },
        // Integer-valued (digits-only input, unlike the float sampling knobs
        // below); any int is meaningful (0 = disable top-k), so no `> 0` filter:
        // only the "default" checkbox, or text that isn't a number, collapses to None.
        top_k: if f.top_k_default {
            None
        } else {
            ini::parse_int_in(f.top_k.as_str(), &ini::INT_ANY)
        },
        top_p: if f.top_p_default {
            None
        } else {
            ini::parse_float(f.top_p.as_str())
        },
        min_p: if f.min_p_default {
            None
        } else {
            ini::parse_float(f.min_p.as_str())
        },
        repeat_penalty: if f.repeat_penalty_default {
            None
        } else {
            ini::parse_float(f.repeat_penalty.as_str())
        },
        presence_penalty: if f.presence_penalty_default {
            None
        } else {
            ini::parse_float(f.presence_penalty.as_str())
        },
        // "default" → None: omit the key. Never `Some(s == "on")`: that would
        // write `backend-sampling = false` into every preset that never asked.
        backend_sampling: tri_bool(f.backend_sampling.as_str()),
        chat_template_kwargs: f.chat_template_kwargs.to_string(),
    }
}

/// Every INTEGER text field of the preset form, as (INI key, its "default" box,
/// its text, the values it takes). The range is the one `form_to_preset` reads
/// the same field with: `invalid_numbers_match_what_the_conversion_drops` holds
/// the two together, and `every_integer_field_is_checked` holds this list to the
/// `PresetForm` struct in ui/types.slint, so a new integer field that skips it
/// fails a test instead of saving a mistyped number as an absent key.
fn int_fields(f: &PresetForm) -> [(&'static str, bool, &str, RangeInclusive<i32>); 11] {
    [
        (
            "image-min-tokens",
            f.image_min_tokens_default,
            f.image_min_tokens.as_str(),
            ini::INT_POSITIVE,
        ),
        (
            "image-max-tokens",
            f.image_max_tokens_default,
            f.image_max_tokens.as_str(),
            ini::INT_POSITIVE,
        ),
        (
            "spec-draft-n-max",
            f.spec_draft_n_max_default,
            f.spec_draft_n_max.as_str(),
            ini::INT_POSITIVE,
        ),
        (
            "ctx-size",
            f.ctx_size_default,
            f.ctx_size.as_str(),
            ini::INT_POSITIVE,
        ),
        (
            "parallel",
            f.parallel_default,
            f.parallel.as_str(),
            ini::INT_POSITIVE,
        ),
        (
            "batch-size",
            f.batch_size_default,
            f.batch_size.as_str(),
            ini::INT_POSITIVE,
        ),
        (
            "ubatch-size",
            f.ubatch_size_default,
            f.ubatch_size.as_str(),
            ini::INT_POSITIVE,
        ),
        (
            "cache-ram",
            f.cache_ram_default,
            f.cache_ram.as_str(),
            ini::INT_ANY,
        ),
        (
            "reasoning-budget",
            f.reasoning_budget_default,
            f.reasoning_budget.as_str(),
            ini::INT_ANY,
        ),
        (
            "n-predict",
            f.n_predict_default,
            f.n_predict.as_str(),
            ini::INT_ANY,
        ),
        ("top-k", f.top_k_default, f.top_k.as_str(), ini::INT_ANY),
    ]
}

/// The integer fields whose "default" box is unticked but whose text is not a
/// number the key takes, one status-line phrase each; empty when the form can be
/// saved. The Save button refuses on any, because `form_to_preset` would write
/// each of them as an ABSENT key, and absent is an instruction of its own
/// (`ctx-size` absent = the model's trained context; see `ini::int_problem`).
pub fn invalid_numbers(f: &PresetForm) -> Vec<String> {
    int_fields(f)
        .into_iter()
        .filter(|(_, default, ..)| !default)
        .filter_map(|(key, _, text, range)| ini::int_problem(key, text, &range))
        .collect()
}

/// `presets::prune_inactive_draft_keys` applied to the live FORM, returning the
/// same list of dropped INI keys. Called when the Model-info box learns what the
/// selected model is (`update_model_info`), so the greyed-out speculative fields
/// cannot keep a value the model can't use; see the policy's own doc comment
/// for what "can't use" costs.
///
/// The policy stays in `presets.rs` and the form conversions stay in the two
/// functions above: this round-trips through `Preset` to reuse both. Only the
/// draft fields are lifted back out, deliberately: writing the whole converted
/// form back would also normalize whatever half-typed text sits in the OTHER
/// numeric fields, and this runs while the user is editing.
pub fn prune_inactive_draft_fields(f: &mut PresetForm, embeds_mtp: bool) -> Vec<&'static str> {
    let mut p = form_to_preset(f);
    let dropped = presets::prune_inactive_draft_keys(&mut p, embeds_mtp);
    if !dropped.is_empty() {
        let pruned = preset_to_form(&p);
        f.spec_type = pruned.spec_type;
        f.spec_draft_n_max = pruned.spec_draft_n_max;
        f.spec_draft_n_max_default = pruned.spec_draft_n_max_default;
        f.spec_draft_type_k = pruned.spec_draft_type_k;
        f.spec_draft_type_v = pruned.spec_draft_type_v;
        f.n_gpu_layers_draft = pruned.n_gpu_layers_draft;
        f.n_gpu_layers_draft_auto = pruned.n_gpu_layers_draft_auto;
        f.device_draft = pruned.device_draft;
    }
    dropped
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeSet;

    use super::*;
    use crate::presets::Preset;

    fn round_trip(p: &Preset) -> Preset {
        form_to_preset(&preset_to_form(p))
    }

    // The prune's FORM spelling, which is not the schema's: an emptied
    // `spec_type` has to come back as the combo's "none" entry (an empty string
    // matches no option and leaves the widget showing the old one), the draft KV
    // types as the combo's "default" entry for the same reason, and the two
    // omit-the-flag companions as their `_default` / `_auto` booleans.
    #[test]
    fn prune_resets_the_draft_fields_to_their_form_spelling() {
        let mut f = preset_to_form(&Preset {
            spec_type: "draft-mtp".into(),
            spec_draft_n_max: Some(2),
            spec_draft_type_k: "q8_0".into(),
            spec_draft_type_v: "q8_0".into(),
            n_gpu_layers_draft: Some(99),
            device_draft: "CUDA0".into(),
            ctx_size: Some(65536),
            ..Preset::default()
        });
        // Half-typed text in an unrelated field: this runs on every model /
        // mmproj / draft change, i.e. WHILE the user is editing, so the write-back
        // must not launder the rest of the form through form_to_preset.
        f.temp = "1.".into();

        let dropped = prune_inactive_draft_fields(&mut f, false);

        assert_eq!(dropped.len(), 6, "all six keys are dead without a draft");
        assert_eq!(f.spec_type, "none");
        assert!(f.spec_draft_n_max_default);
        // Empty ↔ "default", the EnumComboBox's first entry: an empty string
        // matches no option and would leave q8_0 showing.
        assert_eq!(f.spec_draft_type_k, "default");
        assert_eq!(f.spec_draft_type_v, "default");
        assert!(f.n_gpu_layers_draft_auto);
        assert_eq!(f.device_draft, "");
        assert_eq!(f.ctx_size, "65536", "unrelated field survives");
        assert_eq!(f.temp, "1.", "in-progress text survives");
    }

    // Nothing to drop → the form must come back byte-identical, or every
    // model-info refresh would mark a clean preset dirty.
    #[test]
    fn prune_leaves_a_clean_form_untouched() {
        let mut f = preset_to_form(&Preset::default());
        f.temp = "1.".into();
        let before = f.clone();
        assert!(prune_inactive_draft_fields(&mut f, false).is_empty());
        assert_eq!(f, before);
    }

    // A preset in its "saved" shape (string fields non-empty, matching
    // `Preset::default()`) survives form ↔ preset unchanged. This is the guard
    // for the 7-step "add a preset field" recipe: a field wired into one
    // conversion but not the other drops out here.
    #[test]
    fn default_preset_round_trips() {
        let p = Preset::default();
        assert_eq!(round_trip(&p), p);
    }

    /// Put `text` in every integer field and untick every "default" box.
    fn set_all_int_texts(f: &mut PresetForm, text: &str) {
        for (field, default) in [
            (&mut f.image_min_tokens, &mut f.image_min_tokens_default),
            (&mut f.image_max_tokens, &mut f.image_max_tokens_default),
            (&mut f.spec_draft_n_max, &mut f.spec_draft_n_max_default),
            (&mut f.ctx_size, &mut f.ctx_size_default),
            (&mut f.parallel, &mut f.parallel_default),
            (&mut f.batch_size, &mut f.batch_size_default),
            (&mut f.ubatch_size, &mut f.ubatch_size_default),
            (&mut f.cache_ram, &mut f.cache_ram_default),
            (&mut f.reasoning_budget, &mut f.reasoning_budget_default),
            (&mut f.n_predict, &mut f.n_predict_default),
            (&mut f.top_k, &mut f.top_k_default),
        ] {
            *field = text.into();
            *default = false;
        }
    }

    // The Save button's refusal and the conversion must agree field by field:
    // what the refusal lets through reaches the INI as a key, and what it refuses
    // is exactly what the conversion would have written as an ABSENT key. Read
    // back through the rendered section, so there is no per-field accessor to
    // drift. A field left out of `set_all_int_texts` keeps its ticked box, which
    // the conversion drops and the unfiltered refusal passes: it fails here too.
    #[test]
    fn invalid_numbers_match_what_the_conversion_drops() {
        for text in ["", "  ", "x", "0", "-1", "7", " 42 ", "1.5", "196608245760"] {
            let mut f = preset_to_form(&Preset {
                id: "t".into(),
                ..Preset::default()
            });
            set_all_int_texts(&mut f, text);
            let refused: BTreeSet<&str> = int_fields(&f)
                .into_iter()
                .filter(|(key, _, t, range)| ini::int_problem(key, t, range).is_some())
                .map(|(key, ..)| key)
                .collect();
            let section = presets::render_section(&form_to_preset(&f));
            let dropped: BTreeSet<&str> = int_fields(&f)
                .into_iter()
                .map(|(key, ..)| key)
                .filter(|key| {
                    let line = format!("{key} = ");
                    !section.lines().any(|l| l.starts_with(&line))
                })
                .collect();
            assert_eq!(refused, dropped, "text {text:?}");
            assert_eq!(invalid_numbers(&f).len(), refused.len(), "text {text:?}");
        }
        // A ticked box is a choice, not a mistake: nothing to refuse.
        let f = preset_to_form(&Preset::default());
        assert!(invalid_numbers(&f).is_empty());
    }

    // `int_fields` has to list every integer field `PresetForm` has, read off
    // ui/types.slint: each numeric text field there carries a `<name>_default`
    // companion, and the five floats are the only ones that are not integers.
    #[test]
    fn every_integer_field_is_checked() {
        let types = include_str!("../ui/types.slint");
        let body = types
            .split("export struct PresetForm {")
            .nth(1)
            .and_then(|rest| rest.split("\n}").next())
            .expect("PresetForm in ui/types.slint");
        let floats = [
            "temp",
            "top_p",
            "min_p",
            "repeat_penalty",
            "presence_penalty",
        ];
        let in_struct: BTreeSet<String> = body
            .lines()
            .filter_map(|l| l.trim().strip_suffix("_default: bool,"))
            .filter(|name| !floats.contains(name))
            .map(str::to_string)
            .collect();
        let checked: BTreeSet<String> = int_fields(&PresetForm::default())
            .into_iter()
            .map(|(key, ..)| key.replace('-', "_"))
            .collect();
        assert_eq!(in_struct, checked);
    }

    // reasoning-preserve is the one TRI-state field: None (key omitted, i.e.
    // llama.cpp's own default, which is ON since v0.4.0) is a third value, not
    // the absence of one. The two round-trip fixtures above pin
    // Some(true)/Some(false); this pins all three at once, and above all that
    // None SURVIVES. The bug it guards is the natural simplification
    // `Some(f.reasoning_preserve == "on")`, which collapses None to Some(false):
    // every preset that never asked would silently start emitting
    // --no-reasoning-preserve, i.e. turn the default OFF on every model.
    #[test]
    fn reasoning_preserve_keeps_all_three_states_apart() {
        for state in [None, Some(true), Some(false)] {
            let p = Preset {
                reasoning_preserve: state,
                ..Preset::default()
            };
            assert_eq!(round_trip(&p).reasoning_preserve, state, "state {state:?}");
        }

        // …and the form spelling is the one the SegmentedControl's `options` list
        // uses; a mismatch here leaves the control with no segment highlighted.
        let spelling = |state| {
            preset_to_form(&Preset {
                reasoning_preserve: state,
                ..Preset::default()
            })
            .reasoning_preserve
            .to_string()
        };
        assert_eq!(spelling(None), "default");
        assert_eq!(spelling(Some(true)), "on");
        assert_eq!(spelling(Some(false)), "off");
    }

    // The KV-cache card's other three flags have an "unset" that is a real
    // instruction too, so it has to survive the form the way reasoning-preserve's
    // does. It did not: `cache_type_k/v` fell back to `Preset::default()` (which
    // then said q8_0) and `flash_attn` to Some(true) whenever the key was absent,
    // so a preset that had never named a cache type DISPLAYED q8_0 and, on the next
    // save of any unrelated field, WROTE q8_0, quietly requantizing a KV cache
    // llama.cpp would have left at f16. The empty/None state must reach the form as
    // "default" and come back out empty/None.
    #[test]
    fn kv_cache_unset_state_survives_the_form_round_trip() {
        let unset = Preset {
            cache_type_k: String::new(),
            cache_type_v: String::new(),
            flash_attn: None,
            ..Preset::default()
        };
        let back = round_trip(&unset);
        assert_eq!(back.cache_type_k, "", "cache-type-k invented a value");
        assert_eq!(back.cache_type_v, "", "cache-type-v invented a value");
        assert_eq!(back.flash_attn, None, "flash-attn invented a value");

        // …and the form spellings are the ones the widgets' option lists use; a
        // mismatch leaves the ComboBox on its first row / the SegmentedControl with
        // no pill lit. "default" is `Options.cache_types[0]` and the first pill.
        let f = preset_to_form(&unset);
        assert_eq!(f.cache_type_k, "default");
        assert_eq!(f.cache_type_v, "default");
        assert_eq!(f.flash_attn, "default");

        // An explicit choice is still an explicit choice, including "off", which
        // is NOT the same as unset (it passes --flash-attn off, forcing the kernel
        // away even where the backend has it).
        for state in [None, Some(true), Some(false)] {
            let p = Preset {
                flash_attn: state,
                ..Preset::default()
            };
            assert_eq!(round_trip(&p).flash_attn, state, "flash-attn {state:?}");
        }
    }

    #[test]
    fn rich_preset_round_trips() {
        let p = Preset {
            id: "round".into(),
            model: r"E:\m\model.gguf".into(),
            mmproj: r"E:\mmprojs\clip.gguf".into(),
            mmproj_offload: Some(false),
            image_min_tokens: Some(1024),
            image_max_tokens: Some(2048),
            model_draft: r"E:\mtps\model-mtp.gguf".into(),
            spec_type: "draft-mtp".into(),
            spec_draft_n_max: Some(10),
            // Not the cache_type_k/-v below: the draft cache is its own setting.
            spec_draft_type_k: "q4_0".into(),
            spec_draft_type_v: "q4_1".into(),
            n_gpu_layers_draft: Some(99),
            device_draft: "CUDA0".into(),
            device: "CUDA0,ROCm1".into(),
            split_mode: "row".into(),
            tensor_split: "3,1".into(),
            override_tensor: r"token_embd\.weight=ROCm0".into(),
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
            reasoning_effort: "xhigh".into(),
            reasoning_preserve: Some(true),
            reasoning_budget: Some(16384),
            reasoning_budget_message: "Budget reached, write the final answer now.".into(),
            n_predict: Some(24576),
            n_cpu_moe: Some(12),
            n_cpu_ffn: Some(4),
            temp: Some(0.7),
            top_k: Some(40),
            top_p: Some(0.95),
            min_p: Some(0.05),
            repeat_penalty: Some(1.1),
            presence_penalty: Some(0.5),
            backend_sampling: Some(true),
            chat_template_kwargs: r#"{"enable_thinking":true}"#.into(),
        };
        assert_eq!(round_trip(&p), p);
    }

    // backend-sampling is the third tri-state (after reasoning-preserve and
    // flash-attn) and the one where the collapse would be least visible: the
    // flag has no negated form, so `Some(false)` and `None` launch identically
    // and a form that turned "default" into "off" would corrupt every preset's
    // INI without changing a single argv. Pin all three states and the
    // SegmentedControl spellings.
    #[test]
    fn backend_sampling_keeps_all_three_states_apart() {
        for state in [None, Some(true), Some(false)] {
            let p = Preset {
                backend_sampling: state,
                ..Preset::default()
            };
            assert_eq!(round_trip(&p).backend_sampling, state, "state {state:?}");
        }
        let spelling = |state| {
            preset_to_form(&Preset {
                backend_sampling: state,
                ..Preset::default()
            })
            .backend_sampling
            .to_string()
        };
        assert_eq!(spelling(None), "default");
        assert_eq!(spelling(Some(true)), "on");
        assert_eq!(spelling(Some(false)), "off");
    }

    // "0 disables, -1 = no limit": the documented --cache-ram sentinels must
    // survive the form leg (a `> 0` filter here once silently dropped them,
    // falling back to llama-server's 8192 MiB default).
    #[test]
    fn cache_ram_sentinels_round_trip() {
        for v in [0, -1] {
            let p = Preset {
                cache_ram: Some(v),
                ..Preset::default()
            };
            assert_eq!(round_trip(&p).cache_ram, Some(v));
        }
    }

    // Same trap, two more fields: `-1` is "unrestricted" for --reasoning-budget
    // and "until the context is full" for --n-predict, and `0` closes the
    // thinking block immediately. A `> 0` filter copied from the neighbouring
    // integers would silently turn each of those into "key absent", which is a
    // DIFFERENT instruction: it hands the thinking back its unlimited default.
    #[test]
    fn reasoning_budget_and_n_predict_sentinels_round_trip() {
        for v in [0, -1, 16384] {
            let p = Preset {
                reasoning_budget: Some(v),
                n_predict: Some(v),
                ..Preset::default()
            };
            let back = round_trip(&p);
            assert_eq!(back.reasoning_budget, Some(v), "reasoning-budget {v}");
            assert_eq!(back.n_predict, Some(v), "n-predict {v}");
        }
    }

    // The message is free text that reaches the model verbatim, so it must not be
    // laundered on the way through the form (trimmed, quoted, or collapsed to
    // empty): what the user typed is what gets injected before the end tag.
    #[test]
    fn reasoning_budget_message_survives_verbatim() {
        let msg = "Budget reached. Stop analysing and write the final answer now.";
        let p = Preset {
            reasoning_budget_message: msg.into(),
            ..Preset::default()
        };
        assert_eq!(round_trip(&p).reasoning_budget_message, msg);
    }
}
