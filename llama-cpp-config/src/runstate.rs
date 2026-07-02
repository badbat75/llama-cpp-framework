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

/// The full llama-server argument list derived from server.ini.
/// Single source of truth for both `start()` and `command_line()`.
fn server_args(
    cfg: &crate::server_cfg::ServerConfig,
    presets_path: &std::path::Path,
) -> Vec<String> {
    let hostname = cfg.hostname.clone().unwrap_or_else(|| "localhost".into());
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

    let mut args: Vec<String> = vec![
        "--models-preset".into(),
        presets_path.to_string_lossy().into_owned(),
        "--models-max".into(),
        models_max.to_string(),
        "--port".into(),
        port.to_string(),
        "--host".into(),
        hostname,
        "--webui-mcp-proxy".into(),
        "-fit".into(),
        "off".into(),
        "-lv".into(),
        "4".into(),
        "-t".into(),
        threads.to_string(),
        "--threads-batch".into(),
        threads_batch.to_string(),
    ];

    if mlock {
        args.push("--mlock".into());
    }
    if let Some(cr) = cfg.cache_reuse {
        if cr > 0 {
            args.push("--cache-reuse".into());
            args.push(cr.to_string());
        }
    }
    if let Some(dev) = cfg.device.as_deref().map(str::trim) {
        if !dev.is_empty() {
            args.push("--device".into());
            args.push(dev.to_string());
        }
    }
    if let Some(sm) = cfg.split_mode.as_deref().map(str::trim) {
        if !sm.is_empty() {
            args.push("--split-mode".into());
            args.push(sm.to_string());
        }
    }
    if let Some(ts) = cfg.tensor_split.as_deref().map(str::trim) {
        if !ts.is_empty() {
            args.push("--tensor-split".into());
            args.push(ts.to_string());
        }
    }
    args
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

    let models_dir = cfg
        .models_dir
        .clone()
        .filter(|s| !s.is_empty())
        .unwrap_or_else(crate::server_cfg::default_models_dir);

    let data_root = crate::paths::data_root();
    let log_dir = data_root.join("logs");
    std::fs::create_dir_all(&log_dir)?;
    let log_path = log_dir.join("llama-server.log");

    let log_file = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(&log_path)?;

    let mut cmd = std::process::Command::new(&exe);
    cmd.args(server_args(&cfg, &presets_path));

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

/// Force-kill all llama-server.exe processes (taskkill /f — llama-server has
/// no graceful shutdown channel when running detached without a console).
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

/// Shell line-continuation character: PowerShell uses a backtick, POSIX shells
/// use a backslash. Only used to pretty-print `command_line()` for the UI.
#[cfg(windows)]
const LINE_CONTINUATION: char = '`';
#[cfg(not(windows))]
const LINE_CONTINUATION: char = '\\';

/// Returns the command line `start()` would launch, reconstructed from
/// server.ini (the same deterministic args). Formatted for readability: the
/// executable and each `--flag [value]` group sit on their own line, joined
/// with the shell's line-continuation character (`` ` `` on Windows, `\` on
/// POSIX) so the whole block can be pasted straight into a terminal.
pub fn command_line() -> Option<String> {
    let cfg = crate::server_cfg::load();
    let exe = crate::paths::llama_server_exe()?;
    let presets_path = crate::paths::presets_ini();

    // Group the flat arg list into `--flag [value...]` lines: a token that
    // starts with '-' opens a new line; following non-flag tokens (values)
    // attach to the current line.
    let mut lines: Vec<String> = vec![quote_arg(&exe.to_string_lossy())];
    for arg in server_args(&cfg, &presets_path) {
        let q = quote_arg(&arg);
        if arg.starts_with('-') {
            lines.push(q);
        } else if let Some(last) = lines.last_mut() {
            last.push(' ');
            last.push_str(&q);
        }
    }

    let joiner = format!(" {LINE_CONTINUATION}\n  ");
    Some(lines.join(&joiner))
}

fn quote_arg(s: &str) -> String {
    if s.contains(char::is_whitespace) {
        format!("\"{s}\"")
    } else {
        s.to_string()
    }
}
