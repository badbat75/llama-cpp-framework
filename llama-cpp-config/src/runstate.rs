// Probes / starts / stops the llama-server process, and reconstructs its launch
// command from server.ini. `server_args()` is the single source of truth for the
// arg list, shared by `start()` (spawns the process) and `command_line()` (the
// human-readable, shell-pasteable rendering shown in the Server tab's Command
// Line card).

use std::io;

/// `true` if an `llama-server` process is currently running.
pub fn is_running() -> bool {
    #[cfg(windows)]
    {
        let Some(output) = crate::proc::run_hidden(
            std::path::Path::new("tasklist"),
            ["/fi", "IMAGENAME eq llama-server.exe", "/fo", "csv", "/nh"],
        ) else {
            return false;
        };
        // tasklist returns "INFO: No tasks..." when nothing matches — not empty.
        String::from_utf8_lossy(&output.stdout)
            .to_lowercase()
            .contains("llama-server.exe")
    }

    #[cfg(not(windows))]
    {
        std::process::Command::new("pgrep")
            .args(["-f", "llama-server"])
            .output()
            .is_ok_and(|o| o.status.success())
    }
}

/// `true` if the presets file has at least one section (reusing the real INI
/// parser instead of a hand-rolled header scan).
fn has_presets() -> bool {
    !crate::ini::read_all(&crate::paths::presets_ini()).is_empty()
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

    // Fixed framework policy flags (not exposed in the UI):
    //   --webui-mcp-proxy : serve the built-in web UI's MCP proxy endpoint.
    //   -fit off          : disable llama.cpp's auto-fit-to-VRAM. The GUI's
    //                       "auto" n-gpu-layers means "offload ALL layers", so
    //                       auto-fitting would silently override that choice.
    //   -lv 4             : log verbosity 4 (per-request logging) into the
    //                       captured llama-server.log.
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
    ];

    // CPU thread counts: when unset ("auto") omit the flag entirely so llama.cpp
    // applies its own default; only pass an explicit value when the user set one.
    // (We must NOT substitute a computed default here — that would defeat "auto".)
    if let Some(t) = cfg.threads.filter(|&n| n > 0) {
        args.push("-t".into());
        args.push(t.to_string());
    }
    if let Some(tb) = cfg.threads_batch.filter(|&n| n > 0) {
        args.push("--threads-batch".into());
        args.push(tb.to_string());
    }

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
    if is_running() {
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
///
/// Returns `io::Result` for symmetry with `start()`, but is currently infallible:
/// a missing/failed kill is not surfaced here. The caller (`stop_server_async`)
/// re-polls `load()` and reports "still running" if the kill didn't land — that
/// re-check, not this return value, is the source of truth for the outcome.
pub fn stop() -> io::Result<()> {
    #[cfg(windows)]
    {
        // Spawn failure (taskkill missing) is effectively impossible on Windows;
        // if the kill doesn't land, the caller's re-check of the run state
        // surfaces it as "still running".
        crate::proc::run_hidden(
            std::path::Path::new("taskkill"),
            ["/f", "/im", "llama-server.exe"],
        );
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
