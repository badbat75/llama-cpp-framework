//! The one-time migration from the names of release 2.4 and earlier, run by
//! `main` before anything reads or writes the user's tree: the old trees moved
//! by `pai_core::migrate`, then the old login entry replaced by the new one
//! (`startup::migrate_old_entry`). Nothing here prints: `main` reports each
//! failure through a strings key.

use std::io;

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

#[cfg(test)]
mod tests {
    // `main` (a binary root no test can call) runs the migration before the
    // GUI or the CLI can read or write the user's tree: a text check of the
    // order of the calls in `main.rs`.
    #[test]
    fn main_migrates_before_the_gui_or_the_cli_runs() {
        let main = include_str!("main.rs");
        let at = |needle: &str| {
            main.find(needle)
                .unwrap_or_else(|| panic!("main.rs has no {needle}"))
        };
        let migrate = at("migration::run()");
        assert!(migrate < at("gui::run("), "migrate before the GUI");
        assert!(migrate < at("Cli::parse()"), "migrate before the CLI");
    }
}
