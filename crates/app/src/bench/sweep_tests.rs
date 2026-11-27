use super::*;

/// The sweep's up-front check runs the save's checks: a value the setter
/// takes but the save refuses (`parallel = 0`) stops `run` with the save's
/// own refusal before any leg. A unit test, not in the e2e phase, because
/// `run` is CLI-only code (`binding_lint`'s `CLI_ONLY_FNS`), which only the
/// tests of `mod tests` may call. It touches no `paths::`: everything is
/// read through the fake seam, and `reps: 0` is a plan `run` refuses right
/// after the value check, before the server, the marker or bench.db.
#[test]
fn run_refuses_a_value_the_save_refuses_before_any_leg() {
    use crate::server_control::fake::{self, FakeClient};
    use pai_core::client::InstanceState;
    let log = fake::log();
    let c = FakeClient::new(&log, InstanceState::Inactive).with_file(
        "models.ini",
        "[Qwen3.8-27B]\nmodel = /models/Qwen3.8-27B-Q8_0.gguf\nparallel = 2\n",
    );
    let refused = pai_core::checks::Refusal {
        key: "not-whole-number-in-range",
        values: ["parallel", "0", "1", "2147483647"]
            .iter()
            .map(|v| v.to_string())
            .collect(),
    }
    .text();
    let opts = Opts {
        model_profile: "Qwen3.8-27B".into(),
        key: Some("parallel".into()),
        values: vec!["2".into(), "0".into()],
        plan: Plan {
            reps: 0,
            ..Plan::default()
        },
        apply: Apply::Original,
        restore_to: None,
        campaign: String::new(),
        question: String::new(),
    };
    let err = fake::scoped(fake::backend(c), || run(&opts))
        .expect_err("parallel 0 refused before any leg");
    assert!(err.contains(&refused), "sweep: {err}");
    assert_eq!(fake::writes(&log), Vec::<String>::new(), "nothing written");
}

fn leg(value: &str, decode: f64, note: &str) -> Leg {
    Leg {
        value: value.into(),
        points: vec![Point {
            model_profile: "P".into(),
            test: super::super::LIVE_DECODE.into(),
            mean: decode,
            sd: 0.5,
            n: 3,
            note: note.into(),
        }],
        error: String::new(),
        secs: 300,
    }
}

/// Does the rendered section actually EMIT the key, on a line of its own?
/// A plain `contains` cannot tell: `render_section` documents each field in
/// a comment above it, and those comments spell the key out with its `=`
/// ("; spec-draft-n-max = max drafted tokens per step"), so a setter that
/// did nothing at all would still pass a substring check.
fn emits(ini: &str, key: &str) -> bool {
    let head = format!("{key} = ");
    ini.lines().any(|l| {
        let line = l.trim_start();
        !line.starts_with(';') && line.starts_with(&head)
    })
}

fn opts(values: &[&str]) -> Opts {
    Opts {
        model_profile: "Qwen".into(),
        key: Some("spec-draft-n-max".into()),
        values: values.iter().map(|v| (*v).to_string()).collect(),
        plan: Plan::default(),
        apply: Apply::Original,
        restore_to: None,
        campaign: String::new(),
        question: String::new(),
    }
}

#[test]
fn values_split_on_commas_and_expand_ranges() {
    assert_eq!(parse_values("2,3, 4").unwrap(), ["2", "3", "4"]);
    assert_eq!(parse_values("2..5").unwrap(), ["2", "3", "4", "5"]);
    assert_eq!(parse_values("1,3..5,9").unwrap(), ["1", "3", "4", "5", "9"]);
}

/// A value that itself contains a comma (`device`, `tensor-split`) is only
/// reachable through the `;` form, which is the whole reason it exists.
#[test]
fn semicolon_wins_when_present() {
    assert_eq!(
        parse_values("Vulkan1,CUDA0;CUDA0;ROCm0,CUDA0").unwrap(),
        ["Vulkan1,CUDA0", "CUDA0", "ROCm0,CUDA0"]
    );
}

#[test]
fn empty_and_backwards_and_huge_ranges_are_errors() {
    assert!(parse_values("  ").is_err());
    assert!(parse_values(",,").is_err());
    assert!(parse_values("8..2").unwrap_err().contains("backwards"));
    assert!(parse_values("1..500").unwrap_err().contains("500 values"));
}

/// A token that is not a range stays a literal, dots included: `1.5` is a
/// value, not half of a range.
#[test]
fn non_range_tokens_stay_literal() {
    assert_eq!(
        parse_values("1.5,q8_0,a..b").unwrap(),
        ["1.5", "q8_0", "a..b"]
    );
}

#[test]
fn unknown_key_names_the_sweepable_ones() {
    let e = setter("spec-draft-nmax").unwrap_err();
    assert!(e.contains("spec-draft-n-max"), "{e}");
    assert!(e.contains("tensor-split"), "{e}");
}

/// Every entry in the table writes a key `render_section` actually emits.
/// A key that only existed here would sweep nothing: the leg would measure
/// the unchanged model profile and report it as a data point.
#[test]
fn every_sweepable_key_reaches_the_ini() {
    for (key, set) in SWEEPABLE {
        let mut p = model_profiles::ModelProfile {
            id: "T".into(),
            model: "C:\\m.gguf".into(),
            ..Default::default()
        };
        let value = match *key {
            "flash-attn" | "mmproj-offload" | "backend-sampling" | "op-offload" | "repack"
            | "no-host" | "kv-unified" => "true",
            "device" => "CUDA0",
            "tensor-split" => "3,1",
            "split-mode" => "layer",
            "lazy-mode" => "on",
            "load-mode" => "mmap",
            "override-tensor" => "token_embd\\.weight=CUDA0",
            "cache-type-k" | "cache-type-v" | "spec-draft-type-k" | "spec-draft-type-v" => "q8_0",
            "spec-type" => "draft-mtp",
            "model-draft" => "C:\\d.gguf",
            "device-draft" => "CUDA0",
            "spec-draft-p-min" => "0.5",
            _ => "7",
        };
        set(&mut p, value).unwrap_or_else(|e| panic!("{key}: {e}"));
        let ini = model_profiles::render_section(&p);
        assert!(emits(&ini, key), "{key} never reaches models.ini");
    }
}

#[test]
fn unset_clears_a_key() {
    let mut p = model_profiles::ModelProfile {
        id: "T".into(),
        model: "C:\\m.gguf".into(),
        spec_draft_n_max: Some(6),
        flash_attn: Some(true),
        ..Default::default()
    };
    setter("spec-draft-n-max").unwrap()(&mut p, UNSET).unwrap();
    setter("flash-attn").unwrap()(&mut p, "UNSET").unwrap();
    assert_eq!(p.spec_draft_n_max, None);
    assert_eq!(p.flash_attn, None);
    let ini = model_profiles::render_section(&p);
    assert!(!emits(&ini, "spec-draft-n-max"));
    assert!(!emits(&ini, "flash-attn"));
}

#[test]
fn bad_values_fail_before_the_gpu_time() {
    assert!(setter("spec-draft-n-max").unwrap()(
        &mut model_profiles::ModelProfile::default(),
        "six"
    )
    .is_err());
    assert!(
        setter("flash-attn").unwrap()(&mut model_profiles::ModelProfile::default(), "maybe")
            .is_err()
    );
    // A `;` or `#` in a text value reloads truncated, so it is refused here
    // exactly as `model_profiles::save` refuses it.
    assert!(
        setter("device").unwrap()(&mut model_profiles::ModelProfile::default(), "CUDA0;x").is_err()
    );
}

#[test]
fn best_is_the_highest_primary_mean() {
    let legs = [leg("2", 39.2, ""), leg("5", 59.3, ""), leg("6", 43.6, "")];
    assert_eq!(best(&legs, super::super::LIVE_DECODE), Some(1));
    assert_eq!(best(&legs, "prefill"), None);
    assert_eq!(best(&[], super::super::LIVE_DECODE), None);
}

/// A leg that failed keeps its row: "this value does not load" is a result,
/// and a table that omits it reads as if the value was never tried.
#[test]
fn summary_keeps_failed_legs_and_ranks_the_rest() {
    let legs = vec![
        leg("2", 39.2, "draft accepted 85%"),
        Leg {
            value: "3".into(),
            error: "llama-server answered HTTP 500".into(),
            ..Default::default()
        },
        leg("5", 59.3, "draft accepted 80%"),
    ];
    let o = opts(&["2", "3", "5"]);
    let md = render_summary(&Summary {
        opts: &o,
        legs: &legs,
        original: "6",
        applied: "6",
        stamp: "2026-09-01 18:00:00 UTC",
        server_version: "0.3.0 · b10621",
    });
    assert!(
        md.contains("| spec-draft-n-max | decode | +- | vs 2 |"),
        "{md}"
    );
    assert!(md.contains("| 2 | 39.2 | 0.5 | 1.00x |"), "{md}");
    assert!(md.contains("| 5 | 59.3 | 0.5 | 1.51x |"), "{md}");
    assert!(
        md.contains("FAILED: llama-server answered HTTP 500"),
        "{md}"
    );
    assert!(md.contains("1 of 3 legs failed"), "{md}");
    assert!(
        md.contains("**Best: spec-draft-n-max = 5** at 59.3 t/s decode, 1.51x the first value."),
        "{md}"
    );
    // The value to put back is in the table, not only in the marker.
    assert!(
        md.contains("| spec-draft-n-max before the sweep | 6 |"),
        "{md}"
    );
}

#[test]
fn header_json_carries_the_value_to_restore() {
    let o = opts(&["2", "3"]);
    let line = header_json(&o, "6", "20260901-180000");
    let v: serde_json::Value = serde_json::from_str(&line).unwrap();
    assert_eq!(v["kind"], "sweep");
    assert_eq!(v["restore"], "6");
    assert_eq!(v["key"], "spec-draft-n-max");
    assert_eq!(v["values"][1], "3");
}

/// A marker that is not a sweep header is not a sweep: a truncated file,
/// an empty one, or anything else that got the name.
#[test]
fn sweep_state_reads_a_marker_and_ignores_anything_else() {
    let o = opts(&["3", "4"]);
    let state = sweep_state(&header_json(&o, "6", "20260901-192445")).unwrap();
    assert_eq!(state.restore, "6");
    assert_eq!(state.key, "spec-draft-n-max");
    assert_eq!(state.model_profile, "Qwen");
    assert!(sweep_state("").is_none());
    assert!(sweep_state("{\"kind\":\"run\",\"mode\":\"live\"}").is_none());
    assert!(sweep_state("not json at all").is_none());
}

#[test]
fn synthetic_ranks_on_the_first_tg_row() {
    let mut l = leg("2", 0.0, "");
    l.points = vec![
        Point {
            model_profile: "P".into(),
            test: "pp2048".into(),
            mean: 900.0,
            ..Default::default()
        },
        Point {
            model_profile: "P".into(),
            test: "tg128".into(),
            mean: 21.0,
            ..Default::default()
        },
    ];
    assert_eq!(primary_test(Mode::Synthetic, &[l]), "tg128");
    assert_eq!(primary_test(Mode::Live, &[]), "decode");
}
