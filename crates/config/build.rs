// Build script, seven jobs:
// - Icon bootstrap: resources/llama.ico is generated, not checked in; when
//   it's missing, run resources/generate-llama-ico.mjs to rasterize it from
//   the llama-brand icon (the script downloads and caches the SVG).
// - ICO → PNG ×2: decode resources/llama.ico's largest frame and emit the
//   plain tray icon plus a green-dot "running" variant into OUT_DIR, where
//   the Slint @image-url includes resolve them.
// - Compile the .slint UI, emitting element debug info for NON-release
//   profiles only: the e2e tests (src/tests/) need it to find widgets, so
//   they must run under `cargo test`, not `--release`.
// - Windows only: embed the ICO as the EXE resource (winresource).
// - Windows (MSVC) only: reserve an 8 MiB stack for the main thread. The
//   PE default is 1 MiB, and the GUI outgrew it in 2.0.0: building
//   AppWindow on the main thread overflowed it at launch ("thread 'main' has
//   overflowed its stack", exit 0xC00000FD) while the CLI kept working.
//   2 MiB was measured to suffice; 8 MiB matches Linux's default main
//   stack, so both platforms keep the same headroom.
// - The SDK stack llama.cpp was built against (TheRock, CUDA, Vulkan SDK
//   dist versions, the patches applied), read off its CMake cache into
//   PRIVATE_AI_* env vars for bench.db's software row: an installed machine
//   has no build tree to read them from at run time.
// - The per-architecture tables of src/gguf.rs, generated from the llama.cpp
//   clone (arch_tables.rs).

use std::path::Path;

mod arch_tables;

fn main() {
    // Convert llama.ico to PNG so Slint's @image-url can embed it. Emit two
    // variants: the plain icon, and one with a green "running" status dot in the
    // bottom-right corner. The tray binds its icon to whichever matches state.
    let ico_path = "../../resources/llama.ico";
    println!("cargo:rerun-if-changed={ico_path}");
    ensure_icon(ico_path);

    let out_dir = std::env::var("OUT_DIR").expect("OUT_DIR not set");
    emit_arch_tables(&out_dir);

    let img = decode_largest_frame(ico_path);
    let (w, h) = (img.width(), img.height());

    let mut png_bytes = Vec::new();
    img.write_png(&mut png_bytes).expect("Failed to encode PNG");
    let png_path = Path::new(&out_dir).join("llama.png");
    std::fs::write(&png_path, &png_bytes).expect("Failed to write PNG");
    // Both PNGs are loaded by Slint via @image-url("llama.png" / "llama-on.png")
    // resolved against OUT_DIR (see with_include_paths below), so no build-env
    // var is needed to hand their paths to the Rust code.

    // Same frame + green status dot -> llama-on.png.
    let mut rgba = img.into_rgba_data();
    draw_status_dot(&mut rgba, w, h);
    let on_img = ico::IconImage::from_rgba_data(w, h, rgba);
    let mut on_bytes = Vec::new();
    on_img
        .write_png(&mut on_bytes)
        .expect("Failed to encode running PNG");
    std::fs::write(Path::new(&out_dir).join("llama-on.png"), &on_bytes)
        .expect("Failed to write running PNG");

    let mut config = slint_build::CompilerConfiguration::new()
        .with_style("fluent".into())
        .with_include_paths(vec![std::path::PathBuf::from(out_dir)]);
    // The headless UI tests (src/tests/ui_bindings.rs) drive widgets through
    // Slint's ElementHandle API, which needs the compiler to emit element debug info.
    // Enable it only for non-release builds so the size-optimized release binary
    // (opt-level=z + strip) doesn't carry test-only metadata. `cargo test` runs
    // in the "debug" profile; `cargo test --release` would not find widgets.
    if std::env::var("PROFILE").as_deref() != Ok("release") {
        config = config.with_debug_info(true);
    }
    slint_build::compile_with_config("ui/app.slint", config).expect("Slint build failed");

    #[cfg(windows)]
    embed_windows_resources();

    // The TARGET's env, not the host's cfg: /STACK is an MSVC (and lld-link)
    // linker flag. Bins only, so the test harness keeps its own thread stacks.
    if std::env::var("CARGO_CFG_TARGET_ENV").as_deref() == Ok("msvc") {
        println!("cargo:rustc-link-arg-bins=/STACK:8388608");
    }

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
            "{}: {e}. Update crates/config/arch_tables.rs for this llama.cpp release",
            source.display()
        )
    });
    std::fs::write(Path::new(out_dir).join("arch_tables.rs"), tables)
        .expect("Failed to write arch_tables.rs");
}

/// `PRIVATE_AI_ROCM`, `PRIVATE_AI_CUDA`, `PRIVATE_AI_VULKAN_SDK` and
/// `PRIVATE_AI_PATCHES`, each empty when unknown (a bare `cargo build` before
/// 02-build has configured llama.cpp). The dist layout puts the version in the
/// path (`<DistRoot>/<Name>/<Version>/<os>`), so the first cache line naming
/// a dist directory is enough. The patches are the ones 02-build applies: every
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

/// Generate the icon when it's absent (fresh checkout, or deleted on purpose
/// to pick up a brand change) by running the node script that rasterizes it
/// from the llama-brand icon. Needs node + npm and network for the script's
/// one-time SVG download (cached in resources/, gitignored; delete it to
/// re-fetch); a missing prerequisite fails the build with the script's own
/// error message.
fn ensure_icon(ico_path: &str) {
    if Path::new(ico_path).exists() {
        return;
    }
    #[cfg(windows)]
    const NPM: &str = "npm.cmd";
    #[cfg(not(windows))]
    const NPM: &str = "npm";
    let resources = Path::new("../../resources");
    if !resources.join("node_modules/sharp-ico").exists() {
        run_checked(
            std::process::Command::new(NPM)
                .args(["install", "--no-save", "sharp", "sharp-ico"])
                .current_dir(resources),
        );
    }
    run_checked(
        std::process::Command::new("node")
            .arg("generate-llama-ico.mjs")
            .current_dir(resources),
    );
}

fn run_checked(cmd: &mut std::process::Command) {
    let out = cmd
        .output()
        .unwrap_or_else(|e| panic!("Failed to spawn {cmd:?}: {e}"));
    if !out.status.success() {
        panic!(
            "{cmd:?} failed:\n{}{}",
            String::from_utf8_lossy(&out.stdout),
            String::from_utf8_lossy(&out.stderr)
        );
    }
}

fn decode_largest_frame(path: &str) -> ico::IconImage {
    let ico_bytes = std::fs::read(path).expect("Failed to read ICO");
    let dir = ico::IconDir::read(std::io::Cursor::new(&ico_bytes)).expect("Failed to parse ICO");
    let entry = dir
        .entries()
        .iter()
        .max_by_key(|e| e.width())
        .expect("ICO has no entries");
    entry.decode().expect("Failed to decode ICO frame")
}

/// Paint a filled green circle with a white ring in the bottom-right quadrant of
/// the RGBA buffer: a "server running" status badge. Pixels are fully opaque so
/// the dot reads clearly once the platform scales the icon down for the tray.
fn draw_status_dot(rgba: &mut [u8], w: u32, h: u32) {
    let wf = w as f32;
    let hf = h as f32;
    let r = wf * 0.27;
    let margin = wf * 0.05;
    let cx = wf - r - margin;
    let cy = hf - r - margin;
    let ring = (wf * 0.06).max(1.0); // white border thickness for contrast
    for y in 0..h {
        for x in 0..w {
            let dx = x as f32 + 0.5 - cx;
            let dy = y as f32 + 0.5 - cy;
            let d = (dx * dx + dy * dy).sqrt();
            if d <= r {
                let idx = ((y * w + x) * 4) as usize;
                let (cr, cg, cb) = if d >= r - ring {
                    (255u8, 255u8, 255u8) // white ring
                } else {
                    (34u8, 197u8, 94u8) // green fill (#22C55E)
                };
                rgba[idx] = cr;
                rgba[idx + 1] = cg;
                rgba[idx + 2] = cb;
                rgba[idx + 3] = 255;
            }
        }
    }
}

#[cfg(windows)]
fn embed_windows_resources() {
    let icon = "../../resources/llama.ico";
    println!("cargo:rerun-if-changed={icon}");
    let mut res = winresource::WindowsResource::new();
    res.set_icon(icon);
    res.set("FileDescription", "llama.cpp framework configurator");
    res.set("ProductName", "Private AI Framework");
    res.set("OriginalFilename", "pai-config.exe");
    if let Err(e) = res.compile() {
        println!("cargo:warning=Failed to embed icon resource: {e}");
    }
}
