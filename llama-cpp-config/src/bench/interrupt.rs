//! Ctrl-C (or SIGTERM, or a closed terminal) during `bench run`: put back what
//! the run changed, then exit.
//!
//! A run edits presets.ini and owns llama-server for hours; left alone, the
//! signal ends the process with the preset on a leg's value, and the next
//! restart serves that. Every key a run changes is registered here beside its
//! pending marker. The handler restores them newest first, removes their
//! markers and leaves llama-server as the run found it. A kill -9 or a power cut
//! never gets here: the markers are still what tells the next run.
//!
//! The one mutex is also the gate for whatever the main thread does to the
//! preset and the server. The handler takes it and never lets go, so the main
//! thread, still running a leg, stops at its next edit or restart and cannot
//! undo the restore.

use std::path::{Path, PathBuf};
use std::sync::{Mutex, MutexGuard};

struct Held {
    preset: String,
    key: String,
    restore: String,
    marker: PathBuf,
}

#[derive(Default)]
pub struct State {
    held: Vec<Held>,
    /// Whether llama-server ran before the first leg; `None` until then, when
    /// the run has not touched it.
    was_running: Option<bool>,
}

static STATE: Mutex<State> = Mutex::new(State {
    held: Vec::new(),
    was_running: None,
});

/// Hold this across an edit of presets.ini or a stop or start of the server.
pub fn gate() -> MutexGuard<'static, State> {
    STATE.lock().unwrap_or_else(|e| e.into_inner())
}

/// Call before the key changes, once its marker is on disk.
pub fn hold(preset: &str, key: &str, restore: &str, marker: PathBuf) {
    gate().held.push(Held {
        preset: preset.to_string(),
        key: key.to_string(),
        restore: restore.to_string(),
        marker,
    });
}

/// The key is back: forget it and remove its marker.
pub fn release(marker: &Path) {
    let mut state = gate();
    state.held.retain(|h| h.marker != marker);
    let _ = std::fs::remove_file(marker);
}

/// Only the first call counts: a later sweep of the same run sees the server
/// the previous sweep put back, the first one sees the machine as it was.
pub fn note_server(was_running: bool) {
    gate().was_running.get_or_insert(was_running);
}

/// For `bench run`, before anything else it does.
pub fn install() -> Result<(), String> {
    ctrlc::set_handler(|| {
        println!("\nInterrupted: putting the preset and the server back.");
        let code = restore(&mut gate());
        std::process::exit(code);
    })
    .map_err(|e| format!("cannot install the interrupt handler: {e}"))
}

/// Returns the exit code. A key that cannot be put back keeps its marker.
fn restore(state: &mut State) -> i32 {
    let mut code = 130;
    for h in state.held.iter().rev() {
        match super::sweep::write_key(&h.preset, &h.key, &h.restore) {
            Ok(()) => {
                let _ = std::fs::remove_file(&h.marker);
                println!("{} = {} (presets.ini).", h.key, h.restore);
            }
            Err(e) => {
                println!(
                    "Could not restore {} = {}: {e}. The value stays in {}.",
                    h.key,
                    h.restore,
                    h.marker.display()
                );
                code = 1;
            }
        }
    }
    if let Some(was_running) = state.was_running {
        if let Err(e) = super::sweep::put_server_back(was_running) {
            println!("{e}");
            code = 1;
        }
    }
    code
}
