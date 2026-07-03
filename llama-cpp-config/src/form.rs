// Conversion between the Slint edit form (`PresetForm`) and the `presets::Preset`
// schema. Kept out of `gui.rs` so the most common change — adding a preset field —
// touches this file plus `presets.rs`, not the ~1300-line GUI module.

use slint::SharedString;

use crate::gui::PresetForm;
use crate::{ini, presets};

pub fn blank_form() -> PresetForm {
    PresetForm::default()
}

/// The preset's string value, or the schema default when it's empty — so the
/// form's text defaults track `Preset::default()` instead of being re-hardcoded.
fn str_or(val: &str, default: &str) -> SharedString {
    SharedString::from(if val.is_empty() { default } else { val })
}

/// A numeric field rendered as text for a string-typed form field: the preset's
/// value, or the schema default, formatted as a decimal string (empty if both
/// are `None`). Pairs with `ini::parse_int` on the way back in `form_to_preset`.
fn num_or(val: Option<i32>, default: Option<i32>) -> SharedString {
    val.or(default)
        .map(|v| v.to_string())
        .unwrap_or_default()
        .into()
}

pub fn preset_to_form(p: &presets::Preset) -> PresetForm {
    // Domain defaults are pulled from `Preset::default()` so the form and the
    // INI can't drift apart. The literals that remain are UI-only choices with
    // no counterpart in `Preset`: slider fallback positions while a flag is
    // "auto" (99 / 0), and empty→sentinel labels ("none" / "default").
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
        spec_draft_n_max: p
            .spec_draft_n_max
            .map(|v| v.to_string())
            .unwrap_or_default()
            .into(),
        n_gpu_layers_draft: p.n_gpu_layers_draft.unwrap_or(99),
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
        n_gpu_layers: p.n_gpu_layers.unwrap_or(99),
        n_gpu_layers_auto: p.n_gpu_layers.is_none(),
        parallel: p.parallel.or(d.parallel).unwrap_or_default(),
        batch_size: p.batch_size.or(d.batch_size).unwrap_or_default(),
        ubatch_size: p.ubatch_size.or(d.ubatch_size).unwrap_or_default(),
        cache_type_k: str_or(&p.cache_type_k, &d.cache_type_k),
        cache_type_v: str_or(&p.cache_type_v, &d.cache_type_v),
        flash_attn: p.flash_attn.or(d.flash_attn).unwrap_or_default(),
        cache_ram: num_or(p.cache_ram, d.cache_ram),
        jinja: p.jinja.or(d.jinja).unwrap_or_default(),
        reasoning: str_or(&p.reasoning, &d.reasoning),
        reasoning_format: str_or(&p.reasoning_format, &d.reasoning_format),
        n_cpu_moe: p.n_cpu_moe.unwrap_or(0),
        n_cpu_moe_auto: p.n_cpu_moe.is_none(),
        temp: p.temp.map(|v| v.to_string()).unwrap_or_default().into(),
        top_k: p.top_k.map(|v| v.to_string()).unwrap_or_default().into(),
        top_p: p.top_p.map(|v| v.to_string()).unwrap_or_default().into(),
        min_p: p.min_p.map(|v| v.to_string()).unwrap_or_default().into(),
        repeat_penalty: p
            .repeat_penalty
            .map(|v| v.to_string())
            .unwrap_or_default()
            .into(),
        presence_penalty: p
            .presence_penalty
            .map(|v| v.to_string())
            .unwrap_or_default()
            .into(),
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
        spec_draft_n_max: ini::parse_int(f.spec_draft_n_max.as_str()),
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
        ctx_size: Some(f.ctx_size).filter(|v| *v > 0),
        n_gpu_layers: if f.n_gpu_layers_auto {
            None
        } else {
            Some(f.n_gpu_layers)
        },
        parallel: Some(f.parallel).filter(|v| *v > 0),
        batch_size: Some(f.batch_size).filter(|v| *v > 0),
        ubatch_size: Some(f.ubatch_size).filter(|v| *v > 0),
        cache_type_k: f.cache_type_k.to_string(),
        cache_type_v: f.cache_type_v.to_string(),
        flash_attn: Some(f.flash_attn),
        cache_ram: ini::parse_int(f.cache_ram.as_str()).filter(|v| *v > 0),
        jinja: Some(f.jinja),
        reasoning: f.reasoning.to_string(),
        reasoning_format: f.reasoning_format.to_string(),
        n_cpu_moe: if f.n_cpu_moe_auto {
            None
        } else {
            Some(f.n_cpu_moe)
        },
        temp: ini::parse_float(f.temp.as_str()),
        top_k: ini::parse_int(f.top_k.as_str()),
        top_p: ini::parse_float(f.top_p.as_str()),
        min_p: ini::parse_float(f.min_p.as_str()),
        repeat_penalty: ini::parse_float(f.repeat_penalty.as_str()),
        presence_penalty: ini::parse_float(f.presence_penalty.as_str()),
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
}
