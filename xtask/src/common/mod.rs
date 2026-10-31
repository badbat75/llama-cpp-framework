//! What every operating system's phases share: output, paths and processes,
//! the dist layout, the llama.cpp checkout and cmake options, the
//! configurator build, the configure report, the ROCm GPU targets and the
//! package names.

pub mod cmake_options;
pub mod configurator;
pub mod dists;
pub mod llama;
#[cfg(unix)]
pub mod prereq;
pub mod report;
#[cfg(not(target_os = "macos"))]
pub mod rocm;
pub mod ui;
pub mod util;
pub mod version;
