use std::path::Path;

fn main() {
    // Convert llama.ico to PNG so Slint's @image-url can embed it. Emit two
    // variants: the plain icon, and one with a green "running" status dot in the
    // bottom-right corner. The tray binds its icon to whichever matches state.
    let ico_path = "../resources/llama.ico";
    println!("cargo:rerun-if-changed={ico_path}");

    let out_dir = std::env::var("OUT_DIR").expect("OUT_DIR not set");

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
/// the RGBA buffer — a "server running" status badge. Pixels are fully opaque so
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
    let icon = "../resources/llama.ico";
    println!("cargo:rerun-if-changed={icon}");
    let mut res = winresource::WindowsResource::new();
    res.set_icon(icon);
    res.set("FileDescription", "llama.cpp framework configurator");
    res.set("ProductName", "llama.cpp-framework");
    res.set("OriginalFilename", "llama-cpp-config.exe");
    if let Err(e) = res.compile() {
        println!("cargo:warning=Failed to embed icon resource: {e}");
    }
}
