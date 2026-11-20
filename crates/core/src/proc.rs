//! Launch a child process without flashing a console window on Windows.
//!
//! Single home for the `CREATE_NO_WINDOW` dance the process probes share. Entry
//! points: `run_hidden` for the fire-and-forget system probes (the Windows
//! branch of `runstate::is_running`/`stop`: tasklist / taskkill), and
//! `hide_console` for callers that build the `Command` themselves: custom
//! stdio/env then `spawn()` (`runstate::start`). On non-Windows both are no-ops
//! beyond the plain command, so callers that differ only by that flag collapse.
//! `combined_output` joins a probe's stdout+stderr; parse that, not one stream.
//! `keep_std_handles_private` stops a long-lived child (the server) from
//! holding our own stdout open, which hung any caller reading the CLI's output.
//!
//! `run_hidden_probe` is the variant for the transient `llama-server.exe` probes
//! (`devices --list-devices`, `server_version --version`): it registers the child
//! PID in `PROBE_PIDS` for its lifetime so `runstate::is_running` can EXCLUDE it.
//! `run_hidden_probe_within` is the same with a bound: a caller that cannot wait
//! for a hung engine (the manager's GPU probe) kills the child at the bound. It
//! reads both of the child's streams while it runs, so a child writing more
//! than a pipe buffer is not stopped by that before the bound. The bound also
//! covers the reads: the child's helpers inherit its pipes and can keep them
//! open past its own exit, so each stream is given a short grace after the
//! child is gone and taken as empty when that expires.
//!
//! Every llama-server child (the probes here and `runstate::start`) also gets
//! the ROCm runtime dir prepended to its PATH (`prepend_rocm_path`): ggml loads
//! backends dynamically and silently drops ggml-hip.dll when its imports don't
//! resolve, and AMD's HIP SDK installer never puts its bin dir on the system
//! PATH; without the prepend, HIP GPUs enumerate as Vulkan-only.
//! Both share the `llama-server.exe` image name, so a probe running concurrently
//! with the run-status poll otherwise reads as a live server, which made a fresh
//! GUI (whose startup fires the probes and the poll together) wrongly flip to
//! "llama-server is no longer running" seconds after launch.

use std::ffi::OsStr;
use std::path::Path;
use std::process::{Command, Output, Stdio};
use std::sync::Mutex;
use std::time::{Duration, Instant};

/// PIDs of the transient `llama-server.exe` probe children currently alive.
/// `runstate::is_running` subtracts these from the tasklist match.
static PROBE_PIDS: Mutex<Vec<u32>> = Mutex::new(Vec::new());

/// Run `scan` (a look at the process table: `/proc` on Linux, `tasklist` on
/// Windows) with the probe list (see `PROBE_PIDS`) HELD, and hand
/// it the probes to subtract. A snapshot taken before or after the scan is not
/// enough: a probe that starts between the two, or one that ends and
/// deregisters between them, is seen by the scan and missing from the list, so
/// it counts as a server. The GUI fires both probes and the run-status poll
/// together at startup and on every Refresh, which is exactly that overlap:
/// the footer read "Running" with nothing running, and the next tick reported
/// the server as gone, in red. `run_hidden_probe` spawns and registers under
/// the same lock, so for the length of a scan the list cannot change.
pub fn with_probe_pids<R>(scan: impl FnOnce(&[u32]) -> R) -> R {
    let guard = PROBE_PIDS.lock().expect("PROBE_PIDS lock");
    scan(&guard)
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

/// Keep this process's own standard handles out of the children it spawns
/// from now on; a no-op elsewhere. Call before spawning a child that outlives
/// us (`runstate::start`).
///
/// Windows hands a child EVERY inheritable handle of its parent, not only the
/// three it is given as stdio, and the handles a shell passes us are
/// inheritable. So a llama-server spawned by `control start` / `restart` kept
/// the CLI's stdout open for its whole life, and a caller reading that output
/// to its end (`... | tail`, a script capturing it) hung although the restart
/// had succeeded. Clearing the flag costs nothing: `std` duplicates a handle it
/// passes as a child's stdio into an inheritable copy of its own.
pub fn keep_std_handles_private() {
    #[cfg(windows)]
    {
        use windows_sys::Win32::Foundation::{
            SetHandleInformation, HANDLE_FLAG_INHERIT, INVALID_HANDLE_VALUE,
        };
        use windows_sys::Win32::System::Console::{
            GetStdHandle, STD_ERROR_HANDLE, STD_INPUT_HANDLE, STD_OUTPUT_HANDLE,
        };
        for which in [STD_INPUT_HANDLE, STD_OUTPUT_HANDLE, STD_ERROR_HANDLE] {
            // SAFETY: no pointers; a missing handle (a GUI launch has none) is
            // skipped, and a failed call only leaves the flag as it was.
            unsafe {
                let handle = GetStdHandle(which);
                if !handle.is_null() && handle != INVALID_HANDLE_VALUE {
                    SetHandleInformation(handle, HANDLE_FLAG_INHERIT, 0);
                }
            }
        }
    }
}

/// Prepend the ROCm/HIP runtime dir (`paths::rocm_bin_dir`) to `cmd`'s PATH so
/// ggml-hip.dll's imports (`amdhip64_*.dll`, `rocblas.dll`, …) resolve in the
/// child. Apply to every llama-server we spawn (see the module header); a no-op
/// when no ROCm install is found. Harmlessly duplicates the entry if the dir is
/// already on PATH.
pub fn prepend_rocm_path(cmd: &mut Command) {
    if let Some(bin) = crate::paths::rocm_bin_dir() {
        if let Some(path) = prepend_path_var(&bin, std::env::var_os("PATH")) {
            cmd.env("PATH", path);
        }
    }
}

/// `dir` prepended to a PATH-shaped value. Pure so the precedence is testable.
/// `None` only if the parts won't re-join (a dir embedding the separator,
/// unreachable from real installs); the caller then leaves PATH untouched
/// rather than risk clobbering it.
fn prepend_path_var(dir: &Path, current: Option<std::ffi::OsString>) -> Option<std::ffi::OsString> {
    let mut parts = vec![dir.to_path_buf()];
    if let Some(cur) = &current {
        parts.extend(std::env::split_paths(cur));
    }
    std::env::join_paths(parts).ok()
}

/// Run `exe` with `args`, capturing stdout/stderr. Returns `None` if the process
/// can't be spawned. On Windows the child gets `CREATE_NO_WINDOW` (via
/// `hide_console`) so no console window pops up for these background probes.
/// Windows only: its callers are `tasklist` and `taskkill`.
#[cfg(windows)]
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
/// only; NOT for tasklist/taskkill, which are a different image with nothing to
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
    prepend_rocm_path(&mut cmd);
    // Spawned and registered under ONE lock: a process-table scan holding it
    // (`with_probe_pids`) never sees this child without its PID on the list.
    let (child, pid) = {
        let mut guard = PROBE_PIDS.lock().expect("PROBE_PIDS lock");
        let child = cmd.spawn().ok()?;
        let pid = child.id();
        guard.push(pid);
        (child, pid)
    };
    let output = child.wait_with_output().ok();
    deregister_probe_pid(pid);
    output
}

/// `pid` off the probe list (see `PROBE_PIDS`).
fn deregister_probe_pid(pid: u32) {
    let mut guard = PROBE_PIDS.lock().expect("PROBE_PIDS lock");
    if let Some(i) = guard.iter().position(|&p| p == pid) {
        guard.swap_remove(i);
    }
}

/// How much of one of a probe's streams is kept: every block a probe parses is
/// far under it, and the bytes past it are dropped while the stream is read on
/// to its end.
const PROBE_STREAM_CAP: usize = 4 << 20;

/// Reads `pipe` to its end on its own thread, keeping the first `cap` bytes and
/// dropping the rest, and sends them over a channel the caller picks up: the
/// read has no bound of its own, so a caller that cannot wait for it needs a
/// handle it can give up on (see [`pipe_bytes`]). The reader must reach the
/// end: a child whose pipe nobody reads stops on the write once the kernel
/// buffer is full, and a stopped probe is a probe that gets killed. `None` for
/// a stream that is not a pipe.
fn drain_pipe<R: std::io::Read + Send + 'static>(
    pipe: Option<R>,
    cap: usize,
) -> Option<std::sync::mpsc::Receiver<Vec<u8>>> {
    let mut pipe = pipe?;
    let (tx, rx) = std::sync::mpsc::channel();
    std::thread::spawn(move || {
        let mut kept = Vec::new();
        let mut chunk = [0u8; 8 << 10];
        while let Ok(n) = pipe.read(&mut chunk) {
            if n == 0 {
                break;
            }
            let keep = cap.min(kept.len() + n) - kept.len();
            kept.extend_from_slice(&chunk[..keep]);
        }
        // A caller that gave up on this stream is gone: the send fails and the
        // thread ends with it.
        let _ = tx.send(kept);
    });
    Some(rx)
}

/// How long the caller of [`pipe_bytes`] waits for a stream once the probe's
/// child is gone. The child's end of the pipe is closed by then, so a read
/// still open belongs to a helper of the child that inherited the pipe and is
/// still alive: no further byte of that stream is ever coming to the probe,
/// and the probe outliving the child by the grace is the whole cost.
const PROBE_READER_GRACE: Duration = Duration::from_secs(2);

/// Picks up the bytes a [`drain_pipe`] reader sends. `None` when the stream was
/// not a pipe, or its reader died without sending; `Some` empty when the grace
/// ran out, in which case the receiver is dropped and the reader is left
/// detached: it ends on its own when the pipe closes.
fn pipe_bytes(reader: Option<std::sync::mpsc::Receiver<Vec<u8>>>) -> Option<Vec<u8>> {
    match reader?.recv_timeout(PROBE_READER_GRACE) {
        Ok(bytes) => Some(bytes),
        Err(std::sync::mpsc::RecvTimeoutError::Timeout) => Some(Vec::new()),
        Err(std::sync::mpsc::RecvTimeoutError::Disconnected) => None,
    }
}

/// Like `run_hidden_probe`, but killing the child when it is still running
/// after `timeout`: a probe of an engine that hangs must never hold its caller
/// open. The child is reaped, so it is stopped and not left behind. Its stdout
/// and stderr are read while it runs, so a child writing more than a pipe
/// buffer is never stopped by the full pipe; the child gets no stdin. The
/// reads are bounded too: once the child is gone each stream is waited for
/// during `PROBE_READER_GRACE`, and one a helper of the child still holds open
/// is answered empty. `None` when the child cannot be spawned, is killed, or
/// its output cannot be read; a child that finishes, whatever its exit status,
/// is answered with its output.
pub fn run_hidden_probe_within<I, S>(exe: &Path, args: I, timeout: Duration) -> Option<Output>
where
    I: IntoIterator<Item = S>,
    S: AsRef<OsStr>,
{
    let mut cmd = Command::new(exe);
    cmd.args(args)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    hide_console(&mut cmd);
    prepend_rocm_path(&mut cmd);
    // Spawned and registered under ONE lock, as `run_hidden_probe` does.
    let (mut child, pid) = {
        let mut guard = PROBE_PIDS.lock().expect("PROBE_PIDS lock");
        let child = cmd.spawn().ok()?;
        let pid = child.id();
        guard.push(pid);
        (child, pid)
    };
    let stdout = drain_pipe(child.stdout.take(), PROBE_STREAM_CAP);
    let stderr = drain_pipe(child.stderr.take(), PROBE_STREAM_CAP);
    // `try_wait` polled, not a thread waited on: the child's own `wait` would
    // hold it while the caller gives up, and a child no caller can reach is a
    // child nobody kills.
    let at = Instant::now();
    let status = loop {
        match child.try_wait() {
            Ok(Some(status)) => break Some(status),
            Ok(None) if at.elapsed() < timeout => std::thread::sleep(Duration::from_millis(20)),
            Ok(None) => break None,
            Err(_) => break None,
        }
    };
    let status = match status {
        Some(status) => status,
        None => {
            let _ = child.kill();
            let _ = child.wait();
            // The readers are not waited on: this answers nothing, so their
            // bytes are of no interest. Dropping the receivers leaves them
            // detached, ending when the pipes close.
            drop((stdout, stderr));
            deregister_probe_pid(pid);
            return None;
        }
    };
    // The readers are picked up after the child is gone: its end of each pipe
    // is closed by then, so a stream that does not answer inside the grace
    // belongs to a helper of the child that is still holding it open.
    let (Some(stdout), Some(stderr)) = (pipe_bytes(stdout), pipe_bytes(stderr)) else {
        deregister_probe_pid(pid);
        return None;
    };
    deregister_probe_pid(pid);
    Some(Output {
        status,
        stdout,
        stderr,
    })
}

/// Join a child's stdout and stderr into one string. llama.cpp tools split
/// output across the two streams (`--version` prints to **stderr**,
/// `--list-devices` to stdout), so probes must parse the combination; reading
/// a single stream silently blanks them when upstream moves the output.
pub fn combined_output(output: &Output) -> String {
    let mut s = String::from_utf8_lossy(&output.stdout).into_owned();
    s.push('\n');
    s.push_str(&String::from_utf8_lossy(&output.stderr));
    s
}

#[cfg(test)]
mod tests {
    use super::prepend_path_var;
    use std::path::{Path, PathBuf};

    #[test]
    fn prepend_path_var_puts_dir_first_and_keeps_the_rest() {
        let current = std::env::join_paths([Path::new("a"), Path::new("b")]).unwrap();
        let joined = prepend_path_var(Path::new("rocm_bin"), Some(current)).unwrap();
        let parts: Vec<PathBuf> = std::env::split_paths(&joined).collect();
        assert_eq!(
            parts,
            vec![
                PathBuf::from("rocm_bin"),
                PathBuf::from("a"),
                PathBuf::from("b")
            ]
        );
    }

    #[test]
    fn prepend_path_var_without_current_is_just_the_dir() {
        let joined = prepend_path_var(Path::new("rocm_bin"), None).unwrap();
        let parts: Vec<PathBuf> = std::env::split_paths(&joined).collect();
        assert_eq!(parts, vec![PathBuf::from("rocm_bin")]);
    }
}
