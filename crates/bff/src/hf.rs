//! The web route that lists a Hugging Face repo's GGUF files
//! (`GET /api/hf/files`, ARCHITECTURE.md "Choosing the model file on the
//! web", slice M2).
//!
//! The listing itself is `pai-core`'s: the two GETs (`api/models/<repo>/refs`
//! for the commit, `api/models/<repo>/tree/<commit>?recursive=true` for the
//! files) and the mirror of llama.cpp's file rules that groups them into
//! candidates. The bff asks them over its own runner, the fixed endpoint
//! [`pai_core::hf::bff_endpoint`], and no token (section (3): the first
//! slices send none, so a gated, private or missing repo is HF's 401, 403 or
//! 404, all one refusal).
//!
//! ## The outbound boundary (section (2))
//!
//! The browser sends a repository id, never a URL or a host: the id is held to
//! `parse_spec` (which holds it to `valid_repo_id`) before anything is
//! fetched, and the two URLs are built from the fixed endpoint. The runner is
//! the bff's own, not `pai_core::hf::Curl`: the app's runner passes `-L` and
//! honours `MODEL_ENDPOINT` / `HF_ENDPOINT`, and neither may leave the node.
//! [`curl_argv`] is `-q` first (no `~/.curlrc` of the service's account),
//! `-sS`, the config on stdin (`-K -`), and no `-L` anywhere: a 3xx answers
//! `hf-repo-moved`, so no request reaches a host the answer names. The config
//! is [`pai_core::hf::bff_curl_config`]: `proto =https`, no `location`, a
//! connect timeout of 10 s, 20 s in total, an answer of at most 16 MiB.
//!
//! ## The bounds
//!
//! A listing is pinned to a commit, so the repo's tree is kept for
//! [`CACHE_WINDOW`] ([`CACHE_REPOS`] at most, gone at restart, section (6)).
//! The cache holds no caller's tag: the tag and file of the request are
//! applied to the tree as the answer is built, so one caller's tag never
//! answers another. At most [`MAX_CALLS`] listings run at once: beyond that
//! the answer is 429 `hf-busy`, so a page cannot fill the node's children, and
//! the slot comes back on every path, the one where the route gives up
//! included. The whole read is given [`crate::HF_TIMEOUT`], above curl's own
//! 20 s.
//!
//! ## The answer
//!
//! The design's document of section (5), and the refusals of its key list:
//! 400 `hf-repo-invalid`, 404 `hf-repo-unavailable`, 502 `hf-repo-moved`,
//! 502 `hf-bad-answer`, 504 `hf-unreachable`, 503 `hf-tool-missing`, 429
//! `hf-busy`. No words travel: the page has a text for each key.

use crate::session::{Clock, Sessions};
use axum::body::Body;
use axum::extract::Query;
use axum::http::{HeaderMap, StatusCode};
use axum::response::{IntoResponse, Response};
use pai_core::hf::{Fetch, HfError, RepoFile, RepoSpec, Token};
use std::collections::HashMap;
use std::io::Write;
use std::path::PathBuf;
use std::process::{Command, Stdio};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

/// How long a repo's listing is kept: a listing is pinned to a commit, so a
/// stale one still names files that exist (section (6)).
const CACHE_WINDOW: Duration = Duration::from_secs(10 * 60);

/// The most repos kept at once, the oldest going first (section (6)).
const CACHE_REPOS: usize = 64;

/// The most Hugging Face listings that run at once (section (2)): beyond them
/// the answer is 429 `hf-busy`.
const MAX_CALLS: usize = 2;

/// The argv of the bff's own curl runner: `-q` first (no `~/.curlrc`),
/// `-sS`, the config on stdin (`-K -`), never `-L` (a 3xx answers
/// `hf-repo-moved`).
pub fn curl_argv() -> Vec<String> {
    vec![
        "-q".to_string(),
        "-sS".to_string(),
        "-K".to_string(),
        "-".to_string(),
    ]
}

/// The runner's stdin config for `url`: the bff profile of
/// `pai_core::hf::bff_curl_config` (fixed endpoint, `proto =https`, no
/// `location`, the bounds of section (2)).
pub fn curl_config(url: &str) -> Result<String, HfError> {
    pai_core::hf::bff_curl_config(url, None)
}

/// The runner the service lists through.
pub fn runner() -> Arc<dyn Fetch + Send + Sync> {
    Arc::new(Runner)
}

/// The command [`Runner::run`] spawns: the executable, the argv of
/// [`curl_argv`], and the three pipes. The seam the test reads the argv that
/// reaches the process from (the M2 review's G2): `run` builds no command and
/// adds no argument of its own, so the argv the test reads is the argv the
/// child gets.
pub fn runner_command() -> Command {
    let mut cmd = Command::new(curl_exe());
    cmd.args(curl_argv())
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    pai_core::proc::hide_console(&mut cmd);
    cmd
}

/// The bff's own `curl` runner: the app's `pai_core::hf::Curl` follows
/// redirects and honours the endpoint variables, and the bff may do neither
/// (the module header).
pub struct Runner;

impl Runner {
    /// Run `curl` with the command [`runner_command`] builds and return its
    /// stdout. The child bounds itself: `max-time` is in the config, so no
    /// curl outlives the route that started it.
    fn run(config: Result<String, HfError>, token: Option<&Token>) -> Result<Vec<u8>, HfError> {
        let config = config?;
        let mut child = runner_command().spawn().map_err(|e| {
            HfError::MissingTool(pai_core::strings::fill("curl-not-started", &[&e]))
        })?;
        if let Some(mut stdin) = child.stdin.take() {
            // A write error surfaces as curl's own "no URL" failure below.
            let _ = stdin.write_all(config.as_bytes());
        }
        let out = child
            .wait_with_output()
            .map_err(|e| HfError::Io(format!("curl: {e}")))?;
        if !out.status.success() {
            let code = out.status.code().unwrap_or(-1);
            let err = pai_core::hf::redact(String::from_utf8_lossy(&out.stderr).trim(), token);
            return Err(curl_failure(code, &err));
        }
        Ok(out.stdout)
    }
}

impl Fetch for Runner {
    fn get(&self, url: &str, token: Option<&Token>) -> Result<(u16, String), HfError> {
        let out = Runner::run(pai_core::hf::bff_curl_config(url, token), token)?;
        pai_core::hf::parse_curl_output(&String::from_utf8_lossy(&out))
    }

    /// The bff lists, it does not read a file's bytes: the range read of
    /// `Fetch` (the placement estimate, `pai-app`) has no caller here.
    fn get_range(
        &self,
        _url: &str,
        _token: Option<&Token>,
        _start: u64,
        _len: u64,
    ) -> Result<(u16, Vec<u8>), HfError> {
        Err(HfError::Io("the bff lists, it reads no range".to_string()))
    }
}

/// `System32\curl.exe` on Windows when it is there (the OS's own copy, not
/// whatever a PATH entry offers), else `curl` from PATH.
fn curl_exe() -> PathBuf {
    #[cfg(windows)]
    {
        if let Some(root) = std::env::var_os("SystemRoot") {
            let p = PathBuf::from(root).join("System32").join("curl.exe");
            if p.is_file() {
                return p;
            }
        }
    }
    PathBuf::from("curl")
}

/// curl's failure as the route reads it: the codes that mean the host was
/// never reached stay `Network` (the design's `hf-unreachable`); anything
/// else is `Io`, an answer that was refused, `--max-filesize` above all (the
/// design's `hf-bad-answer`).
fn curl_failure(code: i32, stderr: &str) -> HfError {
    match code {
        5 | 6 | 7 | 28 | 35 | 51 | 58 | 59 | 60 | 77 => pai_core::hf::curl_failure(code, stderr),
        _ => HfError::Io(format!("curl {code}: {stderr}")),
    }
}

/// The route's refusal: the design's key, the args its text fills, and the
/// status the key answers with (section (5)). The model profile save answers
/// the same keys ([`Refused::save_response`]), so the listing of slice M2 and
/// the save of slice M3b speak one key list.
pub(crate) struct Refused {
    status: StatusCode,
    key: &'static str,
    args: Vec<String>,
}

impl Refused {
    fn with(status: StatusCode, key: &'static str, args: Vec<String>) -> Self {
        Self { status, key, args }
    }

    /// The id the query sent is one `parse_spec` refuses, so nothing was
    /// fetched.
    fn invalid(repo: &str) -> Self {
        Self::with(
            StatusCode::BAD_REQUEST,
            "hf-repo-invalid",
            vec![repo.to_string()],
        )
    }

    /// HF's 401, 403 and 404 are one refusal: with no token a gated, private
    /// and missing repo answer the same (section (3)).
    fn unavailable(repo: &str) -> Self {
        Self::with(
            StatusCode::NOT_FOUND,
            "hf-repo-unavailable",
            vec![repo.to_string()],
        )
    }

    /// A 3xx: the runner follows no redirect, so the listing stops here and
    /// no request reaches a host the answer names.
    fn moved(repo: &str) -> Self {
        Self::with(
            StatusCode::BAD_GATEWAY,
            "hf-repo-moved",
            vec![repo.to_string()],
        )
    }

    /// An answer that names no model to pick: not JSON, too large, no branch,
    /// a repo with no GGUF.
    fn bad_answer() -> Self {
        Self::with(StatusCode::BAD_GATEWAY, "hf-bad-answer", Vec::new())
    }

    /// Hugging Face was never reached: no host, no connect, no answer in time.
    fn unreachable() -> Self {
        Self::with(StatusCode::GATEWAY_TIMEOUT, "hf-unreachable", Vec::new())
    }

    /// This machine has no `curl`.
    fn tool_missing() -> Self {
        Self::with(
            StatusCode::SERVICE_UNAVAILABLE,
            "hf-tool-missing",
            Vec::new(),
        )
    }

    /// [`MAX_CALLS`] listings are already running.
    fn busy() -> Self {
        Self::with(StatusCode::TOO_MANY_REQUESTS, "hf-busy", Vec::new())
    }

    /// HF's status of one of the two GETs.
    fn from_status(status: u16, repo: &str) -> Self {
        match status {
            300..=399 => Self::moved(repo),
            401 | 403 | 404 => Self::unavailable(repo),
            _ => Self::bad_answer(),
        }
    }

    /// The listing's failure: the runner's transport failures, and the
    /// listing's own.
    fn from_error(error: &HfError, repo: &str) -> Self {
        match error {
            HfError::MissingTool(_) => Self::tool_missing(),
            HfError::Network(_) => Self::unreachable(),
            HfError::NotFoundOrPrivate { .. }
            | HfError::Gated { .. }
            | HfError::TokenRejected { .. } => Self::unavailable(repo),
            HfError::Http { status, .. } if (300u16..=399).contains(status) => Self::moved(repo),
            _ => Self::bad_answer(),
        }
    }

    fn response(&self) -> Response {
        (
            self.status,
            axum::Json(serde_json::json!({ "key": self.key, "args": self.args })),
        )
            .into_response()
    }

    /// The same key and args, at the status a model profile save gives (slice
    /// M3b): the save has handed nothing over to write, so a listing that never
    /// reached Hugging Face, and a node with no `curl` to reach it with, answer
    /// the 503 every read that cannot answer gives, not the 504 of a write whose
    /// outcome is unknown. HF's own refusals keep the status of `response`.
    pub(crate) fn save_response(&self) -> Response {
        let status = match self.key {
            "hf-unreachable" | "hf-tool-missing" => StatusCode::SERVICE_UNAVAILABLE,
            _ => self.status,
        };
        (
            status,
            axum::Json(serde_json::json!({ "key": self.key, "args": self.args })),
        )
            .into_response()
    }
}

/// One repo's tree as the cache keeps it: the commit the tree was listed at,
/// and the files it names, with no tag and no file applied. The cache is the
/// repo's (section (6)); the caller's tag and file are applied to the tree as
/// the answer is built, so no caller's tag answers another caller (the M2
/// review's D1).
#[derive(Clone)]
struct Tree {
    commit: String,
    files: Vec<RepoFile>,
}

/// One repo's tree, with the moment it was read.
struct Cached {
    at: Instant,
    tree: Tree,
}

/// The route's state, shared by every request of one router: the fetch, the
/// clock the cache ages by, the cache, and the slots a listing takes. The model
/// profile save shares it with the route, so a repo the picker listed is checked
/// against that listing (slice M3b, [`file_is_listed`]).
pub(crate) struct Hf {
    fetch: Arc<dyn Fetch + Send + Sync>,
    clock: Clock,
    cache: Mutex<HashMap<String, Cached>>,
    slots: Arc<tokio::sync::Semaphore>,
}

impl Hf {
    fn new(fetch: Arc<dyn Fetch + Send + Sync>, clock: Clock) -> Self {
        Self {
            fetch,
            clock,
            cache: Mutex::new(HashMap::new()),
            slots: Arc::new(tokio::sync::Semaphore::new(MAX_CALLS)),
        }
    }

    /// The repo's tree: the one this cache keeps when it is young, else the
    /// listing, fetched in a blocking task (the runner is a child process) under
    /// [`crate::HF_TIMEOUT`] and kept for the next caller. 429 `hf-busy` when
    /// [`MAX_CALLS`] listings are already running.
    async fn tree(&self, repo: &str) -> Result<Tree, Refused> {
        if let Some(tree) = self.cached(repo) {
            return Ok(tree);
        }
        let Ok(permit) = Arc::clone(&self.slots).try_acquire_owned() else {
            return Err(Refused::busy());
        };
        let fetch = Arc::clone(&self.fetch);
        let asked = repo.to_string();
        let ask = tokio::task::spawn_blocking(move || listing(fetch.as_ref(), &asked));
        let tree = match tokio::time::timeout(crate::HF_TIMEOUT, ask).await {
            Ok(Ok(Ok(tree))) => tree,
            Ok(Ok(Err(refused))) => return Err(refused),
            Ok(Err(_)) => return Err(Refused::bad_answer()),
            Err(_) => return Err(Refused::unreachable()),
        };
        // The slot is held for the whole listing and goes back when this call
        // ends, on every path: a listing the caller gave up on keeps running, it
        // keeps no slot (the M2 review's G1).
        drop(permit);
        self.keep(repo, &tree);
        Ok(tree)
    }

    /// The tree kept for `repo`, when it is younger than [`CACHE_WINDOW`].
    fn cached(&self, repo: &str) -> Option<Tree> {
        let now = (self.clock)();
        let cache = self.cache.lock().unwrap_or_else(|e| e.into_inner());
        let kept = cache.get(repo)?;
        (now.checked_duration_since(kept.at)
            .is_some_and(|age| age < CACHE_WINDOW))
        .then(|| kept.tree.clone())
    }

    /// Keep `tree` for `repo`, the oldest listing going first once the cache
    /// holds [`CACHE_REPOS`] of them.
    fn keep(&self, repo: &str, tree: &Tree) {
        let mut cache = self.cache.lock().unwrap_or_else(|e| e.into_inner());
        if cache.len() >= CACHE_REPOS && !cache.contains_key(repo) {
            let oldest = cache
                .iter()
                .min_by_key(|(_, kept)| kept.at)
                .map(|(repo, _)| repo.clone());
            if let Some(oldest) = oldest {
                cache.remove(&oldest);
            }
        }
        cache.insert(
            repo.to_string(),
            Cached {
                at: (self.clock)(),
                tree: tree.clone(),
            },
        );
    }
}

/// The listing state of one router, from its fetch and the clock its cache ages
/// by: the router hands it to `routes` and to the model profile save, so the two
/// read one cache.
pub(crate) fn state(fetch: Arc<dyn Fetch + Send + Sync>, clock: Clock) -> Arc<Hf> {
    Arc::new(Hf::new(fetch, clock))
}

/// The route of this module: `GET /api/hf/files`, gated by `sessions`, listing
/// through `hf` (its fetch, its cache, its slots).
pub(crate) fn routes(hf: Arc<Hf>, sessions: Arc<Sessions>) -> axum::Router {
    axum::Router::new().route(
        "/api/hf/files",
        axum::routing::get(
            move |headers: HeaderMap, query: Query<HashMap<String, String>>| {
                let (hf, sessions) = (hf.clone(), sessions.clone());
                files(hf, sessions, headers, query.0)
            },
        ),
    )
}

/// The save's check of a `hf-file` (slice M3b of "Choosing the model file on the
/// web"): `true` when `file` is one of `repo`'s listing, read through this
/// cache, warm when the picker listed the repo just before the save; `false`
/// when the listing answered and names no such file. The listing's own failures
/// are this module's refusals, as on the route, and the save answers them at
/// [`Refused::save_response`].
pub(crate) async fn file_is_listed(hf: &Arc<Hf>, repo: &str, file: &str) -> Result<bool, Refused> {
    let tree = hf.tree(repo).await?;
    Ok(tree.files.iter().any(|kept| kept.path == file))
}

/// The answer to `GET /api/hf/files?repo=<id>`: 401 `session-required` with
/// no live session and nothing fetched (an anonymous visitor cannot make the
/// node call out); 400 `hf-repo-invalid` for an id `parse_spec` refuses, also
/// with nothing fetched; the repo's cached tree when one is kept, with the
/// caller's tag and file applied to it; 429 `hf-busy` when [`MAX_CALLS`]
/// listings are already running; the listing's document, fetched in a blocking
/// task (the runner is a child process) under [`crate::HF_TIMEOUT`], and kept
/// for the next call. The slot goes back on every path, the one where the
/// route gives up included. The read asks the manager nothing.
async fn files(
    hf: Arc<Hf>,
    sessions: Arc<Sessions>,
    headers: HeaderMap,
    query: HashMap<String, String>,
) -> Response<Body> {
    if sessions.current(&headers).is_none() {
        return crate::session::error(StatusCode::UNAUTHORIZED, "session-required", &[]);
    }
    let sent = query.get("repo").cloned().unwrap_or_default();
    let spec = match pai_core::hf::parse_spec(&sent) {
        Ok(spec) => spec,
        Err(_) => return Refused::invalid(&sent).response(),
    };
    let tree = match hf.tree(&spec.repo).await {
        Ok(tree) => tree,
        Err(refused) => return refused.response(),
    };
    match document(&tree, &spec) {
        Ok(doc) => axum::Json(doc).into_response(),
        Err(refused) => refused.response(),
    }
}

/// The two GETs of a listing, over `fetch` and the fixed endpoint: the repo's
/// tree at the commit, with no tag and no file applied (the caller's spec is
/// applied to it per request, see [`document`]).
fn listing(fetch: &dyn Fetch, repo: &str) -> Result<Tree, Refused> {
    let endpoint = pai_core::hf::bff_endpoint();
    let refs = get_json(fetch, &format!("{endpoint}api/models/{repo}/refs"), repo)?;
    let commit = pai_core::hf::commit_from_refs(&refs).ok_or_else(Refused::bad_answer)?;
    let tree = get_json(
        fetch,
        &format!("{endpoint}api/models/{repo}/tree/{commit}?recursive=true"),
        repo,
    )?;
    Ok(Tree {
        commit,
        files: pai_core::hf::files_from_tree(&tree),
    })
}

/// One GET of the listing as JSON: 200 and a body that parses, or the
/// refusal its status answers. No token is sent (section (3)).
fn get_json(fetch: &dyn Fetch, url: &str, repo: &str) -> Result<serde_json::Value, Refused> {
    let (status, body) = fetch
        .get(url, None)
        .map_err(|e| Refused::from_error(&e, repo))?;
    if status != 200 {
        return Err(Refused::from_status(status, repo));
    }
    serde_json::from_str(&body).map_err(|_| Refused::bad_answer())
}

/// The design's document (section (5)) of `tree` with the caller's `spec`
/// applied: `source`, `repo`, `revision` (the commit the tree was listed at)
/// and one entry per candidate, with its tag, file, shards, size, `sha256`
/// (the tree's `lfs.oid`), sidecars, `value` and `default`. `value` is what a
/// pick writes into the profile: `hf-repo = <repo>:<tag>`, plus `hf-file` only
/// when the tag alone names no file: another candidate shares the tag, or the
/// name carries no tag at all. The `default` is the candidate
/// `llama-server -hf <spec>` would pick, so it is computed here, per request,
/// from the caller's tag and file and never read out of the cache (the M2
/// review's D1). A tag naming no candidate is the refusal `build_listing`
/// answers.
fn document(tree: &Tree, spec: &RepoSpec) -> Result<serde_json::Value, Refused> {
    let endpoint = pai_core::hf::bff_endpoint();
    let listing = pai_core::hf::build_listing(
        &spec.repo,
        &tree.commit,
        tree.files.clone(),
        spec,
        &endpoint,
    )
    .map_err(|e| Refused::from_error(&e, &spec.repo))?;
    let files: Vec<serde_json::Value> = listing
        .candidates
        .iter()
        .enumerate()
        .map(|(at, candidate)| {
            let shared = listing
                .candidates
                .iter()
                .filter(|other| other.tag == candidate.tag)
                .count()
                > 1;
            let mut value = serde_json::json!({
                "hf-repo": format!("{}:{}", listing.repo, candidate.tag)
            });
            if shared || candidate.tag.is_empty() {
                value["hf-file"] = serde_json::json!(candidate.primary().path);
            }
            serde_json::json!({
                "tag": candidate.tag,
                "file": candidate.primary().path,
                "shards": candidate.shards.len(),
                "size": candidate.size,
                "sha256": candidate.primary().oid,
                "sidecars": {
                    "mmproj": candidate
                        .mmproj
                        .as_ref()
                        .map(|f| serde_json::Value::String(f.path.clone()))
                        .unwrap_or(serde_json::Value::Null),
                },
                "value": value,
                "default": at == listing.default,
            })
        })
        .collect();
    Ok(serde_json::json!({
        "source": "hf",
        "repo": listing.repo,
        "revision": listing.commit,
        "files": files,
    }))
}
