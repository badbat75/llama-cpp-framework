//! The one-time migration from the names of release 2.4 and earlier, run by
//! `main` before anything reads or writes the user's tree: the old trees moved
//! by `pai_core::migrate`, then the old login entry replaced by the new one
//! (`startup::migrate_old_entry`). Nothing here prints: `main` reports each
//! failure through a strings key.

use crate::server_control::{Backend, ConfigFile};
use std::io;
use std::path::Path;

/// What the migration did.
#[derive(Debug)]
pub struct Migration {
    /// Every old tree, its new place and what happened to it.
    pub trees: Vec<pai_core::migrate::TreeMove>,
    /// `Ok(true)` when an old login entry was found and replaced.
    pub login_entry: io::Result<bool>,
}

/// Run the migration (see the module header).
pub fn run() -> Migration {
    Migration {
        trees: pai_core::migrate::migrate_old_trees(),
        login_entry: crate::startup::migrate_old_entry(),
    }
}

impl Migration {
    /// Every failure, each as one line through the strings file, for `main`
    /// to print once the strings reader is set.
    pub fn errors(&self) -> Vec<String> {
        let mut lines: Vec<String> = self
            .trees
            .iter()
            .filter_map(|t| {
                let e = t.result.as_ref().err()?;
                Some(crate::strings::fill(
                    "migration-tree-failed",
                    &[
                        &t.old.display().to_string(),
                        &t.new.display().to_string(),
                        &e.to_string(),
                    ],
                ))
            })
            .collect();
        if let Err(e) = &self.login_entry {
            lines.push(crate::strings::fill(
                "migration-login-failed",
                &[&e.to_string()],
            ));
        }
        lines
    }
}

/// What handing the per-user configuration to the manager did.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Import {
    /// The files written, in the order they were written.
    pub written: Vec<ConfigFile>,
    /// The file the hand-over stopped at and the text to show: the manager's
    /// refusal, or why the file could not be read.
    pub refused: Option<(ConfigFile, String)>,
}

/// The per-user files in the order they are handed over, each with its
/// name in the per-user directory: the server file first, since the manager
/// checks presets.ini against the instances' `ModelsMax`.
const USER_FILES: [(ConfigFile, &str); 3] = [
    (ConfigFile::Server, "server.ini"),
    (ConfigFile::Presets, "presets.ini"),
    (ConfigFile::PresetsDisabled, "presets.ini.disabled"),
];

/// Whether to offer handing the per-user configuration in `user_dir` to the
/// manager: on the manager only, with a per-user server.ini, every manager
/// read succeeding and at least one per-user file the manager does not have.
pub fn import_offered(b: &Backend, user_dir: &Path) -> bool {
    if matches!(b, Backend::Direct) || !user_dir.join("server.ini").exists() {
        return false;
    }
    let mut missing = false;
    for (file, name) in USER_FILES {
        if !user_dir.join(name).exists() {
            continue;
        }
        match b.read_config(file) {
            Ok(None) => missing = true,
            Ok(Some(_)) => {}
            Err(_) => return false,
        }
    }
    missing
}

/// Hand the per-user configuration in `user_dir` to the manager: each file
/// the user has and the manager does not, with the user's bytes, stopping at
/// the first refusal. The per-user files are left as they are.
pub fn import_user_config(b: &Backend, user_dir: &Path) -> Import {
    let mut done = Import::default();
    if matches!(b, Backend::Direct) {
        return done;
    }
    for (file, name) in USER_FILES {
        let text = match std::fs::read_to_string(user_dir.join(name)) {
            Ok(text) => text,
            Err(e) if e.kind() == io::ErrorKind::NotFound => continue,
            Err(e) => {
                done.refused = Some((file, e.to_string()));
                break;
            }
        };
        let result = match b.read_config(file) {
            Ok(Some(_)) => continue,
            Ok(None) => b.write_config(file, &text),
            Err(e) => Err(e),
        };
        match result {
            Ok(()) => done.written.push(file),
            Err(e) => {
                done.refused = Some((file, e));
                break;
            }
        }
    }
    done
}

impl Import {
    /// What the hand-over did, one line each, through the strings file.
    pub fn lines(&self, b: &Backend) -> Vec<String> {
        let path = |f: ConfigFile| b.config_path(f).display().to_string();
        let mut lines: Vec<String> = self
            .written
            .iter()
            .map(|&f| crate::strings::fill("import-wrote", &[&path(f)]))
            .collect();
        if let Some((f, text)) = &self.refused {
            lines.push(crate::strings::fill("import-refused", &[&path(*f), text]));
        }
        if lines.is_empty() {
            lines.push(crate::strings::t("import-nothing"));
        }
        lines
    }
}

/// The body of the dialog offering the hand-over, naming the system directory.
pub fn import_offer_body(b: &Backend) -> String {
    let path = b.config_path(ConfigFile::Server);
    let dir = path.parent().unwrap_or(&path).display().to_string();
    crate::strings::fill("import-offer-body", &[&dir])
}

/// E2E phase for the whole migration under the `PRIVATE_AI_DATA_ROOT`
/// redirect, for the shared UI test to call: the redirect is the whole home,
/// so the 2.4 tree and login entry are built inside it. Linux only: the
/// Windows login entry is the real HKCU Run key, which no test writes.
#[cfg(all(test, target_os = "linux"))]
pub(crate) fn migration_e2e() {
    use crate::paths;
    use pai_core::migrate::{Outcome, OLD_LOGIN_ENTRY};
    let dir = tempfile::tempdir().expect("tempdir");
    paths::redirect_for_tests(dir.path());
    let autostart = dir.path().join(".config").join("autostart");
    let old_entry = autostart.join(format!("{OLD_LOGIN_ENTRY}.desktop"));
    let new_entry = autostart.join("org.privateai.Config.desktop");

    // Nothing old: nothing moves, and no login entry is created.
    let m = run();
    assert!(
        m.trees
            .iter()
            .all(|t| matches!(t.result, Ok(Outcome::NoOldTree))),
        "{:?}",
        m.trees
    );
    assert!(
        !m.login_entry.expect("no error"),
        "no old entry, nothing done"
    );
    assert!(!autostart.exists(), "no login entry created");

    // A 2.4 install: its config and its login entry, set to start minimized.
    let old_config = dir.path().join(".config").join(concat!("llama", ".cpp"));
    std::fs::create_dir_all(&old_config).expect("old config");
    std::fs::write(old_config.join("server.ini"), "[Server]\nPort = 8080\n").expect("w");
    std::fs::create_dir_all(&autostart).expect("autostart");
    std::fs::write(
        &old_entry,
        format!(
            "[Desktop Entry]\nType=Application\nName={}\n\
             Comment=Configure and run llama-server and its model presets\n\
             Exec=\"/usr/bin/{OLD_LOGIN_ENTRY}\" gui --minimized\nIcon={OLD_LOGIN_ENTRY}\n\
             Terminal=false\nX-GNOME-Autostart-enabled=true\n",
            concat!("llama.cpp", " Config")
        ),
    )
    .expect("old entry");

    let m = run();
    assert!(
        m.login_entry.expect("no error"),
        "the old entry was replaced"
    );
    assert!(!old_entry.exists(), "the old entry is removed");
    assert!(new_entry.is_file(), "the new entry is written");
    assert!(
        crate::startup::is_enabled() && crate::startup::starts_minimized(),
        "the setting survives, minimized included"
    );
    assert_eq!(
        std::fs::read_to_string(paths::server_ini()).expect("moved config"),
        "[Server]\nPort = 8080\n"
    );
    assert!(!old_config.exists());

    // Idempotent: a second run changes nothing.
    let m = run();
    assert!(!m.login_entry.expect("no error"));
    assert!(new_entry.is_file() && !old_entry.exists());
}

/// E2E phase (called from `data_root_e2e` in `tests/mod.rs`): what
/// the hand-over reports, named with the system paths under the redirect,
/// and the offer's body naming the system directory.
#[cfg(test)]
pub(crate) fn config_import_e2e() {
    use crate::server_control::fake::{self, FakeClient};
    use pai_core::client::{ClientError, InstanceState};
    let dir = tempfile::tempdir().expect("tempdir");
    crate::paths::redirect_for_tests(dir.path());
    let log = fake::log();
    let b = fake::backend(FakeClient::new(&log, InstanceState::Inactive));
    let path = |f: ConfigFile| b.config_path(f).display().to_string();

    // The body names the system directory the files go to.
    let system_dir = b
        .config_path(ConfigFile::Server)
        .parent()
        .expect("system dir")
        .display()
        .to_string();
    let body = import_offer_body(&b);
    assert!(body.contains(&system_dir), "{body:?} names {system_dir}");

    // Written: one line per file, naming where it went, in order.
    let all = Import {
        written: vec![ConfigFile::Server, ConfigFile::Presets],
        refused: None,
    };
    let lines = all.lines(&b);
    assert_eq!(lines.len(), 2, "{lines:?}");
    assert!(lines[0].contains(&path(ConfigFile::Server)), "{lines:?}");
    assert!(lines[1].contains(&path(ConfigFile::Presets)), "{lines:?}");
    assert_ne!(lines[0], lines[1]);

    // Refused: the written lines, then one naming the file and carrying the
    // refusal's own text.
    let text = ClientError::NotAuthorized.text();
    let stopped = Import {
        written: vec![ConfigFile::Server],
        refused: Some((ConfigFile::Presets, text.clone())),
    };
    let lines = stopped.lines(&b);
    assert_eq!(lines.len(), 2, "{lines:?}");
    assert!(lines[0].contains(&path(ConfigFile::Server)), "{lines:?}");
    assert!(
        lines[1].contains(&path(ConfigFile::Presets)) && lines[1].contains(&text),
        "{lines:?}"
    );

    // Nothing: one line saying so, which is not a key left unfilled.
    let lines = Import::default().lines(&b);
    assert_eq!(lines.len(), 1, "{lines:?}");
    assert!(
        !lines[0].is_empty() && lines[0].contains(' '),
        "a sentence, not a bare key: {lines:?}"
    );
}

/// Handing the per-user configuration to the manager: offered, never done
/// on its own, since a write through the manager asks for an administrator's
/// password. The per-user directory is a tempdir of each test.
#[cfg(test)]
mod import_tests {
    use super::*;
    use crate::server_control::fake::{self, FakeClient};
    use pai_core::client::{ClientError, InstanceState};

    // The user's files as the 2.4 configurator wrote them: comments, CRLF,
    // a trailing blank line, so a rewrite of any byte shows.
    const SERVER: &str = "; written by the configurator\r\n[Server]\r\nPort = 8080\r\n\
                          ModelsMax = 6\r\nModelsDir = /mnt/data/models\r\n\r\n";
    const PRESETS: &str = "[Qwen3.8-27B]\r\nmodel = /mnt/data/models/Qwen3.8-27B-Q8_0.gguf\r\n\
                           load-on-startup = true\r\n";
    const DISABLED: &str = "[Qwen3.8-Flash-Next]\r\n\
                            model = /mnt/data/models/Qwen3.8-Flash-Next-IQ3_S.gguf\r\n";

    const SERVER_SYS: &str = "server@default.ini";
    const PRESETS_SYS: &str = "presets.ini";
    const DISABLED_SYS: &str = "presets.ini.disabled";

    /// A per-user config directory holding the files named.
    fn user_dir(files: &[(&str, &str)]) -> tempfile::TempDir {
        let dir = tempfile::tempdir().expect("tempdir");
        for (name, text) in files {
            std::fs::write(dir.path().join(name), text).expect("user file");
        }
        dir
    }

    fn all_three() -> tempfile::TempDir {
        user_dir(&[
            ("server.ini", SERVER),
            ("presets.ini", PRESETS),
            ("presets.ini.disabled", DISABLED),
        ])
    }

    /// Every file of `dir` with its bytes, sorted by name.
    fn snapshot(dir: &Path) -> Vec<(String, Vec<u8>)> {
        let mut v: Vec<_> = std::fs::read_dir(dir)
            .expect("read dir")
            .map(|e| {
                let e = e.expect("entry");
                let bytes = std::fs::read(e.path()).unwrap_or_default();
                (e.file_name().to_string_lossy().into_owned(), bytes)
            })
            .collect();
        v.sort();
        v
    }

    fn refusal() -> pai_core::checks::Refusal {
        pai_core::checks::Refusal {
            key: "not-whole-number-in-range",
            values: ["Port", "99999", "1", "65535"].map(String::from).to_vec(),
        }
    }

    // ── When it is offered ──────────────────────────────────────────────

    #[test]
    fn import_is_offered_when_the_user_has_server_ini_and_the_manager_none() {
        let dir = all_three();
        let log = fake::log();
        let b = fake::backend(FakeClient::new(&log, InstanceState::Inactive));
        assert!(import_offered(&b, dir.path()));
        assert!(
            fake::actions(&log).is_empty(),
            "deciding writes nothing: {:?}",
            fake::actions(&log)
        );
    }

    #[test]
    fn import_is_offered_with_server_ini_alone() {
        let dir = user_dir(&[("server.ini", SERVER)]);
        let log = fake::log();
        let b = fake::backend(FakeClient::new(&log, InstanceState::Inactive));
        assert!(import_offered(&b, dir.path()));
    }

    /// Direct (macOS, Windows until its manager) reads the per-user files
    /// themselves: there is nothing to hand over, and nothing is written.
    #[test]
    fn import_is_never_offered_nor_done_on_direct() {
        let dir = all_three();
        let before = snapshot(dir.path());
        let b = Backend::Direct;
        assert!(!import_offered(&b, dir.path()));
        assert_eq!(import_user_config(&b, dir.path()), Import::default());
        assert_eq!(snapshot(dir.path()), before, "the user's files untouched");
    }

    /// No manager: no offer (the usual unavailable status says why).
    #[test]
    fn import_is_not_offered_when_the_manager_is_unavailable() {
        let dir = all_three();
        let log = fake::log();
        let mut c = FakeClient::new(&log, InstanceState::Inactive);
        c.fail_read = Some(ClientError::Unavailable);
        c.fail_status = Some(ClientError::Unavailable);
        assert!(!import_offered(&fake::backend(c), dir.path()));
        assert!(fake::actions(&log).is_empty(), "{:?}", fake::actions(&log));

        let no_connection =
            Backend::Manager(crate::server_control::ManagerBackend::new(Box::new(|| {
                Err(ClientError::Unavailable)
            })));
        assert!(!import_offered(&no_connection, dir.path()));
    }

    /// A refused read (or any other read error) is not "the manager has
    /// none": no offer.
    #[test]
    fn import_is_not_offered_when_the_managers_files_cannot_be_read() {
        let dir = all_three();
        let log = fake::log();
        let mut c = FakeClient::new(&log, InstanceState::Inactive);
        c.fail_read = Some(ClientError::NotAuthorized);
        assert!(!import_offered(&fake::backend(c), dir.path()));
    }

    #[test]
    fn import_is_not_offered_without_a_per_user_server_ini() {
        let dir = user_dir(&[("presets.ini", PRESETS), ("presets.ini.disabled", DISABLED)]);
        let log = fake::log();
        let b = fake::backend(FakeClient::new(&log, InstanceState::Inactive));
        assert!(!import_offered(&b, dir.path()));

        let empty = user_dir(&[]);
        let b = fake::backend(FakeClient::new(&log, InstanceState::Inactive));
        assert!(!import_offered(&b, empty.path()));
    }

    /// Once the manager holds every file the user has, the offer never
    /// comes back, with no state file of its own.
    #[test]
    fn import_is_not_offered_once_the_manager_holds_every_file_the_user_has() {
        let dir = user_dir(&[("server.ini", SERVER)]);
        let log = fake::log();
        let b = fake::backend(
            FakeClient::new(&log, InstanceState::Inactive)
                .with_file(SERVER_SYS, "[Server]\nPort = 8099\n"),
        );
        assert!(!import_offered(&b, dir.path()));

        let dir = all_three();
        let b = fake::backend(
            FakeClient::new(&log, InstanceState::Inactive)
                .with_file(SERVER_SYS, "[Server]\nPort = 8099\n")
                .with_file(PRESETS_SYS, "")
                .with_file(DISABLED_SYS, ""),
        );
        assert!(!import_offered(&b, dir.path()));
    }

    /// A hand-over that stopped after the server file (presets.ini refused)
    /// is offered again for what is left, so a refusal never strands the
    /// user's presets.
    #[test]
    fn import_is_offered_again_while_a_preset_file_is_left() {
        let dir = all_three();
        let log = fake::log();
        let b = fake::backend(
            FakeClient::new(&log, InstanceState::Inactive)
                .with_file(SERVER_SYS, "[Server]\nPort = 8099\n"),
        );
        assert!(import_offered(&b, dir.path()));
    }

    // ── What the hand-over writes ───────────────────────────────────────

    /// The server file first: the manager checks presets.ini's
    /// `load-on-startup` count against the instances' `ModelsMax` on disk,
    /// so presets.ini written before any instance would be held to
    /// llama.cpp's default and a valid config with a larger `ModelsMax`
    /// refused; the server file, checked against no presets.ini yet, passes
    /// on its own rules. The disabled presets last, checked against
    /// presets.ini. The user's bytes go over unchanged, and the user's files
    /// stay where they are, unchanged.
    #[test]
    fn import_writes_server_then_presets_then_disabled_with_the_users_bytes() {
        let dir = all_three();
        let before = snapshot(dir.path());
        let log = fake::log();
        let c = FakeClient::new(&log, InstanceState::Inactive);
        let files = c.files.clone();
        let b = fake::backend(c);
        let got = import_user_config(&b, dir.path());
        assert_eq!(
            got,
            Import {
                written: vec![
                    ConfigFile::Server,
                    ConfigFile::Presets,
                    ConfigFile::PresetsDisabled
                ],
                refused: None,
            }
        );
        assert_eq!(
            fake::writes(&log),
            [
                "write_file server@default.ini",
                "write_file presets.ini",
                "write_file presets.ini.disabled"
            ]
        );
        let files = files.lock().unwrap().clone();
        assert_eq!(files.get(SERVER_SYS).map(String::as_str), Some(SERVER));
        assert_eq!(files.get(PRESETS_SYS).map(String::as_str), Some(PRESETS));
        assert_eq!(files.get(DISABLED_SYS).map(String::as_str), Some(DISABLED));
        assert_eq!(snapshot(dir.path()), before, "the user's files untouched");
        assert!(
            fake::actions(&log)
                .iter()
                .all(|a| a.starts_with("write_file ")),
            "no start, stop or restart: {:?}",
            fake::actions(&log)
        );
    }

    #[test]
    fn import_skips_the_files_the_user_does_not_have() {
        let dir = user_dir(&[("server.ini", SERVER)]);
        let log = fake::log();
        let b = fake::backend(FakeClient::new(&log, InstanceState::Inactive));
        let got = import_user_config(&b, dir.path());
        assert_eq!(got.written, [ConfigFile::Server]);
        assert_eq!(got.refused, None);
        assert_eq!(fake::writes(&log), ["write_file server@default.ini"]);
    }

    /// A system file that exists is never replaced, whatever it holds (an
    /// empty presets.ini included).
    #[test]
    fn import_never_overwrites_a_system_file() {
        let dir = all_three();
        let log = fake::log();
        let c = FakeClient::new(&log, InstanceState::Inactive)
            .with_file(PRESETS_SYS, "")
            .with_file(SERVER_SYS, "[Server]\nPort = 8099\n");
        let files = c.files.clone();
        let got = import_user_config(&fake::backend(c), dir.path());
        assert_eq!(got.written, [ConfigFile::PresetsDisabled]);
        assert_eq!(got.refused, None);
        assert_eq!(fake::writes(&log), ["write_file presets.ini.disabled"]);
        let files = files.lock().unwrap().clone();
        assert_eq!(files.get(PRESETS_SYS).map(String::as_str), Some(""));
        assert_eq!(
            files.get(SERVER_SYS).map(String::as_str),
            Some("[Server]\nPort = 8099\n")
        );
    }

    /// A refusal (a content check, the user cancelling polkit) stops the
    /// hand-over at that file: what was written stays, nothing after it is
    /// tried, the refusal's text is reported. Run again once the cause is
    /// gone, it writes only what is left.
    #[test]
    fn a_refusal_stops_at_that_file_and_a_second_run_writes_what_is_left() {
        let order = [
            (ConfigFile::Server, SERVER_SYS),
            (ConfigFile::Presets, PRESETS_SYS),
            (ConfigFile::PresetsDisabled, DISABLED_SYS),
        ];
        for at in 0..order.len() {
            for e in [ClientError::Invalid(refusal()), ClientError::NotAuthorized] {
                let (file, name) = order[at];
                let label = format!("{name} {e:?}");
                let dir = all_three();
                let before = snapshot(dir.path());
                let log = fake::log();
                let mut c = FakeClient::new(&log, InstanceState::Inactive);
                c.fail_write_of.insert(name.to_string(), e.clone());
                let files = c.files.clone();
                let got = import_user_config(&fake::backend(c), dir.path());
                let earlier: Vec<ConfigFile> = order[..at].iter().map(|(f, _)| *f).collect();
                assert_eq!(got.written, earlier, "{label}");
                assert_eq!(got.refused, Some((file, e.text())), "{label}");
                let tried: Vec<String> = order[..=at]
                    .iter()
                    .map(|(_, n)| format!("write_file {n}"))
                    .collect();
                assert_eq!(fake::writes(&log), tried, "{label}: nothing after");
                let held: Vec<String> = files.lock().unwrap().keys().cloned().collect();
                let expected: Vec<String> = {
                    let mut v: Vec<String> =
                        order[..at].iter().map(|(_, n)| n.to_string()).collect();
                    v.sort();
                    v
                };
                assert_eq!(held, expected, "{label}: what was written stays");
                assert_eq!(snapshot(dir.path()), before, "{label}: user untouched");

                // Again, the refusal gone: only what is left.
                let log = fake::log();
                let mut c = FakeClient::new(&log, InstanceState::Inactive);
                c.files = files.clone();
                let again = import_user_config(&fake::backend(c), dir.path());
                let rest: Vec<ConfigFile> = order[at..].iter().map(|(f, _)| *f).collect();
                assert_eq!(again.written, rest, "{label}: the second run");
                assert_eq!(again.refused, None, "{label}: the second run");
                let files = files.lock().unwrap().clone();
                assert_eq!(files.get(SERVER_SYS).map(String::as_str), Some(SERVER));
                assert_eq!(files.get(PRESETS_SYS).map(String::as_str), Some(PRESETS));
                assert_eq!(files.get(DISABLED_SYS).map(String::as_str), Some(DISABLED));
            }
        }
    }

    /// No manager: stopped at the first file with the client's text,
    /// nothing written.
    #[test]
    fn import_with_no_manager_stops_at_the_first_file() {
        let dir = all_three();
        let log = fake::log();
        let mut c = FakeClient::new(&log, InstanceState::Inactive);
        c.fail_read = Some(ClientError::Unavailable);
        c.fail_write = Some(ClientError::Unavailable);
        let got = import_user_config(&fake::backend(c), dir.path());
        assert_eq!(got.written, []);
        assert_eq!(
            got.refused,
            Some((ConfigFile::Server, ClientError::Unavailable.text()))
        );
        assert_eq!(fake::writes(&log), Vec::<String>::new(), "no write tried");
    }

    /// A per-user file that cannot be read (a directory in its place) stops
    /// the hand-over there with an error; it is not skipped as missing.
    #[test]
    fn an_unreadable_user_file_stops_the_hand_over_there() {
        let dir = user_dir(&[("server.ini", SERVER), ("presets.ini.disabled", DISABLED)]);
        std::fs::create_dir(dir.path().join("presets.ini")).expect("dir in its place");
        let log = fake::log();
        let got = import_user_config(
            &fake::backend(FakeClient::new(&log, InstanceState::Inactive)),
            dir.path(),
        );
        assert_eq!(got.written, [ConfigFile::Server]);
        match &got.refused {
            Some((ConfigFile::Presets, text)) => assert!(!text.is_empty()),
            other => panic!("stopped at presets.ini with a text: {other:?}"),
        }
        assert_eq!(fake::writes(&log), ["write_file server@default.ini"]);
    }

    /// Done once, done: a second run writes nothing and the offer is gone.
    #[test]
    fn import_twice_writes_nothing_the_second_time() {
        let dir = all_three();
        let log = fake::log();
        let c = FakeClient::new(&log, InstanceState::Inactive);
        let files = c.files.clone();
        assert_eq!(
            import_user_config(&fake::backend(c), dir.path())
                .written
                .len(),
            3
        );
        let log = fake::log();
        let mut c = FakeClient::new(&log, InstanceState::Inactive);
        c.files = files.clone();
        let b = fake::backend(c);
        assert_eq!(import_user_config(&b, dir.path()), Import::default());
        assert!(!import_offered(&b, dir.path()));
        assert_eq!(fake::writes(&log), Vec::<String>::new());
    }
}
