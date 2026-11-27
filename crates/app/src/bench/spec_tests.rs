use super::*;

const A: &str = "96d8fb4f7d56d59ee53aa145394c8f8adb0122adbe16421391dcfd5576aaaae6";
const SAMPLE: &str = "[Spec]\nName = standard\nVersion = 1\n\
        Prompts = bench-prompt-43k.txt@96d8fb4f7d56d59ee53aa145394c8f8adb0122adbe16421391dcfd5576aaaae6, \
        /x/y.txt@96D8FB4F7D56D59EE53AA145394C8F8ADB0122ADBE16421391DCFD5576AAAAE6\nMaxTokens = 0\n\n\
        [Pass.engine]\nTemp = 0\nOverride = spec-type=none\nReps = 3\n\n\
        [Pass.real]\nTemp = preset\nConcurrency = all\nReps = 5\n";

/// A spec saved by the configurator names it by its new name.
#[test]
fn a_saved_spec_names_pai_config() {
    let cfg = Path::new("/cfg");
    let text = to_ini(&parse(SAMPLE, cfg).unwrap(), cfg);
    assert!(
        text.lines()
            .next()
            .unwrap()
            .contains("(saved by pai-config)"),
        "{text}"
    );
    assert!(!text.contains(concat!("llama-cpp", "-config")), "{text}");
}

#[test]
fn spec_type_none_also_removes_the_draft_file() {
    let o = |k: &str, v: &str| (k.to_string(), v.to_string());
    assert_eq!(
        effective_overrides(&[o("spec-type", "none")]),
        vec![o("spec-type", "none"), o("model-draft", sweep::UNSET)]
    );
    // Any other speculator keeps the file, and a list that names it wins.
    assert_eq!(
        effective_overrides(&[o("spec-type", "draft-mtp")]),
        vec![o("spec-type", "draft-mtp")]
    );
    let named = [o("spec-type", "none"), o("model-draft", "x.gguf")];
    assert_eq!(effective_overrides(&named), named.to_vec());
}

#[test]
fn a_spec_parses_its_passes_and_resolves_bare_prompt_names() {
    let s = parse(SAMPLE, Path::new("/cfg")).unwrap();
    assert_eq!((s.name.as_str(), s.version.as_str()), ("standard", "1"));
    assert_eq!(s.sha256.len(), 64);
    assert_eq!(
        s.prompts,
        vec![
            (PathBuf::from("/cfg/bench-prompt-43k.txt"), A.to_string()),
            (PathBuf::from("/x/y.txt"), A.to_string())
        ]
    );
    assert_eq!(s.max_tokens, 0);
    assert_eq!(s.passes.len(), 2);
    let e = &s.passes[0];
    assert_eq!(
        (e.temp, e.reps, e.concurrency),
        (Some(0.0), 3, Some(Concurrency::Single))
    );
    assert_eq!(
        e.overrides,
        vec![("spec-type".to_string(), "none".to_string())]
    );
    let r = &s.passes[1];
    assert_eq!((r.temp, r.reps, r.concurrency), (None, 5, None));
    assert!(r.overrides.is_empty());
}

#[test]
fn a_pass_can_pick_its_prompts_and_warm_a_prefix() {
    let with = SAMPLE.replace(
        "[Pass.real]\nTemp = preset\n",
        "[Pass.real]\nTemp = preset\nPrompts = y.txt\nWarm = prefix\n",
    );
    let s = parse(&with, Path::new("/cfg")).unwrap();
    let real = &s.passes[1];
    assert_eq!(real.prompts, vec!["y.txt".to_string()]);
    assert!(real.warm && real.runs("y.txt") && !real.runs("bench-prompt-43k.txt"));
    assert!(s.passes[0].runs("anything.txt") && !s.passes[0].warm);
    // The keys enter the digest, and a spec without them keeps its own.
    assert_ne!(s.sha256, parse(SAMPLE, Path::new("/cfg")).unwrap().sha256);
    assert!(canonical(&s).contains(" prompts=y.txt warm=prefix\n"));
    assert!(!canonical(&parse(SAMPLE, Path::new("/cfg")).unwrap()).contains("prompts="));
    // Saved and read back, the same rules.
    let again = parse(&to_ini(&s, Path::new("/cfg")), Path::new("/cfg")).unwrap();
    assert_eq!(again.passes, s.passes);
    for bad in ["Prompts = other.txt", "Warm = yes"] {
        let text = SAMPLE.replace("[Pass.real]\n", &format!("[Pass.real]\n{bad}\n"));
        assert!(parse(&text, Path::new("/cfg")).is_err(), "{bad}");
    }
}

#[test]
fn a_warm_pass_skips_prefill_and_needs_an_agent_turn() {
    let s = parse(
        &SAMPLE.replace("Concurrency = all\n", "Concurrency = all\nWarm = prefix\n"),
        Path::new("/cfg"),
    )
    .unwrap();
    let conv = r#"{"messages":[{"role":"user","content":"a"},{"role":"assistant","content":"b"},{"role":"user","content":"c"}]}"#;
    let got = pass_scenarios(&s.passes[1], conv, None, Concurrency::ALL.to_vec()).unwrap();
    assert!(!got.contains(&Concurrency::Prefill) && got.contains(&Concurrency::PrefillDecode));
    assert!(pass_scenarios(&s.passes[1], "plain text", None, Concurrency::ALL.to_vec()).is_err());
    assert_eq!(
        pass_scenarios(&s.passes[0], "plain text", None, Concurrency::ALL.to_vec())
            .unwrap()
            .len(),
        4
    );
}

#[test]
fn a_spec_refuses_what_it_cannot_run() {
    let cfg = Path::new("/cfg");
    assert!(parse("[Pass.a]\nTemp = 0\n", cfg).is_err());
    assert!(parse(&SAMPLE.replace("Name = standard", "Name = custom"), cfg).is_err());
    assert!(parse(&SAMPLE.replace("Reps = 3", "Reps = 0"), cfg).is_err());
    assert!(parse(
        &SAMPLE.replace("Concurrency = all", "Concurrency = many"),
        cfg
    )
    .is_err());
    assert!(parse(&SAMPLE.replace("spec-type=none", "no-such-key=1"), cfg).is_err());
    let no_pass = format!("[Spec]\nName = a\nVersion = 1\nPrompts = p.txt@{A}\n");
    assert!(parse(&no_pass, cfg).is_err());
    let unpinned = SAMPLE.replace(&format!("bench-prompt-43k.txt@{A}"), "bench-prompt-43k.txt");
    assert!(parse(&unpinned, cfg).is_err());
}

/// The digest follows the rules, not the text: a comment is free, a
/// changed repetition count is another method.
#[test]
fn the_digest_covers_the_rules_and_not_the_comments() {
    let cfg = Path::new("/cfg");
    let base = parse(SAMPLE, cfg).unwrap().sha256;
    let commented = format!("; a note\n{SAMPLE}");
    assert_eq!(parse(&commented, cfg).unwrap().sha256, base);
    let changed = SAMPLE.replace("Reps = 5", "Reps = 6");
    assert_ne!(parse(&changed, cfg).unwrap().sha256, base);
}

/// Every published version of the standard spec is frozen: its digest is
/// in `standard.versions`, so a rule changed without a new `Version`, or a
/// published version edited in place, fails here.
/// Anything that is not the spec's own rules is recorded: a changed
/// field, a swept key a pass sets, rules that are no published version.
#[test]
fn a_run_off_the_spec_says_what_it_changed() {
    let std = load("standard").unwrap();
    let none = Derive::default();
    let (_, d) = prompts_and_derived(&std, &none, Some("split-mode")).unwrap();
    assert!(d.is_empty(), "standard as published, a key no pass sets");
    let (_, d) = prompts_and_derived(&std, &none, Some("spec-type")).unwrap();
    assert_eq!(d, ["override"]);
    let temp = Derive {
        temp: Some(None),
        prompt: Some(vec![(None, "hi".into())]),
        ..Derive::default()
    };
    let (prompts, d) = prompts_and_derived(&std, &temp, None).unwrap();
    assert_eq!(prompts, [(None, "hi".to_string())]);
    assert_eq!(d, ["prompt", "temp"]);
}

/// What Save as… writes reads back as the same rules.
#[test]
fn a_saved_spec_reads_back_as_the_same_rules() {
    let cfg = Path::new("/cfg");
    let std = load("standard").unwrap();
    let mine = Spec {
        name: "mine".into(),
        ..std.clone()
    };
    let back = parse(&to_ini(&mine, cfg), cfg).unwrap();
    assert_eq!(back.passes, mine.passes);
    assert_eq!(back.prompts, mine.prompts);
    assert_eq!(
        (back.max_tokens, back.name.as_str()),
        (mine.max_tokens, "mine")
    );
    assert!(!back.published);
    // Named standard with the same rules, it IS the published version.
    assert!(parse(&to_ini(&std, cfg), cfg).unwrap().published);
}

/// Every published version runs by name, the newest as `standard`; a
/// version that is not published is refused with the reason.
#[test]
fn every_published_standard_version_runs() {
    let published = published_versions(STANDARD_VERSIONS);
    for (v, sha) in &published {
        let s = standard_version(v).unwrap();
        assert_eq!(&s.sha256, sha);
        assert!(s.published);
        assert_eq!(load(&format!("standard-{v}")).unwrap(), s);
    }
    let newest = published
        .iter()
        .map(|(v, _)| v)
        .max_by_key(|v| version_key(v))
        .unwrap();
    assert_eq!(&load("standard").unwrap().version, newest);
    assert!(standard_version("999.0")
        .unwrap_err()
        .contains("not a published version"));
    assert!(version_key("1.10") > version_key("1.9"));
}

/// Every installed `standard-<X.Y>.ini` is frozen: it is the version its
/// name says, and its digest is that version's line in
/// `standard.versions`, so a rule changed without a new version, or a
/// published version edited in place, fails here. And every published
/// version is still installed, so an older one can still be run.
#[test]
fn the_standard_spec_is_a_published_version() {
    let published = published_versions(STANDARD_VERSIONS);
    let mut seen = std::collections::HashSet::new();
    for (v, _) in &published {
        assert!(seen.insert(v.clone()), "version {v} is listed twice");
        assert!(
            specs_dir().join(format!("standard-{v}.ini")).is_file(),
            "standard {v} is published but standard-{v}.ini is gone: keep every version"
        );
    }
    let mut files = 0;
    for e in std::fs::read_dir(specs_dir()).unwrap().flatten() {
        let name = e.file_name().to_string_lossy().into_owned();
        let Some(v) = name
            .strip_prefix("standard-")
            .and_then(|n| n.strip_suffix(".ini"))
        else {
            continue;
        };
        files += 1;
        let s = load_file(&e.path()).unwrap();
        assert_eq!(s.version, v, "{name} says Version {}", s.version);
        let pinned = published.iter().find(|(pv, _)| pv == v).unwrap_or_else(|| {
            panic!(
                "{name}: {v} is not in standard.versions: add `{v} {}`",
                s.sha256
            )
        });
        assert_eq!(
            pinned.1, s.sha256,
            "{name} changed its rules but kept Version {v}: add a new standard-<X.Y>.ini \
                 and the line `<X.Y> {}`",
            s.sha256
        );
    }
    assert!(
        files > 0,
        "no standard-<X.Y>.ini in {}",
        specs_dir().display()
    );
}

/// The tab's steps: prompt x pass x scenario, each pass's overrides on its
/// steps only, the plan at the pass's rules and tagged with it.
#[test]
fn the_tab_runs_one_step_per_prompt_pass_and_scenario() {
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(dir.path().join("p.txt"), "hello").unwrap();
    let sha = super::super::prompt_info("hello").sha256;
    let text = format!(
        "[Spec]\nName = t\nVersion = 2\nPrompts = p.txt@{sha}\nMaxTokens = 0\n\n\
             [Pass.engine]\nTemp = 0\nOverride = spec-type=none\nConcurrency = single\nReps = 3\n\n\
             [Pass.real]\nTemp = preset\nConcurrency = all\nReps = 5\n"
    );
    let spec = parse(&text, dir.path()).unwrap();
    let base = Plan {
        mode: super::super::Mode::Live,
        ..Plan::default()
    };
    let steps = tab_steps(&spec, &base, &Derive::default(), None).unwrap();
    let labels: Vec<&str> = steps.iter().map(|s| s.label.as_str()).collect();
    assert_eq!(
        labels,
        [
            "engine · p.txt · single",
            "real · p.txt · single",
            "real · p.txt · prefill",
            "real · p.txt · decode",
            "real · p.txt · prefill+decode",
        ]
    );
    assert_eq!(
        steps[0].overrides,
        [("spec-type".to_string(), "none".to_string())]
    );
    assert!(steps[1..].iter().all(|s| s.overrides.is_empty()));
    assert_eq!((steps[0].plan.temp, steps[0].plan.reps), (Some(0.0), 3));
    assert_eq!((steps[1].plan.temp, steps[1].plan.reps), (None, 5));
    assert_eq!(steps[0].plan.prompt, "hello");
    assert_eq!(
        steps[4].plan.spec.as_ref().map(|t| t.pass.as_str()),
        Some("real")
    );
    // A spec file that is no published standard version runs, but every
    // run of it is custom, derived from it.
    assert!(!spec.published);
    assert_eq!(steps[0].plan.spec.as_ref().unwrap().derived, ["spec-file"]);
    let synthetic = Plan {
        mode: super::super::Mode::Synthetic,
        ..Plan::default()
    };
    assert!(tab_steps(&spec, &synthetic, &Derive::default(), None).is_err());

    // Override: the changed fields replace the spec's in every step, and
    // every run is recorded as derived from it.
    let derive = Derive {
        reps: Some(2),
        concurrency: Some(Some(Concurrency::Single)),
        ..Derive::default()
    };
    let steps = tab_steps(&spec, &base, &derive, None).unwrap();
    assert_eq!(steps.len(), 2, "one scenario per pass");
    assert!(steps.iter().all(|s| s.plan.reps == 2));
    assert_eq!(
        steps[0].plan.temp,
        Some(0.0),
        "the pass's temperature stays"
    );
    assert_eq!(
        steps[1].plan.spec.as_ref().unwrap().derived,
        ["concurrency", "reps", "spec-file"],
        "a spec that is no published standard version is a derivation too"
    );
}

/// An image prompt runs only on model profiles with an mmproj: with none
/// selected, its steps are left out and the text prompt's stay.
#[test]
fn an_image_prompt_needs_a_model_profile_with_an_mmproj() {
    let dir = tempfile::tempdir().unwrap();
    let image = r#"{"messages": [{"role": "user", "content": [
            {"type": "image_url", "image_url": {"url": "data:image/jpeg;base64,AAAA"}},
            {"type": "text", "text": "Describe it."}]}]}"#;
    std::fs::write(dir.path().join("t.txt"), "hello").unwrap();
    std::fs::write(dir.path().join("i.json"), image).unwrap();
    let sha = |t: &str| super::super::prompt_info(t).sha256;
    let text = format!(
        "[Spec]\nName = t\nVersion = 2\nPrompts = t.txt@{}, i.json@{}\nMaxTokens = 0\n\n\
             [Pass.engine]\nTemp = 0\nConcurrency = single\nReps = 3\n",
        sha("hello"),
        sha(image)
    );
    let spec = parse(&text, dir.path()).unwrap();
    let base = Plan {
        mode: super::super::Mode::Live,
        ..Plan::default()
    };
    let steps = tab_steps(&spec, &base, &Derive::default(), None).unwrap();
    let labels: Vec<&str> = steps.iter().map(|s| s.label.as_str()).collect();
    assert_eq!(labels, ["engine · t.txt · single"]);
}

/// A/B over a key the engine pass overrides: every step once per value,
/// the value over the pass's own, and the runs recorded as derived.
#[test]
fn an_ab_key_runs_every_step_per_value() {
    let spec = load("standard").unwrap();
    let base = Plan {
        mode: super::super::Mode::Live,
        model_profiles: vec![],
        ..Plan::default()
    };
    let derive = Derive {
        concurrency: Some(Some(Concurrency::Single)),
        ..Derive::default()
    };
    let steps = tab_steps(&spec, &base, &derive, Some("spec-type")).unwrap();
    let n = steps.len();
    let values = vec!["none".to_string(), "draft-mtp".to_string()];
    let ab = ab_expand(steps, "spec-type", &values);
    assert_eq!(ab.len(), 2 * n);
    assert!(ab[0].label.starts_with("spec-type=none · "));
    assert_eq!(
        ab[n].overrides,
        [("spec-type".to_string(), "draft-mtp".to_string())]
    );
    assert!(ab[n]
        .plan
        .spec
        .as_ref()
        .unwrap()
        .derived
        .contains(&"override".to_string()));
}

/// Every published version pins prompts that are shipped, with the
/// digest they have: a shipped prompt edited in place fails here, not on
/// someone's machine.
#[test]
fn the_standard_spec_pins_the_shipped_prompts() {
    for (v, _) in published_versions(STANDARD_VERSIONS) {
        let s = standard_version(&v).unwrap();
        assert!(!s.prompts.is_empty());
        for (path, pinned) in &s.prompts {
            assert_eq!(
                path.parent(),
                Some(crate::paths::bench_prompts_dir().as_path())
            );
            let text = super::super::load_prompt_file(path).unwrap();
            assert_eq!(
                &super::super::prompt_info(&text).sha256,
                pinned,
                "{}",
                path.display()
            );
        }
    }
}
