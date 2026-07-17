//! Conversion between the Slint edit form (`PresetForm`) and the `presets::Preset`
//! schema. Kept out of `gui.rs` (which only shuttles the whole `PresetForm` around,
//! never per-field) so adding a preset field touches small files. This is step 7
//! (both directions below) of the 7-step fan-out; the full checklist — including
//! the `ui/types.slint` and `ui/models_page.slint` edits it's easy to forget —
//! lives at the top of `presets.rs`.

use slint::SharedString;

use crate::gui::PresetForm;
use crate::{ini, presets};

/// The preset's string value, or the schema default when it's empty — so the
/// form's text defaults track `Preset::default()` instead of being re-hardcoded.
fn str_or(val: &str, default: &str) -> SharedString {
    SharedString::from(if val.is_empty() { default } else { val })
}

/// An `Option<bool>` as the word the tri-state `SegmentedControl`s show — the
/// widget for a flag whose "unset" is a THIRD instruction rather than the absence
/// of one (`--flash-attn`, whose own default is `auto`; `--reasoning-preserve`,
/// whose default is whatever the chat template does). Pairs with `tri_bool` on the
/// way back. Not a bool anywhere: `Some(false)` (pass the negative flag) and
/// `None` (pass nothing) are different instructions to llama.cpp.
fn tri_state(v: Option<bool>) -> SharedString {
    match v {
        Some(true) => "on",
        Some(false) => "off",
        None => "default",
    }
    .into()
}

/// The form spelling → `Option<bool>`, the inverse of `tri_state`. Anything that
/// isn't an explicit on/off is `None` — the natural simplification
/// `Some(s == "on")` collapses "default" into an explicit off, which is a real
/// flag (`--no-flash-attn` / `--no-reasoning-preserve`) with real consequences.
fn tri_bool(s: &str) -> Option<bool> {
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

/// An optional float (no schema default) as its decimal string, or "" when unset
/// — the blank-able text a `DefaultLineEdit` shows for the sampling overrides
/// (temp / top-p / min-p / repeat- + presence-penalty). Pairs with
/// `ini::parse_float` on the way back.
fn txt(v: Option<f64>) -> SharedString {
    v.map(|n| n.to_string()).unwrap_or_default().into()
}

/// An optional INT as the text its `DefaultLineEdit` shows, falling back to
/// `hint` when the key is unset — the integer twin of `txt`, and the reason the
/// integers are strings on the form at all: they left their `SpinBox` in v1.5.0
/// because Slint's spins itself on a stray mouse-wheel over the page (the full
/// story is on the component, ui/components.slint).
///
/// Unlike `txt` this is never blank: an unset integer still SHOWS its hint, in a
/// disabled field, so unticking "default" starts from a sensible number rather
/// than an empty box. The `*_default` checkbox — not the text — is what carries
/// "unset" back to the preset.
fn itxt(v: Option<i32>, hint: i32) -> SharedString {
    v.unwrap_or(hint).to_string().into()
}

/// "All layers on GPU" sentinel for the `--n-gpu-layers*` sliders: any value
/// above a real block count. The single Rust home (the form fallbacks here,
/// `apply_draft_pick` in gui/models_tab.rs). Mirrors `Options.all_layers` in
/// ui/components.slint — the equality is asserted in the e2e test
/// (src/tests/ui_bindings.rs), so a drift fails the suite instead of shipping
/// two different sentinels.
pub(crate) const ALL_LAYERS: i32 = 99;

/// Where a numeric field parks while its **default** box is ticked: the value the
/// user takes over at the moment they untick it. These are llama.cpp's OWN
/// defaults (`common_params`, common/common.h @ b9995), so unticking a box and
/// saving reproduces what was already running instead of quietly changing it.
///
/// They can't be read off `Preset::default()` any more — a new preset now leaves
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
        // above do — because here the displayed value IS the saved one. An omitted
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
        // Tri-state, so it deliberately does NOT fall back to `d` the way the
        // fields above do: `None` is not "unset, show the default" here, it IS a
        // value — "let the template decide", distinct from an explicit off.
        reasoning_preserve: tri_state(p.reasoning_preserve),
        n_cpu_moe: p.n_cpu_moe.unwrap_or(0),
        n_cpu_moe_auto: p.n_cpu_moe.is_none(),
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
        // unset — same rule as ctx-size above.
        image_min_tokens: if f.image_min_tokens_default {
            None
        } else {
            ini::parse_int(f.image_min_tokens.as_str()).filter(|v| *v > 0)
        },
        image_max_tokens: if f.image_max_tokens_default {
            None
        } else {
            ini::parse_int(f.image_max_tokens.as_str()).filter(|v| *v > 0)
        },
        model_draft: f.model_draft.to_string(),
        spec_type: match f.spec_type.as_str() {
            "" | "none" => String::new(),
            other => other.to_string(),
        },
        // The integer fields are TEXT on the form (see `itxt`), so each one is
        // re-parsed here: unparseable (or blank) text reads as unset, the same rule
        // the float knobs below have always followed. The `> 0` filters that
        // survive are the ones that were the SpinBox's `minimum` — the widget can
        // no longer refuse the value, so this is where a 0 stops being a value.
        spec_draft_n_max: if f.spec_draft_n_max_default {
            None
        } else {
            ini::parse_int(f.spec_draft_n_max.as_str()).filter(|v| *v > 0)
        },
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
            ini::parse_int(f.ctx_size.as_str()).filter(|v| *v > 0)
        },
        n_gpu_layers: if f.n_gpu_layers_auto {
            None
        } else {
            Some(f.n_gpu_layers)
        },
        parallel: if f.parallel_default {
            None
        } else {
            ini::parse_int(f.parallel.as_str()).filter(|v| *v > 0)
        },
        batch_size: if f.batch_size_default {
            None
        } else {
            ini::parse_int(f.batch_size.as_str()).filter(|v| *v > 0)
        },
        ubatch_size: if f.ubatch_size_default {
            None
        } else {
            ini::parse_int(f.ubatch_size.as_str()).filter(|v| *v > 0)
        },
        cache_type_k: enum_or_empty(f.cache_type_k.as_str()),
        cache_type_v: enum_or_empty(f.cache_type_v.as_str()),
        flash_attn: tri_bool(f.flash_attn.as_str()),
        // Any integer is meaningful to --cache-ram (0 disables, -1 = no
        // limit), matching the hint and `Preset::from_keys` — so NO `> 0` filter
        // here, and its field is the one integer taking `decimal` input (the only
        // `input_type` that lets a `-` be typed at all).
        cache_ram: if f.cache_ram_default {
            None
        } else {
            ini::parse_int(f.cache_ram.as_str())
        },
        jinja: Some(f.jinja),
        reasoning: f.reasoning.to_string(),
        reasoning_format: f.reasoning_format.to_string(),
        // "default" → None: omit the key, let the template decide.
        reasoning_preserve: tri_bool(f.reasoning_preserve.as_str()),
        n_cpu_moe: if f.n_cpu_moe_auto {
            None
        } else {
            Some(f.n_cpu_moe)
        },
        temp: if f.temp_default {
            None
        } else {
            ini::parse_float(f.temp.as_str())
        },
        // Integer-valued (digits-only input, unlike the float sampling knobs
        // below) — any int is meaningful (0 = disable top-k), so no `> 0` filter:
        // only the "default" checkbox, or text that isn't a number, collapses to None.
        top_k: if f.top_k_default {
            None
        } else {
            ini::parse_int(f.top_k.as_str())
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
        chat_template_kwargs: f.chat_template_kwargs.to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::presets::Preset;

    fn round_trip(p: &Preset) -> Preset {
        form_to_preset(&preset_to_form(p))
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

    // reasoning-preserve is the one TRI-state field: None ("let the template
    // decide", key omitted) is a third value, not the absence of one. The two
    // round-trip fixtures above pin Some(true)/Some(false); this pins all three at
    // once, and above all that None SURVIVES. The bug it guards is the natural
    // simplification `Some(f.reasoning_preserve == "on")`, which collapses None to
    // Some(false) — every preset that never asked would silently start emitting
    // --no-reasoning-preserve, overriding templates that preserve by default.
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
        // uses — a mismatch here leaves the control with no segment highlighted.
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
    // then said q8_0) and `flash_attn` to Some(true) whenever the key was absent —
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

        // …and the form spellings are the ones the widgets' option lists use — a
        // mismatch leaves the ComboBox on its first row / the SegmentedControl with
        // no pill lit. "default" is `Options.cache_types[0]` and the first pill.
        let f = preset_to_form(&unset);
        assert_eq!(f.cache_type_k, "default");
        assert_eq!(f.cache_type_v, "default");
        assert_eq!(f.flash_attn, "default");

        // An explicit choice is still an explicit choice — including "off", which
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
            reasoning_preserve: Some(true),
            n_cpu_moe: Some(12),
            temp: Some(0.7),
            top_k: Some(40),
            top_p: Some(0.95),
            min_p: Some(0.05),
            repeat_penalty: Some(1.1),
            presence_penalty: Some(0.5),
            chat_template_kwargs: r#"{"enable_thinking":true}"#.into(),
        };
        assert_eq!(round_trip(&p), p);
    }

    // "0 disables, -1 = no limit" — the documented --cache-ram sentinels must
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
}
