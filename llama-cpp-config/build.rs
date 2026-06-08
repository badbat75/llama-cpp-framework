use std::path::Path;

fn main() {
    // Convert llama.ico to PNG so Slint's @image-url can embed it.
    let ico_path = "../resources/llama.ico";
    println!("cargo:rerun-if-changed={ico_path}");
    let png_bytes = ico_to_png(ico_path);
    let out_dir = std::env::var("OUT_DIR").expect("OUT_DIR not set");
    let png_path = Path::new(&out_dir).join("llama.png");
    std::fs::write(&png_path, &png_bytes).expect("Failed to write PNG");
    println!("cargo:rustc-env=LLCONFIG_ICON_PNG={}", png_path.display());

    let config = slint_build::CompilerConfiguration::new()
        .with_style("fluent".into())
        .with_include_paths(vec![std::path::PathBuf::from(out_dir)]);
    slint_build::compile_with_config("ui/app.slint", config)
        .expect("Slint build failed");

    #[cfg(windows)]
    embed_windows_resources();
}

fn ico_to_png(path: &str) -> Vec<u8> {
    let ico_bytes = std::fs::read(path).expect("Failed to read ICO");
    let dir = ico::IconDir::read(std::io::Cursor::new(&ico_bytes))
        .expect("Failed to parse ICO");
    let entry = dir
        .entries()
        .iter()
        .max_by_key(|e| e.width())
        .expect("ICO has no entries");
    let img = entry.decode().expect("Failed to decode ICO frame");
    let mut png_bytes = Vec::new();
    img.write_png(&mut png_bytes).expect("Failed to encode PNG");
    png_bytes
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
