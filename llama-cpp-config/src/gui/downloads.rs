//! The Hugging Face downloads running in the background, and their window.
//!
//! The New preset dialog's Hugging Face source (`gui/hf_dialog.rs`) keeps the
//! job up to the choice: search, listing, estimates, the pick, its sidecars and
//! the token. Its Download hands the job HERE (`start`) and closes, so the main
//! window is usable again at once: the transfer (`hf::spawn_download`), the
//! preset's derivation from the downloaded headers (`hf::preset_from_download`,
//! on a worker thread too) and its save all happen behind it.
//!
//! ## The window
//! `DownloadsWindow` (ui/downloads_window.slint) is an independent root like
//! `LogWindow` and `HwWindow`: non-modal, hidden on close (closing it cancels
//! nothing), its size remembered (`DownloadsWindowSize`). One card per job,
//! newest first: the repository and the quantization, what it fetches, the
//! state (Downloading, Cancelling, Creating the preset, Done, Failed,
//! Cancelled), the overall and per-file progress while bytes flow, the outcome,
//! and Cancel download / Resume / Show preset / Dismiss, each acting on the
//! job's id. It opens by itself when a download starts, and comes back from
//! the footer of the main window (`AppState.downloads_summary`, shown while
//! the window has a row; the click is `AppState.open_downloads`), from the
//! New-from-Hugging-Face dialog's Downloads button (the modal backdrop covers
//! the footer) and from the tray's Open downloads.
//!
//! ## Concurrency
//! Several downloads may run at once, each its own child and its own staging
//! tree, EXCEPT two of the same repository: the staging tree is per repository
//! (`<ModelsDir>/.hf-staging/models--org--repo`), and two children writing it
//! would race. `Jobs::busy_with` refuses the second (the dialog shows why, and
//! a Resume says so in its row) while the first is downloading, cancelling or
//! creating its preset.
//!
//! ## A finished download never disturbs the editor
//! The preset is saved (`hf::save_preset`) and the Models tab refreshed the way
//! the on/off switch refreshes it: `refresh_preset_list` (the list, the
//! selection kept on the preset being edited, by id; the form untouched, dirty
//! or not), the pickers rescanned (`refresh_file_options`, so the new files are
//! listed), and opencode.json and the Benchmark tab followed
//! (`models_tab::presets_moved`). The selection never jumps: the footer says
//! the preset was created, and the row's **Show preset** goes to it. Show
//! raises the main window on the Models tab and selects the preset through the
//! same discard guard a click in the list takes (a dirty form asks first); a
//! dialog open in the main window makes it refuse with a note in the row
//! instead, and a list filter that would hide the row is cleared. The id is
//! re-derived against the presets as they stand at save time, since another
//! one may have taken it while the headers were read.
//!
//! ## Quit and hide
//! Hiding the main window to the tray changes nothing: the transfers run on
//! their own threads. The tray's Quit asks first while any job is active
//! (`request_quit`): the window comes up with a strip saying what quitting
//! does; "Stop downloads and quit" cancels every transfer cooperatively (the
//! child's stdin is closed, `DownloadHandle::cancel`, so llama.cpp keeps its
//! partial for a resume) and the event loop ends once the last one has landed,
//! or after `QUIT_GRACE` at worst (the kill follows a cancel after 10 s, and
//! the process exit closes any pipe still open, which the child also reads as
//! a stop). Nothing is ever orphaned.
//!
//! ## Threading and the token
//! Worker results land through `slint::invoke_from_event_loop` +
//! `Weak::upgrade`, reaching `State` through the thread-local handle
//! `hf_dialog::wire` sets (`hf_dialog::shared_state`). Progress is coalesced on
//! the worker (`Coalescer`: at most one update per `PROGRESS_INTERVAL`, plus
//! every file boundary and the last byte), and the rows are rewritten in place
//! (`sync_rows`), so a click on a button is never lost to a rebuilt row. The
//! token stays inside the `hf::DownloadJob` kept for a Resume: no row, status
//! line, title or preset carries it (`hf::Token` has no `Display`).
//!
//! Under `cfg(test)` no child and no thread is started: the e2e flow lands
//! the progress and the transfer's result itself (`land_progress_for_tests`,
//! `land_transfer_for_tests`), and the preset derivation runs inline. The pure
//! job list (`Jobs`) and the progress text are unit-tested below.

use super::*;

use std::collections::HashMap;
use std::time::{Duration, Instant};

use crate::hf;
use crate::hf_fit;

/// How often a transfer's progress may change on screen.
const PROGRESS_INTERVAL: Duration = Duration::from_millis(250);

/// The longest a confirmed Quit waits for the transfers to stop: the kill
/// follows a cancel after 10 s (`hf::DownloadHandle::cancel`).
#[cfg(not(test))]
const QUIT_GRACE: Duration = Duration::from_secs(15);

// ── Pure parts: progress text ────────────────────────────────────────────

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

/// A row's progress readout: the overall line and bar, the current file's
/// line and bar.
#[derive(Debug, Clone, PartialEq)]
pub(super) struct ProgressView {
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

/// The footer line for a created preset (no token, no path: the id and what
/// came with it). The preset is NOT selected: Show preset goes to it.
fn created_status(d: &hf::Downloaded, id: &str, placed: bool) -> String {
    let mut with: Vec<&str> = Vec::new();
    if d.mmproj.is_some() {
        with.push("mmproj");
    }
    if let Some((s, _)) = &d.draft {
        with.push(s.label());
    }
    let with = if with.is_empty() { String::new() } else { format!(" with {}", with.join(" and ")) };
    let name = d.model.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
    let placed = if placed { ", placed on the GPUs as estimated" } else { "" };
    format!("Downloaded {name}{with} from {}: created [{id}]{placed}. Show preset (Downloads) opens it.", d.repo)
}

// ── Pure parts: the job list ─────────────────────────────────────────────

/// Where one job stands.
#[derive(Debug, Clone, PartialEq)]
pub(super) enum JobState {
    /// The child is fetching.
    Downloading,
    /// Cancel was asked; the child has not stopped yet.
    Cancelling,
    /// The files are in place; the preset is being derived and saved.
    Configuring,
    /// The preset `preset` was created; `placed`: with the estimated split.
    Done { preset: String, placed: bool },
    /// The transfer or the save failed: the error, as the row shows it.
    Failed(String),
    /// Cancelled; the partial files are kept for a resume.
    Cancelled,
}

impl JobState {
    /// A child runs, or its preset is still being made: the repository is
    /// busy, and Quit asks first.
    fn active(&self) -> bool {
        matches!(self, JobState::Downloading | JobState::Cancelling | JobState::Configuring)
    }

    fn label(&self) -> &'static str {
        match self {
            JobState::Downloading => "Downloading",
            JobState::Cancelling => "Cancelling…",
            JobState::Configuring => "Creating the preset…",
            JobState::Done { .. } => "Done",
            JobState::Failed(_) => "Failed",
            JobState::Cancelled => "Cancelled",
        }
    }
}

const CANCELLED: &str = "Cancelled. What was fetched so far is kept: Resume picks up where it stopped.";

#[derive(Debug, Clone)]
struct Job {
    id: u64,
    repo: String,
    title: String,
    detail: String,
    state: JobState,
    /// The last progress readout; `None` before the first one.
    progress: Option<ProgressView>,
    /// A one-off note under the outcome (a Show or a Resume that could not run).
    note: String,
}

/// The downloads the window lists, newest first, and what can be done with
/// each. Pure: `Downloads` drives the children and the UI around it.
#[derive(Debug, Default)]
pub(super) struct Jobs {
    next: u64,
    list: Vec<Job>,
}

impl Jobs {
    /// Another job of `repo` is active: a second one would share its staging
    /// tree. Repository ids compare without case, as Hugging Face resolves them.
    pub(super) fn busy_with(&self, repo: &str) -> bool {
        self.list.iter().any(|j| j.state.active() && j.repo.eq_ignore_ascii_case(repo))
    }

    /// A new job, downloading; its id.
    fn add(&mut self, repo: &str, title: String, detail: String) -> u64 {
        self.next += 1;
        let job = Job {
            id: self.next,
            repo: repo.to_string(),
            title,
            detail,
            state: JobState::Downloading,
            progress: None,
            note: String::new(),
        };
        self.list.insert(0, job);
        self.next
    }

    fn get(&self, id: u64) -> Option<&Job> {
        self.list.iter().find(|j| j.id == id)
    }

    fn get_mut(&mut self, id: u64) -> Option<&mut Job> {
        self.list.iter_mut().find(|j| j.id == id)
    }

    /// A progress readout; ignored once the transfer has ended (a late one).
    fn progress(&mut self, id: u64, view: ProgressView) -> bool {
        match self.get_mut(id) {
            Some(j) if matches!(j.state, JobState::Downloading | JobState::Cancelling) => {
                j.progress = Some(view);
                true
            }
            _ => false,
        }
    }

    /// Cancel asked: only a job that is downloading can be.
    fn cancel(&mut self, id: u64) -> bool {
        match self.get_mut(id) {
            Some(j) if j.state == JobState::Downloading => {
                j.state = JobState::Cancelling;
                true
            }
            _ => false,
        }
    }

    /// The transfer ended: on to the preset, or the cancel, or the error.
    fn transfer_ended(&mut self, id: u64, result: Result<(), &hf::HfError>) -> bool {
        let Some(j) = self.get_mut(id) else {
            return false;
        };
        if !matches!(j.state, JobState::Downloading | JobState::Cancelling) {
            return false;
        }
        j.state = match result {
            // A cancel that came too late to stop it still gets its preset.
            Ok(()) => JobState::Configuring,
            Err(hf::HfError::Cancelled) => JobState::Cancelled,
            Err(e) => JobState::Failed(hf_dialog::sentence(&e.to_string())),
        };
        j.progress = None;
        true
    }

    /// The preset was saved (its id, and whether it carries the estimated
    /// placement), or could not be.
    fn configured(&mut self, id: u64, result: Result<(String, bool), String>) -> bool {
        let Some(j) = self.get_mut(id) else {
            return false;
        };
        if j.state != JobState::Configuring {
            return false;
        }
        j.state = match result {
            Ok((preset, placed)) => JobState::Done { preset, placed },
            Err(e) => JobState::Failed(e),
        };
        true
    }

    /// Resume a failed or cancelled job: downloading again, from what is on
    /// disk. Refused (with the row's note) while another job of the same
    /// repository is active.
    fn restart(&mut self, id: u64) -> Result<(), String> {
        let repo = match self.get(id) {
            Some(j) if matches!(j.state, JobState::Failed(_) | JobState::Cancelled) => j.repo.clone(),
            _ => return Err(String::new()),
        };
        if self.busy_with(&repo) {
            return Err(format!("{repo} is already downloading: Resume once that download ends."));
        }
        if let Some(j) = self.get_mut(id) {
            j.state = JobState::Downloading;
            j.progress = None;
            j.note.clear();
        }
        Ok(())
    }

    /// Drop a finished row.
    fn dismiss(&mut self, id: u64) -> bool {
        let before = self.list.len();
        self.list.retain(|j| j.id != id || j.state.active());
        self.list.len() != before
    }

    /// Drop every finished row; the active ones stay.
    fn clear_finished(&mut self) -> Vec<u64> {
        let gone: Vec<u64> = self.list.iter().filter(|j| !j.state.active()).map(|j| j.id).collect();
        self.list.retain(|j| j.state.active());
        gone
    }

    fn set_note(&mut self, id: u64, note: &str) {
        if let Some(j) = self.get_mut(id) {
            j.note = note.to_string();
        }
    }

    /// The active jobs (what Quit would stop).
    fn active(&self) -> usize {
        self.list.iter().filter(|j| j.state.active()).count()
    }

    /// The jobs still transferring, for a confirmed Quit to cancel.
    fn downloading(&self) -> Vec<u64> {
        self.list.iter().filter(|j| j.state == JobState::Downloading).map(|j| j.id).collect()
    }

    fn preset_of(&self, id: u64) -> Option<String> {
        match &self.get(id)?.state {
            JobState::Done { preset, .. } => Some(preset.clone()),
            _ => None,
        }
    }

    fn title_of(&self, id: u64) -> String {
        self.get(id).map(|j| j.title.clone()).unwrap_or_default()
    }

    /// The window's rows, as Slint shows them.
    fn rows(&self) -> Vec<DownloadRow> {
        self.list.iter().map(row).collect()
    }

    /// The footer indicator: "" with no row, else what runs or how they ended.
    fn summary(&self) -> String {
        let active = self.active();
        if active > 0 {
            return if active == 1 { "1 download running".into() } else { format!("{active} downloads running") };
        }
        let count = |f: fn(&JobState) -> bool| self.list.iter().filter(|j| f(&j.state)).count();
        let parts: Vec<String> = [
            (count(|s| matches!(s, JobState::Done { .. })), "done"),
            (count(|s| matches!(s, JobState::Failed(_))), "failed"),
            (count(|s| *s == JobState::Cancelled), "cancelled"),
        ]
        .into_iter()
        .filter(|(n, _)| *n > 0)
        .map(|(n, what)| format!("{n} {what}"))
        .collect();
        if parts.is_empty() {
            String::new()
        } else {
            format!("Downloads: {}", parts.join(", "))
        }
    }
}

/// One job as its window row.
fn row(j: &Job) -> DownloadRow {
    let transferring = matches!(j.state, JobState::Downloading | JobState::Cancelling);
    let (overall, file, of, ff) = match (&j.progress, transferring) {
        (Some(p), true) => (p.overall.clone(), p.file.clone(), p.overall_fraction, p.file_fraction),
        (None, true) => ("Starting…".to_string(), String::new(), 0.0, 0.0),
        _ => (String::new(), String::new(), 0.0, 0.0),
    };
    let (message, is_error) = match &j.state {
        JobState::Done { preset, placed: true } => (
            format!("Created preset [{preset}], placed on the GPUs as estimated: Show preset opens it to tune it and Save."),
            false,
        ),
        JobState::Done { preset, placed: false } => {
            (format!("Created preset [{preset}]: Show preset opens it in the Models tab."), false)
        }
        JobState::Failed(e) => (e.clone(), true),
        JobState::Cancelled => (CANCELLED.to_string(), false),
        _ => (String::new(), false),
    };
    DownloadRow {
        id: i32::try_from(j.id).unwrap_or(i32::MAX),
        title: SharedString::from(j.title.as_str()),
        detail: SharedString::from(j.detail.as_str()),
        state: SharedString::from(j.state.label()),
        show_progress: transferring,
        overall: SharedString::from(overall),
        file: SharedString::from(file),
        overall_fraction: of,
        file_fraction: ff,
        message: SharedString::from(message),
        is_error,
        note: SharedString::from(j.note.as_str()),
        can_cancel: j.state == JobState::Downloading,
        can_resume: matches!(j.state, JobState::Failed(_) | JobState::Cancelled),
        can_show: matches!(j.state, JobState::Done { .. }),
        can_dismiss: !j.state.active(),
    }
}

// ── The Rust half (`gui::State.downloads`) ───────────────────────────────

/// What a job needs beyond its row: the job itself (kept for a Resume, the
/// token inside it), the transfer's handle while it runs, and the budget the
/// dialog estimated against (the preset gets the placement its row showed).
struct Run {
    job: hf::DownloadJob,
    handle: Option<hf::DownloadHandle>,
    budget: Option<hf_fit::Budget>,
}

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
enum QuitState {
    #[default]
    None,
    /// The tray's Quit is waiting for an answer in the window.
    Asking,
    /// Confirmed: the transfers are stopping; the loop ends when they have.
    Quitting,
}

#[derive(Default)]
pub(super) struct Downloads {
    jobs: Jobs,
    runs: HashMap<u64, Run>,
    window: Option<DownloadsWindow>,
    rows: Rc<VecModel<DownloadRow>>,
    quit: QuitState,
}

impl Downloads {
    /// A download of `repo` is active (the dialog refuses a second).
    pub(super) fn busy_with(&self, repo: &str) -> bool {
        self.jobs.busy_with(repo)
    }
}

fn id_of(v: i32) -> Option<u64> {
    u64::try_from(v).ok()
}

/// Rewrite the window's rows IN PLACE where the count is unchanged, so a row
/// is never rebuilt under the pointer by a progress tick.
fn sync_rows(model: &VecModel<DownloadRow>, rows: Vec<DownloadRow>) {
    if model.row_count() != rows.len() {
        model.set_vec(rows);
        return;
    }
    for (i, r) in rows.into_iter().enumerate() {
        if model.row_data(i).as_ref() != Some(&r) {
            model.set_row_data(i, r);
        }
    }
}

/// Push the job list to the window and the footer.
fn push(app: &AppWindow, state: &Rc<RefCell<State>>) {
    let st = state.borrow();
    let d = &st.downloads;
    sync_rows(&d.rows, d.jobs.rows());
    app.global::<AppState>().set_downloads_summary(SharedString::from(d.jobs.summary()));
    if let Some(w) = &d.window {
        w.set_quit_pending(d.quit == QuitState::Asking);
        w.set_quitting(d.quit == QuitState::Quitting);
        w.set_quit_active(i32::try_from(d.jobs.active()).unwrap_or(i32::MAX));
    }
}

/// Show (or raise) the window.
fn open(state: &Rc<RefCell<State>>) {
    let win = state.borrow().downloads.window.as_ref().map(|w| w.clone_strong());
    if let Some(w) = win {
        raise_window(w.window());
    }
}

/// Hand a download over from the dialog: a new row, the transfer started,
/// the window up. The caller has checked `busy_with`.
pub(super) fn start(
    app: &AppWindow,
    state: &Rc<RefCell<State>>,
    job: hf::DownloadJob,
    title: String,
    detail: String,
    budget: Option<hf_fit::Budget>,
) -> u64 {
    let id = {
        let mut st = state.borrow_mut();
        let d = &mut st.downloads;
        let id = d.jobs.add(&job.repo, title, detail);
        d.runs.insert(id, Run { job, handle: None, budget });
        id
    };
    spawn(app, state, id);
    push(app, state);
    open(state);
    id
}

/// Start (or restart) job `id`'s transfer. Nothing is started under
/// `cfg(test)`: the flow test lands the results itself.
fn spawn(app: &AppWindow, state: &Rc<RefCell<State>>, id: u64) {
    let Some(job) = state.borrow().downloads.runs.get(&id).map(|r| r.job.clone()) else {
        return;
    };
    #[cfg(not(test))]
    {
        let handle = spawn_transfer(app, id, job);
        if let Some(r) = state.borrow_mut().downloads.runs.get_mut(&id) {
            r.handle = Some(handle);
        }
    }
    #[cfg(test)]
    {
        let _ = (app, job);
    }
}

#[cfg(not(test))]
fn spawn_transfer(app: &AppWindow, id: u64, job: hf::DownloadJob) -> hf::DownloadHandle {
    let progress_weak = app.as_weak();
    let done_weak = app.as_weak();
    let mut coalescer = Coalescer::new(PROGRESS_INTERVAL);
    hf::spawn_download(
        job,
        move |p| {
            if !coalescer.admit(&p, Instant::now()) {
                return;
            }
            let view = progress_view(&p);
            let weak = progress_weak.clone();
            slint::invoke_from_event_loop(move || {
                let (Some(app), Some(state)) = (weak.upgrade(), hf_dialog::shared_state()) else {
                    return;
                };
                progressed(&app, &state, id, view);
            })
            .ok();
        },
        move |result| {
            slint::invoke_from_event_loop(move || {
                let (Some(app), Some(state)) = (done_weak.upgrade(), hf_dialog::shared_state()) else {
                    return;
                };
                transfer_ended(&app, &state, id, result);
            })
            .ok();
        },
    )
}

fn progressed(app: &AppWindow, state: &Rc<RefCell<State>>, id: u64, view: ProgressView) {
    if state.borrow_mut().downloads.jobs.progress(id, view) {
        push(app, state);
    }
}

/// A transfer ended (on the event loop).
fn transfer_ended(app: &AppWindow, state: &Rc<RefCell<State>>, id: u64, result: Result<hf::Downloaded, hf::HfError>) {
    let (moved, title) = {
        let mut st = state.borrow_mut();
        let d = &mut st.downloads;
        if let Some(r) = d.runs.get_mut(&id) {
            r.handle = None;
        }
        (d.jobs.transfer_ended(id, result.as_ref().map(|_| ())), d.jobs.title_of(id))
    };
    if !moved {
        return;
    }
    match result {
        Ok(downloaded) => configure(app, state, id, downloaded),
        Err(hf::HfError::Cancelled) => set_status(app, format!("Hugging Face download of {title} cancelled."), false),
        Err(e) => set_status(app, format!("Hugging Face download of {title} failed: {e}"), true),
    }
    push(app, state);
    maybe_quit(state);
}

/// Derive the preset from the downloaded files' headers, off the event loop
/// (inline under `cfg(test)`), then land it in `configured`.
fn configure(app: &AppWindow, state: &Rc<RefCell<State>>, id: u64, d: hf::Downloaded) {
    let existing: Vec<String> = state.borrow().presets.iter().map(|l| l.preset.id.clone()).collect();
    let budget = state.borrow().downloads.runs.get(&id).and_then(|r| r.budget.clone());
    let work = move || {
        let p = hf::preset_from_download(&d, &existing, budget.as_ref());
        (d, p)
    };
    #[cfg(not(test))]
    {
        let weak = app.as_weak();
        std::thread::spawn(move || {
            let (d, p) = work();
            slint::invoke_from_event_loop(move || {
                let (Some(app), Some(state)) = (weak.upgrade(), hf_dialog::shared_state()) else {
                    return;
                };
                configured(&app, &state, id, d, p);
            })
            .ok();
        });
    }
    #[cfg(test)]
    {
        let (d, p) = work();
        configured(app, state, id, d, p);
    }
}

/// The derived preset landed: save it and refresh what lists presets and
/// files, without touching the editor (module header).
fn configured(app: &AppWindow, state: &Rc<RefCell<State>>, id: u64, d: hf::Downloaded, mut p: presets::Preset) {
    let Some(models_dir) = state.borrow().downloads.runs.get(&id).map(|r| r.job.models_dir.clone()) else {
        return;
    };
    // Another preset may have taken the id while the headers were read.
    let ids: Vec<String> = presets::load_listed().into_iter().map(|l| l.preset.id).collect();
    p.id = presets::new_preset_id(&d.model.to_string_lossy(), &ids);
    let placed = !p.placement_note.is_empty();
    match hf::save_preset(&p, &models_dir) {
        Ok(()) => {
            {
                let mut st = state.borrow_mut();
                st.downloads.jobs.configured(id, Ok((p.id.clone(), placed)));
                st.downloads.runs.remove(&id);
            }
            // Status first, like the other write paths, so a failed
            // opencode.json follow-up is not papered over by the success.
            set_status(app, created_status(&d, &p.id, placed), false);
            refresh_preset_list(app, state);
            refresh_file_options(app, state);
            models_tab::presets_moved(app, state);
        }
        Err(e) => {
            state
                .borrow_mut()
                .downloads
                .jobs
                .configured(id, Err(format!("The files are in place, but the preset could not be saved: {e}")));
            set_status(app, format!("Hugging Face download: the preset could not be saved: {e}"), true);
        }
    }
    push(app, state);
    maybe_quit(state);
}

fn cancel(app: &AppWindow, state: &Rc<RefCell<State>>, id: u64) {
    let handle = {
        let mut st = state.borrow_mut();
        let d = &mut st.downloads;
        if !d.jobs.cancel(id) {
            return;
        }
        d.runs.get(&id).and_then(|r| r.handle.clone())
    };
    if let Some(h) = handle {
        h.cancel();
    }
    push(app, state);
}

fn resume(app: &AppWindow, state: &Rc<RefCell<State>>, id: u64) {
    if state.borrow().downloads.quit == QuitState::Quitting {
        return;
    }
    let restarted = state.borrow_mut().downloads.jobs.restart(id);
    match restarted {
        Ok(()) => spawn(app, state, id),
        Err(note) => state.borrow_mut().downloads.jobs.set_note(id, &note),
    }
    push(app, state);
}

/// Show preset: the main window up on the Models tab, the preset selected
/// through the discard guard (module header).
fn show_preset(app: &AppWindow, state: &Rc<RefCell<State>>, id: u64) {
    let Some(target) = state.borrow().downloads.jobs.preset_of(id) else {
        return;
    };
    raise_window(app.window());
    let s = app.global::<AppState>();
    let note = if s.get_modal_open() {
        "A dialog is open in the configurator: close it, then Show preset again.".to_string()
    } else {
        s.set_current_tab(1);
        String::new()
    };
    let position = |state: &Rc<RefCell<State>>| state.borrow().presets.iter().position(|l| l.preset.id == target);
    let note = match (note.is_empty(), position(state)) {
        (false, _) => note,
        (true, None) => format!("[{target}] is no longer listed: renamed or deleted since."),
        (true, Some(_)) => String::new(),
    };
    state.borrow_mut().downloads.jobs.set_note(id, &note);
    push(app, state);
    if !note.is_empty() || s.get_form().id.as_str() == target {
        return; // refused, or already the preset in the editor
    }
    // A filter that hides the row would leave the selection invisible.
    let hidden = state
        .borrow()
        .presets
        .iter()
        .find(|l| l.preset.id == target)
        .is_some_and(|l| !preset_matches(&l.preset, s.get_presets_filter().as_str()));
    if hidden {
        s.set_presets_filter(SharedString::default());
        refresh_preset_list(app, state);
    }
    if let Some(index) = position(state) {
        s.invoke_select_preset(i32::try_from(index).unwrap_or(-1));
    }
}

fn dismiss(app: &AppWindow, state: &Rc<RefCell<State>>, id: u64) {
    let gone = {
        let mut st = state.borrow_mut();
        let d = &mut st.downloads;
        let gone = d.jobs.dismiss(id);
        if gone {
            d.runs.remove(&id);
        }
        gone
    };
    if gone {
        push(app, state);
    }
}

fn clear_finished(app: &AppWindow, state: &Rc<RefCell<State>>) {
    {
        let mut st = state.borrow_mut();
        let d = &mut st.downloads;
        for id in d.jobs.clear_finished() {
            d.runs.remove(&id);
        }
    }
    push(app, state);
}

/// The tray's Quit: at once when nothing is active, else ask in the window.
pub(super) fn request_quit(app: &AppWindow, state: &Rc<RefCell<State>>) {
    if state.borrow().downloads.jobs.active() == 0 {
        quit_now();
        return;
    }
    {
        let mut st = state.borrow_mut();
        if st.downloads.quit == QuitState::None {
            st.downloads.quit = QuitState::Asking;
        }
    }
    push(app, state);
    open(state);
}

/// "Stop downloads and quit": cancel every transfer; the loop ends when the
/// last job has landed (`maybe_quit`), or after `QUIT_GRACE`.
fn confirm_quit(app: &AppWindow, state: &Rc<RefCell<State>>) {
    let handles: Vec<hf::DownloadHandle> = {
        let mut st = state.borrow_mut();
        let d = &mut st.downloads;
        d.quit = QuitState::Quitting;
        let ids = d.jobs.downloading();
        for id in &ids {
            d.jobs.cancel(*id);
        }
        ids.iter().filter_map(|id| d.runs.get(id)?.handle.clone()).collect()
    };
    for h in handles {
        h.cancel();
    }
    push(app, state);
    #[cfg(not(test))]
    slint::Timer::single_shot(QUIT_GRACE, quit_now);
    maybe_quit(state);
}

fn keep_running(app: &AppWindow, state: &Rc<RefCell<State>>) {
    {
        let mut st = state.borrow_mut();
        if st.downloads.quit == QuitState::Asking {
            st.downloads.quit = QuitState::None;
        }
    }
    push(app, state);
}

/// A confirmed Quit ends the loop once nothing is active.
fn maybe_quit(state: &Rc<RefCell<State>>) {
    let done = {
        let st = state.borrow();
        st.downloads.quit == QuitState::Quitting && st.downloads.jobs.active() == 0
    };
    if done {
        quit_now();
    }
}

#[cfg(test)]
thread_local! {
    /// Under test the loop is not ended: the flow test reads this instead.
    static QUIT_CALLED: std::cell::Cell<bool> = const { std::cell::Cell::new(false) };
}

/// End the event loop (`run` then records the window sizes and returns).
fn quit_now() {
    #[cfg(not(test))]
    slint::quit_event_loop().ok();
    #[cfg(test)]
    QUIT_CALLED.with(|q| q.set(true));
}

/// TEST-ONLY: whether a Quit went through since the last call (and reset).
#[cfg(test)]
pub(crate) fn quit_called_for_tests() -> bool {
    QUIT_CALLED.with(|q| q.replace(false))
}

/// Build the window, wire it and `AppState.open_downloads`, and keep a handle
/// in `State.downloads`. The caller (gui::run) keeps the returned window for
/// the remembered size at Quit.
pub(super) fn wire(app: &AppWindow, state: &Rc<RefCell<State>>) -> Result<DownloadsWindow, slint::PlatformError> {
    let win = DownloadsWindow::new()?;
    restore_window_size(&win);
    {
        let win_weak = win.as_weak();
        win.window().on_close_requested(move || {
            if let Some(win) = win_weak.upgrade() {
                remember_window_size(&win);
            }
            slint::CloseRequestResponse::HideWindow
        });
    }
    win.set_rows(ModelRc::from(state.borrow().downloads.rows.clone()));

    // One shape for every row button: upgrade both handles, run the body.
    // Weak handles: `State` holds the window, which holds these closures.
    fn by_id(
        app: &AppWindow,
        state: &Rc<RefCell<State>>,
        body: fn(&AppWindow, &Rc<RefCell<State>>, u64),
    ) -> impl Fn(i32) + 'static {
        let app_weak = app.as_weak();
        let state = Rc::downgrade(state);
        move |id| {
            if let (Some(app), Some(state), Some(id)) = (app_weak.upgrade(), state.upgrade(), id_of(id)) {
                body(&app, &state, id);
            }
        }
    }
    fn plain(
        app: &AppWindow,
        state: &Rc<RefCell<State>>,
        body: fn(&AppWindow, &Rc<RefCell<State>>),
    ) -> impl Fn() + 'static {
        let app_weak = app.as_weak();
        let state = Rc::downgrade(state);
        move || {
            if let (Some(app), Some(state)) = (app_weak.upgrade(), state.upgrade()) {
                body(&app, &state);
            }
        }
    }
    win.on_cancel(by_id(app, state, cancel));
    win.on_resume(by_id(app, state, resume));
    win.on_show_preset(by_id(app, state, show_preset));
    win.on_dismiss(by_id(app, state, dismiss));
    win.on_clear_finished(plain(app, state, clear_finished));
    win.on_confirm_quit(plain(app, state, confirm_quit));
    win.on_keep_running(plain(app, state, keep_running));
    {
        let state = Rc::downgrade(state);
        app.global::<AppState>().on_open_downloads(move || {
            if let Some(state) = state.upgrade() {
                open(&state);
            }
        });
    }
    state.borrow_mut().downloads.window = Some(win.clone_strong());
    push(app, state);
    Ok(win)
}

/// TEST-ONLY seam for the e2e flow (src/tests/save_flow.rs): the window the
/// rows go to.
#[cfg(test)]
pub(crate) fn window_for_tests() -> DownloadsWindow {
    let state = hf_dialog::shared_state().expect("hf_dialog::wire ran");
    let st = state.borrow();
    st.downloads.window.as_ref().expect("downloads::wire ran").clone_strong()
}

/// TEST-ONLY seam: land a progress readout as the worker does.
#[cfg(test)]
pub(crate) fn land_progress_for_tests(app: &AppWindow, id: i32, p: &hf::Progress) {
    let state = hf_dialog::shared_state().expect("hf_dialog::wire ran");
    progressed(app, &state, id_of(id).expect("an id"), progress_view(p));
}

/// TEST-ONLY seam: land a transfer's result exactly as the worker's closure
/// does, without a network or a child.
#[cfg(test)]
pub(crate) fn land_transfer_for_tests(app: &AppWindow, id: i32, result: Result<hf::Downloaded, hf::HfError>) {
    let state = hf_dialog::shared_state().expect("hf_dialog::wire ran");
    transfer_ended(app, &state, id_of(id).expect("an id"), result);
}

/// TEST-ONLY seam: the tray's Quit, and what it decided.
#[cfg(test)]
pub(crate) fn request_quit_for_tests(app: &AppWindow) {
    let state = hf_dialog::shared_state().expect("hf_dialog::wire ran");
    request_quit(app, &state);
}

/// E2E phase, run from `tests/ui_bindings.rs`'s single `#[test]` (it needs the
/// testing backend that test owns): the empty state, the rows as `Jobs::rows`
/// builds them reaching the screen with the right buttons, a button firing
/// with its row's id, an in-place progress update, and the Quit strip.
#[cfg(test)]
pub(super) fn e2e() {
    use i_slint_backend_testing::ElementHandle;

    let win = DownloadsWindow::new().expect("build DownloadsWindow");
    win.window().set_size(slint::PhysicalSize::new(900, 1400));
    win.show().expect("realize window");
    let shown = |text: &str| ElementHandle::find_by_accessible_label(&win, text).count();
    assert_eq!(shown("No downloads. Start one from Models, New…, From Hugging Face."), 1, "the empty state");
    assert_eq!(shown("Clear finished"), 0);

    let mut jobs = Jobs::default();
    let done = jobs.add("org/Done-GGUF", "org/Done-GGUF  ·  Q8_0".into(), "Fetches 1 file, 8.00 GiB: d.gguf".into());
    jobs.transfer_ended(done, Ok(()));
    jobs.configured(done, Ok(("d-Q8_0".into(), false)));
    let running = jobs.add("org/Run-GGUF", "org/Run-GGUF  ·  Q4_K_M".into(), "Fetches 1 file, 4.00 GiB: r.gguf".into());
    let rows: Rc<VecModel<DownloadRow>> = Rc::new(VecModel::default());
    win.set_rows(ModelRc::from(rows.clone()));
    sync_rows(&rows, jobs.rows());
    assert_eq!(shown("No downloads. Start one from Models, New…, From Hugging Face."), 0);
    for text in [
        "org/Run-GGUF  ·  Q4_K_M",
        "Downloading",
        "Starting…",
        "Cancel download",
        "org/Done-GGUF  ·  Q8_0",
        "Done",
        "Show preset",
        "Dismiss",
        "Clear finished",
    ] {
        assert!(shown(text) > 0, "{text:?} must be on screen");
    }
    assert_eq!(shown("Resume"), 0, "nothing to resume");

    let clicked = Rc::new(std::cell::Cell::new(-1));
    {
        let clicked = clicked.clone();
        win.on_cancel(move |id| clicked.set(id));
    }
    ElementHandle::find_by_accessible_label(&win, "Cancel download")
        .next()
        .expect("the button")
        .invoke_accessible_default_action();
    assert_eq!(id_of(clicked.get()), Some(running), "the button acts on its own row's id");

    // A progress tick rewrites the row in place: same model object.
    let p = hf::Progress { done: 1 << 30, total: 4 << 30, file: "r.gguf".into(), file_done: 1 << 30, file_total: 4 << 30, files_done: 0, files_total: 1 };
    jobs.progress(running, progress_view(&p));
    let before = win.get_rows();
    sync_rows(&rows, jobs.rows());
    assert!(win.get_rows() == before, "updated in place");
    assert_eq!(shown("1.00 GiB of 4.00 GiB (25%), file 1 of 1"), 1);
    assert_eq!(shown("Starting…"), 0);

    win.set_quit_pending(true);
    win.set_quit_active(1);
    assert_eq!(shown("Stop downloads and quit"), 1);
    assert_eq!(shown("Keep running"), 1);
    win.set_quitting(true);
    assert_eq!(shown("Stopping the downloads, then quitting…"), 1);
    assert_eq!(shown("Keep running"), 0, "no way back once confirmed");
    win.hide().ok();
}

#[cfg(test)]
mod tests {
    use super::*;
    use hf::{HfError, Progress, Sidecar};

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
        assert_eq!(PROGRESS_INTERVAL, Duration::from_millis(250));
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
            created_status(&d, "m-Q4_K_M", false),
            "Downloaded m-Q4_K_M.gguf with mmproj and MTP from org/Repo-GGUF: created [m-Q4_K_M]. \
             Show preset (Downloads) opens it."
        );
        assert!(
            created_status(&d, "m-Q4_K_M", true).contains("created [m-Q4_K_M], placed on the GPUs as estimated."),
            "the status says the placement is an estimate"
        );
        assert!(!created_status(&d, "m-Q4_K_M", true).contains("selected"), "nothing is selected for the user");
    }

    fn jobs_with(repo: &str) -> (Jobs, u64) {
        let mut j = Jobs::default();
        let id = j.add(repo, format!("{repo}  ·  Q4_K_M"), "Fetches 1 file".into());
        (j, id)
    }

    fn state_of(j: &Jobs, id: u64) -> JobState {
        j.get(id).expect("the job").state.clone()
    }

    #[test]
    fn a_job_runs_to_its_preset() {
        let (mut j, id) = jobs_with("org/Repo-GGUF");
        assert_eq!(state_of(&j, id), JobState::Downloading);
        let r = &j.rows()[0];
        assert_eq!((r.state.as_str(), r.overall.as_str()), ("Downloading", "Starting…"));
        assert!(r.show_progress && r.can_cancel && !r.can_resume && !r.can_show && !r.can_dismiss);
        assert_eq!(j.summary(), "1 download running");

        assert!(j.progress(id, progress_view(&progress(1 << 30, 4 << 30, "a.gguf", 1 << 30, 4 << 30, (0, 1)))));
        let r = &j.rows()[0];
        assert!(r.overall.starts_with("1.00 GiB of 4.00 GiB (25%)"), "{}", r.overall);
        assert_eq!(r.overall_fraction, 0.25);

        assert!(j.transfer_ended(id, Ok(())));
        assert_eq!(state_of(&j, id), JobState::Configuring);
        let r = &j.rows()[0];
        assert_eq!(r.state.as_str(), "Creating the preset…");
        assert!(!r.show_progress && !r.can_cancel && !r.can_dismiss, "nothing to cancel while the preset is written");
        assert!(!j.progress(id, progress_view(&progress(0, 1, "", 0, 0, (0, 1)))), "a late readout is dropped");
        assert_eq!(j.active(), 1, "Quit still asks while the preset is made");

        assert!(j.configured(id, Ok(("m-Q4_K_M".into(), true))));
        let r = &j.rows()[0];
        assert_eq!(r.state.as_str(), "Done");
        assert!(r.message.contains("[m-Q4_K_M]") && r.message.contains("as estimated"), "{}", r.message);
        assert!(r.can_show && r.can_dismiss && !r.can_resume && !r.is_error);
        assert_eq!(j.preset_of(id).as_deref(), Some("m-Q4_K_M"));
        assert_eq!(j.summary(), "Downloads: 1 done");
        assert_eq!(j.active(), 0);
    }

    #[test]
    fn cancel_failure_and_resume() {
        let (mut j, id) = jobs_with("org/Repo-GGUF");
        assert!(j.cancel(id));
        assert!(!j.cancel(id), "once");
        let r = &j.rows()[0];
        assert_eq!(r.state.as_str(), "Cancelling…");
        assert!(r.show_progress && !r.can_cancel, "the bar stays until the child stops");
        assert!(j.transfer_ended(id, Err(&HfError::Cancelled)));
        let r = &j.rows()[0];
        assert_eq!(r.state.as_str(), "Cancelled");
        assert!(r.message.contains("kept") && !r.is_error && r.can_resume && r.can_dismiss);
        assert!(!j.transfer_ended(id, Ok(())), "a job ends once");

        assert_eq!(j.restart(id), Ok(()));
        assert_eq!(state_of(&j, id), JobState::Downloading);
        assert!(j.transfer_ended(id, Err(&HfError::DiskFull("no space left".into()))));
        let r = &j.rows()[0];
        assert_eq!(r.state.as_str(), "Failed");
        assert!(r.is_error && r.message.contains("no space left"), "{}", r.message);
        assert_eq!(j.summary(), "Downloads: 1 failed");
        assert!(j.restart(id).is_ok(), "a failure resumes too");
        assert!(j.restart(id).is_err(), "a running job does not restart");
    }

    #[test]
    fn one_download_per_repository_at_a_time() {
        let (mut j, a) = jobs_with("org/Repo-GGUF");
        assert!(j.busy_with("org/Repo-GGUF") && j.busy_with("ORG/repo-gguf"), "the same staging tree, any case");
        assert!(!j.busy_with("org/Other-GGUF"), "another repository may run beside it");
        let b = j.add("org/Other-GGUF", "o".into(), String::new());
        assert_eq!(j.summary(), "2 downloads running");
        assert_eq!(j.rows()[0].id as u64, b, "newest first");

        // A second job of the busy repository (added after the first failed)
        // cannot resume while the first runs again.
        j.transfer_ended(a, Err(&HfError::Network("x".into())));
        assert!(!j.busy_with("org/Repo-GGUF"), "a failed job frees its repository");
        let c = j.add("org/Repo-GGUF", "again".into(), String::new());
        let refused = j.restart(a).unwrap_err();
        assert!(refused.contains("already downloading"), "{refused}");
        j.set_note(a, &refused);
        assert_eq!(j.get(a).unwrap().note, refused);
        j.cancel(c);
        j.transfer_ended(c, Err(&HfError::Cancelled));
        assert!(j.restart(a).is_ok());
        assert!(j.get(a).unwrap().note.is_empty(), "a resume clears the old note");
    }

    #[test]
    fn dismiss_and_clear_only_touch_finished_rows() {
        let (mut j, a) = jobs_with("org/A");
        let b = j.add("org/B", "b".into(), String::new());
        assert!(!j.dismiss(a), "a running row stays");
        j.transfer_ended(b, Err(&HfError::Cancelled));
        assert!(j.dismiss(b));
        assert_eq!(j.rows().len(), 1);
        let c = j.add("org/C", "c".into(), String::new());
        j.transfer_ended(c, Err(&HfError::Network("x".into())));
        assert_eq!(j.clear_finished(), vec![c]);
        assert_eq!(j.rows().len(), 1, "the running one stays");
        assert_eq!(j.downloading(), vec![a]);
        j.cancel(a);
        j.transfer_ended(a, Err(&HfError::Cancelled));
        j.clear_finished();
        assert!(j.rows().is_empty());
        assert_eq!(j.summary(), "", "no row: no footer indicator");
    }

    #[test]
    fn a_failed_save_is_a_failure_and_ids_map_to_rows() {
        let (mut j, id) = jobs_with("org/A");
        assert!(!j.configured(id, Ok(("x".into(), false))), "not before the transfer ended");
        j.transfer_ended(id, Ok(()));
        assert!(j.configured(id, Err("could not be saved".into())));
        assert!(j.rows()[0].is_error && j.rows()[0].can_resume);
        assert_eq!(id_of(j.rows()[0].id), Some(id));
        assert_eq!(id_of(-1), None);
    }
}
