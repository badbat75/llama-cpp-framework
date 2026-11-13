//! pai-config: GUI + CLI configurator for the Private AI Framework.

//   pai-config                  → GUI
//   pai-config <subcommand> ... → headless CLI (clap-defined in cli.rs)

#![cfg_attr(
    all(not(debug_assertions), target_os = "windows"),
    windows_subsystem = "windows"
)]

use clap::Parser;
use pai_config::cli::reader_of;
use pai_config::{cli, gui, migration, strings};

fn main() {
    // Attach to the parent console (if any) BEFORE branching: the release
    // binary is windows_subsystem = "windows", so without this a GUI-launch
    // failure's eprintln! would be silently discarded when run from a
    // terminal. No-op when launched from Explorer.
    #[cfg(all(not(debug_assertions), target_os = "windows"))]
    unsafe {
        attach_parent_console();
    }

    // The trees and login entry of release 2.4 move before anything reads
    // or writes them; the failures print once the strings reader is set.
    let migrated = migration::run();
    let argv: Vec<String> = std::env::args().collect();
    if argv.len() <= 1 {
        strings::set_reader(reader_of(None));
        for line in migrated.errors() {
            eprintln!("{line}");
        }
        if let Err(e) = gui::run(false) {
            eprintln!(
                "{}",
                strings::fill("gui-launch-error", &[&format!("{e:#}")])
            );
            std::process::exit(1);
        }
        return;
    }

    let cli = cli::Cli::parse();
    strings::set_reader(reader_of(Some(&cli.command)));
    for line in migrated.errors() {
        eprintln!("{line}");
    }
    if let Err(e) = cli::run(cli) {
        eprintln!("{}", strings::fill("error-line", &[&format!("{e:#}")]));
        std::process::exit(1);
    }
}

#[cfg(all(not(debug_assertions), target_os = "windows"))]
unsafe fn attach_parent_console() {
    use windows_sys::Win32::System::Console::{AttachConsole, ATTACH_PARENT_PROCESS};
    AttachConsole(ATTACH_PARENT_PROCESS);
}
