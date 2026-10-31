//! The GPU lock every benchmark takes: `bench sweep` (spec or not) and the
//! Benchmark tab.
//!
//! Two jobs on the same cards wreck both measurements, and nothing in a
//! benchmark's numbers says it happened. So a run holds an exclusive lock on
//! one file for its whole length: `LLAMA_FW_GPU_LOCK` when set (point other GPU
//! jobs of yours at the same file, e.g. `flock -o <file> <job>`), else
//! `gpu.lock` in the bench directory. It is `File::lock`, i.e. `flock` on Linux
//! (compatible with `flock(1)` on the same path) and `LockFileEx` on Windows,
//! released when the process exits however it exits. std opens files
//! close-on-exec / non-inheritable, so the llama-server a run starts never
//! holds it after the run.
//!
//! On Linux the lock is not the whole check: a job that does not take it still
//! shares the cards. After the lock, the run also waits until no other process
//! has `/dev/kfd` (ROCm) or `/dev/nvidia*` (CUDA) open, llama-server excepted:
//! the run restarts that one itself.

use std::fs::{File, OpenOptions, TryLockError};
use std::path::PathBuf;
use std::time::{Duration, Instant};

/// Held for the run; dropping it releases the lock.
#[derive(Debug)]
pub struct Guard {
    _file: File,
}

pub fn path() -> PathBuf {
    match std::env::var_os("LLAMA_FW_GPU_LOCK") {
        Some(p) if !p.is_empty() => PathBuf::from(p),
        _ => super::bench_dir().join("gpu.lock"),
    }
}

fn open() -> Result<File, String> {
    open_at(&path())
}

fn open_at(p: &std::path::Path) -> Result<File, String> {
    if let Some(dir) = p.parent() {
        let _ = std::fs::create_dir_all(dir);
    }
    OpenOptions::new()
        .create(true)
        .truncate(false)
        .write(true)
        .open(p)
        .map_err(|e| format!("cannot open the GPU lock {}: {e}", p.display()))
}

/// The CLI's: waits for the lock, then for the cards, saying why on `say`.
pub fn acquire(say: impl Fn(&str)) -> Result<Guard, String> {
    let file = open()?;
    match file.try_lock() {
        Ok(()) => {}
        Err(TryLockError::WouldBlock) => {
            say(&format!(
                "Waiting for the GPU lock {} (another benchmark holds it)...",
                path().display()
            ));
            file.lock()
                .map_err(|e| format!("cannot take the GPU lock: {e}"))?;
        }
        Err(TryLockError::Error(e)) => return Err(format!("cannot take the GPU lock: {e}")),
    }
    let mut last_said: Option<Instant> = None;
    loop {
        let users = other_gpu_users();
        if users.is_empty() {
            return Ok(Guard { _file: file });
        }
        if last_said.is_none_or(|t| t.elapsed() >= Duration::from_secs(60)) {
            say(&format!(
                "Waiting: another process uses the GPUs ({}).",
                users.join(", ")
            ));
            last_said = Some(Instant::now());
        }
        std::thread::sleep(Duration::from_secs(5));
    }
}

/// The tab's: never waits (a window must not hang on a lock), refuses instead.
pub fn try_acquire() -> Result<Guard, String> {
    let file = open()?;
    match file.try_lock() {
        Ok(()) => {}
        Err(TryLockError::WouldBlock) => {
            return Err(format!(
                "another benchmark holds the GPU lock {}; run when it is done",
                path().display()
            ))
        }
        Err(TryLockError::Error(e)) => return Err(format!("cannot take the GPU lock: {e}")),
    }
    let users = other_gpu_users();
    if !users.is_empty() {
        return Err(format!(
            "another process uses the GPUs ({}): its load would be measured too",
            users.join(", ")
        ));
    }
    Ok(Guard { _file: file })
}

/// `pid name` of every other process with a GPU device node open, llama-server
/// excepted. Linux only: Windows exposes no such list without a driver query,
/// so there the lock is the whole check.
fn other_gpu_users() -> Vec<String> {
    #[cfg(target_os = "linux")]
    {
        let me = std::process::id().to_string();
        let mut out: Vec<String> = Vec::new();
        let Ok(procs) = std::fs::read_dir("/proc") else {
            return out;
        };
        for p in procs.flatten() {
            let pid = p.file_name().to_string_lossy().into_owned();
            if pid == me || !pid.chars().all(|c| c.is_ascii_digit()) {
                continue;
            }
            let Ok(fds) = std::fs::read_dir(p.path().join("fd")) else {
                continue;
            };
            let holds = fds.flatten().any(|fd| {
                std::fs::read_link(fd.path()).is_ok_and(|t| is_gpu_node(&t.to_string_lossy()))
            });
            if !holds {
                continue;
            }
            let comm = std::fs::read_to_string(p.path().join("comm")).unwrap_or_default();
            let comm = comm.trim();
            if comm != "llama-server" {
                out.push(format!("{pid} {comm}"));
            }
        }
        out.sort();
        out
    }
    #[cfg(not(target_os = "linux"))]
    {
        Vec::new()
    }
}

#[cfg_attr(not(target_os = "linux"), allow(dead_code))]
fn is_gpu_node(target: &str) -> bool {
    target == "/dev/kfd"
        || target
            .strip_prefix("/dev/nvidia")
            .is_some_and(|n| !n.is_empty() && n.chars().all(|c| c.is_ascii_digit()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_gpu_nodes_are_kfd_and_the_numbered_nvidia_ones() {
        assert!(is_gpu_node("/dev/kfd"));
        assert!(is_gpu_node("/dev/nvidia0"));
        assert!(!is_gpu_node("/dev/nvidiactl"));
        assert!(!is_gpu_node("/dev/nvidia"));
        assert!(!is_gpu_node("/dev/dri/renderD128"));
    }

    #[test]
    fn a_held_lock_is_refused_without_waiting() {
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join("gpu.lock");
        let first = open_at(&file).unwrap();
        first.try_lock().unwrap();
        let second = open_at(&file).unwrap();
        assert!(matches!(second.try_lock(), Err(TryLockError::WouldBlock)));
        drop(first);
        assert!(second.try_lock().is_ok());
    }
}
