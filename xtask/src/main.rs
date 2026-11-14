//! The build orchestrator: `cargo run -p xtask -- <phase>`, run from the
//! repo root (the scripts in scripts/ are optional wrappers over the same
//! commands). The phases:
//!
//! - `prereq`: the GPU SDK dists and sccache (`scripts/00-install-prerequisites.sh`
//!   installs the system packages and Rust first, natively, since this crate
//!   cannot build before they exist). On Windows
//!   scripts\00-install-prerequisites.ps1 does it all natively
//! - `configure`: detects paths and tools, writes the build config under
//!   `build/<os>/`
//! - `build`: llama.cpp (CMake + Ninja) and the framework (cargo), two
//!   separate products: `build llama`, `build framework`, or both
//! - `package`: the `.deb` and the `.rpm` (Linux, of one product or both),
//!   an MSI of each (Windows), the `.pkg` (macOS)
//! - `install`: installs the newest package on this machine
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
#[cfg(test)]
mod names_lint;
#[cfg(windows)]
mod windows;

use clap::{Parser, Subcommand, ValueEnum};
use common::ui;
use std::path::PathBuf;

#[derive(Parser)]
#[command(name = "xtask", about = "Private AI Framework build orchestrator")]
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
        /// PRIVATE_AI_DIST_ROOT, else the recorded one, else dist-sources.toml.
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
    /// Build llama.cpp and/or the framework (default: both, in that order).
    Build {
        #[arg(value_enum)]
        module: Option<Module>,
    },
    /// Package the build in ./dist/: a .deb and an .rpm on Linux, an MSI per
    /// product on Windows, a .pkg on macOS.
    Package {
        /// The product to package (Linux and Windows; default: both). The
        /// packages of the other one in ./dist/ are left as they are.
        #[arg(value_enum)]
        product: Option<Product>,
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
    /// Install the newest package in ./dist/, on Linux and macOS the newest of
    /// each of the split packages (install, upgrade, same-version reinstall or
    /// downgrade alike).
    Install {
        /// Install the .deb (default: by the host's distribution).
        #[arg(long, conflicts_with = "rpm")]
        deb: bool,
        /// Install the .rpm (default: by the host's distribution).
        #[arg(long)]
        rpm: bool,
        /// Install even while a program of the packages runs (Linux: one that
        /// holds a library of /usr/lib/private-ai; macOS: one from
        /// /opt/llama.cpp/bin or /opt/private-ai/bin; on Windows the MSI
        /// stops them itself).
        #[arg(long)]
        force: bool,
        /// Print the command instead of running it.
        #[arg(long)]
        dry_run: bool,
        /// Install these files instead of the newest in ./dist/: on Linux and
        /// macOS, leave out the runtime of a GPU this machine does not have.
        files: Vec<PathBuf>,
    },
}

/// The two products, built and packaged apart (`xtask build|package
/// [llama|framework]`): the engine with its runtimes, and the framework.
#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum)]
pub enum Product {
    /// llama.cpp: CMake configure + Ninja build; the engine's package and
    /// its runtimes
    Llama,
    /// pai-config, pai, pai-manager, pai-bff and the web UI: cargo build
    /// --release; the framework's packages
    Framework,
}

/// What `build` takes: one of the two products.
type Module = Product;

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
        Phase::Configure {
            llama_cpp_dir,
            dist_root,
        } => linux::configure::run(llama_cpp_dir, dist_root),
        Phase::Build { module } => {
            if module != Some(Module::Framework) {
                linux::build_llama::run()?;
            }
            if module != Some(Module::Llama) {
                linux::build_config()?;
            }
            Ok(())
        }
        Phase::Package {
            product,
            deb,
            rpm,
            keep_staging,
            wix: _,
        } => {
            let both = !deb && !rpm;
            linux::package::run(product, deb || both, rpm || both, keep_staging)
        }
        Phase::Install {
            deb,
            rpm,
            force,
            dry_run,
            files,
        } => {
            let format = if deb {
                Some("deb")
            } else if rpm {
                Some("rpm")
            } else {
                None
            };
            linux::install::run(format, force, dry_run, files)
        }
    }
}

#[cfg(windows)]
fn run(cli: Cli) -> anyhow::Result<()> {
    match cli.phase {
        Phase::Prereq { .. } => {
            anyhow::bail!("on Windows the prerequisites are scripts\\00-install-prerequisites.ps1's, run it directly")
        }
        Phase::Configure {
            llama_cpp_dir,
            dist_root,
        } => windows::configure::run(llama_cpp_dir, dist_root),
        Phase::Build { module } => {
            if module != Some(Module::Framework) {
                windows::build_llama::run()?;
            }
            if module != Some(Module::Llama) {
                windows::build_config()?;
            }
            Ok(())
        }
        Phase::Package {
            product,
            deb,
            rpm,
            keep_staging,
            wix,
        } => {
            if deb || rpm {
                anyhow::bail!(
                    "--deb and --rpm are Linux packages; on Windows the package is the MSI"
                );
            }
            windows::package::run(product, keep_staging, wix)
        }
        Phase::Install {
            deb,
            rpm,
            force: _,
            dry_run,
            files,
        } => {
            if deb || rpm {
                anyhow::bail!(
                    "--deb and --rpm are Linux packages; on Windows the package is the MSI"
                );
            }
            windows::install::run(dry_run, files)
        }
    }
}

#[cfg(target_os = "macos")]
fn run(cli: Cli) -> anyhow::Result<()> {
    match cli.phase {
        Phase::Prereq { dist_root, skip } => macos::prereq::run(dist_root, &skip),
        Phase::Configure {
            llama_cpp_dir,
            dist_root,
        } => macos::configure::run(llama_cpp_dir, dist_root),
        Phase::Build { module } => {
            if module != Some(Module::Framework) {
                macos::build_llama::run()?;
            }
            if module != Some(Module::Llama) {
                macos::build_config()?;
            }
            Ok(())
        }
        Phase::Package {
            product,
            deb,
            rpm,
            keep_staging,
            wix,
        } => {
            if deb || rpm || wix.is_some() {
                anyhow::bail!("--deb, --rpm and --wix are other systems' packages; on macOS the packages are .pkg files");
            }
            macos::package::run(product, keep_staging)
        }
        Phase::Install {
            deb,
            rpm,
            force,
            dry_run,
            files,
        } => {
            if deb || rpm {
                anyhow::bail!(
                    "--deb and --rpm are Linux packages; on macOS the packages are .pkg files"
                );
            }
            macos::install::run(force, dry_run, files)
        }
    }
}

#[cfg(test)]
mod cli_tests {
    use super::{Cli, Phase};
    use clap::{Parser, ValueEnum};

    fn parse(args: &[&str]) -> Result<Cli, clap::Error> {
        Cli::try_parse_from(std::iter::once("xtask").chain(args.iter().copied()))
    }

    /// `xtask build` names the two products: `llama` and `framework`.
    #[test]
    fn build_takes_the_engine_or_the_framework() {
        for name in ["llama", "framework"] {
            let cli = parse(&["build", name]).unwrap_or_else(|e| panic!("build {name}: {e}"));
            match cli.phase {
                Phase::Build { module: Some(m) } => assert_eq!(
                    m.to_possible_value().unwrap().get_name(),
                    name,
                    "build {name}"
                ),
                _ => panic!("build {name} is not a build of one product"),
            }
        }
    }

    /// With no product, `xtask build` builds both (the run builds llama.cpp
    /// first).
    #[test]
    fn build_with_no_product_builds_both() {
        assert!(matches!(
            parse(&["build"]).unwrap().phase,
            Phase::Build { module: None }
        ));
    }

    /// The module was `config` before the products had names; a pre-release
    /// keeps no alias.
    #[test]
    fn build_no_longer_takes_config() {
        assert!(parse(&["build", "config"]).is_err());
        assert!(parse(&["build", "nonsense"]).is_err());
    }

    /// `xtask package` takes the same two products, alone or with the format
    /// and staging flags.
    #[test]
    fn package_takes_the_engine_or_the_framework() {
        for args in [
            &["package", "llama"][..],
            &["package", "framework"],
            &["package", "framework", "--deb"],
            &["package", "llama", "--rpm", "--keep-staging"],
        ] {
            let cli = parse(args).unwrap_or_else(|e| panic!("{args:?}: {e}"));
            assert!(
                matches!(cli.phase, Phase::Package { .. }),
                "{args:?} is not a package run"
            );
        }
    }

    /// With no product, `xtask package` packages both, as before.
    #[test]
    fn package_with_no_product_packages_both() {
        assert!(matches!(
            parse(&["package"]).unwrap().phase,
            Phase::Package { .. }
        ));
    }

    /// Anything but the two products is refused, `config` included.
    #[test]
    fn package_refuses_an_unknown_product() {
        assert!(parse(&["package", "config"]).is_err());
        assert!(parse(&["package", "nonsense"]).is_err());
        assert!(parse(&["package", "llama", "framework"]).is_err());
    }

    /// The products' names as the command line spells them.
    #[test]
    fn the_products_are_named_llama_and_framework() {
        let names: Vec<String> = super::Module::value_variants()
            .iter()
            .map(|m| m.to_possible_value().unwrap().get_name().to_string())
            .collect();
        assert_eq!(names, ["llama", "framework"]);
    }

    /// `install --force` names what it overrides on each OS: on macOS a
    /// program of either tree, the engine's `bin/` (its llama-server) and
    /// the framework's.
    #[cfg(unix)]
    #[test]
    fn install_force_names_both_macos_trees() {
        use crate::common::macos_pkg::{ENGINE_DIR, FRAMEWORK_DIR};
        use clap::CommandFactory;
        let cmd = Cli::command();
        let install = cmd.find_subcommand("install").expect("install");
        let force = install
            .get_arguments()
            .find(|a| a.get_id() == "force")
            .expect("--force");
        let help = force.get_help().expect("--force has help").to_string();
        for tree in [ENGINE_DIR, FRAMEWORK_DIR] {
            assert!(
                help.contains(&format!("{tree}/bin")),
                "--force does not name {tree}/bin: {help}"
            );
        }
    }
}
