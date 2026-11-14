//! pai: the command line of the Private AI Framework (clap-defined in cli.rs).

use clap::Parser;
use pai_cli::{cli, migration, strings};

fn main() {
    // The trees and login entry of release 2.4 move before anything reads
    // or writes them; the failures print once the strings reader is set.
    let migrated = migration::run();
    let cli = cli::Cli::parse();
    strings::set_reader(strings::Reader::Cli);
    for line in migrated.errors() {
        eprintln!("{line}");
    }
    if let Err(e) = cli::run(cli) {
        eprintln!("{}", strings::fill("error-line", &[&format!("{e:#}")]));
        std::process::exit(1);
    }
}
