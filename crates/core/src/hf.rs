//! The Hugging Face side that needs no GUI and no model profile: the
//! repository spec, the listing of a repository's GGUFs, and the HTTPS that
//! answers both. `pai-app` builds on it (its `hf` module keeps the search, the
//! download and the placement estimate); `pai-bff` will list through it for the
//! web UI (ARCHITECTURE.md, "Choosing the model file on the web").
//!
//! ## The listing
//!
//! `list_repo` answers a repository's GGUF choices: two GETs,
//! `api/models/<repo>/refs` for the commit (as llama.cpp's `get_repo_commit`)
//! and `api/models/<repo>/tree/<commit>?recursive=true` for the files, grouped
//! into `Candidate`s by a Rust mirror of llama.cpp's own file rules
//! (`common/download.cpp`: `get_gguf_split_info`, `gguf_filename_is_model`,
//! `find_best_model`, `find_best_sibling`, `get_split_files`), each candidate
//! one main-model GGUF with its shards, size and the sidecars llama.cpp would
//! pair with it. llama.cpp has no "list" mode, and the picker must show sizes
//! and sidecars before a multi-GB transfer starts. Re-check the mirror on a
//! llama.cpp bump.
//!
//! ## HTTPS: `curl`, not a TLS crate
//!
//! `Fetch` is the seam: one GET answering `(status, body)`, and a range read
//! for the placement estimate, so the listing runs against canned answers in a
//! test. `Curl` is the real one: the system's `curl`, which Windows ships in
//! `System32` since 10 1803 and every mainstream Linux has. On Windows the
//! System32 copy is preferred over a PATH lookup. Only options from curl 7.55
//! (the oldest Windows copy) are used: no `--fail-with-body`, the status comes
//! back through `-w`. The url and the headers go through a config on STDIN
//! (`-K -`), never argv, because another process can read a command line and
//! the token rides a header. Every value in that config is quoted by
//! `curl_value`, which escapes `"` and `\` and REFUSES a control character, so
//! no value (a URL built from remote data included) can end its line and add
//! an option. A missing curl is `HfError::MissingTool`, naming it.
//!
//! Two profiles read that config. The configurator's (`curl_config`) follows
//! redirects and honours the endpoint llama.cpp honours (`endpoint`:
//! `MODEL_ENDPOINT`, else `HF_ENDPOINT`, else huggingface.co), because the
//! download child inherits the same variables. The bff's (`bff_curl_config`,
//! `bff_endpoint`) is the outbound boundary of section (2) of the design: a
//! fixed endpoint, `proto = "=https"`, no redirect to follow, a connect
//! timeout of 10 s, 20 s in total and an answer of at most 16 MiB.
//!
//! ## Remote paths are data
//!
//! The tree's file paths and the ids the API answers are the remote's data.
//! `valid_repo_id` (llama.cpp's own rule, `common/hf-cache.cpp`) holds a
//! repository id, `safe_repo_path` a path: only a plain relative
//! `/`-separated path, no `\`, `:`, control character, empty, `.` or `..`
//! segment. A path either refuses is dropped from the listing and refused
//! again by `pai-app`'s `DownloadJob::placements`.
//!
//! ## Secrets
//!
//! A token (`Token`, from `HF_TOKEN` or the Hugging Face CLI's token file, see
//! `token_from_env`) reaches curl through its stdin config and the download
//! child through `HF_TOKEN` in its environment, never an argv. Its `Debug`
//! prints `***` and it has no `Display`: `secret` is how the code reads its
//! text. Nothing here writes it to a model profile, a log or bench.db.

use crate::strings;
use std::fmt;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

use serde_json::Value;

/// The file-name markers llama.cpp's `gguf_filename_is_model` excludes from
/// the main-model candidates (`common/download.cpp`).
pub(crate) const NOT_A_MODEL: [&str; 6] =
    ["mmproj", "imatrix", "mtp-", "eagle3-", "dflash-", "dspark-"];

// ── Errors ───────────────────────────────────────────────────────────────

/// Every way a listing or a download fails, each with a status line a user can
/// act on (`Display`). Never carries the token.
#[derive(Debug, Clone, PartialEq)]
pub enum HfError {
    /// The repository spec does not parse.
    InvalidSpec(String),
    /// `curl` or `llama-server` is not where it should be.
    MissingTool(String),
    /// 404, or a 401 without a token: Hugging Face answers both for a repo that
    /// does not exist and for a private one.
    NotFoundOrPrivate {
        repo: String,
    },
    /// A 401 WITH a token: it is invalid or expired.
    TokenRejected {
        repo: String,
    },
    /// A 403: a gated repo whose conditions this account has not accepted.
    Gated {
        repo: String,
        had_token: bool,
    },
    Network(String),
    Http {
        status: u16,
        message: String,
    },
    /// The repo holds no main-model GGUF.
    NoGguf {
        repo: String,
    },
    /// No GGUF matches the requested quant tag or file.
    NoMatch {
        repo: String,
        wanted: String,
    },
    /// A model profile repo (`preset.ini` at its root): llama.cpp would fetch only that
    /// file and run it as a router model profile, which is not a model.
    ModelProfileRepo {
        repo: String,
    },
    /// A sidecar was requested that the chosen candidate does not have.
    SidecarMissing {
        what: &'static str,
    },
    /// A different file already sits where a download would land.
    TargetConflict {
        path: PathBuf,
        found: u64,
        expected: u64,
    },
    DiskFull(String),
    /// The router child protocol did not hold (see the module header).
    Protocol(String),
    /// The child failed for a reason no rule above recognizes: its stderr tail.
    Download(String),
    Io(String),
    Cancelled,
}

impl fmt::Display for HfError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            HfError::InvalidSpec(m) => write!(f, "{m}"),
            HfError::MissingTool(m) => write!(f, "{m}"),
            HfError::NotFoundOrPrivate { repo } => {
                f.write_str(&strings::fill("hf-not-found", &[&repo]))
            }
            HfError::TokenRejected { repo } => {
                f.write_str(&strings::fill("hf-token-rejected", &[&repo]))
            }
            HfError::Gated { repo, had_token } => {
                if *had_token {
                    f.write_str(&strings::fill("hf-gated-with-token", &[&repo]))
                } else {
                    f.write_str(&strings::fill("hf-gated", &[&repo]))
                }
            }
            HfError::Network(m) => f.write_str(&strings::fill("network-error", &[&m])),
            HfError::Http { status, message } => {
                f.write_str(&strings::fill("hf-http", &[&status, &message]))
            }
            HfError::NoGguf { repo } => f.write_str(&strings::fill("hf-no-gguf", &[&repo])),
            HfError::NoMatch { repo, wanted } => {
                f.write_str(&strings::fill("hf-no-match", &[&repo, &wanted]))
            }
            HfError::ModelProfileRepo { repo } => {
                f.write_str(&strings::fill("hf-preset-repo", &[&repo]))
            }
            HfError::SidecarMissing { what } => {
                f.write_str(&strings::fill("hf-sidecar-missing", &[&what]))
            }
            HfError::TargetConflict {
                path,
                found,
                expected,
            } => f.write_str(&strings::fill(
                "hf-target-conflict",
                &[&path.display(), &found, &expected],
            )),
            HfError::DiskFull(m) => f.write_str(&strings::fill("disk-full", &[&m])),
            HfError::Protocol(m) => f.write_str(&strings::fill("download-protocol-error", &[&m])),
            HfError::Download(m) => f.write_str(&strings::fill("download-failed", &[&m])),
            HfError::Io(m) => write!(f, "{m}"),
            HfError::Cancelled => f.write_str(&strings::t("download-cancelled")),
        }
    }
}

impl std::error::Error for HfError {}

// ── The repository spec ──────────────────────────────────────────────────

/// What the user typed, parsed: the repo id, the optional quant tag
/// (`org/repo:Q4_K_M`) and the optional exact file (a `/blob/<rev>/<file>` URL).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RepoSpec {
    pub repo: String,
    pub tag: Option<String>,
    pub file: Option<String>,
}

/// Parse `org/repo`, `org/repo:QUANT` or a Hugging Face URL
/// (`https://huggingface.co/org/repo`, also `hf.co`, with or without scheme,
/// `/tree/...`, `/blob/<rev>/<file>` or `/resolve/<rev>/<file>`, a query or a
/// fragment). The repo id is held to llama.cpp's own rule
/// (`hf_cache::is_valid_repo_id`: one `/`, `[A-Za-z0-9_]` with `.`/`-`/`/` only
/// between them), since a repo it refuses would fail later inside the child;
/// the tag to `[A-Za-z0-9_-]`, because llama.cpp builds a regex from it.
pub fn parse_spec(input: &str) -> Result<RepoSpec, HfError> {
    let s = input.trim();
    if s.is_empty() {
        return Err(HfError::InvalidSpec(strings::t("enter-a-repository")));
    }
    let lower = s.to_ascii_lowercase();
    let no_scheme = ["https://", "http://"]
        .iter()
        .find(|p| lower.starts_with(*p))
        .map_or(s, |p| &s[p.len()..]);
    let host_end = no_scheme.find('/').unwrap_or(no_scheme.len());
    let host = no_scheme[..host_end].to_ascii_lowercase();
    let is_url = matches!(
        host.as_str(),
        "huggingface.co" | "www.huggingface.co" | "hf.co"
    );
    if !is_url && no_scheme.len() != s.len() {
        return Err(HfError::InvalidSpec(strings::fill("not-a-hf-url", &[&s])));
    }

    let (repo_part, file) = if is_url {
        let path = &no_scheme[host_end..];
        let path = path.split(['?', '#']).next().unwrap_or("");
        let segs: Vec<&str> = path.split('/').filter(|x| !x.is_empty()).collect();
        if matches!(segs.first(), Some(&("datasets" | "spaces"))) {
            return Err(HfError::InvalidSpec(strings::t("url-not-a-model")));
        }
        if segs.len() < 2 {
            return Err(HfError::InvalidSpec(strings::fill("no-repo-in-url", &[&s])));
        }
        let file = match segs.get(2) {
            Some(&("blob" | "resolve")) if segs.len() >= 5 => Some(segs[4..].join("/")),
            _ => None,
        };
        (format!("{}/{}", segs[0], segs[1]), file)
    } else {
        (s.to_string(), None)
    };

    let (repo, tag) = match repo_part.rsplit_once(':') {
        Some((r, t)) => (r.to_string(), Some(t.to_string())),
        None => (repo_part, None),
    };
    if !valid_repo_id(&repo) {
        return Err(HfError::InvalidSpec(strings::fill(
            "not-a-repo-id-expected",
            &[&repo],
        )));
    }
    if let Some(t) = &tag {
        if t.is_empty()
            || !t
                .chars()
                .all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-')
        {
            return Err(HfError::InvalidSpec(strings::fill(
                "not-a-quant-tag",
                &[&t],
            )));
        }
    }
    Ok(RepoSpec { repo, tag, file })
}

/// Mirror of llama.cpp's `is_valid_repo_id` (`common/hf-cache.cpp`).
pub fn valid_repo_id(id: &str) -> bool {
    if id.is_empty() || id.len() > 256 {
        return false;
    }
    let mut slashes = 0;
    let mut special = true;
    for c in id.chars() {
        if c.is_ascii_alphanumeric() || c == '_' {
            special = false;
        } else if matches!(c, '/' | '.' | '-') {
            if special {
                return false;
            }
            slashes += usize::from(c == '/');
            special = true;
        } else {
            return false;
        }
    }
    !special && slashes == 1
}

// ── Token, endpoint ──────────────────────────────────────────────────────

/// A Hugging Face access token. Opaque on purpose: no `Display`, a `Debug`
/// that prints `***`. The field is public only so a test can build one;
/// `secret` is how the code reads it.
#[derive(Clone, PartialEq, Eq)]
pub struct Token(pub String);

impl fmt::Debug for Token {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("Token(***)")
    }
}

impl Token {
    /// Accept a token as typed (trimmed). Refuses whitespace, quotes and
    /// backslashes: they cannot occur in a real token and would break the curl
    /// config line it travels in.
    pub fn new(raw: &str) -> Result<Token, HfError> {
        let t = raw.trim();
        if t.is_empty()
            || !t
                .chars()
                .all(|c| c.is_ascii_graphic() && c != '"' && c != '\\')
        {
            return Err(HfError::InvalidSpec(strings::t("token-bad-characters")));
        }
        Ok(Token(t.to_string()))
    }

    /// The token's text, for curl's `Authorization` header and the download
    /// child's `HF_TOKEN`: never an argv, a log, a model profile, a reply or
    /// bench.db.
    pub fn secret(&self) -> &str {
        &self.0
    }
}

/// The token the user already configured for Hugging Face tools, if any:
/// `HF_TOKEN` (which llama.cpp itself reads), else the file `hf auth login`
/// writes (`HF_TOKEN_PATH`, else `$HF_HOME/token`, else
/// `$XDG_CACHE_HOME/huggingface/token`, else `~/.cache/huggingface/token`).
pub fn token_from_env() -> Option<Token> {
    token_from(
        |k| std::env::var(k).ok(),
        |p| std::fs::read_to_string(p).ok(),
    )
}

/// `token_from_env` over injected lookups, so the precedence is testable.
pub fn token_from(
    env: impl Fn(&str) -> Option<String>,
    read: impl Fn(&Path) -> Option<String>,
) -> Option<Token> {
    let set = |k: &str| env(k).filter(|v| !v.trim().is_empty());
    if let Some(t) = set("HF_TOKEN") {
        return Token::new(&t).ok();
    }
    let file = if let Some(p) = set("HF_TOKEN_PATH") {
        PathBuf::from(p)
    } else if let Some(h) = set("HF_HOME") {
        PathBuf::from(h).join("token")
    } else if let Some(x) = set("XDG_CACHE_HOME") {
        PathBuf::from(x).join("huggingface").join("token")
    } else {
        let home = set(if cfg!(windows) { "USERPROFILE" } else { "HOME" })?;
        PathBuf::from(home)
            .join(".cache")
            .join("huggingface")
            .join("token")
    };
    read(&file).and_then(|t| Token::new(&t).ok())
}

/// The Hugging Face endpoint, as llama.cpp resolves it
/// (`common_get_model_endpoint`): `MODEL_ENDPOINT`, else `HF_ENDPOINT`, else
/// `https://huggingface.co/`, always ending in `/`. The child inherits the same
/// variables, so the listing and the transfer talk to the same host.
pub fn endpoint() -> String {
    endpoint_from(
        std::env::var("MODEL_ENDPOINT").ok(),
        std::env::var("HF_ENDPOINT").ok(),
    )
}

pub fn endpoint_from(model: Option<String>, hf: Option<String>) -> String {
    let mut e = model
        .filter(|s| !s.is_empty())
        .or(hf.filter(|s| !s.is_empty()))
        .unwrap_or_else(|| "https://huggingface.co/".into());
    if !e.ends_with('/') {
        e.push('/');
    }
    e
}

/// Replace every occurrence of the token in `text` (a stderr line on its way
/// into an error message).
pub fn redact(text: &str, token: Option<&Token>) -> String {
    match token {
        Some(t) if !t.0.is_empty() => text.replace(&t.0, "***"),
        _ => text.to_string(),
    }
}

// ── HTTPS through curl ───────────────────────────────────────────────────

/// One HTTPS GET returning `(status, body)`. A trait so the listing is tested
/// against canned answers instead of the network.
pub trait Fetch {
    fn get(&self, url: &str, token: Option<&Token>) -> Result<(u16, String), HfError>;
    /// The bytes `start..start + len` of a file (a GGUF header,
    /// `hf_fit::fetch_shape`), following redirects: `(status, bytes)`, 206 for
    /// an honoured range. The body is BINARY, so it is never decoded.
    fn get_range(
        &self,
        url: &str,
        token: Option<&Token>,
        start: u64,
        len: u64,
    ) -> Result<(u16, Vec<u8>), HfError>;
}

/// The real `Fetch`: the system's `curl` (see the module header).
pub struct Curl;

impl Curl {
    /// Run curl over `config` (on its stdin) and return its stdout.
    fn run(config: Result<String, HfError>, token: Option<&Token>) -> Result<Vec<u8>, HfError> {
        let config = config?;
        let exe = curl_exe();
        let mut cmd = Command::new(&exe);
        cmd.args([
            "-sS",
            "-L",
            "--connect-timeout",
            "20",
            "--max-time",
            "120",
            "-K",
            "-",
        ])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
        crate::proc::hide_console(&mut cmd);
        let mut child = cmd
            .spawn()
            .map_err(|e| HfError::MissingTool(strings::fill("curl-not-started", &[&e])))?;
        if let Some(mut stdin) = child.stdin.take() {
            // A write error surfaces as curl's own "no URL" failure below.
            let _ = stdin.write_all(config.as_bytes());
        }
        let out = child
            .wait_with_output()
            .map_err(|e| HfError::Io(format!("curl: {e}")))?;
        if !out.status.success() {
            let code = out.status.code().unwrap_or(-1);
            let err = redact(String::from_utf8_lossy(&out.stderr).trim(), token);
            return Err(curl_failure(code, &err));
        }
        Ok(out.stdout)
    }
}

impl Fetch for Curl {
    fn get(&self, url: &str, token: Option<&Token>) -> Result<(u16, String), HfError> {
        let out = Curl::run(curl_config(url, token), token)?;
        parse_curl_output(&String::from_utf8_lossy(&out))
    }

    fn get_range(
        &self,
        url: &str,
        token: Option<&Token>,
        start: u64,
        len: u64,
    ) -> Result<(u16, Vec<u8>), HfError> {
        let end = start + len.max(1) - 1;
        let out = Curl::run(
            curl_config_with(url, token, "*/*", Some((start, end))),
            token,
        )?;
        parse_curl_bytes(out)
    }
}

/// `parse_curl_output` for a binary body: the status is what follows the
/// LAST newline (the write-out appends `\n<code>`), the body everything
/// before it, newlines inside it included.
pub fn parse_curl_bytes(mut stdout: Vec<u8>) -> Result<(u16, Vec<u8>), HfError> {
    let (cut, code_start) = match stdout.iter().rposition(|&b| b == b'\n') {
        Some(i) => (i, i + 1),
        None => (0, 0),
    };
    let code = String::from_utf8_lossy(&stdout[code_start..]).to_string();
    let status = code
        .trim()
        .parse::<u16>()
        .map_err(|_| HfError::Network(strings::fill("curl-no-status", &[&code.trim()])))?;
    if status == 0 {
        return Err(HfError::Network(strings::t("no-answer")));
    }
    stdout.truncate(cut);
    Ok((status, stdout))
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

/// One value of the curl config, double-quoted as curl's config parser reads
/// it: `\` and `"` are escaped, and a control character (a newline above
/// all, which would end the line and start an option of the caller's
/// choosing) is refused outright rather than escaped. Every value goes
/// through here, whatever its source: a repo id, a file path from the tree,
/// a `Token` and a percent-encoded query are all held to safe characters
/// upstream, but the config must not depend on every caller getting that
/// right.
pub fn curl_value(v: &str) -> Result<String, HfError> {
    if v.chars().any(char::is_control) {
        return Err(HfError::InvalidSpec(strings::t(
            "request-control-character",
        )));
    }
    let mut out = String::with_capacity(v.len() + 2);
    out.push('"');
    for c in v.chars() {
        if matches!(c, '"' | '\\') {
            out.push('\\');
        }
        out.push(c);
    }
    out.push('"');
    Ok(out)
}

/// The curl config read from stdin: url, headers and the status write-out
/// (`curl_value` quotes each value). `accept` is the `Accept` header (JSON for
/// the API, anything for a header read); `range` adds a byte range
/// (`start-end`, inclusive) and a `max-filesize` just above it, so a server
/// that ignores the range and answers the WHOLE file is refused by its
/// `Content-Length` instead of streamed into memory.
pub fn curl_config_with(
    url: &str,
    token: Option<&Token>,
    accept: &str,
    range: Option<(u64, u64)>,
) -> Result<String, HfError> {
    let mut c = format!(
        "url = {}\nheader = {}\nuser-agent = {}\nwrite-out = \"\\n%{{http_code}}\"\n",
        curl_value(url)?,
        curl_value(&format!("Accept: {accept}"))?,
        curl_value(&format!("pai-config/{}", env!("CARGO_PKG_VERSION")))?,
    );
    if let Some((start, end)) = range {
        c.push_str(&format!(
            "range = \"{start}-{end}\"\nmax-filesize = {}\n",
            end - start + 1 + 65536
        ));
    }
    if let Some(t) = token {
        c.push_str(&format!(
            "header = {}\n",
            curl_value(&format!("Authorization: Bearer {}", t.secret()))?
        ));
    }
    Ok(c)
}

/// The API's config: JSON, no range.
pub fn curl_config(url: &str, token: Option<&Token>) -> Result<String, HfError> {
    curl_config_with(url, token, "application/json", None)
}

/// Split curl's stdout into the body and the status the write-out appended.
pub fn parse_curl_output(stdout: &str) -> Result<(u16, String), HfError> {
    let (body, code) = stdout.rsplit_once('\n').unwrap_or(("", stdout));
    let status = code
        .trim()
        .parse::<u16>()
        .map_err(|_| HfError::Network(strings::fill("curl-no-status", &[&code.trim()])))?;
    if status == 0 {
        return Err(HfError::Network(strings::t("no-answer")));
    }
    Ok((status, body.to_string()))
}

/// curl's exit codes worth a sentence of their own (`man curl`, EXIT CODES).
pub fn curl_failure(code: i32, stderr: &str) -> HfError {
    let what = match code {
        5 | 6 => strings::t("curl-no-host"),
        7 => strings::t("curl-no-connect"),
        28 => strings::t("curl-timed-out"),
        35 | 51 | 58 | 59 | 60 | 77 => strings::t("curl-tls"),
        _ => String::new(),
    };
    if what.is_empty() {
        HfError::Network(strings::fill("curl-failed", &[&code, &stderr]))
    } else {
        HfError::Network(format!("{what}: {stderr}"))
    }
}

/// An API status other than 200, as an error. `body` is the API's JSON
/// (`{"error": "..."}`) when it has one.
pub fn api_error(status: u16, body: &str, had_token: bool, repo: &str) -> HfError {
    let repo = repo.to_string();
    match status {
        401 if had_token => HfError::TokenRejected { repo },
        401 | 404 => HfError::NotFoundOrPrivate { repo },
        403 => HfError::Gated { repo, had_token },
        429 => HfError::Network(strings::t("hf-rate-limited")),
        _ => HfError::Http {
            status,
            message: api_message(body),
        },
    }
}

/// The API's `{"error": "..."}`, else the body's first 200 characters.
pub fn api_message(body: &str) -> String {
    serde_json::from_str::<Value>(body)
        .ok()
        .and_then(|v| v.get("error").and_then(Value::as_str).map(str::to_string))
        .unwrap_or_else(|| body.chars().take(200).collect())
}

pub(crate) fn api_json(
    fetch: &dyn Fetch,
    url: &str,
    token: Option<&Token>,
    repo: &str,
) -> Result<Value, HfError> {
    let (status, body) = fetch.get(url, token)?;
    if status != 200 {
        return Err(api_error(status, &body, token.is_some(), repo));
    }
    serde_json::from_str(&body)
        .map_err(|e| HfError::Network(strings::fill("unreadable-answer", &[&url, &e])))
}

// ── The repository listing ───────────────────────────────────────────────

/// One file of the repo tree: its path in the repo (`/`-separated), size in
/// bytes (the LFS size for a GGUF) and object id.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RepoFile {
    pub path: String,
    pub size: u64,
    pub oid: String,
}

impl RepoFile {
    /// The file name, without the repo directories.
    pub fn name(&self) -> &str {
        self.path.rsplit('/').next().unwrap_or(&self.path)
    }
}

/// Whether a repo file path from the API is safe to join under a local
/// directory and to put in a URL. The tree listing is the REMOTE's data, so a
/// path is taken only as a plain relative `/`-separated path: no backslash
/// (a separator on Windows), no empty segment (which also refuses a leading
/// `/` and `//`), no `.` or `..` segment, no `:` (a drive prefix, or an NTFS
/// stream), no control character. Hugging Face itself never lists such a
/// path; one that does is dropped from the listing (`files_from_tree`,
/// `build_listing`) and refused by `DownloadJob::placements`.
pub fn safe_repo_path(path: &str) -> bool {
    !path.is_empty()
        && !path.contains(['\\', ':'])
        && !path.chars().any(char::is_control)
        && path
            .split('/')
            .all(|seg| !seg.is_empty() && seg != "." && seg != "..")
}

/// A drafter that can come with a model, by its file-name prefix (llama.cpp's
/// `find_best_mtp` / `find_best_dflash` / `find_best_dspark`). Eagle3 is left
/// out: the framework has no `draft-eagle3` handling yet.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Sidecar {
    Mtp,
    Dflash,
    Dspark,
}

impl Sidecar {
    pub const ALL: [Sidecar; 3] = [Sidecar::Mtp, Sidecar::Dflash, Sidecar::Dspark];

    /// The file-name keyword llama.cpp matches it by.
    pub fn keyword(self) -> &'static str {
        match self {
            Sidecar::Mtp => "mtp-",
            Sidecar::Dflash => "dflash-",
            Sidecar::Dspark => "dspark-",
        }
    }

    /// The `--spec-type` it runs under, and the fallback when its header names
    /// none (`gguf::draft_spec_type`).
    pub fn spec_type(self) -> &'static str {
        match self {
            Sidecar::Mtp => "draft-mtp",
            Sidecar::Dflash => "draft-dflash",
            Sidecar::Dspark => "draft-dspark",
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Sidecar::Mtp => "MTP",
            Sidecar::Dflash => "DFlash",
            Sidecar::Dspark => "DSpark",
        }
    }

    /// `mtp` / `dflash` / `dspark`, case-insensitive (the CLI's `--draft`).
    pub fn parse(s: &str) -> Option<Sidecar> {
        Sidecar::ALL
            .into_iter()
            .find(|x| x.label().eq_ignore_ascii_case(s.trim()))
    }
}

/// One main-model choice: its shard(s), total size, quant tag, and the sidecars
/// llama.cpp would pair with it.
#[derive(Debug, Clone, PartialEq)]
pub struct Candidate {
    /// The quant tag llama.cpp reads off the file name (`Q4_K_M`, `UD-Q4_K_XL`
    /// reads `Q4_K_XL`), upper-case; empty when the name carries none.
    pub tag: String,
    /// Every shard, the first one first (one entry for an unsplit file).
    pub shards: Vec<RepoFile>,
    /// All shards together, in bytes.
    pub size: u64,
    pub mmproj: Option<RepoFile>,
    pub mtp: Option<RepoFile>,
    pub dflash: Option<RepoFile>,
    pub dspark: Option<RepoFile>,
}

impl Candidate {
    /// The file llama.cpp loads (the first shard).
    pub fn primary(&self) -> &RepoFile {
        &self.shards[0]
    }

    pub fn sidecar(&self, s: Sidecar) -> Option<&RepoFile> {
        match s {
            Sidecar::Mtp => self.mtp.as_ref(),
            Sidecar::Dflash => self.dflash.as_ref(),
            Sidecar::Dspark => self.dspark.as_ref(),
        }
    }
}

/// A repo's GGUF choices.
#[derive(Debug, Clone, PartialEq)]
pub struct Listing {
    pub repo: String,
    /// The commit the tree was listed at (the `main` branch, else the first).
    pub commit: String,
    /// The model page, what a derived model profile's `project_url` holds.
    pub project_url: String,
    pub candidates: Vec<Candidate>,
    /// The candidate `llama-server -hf <spec>` would pick: the spec's file or
    /// tag, else `Q4_K_M`, else `Q8_0`, else the first model file.
    pub default: usize,
}

/// List `spec`'s repository over the HF API (blocking; run it off the UI
/// thread). See `Listing`.
pub fn list_repo(spec: &RepoSpec, token: Option<&Token>) -> Result<Listing, HfError> {
    list_repo_with(&Curl, &endpoint(), spec, token)
}

/// `list_repo` over an injected `Fetch` and endpoint: the refs call for the
/// commit (as `hf_cache::get_repo_commit`), then the recursive tree at it.
pub fn list_repo_with(
    fetch: &dyn Fetch,
    endpoint: &str,
    spec: &RepoSpec,
    token: Option<&Token>,
) -> Result<Listing, HfError> {
    let repo = &spec.repo;
    let refs = api_json(
        fetch,
        &format!("{endpoint}api/models/{repo}/refs"),
        token,
        repo,
    )?;
    let commit = commit_from_refs(&refs).ok_or_else(|| HfError::Http {
        status: 200,
        message: strings::fill("repo-lists-no-branch", &[&repo]),
    })?;
    let tree = api_json(
        fetch,
        &format!("{endpoint}api/models/{repo}/tree/{commit}?recursive=true"),
        token,
        repo,
    )?;
    build_listing(repo, &commit, files_from_tree(&tree), spec, endpoint)
}

/// The commit of `main`, else of the first branch with a valid commit
/// (`get_repo_commit`'s rule).
pub fn commit_from_refs(refs: &Value) -> Option<String> {
    let branches = refs.get("branches")?.as_array()?;
    let mut first = None;
    for b in branches {
        let (Some(name), Some(commit)) = (
            b.get("name").and_then(Value::as_str),
            b.get("targetCommit").and_then(Value::as_str),
        ) else {
            continue;
        };
        if commit.len() != 40 || !commit.chars().all(|c| c.is_ascii_hexdigit()) {
            continue;
        }
        if name == "main" {
            return Some(commit.to_string());
        }
        first.get_or_insert_with(|| commit.to_string());
    }
    first
}

/// The `type: file` entries of a recursive tree answer, in API order (the
/// order llama.cpp's first-match rules see). LFS files carry their real size
/// and oid under `lfs`. A path `safe_repo_path` refuses is dropped.
pub fn files_from_tree(tree: &Value) -> Vec<RepoFile> {
    let Some(items) = tree.as_array() else {
        return Vec::new();
    };
    items
        .iter()
        .filter(|i| i.get("type").and_then(Value::as_str) == Some("file"))
        .filter_map(|i| {
            let path = i
                .get("path")?
                .as_str()
                .filter(|p| safe_repo_path(p))?
                .to_string();
            let lfs = i.get("lfs").filter(|l| l.is_object());
            let size = lfs
                .and_then(|l| l.get("size"))
                .or_else(|| i.get("size"))
                .and_then(Value::as_u64)
                .unwrap_or(0);
            let oid = lfs
                .and_then(|l| l.get("oid"))
                .or_else(|| i.get("oid"))
                .and_then(Value::as_str)
                .unwrap_or("")
                .to_string();
            Some(RepoFile { path, size, oid })
        })
        .collect()
}

/// Group a file list into candidates (pure; see `Listing`). A path
/// `safe_repo_path` refuses is dropped here too, whoever built the list.
pub fn build_listing(
    repo: &str,
    commit: &str,
    files: Vec<RepoFile>,
    spec: &RepoSpec,
    endpoint: &str,
) -> Result<Listing, HfError> {
    let files: Vec<RepoFile> = files
        .into_iter()
        .filter(|f| safe_repo_path(&f.path))
        .collect();
    if files.iter().any(|f| f.path == "preset.ini") {
        return Err(HfError::ModelProfileRepo { repo: repo.into() });
    }
    let mut candidates = Vec::new();
    for f in files.iter().filter(|f| is_model_file(&f.path)) {
        let split = split_info(&f.path);
        if split.count > 1 && split.index != 1 {
            continue;
        }
        let shards = split_files(&files, f);
        let sib = |kw: &str, tag: &str| {
            let s = find_best_sibling(&files, &f.path, kw, tag);
            (!s.path.is_empty()).then_some(s)
        };
        candidates.push(Candidate {
            size: shards.iter().map(|s| s.size).sum(),
            mmproj: sib("mmproj", ""),
            mtp: sib(Sidecar::Mtp.keyword(), &split.tag),
            dflash: sib(Sidecar::Dflash.keyword(), &split.tag),
            dspark: sib(Sidecar::Dspark.keyword(), &split.tag),
            tag: split.tag,
            shards,
        });
    }
    if candidates.is_empty() {
        return Err(HfError::NoGguf { repo: repo.into() });
    }
    let index_of = |path: &str| candidates.iter().position(|c| c.primary().path == path);
    let default = if let Some(file) = &spec.file {
        index_of(file).ok_or_else(|| HfError::NoMatch {
            repo: repo.into(),
            wanted: file.clone(),
        })?
    } else {
        let tag = spec.tag.as_deref().unwrap_or("");
        let best = find_best_model(&files, tag);
        index_of(&best.path).ok_or_else(|| HfError::NoMatch {
            repo: repo.into(),
            wanted: strings::fill("the-tag", &[&tag]),
        })?
    };
    Ok(Listing {
        repo: repo.into(),
        commit: commit.into(),
        project_url: format!("{endpoint}{repo}"),
        candidates,
        default,
    })
}

// ── llama.cpp's file-matching rules (common/download.cpp), mirrored ────

/// `gguf_split_info`: the path without `.gguf` and without a
/// `-NNNNN-of-NNNNN` suffix, its upper-cased quant tag, and the shard numbers.
/// A path not ending in `.gguf` (case-sensitive, as upstream) is all defaults.
#[derive(Debug, Default, PartialEq)]
pub struct SplitInfo {
    pub prefix: String,
    pub tag: String,
    pub index: u32,
    pub count: u32,
}

pub fn split_info(path: &str) -> SplitInfo {
    let Some(stem) = path.strip_suffix(".gguf") else {
        return SplitInfo::default();
    };
    let (prefix, index, count) = match crate::gguf_shape::split_shard_suffix(stem) {
        Some((base, idx)) => {
            let total = stem.rsplit_once("-of-").map_or("1", |x| x.1);
            (base, idx.parse().unwrap_or(1), total.parse().unwrap_or(1))
        }
        None => (stem, 1, 1),
    };
    // `[-.]([A-Z0-9_]+)$`: the run after the LAST `-` or `.`, when it is all
    // word characters (anything else before the end means no match at all).
    let tag = prefix
        .rfind(['-', '.'])
        .map(|i| &prefix[i + 1..])
        .filter(|t| !t.is_empty() && t.chars().all(|c| c.is_ascii_alphanumeric() || c == '_'))
        .map(str::to_ascii_uppercase)
        .unwrap_or_default();
    SplitInfo {
        prefix: prefix.to_string(),
        tag,
        index,
        count,
    }
}

/// `extract_quant_bits`: the number after the tag's first digit run starts
/// (`Q4_K_M` → 4, `BF16` → 16), 0 without one.
pub fn quant_bits(path: &str) -> i64 {
    bits_of_tag(&split_info(path).tag)
}

pub fn bits_of_tag(tag: &str) -> i64 {
    let Some(pos) = tag.find(|c: char| c.is_ascii_digit()) else {
        return 0;
    };
    let digits: String = tag[pos..]
        .chars()
        .take_while(char::is_ascii_digit)
        .collect();
    digits.parse().unwrap_or(i64::MAX)
}

/// `gguf_filename_is_model`: a `.gguf` whose FILE name (not directory) carries
/// none of the sidecar / imatrix markers.
pub fn is_model_file(path: &str) -> bool {
    if !path.ends_with(".gguf") {
        return false;
    }
    let name = path.rsplit('/').next().unwrap_or(path);
    !NOT_A_MODEL.iter().any(|m| name.contains(m))
}

/// `get_split_files`: every shard of `file`'s split (same prefix and count).
pub(crate) fn split_files(files: &[RepoFile], file: &RepoFile) -> Vec<RepoFile> {
    let split = split_info(&file.path);
    if split.count <= 1 {
        return vec![file.clone()];
    }
    let mut out: Vec<RepoFile> = files
        .iter()
        .filter(|f| {
            let s = split_info(&f.path);
            s.count == split.count && s.prefix == split.prefix
        })
        .cloned()
        .collect();
    out.sort_by_key(|f| split_info(&f.path).index);
    out
}

/// Whether `path` carries `tag` as a whole token, upstream's
/// `(^|[-./])<tag>[.-]`: `F16` is not in a `BF16` file, and `_` is not a
/// separator. Both already lower-cased.
pub(crate) fn has_tag_token(path: &str, tag: &str) -> bool {
    path.match_indices(tag).any(|(at, _)| {
        let before = path[..at].chars().next_back();
        let after = path[at + tag.len()..].chars().next();
        matches!(before, None | Some('-' | '.' | '/')) && matches!(after, Some('.' | '-'))
    })
}

/// `find_best_model`: the first model file carrying `<tag>` as a token
/// (case-insensitive, [`has_tag_token`]) that is not a later shard; with no tag, `Q4_K_M`, then
/// `Q8_0`, then the first model file. An empty `path` when nothing matches.
pub fn find_best_model(files: &[RepoFile], tag: &str) -> RepoFile {
    let tags: Vec<&str> = if tag.is_empty() {
        vec!["Q4_K_M", "Q8_0"]
    } else {
        vec![tag]
    };
    let first_shard = |f: &RepoFile| {
        let s = split_info(&f.path);
        !(s.count > 1 && s.index != 1)
    };
    for t in tags {
        let t = t.to_ascii_lowercase();
        if let Some(f) = files.iter().find(|f| {
            is_model_file(&f.path)
                && has_tag_token(&f.path.to_ascii_lowercase(), &t)
                && first_shard(f)
        }) {
            return f.clone();
        }
    }
    if tag.is_empty() {
        if let Some(f) = files
            .iter()
            .find(|f| is_model_file(&f.path) && first_shard(f))
        {
            return f.clone();
        }
    }
    RepoFile {
        path: String::new(),
        size: 0,
        oid: String::new(),
    }
}

/// `find_best_sibling`: among the `.gguf` files whose path contains `keyword`
/// and that sit in `model`'s directory or one of its ancestors, the deepest,
/// then an exact `-<TAG>.` match, then the closest quant bits (to `tag`'s when
/// given, else to the model's). Ties keep the earlier file. Empty `path` when
/// none qualifies.
pub fn find_best_sibling(files: &[RepoFile], model: &str, keyword: &str, tag: &str) -> RepoFile {
    let tag_upper = tag.to_ascii_uppercase();
    let model_bits = if tag_upper.is_empty() {
        quant_bits(model)
    } else {
        bits_of_tag(&tag_upper)
    };
    let model_parts: Vec<&str> = model.split('/').collect();
    let model_dir = &model_parts[..model_parts.len() - 1];

    let mut best: Option<(&RepoFile, usize, i64, bool)> = None;
    for f in files {
        if !f.path.ends_with(".gguf") || !f.path.contains(keyword) {
            continue;
        }
        let sib_parts: Vec<&str> = f.path.split('/').collect();
        let sib_dir = &sib_parts[..sib_parts.len() - 1];
        let depth = model_dir
            .iter()
            .zip(sib_dir.iter())
            .take_while(|(a, b)| a == b)
            .count();
        if depth != sib_dir.len() {
            continue;
        }
        let diff = (quant_bits(&f.path) - model_bits).abs();
        let exact = !tag_upper.is_empty()
            && f.path
                .to_ascii_uppercase()
                .contains(&format!("-{tag_upper}."));
        let better = match best {
            None => true,
            Some((_, bd, bdiff, bexact)) => {
                depth > bd
                    || (depth == bd && exact && !bexact)
                    || (depth == bd && exact == bexact && diff < bdiff)
            }
        };
        if better {
            best = Some((f, depth, diff, exact));
        }
    }
    best.map(|b| b.0.clone()).unwrap_or(RepoFile {
        path: String::new(),
        size: 0,
        oid: String::new(),
    })
}

// ── The bff's profile ────────────────────────────────────────────────────

/// The endpoint the bff lists from, fixed (ARCHITECTURE.md, "Choosing the
/// model file on the web" (2)): `MODEL_ENDPOINT` and `HF_ENDPOINT`, which
/// `endpoint` honours for the configurator and the download child, never move
/// it. The browser sends a repository id only, so the two URLs the bff builds
/// must name huggingface.co. A mirror is a `bff.ini` key, later.
pub fn bff_endpoint() -> String {
    "https://huggingface.co/".to_string()
}

/// The curl config of one bff request, for a `curl -sS -K -` run with NO `-L`
/// (the app's `Curl::run` passes `-L`; the bff's argv does not, and this
/// config carries no `location` line): a 3xx answers `hf-repo-moved`, so no
/// request reaches a host the answer names. curl restricts a redirect's
/// protocol, never its host (`--proto-redir`, man curl), hence the bounds of
/// section (2) here: `proto = "=https"` so no plain HTTP leaves the node,
/// `connect-timeout` 10, `max-time` 20 (under the route's 25 s deadline) and
/// `max-filesize` 16 MiB. The token rides a header on stdin, never the url.
pub fn bff_curl_config(url: &str, token: Option<&Token>) -> Result<String, HfError> {
    let mut c = format!(
        "url = {url}\nproto = \"=https\"\nconnect-timeout = \"10\"\nmax-time = \"20\"\nmax-filesize = \"16777216\"\nheader = {accept}\nuser-agent = {agent}\nwrite-out = \"\\n%{{http_code}}\"\n",
        url = curl_value(url)?,
        accept = curl_value("Accept: application/json")?,
        agent = curl_value(&format!("pai-bff/{}", env!("CARGO_PKG_VERSION")))?,
    );
    if let Some(t) = token {
        c.push_str(&format!(
            "header = {}\n",
            curl_value(&format!("Authorization: Bearer {}", t.secret()))?
        ));
    }
    Ok(c)
}
