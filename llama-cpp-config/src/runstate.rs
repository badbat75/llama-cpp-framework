// Probes / starts / stops the llama-server process, and reconstructs its launch
// command from server.ini. `server_args()` is the single source of truth for the
// arg list, shared by `start()` (spawns the process) and `command_line()` (the
// human-readable, shell-pasteable rendering shown in the Server tab's Command
// Line card). `start()` additionally sets cwd = `paths::data_root()` and
// `LLAMA_CACHE` = ModelsDir, and appends both output streams to
// `logs\llama-server.log` — env/cwd/logging are NOT part of `command_line()`'s
// pasteable rendering, so a pasted command reproduces the args only.

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
    let hostname = cfg.hostname_or_default();
    let port = cfg.port_or_default();
    let models_max = cfg.models_max.unwrap_or(1);
    let mlock = cfg.mlock_or_default();

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

    let models_dir = cfg.models_dir_or_default();

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

    // No console flash: same CREATE_NO_WINDOW as the fire-and-forget probes, but
    // applied to a Command we spawn with custom stdio/env (so not run_hidden).
    crate::proc::hide_console(&mut cmd);

    cmd.spawn()?;

    Ok(())
}

/// Force-kill all llama-server.exe processes (taskkill /f — llama-server has
/// no graceful shutdown channel when running detached without a console).
///
/// Infallible by design: a missing/failed kill is not surfaced here. The caller
/// (`stop_server_async`) re-polls `is_running()` and reports "still running" if
/// the kill didn't land — that re-check is the source of truth for the outcome.
pub fn stop() {
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
    Some(render_command_line(
        &exe.to_string_lossy(),
        &server_args(&cfg, &presets_path),
    ))
}

/// Group the flat arg list into `--flag [value...]` lines: a token that starts
/// with '-' opens a new line; following non-flag tokens (values) attach to the
/// current line. Pure (no IO) — `command_line()` is the config-loading wrapper,
/// mirroring the `render`/`save` split in server_cfg.
fn render_command_line(exe: &str, args: &[String]) -> String {
    let mut lines: Vec<String> = vec![quote_arg(exe)];
    for arg in args {
        let q = quote_arg(arg);
        if arg.starts_with('-') {
            lines.push(q);
        } else if let Some(last) = lines.last_mut() {
            last.push(' ');
            last.push_str(&q);
        }
    }

    let joiner = format!(" {LINE_CONTINUATION}\n  ");
    lines.join(&joiner)
}

fn quote_arg(s: &str) -> String {
    if s.contains(char::is_whitespace) {
        format!("\"{s}\"")
    } else {
        s.to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::server_cfg::ServerConfig;
    use std::path::Path;

    fn args_for(cfg: &ServerConfig) -> Vec<String> {
        server_args(cfg, Path::new("presets.ini"))
    }

    // "auto" (unset / non-positive) MUST omit the flag entirely so llama.cpp
    // applies its own default — substituting a computed value would defeat it.
    #[test]
    fn auto_fields_omit_their_flags() {
        let a = args_for(&ServerConfig::default());
        for flag in ["-t", "--threads-batch", "--cache-reuse", "--device"] {
            assert!(!a.contains(&flag.to_string()), "{flag} must be omitted");
        }
        // Framework policy flags and defaults are always present.
        assert!(a.contains(&"--mlock".to_string()), "mlock defaults to true");
        assert!(a.contains(&"8080".to_string()), "port defaults to 8080");
        assert!(a.contains(&"localhost".to_string()));
        assert!(a.contains(&"--webui-mcp-proxy".to_string()));
    }

    #[test]
    fn explicit_values_emit_flag_value_pairs() {
        let cfg = ServerConfig {
            threads: Some(12),
            threads_batch: Some(24),
            cache_reuse: Some(256),
            mlock: Some(false),
            ..Default::default()
        };
        let a = args_for(&cfg);
        let pair = |flag: &str, val: &str| {
            a.iter()
                .position(|x| x == flag)
                .is_some_and(|i| a.get(i + 1).is_some_and(|v| v == val))
        };
        assert!(pair("-t", "12"));
        assert!(pair("--threads-batch", "24"));
        assert!(pair("--cache-reuse", "256"));
        assert!(!a.contains(&"--mlock".to_string()));
    }

    #[test]
    fn nonpositive_overrides_are_treated_as_auto() {
        let cfg = ServerConfig {
            threads: Some(0),
            threads_batch: Some(-1),
            cache_reuse: Some(0),
            ..Default::default()
        };
        let a = args_for(&cfg);
        for flag in ["-t", "--threads-batch", "--cache-reuse"] {
            assert!(!a.contains(&flag.to_string()), "{flag} must be omitted");
        }
    }

    #[test]
    fn blank_strings_are_omitted_and_padded_ones_trimmed() {
        let cfg = ServerConfig {
            device: Some("  ".into()),
            split_mode: Some(" row ".into()),
            tensor_split: Some("3,1".into()),
            ..Default::default()
        };
        let a = args_for(&cfg);
        assert!(!a.contains(&"--device".to_string()));
        let i = a.iter().position(|x| x == "--split-mode").unwrap();
        assert_eq!(a[i + 1], "row");
        let i = a.iter().position(|x| x == "--tensor-split").unwrap();
        assert_eq!(a[i + 1], "3,1");
    }

    // The Command Line card's grouping: values share their flag's line, each
    // line joins with the platform continuation char so the block pastes
    // straight into a terminal.
    #[test]
    fn render_command_line_groups_flags_with_their_values() {
        let args: Vec<String> = ["--port", "8080", "--mlock", "-fit", "off"]
            .iter()
            .map(|s| s.to_string())
            .collect();
        let out = render_command_line(r"C:\bin\llama-server.exe", &args);
        let lines: Vec<&str> = out.lines().collect();
        assert_eq!(lines.len(), 4, "exe + one line per flag group");
        assert!(lines[0].starts_with(r"C:\bin\llama-server.exe"));
        assert!(lines[1].contains("--port 8080"), "value attaches to flag");
        assert!(lines[2].contains("--mlock"));
        assert!(lines[3].trim_start().starts_with("-fit off"));
        // Every line but the last ends with the continuation char.
        for line in &lines[..lines.len() - 1] {
            assert!(line.ends_with(LINE_CONTINUATION), "bad tail: {line:?}");
        }
        assert!(!lines[lines.len() - 1].ends_with(LINE_CONTINUATION));
    }

    #[test]
    fn quote_arg_quotes_whitespace_only() {
        assert_eq!(
            quote_arg(r"C:\path with spaces\x.exe"),
            "\"C:\\path with spaces\\x.exe\""
        );
        assert_eq!(quote_arg("--port"), "--port");
        // Embedded quotes are NOT escaped — no config value carries them today;
        // revisit if one ever can.
        assert_eq!(quote_arg("a\"b"), "a\"b");
    }
}
