//! Create a preset from a Hugging Face repository: find GGUF repositories by
//! name, list the GGUFs a repo offers, download one with llama.cpp's OWN
//! downloader, file it into the ModelsDir subfolders the Models tab already
//! scans, and derive the preset's fields from the downloaded headers.
//!
//! ## The steps, and who calls them
//!
//! 0. `classify_input` + `search`: what the user typed is either a repository
//!    spec (step 1, straight to the listing) or a free-text query, which
//!    `search` sends to the HF model search and answers with `SearchHit`s, one
//!    per GGUF repository: `org/repo`, downloads, gated/private, and the quant
//!    tags and sidecars its file names show. Picking a hit is step 2 on its
//!    `org/repo`. See "The search" below.
//! 1. `parse_spec`: `org/repo`, `org/repo:QUANT`, or a `https://huggingface.co/`
//!    URL (a `/blob/<rev>/<file>` URL also names the file) into a `RepoSpec`.
//! 2. `list_repo`: the repo's file tree over the HF API (blocking, a few
//!    hundred ms), grouped into `Candidate`s, one per main-model GGUF with its
//!    shards, size and the sidecars llama.cpp would pair with it.
//! 3. `spawn_download` (or the blocking `run_download` it wraps): fetch the
//!    chosen files, reporting `Progress`, cancellable through the returned
//!    `DownloadHandle`, ending in `Downloaded` (the final paths) or an `HfError`
//!    whose `Display` is the status line.
//! 4. `preset_from_download` (impure: reads the headers) over `derive_preset`
//!    (pure): the `Preset` to save.
//!
//! Everything slow is meant to run on a `std::thread` (steps 0 and 2 by the
//! caller, step 3 by `spawn_download` itself), its result posted back with
//! `slint::invoke_from_event_loop`: the GUI's threading rule (`gui.rs`
//! header). Nothing here touches Slint. The two callers are the CLI's
//! `preset download` (`cli.rs`) and the Models tab's From Hugging Face dialog
//! (`gui/hf_dialog.rs`), which also reads `Progress`'s per-file bytes and asks
//! `has_partial` whether its button should read "Resume download".
//!
//! ## The search: one request, every row's file names included
//!
//! `GET /api/models?search=<query>&filter=gguf&sort=downloads&direction=-1
//! &limit=<n>&expand[]=downloads&expand[]=gated&expand[]=private
//! &expand[]=siblings` (`search_url`). Verified against huggingface.co with
//! curl (2026-10-27):
//! - `search` matches the words of the query against the repo id, org
//!   included, in any order: `qwen3 8b` finds `Qwen/Qwen3-8B-GGUF`, `unsloth
//!   qwen3 8b` finds `unsloth/Qwen3-8B-GGUF`; an `org/name` fragment matches too.
//! - `filter=gguf` keeps the repositories tagged with the GGUF library;
//!   `sort=downloads` + `direction=-1` puts the most downloaded first; `limit`
//!   caps the rows (`SEARCH_LIMIT`). No match is `[]` with a 200.
//! - With `expand[]` the answer carries ONLY `_id`, `id` and the expanded
//!   fields, which keeps it small: `downloads` (the last 30 days, the number
//!   the model page shows), `gated` (`false`, or the string `"auto"` /
//!   `"manual"`), `private`, and `siblings`, the repo's FULL file list as
//!   `{"rfilename": "<path>"}` (no sizes). So the quantizations of every row
//!   come with the search itself: no request per row. Sizes need the tree,
//!   which is the listing's job once a row is picked. (`expand[]=gguf` would
//!   add the parameter count, but also each repo's whole chat template.)
//! - The brackets are sent percent-encoded (`expand%5B%5D`): curl would
//!   otherwise read `[]` as its own URL glob. The API takes both spellings.
//! - A token is sent when there is one, so the account's private repos can
//!   match; an invalid token is ignored by this endpoint (200, public results).
//!
//! The quant tags shown per row (`quants_of`) come from the same mirror of
//! llama.cpp's file rules the listing uses (`split_info`, `is_model_file`):
//! one tag per main-model GGUF (a split counted once, `UD-Q4_K_XL` reading
//! `Q4_K_XL` as llama.cpp reads it), deduplicated, ordered by bit count.
//!
//! What counts as a spec and what as a query (`classify_input`): anything
//! `parse_spec` accepts is a spec, so `org/repo` never costs a search round
//! trip; text without a slash or with a space is a query; slash-carrying text
//! without a space that does NOT parse (`org/repo:`, `a/b/c`, a non-HF URL)
//! is a mistyped spec, reported with `parse_spec`'s error rather than searched.
//!
//! ## Why the listing mirrors llama.cpp instead of asking it
//!
//! The candidates and their sidecars are chosen by a Rust mirror of
//! `common/download.cpp` (`get_gguf_split_info`, `gguf_filename_is_model`,
//! `find_best_model`, `find_best_sibling`, `get_split_files`): llama.cpp has no
//! "list" mode, and the picker must show sizes and sidecars BEFORE a multi-GB
//! transfer starts. The download then names every file EXACTLY (`-hff`, one
//! child per file group), so what the list promises is what lands on disk, and
//! the mirror only decides which mmproj / MTP / DFlash / DSpark file goes with a
//! model, by upstream's own rules: a sibling whose path contains the keyword
//! (`mmproj`, `mtp-`, `dflash-`, `dspark-`), in the model's directory or an
//! ancestor, deepest first, then an exact `-<TAG>.` match, then the closest quant
//! bit count. Re-check the mirror on a llama.cpp bump.
//!
//! ## HTTPS: `curl`, not a TLS crate
//!
//! The listing needs two HTTPS GETs (`/api/models/<repo>/refs`, then
//! `/tree/<commit>?recursive=true`), the search one (`/api/models?search=`),
//! and the crate has no TLS (`http.rs` header: a hand-rolled loopback client,
//! `reqwest` rejected). A TLS stack is a dozen crates plus a certificate store
//! for three GETs, so this shells out to `curl`,
//! which Windows ships in `System32` since 10 1803 and every mainstream Linux
//! has. On Windows the System32 copy is preferred over a PATH lookup. Only
//! options from curl 7.55 (the oldest Windows copy) are used: no `--fail-with-body`,
//! the status comes back through `-w`. The url and the headers go through a
//! config on STDIN (`-K -`), never argv, because another process can read a
//! command line and the token rides a header. A missing curl is
//! `HfError::MissingTool`, naming it.
//!
//! ## The transfer: llama-server's router download child
//!
//! The bytes are fetched by `llama-server` itself, started the way its own
//! router starts a model download (`tools/server/server-models.cpp`): the env
//! var `LLAMA_SERVER_ROUTER_PORT` (its presence alone makes the process a
//! child, `server_child::is_child`), `LLAMA_SERVER_CHILD_MODE=download`,
//! `LLAMA_ARG_HF_REPO` and `LLAMA_ARG_HF_FILE`. No model is loaded and no port
//! is opened in that mode (`server.cpp` returns from `child.run_download`).
//! Why not `llama download`: it draws progress only on a TTY, so a piped child
//! is silent for gigabytes, and its only cancel is a kill. The router protocol
//! gives both:
//!
//! - STDOUT carries only protocol lines (the child dups its stdout aside and
//!   points fd 1 at stderr, `server_reserve_stdout`), each
//!   `cmd_child_to_router:state:` + `{"state":"downloading","payload":{...}}`:
//!   `{"url","downloaded","total"}` per file in flight (files of one plan run in
//!   parallel threads, so progress is tracked per url, at most every 100 ms),
//!   then `{"result":"download_finished"|"download_failed"}` (the reason is on
//!   stderr).
//! - The child then WAITS for `cmd_router_to_child:exit` on stdin, and this
//!   module sends it only AFTER the result line. During the transfer the same
//!   line would be the router's cooperative cancel, and that is exactly what a
//!   cancel here must NOT use: once `is_cancelled()` turns true, upstream's
//!   `common_download_file_single_online` DELETES the `.downloadInProgress`
//!   partial (`common/download.cpp`, after `on_done`), so a cancelled 15 GB
//!   transfer would restart from zero. A cancel closes the child's stdin
//!   instead: EOF makes its monitor thread `exit(1)` on the spot, with no
//!   cleanup, and the partial survives for a range-request resume (`cancel_stop`,
//!   pinned by a test). A kill follows if the child is still alive 10 s later.
//!
//! It is the router's INTERNAL protocol, pinned by two string constants in
//! `server-models.cpp` (`CMD_CHILD_TO_ROUTER_STATE`, `CMD_ROUTER_TO_CHILD_EXIT`,
//! mirrored as `CHILD_STATE_PREFIX` / `CHILD_EXIT_CMD`): re-check both on every
//! llama.cpp bump. A change must fail loudly, never as a silent success: a
//! prefixed line that does not parse, an unknown state or result, and a child
//! that exits without a result line are all `HfError::Protocol` carrying the
//! child's stderr tail. And `download_finished` is not trusted on its own:
//! `run_download` only catches EXCEPTIONS, while a 401/404 during the plan is
//! logged and swallowed into an EMPTY plan, which then "finishes" having fetched
//! nothing. So every expected file must be found on disk afterwards, or the
//! stderr tail is classified into an error.
//!
//! ## Where the files go: staged, then moved into the existing subfolders
//!
//! The ModelsDir layout does not change: the model (and its shards) lands in
//! `models\`, the mmproj in `mmprojs\`, an MTP head in `mtps\`, a DFlash or
//! DSpark drafter in `dflashs\` (`model_scan::Category::subdir`), each under
//! `<org>\<repo>` (`models\unsloth\Qwen3-8B-GGUF\Qwen3-8B-Q4_K_M.gguf`), with
//! its Hugging Face file name. The org is part of it on purpose: `Qwen/` and
//! `unsloth/` both publish a `Qwen3-8B-GGUF` with a `Qwen3-8B-Q4_K_M.gguf`, and
//! with the repo name alone one org's file (at an equal size) would be taken as
//! the other's, already downloaded. The per-repo directory is also what keeps
//! two repos' `mmproj-F16.gguf` (Unsloth names every projector that) apart, and
//! what makes a picker label recognizable; the scans are recursive, so the
//! pickers see the files unchanged.
//!
//! llama.cpp's downloader needs its own cache layout to download and resume
//! (`models--<org>--<repo>\blobs\<oid>` + `.downloadInProgress`, a
//! `snapshots\<commit>\<path>` entry made by `hf_cache::finalize_file`), so the
//! child's `LLAMA_CACHE` is a STAGING directory, `<ModelsDir>\.hf-staging`
//! (`STAGING_DIR`): inside ModelsDir so the final move is a rename on the same
//! volume, not a multi-GB copy, and dot-prefixed outside the scanned
//! subfolders, so a half-finished download is invisible to the pickers. Once a
//! group's child reports success, each file's REAL data is moved: the snapshot
//! entry is a symlink into `blobs\` when the OS allowed one (Linux; Windows with
//! developer mode), and moving the link would leave a dangling pointer once the
//! staging is removed, so a link is resolved to its blob and the blob is moved;
//! without symlinks `finalize_file` already moved the blob into the snapshot,
//! and that file is moved. A rename that fails (ModelsDir's subfolder being a
//! junction onto another volume) falls back to a copy into `<target>.partial`,
//! renamed into place once complete and removed on failure, so no truncated
//! file is ever left at a target (a retry would refuse it as a conflict). When every file
//! is in place the repo's staging tree is removed, and the staging directory
//! too once empty. A failed or cancelled download KEEPS its staging tree: the
//! `.downloadInProgress` partial is exactly what llama.cpp resumes from with a
//! range request on the next attempt (for a cancel, only because it stops the
//! child by EOF, see above; a transfer that failed on its own after its
//! retries also keeps its partial, since upstream deletes it only when
//! cancelled). `has_partial` answers "is there something to resume" for ONE
//! job: a `.downloadInProgress` blob of one of its own files (by oid), not the
//! mere presence of the repo folder, which the child creates (`refs\main`) the
//! moment it starts. A partial of a DIFFERENT quant of the same repo does not
//! count: the button would promise a resume of bytes this job never reads.
//!
//! Path length (Windows): the downloader opens its partial with a plain
//! `std::ofstream`, which is held to MAX_PATH (260), and the staging path is
//! long by construction (`.hf-staging\models--<org>--<repo>\blobs\` + a
//! 64-hex oid + `.downloadInProgress`, ~150 characters before ModelsDir). A
//! ModelsDir deeper than about 100 characters fails there with "error opening
//! local file for writing", reported as such (`classify_failure`), not as the
//! network error its `status code: -1` resembles.
//!
//! A file already at its target is never overwritten: with the size the HF
//! listing reports it is taken as already downloaded (no transfer, the file is
//! used as is: a repeat of the same download, or the same projector shared by
//! two quants of one repo), with any other size the download is refused before
//! anything starts (`HfError::TargetConflict`), naming the file.
//!
//! ## Secrets
//!
//! A token (`Token`, from `HF_TOKEN` or the Hugging Face CLI's token file, see
//! `token_from_env`) reaches curl through its stdin config and the child
//! through `HF_TOKEN` in its environment, never an argv. Its `Debug` prints
//! `***`, it has no `Display`, and every stderr line kept for an error message
//! passes through `redact` first. Nothing here writes it to a preset, a log or
//! bench.db.

use std::collections::VecDeque;
use std::fmt;
use std::io::{BufRead, BufReader, Write};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{mpsc, Arc, Mutex};
use std::time::{Duration, Instant};

use serde_json::Value;

use crate::gguf;
use crate::presets::{self, Preset};

/// `CMD_CHILD_TO_ROUTER_STATE` in llama.cpp's `tools/server/server-models.cpp`:
/// the prefix of every protocol line a router child prints. Re-check on a bump.
pub const CHILD_STATE_PREFIX: &str = "cmd_child_to_router:state:";
/// `CMD_ROUTER_TO_CHILD_EXIT` in the same file: the line that ends a child,
/// and cancels a download in flight. Re-check on a bump.
pub const CHILD_EXIT_CMD: &str = "cmd_router_to_child:exit";
/// The staging cache under ModelsDir (see the module header).
pub const STAGING_DIR: &str = ".hf-staging";

/// The file-name markers llama.cpp's `gguf_filename_is_model` excludes from
/// the main-model candidates (`common/download.cpp`).
const NOT_A_MODEL: [&str; 6] = ["mmproj", "imatrix", "mtp-", "eagle3-", "dflash-", "dspark-"];

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
    NotFoundOrPrivate { repo: String },
    /// A 401 WITH a token: it is invalid or expired.
    TokenRejected { repo: String },
    /// A 403: a gated repo whose conditions this account has not accepted.
    Gated { repo: String, had_token: bool },
    Network(String),
    Http { status: u16, message: String },
    /// The repo holds no main-model GGUF.
    NoGguf { repo: String },
    /// No GGUF matches the requested quant tag or file.
    NoMatch { repo: String, wanted: String },
    /// A preset repo (`preset.ini` at its root): llama.cpp would fetch only that
    /// file and run it as a router preset, which is not a model.
    PresetRepo { repo: String },
    /// A sidecar was requested that the chosen candidate does not have.
    SidecarMissing { what: &'static str },
    /// A different file already sits where a download would land.
    TargetConflict { path: PathBuf, found: u64, expected: u64 },
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
            HfError::NotFoundOrPrivate { repo } => write!(
                f,
                "repository {repo} not found, or private: check the name, or set HF_TOKEN if it is yours"
            ),
            HfError::TokenRejected { repo } => write!(
                f,
                "the Hugging Face token was rejected for {repo}: it is invalid, expired, or has no read access"
            ),
            HfError::Gated { repo, had_token } => {
                if *had_token {
                    write!(f, "{repo} is gated: accept its conditions on the model page with the account the token belongs to, then retry")
                } else {
                    write!(f, "{repo} is gated: accept its conditions on the model page, then set HF_TOKEN and retry")
                }
            }
            HfError::Network(m) => write!(f, "network error: {m}"),
            HfError::Http { status, message } => write!(f, "Hugging Face answered HTTP {status}: {message}"),
            HfError::NoGguf { repo } => write!(f, "{repo} holds no GGUF model file"),
            HfError::NoMatch { repo, wanted } => write!(f, "{repo} has no GGUF matching {wanted}"),
            HfError::PresetRepo { repo } => write!(
                f,
                "{repo} is a preset repository (preset.ini), not a model: it cannot become one preset"
            ),
            HfError::SidecarMissing { what } => write!(f, "this quantization has no {what} file"),
            HfError::TargetConflict { path, found, expected } => write!(
                f,
                "{} already exists with a different size ({} bytes, the repository's is {} bytes): move or rename it first",
                path.display(),
                found,
                expected
            ),
            HfError::DiskFull(m) => write!(f, "the disk is probably full: {m}"),
            HfError::Protocol(m) => write!(f, "llama-server download protocol error: {m}"),
            HfError::Download(m) => write!(f, "download failed: {m}"),
            HfError::Io(m) => write!(f, "{m}"),
            HfError::Cancelled => write!(f, "download cancelled"),
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
        return Err(HfError::InvalidSpec("enter a repository: org/repo, org/repo:QUANT or its URL".into()));
    }
    let lower = s.to_ascii_lowercase();
    let no_scheme = ["https://", "http://"]
        .iter()
        .find(|p| lower.starts_with(*p))
        .map_or(s, |p| &s[p.len()..]);
    let host_end = no_scheme.find('/').unwrap_or(no_scheme.len());
    let host = no_scheme[..host_end].to_ascii_lowercase();
    let is_url = matches!(host.as_str(), "huggingface.co" | "www.huggingface.co" | "hf.co");
    if !is_url && no_scheme.len() != s.len() {
        return Err(HfError::InvalidSpec(format!("not a Hugging Face URL: {s}")));
    }

    let (repo_part, file) = if is_url {
        let path = &no_scheme[host_end..];
        let path = path.split(['?', '#']).next().unwrap_or("");
        let segs: Vec<&str> = path.split('/').filter(|x| !x.is_empty()).collect();
        if matches!(segs.first(), Some(&("datasets" | "spaces"))) {
            return Err(HfError::InvalidSpec("that URL is a dataset or a Space, not a model repository".into()));
        }
        if segs.len() < 2 {
            return Err(HfError::InvalidSpec(format!("no org/repo in the URL: {s}")));
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
        return Err(HfError::InvalidSpec(format!(
            "not a repository id: {repo} (expected org/repo)"
        )));
    }
    if let Some(t) = &tag {
        if t.is_empty() || !t.chars().all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-') {
            return Err(HfError::InvalidSpec(format!("not a quant tag: {t}")));
        }
    }
    Ok(RepoSpec { repo, tag, file })
}

/// Mirror of llama.cpp's `is_valid_repo_id` (`common/hf-cache.cpp`).
fn valid_repo_id(id: &str) -> bool {
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
/// that prints `***`, readable only inside this module.
#[derive(Clone, PartialEq, Eq)]
pub struct Token(String);

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
        if t.is_empty() || !t.chars().all(|c| c.is_ascii_graphic() && c != '"' && c != '\\') {
            return Err(HfError::InvalidSpec("the token contains characters a Hugging Face token never has".into()));
        }
        Ok(Token(t.to_string()))
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
fn token_from(env: impl Fn(&str) -> Option<String>, read: impl Fn(&Path) -> Option<String>) -> Option<Token> {
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
        PathBuf::from(home).join(".cache").join("huggingface").join("token")
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

fn endpoint_from(model: Option<String>, hf: Option<String>) -> String {
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
fn redact(text: &str, token: Option<&Token>) -> String {
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
}

/// The real `Fetch`: the system's `curl` (see the module header).
pub struct Curl;

impl Fetch for Curl {
    fn get(&self, url: &str, token: Option<&Token>) -> Result<(u16, String), HfError> {
        let exe = curl_exe();
        let mut cmd = Command::new(&exe);
        cmd.args(["-sS", "-L", "--connect-timeout", "20", "--max-time", "120", "-K", "-"])
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());
        crate::proc::hide_console(&mut cmd);
        let mut child = cmd.spawn().map_err(|e| {
            HfError::MissingTool(format!(
                "curl could not be started ({e}): it is needed to reach Hugging Face; install it or put it on PATH"
            ))
        })?;
        if let Some(mut stdin) = child.stdin.take() {
            // A write error surfaces as curl's own "no URL" failure below.
            let _ = stdin.write_all(curl_config(url, token).as_bytes());
        }
        let out = child.wait_with_output().map_err(|e| HfError::Io(format!("curl: {e}")))?;
        if !out.status.success() {
            let code = out.status.code().unwrap_or(-1);
            let err = redact(String::from_utf8_lossy(&out.stderr).trim(), token);
            return Err(curl_failure(code, &err));
        }
        parse_curl_output(&String::from_utf8_lossy(&out.stdout))
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

/// The curl config read from stdin: url, headers and the status write-out.
/// Values are double-quoted, so only `"` and `\` would need escaping, and
/// neither can reach here (a repo id and a `Token` refuse both, a search query
/// is percent-encoded).
fn curl_config(url: &str, token: Option<&Token>) -> String {
    let mut c = format!(
        "url = \"{url}\"\nheader = \"Accept: application/json\"\nuser-agent = \"llama-cpp-config/{}\"\nwrite-out = \"\\n%{{http_code}}\"\n",
        env!("CARGO_PKG_VERSION")
    );
    if let Some(t) = token {
        c.push_str(&format!("header = \"Authorization: Bearer {}\"\n", t.0));
    }
    c
}

/// Split curl's stdout into the body and the status the write-out appended.
fn parse_curl_output(stdout: &str) -> Result<(u16, String), HfError> {
    let (body, code) = stdout.rsplit_once('\n').unwrap_or(("", stdout));
    let status = code
        .trim()
        .parse::<u16>()
        .map_err(|_| HfError::Network(format!("curl returned no HTTP status: {}", code.trim())))?;
    if status == 0 {
        return Err(HfError::Network("no answer from the server".into()));
    }
    Ok((status, body.to_string()))
}

/// curl's exit codes worth a sentence of their own (`man curl`, EXIT CODES).
fn curl_failure(code: i32, stderr: &str) -> HfError {
    let what = match code {
        5 | 6 => "could not resolve the host (offline, or DNS)",
        7 => "could not connect",
        28 => "the request timed out",
        35 | 51 | 58 | 59 | 60 | 77 => "the TLS handshake failed (proxy or certificate store)",
        _ => "",
    };
    if what.is_empty() {
        HfError::Network(format!("curl failed (exit {code}): {stderr}"))
    } else {
        HfError::Network(format!("{what}: {stderr}"))
    }
}

/// An API status other than 200, as an error. `body` is the API's JSON
/// (`{"error": "..."}`) when it has one.
fn api_error(status: u16, body: &str, had_token: bool, repo: &str) -> HfError {
    let repo = repo.to_string();
    match status {
        401 if had_token => HfError::TokenRejected { repo },
        401 | 404 => HfError::NotFoundOrPrivate { repo },
        403 => HfError::Gated { repo, had_token },
        429 => HfError::Network("rate limited by Hugging Face: retry in a minute".into()),
        _ => HfError::Http { status, message: api_message(body) },
    }
}

/// The API's `{"error": "..."}`, else the body's first 200 characters.
fn api_message(body: &str) -> String {
    serde_json::from_str::<Value>(body)
        .ok()
        .and_then(|v| v.get("error").and_then(Value::as_str).map(str::to_string))
        .unwrap_or_else(|| body.chars().take(200).collect())
}

fn api_json(fetch: &dyn Fetch, url: &str, token: Option<&Token>, repo: &str) -> Result<Value, HfError> {
    let (status, body) = fetch.get(url, token)?;
    if status != 200 {
        return Err(api_error(status, &body, token.is_some(), repo));
    }
    serde_json::from_str(&body).map_err(|e| HfError::Network(format!("unreadable answer from {url}: {e}")))
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
    fn keyword(self) -> &'static str {
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

    /// The ModelsDir subfolder its scan reads (`model_scan::Category`): DSpark
    /// shares `dflashs\` with DFlash, as the Assets picker expects.
    fn subdir(self) -> &'static str {
        match self {
            Sidecar::Mtp => crate::model_scan::Category::Mtp.subdir(),
            Sidecar::Dflash | Sidecar::Dspark => crate::model_scan::Category::Dflash.subdir(),
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
    /// The model page, what a derived preset's `project_url` holds.
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
fn list_repo_with(fetch: &dyn Fetch, endpoint: &str, spec: &RepoSpec, token: Option<&Token>) -> Result<Listing, HfError> {
    let repo = &spec.repo;
    let refs = api_json(fetch, &format!("{endpoint}api/models/{repo}/refs"), token, repo)?;
    let commit = commit_from_refs(&refs)
        .ok_or_else(|| HfError::Http { status: 200, message: format!("{repo} lists no branch") })?;
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
fn commit_from_refs(refs: &Value) -> Option<String> {
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
/// and oid under `lfs`.
fn files_from_tree(tree: &Value) -> Vec<RepoFile> {
    let Some(items) = tree.as_array() else {
        return Vec::new();
    };
    items
        .iter()
        .filter(|i| i.get("type").and_then(Value::as_str) == Some("file"))
        .filter_map(|i| {
            let path = i.get("path")?.as_str()?.to_string();
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

/// Group a file list into candidates (pure; see `Listing`).
fn build_listing(repo: &str, commit: &str, files: Vec<RepoFile>, spec: &RepoSpec, endpoint: &str) -> Result<Listing, HfError> {
    if files.iter().any(|f| f.path == "preset.ini") {
        return Err(HfError::PresetRepo { repo: repo.into() });
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
        index_of(file).ok_or_else(|| HfError::NoMatch { repo: repo.into(), wanted: file.clone() })?
    } else {
        let tag = spec.tag.as_deref().unwrap_or("");
        let best = find_best_model(&files, tag);
        index_of(&best.path).ok_or_else(|| HfError::NoMatch {
            repo: repo.into(),
            wanted: format!("the tag {tag}"),
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

// ── The search ───────────────────────────────────────────────────────────

/// How many repositories one search returns (the API's `limit`).
pub const SEARCH_LIMIT: usize = 30;

/// The longest query sent: a model name is a few words, and the HF search
/// is a substring match, so anything longer finds nothing anyway.
const QUERY_MAX: usize = 200;

/// What the dialog's field (or the CLI's positional) holds once classified
/// (`classify_input`; the rule is in the module header).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Input {
    /// A repository: straight to `list_repo`.
    Spec(RepoSpec),
    /// Free text for `search`, trimmed, inner whitespace collapsed.
    Query(String),
}

/// Classify what the user typed. `Ok(None)` for an empty field; `Err` for a
/// mistyped spec (slash-carrying, no space, not a repository) or a query
/// `parse_query` refuses.
pub fn classify_input(text: &str) -> Result<Option<Input>, HfError> {
    let t = text.trim();
    if t.is_empty() {
        return Ok(None);
    }
    match parse_spec(t) {
        Ok(spec) => Ok(Some(Input::Spec(spec))),
        Err(e) if t.contains('/') && !t.contains(char::is_whitespace) => Err(e),
        Err(_) => parse_query(t).map(|q| Some(Input::Query(q))),
    }
}

/// A search query as sent: trimmed, runs of whitespace folded to one space,
/// at most `QUERY_MAX` characters, no control characters.
pub fn parse_query(text: &str) -> Result<String, HfError> {
    let q = text.split_whitespace().collect::<Vec<_>>().join(" ");
    if q.is_empty() {
        return Err(HfError::InvalidSpec("enter a model name to search for, or a repository: org/repo".into()));
    }
    if q.chars().any(char::is_control) {
        return Err(HfError::InvalidSpec("the search contains control characters".into()));
    }
    if q.chars().count() > QUERY_MAX {
        return Err(HfError::InvalidSpec(format!("the search is longer than {QUERY_MAX} characters")));
    }
    Ok(q)
}

/// One repository a search found, with what its file names promise.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SearchHit {
    /// `org/repo`.
    pub repo: String,
    /// Downloads over the last 30 days (what the model page shows).
    pub downloads: u64,
    /// Gated: the conditions must be accepted on the model page, and the
    /// download needs a token (`"auto"` or `"manual"` in the API).
    pub gated: bool,
    pub private: bool,
    /// The quant tags of its main-model GGUFs (`quants_of`).
    pub quants: Vec<String>,
    /// It holds an mmproj file.
    pub mmproj: bool,
    /// The drafter kinds its file names show (a separate MTP / DFlash / DSpark
    /// file; MTP heads embedded in the model are not visible from a name).
    pub drafters: Vec<Sidecar>,
}

/// Search the GGUF repositories (blocking; run it off the UI thread).
pub fn search(query: &str, limit: usize, token: Option<&Token>) -> Result<Vec<SearchHit>, HfError> {
    search_with(&Curl, &endpoint(), query, limit, token)
}

fn search_with(
    fetch: &dyn Fetch,
    endpoint: &str,
    query: &str,
    limit: usize,
    token: Option<&Token>,
) -> Result<Vec<SearchHit>, HfError> {
    let query = parse_query(query)?;
    let url = search_url(endpoint, &query, limit);
    let (status, body) = fetch.get(&url, token)?;
    if status != 200 {
        return Err(search_error(status, &body, token.is_some()));
    }
    let v: Value = serde_json::from_str(&body)
        .map_err(|e| HfError::Network(format!("unreadable answer from the Hugging Face search: {e}")))?;
    hits_from_search(&v)
}

/// The search request (see the module header for every parameter).
fn search_url(endpoint: &str, query: &str, limit: usize) -> String {
    let mut url = format!(
        "{endpoint}api/models?search={}&filter=gguf&sort=downloads&direction=-1&limit={}",
        percent_encode(query),
        limit.max(1)
    );
    for field in ["downloads", "gated", "private", "siblings"] {
        url.push_str("&expand%5B%5D=");
        url.push_str(field);
    }
    url
}

/// RFC 3986 percent-encoding of everything but the unreserved characters
/// (so a space is `%20`, never `+`).
fn percent_encode(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for b in s.bytes() {
        if b.is_ascii_alphanumeric() || matches!(b, b'-' | b'.' | b'_' | b'~') {
            out.push(b as char);
        } else {
            out.push_str(&format!("%{b:02X}"));
        }
    }
    out
}

/// A search status other than 200. The search names no repository, so the
/// per-repo errors of `api_error` do not apply: a 401 with a token is the
/// token's fault, a 429 a rate limit, anything else the API's own message.
fn search_error(status: u16, body: &str, had_token: bool) -> HfError {
    match status {
        401 if had_token => HfError::TokenRejected { repo: "the search".into() },
        429 => HfError::Network("rate limited by Hugging Face: retry in a minute".into()),
        _ => HfError::Http { status, message: api_message(body) },
    }
}

/// The rows of a search answer, in its order (most downloaded first). An
/// answer that is not an array is a protocol change, reported as such; an
/// entry without an `id` is skipped.
fn hits_from_search(v: &Value) -> Result<Vec<SearchHit>, HfError> {
    let items = v.as_array().ok_or_else(|| HfError::Http {
        status: 200,
        message: "the search answer is not a list of repositories".into(),
    })?;
    Ok(items
        .iter()
        .filter_map(|m| {
            let repo = m.get("id")?.as_str()?.to_string();
            let files: Vec<&str> = m
                .get("siblings")
                .and_then(Value::as_array)
                .map(|s| s.iter().filter_map(|f| f.get("rfilename")?.as_str()).collect())
                .unwrap_or_default();
            let ggufs = || files.iter().filter(|f| f.ends_with(".gguf"));
            Some(SearchHit {
                downloads: m.get("downloads").and_then(Value::as_u64).unwrap_or(0),
                // `false`, or the gating mode as a string.
                gated: m.get("gated").is_some_and(|g| g.as_bool().unwrap_or(!g.is_null())),
                private: m.get("private").and_then(Value::as_bool).unwrap_or(false),
                quants: quants_of(&files),
                mmproj: ggufs().any(|f| f.contains("mmproj")),
                drafters: Sidecar::ALL
                    .into_iter()
                    .filter(|s| ggufs().any(|f| f.contains(s.keyword())))
                    .collect(),
                repo,
            })
        })
        .collect())
}

/// The quant tags of a repo's main-model GGUFs, from its file paths: one per
/// model file (a split's first shard only), the tag llama.cpp reads off the
/// name (`split_info`), else the file name without `.gguf`; deduplicated and
/// ordered by bit count (tags without one last), then by name.
pub fn quants_of(paths: &[&str]) -> Vec<String> {
    // (sort key, tag): a file name standing in for a tag sorts last.
    let mut tags: Vec<(i64, String)> = Vec::new();
    for p in paths.iter().filter(|p| is_model_file(p)) {
        let s = split_info(p);
        if s.count > 1 && s.index != 1 {
            continue;
        }
        let entry = if s.tag.is_empty() {
            let name = p.rsplit('/').next().unwrap_or(p);
            (i64::MAX, name.strip_suffix(".gguf").unwrap_or(name).to_string())
        } else {
            let bits = bits_of_tag(&s.tag);
            (if bits == 0 { i64::MAX } else { bits }, s.tag)
        };
        if !tags.iter().any(|t| t.1 == entry.1) {
            tags.push(entry);
        }
    }
    tags.sort();
    tags.into_iter().map(|t| t.1).collect()
}

/// `644.6k` / `1.2M` / `37`: a download count for a search row.
pub fn format_count(n: u64) -> String {
    match n {
        0..=999 => n.to_string(),
        1_000..=999_949 => format!("{:.1}k", n as f64 / 1e3),
        _ => format!("{:.1}M", n as f64 / 1e6),
    }
}

// ── llama.cpp's file-matching rules (common/download.cpp), mirrored ────

/// `gguf_split_info`: the path without `.gguf` and without a
/// `-NNNNN-of-NNNNN` suffix, its upper-cased quant tag, and the shard numbers.
/// A path not ending in `.gguf` (case-sensitive, as upstream) is all defaults.
#[derive(Debug, Default, PartialEq)]
struct SplitInfo {
    prefix: String,
    tag: String,
    index: u32,
    count: u32,
}

fn split_info(path: &str) -> SplitInfo {
    let Some(stem) = path.strip_suffix(".gguf") else {
        return SplitInfo::default();
    };
    let (prefix, index, count) = match crate::model_scan::split_shard_suffix(stem) {
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
    SplitInfo { prefix: prefix.to_string(), tag, index, count }
}

/// `extract_quant_bits`: the number after the tag's first digit run starts
/// (`Q4_K_M` → 4, `BF16` → 16), 0 without one.
fn quant_bits(path: &str) -> i64 {
    bits_of_tag(&split_info(path).tag)
}

fn bits_of_tag(tag: &str) -> i64 {
    let Some(pos) = tag.find(|c: char| c.is_ascii_digit()) else {
        return 0;
    };
    let digits: String = tag[pos..].chars().take_while(char::is_ascii_digit).collect();
    digits.parse().unwrap_or(i64::MAX)
}

/// `gguf_filename_is_model`: a `.gguf` whose FILE name (not directory) carries
/// none of the sidecar / imatrix markers.
fn is_model_file(path: &str) -> bool {
    if !path.ends_with(".gguf") {
        return false;
    }
    let name = path.rsplit('/').next().unwrap_or(path);
    !NOT_A_MODEL.iter().any(|m| name.contains(m))
}

/// `get_split_files`: every shard of `file`'s split (same prefix and count).
fn split_files(files: &[RepoFile], file: &RepoFile) -> Vec<RepoFile> {
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

/// `find_best_model`: the first model file matching `<tag>[.-]`
/// (case-insensitive) that is not a later shard; with no tag, `Q4_K_M`, then
/// `Q8_0`, then the first model file. An empty `path` when nothing matches.
fn find_best_model(files: &[RepoFile], tag: &str) -> RepoFile {
    let tags: Vec<&str> = if tag.is_empty() { vec!["Q4_K_M", "Q8_0"] } else { vec![tag] };
    let first_shard = |f: &RepoFile| {
        let s = split_info(&f.path);
        !(s.count > 1 && s.index != 1)
    };
    for t in tags {
        let t = t.to_ascii_lowercase();
        let (dot, dash) = (format!("{t}."), format!("{t}-"));
        if let Some(f) = files.iter().find(|f| {
            let p = f.path.to_ascii_lowercase();
            is_model_file(&f.path) && (p.contains(&dot) || p.contains(&dash)) && first_shard(f)
        }) {
            return f.clone();
        }
    }
    if tag.is_empty() {
        if let Some(f) = files.iter().find(|f| is_model_file(&f.path) && first_shard(f)) {
            return f.clone();
        }
    }
    RepoFile { path: String::new(), size: 0, oid: String::new() }
}

/// `find_best_sibling`: among the `.gguf` files whose path contains `keyword`
/// and that sit in `model`'s directory or one of its ancestors, the deepest,
/// then an exact `-<TAG>.` match, then the closest quant bits (to `tag`'s when
/// given, else to the model's). Ties keep the earlier file. Empty `path` when
/// none qualifies.
fn find_best_sibling(files: &[RepoFile], model: &str, keyword: &str, tag: &str) -> RepoFile {
    let tag_upper = tag.to_ascii_uppercase();
    let model_bits = if tag_upper.is_empty() { quant_bits(model) } else { bits_of_tag(&tag_upper) };
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
        let exact = !tag_upper.is_empty() && f.path.to_ascii_uppercase().contains(&format!("-{tag_upper}."));
        let better = match best {
            None => true,
            Some((_, bd, bdiff, bexact)) => {
                depth > bd || (depth == bd && exact && !bexact) || (depth == bd && exact == bexact && diff < bdiff)
            }
        };
        if better {
            best = Some((f, depth, diff, exact));
        }
    }
    best.map(|b| b.0.clone())
        .unwrap_or(RepoFile { path: String::new(), size: 0, oid: String::new() })
}

// ── Download: the job, its placements, progress ──────────────────────────

/// What a placed file is for.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Role {
    Model,
    Mmproj,
    Draft(Sidecar),
}

/// One file of a job and the path it ends at.
#[derive(Debug, Clone, PartialEq)]
pub struct Placement {
    pub role: Role,
    pub file: RepoFile,
    pub target: PathBuf,
}

/// Everything one download needs. Build it with `DownloadJob::new`.
#[derive(Debug, Clone)]
pub struct DownloadJob {
    pub repo: String,
    /// The commit the listing saw; the staging snapshot is looked up under it
    /// when llama.cpp's own ref names no other.
    pub commit: String,
    pub project_url: String,
    pub candidate: Candidate,
    /// Fetch the candidate's mmproj (when it has one).
    pub mmproj: bool,
    pub sidecar: Option<Sidecar>,
    /// The ModelsDir the server reads (`ServerConfig::models_dir_or_default`).
    pub models_dir: PathBuf,
    pub token: Option<Token>,
    pub server_exe: PathBuf,
}

impl DownloadJob {
    /// A job for `listing.candidates[index]`. `mmproj` is a wish (ignored when
    /// the candidate has none, as llama.cpp ignores it); a `sidecar` the
    /// candidate lacks is an error, since the user picked it. The child is the
    /// llama-server `paths::llama_server_exe` finds.
    pub fn new(
        listing: &Listing,
        index: usize,
        mmproj: bool,
        sidecar: Option<Sidecar>,
        models_dir: PathBuf,
        token: Option<Token>,
    ) -> Result<DownloadJob, HfError> {
        let candidate = listing
            .candidates
            .get(index)
            .cloned()
            .ok_or_else(|| HfError::NoMatch { repo: listing.repo.clone(), wanted: format!("candidate {index}") })?;
        if let Some(s) = sidecar {
            if candidate.sidecar(s).is_none() {
                return Err(HfError::SidecarMissing { what: s.label() });
            }
        }
        let server_exe = crate::paths::llama_server_exe()
            .ok_or_else(|| HfError::MissingTool("llama-server was not found beside the configurator or in the build tree".into()))?;
        Ok(DownloadJob {
            repo: listing.repo.clone(),
            commit: listing.commit.clone(),
            project_url: listing.project_url.clone(),
            mmproj: mmproj && candidate.mmproj.is_some(),
            candidate,
            sidecar,
            models_dir,
            token,
            server_exe,
        })
    }

    /// Where each file of the job lands (pure): `<ModelsDir>\<subfolder>\<org>\
    /// <repo>\<file name>`, the subfolder by role. The org is kept so two orgs'
    /// same-named repos never share a folder (module header).
    pub fn placements(&self) -> Vec<Placement> {
        let (org, name) = self.repo.split_once('/').unwrap_or(("", &self.repo));
        let place = |role: Role, file: &RepoFile| {
            let sub = match role {
                Role::Model => crate::model_scan::Category::Model.subdir(),
                Role::Mmproj => crate::model_scan::Category::Mmproj.subdir(),
                Role::Draft(s) => s.subdir(),
            };
            Placement {
                role,
                target: self.models_dir.join(sub).join(org).join(name).join(file.name()),
                file: file.clone(),
            }
        };
        let mut out: Vec<Placement> = self.candidate.shards.iter().map(|f| place(Role::Model, f)).collect();
        if self.mmproj {
            if let Some(m) = &self.candidate.mmproj {
                out.push(place(Role::Mmproj, m));
            }
        }
        if let Some(s) = self.sidecar {
            if let Some(f) = self.candidate.sidecar(s) {
                out.push(place(Role::Draft(s), f));
            }
        }
        out
    }

    fn staging(&self) -> PathBuf {
        self.models_dir.join(STAGING_DIR)
    }
}

/// A download's position, for a progress bar and a status line.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Progress {
    /// Bytes on disk so far, files already in place included.
    pub done: u64,
    /// Bytes of the whole job.
    pub total: u64,
    /// The file being fetched (its HF path), empty before the first byte.
    pub file: String,
    /// Bytes of `file` on disk so far, and its size (both 0 while `file` is
    /// empty): the per-file half of the Models tab's progress readout.
    pub file_done: u64,
    pub file_total: u64,
    pub files_done: usize,
    pub files_total: usize,
}

/// The files of a finished download, at their final paths.
#[derive(Debug, Clone, PartialEq)]
pub struct Downloaded {
    pub repo: String,
    pub project_url: String,
    /// The model's first shard: what a preset's `model` names.
    pub model: PathBuf,
    pub mmproj: Option<PathBuf>,
    pub draft: Option<(Sidecar, PathBuf)>,
}

/// Per-file byte counts across the children of one job (pure).
struct Tracker {
    files: Vec<(String, u64, u64)>, // (repo path, size, bytes done)
    current: String,
}

impl Tracker {
    /// `present` = the files already at their target: counted as done.
    fn new(placements: &[Placement], present: &[bool]) -> Tracker {
        Tracker {
            files: placements
                .iter()
                .zip(present)
                .map(|(p, &here)| (p.file.path.clone(), p.file.size, if here { p.file.size } else { 0 }))
                .collect(),
            current: String::new(),
        }
    }

    /// Apply one child progress line. The url is llama.cpp's
    /// `<endpoint><repo>/resolve/<commit>/<path>`, so it is matched by its
    /// `/<path>` suffix, the longest path winning. Returns false for a url
    /// matching no file of the job (nothing changes).
    fn update(&mut self, url: &str, downloaded: u64, total: u64) -> bool {
        let Some(i) = self
            .files
            .iter()
            .enumerate()
            .filter(|(_, f)| url.ends_with(&format!("/{}", f.0)))
            .max_by_key(|(_, f)| f.0.len())
            .map(|(i, _)| i)
        else {
            return false;
        };
        let f = &mut self.files[i];
        if f.1 == 0 {
            f.1 = total;
        }
        f.2 = downloaded;
        self.current = f.0.clone();
        true
    }

    /// Mark one file complete (it is in place).
    fn complete(&mut self, path: &str) {
        if let Some(f) = self.files.iter_mut().find(|f| f.0 == path) {
            f.2 = f.1;
        }
    }

    fn progress(&self) -> Progress {
        let current = self.files.iter().find(|f| !self.current.is_empty() && f.0 == self.current);
        Progress {
            done: self.files.iter().map(|f| f.2.min(f.1)).sum(),
            total: self.files.iter().map(|f| f.1).sum(),
            file: self.current.clone(),
            file_done: current.map_or(0, |f| f.2.min(f.1)),
            file_total: current.map_or(0, |f| f.1),
            files_done: self.files.iter().filter(|f| f.1 > 0 && f.2 >= f.1).count(),
            files_total: self.files.len(),
        }
    }
}

// ── Download: the router child protocol ──────────────────────────────────

#[derive(Debug, PartialEq)]
enum ChildEvent {
    Progress { url: String, downloaded: u64, total: u64 },
    Finished,
    Failed,
}

/// One stdout line of the download child. `Ok(None)` for a line without the
/// protocol prefix (not ours to read, kept as noise); `Err` for a prefixed line
/// this build cannot read, which is a protocol change and must stop the job.
fn parse_child_line(line: &str) -> Result<Option<ChildEvent>, String> {
    let Some(json) = line.trim_end().strip_prefix(CHILD_STATE_PREFIX) else {
        return Ok(None);
    };
    let v: Value = serde_json::from_str(json).map_err(|e| format!("unreadable state line ({e}): {line}"))?;
    let state = v.get("state").and_then(Value::as_str).unwrap_or("");
    if state != "downloading" {
        return Err(format!("unexpected child state `{state}`: {line}"));
    }
    let payload = v.get("payload").cloned().unwrap_or(Value::Null);
    if let Some(r) = payload.get("result").and_then(Value::as_str) {
        return match r {
            "download_finished" => Ok(Some(ChildEvent::Finished)),
            "download_failed" => Ok(Some(ChildEvent::Failed)),
            other => Err(format!("unknown download result `{other}`")),
        };
    }
    match payload.get("url").and_then(Value::as_str) {
        Some(url) if !url.is_empty() => Ok(Some(ChildEvent::Progress {
            url: url.to_string(),
            downloaded: payload.get("downloaded").and_then(Value::as_u64).unwrap_or(0),
            total: payload.get("total").and_then(Value::as_u64).unwrap_or(0),
        })),
        _ => Err(format!("state line with neither a url nor a result: {line}")),
    }
}

/// The inherited `LLAMA_ARG_*` variables a child must NOT see, because each
/// changes what it plans to fetch (names from `set_env(...)` in llama.cpp's
/// `common/arg.cpp`; RE-CHECK them on every llama.cpp bump, a renamed one
/// silently stops being cleared):
/// - another model source: `LLAMA_ARG_MODEL`, `_MODEL_URL`, `_DOCKER_REPO`,
///   `_MODELS_PRESET`, `_MODELS_DIR`;
/// - the mmproj: `LLAMA_ARG_MMPROJ`, `_MMPROJ_URL`, `_MMPROJ_AUTO` (the last
///   could undo the `--no-mmproj` every child runs with);
/// - a drafter: `LLAMA_ARG_SPEC_TYPE` (a `draft-*` type makes the plan fetch the
///   matching sidecar, `common/arg.cpp` `common_models_handler_init`),
///   `LLAMA_ARG_SPEC_DRAFT_HF_REPO` (a whole draft repo) and
///   `LLAMA_ARG_SPEC_DRAFT_MODEL`;
/// - `LLAMA_ARG_OFFLINE`, which would turn every fetch into a cache lookup.
const CHILD_ENV_CLEARED: [&str; 12] = [
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
];

/// The child's environment, minus `CHILD_ENV_CLEARED` (an `LLAMA_ARG_MODEL`
/// from the user's shell would make it a local-model child). The token rides
/// `HF_TOKEN`, never an argv.
fn child_env(job: &DownloadJob, hf_file: &str) -> (Vec<(&'static str, String)>, [&'static str; 12]) {
    let mut set = vec![
        // Its presence alone makes the process a router child; no port is
        // opened in download mode, so the number is never used.
        ("LLAMA_SERVER_ROUTER_PORT", "0".to_string()),
        ("LLAMA_SERVER_CHILD_MODE", "download".to_string()),
        ("LLAMA_ARG_HF_REPO", job.repo.clone()),
        ("LLAMA_ARG_HF_FILE", hf_file.to_string()),
        ("LLAMA_CACHE", job.staging().to_string_lossy().into_owned()),
    ];
    if let Some(t) = &job.token {
        set.push(("HF_TOKEN", t.0.clone()));
    }
    (set, CHILD_ENV_CLEARED)
}

/// The child's arguments: `--no-mmproj` always, because every child fetches
/// exactly ONE named file group (`LLAMA_ARG_HF_FILE`); the mmproj and the
/// drafter are groups of their own, and no `--spec-type` is passed, so no
/// sidecar is resolved behind the job's back.
const CHILD_ARGS: [&str; 1] = ["--no-mmproj"];

/// Why a child failed, read off its stderr tail (llama.cpp's own log lines:
/// `GET failed (<status>)` from the API, `failed with status code: <n>` from a
/// transfer, `error writing to file` when a write fails).
fn classify_failure(lines: &[String], had_token: bool, repo: &str) -> HfError {
    let all = lines.join("\n").to_ascii_lowercase();
    let status = |n: &str| all.contains(&format!("({n})")) || all.contains(&format!("status code: {n}"));
    let repo = repo.to_string();
    if status("401") {
        return if had_token { HfError::TokenRejected { repo } } else { HfError::NotFoundOrPrivate { repo } };
    }
    if status("403") {
        return HfError::Gated { repo, had_token };
    }
    if status("404") {
        return HfError::NotFoundOrPrivate { repo };
    }
    let tail = tail_text(lines);
    if all.contains("error opening local file for writing") {
        return HfError::Io(format!(
            "llama-server could not create its download file: no write permission, or (Windows) the path passes 260 characters; a shorter ModelsDir avoids that ({tail})"
        ));
    }
    if ["error writing to file", "no space left", "not enough space", "disk full"]
        .iter()
        .any(|m| all.contains(m))
    {
        return HfError::DiskFull(tail);
    }
    if ["httplib failed", "download failed:", "could not establish connection", "status code: -1", "connection"]
        .iter()
        .any(|m| all.contains(m))
    {
        return HfError::Network(tail);
    }
    HfError::Download(tail)
}

/// The last lines of a child's stderr, for an error message.
fn tail_text(lines: &[String]) -> String {
    let start = lines.len().saturating_sub(8);
    let t = lines[start..].join(" | ");
    if t.is_empty() {
        "(no output from llama-server)".into()
    } else {
        t
    }
}

/// How a cancel stops a child in the middle of a transfer: by CLOSING its
/// stdin, never by writing `CHILD_EXIT_CMD`. The exit line flips the
/// downloader's `is_cancelled()`, and upstream then deletes the
/// `.downloadInProgress` partial; EOF makes the child's stdin monitor
/// `exit(1)` at once, which leaves the partial for a resume (module header).
fn cancel_stop<W: Write>(stdin: &mut Option<W>) {
    drop(stdin.take());
}

/// Feed `sink` every `\n`-terminated line of `r`, decoded LOSSILY: llama.cpp
/// on Windows logs OS messages in the ANSI code page, and a strict UTF-8
/// `lines()` would stop at the first such byte, leaving the pipe undrained
/// until the child blocks on it. Stops at EOF, on a read error, or when `sink`
/// returns false.
fn read_lines_lossy<R: std::io::Read>(r: R, mut sink: impl FnMut(String) -> bool) {
    let mut reader = BufReader::new(r);
    let mut buf = Vec::new();
    loop {
        buf.clear();
        match reader.read_until(b'\n', &mut buf) {
            Ok(0) | Err(_) => break,
            Ok(_) => {
                let line = String::from_utf8_lossy(&buf);
                if !sink(line.trim_end_matches(['\n', '\r']).to_string()) {
                    break;
                }
            }
        }
    }
}

/// Run one download child for the file group headed by `hf_file`, feeding
/// `tracker` and `on_progress`. `cancel` stops it by EOF (`cancel_stop`), a
/// kill following if it is still alive 10 s later. On a reported success,
/// returns the child's (redacted) stderr for `place_group`.
fn run_child(
    job: &DownloadJob,
    hf_file: &str,
    cancel: &AtomicBool,
    tracker: &mut Tracker,
    on_progress: &mut dyn FnMut(&Progress),
) -> Result<Vec<String>, HfError> {
    let (set, clear) = child_env(job, hf_file);
    let mut cmd = Command::new(&job.server_exe);
    cmd.args(CHILD_ARGS)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    for k in clear {
        cmd.env_remove(k);
    }
    for (k, v) in set {
        cmd.env(k, v);
    }
    crate::proc::hide_console(&mut cmd);
    crate::proc::prepend_rocm_path(&mut cmd);
    let mut child = cmd.spawn().map_err(|e| {
        HfError::MissingTool(format!("llama-server could not be started ({}): {e}", job.server_exe.display()))
    })?;
    let mut stdin = child.stdin.take();

    // stderr: the log, kept (redacted) for the error message.
    let log: Arc<Mutex<VecDeque<String>>> = Arc::new(Mutex::new(VecDeque::new()));
    let err_thread = child.stderr.take().map(|err| {
        let log = Arc::clone(&log);
        let token = job.token.clone();
        std::thread::spawn(move || {
            read_lines_lossy(err, |line| {
                let mut l = log.lock().expect("log lock");
                if l.len() == 200 {
                    l.pop_front();
                }
                l.push_back(redact(line.trim_end(), token.as_ref()));
                true
            });
        })
    });
    // stdout: the protocol, through a channel so the cancel flag is polled
    // while the pipe is quiet.
    let (tx, rx) = mpsc::channel::<String>();
    if let Some(out) = child.stdout.take() {
        std::thread::spawn(move || read_lines_lossy(out, |line| tx.send(line).is_ok()));
    }

    let mut result: Option<ChildEvent> = None;
    let mut protocol_error: Option<String> = None;
    loop {
        match rx.recv_timeout(Duration::from_millis(250)) {
            Ok(line) => match parse_child_line(&line) {
                Ok(Some(ChildEvent::Progress { url, downloaded, total })) => {
                    if tracker.update(&url, downloaded, total) {
                        on_progress(&tracker.progress());
                    }
                }
                Ok(Some(end)) => {
                    result = Some(end);
                    // The result is in: the exit line now ends a child that
                    // has nothing left to clean up.
                    if let Some(s) = stdin.as_mut() {
                        let _ = s.write_all(format!("{CHILD_EXIT_CMD}\n").as_bytes());
                        let _ = s.flush();
                    }
                    break;
                }
                Ok(None) => {}
                Err(e) => {
                    protocol_error = Some(e);
                    break;
                }
            },
            Err(mpsc::RecvTimeoutError::Timeout) => {}
            Err(mpsc::RecvTimeoutError::Disconnected) => break,
        }
        if cancel.load(Ordering::SeqCst) {
            // NOT the exit line: see `cancel_stop`.
            cancel_stop(&mut stdin);
            break;
        }
    }

    // Give the child a moment to exit on its own (the exit line or EOF was
    // sent), then stop it: closing stdin is its own hard-stop path, a kill the
    // last.
    let deadline = Instant::now() + Duration::from_secs(10);
    let exit = loop {
        match child.try_wait() {
            Ok(Some(st)) => break Some(st),
            Ok(None) if Instant::now() < deadline => std::thread::sleep(Duration::from_millis(100)),
            _ => {
                drop(stdin.take());
                let _ = child.kill();
                break child.wait().ok();
            }
        }
    };
    drop(stdin);
    if let Some(t) = err_thread {
        let _ = t.join();
    }
    let lines: Vec<String> = log.lock().expect("log lock").iter().cloned().collect();
    let cancelled = cancel.load(Ordering::SeqCst);

    if let Some(e) = protocol_error {
        return Err(HfError::Protocol(format!("{e}; llama-server said: {}", tail_text(&lines))));
    }
    match result {
        Some(ChildEvent::Finished) => Ok(lines),
        _ if cancelled => Err(HfError::Cancelled),
        Some(_) => Err(classify_failure(&lines, job.token.is_some(), &job.repo)),
        None => Err(HfError::Protocol(format!(
            "llama-server ended ({}) without reporting a result; it said: {}",
            exit.map_or_else(|| "killed".to_string(), |s| s.to_string()),
            tail_text(&lines)
        ))),
    }
}

// ── Download: staging → subfolders ───────────────────────────────────────

/// Whether `target` already holds the file: `Ok(true)` with the expected size
/// (or any size when the listing gave none), `Ok(false)` when absent, `Err`
/// for a different file in the way.
fn target_present(target: &Path, expected: u64) -> Result<bool, HfError> {
    match std::fs::metadata(target) {
        Ok(m) if m.is_file() && (expected == 0 || m.len() == expected) => Ok(true),
        Ok(m) => Err(HfError::TargetConflict { path: target.to_path_buf(), found: m.len(), expected }),
        Err(_) => Ok(false),
    }
}

/// `models--<org>--<repo>`, llama.cpp's cache folder for a repo
/// (`repo_to_folder_name`).
fn repo_folder(repo: &str) -> String {
    format!("models--{}", repo.replace('/', "--"))
}

/// Whether an earlier download into `models_dir` stopped part way through one
/// of `candidate`'s OWN files (its shards, mmproj or drafters) and left a
/// `.downloadInProgress` partial a new download of it resumes from: the Models
/// tab then labels its button "Resume download". Looked up where llama.cpp
/// writes it: `blobs\<oid>.downloadInProgress`, or beside the snapshot entry
/// for a file the listing gave no oid. The staging folder alone proves nothing
/// (the child writes `refs\main` before its first byte), and another quant's
/// partial is not this candidate's to resume (module header).
pub fn has_partial(models_dir: &Path, repo: &str, candidate: &Candidate) -> bool {
    let repo_dir = models_dir.join(STAGING_DIR).join(repo_folder(repo));
    if !repo_dir.is_dir() {
        return false;
    }
    let snapshots: Vec<PathBuf> = std::fs::read_dir(repo_dir.join("snapshots"))
        .map(|rd| rd.flatten().map(|e| e.path()).collect())
        .unwrap_or_default();
    let files = candidate
        .shards
        .iter()
        .chain(candidate.mmproj.iter())
        .chain(candidate.mtp.iter())
        .chain(candidate.dflash.iter())
        .chain(candidate.dspark.iter());
    files.into_iter().any(|f| {
        let in_blobs = !f.oid.is_empty()
            && repo_dir.join("blobs").join(format!("{}.downloadInProgress", f.oid)).is_file();
        in_blobs
            || snapshots.iter().any(|s| {
                let entry = f.path.split('/').fold(s.clone(), |p, seg| p.join(seg));
                let mut name = entry.into_os_string();
                name.push(".downloadInProgress");
                Path::new(&name).is_file()
            })
    })
}

/// The commit a cache's `refs\` names, `main` first (`get_cached_ref`).
fn cached_commit(repo_dir: &Path) -> Option<String> {
    let mut first = None;
    for e in std::fs::read_dir(repo_dir.join("refs")).ok()?.flatten() {
        let Ok(text) = std::fs::read_to_string(e.path()) else {
            continue;
        };
        let commit = text.lines().next().unwrap_or("").trim().to_string();
        if commit.len() != 40 || !commit.chars().all(|c| c.is_ascii_hexdigit()) {
            continue;
        }
        if e.file_name() == "main" {
            return Some(commit);
        }
        first.get_or_insert(commit);
    }
    first
}

/// The snapshot entry of `repo_path` in the staging repo: under the commit
/// llama.cpp's ref names, else the listing's, else any snapshot holding it.
fn find_staged(repo_dir: &Path, listing_commit: &str, repo_path: &str) -> Option<PathBuf> {
    let snaps = repo_dir.join("snapshots");
    let rel = |c: &str| repo_path.split('/').fold(snaps.join(c), |p, seg| p.join(seg));
    let mut commits: Vec<String> = cached_commit(repo_dir).into_iter().collect();
    commits.push(listing_commit.to_string());
    if let Ok(rd) = std::fs::read_dir(&snaps) {
        commits.extend(rd.flatten().map(|e| e.file_name().to_string_lossy().into_owned()));
    }
    commits
        .iter()
        .map(|c| rel(c))
        .find(|p| std::fs::symlink_metadata(p).is_ok())
}

/// Move a staged snapshot entry's DATA to `dst`: a symlink is resolved to its
/// blob and the blob moved (the link would dangle once staging is removed); a
/// plain file is moved as is. Rename first; across volumes, a copy to a
/// temporary name BESIDE `dst` (`<name>.partial`) renamed into place only once
/// complete, so a failed copy never leaves a truncated file at the target
/// (where the next attempt would refuse it as a `TargetConflict`). The source
/// is removed after the target exists.
fn move_real_data(src: &Path, dst: &Path) -> std::io::Result<()> {
    let data = if std::fs::symlink_metadata(src)?.file_type().is_symlink() {
        let link = std::fs::read_link(src)?;
        if link.is_absolute() {
            link
        } else {
            src.parent().unwrap_or(Path::new("")).join(link)
        }
    } else {
        src.to_path_buf()
    };
    if let Some(parent) = dst.parent() {
        std::fs::create_dir_all(parent)?;
    }
    if std::fs::rename(&data, dst).is_ok() {
        return Ok(());
    }
    copy_into_place(&data, dst)?;
    std::fs::remove_file(&data)
}

/// `src` copied to `dst` through `<dst>.partial`, which is removed on any
/// failure.
fn copy_into_place(src: &Path, dst: &Path) -> std::io::Result<()> {
    let mut tmp = dst.as_os_str().to_owned();
    tmp.push(".partial");
    let tmp = PathBuf::from(tmp);
    let r = std::fs::copy(src, &tmp).and_then(|_| std::fs::rename(&tmp, dst));
    if r.is_err() {
        let _ = std::fs::remove_file(&tmp);
    }
    r
}

/// Move one finished group out of staging. A file the child reported but that
/// is not there is the empty-plan trap (module header): the stderr is
/// classified, so a swallowed 401 still reads as one.
fn place_group(job: &DownloadJob, group: &[&Placement], child_log: &[String], tracker: &mut Tracker) -> Result<(), HfError> {
    let repo_dir = job.staging().join(repo_folder(&job.repo));
    for p in group {
        if target_present(&p.target, p.file.size)? {
            tracker.complete(&p.file.path);
            continue;
        }
        let Some(src) = find_staged(&repo_dir, &job.commit, &p.file.path) else {
            return Err(match classify_failure(child_log, job.token.is_some(), &job.repo) {
                HfError::Download(tail) => HfError::Download(format!(
                    "llama-server reported success but {} is not in {}; it said: {tail}",
                    p.file.path,
                    repo_dir.display()
                )),
                e => e,
            });
        };
        move_real_data(&src, &p.target)
            .map_err(|e| HfError::Io(format!("moving {} to {}: {e}", p.file.path, p.target.display())))?;
        let got = std::fs::metadata(&p.target).map(|m| m.len()).unwrap_or(0);
        if p.file.size > 0 && got != p.file.size {
            // A wrong-sized file left at the target would be refused as a
            // TargetConflict by every retry: it goes, and the retry fetches
            // it again.
            let _ = std::fs::remove_file(&p.target);
            return Err(HfError::Io(format!(
                "{} came out with {got} bytes, the repository lists {}: removed, download it again",
                p.file.path, p.file.size
            )));
        }
        tracker.complete(&p.file.path);
    }
    Ok(())
}

/// The file groups one child each: the model with its shards, the mmproj, the
/// drafter. Pure.
fn groups(placements: &[Placement]) -> Vec<Vec<&Placement>> {
    let mut out: Vec<Vec<&Placement>> = Vec::new();
    for p in placements {
        match out.iter_mut().find(|g| g[0].role == p.role) {
            Some(g) => g.push(p),
            None => out.push(vec![p]),
        }
    }
    out
}

/// Run a whole job on the calling thread (blocking for as long as the
/// transfer takes): refuse a conflicting target, skip the groups already in
/// place, run one router child per remaining group, move its files into the
/// subfolders, and remove the repo's staging tree at the end. `cancel` is
/// polled throughout; `on_progress` is called from this thread.
pub fn run_download(
    job: &DownloadJob,
    cancel: &AtomicBool,
    on_progress: &mut dyn FnMut(&Progress),
) -> Result<Downloaded, HfError> {
    let placements = job.placements();
    let present = placements
        .iter()
        .map(|p| target_present(&p.target, p.file.size))
        .collect::<Result<Vec<bool>, HfError>>()?;
    let mut tracker = Tracker::new(&placements, &present);
    on_progress(&tracker.progress());

    let mut fetched_any = false;
    for group in groups(&placements) {
        let missing = group.iter().any(|p| {
            let i = placements.iter().position(|x| x == *p).unwrap_or(0);
            !present[i]
        });
        if !missing {
            continue;
        }
        if cancel.load(Ordering::SeqCst) {
            return Err(HfError::Cancelled);
        }
        let log = run_child(job, &group[0].file.path, cancel, &mut tracker, on_progress)?;
        place_group(job, &group, &log, &mut tracker)?;
        fetched_any = true;
        on_progress(&tracker.progress());
    }
    if fetched_any {
        // Everything is in place: the staging tree is dead weight. Best
        // effort, a leftover only costs disk until the next download.
        let _ = std::fs::remove_dir_all(job.staging().join(repo_folder(&job.repo)));
        let _ = std::fs::remove_dir(job.staging());
    }

    let target = |role: Role| placements.iter().find(|p| p.role == role).map(|p| p.target.clone());
    Ok(Downloaded {
        repo: job.repo.clone(),
        project_url: job.project_url.clone(),
        model: target(Role::Model).unwrap_or_default(),
        mmproj: target(Role::Mmproj),
        draft: job.sidecar.and_then(|s| target(Role::Draft(s)).map(|t| (s, t))),
    })
}

/// Cancels the download `spawn_download` started. Cheap to clone; cancelling
/// twice, or after the end, is harmless.
#[derive(Debug, Clone)]
pub struct DownloadHandle {
    cancel: Arc<AtomicBool>,
}

impl DownloadHandle {
    /// Stop the download: the child's stdin is closed (EOF, never the exit
    /// line, which would make llama.cpp delete its partial: `cancel_stop`), a
    /// kill following 10 s later if it is still alive. The job then ends with
    /// `HfError::Cancelled`; the bytes fetched so far stay in the staging tree
    /// and the next download of the same files resumes from them.
    pub fn cancel(&self) {
        self.cancel.store(true, Ordering::SeqCst);
    }
}

/// `run_download` on a new thread. `on_progress` and `on_done` run on THAT
/// thread: a GUI hops back with `slint::invoke_from_event_loop` (and may want
/// to coalesce progress, which arrives up to ten times a second per file).
pub fn spawn_download<P, D>(job: DownloadJob, mut on_progress: P, on_done: D) -> DownloadHandle
where
    P: FnMut(Progress) + Send + 'static,
    D: FnOnce(Result<Downloaded, HfError>) + Send + 'static,
{
    let cancel = Arc::new(AtomicBool::new(false));
    let flag = Arc::clone(&cancel);
    std::thread::spawn(move || {
        let r = run_download(&job, &flag, &mut |p| on_progress(p.clone()));
        on_done(r);
    });
    DownloadHandle { cancel }
}

// ── The preset ───────────────────────────────────────────────────────────

/// The preset a download becomes (pure: the headers are the caller's to read,
/// `preset_from_download` does it). `model` is the model's header, `None` when
/// it could not be read (no ggml-base), in which case only the paths and the
/// URL are filled and every key the header would justify stays at its default.
/// `draft_header_spec` is `gguf::draft_spec_type` of the drafter.
///
/// What it sets, and why (everything else stays `Preset::default()`, the New
/// dialog's preset: a new preset pins only what has a reason, README "A new
/// preset leaves every tunable at default"):
///
/// | field | from | reason |
/// |---|---|---|
/// | `model` | the first shard's final path | what the preset loads |
/// | `project_url` | the listing's model page | the Models tab's Open button |
/// | `mmproj` | the mmproj's final path, when fetched | `llama-server -hf` would load it |
/// | `model_draft` + `spec_type` | the drafter's path; its HEADER's verdict, else the sidecar's prefix | the user asked for it; the header tells DSpark from DFlash, which a name cannot (`gguf::draft_spec_type`) |
/// | `temp`, `top_k`, `top_p`, `min_p`, `repeat_penalty` | `general.sampling.*` | llama-server already applies these from the file (`common_init_sampler_from_model`); written, they show in the form as the values in force instead of "default" |
/// | `jinja` | `off` for an embedded NON-Jinja template, else `on` | `--jinja` is upstream's default, and with it a legacy template string would be rendered as Jinja text; only the legacy route reads it |
/// | `fit` | `on` | llama.cpp's own default, and the one setting that makes an untuned preset load: context and GPU layers stay unset (`form::fit_allowed`), so without it the model's TRAINED context (often 128k-256k) and every layer would be allocated on whatever card this is |
///
/// And what the header could justify but is left alone, on purpose:
/// - `reasoning` stays `auto` whatever the thinking verdict (`gguf` `Thinking`):
///   `auto` is the template's own default for a switchable one, `off` is inert
///   on one that thinks unconditionally, and on a template with no marker it
///   would only cost the parser on a model that thinks without one (MiMo-VL).
/// - `ctx_size`, `n_gpu_layers`, `n_cpu_moe`, `n_cpu_ffn`, `moe_cache_mib`,
///   `tensor_split`: the fit sizes context and placement for the machine (any
///   of the last four would make it skip the placement, `common/fit.cpp`); the
///   MoE shape and the trained context are hardware questions the Models tab's
///   tuning answers, not facts of the file.
/// - Embedded MTP heads (`nextn_predict_layers`) do not switch `spec-type` on:
///   llama.cpp's default is no speculation, the MTP context costs VRAM, and
///   the user can pick it in one click.
/// - `image_min_tokens`: llama.cpp only WARNS (Qwen-VL grounding) and the bound
///   costs tokens on every image; a per-use choice.
/// - A SEPARATE MTP head gets no `device_draft` / `n_gpu_layers_draft` pin,
///   although a multi-device auto fit crashes those heads (AGENTS.md, "Draft
///   placement needs a draft FILE"): the pin names a device, which this pure
///   function cannot know, on one GPU there is nothing to split, and the CPU
///   fallback (`0`) would throw away the speedup the user asked for. A known
///   gap on a multi-GPU machine: the user pins it in the Assets drafter
///   picker (README, "Assets"), and nothing here tells them to.
///
/// The result passes `prune_inactive_draft_keys` / `prune_inactive_moe_cache`
/// unchanged (they run here too, against the header, so a future field cannot
/// slip past them).
pub fn derive_preset(
    id: String,
    d: &Downloaded,
    model: Option<&gguf::ModelInfo>,
    draft_header_spec: Option<&str>,
) -> Preset {
    let s = |p: &Path| p.to_string_lossy().into_owned();
    let mut p = Preset::new_default(id, s(&d.model));
    p.project_url = d.project_url.clone();
    p.mmproj = d.mmproj.as_deref().map(s).unwrap_or_default();
    if let Some((kind, path)) = &d.draft {
        p.model_draft = s(path);
        p.spec_type = draft_header_spec.unwrap_or(kind.spec_type()).to_string();
    }
    p.fit = crate::form::fit_allowed(p.ctx_size.is_none(), p.n_gpu_layers.is_none());
    if let Some(m) = model {
        let sm = &m.sampling;
        p.temp = sm.temp;
        p.top_k = sm.top_k;
        p.top_p = sm.top_p;
        p.min_p = sm.min_p;
        p.repeat_penalty = sm.penalty_repeat;
        if m.chat_template_is_jinja() == Some(false) {
            p.jinja = Some(false);
        }
        presets::prune_inactive_draft_keys(&mut p, m.nextn_predict_layers > 0);
        presets::prune_inactive_moe_cache(&mut p, m.is_moe);
    }
    p
}

/// `derive_preset` over the downloaded files' own headers, with the id the
/// Models tab's New would give the file (`presets::new_preset_id`: the file
/// name, `preset` when it sanitizes to nothing, made unique ignoring case
/// against `existing_ids`; pass every listed preset, the parked ones included).
pub fn preset_from_download(d: &Downloaded, existing_ids: &[String]) -> Preset {
    let id = presets::new_preset_id(&d.model.to_string_lossy(), existing_ids);
    let info = gguf::read_model_info(&d.model);
    let spec = d.draft.as_ref().and_then(|(_, p)| gguf::draft_spec_type(p));
    derive_preset(id, d, info.as_ref(), spec)
}

/// Save a derived preset, first pinning server.ini's ModelsDir to the one the
/// files were placed under when it is unset. `presets::save` would otherwise
/// seed it from the model path (`infer_models_dir`), and the per-repo folder
/// (`models\<org>\<repo>\`) makes that guess a folder below the real root, which
/// the pickers would then scan for a `models` subfolder that is not there. The
/// preset is validated FIRST, so a preset that cannot be saved changes nothing
/// in server.ini either. Touches `paths::`, so it has no unit test (README,
/// Tests); `derive_preset` is the tested part.
pub fn save_preset(p: &Preset, models_dir: &Path) -> std::io::Result<()> {
    presets::validate_for_save(p)?;
    if crate::server_cfg::load().models_dir.is_none() {
        crate::ini::replace_key(
            &crate::paths::server_ini(),
            "Server",
            "ModelsDir",
            &models_dir.to_string_lossy(),
        )?;
    }
    presets::save(p)
}

/// `4.68 GiB` / `812.0 MiB`: a size for a candidate list or a progress line.
pub fn format_size(bytes: u64) -> String {
    const MIB: f64 = 1024.0 * 1024.0;
    let b = bytes as f64;
    if b >= 1024.0 * MIB {
        format!("{:.2} GiB", b / (1024.0 * MIB))
    } else {
        format!("{:.1} MiB", b / MIB)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    const COMMIT: &str = "0123456789abcdef0123456789abcdef01234567";

    fn f(path: &str, size: u64) -> RepoFile {
        RepoFile { path: path.into(), size, oid: format!("oid-{path}") }
    }

    fn spec(repo: &str, tag: Option<&str>) -> RepoSpec {
        RepoSpec { repo: repo.into(), tag: tag.map(str::to_string), file: None }
    }

    // ── spec ──

    #[test]
    fn spec_parses_the_three_forms() {
        assert_eq!(parse_spec(" unsloth/Qwen3-8B-GGUF ").unwrap(), spec("unsloth/Qwen3-8B-GGUF", None));
        assert_eq!(parse_spec("org/repo:q4_k_m").unwrap(), spec("org/repo", Some("q4_k_m")));
        assert_eq!(parse_spec("https://huggingface.co/org/repo").unwrap(), spec("org/repo", None));
        assert_eq!(parse_spec("huggingface.co/org/repo/tree/main?x=1#y").unwrap(), spec("org/repo", None));
        assert_eq!(parse_spec("HTTPS://hf.co/org/repo:Q8_0").unwrap(), spec("org/repo", Some("Q8_0")));
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
        assert_eq!(redact("Bearer hf_abcdefghijklmnopqrstuvwxyz0123456789 bad", Some(&t)), "Bearer *** bad");
        assert_eq!(redact("plain", None), "plain");
    }

    #[test]
    fn token_comes_from_env_then_the_cli_token_file() {
        let read = |p: &Path| (p == Path::new("/h/.cache/huggingface/token")).then(|| "hf_file\n".to_string());
        let home = if cfg!(windows) { "USERPROFILE" } else { "HOME" };
        let env = |vars: Vec<(&'static str, &'static str)>| {
            let m: HashMap<&str, &str> = vars.into_iter().collect();
            move |k: &str| m.get(k).map(|v| v.to_string())
        };
        assert_eq!(token_from(env(vec![("HF_TOKEN", "hf_env"), (home, "/h")]), read), Some(Token("hf_env".into())));
        assert_eq!(token_from(env(vec![(home, "/h")]), read), Some(Token("hf_file".into())));
        // A padded HF_TOKEN is trimmed; a blank one falls through to the file.
        assert_eq!(token_from(env(vec![("HF_TOKEN", " hf_env \n"), (home, "/h")]), read), Some(Token("hf_env".into())));
        assert_eq!(token_from(env(vec![("HF_TOKEN", "  "), (home, "/h")]), read), Some(Token("hf_file".into())));
        // HF_HOME moves the file; a missing file is no token, not an error.
        assert_eq!(token_from(env(vec![("HF_HOME", "/elsewhere"), (home, "/h")]), read), None);
        assert_eq!(token_from(env(vec![]), read), None);
    }

    #[test]
    fn endpoint_follows_llama_cpp_precedence() {
        assert_eq!(endpoint_from(None, None), "https://huggingface.co/");
        assert_eq!(endpoint_from(None, Some("https://mirror.test".into())), "https://mirror.test/");
        assert_eq!(
            endpoint_from(Some("https://a.test/".into()), Some("https://b.test".into())),
            "https://a.test/"
        );
        assert_eq!(endpoint_from(Some(String::new()), None), "https://huggingface.co/");
    }

    // ── curl plumbing ──

    #[test]
    fn curl_config_carries_url_and_token_and_the_status_writeout() {
        let t = Token::new("hf_secret").unwrap();
        let c = curl_config("https://h/api/models/o/r/refs", Some(&t));
        assert!(c.contains("url = \"https://h/api/models/o/r/refs\"\n"));
        assert!(c.contains("header = \"Authorization: Bearer hf_secret\"\n"));
        assert!(c.contains("write-out = \"\\n%{http_code}\"\n"));
        assert!(!curl_config("u", None).contains("Authorization"));
    }

    #[test]
    fn curl_output_splits_body_and_status() {
        assert_eq!(parse_curl_output("{\"a\":1}\n200").unwrap(), (200, "{\"a\":1}".into()));
        assert_eq!(parse_curl_output("line1\nline2\n404").unwrap(), (404, "line1\nline2".into()));
        assert!(parse_curl_output("garbage").is_err());
        assert!(matches!(parse_curl_output("\n000"), Err(HfError::Network(_))));
    }

    #[test]
    fn curl_exit_codes_read_as_sentences() {
        assert!(curl_failure(6, "x").to_string().contains("resolve"));
        assert!(curl_failure(60, "x").to_string().contains("TLS"));
        assert!(curl_failure(99, "boom").to_string().contains("exit 99"));
    }

    #[test]
    fn api_statuses_map_to_actionable_errors() {
        assert_eq!(api_error(404, "", false, "o/r"), HfError::NotFoundOrPrivate { repo: "o/r".into() });
        assert_eq!(api_error(401, "", false, "o/r"), HfError::NotFoundOrPrivate { repo: "o/r".into() });
        assert_eq!(api_error(401, "", true, "o/r"), HfError::TokenRejected { repo: "o/r".into() });
        assert_eq!(api_error(403, "", true, "o/r"), HfError::Gated { repo: "o/r".into(), had_token: true });
        assert_eq!(
            api_error(500, "{\"error\":\"oops\"}", false, "o/r"),
            HfError::Http { status: 500, message: "oops".into() }
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
        for side in ["mmproj-F16.gguf", "mtp-Q8_0.gguf", "x-dflash-Q4.gguf", "dspark-q.gguf", "imatrix.gguf", "eagle3-a.gguf"] {
            assert!(!is_model_file(side), "{side}");
        }
        assert!(!is_model_file("README.md"));
    }

    #[test]
    fn best_model_prefers_q4_k_m_then_q8_0_then_the_first() {
        let files = vec![f("m-Q8_0.gguf", 8), f("m-Q4_K_M.gguf", 4), f("m-Q2_K.gguf", 2)];
        assert_eq!(find_best_model(&files, "").path, "m-Q4_K_M.gguf");
        assert_eq!(find_best_model(&files, "q2_k").path, "m-Q2_K.gguf");
        assert_eq!(find_best_model(&files, "Q5_K_M").path, "");
        let only = vec![f("m-IQ3_XS.gguf", 3)];
        assert_eq!(find_best_model(&only, "").path, "m-IQ3_XS.gguf");
        // A later shard never wins, even when it comes first.
        let split = vec![f("m-Q4_K_M-00002-of-00002.gguf", 1), f("m-Q4_K_M-00001-of-00002.gguf", 1)];
        assert_eq!(find_best_model(&split, "").path, "m-Q4_K_M-00001-of-00002.gguf");
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
        assert_eq!(find_best_sibling(&files, "m-Q8_0.gguf", "mmproj", "").path, "mmproj-F16.gguf");
        // A model in BF16/: its own directory wins over the root.
        assert_eq!(find_best_sibling(&files, "BF16/m-BF16.gguf", "mmproj", "").path, "BF16/mmproj-BF16.gguf");
        // An exact tag beats closer bits.
        let mtps = vec![f("mtp-Q4_0.gguf", 1), f("mtp-Q8_0.gguf", 1)];
        assert_eq!(find_best_sibling(&mtps, "m-Q8_0.gguf", "mtp-", "Q8_0").path, "mtp-Q8_0.gguf");
        assert_eq!(find_best_sibling(&mtps, "m-Q4_K_M.gguf", "mtp-", "Q4_K_M").path, "mtp-Q4_0.gguf");
        assert_eq!(find_best_sibling(&mtps, "m.gguf", "dflash-", "").path, "");
    }

    // ── listing ──

    /// Canned API answers keyed by url.
    struct Canned(HashMap<String, (u16, String)>);
    impl Fetch for Canned {
        fn get(&self, url: &str, _token: Option<&Token>) -> Result<(u16, String), HfError> {
            self.0.get(url).cloned().ok_or_else(|| HfError::Network(format!("unexpected url {url}")))
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
            (format!("{e}api/models/o/r/tree/{COMMIT}?recursive=true"), (200, tree.to_string())),
        ]))
    }

    #[test]
    fn listing_groups_shards_and_pairs_sidecars() {
        let l = list_repo_with(&canned_repo(), "https://hf.test/", &spec("o/r", None), None).unwrap();
        assert_eq!(l.commit, COMMIT);
        assert_eq!(l.project_url, "https://hf.test/o/r");
        let names: Vec<&str> = l.candidates.iter().map(|c| c.primary().path.as_str()).collect();
        assert_eq!(names, ["Q-8B-Q8_0.gguf", "Q-8B-Q4_K_M.gguf", "Q-8B-BF16-00001-of-00002.gguf"]);
        assert_eq!(l.default, 1, "Q4_K_M is upstream's default pick");
        let bf16 = &l.candidates[2];
        assert_eq!((bf16.tag.as_str(), bf16.shards.len(), bf16.size), ("BF16", 2, 16000));
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
        let file = RepoSpec { file: Some("Q-8B-Q8_0.gguf".into()), ..spec("o/r", None) };
        assert_eq!(list_repo_with(&c, "https://hf.test/", &file, None).unwrap().default, 0);
        let missing = list_repo_with(&c, "https://hf.test/", &spec("o/r", Some("Q2_K")), None);
        assert!(matches!(missing, Err(HfError::NoMatch { .. })));
    }

    #[test]
    fn listing_refuses_preset_and_empty_repos_and_maps_api_errors() {
        let s = spec("o/r", None);
        let preset = build_listing("o/r", COMMIT, vec![f("preset.ini", 1), f("m-Q4_K_M.gguf", 1)], &s, "e/");
        assert_eq!(preset, Err(HfError::PresetRepo { repo: "o/r".into() }));
        let empty = build_listing("o/r", COMMIT, vec![f("README.md", 1), f("mmproj-F16.gguf", 1)], &s, "e/");
        assert_eq!(empty, Err(HfError::NoGguf { repo: "o/r".into() }));
        let gated = Canned(HashMap::from([(
            "https://hf.test/api/models/o/r/refs".to_string(),
            (403, "{\"error\":\"gated\"}".to_string()),
        )]));
        let tok = Token::new("hf_x").unwrap();
        assert_eq!(
            list_repo_with(&gated, "https://hf.test/", &s, Some(&tok)),
            Err(HfError::Gated { repo: "o/r".into(), had_token: true })
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
        assert_eq!(classify_input(" org/repo:Q4_K_M ").unwrap(), Some(Input::Spec(spec("org/repo", Some("Q4_K_M")))));
        assert_eq!(
            classify_input("https://huggingface.co/org/repo").unwrap(),
            Some(Input::Spec(spec("org/repo", None)))
        );
        assert_eq!(classify_input("qwen3   8b").unwrap(), Some(Input::Query("qwen3 8b".into())));
        assert_eq!(classify_input("tinygemma3").unwrap(), Some(Input::Query("tinygemma3".into())));
        // A slash with a space is words, not a repo id.
        assert_eq!(classify_input("unsloth/ qwen3").unwrap(), Some(Input::Query("unsloth/ qwen3".into())));
        // Slash-carrying, no space, not a repository: the spec's own error.
        for bad in ["org/repo:", "a/b/c", "https://example.com/org/repo", "https://huggingface.co/org"] {
            assert!(matches!(classify_input(bad), Err(HfError::InvalidSpec(_))), "{bad:?}");
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
             &limit=30&expand%5B%5D=downloads&expand%5B%5D=gated&expand%5B%5D=private&expand%5B%5D=siblings"
        );
        assert!(!u.contains('[') && !u.contains(' ') && !u.contains('"'), "nothing curl globs or the config quotes");
        assert!(search_url("e/", "q", 0).contains("&limit=1&"));
    }

    #[test]
    fn search_answers_parse_into_rows_with_quants_and_sidecars() {
        let e = "https://hf.test/";
        let canned = Canned(HashMap::from([(search_url(e, "qwen3", 30), (200, SEARCH_FIXTURE.to_string()))]));
        let hits = search_with(&canned, e, " qwen3 ", 30, None).unwrap();
        let repos: Vec<&str> = hits.iter().map(|h| h.repo.as_str()).collect();
        assert_eq!(
            repos,
            ["Qwen/Qwen3-0.6B-GGUF", "unsloth/Qwen3.6-35B-A3B-MTP-GGUF", "google/gemma-3-4b-it-qat-q4_0-gguf", "ggml-org/tinygemma3-GGUF"],
            "the API's order (most downloaded first) is kept"
        );
        let q = &hits[0];
        assert_eq!((q.downloads, q.gated, q.private, q.mmproj), (231589, false, false, false));
        assert_eq!(q.quants, ["Q8_0"]);
        let moe = &hits[1];
        assert_eq!(moe.quants, ["IQ2_M", "MXFP4_MOE", "Q4_K_M", "Q4_K_XL", "Q8_0", "BF16"], "a split once, UD- read as upstream reads it");
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
        assert_eq!((h[0].downloads, h[0].gated, h[0].quants.as_slice()), (0, false, &["Q4_K_M".to_string()][..]));

        let empty = Canned(HashMap::from([(search_url(e, "zzz", 5), (200, "[]".to_string()))]));
        assert_eq!(search_with(&empty, e, "zzz", 5, None).unwrap(), []);
        let odd = Canned(HashMap::from([(search_url(e, "zzz", 5), (200, "{\"error\":\"x\"}".to_string()))]));
        assert!(matches!(search_with(&odd, e, "zzz", 5, None), Err(HfError::Http { status: 200, .. })));
    }

    #[test]
    fn search_errors_name_no_repository() {
        assert_eq!(search_error(401, "", true), HfError::TokenRejected { repo: "the search".into() });
        assert!(matches!(search_error(429, "", false), HfError::Network(m) if m.contains("rate limited")));
        assert_eq!(
            search_error(400, "{\"error\":\"bad expand\"}", false),
            HfError::Http { status: 400, message: "bad expand".into() }
        );
        assert!(matches!(search_error(404, "", false), HfError::Http { status: 404, .. }));
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

    #[test]
    fn placements_land_in_the_scanned_subfolders_per_repo() {
        let root = Path::new("M");
        let j = job(root, 2, true, Some(Sidecar::Dspark));
        // BF16 has no dspark sidecar: only the shards and the mmproj.
        let t: Vec<PathBuf> = j.placements().into_iter().map(|p| p.target).collect();
        // `<org>\<repo>`: two orgs' same-named repos never share a folder.
        assert_eq!(
            t,
            [
                root.join("models").join("o").join("r").join("Q-8B-BF16-00001-of-00002.gguf"),
                root.join("models").join("o").join("r").join("Q-8B-BF16-00002-of-00002.gguf"),
                root.join("mmprojs").join("o").join("r").join("mmproj-F16.gguf"),
            ]
        );
        let mut other_org = job(root, 2, true, None);
        other_org.repo = "p/r".into();
        assert_ne!(other_org.placements()[0].target, t[0]);
        let j = job(root, 1, false, Some(Sidecar::Dflash));
        let p = j.placements();
        assert_eq!(p.len(), 2);
        assert_eq!(p[1].role, Role::Draft(Sidecar::Dflash));
        assert_eq!(p[1].target, root.join("dflashs").join("o").join("r").join("dflash-Q4_K_M.gguf"));
        let g = groups(&j.placements()).into_iter().map(|g| g.len()).collect::<Vec<_>>();
        assert_eq!(g, [1, 1]);
        assert_eq!(groups(&job(root, 2, true, None).placements()).len(), 2, "shards share one child");
    }

    #[test]
    fn child_env_names_one_file_and_hides_the_token_from_argv() {
        let j = job(Path::new("M"), 1, true, None);
        let (set, clear) = child_env(&j, "Q-8B-Q4_K_M.gguf");
        let get = |k: &str| set.iter().find(|(n, _)| *n == k).map(|(_, v)| v.clone());
        assert!(get("LLAMA_SERVER_ROUTER_PORT").is_some());
        assert_eq!(get("LLAMA_SERVER_CHILD_MODE").as_deref(), Some("download"));
        assert_eq!(get("LLAMA_ARG_HF_REPO").as_deref(), Some("o/r"));
        assert_eq!(get("LLAMA_ARG_HF_FILE").as_deref(), Some("Q-8B-Q4_K_M.gguf"));
        assert_eq!(get("LLAMA_CACHE").map(PathBuf::from), Some(Path::new("M").join(STAGING_DIR)));
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
            assert!(stdin.is_none(), "the pipe is closed (dropped), so the child reads EOF");
        }
        assert!(written.is_empty(), "no exit line: it would delete the partial");
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
            parse_child_line(&line(r#"{"state":"downloading","payload":{"url":"u/x.gguf","downloaded":5,"total":9}}"#)),
            Ok(Some(ChildEvent::Progress { url: "u/x.gguf".into(), downloaded: 5, total: 9 }))
        );
        assert_eq!(
            parse_child_line(&line(r#"{"state":"downloading","payload":{"result":"download_finished"}}"#)),
            Ok(Some(ChildEvent::Finished))
        );
        assert_eq!(
            parse_child_line(&line("{\"state\":\"downloading\",\"payload\":{\"result\":\"download_failed\"}}\r")),
            Ok(Some(ChildEvent::Failed))
        );
        assert_eq!(parse_child_line("some stray log line"), Ok(None));
        assert!(parse_child_line(&line("{not json")).is_err());
        assert!(parse_child_line(&line(r#"{"state":"loading","payload":{}}"#)).is_err());
        assert!(parse_child_line(&line(r#"{"state":"downloading","payload":{"result":"done"}}"#)).is_err());
        assert!(parse_child_line(&line(r#"{"state":"downloading","payload":{}}"#)).is_err());
    }

    #[test]
    fn tracker_follows_parallel_files_by_url_suffix() {
        let j = job(Path::new("M"), 2, true, None);
        let p = j.placements();
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
        assert_eq!((pr.file_done, pr.file_total), (4000, 9000), "the per-file bytes follow the current file");
        assert!(t.update(&format!("{base}Q-8B-BF16-00002-of-00002.gguf"), 7000, 7000));
        assert!(!t.update(&format!("{base}unrelated.gguf"), 1, 1));
        let pr = t.progress();
        assert_eq!((pr.done, pr.files_done, pr.file.as_str()), (11600, 2, "Q-8B-BF16-00002-of-00002.gguf"));
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
        assert!(!has_partial(dir.path(), "o/r", q4), "the folder alone is nothing to resume");
        // Q8_0's partial is not Q4_K_M's.
        std::fs::create_dir_all(repo_dir.join("blobs")).unwrap();
        std::fs::write(repo_dir.join("blobs").join("o1.downloadInProgress"), b"x").unwrap();
        assert!(has_partial(dir.path(), "o/r", q8));
        assert!(!has_partial(dir.path(), "o/r", q4), "another quant's partial");
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
        let p = j.placements();
        let snap = j.staging().join(repo_folder("o/r")).join("snapshots").join(COMMIT);
        std::fs::create_dir_all(&snap).unwrap();
        std::fs::write(snap.join("Q-8B-Q4_K_M.gguf"), vec![0u8; 10]).unwrap();
        let mut t = Tracker::new(&p, &[false]);
        let err = place_group(&j, &[&p[0]], &[], &mut t).unwrap_err();
        assert!(matches!(err, HfError::Io(ref m) if m.contains("removed")), "{err}");
        assert!(!p[0].target.exists());
        assert_eq!(target_present(&p[0].target, 4000), Ok(false), "a retry starts clean");
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
            classify_failure(&l(&["get_repo_commit: error: GET failed (401): Invalid credentials"]), true, r),
            HfError::TokenRejected { repo: r.into() }
        );
        assert_eq!(
            classify_failure(&l(&["GET failed (401): Repository not found"]), false, r),
            HfError::NotFoundOrPrivate { repo: r.into() }
        );
        assert_eq!(
            classify_failure(&l(&["Download 'x' failed with status code: 403"]), false, r),
            HfError::Gated { repo: r.into(), had_token: false }
        );
        assert!(matches!(
            classify_failure(&l(&["common_pull_file: error writing to file: x.downloadInProgress"]), false, r),
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
        assert!(matches!(classify_failure(&l(&["something odd"]), false, r), HfError::Download(m) if m == "something odd"));
        assert!(matches!(classify_failure(&[], false, r), HfError::Download(m) if m.contains("no output")));
    }

    // ── staging → subfolders (real files in a temp dir; no child) ──

    #[test]
    fn staged_files_move_out_and_existing_ones_are_reused_or_refused() {
        let dir = tempfile::tempdir().unwrap();
        let j = job(dir.path(), 1, true, None);
        let placements = j.placements();
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
        assert_eq!(std::fs::metadata(&placements[0].target).unwrap().len(), 4000);
        assert!(!snap.join("Q-8B-Q4_K_M.gguf").exists(), "moved, not copied");
        assert_eq!(std::fs::read(mm).unwrap(), vec![1u8; 600], "the existing file is kept");
        assert_eq!(t.progress().done, t.progress().total);

        // A different file in the way is refused, naming it.
        std::fs::write(mm, vec![1u8; 5]).unwrap();
        assert!(matches!(
            target_present(mm, 600),
            Err(HfError::TargetConflict { found: 5, expected: 600, .. })
        ));
    }

    #[test]
    fn a_success_with_nothing_staged_reads_as_the_swallowed_error() {
        let dir = tempfile::tempdir().unwrap();
        let j = job(dir.path(), 1, false, None);
        let p = j.placements();
        let mut t = Tracker::new(&p, &[false]);
        let log = vec!["get_repo_commit: error: GET failed (401): nope".to_string()];
        assert_eq!(
            place_group(&j, &[&p[0]], &log, &mut t),
            Err(HfError::TokenRejected { repo: "o/r".into() })
        );
        let other = place_group(&j, &[&p[0]], &["fine".into()], &mut t).unwrap_err();
        assert!(other.to_string().contains("reported success but Q-8B-Q4_K_M.gguf is not in"));
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
        move_real_data(&snap.join("m.gguf"), &dst).unwrap();
        assert!(!std::fs::symlink_metadata(&dst).unwrap().file_type().is_symlink());
        assert_eq!(std::fs::read(&dst).unwrap(), b"data");
        assert!(!blobs.join("oid").exists());
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
            Some(dir.path().join("snapshots").join(&other).join("sub").join("x.gguf"))
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
            mmproj: mmproj.then(|| PathBuf::from("M/mmprojs/r/mmproj-F16.gguf")),
            draft: draft.map(|s| (s, PathBuf::from("M/dflashs/r/dflash-Q4_K_M.gguf"))),
        }
    }

    #[test]
    fn derived_preset_fills_paths_url_sampling_and_fit() {
        let d = downloaded(true, None);
        let p = derive_preset("m-Q4_K_M".into(), &d, Some(&info(Some("{% if x %}{{ y }}{% endif %}"), 0, false)), None);
        assert_eq!(p.model, d.model.to_string_lossy());
        assert_eq!(p.mmproj, d.mmproj.as_ref().unwrap().to_string_lossy());
        assert_eq!(p.project_url, "https://huggingface.co/o/r");
        assert_eq!((p.temp, p.top_k, p.top_p, p.min_p, p.repeat_penalty), (Some(0.6), Some(20), Some(0.95), Some(0.0), None));
        assert_eq!(p.jinja, Some(true));
        assert!(p.fit);
        assert!(crate::form::fit_allowed(p.ctx_size.is_none(), p.n_gpu_layers.is_none()));
        assert_eq!(p.reasoning, "auto");
        assert_eq!((p.model_draft.as_str(), p.spec_type.as_str()), ("", ""), "embedded-MTP-free and no drafter");
        // Everything the derivation does not own is the New dialog's preset.
        let base = Preset {
            model: p.model.clone(),
            project_url: p.project_url.clone(),
            mmproj: p.mmproj.clone(),
            temp: p.temp,
            top_k: p.top_k,
            top_p: p.top_p,
            min_p: p.min_p,
            fit: true,
            ..Preset::new_default("m-Q4_K_M".into(), String::new())
        };
        assert_eq!(p, base);
    }

    #[test]
    fn derived_drafter_takes_the_header_verdict_over_the_prefix() {
        let d = downloaded(false, Some(Sidecar::Dflash));
        let m = info(None, 0, true);
        let header = derive_preset("x".into(), &d, Some(&m), Some("draft-dspark"));
        assert_eq!(header.spec_type, "draft-dspark");
        assert_eq!(header.model_draft, "M/dflashs/r/dflash-Q4_K_M.gguf");
        let silent = derive_preset("x".into(), &d, Some(&m), None);
        assert_eq!(silent.spec_type, "draft-dflash");
        // The prunes leave a drafter-backed preset alone (they are idempotent here).
        let mut again = silent.clone();
        assert!(presets::prune_inactive_draft_keys(&mut again, false).is_empty());
        assert!(!presets::prune_inactive_moe_cache(&mut again, true));
        assert_eq!(again, silent);
    }

    #[test]
    fn derived_jinja_and_the_unreadable_header() {
        let d = downloaded(false, None);
        let legacy = derive_preset("x".into(), &d, Some(&info(Some("chatml"), 0, false)), None);
        assert_eq!(legacy.jinja, Some(false));
        let none = derive_preset("x".into(), &d, Some(&info(None, 2, false)), None);
        assert_eq!(none.jinja, Some(true));
        assert_eq!(none.spec_type, "", "embedded MTP heads are not switched on");
        // No header: paths, URL and fit only.
        let blind = derive_preset("x".into(), &d, None, None);
        assert_eq!((blind.temp, blind.top_k, blind.jinja), (None, None, Some(true)));
        assert!(blind.fit);
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
}
