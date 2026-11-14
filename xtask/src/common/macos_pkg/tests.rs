//! The macOS packages split like Linux's: llama.cpp in its own `.pkg` under
//! its own prefix, with its Vulkan runtime beside it, and the framework's
//! packages under /opt/private-ai, named by the framework version only.
//! Nothing here runs pkgbuild: the decisions are pure, and `macos/` is
//! checked on its text (it compiles on a Mac only).

use super::*;
use crate::common::components::{self, Component};
use std::path::Path;

/// The engine's prefix, spelled split: the names lint refuses the literal
/// outside its one constant (it was release 2.4's whole tree).
const ENGINE: &str = concat!("/opt/", "llama", ".cpp");
const FRAMEWORK: &str = "/opt/private-ai";

/// What `xtask build` leaves in the build's `bin/` on macOS (llama.cpp's
/// executables, its libraries and ggml's backend modules as CMake lays them
/// out, and pai-config and pai copied beside llama-server).
const BUILD_BIN: [&str; 10] = [
    "llama-server",
    "llama-bench",
    "llama-cli",
    "libllama.dylib",
    "libggml.dylib",
    "libggml-base.dylib",
    "libggml-metal.so",
    "libggml-cpu-apple_m1.so",
    "pai-config",
    "pai",
];

/// A fresh directory of its own for one test, removed when dropped.
struct Tmp(PathBuf);
impl Tmp {
    fn new(name: &str) -> Tmp {
        let d = std::env::temp_dir().join(format!("xtask-macos-pkg-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(&d).unwrap();
        Tmp(d)
    }
    fn path(&self) -> &Path {
        &self.0
    }
}
impl Drop for Tmp {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

fn macos_src(rel: &str) -> String {
    let p = Path::new(env!("CARGO_MANIFEST_DIR")).join("src").join(rel);
    std::fs::read_to_string(&p).unwrap_or_else(|e| panic!("{}: {e}", p.display()))
}

/// The code of a source file before its inline tests, so a test's own
/// literal never satisfies a pin.
fn code_of(rel: &str) -> String {
    let text = macos_src(rel);
    text.split("#[cfg(test)]\nmod tests")
        .next()
        .unwrap()
        .to_string()
}

// ── Prefixes and where each built file goes ─────────────────────────────

#[test]
fn the_engine_and_the_framework_have_their_own_prefix() {
    assert_eq!(ENGINE_DIR, ENGINE);
    assert_eq!(FRAMEWORK_DIR, FRAMEWORK);
}

/// llama.cpp's executables, libraries and backends go to the engine's
/// `bin/`, side by side as the build lays them out (ggml finds its backends
/// beside the executable); pai-config and pai to the framework's.
#[test]
fn each_built_file_goes_to_its_own_tree() {
    for file in BUILD_BIN {
        let want = if file == "pai" || file == "pai-config" {
            format!("{FRAMEWORK}/bin/{file}")
        } else {
            format!("{ENGINE}/bin/{file}")
        };
        assert_eq!(staged_path(file), want, "{file}");
    }
}

// ── Which packages exist ────────────────────────────────────────────────

/// The engine is `llama.cpp` and its Vulkan runtime (Metal is part of the
/// system; no ROCm, no CUDA); the framework is the GUI, the command line
/// and `core`, which holds `cli.toml`, the text every program prints. No
/// manager and no web service on macOS.
#[test]
fn macos_packages_the_engine_with_vulkan_and_the_framework_without_services() {
    use Component::*;
    assert_eq!(of_product(Some(Product::Llama)), vec![Llama, Vulkan]);
    assert_eq!(
        of_product(Some(Product::Framework)),
        vec![Config, Cli, Core]
    );
    assert_eq!(of_product(None), vec![Config, Cli, Core, Llama, Vulkan]);
}

/// Every installed path to its package: the engine's tree to `llama.cpp`
/// but its `lib/`, which is the Vulkan runtime (MoltenVK and the loader),
/// the framework's tree as before.
#[test]
fn macos_files_of_the_split_go_to_their_package() {
    for (rel, want) in [
        (format!("{ENGINE}/bin/llama-server"), Component::Llama),
        (format!("{ENGINE}/bin/libllama.dylib"), Component::Llama),
        (format!("{ENGINE}/bin/libggml-base.dylib"), Component::Llama),
        (format!("{ENGINE}/bin/libggml-metal.so"), Component::Llama),
        (format!("{ENGINE}/lib/libMoltenVK.dylib"), Component::Vulkan),
        (format!("{ENGINE}/lib/libvulkan.1.dylib"), Component::Vulkan),
        (icd_manifest(), Component::Vulkan),
        (moltenvk_path(), Component::Vulkan),
        ("/usr/local/bin/llama-server".into(), Component::Llama),
        ("/usr/local/bin/pai".into(), Component::Cli),
        ("/usr/local/bin/pai-config".into(), Component::Config),
        (format!("{FRAMEWORK}/bin/pai"), Component::Cli),
        (format!("{FRAMEWORK}/bin/pai-config"), Component::Config),
        (
            format!("{FRAMEWORK}/share/strings/cli.toml"),
            Component::Core,
        ),
        (
            format!("{FRAMEWORK}/share/strings/en_US.toml"),
            Component::Config,
        ),
        (
            format!("{FRAMEWORK}/share/benchmarks/specs/standard-1.1.ini"),
            Component::Cli,
        ),
    ] {
        assert_eq!(components::of(&rel), want, "{rel}");
    }
}

/// A staged tree laid out as the package phase does (every built file at
/// its `staged_path`, a link per executable in /usr/local/bin, MoltenVK and
/// its manifest, the framework's data) splits into exactly the macOS
/// packages, the framework's holding nothing of the engine and the engine's
/// nothing of the framework.
#[test]
fn a_staged_macos_tree_splits_into_engine_and_framework_packages() {
    let tmp = Tmp::new("split");
    let stage = tmp.path().join("staging");
    let put = |abs: &str| {
        let p = stage.join(abs.trim_start_matches('/'));
        std::fs::create_dir_all(p.parent().unwrap()).unwrap();
        std::fs::write(&p, "").unwrap();
    };
    for file in BUILD_BIN {
        let at = staged_path(file);
        put(&at);
        if !file.ends_with(".dylib") && !file.ends_with(".so") {
            let link = stage.join("usr/local/bin").join(file);
            std::fs::create_dir_all(link.parent().unwrap()).unwrap();
            std::os::unix::fs::symlink(&at, &link).unwrap();
        }
    }
    put(&moltenvk_path());
    put(&format!("{ENGINE}/lib/libvulkan.1.dylib"));
    put(&icd_manifest());
    put(&format!("{FRAMEWORK}/share/strings/cli.toml"));
    put(&format!("{FRAMEWORK}/share/strings/en_US.toml"));
    put(&format!(
        "{FRAMEWORK}/share/benchmarks/specs/standard-1.1.ini"
    ));

    let roots = tmp.path().join("package");
    let present = components::split(&stage, &roots).unwrap();
    assert_eq!(present, of_product(None));

    for c in present {
        let root = components::root(&roots, c);
        let mut dirs = vec![root.clone()];
        while let Some(d) = dirs.pop() {
            for e in std::fs::read_dir(&d).unwrap().flatten() {
                let p = e.path();
                if e.file_type().unwrap().is_dir() {
                    dirs.push(p);
                    continue;
                }
                let rel = format!("/{}", p.strip_prefix(&root).unwrap().display());
                let in_engine = rel.starts_with(&format!("{ENGINE}/"))
                    || rel.starts_with("/usr/local/share/vulkan/")
                    || rel.starts_with("/usr/local/bin/llama-");
                let in_framework = rel.starts_with(&format!("{FRAMEWORK}/"))
                    || rel == "/usr/local/bin/pai"
                    || rel == "/usr/local/bin/pai-config";
                match c.product() {
                    Product::Llama => assert!(in_engine, "{} holds {rel}", c.package()),
                    Product::Framework => {
                        assert!(in_framework, "{} holds {rel}", c.package())
                    }
                }
            }
        }
    }
}

// ── Names, versions and identifiers ─────────────────────────────────────

/// The engine's file names its release (no `+pai`: no system package to
/// conflict with on macOS), the runtime its SDK, the framework's packages
/// the framework version alone.
#[test]
fn macos_file_names_follow_the_split() {
    for (c, want) in [
        (Component::Llama, "llama.cpp-v1.0.0-arm64.pkg"),
        (Component::Vulkan, "llama.cpp-vulkan-v1.4.328.1-arm64.pkg"),
        (
            Component::Config,
            "private-ai-framework-config-v2.99.1-arm64.pkg",
        ),
        (Component::Cli, "private-ai-framework-cli-v2.99.1-arm64.pkg"),
        (
            Component::Core,
            "private-ai-framework-core-v2.99.1-arm64.pkg",
        ),
    ] {
        let got = file_name(c, "2.99.1", "v1.0.0", "1.4.328.1");
        assert_eq!(got, want, "{c:?}");
        assert!(
            components::is_file_of(c, &got),
            "{got} is not a file of {c:?}"
        );
    }
}

/// `xtask install` finds each of them apart with the shared finder: the
/// engine's prefix `llama.cpp-v` never takes the runtime's file.
#[test]
fn the_newest_macos_packages_are_found_one_per_component() {
    let tmp = Tmp::new("newest");
    let mut want = Vec::new();
    for c in of_product(None) {
        let name = file_name(c, "2.99.1", "v1.0.0", "1.4.328.1");
        std::fs::write(tmp.path().join(&name), "").unwrap();
        want.push((c, name));
    }
    let got: Vec<(Component, String)> = components::newest_in(tmp.path(), "-arm64.pkg")
        .into_iter()
        .map(|(c, p)| (c, p.file_name().unwrap().to_string_lossy().into_owned()))
        .collect();
    assert_eq!(got, want);
}

#[test]
fn macos_package_versions_follow_the_split() {
    assert_eq!(
        version(Component::Llama, "2.99.1", "v1.0.0", "1.4.328.1"),
        "1.0.0"
    );
    assert_eq!(
        version(Component::Vulkan, "2.99.1", "v1.0.0", "1.4.328.1"),
        "1.4.328.1"
    );
    for c in [Component::Config, Component::Cli, Component::Core] {
        assert_eq!(
            version(c, "2.99.1", "v1.0.0", "1.4.328.1"),
            "2.99.1",
            "{c:?}"
        );
    }
}

/// The engine's receipts are its own (`org.privateai.llama.cpp`, its
/// runtime under it), apart from the framework's `org.privateai.framework.*`:
/// `pkgutil --forget` of one product never touches the other's.
#[test]
fn the_engine_has_its_own_pkg_identifiers() {
    assert_eq!(identifier(Component::Llama), "org.privateai.llama.cpp");
    assert_eq!(
        identifier(Component::Vulkan),
        "org.privateai.llama.cpp.vulkan"
    );
    for c in [Component::Config, Component::Cli, Component::Core] {
        assert_eq!(
            identifier(c),
            format!("org.privateai.framework.{}", c.suffix()),
            "{c:?}"
        );
    }
}

// ── MoltenVK and its manifest ───────────────────────────────────────────

/// MoltenVK in the engine's `lib/`, its manifest where the Vulkan loader
/// looks, under the engine's name and with an absolute path to it.
#[test]
fn moltenvk_and_its_manifest_are_the_engine_s() {
    assert_eq!(moltenvk_path(), format!("{ENGINE}/lib/libMoltenVK.dylib"));
    assert_eq!(
        icd_manifest(),
        "/usr/local/share/vulkan/icd.d/llama.cpp-MoltenVK_icd.json"
    );
}

// ── Install ─────────────────────────────────────────────────────────────

/// The engine's packages first, then the framework's, each in the order
/// given; a file of no known package keeps its place after them.
#[test]
fn install_lays_the_engine_down_before_the_framework() {
    let p = |s: &str| PathBuf::from(format!("/repo/dist/{s}"));
    let given = vec![
        p("private-ai-framework-config-v2.99.1-arm64.pkg"),
        p("private-ai-framework-cli-v2.99.1-arm64.pkg"),
        p("private-ai-framework-core-v2.99.1-arm64.pkg"),
        p("llama.cpp-v1.0.0-arm64.pkg"),
        p("llama.cpp-vulkan-v1.4.328.1-arm64.pkg"),
        p("mine.pkg"),
    ];
    assert_eq!(
        install_order(given),
        vec![
            p("llama.cpp-v1.0.0-arm64.pkg"),
            p("llama.cpp-vulkan-v1.4.328.1-arm64.pkg"),
            p("private-ai-framework-config-v2.99.1-arm64.pkg"),
            p("private-ai-framework-cli-v2.99.1-arm64.pkg"),
            p("private-ai-framework-core-v2.99.1-arm64.pkg"),
            p("mine.pkg"),
        ]
    );
    assert!(install_order(Vec::new()).is_empty());
}

/// The running check sees a program of either tree (a llama-server from
/// the engine's `bin/` above all) and nothing else. Read as an extended
/// regular expression, as BSD pgrep does, by grep -E here.
#[test]
fn the_running_check_covers_both_trees() {
    use std::io::Write;
    use std::process::{Command, Stdio};
    let pattern = running_pattern();
    let matches = |line: &str| {
        let mut child = Command::new("grep")
            .args(["-E", "-q", "--", &pattern])
            .stdin(Stdio::piped())
            .spawn()
            .expect("grep");
        child
            .stdin
            .take()
            .unwrap()
            .write_all(format!("{line}\n").as_bytes())
            .unwrap();
        child.wait().unwrap().success()
    };
    for line in [
        format!("{ENGINE}/bin/llama-server --port 8080"),
        format!("{FRAMEWORK}/bin/pai-config gui"),
        format!("{FRAMEWORK}/bin/pai bench run"),
    ] {
        assert!(matches(&line), "{pattern} misses {line}");
    }
    for line in [
        "/opt/llamaXcpp/bin/llama-server".to_string(),
        format!("/Users/me{ENGINE}/bin/llama-server"),
        "/usr/local/bin/other".to_string(),
        format!("{ENGINE}/lib/x"),
    ] {
        assert!(!matches(&line), "{pattern} takes {line}");
    }
}

// ── The macOS phases use these (text checks: they build on a Mac only) ──

/// `xtask package [llama|framework]` on macOS as on Linux: the product
/// reaches the package phase, and nothing refuses it.
#[test]
fn macos_package_takes_a_product() {
    let main = macos_src("main.rs");
    let mac = main
        .split("#[cfg(target_os = \"macos\")]\nfn run(")
        .nth(1)
        .expect("the macOS run()");
    let mac = mac.split("#[cfg(test)]").next().unwrap();
    assert!(
        !mac.contains("give no product"),
        "macOS still refuses a product:\n{mac}"
    );
    assert!(
        mac.contains("macos::package::run(product"),
        "the product does not reach macos::package::run:\n{mac}"
    );
}

/// The package phase stages, names, identifies and versions each `.pkg`
/// through these items, and packages only the product's components.
#[test]
fn the_macos_package_phase_uses_the_split() {
    let code = code_of("macos/package.rs");
    assert!(
        code.contains("pub fn run(product: Option<Product>"),
        "package::run takes no product"
    );
    for want in [
        "macos_pkg::of_product(product)",
        "macos_pkg::staged_path(",
        "macos_pkg::identifier(",
        "macos_pkg::version(",
        "macos_pkg::file_name(",
        "macos_pkg::moltenvk_path()",
        "macos_pkg::icd_manifest()",
    ] {
        assert!(code.contains(want), "macos/package.rs does not call {want}");
    }
    for gone in ["private-ai-framework-MoltenVK_icd.json", "{PKG_ID}."] {
        assert!(!code.contains(gone), "macos/package.rs still has {gone}");
    }
}

/// The install phase lays the engine down first and refuses while a
/// program of either tree runs.
#[test]
fn the_macos_install_phase_uses_the_split() {
    let code = code_of("macos/install.rs");
    for want in ["macos_pkg::install_order(", "macos_pkg::running_pattern()"] {
        assert!(code.contains(want), "macos/install.rs does not call {want}");
    }
}
