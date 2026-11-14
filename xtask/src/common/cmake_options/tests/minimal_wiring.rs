//! The minimal build reaches every phase: `xtask configure --minimal`
//! records it in build/<os>/config-build.*, and build and package follow the
//! recorded flag (Windows and macOS; Linux refuses it). `windows/` and
//! `macos/` compile on their own OS only, so their wiring is checked on its
//! text; the decisions themselves are the pure functions tested beside
//! (`load` here, `msi::file_name`, `macos_pkg::of_product` and `file_name`).

use std::path::Path;

fn src(rel: &str) -> String {
    let p = Path::new(env!("CARGO_MANIFEST_DIR")).join("src").join(rel);
    std::fs::read_to_string(&p).unwrap_or_else(|e| panic!("{}: {e}", p.display()))
}

/// The code of a source file before its inline tests, so a test's own
/// literal never satisfies a pin.
fn code_of(rel: &str) -> String {
    let text = src(rel);
    let end = ["#[cfg(test)]\nmod tests {", "#[cfg(test)]\nmod cli_tests {"]
        .iter()
        .filter_map(|m| text.find(m))
        .min()
        .unwrap_or(text.len());
    text[..end].to_string()
}

/// One OS's `run()` in main.rs, up to the next `#[cfg(`.
fn main_run(cfg: &str) -> String {
    let main = code_of("main.rs");
    let head = format!("#[cfg({cfg})]\nfn run(");
    let body = main
        .split(head.as_str())
        .nth(1)
        .unwrap_or_else(|| panic!("no run() under #[cfg({cfg})] in main.rs"));
    body.split("\n#[cfg(").next().unwrap().to_string()
}

/// The `Phase::Configure` arm of a `run()`, up to the next arm.
fn configure_arm(run: &str) -> String {
    let arm = run
        .split("Phase::Configure")
        .nth(1)
        .expect("a Phase::Configure arm");
    arm.split("Phase::Build").next().unwrap().to_string()
}

/// The text of a function, from its `pub fn <name>(` to the next `\n}`.
fn function(code: &str, name: &str) -> String {
    let head = format!("pub fn {name}(");
    let body = code
        .split(head.as_str())
        .nth(1)
        .unwrap_or_else(|| panic!("no pub fn {name}"));
    body.split("\n}").next().unwrap().to_string()
}

#[test]
fn windows_and_macos_configure_hand_the_flag_on() {
    for (cfg, call) in [
        ("windows", "windows::configure::run("),
        ("target_os = \"macos\"", "macos::configure::run("),
    ] {
        let arm = configure_arm(&main_run(cfg));
        assert!(
            arm.contains("minimal") && arm.contains(call),
            "{cfg}: the Configure arm does not hand --minimal to {call}:\n{arm}"
        );
        let args = arm.split(call).nth(1).unwrap();
        let args = args.split(')').next().unwrap();
        assert!(args.contains("minimal"), "{cfg}: {call}{args})");
    }
}

/// Linux packages its GPU runtimes apart already and has no use for it: a
/// flag that is accepted and does nothing would be a lie, so it is refused.
#[test]
fn linux_configure_refuses_the_minimal_flag() {
    let arm = configure_arm(&main_run("target_os = \"linux\""));
    assert!(
        arm.contains("minimal") && arm.contains("bail!"),
        "Linux does not refuse configure --minimal:\n{arm}"
    );
}

/// configure records the flag in the config it writes: `Minimal` in
/// config-build.psd1, `minimal` in config-build.toml.
#[test]
fn configure_records_the_minimal_flag() {
    for (configure, config) in [
        ("windows/configure.rs", "windows/mod.rs"),
        ("macos/configure.rs", "macos/config.rs"),
    ] {
        let code = code_of(configure);
        let run = function(&code, "run");
        let signature = run.split('{').next().unwrap();
        assert!(
            signature.contains("minimal: bool"),
            "{configure}: run() takes no minimal flag: {signature}"
        );
        let literal = run.split("Config {").last().expect("the config it writes");
        assert!(
            literal.contains("minimal"),
            "{configure}: the config written does not record the flag"
        );
        let config_code = code_of(config);
        assert!(
            config_code.contains("pub minimal: bool"),
            "{config}: the build config has no minimal field"
        );
    }
    let win = code_of("windows/mod.rs");
    let save = function(&win, "save");
    assert!(
        save.lines()
            .any(|l| l.trim_start().starts_with("Minimal") && l.contains('=')),
        "windows/mod.rs: save() writes no Minimal entry"
    );
    assert!(
        function(&win, "load_optional").contains("get(\"Minimal\")"),
        "windows/mod.rs: load_optional() does not read Minimal back"
    );
    let mac = code_of("macos/config.rs");
    assert!(
        function(&mac, "save")
            .lines()
            .any(|l| l.starts_with("minimal = {")),
        "macos/config.rs: save() writes no minimal entry"
    );
}

/// build takes its CMake options from the recorded flag.
#[test]
fn build_follows_the_recorded_minimal_flag() {
    for build in ["windows/build_llama.rs", "macos/build_llama.rs"] {
        assert!(
            code_of(build).contains("cmake_options(cfg.minimal)"),
            "{build} does not build the recorded variant"
        );
    }
}

/// A minimal Windows build has no ROCm dist, so the floor on the ROCm
/// clang (which reads the dist's clang resource directory and refuses the
/// build when it finds none) applies only when HIP is built.
#[test]
fn a_minimal_windows_build_does_not_ask_for_the_rocm_clang() {
    let code = code_of("windows/build_llama.rs");
    let at = code
        .find("let clang_major")
        .expect("the ROCm clang floor in windows/build_llama.rs");
    let before = &code[at.saturating_sub(400)..at];
    assert!(
        before.contains("minimal") || before.contains("GGML_HIP"),
        "the ROCm clang floor is not conditioned on a HIP build:\n{before}"
    );
}
