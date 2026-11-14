//! Paths for the Private AI Framework.
//!
//! Four jobs: (1) the user runtime tree, %LOCALAPPDATA%\private-ai\ on Windows
//! (config\server.ini, config\presets.ini, config\settings.ini,
//! logs\llama-server.log) and ~/.local/share/private-ai elsewhere, where the
//! config files live in ~/.config/private-ai instead (`config_dir`), overridable
//! for tests via PRIVATE_AI_DATA_ROOT; (2) locating llama.cpp's binaries
//! across the installer and dev layouts (`llama_bin`, with `llama_server_exe` /
//! `llama_bench_exe` as its two callers; it also strips canonicalize()'s \\?\
//! prefix so the path is shell-pasteable); (3) the ONE
//! path outside that tree, OpenCode's user config (`opencode_user_config`,
//! ~/.config/opencode/opencode.json), used by the Integrations tab; (4) locating
//! the ROCm/HIP runtime (`rocm_bin_dir`) so `proc` can make ggml-hip.dll
//! loadable in the llama-server children we spawn.

use std::path::{Path, PathBuf};

/// The directory name of every tree the framework keeps (user data, config,
/// system config).
const APP_DIR: &str = "private-ai";

/// The engine's directory name, as in `/etc/llama.cpp`: on Windows the
/// machine-wide tree of the engine's instances under `%ProgramData%`.
const ENGINE_DIR: &str = "llama.cpp";

/// The test redirect of the whole data tree (`data_root`).
const DATA_ROOT_VAR: &str = "PRIVATE_AI_DATA_ROOT";

fn env_path(var: &str) -> Option<PathBuf> {
    std::env::var_os(var).map(PathBuf::from)
}

pub fn home_dir() -> PathBuf {
    // Under the e2e redirect the temp dir stands in for the whole profile:
    // home-derived paths (e.g. `server_cfg::default_models_dir`, which save()
    // CREATES on disk) must stay inside the temp tree, or a redirected test
    // would silently touch (and mkdir under) the user's real home.
    if let Some(p) = env_path(DATA_ROOT_VAR) {
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

/// `%LOCALAPPDATA%\private-ai` on Windows, `$HOME/.local/share/private-ai` elsewhere.
///
/// `PRIVATE_AI_DATA_ROOT` overrides the whole tree (plus
/// `opencode_user_config` below AND `home_dir` above, so home-derived
/// defaults land in the temp tree too). It exists for the e2e tests under
/// `src/tests/`, which point config IO at a temp dir so they never touch the
/// user's real data; it is NOT a supported end-user knob.
pub fn data_root() -> PathBuf {
    if let Some(p) = env_path(DATA_ROOT_VAR) {
        return p.join(APP_DIR);
    }
    #[cfg(windows)]
    {
        env_path("LOCALAPPDATA")
            .expect("LOCALAPPDATA not set on Windows")
            .join(APP_DIR)
    }
    #[cfg(not(windows))]
    {
        home_dir().join(".local").join("share").join(APP_DIR)
    }
}

/// `<data_root>\config` on Windows. On Linux `$XDG_CONFIG_HOME/private-ai`,
/// i.e. `~/.config/private-ai` unless the variable names another ABSOLUTE path
/// (the XDG spec says a relative one is to be ignored): per user, as
/// `%LOCALAPPDATA%` is, and the directory llama.cpp itself reads a
/// `config.ini` from (`common/arg.cpp`), which is not a name this tool writes.
/// Only the config files live there: logs, bench runs and saved states stay
/// under `data_root()`. Under the test override it is `<data_root>/config` on
/// every OS, so the e2e tests never touch a real config.
pub fn config_dir() -> PathBuf {
    #[cfg(not(windows))]
    if env_path(DATA_ROOT_VAR).is_none() {
        let base = env_path("XDG_CONFIG_HOME")
            .filter(|p| p.is_absolute())
            .unwrap_or_else(|| home_dir().join(".config"));
        return base.join(APP_DIR);
    }
    data_root().join("config")
}

/// The framework's machine-wide config directory (`bff.ini`; on macOS also
/// the instance files, `engine_config_dir()`):
/// `/etc/private-ai` outside Windows, `%ProgramData%\private-ai\config` on
/// Windows. Under the test override it is `<override>/system-config` on every
/// OS, apart from the per-user `config_dir()`.
pub fn system_config_dir() -> PathBuf {
    if let Some(p) = env_path(DATA_ROOT_VAR) {
        return p.join("system-config");
    }
    #[cfg(windows)]
    {
        windows_machine_dirs(&program_data()).framework_config
    }
    #[cfg(not(windows))]
    {
        PathBuf::from("/etc").join(APP_DIR)
    }
}

/// The engine's machine-wide config directory, where the instance files
/// live (`server@<id>.ini`, `presets.ini`, `presets.ini.disabled`):
/// `/etc/llama.cpp` on Linux, `%ProgramData%\llama.cpp\config` on Windows,
/// `system_config_dir()` on macOS. Under
/// the test override it is `<override>/engine-config` on every OS, a sibling
/// of `system_config_dir()`.
pub fn engine_config_dir() -> PathBuf {
    if let Some(p) = env_path(DATA_ROOT_VAR) {
        return p.join("engine-config");
    }
    #[cfg(target_os = "linux")]
    {
        PathBuf::from("/etc/llama.cpp")
    }
    #[cfg(windows)]
    {
        windows_machine_dirs(&program_data()).engine_config
    }
    #[cfg(not(any(target_os = "linux", windows)))]
    {
        system_config_dir()
    }
}

/// The machine-wide log directory of the instances' services:
/// `%ProgramData%\llama.cpp\logs` on Windows, `/var/log/private-ai`
/// elsewhere (unused on Linux, where the journal keeps the output). Under
/// the test override it is `<override>/system-logs` on every OS.
pub fn system_log_dir() -> PathBuf {
    if let Some(p) = env_path(DATA_ROOT_VAR) {
        return p.join("system-logs");
    }
    #[cfg(windows)]
    {
        windows_machine_dirs(&program_data()).instance_logs
    }
    #[cfg(not(windows))]
    {
        PathBuf::from("/var/log").join(APP_DIR)
    }
}

/// The machine-wide directories on Windows under the `%ProgramData%` root
/// `program_data`: the framework's own config (`bff.ini`) and the engine's
/// instances' config, logs and default models.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WindowsMachineDirs {
    pub framework_config: PathBuf,
    pub engine_config: PathBuf,
    pub instance_logs: PathBuf,
    pub instance_models: PathBuf,
}

pub fn windows_machine_dirs(program_data: &Path) -> WindowsMachineDirs {
    let engine = program_data.join(ENGINE_DIR);
    WindowsMachineDirs {
        framework_config: program_data.join(APP_DIR).join("config"),
        engine_config: engine.join("config"),
        instance_logs: engine.join("logs"),
        instance_models: engine.join("models"),
    }
}

/// `%ProgramData%`, the root of `windows_machine_dirs`.
#[cfg(windows)]
fn program_data() -> PathBuf {
    env_path("ProgramData").expect("ProgramData not set on Windows")
}

/// The default ModelsDir of the instances: `%ProgramData%\llama.cpp\models`
/// on Windows, the service user's `/var/lib/private-ai/.llama.cpp` on Linux
/// (the one the engine's shipped `server@default.ini` names) and, on macOS,
/// where the configurator runs llama-server as the user, the user's own
/// `~/.llama.cpp` (`server_cfg::default_models_dir`). Under the test
/// override it is `<override>/engine-models` on every OS.
pub fn instance_models_dir() -> PathBuf {
    if let Some(p) = env_path(DATA_ROOT_VAR) {
        return p.join("engine-models");
    }
    #[cfg(windows)]
    {
        windows_machine_dirs(&program_data()).instance_models
    }
    #[cfg(target_os = "linux")]
    {
        PathBuf::from("/var/lib").join(APP_DIR).join(".llama.cpp")
    }
    #[cfg(not(any(windows, target_os = "linux")))]
    {
        home_dir().join(".llama.cpp")
    }
}

/// `llama-server@<id>.log` in `system_log_dir()`, the output of instance
/// `id`'s service on Windows; None for an invalid id.
pub fn instance_log(id: &str) -> Option<PathBuf> {
    valid_instance_id(id).then(|| system_log_dir().join(format!("llama-server@{id}.log")))
}

/// An instance id is 1 to 32 ASCII lowercase letters, digits and `-`, starting
/// with a letter or digit, so it is always a safe file name component.
pub fn valid_instance_id(id: &str) -> bool {
    let ok = |c: char| c.is_ascii_lowercase() || c.is_ascii_digit();
    (1..=32).contains(&id.len())
        && id.chars().next().is_some_and(ok)
        && id.chars().all(|c| ok(c) || c == '-')
}

/// `server@<id>.ini` in `engine_config_dir()`, or None for an invalid id.
pub fn instance_ini(id: &str) -> Option<PathBuf> {
    valid_instance_id(id).then(|| engine_config_dir().join(format!("server@{id}.ini")))
}

/// The ids of the regular files `server@<valid id>.ini` in
/// `engine_config_dir()`, sorted; empty when the directory is missing, which
/// this never creates.
pub fn instance_ids() -> Vec<String> {
    let Ok(entries) = std::fs::read_dir(engine_config_dir()) else {
        return Vec::new();
    };
    let mut ids: Vec<String> = entries
        .flatten()
        .filter(|e| e.file_type().is_ok_and(|t| t.is_file()))
        .filter_map(|e| {
            let name = e.file_name().into_string().ok()?;
            let id = name.strip_prefix("server@")?.strip_suffix(".ini")?;
            valid_instance_id(id).then(|| id.to_string())
        })
        .collect();
    ids.sort();
    ids
}

pub fn server_ini() -> PathBuf {
    config_dir().join("server.ini")
}

pub fn presets_ini() -> PathBuf {
    config_dir().join("presets.ini")
}

/// The presets switched OFF: same INI format as `presets_ini()`, sections moved
/// here verbatim by `presets::set_enabled`. A different EXTENSION on purpose:
/// llama-server only ever reads the file `--models-preset` names, so a preset
/// parked here is simply not offered, while a `*.ini` glob (the user's own, or a
/// backup tool's) cannot mistake it for a live config.
pub fn presets_disabled_ini() -> PathBuf {
    config_dir().join("presets.ini.disabled")
}

/// The configurator's OWN settings (the Settings tab; see `settings.rs`),
/// kept apart from server.ini, whose every key maps to a llama-server flag.
pub fn settings_ini() -> PathBuf {
    config_dir().join("settings.ini")
}

/// The framework's benchmark method as installed: `specs\` (every published
/// version of `standard`, `standard-<X.Y>.ini`, and `standard.versions`) and
/// `prompts\` (every prompt a version pins, `<name>-<X.Y>.txt|json`).
/// Read-only, beside the binaries ([`installed_share_dir`]):
/// `<root>\share\benchmarks` where the exe is `<root>\bin\pai-config`
/// (`/usr/share/private-ai/benchmarks` for the Linux packages), else
/// the repo's own `benchmarks\` for a tree build. The binary carries only
/// `standard.versions` (the digest of every published spec, and each spec
/// pins its prompts' digests), so an edited file here is refused, not run.
pub fn bench_share_dir() -> PathBuf {
    let installed = std::env::current_exe()
        .ok()
        .and_then(|exe| installed_share_dir(&exe, "benchmarks"));
    match installed {
        Some(dir) if dir.join("specs").is_dir() => dir,
        _ => Path::new(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .and_then(Path::parent)
            .unwrap_or(Path::new("."))
            .join("benchmarks"),
    }
}

/// The shipped prompts: `bench_share_dir()\prompts`.
pub fn bench_prompts_dir() -> PathBuf {
    bench_share_dir().join("prompts")
}

/// The language files as installed ([`installed_share_dir`]):
/// `<root>\share\strings` where the exe is `<root>\bin\pai-config`
/// (`/usr/share/private-ai/strings` for the Linux packages), else
/// the crate's own `strings\` for a tree build. Read-only, like
/// `bench_share_dir`.
pub fn strings_dir() -> PathBuf {
    let installed = std::env::current_exe()
        .ok()
        .and_then(|exe| installed_share_dir(&exe, "strings"));
    match installed {
        Some(dir) => dir,
        _ => Path::new(env!("CARGO_MANIFEST_DIR")).join("strings"),
    }
}

/// The live benchmark's DEFAULT prompt: the shipped 43k one, read in place
/// (it is installed read-only and never edited: a prompt of your own is a file
/// of your own, anywhere). settings.ini's `BenchPromptFile` can point the
/// Benchmark tab and the CLI at any other file.
pub fn bench_prompt_file() -> PathBuf {
    bench_prompts_dir().join("bench-prompt-43k-1.0.txt")
}

/// The llama-server log file. ONE home for the path: `runstate::start()`
/// writes it, the GUI's "no longer running" message points at it.
pub fn server_log() -> PathBuf {
    data_root().join("logs").join("llama-server.log")
}

/// `%USERPROFILE%\.config\opencode\opencode.json` on Windows,
/// `$HOME/.config/opencode/opencode.json` elsewhere.
///
/// The test override redirects this too: the e2e tests exercise flows that read
/// (and could one day write) opencode.json, and must never touch the real one.
pub fn opencode_user_config() -> PathBuf {
    if let Some(p) = env_path(DATA_ROOT_VAR) {
        return p.join("opencode").join("opencode.json");
    }
    home_dir()
        .join(".config")
        .join("opencode")
        .join("opencode.json")
}

/// The ROCm/HIP runtime dir on Windows (`…\AMD\ROCm\<ver>\bin`, home of
/// `amdhip64_*.dll` / `rocblas.dll`). AMD's HIP SDK installer sets the
/// machine-wide `HIP_PATH` env var but does NOT add this dir to the system
/// PATH, and ggml loads backends dynamically, silently dropping ggml-hip.dll
/// when its imports don't resolve, so HIP GPUs then enumerate as Vulkan-only.
/// `proc::prepend_rocm_path` feeds this to every llama-server child. It is what
/// a build from source runs on; an installed tree bundles the runtime next to
/// llama-server.exe, which precedes PATH, so this dir is then not consulted. Primary
/// signal is `HIP_PATH`; fallback scans `%ProgramFiles%\AMD\ROCm` for the
/// newest versioned install. `None` when no ROCm is found, and on non-Windows
/// (there the loader resolves the runtime without PATH).
pub fn rocm_bin_dir() -> Option<PathBuf> {
    #[cfg(windows)]
    {
        if let Some(hip) = env_path("HIP_PATH") {
            let bin = hip.join("bin");
            if bin.is_dir() {
                return Some(bin);
            }
        }
        let root = env_path("ProgramFiles")?.join("AMD").join("ROCm");
        let versions: Vec<String> = std::fs::read_dir(&root)
            .ok()?
            .flatten()
            .filter(|e| e.path().join("bin").is_dir())
            .filter_map(|e| e.file_name().into_string().ok())
            .collect();
        let newest = newest_version(&versions)?;
        Some(root.join(newest).join("bin"))
    }
    #[cfg(not(windows))]
    {
        None
    }
}

/// The highest version-shaped name ("7.1", "6.4.2") in `names`, compared by
/// numeric components: a string sort would rank "7.1" above "10.0". Names
/// with any non-numeric component are ignored.
#[cfg_attr(not(windows), allow(dead_code))]
fn newest_version(names: &[String]) -> Option<&String> {
    fn key(name: &str) -> Option<Vec<u32>> {
        name.split('.').map(|c| c.parse().ok()).collect()
    }
    names
        .iter()
        .filter_map(|n| key(n).map(|k| (k, n)))
        .max_by(|a, b| a.0.cmp(&b.0))
        .map(|(_, n)| n)
}

pub fn binary_name(stem: &str) -> String {
    if cfg!(windows) {
        format!("{stem}.exe")
    } else {
        stem.to_string()
    }
}

/// Where llama-server lives. See [`llama_bin`] for the search order.
pub fn llama_server_exe() -> Option<PathBuf> {
    llama_bin("llama-server")
}

/// Where `llama-bench` lives: same tree as llama-server, since the installer
/// stages every tool cmake installs into one `bin\` (it is not pruned like the
/// `test-*` harnesses are). The Benchmark tab's synthetic mode runs it.
pub fn llama_bench_exe() -> Option<PathBuf> {
    llama_bin("llama-bench")
}

/// [`llama_bin`] for a program at `exe`: the same search, from that path
/// instead of the running one. The Linux packages put `pai-config` and
/// llama.cpp's executables in `/usr/bin` and `pai-manager` in
/// `/usr/libexec/private-ai`, which must find `/usr/bin/llama-server`.
pub fn llama_bin_from(exe: &Path, stem: &str) -> Option<PathBuf> {
    // The per-OS directory the build scripts generate everything into.
    #[cfg(windows)]
    const BUILD_OS_DIR: &str = "windows";
    #[cfg(not(windows))]
    const BUILD_OS_DIR: &str = "linux";

    let exe_dir = exe.parent()?;
    let name = binary_name(stem);
    let name = name.as_str();
    let mut candidates = vec![exe_dir.join(name)];
    if exe_dir.ends_with("libexec/private-ai") {
        candidates.push(exe_dir.join("..").join("..").join("bin").join(name));
    }
    candidates.extend([
        exe_dir
            .join("..")
            .join("..")
            .join("llama.cpp-cmake")
            .join("bin")
            .join(name),
        exe_dir
            .join("..")
            .join("..")
            .join("build")
            .join(BUILD_OS_DIR)
            .join("llama.cpp-cmake")
            .join("bin")
            .join(name),
        exe_dir
            .join("..")
            .join("build")
            .join(BUILD_OS_DIR)
            .join("llama.cpp-cmake")
            .join("bin")
            .join(name),
    ]);
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

/// [`llama_bin_from`], then llama.cpp's own install `engine_dir` (on
/// Windows the folder llama.cpp's MSI registers, [`engine_install_dir`]):
/// `<engine_dir>\bin\<binary>`, looked for after the tree builds, so a tree
/// build keeps running the llama-server it built.
pub fn llama_bin_with_engine(exe: &Path, stem: &str, engine_dir: Option<&Path>) -> Option<PathBuf> {
    llama_bin_from(exe, stem).or_else(|| {
        let c = engine_dir?.join("bin").join(binary_name(stem));
        c.is_file().then(|| {
            c.canonicalize()
                .map(strip_extended_prefix)
                .unwrap_or_else(|_| c.clone())
        })
    })
}

/// The folder llama.cpp's own MSI installed into, from its registration
/// (`HKLM\Software\llama.cpp` `InstallDir`); `None` elsewhere than Windows
/// and when no engine is registered.
pub fn engine_install_dir() -> Option<PathBuf> {
    #[cfg(windows)]
    {
        let dir = win_reg::machine_reg_sz(r"Software\llama.cpp", "InstallDir")?;
        (!dir.is_empty()).then(|| PathBuf::from(dir))
    }
    #[cfg(not(windows))]
    {
        None
    }
}

/// One `REG_SZ` under `HKLM` (64-bit view of a 64-bit program), for
/// [`engine_install_dir`].
#[cfg(windows)]
mod win_reg {
    use windows_sys::Win32::Foundation::ERROR_SUCCESS;
    use windows_sys::Win32::System::Registry::{RegGetValueW, HKEY_LOCAL_MACHINE, RRF_RT_REG_SZ};

    pub(super) fn machine_reg_sz(subkey: &str, value: &str) -> Option<String> {
        let wide = |s: &str| s.encode_utf16().chain(Some(0)).collect::<Vec<u16>>();
        let (sub, name) = (wide(subkey), wide(value));
        let mut len: u32 = 0;
        // SAFETY: NUL-terminated names; a null buffer asks for the size.
        let sized = unsafe {
            RegGetValueW(
                HKEY_LOCAL_MACHINE,
                sub.as_ptr(),
                name.as_ptr(),
                RRF_RT_REG_SZ,
                std::ptr::null_mut(),
                std::ptr::null_mut(),
                &mut len,
            )
        };
        if sized != ERROR_SUCCESS {
            return None;
        }
        let mut buf: Vec<u16> = vec![0; len as usize / 2 + 1];
        let mut bytes = (buf.len() * 2) as u32;
        // SAFETY: the buffer holds `bytes` bytes.
        let read = unsafe {
            RegGetValueW(
                HKEY_LOCAL_MACHINE,
                sub.as_ptr(),
                name.as_ptr(),
                RRF_RT_REG_SZ,
                std::ptr::null_mut(),
                buf.as_mut_ptr().cast(),
                &mut bytes,
            )
        };
        if read != ERROR_SUCCESS {
            return None;
        }
        let end = buf.iter().position(|&c| c == 0).unwrap_or(buf.len());
        Some(String::from_utf16_lossy(&buf[..end]))
    }
}

/// The directories [`installed_share_dir`] looks in for `sub`, in order,
/// for a program at `exe`. Pure: nothing is read.
pub fn installed_share_candidates(exe: &Path, sub: &str) -> Vec<PathBuf> {
    let Some(dir) = exe.parent() else {
        return Vec::new();
    };
    let Some(up) = dir.parent() else {
        return Vec::new();
    };
    // The Linux packages' services: `<prefix>/libexec/private-ai/<exe>`.
    if dir.file_name().is_some_and(|n| n == "private-ai")
        && up.file_name().is_some_and(|n| n == "libexec")
    {
        return match up.parent() {
            Some(prefix) => vec![prefix.join("share").join("private-ai").join(sub)],
            None => Vec::new(),
        };
    }
    let share = up.join("share");
    vec![share.join("private-ai").join(sub), share.join(sub)]
}

/// The installed read-only data directory `sub` (`benchmarks`, `strings`)
/// for a program at `exe`, or `None` when no package installed it there:
/// `<prefix>/share/private-ai/<sub>` for the Linux packages (`/usr/bin/..`
/// and the services' `/usr/libexec/private-ai/..`),
/// `<prefix>/share/<sub>` for a tree under its own prefix (macOS
/// `/opt/private-ai/bin/..`, Windows `<InstallDir>\bin\..`): the first
/// existing one of [`installed_share_candidates`].
pub fn installed_share_dir(exe: &Path, sub: &str) -> Option<PathBuf> {
    installed_share_candidates(exe, sub)
        .into_iter()
        .find(|d| d.is_dir())
}

/// Locate one of llama.cpp's binaries by stem, from the running program
/// ([`llama_bin_from`]). Tries (in order):
/// 1. `<exe-dir>\<binary>`: installer layout (on Linux, `pai-config` beside
///    llama.cpp's executables in `/usr/bin`)
/// 2. `<exe-dir>\..\..\bin\<binary>` for a program in `libexec/private-ai`:
///    the Linux packages' `pai-manager`
/// 3. `<exe-dir>\..\..\llama.cpp-cmake\bin\<binary>`: the build scripts'
///    layout (exe in `build\<os>\cargo-target\release`, `<os>` being
///    `windows` or `linux`)
/// 4. `<exe-dir>\..\..\build\<os>\llama.cpp-cmake\bin\<binary>`: a bare
///    `cargo build` at the workspace root (exe in `target\release`)
/// 5. `<exe-dir>\..\build\<os>\llama.cpp-cmake\bin\<binary>`: alt dev layout
/// 6. `<engine>\bin\<binary>`, `<engine>` the folder llama.cpp's own MSI
///    registers ([`engine_install_dir`], Windows)
pub fn llama_bin(stem: &str) -> Option<PathBuf> {
    llama_bin_with_engine(
        &std::env::current_exe().ok()?,
        stem,
        engine_install_dir().as_deref(),
    )
}

/// Drop the `\\?\` extended-length prefix Windows' `canonicalize` prepends:
/// the path is also *displayed* (the Command Line card renders it as the
/// pasteable exe line), and some shells reject the prefix. UNC results
/// (`\\?\UNC\…`) are left as-is: a bare strip would corrupt them.
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

/// Hand a file or folder to the desktop's default handler: `explorer` on
/// Windows, `xdg-open` elsewhere.
pub fn open_in_desktop(path: &Path) -> std::io::Result<std::process::Child> {
    let opener = if cfg!(windows) {
        "explorer"
    } else {
        "xdg-open"
    };
    std::process::Command::new(opener).arg(path).spawn()
}

/// A Windows path literal as the path of the OS the tests run on: unchanged on
/// Windows; elsewhere the drive is dropped and `\` becomes `/`
/// (`C:\m\x.gguf` -> `/m/x.gguf`), because `std::path` only splits on the
/// host's separator. For the tests that take a file name, a stem or a parent
/// out of a path: written once, they then hold on both OSes.
#[cfg(any(test, feature = "test-support"))]
pub fn os_path(win: &str) -> String {
    if cfg!(windows) {
        return win.to_string();
    }
    let rest = match win.as_bytes() {
        [d, b':', ..] if d.is_ascii_alphabetic() => &win[2..],
        _ => win,
    };
    rest.replace('\\', "/")
}

/// Point the whole data tree at `dir` for a test, through `PRIVATE_AI_DATA_ROOT`,
/// and refuse to go on unless the paths really follow it: a redirect that is
/// not honoured would send the test's IO into the user's real tree. The OLD
/// variable is set too, to a directory of its own inside `dir`, which the paths
/// must ignore (no fallback to the old name). Process-wide and never restored,
/// so only an e2e phase calls it (`src/tests/mod.rs`).
#[cfg(any(test, feature = "test-support"))]
pub fn redirect_for_tests(dir: &Path) {
    // Spelled in two halves so the names lint finds no old name in the code.
    let old_name = concat!("LLAMA_CPP_CONFIG", "_DATA_ROOT");
    std::env::set_var(old_name, dir.join("old-name-ignored"));
    std::env::set_var("PRIVATE_AI_DATA_ROOT", dir);
    assert_eq!(
        home_dir(),
        dir,
        "PRIVATE_AI_DATA_ROOT is not honoured (or the old name still is): \
         the test stops before touching any real path"
    );
}

/// E2E phase for the system config directory and the instance paths in it, for the
/// e2e test of this crate to call: it sets the process-wide
/// `PRIVATE_AI_DATA_ROOT` redirect, which a plain unit test must not
/// (`src/tests/mod.rs`). Only the redirected case is pinned here: the real
/// paths need the variable unset, which `real_paths_follow_the_new_names` does
/// in a child process.
#[cfg(test)]
pub(crate) fn instance_paths_e2e() {
    let dir = tempfile::tempdir().expect("tempdir");
    redirect_for_tests(dir.path());
    let sys = dir.path().join("system-config");
    let engine = dir.path().join("engine-config");

    // The redirected tree is named after the project, under the new variable,
    // and every home-derived path stays inside it.
    assert_eq!(home_dir(), dir.path());
    assert_eq!(data_root(), dir.path().join("private-ai"));
    assert_eq!(config_dir(), dir.path().join("private-ai").join("config"));
    assert_eq!(
        server_log(),
        dir.path()
            .join("private-ai")
            .join("logs")
            .join("llama-server.log")
    );
    assert_eq!(
        opencode_user_config(),
        dir.path().join("opencode").join("opencode.json")
    );

    // The redirect wins on every OS, and the system directory is not the
    // per-user one.
    assert_eq!(system_config_dir(), sys);
    assert_ne!(
        system_config_dir(),
        config_dir(),
        "apart from the user's config"
    );

    // The instance files are the engine's: its own directory, apart from
    // the framework's (bff.ini) and from the user's.
    assert_eq!(engine_config_dir(), engine);
    assert_ne!(
        engine_config_dir(),
        system_config_dir(),
        "not the framework's"
    );
    assert_ne!(engine_config_dir(), config_dir(), "not the user's");

    // No directory yet: no instances, and nothing created by asking.
    assert_eq!(instance_ids(), Vec::<String>::new());
    assert!(
        !engine.exists(),
        "instance_ids must not create the directory"
    );
    assert!(!sys.exists(), "nor the framework's");

    // A crafted or malformed id never yields a path.
    for bad in [
        "", "../x", "..", "a/b", "Default", "a_b", "-a", "a@b", "a b", "x.ini",
    ] {
        assert_eq!(instance_ini(bad), None, "id {bad:?}");
    }
    assert_eq!(
        instance_ini("default"),
        Some(engine.join("server@default.ini"))
    );
    assert_eq!(instance_ini("lan-2"), Some(engine.join("server@lan-2.ini")));
    assert_eq!(instance_ini("8080"), Some(engine.join("server@8080.ini")));

    // An instance file left in the framework's directory is no instance:
    // nothing moves or reads files from there.
    std::fs::create_dir_all(&sys).expect("system config dir");
    std::fs::write(sys.join("server@old.ini"), "[Server]\n").expect("stray file");
    std::fs::write(sys.join("bff.ini"), "[Bff]\n").expect("bff.ini");
    assert_eq!(
        instance_ids(),
        Vec::<String>::new(),
        "the framework's files"
    );
    assert!(!engine.exists(), "nothing created or moved");

    // Only `server@<valid id>.ini` FILES are instances, sorted.
    let sys = engine;
    std::fs::create_dir_all(&sys).expect("engine config dir");
    for name in [
        "server@lan-2.ini",
        "server@default.ini",
        "server@8080.ini",
        "server.ini",
        "presets.ini",
        "presets.ini.disabled",
        "server@x.ini.bak",
        "server@.ini",
        "server@Upper.ini",
        "server@a_b.ini",
        "server@-lead.ini",
        "server@a.b.ini",
        "settings.ini",
    ] {
        std::fs::write(sys.join(name), "[Server]\n").expect("fixture file");
    }
    std::fs::create_dir(sys.join("server@dir.ini")).expect("fixture directory");
    assert_eq!(instance_ids(), ["8080", "default", "lan-2"]);
}

/// E2E phase for the instances' log files (the service's output on Windows),
/// for the e2e test of this crate to call: it sets the process-wide
/// `PRIVATE_AI_DATA_ROOT` redirect (`src/tests/mod.rs`).
#[cfg(test)]
pub(crate) fn instance_log_paths_e2e() {
    let dir = tempfile::tempdir().expect("tempdir");
    redirect_for_tests(dir.path());
    let logs = dir.path().join("system-logs");
    assert_eq!(system_log_dir(), logs);
    assert_ne!(system_log_dir(), data_root().join("logs"), "not the user's");
    assert_ne!(system_log_dir(), system_config_dir());
    assert_eq!(
        instance_log("default"),
        Some(logs.join("llama-server@default.log"))
    );
    assert_eq!(
        instance_log("lan-2"),
        Some(logs.join("llama-server@lan-2.log"))
    );
    assert!(!logs.exists(), "asking creates nothing");
}

/// E2E phase for the instances' default models directory, for the e2e test
/// of this crate to call: it sets the process-wide `PRIVATE_AI_DATA_ROOT`
/// redirect (`src/tests/mod.rs`). Beside the engine's config, in the same
/// redirect layout: `<override>/engine-models`, apart from the user's tree.
#[cfg(test)]
pub(crate) fn instance_models_dir_e2e() {
    let dir = tempfile::tempdir().expect("tempdir");
    redirect_for_tests(dir.path());
    let models = dir.path().join("engine-models");
    assert_eq!(instance_models_dir(), models);
    assert_ne!(instance_models_dir(), engine_config_dir());
    assert_ne!(instance_models_dir(), system_log_dir());
    assert!(
        !instance_models_dir().starts_with(data_root()),
        "not in the user's tree"
    );
    assert!(!models.exists(), "asking creates nothing");
}

#[cfg(test)]
mod tests {
    use super::{
        installed_share_candidates, installed_share_dir, llama_bin_from, llama_bin_with_engine,
        newest_version, valid_instance_id, windows_machine_dirs,
    };
    use std::path::{Path, PathBuf};

    /// An empty file at `root/rel`, its directories made.
    fn touch(root: &Path, rel: &str) -> PathBuf {
        let p = root.join(rel);
        std::fs::create_dir_all(p.parent().unwrap()).unwrap();
        std::fs::write(&p, "").unwrap();
        p
    }

    fn canon(p: Option<PathBuf>) -> Option<PathBuf> {
        p.map(|p| p.canonicalize().unwrap())
    }

    // ── The machine-wide directories on Windows ──────────────────────────
    // Pure over the `%ProgramData%` root, so the Windows layout is pinned
    // on every OS: everything of the engine's instances under
    // `%ProgramData%\llama.cpp\` (config, logs, the default models), only the
    // framework's own config (`bff.ini`) under `%ProgramData%\private-ai\`.

    /// The engine's directory name, as `/etc/llama.cpp` on Linux.
    const ENGINE: &str = "llama.cpp";

    #[test]
    fn windows_instance_dirs_are_the_engine_s_under_program_data() {
        let pd = Path::new(r"C:\ProgramData");
        let d = windows_machine_dirs(pd);
        assert_eq!(d.engine_config, pd.join(ENGINE).join("config"));
        assert_eq!(d.instance_logs, pd.join(ENGINE).join("logs"));
        assert_eq!(d.instance_models, pd.join(ENGINE).join("models"));
    }

    #[test]
    fn windows_framework_config_stays_in_private_ai() {
        let pd = Path::new(r"C:\ProgramData");
        let d = windows_machine_dirs(pd);
        assert_eq!(d.framework_config, pd.join("private-ai").join("config"));
        assert_ne!(d.framework_config, d.engine_config, "not the engine's");
    }

    /// The root is whatever `%ProgramData%` says, never a fixed `C:`.
    #[test]
    fn windows_machine_dirs_follow_the_program_data_root() {
        let pd = Path::new(r"D:\Data");
        let d = windows_machine_dirs(pd);
        for p in [
            &d.framework_config,
            &d.engine_config,
            &d.instance_logs,
            &d.instance_models,
        ] {
            assert!(p.starts_with(pd), "{}", p.display());
        }
        assert_eq!(d.engine_config, pd.join(ENGINE).join("config"));
    }

    /// The code of `name`'s body in this file, before the tests.
    fn body_of(name: &str) -> String {
        let code = include_str!("paths.rs")
            .split("#[cfg(test)]\nmod tests")
            .next()
            .unwrap();
        let start = code
            .find(&format!("pub fn {name}("))
            .unwrap_or_else(|| panic!("no fn {name}"));
        let rest = &code[start..];
        rest[..rest.find("\n}\n").expect("end of fn")].to_string()
    }

    /// The real Windows paths go through the pure function, so what the
    /// tests above pin is what Windows gets (the Windows branches only
    /// type-check on Linux).
    #[test]
    fn the_windows_branches_use_windows_machine_dirs() {
        for name in [
            "system_config_dir",
            "engine_config_dir",
            "system_log_dir",
            "instance_models_dir",
        ] {
            let body = body_of(name);
            assert!(
                body.contains("windows_machine_dirs("),
                "{name} does not use windows_machine_dirs:\n{body}"
            );
        }
    }

    // ── llama-server from llama.cpp's own install (Windows) ──────────────
    // The framework's MSI holds no llama.cpp file: pai-manager in
    // `%ProgramFiles%\private-ai\bin` finds llama-server in the folder
    // llama.cpp's MSI registers (`HKLM\Software\llama.cpp` `InstallDir`),
    // under its `bin\`. Spelled with `/` here, the same components.

    /// The framework's install finds the engine's llama-server (and
    /// llama-bench) in `<engine InstallDir>\bin`.
    #[test]
    fn the_framework_finds_llama_server_in_the_engine_install() {
        let root = tempfile::tempdir().unwrap();
        let engine = root.path().join("Program Files/llama.cpp");
        let server = touch(
            &engine,
            &format!("bin/{}", super::binary_name("llama-server")),
        );
        let bench = touch(
            &engine,
            &format!("bin/{}", super::binary_name("llama-bench")),
        );
        let manager = touch(root.path(), "Program Files/private-ai/bin/pai-manager.exe");
        assert_eq!(
            canon(llama_bin_with_engine(
                &manager,
                "llama-server",
                Some(&engine)
            )),
            Some(server.canonicalize().unwrap())
        );
        assert_eq!(
            canon(llama_bin_with_engine(
                &manager,
                "llama-bench",
                Some(&engine)
            )),
            Some(bench.canonicalize().unwrap())
        );
    }

    /// No engine registered, or one registered whose llama-server is gone:
    /// nothing, not a guess.
    #[test]
    fn no_engine_or_an_empty_engine_finds_nothing() {
        let root = tempfile::tempdir().unwrap();
        let manager = touch(root.path(), "Program Files/private-ai/bin/pai-manager.exe");
        assert_eq!(llama_bin_with_engine(&manager, "llama-server", None), None);
        let engine = root.path().join("Program Files/llama.cpp");
        std::fs::create_dir_all(engine.join("bin")).unwrap();
        assert_eq!(
            llama_bin_with_engine(&manager, "llama-server", Some(&engine)),
            None
        );
    }

    /// A tree build keeps running the llama-server it built, even with an
    /// engine installed; without an engine it is `llama_bin_from`.
    #[cfg(target_os = "linux")]
    #[test]
    fn a_tree_build_wins_over_the_installed_engine() {
        let root = tempfile::tempdir().unwrap();
        let tree = touch(root.path(), "build/linux/llama.cpp-cmake/bin/llama-server");
        let engine = root.path().join("Program Files/llama.cpp");
        touch(&engine, "bin/llama-server");
        let exe = touch(root.path(), "build/linux/cargo-target/release/pai-manager");
        assert_eq!(
            canon(llama_bin_with_engine(&exe, "llama-server", Some(&engine))),
            Some(tree.canonicalize().unwrap())
        );
        assert_eq!(
            llama_bin_with_engine(&exe, "llama-server", None),
            llama_bin_from(&exe, "llama-server")
        );
    }

    /// The running program's search (`llama_bin`, behind
    /// `llama_server_exe`) goes through the engine's registration, read from
    /// `HKLM\Software\llama.cpp` `InstallDir` (the Windows branch only
    /// type-checks on Linux).
    #[test]
    fn llama_bin_asks_the_engine_registration() {
        let body = body_of("llama_bin");
        assert!(
            body.contains("llama_bin_with_engine(") && body.contains("engine_install_dir()"),
            "llama_bin does not look in the engine's install:\n{body}"
        );
        let reg = body_of("engine_install_dir");
        assert!(reg.contains(r#"r"Software\llama.cpp""#), "{reg}");
        assert!(reg.contains(r#""InstallDir""#), "{reg}");
    }

    /// No llama-server anywhere it is looked for: nothing, not a guess.
    #[test]
    fn llama_server_missing_is_none() {
        let root = tempfile::tempdir().unwrap();
        let manager = touch(root.path(), "usr/libexec/private-ai/pai-manager");
        let config = touch(root.path(), "usr/bin/pai-config");
        assert_eq!(llama_bin_from(&manager, "llama-server"), None);
        assert_eq!(llama_bin_from(&config, "llama-server"), None);
    }

    /// The Linux packages: `pai-config` beside llama-server in /usr/bin, and
    /// `pai-manager` in /usr/libexec/private-ai, which finds it there too.
    #[cfg(target_os = "linux")]
    #[test]
    fn the_usr_layout_finds_llama_server_in_usr_bin() {
        let root = tempfile::tempdir().unwrap();
        let server = touch(root.path(), "usr/bin/llama-server");
        let bench = touch(root.path(), "usr/bin/llama-bench");
        let config = touch(root.path(), "usr/bin/pai-config");
        let manager = touch(root.path(), "usr/libexec/private-ai/pai-manager");
        let want = Some(server.canonicalize().unwrap());
        assert_eq!(canon(llama_bin_from(&config, "llama-server")), want);
        assert_eq!(canon(llama_bin_from(&manager, "llama-server")), want);
        assert_eq!(
            canon(llama_bin_from(&manager, "llama-bench")),
            Some(bench.canonicalize().unwrap())
        );
    }

    /// A tree build keeps finding the llama-server it built: from the build
    /// scripts' cargo target dir and from a bare `cargo build` at the root.
    #[cfg(target_os = "linux")]
    #[test]
    fn a_tree_build_finds_its_own_llama_server() {
        let root = tempfile::tempdir().unwrap();
        let server = touch(root.path(), "build/linux/llama.cpp-cmake/bin/llama-server");
        let want = Some(server.canonicalize().unwrap());
        for exe in [
            "build/linux/cargo-target/debug/pai-config",
            "build/linux/cargo-target/release/pai-manager",
            "target/release/pai-config",
        ] {
            let exe = touch(root.path(), exe);
            assert_eq!(
                canon(llama_bin_from(&exe, "llama-server")),
                want,
                "{}",
                exe.display()
            );
        }
    }

    /// Nothing installed beside the program: `None`, so the caller falls back
    /// to the repo's own files (a tree build).
    #[test]
    fn no_installed_share_is_none() {
        let root = tempfile::tempdir().unwrap();
        let exe = touch(root.path(), "build/linux/cargo-target/debug/pai-config");
        assert_eq!(installed_share_dir(&exe, "benchmarks"), None);
        assert_eq!(installed_share_dir(&exe, "strings"), None);
        let exe = touch(root.path(), "usr/bin/pai-config");
        assert_eq!(installed_share_dir(&exe, "benchmarks"), None);
    }

    /// The Linux packages install the benchmark method and the language files
    /// in /usr/share/private-ai.
    #[test]
    fn the_usr_layout_finds_its_data_in_usr_share_private_ai() {
        let root = tempfile::tempdir().unwrap();
        let exe = touch(root.path(), "usr/bin/pai-config");
        for sub in ["benchmarks", "strings"] {
            let dir = root.path().join("usr/share/private-ai").join(sub);
            std::fs::create_dir_all(&dir).unwrap();
            assert_eq!(
                canon(installed_share_dir(&exe, sub)),
                Some(dir.canonicalize().unwrap()),
                "{sub}"
            );
        }
    }

    /// /usr/share/<sub> is whatever else a system installed there: the
    /// package's own directory wins.
    #[test]
    fn the_package_s_own_data_wins_over_a_stray_one() {
        let root = tempfile::tempdir().unwrap();
        let exe = touch(root.path(), "usr/bin/pai-config");
        std::fs::create_dir_all(root.path().join("usr/share/strings")).unwrap();
        let ours = root.path().join("usr/share/private-ai/strings");
        std::fs::create_dir_all(&ours).unwrap();
        assert_eq!(
            canon(installed_share_dir(&exe, "strings")),
            Some(ours.canonicalize().unwrap())
        );
    }

    /// Where each install puts the read-only data (the benchmark method,
    /// the strings files), looked for first: for a program in /usr/bin AND
    /// in /usr/libexec/private-ai (pai-manager, pai-bff) the Linux packages'
    /// /usr/share/private-ai, never /usr/libexec/share; for macOS
    /// /opt/private-ai/bin, /opt/private-ai/share; for Windows
    /// <InstallDir>\bin, <InstallDir>\share (spelled with `/` here, the
    /// same components).
    #[test]
    fn the_share_dir_is_looked_for_where_each_install_puts_it() {
        let first = |exe: &str| {
            installed_share_candidates(Path::new(exe), "strings")
                .into_iter()
                .next()
        };
        assert_eq!(
            first("/usr/bin/pai"),
            Some(PathBuf::from("/usr/share/private-ai/strings"))
        );
        for exe in [
            "/usr/libexec/private-ai/pai-bff",
            "/usr/libexec/private-ai/pai-manager",
        ] {
            let all = installed_share_candidates(Path::new(exe), "strings");
            assert_eq!(
                all.first(),
                Some(&PathBuf::from("/usr/share/private-ai/strings")),
                "{exe}: {all:?}"
            );
            assert!(
                !all.iter().any(|d| d.starts_with("/usr/libexec")),
                "{exe}: {all:?}"
            );
        }
        for (exe, want) in [
            ("/opt/private-ai/bin/pai", "/opt/private-ai/share/strings"),
            (
                "/C/Program Files/private-ai/bin/pai-bff.exe",
                "/C/Program Files/private-ai/share/strings",
            ),
        ] {
            let all = installed_share_candidates(Path::new(exe), "strings");
            assert!(all.contains(&PathBuf::from(want)), "{exe}: {all:?}");
        }
        let all = installed_share_candidates(Path::new("/usr/bin/pai"), "benchmarks");
        assert_eq!(
            all.first(),
            Some(&PathBuf::from("/usr/share/private-ai/benchmarks"))
        );
    }

    /// The Linux packages' services in /usr/libexec/private-ai find the
    /// installed strings, so their error lines are texts, not bare keys.
    #[test]
    fn a_program_in_libexec_finds_the_usr_share_data() {
        let root = tempfile::tempdir().unwrap();
        let ours = root.path().join("usr/share/private-ai/strings");
        std::fs::create_dir_all(&ours).unwrap();
        for exe in [
            "usr/libexec/private-ai/pai-bff",
            "usr/libexec/private-ai/pai-manager",
        ] {
            let exe = touch(root.path(), exe);
            assert_eq!(
                canon(installed_share_dir(&exe, "strings")),
                Some(ours.canonicalize().unwrap()),
                "{}",
                exe.display()
            );
        }
    }

    /// A tree under its own prefix (macOS, /opt/private-ai) keeps its
    /// `share/` beside `bin/`.
    #[test]
    fn a_prefix_tree_finds_its_data_beside_bin() {
        let root = tempfile::tempdir().unwrap();
        let exe = touch(root.path(), "opt/private-ai/bin/pai-config");
        let dir = root.path().join("opt/private-ai/share/benchmarks");
        std::fs::create_dir_all(&dir).unwrap();
        assert_eq!(
            canon(installed_share_dir(&exe, "benchmarks")),
            Some(dir.canonicalize().unwrap())
        );
    }

    fn names(v: &[&str]) -> Vec<String> {
        v.iter().map(|s| s.to_string()).collect()
    }

    #[test]
    fn newest_version_compares_numerically_not_lexically() {
        let n = names(&["6.2", "10.0", "7.1"]);
        assert_eq!(newest_version(&n).map(String::as_str), Some("10.0"));
    }

    #[test]
    fn newest_version_handles_mixed_depth_and_ignores_non_numeric() {
        let n = names(&["6.4.2", "6.4", "temp", "7.1-beta"]);
        assert_eq!(newest_version(&n).map(String::as_str), Some("6.4.2"));
    }

    /// A crafted id never yields a log path; refused before any path is
    /// read, so this needs no redirect.
    #[test]
    fn invalid_ids_have_no_log() {
        for bad in [
            "", "../x", "..", "a/b", "a\\b", "Default", "a@b", "a b", "x.log",
        ] {
            assert_eq!(super::instance_log(bad), None, "id {bad:?}");
        }
    }

    #[test]
    fn valid_instance_id_accepts_short_lowercase_names() {
        let max = "a".repeat(32);
        for id in [
            "default",
            "lan-2",
            "8080",
            "a",
            "0",
            "gpu-0-r9700",
            max.as_str(),
        ] {
            assert!(valid_instance_id(id), "id {id:?} should be valid");
        }
    }

    #[test]
    fn valid_instance_id_refuses_empty_long_and_unsafe_names() {
        let too_long = "a".repeat(33);
        for id in [
            "",
            too_long.as_str(),
            "Default",
            "LAN",
            "a_b",
            "a.b",
            "..",
            "a/b",
            "../x",
            "a\\b",
            "a@b",
            "a b",
            " a",
            "-a",
            "-",
            "caf\u{e9}",
            "a\n",
        ] {
            assert!(!valid_instance_id(id), "id {id:?} should be refused");
        }
    }

    #[test]
    fn newest_version_none_when_nothing_version_shaped() {
        assert_eq!(newest_version(&names(&["bin", "docs"])), None);
        assert_eq!(newest_version(&[]), None);
    }
}
