//! Paths for the llama.cpp-framework configuration tool.
//!
//! Three jobs: (1) the user runtime tree — %LOCALAPPDATA%\llama.cpp\ on Windows
//! (config\server.ini, config\presets.ini, logs\llama-server.log), overridable
//! for tests via LLAMA_CPP_CONFIG_DATA_ROOT; (2) locating llama-server.exe
//! across the installer and dev layouts (`llama_server_exe`, which also strips
//! canonicalize()'s \\?\ prefix so the path is shell-pasteable); (3) the ONE
//! path outside that tree — OpenCode's user config (`opencode_user_config`,
//! ~/.config/opencode/opencode.json), used by the Integrations tab.

use std::path::PathBuf;

fn env_path(var: &str) -> Option<PathBuf> {
    std::env::var_os(var).map(PathBuf::from)
}

pub(crate) fn home_dir() -> PathBuf {
    // Under the e2e redirect the temp dir stands in for the whole profile:
    // home-derived paths (e.g. `server_cfg::default_models_dir`, which save()
    // CREATES on disk) must stay inside the temp tree, or a redirected test
    // would silently touch — and mkdir under — the user's real home.
    if let Some(p) = env_path("LLAMA_CPP_CONFIG_DATA_ROOT") {
        return p;
    }
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
///
/// `LLAMA_CPP_CONFIG_DATA_ROOT` overrides the whole tree (plus
/// `opencode_user_config` below AND `home_dir` above, so home-derived
/// defaults land in the temp tree too). It exists for the e2e tests under
/// `src/tests/`, which point config IO at a temp dir so they never touch the
/// user's real data — it is NOT a supported end-user knob.
pub fn data_root() -> PathBuf {
    if let Some(p) = env_path("LLAMA_CPP_CONFIG_DATA_ROOT") {
        return p.join("llama.cpp");
    }
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

/// The llama-server log file. ONE home for the path — `runstate::start()`
/// writes it, the GUI's "no longer running — see …" message points at it.
pub fn server_log() -> PathBuf {
    data_root().join("logs").join("llama-server.log")
}

/// `%USERPROFILE%\.config\opencode\opencode.json` on Windows,
/// `$HOME/.config/opencode/opencode.json` elsewhere.
///
/// The test override redirects this too: the e2e tests exercise flows that read
/// (and could one day write) opencode.json, and must never touch the real one.
pub fn opencode_user_config() -> PathBuf {
    if let Some(p) = env_path("LLAMA_CPP_CONFIG_DATA_ROOT") {
        return p.join("opencode").join("opencode.json");
    }
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
            return Some(
                c.canonicalize()
                    .map(strip_extended_prefix)
                    .unwrap_or_else(|_| c.clone()),
            );
        }
    }
    None
}

/// Drop the `\\?\` extended-length prefix Windows' `canonicalize` prepends:
/// the path is also *displayed* (the Command Line card renders it as the
/// pasteable exe line), and some shells reject the prefix. UNC results
/// (`\\?\UNC\…`) are left as-is — a bare strip would corrupt them.
fn strip_extended_prefix(p: PathBuf) -> PathBuf {
    #[cfg(windows)]
    {
        if let Some(rest) = p.to_str().and_then(|s| s.strip_prefix(r"\\?\")) {
            if !rest.starts_with("UNC") {
                return PathBuf::from(rest);
            }
        }
    }
    p
}
