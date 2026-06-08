// Paths for the llama.cpp-framework configuration tool.
//
// Mirrors the layout run-model.ps1 / config-server.ps1 / config-model.ps1 expect.

use std::path::PathBuf;

fn env_path(var: &str) -> Option<PathBuf> {
    std::env::var_os(var).map(PathBuf::from)
}

fn home_dir() -> PathBuf {
    #[cfg(windows)]
    {
        env_path("USERPROFILE").expect("USERPROFILE not set")
    }
    #[cfg(not(windows))]
    {
        env_path("HOME").expect("HOME not set")
    }
}

/// `%LOCALAPPDATA%\llama.cpp` on Windows, `$HOME/.local/share/llama.cpp` elsewhere.
pub fn data_root() -> PathBuf {
    #[cfg(windows)]
    {
        env_path("LOCALAPPDATA")
            .expect("LOCALAPPDATA not set on Windows")
            .join("llama.cpp")
    }
    #[cfg(not(windows))]
    {
        home_dir().join(".local").join("share").join("llama.cpp")
    }
}

pub fn config_dir() -> PathBuf {
    data_root().join("config")
}

pub fn server_ini() -> PathBuf {
    config_dir().join("server.ini")
}

pub fn presets_ini() -> PathBuf {
    config_dir().join("presets.ini")
}

/// `%USERPROFILE%\.config\opencode\opencode.json` on Windows,
/// `$HOME/.config/opencode/opencode.json` elsewhere.
pub fn opencode_user_config() -> PathBuf {
    home_dir()
        .join(".config")
        .join("opencode")
        .join("opencode.json")
}

fn server_binary_name() -> &'static str {
    if cfg!(windows) {
        "llama-server.exe"
    } else {
        "llama-server"
    }
}

/// Where llama-server lives. Tries (in order):
/// 1. `<exe-dir>\<binary>` — installer layout
/// 2. `<exe-dir>\..\..\..\build\llama.cpp-cmake\bin\<binary>` — dev layout
/// 3. `<exe-dir>\..\build\llama.cpp-cmake\bin\<binary>` — alt dev layout
pub fn llama_server_exe() -> Option<PathBuf> {
    let exe = std::env::current_exe().ok()?;
    let exe_dir = exe.parent()?;
    let name = server_binary_name();
    let candidates = [
        exe_dir.join(name),
        exe_dir
            .join("..")
            .join("..")
            .join("..")
            .join("build")
            .join("llama.cpp-cmake")
            .join("bin")
            .join(name),
        exe_dir
            .join("..")
            .join("build")
            .join("llama.cpp-cmake")
            .join("bin")
            .join(name),
    ];
    for c in &candidates {
        if c.exists() {
            return c.canonicalize().ok().or_else(|| Some(c.clone()));
        }
    }
    None
}
