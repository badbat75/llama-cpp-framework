// Build script, two jobs:
// - The SDK stack llama.cpp was built against (TheRock, CUDA, Vulkan SDK
//   dist versions, the patches applied), read off its CMake cache into
//   PRIVATE_AI_* env vars for bench.db's software row: an installed machine
//   has no build tree to read them from at run time.
// - The per-architecture tables of src/gguf.rs, generated from the llama.cpp
//   clone (arch_tables.rs).

use std::path::Path;

mod arch_tables;

fn main() {
    let out_dir = std::env::var("OUT_DIR").expect("OUT_DIR not set");
    emit_arch_tables(&out_dir);
    emit_build_stack();
}

/// Generate `arch_tables.rs` in `OUT_DIR` from the clone's `llama-arch.cpp`:
/// `PRIVATE_AI_LLAMA_CPP_DIR`, which `xtask build` sets from the recorded clone,
/// or the default clone of the target OS for a bare cargo command. There is no
/// fallback list: a wrong one would be silent, so a missing clone or a source
/// of another shape fails the build.
fn emit_arch_tables(out_dir: &str) {
    println!("cargo:rerun-if-env-changed=PRIVATE_AI_LLAMA_CPP_DIR");
    let clone = std::env::var("PRIVATE_AI_LLAMA_CPP_DIR").unwrap_or_else(|_| {
        let os = match std::env::var("CARGO_CFG_TARGET_OS").as_deref() {
            Ok("windows") => "windows",
            Ok("macos") => "macos",
            _ => "linux",
        };
        format!("../../build/{os}/llama.cpp")
    });
    let source = Path::new(&clone).join("src/llama-arch.cpp");
    println!("cargo:rerun-if-changed={}", source.display());
    let text = std::fs::read_to_string(&source).unwrap_or_else(|e| {
        panic!(
            "{}: {e}. The architecture tables are generated from the llama.cpp clone: run xtask build first",
            source.display()
        )
    });
    let tables = arch_tables::render(&text).unwrap_or_else(|e| {
        panic!(
            "{}: {e}. Update crates/app/arch_tables.rs for this llama.cpp release",
            source.display()
        )
    });
    std::fs::write(Path::new(out_dir).join("arch_tables.rs"), tables)
        .expect("Failed to write arch_tables.rs");
}

/// `PRIVATE_AI_ROCM`, `PRIVATE_AI_CUDA`, `PRIVATE_AI_VULKAN_SDK` and
/// `PRIVATE_AI_PATCHES`, each empty when unknown (a bare `cargo build` before
/// `xtask build` has configured llama.cpp). The dist layout puts the version in the
/// path (`<DistRoot>/<Name>/<Version>/<os>`), so the first cache line naming
/// a dist directory is enough. The patches are the ones `xtask build` applies: every
/// `.patch` under patches/llama.cpp.
fn emit_build_stack() {
    let os = if std::env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("windows") {
        "windows"
    } else {
        "linux"
    };
    let cache = format!("../../build/{os}/llama.cpp-cmake/CMakeCache.txt");
    println!("cargo:rerun-if-changed={cache}");
    println!("cargo:rerun-if-changed=../../patches/llama.cpp");
    let text = std::fs::read_to_string(&cache)
        .unwrap_or_default()
        .replace('\\', "/");
    for (var, dist) in [
        ("PRIVATE_AI_ROCM", "/TheRock/"),
        ("PRIVATE_AI_CUDA", "/CUDA/"),
        ("PRIVATE_AI_VULKAN_SDK", "/VulkanSDK/"),
    ] {
        let version = text
            .split(dist)
            .nth(1)
            .and_then(|rest| rest.split('/').next())
            .unwrap_or_default();
        println!("cargo:rustc-env={var}={version}");
    }
    let mut patches: Vec<String> = std::fs::read_dir("../../patches/llama.cpp")
        .map(|d| {
            d.flatten()
                .filter_map(|e| e.file_name().into_string().ok())
                .filter(|n| n.ends_with(".patch"))
                .collect()
        })
        .unwrap_or_default();
    patches.sort();
    println!("cargo:rustc-env=PRIVATE_AI_PATCHES={}", patches.join(","));
}
