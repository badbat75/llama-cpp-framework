//! Probes / starts / stops the llama-server process, and reconstructs its launch
//! command from server.ini. `server_args()` is the single source of truth for the
//! arg list, shared by `start()` (spawns the process) and `command_line()` (the
//! human-readable, shell-pasteable rendering shown in the Server tab's Command
//! Line card). `start()` additionally sets cwd = `paths::data_root()`,
//! `LLAMA_CACHE` = ModelsDir and the ROCm PATH prepend
//! (`proc::prepend_rocm_path` — HIP devices vanish without it), and appends both
//! output streams to `logs\llama-server.log` — env/cwd/logging are NOT part of
//! `command_line()`'s pasteable rendering, so a pasted command reproduces the
//! args only.

use std::io;

// ── Run detection (is_running + tasklist) ────────────────────────────────

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
        running_from_tasklist(
            &String::from_utf8_lossy(&output.stdout),
            &crate::proc::probe_pids(),
        )
    }

    #[cfg(not(windows))]
    {
        std::process::Command::new("pgrep")
            .args(["-f", "llama-server"])
            .output()
            .is_ok_and(|o| o.status.success())
    }
}

/// The PID (2nd CSV field) of a `tasklist /fo csv /nh` row like
/// `"llama-server.exe","1234","Console","1","12,345 K"`. Splitting on `,` is
/// safe for field 2 — the only comma-bearing field (Mem Usage) comes after it.
#[cfg(windows)]
fn parse_tasklist_pid(line: &str) -> Option<u32> {
    line.split(',')
        .nth(1)?
        .trim()
        .trim_matches('"')
        .parse()
        .ok()
}

/// `true` if the tasklist output names a live `llama-server.exe` whose PID is
/// NOT one of our own transient probes. tasklist prints "INFO: No tasks..." when
/// nothing matches (so no row names the image). A row whose PID won't parse is
/// counted as a real server (safer to over-report than to miss a live one).
#[cfg(windows)]
fn running_from_tasklist(stdout: &str, probe_pids: &[u32]) -> bool {
    stdout.lines().any(|line| {
        line.to_lowercase().contains("llama-server.exe")
            && parse_tasklist_pid(line).is_none_or(|pid| !probe_pids.contains(&pid))
    })
}

/// `true` if the presets file has at least one section (reusing the real INI
/// parser instead of a hand-rolled header scan).
fn has_presets() -> bool {
    !crate::ini::read_all(&crate::paths::presets_ini()).is_empty()
}

// ── Launch argument assembly (server_args) ───────────────────────────────

/// The full llama-server argument list derived from server.ini.
/// Single source of truth for both `start()` and `command_line()`.
fn server_args(
    cfg: &crate::server_cfg::ServerConfig,
    presets_path: &std::path::Path,
) -> Vec<String> {
    let hostname = cfg.hostname_or_default();
    let mlock = cfg.mlock_or_default();

    let mut args: Vec<String> = vec![
        "--models-preset".into(),
        presets_path.to_string_lossy().into_owned(),
    ];

    // --port / --models-max are omitted when unset (the UI's "default" checkbox
    // stores None). Forcing a value here is exactly what made "default" still
    // emit `--port 8080` / `--models-max 1`; omitting the flag lets llama.cpp
    // apply its own default (port 8080; 4 resident models). A 0 port is never
    // valid so it counts as unset, but models-max 0 (= unlimited) IS a real
    // value and is passed through.
    if let Some(p) = cfg.port.filter(|&n| n > 0) {
        args.push("--port".into());
        args.push(p.to_string());
    }
    if let Some(mm) = cfg.models_max {
        args.push("--models-max".into());
        args.push(mm.to_string());
    }

    args.push("--host".into());
    args.push(hostname);

    // --webui-mcp-proxy : serve the built-in web UI's MCP proxy endpoint. A
    //   presence flag — omitted when off (llama.cpp then defaults it disabled).
    // -fit on|off       : llama.cpp's auto-fit-to-VRAM. Always passed with an
    //   explicit value; defaults off because the GUI's "default" n-gpu-layers
    //   means "offload ALL layers", which -fit on would silently override.
    // Both were fixed framework policy; the Server tab's Advanced card now
    // exposes them (they still keep those framework defaults when untouched).
    if cfg.webui_mcp_proxy_or_default() {
        args.push("--webui-mcp-proxy".into());
    }
    args.push("-fit".into());
    args.push(if cfg.fit_or_default() {
        "on".into()
    } else {
        "off".into()
    });

    // -lv N : log verbosity threshold into the captured llama-server.log.
    //   Framework default 4 (per-request logging) when unset — always passed
    //   (the Server tab's Advanced card exposes the level).
    args.push("-lv".into());
    args.push(cfg.log_verbosity_or_default().to_string());

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

/// How long `start()` watches a freshly spawned llama-server before declaring
/// success — long enough to catch a launch that dies on a bad preset, a taken
/// port, or a missing model (those exit within ~1 s), short enough that a
/// healthy server (still alive while it loads the model) isn't held up.
const LAUNCH_GRACE: std::time::Duration = std::time::Duration::from_millis(2500);

// ── Process control (start / stop) ───────────────────────────────────────

/// Launch llama-server.exe with args from server.ini + presets.ini.
///
/// Returns `Some(cfg)` — the config the process was ACTUALLY launched with —
/// so the caller can snapshot the client URL from it (re-loading server.ini
/// after the fact would race a save landing between the two reads). `Ok(None)`
/// means the server was already running: nothing was launched, so there is no
/// launch config to snapshot — the live process may be on an older saved
/// config, or not GUI-launched at all.
///
/// After spawning, it watches the child for `LAUNCH_GRACE`: a process that
/// exits in that window (bad preset, port already bound, model load failure)
/// returns an `Err` pointing at the log, so the caller reports the failure NOW
/// instead of an optimistic "started" that the 5 s status tick later contradicts
/// with "no longer running". A process still alive after the window is reported
/// as started (it may still be loading the model — that's fine, it's up).
pub fn start() -> io::Result<Option<crate::server_cfg::ServerConfig>> {
    if is_running() {
        return Ok(None); // already running — we launched nothing
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
    let log_path = crate::paths::server_log();
    if let Some(log_dir) = log_path.parent() {
        std::fs::create_dir_all(log_dir)?;
    }

    let log_file = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(&log_path)?;

    let mut cmd = std::process::Command::new(&exe);
    cmd.args(server_args(&cfg, &presets_path));

    cmd.current_dir(&data_root);
    cmd.env("LLAMA_CACHE", &models_dir);
    // Make ggml-hip.dll loadable — the HIP SDK's bin dir isn't on the system
    // PATH, and ggml silently skips a backend whose DLL deps don't resolve.
    crate::proc::prepend_rocm_path(&mut cmd);
    // llama-server writes most of its logging (model load, request logs,
    // GGML asserts, crash traces) to stderr — capture both streams in the
    // same log file so the log isn't empty and crashes leave a trail.
    let log_file_err = log_file.try_clone()?;
    cmd.stdout(log_file);
    cmd.stderr(log_file_err);

    // No console flash: same CREATE_NO_WINDOW as the fire-and-forget probes, but
    // applied to a Command we spawn with custom stdio/env (so not run_hidden).
    crate::proc::hide_console(&mut cmd);

    let mut child = cmd.spawn()?;

    // Confirm the server SURVIVES launch — spawning only proves the exe started,
    // not that it got past arg parsing / port bind / model load. Poll for a
    // short grace window; if it exits, surface that immediately with the log
    // path (llama-server writes its own error trail there). Runs on the caller's
    // worker thread (`start_server_async`), so the brief blocking wait keeps the
    // UI in "Starting…" rather than stalling it.
    let step = std::time::Duration::from_millis(150);
    let mut waited = std::time::Duration::ZERO;
    while waited < LAUNCH_GRACE {
        match child.try_wait() {
            // Exited during the grace window → a failed launch, not a running server.
            Ok(Some(status)) => {
                return Err(io::Error::other(format!(
                    "llama-server exited on launch ({status}). Check the log: {}",
                    log_path.display()
                )));
            }
            // Still alive — keep it running and stop watching.
            Ok(None) => {}
            // Can't poll the child; don't block the launch on a probe failure.
            Err(_) => break,
        }
        std::thread::sleep(step);
        waited += step;
    }

    // Dropping `child` here does NOT kill the process on Windows (std leaves it
    // detached) — the server keeps running; we just stop watching it.
    Ok(Some(cfg))
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

// ── Command-line rendering ───────────────────────────────────────────────

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
    // PowerShell parses a quoted string at command position as an expression,
    // not a command — and the default install path ("C:\Program Files\…") gets
    // quoted by `quote_arg`. The call operator makes the paste work there and
    // is harmless for unquoted paths.
    #[cfg(windows)]
    let exe_line = format!("& {}", quote_arg(exe));
    #[cfg(not(windows))]
    let exe_line = quote_arg(exe);
    let mut lines: Vec<String> = vec![exe_line];
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

    #[cfg(windows)]
    #[test]
    fn tasklist_pid_parses_the_second_csv_field() {
        assert_eq!(
            parse_tasklist_pid(r#""llama-server.exe","1234","Console","1","12,345 K""#),
            Some(1234)
        );
        assert_eq!(parse_tasklist_pid("INFO: No tasks are running ..."), None);
    }

    // The startup false-positive fix: a --version / --list-devices probe shares
    // the llama-server.exe image name, so is_running must exclude its PID or a
    // fresh GUI flips to "no longer running" when the probe exits.
    #[cfg(windows)]
    #[test]
    fn running_excludes_our_own_probe_pids() {
        let none = "INFO: No tasks are running which match the specified criteria.";
        assert!(!running_from_tasklist(none, &[]));

        let one = r#""llama-server.exe","1234","Console","1","12,345 K""#;
        assert!(
            running_from_tasklist(one, &[]),
            "a real server reads as running"
        );
        assert!(
            !running_from_tasklist(one, &[1234]),
            "the same PID, if it's our probe, must not"
        );

        let two = "\"llama-server.exe\",\"1234\",\"Console\",\"1\",\"10 K\"\n\
                   \"llama-server.exe\",\"5678\",\"Console\",\"1\",\"20 K\"";
        assert!(
            running_from_tasklist(two, &[1234]),
            "a real server alongside a live probe still counts"
        );
        assert!(
            !running_from_tasklist(two, &[1234, 5678]),
            "both are probes"
        );
    }

    // "auto" (unset / non-positive) MUST omit the flag entirely so llama.cpp
    // applies its own default — substituting a computed value would defeat it.
    #[test]
    fn auto_fields_omit_their_flags() {
        let a = args_for(&ServerConfig::default());
        // port / models-max join the unset-omits-the-flag group: the UI
        // "default" checkbox stores None, and a forced value here is what left
        // "default" still showing --port 8080 / --models-max 1.
        for flag in [
            "-t",
            "--threads-batch",
            "--cache-reuse",
            "--device",
            "--port",
            "--models-max",
        ] {
            assert!(!a.contains(&flag.to_string()), "{flag} must be omitted");
        }
        // Framework policy flags and always-written fields remain present.
        assert!(a.contains(&"--mlock".to_string()), "mlock defaults to true");
        assert!(
            a.contains(&"localhost".to_string()),
            "host is always written"
        );
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

    // Step-8 guard of the server-field recipe (top of server_cfg.rs): every
    // ServerConfig field must be consumed by the launch path — mapped to a
    // llama-server flag here, or explicitly waved through with a comment
    // saying where `start()` uses it. The exhaustive destructure breaks
    // compilation the moment a field is added, until this test decides.
    #[test]
    fn server_args_covers_every_config_field() {
        let cfg = ServerConfig {
            port: Some(9090),
            hostname: Some("0.0.0.0".into()),
            mlock: Some(true),
            threads: Some(6),
            cache_reuse: Some(64),
            threads_batch: Some(12),
            models_max: Some(3),
            models_dir: Some(r"D:\models".into()),
            device: Some("CUDA0".into()),
            split_mode: Some("row".into()),
            tensor_split: Some("3,1".into()),
            webui_mcp_proxy: Some(false),
            fit: Some(true),
            log_verbosity: Some(2),
        };
        let ServerConfig {
            port,
            hostname,
            mlock,
            threads,
            cache_reuse,
            threads_batch,
            models_max,
            models_dir: _, // launch env only: start() exports it as LLAMA_CACHE
            device,
            split_mode,
            tensor_split,
            webui_mcp_proxy,
            fit,
            log_verbosity,
        } = cfg.clone();
        let a = args_for(&cfg);
        let pair = |flag: &str, val: String| {
            a.iter()
                .position(|x| x == flag)
                .is_some_and(|i| a.get(i + 1).is_some_and(|v| *v == val))
        };
        assert!(pair("--port", port.unwrap().to_string()));
        assert!(pair("--host", hostname.unwrap()));
        assert_eq!(a.contains(&"--mlock".to_string()), mlock.unwrap());
        assert!(pair("-t", threads.unwrap().to_string()));
        assert!(pair("--cache-reuse", cache_reuse.unwrap().to_string()));
        assert!(pair("--threads-batch", threads_batch.unwrap().to_string()));
        assert!(pair("--models-max", models_max.unwrap().to_string()));
        assert!(pair("--device", device.unwrap()));
        assert!(pair("--split-mode", split_mode.unwrap()));
        assert!(pair("--tensor-split", tensor_split.unwrap()));
        // webui-mcp-proxy is a presence flag: Some(false) here ⇒ omitted.
        assert_eq!(
            a.contains(&"--webui-mcp-proxy".to_string()),
            webui_mcp_proxy.unwrap()
        );
        // fit is always passed with an explicit on|off value.
        assert!(pair("-fit", if fit.unwrap() { "on" } else { "off" }.into()));
        // log verbosity is always passed (framework default 4 when unset).
        assert!(pair("-lv", log_verbosity.unwrap().to_string()));
    }

    // Unlike the thread/port fields (where <= 0 means "unset"), models-max 0 is
    // a real value (= unlimited) and must be passed, not mistaken for "default".
    #[test]
    fn models_max_zero_is_passed_as_unlimited() {
        let a = args_for(&ServerConfig {
            models_max: Some(0),
            ..Default::default()
        });
        let at = a.iter().position(|x| x == "--models-max");
        assert_eq!(
            at.and_then(|i| a.get(i + 1)).map(String::as_str),
            Some("0"),
            "models-max 0 (unlimited) is passed explicitly"
        );
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
        #[cfg(windows)]
        assert!(lines[0].starts_with(r"& C:\bin\llama-server.exe"));
        #[cfg(not(windows))]
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

    // The default NSIS install dir has a space ("C:\Program Files\llama.cpp"),
    // so the pasted line must survive PowerShell's expression-vs-command
    // parsing: a bare quoted path at command position is an error there.
    #[cfg(windows)]
    #[test]
    fn render_command_line_calls_a_quoted_exe_with_the_call_operator() {
        let out = render_command_line(
            r"C:\Program Files\llama.cpp\bin\llama-server.exe",
            &["--port".to_string(), "8080".to_string()],
        );
        assert!(
            out.starts_with("& \"C:\\Program Files\\llama.cpp\\bin\\llama-server.exe\""),
            "bad exe line: {out:?}"
        );
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
