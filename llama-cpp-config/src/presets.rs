//! presets.ini schema and IO for llama.cpp-framework.
//!
//! ADD A PRESET FIELD — the recurring change fans out to all of these (trace an
//! existing field like `ctx-size` as the template; kebab-case INI key ↔
//! snake_case Rust field):
//!   1. `Preset` struct field (+ doc)      — below
//!   2. `impl Default for Preset`          — below
//!   3. `Preset::from_keys`                — INI read, below
//!   4. `render_section` + `emit_*` (+ `;` comment) — INI write, below
//!   5. `PresetForm` struct                — ui/types.slint (a NUMERIC field also
//!      needs a paired `<field>_default: bool` — the "omit the flag" checkbox)
//!   6. the input widget                   — ui/models_page.slint, bind two-way
//!      `<=>`: DefaultSpinBox (int) / DefaultLineEdit (float) for numerics — wire
//!      BOTH `value` and `default`; EnumComboBox for string dropdowns
//!   7. `preset_to_form` + `form_to_preset` — src/form.rs (BOTH directions; a
//!      numeric derives `<field>_default` via `is_none()` one way and
//!      `if <field>_default { None } else { … }` the other)
//!   8. FREE-TEXT field only (any value the user types freely — a filesystem
//!      path, OR raw JSON like `chat-template-kwargs`): add it to
//!      `validate_for_save`'s list below AND to the
//!      `save_validation_rejects_comment_markers_in_free_text_fields` test — the
//!      INI format can't escape `;`/`#` (legal in Windows dirs and in JSON
//!      strings), so an unvalidated value saves fine and reloads TRUNCATED.
//!      Nothing fails if you skip this.
//!
//! Guards: the INI round-trip test in this file (`full_preset_round_trips_through_ini`)
//! and the form round-trip test in form.rs (`form_to_preset(preset_to_form(p)) == p`)
//! — a field wired into one side only drops out of one of them. Give the new
//! field a NON-DEFAULT value when extending the rich fixtures: `None`/empty
//! satisfies the compiler but makes the round-trips vacuous for that field.
//! Step 8 is the one step NO test catches when skipped (round-trip fixtures use
//! clean paths) — same for its widget (step 6: a forgotten widget just never
//! appears in the UI).

use std::fs;
use std::io;
use std::path::PathBuf;

use crate::ini;
use crate::paths;
use crate::server_cfg;

#[derive(Debug, Clone, PartialEq)]
pub struct Preset {
    pub id: String,
    pub model: String,
    pub mmproj: String,
    // Speculative decoding / Multi-Token Prediction (MTP) / DFlash.
    // `model_draft` is the draft GGUF (--model-draft): an MTP head, a DFlash
    // drafter, or a small standalone draft model. `spec_type` selects the
    // speculator (--spec-type, e.g. "draft-mtp" or "draft-dflash"). Empty = unset.
    // `spec_draft_n_max` (--spec-draft-n-max) caps drafted tokens per step;
    // DFlash clamps it to the trained block_size-1 (e.g. 15).
    // `n_gpu_layers_draft` (--n-gpu-layers-draft) controls draft offload;
    // `device_draft` (--device-draft) pins the draft to one GPU (e.g. "CUDA0").
    // gemma4-assistant MTP heads (n_layer=0) crash under the multi-device
    // "auto" split, so pin to a single device to run the draft on GPU.
    pub model_draft: String,
    pub spec_type: String,
    pub spec_draft_n_max: Option<i32>,
    pub n_gpu_layers_draft: Option<i32>,
    pub device_draft: String,
    /// GPU device(s) for THIS model (--device), e.g. "CUDA0". Per-preset
    /// override of server.ini Device. Pinning a small model to one GPU lets it
    /// fit fully (no multi-device memory fitting), which is required for GPU MTP.
    pub device: String,
    /// Multi-GPU split for THIS model. `split_mode` (--split-mode): none|layer|row;
    /// `tensor_split` (--tensor-split): per-GPU weight proportions like "3,1".
    /// Empty = inherit the server.ini default. Identical on CUDA and HIP.
    pub split_mode: String,
    pub tensor_split: String,
    pub ctx_size: Option<i32>,
    pub n_gpu_layers: Option<i32>,
    pub parallel: Option<i32>,
    pub batch_size: Option<i32>,
    pub ubatch_size: Option<i32>,
    pub cache_type_k: String,
    pub cache_type_v: String,
    pub flash_attn: Option<bool>,
    pub cache_ram: Option<i32>,
    pub jinja: Option<bool>,
    pub reasoning: String,
    pub reasoning_format: String,
    pub n_cpu_moe: Option<i32>,
    pub temp: Option<f64>,
    pub top_k: Option<i32>,
    pub top_p: Option<f64>,
    pub min_p: Option<f64>,
    pub repeat_penalty: Option<f64>,
    pub presence_penalty: Option<f64>,
    pub chat_template_kwargs: String,
}

impl Default for Preset {
    fn default() -> Self {
        Self {
            id: String::new(),
            model: String::new(),
            mmproj: String::new(),
            model_draft: String::new(),
            spec_type: String::new(),
            spec_draft_n_max: None,
            n_gpu_layers_draft: None,
            device_draft: String::new(),
            device: String::new(),
            split_mode: String::new(),
            tensor_split: String::new(),
            ctx_size: Some(32768),
            // Default to "auto" (omit --n-gpu-layers) like n-cpu-moe and the draft
            // layers: the GUI shows all three sliders with their "auto" box checked
            // for a new preset. n_cpu_moe / n_gpu_layers_draft are already None.
            n_gpu_layers: None,
            parallel: Some(4),
            batch_size: Some(512),
            ubatch_size: Some(512),
            cache_type_k: "q8_0".into(),
            cache_type_v: "q8_0".into(),
            flash_attn: Some(true),
            cache_ram: Some(8192),
            jinja: Some(true),
            reasoning: "auto".into(),
            reasoning_format: "auto".into(),
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
            model_draft: get("model-draft"),
            spec_type: get("spec-type"),
            spec_draft_n_max: k.get("spec-draft-n-max").and_then(|v| ini::parse_int(v)),
            n_gpu_layers_draft: k.get("n-gpu-layers-draft").and_then(|v| ini::parse_int(v)),
            device_draft: get("device-draft"),
            device: get("device"),
            split_mode: get("split-mode"),
            tensor_split: get("tensor-split"),
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
/// also seeds it — inferred from the model's path (its `models\` grandparent) —
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
/// Enforced at BOTH free-text ways into a header — `rename` and the save
/// boundary — so a hand-authored id (a future `preset set`/import, or an
/// editable-id GUI change) can't corrupt the file. Emptiness is a separate
/// check so `rename` can keep its own "…is empty" message.
fn valid_id(id: &str) -> bool {
    id.chars()
        .all(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '-' | '_'))
}

/// Save-boundary validation, pure so the unit test never touches `paths::`:
/// the id becomes a section header and the free-text fields must survive the
/// INI comment rule — a `;`/`#` in a GGUF path OR in the raw-JSON
/// `chat-template-kwargs` would silently reload truncated (here AND in
/// llama-server's own preset reader), so refuse it with the field name (the
/// cure is renaming the file / fixing the JSON). See `ini::reject_comment_markers`.
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
    ] {
        ini::reject_comment_markers(field, value)?;
    }
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
    // boundary is the other — see `valid_id`). Hold it to the same charset:
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
    emit_str(&mut out, "model-draft", &p.model_draft);

    out.push_str("\r\n; Speculative decoding / Multi-Token Prediction / DFlash\r\n");
    out.push_str("; spec-type pairs model-draft with a speculator: draft-mtp (MTP head),\r\n");
    out.push_str("; draft-dflash (DFlash block-diffusion drafter), or draft-simple.\r\n");
    emit_str(&mut out, "spec-type", &p.spec_type);
    out.push_str("; spec-draft-n-max = max drafted tokens per step. DFlash clamps this to the\r\n");
    out.push_str(
        "; model's trained block_size - 1 (e.g. 15); also applies to draft-mtp/simple.\r\n",
    );
    emit_i32(&mut out, "spec-draft-n-max", p.spec_draft_n_max);
    out.push_str("; Run the draft on GPU by pinning it to ONE device (device-draft, e.g.\r\n");
    out.push_str("; CUDA0) with n-gpu-layers-draft = 99. gemma4-assistant MTP heads\r\n");
    out.push_str("; (n_layer=0) crash under the multi-device auto split; pinning avoids it.\r\n");
    out.push_str("; Use n-gpu-layers-draft = 0 to fall back to CPU.\r\n");
    emit_i32(&mut out, "n-gpu-layers-draft", p.n_gpu_layers_draft);
    emit_str(&mut out, "device-draft", &p.device_draft);

    out.push_str("\r\n; Resource / context\r\n");
    emit_i32(&mut out, "ctx-size", p.ctx_size);
    emit_i32(&mut out, "n-gpu-layers", p.n_gpu_layers);
    out.push_str("; device = CUDA0 pins this model to one GPU (overrides server.ini Device).\r\n");
    emit_str(&mut out, "device", &p.device);
    out.push_str("; Multi-GPU distribution for this model (overrides server.ini; same on\r\n");
    out.push_str("; CUDA and HIP): split-mode = none|layer|row, tensor-split = per-GPU\r\n");
    out.push_str("; weight proportions (e.g. 3,1). Blank = server default.\r\n");
    emit_str(&mut out, "split-mode", &p.split_mode);
    emit_str(&mut out, "tensor-split", &p.tensor_split);
    emit_i32(&mut out, "parallel", p.parallel);
    emit_i32(&mut out, "batch-size", p.batch_size);
    emit_i32(&mut out, "ubatch-size", p.ubatch_size);

    out.push_str("\r\n; KV cache\r\n");
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

    // Validation only — both shapes must reject BEFORE any file IO (so this
    // never touches paths::, per the src/tests/mod.rs warning).
    #[test]
    fn rename_rejects_blank_and_unchanged_ids() {
        assert!(rename("old", "  ").is_err(), "blank new id");
        assert!(rename("old", "old").is_err(), "unchanged id");
        assert!(rename("old", " old ").is_err(), "unchanged after trim");
    }

    // Free-text rename ids must stay inside make_id's charset — `[`/`]`/CR/LF
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
            ..Default::default()
        };
        assert!(validate_for_save(&clean).is_ok());

        for (field, hostile) in [
            ("model", r"C:\Models #1\m.gguf"),
            ("mmproj", r"C:\a;b\m-mmproj.gguf"),
            ("model-draft", r"C:\models\m #mtp.gguf"),
            // Legal inside a JSON string, fatal to the INI reader.
            ("chat-template-kwargs", r##"{"tag":"#think"}"##),
        ] {
            let mut p = clean.clone();
            match field {
                "model" => p.model = hostile.into(),
                "mmproj" => p.mmproj = hostile.into(),
                "chat-template-kwargs" => p.chat_template_kwargs = hostile.into(),
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
            n_gpu_layers_draft: Some(99),
            device_draft: "CUDA0".into(),
            device: "CUDA0".into(),
            ..Default::default()
        };
        let ini = render_section(&p);
        assert!(ini.contains("model-draft = C:\\dflash\\m-dflash.gguf\r\n"));
        assert!(ini.contains("spec-type = draft-dflash\r\n"));
        assert!(ini.contains("spec-draft-n-max = 15\r\n"));
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
        // Only value lines count — the section carries a `; spec-type = …` help
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
        let p = Preset::from_keys("m", &k);
        assert_eq!(p.model_draft, r"C:\dflash\m-dflash.gguf");
        assert_eq!(p.spec_type, "draft-dflash");
        assert_eq!(p.spec_draft_n_max, Some(15));
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
            model_draft: r"E:\dflashs\model-dflash.gguf".into(),
            spec_type: "draft-dflash".into(),
            spec_draft_n_max: Some(15),
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

        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("presets.ini");
        fs::write(&path, render_section(&original)).unwrap();

        let sections = ini::read_all(&path);
        assert_eq!(sections.len(), 1, "one section written");
        let parsed = Preset::from_keys(&sections[0].id, &sections[0].keys);
        assert_eq!(parsed, original);
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
    // parse — padded input must still round-trip to the TRIMMED value, not
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
