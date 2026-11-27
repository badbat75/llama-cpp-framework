use super::*;
use crate::server_cfg::ServerConfig;
use std::path::Path;

fn args_for(cfg: &ServerConfig) -> Vec<String> {
    server_args(cfg, Path::new("models.ini"))
}

// ── Stop wait ────────────────────────────────────────────────────────

/// A process that outlives several polls (the router waiting for a child
/// to write out its KV cache) is waited for, not reported gone at once.
#[test]
fn wait_gone_waits_for_a_slow_exit() {
    use std::time::{Duration, Instant};
    let exits_at = Instant::now() + Duration::from_millis(400);
    let mut polls = 0;
    let gone = wait_gone(
        || {
            polls += 1;
            Instant::now() < exits_at
        },
        Duration::from_secs(5),
        Duration::from_millis(20),
    );
    assert!(gone);
    assert!(Instant::now() >= exits_at);
    assert!(polls > 1);
}

/// One that never exits is reported still running once the grace is up.
#[test]
fn wait_gone_gives_up_after_the_grace() {
    use std::time::{Duration, Instant};
    let began = Instant::now();
    let gone = wait_gone(
        || true,
        Duration::from_millis(200),
        Duration::from_millis(20),
    );
    assert!(!gone);
    assert!(began.elapsed() >= Duration::from_millis(200));
    assert!(began.elapsed() < Duration::from_secs(5));
}

/// Nothing left to wait for: no sleep at all.
#[test]
fn wait_gone_returns_at_once_when_nothing_runs() {
    use std::time::Duration;
    assert!(wait_gone(|| false, Duration::ZERO, Duration::from_secs(60)));
}

// ── Log rotation ─────────────────────────────────────────────────────

fn closed_logs(dir: &Path) -> Vec<String> {
    let mut v: Vec<String> = std::fs::read_dir(dir)
        .unwrap()
        .flatten()
        .map(|e| e.file_name().to_string_lossy().into_owned())
        .filter(|n| n.starts_with("llama-server-"))
        .collect();
    v.sort();
    v
}

#[test]
fn close_log_files_the_run_under_a_stamp_and_leaves_a_blank_successor() {
    let dir = tempfile::tempdir().unwrap();
    let log = dir.path().join("llama-server.log");
    std::fs::write(&log, "run one\n").unwrap();

    let closed = close_log(&log, LogStamp::Now, 0)
        .unwrap()
        .expect("a run to close");
    let name = closed.file_name().unwrap().to_string_lossy().into_owned();
    assert!(
        name.starts_with("llama-server-")
            && name.ends_with(".log")
            && name.len() == "llama-server-yyyymmdd-hhmmss.log".len(),
        "llama-server-yyyymmdd-hhmmss.log, got {name}"
    );
    assert_eq!(std::fs::read_to_string(&closed).unwrap(), "run one\n");
    // The live path is back, empty: the Log window resets on it and the
    // next start appends to a clean file.
    assert_eq!(std::fs::metadata(&log).unwrap().len(), 0);
}

/// The threshold is "larger than", in bytes: a log exactly at it stays.
#[test]
fn close_log_leaves_a_log_at_or_under_the_threshold() {
    let dir = tempfile::tempdir().unwrap();
    let log = dir.path().join("llama-server.log");
    std::fs::write(&log, "x".repeat(1024)).unwrap();
    assert_eq!(close_log(&log, LogStamp::Now, 1024).unwrap(), None);
    assert_eq!(
        std::fs::metadata(&log).unwrap().len(),
        1024,
        "left in place"
    );
    assert!(closed_logs(dir.path()).is_empty());
    std::fs::write(&log, "x".repeat(1025)).unwrap();
    assert!(close_log(&log, LogStamp::Now, 1024).unwrap().is_some());
    assert_eq!(std::fs::metadata(&log).unwrap().len(), 0);
}

#[test]
fn close_log_is_a_no_op_on_a_missing_or_empty_file() {
    let dir = tempfile::tempdir().unwrap();
    let log = dir.path().join("llama-server.log");
    assert_eq!(close_log(&log, LogStamp::Now, 0).unwrap(), None);
    assert!(!log.exists(), "nothing to close creates nothing");

    std::fs::write(&log, "").unwrap();
    assert_eq!(close_log(&log, LogStamp::LastWrite, 0).unwrap(), None);
    assert!(
        closed_logs(dir.path()).is_empty(),
        "an empty log is never filed"
    );
}

#[test]
fn close_log_keeps_both_runs_on_a_same_second_collision() {
    let dir = tempfile::tempdir().unwrap();
    let log = dir.path().join("llama-server.log");
    std::fs::write(&log, "first\n").unwrap();
    let a = close_log(&log, LogStamp::Now, 0).unwrap().unwrap();
    std::fs::write(&log, "second\n").unwrap();
    let b = close_log(&log, LogStamp::Now, 0).unwrap().unwrap();
    assert_ne!(a, b);
    assert_eq!(std::fs::read_to_string(&a).unwrap(), "first\n");
    assert_eq!(std::fs::read_to_string(&b).unwrap(), "second\n");
    // Same second, or the clock ticked over between the two closes; either
    // way both stamps sort after the live file and before nothing.
    assert_eq!(closed_logs(dir.path()).len(), 2);
}

#[test]
fn close_log_last_write_stamps_from_the_file_mtime() {
    let dir = tempfile::tempdir().unwrap();
    let log = dir.path().join("llama-server.log");
    std::fs::write(&log, "crashed run\n").unwrap();
    // Push the mtime a day into the past: the stamp must follow the FILE,
    // not the clock, or a log found at start would be filed under the
    // start time rather than the run's end.
    let past = std::time::SystemTime::now() - std::time::Duration::from_secs(86_400);
    let f = std::fs::OpenOptions::new().write(true).open(&log).unwrap();
    f.set_modified(past).unwrap();
    drop(f);
    let expected = crate::bench::stamp(
        past.duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs(),
    );
    let closed = close_log(&log, LogStamp::LastWrite, 0).unwrap().unwrap();
    assert_eq!(
        closed.file_name().unwrap().to_string_lossy(),
        format!("llama-server-{expected}.log")
    );
}

#[test]
fn prune_keeps_the_newest_closed_logs_and_never_the_live_one() {
    let dir = tempfile::tempdir().unwrap();
    for stamp in ["20260101-000000", "20260102-000000", "20260103-000000"] {
        std::fs::write(dir.path().join(format!("llama-server-{stamp}.log")), "x").unwrap();
    }
    std::fs::write(dir.path().join("llama-server.log"), "live").unwrap();
    std::fs::write(dir.path().join("other.log"), "not ours").unwrap();
    prune_closed_logs(dir.path(), "llama-server", 2);
    assert_eq!(
        closed_logs(dir.path()),
        vec![
            "llama-server-20260102-000000.log".to_string(),
            "llama-server-20260103-000000.log".to_string(),
        ]
    );
    assert!(dir.path().join("llama-server.log").exists());
    assert!(dir.path().join("other.log").exists());
}

#[test]
fn close_log_prunes_past_the_retention_cap() {
    let dir = tempfile::tempdir().unwrap();
    for i in 0..ROTATED_LOGS_KEPT {
        std::fs::write(
            dir.path().join(format!("llama-server-20260101-{i:06}.log")),
            "x",
        )
        .unwrap();
    }
    let log = dir.path().join("llama-server.log");
    std::fs::write(&log, "one more\n").unwrap();
    close_log(&log, LogStamp::Now, 0).unwrap().unwrap();
    let kept = closed_logs(dir.path());
    assert_eq!(kept.len(), ROTATED_LOGS_KEPT);
    assert!(
        !kept.contains(&"llama-server-20260101-000000.log".to_string()),
        "oldest goes"
    );
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

// The Linux probe reads /proc/<pid>/stat. A zombie is the regression: the
// killed server stays listed under its name until the configurator reaps
// it, and counting it made every Stop time out.
#[cfg(not(windows))]
#[test]
fn live_server_pid_skips_zombies_and_other_names() {
    use super::live_server_pid;
    assert_eq!(
        live_server_pid("29023 (llama-server) S 28855 29023 5495 0 -1"),
        Some(29023)
    );
    assert_eq!(live_server_pid("29023 (llama-server) R 1 2 3"), Some(29023));
    assert_eq!(
        live_server_pid("29023 (llama-server) Z 28855 29023 5495 0 -1"),
        None,
        "a zombie is not a running server"
    );
    assert_eq!(live_server_pid("29023 (llama-server) X 1 2 3"), None);
    assert_eq!(
        live_server_pid("28855 (llama-cpp-confi) S 5495 1 2"),
        None,
        "the configurator itself"
    );
    assert_eq!(
        live_server_pid("31 (tail) S 1 2 3"),
        None,
        "a command line naming llama-server.log is not a server"
    );
    // The name is everything up to the LAST ')': it may hold parentheses.
    assert_eq!(live_server_pid("7 (a) (llama-server) S 1 2 3"), None);
    assert_eq!(live_server_pid("garbage"), None);
}

// The Command Line card's grouping: values share their flag's line, each
// line joins with the platform continuation char so the block pastes
// straight into a terminal.
#[test]
fn render_command_line_groups_flags_with_their_values() {
    // --webui-mcp-proxy is the valueless flag of the three: it must get a
    // line of its own rather than swallow the next flag as its value.
    let args: Vec<String> = ["--port", "8080", "--webui-mcp-proxy", "-fit", "off"]
        .iter()
        .map(|s| s.to_string())
        .collect();
    let out = render_command_line(r"C:\bin\llama-server.exe", &args, &[]);
    let lines: Vec<&str> = out.lines().collect();
    assert_eq!(lines.len(), 4, "exe + one line per flag group");
    #[cfg(windows)]
    assert!(lines[0].starts_with(r"& C:\bin\llama-server.exe"));
    #[cfg(not(windows))]
    assert!(lines[0].starts_with(r"C:\bin\llama-server.exe"));
    assert!(lines[1].contains("--port 8080"), "value attaches to flag");
    assert!(lines[2].contains("--webui-mcp-proxy"));
    assert!(lines[3].trim_start().starts_with("-fit off"));
    // Every line but the last ends with the continuation char.
    for line in &lines[..lines.len() - 1] {
        assert!(line.ends_with(LINE_CONTINUATION), "bad tail: {line:?}");
    }
    assert!(!lines[lines.len() - 1].ends_with(LINE_CONTINUATION));
}

// The default install dir has a space ("C:\Program Files\llama.cpp"),
// so the pasted line must survive PowerShell's expression-vs-command
// parsing: a bare quoted path at command position is an error there.
#[cfg(windows)]
#[test]
fn render_command_line_calls_a_quoted_exe_with_the_call_operator() {
    let out = render_command_line(
        r"C:\Program Files\llama.cpp\bin\llama-server.exe",
        &["--port".to_string(), "8080".to_string()],
        &[],
    );
    assert!(
        out.starts_with("& \"C:\\Program Files\\llama.cpp\\bin\\llama-server.exe\""),
        "bad exe line: {out:?}"
    );
}

/// A default config with the FlashAttention band OFF: the one SDK knob
/// whose default exports something, switched off so the env tests of the
/// others can pin "unset exports nothing".
fn band_off() -> ServerConfig {
    ServerConfig {
        ..Default::default()
    }
}

// The SDK-tuning tri-state, on the env-only path.
// Three things it pins: the value is the INT rocBLAS parses ("0"/"1", never
// "false"/"true"), `None` exports NOTHING (a default-on-every-machine
// workaround is exactly what the tri-state exists to avoid), and it reaches
// the pasteable command line; a block that dropped it would run the backend
// this setting exists to steer away from.

/// The launch env without the GTT pair (GT1): `env_vars` exports
/// `HSA_USERPTR_FOR_PAGED_MEM` and `HSA_SVM_FOR_PAGED_MEM` for every profile
/// that does not switch them off, so these env tests of the other knobs read
/// the env with that pair filtered out. The pair itself is pinned in
/// `pai_core::launch`.
fn env_vars_no_gtt(cfg: &ServerConfig) -> Vec<(String, String)> {
    env_vars(cfg)
        .into_iter()
        .filter(|(k, _)| !k.starts_with("HSA_"))
        .collect()
}

#[test]
fn rocblas_hipblaslt_rides_the_env_as_zero_or_one() {
    let with = |v: Option<bool>| ServerConfig {
        rocblas_use_hipblaslt: v,
        ..band_off()
    };
    assert_eq!(
        env_vars_no_gtt(&with(Some(false))),
        [("ROCBLAS_USE_HIPBLASLT".to_string(), "0".to_string())]
    );
    assert_eq!(
        env_vars_no_gtt(&with(Some(true))),
        [("ROCBLAS_USE_HIPBLASLT".to_string(), "1".to_string())]
    );
    assert!(
        env_vars_no_gtt(&with(None)).is_empty(),
        "unset must export nothing"
    );
    // Not an argument on either surface: llama-server would refuse it.
    assert!(!args_for(&with(Some(false)))
        .iter()
        .any(|a| a.contains("HIPBLASLT")));

    let out = render_command_line(
        r"C:\bin\llama-server.exe",
        &[],
        &env_vars(&with(Some(false))),
    );
    assert!(
        out.contains("ROCBLAS_USE_HIPBLASLT"),
        "no env line in:\n{out}"
    );
}

#[test]
fn quote_arg_quotes_whitespace_only() {
    assert_eq!(
        quote_arg(r"C:\path with spaces\x.exe"),
        "\"C:\\path with spaces\\x.exe\""
    );
    assert_eq!(quote_arg("--port"), "--port");
    // Embedded quotes are NOT escaped: no config value carries them today;
    // revisit if one ever can.
    assert_eq!(quote_arg("a\"b"), "a\"b");
}
