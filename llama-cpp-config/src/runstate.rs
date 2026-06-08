// Probes / starts / stops the llama-server process.

use std::io;

#[derive(Debug, Clone)]
pub struct RunState;

/// Returns `Some(state)` if llama-server.exe is running.
pub fn load() -> Option<RunState> {
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        const CREATE_NO_WINDOW: u32 = 0x08000000;
        let output = std::process::Command::new("tasklist")
            .args(["/fi", "IMAGENAME eq llama-server.exe", "/fo", "csv", "/nh"])
            .creation_flags(CREATE_NO_WINDOW)
            .output()
            .ok()?;
        let stdout = String::from_utf8_lossy(&output.stdout);
        // tasklist returns "INFO: No tasks..." when nothing matches — not empty
        if stdout.to_lowercase().contains("llama-server.exe") {
            return Some(RunState);
        }
        None
    }

    #[cfg(not(windows))]
    {
        let output = std::process::Command::new("pgrep")
            .args(["-f", "llama-server"])
            .output()
            .ok()?;
        if output.status.success() {
            return Some(RunState);
        }
        None
    }
}

/// Check that the presets file has at least one [section] header.
fn has_presets() -> bool {
    let path = crate::paths::presets_ini();
    match std::fs::read_to_string(&path) {
        Ok(content) => {
            for line in content.lines() {
                let trimmed = line.trim();
                if trimmed.starts_with('[') && trimmed.ends_with(']') && trimmed.len() > 2 {
                    return true;
                }
            }
            false
        }
        Err(_) => false,
    }
}

fn cpu_cores() -> u32 {
    std::thread::available_parallelism()
        .map(|n| n.get() as u32)
        .unwrap_or(4)
}

/// Launch llama-server.exe with args from server.ini + presets.ini.
pub fn start() -> io::Result<()> {
    if load().is_some() {
        return Ok(()); // already running
    }

    if !has_presets() {
        return Err(io::Error::new(
            io::ErrorKind::NotFound,
            "No model presets configured — add one on the Models page first.",
        ));
    }

    let exe = crate::paths::llama_server_exe().ok_or_else(|| {
        io::Error::new(
            io::ErrorKind::NotFound,
            "llama-server.exe not found. Build llama.cpp first.",
        )
    })?;

    let cfg = crate::server_cfg::load();
    let presets_path = crate::paths::presets_ini();

    let hostname = cfg.hostname.unwrap_or_else(|| "localhost".into());
    let port = cfg.port.unwrap_or(8080);
    let models_max = cfg.models_max.unwrap_or(1);
    let mlock = cfg.mlock.unwrap_or(true);
    let models_dir = cfg
        .models_dir
        .clone()
        .filter(|s| !s.is_empty())
        .unwrap_or_else(crate::server_cfg::default_models_dir);

    let cores = cpu_cores();
    let threads = cfg
        .threads
        .filter(|&n| n > 0)
        .unwrap_or((cores as f64 * 0.5) as i32);
    let threads_batch = cfg
        .threads_batch
        .filter(|&n| n > 0)
        .unwrap_or((cores as f64 * 0.75) as i32);

    let data_root = crate::paths::data_root();
    let log_dir = data_root.join("logs");
    std::fs::create_dir_all(&log_dir)?;
    let log_path = log_dir.join("llama-server.log");

    let log_file = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(&log_path)?;

    let mut cmd = std::process::Command::new(&exe);
    cmd.arg("--models-preset")
        .arg(presets_path.to_string_lossy().as_ref())
        .arg("--models-max")
        .arg(models_max.to_string())
        .arg("--port")
        .arg(port.to_string())
        .arg("--host")
        .arg(&hostname)
        .arg("--webui-mcp-proxy")
        .arg("-fit")
        .arg("off")
        .arg("-lv")
        .arg("4")
        .arg("-t")
        .arg(threads.to_string())
        .arg("--threads-batch")
        .arg(threads_batch.to_string());

    if mlock {
        cmd.arg("--mlock");
    }
    if let Some(cr) = cfg.cache_reuse {
        if cr > 0 {
            cmd.arg("--cache-reuse").arg(cr.to_string());
        }
    }

    cmd.current_dir(&data_root);
    cmd.env("LLAMA_CACHE", &models_dir);
    // llama-server writes most of its logging (model load, request logs,
    // GGML asserts, crash traces) to stderr — capture both streams in the
    // same log file so the log isn't empty and crashes leave a trail.
    let log_file_err = log_file.try_clone()?;
    cmd.stdout(log_file);
    cmd.stderr(log_file_err);

    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        const CREATE_NO_WINDOW: u32 = 0x08000000;
        cmd.creation_flags(CREATE_NO_WINDOW);
    }

    cmd.spawn()?;

    Ok(())
}

/// Kill all llama-server.exe processes (graceful Ctrl+C-like on Windows).
pub fn stop() -> io::Result<()> {
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        const CREATE_NO_WINDOW: u32 = 0x08000000;
        std::process::Command::new("taskkill")
            .args(["/f", "/im", "llama-server.exe"])
            .creation_flags(CREATE_NO_WINDOW)
            .output()?;
    }
    #[cfg(not(windows))]
    {
        let _ = std::process::Command::new("pkill")
            .arg("-f")
            .arg("llama-server")
            .output();
    }
    Ok(())
}

/// Returns the command line of the running llama-server process, reconstructed
/// from server.ini (the same deterministic args `start()` uses).
pub fn command_line() -> Option<String> {
    let cfg = crate::server_cfg::load();
    let exe = crate::paths::llama_server_exe()?;
    let presets_path = crate::paths::presets_ini();

    let hostname = cfg.hostname.unwrap_or_else(|| "localhost".into());
    let port = cfg.port.unwrap_or(8080);
    let models_max = cfg.models_max.unwrap_or(1);
    let mlock = cfg.mlock.unwrap_or(true);

    let cores = cpu_cores();
    let threads = cfg
        .threads
        .filter(|&n| n > 0)
        .unwrap_or((cores as f64 * 0.5) as i32);
    let threads_batch = cfg
        .threads_batch
        .filter(|&n| n > 0)
        .unwrap_or((cores as f64 * 0.75) as i32);

    let mut parts: Vec<String> = vec![
        format!("\"{}\"", exe.display()),
        format!("--models-preset \"{}\"", presets_path.display()),
        format!("--models-max {models_max}"),
        format!("--port {port}"),
        format!("--host {hostname}"),
        "--webui-mcp-proxy".into(),
        "-fit off".into(),
        "-lv 4".into(),
        format!("-t {threads}"),
        format!("--threads-batch {threads_batch}"),
    ];

    if mlock {
        parts.push("--mlock".into());
    }
    if let Some(cr) = cfg.cache_reuse {
        if cr > 0 {
            parts.push(format!("--cache-reuse {cr}"));
        }
    }

    Some(parts.join(" "))
}
