//! The Models tab's "New preset from Hugging Face" dialog: the New preset
//! dialog's second source, beside picking a file already in ModelsDir.
//!
//! The user types a model name and searches (`hf::search`: GGUF repositories
//! by downloads, each row with its quantizations), picks a repository, or
//! types one directly (a spec or its URL); its GGUF quantizations are listed
//! (`hf::list_repo`), the user picks one with its sidecars, downloads it
//! (`hf::spawn_download`) and gets the preset created from the files
//! (`hf::preset_from_download` + `hf::save_preset`, then the same tail every
//! new preset takes: `models_tab::commit_downloaded_preset`).
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
//! Download → `Downloading` → Cancel → `Cancelling`. A typed spec skips the
//! first two: `Idle` → List → `Listing`. Picking a row puts its `org/repo` in
//! the field and keeps the results (`Session::results`): **Back to results**
//! returns to `Searched` with the query back in the field, from `Listing` too
//! (the listing in flight is dropped by the generation). A typed List or a new
//! Search drops them. A finished download closes the dialog and selects the
//! new preset; a failed or cancelled one returns to `Listed` with the error
//! (or the note) IN the dialog, the listing kept so a retry is one click. The
//! button reads "Resume download" only when the PICKED candidate has a real
//! partial to resume (`hf::has_partial`: a `.downloadInProgress` of one of its
//! own files, re-checked on every pick and after every failure); a partial of
//! another quant of the repo does not count, since this job would not read
//! it. A failed search returns to `Idle`, a failed listing to `Searched` when
//! it came from a row (the results stay) and to `Idle` otherwise. One request
//! runs at a time: the button is disabled while a search or a listing runs
//! (`Phase::can_request`). Every request still carries a generation, bumped by
//! a dismiss and by Back too, so the answer of a request whose dialog was
//! closed (and perhaps reopened) is dropped (the curl it ran finishes on its
//! own thread, bounded by its own `--max-time`).
//!
//! ## Dismissing during a transfer is REFUSED
//! Esc and the backdrop go to `hf_dismiss`, which closes the dialog in every
//! phase but `Downloading` / `Cancelling`; there it only says why (Close is
//! disabled too). An accidental Esc must not throw away gigabytes, and since
//! the modal also keeps the form untouchable, the finished download can
//! replace the editor's selection safely. No orphan child either way:
//! - Cancel (`DownloadHandle::cancel`) closes the child's stdin, a kill
//!   following 10 s later; NOT the router's exit line, on which llama.cpp
//!   deletes its partial (`hf.rs` module header). The bytes fetched so far stay
//!   in the staging tree and the next download of the same files resumes.
//! - Closing the WINDOW hides it to the tray: the download carries on and
//!   still creates its preset.
//! - The tray's Quit ends the process, which closes the child's stdin pipe:
//!   llama-server's router child treats EOF on stdin as a hard stop
//!   (`hf.rs` module header), so it exits with the configurator.
//!
//! ## The token
//! `AppState.hf_token` holds only what the user TYPES (masked). Empty means
//! the environment's token (`hf::token_from_env`), read here at request time
//! and never copied into the UI; the field's placeholder only says that one
//! exists. Neither form reaches settings.ini, a log, a status line or a
//! preset: `hf::Token` has no `Display` and `HfError` never carries it.
//!
//! ## Threading
//! The three slow calls run on their own thread and come back through
//! `slint::invoke_from_event_loop` + `Weak::upgrade` (the `spawn_device_probe`
//! pattern). The landing closure must be `Send`, so it cannot carry the
//! `Rc<RefCell<State>>`: it reaches it through `STATE`, a thread-local weak
//! handle `wire` sets on the UI thread (the only thread those closures run
//! on). Progress is coalesced on the worker (`Coalescer`: at most one update
//! per `PROGRESS_INTERVAL`, plus every file boundary and the last byte), so
//! the child's ten-per-second-per-file lines never flood the event loop.
//!
//! The pure parts (phases, input check, result and candidate labels, summary,
//! progress text, the coalescer, the outcome of a failure) are unit-tested
//! below; the e2e flow
//! test (`src/tests/save_flow.rs`) drives the wiring that needs no network.

use super::*;

use std::path::Path;
use std::time::{Duration, Instant};

use crate::hf;

/// The dialog's phase. Its `i32` is `AppState.hf_phase` (state.slint reads it
/// for the button gates), so the numbering is part of the UI contract.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum Phase {
    Idle = 0,
    Searching = 1,
    Searched = 2,
    Listing = 3,
    Listed = 4,
    Downloading = 5,
    Cancelling = 6,
}

impl Phase {
    fn from_int(v: i32) -> Phase {
        match v {
            1 => Phase::Searching,
            2 => Phase::Searched,
            3 => Phase::Listing,
            4 => Phase::Listed,
            5 => Phase::Downloading,
            6 => Phase::Cancelling,
            _ => Phase::Idle,
        }
    }

    /// A child is (still) running: the dialog cannot be dismissed, the inputs
    /// are frozen.
    fn transfer_running(self) -> bool {
        matches!(self, Phase::Downloading | Phase::Cancelling)
    }

    /// A search or a listing is in flight (`AppState.hf_busy`).
    fn busy(self) -> bool {
        matches!(self, Phase::Searching | Phase::Listing)
    }

    /// Search, List and a result pick are open in every phase but a request
    /// in flight (app.slint disables the button then, and this mirrors it)
    /// and a running transfer.
    fn can_request(self) -> bool {
        !self.transfer_running() && !self.busy()
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
    /// The transfer in flight, for Cancel.
    handle: Option<hf::DownloadHandle>,
}

thread_local! {
    /// The shared `State`, for the worker results landing on the event loop
    /// (see the module header, Threading).
    static STATE: RefCell<std::rc::Weak<RefCell<State>>> = const { RefCell::new(std::rc::Weak::new()) };
}

fn shared_state() -> Option<Rc<RefCell<State>>> {
    STATE.with(|s| s.borrow().upgrade())
}

/// How often the progress readout may change while bytes flow.
const PROGRESS_INTERVAL: Duration = Duration::from_millis(250);

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

/// A search row's first line: the repository, its downloads, and whether it
/// is gated or private (both need a token to download).
fn result_label(h: &hf::SearchHit) -> String {
    let mut parts = vec![h.repo.clone(), format!("{} downloads", hf::format_count(h.downloads))];
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
/// prefix, the dialog shows them on their own.
fn sentence(s: &str) -> String {
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

/// One row of the candidate list: quant, size, shards, sidecars, and a mark on
/// the one `llama-server -hf` would pick.
fn candidate_label(c: &hf::Candidate, is_default: bool) -> String {
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
    parts.join("  ·  ")
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

fn percent(done: u64, total: u64) -> u64 {
    if total == 0 {
        0
    } else {
        (done.min(total) as u128 * 100 / total as u128) as u64
    }
}

fn fraction(done: u64, total: u64) -> f32 {
    if total == 0 {
        0.0
    } else {
        (done.min(total) as f64 / total as f64) as f32
    }
}

/// The progress readout: the overall line and bar, the current file's line
/// and bar.
#[derive(Debug, Clone, PartialEq)]
struct ProgressView {
    overall: String,
    file: String,
    overall_fraction: f32,
    file_fraction: f32,
}

fn progress_view(p: &hf::Progress) -> ProgressView {
    let current = (p.files_done + 1).min(p.files_total.max(1));
    let overall = format!(
        "{} of {} ({}%), file {current} of {}",
        hf::format_size(p.done),
        hf::format_size(p.total),
        percent(p.done, p.total),
        p.files_total
    );
    let file = if p.file.is_empty() {
        "Waiting for llama-server to start the transfer…".to_string()
    } else {
        let name = p.file.rsplit('/').next().unwrap_or(&p.file);
        format!(
            "{name}: {} of {} ({}%)",
            hf::format_size(p.file_done),
            hf::format_size(p.file_total),
            percent(p.file_done, p.file_total)
        )
    };
    ProgressView {
        overall,
        file,
        overall_fraction: fraction(p.done, p.total),
        file_fraction: fraction(p.file_done, p.file_total),
    }
}

/// The worker-side throttle for progress updates: the first one, every file
/// boundary (a new current file or one more done), the last byte, and
/// otherwise at most one per `interval`.
struct Coalescer {
    interval: Duration,
    last: Option<(Instant, String, usize)>,
}

impl Coalescer {
    fn new(interval: Duration) -> Coalescer {
        Coalescer { interval, last: None }
    }

    fn admit(&mut self, p: &hf::Progress, now: Instant) -> bool {
        let pass = match &self.last {
            None => true,
            Some((at, file, files_done)) => {
                *file != p.file
                    || *files_done != p.files_done
                    || (p.total > 0 && p.done >= p.total)
                    || now.duration_since(*at) >= self.interval
            }
        };
        if pass {
            self.last = Some((now, p.file.clone(), p.files_done));
        }
        pass
    }
}

/// What a failed (or cancelled) transfer leaves in the dialog: (error, note).
fn after_failure(e: &hf::HfError) -> (String, String) {
    match e {
        hf::HfError::Cancelled => (
            String::new(),
            "Download cancelled. What was fetched so far is kept: Resume download picks up where it stopped."
                .to_string(),
        ),
        _ => (dialog_error(e), String::new()),
    }
}

/// The note a refused dismiss shows.
const DISMISS_REFUSED: &str = "A download is running: Cancel download stops it (what is fetched is kept \
     for a resume). Closing the window instead leaves it running in the background.";

/// The footer line for a created preset (no token, no path: the id and what
/// came with it).
fn created_status(d: &hf::Downloaded, id: &str) -> String {
    let mut with: Vec<&str> = Vec::new();
    if d.mmproj.is_some() {
        with.push("mmproj");
    }
    if let Some((s, _)) = &d.draft {
        with.push(s.label());
    }
    let with = if with.is_empty() { String::new() } else { format!(" with {}", with.join(" and ")) };
    let name = d.model.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
    format!("Downloaded {name}{with} from {}: created [{id}], selected. Tweak parameters and Save.", d.repo)
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
    s.set_hf_listing_title(SharedString::default());
    s.set_hf_candidate_labels(string_model(Vec::new()));
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

/// Pick candidate `index`: its sidecar choices (mmproj on when it has one, no
/// drafter, as `llama-server -hf` does) and the summary.
fn select(app: &AppWindow, state: &Rc<RefCell<State>>, index: i32) {
    if phase(app).transfer_running() {
        return;
    }
    let s = app.global::<AppState>();
    let picked = usize::try_from(index)
        .ok()
        .and_then(|i| state.borrow().hf.listing.as_ref()?.candidates.get(i).cloned());
    let Some(c) = picked else {
        return;
    };
    s.set_hf_candidate_index(index);
    s.set_hf_mmproj_available(c.mmproj.is_some());
    s.set_hf_take_mmproj(c.mmproj.is_some());
    s.set_hf_drafter_options(string_model(drafter_options(&c)));
    s.set_hf_drafter(SharedString::from("none"));
    refresh_summary(app, state);
    refresh_resume(app, state, &models_dir());
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
    if !phase(app).transfer_running() {
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
    }
    s.set_hf_env_token(hf::token_from_env().is_some());
    s.set_show_hf_dialog(true);
}

fn dismiss(app: &AppWindow, state: &Rc<RefCell<State>>) {
    let s = app.global::<AppState>();
    if phase(app).transfer_running() {
        s.set_hf_note(SharedString::from(DISMISS_REFUSED));
        return;
    }
    // A search or a listing still in flight lands on a closed dialog: drop it.
    state.borrow_mut().hf.generation += 1;
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
    if phase(app).transfer_running() || phase(app).shows_results() || state.borrow().hf.results.is_empty() {
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

/// A listing landed (current generation).
fn listed(app: &AppWindow, state: &Rc<RefCell<State>>, result: Result<hf::Listing, hf::HfError>) {
    let s = app.global::<AppState>();
    match result {
        Ok(l) => {
            let labels: Vec<String> = l
                .candidates
                .iter()
                .enumerate()
                .map(|(i, c)| candidate_label(c, i == l.default))
                .collect();
            s.set_hf_listing_title(SharedString::from(listing_title(&l)));
            s.set_hf_candidate_labels(string_model(labels));
            let default = l.default as i32;
            state.borrow_mut().hf.listing = Some(l);
            set_phase(app, Phase::Listed);
            select(app, state, default);
        }
        Err(e) => {
            // From a picked row the results stay on screen, the error under them.
            let back = if state.borrow().hf.results.is_empty() { Phase::Idle } else { Phase::Searched };
            set_phase(app, back);
            s.set_hf_error(SharedString::from(dialog_error(&e)));
        }
    }
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
    let models_dir = models_dir();
    let job = {
        let st = state.borrow();
        let Some(listing) = st.hf.listing.as_ref() else {
            return;
        };
        hf::DownloadJob::new(
            listing,
            index,
            s.get_hf_take_mmproj(),
            drafter_choice(s.get_hf_drafter().as_str()),
            models_dir.clone(),
            token,
        )
    };
    let job = match job {
        Ok(j) => j,
        Err(e) => {
            s.set_hf_error(SharedString::from(dialog_error(&e)));
            return;
        }
    };
    s.set_hf_error(SharedString::default());
    s.set_hf_note(SharedString::default());
    s.set_hf_progress_overall(SharedString::from("Starting…"));
    s.set_hf_progress_file(SharedString::default());
    s.set_hf_progress_fraction(0.0);
    s.set_hf_file_fraction(0.0);
    set_phase(app, Phase::Downloading);
    set_status(app, format!("Downloading {} from Hugging Face…", job.candidate.primary().name()), false);

    let progress_weak = app.as_weak();
    let done_weak = app.as_weak();
    let mut coalescer = Coalescer::new(PROGRESS_INTERVAL);
    let handle = hf::spawn_download(
        job,
        move |p| {
            if !coalescer.admit(&p, Instant::now()) {
                return;
            }
            let view = progress_view(&p);
            let weak = progress_weak.clone();
            slint::invoke_from_event_loop(move || {
                let Some(app) = weak.upgrade() else {
                    return;
                };
                let s = app.global::<AppState>();
                if !phase(&app).transfer_running() {
                    return; // a late update after the end
                }
                s.set_hf_progress_overall(SharedString::from(view.overall));
                s.set_hf_progress_file(SharedString::from(view.file));
                s.set_hf_progress_fraction(view.overall_fraction);
                s.set_hf_file_fraction(view.file_fraction);
            })
            .ok();
        },
        move |result| {
            slint::invoke_from_event_loop(move || {
                let (Some(app), Some(state)) = (done_weak.upgrade(), shared_state()) else {
                    return;
                };
                downloaded(&app, &state, &models_dir, result);
            })
            .ok();
        },
    );
    state.borrow_mut().hf.handle = Some(handle);
}

/// The transfer ended.
fn downloaded(
    app: &AppWindow,
    state: &Rc<RefCell<State>>,
    models_dir: &Path,
    result: Result<hf::Downloaded, hf::HfError>,
) {
    let s = app.global::<AppState>();
    state.borrow_mut().hf.handle = None;
    match result {
        Ok(d) => {
            set_phase(app, Phase::Idle);
            s.set_show_hf_dialog(false);
            let existing: Vec<String> = presets::load_listed().into_iter().map(|l| l.preset.id).collect();
            let p = hf::preset_from_download(&d, &existing);
            let status = created_status(&d, &p.id);
            models_tab::commit_downloaded_preset(app, state, p, models_dir, status);
        }
        Err(e) => {
            let (error, note) = after_failure(&e);
            set_phase(app, Phase::Listed);
            s.set_hf_error(SharedString::from(error));
            s.set_hf_note(SharedString::from(note));
            refresh_resume(app, state, models_dir);
            let footer = if e == hf::HfError::Cancelled {
                ("Hugging Face download cancelled.".to_string(), false)
            } else {
                (format!("Hugging Face download failed: {e}"), true)
            };
            set_status(app, footer.0, footer.1);
        }
    }
}

/// TEST-ONLY seam for the e2e flow (src/tests/save_flow.rs): land a transfer's
/// result exactly as the worker's closure does, without a network or a child.
#[cfg(test)]
pub(crate) fn land_download_for_tests(
    app: &AppWindow,
    models_dir: &Path,
    result: Result<hf::Downloaded, hf::HfError>,
) {
    let state = shared_state().expect("hf_dialog::wire ran");
    downloaded(app, &state, models_dir, result);
}

fn cancel(app: &AppWindow, state: &Rc<RefCell<State>>) {
    if phase(app) != Phase::Downloading {
        return;
    }
    if let Some(h) = state.borrow().hf.handle.as_ref() {
        h.cancel();
    }
    set_phase(app, Phase::Cancelling);
    app.global::<AppState>()
        .set_hf_note(SharedString::from("Stopping the download…"));
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
    s.on_hf_cancel(hook(app, state, cancel));
    s.on_hf_choices_changed(hook(app, state, refresh_summary));

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
    use hf::{Candidate, HfError, Progress, RepoFile, Sidecar};

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

    const ALL: [Phase; 7] = [
        Phase::Idle,
        Phase::Searching,
        Phase::Searched,
        Phase::Listing,
        Phase::Listed,
        Phase::Downloading,
        Phase::Cancelling,
    ];

    #[test]
    fn phase_numbering_is_the_ui_contract() {
        for p in ALL {
            assert_eq!(Phase::from_int(p as i32), p);
        }
        assert_eq!(Phase::from_int(99), Phase::Idle);
        for p in ALL {
            let n = p as i32;
            // state.slint: `hf_transfer_running: hf_phase >= 5`,
            // `hf_busy: hf_phase == 1 || hf_phase == 3`, `hf_show_results: hf_phase <= 2`.
            assert_eq!(p.transfer_running(), n >= 5, "{p:?}");
            assert_eq!(p.busy(), n == 1 || n == 3, "{p:?}");
            assert_eq!(p.shows_results(), n <= 2, "{p:?}");
        }
    }

    /// Which phase accepts which step: one request at a time, nothing during
    /// a transfer, and the result rows only up to `Searched`.
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
        }
    }

    #[test]
    fn result_rows_carry_repo_downloads_access_and_quants() {
        let mut h = hit("Qwen/Qwen3-8B-GGUF", 644_587, &["Q4_K_M", "Q5_K_M", "Q8_0"]);
        assert_eq!(result_label(&h), "Qwen/Qwen3-8B-GGUF  ·  644.6k downloads");
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
        assert_eq!(candidate_label(&c, false), "Q4_K_M  ·  4.00 GiB");
        c.mmproj = Some(file("mmproj-F16.gguf", 600 << 20));
        c.mtp = Some(file("mtp-Q8_0.gguf", 300 << 20));
        assert_eq!(candidate_label(&c, true), "Q4_K_M  ·  4.00 GiB  ·  with mmproj, MTP  ·  default");
        let c = candidate("BF16", 2);
        assert_eq!(candidate_label(&c, false), "BF16  ·  4.00 GiB  ·  2 shards");
        let mut c = candidate("", 1);
        c.shards[0].path = "dir/model.gguf".into();
        assert!(candidate_label(&c, false).starts_with("model.gguf  ·  "));
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

    fn progress(done: u64, total: u64, file: &str, fd: u64, ft: u64, files: (usize, usize)) -> Progress {
        Progress {
            done,
            total,
            file: file.into(),
            file_done: fd,
            file_total: ft,
            files_done: files.0,
            files_total: files.1,
        }
    }

    #[test]
    fn the_progress_readout_shows_overall_and_per_file_bytes() {
        let v = progress_view(&progress(0, 0, "", 0, 0, (0, 2)));
        assert_eq!(v.overall, "0.0 MiB of 0.0 MiB (0%), file 1 of 2");
        assert!(v.file.starts_with("Waiting"));
        assert_eq!((v.overall_fraction, v.file_fraction), (0.0, 0.0));

        let gib = 1u64 << 30;
        let v = progress_view(&progress(gib, 4 * gib, "sub/a.gguf", gib / 2, 2 * gib, (1, 3)));
        assert_eq!(v.overall, "1.00 GiB of 4.00 GiB (25%), file 2 of 3");
        assert_eq!(v.file, "a.gguf: 512.0 MiB of 2.00 GiB (25%)");
        assert_eq!((v.overall_fraction, v.file_fraction), (0.25, 0.25));

        // The end: never "file 4 of 3", never past 100%.
        let v = progress_view(&progress(5 * gib, 4 * gib, "a.gguf", 2 * gib, 2 * gib, (3, 3)));
        assert!(v.overall.ends_with("(100%), file 3 of 3"), "{}", v.overall);
        assert_eq!(v.overall_fraction, 1.0);
    }

    #[test]
    fn the_coalescer_passes_boundaries_and_throttles_the_rest() {
        let t0 = Instant::now();
        let ms = |n| t0 + Duration::from_millis(n);
        let mut c = Coalescer::new(Duration::from_millis(250));
        assert!(c.admit(&progress(0, 100, "", 0, 0, (0, 2)), ms(0)), "the first one");
        assert!(c.admit(&progress(1, 100, "a", 1, 50, (0, 2)), ms(10)), "a new current file");
        assert!(!c.admit(&progress(2, 100, "a", 2, 50, (0, 2)), ms(20)), "throttled");
        assert!(!c.admit(&progress(3, 100, "a", 3, 50, (0, 2)), ms(259)), "still inside the interval");
        assert!(c.admit(&progress(4, 100, "a", 4, 50, (0, 2)), ms(260)), "the interval passed");
        assert!(c.admit(&progress(50, 100, "a", 50, 50, (1, 2)), ms(270)), "one more file done");
        assert!(c.admit(&progress(100, 100, "b", 50, 50, (1, 2)), ms(280)), "new file");
        assert!(c.admit(&progress(100, 100, "b", 50, 50, (2, 2)), ms(281)), "the end always passes");
        assert!(c.admit(&progress(100, 100, "b", 50, 50, (2, 2)), ms(282)), "the last byte always passes");
    }

    #[test]
    fn a_cancel_is_a_note_and_a_failure_an_error() {
        let (error, note) = after_failure(&HfError::Cancelled);
        assert!(error.is_empty() && note.contains("Resume download"), "{note}");
        let (error, note) = after_failure(&HfError::DiskFull("no space".into()));
        assert_eq!((error.as_str(), note.as_str()), ("The disk is probably full: no space", ""));
    }

    #[test]
    fn the_created_status_names_the_file_the_extras_and_the_id() {
        let d = hf::Downloaded {
            repo: "org/Repo-GGUF".into(),
            project_url: "https://huggingface.co/org/Repo-GGUF".into(),
            model: PathBuf::from("M").join("models").join("Repo-GGUF").join("m-Q4_K_M.gguf"),
            mmproj: Some(PathBuf::from("mm.gguf")),
            draft: Some((Sidecar::Mtp, PathBuf::from("mtp.gguf"))),
        };
        assert_eq!(
            created_status(&d, "m-Q4_K_M"),
            "Downloaded m-Q4_K_M.gguf with mmproj and MTP from org/Repo-GGUF: created [m-Q4_K_M], selected. \
             Tweak parameters and Save."
        );
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
