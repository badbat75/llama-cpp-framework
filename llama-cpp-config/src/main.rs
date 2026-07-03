// llama-cpp-config — GUI + CLI configurator for llama.cpp-framework.

//   llama-cpp-config                  → GUI
//   llama-cpp-config <subcommand> ... → headless CLI (clap-defined in cli.rs)

#![cfg_attr(
    all(not(debug_assertions), target_os = "windows"),
    windows_subsystem = "windows"
)]

mod cli;
mod devices;
mod form;
mod gguf;
mod gui;
mod ini;
mod integrations;
mod model_scan;
mod net_ifaces;
mod paths;
mod presets;
mod proc;
mod runstate;
mod server_cfg;
mod server_version;
#[cfg(windows)]
mod single_instance;
// Cross-cutting end-to-end tests (e.g. the Slint UI regression test) live under
// src/tests/; per-module unit tests stay inline in their own files.
#[cfg(test)]
mod tests;

use clap::Parser;

fn main() {
    let argv: Vec<String> = std::env::args().collect();
    if argv.len() <= 1 {
        if let Err(e) = gui::run() {
            eprintln!("GUI error: {e:#}");
            std::process::exit(1);
        }
        return;
    }

    #[cfg(all(not(debug_assertions), target_os = "windows"))]
    unsafe {
        attach_parent_console();
    }

    let cli = cli::Cli::parse();
    if let Err(e) = cli::run(cli) {
        eprintln!("Error: {e:#}");
        std::process::exit(1);
    }
}

#[cfg(all(not(debug_assertions), target_os = "windows"))]
unsafe fn attach_parent_console() {
    #[link(name = "kernel32")]
    extern "system" {
        fn AttachConsole(dw_process_id: u32) -> i32;
    }
    const ATTACH_PARENT_PROCESS: u32 = u32::MAX;
    AttachConsole(ATTACH_PARENT_PROCESS);
}
