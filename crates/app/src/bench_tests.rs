use super::*;

/// A short prompt of the user's, for what a long one would hide.
const SHORT: &str = "Explain how a transformer decoder's key-value cache works.";

fn model_profile(id: &str) -> ModelProfile {
    ModelProfile {
        id: id.into(),
        model: r"E:\models\m.gguf".into(),
        ..Default::default()
    }
}

fn argv_of(a: &[String], flag: &str) -> Option<String> {
    a.iter()
        .position(|x| x == flag)
        .and_then(|i| a.get(i + 1))
        .cloned()
}

// A list entry `parse_int_list` would drop is a length or a depth the run
// never measures, so it is refused by value, and so is a negative one (kept
// by the parser, meaningless as a token count). Empty entries and an empty
// list are what the user meant, and 0 is a real depth.
#[test]
fn int_list_problem_names_the_entry_parse_int_list_drops() {
    for ok in ["", " ", "2048,8192,32768", "0, 8192,", "2048"] {
        assert_eq!(int_list_problem("Depths", ok), None, "{ok:?}");
    }
    for (bad, entry, dropped) in [
        ("2048,abc", "abc", true),
        ("2048, 8.192", "8.192", true),
        ("99999999999", "99999999999", true),
        ("-1,0", "-1", false),
    ] {
        let problem = int_list_problem("Prompt lengths", bad).expect(bad);
        assert!(
            problem.starts_with("Prompt lengths = ")
                && problem.contains(&format!("`{entry}` is not")),
            "{problem}"
        );
        assert_eq!(
            parse_int_list(bad).len() < bad.split(',').count(),
            dropped,
            "{bad:?}"
        );
    }
}

/// The prefill sweep of the default plan, the one most assertions want.
fn first_sweep(plan: &Plan) -> Sweep {
    synthetic_sweeps(plan).first().cloned().expect("a sweep")
}

// One invocation with both halves would run the CARTESIAN product: `-d`
// multiplies the pp tests too, so the default sweep would be 3 lengths x 3
// depths + 3 depths = 12 tests instead of 6, for the same information at
// twice the wall clock. This pins the split (and that the depths ride the
// decode half only).
#[test]
fn the_workload_splits_into_a_prefill_and_a_decode_sweep() {
    let sweeps = synthetic_sweeps(&Plan::default());
    assert_eq!(sweeps.len(), 2);
    assert_eq!(sweeps[0].name, "prefill");
    assert_eq!(
        argv_of(&sweeps[0].args, "-p").as_deref(),
        Some("2048,8192,32768")
    );
    assert_eq!(argv_of(&sweeps[0].args, "-n").as_deref(), Some("0"));
    assert!(
        !sweeps[0].args.iter().any(|a| a == "-d"),
        "no depths on prefill"
    );

    assert_eq!(sweeps[1].name, "decode");
    assert_eq!(argv_of(&sweeps[1].args, "-p").as_deref(), Some("0"));
    assert_eq!(argv_of(&sweeps[1].args, "-n").as_deref(), Some("128"));
    assert_eq!(
        argv_of(&sweeps[1].args, "-d").as_deref(),
        Some("0,8192,32768")
    );

    // Each half can be asked for alone, and a plan with neither is what
    // `validate` refuses.
    let only_pp = Plan {
        n_gen: 0,
        ..Plan::default()
    };
    assert_eq!(synthetic_sweeps(&only_pp).len(), 1);
    assert_eq!(synthetic_sweeps(&only_pp)[0].name, "prefill");
    let only_tg = Plan {
        prompt_lens: Vec::new(),
        ..Plan::default()
    };
    assert_eq!(synthetic_sweeps(&only_tg)[0].name, "decode");
    let neither = Plan {
        prompt_lens: Vec::new(),
        n_gen: 0,
        model_profiles: vec!["a".into()],
        mode: Mode::Synthetic,
        ..Plan::default()
    };
    assert!(synthetic_sweeps(&neither).is_empty());
    assert!(validate(&neither).is_some());
}

// The three separator conversions, each of which turns ONE benchmark into
// several when it is missed (a comma is llama-bench's "run these as separate
// configurations"), so the model profile would be benchmarked in a placement it
// never runs in.
#[test]
fn ini_separators_become_llama_bench_separators() {
    let p = ModelProfile {
        device: "ROCm0,CUDA0".into(),
        tensor_split: "54,12".into(),
        override_tensor: r"token_embd\.weight=ROCm0,output\.weight=CUDA0".into(),
        ..model_profile("a")
    };
    let plan = Plan::default();
    let a = synthetic_argv(&p, &ServerConfig::default(), &plan, &first_sweep(&plan));
    assert_eq!(argv_of(&a, "-dev").as_deref(), Some("ROCm0/CUDA0"));
    assert_eq!(argv_of(&a, "-ts").as_deref(), Some("54/12"));
    assert_eq!(
        argv_of(&a, "-ot").as_deref(),
        Some(r"token_embd\.weight=ROCm0;output\.weight=CUDA0")
    );
    assert!(!a.iter().any(|x| x.contains(',') && x.contains('=')));
}

// llama-bench has `-ncmoe` but no `-ncffn`, so `n-cpu-ffn` rides `-ot` as
// the per-layer rules llama.cpp expands it into, and it has to share the ONE
// `-ot` value with the model profile's own rules: a second `-ot` flag would be a
// second configuration group, i.e. a separate benchmark.
#[test]
fn n_cpu_ffn_expands_into_the_single_ot_value() {
    let plan = Plan::default();
    let p = ModelProfile {
        n_cpu_ffn: Some(2),
        override_tensor: r"token_embd\.weight=ROCm0".into(),
        ..model_profile("a")
    };
    let a = synthetic_argv(&p, &ServerConfig::default(), &plan, &first_sweep(&plan));
    assert_eq!(a.iter().filter(|x| *x == "-ot").count(), 1);
    assert_eq!(
        argv_of(&a, "-ot").as_deref(),
        Some(
            r"blk\.0\.ffn_(up|down|gate)\.=CPU;blk\.1\.ffn_(up|down|gate)\.=CPU;token_embd\.weight=ROCm0"
        )
    );
    assert!(!a.iter().any(|x| x == "-ncffn"));

    // Alone it still produces the flag; 0 and None produce nothing, like
    // llama.cpp's own `for i in 0..N`.
    let p = ModelProfile {
        n_cpu_ffn: Some(1),
        ..model_profile("a")
    };
    let a = synthetic_argv(&p, &ServerConfig::default(), &plan, &first_sweep(&plan));
    assert_eq!(
        argv_of(&a, "-ot").as_deref(),
        Some(r"blk\.0\.ffn_(up|down|gate)\.=CPU")
    );
    assert!(n_cpu_ffn_rules(Some(0)).is_empty());
    assert!(n_cpu_ffn_rules(None).is_empty());
}

// `op-offload` reaches llama-bench as `-nopo`, whose sense is INVERTED and
// whose value is 0|1: a straight copy of the bool would benchmark the
// opposite of the model profile.
#[test]
fn op_offload_rides_the_inverted_nopo_flag() {
    let plan = Plan::default();
    let argv = |state| {
        let p = ModelProfile {
            op_offload: state,
            ..model_profile("a")
        };
        synthetic_argv(&p, &ServerConfig::default(), &plan, &first_sweep(&plan))
    };
    assert_eq!(argv_of(&argv(Some(false)), "-nopo").as_deref(), Some("1"));
    assert_eq!(argv_of(&argv(Some(true)), "-nopo").as_deref(), Some("0"));
    assert!(!argv(None).iter().any(|x| x == "-nopo"));
}

#[test]
fn repack_rides_llama_bench_repack_as_0_or_1() {
    let plan = Plan::default();
    let argv = |state| {
        let p = ModelProfile {
            repack: state,
            ..model_profile("a")
        };
        synthetic_argv(&p, &ServerConfig::default(), &plan, &first_sweep(&plan))
    };
    assert_eq!(
        argv_of(&argv(Some(false)), "--repack").as_deref(),
        Some("0")
    );
    assert_eq!(argv_of(&argv(Some(true)), "--repack").as_deref(), Some("1"));
    assert!(!argv(None).iter().any(|x| x == "--repack"));
}

#[test]
fn an_untouched_model_profile_passes_only_the_workload() {
    let plan = Plan::default();
    let a = synthetic_argv(
        &model_profile("a"),
        &ServerConfig::default(),
        &plan,
        &first_sweep(&plan),
    );
    for flag in [
        "-dev", "-ts", "-sm", "-ot", "-ngl", "-ncmoe", "-ctk", "-fa", "-b", "-ub",
    ] {
        assert!(!a.iter().any(|x| x == flag), "{flag} should be absent");
    }
    assert_eq!(argv_of(&a, "-m").as_deref(), Some(r"E:\models\m.gguf"));
    assert_eq!(argv_of(&a, "-p").as_deref(), Some("2048,8192,32768"));
    // jsonl, not json: one flushed object per completed test, so a run
    // killed by a driver reset keeps what it measured.
    assert_eq!(argv_of(&a, "-o").as_deref(), Some("jsonl"));
}

#[test]
fn concurrency_round_trips_and_names_its_slots() {
    for c in Concurrency::ALL {
        assert_eq!(Concurrency::from_str(c.as_str()), Some(c));
    }
    assert_eq!(Concurrency::from_str("2-decode"), None);
    assert!(Concurrency::Single.warm_slots(4).is_empty());
    assert_eq!(Concurrency::Prefill.warm_slots(3), vec![false; 3]);
    assert_eq!(Concurrency::Decode.warm_slots(2), vec![true; 2]);
    // The last slot prefills while the others decode.
    assert_eq!(Concurrency::PrefillDecode.warm_slots(2), vec![true, false]);
    assert_eq!(
        Concurrency::PrefillDecode.warm_slots(4),
        vec![true, true, true, false]
    );
}

// A warm slot reuses its primed prompt, a cold one prefills it all; both
// pinned, since an unpinned pair may land on one slot and queue.
#[test]
fn a_slot_body_is_pinned_and_caches_only_when_warm() {
    let plan = Plan {
        prompt: "hi".into(),
        ..Plan::default()
    };
    let warm: serde_json::Value =
        serde_json::from_str(&live_body_slot("q", &plan, 0, true, 1)).unwrap();
    assert_eq!(warm["id_slot"], serde_json::json!(0));
    assert_eq!(warm["cache_prompt"], serde_json::json!(true));
    assert_eq!(warm["max_tokens"], serde_json::json!(1));
    let cold: serde_json::Value =
        serde_json::from_str(&live_body_slot("q", &plan, 1, false, 99)).unwrap();
    assert_eq!(cold["id_slot"], serde_json::json!(1));
    assert_eq!(cold["cache_prompt"], serde_json::json!(false));
    assert_eq!(cold["max_tokens"], serde_json::json!(99));
    let open: serde_json::Value =
        serde_json::from_str(&live_body_slot("q", &plan, 1, false, 0)).unwrap();
    assert!(open.get("max_tokens").is_none());
}

#[test]
fn the_prefix_ends_before_the_last_assistant_turn() {
    let conv = r#"{"messages":[{"role":"system","content":"s"},{"role":"user","content":"u"},
            {"role":"assistant","content":"a1"},{"role":"tool","content":"t1"},
            {"role":"assistant","content":"a2"},{"role":"tool","content":"t2"}],"tools":[{"x":1}]}"#;
    let p: serde_json::Value = serde_json::from_str(&prompt_prefix(conv).unwrap()).unwrap();
    let roles: Vec<&str> = p["messages"]
        .as_array()
        .unwrap()
        .iter()
        .map(|m| m["role"].as_str().unwrap())
        .collect();
    assert_eq!(roles, ["system", "user", "assistant", "tool"]);
    assert_eq!(p["tools"], serde_json::json!([{"x": 1}]));
    assert!(prompt_prefix("just text").is_none());
    assert!(prompt_prefix(r#"{"messages":[{"role":"assistant","content":"a"}]}"#).is_none());
}

#[test]
fn concurrent_totals_span_the_shared_window() {
    let t = |prompt_n, prompt_tps, predicted_n, predicted_tps| Timings {
        prompt_n,
        prompt_tps,
        predicted_n,
        predicted_tps,
        ..Timings::default()
    };
    // Two cold requests sent together: 1000 tokens each in 2 s and 4 s,
    // then 100 tokens each in 10 s and 5 s.
    let a = t(1000, 500.0, 100, 10.0);
    let b = t(1000, 250.0, 100, 20.0);
    let (p, d) = concurrent_totals(&[(0.0, &a, false), (0.0, &b, false)]);
    // 2000 tokens by the end of the slower prefill (4 s).
    assert!((p.unwrap() - 500.0).abs() < 1e-9);
    // Decode from 2 s (first start) to 12 s (a's end): 200 tokens in 10 s.
    assert!((d.unwrap() - 20.0).abs() < 1e-9);
    // A warm slot adds no prefill tokens; with no cold one there is no total.
    let w = t(1, 100.0, 50, 25.0);
    let (p, _) = concurrent_totals(&[(0.0, &w, true), (0.0, &w, true)]);
    assert_eq!(p, None);
    let (p, _) = concurrent_totals(&[(0.0, &w, true), (0.5, &a, false)]);
    // 1000 cold tokens over 0 .. 0.5 + 2 s.
    assert!((p.unwrap() - 400.0).abs() < 1e-9);
}

#[test]
fn concurrency_needs_the_live_mode() {
    let plan = Plan {
        mode: Mode::Synthetic,
        model_profiles: vec!["q".into()],
        concurrency: Concurrency::Decode,
        ..Plan::default()
    };
    assert!(validate(&plan).unwrap().contains("live"));
}

// cache_prompt is what keeps every repetition a cold prefill; without it the
// second one reports the prefix cache's speed and the mean is a fiction.
#[test]
fn the_live_body_disables_the_prefix_cache_and_carries_the_override() {
    let plan = Plan {
        prompt: "hi".into(),
        temp: Some(0.0),
        max_tokens: 64,
        ..Plan::default()
    };
    let body: serde_json::Value = serde_json::from_str(&live_body("qwen", &plan)).unwrap();
    assert_eq!(body["cache_prompt"], serde_json::json!(false));
    assert_eq!(body["model"], "qwen");
    assert_eq!(body["temperature"], serde_json::json!(0.0));
    assert_eq!(body["max_tokens"], serde_json::json!(64));
    // Streamed, with the cumulative timings on every event: what the
    // reasoning/answer split is computed from.
    assert_eq!(body["stream"], serde_json::json!(true));
    assert_eq!(body["timings_per_token"], serde_json::json!(true));

    // "the model profile's own" must OMIT the key: sending any number would
    // override the model profile with it.
    let plan = Plan { temp: None, ..plan };
    let body: serde_json::Value = serde_json::from_str(&live_body("qwen", &plan)).unwrap();
    assert!(body.get("temperature").is_none());

    // 0 = no cap: the key is OMITTED, so the model stops on its own.
    let plan = Plan {
        max_tokens: 0,
        ..plan
    };
    let body: serde_json::Value = serde_json::from_str(&live_body("qwen", &plan)).unwrap();
    assert!(body.get("max_tokens").is_none());
}

#[test]
fn timings_are_read_from_the_right_keys() {
    let body = r#"data: {"choices":[],"timings":{
            "cache_n": 7, "prompt_n": 1024, "prompt_per_second": 900.5,
            "predicted_n": 128, "predicted_per_second": 35.25,
            "draft_n": 100, "draft_n_accepted": 74 }}"#
        .replace('\n', "");
    let t = parse_stream(&body).unwrap().timings;
    assert_eq!(t.prompt_n, 1024);
    assert!((t.prompt_tps - 900.5).abs() < 1e-9);
    assert!((t.predicted_tps - 35.25).abs() < 1e-9);
    assert_eq!(t.cache_n, 7);
    assert_eq!((t.draft_n, t.draft_accepted), (100, 74));
}

/// One SSE event the way llama-server writes it with `timings_per_token`:
/// a delta of one kind plus the slot's cumulative timings.
fn event(kind: &str, text: &str, n: i64, ms: f64, dn: i64, da: i64) -> String {
    let delta = if kind.is_empty() {
        serde_json::json!({})
    } else {
        serde_json::json!({ kind: text })
    };
    let v = serde_json::json!({
        "choices": [{ "delta": delta }],
        "timings": {
            "prompt_n": 1000, "prompt_per_second": 800.0, "cache_n": 0,
            "predicted_n": n, "predicted_ms": ms,
            "predicted_per_second": if ms > 0.0 { 1e3 * (n - 1) as f64 / ms } else { 0.0 },
            "draft_n": dn, "draft_n_accepted": da,
        },
    });
    format!("data: {v}\n\n")
}

// The split is a difference of the server's cumulative timings at the last
// reasoning event, so each phase gets its own rate and acceptance.
#[test]
fn a_stream_splits_decode_at_the_reasoning_answer_boundary() {
    let mut body = String::new();
    body += &event("reasoning_content", "Let", 1, 0.0, 0, 0);
    body += &event("reasoning_content", " me", 101, 5000.0, 150, 60);
    body += &event("content", "The", 102, 5010.0, 152, 61);
    body += &event("content", " end", 301, 9000.0, 400, 300);
    body += &event("", "", 301, 9000.0, 400, 300); // finish_reason event
    body += "data: [DONE]\n\n";
    let s = parse_stream(&body).unwrap();

    assert_eq!(s.timings.predicted_n, 301);
    assert_eq!(s.timings.prompt_n, 1000);
    let split = s.split.expect("the model reasoned");
    assert_eq!(split.reasoning.tokens, 101);
    assert_eq!(split.reasoning.steps, 100, "the first token is free");
    assert!((split.reasoning.tps().unwrap() - 20.0).abs() < 1e-9);
    assert_eq!(split.reasoning.acceptance().map(f64::round), Some(40.0));
    assert_eq!(split.answer.tokens, 200);
    assert!((split.answer.tps().unwrap() - 50.0).abs() < 1e-9);
    assert_eq!(split.answer.draft_n, 250);
    assert_eq!(split.answer.acceptance().map(f64::round), Some(96.0));
}

// An agent turn answers with a tool call, which streams no content: the
// call is the answer, or the whole decode would read as reasoning.
#[test]
fn a_tool_call_is_the_answer_after_the_reasoning() {
    let call = |n: i64, ms: f64| {
        let v = serde_json::json!({
            "choices": [{ "delta": { "tool_calls": [{ "index": 0, "function": { "arguments": "{\"pa" } }] } }],
            "timings": {
                "prompt_n": 1000, "prompt_per_second": 800.0, "cache_n": 0,
                "predicted_n": n, "predicted_ms": ms, "predicted_per_second": 0.0,
                "draft_n": 0, "draft_n_accepted": 0,
            },
        });
        format!("data: {v}\n\n")
    };
    let body = event("reasoning_content", "Let", 1, 0.0, 0, 0)
        + &event("reasoning_content", " me", 101, 5000.0, 0, 0)
        + &call(102, 5010.0)
        + &call(301, 9000.0);
    let split = parse_stream(&body)
        .unwrap()
        .split
        .expect("the model reasoned");
    assert_eq!(split.reasoning.tokens, 101);
    assert_eq!(split.answer.tokens, 200);
}

#[test]
fn a_stream_without_reasoning_has_no_split() {
    let body = event("content", "Hi", 1, 0.0, 0, 0) + &event("content", "!", 2, 30.0, 0, 0);
    let s = parse_stream(&body).unwrap();
    assert_eq!(s.split, None);
    assert_eq!(s.timings.predicted_n, 2);
}

// Output cut by max_tokens mid-thought: the whole decode is reasoning and
// the answer span is empty, which must read as "no rate", not as 0 t/s.
#[test]
fn a_stream_that_never_answers_has_an_empty_answer_span() {
    let body = event("reasoning_content", "a", 1, 0.0, 0, 0)
        + &event("reasoning_content", "b", 50, 1000.0, 60, 30);
    let split = parse_stream(&body).unwrap().split.unwrap();
    assert_eq!(split.reasoning.tokens, 50);
    assert_eq!(split.answer.tokens, 0);
    assert_eq!(split.answer.tps(), None);
}

/// A reply ending the way the chat stream ends one: a token event whose
/// `finish_reason` is null, then the final event with an empty delta.
fn reply(reason: &str, n: i64) -> Streamed {
    let last = |reason: serde_json::Value, n: i64| {
        let v = serde_json::json!({
            "choices": [{ "finish_reason": reason, "index": 0, "delta": {} }],
            "timings": {
                "prompt_n": 1000, "prompt_per_second": 800.0, "cache_n": 24,
                "predicted_n": n, "predicted_ms": 1000.0, "predicted_per_second": 40.0,
            },
        });
        format!("data: {v}\n\n")
    };
    let body = last(serde_json::Value::Null, n) + &last(serde_json::json!(reason), n);
    parse_stream(&(body + "data: [DONE]\n\n")).unwrap()
}

#[test]
fn a_model_that_stops_on_its_own_is_never_a_context_cut() {
    for reason in ["stop", "tool_calls"] {
        let s = reply(reason, 100);
        assert_eq!(s.finish_reason.as_deref(), Some(reason));
        assert!(!cut_by_context(&s, None));
        assert!(!cut_by_context(&s, Some(256)));
    }
    // A stream with no final event carries no verdict, so it fails nothing.
    let partial = parse_stream(&event("content", "Hi", 1, 0.0, 0, 0)).unwrap();
    assert_eq!(partial.finish_reason, None);
    assert!(!cut_by_context(&partial, None));
}

// A reached cap reports `length` too, and the count is what tells it apart.
#[test]
fn an_answer_at_the_requested_cap_stays_valid() {
    let plan = Plan {
        max_tokens: 256,
        ..Default::default()
    };
    let cap = output_cap(&plan, &model_profile("a"));
    assert_eq!(cap, Some(256));
    assert!(!cut_by_context(&reply("length", 256), cap));

    // With no cap in the request, the model profile's n-predict is the one applied.
    let plan = Plan {
        max_tokens: 0,
        ..Default::default()
    };
    let capped = ModelProfile {
        n_predict: Some(512),
        ..model_profile("a")
    };
    let cap = output_cap(&plan, &capped);
    assert_eq!(cap, Some(512));
    assert!(!cut_by_context(&reply("length", 512), cap));

    // The request's cap wins over the model profile's, in either direction.
    let plan = Plan {
        max_tokens: 64,
        ..Default::default()
    };
    assert_eq!(output_cap(&plan, &capped), Some(64));
}

#[test]
fn a_reply_the_context_cut_fails_and_names_the_numbers() {
    let plan = Plan {
        max_tokens: 0,
        ..Default::default()
    };
    // Uncapped: any `length` stop is the context, -1 included.
    for n_predict in [None, Some(-1)] {
        let p = ModelProfile {
            n_predict,
            ..model_profile("a")
        };
        assert_eq!(output_cap(&plan, &p), None);
    }
    let cut = reply("length", 100);
    assert!(cut_by_context(&cut, None));
    // Capped, but the slot ran out first.
    assert!(cut_by_context(&cut, Some(256)));

    let e = context_cut_error(&cut, None, Some(1200));
    assert!(e.contains("cut by the context limit"), "{e}");
    assert!(e.contains("slot size 1200 tokens"), "{e}");
    assert!(e.contains("1024 prompt + 100 generated tokens"), "{e}");
    assert!(e.contains("with no output cap"), "{e}");
    let e = context_cut_error(&cut, Some(256), None);
    assert!(e.contains("slot size unknown"), "{e}");
    assert!(e.contains("below the output cap of 256"), "{e}");
}

#[test]
fn the_slot_size_comes_from_props_and_the_routers_zero_is_none() {
    let props =
        |n: i64| serde_json::json!({ "default_generation_settings": { "n_ctx": n } }).to_string();
    assert_eq!(props_n_ctx(&props(124000)), Some(124000));
    assert_eq!(props_n_ctx(&props(0)), None);
    assert_eq!(props_n_ctx("not json"), None);
}

#[test]
fn a_stream_traces_every_n_tokens_and_marks_the_phase() {
    let mut body = String::new();
    for n in 1..=200 {
        let kind = if n <= 100 {
            "reasoning_content"
        } else {
            "content"
        };
        body += &event(kind, "x", n, n as f64 * 10.0, n, n / 2);
    }
    let trace = parse_stream(&body).unwrap().trace;
    let at: Vec<i64> = trace.iter().map(|p| p.tokens).collect();
    assert_eq!(at, vec![64, 128, 192]);
    assert!(trace[0].reasoning && !trace[1].reasoning);
}

#[test]
fn a_phase_point_averages_rates_and_notes_span_and_acceptance() {
    let span = |tokens, dn, da| Phase {
        tokens,
        ms: 1.0,
        steps: tokens,
        draft_n: dn,
        draft_accepted: da,
    };
    let p = phase_point(
        "q",
        LIVE_DECODE_REASONING,
        &[(20.0, span(900, 100, 50)), (22.0, span(1000, 100, 70))],
    )
    .unwrap();
    assert!((p.mean - 21.0).abs() < 1e-9);
    assert_eq!(p.n, 2);
    assert_eq!(p.note, "950 tokens, draft accepted 60%");
    assert_eq!(phase_point("q", LIVE_DECODE_ANSWER, &[]), None);

    let row = trace_json(&[TracePoint {
        tokens: 64,
        ms: 3.5,
        draft_n: 10,
        draft_accepted: 5,
        reasoning: true,
    }]);
    assert_eq!(row, serde_json::json!([[64, 3.5, 10, 5, "r"]]));
}

#[test]
fn a_stream_error_event_surfaces_as_the_error() {
    let body = r#"data: {"error":{"message":"model not found"}}"#;
    let err = parse_stream(body).unwrap_err();
    assert!(err.contains("model not found"), "{err}");
    assert!(parse_stream("").is_err(), "no timings at all is an error");
}

#[test]
fn a_bench_line_becomes_a_point_with_llama_bench_s_own_label() {
    let line = r#"{"n_prompt":0,"n_gen":128,"n_depth":8192,"avg_ts":35.5,"stddev_ts":0.5,"build_number":10566}"#;
    let p = parse_bench_line(line, "qwen", 3).unwrap();
    assert_eq!(p.test, "tg128 @ d8192");
    assert_eq!(p.model_profile, "qwen");
    assert_eq!(p.n, 3);
    assert_eq!(parse_bench_build(line), Some(10566));

    let line = r#"{"n_prompt":2048,"n_gen":0,"n_depth":0,"avg_ts":600.0}"#;
    assert_eq!(parse_bench_line(line, "q", 1).unwrap().test, "pp2048");
    // A progress line or any other non-row must not become a point.
    assert!(parse_bench_line("not json", "q", 1).is_none());
    assert!(parse_bench_line(r#"{"n_prompt":1}"#, "q", 1).is_none());
}

#[test]
fn rows_group_by_test_and_ratio_against_the_first_model_profile() {
    let pt = |model_profile: &str, test: &str, mean: f64| Point {
        model_profile: model_profile.into(),
        test: test.into(),
        mean,
        sd: 0.0,
        n: 3,
        note: String::new(),
    };
    let points = vec![
        pt("a", "pp2048", 100.0),
        pt("b", "pp2048", 50.0),
        pt("a", "tg128", 20.0),
        pt("b", "tg128", 30.0),
    ];
    let order = vec!["a".to_string(), "b".to_string()];
    let r = rows(&points, &order);
    assert_eq!(r.len(), 4);
    assert_eq!(
        (r[0].test.as_str(), r[0].model_profile.as_str()),
        ("pp2048", "a")
    );
    assert_eq!(r[0].ratio, "1.00x");
    assert_eq!(r[1].ratio, "0.50x");
    assert_eq!(
        (r[2].test.as_str(), r[2].model_profile.as_str()),
        ("tg128", "a")
    );
    assert_eq!(r[3].ratio, "1.50x");

    // A model profile that produced a test the baseline did not gets no invented
    // ratio.
    let r = rows(&[pt("b", "pp4096", 10.0)], &order);
    assert_eq!(r[0].ratio, "-");
}

// The confound this exists to catch, in the exact shape it took: two model profiles
// that differ only by device, NEITHER pinning flash-attn, so both ran at
// `auto` and `auto` resolved differently per backend. Nothing in the rates
// shows it.
#[test]
fn a_flash_attn_difference_between_compared_model_profiles_is_reported() {
    let seen = |pairs: &[(&str, &str)]| -> BTreeMap<String, String> {
        pairs
            .iter()
            .map(|(k, v)| (k.to_string(), v.to_string()))
            .collect()
    };
    let w = flash_attn_mismatch(&seen(&[("vk", "off"), ("rocm", "on")])).expect("warned");
    assert!(w.contains("vk=off") && w.contains("rocm=on"), "{w}");

    // Agreement is silence, however it was reached: a model profile pinning `on`
    // and one resolving `on` from auto are the same configuration.
    assert_eq!(
        flash_attn_mismatch(&seen(&[("a", "on"), ("b", "on")])),
        None
    );
    // One model profile is nothing to compare, and an empty map is a run that
    // produced no rows.
    assert_eq!(flash_attn_mismatch(&seen(&[("a", "on")])), None);
    assert_eq!(flash_attn_mismatch(&BTreeMap::new()), None);
}

// llama-bench spells the field differently per output format, and reading it
// wrong would silence the warning above rather than fire it, which is the
// failure that hides.
#[test]
fn the_resolved_flash_attn_is_read_in_every_spelling() {
    assert_eq!(
        parse_bench_flash_attn(r#"{"flash_attn":true}"#).as_deref(),
        Some("on")
    );
    assert_eq!(
        parse_bench_flash_attn(r#"{"flash_attn":false}"#).as_deref(),
        Some("off")
    );
    assert_eq!(
        parse_bench_flash_attn(r#"{"flash_attn":1}"#).as_deref(),
        Some("on")
    );
    assert_eq!(
        parse_bench_flash_attn(r#"{"flash_attn":0}"#).as_deref(),
        Some("off")
    );
    assert_eq!(
        parse_bench_flash_attn(r#"{"flash_attn":"auto"}"#).as_deref(),
        Some("auto")
    );
    assert_eq!(parse_bench_flash_attn(r#"{"avg_ts":1}"#), None);
    assert_eq!(parse_bench_flash_attn("not json"), None);
}

// The ids are not stable across driver states, so the report records what
// each one pointed at, and an id the probe does not know is a warning rather
// than a blank.
#[test]
fn device_ids_are_resolved_to_names_and_unknown_ones_warn() {
    let devs = crate::devices::parse(
        "Available devices:\n  \
             CUDA0: NVIDIA GeForce RTX 4070 SUPER (12281 MiB, 10844 MiB free)\n  \
             ROCm0: AMD Radeon AI PRO R9700 (32624 MiB, 32462 MiB free)\n",
    );
    let got = device_names("ROCm0, CUDA0", &devs);
    assert_eq!(got[0].1, "AMD Radeon AI PRO R9700");
    assert_eq!(got[1].1, "NVIDIA GeForce RTX 4070 SUPER");
    assert_eq!(unknown_device_warning(&[("p".into(), got)]), None);

    // A stale id keeps its slot with an empty name, and is called out.
    let stale = device_names("ROCm1", &devs);
    assert_eq!(stale, vec![("ROCm1".to_string(), String::new())]);
    let w = unknown_device_warning(&[("p".into(), stale)]).expect("warned");
    assert!(w.contains("p -> ROCm1"), "{w}");

    // An empty device list is "llama.cpp picks", not an error.
    assert!(device_names("", &devs).is_empty());
}

// The raw row is the audit trail; wrapping it must not lose it, and a row
// that is not JSON must not take the run down with it.
#[test]
fn warnings_precede_the_results_and_the_environment_is_recorded() {
    let plan = Plan {
        model_profiles: vec!["a".into()],
        ..Plan::default()
    };
    let meta = RunMeta {
        warnings: vec!["legs disagree".into()],
        env: vec![("booted".into(), "yesterday".into())],
        ..RunMeta::default()
    };
    let md = render_report(&plan, &[], &meta);
    let warn_at = md.find("legs disagree").expect("warning present");
    let results_at = md.find("## Results").expect("results heading");
    assert!(warn_at < results_at, "the warning must come first");
    assert!(md.contains("## Environment"));
    assert!(md.contains("| booted | yesterday |"));
}

#[test]
fn mean_and_sample_stddev() {
    let (m, sd) = mean_sd(&[10.0, 12.0, 14.0]);
    assert!((m - 12.0).abs() < 1e-9);
    assert!((sd - 2.0).abs() < 1e-9);
    assert_eq!(mean_sd(&[5.0]), (5.0, 0.0));
    assert_eq!(mean_sd(&[]), (0.0, 0.0));
}

#[test]
fn validate_refuses_the_plans_that_would_benchmark_nothing() {
    assert!(
        validate(&Plan::default()).is_some(),
        "no model profile selected"
    );
    // A default plan carries a prompt FILE and no text: the caller reads
    // one into the other at Run. So it must NOT validate, or a forgotten
    // read would benchmark the model answering nothing, and the message
    // has to name the file the text was supposed to come from.
    let unresolved = Plan {
        model_profiles: vec!["a".into()],
        ..Plan::default()
    };
    let problem = validate(&unresolved).expect("an unresolved prompt is not a plan");
    assert!(
        problem.contains("bench-prompt-43k-1.0.txt"),
        "the message names the file: {problem}"
    );

    let base = Plan {
        prompt: SHORT.into(),
        ..unresolved
    };
    assert!(validate(&base).is_none());
    assert!(validate(&Plan {
        prompt: "  ".into(),
        ..base.clone()
    })
    .is_some());
    // The synthetic mode needs no prompt, so a blank one must not stop it.
    assert!(validate(&Plan {
        mode: Mode::Synthetic,
        prompt: String::new(),
        ..base.clone()
    })
    .is_none());
    assert!(validate(&Plan { reps: 0, ..base }).is_some());
}

/// The two normalizations that decide whether two runs are comparable: a
/// BOM is a token of the prompt if it survives, and CRLF against LF is a
/// different tokenization of "the same" text saved by two editors.
#[test]
fn normalize_strips_the_bom_and_folds_crlf() {
    assert_eq!(normalize_prompt("\u{feff}hello"), "hello");
    assert_eq!(normalize_prompt("a\r\nb\r\n"), "a\nb\n");
    assert_eq!(normalize_prompt("\u{feff}a\r\nb"), "a\nb");
    // A lone CR is left alone: it is not a line ending Windows editors
    // write, and rewriting bytes nobody asked about is how a prompt stops
    // being what the file says.
    assert_eq!(normalize_prompt("a\rb"), "a\rb");
    // Trailing whitespace survives: a prompt that ends in a newline is a
    // deliberately different prompt for some templates.
    assert_eq!(normalize_prompt("hi \n"), "hi \n");
    // The digest follows the normalization, which is the whole point: the
    // same prompt saved two ways must hash the same.
    assert_eq!(
        prompt_info(&normalize_prompt("a\r\nb")).sha256,
        prompt_info(&normalize_prompt("a\nb")).sha256
    );
}

#[test]
fn load_prompt_file_reads_normalizes_and_names_its_failures() {
    let dir = tempfile::tempdir().unwrap();
    let missing = dir.path().join("nope.txt");
    let err = load_prompt_file(&missing).expect_err("a missing file is an error");
    assert!(err.contains("nope.txt"), "the error names the file: {err}");

    let empty = dir.path().join("empty.txt");
    std::fs::write(&empty, "   \r\n  ").unwrap();
    let err = load_prompt_file(&empty).expect_err("whitespace is not a prompt");
    assert!(err.contains("empty.txt"), "the error names the file: {err}");

    let good = dir.path().join("p.txt");
    std::fs::write(&good, "\u{feff}line one\r\nline two").unwrap();
    assert_eq!(load_prompt_file(&good).unwrap(), "line one\nline two");
}

/// The shipped prompts are only worth shipping if the long ones are long,
/// and only comparable if they survive normalization unchanged: a CRLF
/// checkout (or a BOM) would make one a different prompt on a different
/// machine. `.gitattributes` pins that; this notices if the pin is lost.
/// Each is `<name>-<X.Y>`, and a `.json` one is a conversation.
#[test]
fn the_shipped_prompts_are_versioned_normalized_and_long() {
    let shipped = shipped_prompts();
    let text = |name: &str| {
        let p = shipped.iter().find(|p| p.file_name().unwrap() == name);
        std::fs::read_to_string(p.unwrap_or_else(|| panic!("{name} is shipped"))).unwrap()
    };
    let p43 = text("bench-prompt-43k-1.0.txt");
    let p110 = text("bench-prompt-110k-1.0.txt");
    assert!(p43.chars().count() > 100_000, "got {}", p43.chars().count());
    assert!(p110.chars().count() > p43.chars().count() * 2);
    for p in &shipped {
        let name = p.file_stem().unwrap().to_string_lossy().into_owned();
        let (_, version) = name.rsplit_once('-').expect("<name>-<X.Y>");
        assert!(
            version.split('.').count() == 2
                && version
                    .split('.')
                    .all(|n| !n.is_empty() && n.chars().all(|c| c.is_ascii_digit())),
            "{name}: the file name ends in -<X.Y>"
        );
        let raw = std::fs::read_to_string(p).unwrap();
        assert_eq!(
            normalize_prompt(&raw),
            raw,
            "{name} must already be LF and BOM-free"
        );
        if p.extension().unwrap() == "json" && prompt_has_image(&raw) {
            // Its image is shipped beside it, matches its pin, and is
            // sent fitted into the default size.
            let size = parse_image_size(DEFAULT_IMAGE_SIZE).unwrap();
            let sent = resolve_images(&raw, p.parent().unwrap(), size, None).expect(&name);
            let (messages, _) = prompt_request(&sent, None);
            let url = messages[0]["content"][0]["image_url"]["url"]
                .as_str()
                .unwrap();
            assert!(
                url.starts_with("data:image/jpeg;base64,"),
                "{name}: the image inline"
            );
            assert_eq!(
                resolve_images(&sent, p.parent().unwrap(), size, None).unwrap(),
                sent
            );
        } else if p.extension().unwrap() == "json" {
            let (messages, tools) = prompt_request(&raw, None);
            assert!(
                messages.as_array().unwrap().len() > 1,
                "{name}: a conversation"
            );
            // An agent turn carries the tools its calls name; a request
            // to write a file (code-write) has no tool turn and no tools.
            let calls = messages
                .as_array()
                .unwrap()
                .iter()
                .any(|m| m["role"] == "tool");
            assert_eq!(
                tools.is_some(),
                calls,
                "{name}: tools exactly when it has tool turns"
            );
        }
    }
}

/// A JSON prompt is a conversation: its messages and tools go into the
/// request as given; any other text is one user message, a JSON-looking
/// one included when it has no `messages`.
#[test]
fn a_json_prompt_is_a_conversation_with_tools() {
    let conv = serde_json::json!({
        "messages": [
            { "role": "system", "content": "You are an agent." },
            { "role": "user", "content": "Fix it." },
        ],
        "tools": [{ "type": "function", "function": { "name": "read_file", "parameters": {} } }],
    })
    .to_string();
    let plan = Plan {
        model_profiles: vec!["a".into()],
        prompt: conv,
        ..Plan::default()
    };
    let body: serde_json::Value = serde_json::from_str(&live_body("a", &plan)).unwrap();
    assert_eq!(body["messages"].as_array().unwrap().len(), 2);
    assert_eq!(body["messages"][1]["content"], "Fix it.");
    assert_eq!(body["tools"][0]["function"]["name"], "read_file");

    let (m, t) = prompt_request("{\"not\": \"a conversation\"}", None);
    assert_eq!(m[0]["role"], "user");
    assert_eq!(m[0]["content"], "{\"not\": \"a conversation\"}");
    assert_eq!(t, None);

    // The preview elides each message, and stays a conversation.
    let long = serde_json::json!({
        "messages": [{ "role": "user", "content": "z".repeat(PREVIEW_PROMPT_CHARS * 3) }],
        "tools": [],
    })
    .to_string();
    let (m, _) = prompt_request(&long, Some(PREVIEW_PROMPT_CHARS));
    assert!(m[0]["content"].as_str().unwrap().contains("chars shown]"));
}

/// An `image_file` part is read, checked against its sha256 and fitted
/// into the size (never enlarged); `original` sends the bytes as they are.
#[test]
fn an_image_file_is_checked_and_fitted_into_the_size() {
    use base64::Engine as _;
    let dir = tempfile::tempdir().unwrap();
    let mut png = Vec::new();
    image::RgbImage::from_pixel(600, 400, image::Rgb([200, 100, 50]))
        .write_to(&mut std::io::Cursor::new(&mut png), image::ImageFormat::Png)
        .unwrap();
    std::fs::write(dir.path().join("i.png"), &png).unwrap();
    let prompt = |sha: &str| {
        serde_json::json!({ "messages": [{ "role": "user", "content": [
            { "type": "image_file", "file": "i.png", "sha256": sha },
            { "type": "text", "text": "Describe it." },
        ]}]})
        .to_string()
    };
    let good = prompt(&crate::sha256::hex(&png));
    assert!(prompt_has_image(&good));
    let decoded = |sent: &str| {
        let (m, _) = prompt_request(sent, None);
        let url = m[0]["content"][0]["image_url"]["url"]
            .as_str()
            .unwrap()
            .to_string();
        let (mime, data) = url
            .strip_prefix("data:")
            .unwrap()
            .split_once(";base64,")
            .unwrap();
        let bytes = base64::engine::general_purpose::STANDARD
            .decode(data)
            .unwrap();
        (mime.to_string(), bytes)
    };
    let (mime, bytes) =
        decoded(&resolve_images(&good, dir.path(), Some((300, 300)), None).unwrap());
    let img = image::load_from_memory(&bytes).unwrap();
    assert_eq!(
        (mime.as_str(), img.width(), img.height()),
        ("image/jpeg", 300, 200)
    );
    // Already inside the box, or original: the file's own bytes.
    for size in [Some((1344, 896)), None] {
        let (mime, bytes) = decoded(&resolve_images(&good, dir.path(), size, None).unwrap());
        assert_eq!((mime.as_str(), &bytes), ("image/png", &png));
    }
    let bad = prompt(&"0".repeat(64));
    assert!(resolve_images(&bad, dir.path(), None, None)
        .unwrap_err()
        .contains("the prompt pins"));
    // An image of the user's replaces the prompt's, with no pin to match.
    let mine = dir.path().join("mine.png");
    let mut other = Vec::new();
    image::RgbImage::from_pixel(10, 10, image::Rgb([0, 0, 0]))
        .write_to(
            &mut std::io::Cursor::new(&mut other),
            image::ImageFormat::Png,
        )
        .unwrap();
    std::fs::write(&mine, &other).unwrap();
    let (_, bytes) = decoded(&resolve_images(&bad, dir.path(), None, Some(&mine)).unwrap());
    assert_eq!(bytes, other);
    assert_eq!(
        prompt_image_file(&good, dir.path()),
        Some(dir.path().join("i.png"))
    );
    assert_eq!(prompt_image_file("hello", dir.path()), None);
    // A prompt without an image_file comes back as it was.
    assert_eq!(
        resolve_images("hello", dir.path(), None, None).unwrap(),
        "hello"
    );
    assert_eq!(parse_image_size("1344x896").unwrap(), Some((1344, 896)));
    assert_eq!(parse_image_size("original").unwrap(), None);
    assert!(parse_image_size("0x5").is_err() && parse_image_size("big").is_err());
    assert_eq!(image_size_str(Some((1344, 896))), "1344x896");
}

/// An image prompt: a content part of type `image_url`. The preview
/// elides its data URL; the request sends it whole.
#[test]
fn an_image_prompt_is_recognized_and_its_data_url_elided_in_the_preview() {
    let url = format!(
        "data:image/jpeg;base64,{}",
        "A".repeat(PREVIEW_PROMPT_CHARS * 3)
    );
    let image = serde_json::json!({
        "messages": [{ "role": "user", "content": [
            { "type": "image_url", "image_url": { "url": url } },
            { "type": "text", "text": "Describe it." },
        ]}],
    })
    .to_string();
    assert!(prompt_has_image(&image));
    assert!(!prompt_has_image("Describe it."));
    assert!(!prompt_has_image(
        r#"{"messages": [{"role": "user", "content": "hi"}]}"#
    ));
    let (m, _) = prompt_request(&image, Some(PREVIEW_PROMPT_CHARS));
    assert!(m[0]["content"][0]["image_url"]["url"]
        .as_str()
        .unwrap()
        .contains("chars shown]"));
    assert_eq!(m[0]["content"][1]["text"], "Describe it.");
    let (m, _) = prompt_request(&image, None);
    assert_eq!(m[0]["content"][0]["image_url"]["url"], url.as_str());
}

/// The preview elides a long prompt; the REQUEST never does. One formatter
/// builds both, so this pins the only difference between them.
#[test]
fn the_preview_elides_the_prompt_and_the_request_does_not() {
    let long: String = "x".repeat(PREVIEW_PROMPT_CHARS * 3);
    let plan = Plan {
        model_profiles: vec!["a".into()],
        prompt: long.clone(),
        ..Plan::default()
    };
    let preview = live_preview(crate::server_cfg::DEFAULT_PORT as u16, "a", &plan);
    assert!(preview.contains("chars shown]"), "the preview says it cut");
    assert!(preview.len() < long.len(), "and actually cut");

    let body = live_body("a", &plan);
    let parsed: serde_json::Value = serde_json::from_str(&body).unwrap();
    assert_eq!(parsed["messages"][0]["content"], serde_json::json!(long));
    assert_eq!(parsed["cache_prompt"], serde_json::json!(false));
}

/// What makes an old comparison checkable: the report identifies the prompt
/// by path AND digest, and embeds the text only up to a cap.
#[test]
fn the_report_identifies_the_prompt_by_path_and_digest() {
    let plan = Plan {
        model_profiles: vec!["a".into()],
        prompt: SHORT.into(),
        prompt_file: Some(PathBuf::from(r"D:\prompts\short.txt")),
        ..Plan::default()
    };
    let md = render_report(&plan, &[], &RunMeta::default());
    assert!(md.contains(r"short.txt"), "the path is in the report");
    assert!(md.contains(&crate::sha256::hex(SHORT.as_bytes())));
    assert!(md.contains(SHORT), "a short prompt is embedded whole");

    // Inline (`--prompt`) says so instead of printing an invented path.
    let inline = Plan {
        prompt_file: None,
        ..plan.clone()
    };
    assert!(render_report(&inline, &[], &RunMeta::default()).contains("--prompt"));

    // Past the cap the text is cut, the digest is not.
    let long = Plan {
        prompt: "y".repeat(REPORT_PROMPT_CHARS * 2),
        ..plan
    };
    let md = render_report(&long, &[], &RunMeta::default());
    assert!(md.contains("chars shown]"));
    assert!(
        md.len() < REPORT_PROMPT_CHARS * 2,
        "the report stayed small"
    );
    assert!(md.contains(&crate::sha256::hex(long.prompt.as_bytes())));
}

#[test]
fn stamps_are_utc_and_sort_chronologically() {
    // 2026-08-31 12:34:56 UTC.
    let secs = 1_788_179_696;
    assert_eq!(stamp(secs), "20260831-123456");
    assert_eq!(stamp_human(secs), "2026-08-31 12:34:56 UTC");
    assert!(stamp(secs) > stamp(secs - 1));
    assert_eq!(stamp(0), "19700101-000000");
}

#[test]
fn the_report_names_what_the_mode_cannot_measure() {
    let plan = Plan {
        model_profiles: vec!["a".into()],
        mode: Mode::Synthetic,
        ..Plan::default()
    };
    let md = render_report(&plan, &[], &RunMeta::default());
    assert!(md.contains("synthetic (llama-bench)"));
    assert!(md.contains("No results were collected."));
    assert!(md.contains("No speculative decoding"));
    assert!(md.contains("takes no prompt"));

    let plan = Plan {
        mode: Mode::Live,
        ..plan
    };
    let md = render_report(&plan, &[], &RunMeta::default());
    assert!(md.contains("cache_prompt"));
    assert!(md.contains("## Prompt"));
}
