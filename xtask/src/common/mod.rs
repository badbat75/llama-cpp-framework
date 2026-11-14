//! What every operating system's phases share: output, paths and processes,
//! the dist layout, the llama.cpp checkout and cmake options, the
//! configurator build, the configure report, the ROCm GPU targets and the
//! package names.

pub mod bench_share;
pub mod cmake_options;
#[cfg(unix)]
pub mod components;
pub mod configurator;
pub mod dists;
pub mod licenses;
pub mod llama;
#[cfg(unix)]
#[cfg_attr(not(target_os = "macos"), allow(dead_code))]
pub mod macos_pkg;
pub mod msi;
#[cfg(unix)]
pub mod prereq;
pub mod report;
#[cfg(not(target_os = "macos"))]
pub mod rocm;
pub mod strings_share;
pub mod ui;
pub mod util;
pub mod version;
