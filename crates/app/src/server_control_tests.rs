use super::fake::{self, FakeClient};
use super::*;
use pai_core::client::InstanceState;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;

// These touch no path: start and restart (which load server.ini) are
// pinned in `server_control_e2e`.

#[test]
fn manager_stop_stops_default_and_waits_until_it_is_gone() {
    let log = fake::log();
    let b = fake::backend(FakeClient::new(&log, InstanceState::Active));
    assert_eq!(b.stop(), Ok(true));
    assert_eq!(fake::actions(&log), ["stop default"]);
    // The stop is queued, not done, when the manager answers: the seam
    // asks again until the unit is past Deactivating.
    let entries = log.lock().unwrap().clone();
    let after_stop = entries
        .iter()
        .skip_while(|e| *e != "stop default")
        .filter(|e| *e == "status default")
        .count();
    assert!(
        after_stop >= 2,
        "status asked until Inactive, once per state: {entries:?}"
    );
}

#[test]
fn manager_stop_refused_is_the_client_text() {
    let log = fake::log();
    let mut c = FakeClient::new(&log, InstanceState::Active);
    c.fail_actions = Some(ClientError::NotAuthorized);
    let b = fake::backend(c);
    assert_eq!(b.stop(), Err(ClientError::NotAuthorized.text()));
    assert_eq!(fake::actions(&log), ["stop default"]);
}

/// A process is there from Activating to Deactivating: it holds the GPU
/// and the port, which is what every caller of `is_running` asks (the
/// benchmarks wait for it to be gone before they touch the GPU).
#[test]
fn manager_is_running_reads_each_state() {
    for (state, running) in [
        (InstanceState::Active, true),
        (InstanceState::Activating, true),
        (InstanceState::Deactivating, true),
        (InstanceState::Inactive, false),
        (InstanceState::Failed, false),
        (InstanceState::Other("maintenance".into()), false),
    ] {
        let log = fake::log();
        let b = fake::backend(FakeClient::new(&log, state.clone()));
        assert_eq!(b.is_running(), running, "{state:?}");
        assert_eq!(
            log.lock().unwrap().clone(),
            ["status default"],
            "{state:?}: one status probe of default, nothing else"
        );
    }
}

#[test]
fn manager_status_error_reads_as_not_running() {
    for e in [ClientError::Unavailable, ClientError::NotAuthorized] {
        let log = fake::log();
        let mut c = FakeClient::new(&log, InstanceState::Active);
        c.fail_status = Some(e.clone());
        assert!(!fake::backend(c).is_running(), "{e:?}");
    }
}

/// No manager: the user reads that the manager is not running, from the
/// strings file, and nothing panics.
#[test]
fn no_manager_says_so_and_reads_as_not_running() {
    let log = fake::log();
    let mut c = FakeClient::new(&log, InstanceState::Inactive);
    c.fail_actions = Some(ClientError::Unavailable);
    c.fail_status = Some(ClientError::Unavailable);
    let b = fake::backend(c);
    assert!(!b.is_running());
    assert_eq!(b.stop(), Err(ClientError::Unavailable.text()));
}

// ── Start at boot ───────────────────────────────────────────────────
// Where llama-server runs as a service, the Settings tab's toggle and
// `pai server boot` are the instance's start at boot, the service
// manager's own state, read and changed through the manager.

#[test]
fn boot_enabled_reads_the_managers_state_of_default() {
    for on in [true, false] {
        let log = fake::log();
        let c = FakeClient::new(&log, InstanceState::Inactive);
        *c.boot.lock().unwrap() = on;
        let b = fake::backend(c);
        assert_eq!(b.boot_enabled(), Ok(on));
        assert_eq!(log.lock().unwrap().clone(), ["boot_enabled default"]);
    }
}

#[test]
fn a_failed_boot_read_is_the_client_text() {
    for e in [ClientError::Unavailable, ClientError::NoInstance] {
        let log = fake::log();
        let mut c = FakeClient::new(&log, InstanceState::Inactive);
        c.fail_boot = Some(e.clone());
        assert_eq!(fake::backend(c).boot_enabled(), Err(e.text()), "{e:?}");
    }
    let b = Backend::Manager(ManagerBackend::new(Box::new(|| {
        Err(ClientError::Unavailable)
    })));
    assert_eq!(b.boot_enabled(), Err(ClientError::Unavailable.text()));
    assert_eq!(
        b.set_boot_enabled(true),
        Err(ClientError::Unavailable.text())
    );
}

#[test]
fn set_boot_enabled_asks_the_manager_for_default() {
    for on in [true, false] {
        let log = fake::log();
        let c = FakeClient::new(&log, InstanceState::Inactive);
        *c.boot.lock().unwrap() = !on;
        let boot = c.boot.clone();
        let b = fake::backend(c);
        assert_eq!(b.set_boot_enabled(on), Ok(()));
        let word = if on { "on" } else { "off" };
        assert_eq!(
            fake::actions(&log),
            [format!("set_boot_enabled default {word}")]
        );
        assert_eq!(*boot.lock().unwrap(), on);
    }
}

/// Enabling is under `configure` (an administrator): a user without it
/// reads the manager's refusal in the strings file, nothing changed.
#[test]
fn a_refused_boot_change_is_the_client_text() {
    let log = fake::log();
    let mut c = FakeClient::new(&log, InstanceState::Inactive);
    c.fail_set_boot = Some(ClientError::NotAuthorized);
    let boot = c.boot.clone();
    let b = fake::backend(c);
    assert_eq!(
        b.set_boot_enabled(true),
        Err(ClientError::NotAuthorized.text())
    );
    assert!(!*boot.lock().unwrap(), "nothing changed");
}

/// The new strings, by content: `strings::t` gives back the key itself
/// when it is missing, which a stub using the same key would pass.
fn boot_message(on: bool) -> String {
    let key = if on {
        "start-at-boot-on"
    } else {
        "start-at-boot-off"
    };
    let text = strings::t(key);
    assert!(
        !text.starts_with(key) && text.contains("when the system starts"),
        "{key}: {text:?}"
    );
    text
}

/// A click applies at once, as every Settings toggle does: the toggle
/// keeps the state asked for and the footer says what changed.
#[test]
fn a_boot_toggle_the_manager_accepts_keeps_its_state_and_says_so() {
    for want in [true, false] {
        let log = fake::log();
        let c = FakeClient::new(&log, InstanceState::Inactive);
        *c.boot.lock().unwrap() = !want;
        let boot = c.boot.clone();
        let b = fake::backend(c);
        assert_eq!(
            toggle_boot(&b, want),
            BootToggle {
                enabled: want,
                message: boot_message(want),
                error: false,
            },
            "{want}"
        );
        assert_eq!(*boot.lock().unwrap(), want);
        let word = if want { "on" } else { "off" };
        assert_eq!(
            fake::actions(&log),
            [format!("set_boot_enabled default {word}")]
        );
    }
}

/// A refusal shows the manager's reason as an error and puts the
/// toggle back to the manager's state, read again.
#[test]
fn a_refused_boot_toggle_is_put_back_and_shows_the_refusal() {
    for want in [true, false] {
        let log = fake::log();
        let mut c = FakeClient::new(&log, InstanceState::Inactive);
        *c.boot.lock().unwrap() = !want;
        c.fail_set_boot = Some(ClientError::NotAuthorized);
        let b = fake::backend(c);
        assert_eq!(
            toggle_boot(&b, want),
            BootToggle {
                enabled: !want,
                message: ClientError::NotAuthorized.text(),
                error: true,
            },
            "{want}"
        );
        let entries = log.lock().unwrap().clone();
        let word = if want { "on" } else { "off" };
        let set = entries
            .iter()
            .position(|e| *e == format!("set_boot_enabled default {word}"))
            .unwrap_or_else(|| panic!("no set: {entries:?}"));
        assert!(
            entries[set..].iter().any(|e| e == "boot_enabled default"),
            "the state read again after the refusal: {entries:?}"
        );
    }
}

/// No manager at all: the toggle goes back to where it was before the
/// click, the footer says the manager is not running.
#[test]
fn a_boot_toggle_with_no_manager_is_put_back() {
    for want in [true, false] {
        let log = fake::log();
        let mut c = FakeClient::new(&log, InstanceState::Inactive);
        c.fail_set_boot = Some(ClientError::Unavailable);
        c.fail_boot = Some(ClientError::Unavailable);
        assert_eq!(
            toggle_boot(&fake::backend(c), want),
            BootToggle {
                enabled: !want,
                message: ClientError::Unavailable.text(),
                error: true,
            },
            "{want}"
        );
    }
}

/// pai-config starts llama-server at its launch only where it runs it
/// itself (macOS, `Direct`), and only when settings.ini asks; under the
/// manager the OS starts the instance at boot, and the launch starts
/// nothing whatever an earlier install's `StartServerOnLaunch` says.
#[test]
fn only_direct_starts_the_server_at_launch_and_only_when_asked() {
    assert!(start_at_launch(Kind::Direct, true));
    assert!(!start_at_launch(Kind::Direct, false));
    assert!(!start_at_launch(Kind::Manager, true));
    assert!(!start_at_launch(Kind::Manager, false));
    for os in [Os::Linux, Os::Windows] {
        assert!(!start_at_launch(backend_kind(os), true), "{os:?}");
    }
    assert!(start_at_launch(backend_kind(Os::MacOs), true));
}

/// The manager leaves the two directories unset, on Linux as on Windows:
/// its instances' accounts (`private-ai` on Linux, the virtual accounts on
/// Windows) cannot read a folder under the saving user's home. macOS
/// (no manager) and any direct launch keep writing them resolved.
#[test]
fn the_manager_leaves_the_dirs_unset_on_linux_and_windows() {
    assert!(dirs_left_unset(Kind::Manager, Os::Windows));
    assert!(dirs_left_unset(Kind::Manager, Os::Linux));
    assert!(!dirs_left_unset(Kind::Manager, Os::MacOs));
    for os in [Os::Linux, Os::Windows, Os::MacOs] {
        assert!(!dirs_left_unset(Kind::Direct, os), "{os:?}");
    }
    assert!(dirs_left_unset(backend_kind(Os::Windows), Os::Windows));
    assert!(dirs_left_unset(backend_kind(Os::Linux), Os::Linux));
    assert!(!dirs_left_unset(backend_kind(Os::MacOs), Os::MacOs));
}

/// The code of `server_cfg::save` (pai-app), before any tests.
fn app_save_body() -> String {
    let code = include_str!("server_cfg.rs")
        .split("#[cfg(test)]")
        .next()
        .unwrap();
    let start = code.find("pub fn save(").expect("fn save");
    let rest = &code[start..];
    rest[..rest.find("\n}\n").expect("end of fn")].to_string()
}

/// The save of the GUI and `pai` renders through `render_for` with the
/// choice above for the real OS. The Windows branch only type-checks on
/// Linux, hence a check on the text.
#[test]
fn the_save_leaves_the_dirs_unset_where_the_os_says() {
    let body = app_save_body();
    for want in ["render_for(", "dirs_left_unset(", "Os::current()"] {
        assert!(
            body.contains(want),
            "server_cfg::save has no {want}:\n{body}"
        );
    }
    assert!(
        !body.contains("&render(cfg)"),
        "server_cfg::save still renders with render():\n{body}"
    );
}

/// The content check belongs to `pai-core`: pai-app holds no copy of the
/// rule beside the core's. That the save runs the core's check is pinned
/// by `the_save_refuses_threads_above_the_cpu_count`.
#[test]
fn the_save_runs_the_core_s_check() {
    let code = include_str!("server_cfg.rs")
        .split("#[cfg(test)]")
        .next()
        .unwrap();
    assert!(
        !code.contains("fn check("),
        "pai-app defines its own check beside the core's:\n{code}"
    );
}

/// The save of the GUI and `pai` refuses a thread count above this
/// machine's logical CPU count, before asking the manager for anything.
#[test]
fn the_save_refuses_threads_above_the_cpu_count() {
    use crate::server_cfg::{self, ServerConfig};
    let cpus = std::thread::available_parallelism()
        .expect("this machine's CPU count")
        .get();
    let over = i32::try_from(cpus + 1).expect("a CPU count in range");
    let at = i32::try_from(cpus).unwrap_or(i32::MAX);
    // The folders a save may create (macOS, where the dirs are not left
    // unset) go to a temp tree: a plain unit test never redirects the
    // real data root, only an e2e phase does (`paths::redirect_for_tests`).
    let root = tempfile::tempdir().expect("tempdir");
    let base = || ServerConfig {
        port: Some(8099),
        models_dir: Some(root.path().join("models").display().to_string()),
        state_dir: Some(root.path().join("state").display().to_string()),
        ..ServerConfig::default()
    };
    let log = fake::log();
    let c = FakeClient::new(&log, InstanceState::Inactive)
        .with_file("server@default.ini", "[Server]\nPort = 8099\n");
    let files = c.files.clone();
    let before = files.lock().unwrap().clone();
    fake::scoped(fake::backend(c), || {
        for (key, threads, threads_batch) in [
            ("Threads", Some(over), None),
            ("ThreadsBatch", None, Some(over)),
        ] {
            let cfg = ServerConfig {
                threads,
                threads_batch,
                ..base()
            };
            let want = pai_core::checks::Refusal {
                key: "not-whole-number-in-range",
                values: vec![key.into(), over.to_string(), "1".into(), cpus.to_string()],
            };
            let err = server_cfg::save(&cfg).expect_err("threads above the CPU count");
            assert_eq!(err.to_string(), want.text(), "{key} = {over}");
            assert_eq!(
                fake::writes(&log),
                Vec::<String>::new(),
                "{key} = {over}: the manager was not asked"
            );
            assert_eq!(
                *files.lock().unwrap(),
                before,
                "{key} = {over}: nothing written"
            );
        }
        // The same count, at the CPU count, saves: the refusal is about
        // the count, not a save that always fails.
        server_cfg::save(&ServerConfig {
            threads: Some(at),
            ..base()
        })
        .expect("threads at the CPU count");
        assert_eq!(fake::writes(&log).len(), 1, "one write");
    });
}

#[test]
fn a_connection_that_fails_is_unavailable() {
    let b = Backend::Manager(ManagerBackend::new(Box::new(|| {
        Err(ClientError::Unavailable)
    })));
    assert!(!b.is_running());
    assert_eq!(b.stop(), Err(ClientError::Unavailable.text()));
}

#[test]
fn the_manager_is_connected_on_first_use_and_once() {
    let connects = Arc::new(AtomicUsize::new(0));
    let log = fake::log();
    let client = std::sync::Mutex::new(Some(FakeClient::new(&log, InstanceState::Active)));
    let counted = connects.clone();
    let b = Backend::Manager(ManagerBackend::new(Box::new(move || {
        counted.fetch_add(1, Ordering::SeqCst);
        let c = client.lock().unwrap().take().expect("connected twice");
        Ok(Box::new(c) as Client)
    })));
    assert_eq!(connects.load(Ordering::SeqCst), 0, "not at construction");
    assert!(b.is_running());
    assert_eq!(b.stop(), Ok(true));
    assert!(!b.is_running());
    assert_eq!(connects.load(Ordering::SeqCst), 1, "once, then reused");
}

/// The manager may start after the configurator: a connect that failed is
/// tried again on the next call, and the client it then makes is used.
#[test]
fn a_failed_connect_is_retried_on_the_next_call() {
    let connects = Arc::new(AtomicUsize::new(0));
    let log = fake::log();
    let client = std::sync::Mutex::new(Some(FakeClient::new(&log, InstanceState::Active)));
    let counted = connects.clone();
    let b = Backend::Manager(ManagerBackend::new(Box::new(move || {
        if counted.fetch_add(1, Ordering::SeqCst) == 0 {
            return Err(ClientError::Unavailable);
        }
        let c = client
            .lock()
            .unwrap()
            .take()
            .expect("connected after success");
        Ok(Box::new(c) as Client)
    })));
    assert!(!b.is_running(), "no manager yet");
    assert_eq!(connects.load(Ordering::SeqCst), 1);
    assert!(
        b.is_running(),
        "the manager is up now: the new client answers"
    );
    assert_eq!(connects.load(Ordering::SeqCst), 2, "tried again");
    assert_eq!(b.stop(), Ok(true));
    assert_eq!(connects.load(Ordering::SeqCst), 2, "kept once connected");
    assert_eq!(fake::actions(&log), ["stop default"]);
}

#[cfg(target_os = "linux")]
#[test]
fn the_backend_on_linux_is_the_manager() {
    assert!(matches!(os_backend(), Backend::Manager(_)));
}

#[cfg(not(target_os = "linux"))]
#[test]
fn the_backend_off_linux_is_direct() {
    assert!(matches!(os_backend(), Backend::Direct));
}

// ── The backend of each OS ──────────────────────────────────────────

#[cfg(target_os = "linux")]
#[test]
fn the_current_os_is_linux_here() {
    assert_eq!(Os::current(), Os::Linux);
}

/// The manager runs llama-server on Linux (D-Bus) and Windows (the named
/// pipe); macOS has no manager.
#[test]
fn the_manager_runs_llama_server_on_linux_and_windows_not_on_macos() {
    assert_eq!(backend_kind(Os::Linux), Kind::Manager);
    assert_eq!(backend_kind(Os::Windows), Kind::Manager);
    assert_eq!(backend_kind(Os::MacOs), Kind::Direct);
}

/// `os_backend_for` builds the kind `backend_kind` names; the Windows
/// manager connects through `system_connect`, which a test build never
/// dials: the instance reads as not running.
#[test]
fn the_backend_of_each_os_is_its_kind_and_never_dials_from_a_test() {
    for os in [Os::Linux, Os::Windows] {
        let b = os_backend_for(os);
        assert!(matches!(b, Backend::Manager(_)), "{os:?}");
        assert!(!b.is_running(), "{os:?}: no manager in a test build");
        assert_eq!(b.stop(), Err(ClientError::Unavailable.text()), "{os:?}");
    }
    assert!(matches!(os_backend_for(Os::MacOs), Backend::Direct));
}

/// Only the client that kills llama-server saves the slot snapshots:
/// `Direct` (macOS) stops the process it spawned, so the save is its job;
/// under the manager the instance saves at its own stop, so the client
/// saving too would write the same gigabytes twice. The decision is pure:
/// it asks the manager nothing.
#[test]
fn the_client_saves_the_snapshots_only_on_the_local_backend() {
    assert!(
        client_saves_snapshots(&Backend::Direct),
        "Direct kills llama-server: nothing else can save before it"
    );
    // The manager backend of this OS, the one a shipped Linux build runs.
    let log = fake::log();
    let b = fake::backend(FakeClient::new(&log, InstanceState::Active));
    assert!(
        !client_saves_snapshots(&b),
        "under the manager the instance saves at its own stop"
    );
    assert_eq!(
        log.lock().unwrap().clone(),
        Vec::<String>::new(),
        "the decision asks the manager nothing"
    );
}

/// Under the manager the log is the unit's journal on Linux and the
/// service's file `llama-server@default.log` on Windows, read by the file
/// tail; `Direct` reads the per-user file on every OS. (The Windows
/// file's directory is pinned in `server_control_e2e`, under the redirect.)
#[test]
fn the_log_is_the_journal_on_linux_and_the_services_file_on_windows() {
    assert_eq!(
        log_source_for(Kind::Manager, Os::Linux),
        LogSource::Journal {
            unit: "llama.cpp@default.service".into()
        }
    );
    match log_source_for(Kind::Manager, Os::Windows) {
        LogSource::File(p) => {
            assert_eq!(p.file_name().unwrap(), "llama-server@default.log", "{p:?}")
        }
        other => panic!("Windows manager reads the service's file, got {other:?}"),
    }
    for os in [Os::Linux, Os::Windows, Os::MacOs] {
        match log_source_for(Kind::Direct, os) {
            LogSource::File(p) => {
                assert_eq!(p.file_name().unwrap(), "llama-server.log", "{os:?}: {p:?}")
            }
            other => panic!("{os:?}: Direct reads a file, got {other:?}"),
        }
    }
}

/// The Settings note under the manager: journald rotates on Linux; on
/// Windows the service keeps one file per run and the previous run's
/// (`manager::windows::open_instance_log`), so the note names no journal.
#[test]
fn the_rotation_note_names_journald_on_linux_only() {
    assert_eq!(log_rotate_note_key(Os::Linux), "log-rotate-journald");
    let key = log_rotate_note_key(Os::Windows);
    assert_ne!(key, "log-rotate-journald");
    let text = crate::strings::t(key);
    assert_ne!(text, key, "{key} is in the strings file");
    assert!(
        !text.to_lowercase().contains("journal"),
        "the Windows note names no journal: {text}"
    );
}

// ── The configuration files ─────────────────────────────────────────

#[test]
fn config_files_have_the_manager_names() {
    assert_eq!(ConfigFile::Server.manager_name(), "server@default.ini");
    assert_eq!(ConfigFile::Models.manager_name(), "models.ini");
    assert_eq!(
        ConfigFile::ModelsDisabled.manager_name(),
        "models.ini.disabled"
    );
}

#[test]
fn manager_read_config_reads_each_file_from_the_manager() {
    let log = fake::log();
    let c = FakeClient::new(&log, InstanceState::Inactive)
        .with_file("server@default.ini", "[Server]\nPort = 8099\n")
        .with_file("models.ini", "[Qwen3.8-27B]\nctx-size = 8192\n");
    let b = fake::backend(c);
    assert_eq!(
        b.read_config(ConfigFile::Server),
        Ok(Some("[Server]\nPort = 8099\n".to_string()))
    );
    assert_eq!(
        b.read_config(ConfigFile::Models),
        Ok(Some("[Qwen3.8-27B]\nctx-size = 8192\n".to_string()))
    );
    assert_eq!(
        b.read_config(ConfigFile::ModelsDisabled),
        Ok(None),
        "a file the manager does not have is none, not an error"
    );
    assert_eq!(
        log.lock().unwrap().clone(),
        [
            "read_file server@default.ini",
            "read_file models.ini",
            "read_file models.ini.disabled"
        ]
    );
}

#[test]
fn manager_write_config_hands_the_contents_to_the_manager() {
    for file in ConfigFile::ALL {
        let log = fake::log();
        let c = FakeClient::new(&log, InstanceState::Inactive);
        let files = c.files.clone();
        let b = fake::backend(c);
        let text = format!("; {file:?}\r\n[m]\r\nctx-size = 4096\r\n");
        assert_eq!(b.write_config(file, &text), Ok(()), "{file:?}");
        let name = file.manager_name();
        assert_eq!(fake::writes(&log), [format!("write_file {name}")]);
        assert_eq!(files.lock().unwrap().get(&name), Some(&text), "{file:?}");
    }
}

/// A caller the manager withheld the API key from saves back what it
/// read: the backend remembers that the last read of the server file was
/// withheld, and a server write that holds no key (the key field left
/// unset: a blank `OpencodeApiKey =`, or only the commented hint a
/// rendering of an unset key writes) asks the manager to keep its own.
/// A key the user typed is sent as typed, nothing kept. A read of the
/// model profile files in between (a save reads them first) changes nothing.
#[test]
fn a_server_write_after_a_withheld_read_keeps_the_managers_key() {
    let blanked = "[Server]\nPort = 8080\nOpencodeApiKey =\n";
    let log = fake::log();
    let c = FakeClient::new(&log, InstanceState::Inactive)
        .with_file("server@default.ini", blanked)
        .with_file("models.ini", "[Qwen3.8-27B]\nctx-size = 8192\n");
    *c.withhold.lock().unwrap() = true;
    let b = fake::backend(c);
    assert!(!b.api_key_withheld(), "nothing read yet");
    assert_eq!(
        b.read_config(ConfigFile::Server),
        Ok(Some(blanked.to_string()))
    );
    assert!(b.api_key_withheld(), "the last server read was withheld");
    assert!(b.read_config(ConfigFile::Models).is_ok());
    assert!(b.api_key_withheld(), "a preset read changes nothing");
    let kept = format!("write_file server@default.ini {}", fake::KEEP_API_KEY);
    for text in [
        "[Server]\nPort = 8090\nOpencodeApiKey =\n",
        "[Server]\nPort = 8090\n; OpencodeApiKey = sk-xxx  ; API key for proxy/gateway authentication (opencode.json apiKey + Claude Code); blank = none\n",
    ] {
        log.lock().unwrap().clear();
        assert_eq!(b.write_config(ConfigFile::Server, text), Ok(()), "{text}");
        assert_eq!(fake::writes(&log), std::slice::from_ref(&kept), "{text}");
    }
    log.lock().unwrap().clear();
    assert_eq!(
        b.write_config(
            ConfigFile::Server,
            "[Server]\nPort = 8090\nOpencodeApiKey = sk-typed-0000\n"
        ),
        Ok(())
    );
    assert_eq!(
        fake::writes(&log),
        ["write_file server@default.ini"],
        "a typed key is sent, nothing kept"
    );
    log.lock().unwrap().clear();
    assert_eq!(b.write_config(ConfigFile::Models, "[m]\n"), Ok(()));
    assert_eq!(
        fake::writes(&log),
        ["write_file models.ini"],
        "a preset file never keeps a key"
    );
}

/// Nothing withheld, nothing kept: a server write with no key replaces
/// the manager's (the key cleared, as before), when no read of the
/// server file in this process was withheld.
#[test]
fn a_server_write_after_a_full_read_keeps_nothing() {
    let log = fake::log();
    let c = FakeClient::new(&log, InstanceState::Inactive).with_file(
        "server@default.ini",
        "[Server]\nPort = 8080\nOpencodeApiKey = sk-local-3f9a\n",
    );
    let b = fake::backend(c);
    let no_key = "[Server]\nPort = 8090\nOpencodeApiKey =\n";

    assert!(b.read_config(ConfigFile::Server).is_ok());
    assert!(!b.api_key_withheld());
    assert_eq!(b.write_config(ConfigFile::Server, no_key), Ok(()));
    assert_eq!(fake::writes(&log), ["write_file server@default.ini"]);
    assert!(b.read_config(ConfigFile::Server).is_ok());
    assert!(!b.api_key_withheld(), "still nothing withheld");
}

/// Withheld once, withheld for the process: on Linux polkit's
/// `auth_admin_keep` lets a later quiet check pass, so the read after an
/// administrator's first save comes back in full while the form still
/// holds no key. The flag must not follow that read, or the second save
/// would erase the key. A process that saw the key withheld cannot clear
/// it; a new process can.
#[test]
fn a_withheld_read_keeps_the_key_after_a_later_full_read() {
    let log = fake::log();
    let c = FakeClient::new(&log, InstanceState::Inactive).with_file(
        "server@default.ini",
        "[Server]\nPort = 8080\nOpencodeApiKey =\n",
    );
    let withhold = c.withhold.clone();
    *withhold.lock().unwrap() = true;
    let b = fake::backend(c);
    let kept = format!("write_file server@default.ini {}", fake::KEEP_API_KEY);

    assert!(b.read_config(ConfigFile::Server).is_ok());
    assert!(b.api_key_withheld(), "withheld");
    assert_eq!(
        b.write_config(
            ConfigFile::Server,
            "[Server]\nPort = 8090\nOpencodeApiKey =\n"
        ),
        Ok(())
    );
    assert_eq!(
        fake::writes(&log),
        std::slice::from_ref(&kept),
        "first save"
    );

    *withhold.lock().unwrap() = false;
    assert!(b.read_config(ConfigFile::Server).is_ok());
    assert!(
        b.api_key_withheld(),
        "a later full read does not clear the flag"
    );
    log.lock().unwrap().clear();
    assert_eq!(
        b.write_config(
            ConfigFile::Server,
            "[Server]\nPort = 8091\nOpencodeApiKey =\n"
        ),
        Ok(())
    );
    assert_eq!(
        fake::writes(&log),
        std::slice::from_ref(&kept),
        "second save"
    );

    log.lock().unwrap().clear();
    assert_eq!(
        b.write_config(
            ConfigFile::Server,
            "[Server]\nPort = 8091\nOpencodeApiKey = sk-typed-0000\n"
        ),
        Ok(())
    );
    assert_eq!(
        fake::writes(&log),
        ["write_file server@default.ini"],
        "a typed key is still sent, nothing kept"
    );
}

/// `Direct` reads the user's own file: nothing is ever withheld.
#[test]
fn direct_never_withholds() {
    assert!(!Backend::Direct.api_key_withheld());
}

/// A write the manager refuses reaches the user as the refusal's own text
/// (in the language of the strings file this process reads), and the
/// manager's other errors as the client's text.
#[test]
fn a_refused_config_write_is_the_refusals_text() {
    let refusal = pai_core::checks::Refusal {
        key: "not-whole-number-in-range",
        values: ["Port", "99999", "1", "65535"].map(String::from).to_vec(),
    };
    for e in [
        ClientError::Invalid(refusal.clone()),
        ClientError::NotAuthorized,
        ClientError::Unavailable,
    ] {
        let log = fake::log();
        let mut c = FakeClient::new(&log, InstanceState::Inactive)
            .with_file("server@default.ini", "[Server]\nPort = 8080\n");
        c.fail_write = Some(e.clone());
        let files = c.files.clone();
        let b = fake::backend(c);
        assert_eq!(
            b.write_config(ConfigFile::Server, "[Server]\nPort = 99999\n"),
            Err(e.text()),
            "{e:?}"
        );
        assert_eq!(
            files.lock().unwrap().get("server@default.ini").cloned(),
            Some("[Server]\nPort = 8080\n".to_string()),
            "{e:?}: the file as it was"
        );
    }
    assert_eq!(
        ClientError::Invalid(refusal.clone()).text(),
        refusal.text(),
        "Invalid is the refusal's text"
    );
}

#[test]
fn a_failed_config_read_is_the_client_text() {
    for e in [ClientError::NotAuthorized, ClientError::Unavailable] {
        let log = fake::log();
        let mut c = FakeClient::new(&log, InstanceState::Inactive).with_file("models.ini", "[m]\n");
        c.fail_read = Some(e.clone());
        let b = fake::backend(c);
        assert_eq!(b.read_config(ConfigFile::Models), Err(e.text()), "{e:?}");
    }
    let b = Backend::Manager(ManagerBackend::new(Box::new(|| {
        Err(ClientError::Unavailable)
    })));
    assert_eq!(
        b.read_config(ConfigFile::Server),
        Err(ClientError::Unavailable.text()),
        "no connection"
    );
}

/// No test binary of pai-config dials the system bus, whatever runs
/// first: `system_connect` is `Unavailable` in a test build even without
/// the redirect. The redirect cannot be unset in this process (the e2e
/// test sets it), so the check runs in a child of this binary with no
/// redirect and `DBUS_SYSTEM_BUS_ADDRESS` on a socket of ours, which
/// nothing may connect to.
#[cfg(target_os = "linux")]
#[test]
fn system_connect_never_dials_from_a_test_binary() {
    use std::time::Instant;
    let dir = tempfile::tempdir().expect("tempdir");
    let socket = dir.path().join("system-bus");
    let listener = std::os::unix::net::UnixListener::bind(&socket).expect("bind bus socket");
    listener.set_nonblocking(true).expect("nonblocking");
    let mut child = std::process::Command::new(std::env::current_exe().expect("test binary"))
        .args([
            "--exact",
            "server_control::tests::system_connect_child",
            "--include-ignored",
        ])
        .args(["--nocapture", "--test-threads", "1"])
        .env(CHILD_MARKER, "1")
        .env_remove("PRIVATE_AI_DATA_ROOT")
        .env(
            "DBUS_SYSTEM_BUS_ADDRESS",
            format!("unix:path={}", socket.display()),
        )
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .spawn()
        .expect("run the child test");
    let deadline = Instant::now() + Duration::from_secs(30);
    let status = loop {
        if let Some(status) = child.try_wait().expect("wait") {
            break status;
        }
        if Instant::now() > deadline {
            let _ = child.kill();
            let _ = child.wait();
            panic!("the child did not finish within 30 s");
        }
        std::thread::sleep(Duration::from_millis(50));
    };
    let out = child.wait_with_output().expect("child output");
    let stdout = String::from_utf8_lossy(&out.stdout);
    match listener.accept() {
        Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => {}
        Ok(_) => panic!("system_connect dialed the system bus from a test binary:\n{stdout}"),
        Err(e) => panic!("accept: {e}"),
    }
    assert!(
        status.success(),
        "system_connect_child failed:\n{stdout}\n{}",
        String::from_utf8_lossy(&out.stderr)
    );
    assert!(
        stdout.contains("1 passed"),
        "the child ran no test:\n{stdout}"
    );
}

/// Set by `system_connect_never_dials_from_a_test_binary` on its child.
#[cfg(target_os = "linux")]
const CHILD_MARKER: &str = "PAI_TEST_SYSTEM_CONNECT_CHILD";

/// The child half of `system_connect_never_dials_from_a_test_binary`:
/// ignored, and a no-op without the marker.
#[cfg(target_os = "linux")]
#[test]
#[ignore = "run by system_connect_never_dials_from_a_test_binary in a child process"]
fn system_connect_child() {
    if std::env::var_os(CHILD_MARKER).is_none() {
        return;
    }
    assert!(std::env::var_os("PRIVATE_AI_DATA_ROOT").is_none());
    let (tx, rx) = std::sync::mpsc::channel();
    std::thread::spawn(move || {
        let _ = tx.send(system_connect().map(|_| ()));
    });
    match rx.recv_timeout(Duration::from_secs(10)) {
        Ok(r) => assert_eq!(r, Err(ClientError::Unavailable)),
        Err(std::sync::mpsc::RecvTimeoutError::Timeout) => {
            panic!("system_connect did not return within 10 s")
        }
        Err(e) => panic!("the probe thread panicked: {e:?}"),
    }
}

// ── The log, through journald on the manager ────────────────────────

#[test]
fn the_manager_reads_the_log_from_the_units_journal_without_a_call() {
    let log = fake::log();
    let b = fake::backend(FakeClient::new(&log, InstanceState::Active));
    assert_eq!(
        b.log_source(),
        LogSource::Journal {
            unit: "llama.cpp@default.service".into()
        }
    );
    assert!(
        log.lock().unwrap().is_empty(),
        "no manager call to know where the log is"
    );
}

#[test]
fn direct_reads_the_log_from_the_per_user_file() {
    match Backend::Direct.log_source() {
        LogSource::File(p) => {
            assert_eq!(p.file_name().unwrap(), "llama-server.log", "{p:?}")
        }
        other => panic!("Direct reads a file, got {other:?}"),
    }
}

/// `LogRotate`/`LogRotateKb` rotate the per-user file; under the manager
/// journald rotates the journal and they do not apply.
#[test]
fn only_direct_rotates_its_log_file() {
    assert!(Backend::Direct.rotates_log_file());
    let log = fake::log();
    let b = fake::backend(FakeClient::new(&log, InstanceState::Active));
    assert!(!b.rotates_log_file());
    assert!(log.lock().unwrap().is_empty());
}

/// The manager runs `llama.cpp@<id>.service` (manager/dbus.rs).
#[test]
fn the_instance_unit_is_the_managers_template_instance() {
    assert_eq!(instance_unit(), "llama.cpp@default.service");
}

/// journalctl(1), systemd 259: `-u` the unit, `-f` follow, `-n` the last
/// N entries first, `-o cat` the message alone, `--no-pager`. No `-q`:
/// it hides the access hint `journal_access_denied` reads.
#[test]
fn journalctl_args_follow_the_unit_message_only_without_a_pager() {
    assert_eq!(
        journalctl_args("llama.cpp@default.service", 5000),
        [
            "-u",
            "llama.cpp@default.service",
            "-f",
            "-n",
            "5000",
            "-o",
            "cat",
            "--no-pager"
        ]
    );
    let args = journalctl_args("llama.cpp@default.service", 7);
    assert!(
        !args.iter().any(|a| a == "-q" || a == "--quiet"),
        "{args:?}"
    );
}

#[test]
fn journalctl_command_runs_journalctl_with_the_args_unspawned() {
    let args = journalctl_args("llama.cpp@default.service", 5000);
    let cmd = journalctl_command(&args);
    assert_eq!(cmd.get_program(), "journalctl");
    let got: Vec<&std::ffi::OsStr> = cmd.get_args().collect();
    let want: Vec<&std::ffi::OsStr> = args.iter().map(std::ffi::OsStr::new).collect();
    assert_eq!(got, want);
}

/// No test reads the real journal: the spawner of a test build spawns
/// nothing and fails.
#[test]
fn the_journal_spawner_of_a_test_build_spawns_nothing() {
    let args = journalctl_args(&instance_unit(), 10);
    if let Ok(mut child) = journal_spawner()(&args) {
        let _ = child.kill();
        let _ = child.wait();
        panic!("a test build spawned a journal reader (pid {})", child.id());
    }
}

/// The texts systemd's `journal_access_check_and_warn` and
/// `access_check_var_log_journal` (src/shared/journal-util.c) print on
/// stderr when the user is not root and not in a group the journal's ACL
/// names (systemd-journal, adm, wheel). The hint comes with exit 0 when
/// the user's own journal opened, the error with a failure exit when
/// nothing did. Captured from systemd 259 (Ubuntu).
#[test]
fn journal_access_denied_reads_systemds_permission_texts() {
    let hint =
        "Hint: You are currently not seeing messages from other users and the system.\n      \
                Users in groups 'adm', 'systemd-journal', 'wheel' can see all messages.\n      \
                Pass -q to turn off this notice.\n";
    assert!(journal_access_denied(hint));
    let short = "Hint: You are currently not seeing messages from the system.\n      \
                 Users in the 'systemd-journal' group can see all messages. Pass -q to\n      \
                 turn off this notice.\n";
    assert!(journal_access_denied(short));
    assert!(journal_access_denied(
        "No journal files were opened due to insufficient permissions.\n"
    ));
    // Not a permission problem: there is simply no journal yet, or nothing.
    assert!(!journal_access_denied("No journal files were found.\n"));
    assert!(!journal_access_denied("-- No entries --\n"));
    assert!(!journal_access_denied(""));
}

// ── A bounded read of the instance's log (`read_log`, `log_text`) ───
// For the readers that want the log's end once, not a follower: the
// memory a benchmark run records, the "server-gone" message. Files are
// temp files; the journal is a fake runner: nothing reads the real
// journal or the user's log.

use std::time::{Duration as Span, SystemTime, UNIX_EPOCH};

/// A runner that must not be called: a file source never runs journalctl.
fn no_journal(args: &[String]) -> std::io::Result<std::process::Output> {
    panic!("a file source ran journalctl with {args:?}");
}

fn file_source(dir: &tempfile::TempDir, text: &[u8]) -> LogSource {
    let path = dir.path().join("llama-server@default.log");
    std::fs::write(&path, text).expect("write the log");
    LogSource::File(path)
}

// Failure paths first.

/// No file yet (the instance never started): an empty log, not an error.
#[test]
fn a_missing_log_file_reads_as_empty() {
    let dir = tempfile::tempdir().unwrap();
    let source = LogSource::File(dir.path().join("llama-server.log"));
    assert_eq!(
        read_log(&source, None, 4096, &no_journal),
        Ok(String::new())
    );
}

/// A log that cannot be read (here a directory where the file should be)
/// is an error with the system's text, not an empty log.
#[test]
fn an_unreadable_log_file_is_an_error() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("llama-server.log");
    std::fs::create_dir(&path).unwrap();
    match read_log(&LogSource::File(path), None, 4096, &no_journal) {
        Err(e) => assert!(!e.is_empty(), "the error says why"),
        Ok(t) => panic!("a directory read as a log: {t:?}"),
    }
}

/// Bytes that are not UTF-8 (a cut multi-byte char, a binary line) are
/// read lossily, never refused.
#[test]
fn a_log_with_invalid_utf8_reads_lossily() {
    let dir = tempfile::tempdir().unwrap();
    let source = file_source(&dir, b"srv  load: ok\n\xff\xfe broken\nlast line\n");
    let text = read_log(&source, None, 4096, &no_journal).expect("read");
    assert!(text.ends_with("last line\n"), "{text:?}");
    assert!(text.contains('\u{fffd}'), "{text:?}");
}

/// A file last written before `since` holds nothing of that window: the
/// file's lines carry no time, its modification time is the cheap check.
#[test]
fn a_log_file_untouched_since_the_window_opened_reads_as_empty() {
    let dir = tempfile::tempdir().unwrap();
    let source = file_source(&dir, b"[52043] old load\n");
    let LogSource::File(path) = &source else {
        unreachable!()
    };
    let written = SystemTime::now() - Span::from_secs(3600);
    std::fs::File::options()
        .write(true)
        .open(path)
        .unwrap()
        .set_modified(written)
        .unwrap();
    let since = written + Span::from_secs(60);
    assert_eq!(
        read_log(&source, Some(since), 4096, &no_journal),
        Ok(String::new())
    );
    // Written inside the window: the whole tail (the lines carry no time).
    let since = written - Span::from_secs(60);
    assert_eq!(
        read_log(&source, Some(since), 4096, &no_journal),
        Ok("[52043] old load\n".to_string())
    );
}

/// A short file comes back whole.
#[test]
fn a_log_under_the_bound_reads_whole() {
    let dir = tempfile::tempdir().unwrap();
    let text = "srv  main: listening\n[52043] llama_server: model loaded\n";
    let source = file_source(&dir, text.as_bytes());
    assert_eq!(
        read_log(&source, None, 4096, &no_journal),
        Ok(text.to_string())
    );
}

/// A long file: only its end, at most `max_bytes`, starting at a line
/// start (the partial first line dropped), ending with the file's end.
#[test]
fn a_log_over_the_bound_reads_its_last_whole_lines() {
    let dir = tempfile::tempdir().unwrap();
    let mut text = String::new();
    for i in 0..1000 {
        text.push_str(&format!("[52043] line {i:04} of the load\n"));
    }
    let source = file_source(&dir, text.as_bytes());
    let max = 100;
    let got = read_log(&source, None, max, &no_journal).expect("read");
    assert!(got.len() <= max, "{} bytes > {max}", got.len());
    assert!(got.ends_with("[52043] line 0999 of the load\n"), "{got:?}");
    assert!(
        got.starts_with("[52043] line "),
        "starts at a line: {got:?}"
    );
    assert!(text.ends_with(&got));
    // 30 bytes a line: 3 whole lines fit in 100 bytes.
    assert_eq!(got.lines().count(), 3, "{got:?}");
}

/// A Windows-style file (CRLF) is cut the same way.
#[test]
fn a_crlf_log_over_the_bound_starts_at_a_line() {
    let dir = tempfile::tempdir().unwrap();
    let text = "first line that is long\r\nsecond\r\nthird\r\n";
    let source = file_source(&dir, text.as_bytes());
    assert_eq!(
        read_log(&source, None, 18, &no_journal),
        Ok("second\r\nthird\r\n".to_string())
    );
}

// ── The journal, once (`journalctl` without `-f`) ───────────────────

const UNIT: &str = "llama.cpp@default.service";

/// journalctl(1), systemd 259: `-u` the unit, `-o cat` the message alone,
/// `--no-pager`, `-n` the last N entries, `-S/--since` entries "on or
/// newer than" a date, which systemd.time(7) lets be "an integer prefixed
/// with @ ... relative to the UNIX epoch". No `-f` (one read to the end)
/// and no `-q` (it hides the access hint `journal_access_denied` reads).
#[test]
fn journalctl_once_args_print_the_units_end_without_following() {
    assert_eq!(
        journalctl_once_args(UNIT, 4096, None),
        ["-u", UNIT, "-o", "cat", "--no-pager", "-n", "4096"]
    );
    let since = UNIX_EPOCH + Span::from_millis(1_795_000_123_900);
    assert_eq!(
        journalctl_once_args(UNIT, 10, Some(since)),
        [
            "-u",
            UNIT,
            "-o",
            "cat",
            "--no-pager",
            "-n",
            "10",
            "--since",
            // Whole seconds, rounded down: the entries of that second count.
            "@1795000123"
        ]
    );
    let args = journalctl_once_args(UNIT, 10, Some(since));
    assert!(
        !args
            .iter()
            .any(|a| a == "-f" || a == "--follow" || a == "-q" || a == "--quiet"),
        "{args:?}"
    );
}

/// The fake journalctl: records the argv, answers with `exit`, `stdout`
/// and `stderr`.
#[cfg(unix)]
fn journal(
    argv: &Arc<Mutex<Vec<Vec<String>>>>,
    exit: i32,
    stdout: &'static str,
    stderr: &'static str,
) -> impl Fn(&[String]) -> std::io::Result<std::process::Output> + Send + Sync {
    use std::os::unix::process::ExitStatusExt;
    let argv = argv.clone();
    move |args: &[String]| {
        argv.lock().unwrap().push(args.to_vec());
        Ok(std::process::Output {
            status: std::process::ExitStatus::from_raw(exit << 8),
            stdout: stdout.as_bytes().to_vec(),
            stderr: stderr.as_bytes().to_vec(),
        })
    }
}

fn journal_source() -> LogSource {
    LogSource::Journal { unit: UNIT.into() }
}

/// Not root and in none of the journal's groups: journalctl prints the
/// hint and exits 0 with nothing of the system unit. The same groups
/// message as the Log window, not an empty log.
#[cfg(unix)]
#[test]
fn a_journal_hint_that_the_system_is_hidden_is_the_groups_message() {
    let argv = Arc::new(Mutex::new(Vec::new()));
    let run = journal(
        &argv,
        0,
        "",
        "Hint: You are currently not seeing messages from other users and the system.\n      \
         Users in groups 'adm', 'systemd-journal', 'wheel' can see all messages.\n      \
         Pass -q to turn off this notice.\n",
    );
    assert_eq!(
        read_log(&journal_source(), None, 4096, &run),
        Err(strings::t("log-journal-denied"))
    );
}

#[cfg(unix)]
#[test]
fn no_journal_file_opened_is_the_groups_message() {
    let argv = Arc::new(Mutex::new(Vec::new()));
    let run = journal(
        &argv,
        1,
        "",
        "No journal files were opened due to insufficient permissions.\n",
    );
    assert_eq!(
        read_log(&journal_source(), None, 4096, &run),
        Err(strings::t("log-journal-denied"))
    );
}

/// Any other failure exit: the Log window's failure message with the
/// last line journalctl wrote on stderr.
#[cfg(unix)]
#[test]
fn a_journal_read_that_fails_otherwise_says_why() {
    let argv = Arc::new(Mutex::new(Vec::new()));
    let run = journal(
        &argv,
        1,
        "srv  load_model: loading\n",
        "Warning: something\nFailed to iterate through journal: Bad message\n",
    );
    assert_eq!(
        read_log(&journal_source(), None, 4096, &run),
        Err(strings::fill(
            "log-journal-failed",
            &[&"Failed to iterate through journal: Bad message"]
        ))
    );
}

#[test]
fn a_journal_reader_that_cannot_run_says_why() {
    let run = |_: &[String]| -> std::io::Result<std::process::Output> {
        Err(std::io::Error::new(
            std::io::ErrorKind::NotFound,
            "journalctl not found",
        ))
    };
    assert_eq!(
        read_log(&journal_source(), None, 4096, &run),
        Err(strings::fill(
            "log-journal-failed",
            &[&"journalctl not found"]
        ))
    );
}

/// The happy path: one run for the unit with the bound as `-n` (every
/// entry is at least one byte, so the last `max_bytes` entries hold the
/// last `max_bytes` bytes) and the window as `--since`; stdout is the
/// log, cut to `max_bytes` from a line start like a file.
#[cfg(unix)]
#[test]
fn a_journal_read_runs_journalctl_once_for_the_unit_and_window() {
    let argv = Arc::new(Mutex::new(Vec::new()));
    let run = journal(
        &argv,
        0,
        "first entry that will be cut off\n[52043] llama_server: model loaded\n",
        "",
    );
    let since = UNIX_EPOCH + Span::from_secs(1_795_000_000);
    let got = read_log(&journal_source(), Some(since), 40, &run);
    assert_eq!(got, Ok("[52043] llama_server: model loaded\n".to_string()));
    assert_eq!(
        *argv.lock().unwrap(),
        [journalctl_once_args(UNIT, 40, Some(since))]
    );
}

/// No test reads the real journal: the runner of a test build runs
/// nothing and fails.
#[test]
fn the_journal_runner_of_a_test_build_runs_nothing() {
    let args = journalctl_once_args(&instance_unit(), 10, None);
    if let Ok(out) = journal_runner()(&args) {
        panic!("a test build ran journalctl: {out:?}");
    }
}

/// The manager's `log_text` on Linux reads the unit's journal through
/// `journal_runner()`: in a test build that is the failure message, with
/// no call to the manager.
#[cfg(target_os = "linux")]
#[test]
fn the_managers_log_text_reads_the_journal_without_a_manager_call() {
    let log = fake::log();
    let b = fake::backend(FakeClient::new(&log, InstanceState::Active));
    match b.log_text(None, 4096) {
        Err(e) => assert!(
            e.starts_with(&strings::fill("log-journal-failed", &[&""])),
            "{e:?}"
        ),
        Ok(t) => panic!("a test build read a journal: {t:?}"),
    }
    assert!(log.lock().unwrap().is_empty(), "no manager call");
}
