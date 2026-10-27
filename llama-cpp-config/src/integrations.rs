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
//!
//! **A preset whose context llama.cpp's fit may cut has NO number at all**
//! (`fit_may_cut_ctx`, the single rule, used by every path that advertises a
//! context: `advertised`). With the fit on and the context automatic, the fit's
//! step 2 shrinks the context at LOAD time until the model fits the free memory
//! (`common/fit.cpp`), so any static figure is only a ceiling the server may never
//! load, and opencode would pack prompts the server rejects. The user's decision:
//! publish no context window for those models rather than a guess.
//!
//! - **opencode.json**: the model entry carries no `limit` object at all, not a
//!   partial one. opencode's v1 config schema makes `limit` optional but, when
//!   present, requires both `context` and `output` (`packages/core/src/v1/config/
//!   provider.ts`), so `limit.output` alone fails validation, and a `limit.input`
//!   without a context would be a number derived from nothing. Without `limit`,
//!   opencode (1.19.4, `packages/opencode/src/provider/provider.ts`) fills
//!   `context: 0, output: 0`: `isOverflow` returns false at `context === 0`
//!   (`session/overflow.ts`), so it never compacts ahead of time; the reply cap
//!   falls back to `OUTPUT_TOKEN_MAX` = 32000 (`provider/transform.ts`
//!   `maxOutputTokens`); and when llama-server rejects a prompt ("exceeds the
//!   available context size", one of `packages/llm/src/provider-error.ts`'s
//!   overflow patterns), the session processor marks it `needsCompaction` and
//!   compacts then. opencode 2.x behaves alike (an unknown window is an infinite
//!   ceiling, compaction on the provider's rejection: `packages/core/src/session/
//!   compaction.ts` `calculateCeiling`). A `limit` an earlier rewrite wrote for
//!   such a model does not linger: every rewrite rebuilds the model entries from
//!   scratch (`fill_models`), while everything outside `provider.llama.cpp.models`
//!   is left as it was.
//! - **Claude Code snippet**: its example is the first enabled preset that HAS a
//!   context (`claude_code_example`); when none has, the example names the model
//!   only, and a comment says why there is no context or compaction line.
//! - The Integrations tab shows the advertised context nowhere but in that
//!   snippet, so the snippet's comment is the whole UI side.

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

/// Context kept free for the client's auto-compaction, as a share of the context
/// plus a fixed amount: the summary the client asks for, and the turn that tripped
/// it, have to fit in what is left. opencode reserves a fixed 20k tokens on its own
/// (`OPENCODE_COMPACTION_BUFFER`, 7.6% of 262k), and Claude Code assumes a 200k
/// window for an id it does not know, so both are told to compact at `compact_at`,
/// or at opencode's own point when that comes earlier.
const COMPACT_RESERVE_PCT: i64 = 15;
const COMPACT_RESERVE_TOKENS: i64 = 10_000;

/// opencode's own compaction reserve: it compacts at `limit.input - reserved`,
/// with `reserved = compaction.reserved ?? min(20000, limit.output)`
/// (`packages/opencode/src/session/overflow.ts`, `COMPACTION_BUFFER`), and
/// `limit.output` is never under 32000 here, so the 20000 is what it subtracts.
const OPENCODE_COMPACTION_BUFFER: i64 = 20_000;

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
/// `base_url` comes from the current server.ini (e.g. "http://127.0.0.1:9931/v1").
/// `api_key` is the optional API key for proxy/gateway authentication.
///
/// A rebuild that comes out identical to the file is not written: every preset
/// save now lands here (`follow_presets`), and opencode.json is the user's file,
/// so it is touched only when its content actually has to change.
pub fn save_opencode_models(base_url: &str, api_key: Option<&str>) -> Result<()> {
    let path = paths::opencode_user_config();
    let mut v = read_or_create_value(&path)?;

    ensure_provider_section(&mut v, base_url, api_key)?;

    let sections = ini::read_all(&paths::presets_ini());
    let entries = presets::load_all()
        .into_iter()
        .map(|p| {
            let context = advertised(&p, fit_on(&sections, &p.id), trained_ctx(&p));
            let entry = preset_to_opencode_model(&p, context);
            (p.id, entry)
        })
        .collect();
    fill_models(&mut v, entries)?;

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

/// Replace the provider's model entries with `entries`, whole: an entry is never
/// merged into what the file held, so a `limit` written for a model whose
/// context is now the fit's (see the module header) disappears with the rewrite.
/// Only `provider.llama.cpp.models` is touched.
fn fill_models(v: &mut Value, entries: Vec<(String, Value)>) -> Result<()> {
    let models = v
        .get_mut("provider")
        .and_then(|p| p.get_mut(PROVIDER_KEY))
        .and_then(|p| p.get_mut("models"))
        .and_then(|m| m.as_object_mut())
        .ok_or_else(|| anyhow::anyhow!("provider.{PROVIDER_KEY}.models is not an object"))?;
    models.clear();
    models.extend(entries);
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
/// Claude Code to use llama-server as a custom model provider. `example` is
/// a preset id to show in the commented model line, with its advertised context,
/// `None` when the fit decides it at load time (`claude_code_example` picks it;
/// the caller already holds the loaded presets, so this stays a pure string
/// builder, no hidden disk IO).
/// `api_key` is the optional API key for proxy/gateway authentication.
///
/// Claude Code does not know a preset id, so it assumes a 200k window and compacts
/// there: `CLAUDE_CODE_MAX_CONTEXT_TOKENS` gives it the real one (it applies on its
/// own to an unrecognized id that does not start with `claude-`), and
/// `CLAUDE_AUTOCOMPACT_PCT_OVERRIDE` moves the compaction to `compact_at` (it takes a
/// percentage, so it follows the example's context like the window does, and it
/// can only lower the default threshold, never raise it). An example without a
/// context gets neither line: a comment says the fit sizes it at load time and
/// where the loaded figure can be read instead.
pub fn claude_code_env_script(
    base_url: &str,
    api_key: Option<&str>,
    example: Option<(&str, Option<i64>)>,
) -> String {
    let api_key_val = api_key.unwrap_or("not-needed");
    let head = format!(
        "# Run this in PowerShell before launching Claude Code to use your local llama-server.\r\n\
         $env:ANTHROPIC_BASE_URL = \"{base_url}\"\r\n\
         $env:ANTHROPIC_API_KEY = \"{api_key_val}\"\r\n"
    );
    let (example_id, example_ctx) = match example {
        Some((id, ctx)) => (id, ctx.map(|c| (c.to_string(), compact_pct(c).to_string()))),
        None => (
            "<preset-id>",
            Some(("<context>".to_string(), "<percent>".to_string())),
        ),
    };
    let body = match example_ctx {
        Some((ctx, pct)) => format!(
            "# Pick a preset id from your presets.ini as the model name, with its context and\r\n\
             # where to compact ({COMPACT_RESERVE_PCT}% + {COMPACT_RESERVE_TOKENS} tokens short of it), e.g.:\r\n\
             # $env:ANTHROPIC_DEFAULT_SONNET_MODEL = \"{example_id}\"\r\n\
             # $env:CLAUDE_CODE_MAX_CONTEXT_TOKENS = \"{ctx}\"\r\n\
             # $env:CLAUDE_AUTOCOMPACT_PCT_OVERRIDE = \"{pct}\"\r\n\
             # A preset whose context llama.cpp's fit sizes at load time has no fixed number:\r\n\
             # name only its model, or copy n_ctx_slot from the server log after it loads.\r\n"
        ),
        None => format!(
            "# Pick a preset id from your presets.ini as the model name, e.g.:\r\n\
             # $env:ANTHROPIC_DEFAULT_SONNET_MODEL = \"{example_id}\"\r\n\
             # No context or compaction line for it: llama.cpp's fit sizes its context at load\r\n\
             # time to fit free memory, so there is no fixed number to give. To set one, copy\r\n\
             # n_ctx_slot from the server log after it loads into CLAUDE_CODE_MAX_CONTEXT_TOKENS.\r\n"
        ),
    };
    format!("{head}{body}# Then launch: claude\r\n")
}

/// The Claude Code snippet's example among `enabled`: the first preset with an
/// advertised context, so the snippet can show the context and compaction lines,
/// else the first preset, without a context (see `claude_code_env_script`).
/// Reads presets.ini (the raw `fit` key, `fit_on`) and the GGUF headers.
pub fn claude_code_example(enabled: &[presets::Preset]) -> Option<(&str, Option<i64>)> {
    let sections = ini::read_all(&paths::presets_ini());
    let mut first = None;
    for p in enabled {
        let ctx = advertised(p, fit_on(&sections, &p.id), trained_ctx(p));
        if ctx.is_some() {
            return Some((p.id.as_str(), ctx));
        }
        first.get_or_insert((p.id.as_str(), None));
    }
    first
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

/// One opencode model entry. `context` is `advertised`'s: `None` (the fit sizes
/// the context at load time) leaves the `limit` object out entirely (module
/// header: opencode then compacts on llama-server's rejection).
fn preset_to_opencode_model(p: &presets::Preset, context: Option<i64>) -> Value {
    let name = friendly_model_name(&p.id, &p.model);
    let mut entry = json!({ "name": name });

    if !p.reasoning.is_empty() && p.reasoning != "auto" {
        entry["reasoning"] = json!(p.reasoning == "on");
    }

    let Some(context) = context else {
        return entry;
    };
    let output = std::cmp::max(context / 4, 32000);
    // opencode compacts at `input` minus its own buffer, so `input` is set to put
    // that point at `compact_at`. Never above `context`: under ~67k tokens the
    // buffer alone already exceeds the reserve.
    let input = std::cmp::min(context, compact_at(context) + OPENCODE_COMPACTION_BUFFER);
    entry["limit"] = json!({
        "context": context,
        "input": input,
        "output": output,
    });

    entry
}

/// Where a client should compact a conversation in a `context`-token window:
/// `COMPACT_RESERVE_PCT` percent plus `COMPACT_RESERVE_TOKENS` short of it.
fn compact_at(context: i64) -> i64 {
    std::cmp::max(0, context * (100 - COMPACT_RESERVE_PCT) / 100 - COMPACT_RESERVE_TOKENS)
}

/// `compact_at` as the percentage Claude Code takes, rounded down so it never
/// compacts later than that; at least 1, the variable's floor.
fn compact_pct(context: i64) -> i64 {
    if context <= 0 {
        return 1;
    }
    std::cmp::max(1, compact_at(context) * 100 / context)
}

/// The model's TRAINED context: the only source for what a preset that names no
/// `ctx-size` will actually load (see `effective_ctx`). A header read, so it costs
/// nothing next to the file it describes; `None` when the GGUF (or ggml-base.dll)
/// can't be read, and then the fallback bites.
fn trained_ctx(p: &presets::Preset) -> Option<u32> {
    gguf::read_model_info(Path::new(&p.model))
        .map(|i| i.n_ctx_train)
        .filter(|c| *c > 0)
}

/// The context one request to `p` may use, as opencode's `limit.context` and
/// Claude Code's `CLAUDE_CODE_MAX_CONTEXT_TOKENS` advertise it, and the one place
/// both derive it from: `effective_ctx`, unless the fit may cut it at load time
/// (`fit_may_cut_ctx`), in which case `None`, no number to publish. `fit_on` is
/// the raw key's verdict (`fit_on`), `n_ctx_train` the GGUF header's.
fn advertised(p: &presets::Preset, fit_on: bool, n_ctx_train: Option<u32>) -> Option<i64> {
    if fit_may_cut_ctx(p, fit_on, n_ctx_train) {
        None
    } else {
        Some(effective_ctx(p, n_ctx_train))
    }
}

/// Whether llama-server runs the fit for preset `id`, as llama.cpp resolves the
/// `fit` key: the section's own, else the `[*]` section's (`common/preset.cpp`
/// cascades the global preset onto every one; the router's own command line
/// never carries `-fit`, `runstate::server_args`), else upstream's default ON
/// (`common_params::fit_params = true`, common/common.h). Read from the RAW
/// section because `Preset::fit` reads an absent key as off, the framework's
/// own default that it always writes; a hand-written section without the key is
/// fit ON for llama-server. Only `is_falsey` values (`off`, `disabled`, `false`,
/// `0`; common/arg.cpp) turn it off; anything else is on, or a launch error.
fn fit_on(sections: &[ini::Section], id: &str) -> bool {
    let key = |sid: &str| {
        sections
            .iter()
            .find(|s| s.id == sid)
            .and_then(|s| s.keys.get("fit"))
            .map(|v| v.trim().to_string())
    };
    match key(id).or_else(|| key("*")) {
        Some(v) => !matches!(v.as_str(), "off" | "disabled" | "false" | "0"),
        None => true,
    }
}

/// `fit_params_min_ctx` as the child ends up with it: `fit-ctx` when the preset
/// names one, else `UINT32_MAX` for an explicit `ctx-size = 0` (common/arg.cpp:
/// "disable context reduction in llama_params_fit if the user explicitly requests
/// the full context size"), else 4096 (common/common.h). `fit-ctx` wins over
/// `ctx-size = 0` because the router renders a preset's options sorted by their
/// first spelling (`common_preset::to_args`, `common_arg::operator<`), so `-c`
/// is parsed before `-fitc`, which overwrites the sentinel. The int32 is read as
/// `uint32_t` upstream, hence the wrapping cast (`fit-ctx = -1` is the sentinel).
fn fit_min_ctx(p: &presets::Preset) -> u32 {
    match p.fit_ctx {
        Some(n) => n as u32,
        None if p.ctx_size == Some(0) => u32::MAX,
        None => 4096,
    }
}

/// Whether llama.cpp's fit may REDUCE `p`'s context at load time, which makes any
/// advertised figure a mere ceiling (module header). Each condition is a line of
/// upstream (v0.7.0):
///
/// 1. **The fit runs**: `if (params.fit_params)` around `common_fit_params`
///    (common/common.cpp `common_init_result`); `fit_on` resolves the key.
/// 2. **…and is not skipped for `split-mode tensor`**: `common_params_fit_impl`
///    throws "not implemented for SPLIT_MODE_TENSOR" before touching anything
///    (common/fit.cpp), and `common_fit_params` only logs the failure.
/// 3. **The context is automatic**: step 2 cuts only `if (n_ctx_auto)`, with
///    `n_ctx_auto = cparams->n_ctx == 0`; otherwise "context size set by user ->
///    no change". So `ctx-size` must be absent or 0…
/// 4. **…and not sized by `kv-unified-per-slot`**: with a per-slot cap, an
///    automatic `ctx-size` and the sentinel unset, tools/server/server.cpp sets
///    `n_ctx = n_parallel * kv_unified_per_slot` BEFORE the model loads, so the
///    fit sees a context set by the user.
/// 5. **The floor is below the trained context**: step 2 cuts only
///    `if (n_ctx_max > n_ctx_min_total)`, i.e. when
///    `n_ctx_train * n_streams > fit_min_ctx * n_streams` (the stream count cancels). `fit_min_ctx` is `UINT32_MAX` for an
///    explicit `ctx-size = 0` ("user has requested full context size -> no
///    change"), so that is excluded here, unless a `fit-ctx` overrides it. An
///    unreadable trained context counts as above the floor.
///
/// Whether it DOES cut depends on the free memory at load (step 1 returns early
/// when everything fits), which no static reading can know, hence "may".
/// `n-gpu-layers` set by hand does not exclude it: step 2 runs before step 3's
/// "n_gpu_layers already set by user, abort", and its context cut stays.
fn fit_may_cut_ctx(p: &presets::Preset, fit_on: bool, n_ctx_train: Option<u32>) -> bool {
    if !fit_on || p.split_mode.trim().eq_ignore_ascii_case("tensor") {
        return false;
    }
    if p.ctx_size.unwrap_or(0) != 0 {
        return false;
    }
    if pool_sized_per_slot(p) {
        return false;
    }
    let min_ctx = fit_min_ctx(p);
    match n_ctx_train {
        Some(t) => t > min_ctx,
        None => min_ctx != u32::MAX,
    }
}

/// Rule 4 of `effective_ctx` and condition 4 of `fit_may_cut_ctx`: whether
/// tools/server/server.cpp sizes the KV pool to `n_parallel * kv-unified-per-slot`
/// (`ctx_pool_auto_sized`: a cap above 0, `n_ctx == 0` and `fit_params_min_ctx
/// != UINT32_MAX`; independent of whether the fit itself runs).
fn pool_sized_per_slot(p: &presets::Preset) -> bool {
    p.kv_unified_per_slot.is_some_and(|n| n > 0)
        && p.ctx_size.unwrap_or(0) == 0
        && fit_min_ctx(p) != u32::MAX
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
/// 3. **…unless the KV cache is unified.** llama-server defaults `n_parallel` to
///    `-1` = auto (common/arg.cpp, server-only), and auto picks 4 slots *and a
///    unified KV cache* (tools/server/server.cpp), under which `n_ctx_seq = n_ctx`,
///    i.e. every slot sees the whole context. Omitting the key is thus the way to
///    get the model's full trained context per request, and pinning `parallel` is
///    what silently divides it, unless the preset also sets `kv-unified = true`.
///    (`kv-unified = false` under auto parallel changes nothing: the server
///    overwrites it.)
/// 4. **`kv-unified-per-slot N` caps a slot at N** (`n_ctx_slot()`), and with
///    `ctx-size` ABSENT it sizes the pool to `n_parallel * N` instead of the
///    trained context (`server.cpp`); an explicit `ctx-size = 0` opts out of
///    that sizing (it sets `fit_params_min_ctx = UINT32_MAX`, which the sizing
///    checks; `pool_sized_per_slot`).
///
/// All of this is the context the preset loads when nothing shrinks it. With
/// the fit free to cut it at load time, the caller publishes nothing instead
/// (`advertised`, `fit_may_cut_ctx`).
///
/// Finally the server caps a slot at the trained context
/// (`server-context.cpp` `n_ctx_slot()`: `min(llama_n_ctx_seq, n_ctx_train)`),
/// so a `ctx-size` above what the model was trained for buys nothing here either.
///
/// One v0.4.0 cap this deliberately does NOT model, because it has no field to
/// read from: a preset pinning custom YaRN scaling (`rope-scaling yarn` + a
/// `rope-freq-scale` off the trained one) makes llama.cpp REWRITE `n_ctx_train`
/// to `n_ctx_orig_yarn / freq_scale` at context creation (#28030), which raises
/// the cap this function reads from the header. Add the field first, then the
/// rule, in that order.
fn effective_ctx(p: &presets::Preset, n_ctx_train: Option<u32>) -> i64 {
    let trained = n_ctx_train.map(i64::from);
    let pinned = p.parallel.filter(|n| *n > 0).map(i64::from);
    // Auto parallel is 4 slots (tools/server/server.cpp).
    let n_parallel = pinned.unwrap_or(4);
    let per_slot_cap = p.kv_unified_per_slot.filter(|n| *n > 0).map(i64::from);
    // Rules 1 and 4. `ctx-size = 0` is llama.cpp's own spelling of "from the
    // model", and `Preset::from_keys` keeps it as Some(0); it loads like the
    // absent key, but only the ABSENT key lets the per-slot cap size the pool
    // (unless a `fit-ctx` overwrites the sentinel `0` set: `pool_sized_per_slot`).
    let n_ctx = match (p.ctx_size.map(i64::from), per_slot_cap) {
        (Some(c), _) if c > 0 => c,
        (_, Some(n)) if pool_sized_per_slot(p) => n_parallel * n,
        _ => match trained {
            Some(t) => t,
            None => return CTX_FALLBACK,
        },
    };
    let n_ctx = pad_256(n_ctx);

    // Rules 2 and 3. A slot gets the whole context under a unified KV cache
    // (auto parallel forces one), and a `1 / n_seq_max` share otherwise.
    let unified = pinned.is_none() || p.kv_unified == Some(true);
    let per_slot = if unified {
        n_ctx
    } else {
        pad_256(n_ctx / n_parallel)
    };
    // Rule 4's cap.
    let per_slot = per_slot_cap.map_or(per_slot, |n| per_slot.min(n));

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
        // An explicit ctx-size: the fit cannot cut it, so a number is advertised.
        let entry = |p: &presets::Preset| preset_to_opencode_model(p, advertised(p, true, None));
        // Output is a quarter of the advertised context…
        p.ctx_size = Some(131_072);
        let e = entry(&p);
        assert_eq!(e["limit"]["context"], 131_072);
        assert_eq!(e["limit"]["output"], 32_768);
        // …and opencode compacts at `input` minus its 20k buffer, i.e. 15% + 10k short.
        assert_eq!(e["limit"]["input"], 101_411 + 20_000);
        p.ctx_size = Some(262_144);
        let e = entry(&p);
        assert_eq!(e["limit"]["input"], 212_822 + 20_000);
        assert_eq!(compact_pct(262_144), 81);
        // …with a 32k floor, so a small context still advertises a usable reply.
        p.ctx_size = Some(8192);
        let e = entry(&p);
        assert_eq!(e["limit"]["context"], 8192);
        assert_eq!(e["limit"]["output"], 32_000);
        // Under ~67k the buffer alone exceeds the reserve: `input` stays `context`.
        assert_eq!(e["limit"]["input"], 8192);
    }

    /// A context the fit may cut gets no `limit` object at all (opencode's v1
    /// schema requires `context` and `output` together), and a rewrite drops the
    /// one an earlier write left, while the rest of the file stays.
    #[test]
    fn autofit_model_has_no_limit_and_a_rewrite_drops_the_stale_one() {
        let p = presets::Preset {
            id: "m".into(),
            model: r"C:\models\m.gguf".into(),
            reasoning: "on".into(),
            ..Default::default()
        };
        let e = preset_to_opencode_model(&p, advertised(&p, true, Some(262_144)));
        assert!(e.get("limit").is_none());
        assert_eq!(e["reasoning"], json!(true));
        // The same preset with the fit off loads its trained context: a number.
        let e = preset_to_opencode_model(&p, advertised(&p, false, Some(262_144)));
        assert_eq!(e["limit"]["context"], 262_144);

        let mut v = json!({
            "theme": "dark",
            "provider": {
                "other": { "models": { "x": { "limit": { "context": 1, "output": 1 } } } },
                "llama.cpp": {
                    "options": { "baseURL": "http://h/v1", "timeout": 5 },
                    "models": {
                        "m": { "name": "M", "limit": { "context": 262_144, "input": 1, "output": 1 } },
                        "gone": { "name": "Gone" }
                    }
                }
            }
        });
        let fresh = preset_to_opencode_model(&p, None);
        fill_models(&mut v, vec![("m".into(), fresh.clone())]).unwrap();
        let models = &v["provider"][PROVIDER_KEY]["models"];
        assert_eq!(models["m"], fresh);
        assert!(models["m"].get("limit").is_none());
        assert!(models.get("gone").is_none());
        // Everything outside this provider's model list is untouched.
        assert_eq!(v["theme"], "dark");
        assert_eq!(v["provider"][PROVIDER_KEY]["options"]["timeout"], 5);
        assert_eq!(v["provider"]["other"]["models"]["x"]["limit"]["context"], 1);
    }

    /// `fit_may_cut_ctx`, one case per upstream condition (see its doc).
    #[test]
    fn fit_may_cut_ctx_follows_upstream() {
        const TRAINED: Option<u32> = Some(262_144);
        let base = presets::Preset::default();
        let cut = |p: &presets::Preset, fit: bool, t: Option<u32>| fit_may_cut_ctx(p, fit, t);

        // The plain autofit preset: fit on, no ctx-size.
        assert!(cut(&base, true, TRAINED));
        // 1. Fit off: the context is exactly what effective_ctx derives.
        assert!(!cut(&base, false, TRAINED));
        // 2. split-mode tensor: the fit throws before doing anything.
        let tensor = presets::Preset { split_mode: "tensor".into(), ..Default::default() };
        assert!(!cut(&tensor, true, TRAINED));
        let layer = presets::Preset { split_mode: "layer".into(), ..Default::default() };
        assert!(cut(&layer, true, TRAINED));
        // 3. An explicit ctx-size is "set by user -> no change".
        let explicit = presets::Preset { ctx_size: Some(65_536), ..Default::default() };
        assert!(!cut(&explicit, true, TRAINED));
        // 3/5. ctx-size = 0 is automatic but sets the full-context sentinel...
        let zero = presets::Preset { ctx_size: Some(0), ..Default::default() };
        assert!(!cut(&zero, true, TRAINED));
        assert!(!cut(&zero, true, None));
        // ...which a fit-ctx overwrites (`-fitc` is parsed after `-c`).
        let zero_floor = presets::Preset { ctx_size: Some(0), fit_ctx: Some(8192), ..Default::default() };
        assert!(cut(&zero_floor, true, TRAINED));
        // fit-ctx = -1 is the sentinel itself.
        let sentinel = presets::Preset { fit_ctx: Some(-1), ..Default::default() };
        assert!(!cut(&sentinel, true, TRAINED));
        // 4. kv-unified-per-slot with ctx-size absent sizes the pool first.
        let per_slot = presets::Preset { kv_unified_per_slot: Some(32_768), ..Default::default() };
        assert!(!cut(&per_slot, true, TRAINED));
        // Under ctx-size = 0 the pool is not sized, but the sentinel stops the cut.
        let per_slot_zero = presets::Preset { ctx_size: Some(0), kv_unified_per_slot: Some(32_768), ..Default::default() };
        assert!(!cut(&per_slot_zero, true, TRAINED));
        // ...and with a fit-ctx beside ctx-size = 0 the pool is sized again.
        let per_slot_floor = presets::Preset {
            ctx_size: Some(0),
            fit_ctx: Some(8192),
            kv_unified_per_slot: Some(32_768),
            ..Default::default()
        };
        assert!(!cut(&per_slot_floor, true, TRAINED));
        // 5. A trained context at or below the floor leaves nothing to cut.
        assert!(!cut(&base, true, Some(4096)));
        assert!(cut(&base, true, Some(4097)));
        let floor = presets::Preset { fit_ctx: Some(300_000), ..Default::default() };
        assert!(!cut(&floor, true, TRAINED));
        // An unreadable header counts as above the floor.
        assert!(cut(&base, true, None));
        // n-gpu-layers by hand does not stop the context cut (step 2 runs first).
        let ngl = presets::Preset { n_gpu_layers: Some(99), ..Default::default() };
        assert!(cut(&ngl, true, TRAINED));
    }

    /// The pool sizing follows the same sentinel the fit reads.
    #[test]
    fn per_slot_pool_sizing_honours_a_fit_ctx_beside_ctx_size_zero() {
        let p = presets::Preset {
            ctx_size: Some(0),
            parallel: Some(2),
            fit_ctx: Some(8192),
            kv_unified_per_slot: Some(40_000),
            ..Default::default()
        };
        // Sized to 2 * 40000 (padded), each slot capped at 40000.
        assert_eq!(effective_ctx(&p, Some(262_144)), 40_000);
    }

    /// The `fit` key as llama-server resolves it: own key, `[*]`, default on.
    #[test]
    fn fit_on_resolves_like_llama_cpp() {
        let sec = |id: &str, fit: Option<&str>| ini::Section {
            id: id.into(),
            keys: fit.map(|v| [("fit".to_string(), v.to_string())].into()).unwrap_or_default(),
            comments: Vec::new(),
        };
        // Absent everywhere: upstream's default, on.
        assert!(fit_on(&[sec("a", None)], "a"));
        assert!(fit_on(&[], "a"));
        // The section's own key, any falsey spelling.
        for off in ["off", "disabled", "false", "0", " off "] {
            assert!(!fit_on(&[sec("a", Some(off))], "a"), "{off}");
        }
        assert!(fit_on(&[sec("a", Some("on"))], "a"));
        assert!(fit_on(&[sec("a", Some("1"))], "a"));
        // `[*]` applies to a section without the key, and loses to one with it.
        assert!(!fit_on(&[sec("*", Some("off")), sec("a", None)], "a"));
        assert!(fit_on(&[sec("*", Some("off")), sec("a", Some("on"))], "a"));
    }

    #[test]
    fn claude_snippet_without_a_context_names_the_model_only() {
        let with = claude_code_env_script("http://h", None, Some(("m", Some(262_144))));
        assert!(with.contains("CLAUDE_CODE_MAX_CONTEXT_TOKENS = \"262144\""));
        assert!(with.contains("CLAUDE_AUTOCOMPACT_PCT_OVERRIDE = \"81\""));
        let without = claude_code_env_script("http://h", None, Some(("m", None)));
        assert!(without.contains("ANTHROPIC_DEFAULT_SONNET_MODEL = \"m\""));
        assert!(!without.contains("$env:CLAUDE_CODE_MAX_CONTEXT_TOKENS"));
        assert!(!without.contains("$env:CLAUDE_AUTOCOMPACT_PCT_OVERRIDE"));
        assert!(without.contains("fit sizes its context at load"));
        assert!(without.ends_with("# Then launch: claude\r\n"));
        // No preset at all: placeholders, both lines.
        let none = claude_code_env_script("http://h", None, None);
        assert!(none.contains("\"<context>\"") && none.contains("\"<percent>\""));
    }

    /// The advertised context is what ONE request may use, and llama.cpp derives
    /// that from `ctx-size`, `parallel` AND the model's trained context together.
    /// Each case here is a way the old flat `ctx_size.unwrap_or(131072)` lied.
    #[test]
    fn advertised_context_follows_llama_cpps_slot_math() {
        const TRAINED: Option<u32> = Some(262_144); // Qwen3.6-27B
        const NONE: Option<u32> = None;
        let effective_ctx = |ctx_size, parallel, trained| {
            effective_ctx(
                &presets::Preset {
                    ctx_size,
                    parallel,
                    ..Default::default()
                },
                trained,
            )
        };

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

    /// Rules 3 and 4: the two KV keys that change the per-slot share.
    #[test]
    fn advertised_context_follows_the_kv_unified_keys() {
        const TRAINED: Option<u32> = Some(262_144);
        let ctx = |ctx_size, parallel, kv_unified, per_slot| {
            effective_ctx(
                &presets::Preset {
                    ctx_size,
                    parallel,
                    kv_unified,
                    kv_unified_per_slot: per_slot,
                    ..Default::default()
                },
                TRAINED,
            )
        };
        // Unified KV with parallel pinned: every slot sees the whole context.
        assert_eq!(ctx(Some(65_536), Some(4), Some(true), None), 65_536);
        // Explicitly off is the pinned-parallel default: split.
        assert_eq!(ctx(Some(65_536), Some(4), Some(false), None), 16_384);
        // Off under auto parallel is overwritten by the server: still unified.
        assert_eq!(ctx(None, None, Some(false), None), 262_144);
        // The per-slot cap bites on a unified pool...
        assert_eq!(
            ctx(Some(131_072), Some(2), Some(true), Some(32_768)),
            32_768
        );
        // ...and is a no-op when the split share is already smaller.
        assert_eq!(ctx(Some(65_536), Some(4), None, Some(32_768)), 16_384);
        // ctx-size absent: the pool is sized to parallel * N (auto = 4 slots),
        // so each slot gets N (the cap applies after the padding).
        assert_eq!(ctx(None, Some(2), None, Some(40_000)), 40_000);
        assert_eq!(ctx(None, None, None, Some(32_768)), 32_768);
        // An explicit ctx-size 0 opts out of that sizing: trained context,
        // still capped per slot.
        assert_eq!(ctx(Some(0), Some(2), None, Some(200_000)), 131_072);
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
            friendly_model_name("id", &crate::paths::os_path(r"C:\m\gemma-4-12b_q6.gguf")),
            "Gemma 4 12b Q6"
        );
        // Caps at 6 words.
        assert_eq!(
            friendly_model_name("id", &crate::paths::os_path(r"C:\m\a-b-c-d-e-f-g-h.gguf")),
            "A B C D E F"
        );
        // No usable stem falls back to the preset id.
        assert_eq!(friendly_model_name("fallback-id", ""), "Fallback Id");
    }
}
