// Provider & model integration for opencode and Claude Code.
//
// Manages the `provider.llama.cpp` section in opencode.json, auto-generating
// model entries from presets.ini. Claude Code does not support per-file
// custom providers natively; an env-var snippet is generated for the GUI's
// Claude Code card instead (copy-paste only — never written to disk).

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
        .get_mut("provider")
        .and_then(|p| p.get_mut(PROVIDER_KEY))
        .and_then(|p| p.get_mut("models"))
        .and_then(|m| m.as_object_mut())
        .ok_or_else(|| anyhow::anyhow!("provider.{PROVIDER_KEY}.models is not an object"))?;

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
        .and_then(|v| v.get("provider").and_then(|p| p.get(PROVIDER_KEY)).cloned())
        .is_some()
}

// ── Claude Code ────────────────────────────────────────────────────────

/// Generates a PowerShell snippet that sets the env vars needed for
/// Claude Code to use llama-server as a custom model provider. `example_id` is
/// a preset id to show in the commented model line (the caller already holds
/// the loaded presets — this stays a pure string builder, no hidden disk IO).
pub fn claude_code_env_script(base_url: &str, example_id: Option<&str>) -> String {
    let base = base_url.trim_end_matches("/v1");
    let example = example_id.unwrap_or("<preset-id>");
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

    lp_obj.insert("npm".to_string(), json!(PROVIDER_NPM));
    lp_obj.insert("name".to_string(), json!(PROVIDER_NAME));

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

    // Advertised context for the tool's model entry. Deliberately larger than
    // Preset::default().ctx_size (32768): opencode only needs the ceiling the
    // model *can* serve, not the preset's runtime ctx-size — so don't "align"
    // these two numbers.
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

    readable
        .split_whitespace()
        .take(6)
        .map(|w| {
            let mut c = w.chars();
            match c.next() {
                None => String::new(),
                Some(first) => first.to_uppercase().chain(c).collect(),
            }
        })
        .collect::<Vec<_>>()
        .join(" ")
}

#[cfg(test)]
mod tests {
    use super::*;

    // The merge invariant that keeps hand-edited opencode.json safe: syncing the
    // provider section must not touch unrelated top-level keys or other providers.
    // (save_opencode_models itself reads global paths, so the merge logic is
    // tested here at the ensure_provider_section seam.)
    #[test]
    fn ensure_provider_section_preserves_unrelated_config() {
        let mut v = json!({
            "theme": "dark",
            "provider": {
                "other": { "npm": "@other/sdk", "models": { "x": {} } }
            }
        });
        ensure_provider_section(&mut v, "http://localhost:8080/v1").unwrap();
        assert_eq!(v["theme"], "dark");
        assert_eq!(v["provider"]["other"]["npm"], "@other/sdk");
        assert_eq!(v["provider"]["other"]["models"]["x"], json!({}));
        assert_eq!(v["provider"][PROVIDER_KEY]["npm"], PROVIDER_NPM);
        assert_eq!(
            v["provider"][PROVIDER_KEY]["options"]["baseURL"],
            "http://localhost:8080/v1"
        );
        assert!(v["provider"][PROVIDER_KEY]["models"].is_object());
    }

    #[test]
    fn ensure_provider_section_updates_base_url_in_place() {
        let mut v = json!({});
        ensure_provider_section(&mut v, "http://localhost:8080/v1").unwrap();
        ensure_provider_section(&mut v, "http://0.0.0.0:9090/v1").unwrap();
        assert_eq!(
            v["provider"][PROVIDER_KEY]["options"]["baseURL"],
            "http://0.0.0.0:9090/v1"
        );
    }

    #[test]
    fn opencode_model_reasoning_emitted_only_when_not_auto() {
        let mut p = presets::Preset {
            id: "m".into(),
            model: r"C:\models\m.gguf".into(),
            ..Default::default()
        };
        p.reasoning = "auto".into();
        assert!(preset_to_opencode_model(&p).get("reasoning").is_none());
        p.reasoning = "on".into();
        assert_eq!(preset_to_opencode_model(&p)["reasoning"], json!(true));
        p.reasoning = "off".into();
        assert_eq!(preset_to_opencode_model(&p)["reasoning"], json!(false));
    }

    #[test]
    fn opencode_model_limit_math() {
        let mut p = presets::Preset {
            id: "m".into(),
            model: r"C:\models\m.gguf".into(),
            ..Default::default()
        };
        // Unset ctx-size advertises the deliberate 131072 ceiling (see the
        // comment in preset_to_opencode_model), output = context / 4.
        p.ctx_size = None;
        let e = preset_to_opencode_model(&p);
        assert_eq!(e["limit"]["context"], 131072);
        assert_eq!(e["limit"]["output"], 32768);
        // Small contexts still advertise at least 32000 output tokens.
        p.ctx_size = Some(8192);
        let e = preset_to_opencode_model(&p);
        assert_eq!(e["limit"]["context"], 8192);
        assert_eq!(e["limit"]["output"], 32000);
    }

    #[test]
    fn friendly_model_name_cases() {
        // Title-cases the filename stem, separators become spaces.
        assert_eq!(
            friendly_model_name("id", r"C:\m\gemma-4-12b_q6.gguf"),
            "Gemma 4 12b Q6"
        );
        // Caps at 6 words.
        assert_eq!(
            friendly_model_name("id", r"C:\m\a-b-c-d-e-f-g-h.gguf"),
            "A B C D E F"
        );
        // No usable stem falls back to the preset id.
        assert_eq!(friendly_model_name("fallback-id", ""), "Fallback Id");
    }
}
