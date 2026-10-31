//! The build orchestrator: `cargo run -p xtask -- <phase>`, run from the
//! repo root (the scripts in scripts/ are optional wrappers over the same
//! commands). The phases:
//!
//! - `prereq`     the GPU SDK dists and sccache (`scripts/00-install-prerequisites.sh`
//!                installs the system packages and Rust first, natively, since
//!                this crate cannot build before they exist). On Windows
//!                scripts\00-install-prerequisites.ps1 does it all natively
//! - `configure`  detects paths and tools, writes the build config under
//!                `build/<os>/`
//! - `build`      llama.cpp (CMake + Ninja) and llama-cpp-config (cargo), two
//!                separate modules: `build llama`, `build config`, or both
//! - `package`    the `.deb` and the `.rpm` (Linux), the MSI (Windows), the
//!                `.pkg` (macOS)
//! - `install`    installs the newest package on this machine
//!
//! `common/` is what the operating systems share (the dist layout, the
//! llama.cpp checkout and its patches, the CMake options of xtask/cmake/,
//! sccache, the configurator build, the package names); `linux/`, `windows/` and `macos/`
//! hold each one's configure line, packaging and prerequisites, and only the
//! one being built is compiled. docs/build-pipeline.md explains the decisions
//! behind each phase.

mod common;
#[cfg(target_os = "linux")]
mod linux;
#[cfg(target_os = "macos")]
mod macos;
#[cfg(windows)]
mod windows;

use common::ui;
use clap::{Parser, Subcommand, ValueEnum};
use std::path::PathBuf;

#[derive(Parser)]
#[command(name = "xtask", about = "llama.cpp-framework build orchestrator")]
struct Cli {
    #[command(subcommand)]
    phase: Phase,
}

#[derive(Subcommand)]
enum Phase {
    /// Install or update the newest stable GPU SDK dists (Linux: ROCm, CUDA
    /// Toolkit, NCCL, Vulkan SDK; macOS: Vulkan SDK) and sccache.
    /// Idempotent; a source that cannot be reached keeps what is installed.
    Prereq {
        /// Where the dists live (<root>/<Name>/<Version>/<os>). Default:
        /// LLAMA_FW_DIST_ROOT, else the recorded one, else dist-sources.toml.
        #[arg(long)]
        dist_root: Option<PathBuf>,
        /// Comma-separated legs to skip: sccache,rocm,cuda,nccl,vulkan (macOS:
        /// sccache,vulkan; apt, brew and rust are the shell script's and are
        /// ignored here).
        #[arg(long, value_delimiter = ',')]
        skip: Vec<String>,
    },
    /// Detect paths and tools and write the build config
    /// (build/<os>/config-build.toml, build\windows\config-build.psd1).
    Configure {
        /// The llama.cpp clone (default: build/<os>/llama.cpp).
        #[arg(long)]
        llama_cpp_dir: Option<PathBuf>,
        /// Where the dists live; recorded in the config.
        #[arg(long)]
        dist_root: Option<PathBuf>,
    },
    /// Build llama.cpp and/or llama-cpp-config (default: both, in that order).
    Build {
        #[arg(value_enum)]
        module: Option<Module>,
    },
    /// Package the build in ./dist/: a .deb and an .rpm on Linux, the MSI on
    /// Windows, a .pkg on macOS.
    Package {
        /// Build the .deb (Linux; default: both formats).
        #[arg(long)]
        deb: bool,
        /// Build the .rpm (Linux; default: both formats).
        #[arg(long)]
        rpm: bool,
        /// Leave build/<os>/staging/ in place for inspection.
        #[arg(long)]
        keep_staging: bool,
        /// The wix.exe to build the MSI with (Windows; default: the
        /// configured one, else PATH, else the dotnet tools directory).
        #[arg(long)]
        wix: Option<PathBuf>,
    },
    /// Install the newest package in ./dist/ (install, upgrade, same-version
    /// reinstall or downgrade alike).
    Install {
        /// Install the .deb (default: by the host's distribution).
        #[arg(long, conflicts_with = "rpm")]
        deb: bool,
        /// Install the .rpm (default: by the host's distribution).
        #[arg(long)]
        rpm: bool,
        /// Install even while a program from /opt/llama.cpp/bin runs (Linux,
        /// macOS; on Windows the MSI stops them itself).
        #[arg(long)]
        force: bool,
        /// Print the command instead of running it.
        #[arg(long)]
        dry_run: bool,
        /// Install this file instead of the newest in ./dist/.
        file: Option<PathBuf>,
    },
}

#[derive(Clone, Copy, PartialEq, Eq, ValueEnum)]
enum Module {
    /// llama.cpp: CMake configure + Ninja build
    Llama,
    /// llama-cpp-config: cargo build --release
    Config,
}

fn main() {
    let cli = Cli::parse();
    if let Err(e) = run(cli) {
        ui::err(format!("ERROR: {e:#}"));
        std::process::exit(1);
    }
}

#[cfg(target_os = "linux")]
fn run(cli: Cli) -> anyhow::Result<()> {
    match cli.phase {
        Phase::Prereq { dist_root, skip } => linux::prereq::run(dist_root, &skip),
        Phase::Configure { llama_cpp_dir, dist_root } => linux::configure::run(llama_cpp_dir, dist_root),
        Phase::Build { module } => {
            if module != Some(Module::Config) {
                linux::build_llama::run()?;
            }
            if module != Some(Module::Llama) {
                linux::build_config()?;
            }
            Ok(())
        }
        Phase::Package { deb, rpm, keep_staging, wix: _ } => {
            let both = !deb && !rpm;
            linux::package::run(deb || both, rpm || both, keep_staging)
        }
        Phase::Install { deb, rpm, force, dry_run, file } => {
            let format = if deb { Some("deb") } else if rpm { Some("rpm") } else { None };
            linux::install::run(format, force, dry_run, file)
        }
    }
}

#[cfg(windows)]
fn run(cli: Cli) -> anyhow::Result<()> {
    match cli.phase {
        Phase::Prereq { .. } => {
            anyhow::bail!("on Windows the prerequisites are scripts\\00-install-prerequisites.ps1's, run it directly")
        }
        Phase::Configure { llama_cpp_dir, dist_root } => windows::configure::run(llama_cpp_dir, dist_root),
        Phase::Build { module } => {
            if module != Some(Module::Config) {
                windows::build_llama::run()?;
            }
            if module != Some(Module::Llama) {
                windows::build_config()?;
            }
            Ok(())
        }
        Phase::Package { deb, rpm, keep_staging, wix } => {
            if deb || rpm {
                anyhow::bail!("--deb and --rpm are Linux packages; on Windows the package is the MSI");
            }
            windows::package::run(keep_staging, wix)
        }
        Phase::Install { deb, rpm, force: _, dry_run, file } => {
            if deb || rpm {
                anyhow::bail!("--deb and --rpm are Linux packages; on Windows the package is the MSI");
            }
            windows::install::run(dry_run, file)
        }
    }
}

#[cfg(target_os = "macos")]
fn run(cli: Cli) -> anyhow::Result<()> {
    match cli.phase {
        Phase::Prereq { dist_root, skip } => macos::prereq::run(dist_root, &skip),
        Phase::Configure { llama_cpp_dir, dist_root } => macos::configure::run(llama_cpp_dir, dist_root),
        Phase::Build { module } => {
            if module != Some(Module::Config) {
                macos::build_llama::run()?;
            }
            if module != Some(Module::Llama) {
                macos::build_config()?;
            }
            Ok(())
        }
        Phase::Package { deb, rpm, keep_staging, wix } => {
            if deb || rpm || wix.is_some() {
                anyhow::bail!("--deb, --rpm and --wix are other systems' packages; on macOS the package is the .pkg");
            }
            macos::package::run(keep_staging)
        }
        Phase::Install { deb, rpm, force, dry_run, file } => {
            if deb || rpm {
                anyhow::bail!("--deb and --rpm are Linux packages; on macOS the package is the .pkg");
            }
            macos::install::run(force, dry_run, file)
        }
    }
}
