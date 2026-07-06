// Conversion between the Slint edit form (`PresetForm`) and the `presets::Preset`
// schema. Kept out of `gui.rs` (which only shuttles the whole `PresetForm` around,
// never per-field) so adding a preset field touches small files. This is step 7
// (both directions below) of the 7-step fan-out; the full checklist — including
// the `ui/types.slint` and `ui/models_page.slint` edits it's easy to forget —
// lives at the top of `presets.rs`.

use slint::SharedString;

use crate::gui::PresetForm;
use crate::{ini, presets};

/// The preset's string value, or the schema default when it's empty — so the
/// form's text defaults track `Preset::default()` instead of being re-hardcoded.
fn str_or(val: &str, default: &str) -> SharedString {
    SharedString::from(if val.is_empty() { default } else { val })
}

/// An optional value (no schema default) as its decimal string, or "" when unset
/// — the blank-able text a LineEdit shows for the "blank = leave unset" sampling
/// overrides. Pairs with `ini::parse_int` / `parse_float` on the way back.
// Also used by the server-side mirror (server_form.rs) — one home for the rule.
pub(crate) fn txt<T: ToString>(v: Option<T>) -> SharedString {
    v.map(|n| n.to_string()).unwrap_or_default().into()
}

/// "All layers on GPU" sentinel for the `--n-gpu-layers*` sliders: any value
/// above a real block count. The single Rust home (the form fallbacks here,
/// `apply_draft_pick` in gui/models_tab.rs). Mirrors `Options.all_layers` in
/// ui/components.slint — the equality is asserted in the e2e test
/// (src/tests/ui_bindings.rs), so a drift fails the suite instead of shipping
/// two different sentinels.
pub(crate) const ALL_LAYERS: i32 = 99;

pub fn preset_to_form(p: &presets::Preset) -> PresetForm {
    // Domain defaults are pulled from `Preset::default()` so the form and the
    // INI can't drift apart. The literals that remain are UI-only choices with
    // no counterpart in `Preset`: slider fallback positions while a flag is
    // "auto"/"default" (ALL_LAYERS / 0 / the schema default), and empty→sentinel
    // labels ("none" / "default"). For the `*_default` checkbox fields, the int
    // value is always populated (even when the checkbox is on) so the disabled
    // SpinBox shows a sensible hint — unwrap_or the schema default, or 0.
    let d = presets::Preset::default();
    PresetForm {
        id: p.id.clone().into(),
        model: p.model.clone().into(),
        mmproj: p.mmproj.clone().into(),
        model_draft: p.model_draft.clone().into(),
        spec_type: if p.spec_type.is_empty() {
            "none".into()
        } else {
            p.spec_type.clone().into()
        },
        spec_draft_n_max: p.spec_draft_n_max.unwrap_or(0),
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
        ctx_size: p.ctx_size.or(d.ctx_size).unwrap_or_default(),
        ctx_size_default: p.ctx_size.is_none(),
        n_gpu_layers: p.n_gpu_layers.unwrap_or(ALL_LAYERS),
        n_gpu_layers_auto: p.n_gpu_layers.is_none(),
        parallel: p.parallel.or(d.parallel).unwrap_or_default(),
        parallel_default: p.parallel.is_none(),
        batch_size: p.batch_size.or(d.batch_size).unwrap_or_default(),
        batch_size_default: p.batch_size.is_none(),
        ubatch_size: p.ubatch_size.or(d.ubatch_size).unwrap_or_default(),
        ubatch_size_default: p.ubatch_size.is_none(),
        cache_type_k: str_or(&p.cache_type_k, &d.cache_type_k),
        cache_type_v: str_or(&p.cache_type_v, &d.cache_type_v),
        flash_attn: p.flash_attn.or(d.flash_attn).unwrap_or_default(),
        cache_ram: p.cache_ram.or(d.cache_ram).unwrap_or(0),
        cache_ram_default: p.cache_ram.is_none(),
        jinja: p.jinja.or(d.jinja).unwrap_or_default(),
        reasoning: str_or(&p.reasoning, &d.reasoning),
        reasoning_format: str_or(&p.reasoning_format, &d.reasoning_format),
        n_cpu_moe: p.n_cpu_moe.unwrap_or(0),
        n_cpu_moe_auto: p.n_cpu_moe.is_none(),
        temp: txt(p.temp),
        temp_default: p.temp.is_none(),
        top_k: txt(p.top_k),
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
        model_draft: f.model_draft.to_string(),
        spec_type: match f.spec_type.as_str() {
            "" | "none" => String::new(),
            other => other.to_string(),
        },
        spec_draft_n_max: if f.spec_draft_n_max_default {
            None
        } else {
            Some(f.spec_draft_n_max)
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
        ctx_size: if f.ctx_size_default {
            None
        } else {
            Some(f.ctx_size).filter(|v| *v > 0)
        },
        n_gpu_layers: if f.n_gpu_layers_auto {
            None
        } else {
            Some(f.n_gpu_layers)
        },
        parallel: if f.parallel_default {
            None
        } else {
            Some(f.parallel).filter(|v| *v > 0)
        },
        batch_size: if f.batch_size_default {
            None
        } else {
            Some(f.batch_size).filter(|v| *v > 0)
        },
        ubatch_size: if f.ubatch_size_default {
            None
        } else {
            Some(f.ubatch_size).filter(|v| *v > 0)
        },
        cache_type_k: f.cache_type_k.to_string(),
        cache_type_v: f.cache_type_v.to_string(),
        flash_attn: Some(f.flash_attn),
        // Any integer is meaningful to --cache-ram (0 disables, -1 = no
        // limit), matching the hint and `Preset::from_keys` — only the
        // "default" checkbox collapses to None.
        cache_ram: if f.cache_ram_default {
            None
        } else {
            Some(f.cache_ram)
        },
        jinja: Some(f.jinja),
        reasoning: f.reasoning.to_string(),
        reasoning_format: f.reasoning_format.to_string(),
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

    #[test]
    fn rich_preset_round_trips() {
        let p = Preset {
            id: "round".into(),
            model: r"E:\m\model.gguf".into(),
            mmproj: r"E:\mmprojs\clip.gguf".into(),
            model_draft: r"E:\mtps\model-mtp.gguf".into(),
            spec_type: "draft-mtp".into(),
            spec_draft_n_max: Some(10),
            n_gpu_layers_draft: Some(99),
            device_draft: "CUDA0".into(),
            device: "CUDA0".into(),
            split_mode: "row".into(),
            tensor_split: "3,1".into(),
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
