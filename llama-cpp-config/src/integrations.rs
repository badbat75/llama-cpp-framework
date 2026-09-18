//! Provider & model integration for opencode and Claude Code.
//!
//! Manages the `provider.llama.cpp` section in opencode.json, auto-generating
//! model entries from presets.ini. Claude Code does not support per-file
//! custom providers natively; an env-var snippet is generated for the GUI's
//! Claude Code card instead (copy-paste only, never written to disk).
//!
//! The model list is not a selection of its own: it is every ENABLED preset
//! (`presets::load_all`, i.e. presets.ini), the same set llama-server serves,
//! so switching a preset off in the Models tab is also how it leaves OpenCode.
//! It used to be a second checkbox list here, which could offer OpenCode a model
//! llama-server had no preset for, or hide one it did. The provider section is
//! created only by the Integrations tab's Save; after that every preset change
//! re-derives the list on its own (`follow_presets`), from the GUI and the CLI
//! alike, and a machine that never set OpenCode up is never written to.
//!
//! The one non-mechanical field is a model's `limit.context`, and it is not a
//! copy of the preset's `ctx-size`: opencode fills a prompt to whatever ceiling it
//! is given, so the number has to be the context ONE request can actually use,
//! which llama.cpp derives from `ctx-size`, `parallel` and the model's trained
//! context TOGETHER. The whole derivation, and the three upstream rules behind it,
//! lives in `effective_ctx`; it is the reason this module reads GGUF headers.

use std::fs;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use serde_json::{json, Value};

use crate::gguf;
use crate::ini;
use crate::paths;
use crate::presets;
use crate::server_cfg;

const PROVIDER_KEY: &str = "llama.cpp";
const PROVIDER_NPM: &str = "@ai-sdk/openai-compatible";
const PROVIDER_NAME: &str = "llama-server (local)";

/// Advertised context when the model's trained context can't be read (no
/// `ggml-base.dll` beside the app, or an unreadable GGUF) AND the preset names no
/// `ctx-size`. A guess, and the only one left; `effective_ctx` derives every
/// other case.
const CTX_FALLBACK: i64 = 131_072;

// ── opencode.json ──────────────────────────────────────────────────────

/// Returns the preset IDs currently registered as models in opencode.json.
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

/// Ensures the provider entry exists and rebuilds its models list: one entry per
/// ENABLED preset (see the module header).
/// `base_url` comes from the current server.ini (e.g. "http://127.0.0.1:8080/v1").
/// `api_key` is the optional API key for proxy/gateway authentication.
///
/// A rebuild that comes out identical to the file is not written: every preset
/// save now lands here (`follow_presets`), and opencode.json is the user's file,
/// so it is touched only when its content actually has to change.
pub fn save_opencode_models(base_url: &str, api_key: Option<&str>) -> Result<()> {
    let path = paths::opencode_user_config();
    let mut v = read_or_create_value(&path)?;

    ensure_provider_section(&mut v, base_url, api_key)?;

    let models = v
        .get_mut("provider")
        .and_then(|p| p.get_mut(PROVIDER_KEY))
        .and_then(|p| p.get_mut("models"))
        .and_then(|m| m.as_object_mut())
        .ok_or_else(|| anyhow::anyhow!("provider.{PROVIDER_KEY}.models is not an object"))?;

    models.clear();

    for p in presets::load_all() {
        // The model's TRAINED context: the only source for what a preset that
        // names no `ctx-size` will actually load (see `effective_ctx`). A header
        // read, so it costs nothing next to the file it describes; `None` when
        // the GGUF (or ggml-base.dll) can't be read, and then the fallback bites.
        let trained = gguf::read_model_info(Path::new(&p.model))
            .map(|i| i.n_ctx_train)
            .filter(|c| *c > 0);
        let entry = preset_to_opencode_model(&p, trained);
        models.insert(p.id.clone(), entry);
    }

    let serialized = serde_json::to_string_pretty(&v)? + "\n";
    if fs::read_to_string(&path).is_ok_and(|current| current == serialized) {
        return Ok(());
    }
    // OpenCode may never have run on this machine; create its config dir
    // like every other writer does (read_or_create_value already treats the
    // missing FILE as an empty object, so the missing DIR must not fail).
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).with_context(|| format!("create {}", parent.display()))?;
    }
    ini::atomic_write(&path, &serialized).with_context(|| format!("write {}", path.display()))?;
    Ok(())
}

/// The follow-up to every change of the enabled preset set or of a preset's
/// content (a switch, a save, a rename, a delete, a New/Clone, and a server.ini
/// save, which moves the base URL): rebuild opencode.json's model list, but ONLY
/// when it already carries this provider. Returns whether it did. Creating the
/// provider stays the Integrations tab's Save, so a machine that never set
/// OpenCode up is never handed a config it did not ask for.
pub fn follow_presets() -> Result<bool> {
    if !detect_opencode_provider() {
        return Ok(false);
    }
    let cfg = server_cfg::load();
    save_opencode_models(
        &cfg.opencode_base_url_or_default(),
        cfg.opencode_api_key.as_deref(),
    )?;
    Ok(true)
}

/// How opencode.json's model list differs from the enabled preset set, by id:
/// see `drift`. Both empty = in step. A difference is what a file written by a
/// version with the old checkbox list, a hand-edit, or a failed `follow_presets`
/// leaves behind; nothing rewrites it until the next preset change or Save, so
/// the Integrations tab names it instead.
pub fn opencode_drift() -> (Vec<String>, Vec<String>) {
    let enabled: Vec<String> = presets::load_all().into_iter().map(|p| p.id).collect();
    drift(&opencode_model_ids(), &enabled)
}

/// `(missing, extra)`: the enabled presets `listed` lacks, and the entries of
/// `listed` that are not an enabled preset (switched off, renamed or deleted).
fn drift(listed: &[String], enabled: &[String]) -> (Vec<String>, Vec<String>) {
    let missing = enabled
        .iter()
        .filter(|id| !listed.contains(id))
        .cloned()
        .collect();
    let extra = listed
        .iter()
        .filter(|id| !enabled.contains(id))
        .cloned()
        .collect();
    (missing, extra)
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
/// the loaded presets; this stays a pure string builder, no hidden disk IO).
/// `api_key` is the optional API key for proxy/gateway authentication.
pub fn claude_code_env_script(
    base_url: &str,
    api_key: Option<&str>,
    example_id: Option<&str>,
) -> String {
    let example = example_id.unwrap_or("<preset-id>");
    let api_key_val = api_key.unwrap_or("not-needed");
    format!(
        "# Run this in PowerShell before launching Claude Code to use your local llama-server.\r\n\
         $env:ANTHROPIC_BASE_URL = \"{base_url}\"\r\n\
         $env:ANTHROPIC_API_KEY = \"{api_key_val}\"\r\n\
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

fn ensure_provider_section(v: &mut Value, base_url: &str, api_key: Option<&str>) -> Result<()> {
    let obj = v
        .as_object_mut()
        .ok_or_else(|| anyhow::anyhow!("top-level is not an object"))?;

    // Append /v1 for the OpenAI-compatible API endpoint.
    let api_base = format!("{base_url}/v1");

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
                "options": { "baseURL": api_base },
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
    opts_obj.insert("baseURL".to_string(), json!(api_base));
    if let Some(key) = api_key {
        opts_obj.insert("apiKey".to_string(), json!(key));
    } else {
        opts_obj.remove("apiKey");
    }

    lp_obj
        .entry("models".to_string())
        .or_insert_with(|| json!({}));

    Ok(())
}

fn preset_to_opencode_model(p: &presets::Preset, n_ctx_train: Option<u32>) -> Value {
    let name = friendly_model_name(&p.id, &p.model);
    let mut entry = json!({ "name": name });

    if !p.reasoning.is_empty() && p.reasoning != "auto" {
        entry["reasoning"] = json!(p.reasoning == "on");
    }

    let context = effective_ctx(p.ctx_size, p.parallel, n_ctx_train);
    let output = std::cmp::max(context / 4, 32000);
    entry["limit"] = json!({
        "context": context,
        "input": context,
        "output": output,
    });

    entry
}

/// The context ONE request may actually use, which is what an opencode model
/// entry must advertise, since opencode packs prompts up to that ceiling and
/// llama-server rejects whatever exceeds a slot. It is neither `ctx-size` nor the
/// model's trained context; three llama.cpp rules stand between them, and each one
/// is a place this used to lie (it advertised a flat 131072 whenever `ctx-size`
/// was absent):
///
/// 1. **`ctx-size` absent means the model's TRAINED context, not a fixed default.**
///    `common_params::n_ctx = 0 // 0 == context the model was trained with`
///    (common/common.h), and `llama-context.cpp` resolves it: `n_ctx = params.n_ctx
///    == 0 ? hparams.n_ctx_train : params.n_ctx`. A 256k-trained model therefore
///    loads 256k, and advertising 128k under-sells it by half.
/// 2. **…but the context is SPLIT ACROSS SLOTS**: `n_ctx_seq = n_ctx / n_seq_max`,
///    with `n_seq_max = n_parallel` (llama-context.cpp / common.cpp). So `parallel
///    = 2` halves what a single request may ask for; the same 256k model serves
///    128k per request. This is the half that makes the naive "just use the trained
///    context" fix WORSE than the bug: it would promise a context the server
///    refuses.
/// 3. **…unless `parallel` is omitted.** llama-server defaults `n_parallel` to
///    `-1` = auto (common/arg.cpp, server-only), and auto picks 4 slots *and a
///    unified KV cache* (tools/server/server.cpp), under which `n_ctx_seq = n_ctx`,
///    i.e. every slot sees the whole context. Omitting the key is thus the way to
///    get the model's full trained context per request, and pinning `parallel` is
///    what silently divides it.
///
/// Finally the server caps a slot at the trained context
/// (`server-context.cpp` `n_ctx_slot()`: `min(llama_n_ctx_seq, n_ctx_train)`),
/// so a `ctx-size` above what the model was trained for buys nothing here either.
///
/// Two v0.4.0 caps this deliberately does NOT model, because neither has a
/// field to read from: `--kv-unified-per-slot N` (#24124, server-only) lowers
/// the per-slot figure to `min(n_ctx_seq, N)` and, with no `-c`, sizes the pool
/// to `n_parallel * N`; and a preset pinning custom YaRN scaling (`rope-scaling
/// yarn` + a `rope-freq-scale` off the trained one) makes llama.cpp REWRITE
/// `n_ctx_train` to `n_ctx_orig_yarn / freq_scale` at context creation (#28030),
/// which raises the cap this function reads from the header. Add the field
/// first, then the rule, in that order.
fn effective_ctx(ctx_size: Option<i32>, parallel: Option<i32>, n_ctx_train: Option<u32>) -> i64 {
    let trained = n_ctx_train.map(i64::from);
    // Rule 1. `ctx-size = 0` is llama.cpp's own spelling of "from the model", and
    // `Preset::from_keys` keeps it as Some(0); treat it like the absent key.
    let n_ctx = match ctx_size.map(i64::from).filter(|c| *c > 0) {
        Some(c) => c,
        None => match trained {
            Some(t) => t,
            None => return CTX_FALLBACK,
        },
    };
    let n_ctx = pad_256(n_ctx);

    // Rules 2 and 3. A slot gets the whole context under the unified KV cache that
    // auto-parallel turns on, and a `1 / n_seq_max` share otherwise.
    let per_slot = match parallel.filter(|n| *n > 0) {
        None => n_ctx, // auto → unified KV
        Some(n) => pad_256(n_ctx / i64::from(n)),
    };

    // The server's own cap. Only bites when a preset asks for more context than the
    // model was trained on, which llama.cpp honours for the KV cache but not per slot.
    match trained {
        Some(t) => std::cmp::min(per_slot, t),
        None => per_slot,
    }
}

/// `GGML_PAD(x, 256)`: llama.cpp rounds both `n_ctx` and the per-sequence slice
/// up to a 256-token boundary (llama-context.cpp), so a preset asking for 100,000
/// really gets 100,096. Mirrored rather than ignored because the advertised number
/// is compared against by a client that will happily fill it to the last token.
fn pad_256(v: i64) -> i64 {
    (v + 255) & !255
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
        ensure_provider_section(&mut v, "http://localhost:8080", None).unwrap();
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
        ensure_provider_section(&mut v, "http://localhost:8080", Some("sk-old")).unwrap();
        ensure_provider_section(&mut v, "http://0.0.0.0:9090", None).unwrap();
        assert_eq!(
            v["provider"][PROVIDER_KEY]["options"]["baseURL"],
            "http://0.0.0.0:9090/v1"
        );
        // apiKey should be removed when None is passed
        assert!(v["provider"][PROVIDER_KEY]["options"]
            .get("apiKey")
            .is_none());
    }

    #[test]
    fn opencode_model_reasoning_emitted_only_when_not_auto() {
        let mut p = presets::Preset {
            id: "m".into(),
            model: r"C:\models\m.gguf".into(),
            ..Default::default()
        };
        p.reasoning = "auto".into();
        assert!(preset_to_opencode_model(&p, None)
            .get("reasoning")
            .is_none());
        p.reasoning = "on".into();
        assert_eq!(preset_to_opencode_model(&p, None)["reasoning"], json!(true));
        p.reasoning = "off".into();
        assert_eq!(
            preset_to_opencode_model(&p, None)["reasoning"],
            json!(false)
        );
    }

    #[test]
    fn opencode_model_limit_math() {
        let mut p = presets::Preset {
            id: "m".into(),
            model: r"C:\models\m.gguf".into(),
            parallel: Some(1),
            ..Default::default()
        };
        // Output is a quarter of the advertised context…
        p.ctx_size = Some(131_072);
        let e = preset_to_opencode_model(&p, None);
        assert_eq!(e["limit"]["context"], 131_072);
        assert_eq!(e["limit"]["input"], 131_072);
        assert_eq!(e["limit"]["output"], 32_768);
        // …with a 32k floor, so a small context still advertises a usable reply.
        p.ctx_size = Some(8192);
        let e = preset_to_opencode_model(&p, None);
        assert_eq!(e["limit"]["context"], 8192);
        assert_eq!(e["limit"]["output"], 32_000);
    }

    /// The advertised context is what ONE request may use, and llama.cpp derives
    /// that from `ctx-size`, `parallel` AND the model's trained context together.
    /// Each case here is a way the old flat `ctx_size.unwrap_or(131072)` lied.
    #[test]
    fn advertised_context_follows_llama_cpps_slot_math() {
        const TRAINED: Option<u32> = Some(262_144); // Qwen3.6-27B
        const NONE: Option<u32> = None;

        // No ctx-size, no parallel: llama-server auto-parallels (4 slots) with a
        // UNIFIED KV cache, so every slot sees the model's whole trained context.
        assert_eq!(effective_ctx(None, None, TRAINED), 262_144);
        // No ctx-size, parallel pinned: the trained context is now SPLIT. This is
        // the case that made the old hard-coded 131072 look right by accident.
        assert_eq!(effective_ctx(None, Some(2), TRAINED), 131_072);
        assert_eq!(effective_ctx(None, Some(4), TRAINED), 65_536);
        // An explicit ctx-size is split the same way; it is not a per-slot number.
        assert_eq!(effective_ctx(Some(65_536), Some(2), TRAINED), 32_768);
        assert_eq!(effective_ctx(Some(65_536), None, TRAINED), 65_536);
        // ctx-size = 0 is llama.cpp's own "load it from the model".
        assert_eq!(effective_ctx(Some(0), Some(2), TRAINED), 131_072);
        // Asking for more than the model was trained on: the server caps the slot.
        assert_eq!(effective_ctx(Some(1_000_000), Some(1), TRAINED), 262_144);
        // Non-multiples of 256 are padded up, exactly as llama.cpp pads them.
        assert_eq!(effective_ctx(Some(100_000), Some(1), NONE), 100_096);
        // Unreadable GGUF and no ctx-size: nothing to derive from → the fallback.
        assert_eq!(effective_ctx(None, Some(2), NONE), 131_072);
        // …but an explicit ctx-size still works without the header.
        assert_eq!(effective_ctx(Some(32_768), Some(2), NONE), 16_384);
    }

    #[test]
    fn drift_names_both_directions() {
        let s = |v: &[&str]| v.iter().map(|x| (*x).to_string()).collect::<Vec<_>>();
        // In step, whatever the order.
        assert_eq!(drift(&s(&["b", "a"]), &s(&["a", "b"])), (vec![], vec![]));
        // A preset switched on since the last write, and one switched off.
        assert_eq!(
            drift(&s(&["a", "gone"]), &s(&["a", "new"])),
            (s(&["new"]), s(&["gone"]))
        );
        // No provider models at all: everything enabled is missing.
        assert_eq!(drift(&[], &s(&["a"])), (s(&["a"]), vec![]));
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
