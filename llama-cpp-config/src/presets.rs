// presets.ini schema and IO for llama.cpp-framework.

use std::fs;
use std::io;
use std::path::PathBuf;

use crate::ini;
use crate::paths;
use crate::server_cfg;

#[derive(Debug, Clone)]
pub struct Preset {
    pub id: String,
    pub model: String,
    pub mmproj: String,
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
            ctx_size: Some(32768),
            n_gpu_layers: Some(99),
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

pub fn save(preset: &Preset) -> io::Result<()> {
    let path = paths::presets_ini();
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    let body = render_section(preset);
    ini::replace_section(&path, &preset.id, &body)?;
    let current = server_cfg::load().models_dir.unwrap_or_default();
    if current.is_empty() {
        if let Some(models_dir) = infer_models_dir(&preset.model) {
            let _ = ini::replace_key(&paths::server_ini(), "Server", "ModelsDir", &models_dir);
        }
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
    let path = paths::presets_ini();
    ini::rename_section(&path, old_id, new)
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
    if let Some((head, tail)) = stem.rsplit_once("-of-") {
        if tail.len() == 5 && tail.chars().all(|c| c.is_ascii_digit()) {
            if let Some(idx) = head.rfind('-') {
                let counter = &head[idx + 1..];
                if counter.len() == 5 && counter.chars().all(|c| c.is_ascii_digit()) {
                    return head[..idx].to_string();
                }
            }
        }
    }
    stem.to_string()
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
    out.push_str("; Re-running the wizard rewrites this section; hand-edits to OTHER sections\r\n");
    out.push_str("; in this file are preserved.\r\n\r\n");

    out.push_str("; Model: local path (-m).\r\n");
    out.push_str(&format!("model = {}\r\n", p.model));
    out.push_str("\r\n; Sub-model paths\r\n");
    emit_str(&mut out, "mmproj", &p.mmproj);

    out.push_str("\r\n; Resource / context\r\n");
    emit_i32(&mut out, "ctx-size", p.ctx_size);
    emit_i32(&mut out, "n-gpu-layers", p.n_gpu_layers);
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
    if !val.trim().is_empty() {
        out.push_str(&format!("{key} = {val}\r\n"));
    }
}

fn emit_bool(out: &mut String, key: &str, val: Option<bool>) {
    if let Some(v) = val {
        out.push_str(&format!(
            "{key} = {}\r\n",
            if v { "true" } else { "false" }
        ));
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
