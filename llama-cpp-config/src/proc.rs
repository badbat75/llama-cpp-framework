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
//!
//! Every llama-server child — the probes here and `runstate::start` — also gets
//! the ROCm runtime dir prepended to its PATH (`prepend_rocm_path`): ggml loads
//! backends dynamically and silently drops ggml-hip.dll when its imports don't
//! resolve, and AMD's HIP SDK installer never puts its bin dir on the system
//! PATH — without the prepend, HIP GPUs enumerate as Vulkan-only.
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
/// `None` only if the parts won't re-join (a dir embedding the separator —
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
    prepend_rocm_path(&mut cmd);
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
