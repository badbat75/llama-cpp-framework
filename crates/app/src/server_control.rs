//! Starting, stopping and probing the `default` llama-server instance, behind
//! one seam with two backends: `Direct` (the configurator spawns and kills the
//! process itself, `runstate`) and `Manager` (the manager service runs it, asked
//! through `pai_core::client`). `backend()` picks the one of this OS.

use crate::server_cfg::ServerConfig;
use crate::strings;
use pai_core::client::{ClientError, InstanceState, ManagerClient};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

/// The instance every caller of this seam acts on.
pub const INSTANCE: &str = "default";

/// A connected manager client, shared by the GUI's worker threads.
pub type Client = Box<dyn ManagerClient + Send + Sync>;

/// How the `Manager` backend connects, called on its first use.
pub type Connect = Box<dyn Fn() -> Result<Client, ClientError> + Send + Sync>;

/// Who runs llama-server.
pub enum Backend {
    /// The configurator itself: `runstate`. Built on macOS only (`backend_kind`).
    Direct,
    /// The manager service.
    Manager(ManagerBackend),
}

/// The `Manager` backend: a client connected lazily.
pub struct ManagerBackend {
    connect: Connect,
    /// The client once a connect has succeeded; shared out by `Arc` so a long
    /// call (a stop waiting for the exit) does not hold the lock other
    /// threads' probes need.
    client: Mutex<Option<Arc<Client>>>,
    /// Whether the manager has withheld the API key from a read of the
    /// server file in this process: sticky, so a later full read (polkit's
    /// cached authorization) never clears it.
    withheld: AtomicBool,
}

impl ManagerBackend {
    /// A backend that connects with `connect` on its first use, not now.
    pub fn new(connect: Connect) -> ManagerBackend {
        ManagerBackend {
            connect,
            client: Mutex::new(None),
            withheld: AtomicBool::new(false),
        }
    }

    /// The connected client, connecting now if no connect has succeeded yet.
    fn client(&self) -> Result<Arc<Client>, ClientError> {
        let mut slot = self.client.lock().unwrap_or_else(|e| e.into_inner());
        if let Some(c) = slot.as_ref() {
            return Ok(c.clone());
        }
        let c = Arc::new((self.connect)()?);
        *slot = Some(c.clone());
        Ok(c)
    }

    fn is_running(&self) -> bool {
        self.client()
            .and_then(|c| c.status(INSTANCE))
            .is_ok_and(|s| {
                matches!(
                    s,
                    InstanceState::Active | InstanceState::Activating | InstanceState::Deactivating
                )
            })
    }

    fn start(&self) -> Result<Option<ServerConfig>, ClientError> {
        if self.is_running() {
            return Ok(None);
        }
        self.client()?.start(INSTANCE)?;
        Ok(Some(self.server_config()))
    }

    fn stop(&self) -> Result<bool, ClientError> {
        self.client()?.stop(INSTANCE)?;
        // The manager queues the stop: wait for the unit to be gone, as
        // `runstate::stop` waits for the process.
        Ok(crate::runstate::wait_gone(
            || self.is_running(),
            crate::runstate::STOP_GRACE,
            STOP_POLL,
        ))
    }

    fn restart(&self) -> Result<Option<ServerConfig>, ClientError> {
        self.client()?.restart(INSTANCE)?;
        Ok(Some(self.server_config()))
    }

    /// The instance's config as the manager holds it; defaults when it has
    /// none or cannot be read, as `server_cfg::load` reads a missing file.
    fn server_config(&self) -> ServerConfig {
        let text = self.read_config(ConfigFile::Server).ok().flatten();
        crate::server_cfg::from_text(&text.unwrap_or_default())
    }

    /// The manager's name of `file`: the model profiles pair keeps its name,
    /// the server file is the profile the record names for `INSTANCE`,
    /// `server@<server_profile>.ini`. `Instances()` is the only source of the
    /// name: a record that cannot be read, and an `INSTANCE` the record does
    /// not list, answer the manager's own error, never a fall back to the file
    /// named after the id, which the instance does not read.
    fn manager_name(&self, file: ConfigFile) -> Result<String, ClientError> {
        if file != ConfigFile::Server {
            return Ok(file.manager_name());
        }
        let profile = self
            .client()?
            .instances()?
            .into_iter()
            .find(|i| i.id == INSTANCE)
            .and_then(|i| i.server_profile)
            .ok_or(ClientError::NoInstance)?;
        Ok(format!("server@{profile}.ini"))
    }

    /// The name to show the user, in a path: the profile's when the record
    /// answers, the id's when it cannot be read. A name to show, never one to
    /// read or write.
    fn display_name(&self, file: ConfigFile) -> String {
        self.manager_name(file)
            .unwrap_or_else(|_| file.manager_name())
    }

    fn read_config(&self, file: ConfigFile) -> Result<Option<String>, ClientError> {
        let read = self.client()?.read_file(&self.manager_name(file)?)?;
        if file == ConfigFile::Server && read.as_ref().is_some_and(|f| f.withheld) {
            self.withheld.store(true, Ordering::Relaxed);
        }
        Ok(read.map(|f| f.text))
    }

    /// A server file written with no key after a withheld read asks the
    /// manager to keep the key it holds.
    fn write_config(&self, file: ConfigFile, contents: &str) -> Result<(), ClientError> {
        let keep = file == ConfigFile::Server
            && self.withheld.load(Ordering::Relaxed)
            && crate::server_cfg::from_text(contents)
                .opencode_api_key
                .is_none();
        self.client()?
            .write_file(&self.manager_name(file)?, contents, keep)
    }
}

/// How often a stop asks whether the instance is gone.
const STOP_POLL: Duration = Duration::from_millis(250);

/// A configuration file of the instance: read and written only through this
/// seam, from the manager on Linux and from the per-user files on `Direct`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ConfigFile {
    /// The instance's server profile file, `server@<server_profile>.ini`, the
    /// profile `Instances()` names (`server.ini` per user).
    Server,
    /// `models.ini`.
    Models,
    /// `models.ini.disabled`.
    ModelsDisabled,
}

impl ConfigFile {
    #[cfg(any(test, feature = "test-support"))]
    pub const ALL: [ConfigFile; 3] = [
        ConfigFile::Server,
        ConfigFile::Models,
        ConfigFile::ModelsDisabled,
    ];

    /// The file's name among the manager's files, named after the instance's
    /// id. The server file is the profile the record names for `INSTANCE`,
    /// which is what the seam reads and writes
    /// ([`ManagerBackend::manager_name`]).
    pub fn manager_name(self) -> String {
        match self {
            ConfigFile::Server => format!("server@{INSTANCE}.ini"),
            ConfigFile::Models => "models.ini".into(),
            ConfigFile::ModelsDisabled => "models.ini.disabled".into(),
        }
    }

    /// The per-user file `Direct` reads and writes.
    fn user_path(self) -> PathBuf {
        match self {
            ConfigFile::Server => crate::paths::server_ini(),
            ConfigFile::Models => crate::paths::models_ini(),
            ConfigFile::ModelsDisabled => crate::paths::models_disabled_ini(),
        }
    }
}

impl Backend {
    /// Start the instance: `Ok(Some(cfg))` with the config it was started
    /// with, `Ok(None)` when it was already running, `Err` with the text to show.
    pub fn start(&self) -> Result<Option<ServerConfig>, String> {
        match self {
            Backend::Direct => crate::runstate::start().map_err(|e| e.to_string()),
            Backend::Manager(m) => m.start().map_err(|e| e.text()),
        }
    }

    /// Stop the instance: `Ok(true)` once it is gone, `Ok(false)` when it is
    /// still running after the grace, `Err` with the text to show.
    pub fn stop(&self) -> Result<bool, String> {
        match self {
            Backend::Direct => Ok(crate::runstate::stop()),
            Backend::Manager(m) => m.stop().map_err(|e| e.text()),
        }
    }

    /// Stop and start again; what `start` returns.
    pub fn restart(&self) -> Result<Option<ServerConfig>, String> {
        match self {
            Backend::Direct => {
                // `stop()` returns only once the old router and its children
                // are gone: starting earlier saw the old process and launched
                // nothing.
                if !crate::runstate::stop() {
                    return Err(strings::fill(
                        "old-server-still-running",
                        &[&crate::runstate::STOP_GRACE.as_secs()],
                    ));
                }
                std::thread::sleep(Duration::from_millis(1000));
                crate::runstate::start().map_err(|e| e.to_string())
            }
            Backend::Manager(m) => m.restart().map_err(|e| e.text()),
        }
    }

    /// Whether a llama-server of the instance is there.
    pub fn is_running(&self) -> bool {
        match self {
            Backend::Direct => crate::runstate::is_running(),
            Backend::Manager(m) => m.is_running(),
        }
    }

    /// The file's text, `Ok(None)` when it does not exist yet, `Err` with the
    /// text to show.
    pub fn read_config(&self, file: ConfigFile) -> Result<Option<String>, String> {
        match self {
            Backend::Direct => match std::fs::read_to_string(file.user_path()) {
                Ok(text) => Ok(Some(text)),
                Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(None),
                Err(e) => Err(e.to_string()),
            },
            Backend::Manager(m) => m.read_config(file).map_err(|e| e.text()),
        }
    }

    /// Replace the file with `contents`; `Err` with the text to show (a
    /// refusal of the manager in the user's language).
    pub fn write_config(&self, file: ConfigFile, contents: &str) -> Result<(), String> {
        match self {
            Backend::Direct => {
                let path = file.user_path();
                if let Some(parent) = path.parent() {
                    std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
                }
                crate::ini::atomic_write(&path, contents).map_err(|e| e.to_string())
            }
            Backend::Manager(m) => m.write_config(file, contents).map_err(|e| e.text()),
        }
    }

    /// Whether the manager has once withheld `OpencodeApiKey` in this
    /// process, from a read of the server file (the caller may not
    /// configure); a server write with no key then asks the manager to keep
    /// its own, even after a later full read.
    pub fn api_key_withheld(&self) -> bool {
        match self {
            Backend::Direct => false,
            Backend::Manager(m) => m.withheld.load(Ordering::Relaxed),
        }
    }

    /// Where the file is, to show the user.
    pub fn config_path(&self, file: ConfigFile) -> PathBuf {
        match self {
            Backend::Direct => file.user_path(),
            Backend::Manager(m) => crate::paths::engine_config_dir().join(m.display_name(file)),
        }
    }
}

impl Backend {
    /// Whether the instance starts when the system starts, as the manager
    /// reads it from the service manager; `Err` with the text to show.
    /// `Direct` has no service manager to ask: no manager is there.
    pub fn boot_enabled(&self) -> Result<bool, String> {
        match self {
            Backend::Direct => Err(ClientError::Unavailable.text()),
            Backend::Manager(m) => m
                .client()
                .and_then(|c| c.boot_enabled(INSTANCE))
                .map_err(|e| e.text()),
        }
    }

    /// Make the instance start when the system starts, or not; `Err` with
    /// the text to show (a refusal of the manager in the user's language).
    pub fn set_boot_enabled(&self, enabled: bool) -> Result<(), String> {
        match self {
            Backend::Direct => Err(ClientError::Unavailable.text()),
            Backend::Manager(m) => m
                .client()
                .and_then(|c| c.set_boot_enabled(INSTANCE, enabled))
                .map_err(|e| e.text()),
        }
    }
}

/// What the Settings tab shows after a click on "start llama-server when
/// the system starts".
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BootToggle {
    /// The toggle's state: the one asked for, or the manager's after a
    /// refusal.
    pub enabled: bool,
    /// The footer's message.
    pub message: String,
    /// Whether the message is an error.
    pub error: bool,
}

/// Ask `b` for the start at boot to be `want`, what the toggle then shows.
/// A refusal puts the toggle back to the manager's state, read again, or
/// to where it was before the click when that read fails too.
pub fn toggle_boot(b: &Backend, want: bool) -> BootToggle {
    match b.set_boot_enabled(want) {
        Ok(()) => BootToggle {
            enabled: want,
            message: if want {
                strings::t("start-at-boot-on")
            } else {
                strings::t("start-at-boot-off")
            },
            error: false,
        },
        Err(message) => BootToggle {
            enabled: b.boot_enabled().unwrap_or(!want),
            message,
            error: true,
        },
    }
}

/// Whether pai-config starts llama-server when it launches, under backend
/// `kind` with settings.ini's `StartServerOnLaunch` at `setting`.
/// Under the manager the OS starts the instance at boot, so never.
pub fn start_at_launch(kind: Kind, setting: bool) -> bool {
    kind == Kind::Direct && setting
}

/// Whether a save leaves an unset ModelsDir and StateDir unset in the file
/// (`server_cfg::render_for`) under `kind` on `os`: only the manager on
/// Windows and Linux, whose instances then use the engine's folders
/// (`launch::with_instance_defaults`: under `%ProgramData%\llama.cpp` on
/// Windows, the service user's home on Linux), not the saving user's home,
/// which their accounts cannot read. Elsewhere both are written resolved,
/// as before.
pub fn dirs_left_unset(kind: Kind, os: Os) -> bool {
    kind == Kind::Manager && matches!(os, Os::Windows | Os::Linux)
}

/// Where the instance's log is read from.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LogSource {
    /// A file: the per-user one `Direct` writes (`paths::server_log`), rotated
    /// by `runstate` (`LogRotate`, `LogRotateKb`), or the Windows service's
    /// (`paths::instance_log`).
    File(PathBuf),
    /// The journal of the instance's systemd unit, read with `journalctl`;
    /// journald rotates it.
    Journal { unit: String },
}

impl Backend {
    /// Which backend this is, without its connection.
    pub fn kind(&self) -> Kind {
        match self {
            Backend::Direct => Kind::Direct,
            Backend::Manager(_) => Kind::Manager,
        }
    }

    /// Where the Log window reads the instance's log: `log_source_for` this
    /// backend's kind on this OS (no call to the manager).
    pub fn log_source(&self) -> LogSource {
        log_source_for(self.kind(), Os::current())
    }

    /// Whether `LogRotate` and `LogRotateKb` apply: only on `Direct`; under the
    /// manager journald or the service handles the log.
    pub fn rotates_log_file(&self) -> bool {
        matches!(self, Backend::Direct)
    }
}

/// The systemd unit of the instance, as the manager names it.
pub fn instance_unit() -> String {
    format!("llama.cpp@{INSTANCE}.service")
}

/// The arguments after `journalctl` that follow `unit`'s journal, starting
/// with its last `lines` entries.
pub fn journalctl_args(unit: &str, lines: usize) -> Vec<String> {
    [
        "-u",
        unit,
        "-f",
        "-n",
        &lines.to_string(),
        "-o",
        "cat",
        "--no-pager",
    ]
    .map(String::from)
    .to_vec()
}

/// `journalctl` with `args`, stdin closed and both output streams piped; not
/// spawned.
pub fn journalctl_command(args: &[String]) -> std::process::Command {
    use std::process::Stdio;
    let mut cmd = std::process::Command::new("journalctl");
    cmd.args(args)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    cmd
}

/// Spawns the journal reader with these arguments; the child's stdout and
/// stderr are piped.
pub type JournalSpawn = dyn Fn(&[String]) -> std::io::Result<std::process::Child> + Send + Sync;

/// The spawner the Log window uses: `journalctl_command(args).spawn()`. In a
/// test build one that spawns nothing and fails, so no test reads the real
/// journal.
pub fn journal_spawner() -> &'static JournalSpawn {
    #[cfg(any(test, feature = "test-support"))]
    return &|_: &[String]| {
        Err(std::io::Error::new(
            std::io::ErrorKind::Unsupported,
            "a test build reads no journal",
        ))
    };
    #[cfg(not(any(test, feature = "test-support")))]
    &|args: &[String]| journalctl_command(args).spawn()
}

/// Whether journalctl's stderr says the user may not read the system journal.
pub fn journal_access_denied(stderr: &str) -> bool {
    stderr.contains("Hint: You are currently not seeing messages from")
        || stderr.contains("No journal files were opened due to insufficient permissions")
}

/// Runs the journal reader once with these arguments, to its exit, and
/// returns what it printed (`journalctl_command(args).output()`).
pub type JournalRun = dyn Fn(&[String]) -> std::io::Result<std::process::Output> + Send + Sync;

/// The one-shot runner `Backend::log_text` uses. In a test build one that
/// runs nothing and fails, so no test reads the real journal.
pub fn journal_runner() -> &'static JournalRun {
    #[cfg(any(test, feature = "test-support"))]
    return &|_: &[String]| {
        Err(std::io::Error::new(
            std::io::ErrorKind::Unsupported,
            "a test build reads no journal",
        ))
    };
    #[cfg(not(any(test, feature = "test-support")))]
    &|args: &[String]| journalctl_command(args).output()
}

/// The arguments after `journalctl` that print `unit`'s last `lines` entries
/// once (no `-f`), only those on or after `since` when given.
pub fn journalctl_once_args(
    unit: &str,
    lines: usize,
    since: Option<std::time::SystemTime>,
) -> Vec<String> {
    let mut args = [
        "-u",
        unit,
        "-o",
        "cat",
        "--no-pager",
        "-n",
        &lines.to_string(),
    ]
    .map(String::from)
    .to_vec();
    if let Some(since) = since {
        let secs = since
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();
        args.push("--since".into());
        args.push(format!("@{secs}"));
    }
    args
}

/// The end of `bytes`, at most `max` of them, starting at a line start.
fn tail_lines(bytes: &[u8], max: usize) -> &[u8] {
    if bytes.len() <= max {
        return bytes;
    }
    let start = bytes.len() - max;
    if bytes[start - 1] == b'\n' {
        return &bytes[start..];
    }
    match bytes[start..].iter().position(|&b| b == b'\n') {
        Some(i) => &bytes[start + i + 1..],
        None => &[],
    }
}

/// The last `max_bytes` (at most) of the log `source` holds, from a line
/// start; a journal is read once through `run`.
pub fn read_log(
    source: &LogSource,
    since: Option<std::time::SystemTime>,
    max_bytes: usize,
    run: &JournalRun,
) -> Result<String, String> {
    match source {
        LogSource::File(path) => read_log_file(path, since, max_bytes).map_err(|e| e.to_string()),
        LogSource::Journal { unit } => {
            let out = run(&journalctl_once_args(unit, max_bytes, since))
                .map_err(|e| strings::fill("log-journal-failed", &[&e]))?;
            let stderr = String::from_utf8_lossy(&out.stderr);
            if journal_access_denied(&stderr) {
                return Err(strings::t("log-journal-denied"));
            }
            if !out.status.success() {
                let why = stderr
                    .lines()
                    .map(str::trim)
                    .rfind(|l| !l.is_empty())
                    .map(str::to_string)
                    .unwrap_or_else(|| out.status.to_string());
                return Err(strings::fill("log-journal-failed", &[&why]));
            }
            Ok(String::from_utf8_lossy(tail_lines(&out.stdout, max_bytes)).into_owned())
        }
    }
}

/// `read_log` of a file: missing, or last written before `since`, is empty.
fn read_log_file(
    path: &Path,
    since: Option<std::time::SystemTime>,
    max_bytes: usize,
) -> std::io::Result<String> {
    use std::io::{Read, Seek, SeekFrom};
    let mut file = match std::fs::File::open(path) {
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(String::new()),
        other => other?,
    };
    let meta = file.metadata()?;
    if let Some(since) = since {
        if meta.modified()? < since {
            return Ok(String::new());
        }
    }
    // One byte more than the bound: it tells whether the window starts at a line.
    let window = max_bytes.saturating_add(1) as u64;
    file.seek(SeekFrom::Start(meta.len().saturating_sub(window)))?;
    let mut bytes = Vec::new();
    file.read_to_end(&mut bytes)?;
    Ok(String::from_utf8_lossy(tail_lines(&bytes, max_bytes)).into_owned())
}

impl Backend {
    /// The end of the log this backend's instance writes: `read_log` of
    /// `log_source()` through `journal_runner()`.
    pub fn log_text(
        &self,
        since: Option<std::time::SystemTime>,
        max_bytes: usize,
    ) -> Result<String, String> {
        read_log(&self.log_source(), since, max_bytes, journal_runner())
    }
}

/// The sections of the file as `backend()` reads it; none when it is missing
/// or cannot be read.
pub fn read_sections(file: ConfigFile) -> Vec<crate::ini::Section> {
    backend()
        .read_config(file)
        .ok()
        .flatten()
        .map(|t| crate::ini::parse_sections(&t))
        .unwrap_or_default()
}

/// The keys of one section of the file (named case-insensitively), as
/// `ini::read_section` reads a file: empty when it is not there.
pub fn read_section(file: ConfigFile, section: &str) -> std::collections::BTreeMap<String, String> {
    read_sections(file)
        .into_iter()
        .find(|s| s.id.eq_ignore_ascii_case(section))
        .map(|s| s.keys)
        .unwrap_or_default()
}

/// The connection the manager backend makes: the system bus on Linux, the
/// named pipe on Windows, except in a test build or under the test redirect
/// (`PRIVATE_AI_DATA_ROOT` set), where it is `Unavailable` without dialing
/// anything. macOS has no manager.
// Reached only from a shipped build's `backend()`: a test build takes the fake.
#[cfg_attr(all(not(test), feature = "test-support"), allow(dead_code))]
pub(crate) fn system_connect() -> Result<Client, ClientError> {
    if cfg!(any(test, feature = "test-support"))
        || std::env::var_os("PRIVATE_AI_DATA_ROOT").is_some()
    {
        return Err(ClientError::Unavailable);
    }
    #[cfg(target_os = "linux")]
    {
        pai_core::client::dbus::DbusClient::connect_system().map(|c| Box::new(c) as Client)
    }
    #[cfg(windows)]
    {
        pai_core::client::pipe::connect().map(|c| Box::new(c) as Client)
    }
    #[cfg(not(any(target_os = "linux", windows)))]
    {
        Err(ClientError::Unavailable)
    }
}

/// A new backend of this OS: the manager on Linux and Windows, `Direct` on
/// macOS.
// Reached only from a shipped build's `backend()`: a test build takes the fake.
#[cfg_attr(all(not(test), feature = "test-support"), allow(dead_code))]
pub(crate) fn os_backend() -> Backend {
    os_backend_for(Os::current())
}

/// The OSes the backend choice tells apart, so the choice of every OS is a
/// pure function tested on Linux.
// `MacOs` is the OS's name, not the enum's repeated.
#[allow(clippy::enum_variant_names)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Os {
    Linux,
    Windows,
    MacOs,
}

impl Os {
    /// The OS this crate is built for.
    pub fn current() -> Os {
        if cfg!(windows) {
            Os::Windows
        } else if cfg!(target_os = "macos") {
            Os::MacOs
        } else {
            Os::Linux
        }
    }
}

/// Which backend runs llama-server, without its connection.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    Direct,
    Manager,
}

/// The backend kind of `os`.
pub fn backend_kind(os: Os) -> Kind {
    match os {
        Os::Linux | Os::Windows => Kind::Manager,
        Os::MacOs => Kind::Direct,
    }
}

/// Whether this process must save the slot snapshots before it asks the
/// instance to stop. `Direct` runs llama-server itself and kills it, so the
/// save is the client's; under the manager the instance saves at its own stop
/// (the unit's `ExecStop=`, the service's `Stop`), and a second save would
/// write the same gigabytes twice.
pub fn client_saves_snapshots(backend: &Backend) -> bool {
    matches!(backend, Backend::Direct)
}

/// A new backend of `os`: the manager over `system_connect`, or `Direct`.
// Reached only from a shipped build's `backend()`: a test build takes the fake.
#[cfg_attr(all(not(test), feature = "test-support"), allow(dead_code))]
pub(crate) fn os_backend_for(os: Os) -> Backend {
    match backend_kind(os) {
        Kind::Manager => Backend::Manager(ManagerBackend::new(Box::new(system_connect))),
        Kind::Direct => Backend::Direct,
    }
}

/// Where the Log window reads the instance's log under `kind` on `os`.
pub fn log_source_for(kind: Kind, os: Os) -> LogSource {
    match (kind, os) {
        (Kind::Manager, Os::Linux) => LogSource::Journal {
            unit: instance_unit(),
        },
        (Kind::Manager, _) => LogSource::File(
            crate::paths::instance_log(INSTANCE).expect("INSTANCE is a valid instance id"),
        ),
        (Kind::Direct, _) => LogSource::File(crate::paths::server_log()),
    }
}

/// The strings key of the Settings tab's note saying why `LogRotate` and
/// `LogRotateKb` do not apply under the manager on `os`.
pub fn log_rotate_note_key(os: Os) -> &'static str {
    match os {
        Os::Linux => strings::key("log-rotate-journald"),
        _ => strings::key("log-rotate-service"),
    }
}

/// The backend of this OS: the manager on Linux and Windows, `Direct` on
/// macOS. In a test binary, the one `fake::scoped` set on this thread, else
/// `fake::default_backend()`.
pub fn backend() -> &'static Backend {
    #[cfg(any(test, feature = "test-support"))]
    return fake::current();
    #[cfg(not(any(test, feature = "test-support")))]
    {
        static BACKEND: std::sync::OnceLock<Backend> = std::sync::OnceLock::new();
        BACKEND.get_or_init(os_backend)
    }
}

/// A recording [`ManagerClient`] for the tests of this seam and its callers.
#[cfg(any(test, feature = "test-support"))]
pub mod fake {
    use pai_core::client::{ClientError, InstanceInfo, InstanceState, ManagerClient, ManagerFile};
    use std::sync::{Arc, Mutex};

    /// One log shared by the fake and whatever else a test records (the
    /// llama-server stub), so the order of the two can be checked.
    pub type Log = Arc<Mutex<Vec<String>>>;

    pub fn log() -> Log {
        Arc::new(Mutex::new(Vec::new()))
    }

    /// What the fake was asked, `"<method> <id>"`, status probes, file
    /// reads and start-at-boot reads left out.
    pub fn actions(log: &Log) -> Vec<String> {
        log.lock()
            .unwrap()
            .iter()
            .filter(|e| {
                !e.starts_with("status ")
                    && !e.starts_with("read_file ")
                    && !e.starts_with("boot_enabled ")
            })
            .cloned()
            .collect()
    }

    /// What a write that keeps the manager's API key is logged with, after
    /// its name: `"write_file <name> keep_api_key"`.
    pub const KEEP_API_KEY: &str = "keep_api_key";

    /// The writes the fake was asked for, `"write_file <name>"` (with
    /// [`KEEP_API_KEY`] after it when the write keeps the manager's API key),
    /// in order.
    pub fn writes(log: &Log) -> Vec<String> {
        log.lock()
            .unwrap()
            .iter()
            .filter(|e| e.starts_with("write_file "))
            .cloned()
            .collect()
    }

    /// The manager's files as the fake holds them, by name; shared out so a
    /// test reads them after the fake moved into a backend.
    pub type Files = Arc<Mutex<std::collections::BTreeMap<String, String>>>;

    /// The manager as the tests need it: a state that start, stop and restart
    /// move (a stop goes through `Deactivating` once, as systemd's does), and
    /// optional failures.
    pub struct FakeClient {
        pub log: Log,
        /// The states `status` answers, the last one repeated.
        states: Mutex<Vec<InstanceState>>,
        /// What start, stop and restart fail with.
        pub fail_actions: Option<ClientError>,
        /// What status fails with.
        pub fail_status: Option<ClientError>,
        /// The instances `instances()` answers. The default is the manager of
        /// the files these tests name: `default`, running the server profile
        /// `default`.
        pub instances: Vec<InstanceInfo>,
        /// What `instances()` fails with (an older manager, no connection).
        pub fail_instances: Option<ClientError>,
        /// The files `read_file` answers and `write_file` replaces.
        pub files: Files,
        /// What read_file fails with.
        pub fail_read: Option<ClientError>,
        /// Whether read_file says the API key was withheld from an instance
        /// file (`server@<id>.ini`), as the manager does for a caller who may
        /// not configure. The text is answered as held: a test holds the
        /// blanked text the manager would send. Shared out so a test changes
        /// it after the fake moved into a backend.
        pub withhold: Arc<Mutex<bool>>,
        /// What write_file fails with (the file is then left as it was).
        pub fail_write: Option<ClientError>,
        /// What write_file fails with for one file name only, the others
        /// written: a refusal in the middle of several writes.
        pub fail_write_of: std::collections::BTreeMap<String, ClientError>,
        /// Whether the instance starts at boot; shared out so a test reads
        /// it after the fake moved into a backend.
        pub boot: Arc<Mutex<bool>>,
        /// What boot_enabled fails with.
        pub fail_boot: Option<ClientError>,
        /// What set_boot_enabled fails with (the state is then left as it was).
        pub fail_set_boot: Option<ClientError>,
    }

    impl FakeClient {
        pub fn new(log: &Log, state: InstanceState) -> FakeClient {
            FakeClient {
                log: log.clone(),
                states: Mutex::new(vec![state]),
                fail_actions: None,
                fail_status: None,
                instances: vec![InstanceInfo {
                    id: "default".into(),
                    engine: "llama.cpp".into(),
                    server_profile: Some("default".into()),
                    port: 0,
                    profile_tags: vec![],
                    instance_tags: vec![],
                    label: None,
                    converged: true,
                    reason: None,
                    pending_restart: false,
                    instance_profile: None,
                    hostname: None,
                }],
                fail_instances: None,
                files: Files::default(),
                fail_read: None,
                withhold: Arc::new(Mutex::new(false)),
                fail_write: None,
                fail_write_of: Default::default(),
                boot: Arc::new(Mutex::new(false)),
                fail_boot: None,
                fail_set_boot: None,
            }
        }

        /// The fake with the manager's file `name` holding `text`.
        pub fn with_file(self, name: &str, text: &str) -> FakeClient {
            self.files
                .lock()
                .unwrap()
                .insert(name.to_string(), text.to_string());
            self
        }

        /// The fake whose `instances()` answers one instance: `id` running the
        /// server profile `server_profile` on `port`.
        pub fn with_instance(mut self, id: &str, server_profile: &str, port: u16) -> FakeClient {
            self.instances = vec![InstanceInfo {
                id: id.into(),
                engine: "llama.cpp".into(),
                server_profile: Some(server_profile.into()),
                port,
                profile_tags: vec![],
                instance_tags: vec![],
                label: None,
                converged: true,
                reason: None,
                pending_restart: false,
                instance_profile: None,
                hostname: None,
            }];
            self
        }

        fn record(&self, method: &str, id: &str) {
            self.log.lock().unwrap().push(format!("{method} {id}"));
        }

        fn act(&self, method: &str, id: &str, then: Vec<InstanceState>) -> Result<(), ClientError> {
            self.record(method, id);
            if let Some(e) = &self.fail_actions {
                return Err(e.clone());
            }
            *self.states.lock().unwrap() = then;
            Ok(())
        }
    }

    impl ManagerClient for FakeClient {
        fn list_instances(&self) -> Result<Vec<String>, ClientError> {
            self.record("list_instances", "");
            Ok(vec!["default".into()])
        }
        fn status(&self, id: &str) -> Result<InstanceState, ClientError> {
            self.record("status", id);
            if let Some(e) = &self.fail_status {
                return Err(e.clone());
            }
            let mut states = self.states.lock().unwrap();
            Ok(if states.len() > 1 {
                states.remove(0)
            } else {
                states[0].clone()
            })
        }
        fn start(&self, id: &str) -> Result<(), ClientError> {
            self.act("start", id, vec![InstanceState::Active])
        }
        fn stop(&self, id: &str) -> Result<(), ClientError> {
            self.act(
                "stop",
                id,
                vec![InstanceState::Deactivating, InstanceState::Inactive],
            )
        }
        fn restart(&self, id: &str) -> Result<(), ClientError> {
            self.act("restart", id, vec![InstanceState::Active])
        }
        /// The instances. Deliberately not recorded in the log: the log pins
        /// the actions and the file names, and a listing read is neither.
        fn instances(&self) -> Result<Vec<InstanceInfo>, ClientError> {
            if let Some(e) = &self.fail_instances {
                return Err(e.clone());
            }
            Ok(self.instances.clone())
        }
        fn read_file(&self, name: &str) -> Result<Option<ManagerFile>, ClientError> {
            self.record("read_file", name);
            if let Some(e) = &self.fail_read {
                return Err(e.clone());
            }
            Ok(self
                .files
                .lock()
                .unwrap()
                .get(name)
                .map(|text| ManagerFile {
                    text: text.clone(),
                    withheld: *self.withhold.lock().unwrap() && name.starts_with("server@"),
                }))
        }
        fn write_file(
            &self,
            name: &str,
            contents: &str,
            keep_api_key: bool,
        ) -> Result<(), ClientError> {
            if keep_api_key {
                self.record("write_file", &format!("{name} {KEEP_API_KEY}"));
            } else {
                self.record("write_file", name);
            }
            if let Some(e) = self.fail_write.as_ref().or(self.fail_write_of.get(name)) {
                return Err(e.clone());
            }
            self.files
                .lock()
                .unwrap()
                .insert(name.to_string(), contents.to_string());
            Ok(())
        }
        fn boot_enabled(&self, id: &str) -> Result<bool, ClientError> {
            self.record("boot_enabled", id);
            if let Some(e) = &self.fail_boot {
                return Err(e.clone());
            }
            Ok(*self.boot.lock().unwrap())
        }
        fn set_boot_enabled(&self, id: &str, enabled: bool) -> Result<(), ClientError> {
            let on = if enabled { "on" } else { "off" };
            self.record("set_boot_enabled", &format!("{id} {on}"));
            if let Some(e) = &self.fail_set_boot {
                return Err(e.clone());
            }
            *self.boot.lock().unwrap() = enabled;
            Ok(())
        }
        fn authenticate(&self, _user: &str, _password: &str) -> Result<(bool, bool), ClientError> {
            Err(ClientError::Unavailable)
        }
    }

    /// The manager a test binary's `backend()` has by default: no instance to
    /// run (every action and probe is `Unavailable`, as with no manager, so no
    /// test spawns or kills a llama-server), and the configuration files kept
    /// where `Direct` keeps them, the per-user files under the test redirect,
    /// so the e2e phases that read them back keep working. Where the manager's
    /// bytes go is pinned with `FakeClient`, not with this.
    pub struct DiskClient;

    impl DiskClient {
        fn path(name: &str) -> Result<std::path::PathBuf, ClientError> {
            match name {
                "server@default.ini" => Ok(crate::paths::server_ini()),
                "models.ini" => Ok(crate::paths::models_ini()),
                "models.ini.disabled" => Ok(crate::paths::models_disabled_ini()),
                _ => Err(ClientError::BadFileName),
            }
        }
    }

    impl ManagerClient for DiskClient {
        fn list_instances(&self) -> Result<Vec<String>, ClientError> {
            Err(ClientError::Unavailable)
        }
        /// The record the packages ship, as the manager of these files reads
        /// it: the instance `default` running the server profile `default`, so
        /// the server file keeps the name `path` maps.
        fn instances(&self) -> Result<Vec<InstanceInfo>, ClientError> {
            Ok(vec![InstanceInfo {
                id: super::INSTANCE.to_string(),
                engine: "llama.cpp".to_string(),
                server_profile: Some(super::INSTANCE.to_string()),
                port: 0,
                profile_tags: Vec::new(),
                instance_tags: Vec::new(),
                label: None,
                converged: true,
                reason: None,
                pending_restart: false,
                instance_profile: None,
                hostname: None,
            }])
        }
        fn status(&self, _id: &str) -> Result<InstanceState, ClientError> {
            Err(ClientError::Unavailable)
        }
        fn start(&self, _id: &str) -> Result<(), ClientError> {
            Err(ClientError::Unavailable)
        }
        fn stop(&self, _id: &str) -> Result<(), ClientError> {
            Err(ClientError::Unavailable)
        }
        fn restart(&self, _id: &str) -> Result<(), ClientError> {
            Err(ClientError::Unavailable)
        }
        fn read_file(&self, name: &str) -> Result<Option<ManagerFile>, ClientError> {
            match std::fs::read_to_string(Self::path(name)?) {
                Ok(text) => Ok(Some(ManagerFile {
                    text,
                    withheld: false,
                })),
                Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(None),
                Err(e) => Err(ClientError::Io(format!("{:?}", e.kind()))),
            }
        }
        fn write_file(
            &self,
            name: &str,
            contents: &str,
            _keep_api_key: bool,
        ) -> Result<(), ClientError> {
            let path = Self::path(name)?;
            let io = |e: std::io::Error| ClientError::Io(format!("{:?}", e.kind()));
            if let Some(parent) = path.parent() {
                std::fs::create_dir_all(parent).map_err(io)?;
            }
            crate::ini::atomic_write(&path, contents).map_err(io)
        }
        fn boot_enabled(&self, _id: &str) -> Result<bool, ClientError> {
            Err(ClientError::Unavailable)
        }
        fn set_boot_enabled(&self, _id: &str, _enabled: bool) -> Result<(), ClientError> {
            Err(ClientError::Unavailable)
        }
        fn authenticate(&self, _user: &str, _password: &str) -> Result<(bool, bool), ClientError> {
            Err(ClientError::Unavailable)
        }
    }

    /// `backend()` in a test binary when no `scoped` is active on the thread.
    pub fn default_backend() -> &'static super::Backend {
        static DEFAULT: std::sync::OnceLock<super::Backend> = std::sync::OnceLock::new();
        DEFAULT.get_or_init(|| {
            super::Backend::Manager(super::ManagerBackend::new(Box::new(|| {
                Ok(Box::new(DiskClient) as super::Client)
            })))
        })
    }

    thread_local! {
        static SCOPED: std::cell::Cell<Option<&'static super::Backend>> =
            const { std::cell::Cell::new(None) };
    }

    /// What `backend()` returns on this thread.
    pub fn current() -> &'static super::Backend {
        SCOPED.with(|s| s.get()).unwrap_or_else(default_backend)
    }

    /// Run `f` with `backend()` returning `b` on this thread (not on threads
    /// `f` spawns), then put back what it returned before, even on a panic.
    pub fn scoped<R>(b: super::Backend, f: impl FnOnce() -> R) -> R {
        struct Restore(Option<&'static super::Backend>);
        impl Drop for Restore {
            fn drop(&mut self) {
                SCOPED.with(|s| s.set(self.0));
            }
        }
        let b: &'static super::Backend = Box::leak(Box::new(b));
        let _restore = Restore(SCOPED.with(|s| s.replace(Some(b))));
        f()
    }

    /// A llama-server stand-in on a free loopback port: records each request
    /// as `"<METHOD> <path>"` in `log` and answers the snapshot calls
    /// (`/models` with no model loaded, `/health`, a slot restore). Its thread
    /// lives as long as the test binary.
    pub fn http_stub(log: &Log) -> u16 {
        use std::io::{Read, Write};
        let listener = std::net::TcpListener::bind(("127.0.0.1", 0)).expect("bind");
        let port = listener.local_addr().expect("addr").port();
        let log = log.clone();
        std::thread::spawn(move || {
            for stream in listener.incoming() {
                let Ok(mut s) = stream else { continue };
                let _ = s.set_read_timeout(Some(std::time::Duration::from_secs(5)));
                let mut raw = Vec::new();
                let mut buf = [0u8; 4096];
                let head_end = loop {
                    match s.read(&mut buf) {
                        Ok(0) | Err(_) => break None,
                        Ok(n) => raw.extend_from_slice(&buf[..n]),
                    }
                    if let Some(i) = raw.windows(4).position(|w| w == b"\r\n\r\n") {
                        break Some(i + 4);
                    }
                };
                let Some(head_end) = head_end else { continue };
                let head = String::from_utf8_lossy(&raw[..head_end]).to_string();
                let length: usize = head
                    .lines()
                    .find_map(|l| l.strip_prefix("Content-Length: "))
                    .and_then(|v| v.trim().parse().ok())
                    .unwrap_or(0);
                while raw.len() < head_end + length {
                    match s.read(&mut buf) {
                        Ok(0) | Err(_) => break,
                        Ok(n) => raw.extend_from_slice(&buf[..n]),
                    }
                }
                let line = head.lines().next().unwrap_or("");
                let mut parts = line.split(' ');
                let request = format!(
                    "{} {}",
                    parts.next().unwrap_or(""),
                    parts.next().unwrap_or("")
                );
                log.lock().unwrap().push(request.clone());
                let body = if request.starts_with("GET /models") {
                    r#"{"data":[]}"#
                } else if request.starts_with("GET /health") {
                    r#"{"status":"ok"}"#
                } else {
                    r#"{"id_slot":0,"n_restored":1,"n_read":1}"#
                };
                let _ = write!(
                    s,
                    "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\n\
                     Content-Length: {}\r\nConnection: close\r\n\r\n{body}",
                    body.len()
                );
            }
        });
        port
    }

    /// A `Manager` backend over `client`.
    pub fn backend(client: FakeClient) -> super::Backend {
        let client = std::sync::Mutex::new(Some(client));
        super::Backend::Manager(super::ManagerBackend::new(Box::new(move || {
            let c = client.lock().unwrap().take().expect("connected twice");
            Ok(Box::new(c) as super::Client)
        })))
    }
}

/// E2E phase (an isolated test of `tests/mod.rs`): the save of
/// a server config read with the API key withheld. `server_cfg::load` reads
/// no key (the manager blanked it); saved untouched, the manager is asked to
/// keep its own; saved with a key the user typed, that key is sent and
/// nothing kept.
#[cfg(test)]
pub(crate) fn api_key_kept_e2e() {
    use crate::server_cfg::{self, ServerConfig};
    use fake::FakeClient;
    let dir = tempfile::tempdir().expect("tempdir");
    crate::paths::redirect_for_tests(dir.path());
    let log = fake::log();
    let c = FakeClient::new(&log, InstanceState::Inactive)
        .with_file(
            "server@default.ini",
            "[Server]\nPort = 8099\nOpencodeApiKey =\n",
        )
        .with_file(
            "models.ini",
            "[Qwen3.8-27B]\nmodel = /models/Qwen3.8-27B-Q8_0.gguf\n",
        );
    *c.withhold.lock().unwrap() = true;
    let files = c.files.clone();
    let file = || {
        files
            .lock()
            .unwrap()
            .get("server@default.ini")
            .cloned()
            .unwrap_or_default()
    };
    fake::scoped(fake::backend(c), || {
        let cfg = server_cfg::load();
        assert_eq!(cfg.port, Some(8099));
        assert_eq!(cfg.opencode_api_key, None, "the manager blanked it");
        assert!(backend().api_key_withheld(), "load read it withheld");

        // Untouched (another field changed): the manager keeps its key.
        log.lock().unwrap().clear();
        server_cfg::save(&ServerConfig {
            port: Some(8100),
            ..cfg.clone()
        })
        .expect("server save");
        assert_eq!(
            fake::writes(&log),
            [format!(
                "write_file server@default.ini {}",
                fake::KEEP_API_KEY
            )]
        );
        assert_eq!(server_cfg::from_text(&file()).port, Some(8100));

        // A key typed in: sent as typed, nothing kept.
        log.lock().unwrap().clear();
        server_cfg::save(&ServerConfig {
            opencode_api_key: Some("sk-typed-0000".into()),
            ..cfg.clone()
        })
        .expect("server save with a key");
        assert_eq!(fake::writes(&log), ["write_file server@default.ini"]);
        assert_eq!(
            server_cfg::from_text(&file()).opencode_api_key,
            Some("sk-typed-0000".into())
        );
    });
}

/// E2E phase (an isolated test of `tests/mod.rs`): the
/// `Manager` backend's start and restart, which load server.ini, and that it
/// leaves the per-user tree to the manager (no model profiles or binary check, no log
/// rotation).
#[cfg(test)]
pub(crate) fn server_control_e2e() {
    use fake::FakeClient;
    use pai_core::client::InstanceState;
    let dir = tempfile::tempdir().expect("tempdir");
    crate::paths::redirect_for_tests(dir.path());

    // The Windows manager: the log is the service's file in the machine-wide
    // log directory, the config files the machine-wide ones, as on Linux.
    let log_file = crate::paths::instance_log(INSTANCE).expect("valid id");
    assert_eq!(
        log_file.parent(),
        Some(crate::paths::system_log_dir().as_path())
    );
    assert_eq!(
        log_source_for(Kind::Manager, Os::Windows),
        LogSource::File(log_file)
    );
    let windows = os_backend_for(Os::Windows);
    for file in ConfigFile::ALL {
        assert_eq!(
            windows.config_path(file),
            crate::paths::engine_config_dir().join(file.manager_name()),
            "{file:?}"
        );
    }

    // Under the redirect the Linux backend never dials the system bus: a
    // socket of ours stands in for it, and nothing may connect to it. The only
    // place in this binary that sets `DBUS_SYSTEM_BUS_ADDRESS`.
    #[cfg(target_os = "linux")]
    {
        let socket = dir.path().join("system-bus");
        let listener = std::os::unix::net::UnixListener::bind(&socket).expect("bind bus socket");
        listener.set_nonblocking(true).expect("nonblocking");
        std::env::set_var(
            "DBUS_SYSTEM_BUS_ADDRESS",
            format!("unix:path={}", socket.display()),
        );
        let (tx, rx) = std::sync::mpsc::channel();
        std::thread::spawn(move || {
            let connected = system_connect().map(|_| ());
            // The backend this OS builds, over the real connect.
            let running =
                Backend::Manager(ManagerBackend::new(Box::new(system_connect))).is_running();
            let _ = tx.send((connected, running));
        });
        match rx.recv_timeout(std::time::Duration::from_secs(10)) {
            Ok((connected, running)) => {
                assert_eq!(connected, Err(ClientError::Unavailable), "system_connect");
                assert!(!running, "the manager backend reads as not running");
            }
            Err(std::sync::mpsc::RecvTimeoutError::Timeout) => {
                panic!("system_connect or backend() did not return within 10 s")
            }
            Err(e) => panic!("the probe thread panicked: {e:?}"),
        }
        match listener.accept() {
            Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => {}
            Ok(_) => panic!("something dialed the system bus under the test redirect"),
            Err(e) => panic!("accept: {e}"),
        }
    }

    // The per-user server.ini is not the instance's config under this
    // backend: the manager's server@default.ini is (8099 below).
    let server_ini = crate::paths::server_ini();
    std::fs::create_dir_all(server_ini.parent().unwrap()).expect("config dir");
    std::fs::write(&server_ini, "[Server]\nPort = 9999\n").expect("server.ini");
    const MANAGED: &str = "[Server]\nPort = 8099\n";
    // No models.ini and no llama-server binary: those are the manager's to
    // check under this backend, not the configurator's.
    assert!(!crate::paths::models_ini().exists());

    // Started from Inactive: one start of default, the config read back from
    // the manager.
    let log = fake::log();
    let b = fake::backend(
        FakeClient::new(&log, InstanceState::Inactive).with_file("server@default.ini", MANAGED),
    );
    match b.start() {
        Ok(Some(cfg)) => assert_eq!(cfg.port_or_default(), 8099),
        other => panic!("start from Inactive: {other:?}"),
    }
    assert_eq!(fake::actions(&log), ["start default"]);

    // Already there (Activating is a start under way): nothing asked.
    for state in [InstanceState::Active, InstanceState::Activating] {
        let log = fake::log();
        let b = fake::backend(FakeClient::new(&log, state.clone()));
        assert!(matches!(b.start(), Ok(None)), "{state:?}");
        assert!(fake::actions(&log).is_empty(), "{state:?}: {log:?}");
    }

    // A refused start and no manager: the client's text.
    for e in [ClientError::NotAuthorized, ClientError::Unavailable] {
        let log = fake::log();
        let mut c = FakeClient::new(&log, InstanceState::Inactive);
        c.fail_actions = Some(e.clone());
        if e == ClientError::Unavailable {
            c.fail_status = Some(e.clone());
        }
        let b = fake::backend(c);
        match b.start() {
            Err(text) => assert_eq!(text, e.text(), "{e:?}"),
            Ok(x) => panic!("{e:?}: started {x:?}"),
        }
        match b.restart() {
            Err(text) => assert_eq!(text, e.text(), "{e:?}"),
            Ok(x) => panic!("{e:?}: restarted {x:?}"),
        }
    }

    // Restart: the manager's restart, the config read back from the manager.
    let log = fake::log();
    let b = fake::backend(
        FakeClient::new(&log, InstanceState::Active).with_file("server@default.ini", MANAGED),
    );
    match b.restart() {
        Ok(Some(cfg)) => assert_eq!(cfg.port_or_default(), 8099),
        other => panic!("restart: {other:?}"),
    }
    assert_eq!(fake::actions(&log), ["restart default"]);

    // The per-user log is not this backend's: a log well past the rotation
    // threshold (settings default) is neither closed on start nor on stop.
    let server_log = crate::paths::server_log();
    let logs = server_log.parent().unwrap().to_path_buf();
    std::fs::create_dir_all(&logs).expect("logs dir");
    let big = "x".repeat(2 * 1024 * 1024);
    std::fs::write(&server_log, &big).expect("big log");
    let log = fake::log();
    let b = fake::backend(FakeClient::new(&log, InstanceState::Inactive));
    assert!(matches!(b.start(), Ok(Some(_))));
    assert_eq!(b.stop(), Ok(true));
    assert_eq!(fake::actions(&log), ["start default", "stop default"]);
    assert_eq!(
        std::fs::read_to_string(&server_log).expect("log still there"),
        big,
        "log untouched"
    );
    let listed: Vec<_> = std::fs::read_dir(&logs)
        .expect("logs dir")
        .map(|e| e.unwrap().file_name())
        .collect();
    assert_eq!(listed, [std::ffi::OsString::from("llama-server.log")]);
}

/// E2E phase (an isolated test of `tests/mod.rs`): every
/// reader and writer of the instance's configuration files goes through the
/// seam. `Direct` keeps the per-user files under their names; the `Manager`
/// backend reads and writes the manager's files and nothing under the
/// per-user config directory; a write the manager refuses reaches the caller
/// (the CLI, the GUI's save status) as the refusal's own text.
#[cfg(test)]
pub(crate) fn config_seam_e2e() {
    use crate::model_profiles::{self, ModelProfile};
    use crate::paths;
    use crate::server_cfg::{self, ServerConfig};
    use fake::FakeClient;
    use pai_core::client::InstanceState;
    let dir = tempfile::tempdir().expect("tempdir");
    paths::redirect_for_tests(dir.path());
    let user = paths::config_dir();

    // ── Direct: the per-user files, under the names they always had ──────
    let d = Backend::Direct;
    for (file, path) in [
        (ConfigFile::Server, paths::server_ini()),
        (ConfigFile::Models, paths::models_ini()),
        (ConfigFile::ModelsDisabled, paths::models_disabled_ini()),
    ] {
        assert_eq!(d.config_path(file), path, "{file:?}");
        assert_eq!(d.read_config(file), Ok(None), "{file:?}: no file yet");
        let text = format!("; {file:?}\r\n[m]\r\nctx-size = 4096\r\n");
        assert_eq!(d.write_config(file, &text), Ok(()), "{file:?}");
        assert_eq!(std::fs::read_to_string(&path).expect("written"), text);
        assert_eq!(d.read_config(file), Ok(Some(text)), "{file:?}");
    }

    // A write that fails: an error, the old file kept, no temp file left.
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let listing = || {
            let mut v: Vec<_> = std::fs::read_dir(&user)
                .expect("config dir")
                .map(|e| e.unwrap().file_name())
                .collect();
            v.sort();
            v
        };
        let before = listing();
        let old = std::fs::read_to_string(paths::server_ini()).expect("server.ini");
        std::fs::set_permissions(&user, std::fs::Permissions::from_mode(0o555)).expect("chmod");
        // Root ignores the mode: only check what a failing write does when
        // a write really fails.
        let probe = std::fs::write(user.join("probe"), "x");
        let result = probe
            .is_err()
            .then(|| d.write_config(ConfigFile::Server, "[Server]\nPort = 1\n"));
        std::fs::set_permissions(&user, std::fs::Permissions::from_mode(0o755)).expect("chmod");
        if let Some(result) = result {
            assert!(result.is_err(), "a failed write is an error: {result:?}");
            assert_eq!(
                std::fs::read_to_string(paths::server_ini()).expect("server.ini"),
                old
            );
            assert_eq!(listing(), before, "no temp file left");
        } else {
            let _ = std::fs::remove_file(user.join("probe"));
        }
    }

    // A file that cannot be read is an error, not "no file": a directory
    // in its place.
    std::fs::remove_file(paths::models_disabled_ini()).expect("remove");
    std::fs::create_dir(paths::models_disabled_ini()).expect("dir in its place");
    assert!(
        d.read_config(ConfigFile::ModelsDisabled).is_err(),
        "an unreadable file is not none"
    );
    std::fs::remove_dir(paths::models_disabled_ini()).expect("remove dir");

    // ── Manager: the manager's files, shown where the manager keeps them ──
    let log = fake::log();
    let m = fake::backend(FakeClient::new(&log, InstanceState::Inactive));
    for file in ConfigFile::ALL {
        assert_eq!(
            m.config_path(file),
            paths::engine_config_dir().join(file.manager_name()),
            "{file:?}"
        );
    }
    // The engine's directory, not the framework's (bff.ini's).
    assert_ne!(
        m.config_path(ConfigFile::Models).parent(),
        Some(paths::system_config_dir().as_path()),
        "the instance files are the engine's"
    );

    // Reads: from the manager, never from the per-user files, which say
    // something else here.
    std::fs::write(paths::server_ini(), "[Server]\nPort = 9999\n").expect("user server.ini");
    std::fs::write(
        paths::models_ini(),
        "[user-only]\nmodel = /models/user-only.gguf\n",
    )
    .expect("user models.ini");
    let seeded = || {
        FakeClient::new(&log, InstanceState::Inactive)
            .with_file("server@default.ini", "[Server]\nPort = 8099\n")
            .with_file(
                "models.ini",
                "[Qwen3.8-27B]\nmodel = /models/Qwen3.8-27B-Q8_0.gguf\n",
            )
            .with_file(
                "models.ini.disabled",
                "[Qwen3.8-Flash-Next]\nmodel = /models/Qwen3.8-Flash-Next-IQ3_S.gguf\n",
            )
    };
    fake::scoped(fake::backend(seeded()), || {
        assert_eq!(server_cfg::load().port_or_default(), 8099, "server.ini");
        let enabled: Vec<String> = model_profiles::load_all()
            .into_iter()
            .map(|p| p.id)
            .collect();
        assert_eq!(enabled, ["Qwen3.8-27B"], "models.ini");
        let listed: Vec<(String, bool)> = model_profiles::load_listed()
            .into_iter()
            .map(|l| (l.model_profile.id, l.enabled))
            .collect();
        assert_eq!(
            listed,
            [
                ("Qwen3.8-27B".to_string(), true),
                ("Qwen3.8-Flash-Next".to_string(), false)
            ]
        );
    });

    // Writes: the manager's files, and nothing under the per-user directory.
    std::fs::remove_dir_all(&user).expect("clear the per-user config");
    let nothing_per_user = |when: &str| {
        for name in ["server.ini", "models.ini", "models.ini.disabled"] {
            assert!(
                !user.join(name).exists(),
                "{when}: {name} written under the per-user config directory"
            );
        }
    };
    let c = seeded();
    let files = c.files.clone();
    let file = |name: &str| files.lock().unwrap().get(name).cloned().unwrap_or_default();
    let ids = |text: String| -> Vec<String> {
        crate::ini::parse_sections(&text)
            .into_iter()
            .map(|s| s.id)
            .collect()
    };
    let cfg = ServerConfig {
        port: Some(8100),
        models_dir: Some(dir.path().join("models").display().to_string()),
        state_dir: Some(dir.path().join("state").display().to_string()),
        ..Default::default()
    };
    let q8 = ModelProfile::new_default(
        "Qwen3.8-27B-Q8".into(),
        "/models/Qwen3.8-27B-Q8_0.gguf".into(),
    );
    fake::scoped(fake::backend(c), || {
        log.lock().unwrap().clear();
        server_cfg::save(&cfg).expect("server save");
        assert_eq!(fake::writes(&log), ["write_file server@default.ini"]);
        assert_eq!(file("server@default.ini"), server_cfg::render(&cfg));
        nothing_per_user("server_cfg::save");

        log.lock().unwrap().clear();
        model_profiles::save(&q8).expect("preset save");
        assert_eq!(fake::writes(&log), ["write_file models.ini"]);
        assert_eq!(ids(file("models.ini")), ["Qwen3.8-27B", "Qwen3.8-27B-Q8"]);
        let section = crate::ini::parse_sections(&file("models.ini"))
            .into_iter()
            .find(|s| s.id == "Qwen3.8-27B-Q8")
            .expect("saved section");
        assert_eq!(
            section.keys.get("model").map(String::as_str),
            Some("/models/Qwen3.8-27B-Q8_0.gguf")
        );
        nothing_per_user("model_profiles::save");

        // Switched off: the destination written first, as with the files.
        log.lock().unwrap().clear();
        model_profiles::set_enabled("Qwen3.8-27B", false).expect("switch off");
        assert_eq!(
            fake::writes(&log),
            ["write_file models.ini.disabled", "write_file models.ini"]
        );
        assert_eq!(ids(file("models.ini")), ["Qwen3.8-27B-Q8"]);
        assert_eq!(
            ids(file("models.ini.disabled")),
            ["Qwen3.8-Flash-Next", "Qwen3.8-27B"]
        );
        log.lock().unwrap().clear();
        model_profiles::set_enabled("Qwen3.8-27B", true).expect("switch on");
        assert_eq!(
            fake::writes(&log),
            ["write_file models.ini", "write_file models.ini.disabled"]
        );
        assert_eq!(ids(file("models.ini.disabled")), ["Qwen3.8-Flash-Next"]);
        assert_eq!(ids(file("models.ini")), ["Qwen3.8-27B-Q8", "Qwen3.8-27B"]);

        model_profiles::rename("Qwen3.8-27B-Q8", "Qwen3.8-27B-Q8_0").expect("rename");
        assert_eq!(ids(file("models.ini")), ["Qwen3.8-27B-Q8_0", "Qwen3.8-27B"]);
        model_profiles::delete("Qwen3.8-Flash-Next").expect("delete");
        assert!(ids(file("models.ini.disabled")).is_empty());
        nothing_per_user("set_enabled, rename, delete");

        // Under the manager on Linux and Windows nothing seeds ModelsDir into
        // its server file: an unset one is the instance's default folder
        // (`paths::instance_models_dir`), and a folder inferred from a model
        // path or a download could be one the instance's account cannot
        // read. macOS seeds it as before.
        let seeds = !cfg!(any(windows, target_os = "linux"));
        let seeded_dir = |dir: &std::path::Path| seeds.then(|| dir.display().to_string());

        // A server save with both folders unset leaves them unset in the
        // manager's file: no line names either, nothing of the saving user's
        // home reaches it, and the user's default folder is not created.
        let user_models = std::path::PathBuf::from(crate::server_cfg::default_models_dir());
        assert!(
            user_models.starts_with(dir.path()),
            "fixture sanity: redirected"
        );
        let _ = std::fs::remove_dir(&user_models);
        let unset_dirs = ServerConfig {
            port: Some(8101),
            ..Default::default()
        };
        log.lock().unwrap().clear();
        server_cfg::save(&unset_dirs).expect("server save with the folders unset");
        assert_eq!(fake::writes(&log), ["write_file server@default.ini"]);
        let written = file("server@default.ini");
        let named = |key: &str| written.lines().any(|l| l.trim_start().starts_with(key));
        if seeds {
            assert!(named("ModelsDir") && named("StateDir"), "{written}");
        } else {
            assert!(!named("ModelsDir") && !named("StateDir"), "{written}");
            assert!(
                !written.contains(&*user_models.to_string_lossy())
                    && !written.contains(&*dir.path().to_string_lossy()),
                "a folder of the saving user's in the manager's file:\n{written}"
            );
            assert!(
                !user_models.exists(),
                "the user's default models folder created for nothing"
            );
        }
        assert_eq!(server_cfg::from_text(&written).port, Some(8101));

        // The first model profile saved with no ModelsDir seeds it in the server
        // file only where the folders are not left unset (above).
        files.lock().unwrap().insert(
            "server@default.ini".into(),
            "[Server]\nPort = 8099\n".into(),
        );
        let llm = dir.path().join("llm");
        let model = llm.join("models").join("Qwen3.8-4B-Q4_K_M.gguf");
        model_profiles::save(&ModelProfile::new_default(
            "Qwen3.8-4B".into(),
            model.display().to_string(),
        ))
        .expect("preset save with ModelsDir unset");
        assert_eq!(
            server_cfg::from_text(&file("server@default.ini")).models_dir,
            seeded_dir(&llm)
        );

        // A download's model profile pins ModelsDir to where its files went, again
        // only where the folders are not left unset.
        files.lock().unwrap().insert(
            "server@default.ini".into(),
            "[Server]\nPort = 8099\n".into(),
        );
        let hf_root = dir.path().join("hf-models");
        let downloaded = ModelProfile::new_default(
            "Qwen3.8-8B".into(),
            hf_root
                .join("models")
                .join("example")
                .join("Qwen3.8-8B-GGUF")
                .join("Qwen3.8-8B-Q4_K_M.gguf")
                .display()
                .to_string(),
        );
        crate::hf::save_model_profile(&downloaded, &hf_root).expect("download's model profile");
        assert_eq!(
            server_cfg::from_text(&file("server@default.ini")).models_dir,
            seeded_dir(&hf_root)
        );
        assert!(ids(file("models.ini")).contains(&"Qwen3.8-8B".to_string()));
        nothing_per_user("ModelsDir seeding");
    });

    // ── Refused writes: the refusal's own text, and nothing changed ──────
    let refusal = pai_core::checks::Refusal {
        key: "not-whole-number-in-range",
        values: ["Port", "99999", "1", "65535"].map(String::from).to_vec(),
    };
    for e in [
        ClientError::Invalid(refusal),
        ClientError::NotAuthorized,
        ClientError::Unavailable,
    ] {
        let mut c = seeded();
        c.fail_write = Some(e.clone());
        let files = c.files.clone();
        let before = files.lock().unwrap().clone();
        fake::scoped(fake::backend(c), || {
            let err = server_cfg::save(&cfg).expect_err("server save refused");
            assert_eq!(err.to_string(), e.text(), "{e:?}: server_cfg::save");
            let err = model_profiles::save(&q8).expect_err("preset save refused");
            assert_eq!(err.to_string(), e.text(), "{e:?}: model_profiles::save");
            let err =
                model_profiles::set_enabled("Qwen3.8-27B", false).expect_err("switch refused");
            assert_eq!(err.to_string(), e.text(), "{e:?}: set_enabled");
        });
        assert_eq!(*files.lock().unwrap(), before, "{e:?}: nothing changed");
        nothing_per_user("a refused write");
    }

    // ── A read that fails stops the write ────────────────────────────────
    // Writing what an empty read leaves would replace the manager's file
    // with one section, or none: every other model profile lost.
    let e = ClientError::Io("PermissionDenied".into());
    let mut c = seeded();
    c.fail_read = Some(e.clone());
    let files = c.files.clone();
    let before = files.lock().unwrap().clone();
    log.lock().unwrap().clear();
    fake::scoped(fake::backend(c), || {
        for (what, result) in [
            ("model_profiles::save", model_profiles::save(&q8)),
            (
                "set_enabled off",
                model_profiles::set_enabled("Qwen3.8-27B", false),
            ),
            (
                "set_enabled on",
                model_profiles::set_enabled("Qwen3.8-Flash-Next", true),
            ),
            (
                "rename",
                model_profiles::rename("Qwen3.8-27B", "Qwen3.8-27B-Q8_0"),
            ),
            ("delete", model_profiles::delete("Qwen3.8-27B")),
            (
                "hf::save_model_profile",
                crate::hf::save_model_profile(&q8, &dir.path().join("hf-models")),
            ),
        ] {
            let err = result.expect_err(what);
            assert_eq!(err.to_string(), e.text(), "{what}");
        }
    });
    assert_eq!(fake::writes(&log), Vec::<String>::new(), "nothing written");
    assert_eq!(*files.lock().unwrap(), before, "the files as they were");
    nothing_per_user("a failed read");
}

#[cfg(test)]
#[path = "server_control_tests.rs"]
mod tests;

/// The server file of the instance is the profile `Instances()` names for it,
/// never `server@<id>.ini` (slice 13, ARCHITECTURE.md).
#[cfg(test)]
#[path = "instance_profile_tests.rs"]
mod instance_profile_tests;
