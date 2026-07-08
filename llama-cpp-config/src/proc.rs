//! Launch a child process without flashing a console window on Windows.
//!
//! Single home for the `CREATE_NO_WINDOW` dance the process probes share. Entry
//! points: `run_hidden` for the fire-and-forget system probes (the Windows
//! branch of `runstate::is_running`/`stop` — tasklist / taskkill), and
//! `hide_console` for callers that build the `Command` themselves — custom
//! stdio/env then `spawn()` (`runstate::start`). On non-Windows both are no-ops
//! beyond the plain command, so callers that differ only by that flag collapse.
//! `combined_output` joins a probe's stdout+stderr — parse that, not one stream.
//!
//! `run_hidden_probe` is the variant for the transient `llama-server.exe` probes
//! (`devices --list-devices`, `server_version --version`): it registers the child
//! PID in `PROBE_PIDS` for its lifetime so `runstate::is_running` can EXCLUDE it.
//! Both share the `llama-server.exe` image name, so a probe running concurrently
//! with the run-status poll otherwise reads as a live server — which made a fresh
//! GUI (whose startup fires the probes and the poll together) wrongly flip to
//! "llama-server is no longer running" seconds after launch.

use std::ffi::OsStr;
use std::path::Path;
use std::process::{Command, Output, Stdio};
use std::sync::Mutex;

/// PIDs of the transient `llama-server.exe` probe children currently alive.
/// `runstate::is_running` subtracts these from the tasklist match.
static PROBE_PIDS: Mutex<Vec<u32>> = Mutex::new(Vec::new());

/// A snapshot of the live probe PIDs (see `PROBE_PIDS`).
pub fn probe_pids() -> Vec<u32> {
    PROBE_PIDS.lock().expect("PROBE_PIDS lock").clone()
}

/// Apply `CREATE_NO_WINDOW` to `cmd` on Windows so no console window pops up for
/// these background children; a no-op elsewhere. The single definition of the
/// flag, shared with `run_hidden`.
pub fn hide_console(cmd: &mut Command) {
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        const CREATE_NO_WINDOW: u32 = 0x08000000;
        cmd.creation_flags(CREATE_NO_WINDOW);
    }
    #[cfg(not(windows))]
    {
        let _ = cmd;
    }
}

/// Run `exe` with `args`, capturing stdout/stderr. Returns `None` if the process
/// can't be spawned. On Windows the child gets `CREATE_NO_WINDOW` (via
/// `hide_console`) so no console window pops up for these background probes.
pub fn run_hidden<I, S>(exe: &Path, args: I) -> Option<Output>
where
    I: IntoIterator<Item = S>,
    S: AsRef<OsStr>,
{
    let mut cmd = Command::new(exe);
    cmd.args(args);
    hide_console(&mut cmd);
    cmd.output().ok()
}

/// Like `run_hidden`, but registers the child PID in `PROBE_PIDS` for its
/// lifetime so a concurrent `runstate::is_running` can exclude this transient
/// `llama-server.exe` (see the module header). Use for the llama-server probes
/// only — NOT for tasklist/taskkill, which are a different image with nothing to
/// exclude. Spawns explicitly (not `cmd.output()`) so the PID is knowable while
/// the child runs; stdout/stderr are piped so `wait_with_output` still captures
/// them.
pub fn run_hidden_probe<I, S>(exe: &Path, args: I) -> Option<Output>
where
    I: IntoIterator<Item = S>,
    S: AsRef<OsStr>,
{
    let mut cmd = Command::new(exe);
    cmd.args(args).stdout(Stdio::piped()).stderr(Stdio::piped());
    hide_console(&mut cmd);
    let child = cmd.spawn().ok()?;
    let pid = child.id();
    PROBE_PIDS.lock().expect("PROBE_PIDS lock").push(pid);
    let output = child.wait_with_output().ok();
    let mut guard = PROBE_PIDS.lock().expect("PROBE_PIDS lock");
    if let Some(i) = guard.iter().position(|&p| p == pid) {
        guard.swap_remove(i);
    }
    output
}

/// Join a child's stdout and stderr into one string. llama.cpp tools split
/// output across the two streams (`--version` prints to **stderr**,
/// `--list-devices` to stdout), so probes must parse the combination — reading
/// a single stream silently blanks them when upstream moves the output.
pub fn combined_output(output: &Output) -> String {
    let mut s = String::from_utf8_lossy(&output.stdout).into_owned();
    s.push('\n');
    s.push_str(&String::from_utf8_lossy(&output.stderr));
    s
}
