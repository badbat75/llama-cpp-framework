// Provider & model integration for opencode and Claude Code.
//
// Manages the `provider.llama.cpp` section in opencode.json, auto-generating
// model entries from presets.ini. Claude Code does not support per-file
// custom providers natively; a setup script is generated instead.

use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use serde_json::{json, Value};

use crate::ini;
use crate::paths;
use crate::presets;

const PROVIDER_KEY: &str = "llama.cpp";
const PROVIDER_NPM: &str = "@ai-sdk/openai-compatible";
const PROVIDER_NAME: &str = "llama-server (local)";

// ── opencode.json ──────────────────────────────────────────────────────

/// Returns the set of preset IDs currently registered as models in opencode.json.
pub fn opencode_model_ids() -> Vec<String> {
    let cfg = read_opencode();
    cfg.as_ref()
        .and_then(|v| v.get("provider"))
        .and_then(|p| p.get(PROVIDER_KEY))
        .and_then(|p| p.get("models"))
        .and_then(|m| m.as_object())
        .map(|obj| obj.keys().cloned().collect())
        .unwrap_or_default()
}

/// Ensures the provider entry exists and syncs the models list from presets.
/// `checked_ids` are the preset IDs the user wants exposed.
/// `base_url` comes from the current server.ini (e.g. "http://127.0.0.1:8080/v1").
pub fn save_opencode_models(checked_ids: &[String], base_url: &str) -> Result<()> {
    let path = paths::opencode_user_config();
    let mut v = read_or_create_value(&path)?;

    ensure_provider_section(&mut v, base_url)?;

    let all_presets = presets::load_all();
    let preset_map: BTreeMap<&str, &presets::Preset> =
        all_presets.iter().map(|p| (p.id.as_str(), p)).collect();

    let models = v
        .pointer_mut("/provider/llama.cpp/models")
        .and_then(|m| m.as_object_mut())
        .ok_or_else(|| anyhow::anyhow!("provider.llama.cpp.models is not an object"))?;

    models.clear();

    for id in checked_ids {
        if let Some(p) = preset_map.get(id.as_str()) {
            let entry = preset_to_opencode_model(p);
            models.insert(id.clone(), entry);
        }
    }

    let serialized = serde_json::to_string_pretty(&v)?;
    ini::atomic_write(&path, &(serialized + "\n"))
        .with_context(|| format!("write {}", path.display()))?;
    Ok(())
}

pub fn detect_opencode_provider() -> bool {
    read_opencode()
        .and_then(|v| v.pointer("/provider/llama.cpp").cloned())
        .is_some()
}

// ── Claude Code ────────────────────────────────────────────────────────

/// Generates a PowerShell snippet that sets the env vars needed for
/// Claude Code to use llama-server as a custom model provider.
pub fn claude_code_env_script(base_url: &str) -> String {
    let base = base_url.trim_end_matches("/v1");
    let example = presets::load_all()
        .first()
        .map(|p| p.id.clone())
        .unwrap_or_else(|| "<preset-id>".to_string());
    format!(
        "# Run this in PowerShell before launching Claude Code to use your local llama-server.\r\n\
         $env:ANTHROPIC_BASE_URL = \"{base}\"\r\n\
         $env:ANTHROPIC_API_KEY = \"not-needed\"\r\n\
         # Pick a preset id from your presets.ini as the model name, e.g.:\r\n\
         # $env:ANTHROPIC_DEFAULT_SONNET_MODEL = \"{example}\"\r\n\
         # Then launch: claude\r\n",
    )
}

// ── Helpers ────────────────────────────────────────────────────────────

fn read_opencode() -> Option<Value> {
    let path = paths::opencode_user_config();
    if !path.exists() {
        return None;
    }
    let txt = fs::read_to_string(&path).ok()?;
    if txt.trim().is_empty() {
        return None;
    }
    serde_json::from_str(&txt).ok()
}

fn read_or_create_value(path: &Path) -> Result<Value> {
    if path.exists() {
        let txt = fs::read_to_string(path).context("read existing config")?;
        if txt.trim().is_empty() {
            Ok(Value::Object(Default::default()))
        } else {
            serde_json::from_str(&txt).context("parse existing config as JSON")
        }
    } else {
        Ok(Value::Object(Default::default()))
    }
}

fn ensure_provider_section(v: &mut Value, base_url: &str) -> Result<()> {
    let obj = v
        .as_object_mut()
        .ok_or_else(|| anyhow::anyhow!("top-level is not an object"))?;

    let provider = obj
        .entry("provider".to_string())
        .or_insert_with(|| json!({}));
    let provider_obj = provider
        .as_object_mut()
        .ok_or_else(|| anyhow::anyhow!("provider is not an object"))?;

    let lp = provider_obj
        .entry(PROVIDER_KEY.to_string())
        .or_insert_with(|| {
            json!({
                "npm": PROVIDER_NPM,
                "name": PROVIDER_NAME,
                "options": { "baseURL": base_url },
                "models": {}
            })
        });
    let lp_obj = lp
        .as_object_mut()
        .ok_or_else(|| anyhow::anyhow!("provider.llama.cpp is not an object"))?;

    lp_obj.insert(
        "npm".to_string(),
        json!(PROVIDER_NPM),
    );
    lp_obj.insert(
        "name".to_string(),
        json!(PROVIDER_NAME),
    );

    let opts = lp_obj
        .entry("options".to_string())
        .or_insert_with(|| json!({}));
    let opts_obj = opts
        .as_object_mut()
        .ok_or_else(|| anyhow::anyhow!("options is not an object"))?;
    opts_obj.insert("baseURL".to_string(), json!(base_url));

    lp_obj
        .entry("models".to_string())
        .or_insert_with(|| json!({}));

    Ok(())
}

fn preset_to_opencode_model(p: &presets::Preset) -> Value {
    let name = friendly_model_name(&p.id, &p.model);
    let mut entry = json!({ "name": name });

    if !p.reasoning.is_empty() && p.reasoning != "auto" {
        entry["reasoning"] = json!(p.reasoning == "on");
    }

    let context = p.ctx_size.unwrap_or(131072);
    let output = std::cmp::max(context / 4, 32000);
    entry["limit"] = json!({
        "context": context,
        "input": context,
        "output": output,
    });

    entry
}

/// Human-readable title-cased label derived from the model filename,
/// falling back to the preset id. Shared by the Integrations tab and
/// the opencode model entries.
pub(crate) fn friendly_model_name(id: &str, model_path: &str) -> String {
    let stem = PathBuf::from(model_path)
        .file_stem()
        .map(|s| s.to_string_lossy().to_string())
        .unwrap_or_else(|| id.to_string());

    let readable = stem.replace(['-', '_'], " ");

    let mut words: Vec<&str> = readable.split_whitespace().collect();
    if words.len() > 6 {
        words.truncate(6);
    }

    let title: String = words
        .iter()
        .map(|w| {
            let mut c = w.chars();
            match c.next() {
                None => String::new(),
                Some(first) => first.to_uppercase().chain(c).collect(),
            }
        })
        .collect::<Vec<_>>()
        .join(" ");

    title
}

