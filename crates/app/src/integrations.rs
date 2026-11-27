//! Provider & model integration for opencode and Claude Code.
//!
//! Manages the `provider.llama.cpp` section in opencode.json, auto-generating
//! model entries from models.ini. Claude Code does not support per-file
//! custom providers natively; an env-var snippet is generated for the GUI's
//! Claude Code card instead (copy-paste only, never written to disk).
//!
//! The model list is not a selection of its own: it is every ENABLED model profile
//! (`model_profiles::load_all`, i.e. models.ini), the same set llama-server serves,
//! so switching a model profile off in the Models tab is also how it leaves OpenCode.
//! It used to be a second checkbox list here, which could offer OpenCode a model
//! llama-server had no model profile for, or hide one it did.
//!
//! **opencode.json is written by the Integrations tab's Save and nothing else**:
//! no model profile change, server.ini save or CLI command touches it, so a model profile can
//! be tuned without its OpenCode entry moving under a running session. The tab's
//! status line names what is out of step, and Save first shows the change per
//! model (`opencode_plan`), each with a tick: an unticked model's entry is left
//! exactly as the file holds it (`save_opencode_models`'s `keep`). A machine
//! that never set OpenCode up is never written to unless its user saves.
//!
//! The one non-mechanical field is a model's `limit.context`, and it is not a
//! copy of the model profile's `ctx-size`: opencode fills a prompt to whatever ceiling it
//! is given, so the number has to be the context ONE request can actually use,
//! which llama.cpp derives from `ctx-size`, `parallel` and the model's trained
//! context TOGETHER. The whole derivation, and the three upstream rules behind it,
//! lives in `effective_ctx`; it is the reason this module reads GGUF headers.
//!
//! **A model profile whose context llama.cpp's fit may cut has NO number at all**
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
//! - **Claude Code snippet**: its example is the first enabled model profile that HAS a
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
use crate::model_profiles;
use crate::paths;
use crate::server_control::{self, ConfigFile};
use crate::strings;

const PROVIDER_KEY: &str = "llama.cpp";
const PROVIDER_NPM: &str = "@ai-sdk/openai-compatible";
const PROVIDER_NAME: &str = "llama-server (local)";

/// Advertised context when the model's trained context can't be read (no
/// `ggml-base.dll` beside the app, or an unreadable GGUF) AND the model profile names no
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

/// Returns the model profile IDs currently registered as models in opencode.json.
#[cfg(any(test, feature = "test-support"))]
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

/// The API key server.ini gives the clients (opencode.json's provider, the
/// Claude Code snippet).
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum ApiKey<'a> {
    /// server.ini sets none.
    Unset,
    Key(&'a str),
    /// The manager withheld it from this caller (no `configure`): the value
    /// is unknown, so opencode.json's `apiKey` is left as the file has it and
    /// the snippet shows a placeholder.
    Withheld,
}

/// One field of an opencode.json entry that Save would change: its dotted path
/// (`limit.context`), the value the file holds and the value Save writes, each
/// `None` when absent. The preview masks the API key's values.
#[derive(Debug, Clone, PartialEq)]
pub struct FieldDiff {
    pub path: String,
    pub old: Option<String>,
    pub new: Option<String>,
}

/// What Save would do to one model entry.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum ChangeKind {
    /// An enabled model profile the file does not list.
    Add,
    /// A listed model that is no enabled model profile (switched off, renamed, deleted).
    Remove,
    /// A listed enabled model profile whose entry would change.
    Update,
}

#[derive(Debug, Clone, PartialEq)]
pub struct ModelChange {
    pub id: String,
    pub kind: ChangeKind,
    pub fields: Vec<FieldDiff>,
}

/// Everything the Integrations tab's Save would change in opencode.json: the
/// provider's own fields (created whole when absent; base URL and API key follow
/// server.ini) and one change per model whose entry differs. A model not listed
/// here is already what Save would write.
#[derive(Debug, Clone, PartialEq)]
pub struct OpencodePlan {
    /// The file already carries the provider.
    pub provider_active: bool,
    pub provider: Vec<FieldDiff>,
    pub models: Vec<ModelChange>,
}

impl OpencodePlan {
    /// Save would write nothing.
    pub fn is_empty(&self) -> bool {
        self.provider.is_empty() && self.models.is_empty()
    }
}

/// Compares opencode.json with what Save would write (see `OpencodePlan`).
/// Read-only. `base_url` and `api_key` as for `save_opencode_models`.
pub fn opencode_plan(base_url: &str, api_key: ApiKey) -> Result<OpencodePlan> {
    let current = read_or_create_value(&paths::opencode_user_config())?;
    let mut target = current.clone();
    ensure_provider_section(&mut target, base_url, api_key)?;
    let provider_active = provider_of(&current).is_some();

    let without_models = |v: &Value| {
        let mut p = provider_of(v).cloned().unwrap_or(Value::Null);
        if let Some(o) = p.as_object_mut() {
            o.remove("models");
        }
        p
    };
    let mut provider = Vec::new();
    leaf_diff(
        "",
        &without_models(&current),
        &without_models(&target),
        &mut provider,
    );

    let old = current_models(&current);
    let desired = desired_models();
    let mut models = Vec::new();
    for (id, new) in &desired {
        let (kind, before) = match old.get(id) {
            None => (ChangeKind::Add, Value::Null),
            Some(o) if o != new => (ChangeKind::Update, o.clone()),
            Some(_) => continue,
        };
        let mut fields = Vec::new();
        leaf_diff("", &before, new, &mut fields);
        models.push(ModelChange {
            id: id.clone(),
            kind,
            fields,
        });
    }
    for id in old
        .keys()
        .filter(|id| !desired.iter().any(|(d, _)| d == *id))
    {
        models.push(ModelChange {
            id: id.clone(),
            kind: ChangeKind::Remove,
            fields: Vec::new(),
        });
    }
    Ok(OpencodePlan {
        provider_active,
        provider,
        models,
    })
}

/// Writes the provider entry (created when absent) and its models list: one
/// entry per ENABLED model profile (see the module header), except the ids in `keep`,
/// whose entry is left exactly as the file holds it: kept when listed, still
/// absent when not (the preview's unticked rows, `opencode_plan`).
/// `base_url` comes from the current server.ini (e.g. "http://127.0.0.1:9931/v1").
/// `api_key` is the optional API key for proxy/gateway authentication.
///
/// A rebuild that comes out identical to the file is not written: opencode.json
/// is the user's file, so it is touched only when its content has to change.
pub fn save_opencode_models(base_url: &str, api_key: ApiKey, keep: &[String]) -> Result<()> {
    let path = paths::opencode_user_config();
    let mut v = read_or_create_value(&path)?;
    let current = current_models(&v);

    ensure_provider_section(&mut v, base_url, api_key)?;
    fill_models(&mut v, merge_models(&current, desired_models(), keep))?;

    let serialized = serde_json::to_string_pretty(&v)? + "\n";
    if fs::read_to_string(&path).is_ok_and(|current| current == serialized) {
        return Ok(());
    }
    // OpenCode may never have run on this machine; create its config dir
    // like every other writer does (read_or_create_value already treats the
    // missing FILE as an empty object, so the missing DIR must not fail).
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)
            .with_context(|| strings::fill("create-failed", &[&parent.display()]))?;
    }
    ini::atomic_write(&path, &serialized)
        .with_context(|| strings::fill("write-failed", &[&path.display()]))?;
    Ok(())
}

/// The entries Save writes, one per ENABLED model profile, in models.ini order.
fn desired_models() -> Vec<(String, Value)> {
    let sections = server_control::read_sections(ConfigFile::Models);
    model_profiles::load_all()
        .into_iter()
        .map(|p| {
            let context = advertised(&p, fit_on(&sections, &p.id), trained_ctx(&p));
            let entry = model_profile_to_opencode_model(&p, context);
            (p.id, entry)
        })
        .collect()
}

/// `desired`, except that every id in `keep` stays as `current` has it: its old
/// entry when it has one (in the desired position, or after the others when it is
/// no enabled model profile any more), nothing when it has none.
fn merge_models(
    current: &serde_json::Map<String, Value>,
    desired: Vec<(String, Value)>,
    keep: &[String],
) -> Vec<(String, Value)> {
    let kept = |id: &String| keep.contains(id);
    let mut out: Vec<(String, Value)> = Vec::new();
    for (id, entry) in &desired {
        if !kept(id) {
            out.push((id.clone(), entry.clone()));
        } else if let Some(old) = current.get(id) {
            out.push((id.clone(), old.clone()));
        }
    }
    for (id, old) in current {
        if kept(id) && !desired.iter().any(|(d, _)| d == id) {
            out.push((id.clone(), old.clone()));
        }
    }
    out
}

fn provider_of(v: &Value) -> Option<&Value> {
    v.get("provider").and_then(|p| p.get(PROVIDER_KEY))
}

/// The provider's model entries as the file holds them (empty without a provider).
fn current_models(v: &Value) -> serde_json::Map<String, Value> {
    provider_of(v)
        .and_then(|p| p.get("models"))
        .and_then(|m| m.as_object())
        .cloned()
        .unwrap_or_default()
}

/// The leaves where `old` and `new` differ, as dotted paths under `prefix`. An
/// object on either side is walked key by key, the missing side read as absent.
fn leaf_diff(prefix: &str, old: &Value, new: &Value, out: &mut Vec<FieldDiff>) {
    if old.is_object() || new.is_object() {
        let empty = serde_json::Map::new();
        let (o, n) = (
            old.as_object().unwrap_or(&empty),
            new.as_object().unwrap_or(&empty),
        );
        let keys = o.keys().chain(n.keys().filter(|k| !o.contains_key(*k)));
        for k in keys {
            let path = if prefix.is_empty() {
                k.clone()
            } else {
                format!("{prefix}.{k}")
            };
            let null = Value::Null;
            leaf_diff(
                &path,
                o.get(k).unwrap_or(&null),
                n.get(k).unwrap_or(&null),
                out,
            );
        }
        return;
    }
    if old != new {
        out.push(FieldDiff {
            path: prefix.to_string(),
            old: render(old),
            new: render(new),
        });
    }
}

/// A leaf as the preview shows it: a string bare, anything else as JSON.
fn render(v: &Value) -> Option<String> {
    match v {
        Value::Null => None,
        Value::String(s) => Some(s.clone()),
        other => Some(other.to_string()),
    }
}

/// `what`, a place in opencode.json, holds something else than the object it must.
fn not_an_object(what: &str) -> anyhow::Error {
    anyhow::anyhow!(strings::fill("not-an-object", &[&what]))
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
        .ok_or_else(|| not_an_object(&format!("provider.{PROVIDER_KEY}.models")))?;
    models.clear();
    models.extend(entries);
    Ok(())
}

pub fn detect_opencode_provider() -> bool {
    read_opencode()
        .and_then(|v| v.get("provider").and_then(|p| p.get(PROVIDER_KEY)).cloned())
        .is_some()
}

// ── Claude Code ────────────────────────────────────────────────────────

/// Generates a snippet, for this OS's shell, that sets the env vars needed for
/// Claude Code to use llama-server as a custom model provider. `example` is
/// a model profile id to show in the commented model line, with its advertised context,
/// `None` when the fit decides it at load time (`claude_code_example` picks it;
/// the caller already holds the loaded model profiles, so this stays a pure string
/// builder, no hidden disk IO).
/// `api_key` is the optional API key for proxy/gateway authentication.
///
/// Claude Code does not know a model profile id, so it assumes a 200k window and compacts
/// there: `CLAUDE_CODE_MAX_CONTEXT_TOKENS` gives it the real one (it applies on its
/// own to an unrecognized id that does not start with `claude-`), and
/// `CLAUDE_AUTOCOMPACT_PCT_OVERRIDE` moves the compaction to `compact_at` (it takes a
/// percentage, so it follows the example's context like the window does, and it
/// can only lower the default threshold, never raise it). An example without a
/// context gets neither line: a comment says the fit sizes it at load time and
/// where the loaded figure can be read instead.
pub fn claude_code_env_script(
    base_url: &str,
    api_key: ApiKey,
    example: Option<(&str, Option<i64>)>,
) -> String {
    let shell = if cfg!(windows) {
        Shell::PowerShell
    } else {
        Shell::Posix
    };
    env_script(shell, base_url, api_key, example)
}

/// The shell a Claude Code snippet is written for.
#[derive(Clone, Copy)]
enum Shell {
    PowerShell,
    Posix,
}

impl Shell {
    fn eol(self) -> &'static str {
        match self {
            Shell::PowerShell => "\r\n",
            Shell::Posix => "\n",
        }
    }

    fn set(self, var: &str, value: &str) -> String {
        match self {
            Shell::PowerShell => format!("$env:{var} = \"{value}\""),
            Shell::Posix => format!("export {var}=\"{value}\""),
        }
    }
}

fn env_script(
    shell: Shell,
    base_url: &str,
    api_key: ApiKey,
    example: Option<(&str, Option<i64>)>,
) -> String {
    let (example_id, example_ctx) = match example {
        Some((id, ctx)) => (id, ctx.map(|c| (c.to_string(), compact_pct(c).to_string()))),
        None => (
            "<model-profile-id>",
            Some(("<context>".to_string(), "<percent>".to_string())),
        ),
    };
    let comment = |text: String| text.lines().map(|l| format!("# {l}")).collect::<Vec<_>>();
    let mut out = comment(crate::strings::t("claude-snippet-run").to_string());
    out.push(shell.set("ANTHROPIC_BASE_URL", base_url));
    let key = match api_key {
        ApiKey::Key(k) => k,
        ApiKey::Unset => "not-needed",
        ApiKey::Withheld => "<api-key>",
    };
    out.push(shell.set("ANTHROPIC_API_KEY", key));
    let model = format!(
        "# {}",
        shell.set("ANTHROPIC_DEFAULT_SONNET_MODEL", example_id)
    );
    match example_ctx {
        Some((ctx, pct)) => {
            out.extend(comment(crate::strings::fill(
                "claude-snippet-pick-with-context",
                &[&COMPACT_RESERVE_PCT, &COMPACT_RESERVE_TOKENS],
            )));
            out.push(model);
            out.push(format!(
                "# {}",
                shell.set("CLAUDE_CODE_MAX_CONTEXT_TOKENS", &ctx)
            ));
            out.push(format!(
                "# {}",
                shell.set("CLAUDE_AUTOCOMPACT_PCT_OVERRIDE", &pct)
            ));
            out.extend(comment(
                crate::strings::t("claude-snippet-fit-note").to_string(),
            ));
        }
        None => {
            out.extend(comment(
                crate::strings::t("claude-snippet-pick").to_string(),
            ));
            out.push(model);
            out.extend(comment(
                crate::strings::t("claude-snippet-no-context").to_string(),
            ));
        }
    }
    out.extend(comment(
        crate::strings::t("claude-snippet-launch").to_string(),
    ));
    let eol = shell.eol();
    out.iter().map(|l| format!("{l}{eol}")).collect()
}

/// The Claude Code snippet's example among `enabled`: the first model profile with an
/// advertised context, so the snippet can show the context and compaction lines,
/// else the first model profile, without a context (see `claude_code_env_script`).
/// Reads models.ini (the raw `fit` key, `fit_on`) and the GGUF headers.
pub fn claude_code_example(
    enabled: &[model_profiles::ModelProfile],
) -> Option<(&str, Option<i64>)> {
    let sections = server_control::read_sections(ConfigFile::Models);
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
        let txt = fs::read_to_string(path).with_context(|| strings::t("read-existing-config"))?;
        if txt.trim().is_empty() {
            Ok(Value::Object(Default::default()))
        } else {
            serde_json::from_str(&txt).with_context(|| strings::t("parse-existing-config"))
        }
    } else {
        Ok(Value::Object(Default::default()))
    }
}

fn ensure_provider_section(v: &mut Value, base_url: &str, api_key: ApiKey) -> Result<()> {
    let obj = v
        .as_object_mut()
        .ok_or_else(|| anyhow::anyhow!(strings::t("top-level-not-an-object")))?;

    // Append /v1 for the OpenAI-compatible API endpoint.
    let api_base = format!("{base_url}/v1");

    let provider = obj
        .entry("provider".to_string())
        .or_insert_with(|| json!({}));
    let provider_obj = provider
        .as_object_mut()
        .ok_or_else(|| not_an_object("provider"))?;

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
        .ok_or_else(|| not_an_object("provider.llama.cpp"))?;

    lp_obj.insert("npm".to_string(), json!(PROVIDER_NPM));
    lp_obj.insert("name".to_string(), json!(PROVIDER_NAME));

    let opts = lp_obj
        .entry("options".to_string())
        .or_insert_with(|| json!({}));
    let opts_obj = opts
        .as_object_mut()
        .ok_or_else(|| not_an_object("options"))?;
    opts_obj.insert("baseURL".to_string(), json!(api_base));
    match api_key {
        ApiKey::Key(key) => {
            opts_obj.insert("apiKey".to_string(), json!(key));
        }
        ApiKey::Unset => {
            opts_obj.remove("apiKey");
        }
        ApiKey::Withheld => {}
    }

    lp_obj
        .entry("models".to_string())
        .or_insert_with(|| json!({}));

    Ok(())
}

/// One opencode model entry. `context` is `advertised`'s: `None` (the fit sizes
/// the context at load time) leaves the `limit` object out entirely (module
/// header: opencode then compacts on llama-server's rejection).
fn model_profile_to_opencode_model(
    p: &model_profiles::ModelProfile,
    context: Option<i64>,
) -> Value {
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
    std::cmp::max(
        0,
        context * (100 - COMPACT_RESERVE_PCT) / 100 - COMPACT_RESERVE_TOKENS,
    )
}

/// `compact_at` as the percentage Claude Code takes, rounded down so it never
/// compacts later than that; at least 1, the variable's floor.
fn compact_pct(context: i64) -> i64 {
    if context <= 0 {
        return 1;
    }
    std::cmp::max(1, compact_at(context) * 100 / context)
}

/// The model's TRAINED context: the only source for what a model profile that names no
/// `ctx-size` will actually load (see `effective_ctx`). A header read, so it costs
/// nothing next to the file it describes; `None` when the GGUF (or ggml-base.dll)
/// can't be read, and then the fallback bites.
fn trained_ctx(p: &model_profiles::ModelProfile) -> Option<u32> {
    gguf::read_model_info(Path::new(&p.model))
        .map(|i| i.n_ctx_train)
        .filter(|c| *c > 0)
}

/// The context one request to `p` may use, as opencode's `limit.context` and
/// Claude Code's `CLAUDE_CODE_MAX_CONTEXT_TOKENS` advertise it, and the one place
/// both derive it from: `effective_ctx`, unless the fit may cut it at load time
/// (`fit_may_cut_ctx`), in which case `None`, no number to publish. `fit_on` is
/// the raw key's verdict (`fit_on`), `n_ctx_train` the GGUF header's.
fn advertised(
    p: &model_profiles::ModelProfile,
    fit_on: bool,
    n_ctx_train: Option<u32>,
) -> Option<i64> {
    if fit_may_cut_ctx(p, fit_on, n_ctx_train) {
        None
    } else {
        Some(effective_ctx(p, n_ctx_train))
    }
}

/// Whether llama-server runs the fit for model profile `id`, as llama.cpp resolves the
/// `fit` key: the section's own, else the `[*]` section's (`common/preset.cpp`
/// cascades the global model profile onto every one; the router's own command line
/// never carries `-fit`, `pai_core::launch::server_args`), else upstream's default ON
/// (`common_params::fit_params = true`, common/common.h). Read from the RAW
/// section because `ModelProfile::fit` reads an absent key as off, the framework's
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

/// `fit_params_min_ctx` as the child ends up with it: `fit-ctx` when the model profile
/// names one, else `UINT32_MAX` for an explicit `ctx-size = 0` (common/arg.cpp:
/// "disable context reduction in llama_params_fit if the user explicitly requests
/// the full context size"), else 4096 (common/common.h). `fit-ctx` wins over
/// `ctx-size = 0` because the router renders a model profile's options sorted by their
/// first spelling (`common_preset::to_args`, `common_arg::operator<`), so `-c`
/// is parsed before `-fitc`, which overwrites the sentinel. The int32 is read as
/// `uint32_t` upstream, hence the wrapping cast (`fit-ctx = -1` is the sentinel).
fn fit_min_ctx(p: &model_profiles::ModelProfile) -> u32 {
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
fn fit_may_cut_ctx(
    p: &model_profiles::ModelProfile,
    fit_on: bool,
    n_ctx_train: Option<u32>,
) -> bool {
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
fn pool_sized_per_slot(p: &model_profiles::ModelProfile) -> bool {
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
///    what silently divides it, unless the model profile also sets `kv-unified = true`.
///    (`kv-unified = false` under auto parallel changes nothing: the server
///    overwrites it.)
/// 4. **`kv-unified-per-slot N` caps a slot at N** (`n_ctx_slot()`), and with
///    `ctx-size` ABSENT it sizes the pool to `n_parallel * N` instead of the
///    trained context (`server.cpp`); an explicit `ctx-size = 0` opts out of
///    that sizing (it sets `fit_params_min_ctx = UINT32_MAX`, which the sizing
///    checks; `pool_sized_per_slot`).
///
/// All of this is the context the model profile loads when nothing shrinks it. With
/// the fit free to cut it at load time, the caller publishes nothing instead
/// (`advertised`, `fit_may_cut_ctx`).
///
/// Finally the server caps a slot at the trained context
/// (`server-context.cpp` `n_ctx_slot()`: `min(llama_n_ctx_seq, n_ctx_train)`),
/// so a `ctx-size` above what the model was trained for buys nothing here either.
///
/// One v0.4.0 cap this deliberately does NOT model, because it has no field to
/// read from: a model profile pinning custom YaRN scaling (`rope-scaling yarn` + a
/// `rope-freq-scale` off the trained one) makes llama.cpp REWRITE `n_ctx_train`
/// to `n_ctx_orig_yarn / freq_scale` at context creation (#28030), which raises
/// the cap this function reads from the header. Add the field first, then the
/// rule, in that order.
fn effective_ctx(p: &model_profiles::ModelProfile, n_ctx_train: Option<u32>) -> i64 {
    let trained = n_ctx_train.map(i64::from);
    let pinned = p.parallel.filter(|n| *n > 0).map(i64::from);
    // Auto parallel is 4 slots (tools/server/server.cpp).
    let n_parallel = pinned.unwrap_or(4);
    let per_slot_cap = p.kv_unified_per_slot.filter(|n| *n > 0).map(i64::from);
    // Rules 1 and 4. `ctx-size = 0` is llama.cpp's own spelling of "from the
    // model", and `ModelProfile::from_keys` keeps it as Some(0); it loads like the
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

    // The server's own cap. Only bites when a model profile asks for more context than the
    // model was trained on, which llama.cpp honours for the KV cache but not per slot.
    match trained {
        Some(t) => std::cmp::min(per_slot, t),
        None => per_slot,
    }
}

/// `GGML_PAD(x, 256)`: llama.cpp rounds both `n_ctx` and the per-sequence slice
/// up to a 256-token boundary (llama-context.cpp), so a model profile asking for 100,000
/// really gets 100,096. Mirrored rather than ignored because the advertised number
/// is compared against by a client that will happily fill it to the last token.
fn pad_256(v: i64) -> i64 {
    (v + 255) & !255
}

/// Human-readable title-cased label derived from the model filename,
/// falling back to the model profile id. Shared by the Integrations tab and
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

/// The API key withheld by the manager (the caller may not configure): the
/// Integrations tab's preview and Save leave opencode.json's `apiKey` exactly
/// as the file has it, while an unset key still removes it. Runs in
/// `data_root_e2e` (`src/tests/mod.rs`): it writes opencode.json under the
/// redirected tree.
#[cfg(test)]
pub(crate) fn api_key_withheld_e2e() {
    let dir = tempfile::tempdir().expect("tempdir");
    crate::paths::redirect_for_tests(dir.path());
    let path = paths::opencode_user_config();
    assert!(path.starts_with(dir.path()), "{}", path.display());
    fs::create_dir_all(path.parent().unwrap()).expect("opencode dir");
    let base = "http://127.0.0.1:8099";
    let seed = || {
        let v = json!({
            "$schema": "https://opencode.ai/config.json",
            "provider": {
                PROVIDER_KEY: {
                    "npm": PROVIDER_NPM,
                    "name": PROVIDER_NAME,
                    "options": { "baseURL": format!("{base}/v1"), "apiKey": "sk-user-0000" },
                    "models": {}
                }
            }
        });
        let text = serde_json::to_string_pretty(&v).unwrap() + "\n";
        fs::write(&path, &text).expect("seed opencode.json");
        text
    };
    let api_key_of = || {
        let v: Value = serde_json::from_str(&fs::read_to_string(&path).unwrap()).unwrap();
        v["provider"][PROVIDER_KEY]["options"]
            .get("apiKey")
            .cloned()
    };

    // Withheld: the plan names no change to the key (nothing at all here).
    let seeded = seed();
    let plan = opencode_plan(base, ApiKey::Withheld).expect("plan");
    assert!(
        !plan.provider.iter().any(|d| d.path.contains("apiKey")),
        "a withheld key is no change: {:?}",
        plan.provider
    );
    assert!(plan.is_empty(), "{plan:?}");
    // ...and Save keeps it, the file untouched.
    save_opencode_models(base, ApiKey::Withheld, &[]).expect("save");
    assert_eq!(
        api_key_of(),
        Some(json!("sk-user-0000")),
        "Save kept the key"
    );
    assert_eq!(
        fs::read_to_string(&path).unwrap(),
        seeded,
        "nothing to write"
    );

    // Not withheld and unset: today's behaviour, the plan removes the key and
    // Save does.
    seed();
    let plan = opencode_plan(base, ApiKey::Unset).expect("plan");
    assert_eq!(
        plan.provider,
        vec![FieldDiff {
            path: "options.apiKey".into(),
            old: Some("sk-user-0000".into()),
            new: None,
        }],
    );
    save_opencode_models(base, ApiKey::Unset, &[]).expect("save");
    assert_eq!(api_key_of(), None, "an unset key is removed");

    // A key of server.ini still replaces the file's.
    seed();
    save_opencode_models(base, ApiKey::Key("sk-server-1111"), &[]).expect("save");
    assert_eq!(api_key_of(), Some(json!("sk-server-1111")));
}

#[cfg(test)]
#[path = "integrations_tests.rs"]
mod tests;
