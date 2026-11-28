//! Create a model profile from a Hugging Face repository: find GGUF repositories by
//! name, list the GGUFs a repo offers, download one with llama.cpp's OWN
//! downloader, file it into the ModelsDir subfolders the Models tab already
//! scans, and derive the model profile's fields from the downloaded headers.
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
//! 4. `model_profile_from_download` (impure: reads the headers) over `derive_model_profile`
//!    (pure): the `ModelProfile` to save.
//!
//! Everything slow is meant to run on a `std::thread` (steps 0 and 2 by the
//! caller, step 3 by `spawn_download` itself), its result posted back with
//! `slint::invoke_from_event_loop`: the GUI's threading rule (`gui.rs`
//! header). Nothing here touches Slint. The two callers are the CLI's
//! `model-profile download` (`cli.rs`) and the Models tab's From Hugging Face dialog
//! (`gui/hf_dialog.rs`), which asks `has_partial` whether its button should
//! read "Resume download" and hands the job to the GUI's background downloads
//! (`gui/downloads.rs`), which read `Progress`'s per-file bytes and run
//! `model_profile_from_download` off the event loop.
//!
//! ## The search: one request, every row's file names included
//!
//! `GET /api/models?search=<query>&filter=gguf&sort=downloads&direction=-1
//! &limit=<n>&expand[]=downloads&expand[]=gated&expand[]=private
//! &expand[]=siblings&expand[]=gguf` (`search_url`). Verified against
//! huggingface.co with curl (2026-10-27):
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
//!   which is the listing's job once a row is picked.
//! - `gguf` is the header summary of the repo's GGUFs: `total` (the parameter
//!   count), `architecture`, `context_length`, `totalFileSize`, and the whole
//!   chat template (per row, so it sets the size of the answer). Only
//!   `total` and `architecture` are read, into the row's model marker
//!   (`model_kind`, `kind_label`: `MoE 30.5B`, `dense 8.2B`). The architecture
//!   alone does not always tell: Mixtral is `llama`, so an arch that serves
//!   both reads as "dense/MoE unknown" unless the repo name carries an MoE
//!   size pattern (`30B-A3B`, `8x7B`). A row without `gguf` shows no marker.
//! - Every `id` is held to `valid_repo_id` (the rule `parse_spec` enforces)
//!   before it becomes a row: the answer is the remote's data, and a row's id
//!   becomes a `RepoSpec`, a URL and a folder name. A hit that fails is dropped.
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
//! ## The listing, the spec, the transport: `pai_core::hf`
//!
//! `parse_spec`, `list_repo` with its mirror of llama.cpp's file rules, the
//! `Token`, the endpoint and the curl transport are core's, and core's module
//! header says how they work: the bff lists through the same code
//! (ARCHITECTURE.md, "Choosing the model file on the web"). This module
//! re-exports the names so the callers keep their `crate::hf::` paths, and
//! keeps what is the configurator's own: the search, the download through
//! llama-server's router child, the placement into the ModelsDir subfolders,
//! and the placement estimate (`hf_fit`), which asks the same `Fetch` seam for
//! the first MiBs of a candidate's GGUF (`Fetch::get_range`, sent with a
//! `range` and a `max-filesize` so a server that ignores the range is refused
//! instead of streaming a whole model into memory).
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
//! `<org>\<repo>` (`models\unsloth\Qwen3-8B-GGUF\Qwen3-8B-Q4_K_M.gguf`). The
//! org is part of it on purpose: `Qwen/` and `unsloth/` both publish a
//! `Qwen3-8B-GGUF` with a `Qwen3-8B-Q4_K_M.gguf`, and with the repo name alone
//! one org's file (at an equal size) would be taken as the other's, already
//! downloaded. The per-repo directory also keeps two repos' projectors apart;
//! the scans are recursive, so the pickers see the files unchanged.
//!
//! The model and its drafters keep their Hugging Face file names. The mmproj
//! does not: on Hugging Face it is usually generic (`mmproj-F16.gguf`, which
//! Unsloth names every projector; `mmproj-model-f16.gguf`), and the mmproj
//! picker shows file names, so it would say nothing about which model it
//! serves. It is named after the model instead (`mmproj_target_name`): the
//! model's first shard's name without `.gguf`, its shard suffix and its quant
//! tag (`UD-` included), then `-mmproj`, then the projector's own quant tag
//! when its HF name carries one: `Qwen3-VL-8B-Instruct-Q4_K_M.gguf` +
//! `mmproj-F16.gguf` lands as `Qwen3-VL-8B-Instruct-mmproj-F16.gguf`. With the
//! quant cut off, every quant of one repo maps a shared projector to the SAME
//! name, so the second quant's download finds it in place (the same-size rule
//! below) instead of storing a copy per quant. Drafters (MTP, DFlash, DSpark)
//! are left alone: they are models in their own right, published under names
//! of their own that the drafter picker shows as they are. Everything
//! downstream (the model profile's `mmproj` key, the status line, the delete
//! dialog's linked files, the skip/conflict rule) reads the placement's
//! target, so it sees the new name.
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
//! The token reaches curl through core's stdin config and the download child
//! through `HF_TOKEN` in its environment, never an argv, and every stderr line
//! kept for an error message passes through `redact` first. Nothing here
//! writes it to a model profile, a log or bench.db.

use crate::strings;
use std::collections::VecDeque;
use std::io::{BufRead, BufReader, Write};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{mpsc, Arc, Mutex};
use std::time::{Duration, Instant};

use serde_json::Value;

use crate::gguf;
use crate::hf_fit;
use crate::model_profiles::{self, ModelProfile};

// ── The listing half: `pai_core::hf` ─────────────────────────────────────
//
// The repository spec, the listing, the token, the endpoint and the curl
// transport live in core, so the bff lists through the same code
// (ARCHITECTURE.md, "Choosing the model file on the web"). Re-exported here:
// the configurator's callers (`gui/hf_dialog.rs`, `gui/downloads.rs`), the CLI
// and `hf_fit` keep their `crate::hf::` paths.
pub use pai_core::hf::{
    api_error, api_message, bits_of_tag, build_listing, commit_from_refs, curl_config,
    curl_config_with, curl_failure, curl_value, endpoint, endpoint_from, files_from_tree,
    find_best_model, find_best_sibling, is_model_file, list_repo, list_repo_with, parse_curl_bytes,
    parse_curl_output, parse_spec, quant_bits, redact, safe_repo_path, split_info, token_from,
    token_from_env, valid_repo_id, Candidate, Curl, Fetch, HfError, Listing, RepoFile, RepoSpec,
    Sidecar, SplitInfo, Token,
};

/// The ModelsDir subfolder a sidecar lands in: DSpark shares `dflashs` with
/// DFlash, as the Assets picker expects. The layout is the configurator's, so
/// it is not core's `Sidecar`'s method.
fn sidecar_subdir(sidecar: Sidecar) -> &'static str {
    match sidecar {
        Sidecar::Mtp => crate::model_scan::Category::Mtp.subdir(),
        Sidecar::Dflash | Sidecar::Dspark => crate::model_scan::Category::Dflash.subdir(),
    }
}

/// `CMD_CHILD_TO_ROUTER_STATE` in llama.cpp's `tools/server/server-models.cpp`:
/// the prefix of every protocol line a router child prints. Re-check on a bump.
pub const CHILD_STATE_PREFIX: &str = "cmd_child_to_router:state:";
/// `CMD_ROUTER_TO_CHILD_EXIT` in the same file: the line that ends a child,
/// and cancels a download in flight. Re-check on a bump.
pub const CHILD_EXIT_CMD: &str = "cmd_router_to_child:exit";
/// The staging cache under ModelsDir (see the module header).
pub const STAGING_DIR: &str = ".hf-staging";

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
        return Err(HfError::InvalidSpec(strings::t("enter-a-search")));
    }
    if q.chars().any(char::is_control) {
        return Err(HfError::InvalidSpec(strings::t(
            "search-control-characters",
        )));
    }
    if q.chars().count() > QUERY_MAX {
        return Err(HfError::InvalidSpec(strings::fill(
            "search-too-long",
            &[&QUERY_MAX],
        )));
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
    /// Dense or MoE, when the API's `gguf.architecture` (or the repo name)
    /// actually tells (`model_kind`).
    pub kind: ModelKind,
    /// `gguf.total`: the parameter count of the model the repo's GGUFs hold.
    pub params: Option<u64>,
}

/// Whether a model is a mixture of experts, as far as the search answer can
/// tell without a GGUF header. See `model_kind` for what is trusted.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ModelKind {
    Dense,
    Moe,
    /// The architecture name serves both (`llama` also carries Mixtral), or is
    /// one this build does not know, and the repo name has no MoE pattern.
    Unknown,
}

/// The architectures (`general.architecture`) llama.cpp implements as
/// mixtures of experts and that carry no `moe` in their name, which already
/// counts (`qwen3moe`, `glm4moe`, `bailingmoe2`, ...). From `src/llama-arch.cpp`
/// (v0.9.0): each of these sets `expert_count`. Re-check on a bump; a missing
/// one only reads as "unknown".
const MOE_ARCHS: [&str; 23] = [
    "deepseek",
    "deepseek2",
    "dbrx",
    "arctic",
    "grok",
    "gpt-oss",
    "llama4",
    "jamba",
    "qwen3next",
    "minimax-m2",
    "dots1",
    "smallthinker",
    "kimi-linear",
    "step35",
    "beam",
    "deepseek41",
    "flint",
    "kolibri1",
    "laguna",
    "mellum",
    "metis16",
    "mistral-large4",
    "muse-spark",
];

/// The architectures llama.cpp implements as DENSE only (their MoE siblings
/// are separate archs, `qwen3` vs `qwen3moe`), so the name alone settles it.
/// `llama`, `mistral3`, `minicpm`, `granite` and the like are NOT here: one
/// arch name serves both a dense and an MoE model there (Mixtral is `llama`,
/// MiniCPM-MoE is `minicpm`), so they read as unknown unless the repo name
/// says more.
const DENSE_ARCHS: [&str; 24] = [
    "qwen2",
    "qwen3",
    "qwen2vl",
    "qwen3vl",
    "gemma",
    "gemma2",
    "gemma3",
    "gemma3n",
    "gemma-embedding",
    "phi2",
    "phi3",
    "starcoder2",
    "falcon",
    "gpt2",
    "stablelm",
    "command-r",
    "cohere2",
    "internlm2",
    "exaone",
    "exaone4",
    "olmo2",
    "smollm3",
    "seed_oss",
    "glm4",
];

/// Dense or MoE from the search answer's `gguf.architecture` and the repo
/// name (pure). In order: an arch with `moe` in its name, or in `MOE_ARCHS`,
/// is MoE; a repo name with an MoE size pattern (`30B-A3B`, an active-
/// parameter `A<n>B` token, or `8x7B`) is MoE; an arch in `DENSE_ARCHS` is
/// dense; everything else is `Unknown`, shown as such rather than guessed.
pub fn model_kind(arch: Option<&str>, repo: &str) -> ModelKind {
    let arch = arch.map(str::to_ascii_lowercase).unwrap_or_default();
    if arch.contains("moe") || MOE_ARCHS.contains(&arch.as_str()) {
        return ModelKind::Moe;
    }
    if name_says_moe(repo) {
        return ModelKind::Moe;
    }
    if DENSE_ARCHS.contains(&arch.as_str()) {
        return ModelKind::Dense;
    }
    ModelKind::Unknown
}

/// The repo NAME's MoE size patterns: a token `a<digits>[.<digits>]b` (the
/// active parameters, `Qwen3-30B-A3B`) or `<n>x<m>b` (`Mixtral-8x7B`), the
/// name split into tokens at `-` and `_` (a `.` stays inside, `A1.5B`).
fn name_says_moe(repo: &str) -> bool {
    let name = repo.rsplit('/').next().unwrap_or(repo).to_ascii_lowercase();
    name.split(['-', '_']).any(|tok| {
        let active = tok
            .strip_prefix('a')
            .and_then(|r| r.strip_suffix('b'))
            .is_some_and(|n| {
                !n.is_empty()
                    && n.chars().all(|c| c.is_ascii_digit() || c == '.')
                    && n.starts_with(|c: char| c.is_ascii_digit())
            });
        let experts = tok
            .strip_suffix('b')
            .and_then(|r| r.split_once('x'))
            .is_some_and(|(a, b)| {
                !a.is_empty()
                    && !b.is_empty()
                    && a.chars().all(|c| c.is_ascii_digit())
                    && b.chars().all(|c| c.is_ascii_digit() || c == '.')
            });
        active || experts
    })
}

/// `30.5B` / `8.2B` / `596M` / `1.0T`: a parameter count for a search row.
pub fn format_params(n: u64) -> String {
    let f = n as f64;
    if f >= 1e12 {
        format!("{:.1}T", f / 1e12)
    } else if f >= 100e9 {
        format!("{:.0}B", f / 1e9)
    } else if f >= 1e9 {
        format!("{:.1}B", f / 1e9)
    } else if f >= 1e6 {
        format!("{:.0}M", f / 1e6)
    } else {
        n.to_string()
    }
}

/// The search row's model marker: `MoE 30.5B`, `dense 8.2B`, or for an
/// unknown kind the count alone with `dense/MoE unknown`; `None` when the
/// answer says nothing at all (no count, no kind).
pub fn kind_label(kind: ModelKind, params: Option<u64>) -> Option<String> {
    let p = params.map(format_params);
    match (kind, p) {
        (ModelKind::Moe, Some(p)) => Some(strings::fill("kind-moe-params", &[&p])),
        (ModelKind::Moe, None) => Some("MoE".into()),
        (ModelKind::Dense, Some(p)) => Some(strings::fill("kind-dense-params", &[&p])),
        (ModelKind::Dense, None) => Some(strings::t("modality-dense")),
        (ModelKind::Unknown, Some(p)) => Some(strings::fill("kind-unknown-params", &[&p])),
        (ModelKind::Unknown, None) => None,
    }
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
        .map_err(|e| HfError::Network(strings::fill("search-unreadable", &[&e])))?;
    hits_from_search(&v)
}

/// The search request (see the module header for every parameter).
fn search_url(endpoint: &str, query: &str, limit: usize) -> String {
    let mut url = format!(
        "{endpoint}api/models?search={}&filter=gguf&sort=downloads&direction=-1&limit={}",
        percent_encode(query),
        limit.max(1)
    );
    for field in ["downloads", "gated", "private", "siblings", "gguf"] {
        url.push_str("&expand%5B%5D=");
        url.push_str(field);
    }
    url
}

/// RFC 3986 percent-encoding of everything but the unreserved characters
/// (so a space is `%20`, never `+`).
pub(crate) fn percent_encode(s: &str) -> String {
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
        401 if had_token => HfError::TokenRejected {
            repo: strings::t("the-search"),
        },
        429 => HfError::Network(strings::t("hf-rate-limited")),
        _ => HfError::Http {
            status,
            message: api_message(body),
        },
    }
}

/// The rows of a search answer, in its order (most downloaded first). An
/// answer that is not an array is a protocol change, reported as such; an
/// entry without an `id`, or with one `valid_repo_id` refuses (the id is the
/// remote's data and becomes a `RepoSpec`, a URL and a folder name), is
/// skipped. `gguf.total` and `gguf.architecture` give the model marker; only
/// those two fields of the (large: it carries the whole chat template)
/// `gguf` object are read.
fn hits_from_search(v: &Value) -> Result<Vec<SearchHit>, HfError> {
    let items = v.as_array().ok_or_else(|| HfError::Http {
        status: 200,
        message: strings::t("search-not-a-list"),
    })?;
    Ok(items
        .iter()
        .filter_map(|m| {
            let repo = m
                .get("id")?
                .as_str()
                .filter(|id| valid_repo_id(id))?
                .to_string();
            let gguf = m.get("gguf");
            let arch = gguf
                .and_then(|g| g.get("architecture"))
                .and_then(Value::as_str);
            let kind = model_kind(arch, &repo);
            let params = gguf
                .and_then(|g| g.get("total"))
                .and_then(Value::as_u64)
                .filter(|&n| n > 0);
            let files: Vec<&str> = m
                .get("siblings")
                .and_then(Value::as_array)
                .map(|s| {
                    s.iter()
                        .filter_map(|f| f.get("rfilename")?.as_str())
                        .collect()
                })
                .unwrap_or_default();
            let ggufs = || files.iter().filter(|f| f.ends_with(".gguf"));
            Some(SearchHit {
                downloads: m.get("downloads").and_then(Value::as_u64).unwrap_or(0),
                // `false`, or the gating mode as a string.
                gated: m
                    .get("gated")
                    .is_some_and(|g| g.as_bool().unwrap_or(!g.is_null())),
                private: m.get("private").and_then(Value::as_bool).unwrap_or(false),
                quants: quants_of(&files),
                mmproj: ggufs().any(|f| f.contains("mmproj")),
                drafters: Sidecar::ALL
                    .into_iter()
                    .filter(|s| ggufs().any(|f| f.contains(s.keyword())))
                    .collect(),
                kind,
                params,
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
            (
                i64::MAX,
                name.strip_suffix(".gguf").unwrap_or(name).to_string(),
            )
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

/// The file name a downloaded mmproj gets (pure), from the model's first
/// shard's file name and the mmproj's Hugging Face file name: the model's
/// `split_info` prefix (no `.gguf`, no `-NNNNN-of-NNNNN`) with its quant tag
/// cut off (the `-`/`.` before it included, and an Unsloth `-UD` left before
/// it), then `-mmproj`, then `-<TAG>` when the mmproj's own `split_info` tag
/// is a quant, then `.gguf`. `tinygemma3-Q8_0.gguf` + `mmproj-tinygemma3.gguf`
/// is `tinygemma3-mmproj.gguf`; `Qwen3-VL-8B-Instruct-Q4_K_M.gguf` +
/// `mmproj-F16.gguf` is `Qwen3-VL-8B-Instruct-mmproj-F16.gguf`. Every quant of
/// one repo maps a shared projector to ONE name, so a second quant's download
/// finds it in place and skips it (module header). A model name whose tag
/// cut leaves nothing keeps the mmproj's own name.
fn mmproj_target_name(model_file: &str, mmproj_file: &str) -> String {
    let model = split_info(model_file);
    let mut base = model.prefix.as_str();
    if is_quant_tag(&model.tag) {
        // The tag is the prefix's tail, upper-cased: cut by length.
        let cut = base.len() - model.tag.len();
        base = base[..cut].strip_suffix(['-', '.']).unwrap_or(&base[..cut]);
        if let Some(b) = base
            .strip_suffix("-UD")
            .or_else(|| base.strip_suffix("-ud"))
        {
            base = b;
        }
    }
    if base.is_empty() {
        return mmproj_file.to_string();
    }
    let tag = split_info(mmproj_file).tag;
    if is_quant_tag(&tag) {
        format!("{base}-mmproj-{tag}.gguf")
    } else {
        format!("{base}-mmproj.gguf")
    }
}

/// Whether a `split_info` tag names a quantization (`Q4_K_M`, `IQ2_XXS`,
/// `TQ1_0`, `F16`, `BF16`, `MXFP4`, `NVFP4`, `Q4_K_XL`) rather than whatever
/// else ends a file name (`8B`, `A3B`, `TINYGEMMA3`, `INSTRUCT`): a type
/// prefix followed by a digit.
fn is_quant_tag(tag: &str) -> bool {
    ["MXFP", "NVFP", "BF", "IQ", "TQ", "Q", "F"]
        .iter()
        .find_map(|p| tag.strip_prefix(p))
        .is_some_and(|rest| rest.starts_with(|c: char| c.is_ascii_digit()))
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
    /// The ModelsDir the server reads (`server_cfg::models_dir`).
    pub models_dir: PathBuf,
    pub token: Option<Token>,
    pub server_exe: PathBuf,
}

/// Refuse a ModelsDir the user cannot write (under the Windows manager, the
/// instance's folder for a user who is not elevated) before any child starts:
/// the folder is created when missing, then a probe file is created and
/// removed.
fn check_writable(dir: &Path) -> Result<(), HfError> {
    let refused = || {
        HfError::Io(strings::fill(
            "download-folder-not-writable",
            &[&dir.display()],
        ))
    };
    std::fs::create_dir_all(dir).map_err(|_| refused())?;
    let probe = dir.join(format!(".pai-write-probe-{}", std::process::id()));
    let file = std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&probe)
        .map_err(|_| refused())?;
    drop(file);
    let _ = std::fs::remove_file(&probe);
    Ok(())
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
        let server_exe = crate::paths::llama_server_exe()
            .ok_or_else(|| HfError::MissingTool(strings::t("server-not-found-for-download")))?;
        DownloadJob::for_server(
            listing, index, mmproj, sidecar, models_dir, token, server_exe,
        )
    }

    /// `new` with the child's executable given: the GUI's background download
    /// (`gui/downloads.rs`) passes the one it found, and its e2e test a
    /// stand-in, since no test starts a child.
    pub fn for_server(
        listing: &Listing,
        index: usize,
        mmproj: bool,
        sidecar: Option<Sidecar>,
        models_dir: PathBuf,
        token: Option<Token>,
        server_exe: PathBuf,
    ) -> Result<DownloadJob, HfError> {
        let candidate = listing
            .candidates
            .get(index)
            .cloned()
            .ok_or_else(|| HfError::NoMatch {
                repo: listing.repo.clone(),
                wanted: strings::fill("candidate-numbered", &[&index]),
            })?;
        if let Some(s) = sidecar {
            if candidate.sidecar(s).is_none() {
                return Err(HfError::SidecarMissing { what: s.label() });
            }
        }
        check_writable(&models_dir)?;
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
    /// same-named repos never share a folder (module header). The file name is
    /// the Hugging Face one, except the mmproj's, which is named after the
    /// model (`mmproj_target_name`).
    ///
    /// The repo id and every file path are the remote's data, so they are
    /// held to `valid_repo_id` / `safe_repo_path` again here (whoever built
    /// the job), and each joined target must stay under its subfolder
    /// (`lexically_under`): `HfError::InvalidSpec` otherwise, before any file
    /// is touched.
    pub fn placements(&self) -> Result<Vec<Placement>, HfError> {
        if !valid_repo_id(&self.repo) {
            return Err(HfError::InvalidSpec(strings::fill(
                "not-a-repo-id",
                &[&self.repo],
            )));
        }
        let (org, name) = self.repo.split_once('/').unwrap_or(("", &self.repo));
        let place = |role: Role, file: &RepoFile| -> Result<Placement, HfError> {
            let sub = match role {
                Role::Model => crate::model_scan::Category::Model.subdir(),
                Role::Mmproj => crate::model_scan::Category::Mmproj.subdir(),
                Role::Draft(s) => sidecar_subdir(s),
            };
            let refused = || {
                HfError::InvalidSpec(strings::fill(
                    "refused-repo-path",
                    &[&format!("{:?}", file.path)],
                ))
            };
            if !safe_repo_path(&file.path) {
                return Err(refused());
            }
            let file_name = match role {
                Role::Mmproj => match self.candidate.shards.first() {
                    Some(model) => mmproj_target_name(model.name(), file.name()),
                    None => file.name().to_string(),
                },
                _ => file.name().to_string(),
            };
            // Derived from two checked paths, held to the same rule anyway:
            // one plain segment.
            if !safe_repo_path(&file_name) || file_name.contains('/') {
                return Err(refused());
            }
            let root = self.models_dir.join(sub);
            let target = root.join(org).join(name).join(&file_name);
            if !lexically_under(&target, &root) || target == root {
                return Err(refused());
            }
            Ok(Placement {
                role,
                target,
                file: file.clone(),
            })
        };
        let mut out: Vec<Placement> = self
            .candidate
            .shards
            .iter()
            .map(|f| place(Role::Model, f))
            .collect::<Result<_, _>>()?;
        if self.mmproj {
            if let Some(m) = &self.candidate.mmproj {
                out.push(place(Role::Mmproj, m)?);
            }
        }
        if let Some(s) = self.sidecar {
            if let Some(f) = self.candidate.sidecar(s) {
                out.push(place(Role::Draft(s), f)?);
            }
        }
        Ok(out)
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
    /// The model's first shard: what a model profile's `model` names.
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
                .map(|(p, &here)| {
                    (
                        p.file.path.clone(),
                        p.file.size,
                        if here { p.file.size } else { 0 },
                    )
                })
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
        let current = self
            .files
            .iter()
            .find(|f| !self.current.is_empty() && f.0 == self.current);
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
    Progress {
        url: String,
        downloaded: u64,
        total: u64,
    },
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
    let v: Value = serde_json::from_str(json)
        .map_err(|e| strings::fill("unreadable-state-line", &[&e, &line]))?;
    let state = v.get("state").and_then(Value::as_str).unwrap_or("");
    if state != "downloading" {
        return Err(strings::fill("unexpected-child-state", &[&state, &line]));
    }
    let payload = v.get("payload").cloned().unwrap_or(Value::Null);
    if let Some(r) = payload.get("result").and_then(Value::as_str) {
        return match r {
            "download_finished" => Ok(Some(ChildEvent::Finished)),
            "download_failed" => Ok(Some(ChildEvent::Failed)),
            other => Err(strings::fill("unknown-download-result", &[&other])),
        };
    }
    match payload.get("url").and_then(Value::as_str) {
        Some(url) if !url.is_empty() => Ok(Some(ChildEvent::Progress {
            url: url.to_string(),
            downloaded: payload
                .get("downloaded")
                .and_then(Value::as_u64)
                .unwrap_or(0),
            total: payload.get("total").and_then(Value::as_u64).unwrap_or(0),
        })),
        _ => Err(strings::fill("state-line-no-url", &[&line])),
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
fn child_env(
    job: &DownloadJob,
    hf_file: &str,
) -> (Vec<(&'static str, String)>, [&'static str; 12]) {
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
        set.push(("HF_TOKEN", t.secret().to_string()));
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
    let status =
        |n: &str| all.contains(&format!("({n})")) || all.contains(&format!("status code: {n}"));
    let repo = repo.to_string();
    if status("401") {
        return if had_token {
            HfError::TokenRejected { repo }
        } else {
            HfError::NotFoundOrPrivate { repo }
        };
    }
    if status("403") {
        return HfError::Gated { repo, had_token };
    }
    if status("404") {
        return HfError::NotFoundOrPrivate { repo };
    }
    let tail = tail_text(lines);
    if all.contains("error opening local file for writing") {
        return HfError::Io(strings::fill("cannot-create-download-file", &[&tail]));
    }
    if [
        "error writing to file",
        "no space left",
        "not enough space",
        "disk full",
    ]
    .iter()
    .any(|m| all.contains(m))
    {
        return HfError::DiskFull(tail);
    }
    if [
        "httplib failed",
        "download failed:",
        "could not establish connection",
        "status code: -1",
        "connection",
    ]
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
        strings::t("no-output-from-server")
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
        HfError::MissingTool(strings::fill(
            "server-not-started",
            &[&job.server_exe.display(), &e],
        ))
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
                Ok(Some(ChildEvent::Progress {
                    url,
                    downloaded,
                    total,
                })) => {
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
        return Err(HfError::Protocol(strings::fill(
            "protocol-server-said",
            &[&e, &tail_text(&lines)],
        )));
    }
    match result {
        Some(ChildEvent::Finished) => Ok(lines),
        _ if cancelled => Err(HfError::Cancelled),
        Some(_) => Err(classify_failure(&lines, job.token.is_some(), &job.repo)),
        None => Err(HfError::Protocol(strings::fill(
            "server-ended-no-result",
            &[
                &exit.map_or_else(|| strings::t("server-killed"), |s| s.to_string()),
                &tail_text(&lines),
            ],
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
        Ok(m) => Err(HfError::TargetConflict {
            path: target.to_path_buf(),
            found: m.len(),
            expected,
        }),
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
            && repo_dir
                .join("blobs")
                .join(format!("{}.downloadInProgress", f.oid))
                .is_file();
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
    let rel = |c: &str| {
        repo_path
            .split('/')
            .fold(snaps.join(c), |p, seg| p.join(seg))
    };
    let mut commits: Vec<String> = cached_commit(repo_dir).into_iter().collect();
    commits.push(listing_commit.to_string());
    if let Ok(rd) = std::fs::read_dir(&snaps) {
        commits.extend(
            rd.flatten()
                .map(|e| e.file_name().to_string_lossy().into_owned()),
        );
    }
    commits
        .iter()
        .map(|c| rel(c))
        .find(|p| std::fs::symlink_metadata(p).is_ok())
}

/// `path` with its `.` and `..` components folded away, by text alone (no
/// filesystem access, so no link is followed). `None` when a `..` would climb
/// above the start.
fn lexical_normal(path: &Path) -> Option<PathBuf> {
    use std::path::Component;
    let mut out = PathBuf::new();
    let mut depth = 0usize;
    for c in path.components() {
        match c {
            Component::CurDir => {}
            Component::ParentDir => {
                if depth == 0 {
                    return None;
                }
                out.pop();
                depth -= 1;
            }
            Component::Normal(n) => {
                out.push(n);
                depth += 1;
            }
            other => out.push(other.as_os_str()),
        }
    }
    Some(out)
}

/// Whether `path` stays under `root` once both are folded (`lexical_normal`):
/// the guard between a remote-supplied name and the place a file is moved
/// from or to.
fn lexically_under(path: &Path, root: &Path) -> bool {
    match (lexical_normal(path), lexical_normal(root)) {
        (Some(p), Some(r)) => p.starts_with(&r),
        _ => false,
    }
}

/// Move a staged snapshot entry's DATA to `dst`: a symlink is resolved to its
/// blob and the blob moved (the link would dangle once staging is removed); a
/// plain file is moved as is. Rename first; across volumes, a copy to a
/// temporary name BESIDE `dst` (`<name>.partial`) renamed into place only once
/// complete, so a failed copy never leaves a truncated file at the target
/// (where the next attempt would refuse it as a `TargetConflict`). The source
/// is removed after the target exists.
///
/// The entry and the data it resolves to must both stay under `staging`
/// (`lexically_under`), or nothing is moved: a link pointing out of the
/// staging tree would otherwise move (and then delete) a file of the user's.
fn move_real_data(src: &Path, dst: &Path, staging: &Path) -> std::io::Result<()> {
    let outside = |p: &Path| {
        std::io::Error::new(
            std::io::ErrorKind::PermissionDenied,
            strings::fill("outside-staging", &[&p.display(), &staging.display()]),
        )
    };
    if !lexically_under(src, staging) {
        return Err(outside(src));
    }
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
    if !lexically_under(&data, staging) {
        return Err(outside(&data));
    }
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
fn place_group(
    job: &DownloadJob,
    group: &[&Placement],
    child_log: &[String],
    tracker: &mut Tracker,
) -> Result<(), HfError> {
    let repo_dir = job.staging().join(repo_folder(&job.repo));
    for p in group {
        if target_present(&p.target, p.file.size)? {
            tracker.complete(&p.file.path);
            continue;
        }
        let Some(src) = find_staged(&repo_dir, &job.commit, &p.file.path) else {
            return Err(
                match classify_failure(child_log, job.token.is_some(), &job.repo) {
                    HfError::Download(tail) => HfError::Download(strings::fill(
                        "success-but-missing",
                        &[&p.file.path, &repo_dir.display(), &tail],
                    )),
                    e => e,
                },
            );
        };
        move_real_data(&src, &p.target, &repo_dir).map_err(|e| {
            HfError::Io(strings::fill(
                "moving-failed",
                &[&p.file.path, &p.target.display(), &e],
            ))
        })?;
        let got = std::fs::metadata(&p.target).map(|m| m.len()).unwrap_or(0);
        if p.file.size > 0 && got != p.file.size {
            // A wrong-sized file left at the target would be refused as a
            // TargetConflict by every retry: it goes, and the retry fetches
            // it again.
            let _ = std::fs::remove_file(&p.target);
            return Err(HfError::Io(strings::fill(
                "size-mismatch-removed",
                &[&p.file.path, &got, &p.file.size],
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
    let placements = job.placements()?;
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

    let target = |role: Role| {
        placements
            .iter()
            .find(|p| p.role == role)
            .map(|p| p.target.clone())
    };
    Ok(Downloaded {
        repo: job.repo.clone(),
        project_url: job.project_url.clone(),
        model: target(Role::Model).unwrap_or_default(),
        mmproj: target(Role::Mmproj),
        draft: job
            .sidecar
            .and_then(|s| target(Role::Draft(s)).map(|t| (s, t))),
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

// ── The model profile ───────────────────────────────────────────────────────────

/// The model profile a download becomes (pure: the headers are the caller's to read,
/// `model_profile_from_download` does it). `model` is the model's header, `None` when
/// it could not be read (no ggml-base), in which case only the paths and the
/// URL are filled and every key the header would justify stays at its default.
/// `draft_header_spec` is `gguf::draft_spec_type` of the drafter.
///
/// What it sets, and why (everything else stays `ModelProfile::default()`, the New
/// dialog's model profile: a new model profile pins only what has a reason, README "A new
/// model profile leaves every tunable at default"):
///
/// | field | from | reason |
/// |---|---|---|
/// | `model` | the first shard's final path | what the model profile loads |
/// | `project_url` | the listing's model page | the Models tab's Open button |
/// | `mmproj` | the mmproj's final path, when fetched | `llama-server -hf` would load it |
/// | `model_draft` + `spec_type` | the drafter's path; its HEADER's verdict, else the sidecar's prefix | the user asked for it; the header tells DSpark from DFlash, which a name cannot (`gguf::draft_spec_type`) |
/// | `temp`, `top_k`, `top_p`, `min_p`, `repeat_penalty` | `general.sampling.*` | llama-server already applies these from the file (`common_init_sampler_from_model`); written, they show in the form as the values in force instead of "default" |
/// | `jinja` | `off` for an embedded NON-Jinja template, else `on` | `--jinja` is upstream's default, and with it a legacy template string would be rendered as Jinja text; only the legacy route reads it |
/// | `fit` | `on` unless the placement sets the context | llama.cpp's own default, and the one setting that makes an untuned model profile load: context and GPU layers stay unset (`model_profiles::fit_allowed`), so without it the model's TRAINED context (often 128k-256k) and every layer would be allocated on whatever card this is |
/// | `device`, `tensor_split`, `n_cpu_moe`, `n_gpu_layers`, `ctx_size`, `device_draft`, `mmproj_device`, `mmproj_offload`, `placement_note` | `placement`: `hf_fit::placement` of the downloaded files' estimate on this machine, the mmproj and the drafter fetched counted (`local_extras`), `None` without a GPU estimate | the split per card, or the CPU part, the dialog showed for this quantization with these extras (the table in `hf_fit`'s header says which keys in which case); `fit` turns off with `ctx_size` set, as the form would make it, so the cut is not resized against the TOTAL memory (`common/fit.cpp`); the mmproj keys only with an mmproj, `device_draft` only with a drafter |
///
/// And what the header could justify but is left alone, on purpose:
/// - `reasoning` stays `auto` whatever the thinking verdict (`gguf` `Thinking`):
///   `auto` is the template's own default for a switchable one, `off` is inert
///   on one that thinks unconditionally, and on a template with no marker it
///   would only cost the parser on a model that thinks without one (MiMo-VL).
/// - Without a placement (no GPU estimate, a one-card machine the model fits,
///   or one it cannot be placed on at all): `ctx_size`, `n_gpu_layers`,
///   `n_cpu_moe`, `tensor_split` stay unset and the fit sizes context and
///   placement for the machine. `n_cpu_ffn` and `moe_cache_mib` are never
///   set: tuning levers, not estimates.
/// - Embedded MTP heads (`nextn_predict_layers`) do not switch `spec-type` on:
///   llama.cpp's default is no speculation, the MTP context costs VRAM, and
///   the user can pick it in one click.
/// - `image_min_tokens`: llama.cpp only WARNS (Qwen-VL grounding) and the bound
///   costs tokens on every image; a per-use choice.
/// - A separate drafter gets a `device_draft` pin only from a placement that
///   splits the model over several cards (the last one: an empty pin would
///   inherit the whole list, and a multi-device auto fit crashes separate MTP
///   heads, AGENTS.md "Draft placement needs a draft FILE"), or that keeps
///   the model alone on one card by counting the drafter on another. With
///   the model and the drafter on one card, the inherited single device is
///   already the pin; `n_gpu_layers_draft` stays unset, the CPU fallback
///   (`0`) would throw away the speedup the user asked for.
///
/// The result passes `prune_inactive_draft_keys` / `prune_inactive_moe_cache`
/// unchanged (they run here too, against the header, so a future field cannot
/// slip past them).
pub fn derive_model_profile(
    id: String,
    d: &Downloaded,
    model: Option<&gguf::ModelInfo>,
    draft_header_spec: Option<&str>,
    placement: Option<&hf_fit::Placement>,
) -> ModelProfile {
    let s = |p: &Path| p.to_string_lossy().into_owned();
    let mut p = ModelProfile::new_default(id, s(&d.model));
    p.project_url = d.project_url.clone();
    p.mmproj = d.mmproj.as_deref().map(s).unwrap_or_default();
    if let Some((kind, path)) = &d.draft {
        p.model_draft = s(path);
        p.spec_type = draft_header_spec.unwrap_or(kind.spec_type()).to_string();
    }
    if let Some(pl) = placement {
        p.device = pl.device.clone();
        p.tensor_split = pl.tensor_split.clone();
        p.n_cpu_moe = pl.n_cpu_moe;
        p.n_gpu_layers = pl.n_gpu_layers;
        p.ctx_size = pl.ctx_size;
        if d.draft.is_some() {
            p.device_draft = pl.device_draft.clone();
        }
        if d.mmproj.is_some() {
            p.mmproj_device = pl.mmproj_device.clone();
            if let Some(off) = pl.mmproj_offload {
                p.mmproj_offload = Some(off);
            }
        }
        p.placement_note = pl.note.clone();
    }
    p.fit = crate::model_profiles::fit_allowed(p.ctx_size.is_none(), p.n_gpu_layers.is_none());
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
        model_profiles::prune_inactive_draft_keys(&mut p, m.nextn_predict_layers > 0);
        model_profiles::prune_inactive_moe_cache(&mut p, m.is_moe);
    }
    p
}

/// `derive_model_profile` over the downloaded files' own headers, with the id the
/// Models tab's New would give the file (`model_profiles::new_model_profile_id`: the file
/// name, `model-profile` when it sanitizes to nothing, made unique ignoring case
/// against `existing_ids`; pass every listed model profile, the parked ones included).
/// `budget` is the machine the dialog or `--list` estimated against
/// (`hf_fit::budget`), `None` without a GPU: the placement is then left out,
/// as it is when the downloaded header cannot be read.
pub fn model_profile_from_download(
    d: &Downloaded,
    existing_ids: &[String],
    budget: Option<&hf_fit::Budget>,
) -> ModelProfile {
    let id = model_profiles::new_model_profile_id(&d.model.to_string_lossy(), existing_ids);
    let info = gguf::read_model_info(&d.model);
    let spec = d.draft.as_ref().and_then(|(_, p)| gguf::draft_spec_type(p));
    let placement = budget.and_then(|b| {
        let shape = hf_fit::local_shape(&d.model).ok()?;
        hf_fit::placement(&hf_fit::estimate_with(&shape, b, &local_extras(d)), &shape)
    });
    derive_model_profile(id, d, info.as_ref(), spec, placement.as_ref())
}

/// The extras of a download as the estimate counts them (`hf_fit::Extras`):
/// the mmproj's size on disk, the drafter's own header (its size alone when
/// unreadable). A file that cannot be stat'ed counts nothing.
pub fn local_extras(d: &Downloaded) -> hf_fit::Extras {
    let size = |p: &Path| std::fs::metadata(p).map_or(0, |m| m.len());
    hf_fit::Extras {
        mmproj: d.mmproj.as_deref().map_or(0, size),
        drafter: d.draft.as_ref().map(|(kind, path)| {
            let shape = hf_fit::local_draft_shape(path)
                .unwrap_or_else(|_| hf_fit::DraftShape::from_size(size(path)));
            (format!("{} drafter", kind.label()), shape)
        }),
    }
}

/// Save a derived model profile, first pinning server.ini's ModelsDir to the one the
/// files were placed under when it is unset. `model_profiles::save` would otherwise
/// seed it from the model path (`infer_models_dir`), and the per-repo folder
/// (`models\<org>\<repo>\`) makes that guess a folder below the real root, which
/// the pickers would then scan for a `models` subfolder that is not there. The
/// model profile is validated FIRST, so a model profile that cannot be saved changes nothing
/// in server.ini either. Touches `paths::`, so it has no unit test (README,
/// Tests); `derive_model_profile` is the tested part.
pub fn save_model_profile(p: &ModelProfile, models_dir: &Path) -> std::io::Result<()> {
    model_profiles::validate_for_save(p)?;
    let cfg = crate::server_cfg::load().unwrap_or_default();
    let kind = crate::server_control::backend().kind();
    if crate::server_cfg::seeds_models_dir(&cfg, kind, crate::server_control::Os::current()) {
        model_profiles::set_server_key("ModelsDir", &models_dir.to_string_lossy())?;
    }
    model_profiles::save(p)
}

/// Create the download's model profile `p` (the fit's placement filled in by
/// `model_profile_from_download`) through `CreateModelProfile`'s rules: the
/// write to `models.ini` carries the stay keys
/// (`pai_core::checks::MODEL_PROFILE_STAY_KEYS`) only, never a moved key
/// (`PLACEMENT_KEYS`). When `instance_profile` names one, the model is added
/// to it (`add_instance_profile_model`) and the fit's placement keys are
/// saved into the entry `[<profile>/<id>]` (`set_instance_profile_model`);
/// without it the placement is the caller's to print as advice. `models_dir`
/// is the folder the files landed in, for the ModelsDir seed that is not the
/// manager's: `Direct` (macOS) has no manager and no pool of the node, so the
/// per-user save is kept there, the profile's own keys and all.
/// `Ok(None)`: created, the entry saved or not asked for.
/// `Ok(Some(refusal))`: created, the entry refused, the caller says so.
/// `Err`: nothing created.
pub fn save_downloaded_profile(
    p: &ModelProfile,
    models_dir: &Path,
    instance_profile: Option<&str>,
) -> Result<Option<pai_core::client::ClientError>, pai_core::client::ClientError> {
    use pai_core::checks;
    use pai_core::client::ModelProfileSource;
    use pai_core::ini;
    let files = crate::server_control::backend();
    // `Direct` (macOS) has no manager, no instance profile pool and no
    // `models.ini` of the node: the download keeps the per-user write there,
    // the profile's own keys and all.
    if files.kind() == crate::server_control::Kind::Direct {
        return save_model_profile(p, models_dir)
            .map(|()| None)
            .map_err(|e| pai_core::client::ClientError::Other(e.to_string()));
    }
    // The section's keys, as a save renders them, split the way the migration
    // split them: the moved keys go to the entry, the stay keys stay.
    let rendered = model_profiles::render_section(p);
    let keys = ini::section_keys(&rendered, &p.id);
    let mut values: Vec<(String, String)> = Vec::new();
    let mut placement: Vec<(String, String)> = Vec::new();
    for (key, value) in keys {
        if checks::placement_key(&key) {
            placement.push((key, value));
        } else {
            values.push((key, value));
        }
    }
    // One source key: the repository the download names carries the file and
    // its provenance, so the file's own path rides in no create (the manager
    // refuses a source that names two).
    if values.iter().any(|(key, _)| key == "hf-repo") {
        values.retain(|(key, _)| key != "model");
    }
    files.create_model_profile(&p.id, &ModelProfileSource::Keys(values))?;
    let Some(entry) = instance_profile else {
        return Ok(None);
    };
    // The entry first empty, then whole: `SetInstanceProfileModel` saves the
    // entry it is given, so the placement of a fit that found nothing leaves
    // the entry with no key, and the model stays listed.
    if let Err(e) = files.add_instance_profile_model(entry, &p.id) {
        return Ok(Some(e));
    }
    if placement.is_empty() {
        return Ok(None);
    }
    let pool = match files.instance_profile_pool(entry) {
        Ok(pool) => pool,
        Err(e) => return Ok(Some(e)),
    };
    Ok(files
        .set_instance_profile_model(entry, &p.id, &placement, &pool)
        .err())
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
#[path = "hf_tests.rs"]
mod tests;
