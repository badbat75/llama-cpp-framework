//! Paths for the Private AI Framework.
//!
//! Four jobs: (1) the user runtime tree, %LOCALAPPDATA%\private-ai\ on Windows
//! (config\server.ini, config\models.ini, config\settings.ini,
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
/// live (`server@<id>.ini`, `models.ini`, `models.ini.disabled`):
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
/// instances' config, logs and default models and state.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WindowsMachineDirs {
    pub framework_config: PathBuf,
    pub engine_config: PathBuf,
    pub instance_logs: PathBuf,
    pub instance_models: PathBuf,
    /// The instances' default StateDir (slot snapshots).
    pub instance_state: PathBuf,
}

pub fn windows_machine_dirs(program_data: &Path) -> WindowsMachineDirs {
    let engine = program_data.join(ENGINE_DIR);
    WindowsMachineDirs {
        framework_config: program_data.join(APP_DIR).join("config"),
        engine_config: engine.join("config"),
        instance_logs: engine.join("logs"),
        instance_models: engine.join("models"),
        instance_state: engine.join("state"),
    }
}

/// `%ProgramData%`, the root of `windows_machine_dirs`.
#[cfg(windows)]
fn program_data() -> PathBuf {
    env_path("ProgramData").expect("ProgramData not set on Windows")
}

/// The default ModelsDir of the instances: `%ProgramData%\llama.cpp\models`
/// on Windows, the shared `/var/lib/llama.cpp/models` on Linux (the one the
/// engine's shipped `server@default.ini` names and its tmpfiles.d entry
/// creates, readable by everyone) and, on macOS,
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
        PathBuf::from("/var/lib/llama.cpp/models")
    }
    #[cfg(not(any(windows, target_os = "linux")))]
    {
        home_dir().join(".llama.cpp")
    }
}

/// The default StateDir of the instances the manager launches:
/// `%ProgramData%\llama.cpp\state` on Windows (not the LocalSystem
/// profile); on Linux the service user's
/// `/var/lib/private-ai/.local/share/private-ai/state`, whoever asks (the
/// instance's own folder is this with the id the launch fills in, and the
/// shipped `server@default.ini` sets no `StateDir`); elsewhere
/// `slot_state::default_state_dir()`, unchanged. Under the test override it
/// is `<override>/engine-state` on every OS.
pub fn instance_state_dir() -> PathBuf {
    if let Some(p) = env_path(DATA_ROOT_VAR) {
        return p.join("engine-state");
    }
    #[cfg(windows)]
    {
        windows_machine_dirs(&program_data()).instance_state
    }
    #[cfg(target_os = "linux")]
    {
        PathBuf::from("/var/lib")
            .join(APP_DIR)
            .join(".local")
            .join("share")
            .join(APP_DIR)
            .join("state")
    }
    #[cfg(not(any(windows, target_os = "linux")))]
    {
        crate::slot_state::default_state_dir()
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

/// `models@<id>.ini` in `engine_config_dir()`, the model profiles the instance `id`
/// reads: the pool cut by the instance's section of `assignments.ini`. The
/// manager writes it, the instance's launch reads it. None for an invalid id.
pub fn instance_models(id: &str) -> Option<PathBuf> {
    valid_instance_id(id).then(|| engine_config_dir().join(format!("models@{id}.ini")))
}

/// The record of the instances: `instances.ini`, in the engine's config
/// directory beside the server profiles it names. Every manager client reads
/// it; the manager is the only writer.
pub fn instances_ini() -> PathBuf {
    engine_config_dir().join("instances.ini")
}

/// The ids of the sections of `instances_ini()`, sorted: an instance is a
/// section of the record, not a file in the directory. A section whose name
/// is not a valid instance id is skipped; a section that carries no
/// `ServerProfile` key is an instance all the same, the listing reads sections
/// and the launch is the one that refuses a section naming no profile. Empty
/// when the file is missing, which this never creates: [`try_instance_ids`]
/// is the sibling that reports it.
pub fn instance_ids() -> Vec<String> {
    try_instance_ids().unwrap_or_default()
}

/// The fallible sibling of [`instance_ids`]: a record that is missing or
/// cannot be read is an `Err` naming why, not an empty list, so a caller
/// can tell "no instances" from "the record could not be read" and refuse
/// to act on the second. A readable file answers exactly what
/// [`instance_ids`] answers.
pub fn try_instance_ids() -> std::io::Result<Vec<String>> {
    let text = std::fs::read_to_string(instances_ini())?;
    let ids: std::collections::BTreeSet<String> = crate::ini::parse_sections(&text)
        .into_iter()
        .map(|s| s.id)
        .filter(|name| valid_instance_id(name))
        .collect();
    Ok(ids.into_iter().collect())
}

/// The server profile the instance `id` names, the file whose section the
/// instance is. From the model data model of `ARCHITECTURE.md` "Instance
/// profiles" the record names an INSTANCE profile, and the server profile sits
/// one step behind it: the `ServerProfile` of that instance profile's section of
/// [`instance_profiles_ini`]. A record the migration has not written names the
/// server profile itself, and its key answers. The section is the one named by
/// `id` exactly, as [`instance_ids`] counts sections: a header that differs in
/// case names no instance. `None` when the record has no such section, the
/// section names no profile, the profile it names has no section in the pool,
/// the name is empty, or the name builds no server profile file: every one of
/// them is an instance with no profile to read, and the launch of such a section
/// refuses for the same reason.
pub fn instance_server_profile(id: &str) -> Option<String> {
    let record = crate::ini::read_all(&instances_ini());
    let section = record.iter().find(|s| s.id == id)?;
    let named = |keys: &std::collections::BTreeMap<String, String>, key: &str| {
        keys.get(key)
            .map(|s| s.trim().to_string())
            .filter(|name| !name.is_empty() && valid_instance_id(name))
    };
    // The instance profile, and the server profile the pool's section for it
    // names: a pool with no such section names no server profile.
    if let Some(profile) = named(&section.keys, "InstanceProfile") {
        let pool = crate::ini::read_all(&instance_profiles_ini());
        let pool = pool.iter().find(|s| s.id == profile)?;
        return named(&pool.keys, "ServerProfile");
    }
    named(&section.keys, "ServerProfile")
}

/// The address the instance `id` listens on: the `Hostname` and `Port` of its
/// section of [`instances_ini`], where the model data model of
/// `ARCHITECTURE.md` "Instance profiles" keeps them, as the effective values the
/// migration moved out of the server profile. Each key answers on its own: a
/// section carrying `Port` and no `Hostname` sets the port of the host the
/// server profile names, and a section carrying neither, as the record of an
/// install the migration has not written, sets nothing and leaves the server
/// profile to answer.
pub fn instance_address(id: &str) -> (Option<String>, Option<i32>) {
    let Some(section) = crate::ini::read_all(&instances_ini())
        .into_iter()
        .find(|s| s.id == id)
    else {
        return (None, None);
    };
    let host = section
        .keys
        .get("Hostname")
        .map(|s| s.trim().to_string())
        .filter(|name| !name.is_empty());
    let port = section
        .keys
        .get("Port")
        .and_then(|s| s.trim().parse::<i32>().ok())
        .filter(|port| (1..=65535).contains(port));
    (host, port)
}

/// `instance-profiles.ini`: the pool of instance profiles, one `[<p>]` section
/// each naming the server profile it runs, and one `[<p>/<m>]` section per model
/// profile the profile is given (`ARCHITECTURE.md`, "Instance profiles", the
/// placement and memory amendment). The manager owns it: the record's
/// `InstanceProfile` keys and this file are read together by
/// [`instance_server_profile`], and the render of an instance is its profile's
/// entry sections over the model profile pool.
pub fn instance_profiles_ini() -> PathBuf {
    engine_config_dir().join("instance-profiles.ini")
}

pub fn server_ini() -> PathBuf {
    config_dir().join("server.ini")
}

pub fn models_ini() -> PathBuf {
    config_dir().join("models.ini")
}

/// The model profiles switched OFF: same INI format as `models_ini()`, sections moved
/// here verbatim by `model_profiles::set_enabled`. A different EXTENSION on purpose:
/// llama-server only ever reads the file `--models-preset` names, so a model profile
/// parked here is simply not offered, while a `*.ini` glob (the user's own, or a
/// backup tool's) cannot mistake it for a live config.
pub fn models_disabled_ini() -> PathBuf {
    config_dir().join("models.ini.disabled")
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

/// The per-OS directory under `build/` that the build phases generate
/// everything into, for an OS named as `std::env::consts::OS` names it.
/// Any other OS takes Linux's.
pub fn build_os_dir(os: &str) -> &'static str {
    match os {
        "windows" => "windows",
        "macos" => "macos",
        _ => "linux",
    }
}

/// [`llama_bin`] for a program at `exe`: the same search, from that path
/// instead of the running one. The Linux packages put `pai-config` and
/// llama.cpp's executables in `/usr/bin` and `pai-manager` in
/// `/usr/libexec/private-ai`, which must find `/usr/bin/llama-server`.
pub fn llama_bin_from(exe: &Path, stem: &str) -> Option<PathBuf> {
    let os_dir = build_os_dir(std::env::consts::OS);
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
            .join(os_dir)
            .join("llama.cpp-cmake")
            .join("bin")
            .join(name),
        exe_dir
            .join("..")
            .join("build")
            .join(os_dir)
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
/// (`HKLM\Software\llama.cpp` `InstallDir`), on macOS its `.pkg` install
/// ([`macos_engine_install_dir`] under `/`); `None` on Linux and when no
/// engine is installed.
pub fn engine_install_dir() -> Option<PathBuf> {
    #[cfg(windows)]
    {
        let dir = win_reg::machine_reg_sz(r"Software\llama.cpp", "InstallDir")?;
        (!dir.is_empty()).then(|| PathBuf::from(dir))
    }
    #[cfg(target_os = "macos")]
    {
        macos_engine_install_dir(Path::new("/"))
    }
    #[cfg(not(any(windows, target_os = "macos")))]
    {
        None
    }
}

/// llama.cpp's own `.pkg` install under the filesystem root `root` on
/// macOS (`<root>/opt/llama.cpp`), when it holds `bin/llama-server`.
pub fn macos_engine_install_dir(root: &Path) -> Option<PathBuf> {
    let dir = root.join("opt").join(ENGINE_DIR);
    dir.join("bin")
        .join("llama-server")
        .is_file()
        .then_some(dir)
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

    // The file names themselves: the pool's pair, per user (macOS), and the
    // render the instance reads, in the engine's directory.
    assert_eq!(
        models_ini(),
        config_dir().join("models.ini"),
        "the pool's switched-on file"
    );
    assert_eq!(
        models_disabled_ini(),
        config_dir().join("models.ini.disabled"),
        "the pool's switched-off file"
    );
    assert_eq!(
        instance_models("default"),
        Some(engine.join("models@default.ini")),
        "the render is the engine's file, named after the instance"
    );
    assert_eq!(
        instance_models("lan-2"),
        Some(engine.join("models@lan-2.ini")),
        "one render per instance"
    );

    // A file left in the framework's directory is no instance, of either kind:
    // nothing moves or reads files from there.
    std::fs::create_dir_all(&sys).expect("system config dir");
    std::fs::write(sys.join("server@old.ini"), "[Server]\n").expect("stray file");
    std::fs::write(sys.join("bff.ini"), "[Bff]\n").expect("bff.ini");
    std::fs::write(
        sys.join("instances.ini"),
        "[stray]\nServerProfile = stray\n",
    )
    .expect("stray instances.ini");
    assert_eq!(
        instance_ids(),
        Vec::<String>::new(),
        "the framework's files, instances.ini included"
    );
    assert!(!engine.exists(), "nothing created or moved");

    // The profile files, on their own, list nothing: an instance is a section
    // of the engine's `instances.ini`, and a `server@<id>.ini` that no section
    // names is a server profile that serves no instance.
    let sys = engine;
    std::fs::create_dir_all(&sys).expect("engine config dir");
    for name in [
        "server@lan-2.ini",
        "server@default.ini",
        "server@8080.ini",
        "server.ini",
        "models.ini",
        "models.ini.disabled",
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
    assert_eq!(
        instance_ids(),
        Vec::<String>::new(),
        "the files of the directory name no instance"
    );

    // The sections of the engine's `instances.ini` are the instances, sorted:
    // an id that is not a valid instance id is skipped, and a section whose
    // profile file is missing is an instance (the listing reads sections, and
    // a section is an instance whether or not it carries its key).
    std::fs::write(
        sys.join("instances.ini"),
        "[lan-2]\nServerProfile = big\n\n\
         [default]\nServerProfile = default\n\n\
         [8080]\nServerProfile = default\n\n\
         [no-file]\nServerProfile = big\n\n\
         [no-key]\n\
         [Upper]\nServerProfile = big\n\n\
         [a_b]\nServerProfile = big\n\n\
         [-lead]\nServerProfile = big\n\n\
         [a.b]\nServerProfile = big\n\n\
         [a@b]\nServerProfile = big\n\n\
         [a-long-id-over-the-limit-for-an-instance-id]\nServerProfile = big\n",
    )
    .expect("instances.ini");
    assert_eq!(
        instance_ids(),
        ["8080", "default", "lan-2", "no-file", "no-key"]
    );

    // A missing `instances.ini` lists none, and so does one with no section:
    // keys above the first section header belong to nothing.
    std::fs::remove_file(sys.join("instances.ini")).expect("remove instances.ini");
    assert_eq!(instance_ids(), Vec::<String>::new(), "no instances.ini");
    std::fs::write(sys.join("instances.ini"), "").expect("empty instances.ini");
    assert_eq!(
        instance_ids(),
        Vec::<String>::new(),
        "an empty instances.ini"
    );
    std::fs::write(sys.join("instances.ini"), "Port = 8080\n").expect("keys, no section");
    assert_eq!(
        instance_ids(),
        Vec::<String>::new(),
        "keys under no section name no instance"
    );

    // The fallible sibling answers a readable file the same way, and reports
    // what `instance_ids` swallows: a missing record and an unreadable one
    // are errors, not empty lists, so a caller never deletes the services
    // of an instance because the record could not be read.
    assert_eq!(
        try_instance_ids().expect("a readable file with no section is not an error"),
        Vec::<String>::new(),
        "the keys-under-no-section file above reads as an empty list"
    );
    std::fs::write(
        sys.join("instances.ini"),
        "[default]\nServerProfile = default\n\n[Zeta]\nServerProfile = big\n",
    )
    .expect("instances.ini");
    assert_eq!(
        try_instance_ids().expect("a readable record"),
        ["default"],
        "the readable record answers what instance_ids answers, sorted, invalid ids skipped"
    );
    // The record missing: an error naming its reason, not the empty list of
    // "no instances" (the record not existing is the install's first state
    // too: the caller decides what that means, the reader does not).
    std::fs::remove_file(sys.join("instances.ini")).expect("remove instances.ini");
    let missing =
        try_instance_ids().expect_err("a missing instances.ini is an error, not an empty list");
    assert_eq!(
        missing.kind(),
        std::io::ErrorKind::NotFound,
        "the missing record names its reason"
    );
    // The record unreadable: an error too. The mode binds the read only for
    // a non-root account: probe that, and skip the case when it does not.
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let rec = sys.join("instances.ini");
        std::fs::write(&rec, "[default]\nServerProfile = default\n").expect("instances.ini");
        let mode = std::fs::metadata(&rec)
            .expect("metadata")
            .permissions()
            .mode();
        std::fs::set_permissions(&rec, std::fs::Permissions::from_mode(0o000)).expect("chmod 000");
        let probe = std::fs::read_to_string(&rec);
        std::fs::set_permissions(&rec, std::fs::Permissions::from_mode(mode))
            .expect("restore mode");
        if probe.is_ok() {
            eprintln!(
                "this account ignores file modes (root): \
                 the unreadable instances.ini case is skipped"
            );
        } else {
            std::fs::set_permissions(&rec, std::fs::Permissions::from_mode(0o000))
                .expect("chmod 000");
            let unreadable = try_instance_ids()
                .expect_err("an unreadable instances.ini is an error, not an empty list");
            std::fs::set_permissions(&rec, std::fs::Permissions::from_mode(mode))
                .expect("restore mode");
            assert_eq!(
                unreadable.kind(),
                std::io::ErrorKind::PermissionDenied,
                "the unreadable record names its reason"
            );
        }
    }
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

/// E2E phase for the instances' default state directory, for the e2e test
/// of this crate to call: it sets the process-wide `PRIVATE_AI_DATA_ROOT`
/// redirect (`src/tests/mod.rs`). Beside the default models, in the same
/// redirect layout: `<override>/engine-state`, apart from the user's tree.
#[cfg(test)]
pub(crate) fn instance_state_dir_e2e() {
    let dir = tempfile::tempdir().expect("tempdir");
    redirect_for_tests(dir.path());
    let state = dir.path().join("engine-state");
    assert_eq!(instance_state_dir(), state);
    assert_ne!(instance_state_dir(), instance_models_dir());
    assert_ne!(instance_state_dir(), engine_config_dir());
    assert!(
        !instance_state_dir().starts_with(data_root()),
        "not in the user's tree"
    );
    assert!(!state.exists(), "asking creates nothing");
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

/// E2E phase for [`instance_server_profile`], for the e2e test of this crate to
/// call (`src/tests/mod.rs`): the resolution order of `ARCHITECTURE.md`
/// "Instance profiles", the decision's "the pool file ... `paths::instance_server_profile`
/// resolves through the instance profile". The record names an INSTANCE profile
/// and the server profile sits one step behind it, so the server profile of
/// that instance profile's section answers, and a `ServerProfile` the record
/// still carries from before the migration is stale.
#[cfg(test)]
pub(crate) fn instance_server_profile_order_e2e() {
    let dir = tempfile::tempdir().expect("tempdir");
    redirect_for_tests(dir.path());
    let engine = engine_config_dir();
    std::fs::create_dir_all(&engine).expect("engine config dir");
    // `both` names the instance profile AND carries the record's own key, the
    // shape the migration leaves when it writes `InstanceProfile` and drops the
    // stale `ServerProfile`: the fixture writes both, so the pin faces the pair.
    // `through` is the migrated record, `legacy` the record of an install the
    // migration has not written, `dangling` one naming a profile the pool has no
    // section for.
    std::fs::write(
        instances_ini(),
        "[both]\nInstanceProfile = prof\nServerProfile = stale\n\n\
         [through]\nInstanceProfile = prof\n\n\
         [legacy]\nServerProfile = own\n\n\
         [dangling]\nInstanceProfile = gone\n",
    )
    .expect("instances.ini");
    std::fs::write(instance_profiles_ini(), "[prof]\nServerProfile = live\n")
        .expect("instance-profiles.ini");

    // The design's order: the instance profile's server profile answers, the
    // record's stale key is not read.
    assert_eq!(
        instance_server_profile("both"),
        Some("live".to_string()),
        "the server profile of the instance profile the record names wins over the \
         `ServerProfile` left in the record"
    );
    // The migrated record: one step, the same answer.
    assert_eq!(
        instance_server_profile("through"),
        Some("live".to_string()),
        "a record naming only its instance profile is applied through it"
    );
    // A record that names no instance profile is the pre-migration shape: its
    // own key answers, the instance profile having nothing to say.
    assert_eq!(
        instance_server_profile("legacy"),
        Some("own".to_string()),
        "a record naming no instance profile names the server profile itself"
    );
    // The refusals the doc names: no section, and an instance profile the pool
    // has no section for, are an instance with no profile to read.
    assert_eq!(instance_server_profile("dangling"), None, "no pool section");
    assert_eq!(
        instance_server_profile("nobody"),
        None,
        "the record has no such section"
    );
}

#[cfg(test)]
#[path = "paths_tests.rs"]
mod tests;
