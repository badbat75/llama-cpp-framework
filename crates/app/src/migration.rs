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

/// The per-user files in the order they are handed over, each with two
/// names: the one an earlier install wrote it under in the per-user directory
/// (`pai_core::migrate` moves the tree and never renames a file, so the
/// hand-over reads THOSE names), and the one the manager knows the same
/// content under, which is where the bytes go. The server file first, since
/// the manager checks models.ini against the instances' `ModelsMax`.
const USER_FILES: [(ConfigFile, &str, &str); 3] = [
    (ConfigFile::Server, "server.ini", "server@default.ini"),
    (ConfigFile::Models, "presets.ini", "models.ini"),
    (
        ConfigFile::ModelsDisabled,
        "presets.ini.disabled",
        "models.ini.disabled",
    ),
];

/// Whether to offer handing the per-user configuration in `user_dir` to the
/// manager: on the manager only, with a per-user server.ini, every manager
/// read succeeding and at least one per-user file the manager does not have.
pub fn import_offered(b: &Backend, user_dir: &Path) -> bool {
    if matches!(b, Backend::Direct) || !user_dir.join("server.ini").exists() {
        return false;
    }
    let mut missing = false;
    for (file, name, _) in USER_FILES {
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
    for (file, name, _) in USER_FILES {
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
    // The new entry is written fresh, so it carries the current comment: the
    // product speaks of model profiles.
    let new_text = std::fs::read_to_string(&new_entry).expect("new entry");
    assert!(
        new_text.contains("Comment=Configure and run llama-server and its model profiles"),
        "{new_text}"
    );
    assert!(!new_text.contains("model presets"), "{new_text}");
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

/// E2E phase (an isolated test of `tests/mod.rs`): what
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
        written: vec![ConfigFile::Server, ConfigFile::Models],
        refused: None,
    };
    let lines = all.lines(&b);
    assert_eq!(lines.len(), 2, "{lines:?}");
    assert!(lines[0].contains(&path(ConfigFile::Server)), "{lines:?}");
    assert!(lines[1].contains(&path(ConfigFile::Models)), "{lines:?}");
    assert_ne!(lines[0], lines[1]);

    // Refused: the written lines, then one naming the file and carrying the
    // refusal's own text.
    let text = ClientError::NotAuthorized.text();
    let stopped = Import {
        written: vec![ConfigFile::Server],
        refused: Some((ConfigFile::Models, text.clone())),
    };
    let lines = stopped.lines(&b);
    assert_eq!(lines.len(), 2, "{lines:?}");
    assert!(lines[0].contains(&path(ConfigFile::Server)), "{lines:?}");
    assert!(
        lines[1].contains(&path(ConfigFile::Models)) && lines[1].contains(&text),
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
#[path = "migration_tests.rs"]
mod import_tests;
