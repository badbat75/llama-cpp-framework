//! The minimal build (`xtask configure --minimal`: llama.cpp with the CPU
//! backend alone, no ROCm, CUDA or Vulkan SDK, for GitHub's Windows runner)
//! still builds both MSIs. Its llama.cpp MSI must never pass for the release:
//! its file carries `-minimal`, so the release's upload command (which names
//! `llama.cpp-v<release>-x64.msi` exactly) never takes it, while `xtask
//! install` still installs it on the runner. The framework's MSI is the same
//! build either way (nothing in it depends on llama.cpp's backends) and keeps
//! its name.

use super::two_installers::scratch;
use crate::common::msi::{file_name, files_plan, install_plan, msi_product};
use crate::Product;

const MINIMAL_ENGINE: &str = "llama.cpp-v1.0.0-minimal-x64.msi";

#[test]
fn a_minimal_engine_msi_says_so_in_its_name() {
    assert_eq!(
        file_name(Product::Llama, "2.99.1", "v1.0.0", "x64", true),
        MINIMAL_ENGINE
    );
    assert_eq!(
        file_name(Product::Llama, "2.99.1", "v1.0.0", "x64", false),
        "llama.cpp-v1.0.0-x64.msi"
    );
}

#[test]
fn the_framework_msi_keeps_its_name_in_a_minimal_build() {
    for minimal in [false, true] {
        assert_eq!(
            file_name(Product::Framework, "2.99.1", "v1.0.0", "x64", minimal),
            "private-ai-framework-v2.99.1-x64.msi",
            "minimal = {minimal}"
        );
    }
}

/// `xtask install` reads the minimal engine MSI as llama.cpp at its release:
/// the runner installs what it packaged.
#[test]
fn a_minimal_engine_msi_is_read_back_as_the_engine() {
    assert_eq!(
        msi_product(MINIMAL_ENGINE),
        Some((Product::Llama, "1.0.0".to_string()))
    );
    let n = file_name(Product::Llama, "2.99.1", "v1.0.0", "x64", true);
    assert_eq!(msi_product(&n).map(|(p, _)| p), Some(Product::Llama), "{n}");
    // Only the one marker, once: anything else after the version is still
    // not a name xtask package writes.
    for other in [
        "llama.cpp-v1.0.0-tiny-x64.msi",
        "llama.cpp-v1.0.0-minimal-minimal-x64.msi",
        "llama.cpp-v1.0.0-minimal.msi",
        "llama.cpp-minimal-v1.0.0-x64.msi",
        "private-ai-framework-v2.99.1-minimal-minimal-x64.msi",
    ] {
        assert_eq!(msi_product(other), None, "{other:?}");
    }
}

/// On the runner `dist\` holds the minimal engine MSI and the framework's:
/// `xtask install` installs both, the engine first.
#[test]
fn install_takes_a_minimal_engine_msi() {
    let d = scratch("minimal");
    let fw = d.join("private-ai-framework-v2.99.1-x64.msi");
    let engine = d.join(MINIMAL_ENGINE);
    std::fs::write(&fw, b"").unwrap();
    std::fs::write(&engine, b"").unwrap();
    let plan = install_plan(&d);
    std::fs::remove_dir_all(&d).unwrap();
    assert_eq!(plan, [engine.clone(), fw.clone()]);
    assert_eq!(
        files_plan(&[fw.clone(), engine.clone()]).unwrap(),
        [engine, fw]
    );
}

/// Given a full and a minimal engine MSI at once, `xtask install <files>`
/// refuses, as for any two files of one product.
#[test]
fn install_refuses_a_full_and_a_minimal_engine_together() {
    let err = files_plan(&[
        "dist/llama.cpp-v1.0.0-x64.msi".into(),
        format!("dist/{MINIMAL_ENGINE}").into(),
    ])
    .expect_err("two engines")
    .to_string();
    assert!(err.contains("same product"), "{err}");
}

/// The Windows package phase names the MSI by the recorded build: the flag
/// `xtask configure --minimal` wrote into config-build.psd1.
#[test]
fn the_windows_package_phase_names_a_minimal_build() {
    let package = super::read("xtask/src/windows/package.rs");
    let call = package
        .split("msi::file_name(")
        .nth(1)
        .and_then(|r| r.split(')').next())
        .expect("windows/package.rs calls msi::file_name");
    assert!(
        call.contains("cfg.minimal"),
        "msi::file_name is not given the recorded minimal flag: ({call})"
    );
}
