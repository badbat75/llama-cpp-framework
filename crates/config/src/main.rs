//! pai-config: the GUI configurator of the Private AI Framework. The command
//! line is `pai`'s (crates/cli); this binary reads only the GUI's own
//! arguments (`launch.rs`).

#![cfg_attr(
    all(not(debug_assertions), target_os = "windows"),
    windows_subsystem = "windows"
)]

use pai_config::launch::{self, Launch};
use pai_config::{gui, migration, strings};

/// `pai-config <version>`, which `xtask package` and `xtask install` read back.
const VERSION_LINE: &str = concat!(env!("CARGO_BIN_NAME"), " ", env!("CARGO_PKG_VERSION"));

fn main() {
    // The trees and login entry of release 2.4 move before anything reads
    // or writes them; the failures print once the strings reader is set.
    let migrated = migration::run();
    let args: Vec<String> = std::env::args().skip(1).collect();
    let minimized = match launch::parse(&args) {
        Launch::Gui { minimized } => minimized,
        Launch::SdkDevices => {
            #[cfg(any(target_os = "linux", windows))]
            pai_app::hwmon::sdk::print();
            return;
        }
        Launch::Version => {
            println!("{VERSION_LINE}");
            return;
        }
        Launch::Moved => {
            strings::set_reader(strings::Reader::Gui);
            eprintln!(
                "{}",
                strings::fill("command-moved-to-pai", &[&args.join(" ")])
            );
            std::process::exit(2);
        }
    };
    strings::set_reader(strings::Reader::Gui);
    for line in migrated.errors() {
        eprintln!("{line}");
    }
    if let Err(e) = gui::run(minimized) {
        eprintln!(
            "{}",
            strings::fill("gui-launch-error", &[&format!("{e:#}")])
        );
        std::process::exit(1);
    }
}
