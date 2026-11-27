use super::*;

#[test]
fn ensure_provider_section_leaves_a_withheld_key_as_the_file_has_it() {
    let mut v = json!({});
    ensure_provider_section(&mut v, "http://h", ApiKey::Key("sk-user-0000")).unwrap();
    ensure_provider_section(&mut v, "http://h2", ApiKey::Withheld).unwrap();
    assert_eq!(
        v["provider"][PROVIDER_KEY]["options"]["apiKey"], "sk-user-0000",
        "withheld: neither removed nor changed"
    );
    assert_eq!(
        v["provider"][PROVIDER_KEY]["options"]["baseURL"],
        "http://h2/v1"
    );
    // A file without a key gets none.
    let mut v = json!({});
    ensure_provider_section(&mut v, "http://h", ApiKey::Withheld).unwrap();
    assert!(v["provider"][PROVIDER_KEY]["options"]
        .get("apiKey")
        .is_none());
}

#[test]
fn claude_snippet_with_a_withheld_key_shows_a_placeholder() {
    for shell in [Shell::Posix, Shell::PowerShell] {
        let s = env_script(shell, "http://h", ApiKey::Withheld, None);
        assert!(!s.contains("not-needed"), "{s}");
        assert!(
            s.contains("ANTHROPIC_API_KEY=\"<api-key>\"")
                || s.contains("ANTHROPIC_API_KEY = \"<api-key>\""),
            "{s}"
        );
    }
    // Unset stays as it was.
    let s = env_script(Shell::Posix, "http://h", ApiKey::Unset, None);
    assert!(s.contains("export ANTHROPIC_API_KEY=\"not-needed\""), "{s}");
}

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
    ensure_provider_section(&mut v, "http://localhost:8080", ApiKey::Unset).unwrap();
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
    ensure_provider_section(&mut v, "http://localhost:8080", ApiKey::Key("sk-old")).unwrap();
    ensure_provider_section(&mut v, "http://0.0.0.0:9090", ApiKey::Unset).unwrap();
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
    let mut p = model_profiles::ModelProfile {
        id: "m".into(),
        model: r"C:\models\m.gguf".into(),
        ..Default::default()
    };
    p.reasoning = "auto".into();
    assert!(model_profile_to_opencode_model(&p, None)
        .get("reasoning")
        .is_none());
    p.reasoning = "on".into();
    assert_eq!(
        model_profile_to_opencode_model(&p, None)["reasoning"],
        json!(true)
    );
    p.reasoning = "off".into();
    assert_eq!(
        model_profile_to_opencode_model(&p, None)["reasoning"],
        json!(false)
    );
}

#[test]
fn opencode_model_limit_math() {
    let mut p = model_profiles::ModelProfile {
        id: "m".into(),
        model: r"C:\models\m.gguf".into(),
        parallel: Some(1),
        ..Default::default()
    };
    // An explicit ctx-size: the fit cannot cut it, so a number is advertised.
    let entry = |p: &model_profiles::ModelProfile| {
        model_profile_to_opencode_model(p, advertised(p, true, None))
    };
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
    let p = model_profiles::ModelProfile {
        id: "m".into(),
        model: r"C:\models\m.gguf".into(),
        reasoning: "on".into(),
        ..Default::default()
    };
    let e = model_profile_to_opencode_model(&p, advertised(&p, true, Some(262_144)));
    assert!(e.get("limit").is_none());
    assert_eq!(e["reasoning"], json!(true));
    // The same model profile with the fit off loads its trained context: a number.
    let e = model_profile_to_opencode_model(&p, advertised(&p, false, Some(262_144)));
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
    let fresh = model_profile_to_opencode_model(&p, None);
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
    let base = model_profiles::ModelProfile::default();
    let cut =
        |p: &model_profiles::ModelProfile, fit: bool, t: Option<u32>| fit_may_cut_ctx(p, fit, t);

    // The plain autofit model profile: fit on, no ctx-size.
    assert!(cut(&base, true, TRAINED));
    // 1. Fit off: the context is exactly what effective_ctx derives.
    assert!(!cut(&base, false, TRAINED));
    // 2. split-mode tensor: the fit throws before doing anything.
    let tensor = model_profiles::ModelProfile {
        split_mode: "tensor".into(),
        ..Default::default()
    };
    assert!(!cut(&tensor, true, TRAINED));
    let layer = model_profiles::ModelProfile {
        split_mode: "layer".into(),
        ..Default::default()
    };
    assert!(cut(&layer, true, TRAINED));
    // 3. An explicit ctx-size is "set by user -> no change".
    let explicit = model_profiles::ModelProfile {
        ctx_size: Some(65_536),
        ..Default::default()
    };
    assert!(!cut(&explicit, true, TRAINED));
    // 3/5. ctx-size = 0 is automatic but sets the full-context sentinel...
    let zero = model_profiles::ModelProfile {
        ctx_size: Some(0),
        ..Default::default()
    };
    assert!(!cut(&zero, true, TRAINED));
    assert!(!cut(&zero, true, None));
    // ...which a fit-ctx overwrites (`-fitc` is parsed after `-c`).
    let zero_floor = model_profiles::ModelProfile {
        ctx_size: Some(0),
        fit_ctx: Some(8192),
        ..Default::default()
    };
    assert!(cut(&zero_floor, true, TRAINED));
    // fit-ctx = -1 is the sentinel itself.
    let sentinel = model_profiles::ModelProfile {
        fit_ctx: Some(-1),
        ..Default::default()
    };
    assert!(!cut(&sentinel, true, TRAINED));
    // 4. kv-unified-per-slot with ctx-size absent sizes the pool first.
    let per_slot = model_profiles::ModelProfile {
        kv_unified_per_slot: Some(32_768),
        ..Default::default()
    };
    assert!(!cut(&per_slot, true, TRAINED));
    // Under ctx-size = 0 the pool is not sized, but the sentinel stops the cut.
    let per_slot_zero = model_profiles::ModelProfile {
        ctx_size: Some(0),
        kv_unified_per_slot: Some(32_768),
        ..Default::default()
    };
    assert!(!cut(&per_slot_zero, true, TRAINED));
    // ...and with a fit-ctx beside ctx-size = 0 the pool is sized again.
    let per_slot_floor = model_profiles::ModelProfile {
        ctx_size: Some(0),
        fit_ctx: Some(8192),
        kv_unified_per_slot: Some(32_768),
        ..Default::default()
    };
    assert!(!cut(&per_slot_floor, true, TRAINED));
    // 5. A trained context at or below the floor leaves nothing to cut.
    assert!(!cut(&base, true, Some(4096)));
    assert!(cut(&base, true, Some(4097)));
    let floor = model_profiles::ModelProfile {
        fit_ctx: Some(300_000),
        ..Default::default()
    };
    assert!(!cut(&floor, true, TRAINED));
    // An unreadable header counts as above the floor.
    assert!(cut(&base, true, None));
    // n-gpu-layers by hand does not stop the context cut (step 2 runs first).
    let ngl = model_profiles::ModelProfile {
        n_gpu_layers: Some(99),
        ..Default::default()
    };
    assert!(cut(&ngl, true, TRAINED));
}

/// The pool sizing follows the same sentinel the fit reads.
#[test]
fn per_slot_pool_sizing_honours_a_fit_ctx_beside_ctx_size_zero() {
    let p = model_profiles::ModelProfile {
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
        keys: fit
            .map(|v| [("fit".to_string(), v.to_string())].into())
            .unwrap_or_default(),
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
    let script = |ex| env_script(Shell::Posix, "http://h", ApiKey::Unset, ex);
    let with = script(Some(("m", Some(262_144))));
    assert!(with.contains("export CLAUDE_CODE_MAX_CONTEXT_TOKENS=\"262144\""));
    assert!(with.contains("export CLAUDE_AUTOCOMPACT_PCT_OVERRIDE=\"81\""));
    let without = script(Some(("m", None)));
    assert!(without.contains("export ANTHROPIC_DEFAULT_SONNET_MODEL=\"m\""));
    assert!(!without.contains("CLAUDE_CODE_MAX_CONTEXT_TOKENS=\""));
    assert!(!without.contains("CLAUDE_AUTOCOMPACT_PCT_OVERRIDE"));
    assert!(without.contains("fit sizes its context at load"));
    // No model profile at all: placeholders, both lines.
    let none = script(None);
    assert!(none.contains("\"<context>\"") && none.contains("\"<percent>\""));
}

#[test]
fn claude_snippet_speaks_the_shell_of_its_os() {
    let posix = env_script(Shell::Posix, "http://h", ApiKey::Key("k"), None);
    assert!(posix.contains("\nexport ANTHROPIC_BASE_URL=\"http://h\"\n"));
    assert!(posix.contains("\nexport ANTHROPIC_API_KEY=\"k\"\n"));
    assert!(!posix.contains("$env:") && !posix.contains('\r'));
    assert!(posix.ends_with("# Then launch: claude\n"));

    let ps = env_script(Shell::PowerShell, "http://h", ApiKey::Unset, None);
    assert!(ps.contains("\r\n$env:ANTHROPIC_BASE_URL = \"http://h\"\r\n"));
    assert!(ps.contains("$env:ANTHROPIC_API_KEY = \"not-needed\"\r\n"));
    assert!(!ps.contains("export "));
    assert!(ps.ends_with("# Then launch: claude\r\n"));
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
            &model_profiles::ModelProfile {
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
            &model_profiles::ModelProfile {
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

/// An unticked id keeps the file's state for that model: its old entry
/// (listed and changed, or listed and no longer enabled) or its absence (a new
/// model profile); every other id takes the rebuilt entry or goes.
#[test]
fn merge_models_leaves_kept_ids_as_the_file_has_them() {
    let current = json!({
        "changed": { "name": "old" },
        "kept-off": { "name": "off" },
        "dropped-off": { "name": "gone" },
    });
    let current = current.as_object().unwrap();
    let desired = vec![
        ("changed".to_string(), json!({ "name": "new" })),
        ("new-kept".to_string(), json!({ "name": "n" })),
        ("new".to_string(), json!({ "name": "n2" })),
    ];
    let keep = [
        "changed".to_string(),
        "new-kept".to_string(),
        "kept-off".to_string(),
    ];
    let out = merge_models(current, desired.clone(), &keep);
    assert_eq!(
        out,
        vec![
            ("changed".to_string(), json!({ "name": "old" })),
            ("new".to_string(), json!({ "name": "n2" })),
            ("kept-off".to_string(), json!({ "name": "off" })),
        ]
    );
    // Nothing kept: exactly the desired list.
    assert_eq!(merge_models(current, desired.clone(), &[]), desired);
}

#[test]
fn leaf_diff_names_each_changed_leaf() {
    let mut out = Vec::new();
    leaf_diff(
        "",
        &json!({ "name": "M", "limit": { "context": 131072, "output": 32768 } }),
        &json!({ "name": "M", "limit": { "context": 65536, "output": 32768 }, "reasoning": true }),
        &mut out,
    );
    let d = |p: &str, o: Option<&str>, n: Option<&str>| FieldDiff {
        path: p.into(),
        old: o.map(Into::into),
        new: n.map(Into::into),
    };
    assert_eq!(
        out,
        vec![
            d("limit.context", Some("131072"), Some("65536")),
            d("reasoning", None, Some("true"))
        ]
    );
    // A whole object appearing lists its leaves.
    let mut out = Vec::new();
    leaf_diff(
        "",
        &Value::Null,
        &json!({ "options": { "baseURL": "http://h/v1", "timeout": 5 } }),
        &mut out,
    );
    assert_eq!(
        out,
        vec![
            d("options.baseURL", None, Some("http://h/v1")),
            d("options.timeout", None, Some("5"))
        ]
    );
    // Equal values, in any key order: nothing.
    let mut out = Vec::new();
    leaf_diff(
        "",
        &json!({ "a": 1, "b": 2 }),
        &json!({ "b": 2, "a": 1 }),
        &mut out,
    );
    assert!(out.is_empty());
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
    // No usable stem falls back to the model profile id.
    assert_eq!(friendly_model_name("fallback-id", ""), "Fallback Id");
}
