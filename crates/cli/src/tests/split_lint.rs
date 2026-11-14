//! `pai` carries the command line the configurator had, as its own console
//! executable: checked on the sources, by path (a binary root no test can
//! call). pai-config's side is `crates/config/src/tests/cli_split_lint.rs`.

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

/// Every `.rs` under `dir`.
fn rust_files(dir: &Path, out: &mut Vec<PathBuf>) {
    for entry in std::fs::read_dir(dir).expect("src dir") {
        let path = entry.expect("dir entry").path();
        if path.is_dir() {
            rust_files(&path, out);
        } else if path.extension().is_some_and(|e| e == "rs") {
            out.push(path);
        }
    }
}

/// `pai` is a console executable: no `windows_subsystem`, so a terminal gets
/// its output with no console attach (which the GUI binary no longer has
/// either).
#[test]
fn pai_is_a_console_executable() {
    let main = code(&read("cli/src/main.rs"));
    assert!(
        main.contains("fn main("),
        "crates/cli/src/main.rs has a main"
    );
    assert!(
        !main.contains("windows_subsystem"),
        "pai is a console executable: no windows_subsystem"
    );
    assert!(!main.contains("AttachConsole"), "no console attach in pai");
}

/// `pai` moves the 2.4 trees and login entry before anything reads them, as
/// the GUI does, and reads the CLI's strings file (`cli.toml`) whatever the
/// command.
#[test]
fn pai_migrates_before_it_parses_and_reads_the_cli_strings() {
    let main = code(&read("cli/src/main.rs"));
    let migrate = main
        .find("migration::run()")
        .expect("crates/cli/src/main.rs runs migration::run()");
    let parse = main
        .find("parse(")
        .expect("crates/cli/src/main.rs parses the command line");
    assert!(migrate < parse, "migrate before the command line is parsed");
    assert!(
        main.contains("Reader::Cli"),
        "pai reads strings/cli.toml: strings::set_reader(Reader::Cli)"
    );
    assert!(
        !main.contains("Reader::Gui"),
        "pai never reads the GUI's strings"
    );
}

/// The MSI's graceful stop runs `control stop-and-close`, which on Windows
/// also closes a running GUI through a named event. The event's name is
/// spelled in one source only, so the side that waits (the GUI) and the side
/// that signals (`pai`) cannot drift apart.
#[test]
fn stop_and_close_still_closes_the_gui() {
    let cli = code(&read("cli/src/cli.rs"));
    let arm = cli
        .find("ControlCmd::StopAndClose =>")
        .map(|at| &cli[at..])
        .expect("cli.rs dispatches ControlCmd::StopAndClose");
    let arm = &arm[..arm.find("\n        }\n").unwrap_or(arm.len())];
    assert!(
        arm.contains("signal_close()"),
        "the StopAndClose arm signals the GUI to close: {arm}"
    );

    let name = concat!("org.privateai.Config", ".close");
    let mut files = Vec::new();
    for c in ["app", "cli", "config"] {
        rust_files(&crates().join(c).join("src"), &mut files);
    }
    let spelling: Vec<_> = files
        .iter()
        .filter(|f| code(&std::fs::read_to_string(f).expect("read .rs")).contains(name))
        .collect();
    assert_eq!(spelling.len(), 1, "{name} spelled in: {spelling:?}");
}

/// A command a message tells the user to run is a `pai` command now: no text
/// of either strings file names `pai-config <command>`.
#[test]
fn the_strings_name_pai_for_commands() {
    let mut hits = Vec::new();
    for file in ["cli.toml", "en_US.toml"] {
        let text = read(&format!("core/strings/{file}"));
        for (n, line) in text.lines().enumerate() {
            for cmd in ["server", "preset", "control", "bench", "config"] {
                if line.contains(&format!("pai-config {cmd}")) {
                    hits.push(format!("{file}:{}: {line}", n + 1));
                }
            }
        }
    }
    assert!(
        hits.is_empty(),
        "say `pai <command>`:\n  {}",
        hits.join("\n  ")
    );
}
