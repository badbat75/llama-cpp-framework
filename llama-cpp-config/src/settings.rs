//! settings.ini schema and IO: the configurator's OWN settings.
//!
//! Not to be confused with server.ini (`server_cfg.rs`): every key there maps
//! to a llama-server flag or launch-env value and rides the full per-field
//! recipe (CLI `server set`, `runstate::server_args`, …). The keys here
//! configure llama-cpp-config itself (they never reach a llama-server command
//! line), so they live in their own file with none of that machinery.
//!
//! Note the one Settings-tab toggle that is deliberately NOT here: "Start with
//! Windows" is the HKCU Run registry entry itself (`startup.rs`); mirroring it
//! into this file would only add a copy that Task Manager's Startup panel can
//! silently desync.

use std::fs;
use std::io;

use crate::ini;
use crate::paths;

/// `LogRotateKb` when the key is absent: 1 MiB. Below it a run's log is short
/// enough that keeping it above the next run costs nothing worth a file.
pub const LOG_ROTATE_KB_DEFAULT: u32 = 1024;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Settings {
    /// Start llama-server automatically when the GUI launches (any launch, not
    /// just the logon one). `false` when unset: the framework default.
    pub start_server_on_launch: bool,
    /// Close `logs\llama-server.log` when llama-server stops (`runstate::stop`,
    /// and `runstate::start` for a run that ended without one), filing it as
    /// `llama-server-<stamp>.log` so the next run writes into a clean file. ON
    /// when unset. Off = the pre-1.15.1 behaviour, one file growing forever.
    pub log_rotate: bool,
    /// The size, in KB, a log must EXCEED to be closed; a shorter one stays
    /// and the next run appends below it. `LOG_ROTATE_KB_DEFAULT` when unset
    /// or unparsable; 0 = close every non-empty log. Read through
    /// `log_rotate_threshold`, which folds `log_rotate` in.
    pub log_rotate_kb: u32,
    /// The live benchmark's prompt file. Empty = the default under `config\`
    /// (`paths::bench_prompt_file`); read through `bench_prompt_path` below,
    /// never directly, so "unset" resolves in ONE place.
    ///
    /// It lives here rather than in a benchmark-specific store because it is
    /// exactly what this file is for: a preference of the configurator, not a
    /// llama-server flag. It is also the only piece of the benchmark plan that
    /// persists across launches, and deliberately so: a workload is a decision
    /// per run, while which prompt file you use is a decision per machine.
    pub bench_prompt_file: String,
}

impl Default for Settings {
    /// The framework defaults, i.e. what an absent settings.ini means. Hand
    /// written because two of them are not the type's zero: rotation is on and
    /// its threshold is `LOG_ROTATE_KB_DEFAULT`.
    fn default() -> Self {
        Settings {
            start_server_on_launch: false,
            log_rotate: true,
            log_rotate_kb: LOG_ROTATE_KB_DEFAULT,
            bench_prompt_file: String::new(),
        }
    }
}

impl Settings {
    /// The configured prompt file, or the default when nothing is set.
    pub fn bench_prompt_path(&self) -> std::path::PathBuf {
        if self.bench_prompt_file.trim().is_empty() {
            paths::bench_prompt_file()
        } else {
            std::path::PathBuf::from(self.bench_prompt_file.trim())
        }
    }

    /// The size in BYTES a log must exceed to be closed, or `None` when
    /// rotation is off. The one place the two keys combine, so `runstate`
    /// never reads them apart.
    pub fn log_rotate_threshold(&self) -> Option<u64> {
        self.log_rotate.then_some(u64::from(self.log_rotate_kb) * 1024)
    }
}

pub fn load() -> Settings {
    from_keys(&ini::read_section(&paths::settings_ini(), "Settings"))
}

/// Parse the `[Settings]` key/value map. Split out of `load()` so the
/// round-trip test can run against `render()`'s output without touching the
/// real config path (mirroring `server_cfg::from_keys`).
fn from_keys(keys: &std::collections::BTreeMap<String, String>) -> Settings {
    let defaults = Settings::default();
    Settings {
        start_server_on_launch: keys
            .get("StartServerOnLaunch")
            .and_then(|v| ini::parse_bool(v))
            .unwrap_or(defaults.start_server_on_launch),
        log_rotate: keys
            .get("LogRotate")
            .and_then(|v| ini::parse_bool(v))
            .unwrap_or(defaults.log_rotate),
        // A negative or unparsable value is the default, not "off": off has
        // its own key, and a typo in a size should not silently stop rotation.
        log_rotate_kb: keys
            .get("LogRotateKb")
            .and_then(|v| ini::parse_int(v))
            .and_then(|n| u32::try_from(n).ok())
            .unwrap_or(defaults.log_rotate_kb),
        bench_prompt_file: keys.get("BenchPromptFile").cloned().unwrap_or_default(),
    }
}

pub fn save(cfg: &Settings) -> io::Result<()> {
    // A path field, so it rides the same save-boundary guard every other one
    // does: this reader strips everything from the first `;`/`#`, with no
    // escape, so such a path would write fine and reload pointing somewhere
    // else (`ini::reject_comment_markers`).
    ini::reject_comment_markers("BenchPromptFile", &cfg.bench_prompt_file)?;
    let path = paths::settings_ini();
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    ini::atomic_write(&path, &render(cfg))
}

/// Render the whole settings.ini body. Like server.ini, the file is fully
/// generated and self-documents its keys.
fn render(cfg: &Settings) -> String {
    let autostart_lit = if cfg.start_server_on_launch {
        "true"
    } else {
        "false"
    };
    let log_rotate_lit = if cfg.log_rotate { "true" } else { "false" };
    let log_rotate_kb = cfg.log_rotate_kb;
    let kept = crate::runstate::ROTATED_LOGS_KEPT;
    let prompt_file = cfg.bench_prompt_file.trim();
    format!(
        "; Generated by llama-cpp-config.
;
; Settings of the configurator itself (Settings tab). llama-server's runtime
; configuration lives in server.ini; per-model knobs in presets.ini.

[Settings]
; StartServerOnLaunch: start llama-server automatically whenever
; llama-cpp-config launches. A no-op when the server is already running.
StartServerOnLaunch = {autostart_lit}

; LogRotate: when llama-server stops, file logs\\llama-server.log away as
; llama-server-<yyyymmdd-hhmmss>.log (the closing instant, UTC) and start the
; next run in a clean file; the newest {kept} closed logs are kept. A run that
; ended without a stop (crash, external kill) is filed at the next start under
; its last-write time instead.
LogRotate = {log_rotate_lit}
; LogRotateKb: only a log LARGER than this many KB is filed; a shorter one
; stays and the next run appends below it. 0 = file every non-empty log.
LogRotateKb = {log_rotate_kb}

; BenchPromptFile: the text file the live benchmark sends (Benchmark tab, and
; `bench sweep` with no --prompt / --prompt-file). Empty = the
; default, config\\bench-prompt.txt, which is created from the framework's own
; prompt the first time a benchmark needs one. Point it at a longer file to
; measure the regime a long conversation actually runs in.
BenchPromptFile = {prompt_file}
"
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    /// `render` → parse-back through the real INI reader: a key-name typo
    /// between `from_keys` and the writer fails here (same guard shape as
    /// server_cfg's round-trip).
    fn round_trip(cfg: &Settings) -> Settings {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("settings.ini");
        std::fs::write(&path, render(cfg)).unwrap();
        from_keys(&ini::read_section(&path, "Settings"))
    }

    #[test]
    fn both_states_round_trip_through_ini() {
        for on in [false, true] {
            let cfg = Settings {
                start_server_on_launch: on,
                log_rotate: !on,
                ..Settings::default()
            };
            assert_eq!(round_trip(&cfg), cfg);
        }
    }

    /// The threshold rides as a bare integer; 0 (file every non-empty log) and
    /// a large value both survive, and the OFF state is its own key, not a
    /// magic size.
    #[test]
    fn log_rotation_keys_round_trip() {
        for kb in [0, 1, 1024, 250_000] {
            let cfg = Settings {
                log_rotate_kb: kb,
                ..Settings::default()
            };
            assert_eq!(round_trip(&cfg), cfg);
        }
        let off = Settings {
            log_rotate: false,
            ..Settings::default()
        };
        assert_eq!(round_trip(&off), off);
        assert_eq!(off.log_rotate_threshold(), None);
        assert_eq!(
            Settings::default().log_rotate_threshold(),
            Some(u64::from(LOG_ROTATE_KB_DEFAULT) * 1024)
        );
    }

    /// A typo in the size is the default, never "off": off has its own key.
    #[test]
    fn unparsable_or_negative_threshold_falls_back_to_the_default() {
        for bad in ["-5", "abc", "12.5", ""] {
            let mut keys = std::collections::BTreeMap::new();
            keys.insert("LogRotateKb".to_string(), bad.to_string());
            let cfg = from_keys(&keys);
            assert_eq!(cfg.log_rotate_kb, LOG_ROTATE_KB_DEFAULT, "{bad:?}");
            assert!(cfg.log_rotate, "{bad:?} must not switch rotation off");
        }
    }

    /// A path with spaces survives the trip: the value is written bare (no
    /// quoting in this format) and read back trimmed, so a quoting bug would
    /// show up as a path that gained or lost characters.
    #[test]
    fn prompt_file_round_trips() {
        let cfg = Settings {
            bench_prompt_file: r"D:\bench prompts\long-context.txt".into(),
            ..Settings::default()
        };
        assert_eq!(round_trip(&cfg), cfg);
    }

    #[test]
    fn missing_file_or_key_reads_as_default_off() {
        let cfg = from_keys(&Default::default());
        assert!(!cfg.start_server_on_launch);
        // Rotation is the one preference whose default is ON.
        assert!(cfg.log_rotate);
        assert_eq!(cfg.log_rotate_kb, LOG_ROTATE_KB_DEFAULT);
        // Unset means "the default file", resolved in ONE place: an empty
        // string must never reach a `PathBuf` as an empty path. Checked by file
        // NAME, not against `paths::bench_prompt_file()`: the data root is a
        // process-wide env var the e2e test redirects, so comparing two
        // separate resolutions would be a flake waiting for a scheduler.
        assert!(cfg.bench_prompt_file.is_empty());
        assert_eq!(
            cfg.bench_prompt_path().file_name().unwrap(),
            "bench-prompt.txt"
        );
    }

    /// The save-boundary guard on the one path field here: this format has no
    /// escaping, so a `;`/`#` in the path would reload truncated and the
    /// benchmark would read a different file (or none).
    #[test]
    fn prompt_file_with_a_comment_marker_is_refused() {
        let cfg = Settings {
            bench_prompt_file: r"D:\prompts\run #2.txt".into(),
            ..Settings::default()
        };
        let err = save(&cfg).expect_err("a '#' in the path must not be persisted");
        assert!(
            err.to_string().contains("BenchPromptFile"),
            "the error names the field: {err}"
        );
    }
}
