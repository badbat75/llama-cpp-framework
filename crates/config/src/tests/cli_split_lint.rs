//! The command line is `pai`'s (crates/cli); pai-config opens the GUI and
//! parses only the GUI's own arguments (`launch.rs`). Checked on the files,
//! by path: `main` is a binary root no test can call.

use std::path::{Path, PathBuf};

fn crates() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("..")
}

fn read(rel: &str) -> String {
    std::fs::read_to_string(crates().join(rel)).unwrap_or_else(|e| panic!("{rel}: {e}"))
}

/// The code lines of a source, `//` comments left out.
fn code(source: &str) -> String {
    source
        .lines()
        .map(|l| l.split("//").next().unwrap_or(""))
        .collect::<Vec<_>>()
        .join("\n")
}

/// The dependency names of a manifest's `*dependencies` tables (`name = ...`
/// lines; the configurator's manifest has no `[dependencies.name]` tables).
fn dependencies(manifest: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut in_deps = false;
    for line in manifest.lines() {
        let line = line.split('#').next().unwrap_or("").trim();
        if line.starts_with('[') {
            in_deps = line.trim_end_matches(']').ends_with("dependencies");
        } else if in_deps {
            if let Some((key, _)) = line.split_once('=') {
                out.push(key.trim().to_string());
            }
        }
    }
    out
}

#[test]
fn the_cli_moved_out_of_pai_config() {
    assert!(
        !crates().join("config/src/cli.rs").exists(),
        "crates/config/src/cli.rs: still here (it is crates/cli/src/cli.rs)"
    );
    let lib = code(&read("config/src/lib.rs"));
    assert!(
        !lib.contains("mod cli;"),
        "crates/config/src/lib.rs still declares the cli module"
    );
}

/// No argument parser beyond `launch::parse`: no clap in the manifest or in
/// `main.rs`, which dispatches on `launch::parse`.
#[test]
fn pai_config_parses_only_the_gui_arguments() {
    let deps = dependencies(&read("config/Cargo.toml"));
    assert!(
        deps.contains(&"slint".to_string()),
        "the scan reads the manifest: {deps:?}"
    );
    assert!(
        !deps.contains(&"clap".to_string()),
        "crates/config/Cargo.toml still depends on clap"
    );
    let main = code(&read("config/src/main.rs"));
    for parser in ["clap", "Cli::", "cli::"] {
        assert!(!main.contains(parser), "main.rs names {parser}");
    }
    assert!(
        main.contains("launch::parse("),
        "main.rs dispatches on launch::parse"
    );
}

/// The GUI binary stays a Windows GUI executable (no console window at a
/// launch from Explorer) with no console attach: what printed to a terminal
/// is `pai`'s now.
#[test]
fn pai_config_is_a_gui_executable_with_no_console_attach() {
    let main = code(&read("config/src/main.rs"));
    assert!(
        main.contains("windows_subsystem = \"windows\""),
        "main.rs keeps windows_subsystem = \"windows\""
    );
    assert!(
        !main.contains("AttachConsole"),
        "main.rs attaches a console"
    );
    let manifest = read("config/Cargo.toml");
    assert!(
        !manifest.contains("Win32_System_Console"),
        "crates/config/Cargo.toml still asks windows-sys for the console API"
    );
}

/// The window reads the language file: `main` sets the GUI reader before
/// `gui::run` and never the CLI's (`pai` reads `cli.toml`, checked in
/// `crates/cli/src/tests/split_lint.rs`).
#[test]
fn pai_config_reads_the_language_file() {
    let main = code(&read("config/src/main.rs"));
    let reader = main
        .find("set_reader(strings::Reader::Gui)")
        .expect("main.rs sets the GUI reader");
    let gui = main.find("gui::run(").expect("main.rs runs the GUI");
    assert!(reader < gui, "the reader is set before the window opens");
    assert!(
        !main.contains("Reader::Cli"),
        "pai-config never reads cli.toml"
    );
}

#[test]
fn dependency_scan_reads_the_dependency_tables_only() {
    let deps = dependencies(
        "[package]\nname = \"x\"\n\n[dependencies]\nslint = \"1\"\n# clap = \"4\"\n\
         [target.'cfg(windows)'.dependencies]\nwindows-sys = { version = \"0.61\" }\n\
         [features]\nclap = []\n[build-dependencies]\nico = \"0.6\"\n",
    );
    assert_eq!(deps, ["slint", "windows-sys", "ico"]);
}
