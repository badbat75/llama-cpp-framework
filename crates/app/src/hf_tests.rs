use super::*;
use std::collections::HashMap;

const COMMIT: &str = "0123456789abcdef0123456789abcdef01234567";

fn f(path: &str, size: u64) -> RepoFile {
    RepoFile {
        path: path.into(),
        size,
        oid: format!("oid-{path}"),
    }
}

fn spec(repo: &str, tag: Option<&str>) -> RepoSpec {
    RepoSpec {
        repo: repo.into(),
        tag: tag.map(str::to_string),
        file: None,
    }
}

// ── spec ──

#[test]
fn spec_parses_the_three_forms() {
    assert_eq!(
        parse_spec(" unsloth/Qwen3-8B-GGUF ").unwrap(),
        spec("unsloth/Qwen3-8B-GGUF", None)
    );
    assert_eq!(
        parse_spec("org/repo:q4_k_m").unwrap(),
        spec("org/repo", Some("q4_k_m"))
    );
    assert_eq!(
        parse_spec("https://huggingface.co/org/repo").unwrap(),
        spec("org/repo", None)
    );
    assert_eq!(
        parse_spec("huggingface.co/org/repo/tree/main?x=1#y").unwrap(),
        spec("org/repo", None)
    );
    assert_eq!(
        parse_spec("HTTPS://hf.co/org/repo:Q8_0").unwrap(),
        spec("org/repo", Some("Q8_0"))
    );
    let blob = parse_spec("https://huggingface.co/org/repo/blob/main/sub/m-Q4_K_M.gguf").unwrap();
    assert_eq!(blob.file.as_deref(), Some("sub/m-Q4_K_M.gguf"));
    assert_eq!(blob.repo, "org/repo");
}

#[test]
fn spec_refuses_what_llama_cpp_would() {
    for bad in [
        "",
        "repo",
        "a/b/c",
        "org/-repo",
        "org//repo",
        "org/repo:",
        "org/repo:Q4 K",
        "org/re po",
        "https://example.com/org/repo",
        "https://huggingface.co/datasets/org/data",
        "https://huggingface.co/org",
    ] {
        assert!(parse_spec(bad).is_err(), "{bad:?} should be refused");
    }
}

// ── token, endpoint, redaction ──

#[test]
fn token_debug_never_prints_the_secret() {
    let t = Token::new(" hf_abcdefghijklmnopqrstuvwxyz0123456789 ").unwrap();
    assert_eq!(format!("{t:?}"), "Token(***)");
    assert!(Token::new("hf_a b").is_err());
    assert!(Token::new("hf_\"x").is_err());
    assert!(Token::new("  ").is_err());
    assert_eq!(
        redact(
            "Bearer hf_abcdefghijklmnopqrstuvwxyz0123456789 bad",
            Some(&t)
        ),
        "Bearer *** bad"
    );
    assert_eq!(redact("plain", None), "plain");
}

#[test]
fn token_comes_from_env_then_the_cli_token_file() {
    let read =
        |p: &Path| (p == Path::new("/h/.cache/huggingface/token")).then(|| "hf_file\n".to_string());
    let home = if cfg!(windows) { "USERPROFILE" } else { "HOME" };
    let env = |vars: Vec<(&'static str, &'static str)>| {
        let m: HashMap<&str, &str> = vars.into_iter().collect();
        move |k: &str| m.get(k).map(|v| v.to_string())
    };
    assert_eq!(
        token_from(env(vec![("HF_TOKEN", "hf_env"), (home, "/h")]), read),
        Some(Token("hf_env".into()))
    );
    assert_eq!(
        token_from(env(vec![(home, "/h")]), read),
        Some(Token("hf_file".into()))
    );
    // A padded HF_TOKEN is trimmed; a blank one falls through to the file.
    assert_eq!(
        token_from(env(vec![("HF_TOKEN", " hf_env \n"), (home, "/h")]), read),
        Some(Token("hf_env".into()))
    );
    assert_eq!(
        token_from(env(vec![("HF_TOKEN", "  "), (home, "/h")]), read),
        Some(Token("hf_file".into()))
    );
    // HF_HOME moves the file; a missing file is no token, not an error.
    assert_eq!(
        token_from(env(vec![("HF_HOME", "/elsewhere"), (home, "/h")]), read),
        None
    );
    assert_eq!(token_from(env(vec![]), read), None);
}

#[test]
fn endpoint_follows_llama_cpp_precedence() {
    assert_eq!(endpoint_from(None, None), "https://huggingface.co/");
    assert_eq!(
        endpoint_from(None, Some("https://mirror.test".into())),
        "https://mirror.test/"
    );
    assert_eq!(
        endpoint_from(
            Some("https://a.test/".into()),
            Some("https://b.test".into())
        ),
        "https://a.test/"
    );
    assert_eq!(
        endpoint_from(Some(String::new()), None),
        "https://huggingface.co/"
    );
}

// ── curl plumbing ──

#[test]
fn curl_config_carries_url_and_token_and_the_status_writeout() {
    let t = Token::new("hf_secret").unwrap();
    let c = curl_config("https://h/api/models/o/r/refs", Some(&t)).unwrap();
    assert!(c.contains("url = \"https://h/api/models/o/r/refs\"\n"));
    assert!(c.contains("header = \"Authorization: Bearer hf_secret\"\n"));
    assert!(c.contains("write-out = \"\\n%{http_code}\"\n"));
    assert!(!c.contains("range"));
    assert!(!curl_config("u", None).unwrap().contains("Authorization"));
    assert!(
        c.contains(&format!(
            "user-agent = \"pai-config/{}\"\n",
            env!("CARGO_PKG_VERSION")
        )),
        "{c}"
    );
    let r = curl_config_with(
        "https://h/o/r/resolve/c/m.gguf",
        None,
        "*/*",
        Some((0, 1023)),
    )
    .unwrap();
    assert!(
        r.contains("range = \"0-1023\"\nmax-filesize = 66560\n"),
        "{r}"
    );
}

#[test]
fn curl_config_values_cannot_break_out_of_their_quotes() {
    assert_eq!(curl_value(r#"a"b\c"#).unwrap(), r#""a\"b\\c""#);
    // A newline would end the line and start an option of its own.
    for bad in ["u\nurl = \"https://evil\"", "u\r", "u\0", "u\u{1b}"] {
        assert!(
            matches!(curl_config(bad, None), Err(HfError::InvalidSpec(_))),
            "{bad:?}"
        );
    }
    let c = curl_config("https://h/x\"y", None).unwrap();
    assert!(c.starts_with("url = \"https://h/x\\\"y\"\n"), "{c}");
    assert_eq!(
        c.lines().count(),
        4,
        "one line per option, nothing smuggled"
    );
}

#[test]
fn binary_curl_output_splits_at_the_last_newline() {
    let mut out = b"GGUF\n\x00\x01\n".to_vec();
    out.extend_from_slice(b"\n206");
    assert_eq!(
        parse_curl_bytes(out).unwrap(),
        (206, b"GGUF\n\x00\x01\n".to_vec())
    );
    assert!(matches!(
        parse_curl_bytes(b"\n000".to_vec()),
        Err(HfError::Network(_))
    ));
    assert!(parse_curl_bytes(b"junk".to_vec()).is_err());
}

#[test]
fn curl_output_splits_body_and_status() {
    assert_eq!(
        parse_curl_output("{\"a\":1}\n200").unwrap(),
        (200, "{\"a\":1}".into())
    );
    assert_eq!(
        parse_curl_output("line1\nline2\n404").unwrap(),
        (404, "line1\nline2".into())
    );
    assert!(parse_curl_output("garbage").is_err());
    assert!(matches!(
        parse_curl_output("\n000"),
        Err(HfError::Network(_))
    ));
}

#[test]
fn curl_exit_codes_read_as_sentences() {
    assert!(curl_failure(6, "x").to_string().contains("resolve"));
    assert!(curl_failure(60, "x").to_string().contains("TLS"));
    assert!(curl_failure(99, "boom").to_string().contains("exit 99"));
}

#[test]
fn api_statuses_map_to_actionable_errors() {
    assert_eq!(
        api_error(404, "", false, "o/r"),
        HfError::NotFoundOrPrivate { repo: "o/r".into() }
    );
    assert_eq!(
        api_error(401, "", false, "o/r"),
        HfError::NotFoundOrPrivate { repo: "o/r".into() }
    );
    assert_eq!(
        api_error(401, "", true, "o/r"),
        HfError::TokenRejected { repo: "o/r".into() }
    );
    assert_eq!(
        api_error(403, "", true, "o/r"),
        HfError::Gated {
            repo: "o/r".into(),
            had_token: true
        }
    );
    assert_eq!(
        api_error(500, "{\"error\":\"oops\"}", false, "o/r"),
        HfError::Http {
            status: 500,
            message: "oops".into()
        }
    );
}

// ── llama.cpp mirrors ──

#[test]
fn split_info_reads_tag_and_shards_like_upstream() {
    let s = split_info("Q4_K_M/Model-UD-Q4_K_XL-00002-of-00003.gguf");
    assert_eq!((s.tag.as_str(), s.index, s.count), ("Q4_K_XL", 2, 3));
    assert_eq!(s.prefix, "Q4_K_M/Model-UD-Q4_K_XL");
    assert_eq!(split_info("model.Q8_0.gguf").tag, "Q8_0");
    assert_eq!(split_info("model.gguf").tag, "");
    assert_eq!(split_info("model.GGUF"), SplitInfo::default());
    assert_eq!(quant_bits("m-BF16.gguf"), 16);
    assert_eq!(quant_bits("m-Q4_K_M.gguf"), 4);
    assert_eq!(quant_bits("m.gguf"), 0);
}

#[test]
fn model_files_exclude_sidecars_by_file_name_only() {
    assert!(is_model_file("Qwen3-8B-Q4_K_M.gguf"));
    assert!(is_model_file("mmproj-dir/Qwen3-8B-Q4_K_M.gguf"));
    for side in [
        "mmproj-F16.gguf",
        "mtp-Q8_0.gguf",
        "x-dflash-Q4.gguf",
        "dspark-q.gguf",
        "imatrix.gguf",
        "eagle3-a.gguf",
    ] {
        assert!(!is_model_file(side), "{side}");
    }
    assert!(!is_model_file("README.md"));
}

#[test]
fn best_model_prefers_q4_k_m_then_q8_0_then_the_first() {
    let files = vec![
        f("m-Q8_0.gguf", 8),
        f("m-Q4_K_M.gguf", 4),
        f("m-Q2_K.gguf", 2),
    ];
    assert_eq!(find_best_model(&files, "").path, "m-Q4_K_M.gguf");
    assert_eq!(find_best_model(&files, "q2_k").path, "m-Q2_K.gguf");
    assert_eq!(find_best_model(&files, "Q5_K_M").path, "");
    // The tag is a whole token: F16 is not BF16 (#31620), `_` does not
    // separate (#31639), a directory does.
    let twins = [f("m-BF16.gguf", 1), f("m-F16.gguf", 1)];
    assert_eq!(find_best_model(&twins, "F16").path, "m-F16.gguf");
    assert_eq!(find_best_model(&[f("m_Q8_0.gguf", 1)], "Q8_0").path, "");
    assert_eq!(
        find_best_model(&[f("Q8_0/Q8_0.gguf", 1)], "Q8_0").path,
        "Q8_0/Q8_0.gguf"
    );
    let only = vec![f("m-IQ3_XS.gguf", 3)];
    assert_eq!(find_best_model(&only, "").path, "m-IQ3_XS.gguf");
    // A later shard never wins, even when it comes first.
    let split = vec![
        f("m-Q4_K_M-00002-of-00002.gguf", 1),
        f("m-Q4_K_M-00001-of-00002.gguf", 1),
    ];
    assert_eq!(
        find_best_model(&split, "").path,
        "m-Q4_K_M-00001-of-00002.gguf"
    );
}

#[test]
fn best_sibling_prefers_depth_then_exact_tag_then_closest_bits() {
    let files = vec![
        f("mmproj-F32.gguf", 1),
        f("mmproj-F16.gguf", 1),
        f("BF16/mmproj-BF16.gguf", 1),
        f("BF16/m-BF16.gguf", 1),
        f("m-Q8_0.gguf", 1),
        f("other/mmproj-Q8_0.gguf", 1),
    ];
    // A root model: the root projectors only (not other/), closest bits to 8.
    assert_eq!(
        find_best_sibling(&files, "m-Q8_0.gguf", "mmproj", "").path,
        "mmproj-F16.gguf"
    );
    // A model in BF16/: its own directory wins over the root.
    assert_eq!(
        find_best_sibling(&files, "BF16/m-BF16.gguf", "mmproj", "").path,
        "BF16/mmproj-BF16.gguf"
    );
    // An exact tag beats closer bits.
    let mtps = vec![f("mtp-Q4_0.gguf", 1), f("mtp-Q8_0.gguf", 1)];
    assert_eq!(
        find_best_sibling(&mtps, "m-Q8_0.gguf", "mtp-", "Q8_0").path,
        "mtp-Q8_0.gguf"
    );
    assert_eq!(
        find_best_sibling(&mtps, "m-Q4_K_M.gguf", "mtp-", "Q4_K_M").path,
        "mtp-Q4_0.gguf"
    );
    assert_eq!(find_best_sibling(&mtps, "m.gguf", "dflash-", "").path, "");
}

// ── listing ──

/// Canned API answers keyed by url.
struct Canned(HashMap<String, (u16, String)>);
impl Fetch for Canned {
    fn get(&self, url: &str, _token: Option<&Token>) -> Result<(u16, String), HfError> {
        self.0
            .get(url)
            .cloned()
            .ok_or_else(|| HfError::Network(format!("unexpected url {url}")))
    }
    fn get_range(
        &self,
        url: &str,
        _token: Option<&Token>,
        _start: u64,
        _len: u64,
    ) -> Result<(u16, Vec<u8>), HfError> {
        Err(HfError::Network(format!("unexpected range request {url}")))
    }
}

fn canned_repo() -> Canned {
    let refs = format!(
        r#"{{"branches":[{{"name":"dev","targetCommit":"{}"}},{{"name":"main","targetCommit":"{COMMIT}"}}]}}"#,
        "f".repeat(40)
    );
    let tree = r#"[
            {"type":"directory","path":"sub"},
            {"type":"file","path":"README.md","size":12,"oid":"aa"},
            {"type":"file","path":"Q-8B-Q8_0.gguf","size":10,"lfs":{"oid":"o1","size":8000}},
            {"type":"file","path":"Q-8B-Q4_K_M.gguf","size":10,"lfs":{"oid":"o2","size":4000}},
            {"type":"file","path":"Q-8B-BF16-00001-of-00002.gguf","lfs":{"oid":"o3","size":9000}},
            {"type":"file","path":"Q-8B-BF16-00002-of-00002.gguf","lfs":{"oid":"o4","size":7000}},
            {"type":"file","path":"mmproj-F16.gguf","lfs":{"oid":"o5","size":600}},
            {"type":"file","path":"mtp-Q8_0.gguf","lfs":{"oid":"o6","size":300}},
            {"type":"file","path":"dflash-Q4_K_M.gguf","lfs":{"oid":"o7","size":200}}
        ]"#;
    let e = "https://hf.test/";
    Canned(HashMap::from([
        (format!("{e}api/models/o/r/refs"), (200, refs)),
        (
            format!("{e}api/models/o/r/tree/{COMMIT}?recursive=true"),
            (200, tree.to_string()),
        ),
    ]))
}

#[test]
fn listing_groups_shards_and_pairs_sidecars() {
    let l = list_repo_with(&canned_repo(), "https://hf.test/", &spec("o/r", None), None).unwrap();
    assert_eq!(l.commit, COMMIT);
    assert_eq!(l.project_url, "https://hf.test/o/r");
    let names: Vec<&str> = l
        .candidates
        .iter()
        .map(|c| c.primary().path.as_str())
        .collect();
    assert_eq!(
        names,
        [
            "Q-8B-Q8_0.gguf",
            "Q-8B-Q4_K_M.gguf",
            "Q-8B-BF16-00001-of-00002.gguf"
        ]
    );
    assert_eq!(l.default, 1, "Q4_K_M is upstream's default pick");
    let bf16 = &l.candidates[2];
    assert_eq!(
        (bf16.tag.as_str(), bf16.shards.len(), bf16.size),
        ("BF16", 2, 16000)
    );
    let q4 = &l.candidates[1];
    assert_eq!(q4.size, 4000, "the LFS size, not the pointer's");
    assert_eq!(q4.mmproj.as_ref().unwrap().path, "mmproj-F16.gguf");
    assert_eq!(q4.mtp.as_ref().unwrap().path, "mtp-Q8_0.gguf");
    assert_eq!(q4.dflash.as_ref().unwrap().path, "dflash-Q4_K_M.gguf");
    assert!(q4.dspark.is_none());
    assert_eq!(q4.sidecar(Sidecar::Mtp), q4.mtp.as_ref());
}

#[test]
fn listing_default_follows_the_spec() {
    let c = canned_repo();
    let tagged = list_repo_with(&c, "https://hf.test/", &spec("o/r", Some("bf16")), None).unwrap();
    assert_eq!(tagged.default, 2);
    let file = RepoSpec {
        file: Some("Q-8B-Q8_0.gguf".into()),
        ..spec("o/r", None)
    };
    assert_eq!(
        list_repo_with(&c, "https://hf.test/", &file, None)
            .unwrap()
            .default,
        0
    );
    let missing = list_repo_with(&c, "https://hf.test/", &spec("o/r", Some("Q2_K")), None);
    assert!(matches!(missing, Err(HfError::NoMatch { .. })));
}

#[test]
fn listing_refuses_model_profile_and_empty_repos_and_maps_api_errors() {
    let s = spec("o/r", None);
    let model_profile = build_listing(
        "o/r",
        COMMIT,
        vec![f("preset.ini", 1), f("m-Q4_K_M.gguf", 1)],
        &s,
        "e/",
    );
    assert_eq!(
        model_profile,
        Err(HfError::ModelProfileRepo { repo: "o/r".into() })
    );
    let empty = build_listing(
        "o/r",
        COMMIT,
        vec![f("README.md", 1), f("mmproj-F16.gguf", 1)],
        &s,
        "e/",
    );
    assert_eq!(empty, Err(HfError::NoGguf { repo: "o/r".into() }));
    let gated = Canned(HashMap::from([(
        "https://hf.test/api/models/o/r/refs".to_string(),
        (403, "{\"error\":\"gated\"}".to_string()),
    )]));
    let tok = Token::new("hf_x").unwrap();
    assert_eq!(
        list_repo_with(&gated, "https://hf.test/", &s, Some(&tok)),
        Err(HfError::Gated {
            repo: "o/r".into(),
            had_token: true
        })
    );
}

#[test]
fn refs_pick_main_else_the_first_valid_branch() {
    let v: Value = serde_json::from_str(&format!(
        r#"{{"branches":[{{"name":"x","targetCommit":"bad"}},{{"name":"y","targetCommit":"{COMMIT}"}}]}}"#
    ))
    .unwrap();
    assert_eq!(commit_from_refs(&v).as_deref(), Some(COMMIT));
    assert_eq!(commit_from_refs(&serde_json::json!({"branches": []})), None);
}

// ── search ──

/// A real `/api/models?search=...&expand[]=...` answer (huggingface.co,
/// 2026-10-27), trimmed: four repos from three searches, file lists cut
/// to what the rules read (tinygemma3 also carries ~100 test PNGs).
const SEARCH_FIXTURE: &str = r#"[
        {"_id":"681881ef3319aedbfd81dc51","id":"Qwen/Qwen3-0.6B-GGUF","gated":false,"private":false,"downloads":231589,
         "siblings":[{"rfilename":".gitattributes"},{"rfilename":"LICENSE"},{"rfilename":"Qwen3-0.6B-Q8_0.gguf"},{"rfilename":"README.md"},{"rfilename":"params"}]},
        {"_id":"6a01d228550577cadfac8843","id":"unsloth/Qwen3.6-35B-A3B-MTP-GGUF","gated":false,"private":false,"downloads":1251077,
         "siblings":[{"rfilename":"BF16/Qwen3.6-35B-A3B-BF16-00001-of-00002.gguf"},{"rfilename":"BF16/Qwen3.6-35B-A3B-BF16-00002-of-00002.gguf"},
                     {"rfilename":"Qwen3.6-35B-A3B-MXFP4_MOE.gguf"},{"rfilename":"Qwen3.6-35B-A3B-Q8_0.gguf"},{"rfilename":"Qwen3.6-35B-A3B-UD-IQ2_M.gguf"},
                     {"rfilename":"Qwen3.6-35B-A3B-UD-Q4_K_M.gguf"},{"rfilename":"Qwen3.6-35B-A3B-UD-Q4_K_XL.gguf"},{"rfilename":"imatrix_unsloth.gguf_file"},
                     {"rfilename":"mmproj-BF16.gguf"},{"rfilename":"mmproj-F16.gguf"}]},
        {"_id":"67d1f1f08dbf13ec3de1e1fd","id":"google/gemma-3-4b-it-qat-q4_0-gguf","gated":"manual","trendingScore":4,"private":false,"downloads":3330,
         "siblings":[{"rfilename":".gitattributes"},{"rfilename":"README.md"},{"rfilename":"gemma-3-4b-it-q4_0.gguf"},{"rfilename":"mmproj-model-f16-4B.gguf"}]},
        {"_id":"681a7471f403e658abe3f32f","id":"ggml-org/tinygemma3-GGUF","gated":false,"downloads":73988,
         "siblings":[{"rfilename":"README.md"},{"rfilename":"mmproj-tinygemma3.gguf"},{"rfilename":"test/0_cat.png"},{"rfilename":"tinygemma3-Q8_0.gguf"}]}
    ]"#;

#[test]
fn input_is_a_spec_when_it_parses_else_a_query_unless_it_is_a_mistyped_spec() {
    assert_eq!(classify_input("   ").unwrap(), None);
    assert_eq!(
        classify_input(" org/repo:Q4_K_M ").unwrap(),
        Some(Input::Spec(spec("org/repo", Some("Q4_K_M"))))
    );
    assert_eq!(
        classify_input("https://huggingface.co/org/repo").unwrap(),
        Some(Input::Spec(spec("org/repo", None)))
    );
    assert_eq!(
        classify_input("qwen3   8b").unwrap(),
        Some(Input::Query("qwen3 8b".into()))
    );
    assert_eq!(
        classify_input("tinygemma3").unwrap(),
        Some(Input::Query("tinygemma3".into()))
    );
    // A slash with a space is words, not a repo id.
    assert_eq!(
        classify_input("unsloth/ qwen3").unwrap(),
        Some(Input::Query("unsloth/ qwen3".into()))
    );
    // Slash-carrying, no space, not a repository: the spec's own error.
    for bad in [
        "org/repo:",
        "a/b/c",
        "https://example.com/org/repo",
        "https://huggingface.co/org",
    ] {
        assert!(
            matches!(classify_input(bad), Err(HfError::InvalidSpec(_))),
            "{bad:?}"
        );
    }
    assert!(parse_query(&"x".repeat(201)).is_err());
    assert!(parse_query("a\u{7}b").is_err());
}

#[test]
fn the_search_url_carries_every_verified_parameter_encoded() {
    let u = search_url("https://hf.test/", "qwen3 8b/ü&x", 30);
    assert_eq!(
        u,
        "https://hf.test/api/models?search=qwen3%208b%2F%C3%BC%26x&filter=gguf&sort=downloads&direction=-1\
             &limit=30&expand%5B%5D=downloads&expand%5B%5D=gated&expand%5B%5D=private&expand%5B%5D=siblings\
             &expand%5B%5D=gguf"
    );
    assert!(
        !u.contains('[') && !u.contains(' ') && !u.contains('"'),
        "nothing curl globs or the config quotes"
    );
    assert!(search_url("e/", "q", 0).contains("&limit=1&"));
}

#[test]
fn search_answers_parse_into_rows_with_quants_and_sidecars() {
    let e = "https://hf.test/";
    let canned = Canned(HashMap::from([(
        search_url(e, "qwen3", 30),
        (200, SEARCH_FIXTURE.to_string()),
    )]));
    let hits = search_with(&canned, e, " qwen3 ", 30, None).unwrap();
    let repos: Vec<&str> = hits.iter().map(|h| h.repo.as_str()).collect();
    assert_eq!(
        repos,
        [
            "Qwen/Qwen3-0.6B-GGUF",
            "unsloth/Qwen3.6-35B-A3B-MTP-GGUF",
            "google/gemma-3-4b-it-qat-q4_0-gguf",
            "ggml-org/tinygemma3-GGUF"
        ],
        "the API's order (most downloaded first) is kept"
    );
    let q = &hits[0];
    assert_eq!(
        (q.downloads, q.gated, q.private, q.mmproj),
        (231589, false, false, false)
    );
    assert_eq!(q.quants, ["Q8_0"]);
    let moe = &hits[1];
    assert_eq!(
        moe.quants,
        ["IQ2_M", "MXFP4_MOE", "Q4_K_M", "Q4_K_XL", "Q8_0", "BF16"],
        "a split once, UD- read as upstream reads it"
    );
    assert!(moe.mmproj && moe.drafters.is_empty());
    let gated = &hits[2];
    assert!(gated.gated, "\"manual\" is gated");
    assert_eq!(gated.quants, ["Q4_0"]);
    assert!(!hits[3].private, "a missing field is not private");
    assert_eq!(hits[3].quants, ["Q8_0"]);
    // Drafter files by their prefix; an entry without an id is skipped.
    let v = serde_json::json!([
        {"id": "o/r", "siblings": [{"rfilename": "m-Q4_K_M.gguf"}, {"rfilename": "mtp-Q8_0.gguf"}, {"rfilename": "x-dflash-Q4.gguf"}]},
        {"siblings": []}
    ]);
    let h = hits_from_search(&v).unwrap();
    assert_eq!(h.len(), 1);
    assert_eq!(h[0].drafters, [Sidecar::Mtp, Sidecar::Dflash]);
    assert_eq!(
        (h[0].downloads, h[0].gated, h[0].quants.as_slice()),
        (0, false, &["Q4_K_M".to_string()][..])
    );

    // Ids are the remote's data: one `parse_spec` would refuse is dropped.
    let v = serde_json::json!([
        {"id": "../evil", "siblings": [{"rfilename": "m-Q4_K_M.gguf"}]},
        {"id": "o/r\nx", "siblings": []},
        {"id": "a/b/c", "siblings": []},
        {"id": "o/ok", "siblings": []}
    ]);
    let h = hits_from_search(&v).unwrap();
    assert_eq!(
        h.iter().map(|h| h.repo.as_str()).collect::<Vec<_>>(),
        ["o/ok"]
    );

    let empty = Canned(HashMap::from([(
        search_url(e, "zzz", 5),
        (200, "[]".to_string()),
    )]));
    assert_eq!(search_with(&empty, e, "zzz", 5, None).unwrap(), []);
    let odd = Canned(HashMap::from([(
        search_url(e, "zzz", 5),
        (200, "{\"error\":\"x\"}".to_string()),
    )]));
    assert!(matches!(
        search_with(&odd, e, "zzz", 5, None),
        Err(HfError::Http { status: 200, .. })
    ));
}

#[test]
fn the_gguf_summary_gives_kind_and_parameter_count() {
    // Shapes of real `expand[]=gguf` answers (2026-10-27), template cut.
    let v = serde_json::json!([
        {"id": "unsloth/Qwen3-30B-A3B-GGUF", "gguf": {"total": 30532122624u64, "architecture": "qwen3moe", "context_length": 40960, "chat_template": "{%...%}"}},
        {"id": "Qwen/Qwen3-8B-GGUF", "gguf": {"total": 8190735360u64, "architecture": "qwen3"}},
        {"id": "TheBloke/Mixtral-8x7B-v0.1-GGUF", "gguf": {"total": 46702792704u64, "architecture": "llama"}},
        {"id": "bartowski/Meta-Llama-3.1-8B-Instruct-GGUF", "gguf": {"total": 8030261312u64, "architecture": "llama"}},
        {"id": "o/no-summary"}
    ]);
    let h = hits_from_search(&v).unwrap();
    let labels: Vec<Option<String>> = h.iter().map(|h| kind_label(h.kind, h.params)).collect();
    assert_eq!(
        labels,
        [
            Some("MoE 30.5B".to_string()),
            Some("dense 8.2B".to_string()),
            Some("MoE 46.7B".to_string()),
            Some("8.0B, dense/MoE unknown".to_string()),
            None
        ]
    );
}

#[test]
fn model_kind_trusts_the_arch_then_the_name_and_never_guesses() {
    assert_eq!(model_kind(Some("qwen3moe"), "o/x"), ModelKind::Moe);
    assert_eq!(model_kind(Some("deepseek2"), "o/x"), ModelKind::Moe);
    assert_eq!(model_kind(Some("gpt-oss"), "o/x"), ModelKind::Moe);
    assert_eq!(model_kind(Some("mistral-large4"), "o/x"), ModelKind::Moe);
    assert_eq!(model_kind(Some("qwen3"), "o/Qwen3-8B"), ModelKind::Dense);
    assert_eq!(model_kind(Some("gemma3"), "o/x"), ModelKind::Dense);
    // `llama` serves Mixtral too: only the name can tell.
    assert_eq!(
        model_kind(Some("llama"), "o/Llama-3.1-8B"),
        ModelKind::Unknown
    );
    assert_eq!(
        model_kind(Some("llama"), "o/Mixtral-8x22B-v0.1"),
        ModelKind::Moe
    );
    assert_eq!(
        model_kind(None, "o/Qwen3-Coder-30B-A3B-Instruct-GGUF"),
        ModelKind::Moe
    );
    assert_eq!(model_kind(None, "o/Ling-A1.5B"), ModelKind::Moe);
    assert_eq!(model_kind(None, "o/Qwen3-8B"), ModelKind::Unknown);
    // Words that merely look alike are not size patterns.
    for name in ["o/alpha-b", "o/ab", "o/AWQ-8B", "o/x-7b", "o/max-8b"] {
        assert_eq!(
            model_kind(Some("llama"), name),
            ModelKind::Unknown,
            "{name}"
        );
    }
}

#[test]
fn parameter_counts_read_short() {
    assert_eq!(format_params(30_532_122_624), "30.5B");
    assert_eq!(format_params(8_190_735_360), "8.2B");
    assert_eq!(format_params(235_093_634_560), "235B");
    assert_eq!(format_params(1_026_000_000_000), "1.0T");
    assert_eq!(format_params(596_049_920), "596M");
    assert_eq!(format_params(5_000), "5000");
    assert_eq!(kind_label(ModelKind::Moe, None).as_deref(), Some("MoE"));
    assert_eq!(kind_label(ModelKind::Dense, None).as_deref(), Some("dense"));
}

#[test]
fn search_errors_name_no_repository() {
    assert_eq!(
        search_error(401, "", true),
        HfError::TokenRejected {
            repo: strings::t("the-search")
        }
    );
    assert!(
        matches!(search_error(429, "", false), HfError::Network(m) if m.contains("rate limited"))
    );
    assert_eq!(
        search_error(400, "{\"error\":\"bad expand\"}", false),
        HfError::Http {
            status: 400,
            message: "bad expand".into()
        }
    );
    assert!(matches!(
        search_error(404, "", false),
        HfError::Http { status: 404, .. }
    ));
}

#[test]
fn quants_come_one_per_model_file_ordered_by_bits() {
    let paths = [
        "m-Q8_0.gguf",
        "m-Q4_K_M.gguf",
        "Q4_K_M/m-Q4_K_M.gguf",
        "BF16/m-BF16-00002-of-00002.gguf",
        "BF16/m-BF16-00001-of-00002.gguf",
        "mtp-Q8_0.gguf",
        "mmproj-F16.gguf",
        "model.gguf",
        "README.md",
    ];
    assert_eq!(quants_of(&paths), ["Q4_K_M", "Q8_0", "BF16", "model"]);
    assert!(quants_of(&["mmproj-F16.gguf"]).is_empty());
}

#[test]
fn download_counts_read_short() {
    assert_eq!(format_count(37), "37");
    assert_eq!(format_count(73_988), "74.0k");
    assert_eq!(format_count(644_587), "644.6k");
    assert_eq!(format_count(999_960), "1.0M");
    assert_eq!(format_count(1_251_077), "1.3M");
}

// ── download job ──

fn listing() -> Listing {
    list_repo_with(&canned_repo(), "https://hf.test/", &spec("o/r", None), None).unwrap()
}

fn job(models_dir: &Path, idx: usize, mmproj: bool, sidecar: Option<Sidecar>) -> DownloadJob {
    let l = listing();
    DownloadJob {
        repo: l.repo.clone(),
        commit: l.commit.clone(),
        project_url: l.project_url.clone(),
        candidate: l.candidates[idx].clone(),
        mmproj,
        sidecar,
        models_dir: models_dir.to_path_buf(),
        token: Some(Token::new("hf_tok").unwrap()),
        server_exe: PathBuf::from("llama-server"),
    }
}

/// Whether a write into `dir` really fails (root ignores the mode).
#[cfg(unix)]
fn write_refused(dir: &Path) -> bool {
    let probe = dir.join("probe");
    let refused = std::fs::write(&probe, b"x").is_err();
    let _ = std::fs::remove_file(&probe);
    refused
}

#[cfg(unix)]
fn set_mode(dir: &Path, mode: u32) {
    use std::os::unix::fs::PermissionsExt;
    std::fs::set_permissions(dir, std::fs::Permissions::from_mode(mode)).expect("chmod");
}

/// A download into a ModelsDir the user cannot write (under the Windows
/// manager, the instance's `%ProgramData%\llama.cpp\models` for a user
/// who is not elevated) is refused at once, naming the folder, before a
/// child is started; not left to fail inside llama-server.
#[cfg(unix)]
#[test]
fn a_download_into_a_folder_the_user_cannot_write_is_refused_naming_it() {
    let tmp = tempfile::tempdir().unwrap();
    let models = tmp.path().join("models");
    std::fs::create_dir(&models).unwrap();
    let missing = tmp.path().join("ro").join("models");
    std::fs::create_dir(tmp.path().join("ro")).unwrap();
    set_mode(&models, 0o555);
    set_mode(&tmp.path().join("ro"), 0o555);
    if !write_refused(&models) {
        set_mode(&models, 0o755);
        set_mode(&tmp.path().join("ro"), 0o755);
        eprintln!("skipped: the mode does not stop this user (root)");
        return;
    }
    let results: Vec<(PathBuf, Result<DownloadJob, HfError>)> = [models.clone(), missing]
        .into_iter()
        .map(|dir| {
            let r = DownloadJob::for_server(
                &listing(),
                0,
                false,
                None,
                dir.clone(),
                None,
                PathBuf::from("llama-server"),
            );
            (dir, r)
        })
        .collect();
    let left: Vec<_> = std::fs::read_dir(&models).unwrap().collect();
    set_mode(&models, 0o755);
    set_mode(&tmp.path().join("ro"), 0o755);
    for (dir, r) in results {
        let err = r
            .err()
            .unwrap_or_else(|| panic!("{} was accepted", dir.display()));
        assert_eq!(
            err.to_string(),
            strings::fill("download-folder-not-writable", &[&dir.display()]),
            "{}",
            dir.display()
        );
    }
    assert!(left.is_empty(), "files left in the folder: {left:?}");
}

/// A writable folder, or one not created yet under a writable parent,
/// is accepted, and the check leaves no file behind.
#[test]
fn a_download_into_a_writable_folder_is_accepted_and_leaves_nothing() {
    let tmp = tempfile::tempdir().unwrap();
    let models = tmp.path().join("models");
    std::fs::create_dir(&models).unwrap();
    for dir in [models.clone(), tmp.path().join("new").join("models")] {
        let job = DownloadJob::for_server(
            &listing(),
            0,
            false,
            None,
            dir.clone(),
            None,
            PathBuf::from("llama-server"),
        )
        .unwrap_or_else(|e| panic!("{}: {e}", dir.display()));
        assert_eq!(job.models_dir, dir);
    }
    let left: Vec<_> = std::fs::read_dir(&models).unwrap().collect();
    assert!(left.is_empty(), "files left in the folder: {left:?}");
}

/// The refusal is text the user reads, in both strings files (the GUI's
/// dialog and `pai model-profile download`), with the folder's slot.
#[test]
fn the_unwritable_folder_refusal_is_in_both_strings_files() {
    for (name, text) in [
        ("en_US.toml", include_str!("../../core/strings/en_US.toml")),
        ("cli.toml", include_str!("../../core/strings/cli.toml")),
    ] {
        let line = text
            .lines()
            .find(|l| l.starts_with("download-folder-not-writable ="))
            .unwrap_or_else(|| panic!("download-folder-not-writable is not in {name}"));
        assert!(
            line.contains("{0}"),
            "{name}: no {{0}} for the folder: {line}"
        );
    }
}

#[test]
fn placements_land_in_the_scanned_subfolders_per_repo() {
    let root = Path::new("M");
    let j = job(root, 2, true, Some(Sidecar::Dspark));
    // BF16 has no dspark sidecar: only the shards and the mmproj.
    let t: Vec<PathBuf> = j
        .placements()
        .unwrap()
        .into_iter()
        .map(|p| p.target)
        .collect();
    // `<org>\<repo>`: two orgs' same-named repos never share a folder.
    assert_eq!(
        t,
        [
            root.join("models")
                .join("o")
                .join("r")
                .join("Q-8B-BF16-00001-of-00002.gguf"),
            root.join("models")
                .join("o")
                .join("r")
                .join("Q-8B-BF16-00002-of-00002.gguf"),
            // Named after the model, not the generic HF name.
            root.join("mmprojs")
                .join("o")
                .join("r")
                .join("Q-8B-mmproj-F16.gguf"),
        ]
    );
    // Every quant of the repo puts the shared projector at ONE target, so
    // the second download finds it in place.
    let mm = |idx| {
        job(root, idx, true, None)
            .placements()
            .unwrap()
            .pop()
            .unwrap()
    };
    assert_eq!(mm(0).role, Role::Mmproj);
    assert_eq!(mm(0).target, mm(1).target);
    assert_eq!(mm(1).target, t[2]);
    let mut other_org = job(root, 2, true, None);
    other_org.repo = "p/r".into();
    assert_ne!(other_org.placements().unwrap()[0].target, t[0]);
    let j = job(root, 1, false, Some(Sidecar::Dflash));
    let p = j.placements().unwrap();
    assert_eq!(p.len(), 2);
    assert_eq!(p[1].role, Role::Draft(Sidecar::Dflash));
    assert_eq!(
        p[1].target,
        root.join("dflashs")
            .join("o")
            .join("r")
            .join("dflash-Q4_K_M.gguf")
    );
    let g = groups(&j.placements().unwrap())
        .into_iter()
        .map(|g| g.len())
        .collect::<Vec<_>>();
    assert_eq!(g, [1, 1]);
    assert_eq!(
        groups(&job(root, 2, true, None).placements().unwrap()).len(),
        2,
        "shards share one child"
    );
}

#[test]
fn the_mmproj_is_named_after_the_model() {
    let n = mmproj_target_name;
    // No quant tag on the mmproj; one on it (lower-case read upper).
    assert_eq!(
        n("tinygemma3-Q8_0.gguf", "mmproj-tinygemma3.gguf"),
        "tinygemma3-mmproj.gguf"
    );
    assert_eq!(
        n("Qwen3-VL-8B-Instruct-Q4_K_M.gguf", "mmproj-F16.gguf"),
        "Qwen3-VL-8B-Instruct-mmproj-F16.gguf"
    );
    assert_eq!(
        n("gemma-3-4b-it-q4_0.gguf", "mmproj-model-f16.gguf"),
        "gemma-3-4b-it-mmproj-F16.gguf"
    );
    assert_eq!(
        n("m-Q8_0.gguf", "BF16/mmproj-BF16.gguf"),
        "m-mmproj-BF16.gguf"
    );
    // A tail that is not a quant is not a tag (the size, the model's name).
    assert_eq!(
        n("gemma-3-4b-it-q4_0.gguf", "mmproj-model-f16-4B.gguf"),
        "gemma-3-4b-it-mmproj.gguf"
    );
    assert_eq!(
        n("Qwen3-30B-A3B.gguf", "mmproj-F16.gguf"),
        "Qwen3-30B-A3B-mmproj-F16.gguf"
    );
    // A split's first shard: the shard suffix and the tag both go.
    assert_eq!(
        n("Qwen3-VL-32B-BF16-00001-of-00002.gguf", "mmproj-BF16.gguf"),
        "Qwen3-VL-32B-mmproj-BF16.gguf"
    );
    // Dots and spaces in the stem are part of the name.
    assert_eq!(
        n("Llama-3.2-11B-Vision.Q4_K_M.gguf", "mmproj-F16.gguf"),
        "Llama-3.2-11B-Vision-mmproj-F16.gguf"
    );
    assert_eq!(
        n("My Vision Model v1.5-Q6_K.gguf", "mmproj.gguf"),
        "My Vision Model v1.5-mmproj.gguf"
    );
    // Unusual tags: Unsloth's `UD-` goes with its tag, IQ and ternary quants.
    assert_eq!(
        n("Qwen3-VL-30B-A3B-UD-Q4_K_XL.gguf", "mmproj-F16.gguf"),
        "Qwen3-VL-30B-A3B-mmproj-F16.gguf"
    );
    assert_eq!(
        n("Qwen3-VL-30B-A3B-UD-IQ2_XXS.gguf", "mmproj-F16.gguf"),
        "Qwen3-VL-30B-A3B-mmproj-F16.gguf"
    );
    assert_eq!(
        n("m-IQ2_XXS.gguf", "mmproj-Q8_0.gguf"),
        "m-mmproj-Q8_0.gguf"
    );
    assert_eq!(
        n("m-TQ1_0.gguf", "mmproj-MXFP4.gguf"),
        "m-mmproj-MXFP4.gguf"
    );
    // Two quants of one repo share their projector's name.
    for model in [
        "Qwen3-VL-8B-Instruct-Q4_K_M.gguf",
        "Qwen3-VL-8B-Instruct-Q8_0.gguf",
        "Qwen3-VL-8B-Instruct-UD-Q6_K_XL.gguf",
    ] {
        assert_eq!(
            n(model, "mmproj-F16.gguf"),
            "Qwen3-VL-8B-Instruct-mmproj-F16.gguf",
            "{model}"
        );
    }
    // A name with no `-`/`.` has no tag to cut; one that is nothing but
    // its tag keeps the HF name.
    assert_eq!(
        n("Q4_K_M.gguf", "mmproj-F16.gguf"),
        "Q4_K_M-mmproj-F16.gguf"
    );
    assert_eq!(n("-Q4_K_M.gguf", "mmproj-F16.gguf"), "mmproj-F16.gguf");
}

#[test]
fn quant_tags_are_told_from_other_name_tails() {
    for q in [
        "Q4_K_M", "Q8_0", "IQ2_XXS", "IQ4_NL", "TQ2_0", "F16", "F32", "BF16", "MXFP4", "NVFP4",
        "Q4_K_XL",
    ] {
        assert!(is_quant_tag(q), "{q}");
    }
    for t in [
        "",
        "8B",
        "A3B",
        "TINYGEMMA3",
        "INSTRUCT",
        "FP8X",
        "QAT",
        "UD",
    ] {
        assert!(!is_quant_tag(t), "{t}");
    }
}

#[test]
fn placements_refuse_remote_paths_and_ids_that_escape_the_models_dir() {
    let root = Path::new("M");
    for bad in [
        "../x.gguf",
        "a/../../x.gguf",
        "./x.gguf",
        "a//x.gguf",
        "/x.gguf",
        "a\\..\\x.gguf",
        "C:x.gguf",
        "c:/x.gguf",
        "x\n.gguf",
        "",
    ] {
        assert!(!safe_repo_path(bad), "{bad:?}");
        let mut j = job(root, 1, false, None);
        j.candidate.shards[0].path = bad.to_string();
        assert!(
            matches!(j.placements(), Err(HfError::InvalidSpec(_))),
            "{bad:?}"
        );
    }
    assert!(safe_repo_path("BF16/m-BF16-00001-of-00002.gguf"));
    assert!(
        safe_repo_path("a..b/x.gguf"),
        "dots inside a name are a name"
    );
    let mut j = job(root, 1, false, None);
    j.repo = "../r".into();
    assert!(matches!(j.placements(), Err(HfError::InvalidSpec(_))));
    // A sidecar is checked like a shard.
    let mut j = job(root, 1, true, None);
    j.candidate.mmproj.as_mut().unwrap().path = "../../evil.gguf".into();
    assert!(j.placements().is_err());
}

#[test]
fn the_listing_drops_unsafe_tree_paths() {
    let tree = serde_json::json!([
        {"type": "file", "path": "../evil-Q4_K_M.gguf", "size": 1},
        {"type": "file", "path": "a\\b-Q8_0.gguf", "size": 1},
        {"type": "file", "path": "ok-Q4_K_M.gguf", "size": 1}
    ]);
    let files = files_from_tree(&tree);
    assert_eq!(
        files.iter().map(|f| f.path.as_str()).collect::<Vec<_>>(),
        ["ok-Q4_K_M.gguf"]
    );
    let l = build_listing(
        "o/r",
        COMMIT,
        vec![f("/abs-Q8_0.gguf", 1), f("m-Q4_K_M.gguf", 1)],
        &spec("o/r", None),
        "e/",
    )
    .unwrap();
    assert_eq!(l.candidates.len(), 1);
    assert_eq!(l.candidates[0].primary().path, "m-Q4_K_M.gguf");
}

#[test]
fn lexical_containment_folds_dots_and_refuses_climbing_out() {
    let root = Path::new("S").join("repo");
    assert!(lexically_under(
        &root
            .join("snapshots")
            .join("c")
            .join("..")
            .join("..")
            .join("blobs")
            .join("x"),
        &root
    ));
    assert!(!lexically_under(&root.join("..").join("other"), &root));
    assert!(!lexically_under(
        &root.join("..").join("..").join("..").join("x"),
        &root
    ));
    assert!(!lexically_under(Path::new("elsewhere"), &root));
}

#[test]
fn child_env_names_one_file_and_hides_the_token_from_argv() {
    let j = job(Path::new("M"), 1, true, None);
    let (set, clear) = child_env(&j, "Q-8B-Q4_K_M.gguf");
    let get = |k: &str| set.iter().find(|(n, _)| *n == k).map(|(_, v)| v.clone());
    assert!(get("LLAMA_SERVER_ROUTER_PORT").is_some());
    assert_eq!(get("LLAMA_SERVER_CHILD_MODE").as_deref(), Some("download"));
    assert_eq!(get("LLAMA_ARG_HF_REPO").as_deref(), Some("o/r"));
    assert_eq!(
        get("LLAMA_ARG_HF_FILE").as_deref(),
        Some("Q-8B-Q4_K_M.gguf")
    );
    assert_eq!(
        get("LLAMA_CACHE").map(PathBuf::from),
        Some(Path::new("M").join(STAGING_DIR))
    );
    assert_eq!(get("HF_TOKEN").as_deref(), Some("hf_tok"));
    assert!(!CHILD_ARGS.iter().any(|a| a.contains("hf_tok")));
    assert_eq!(CHILD_ARGS, ["--no-mmproj"]);
    // The inherited variables that would change the plan, by their names in
    // llama.cpp's common/arg.cpp (`set_env`). RE-CHECK on a llama.cpp bump:
    // a renamed variable silently stops being cleared.
    assert_eq!(
        clear,
        [
            "LLAMA_ARG_MODEL",
            "LLAMA_ARG_MODEL_URL",
            "LLAMA_ARG_DOCKER_REPO",
            "LLAMA_ARG_MODELS_PRESET",
            "LLAMA_ARG_MODELS_DIR",
            "LLAMA_ARG_MMPROJ",
            "LLAMA_ARG_MMPROJ_URL",
            "LLAMA_ARG_MMPROJ_AUTO",
            "LLAMA_ARG_SPEC_TYPE",
            "LLAMA_ARG_SPEC_DRAFT_HF_REPO",
            "LLAMA_ARG_SPEC_DRAFT_MODEL",
            "LLAMA_ARG_OFFLINE",
        ]
    );
}

/// A cancel must stop the child by EOF and NEVER by the exit line: upstream
/// deletes the `.downloadInProgress` partial once `is_cancelled()` is true,
/// which the exit line sets, while EOF makes the child `exit(1)` with the
/// partial intact for a resume.
#[test]
fn cancel_closes_stdin_and_writes_nothing() {
    let mut written: Vec<u8> = Vec::new();
    {
        let mut stdin: Option<&mut Vec<u8>> = Some(&mut written);
        cancel_stop(&mut stdin);
        assert!(
            stdin.is_none(),
            "the pipe is closed (dropped), so the child reads EOF"
        );
    }
    assert!(
        written.is_empty(),
        "no exit line: it would delete the partial"
    );
}

/// The pipe readers decode lossily: a Windows ANSI-code-page byte in the
/// log must not stop the reader (the child would then block on a full
/// pipe).
#[test]
fn pipe_lines_are_read_through_invalid_utf8() {
    let raw: &[u8] = b"one\r\nbad \xe8 byte\nthree";
    let mut got = Vec::new();
    read_lines_lossy(raw, |l| {
        got.push(l);
        true
    });
    assert_eq!(got, ["one", "bad \u{fffd} byte", "three"]);
    let mut first = Vec::new();
    read_lines_lossy(raw, |l| {
        first.push(l);
        false
    });
    assert_eq!(first, ["one"], "a sink returning false stops the reader");
}

// ── protocol ──

#[test]
fn child_lines_parse_and_a_protocol_change_is_an_error() {
    let line = |j: &str| format!("{CHILD_STATE_PREFIX}{j}");
    assert_eq!(
        parse_child_line(&line(
            r#"{"state":"downloading","payload":{"url":"u/x.gguf","downloaded":5,"total":9}}"#
        )),
        Ok(Some(ChildEvent::Progress {
            url: "u/x.gguf".into(),
            downloaded: 5,
            total: 9
        }))
    );
    assert_eq!(
        parse_child_line(&line(
            r#"{"state":"downloading","payload":{"result":"download_finished"}}"#
        )),
        Ok(Some(ChildEvent::Finished))
    );
    assert_eq!(
        parse_child_line(&line(
            "{\"state\":\"downloading\",\"payload\":{\"result\":\"download_failed\"}}\r"
        )),
        Ok(Some(ChildEvent::Failed))
    );
    assert_eq!(parse_child_line("some stray log line"), Ok(None));
    assert!(parse_child_line(&line("{not json")).is_err());
    assert!(parse_child_line(&line(r#"{"state":"loading","payload":{}}"#)).is_err());
    assert!(parse_child_line(&line(
        r#"{"state":"downloading","payload":{"result":"done"}}"#
    ))
    .is_err());
    assert!(parse_child_line(&line(r#"{"state":"downloading","payload":{}}"#)).is_err());
}

#[test]
fn tracker_follows_parallel_files_by_url_suffix() {
    let j = job(Path::new("M"), 2, true, None);
    let p = j.placements().unwrap();
    let mut t = Tracker::new(&p, &[false, false, true]);
    assert_eq!(
        t.progress(),
        Progress {
            done: 600,
            total: 16600,
            file: String::new(),
            file_done: 0,
            file_total: 0,
            files_done: 1,
            files_total: 3
        }
    );
    let base = format!("https://hf.test/o/r/resolve/{COMMIT}/");
    assert!(t.update(&format!("{base}Q-8B-BF16-00001-of-00002.gguf"), 4000, 9000));
    let pr = t.progress();
    assert_eq!(
        (pr.file_done, pr.file_total),
        (4000, 9000),
        "the per-file bytes follow the current file"
    );
    assert!(t.update(&format!("{base}Q-8B-BF16-00002-of-00002.gguf"), 7000, 7000));
    assert!(!t.update(&format!("{base}unrelated.gguf"), 1, 1));
    let pr = t.progress();
    assert_eq!(
        (pr.done, pr.files_done, pr.file.as_str()),
        (11600, 2, "Q-8B-BF16-00002-of-00002.gguf")
    );
    assert_eq!((pr.file_done, pr.file_total), (7000, 7000));
    t.complete("Q-8B-BF16-00001-of-00002.gguf");
    assert_eq!(t.progress().done, t.progress().total);
}

#[test]
fn a_partial_download_is_a_partial_of_the_candidates_own_files() {
    let dir = tempfile::tempdir().unwrap();
    let l = listing();
    let q4 = &l.candidates[1];
    let q8 = &l.candidates[0];
    assert!(!has_partial(dir.path(), "o/r", q4));
    // The child writes refs\main before its first byte: no partial yet.
    let repo_dir = dir.path().join(STAGING_DIR).join("models--o--r");
    std::fs::create_dir_all(repo_dir.join("refs")).unwrap();
    std::fs::write(repo_dir.join("refs").join("main"), COMMIT).unwrap();
    assert!(
        !has_partial(dir.path(), "o/r", q4),
        "the folder alone is nothing to resume"
    );
    // Q8_0's partial is not Q4_K_M's.
    std::fs::create_dir_all(repo_dir.join("blobs")).unwrap();
    std::fs::write(repo_dir.join("blobs").join("o1.downloadInProgress"), b"x").unwrap();
    assert!(has_partial(dir.path(), "o/r", q8));
    assert!(
        !has_partial(dir.path(), "o/r", q4),
        "another quant's partial"
    );
    assert!(!has_partial(dir.path(), "o/other", q8), "another repo's");
    // Its own mmproj counts (a resume of that group).
    std::fs::write(repo_dir.join("blobs").join("o5.downloadInProgress"), b"x").unwrap();
    assert!(has_partial(dir.path(), "o/r", q4));
    // A file the listing gave no oid for: beside its snapshot entry.
    let mut no_oid = q4.clone();
    no_oid.mmproj = None;
    no_oid.shards[0].oid.clear();
    assert!(!has_partial(dir.path(), "o/r", &no_oid));
    let snap = repo_dir.join("snapshots").join(COMMIT);
    std::fs::create_dir_all(&snap).unwrap();
    std::fs::write(snap.join("Q-8B-Q4_K_M.gguf.downloadInProgress"), b"x").unwrap();
    assert!(has_partial(dir.path(), "o/r", &no_oid));
}

#[test]
fn a_wrong_sized_move_is_removed_so_a_retry_is_not_a_conflict() {
    let dir = tempfile::tempdir().unwrap();
    let j = job(dir.path(), 1, false, None);
    let p = j.placements().unwrap();
    let snap = j
        .staging()
        .join(repo_folder("o/r"))
        .join("snapshots")
        .join(COMMIT);
    std::fs::create_dir_all(&snap).unwrap();
    std::fs::write(snap.join("Q-8B-Q4_K_M.gguf"), vec![0u8; 10]).unwrap();
    let mut t = Tracker::new(&p, &[false]);
    let err = place_group(&j, &[&p[0]], &[], &mut t).unwrap_err();
    assert!(
        matches!(err, HfError::Io(ref m) if m.contains("removed")),
        "{err}"
    );
    assert!(!p[0].target.exists());
    assert_eq!(
        target_present(&p[0].target, 4000),
        Ok(false),
        "a retry starts clean"
    );
}

#[test]
fn a_failed_copy_leaves_nothing_at_the_target() {
    let dir = tempfile::tempdir().unwrap();
    let dst = dir.path().join("out").join("m.gguf");
    std::fs::create_dir_all(dst.parent().unwrap()).unwrap();
    // A source that cannot be copied (absent): no target, no temporary.
    assert!(copy_into_place(&dir.path().join("missing"), &dst).is_err());
    assert!(!dst.exists());
    assert!(!dir.path().join("out").join("m.gguf.partial").exists());
    let src = dir.path().join("src.gguf");
    std::fs::write(&src, b"data").unwrap();
    copy_into_place(&src, &dst).unwrap();
    assert_eq!(std::fs::read(&dst).unwrap(), b"data");
    assert!(!dir.path().join("out").join("m.gguf.partial").exists());
}

#[test]
fn failures_are_classified_from_the_stderr_tail() {
    let l = |s: &[&str]| s.iter().map(|x| x.to_string()).collect::<Vec<_>>();
    let r = "o/r";
    assert_eq!(
        classify_failure(
            &l(&["get_repo_commit: error: GET failed (401): Invalid credentials"]),
            true,
            r
        ),
        HfError::TokenRejected { repo: r.into() }
    );
    assert_eq!(
        classify_failure(&l(&["GET failed (401): Repository not found"]), false, r),
        HfError::NotFoundOrPrivate { repo: r.into() }
    );
    assert_eq!(
        classify_failure(&l(&["Download 'x' failed with status code: 403"]), false, r),
        HfError::Gated {
            repo: r.into(),
            had_token: false
        }
    );
    assert!(matches!(
        classify_failure(
            &l(&["common_pull_file: error writing to file: x.downloadInProgress"]),
            false,
            r
        ),
        HfError::DiskFull(_)
    ));
    assert!(matches!(
        classify_failure(&l(&["common_pull_file: download failed: Could not establish connection (status: -1)"]), false, r),
        HfError::Network(_)
    ));
    // A path past MAX_PATH ends in the same "status code: -1" as a network
    // failure: the open failure must win.
    assert!(matches!(
        classify_failure(
            &l(&["common_pull_file: error opening local file for writing: x", "failed with status code: -1"]),
            false,
            r
        ),
        HfError::Io(m) if m.contains("260 characters")
    ));
    assert!(
        matches!(classify_failure(&l(&["something odd"]), false, r), HfError::Download(m) if m == "something odd")
    );
    assert!(
        matches!(classify_failure(&[], false, r), HfError::Download(m) if m.contains("no output"))
    );
}

// ── staging → subfolders (real files in a temp dir; no child) ──

#[test]
fn staged_files_move_out_and_existing_ones_are_reused_or_refused() {
    let dir = tempfile::tempdir().unwrap();
    let j = job(dir.path(), 1, true, None);
    let placements = j.placements().unwrap();
    // Fake what the child leaves: refs/main, a snapshot entry per file.
    let repo_dir = j.staging().join(repo_folder("o/r"));
    std::fs::create_dir_all(repo_dir.join("refs")).unwrap();
    std::fs::write(repo_dir.join("refs").join("main"), format!("{COMMIT}\n")).unwrap();
    let snap = repo_dir.join("snapshots").join(COMMIT);
    std::fs::create_dir_all(&snap).unwrap();
    std::fs::write(snap.join("Q-8B-Q4_K_M.gguf"), vec![0u8; 4000]).unwrap();
    // The mmproj is already in place (a repeat download): reused.
    let mm = &placements[1].target;
    std::fs::create_dir_all(mm.parent().unwrap()).unwrap();
    std::fs::write(mm, vec![1u8; 600]).unwrap();

    let mut t = Tracker::new(&placements, &[false, true]);
    place_group(&j, &[&placements[0]], &[], &mut t).unwrap();
    place_group(&j, &[&placements[1]], &[], &mut t).unwrap();
    assert_eq!(
        std::fs::metadata(&placements[0].target).unwrap().len(),
        4000
    );
    assert!(!snap.join("Q-8B-Q4_K_M.gguf").exists(), "moved, not copied");
    assert_eq!(
        std::fs::read(mm).unwrap(),
        vec![1u8; 600],
        "the existing file is kept"
    );
    assert_eq!(t.progress().done, t.progress().total);

    // A different file in the way is refused, naming it.
    std::fs::write(mm, vec![1u8; 5]).unwrap();
    assert!(matches!(
        target_present(mm, 600),
        Err(HfError::TargetConflict {
            found: 5,
            expected: 600,
            ..
        })
    ));
}

#[test]
fn a_success_with_nothing_staged_reads_as_the_swallowed_error() {
    let dir = tempfile::tempdir().unwrap();
    let j = job(dir.path(), 1, false, None);
    let p = j.placements().unwrap();
    let mut t = Tracker::new(&p, &[false]);
    let log = vec!["get_repo_commit: error: GET failed (401): nope".to_string()];
    assert_eq!(
        place_group(&j, &[&p[0]], &log, &mut t),
        Err(HfError::TokenRejected { repo: "o/r".into() })
    );
    let other = place_group(&j, &[&p[0]], &["fine".into()], &mut t).unwrap_err();
    assert!(other
        .to_string()
        .contains("reported success but Q-8B-Q4_K_M.gguf is not in"));
}

#[cfg(unix)]
#[test]
fn a_snapshot_symlink_moves_its_blob_not_the_link() {
    let dir = tempfile::tempdir().unwrap();
    let blobs = dir.path().join("blobs");
    let snap = dir.path().join("snapshots").join(COMMIT);
    std::fs::create_dir_all(&blobs).unwrap();
    std::fs::create_dir_all(&snap).unwrap();
    std::fs::write(blobs.join("oid"), b"data").unwrap();
    std::os::unix::fs::symlink(Path::new("../../blobs/oid"), snap.join("m.gguf")).unwrap();
    let dst = dir.path().join("out").join("m.gguf");
    move_real_data(&snap.join("m.gguf"), &dst, dir.path()).unwrap();
    assert!(!std::fs::symlink_metadata(&dst)
        .unwrap()
        .file_type()
        .is_symlink());
    assert_eq!(std::fs::read(&dst).unwrap(), b"data");
    assert!(!blobs.join("oid").exists());
}

#[cfg(unix)]
#[test]
fn a_snapshot_symlink_out_of_the_staging_tree_is_refused() {
    let dir = tempfile::tempdir().unwrap();
    let outside = tempfile::tempdir().unwrap();
    std::fs::write(outside.path().join("precious"), b"mine").unwrap();
    let snap = dir.path().join("snapshots").join(COMMIT);
    std::fs::create_dir_all(&snap).unwrap();
    std::os::unix::fs::symlink(outside.path().join("precious"), snap.join("m.gguf")).unwrap();
    let dst = dir.path().join("out").join("m.gguf");
    assert!(move_real_data(&snap.join("m.gguf"), &dst, dir.path()).is_err());
    assert_eq!(
        std::fs::read(outside.path().join("precious")).unwrap(),
        b"mine"
    );
    assert!(!dst.exists());
}

#[test]
fn a_staged_entry_outside_the_staging_tree_is_refused() {
    let dir = tempfile::tempdir().unwrap();
    let staging = dir.path().join("staging");
    std::fs::create_dir_all(&staging).unwrap();
    std::fs::write(dir.path().join("precious"), b"mine").unwrap();
    let src = staging.join("..").join("precious");
    let dst = dir.path().join("out.gguf");
    assert!(move_real_data(&src, &dst, &staging).is_err());
    assert_eq!(std::fs::read(dir.path().join("precious")).unwrap(), b"mine");
}

#[test]
fn find_staged_uses_the_ref_then_any_snapshot() {
    let dir = tempfile::tempdir().unwrap();
    let other = "a".repeat(40);
    let snap = dir.path().join("snapshots").join(&other).join("sub");
    std::fs::create_dir_all(&snap).unwrap();
    std::fs::write(snap.join("x.gguf"), b"1").unwrap();
    assert_eq!(
        find_staged(dir.path(), COMMIT, "sub/x.gguf"),
        Some(
            dir.path()
                .join("snapshots")
                .join(&other)
                .join("sub")
                .join("x.gguf")
        )
    );
    assert_eq!(find_staged(dir.path(), COMMIT, "y.gguf"), None);
    std::fs::create_dir_all(dir.path().join("refs")).unwrap();
    std::fs::write(dir.path().join("refs").join("main"), &other).unwrap();
    assert_eq!(cached_commit(dir.path()), Some(other));
}

// ── derivation ──

fn info(template: Option<&str>, nextn: u32, moe: bool) -> gguf::ModelInfo {
    gguf::ModelInfo {
        arch: "qwen3".into(),
        size_label: String::new(),
        quant: "Q4_K_M".into(),
        is_moe: moe,
        expert_count: if moe { 128 } else { 0 },
        expert_used: if moe { 8 } else { 0 },
        expert_shared_count: 0,
        leading_dense_block_count: 0,
        moe_every_n_layers: 0,
        n_layer: 36,
        n_ctx_train: 262144,
        head_count: 32,
        head_count_kv: 8,
        n_embd: 4096,
        nextn_predict_layers: nextn,
        block_size: 0,
        chat_template: template.map(str::to_string),
        embd: None,
        sampling: gguf::Sampling {
            temp: Some(0.6),
            top_k: Some(20),
            top_p: Some(0.95),
            min_p: Some(0.0),
            penalty_repeat: None,
        },
    }
}

fn downloaded(mmproj: bool, draft: Option<Sidecar>) -> Downloaded {
    Downloaded {
        repo: "o/r".into(),
        project_url: "https://huggingface.co/o/r".into(),
        model: PathBuf::from("M/models/r/m-Q4_K_M.gguf"),
        mmproj: mmproj.then(|| PathBuf::from("M/mmprojs/r/m-mmproj-F16.gguf")),
        draft: draft.map(|s| (s, PathBuf::from("M/dflashs/r/dflash-Q4_K_M.gguf"))),
    }
}

#[test]
fn derived_model_profile_fills_paths_url_sampling_and_fit() {
    let d = downloaded(true, None);
    let p = derive_model_profile(
        "m-Q4_K_M".into(),
        &d,
        Some(&info(Some("{% if x %}{{ y }}{% endif %}"), 0, false)),
        None,
        None,
    );
    assert_eq!(p.model, d.model.to_string_lossy());
    assert_eq!(p.mmproj, d.mmproj.as_ref().unwrap().to_string_lossy());
    assert_eq!(p.project_url, "https://huggingface.co/o/r");
    assert_eq!(
        (p.temp, p.top_k, p.top_p, p.min_p, p.repeat_penalty),
        (Some(0.6), Some(20), Some(0.95), Some(0.0), None)
    );
    assert_eq!(p.jinja, Some(true));
    assert!(p.fit);
    assert!(crate::model_profiles::fit_allowed(
        p.ctx_size.is_none(),
        p.n_gpu_layers.is_none()
    ));
    assert_eq!(p.reasoning, "auto");
    assert_eq!(
        (p.model_draft.as_str(), p.spec_type.as_str()),
        ("", ""),
        "embedded-MTP-free and no drafter"
    );
    // Everything the derivation does not own is the New dialog's model profile.
    let base = ModelProfile {
        model: p.model.clone(),
        project_url: p.project_url.clone(),
        mmproj: p.mmproj.clone(),
        temp: p.temp,
        top_k: p.top_k,
        top_p: p.top_p,
        min_p: p.min_p,
        fit: true,
        ..ModelProfile::new_default("m-Q4_K_M".into(), String::new())
    };
    assert_eq!(p, base);
}

#[test]
fn derived_drafter_takes_the_header_verdict_over_the_prefix() {
    let d = downloaded(false, Some(Sidecar::Dflash));
    let m = info(None, 0, true);
    let header = derive_model_profile("x".into(), &d, Some(&m), Some("draft-dspark"), None);
    assert_eq!(header.spec_type, "draft-dspark");
    assert_eq!(header.model_draft, "M/dflashs/r/dflash-Q4_K_M.gguf");
    let silent = derive_model_profile("x".into(), &d, Some(&m), None, None);
    assert_eq!(silent.spec_type, "draft-dflash");
    // The prunes leave a drafter-backed model profile alone (they are idempotent here).
    let mut again = silent.clone();
    assert!(model_profiles::prune_inactive_draft_keys(&mut again, false).is_empty());
    assert!(!model_profiles::prune_inactive_moe_cache(&mut again, true));
    assert_eq!(again, silent);
}

#[test]
fn derived_jinja_and_the_unreadable_header() {
    let d = downloaded(false, None);
    let legacy = derive_model_profile(
        "x".into(),
        &d,
        Some(&info(Some("chatml"), 0, false)),
        None,
        None,
    );
    assert_eq!(legacy.jinja, Some(false));
    let none = derive_model_profile("x".into(), &d, Some(&info(None, 2, false)), None, None);
    assert_eq!(none.jinja, Some(true));
    assert_eq!(none.spec_type, "", "embedded MTP heads are not switched on");
    // No header: paths, URL and fit only.
    let blind = derive_model_profile("x".into(), &d, None, None, None);
    assert_eq!(
        (blind.temp, blind.top_k, blind.jinja),
        (None, None, Some(true))
    );
    assert!(blind.fit);
}

/// A small MoE on disk (4 blocks of 2 MiB attention and 16 MiB experts,
/// 4 MiB of KV at its 4096-token context, a 4 MiB output head) and a
/// pair of cards of 48 and 24 MiB usable: 80 MiB does not fit 72. With
/// `extras`, a 1 MiB mmproj and a 2 MiB drafter on disk too, and the
/// second card larger by exactly what they take (their compute
/// allowances included): neither fits the first card, so both land on the
/// second, pinned, and the model's cut stays the same.
fn placed_download(dir: &Path, extras: bool) -> (Downloaded, hf_fit::Budget) {
    use crate::hf_fit::tests::{gguf, write_sparse, W};
    let mut tensors = vec![("token_embd.weight".to_string(), 8 * MIB)];
    for b in 0..4 {
        tensors.push((format!("blk.{b}.attn_q.weight"), 2 * MIB));
        tensors.push((format!("blk.{b}.ffn_up_exps.weight"), 16 * MIB));
    }
    tensors.push(("output.weight".into(), 4 * MIB));
    let t: Vec<(&str, u64)> = tensors.iter().map(|(n, s)| (n.as_str(), *s)).collect();
    let (h, size) = gguf(
        &[
            ("general.architecture", W::Str("qwen3moe")),
            ("qwen3moe.block_count", W::U32(4)),
            ("qwen3moe.context_length", W::U32(4096)),
            ("qwen3moe.embedding_length", W::U32(256)),
            ("qwen3moe.attention.head_count", W::U32(4)),
            ("qwen3moe.attention.head_count_kv", W::U32(1)),
            ("qwen3moe.attention.key_length", W::U32(64)),
            ("qwen3moe.attention.value_length", W::U32(64)),
        ],
        &t,
    );
    let model = dir.join("m-Q4_K_M.gguf");
    write_sparse(&model, &h, size);
    let mut d = downloaded(false, None);
    d.model = model;
    let mut room = 0;
    if extras {
        let mmproj = dir.join("mmproj-F16.gguf");
        std::fs::write(&mmproj, vec![0u8; MIB as usize]).unwrap();
        d.mmproj = Some(mmproj);
        let (h, size) = gguf(
            &[("general.architecture", W::Str("qwen3"))],
            &[("blk.0.attn_q.weight", 2 * MIB)],
        );
        let draft = dir.join("m-MTP-Q8_0.gguf");
        write_sparse(&draft, &h, size);
        d.draft = Some((Sidecar::Mtp, draft));
        room = (MIB + hf_fit::MMPROJ_COMPUTE + 2 * MIB + hf_fit::DRAFT_COMPUTE) / MIB;
    }
    let card = |id: &str, name: &str, usable_mib: u64| hf_fit::Gpu {
        id: id.into(),
        name: name.into(),
        total_mib: usable_mib + hf_fit::GPU_RESERVE / MIB,
    };
    let b = hf_fit::Budget {
        gpus: vec![
            card("ROCm1", "AMD Radeon AI PRO R9700", 48),
            card("CUDA0", "NVIDIA GeForce RTX 4070 SUPER", 24 + room),
        ],
        ram: Some(60 << 30),
        ram_total: Some(64 << 30),
    };
    (d, b)
}

const MIB: u64 = 1 << 20;

#[test]
fn a_download_is_placed_per_card_and_survives_the_ini_and_the_form() {
    let dir = tempfile::tempdir().unwrap();
    let (d, b) = placed_download(dir.path(), true);
    let p = model_profile_from_download(&d, &[], Some(&b));
    // Block 0's experts in RAM; blocks 0-2 on the R9700 (41 of 48 MiB),
    // block 3 and the output head on the 4070 (23 of 24 MiB).
    assert_eq!(
        (
            p.device.as_str(),
            p.tensor_split.as_str(),
            p.n_cpu_moe,
            p.n_gpu_layers,
            p.ctx_size
        ),
        ("ROCm1,CUDA0", "3,2", Some(1), None, Some(4096))
    );
    assert!(
        !p.fit,
        "the context is set, so the fit is off (model_profiles::fit_allowed)"
    );
    assert_eq!(
        p.device_draft, "CUDA0",
        "a separate drafter on the last card of the split"
    );
    // The encoder does not fit the first device (its default): moved to
    // the second, where the room was left for it.
    assert_eq!(
        (p.mmproj_device.as_str(), p.mmproj_offload),
        ("CUDA0", Some(true))
    );
    assert!(
        p.placement_note
            .starts_with("estimated at download for a 4k context"),
        "{}",
        p.placement_note
    );
    assert!(
        p.placement_note.contains("; counting the image encoder (0.5 GiB) on CUDA0 and the MTP drafter (0.8 GiB) on CUDA0;"),
        "{}",
        p.placement_note
    );
    model_profiles::validate_for_save(&p).unwrap();

    // Through the INI: the note is a comment, never a key, and reads back.
    let path = dir.path().join("models.ini");
    std::fs::write(&path, model_profiles::render_section(&p)).unwrap();
    let text = std::fs::read_to_string(&path).unwrap();
    assert!(
        text.contains(&format!("\r\n; placement = {}\r\n", p.placement_note)),
        "{text}"
    );
    assert!(
        text.contains("\r\nmmproj-device = CUDA0\r\n")
            && text.contains("\r\ndevice-draft = CUDA0\r\n"),
        "{text}"
    );
    let back = model_profiles::read_model_profiles(&path);
    assert_eq!(back, std::slice::from_ref(&p));
    // The form half of this round trip is pai-config's (the form is a
    // Slint struct): `form::tests::rich_model_profile_round_trips`.
}

#[test]
fn without_a_gpu_estimate_the_download_keeps_the_defaults() {
    let dir = tempfile::tempdir().unwrap();
    let (d, b) = placed_download(dir.path(), false);
    let p = model_profile_from_download(&d, &[], None);
    assert_eq!(
        (
            p.device.as_str(),
            p.tensor_split.as_str(),
            p.n_cpu_moe,
            p.ctx_size
        ),
        ("", "", None, None)
    );
    assert!(p.fit && p.placement_note.is_empty());
    // One card that holds it all: nothing to write either.
    let big = hf_fit::Budget {
        gpus: vec![hf_fit::Gpu {
            id: "CUDA0".into(),
            name: "G".into(),
            total_mib: 4096,
        }],
        ..b
    };
    let one = model_profile_from_download(&d, &[], Some(&big));
    assert_eq!(one, p);
    // An unreadable file: no placement, no failure.
    let mut gone = d.clone();
    gone.model = dir.path().join("missing.gguf");
    assert!(model_profile_from_download(&gone, &[], Some(&big))
        .placement_note
        .is_empty());
}

#[test]
fn sidecar_names_round_trip_and_sizes_format() {
    for s in Sidecar::ALL {
        assert_eq!(Sidecar::parse(&s.label().to_ascii_lowercase()), Some(s));
    }
    assert_eq!(Sidecar::parse("eagle3"), None);
    assert_eq!(format_size(4000 * 1024 * 1024), "3.91 GiB");
    assert_eq!(format_size(600 * 1024 * 1024), "600.0 MiB");
}
