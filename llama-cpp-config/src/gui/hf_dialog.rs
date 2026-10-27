//! The Models tab's "New preset from Hugging Face" dialog: the New preset
//! dialog's second source, beside picking a file already in ModelsDir.
//!
//! The user types a model name and searches (`hf::search`: GGUF repositories
//! by downloads, each row with its quantizations), picks a repository, or
//! types one directly (a spec or its URL); its GGUF quantizations are listed
//! (`hf::list_repo`), the user picks one with its sidecars and the token, and
//! Download hands the job to the background (`downloads::start`) and closes
//! the dialog: the transfer, the preset's creation from the files and the
//! progress all live in `gui/downloads.rs` and its window, so the main window
//! is usable again at once.
//!
//! ## One field, two requests
//! `AppState.hf_input` holds either a search or a repository, told apart by
//! `hf::classify_input` (a spec is anything `parse_spec` accepts, so
//! `org/repo` never costs a search round trip; the rule is in `hf.rs`'s
//! header). The button follows the field on every keystroke ("Search" or
//! "List", via the pure `hf_input_is_query`), and `hf_submit` dispatches.
//!
//! ## Phases (`Phase`, mirrored in `AppState.hf_phase`)
//! `Idle` (typing) → Search → `Searching` → `Searched` (result rows shown) →
//! click a row → `Listing` → `Listed` (candidates shown, one picked) →
//! Download, which closes the dialog (back to `Idle`). A typed spec skips the
//! first two: `Idle` → List → `Listing`. Picking a row puts its `org/repo` in
//! the field and keeps the results (`Session::results`): **Back to results**
//! returns to `Searched` with the query back in the field, from `Listing` too
//! (the listing in flight is dropped by the generation). A typed List or a new
//! Search drops them. Download is refused IN the dialog (the listing kept)
//! when the job cannot be built (a sidecar the pick lacks, no llama-server, a
//! token that cannot be one) or when the same repository is already
//! downloading (`downloads::Downloads::busy_with`: one staging tree per
//! repository). The button reads "Resume download" only when the PICKED
//! candidate has a real partial to resume (`hf::has_partial`: a
//! `.downloadInProgress` of one of its own files, re-checked on every pick); a
//! partial of another quant of the repo does not count, since this job would
//! not read it. A failed search returns to `Idle`, a failed listing to `Searched` when
//! it came from a row (the results stay) and to `Idle` otherwise. One request
//! runs at a time: the button is disabled while a search or a listing runs
//! (`Phase::can_request`). Every request still carries a generation, bumped by
//! a dismiss and by Back too, so the answer of a request whose dialog was
//! closed (and perhaps reopened) is dropped (the curl it ran finishes on its
//! own thread, bounded by its own `--max-time`).
//!
//! ## The model marker, the placement estimate, the recommendation
//! A search row's first line carries `MoE 30.5B` / `dense 8.2B` when the
//! search's `gguf` summary tells (`hf::kind_label`; nothing is guessed). Once
//! a repository is listed, every candidate row gets a second line: its
//! placement on THIS machine (`hf_fit`: all on GPU, an MoE split with its
//! `--n-cpu-moe`, a dense split, too big, and whether the RAM part pages from
//! disk; with several cards, the card or the per-card cut and its
//! `tensor-split`), and a basis line under the title says what that is
//! measured against (each card in split order, the RAM, the margins). The
//! download's preset gets that placement as keys (`hf_fit::placement`,
//! against the same `Session::budget`, so the preset holds the cut the picked
//! row showed) and the status line says it is an estimate. The estimates come
//! from each candidate's GGUF header, read remotely by `hf_fit::fetch_shapes`
//! on its own threads (`spawn_estimates`), so the listing is usable at once:
//! rows read "estimating" and fill in one by one (`estimated`), stale answers
//! dropped by the same generation as every other request, and a superseded
//! listing stops its reads early (`ESTIMATE_RUN`). Once every row has its
//! answer, `hf_fit::recommend` marks one row "recommended" with its reason,
//! and the pick moves to it unless the user already clicked a row (`select`
//! sets `Session::user_picked`; `pick` is the move itself): while estimates
//! load or when none can be made, llama.cpp's default stays the pick. No GPU
//! in the device probe: every row says the estimate is unavailable and
//! nothing is fetched.
//!
//! The estimates count the extras the download would bring (`hf_fit`'s
//! header, "The extras"): the mmproj tick and the drafter pill are WISHES
//! applied to every row that has the file (`Session::wish_mmproj`,
//! `wish_drafter`), kept across picks (a row without the file shows the
//! tick off or the pill on none, and the wish comes back on a row that has
//! it). Toggling either (`choices_changed`) re-estimates every row and the
//! recommendation from the headers already read (`recompute`, no fetch);
//! only a drafter kind not read yet fetches its headers (`spawn_drafts`,
//! one per file), its rows "estimating" until they land (`drafted`).
//!
//! ## No transfer runs IN the dialog
//! Esc, the backdrop and Close go to `hf_dismiss`, which closes the dialog in
//! every phase: once Download is clicked the job belongs to `gui/downloads.rs`
//! (Cancel, Resume, the progress, the preset, Quit asking first), so closing
//! the dialog can never drop gigabytes. The dialog's Downloads button opens
//! that window (the modal backdrop covers the footer's indicator).
//!
//! ## The token
//! `AppState.hf_token` holds only what the user TYPES (masked). Empty means
//! the environment's token (`hf::token_from_env`), read here at request time
//! and never copied into the UI; the field's placeholder only says that one
//! exists. Neither form reaches settings.ini, a log, a status line or a
//! preset: `hf::Token` has no `Display` and `HfError` never carries it.
//!
//! ## Threading
//! The slow calls (search, listing, header reads) run on their own threads
//! and come back through `slint::invoke_from_event_loop` + `Weak::upgrade`
//! (the `spawn_device_probe` pattern). The landing closure must be `Send`, so
//! it cannot carry the `Rc<RefCell<State>>`: it reaches it through `STATE`, a
//! thread-local weak handle `wire` sets on the UI thread (the only thread those
//! closures run on), which `gui/downloads.rs` uses too (`shared_state`).
//!
//! The pure parts (phases, input check, result and candidate labels, summary)
//! are unit-tested below; the e2e flow test (`src/tests/save_flow.rs`) drives
//! the wiring that needs no network, the hand-over to the background included.

use super::*;

use std::collections::{HashMap, HashSet};
use std::path::Path;

use crate::hf;
use crate::hf_fit;

/// The dialog's phase. Its `i32` is `AppState.hf_phase` (state.slint reads it
/// for the button gates), so the numbering is part of the UI contract.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum Phase {
    Idle = 0,
    Searching = 1,
    Searched = 2,
    Listing = 3,
    Listed = 4,
}

impl Phase {
    fn from_int(v: i32) -> Phase {
        match v {
            1 => Phase::Searching,
            2 => Phase::Searched,
            3 => Phase::Listing,
            4 => Phase::Listed,
            _ => Phase::Idle,
        }
    }

    /// A search or a listing is in flight (`AppState.hf_busy`).
    fn busy(self) -> bool {
        matches!(self, Phase::Searching | Phase::Listing)
    }

    /// Search, List and a result pick are open in every phase but a request
    /// in flight (app.slint disables the button then, and this mirrors it).
    fn can_request(self) -> bool {
        !self.busy()
    }

    /// The result rows show instead of the candidates (`AppState.hf_show_results`).
    fn shows_results(self) -> bool {
        (self as i32) <= Phase::Searched as i32
    }
}

/// The Rust half of the dialog (`gui::State.hf`): what Slint cannot hold.
#[derive(Default)]
pub(super) struct Session {
    /// Bumped by every request, every Back and every dismiss: an answer
    /// stamped with an older one is stale and dropped.
    generation: u64,
    /// The last search's query and rows, kept while a picked row is listed
    /// (for Back to results); empty when the listing came from a typed spec.
    query: String,
    results: Vec<hf::SearchHit>,
    listing: Option<hf::Listing>,
    /// What the listing's estimates are measured against; `None` without a
    /// GPU (every row then says so, and nothing is fetched).
    budget: Option<hf_fit::Budget>,
    /// Per candidate: `None` while its header is being read, then its shape
    /// or why not. Kept, so a change of choices re-estimates without a fetch.
    shapes: Vec<Option<Result<std::sync::Arc<hf_fit::Shape>, String>>>,
    /// The drafter files' shapes read so far, by repository path (this
    /// listing's), and the ones being read.
    drafts: HashMap<String, Result<std::sync::Arc<hf_fit::DraftShape>, String>>,
    drafts_pending: HashSet<String>,
    /// The extras the user asked for, applied to every row: the mmproj
    /// (where a row has one) and the drafter kind (where a row has it). Set
    /// by the tick and the pills (`choices_changed`) and kept across picks,
    /// so the estimates, the recommendation and the pick agree.
    wish_mmproj: bool,
    wish_drafter: Option<hf::Sidecar>,
    /// Per candidate: `None` while its header (or its drafter's) is being
    /// read, then its estimate with the extras wished for (with the context
    /// the KV cache was counted for) or why not (`recompute`).
    estimates: Vec<Option<Result<(hf_fit::Estimate, u64), String>>>,
    /// The recommended candidate, once every row has its answer.
    recommended: Option<hf_fit::Recommendation>,
    /// The user clicked a candidate in this listing: the recommendation
    /// then marks its row but no longer moves the pick.
    user_picked: bool,
}

/// The listing whose header reads may go on: bumped by every new listing
/// and every clear, read by the workers (`hf_fit::fetch_shapes`'s
/// `keep_going`) so a superseded listing stops fetching early.
static ESTIMATE_RUN: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);

thread_local! {
    /// The shared `State`, for the worker results landing on the event loop
    /// (see the module header, Threading).
    static STATE: RefCell<std::rc::Weak<RefCell<State>>> = const { RefCell::new(std::rc::Weak::new()) };
}

/// The shared `State` from a worker's landing closure (`gui/downloads.rs`
/// lands through it too); `None` once the GUI is gone.
pub(super) fn shared_state() -> Option<Rc<RefCell<State>>> {
    STATE.with(|s| s.borrow().upgrade())
}

// ── Pure parts ───────────────────────────────────────────────────────────

/// The field's error as the hint under it, "" for a good spec, a query, or an
/// empty field (which just keeps the button disabled).
fn input_problem(text: &str) -> String {
    match hf::classify_input(text) {
        Ok(_) => String::new(),
        Err(e) => sentence(&e.to_string()),
    }
}

/// The field holds a search (the button reads "Search"), not a repository.
fn input_is_query(text: &str) -> bool {
    matches!(hf::classify_input(text), Ok(Some(hf::Input::Query(_))))
}

/// A search row's first line: the repository, the model marker when the
/// search knows it (`MoE 30.5B`, `dense 8.2B`, `hf::kind_label`), its
/// downloads, and whether it is gated or private (both need a token to
/// download).
fn result_label(h: &hf::SearchHit) -> String {
    let mut parts = vec![h.repo.clone()];
    parts.extend(hf::kind_label(h.kind, h.params));
    parts.push(format!("{} downloads", hf::format_count(h.downloads)));
    if h.gated {
        parts.push("gated".into());
    }
    if h.private {
        parts.push("private".into());
    }
    parts.join("  ·  ")
}

/// A search row's second line: the quantizations its file names show, and
/// the sidecars (the list's own wording, `candidate_label`).
fn result_detail(h: &hf::SearchHit) -> String {
    let quants = if h.quants.is_empty() { "no GGUF model file".to_string() } else { h.quants.join(", ") };
    let mut with: Vec<&str> = Vec::new();
    if h.mmproj {
        with.push("mmproj");
    }
    with.extend(h.drafters.iter().map(|s| s.label()));
    if with.is_empty() {
        quants
    } else {
        format!("{quants}  ·  with {}", with.join(", "))
    }
}

/// The line over the result rows.
fn results_title(query: &str, n: usize, limit: usize) -> String {
    match n {
        0 => format!("No GGUF repository matches \"{query}\"."),
        _ if n >= limit => format!(
            "The {n} most downloaded GGUF repositories for \"{query}\" (add words to narrow it); downloads over the last 30 days"
        ),
        1 => format!("1 GGUF repository for \"{query}\"; downloads over the last 30 days"),
        _ => format!("{n} GGUF repositories for \"{query}\", most downloaded first; downloads over the last 30 days"),
    }
}

/// Upper-case the first letter: `HfError`'s lines are written to follow a
/// prefix, the dialog (and a Downloads row) shows them on their own.
pub(super) fn sentence(s: &str) -> String {
    let mut c = s.chars();
    match c.next() {
        Some(f) => f.to_uppercase().chain(c).collect(),
        None => String::new(),
    }
}

/// An `HfError` as the dialog shows it. The two that tell the user to set
/// `HF_TOKEN` also name the field this dialog has for it.
fn dialog_error(e: &hf::HfError) -> String {
    let text = sentence(&e.to_string());
    match e {
        hf::HfError::NotFoundOrPrivate { .. } | hf::HfError::Gated { had_token: false, .. } => {
            format!("{text}, or paste a token in the Access token field.")
        }
        _ => text,
    }
}

/// The token a request carries: the typed one when there is one (refused if
/// it cannot be a token), else `env()` (the environment's, possibly none).
fn request_token(
    typed: &str,
    env: impl FnOnce() -> Option<hf::Token>,
) -> Result<Option<hf::Token>, hf::HfError> {
    if typed.trim().is_empty() {
        Ok(env())
    } else {
        hf::Token::new(typed).map(Some)
    }
}

/// One row of the candidate list: quant, size, shards, sidecars, and marks on
/// the one `llama-server -hf` would pick and on the one recommended for this
/// machine (`hf_fit::recommend`).
fn candidate_label(c: &hf::Candidate, is_default: bool, is_recommended: bool) -> String {
    let name = if c.tag.is_empty() { c.primary().name().to_string() } else { c.tag.clone() };
    let mut parts = vec![name, hf::format_size(c.size)];
    if c.shards.len() > 1 {
        parts.push(format!("{} shards", c.shards.len()));
    }
    let mut with: Vec<&str> = Vec::new();
    if c.mmproj.is_some() {
        with.push("mmproj");
    }
    for s in hf::Sidecar::ALL {
        if c.sidecar(s).is_some() {
            with.push(s.label());
        }
    }
    if !with.is_empty() {
        parts.push(format!("with {}", with.join(", ")));
    }
    if is_default {
        parts.push("default".into());
    }
    if is_recommended {
        parts.push("recommended".into());
    }
    parts.join("  ·  ")
}

/// What the estimate line of a candidate row says while it has no estimate.
const ESTIMATING: &str = "estimating: reading the GGUF header…";

/// A candidate row's second line: its placement estimate (or why there is
/// none), led by the recommendation when it is the one.
fn candidate_detail(slot: &Option<Result<(hf_fit::Estimate, u64), String>>, recommended: Option<&str>) -> String {
    let label = match slot {
        None => ESTIMATING.to_string(),
        Some(Ok((e, ctx))) => hf_fit::estimate_label(e, *ctx),
        Some(Err(m)) => format!("estimate unavailable: {m}"),
    };
    hf_fit::detail_line(&label, recommended)
}

/// The drafter pills for a candidate: "none" plus each sidecar it has, or
/// nothing at all when it has none (the pills are then hidden).
fn drafter_options(c: &hf::Candidate) -> Vec<String> {
    let have: Vec<String> = hf::Sidecar::ALL
        .into_iter()
        .filter(|s| c.sidecar(*s).is_some())
        .map(|s| s.label().to_string())
        .collect();
    if have.is_empty() {
        return have;
    }
    std::iter::once("none".to_string()).chain(have).collect()
}

/// The pill's value as the job's sidecar ("none", or anything unknown, is no
/// drafter).
fn drafter_choice(value: &str) -> Option<hf::Sidecar> {
    hf::Sidecar::parse(value)
}

/// What the pick would fetch: file count, total size, the files.
fn selection_summary(c: &hf::Candidate, mmproj: bool, drafter: Option<hf::Sidecar>) -> String {
    let mut files: Vec<String> = Vec::new();
    let mut size = c.size;
    let mut count = c.shards.len();
    files.push(if c.shards.len() > 1 {
        format!("{} ({} shards)", c.primary().name(), c.shards.len())
    } else {
        c.primary().name().to_string()
    });
    if mmproj {
        if let Some(m) = &c.mmproj {
            files.push(m.name().to_string());
            size += m.size;
            count += 1;
        }
    }
    if let Some(f) = drafter.and_then(|s| c.sidecar(s)) {
        files.push(f.name().to_string());
        size += f.size;
        count += 1;
    }
    let noun = if count == 1 { "file" } else { "files" };
    format!("Fetches {count} {noun}, {}: {}", hf::format_size(size), files.join(", "))
}

/// The line over the candidate list.
fn listing_title(l: &hf::Listing) -> String {
    let commit = &l.commit[..12.min(l.commit.len())];
    let n = l.candidates.len();
    let noun = if n == 1 { "quantization" } else { "quantizations" };
    format!("{} @ {commit}: {n} {noun}", l.repo)
}

/// The refusal of a second download of a repository already downloading.
fn busy_refusal(repo: &str) -> String {
    format!(
        "{repo} is already downloading (Downloads window): one download per repository at a time, \
         since both would write its staging folder. Start this one when that one ends."
    )
}

/// A Downloads row's title: the repository and the quantization.
fn job_title(repo: &str, c: &hf::Candidate) -> String {
    let quant = if c.tag.is_empty() { c.primary().name().to_string() } else { c.tag.clone() };
    format!("{repo}  ·  {quant}")
}

// ── Wiring ───────────────────────────────────────────────────────────────

fn phase(app: &AppWindow) -> Phase {
    Phase::from_int(app.global::<AppState>().get_hf_phase())
}

fn set_phase(app: &AppWindow, p: Phase) {
    app.global::<AppState>().set_hf_phase(p as i32);
}

/// Empty every listing-derived property (a new List, a failed one, a reopen).
fn clear_listing(app: &AppWindow) {
    let s = app.global::<AppState>();
    ESTIMATE_RUN.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    s.set_hf_listing_title(SharedString::default());
    s.set_hf_estimate_basis(SharedString::default());
    s.set_hf_candidate_labels(string_model(Vec::new()));
    s.set_hf_candidate_details(string_model(Vec::new()));
    s.set_hf_candidate_index(-1);
    s.set_hf_mmproj_available(false);
    s.set_hf_drafter_options(string_model(Vec::new()));
    s.set_hf_drafter(SharedString::from("none"));
    s.set_hf_selection_summary(SharedString::default());
    s.set_hf_resume(false);
}

fn models_dir() -> PathBuf {
    PathBuf::from(server_cfg::load().models_dir_or_default())
}

/// Recompute the summary line from the current pick and choices.
fn refresh_summary(app: &AppWindow, state: &Rc<RefCell<State>>) {
    let s = app.global::<AppState>();
    let st = state.borrow();
    let c = usize::try_from(s.get_hf_candidate_index())
        .ok()
        .and_then(|i| st.hf.listing.as_ref()?.candidates.get(i));
    let text = c.map_or_else(String::new, |c| {
        selection_summary(c, s.get_hf_take_mmproj(), drafter_choice(s.get_hf_drafter().as_str()))
    });
    s.set_hf_selection_summary(SharedString::from(text));
}

/// The user clicked candidate `index`: from now on the recommendation no
/// longer moves the pick.
fn select(app: &AppWindow, state: &Rc<RefCell<State>>, index: i32) {
    state.borrow_mut().hf.user_picked = true;
    pick(app, state, index);
}

/// Pick candidate `index`: its sidecar choices (the wishes where it has the
/// files: on a fresh listing the mmproj on when it has one and no drafter,
/// as `llama-server -hf` does) and the summary.
fn pick(app: &AppWindow, state: &Rc<RefCell<State>>, index: i32) {
    let s = app.global::<AppState>();
    let (picked, wish_mmproj, wish_drafter) = {
        let st = state.borrow();
        let c = usize::try_from(index).ok().and_then(|i| st.hf.listing.as_ref()?.candidates.get(i).cloned());
        (c, st.hf.wish_mmproj, st.hf.wish_drafter)
    };
    let Some(c) = picked else {
        return;
    };
    s.set_hf_candidate_index(index);
    s.set_hf_mmproj_available(c.mmproj.is_some());
    s.set_hf_take_mmproj(c.mmproj.is_some() && wish_mmproj);
    s.set_hf_drafter_options(string_model(drafter_options(&c)));
    let drafter = wish_drafter.filter(|k| c.sidecar(*k).is_some()).map_or("none", |k| k.label());
    s.set_hf_drafter(SharedString::from(drafter));
    refresh_summary(app, state);
    refresh_resume(app, state, &models_dir());
}

/// The tick or a pill changed: the summary, the wishes (from a pick that
/// offers the choice), and every row's estimate and the recommendation
/// again (`recompute`, fetching only drafter headers not read yet).
fn choices_changed(app: &AppWindow, state: &Rc<RefCell<State>>) {
    refresh_summary(app, state);
    let s = app.global::<AppState>();
    {
        let mut st = state.borrow_mut();
        let c = usize::try_from(s.get_hf_candidate_index())
            .ok()
            .and_then(|i| st.hf.listing.as_ref()?.candidates.get(i).cloned());
        let Some(c) = c else {
            return;
        };
        if c.mmproj.is_some() {
            st.hf.wish_mmproj = s.get_hf_take_mmproj();
        }
        if !drafter_options(&c).is_empty() {
            st.hf.wish_drafter = drafter_choice(s.get_hf_drafter().as_str());
        }
    }
    spawn_drafts(app, state);
    reestimate(app, state);
}

/// `recompute`, then the rows, then the pick moved to a new recommendation
/// unless the user chose a row.
fn reestimate(app: &AppWindow, state: &Rc<RefCell<State>>) {
    let move_to = recompute(&mut state.borrow_mut().hf);
    refresh_rows(app, state);
    if let Some(i) = move_to {
        let s = app.global::<AppState>();
        if phase(app) == Phase::Listed && s.get_hf_candidate_index() != i {
            pick(app, state, i);
        }
    }
}

/// Every row's estimate from the shapes read so far and the wishes, then
/// the recommendation once every row has its answer (pure over the
/// session). Returns where the pick should move: the recommendation, unless
/// the user already chose a row.
fn recompute(h: &mut Session) -> Option<i32> {
    let (Some(l), Some(b)) = (h.listing.as_ref(), h.budget.as_ref()) else {
        return None;
    };
    let drafts = &h.drafts;
    let lookup = |f: &hf::RepoFile| drafts.get(&f.path).cloned();
    h.estimates = l
        .candidates
        .iter()
        .enumerate()
        .map(|(i, c)| match h.shapes.get(i).cloned().flatten() {
            None => None,
            Some(Err(m)) => Some(Err(m)),
            Some(Ok(s)) => hf_fit::extras_of(c, h.wish_mmproj, h.wish_drafter, &lookup)
                .map(|x| Ok((hf_fit::estimate_with(&s, b, &x), s.ctx))),
        })
        .collect();
    if h.estimates.iter().any(Option::is_none) {
        h.recommended = None;
        return None;
    }
    let rows: Vec<hf_fit::Row> = l
        .candidates
        .iter()
        .zip(&h.estimates)
        .map(|(c, e)| hf_fit::Row {
            tag: &c.tag,
            size: c.size,
            estimate: e.as_ref().and_then(|r| r.as_ref().ok()).map(|(e, _)| e),
        })
        .collect();
    let rec = hf_fit::recommend(&rows);
    let to = rec.as_ref().filter(|_| !h.user_picked).map(|r| r.index as i32);
    h.recommended = rec;
    to
}

/// Read the headers of the wished drafter's files not read yet, off the
/// event loop, each landing in `drafted`. Nothing is spawned under
/// `cfg(test)` (`spawn_request`): the e2e test lands them by hand.
fn spawn_drafts(app: &AppWindow, state: &Rc<RefCell<State>>) {
    let job = {
        let mut st = state.borrow_mut();
        let h = &mut st.hf;
        let (Some(kind), Some(l), Some(_)) = (h.wish_drafter, h.listing.as_ref(), h.budget.as_ref()) else {
            return;
        };
        let mut files: Vec<hf::RepoFile> = Vec::new();
        for f in l.candidates.iter().filter_map(|c| c.sidecar(kind)) {
            if !h.drafts.contains_key(&f.path) && !h.drafts_pending.contains(&f.path) && !files.iter().any(|x| x.path == f.path) {
                files.push(f.clone());
            }
        }
        if files.is_empty() {
            return;
        }
        h.drafts_pending.extend(files.iter().map(|f| f.path.clone()));
        (l.repo.clone(), l.commit.clone(), files, h.generation)
    };
    let token: Option<hf::Token> =
        request_token(app.global::<AppState>().get_hf_token().as_str(), hf::token_from_env).unwrap_or_default();
    let app_weak = app.as_weak();
    spawn_request(move || {
        let (repo, commit, files, generation) = job;
        let endpoint = hf::endpoint();
        for f in files {
            let result = hf_fit::fetch_draft_shape(&hf::Curl, &endpoint, &repo, &commit, &f, token.as_ref()).map_err(|e| e.to_string());
            let weak = app_weak.clone();
            slint::invoke_from_event_loop(move || {
                let (Some(app), Some(state)) = (weak.upgrade(), shared_state()) else {
                    return;
                };
                if state.borrow().hf.generation != generation {
                    return;
                }
                drafted(&app, &state, f.path, result);
            })
            .ok();
        }
    });
}

/// A drafter file's header landed (current generation): its rows again.
fn drafted(app: &AppWindow, state: &Rc<RefCell<State>>, path: String, result: Result<std::sync::Arc<hf_fit::DraftShape>, String>) {
    {
        let mut st = state.borrow_mut();
        st.hf.drafts_pending.remove(&path);
        st.hf.drafts.insert(path, result);
    }
    reestimate(app, state);
}

/// TEST-ONLY seam: land a drafter header read as the worker does.
#[cfg(test)]
pub(crate) fn land_draft_for_tests(app: &AppWindow, path: &str, result: Result<std::sync::Arc<hf_fit::DraftShape>, String>) {
    let state = shared_state().expect("hf_dialog::wire ran");
    drafted(app, &state, path.to_string(), result);
}

/// "Resume download" or "Download": whether the picked candidate has a partial
/// of its own files in the staging tree (`hf::has_partial`).
fn refresh_resume(app: &AppWindow, state: &Rc<RefCell<State>>, models_dir: &Path) {
    let s = app.global::<AppState>();
    let st = state.borrow();
    let resume = usize::try_from(s.get_hf_candidate_index()).ok().is_some_and(|i| {
        st.hf
            .listing
            .as_ref()
            .and_then(|l| Some((l, l.candidates.get(i)?)))
            .is_some_and(|(l, c)| hf::has_partial(models_dir, &l.repo, c))
    });
    s.set_hf_resume(resume);
}

/// Forget the search: its rows, its title and the Back button.
fn clear_results(app: &AppWindow, state: &Rc<RefCell<State>>) {
    {
        let mut st = state.borrow_mut();
        st.hf.results.clear();
        st.hf.query.clear();
    }
    let s = app.global::<AppState>();
    s.set_hf_results_title(SharedString::default());
    s.set_hf_result_labels(string_model(Vec::new()));
    s.set_hf_result_details(string_model(Vec::new()));
    s.set_hf_result_index(-1);
    s.set_hf_has_results(false);
}

fn open(app: &AppWindow, state: &Rc<RefCell<State>>) {
    let s = app.global::<AppState>();
    {
        let mut st = state.borrow_mut();
        st.hf.generation += 1;
        st.hf.listing = None;
    }
    clear_listing(app);
    clear_results(app, state);
    set_phase(app, Phase::Idle);
    s.set_hf_error(SharedString::default());
    s.set_hf_note(SharedString::default());
    s.set_hf_env_token(hf::token_from_env().is_some());
    s.set_show_hf_dialog(true);
}

fn dismiss(app: &AppWindow, state: &Rc<RefCell<State>>) {
    let s = app.global::<AppState>();
    // A search or a listing still in flight lands on a closed dialog: drop it.
    state.borrow_mut().hf.generation += 1;
    ESTIMATE_RUN.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    set_phase(app, Phase::Idle);
    s.set_show_hf_dialog(false);
}

/// The token for a request, or the refusal shown in the dialog (`None`).
fn token_or_error(app: &AppWindow) -> Option<Option<hf::Token>> {
    let s = app.global::<AppState>();
    match request_token(s.get_hf_token().as_str(), hf::token_from_env) {
        Ok(t) => Some(t),
        Err(e) => {
            s.set_hf_error(SharedString::from(dialog_error(&e)));
            None
        }
    }
}

/// Run a search's or a listing's worker thread. Under `cfg(test)` nothing is
/// spawned: the e2e flow lands every answer itself (`land_search_for_tests`,
/// `land_listing_for_tests`), so no test reaches the network.
fn spawn_request(work: impl FnOnce() + Send + 'static) {
    #[cfg(not(test))]
    std::thread::spawn(work);
    #[cfg(test)]
    drop(work);
}

/// Search or List, by what the field holds.
fn submit(app: &AppWindow, state: &Rc<RefCell<State>>) {
    let s = app.global::<AppState>();
    if !phase(app).can_request() {
        return;
    }
    match hf::classify_input(s.get_hf_input().as_str()) {
        Ok(None) => {}
        Ok(Some(hf::Input::Spec(spec))) => list(app, state, spec, false),
        Ok(Some(hf::Input::Query(q))) => search(app, state, q),
        Err(e) => s.set_hf_error(SharedString::from(dialog_error(&e))),
    }
}

fn search(app: &AppWindow, state: &Rc<RefCell<State>>, query: String) {
    let s = app.global::<AppState>();
    let Some(token) = token_or_error(app) else {
        return;
    };
    {
        let mut st = state.borrow_mut();
        st.hf.generation += 1;
        st.hf.listing = None;
    }
    clear_listing(app);
    clear_results(app, state);
    let generation = state.borrow().hf.generation;
    s.set_hf_error(SharedString::default());
    s.set_hf_note(SharedString::default());
    set_phase(app, Phase::Searching);

    let app_weak = app.as_weak();
    spawn_request(move || {
        let result = hf::search(&query, hf::SEARCH_LIMIT, token.as_ref());
        slint::invoke_from_event_loop(move || {
            let (Some(app), Some(state)) = (app_weak.upgrade(), shared_state()) else {
                return;
            };
            if state.borrow().hf.generation != generation {
                return; // superseded by another request, or the dialog closed
            }
            searched(&app, &state, query, result);
        })
        .ok();
    });
}

/// A search landed (current generation).
fn searched(app: &AppWindow, state: &Rc<RefCell<State>>, query: String, result: Result<Vec<hf::SearchHit>, hf::HfError>) {
    let s = app.global::<AppState>();
    match result {
        Ok(hits) => {
            s.set_hf_results_title(SharedString::from(results_title(&query, hits.len(), hf::SEARCH_LIMIT)));
            show_results(app, &hits, -1);
            let mut st = state.borrow_mut();
            st.hf.query = query;
            st.hf.results = hits;
            drop(st);
            set_phase(app, Phase::Searched);
        }
        Err(e) => {
            set_phase(app, Phase::Idle);
            s.set_hf_error(SharedString::from(dialog_error(&e)));
        }
    }
}

/// Fill the result rows (`picked` = the row to mark, -1 for none).
fn show_results(app: &AppWindow, hits: &[hf::SearchHit], picked: i32) {
    let s = app.global::<AppState>();
    s.set_hf_result_labels(string_model(hits.iter().map(result_label).collect()));
    s.set_hf_result_details(string_model(hits.iter().map(result_detail).collect()));
    s.set_hf_result_index(picked);
    s.set_hf_has_results(!hits.is_empty());
}

/// A result row was clicked: list its repository, keeping the results.
fn pick_result(app: &AppWindow, state: &Rc<RefCell<State>>, index: i32) {
    if !phase(app).can_request() {
        return;
    }
    let repo = usize::try_from(index)
        .ok()
        .and_then(|i| Some(state.borrow().hf.results.get(i)?.repo.clone()));
    let Some(repo) = repo else {
        return;
    };
    let s = app.global::<AppState>();
    s.set_hf_result_index(index);
    s.set_hf_input(SharedString::from(repo.as_str()));
    list(app, state, hf::RepoSpec { repo, tag: None, file: None }, true);
}

/// Back from a picked row's listing to the results, the query back in the
/// field. A listing still in flight is dropped (generation).
fn back_to_results(app: &AppWindow, state: &Rc<RefCell<State>>) {
    if phase(app).shows_results() || state.borrow().hf.results.is_empty() {
        return;
    }
    let query = {
        let mut st = state.borrow_mut();
        st.hf.generation += 1;
        st.hf.listing = None;
        st.hf.query.clone()
    };
    clear_listing(app);
    let s = app.global::<AppState>();
    s.set_hf_input(SharedString::from(query));
    s.set_hf_error(SharedString::default());
    s.set_hf_note(SharedString::default());
    set_phase(app, Phase::Searched);
}

/// List `spec`'s repository. `from_result`: it was a picked search row, so
/// the results stay for Back; a typed spec drops them.
fn list(app: &AppWindow, state: &Rc<RefCell<State>>, spec: hf::RepoSpec, from_result: bool) {
    let s = app.global::<AppState>();
    let Some(token) = token_or_error(app) else {
        return;
    };
    if !from_result {
        clear_results(app, state);
    }
    let generation = {
        let mut st = state.borrow_mut();
        st.hf.generation += 1;
        st.hf.listing = None;
        st.hf.generation
    };
    clear_listing(app);
    s.set_hf_error(SharedString::default());
    s.set_hf_note(SharedString::default());
    set_phase(app, Phase::Listing);

    let app_weak = app.as_weak();
    spawn_request(move || {
        let result = hf::list_repo(&spec, token.as_ref());
        slint::invoke_from_event_loop(move || {
            let (Some(app), Some(state)) = (app_weak.upgrade(), shared_state()) else {
                return;
            };
            if state.borrow().hf.generation != generation {
                return; // superseded by another List, or the dialog closed
            }
            listed(&app, &state, result);
        })
        .ok();
    });
}

/// The estimates' budget: the probed GPUs and the system RAM.
fn machine_budget() -> Option<hf_fit::Budget> {
    hf_fit::budget(&devices::probed(), hf_fit::system_ram_total())
}

/// A listing landed (current generation).
fn listed(app: &AppWindow, state: &Rc<RefCell<State>>, result: Result<hf::Listing, hf::HfError>) {
    listed_with(app, state, result, machine_budget());
}

/// `listed`, against `budget`: rows, then the default pick, then the header
/// reads for the estimates (none without a GPU: every row says why).
fn listed_with(
    app: &AppWindow,
    state: &Rc<RefCell<State>>,
    result: Result<hf::Listing, hf::HfError>,
    budget: Option<hf_fit::Budget>,
) {
    let s = app.global::<AppState>();
    match result {
        Ok(l) => {
            s.set_hf_listing_title(SharedString::from(listing_title(&l)));
            s.set_hf_estimate_basis(SharedString::from(budget.as_ref().map(hf_fit::basis_label).unwrap_or_default()));
            let n = l.candidates.len();
            let default = l.default as i32;
            let job = (l.repo.clone(), l.commit.clone(), l.candidates.clone());
            {
                let mut st = state.borrow_mut();
                st.hf.listing = Some(l);
                st.hf.shapes = vec![None; n];
                st.hf.drafts.clear();
                st.hf.drafts_pending.clear();
                st.hf.wish_mmproj = true;
                st.hf.wish_drafter = None;
                st.hf.estimates = match budget {
                    Some(_) => vec![None; n],
                    None => vec![Some(Err(hf_fit::NO_GPU.to_string())); n],
                };
                st.hf.recommended = None;
                st.hf.user_picked = false;
                st.hf.budget = budget.clone();
            }
            refresh_rows(app, state);
            set_phase(app, Phase::Listed);
            pick(app, state, default);
            if budget.is_some() {
                spawn_estimates(app, state, job);
            }
        }
        Err(e) => {
            // From a picked row the results stay on screen, the error under them.
            let back = if state.borrow().hf.results.is_empty() { Phase::Idle } else { Phase::Searched };
            set_phase(app, back);
            s.set_hf_error(SharedString::from(dialog_error(&e)));
        }
    }
}

/// Rewrite every candidate row (label and estimate line) from the session.
fn refresh_rows(app: &AppWindow, state: &Rc<RefCell<State>>) {
    let st = state.borrow();
    let Some(l) = st.hf.listing.as_ref() else {
        return;
    };
    let rec = st.hf.recommended.as_ref();
    let is_rec = |i: usize| rec.is_some_and(|r| r.index == i);
    let labels: Vec<String> =
        l.candidates.iter().enumerate().map(|(i, c)| candidate_label(c, i == l.default, is_rec(i))).collect();
    let details: Vec<String> = (0..l.candidates.len())
        .map(|i| {
            let slot = st.hf.estimates.get(i).cloned().flatten();
            candidate_detail(&slot, rec.filter(|r| r.index == i).map(|r| r.reason))
        })
        .collect();
    let s = app.global::<AppState>();
    s.set_hf_candidate_labels(string_model(labels));
    s.set_hf_candidate_details(string_model(details));
}

/// Read every candidate's GGUF header off the event loop
/// (`hf_fit::fetch_shapes`, `WORKERS` at a time), each answer landing in
/// `estimated` as it comes. Stops early once the listing is superseded
/// (`ESTIMATE_RUN`). Nothing is spawned under `cfg(test)` (`spawn_request`).
fn spawn_estimates(app: &AppWindow, state: &Rc<RefCell<State>>, job: (String, String, Vec<hf::Candidate>)) {
    use std::sync::atomic::Ordering;
    // The listing's own token rule; a typed token that cannot be one already
    // stopped the listing, so this falls back to none only in theory.
    let token: Option<hf::Token> =
        request_token(app.global::<AppState>().get_hf_token().as_str(), hf::token_from_env).unwrap_or_default();
    let generation = state.borrow().hf.generation;
    let run = ESTIMATE_RUN.fetch_add(1, Ordering::Relaxed) + 1;
    let app_weak = app.as_weak();
    spawn_request(move || {
        let (repo, commit, candidates) = job;
        let endpoint = hf::endpoint();
        hf_fit::fetch_shapes(
            &hf::Curl,
            &endpoint,
            &repo,
            &commit,
            &candidates,
            token.as_ref(),
            &|| ESTIMATE_RUN.load(Ordering::Relaxed) == run,
            &|index, result| {
                let weak = app_weak.clone();
                slint::invoke_from_event_loop(move || {
                    let (Some(app), Some(state)) = (weak.upgrade(), shared_state()) else {
                        return;
                    };
                    if state.borrow().hf.generation != generation {
                        return; // another listing, a search, Back or a dismiss came first
                    }
                    estimated(&app, &state, index, result);
                })
                .ok();
            },
        );
    });
}

/// One candidate's header landed (current generation): its estimate, and
/// once every row has its answer the recommendation, which also moves the
/// pick unless the user already chose a row (`reestimate`).
fn estimated(app: &AppWindow, state: &Rc<RefCell<State>>, index: usize, result: Result<std::sync::Arc<hf_fit::Shape>, hf::HfError>) {
    {
        let mut st = state.borrow_mut();
        if st.hf.budget.is_none() {
            return;
        }
        let Some(slot) = st.hf.shapes.get_mut(index) else {
            return;
        };
        *slot = Some(result.map_err(|e| e.to_string()));
    }
    reestimate(app, state);
}

/// TEST-ONLY seam: land a listing measured against `budget` (the probe is
/// empty under test), as `listed` does.
#[cfg(test)]
pub(crate) fn land_listing_with_budget_for_tests(app: &AppWindow, result: Result<hf::Listing, hf::HfError>, budget: Option<hf_fit::Budget>) {
    let state = shared_state().expect("hf_dialog::wire ran");
    listed_with(app, &state, result, budget);
}

/// TEST-ONLY seam: land one candidate's header read as the worker does.
#[cfg(test)]
pub(crate) fn land_estimate_for_tests(app: &AppWindow, index: usize, result: Result<std::sync::Arc<hf_fit::Shape>, hf::HfError>) {
    let state = shared_state().expect("hf_dialog::wire ran");
    estimated(app, &state, index, result);
}

fn download(app: &AppWindow, state: &Rc<RefCell<State>>) {
    let s = app.global::<AppState>();
    if phase(app) != Phase::Listed {
        return;
    }
    let Ok(index) = usize::try_from(s.get_hf_candidate_index()) else {
        return;
    };
    let token = match request_token(s.get_hf_token().as_str(), hf::token_from_env) {
        Ok(t) => t,
        Err(e) => {
            s.set_hf_error(SharedString::from(dialog_error(&e)));
            return;
        }
    };
    let Some(server_exe) = server_exe() else {
        s.set_hf_error(SharedString::from(dialog_error(&hf::HfError::MissingTool(
            "llama-server was not found beside the configurator or in the build tree".into(),
        ))));
        return;
    };
    let mmproj = s.get_hf_take_mmproj();
    let drafter = drafter_choice(s.get_hf_drafter().as_str());
    let built = {
        let st = state.borrow();
        let Some(listing) = st.hf.listing.as_ref() else {
            return;
        };
        if st.downloads.busy_with(&listing.repo) {
            Err(busy_refusal(&listing.repo))
        } else {
            let detail = listing.candidates.get(index).map(|c| selection_summary(c, mmproj, drafter)).unwrap_or_default();
            hf::DownloadJob::for_server(listing, index, mmproj, drafter, models_dir(), token, server_exe)
                .map(|job| {
                    let title = job_title(&job.repo, &job.candidate);
                    (job, title, detail)
                })
                .map_err(|e| dialog_error(&e))
        }
    };
    let (job, title, detail) = match built {
        Ok(b) => b,
        Err(e) => {
            s.set_hf_error(SharedString::from(e));
            return;
        }
    };
    // Against the budget the rows were estimated with, so the preset gets the
    // split the picked row showed.
    let budget = state.borrow().hf.budget.clone();
    set_status(app, format!("Downloading {title} in the background: the Downloads window shows its progress."), false);
    // The dialog's part ends here: close it as a dismiss does (a header read
    // still in flight lands on nothing), then hand the job over.
    dismiss(app, state);
    downloads::start(app, state, job, title, detail, budget);
}

/// The llama-server the download's child runs. Under `cfg(test)` a stand-in:
/// no test starts a child (`downloads::spawn`), and the build tree may hold
/// no server.
fn server_exe() -> Option<PathBuf> {
    if cfg!(test) {
        Some(PathBuf::from("llama-server"))
    } else {
        crate::paths::llama_server_exe()
    }
}

pub(super) fn wire(app: &AppWindow, state: &Rc<RefCell<State>>) {
    STATE.with(|s| *s.borrow_mut() = Rc::downgrade(state));
    let s = app.global::<AppState>();
    s.on_hf_input_problem(|text| SharedString::from(input_problem(text.as_str())));
    s.on_hf_input_is_query(|text| input_is_query(text.as_str()));

    // One shape for every handler: upgrade the window, run the body.
    fn hook(
        app: &AppWindow,
        state: &Rc<RefCell<State>>,
        body: fn(&AppWindow, &Rc<RefCell<State>>),
    ) -> impl Fn() + 'static {
        let app_weak = app.as_weak();
        let state = state.clone();
        move || {
            if let Some(app) = app_weak.upgrade() {
                body(&app, &state);
            }
        }
    }
    s.on_hf_open(hook(app, state, open));
    s.on_hf_dismiss(hook(app, state, dismiss));
    s.on_hf_submit(hook(app, state, submit));
    s.on_hf_back_to_results(hook(app, state, back_to_results));
    s.on_hf_download(hook(app, state, download));
    s.on_hf_choices_changed(hook(app, state, choices_changed));

    // The same shape, for the handlers that take a row index.
    fn hook_index(
        app: &AppWindow,
        state: &Rc<RefCell<State>>,
        body: fn(&AppWindow, &Rc<RefCell<State>>, i32),
    ) -> impl Fn(i32) + 'static {
        let app_weak = app.as_weak();
        let state = state.clone();
        move |index| {
            if let Some(app) = app_weak.upgrade() {
                body(&app, &state, index);
            }
        }
    }
    s.on_hf_select(hook_index(app, state, select));
    s.on_hf_pick_result(hook_index(app, state, pick_result));
}

/// TEST-ONLY seam for the e2e flow (src/tests/save_flow.rs): land a search's
/// answer exactly as the worker's closure does, without a network.
#[cfg(test)]
pub(crate) fn land_search_for_tests(app: &AppWindow, query: &str, result: Result<Vec<hf::SearchHit>, hf::HfError>) {
    let state = shared_state().expect("hf_dialog::wire ran");
    searched(app, &state, query.to_string(), result);
}

/// TEST-ONLY seam: land a listing's answer as the worker's closure does.
#[cfg(test)]
pub(crate) fn land_listing_for_tests(app: &AppWindow, result: Result<hf::Listing, hf::HfError>) {
    let state = shared_state().expect("hf_dialog::wire ran");
    listed(app, &state, result);
}

#[cfg(test)]
mod tests {
    use super::*;
    use hf::{Candidate, HfError, RepoFile, Sidecar};

    fn file(path: &str, size: u64) -> RepoFile {
        RepoFile { path: path.into(), size, oid: String::new() }
    }

    fn candidate(tag: &str, shards: usize) -> Candidate {
        let shards: Vec<RepoFile> = if shards == 1 {
            vec![file(&format!("M-{tag}.gguf"), 4 << 30)]
        } else {
            (1..=shards)
                .map(|i| file(&format!("{tag}/M-{tag}-0000{i}-of-0000{shards}.gguf"), 2 << 30))
                .collect()
        };
        let size = shards.iter().map(|f| f.size).sum();
        Candidate { tag: tag.into(), shards, size, mmproj: None, mtp: None, dflash: None, dspark: None }
    }

    const ALL: [Phase; 5] = [Phase::Idle, Phase::Searching, Phase::Searched, Phase::Listing, Phase::Listed];

    #[test]
    fn phase_numbering_is_the_ui_contract() {
        for p in ALL {
            assert_eq!(Phase::from_int(p as i32), p);
        }
        assert_eq!(Phase::from_int(99), Phase::Idle);
        assert_eq!(Phase::from_int(5), Phase::Idle, "the old transfer phases are gone: no transfer runs in the dialog");
        for p in ALL {
            let n = p as i32;
            // state.slint: `hf_busy: hf_phase == 1 || hf_phase == 3`, `hf_show_results: hf_phase <= 2`.
            assert_eq!(p.busy(), n == 1 || n == 3, "{p:?}");
            assert_eq!(p.shows_results(), n <= 2, "{p:?}");
        }
    }

    /// Which phase accepts which step: one request at a time, and the result
    /// rows only up to `Searched`.
    #[test]
    fn phase_transitions_allow_one_request_at_a_time() {
        let can: Vec<Phase> = ALL.into_iter().filter(|p| p.can_request()).collect();
        assert_eq!(can, [Phase::Idle, Phase::Searched, Phase::Listed], "search, list and a row pick");
        assert!(!Phase::Searching.can_request(), "one search at a time, as the button says");
        assert!(!Phase::Listing.can_request(), "one listing at a time");
        // A picked row's listing hides the results; Back returns to them.
        assert!(Phase::Searched.shows_results() && !Phase::Listing.shows_results() && !Phase::Listed.shows_results());
    }

    #[test]
    fn the_input_hint_is_empty_for_a_spec_a_query_or_nothing() {
        assert_eq!(input_problem(""), "");
        assert_eq!(input_problem("   "), "");
        assert_eq!(input_problem("unsloth/Qwen3-8B-GGUF"), "");
        assert_eq!(input_problem("unsloth/Qwen3-8B-GGUF:Q4_K_M"), "");
        assert_eq!(input_problem("https://huggingface.co/unsloth/Qwen3-8B-GGUF/tree/main"), "");
        assert_eq!(input_problem("qwen3 8b"), "", "words are a search");
        let bad = input_problem("org/repo:");
        assert!(bad.starts_with("Not a quant tag"), "{bad}");
        let bad = input_problem("a/b/c");
        assert!(bad.starts_with("Not a repository id"), "{bad}");
    }

    #[test]
    fn the_button_searches_words_and_lists_a_repository() {
        assert!(input_is_query("qwen3 8b"));
        assert!(input_is_query("tinygemma3"));
        assert!(!input_is_query("unsloth/Qwen3-8B-GGUF"), "a spec goes straight to the listing");
        assert!(!input_is_query("https://hf.co/org/repo"));
        assert!(!input_is_query(""));
        assert!(!input_is_query("a/b/c"), "a mistyped spec is an error, not a search");
    }

    fn hit(repo: &str, downloads: u64, quants: &[&str]) -> hf::SearchHit {
        hf::SearchHit {
            repo: repo.into(),
            downloads,
            gated: false,
            private: false,
            quants: quants.iter().map(|q| q.to_string()).collect(),
            mmproj: false,
            drafters: Vec::new(),
            kind: hf::ModelKind::Unknown,
            params: None,
        }
    }

    #[test]
    fn result_rows_carry_repo_downloads_access_and_quants() {
        let mut h = hit("Qwen/Qwen3-8B-GGUF", 644_587, &["Q4_K_M", "Q5_K_M", "Q8_0"]);
        assert_eq!(result_label(&h), "Qwen/Qwen3-8B-GGUF  ·  644.6k downloads", "no gguf summary, no marker");
        h.kind = hf::ModelKind::Dense;
        h.params = Some(8_190_735_360);
        assert_eq!(result_label(&h), "Qwen/Qwen3-8B-GGUF  ·  dense 8.2B  ·  644.6k downloads");
        h.kind = hf::ModelKind::Unknown;
        assert_eq!(result_label(&h), "Qwen/Qwen3-8B-GGUF  ·  8.2B, dense/MoE unknown  ·  644.6k downloads");
        h.kind = hf::ModelKind::Moe;
        h.params = Some(30_532_122_624);
        assert_eq!(result_label(&h), "Qwen/Qwen3-8B-GGUF  ·  MoE 30.5B  ·  644.6k downloads");
        h.kind = hf::ModelKind::Unknown;
        h.params = None;
        assert_eq!(result_detail(&h), "Q4_K_M, Q5_K_M, Q8_0");
        h.gated = true;
        h.private = true;
        h.mmproj = true;
        h.drafters = vec![Sidecar::Mtp];
        assert_eq!(result_label(&h), "Qwen/Qwen3-8B-GGUF  ·  644.6k downloads  ·  gated  ·  private");
        assert_eq!(result_detail(&h), "Q4_K_M, Q5_K_M, Q8_0  ·  with mmproj, MTP");
        let empty = hit("o/r", 3, &[]);
        assert_eq!(result_detail(&empty), "no GGUF model file");
    }

    #[test]
    fn the_results_title_counts_and_says_when_the_limit_cut_it() {
        assert_eq!(results_title("zzz", 0, 30), "No GGUF repository matches \"zzz\".");
        assert_eq!(results_title("q", 1, 30), "1 GGUF repository for \"q\"; downloads over the last 30 days");
        assert!(results_title("q", 4, 30).starts_with("4 GGUF repositories for \"q\", most downloaded first"));
        assert!(results_title("q", 30, 30).contains("(add words to narrow it)"));
    }

    #[test]
    fn dialog_errors_point_at_the_token_field_where_a_token_would_help() {
        let e = dialog_error(&HfError::NotFoundOrPrivate { repo: "o/r".into() });
        assert!(e.starts_with("Repository o/r not found") && e.ends_with("Access token field."), "{e}");
        let e = dialog_error(&HfError::Gated { repo: "o/r".into(), had_token: false });
        assert!(e.ends_with("Access token field."), "{e}");
        let e = dialog_error(&HfError::Gated { repo: "o/r".into(), had_token: true });
        assert!(!e.contains("Access token field"), "{e}");
        assert_eq!(dialog_error(&HfError::Network("timeout".into())), "Network error: timeout");
    }

    #[test]
    fn a_typed_token_wins_and_an_empty_field_falls_back_to_the_environment() {
        let env = || hf::Token::new("hf_env").ok();
        assert_eq!(request_token("", env).unwrap(), hf::Token::new("hf_env").ok());
        assert_eq!(request_token("  ", || None).unwrap(), None);
        assert_eq!(request_token(" hf_typed ", env).unwrap(), hf::Token::new("hf_typed").ok());
        let err = request_token("hf bad", env).unwrap_err();
        assert!(!err.to_string().contains("hf bad"), "the error never echoes the token");
    }

    #[test]
    fn candidate_labels_carry_size_shards_sidecars_and_the_default_mark() {
        let mut c = candidate("Q4_K_M", 1);
        assert_eq!(candidate_label(&c, false, false), "Q4_K_M  ·  4.00 GiB");
        c.mmproj = Some(file("mmproj-F16.gguf", 600 << 20));
        c.mtp = Some(file("mtp-Q8_0.gguf", 300 << 20));
        assert_eq!(candidate_label(&c, true, false), "Q4_K_M  ·  4.00 GiB  ·  with mmproj, MTP  ·  default");
        assert_eq!(
            candidate_label(&c, true, true),
            "Q4_K_M  ·  4.00 GiB  ·  with mmproj, MTP  ·  default  ·  recommended"
        );
        let c = candidate("BF16", 2);
        assert_eq!(candidate_label(&c, false, false), "BF16  ·  4.00 GiB  ·  2 shards");
        let mut c = candidate("", 1);
        c.shards[0].path = "dir/model.gguf".into();
        assert!(candidate_label(&c, false, false).starts_with("model.gguf  ·  "));
    }

    #[test]
    fn candidate_details_say_estimating_the_estimate_or_why_not() {
        assert_eq!(candidate_detail(&None, None), ESTIMATING);
        let e = hf_fit::Estimate::plain(hf_fit::Fit::Gpu { need: 6 << 30, budget: 12 << 30 }, Some(0), vec![]);
        assert_eq!(candidate_detail(&Some(Ok((e.clone(), 32768))), None), "all on GPU: 6.0 of 12.0 GiB with a 32k context");
        assert_eq!(
            candidate_detail(&Some(Ok((e, 32768))), Some("the best quality that fits in GPU memory")),
            "recommended, the best quality that fits in GPU memory: all on GPU: 6.0 of 12.0 GiB with a 32k context"
        );
        assert_eq!(
            candidate_detail(&Some(Err(hf_fit::NO_GPU.into())), None),
            "estimate unavailable: llama-server --list-devices reports no GPU"
        );
    }

    #[test]
    fn drafter_pills_appear_only_for_a_candidate_with_a_drafter() {
        let mut c = candidate("Q8_0", 1);
        assert!(drafter_options(&c).is_empty());
        c.mmproj = Some(file("mmproj.gguf", 1));
        assert!(drafter_options(&c).is_empty(), "an mmproj is not a drafter");
        c.dflash = Some(file("dflash-Q4_K_M.gguf", 1));
        c.mtp = Some(file("mtp-Q8_0.gguf", 1));
        assert_eq!(drafter_options(&c), ["none", "MTP", "DFlash"]);
        assert_eq!(drafter_choice("none"), None);
        assert_eq!(drafter_choice("MTP"), Some(Sidecar::Mtp));
        assert_eq!(drafter_choice("DFlash"), Some(Sidecar::Dflash));
    }

    #[test]
    fn the_summary_counts_what_the_choices_fetch() {
        let mut c = candidate("BF16", 2);
        c.mmproj = Some(file("mmproj-F16.gguf", 1 << 30));
        c.mtp = Some(file("mtp-Q8_0.gguf", 1 << 30));
        assert_eq!(
            selection_summary(&c, false, None),
            "Fetches 2 files, 4.00 GiB: M-BF16-00001-of-00002.gguf (2 shards)"
        );
        assert_eq!(
            selection_summary(&c, true, Some(Sidecar::Mtp)),
            "Fetches 4 files, 6.00 GiB: M-BF16-00001-of-00002.gguf (2 shards), mmproj-F16.gguf, mtp-Q8_0.gguf"
        );
        // A drafter the candidate lacks adds nothing (the job would refuse it).
        assert!(selection_summary(&c, false, Some(Sidecar::Dspark)).starts_with("Fetches 2 files"));
        let c = candidate("Q4_K_M", 1);
        assert_eq!(selection_summary(&c, true, None), "Fetches 1 file, 4.00 GiB: M-Q4_K_M.gguf");
    }

    #[test]
    fn a_downloads_row_is_titled_by_repository_and_quantization() {
        assert_eq!(job_title("o/r", &candidate("Q4_K_M", 1)), "o/r  ·  Q4_K_M");
        let mut c = candidate("", 1);
        c.shards[0].path = "dir/model.gguf".into();
        assert_eq!(job_title("o/r", &c), "o/r  ·  model.gguf", "no tag: the file");
        let refused = busy_refusal("o/r");
        assert!(refused.starts_with("o/r is already downloading") && refused.contains("one download per repository"), "{refused}");
    }

    #[test]
    fn the_listing_title_names_repo_commit_and_count() {
        let l = hf::Listing {
            repo: "o/r".into(),
            commit: "0123456789abcdef0123456789abcdef01234567".into(),
            project_url: String::new(),
            candidates: vec![candidate("Q4_K_M", 1)],
            default: 0,
        };
        assert_eq!(listing_title(&l), "o/r @ 0123456789ab: 1 quantization");
    }
}
