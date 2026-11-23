//! Slice M1 of "Choosing the model file on the web" (ARCHITECTURE.md): the
//! Hugging Face listing (`parse_spec`, `valid_repo_id`, `safe_repo_path`, the
//! `Fetch` seam, `Curl`, `list_repo`/`build_listing`) moved into
//! `pai_core::hf`, plus the curl profile the bff will use, the bounds of
//! section (2) "The outbound boundary (SSRF)": a fixed endpoint
//! `https://huggingface.co/` that ignores `MODEL_ENDPOINT` and `HF_ENDPOINT`,
//! `--proto =https`, no `-L` (no `location` line), `--connect-timeout 10`,
//! `--max-time 20`, `--max-filesize 16M`.
//!
//! No network: the profile is pinned as the text curl reads on stdin (the
//! config the `-K -` design builds), and the listing runs over an injected
//! `Fetch` with canned answers.

use pai_core::hf::{self, Fetch, HfError, RepoSpec, Token};

/// The curl config as curl's parser reads it: `key = "value"` lines. Returns
/// every `(key, value)` with the key trimmed and the value trimmed and
/// stripped of its surrounding quotes, so the pins tolerate the quoting style.
fn config_pairs(cfg: &str) -> Vec<(String, String)> {
    cfg.lines()
        .filter_map(|line| line.split_once('='))
        .map(|(k, v)| (k.trim().to_string(), v.trim().trim_matches('"').to_string()))
        .collect()
}

/// The value of the first line with this key, if any.
fn value_of(cfg: &str, key: &str) -> Option<String> {
    config_pairs(cfg)
        .into_iter()
        .find(|(k, _)| k == key)
        .map(|(_, v)| v)
}

/// The bff endpoint is fixed (section (2)): a mirror set in `HF_ENDPOINT` or
/// `MODEL_ENDPOINT` (the variables `endpoint()` honours for the configurator
/// and the download child) must not move it. Restores the variables: no other
/// test in this file reads them.
#[test]
fn m1_bff_endpoint_ignores_hf_endpoint() {
    let saved_hf = std::env::var_os("HF_ENDPOINT");
    let saved_model = std::env::var_os("MODEL_ENDPOINT");
    std::env::set_var("HF_ENDPOINT", "https://mirror.test/");
    std::env::set_var("MODEL_ENDPOINT", "https://mirror.test/");
    let endpoint = hf::bff_endpoint();
    match saved_hf {
        Some(v) => std::env::set_var("HF_ENDPOINT", v),
        None => std::env::remove_var("HF_ENDPOINT"),
    }
    match saved_model {
        Some(v) => std::env::set_var("MODEL_ENDPOINT", v),
        None => std::env::remove_var("MODEL_ENDPOINT"),
    }
    assert_eq!(endpoint, "https://huggingface.co/");
}

/// The bff profile pins the protocol: `--proto =https`, so no plain HTTP
/// request leaves the node (section (2)).
#[test]
fn m1_bff_config_pins_the_protocol() {
    let cfg = hf::bff_curl_config("https://huggingface.co/api/models/o/r/refs", None).unwrap();
    assert_eq!(
        value_of(&cfg, "proto").as_deref(),
        Some("=https"),
        "config:\n{cfg}"
    );
}

/// The bff profile follows no redirects: no `location` line, so a 3xx answers
/// `hf-repo-moved` and no request reaches a host the answer names (section
/// (2): curl cannot restrict a redirect's host, only its protocol).
#[test]
fn m1_bff_config_follows_no_redirects() {
    let cfg = hf::bff_curl_config("https://huggingface.co/api/models/o/r/refs", None).unwrap();
    let keys: Vec<String> = config_pairs(&cfg).into_iter().map(|(k, _)| k).collect();
    assert!(
        !keys.iter().any(|k| k == "location"),
        "a location line in: {keys:?}"
    );
}

/// The bff profile carries the bounds of section (2): a connect timeout of
/// 10 s, a total of 20 s (under the route's 25 s deadline), and a
/// `max-filesize` of 16 MiB so an oversized answer is cut.
#[test]
fn m1_bff_config_carries_the_bounds() {
    let cfg = hf::bff_curl_config("https://huggingface.co/api/models/o/r/refs", None).unwrap();
    assert_eq!(
        value_of(&cfg, "connect-timeout").as_deref(),
        Some("10"),
        "config:\n{cfg}"
    );
    assert_eq!(
        value_of(&cfg, "max-time").as_deref(),
        Some("20"),
        "config:\n{cfg}"
    );
    let max_filesize = value_of(&cfg, "max-filesize");
    assert!(
        matches!(max_filesize.as_deref(), Some("16777216") | Some("16M")),
        "max-filesize: {max_filesize:?} in:\n{cfg}"
    );
}

/// The bff profile sends the url it is given (the fixed endpoint is the
/// caller's, not curl's business) and, when there is a token, carries it as
/// an `Authorization` header on the config's stdin, never in the url, and
/// `Token`'s `Debug` stays opaque.
#[test]
fn m1_bff_config_sends_the_url_and_the_token_as_a_header() {
    let token = Token::new("hf_secrettoken123").unwrap();
    let url = "https://huggingface.co/api/models/o/r/refs";
    let cfg = hf::bff_curl_config(url, Some(&token)).unwrap();
    assert_eq!(
        value_of(&cfg, "url").as_deref(),
        Some(url),
        "config:\n{cfg}"
    );
    let headers: Vec<String> = config_pairs(&cfg)
        .into_iter()
        .filter(|(k, _)| k == "header")
        .map(|(_, v)| v)
        .collect();
    assert!(
        headers
            .iter()
            .any(|h| h == "Authorization: Bearer hf_secrettoken123"),
        "headers: {headers:?}"
    );
    assert!(!value_of(&cfg, "url").unwrap().contains("hf_secrettoken123"));
    assert_eq!(format!("{token:?}"), "Token(***)");
}

/// A `Fetch` answering canned bodies in call order and recording the urls.
struct Fake {
    answers: std::cell::RefCell<std::vec::IntoIter<(u16, String)>>,
    asked: std::cell::RefCell<Vec<String>>,
}

impl Fake {
    fn new(answers: Vec<(u16, String)>) -> Fake {
        Fake {
            answers: std::cell::RefCell::new(answers.into_iter()),
            asked: std::cell::RefCell::new(Vec::new()),
        }
    }
}

impl Fetch for Fake {
    fn get(&self, url: &str, _token: Option<&Token>) -> Result<(u16, String), HfError> {
        self.asked.borrow_mut().push(url.to_string());
        Ok(self
            .answers
            .borrow_mut()
            .next()
            .expect("fake Fetch asked more often than it can answer"))
    }

    fn get_range(
        &self,
        _url: &str,
        _token: Option<&Token>,
        _start: u64,
        _len: u64,
    ) -> Result<(u16, Vec<u8>), HfError> {
        unimplemented!("the listing never reads a range")
    }
}

/// The moved API is reachable from outside the crate and works: `parse_spec`
/// parses and refuses, `valid_repo_id` and `safe_repo_path` answer, and
/// `list_repo_with` over an injected `Fetch` and endpoint lists a repo with
/// canned answers, asking exactly the two endpoint-prefixed API urls.
#[test]
fn m1_moved_api_lists_a_repo_over_a_fake_fetch() {
    // The spec parser and the two guards.
    assert_eq!(
        hf::parse_spec("org/repo").unwrap(),
        RepoSpec {
            repo: "org/repo".into(),
            tag: None,
            file: None,
        }
    );
    assert_eq!(
        hf::parse_spec("org/repo:Q4_K_M").unwrap().tag.as_deref(),
        Some("Q4_K_M")
    );
    assert!(matches!(
        hf::parse_spec("a/b/c"),
        Err(HfError::InvalidSpec(_))
    ));
    assert!(hf::valid_repo_id("org/repo"));
    assert!(!hf::valid_repo_id("a/b/c"));
    assert!(!hf::valid_repo_id("org/"));
    assert!(hf::safe_repo_path("a/b.gguf"));
    assert!(!hf::safe_repo_path("../x.gguf"));
    assert!(!hf::safe_repo_path("a\\b.gguf"));
    assert!(!hf::safe_repo_path("a/b:c.gguf"));

    // The listing over the seam.
    let commit = "0123456789012345678901234567890123456789";
    let fake = Fake::new(vec![
        (
            200,
            format!(r#"{{"branches":[{{"name":"main","targetCommit":"{commit}"}}]}}"#),
        ),
        (
            200,
            r#"[{"type":"file","path":"Model-Q4_K_M.gguf","size":100,"lfs":{"oid":"aa"}}]"#
                .to_string(),
        ),
    ]);
    let endpoint = "https://huggingface.co/";
    let spec = hf::parse_spec("org/repo").unwrap();
    let listing = hf::list_repo_with(&fake, endpoint, &spec, None).unwrap();
    assert_eq!(listing.repo, "org/repo");
    assert_eq!(listing.commit, commit);
    assert_eq!(listing.project_url, "https://huggingface.co/org/repo");
    assert_eq!(listing.candidates.len(), 1);
    assert_eq!(listing.candidates[0].tag, "Q4_K_M");
    assert_eq!(listing.candidates[0].primary().path, "Model-Q4_K_M.gguf");
    assert_eq!(listing.default, 0);
    assert_eq!(
        fake.asked.borrow().as_slice(),
        [
            "https://huggingface.co/api/models/org/repo/refs".to_string(),
            format!("https://huggingface.co/api/models/org/repo/tree/{commit}?recursive=true"),
        ]
    );

    // The plain entry points exist for the bff and the app.
    let _ = hf::list_repo;
    let _ = hf::Curl;
}
