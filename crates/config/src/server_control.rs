//! Starting, stopping and probing the `default` llama-server instance, behind
//! one seam with two backends: `Direct` (the configurator spawns and kills the
//! process itself, `runstate`) and `Manager` (the manager service runs it, asked
//! through `pai_core::client`). `backend()` picks the one of this OS.

use crate::server_cfg::ServerConfig;
use crate::strings;
use pai_core::client::{ClientError, InstanceState, ManagerClient};
use std::path::{Path, PathBuf};
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
}

impl ManagerBackend {
    /// A backend that connects with `connect` on its first use, not now.
    pub fn new(connect: Connect) -> ManagerBackend {
        ManagerBackend {
            connect,
            client: Mutex::new(None),
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

    fn read_config(&self, file: ConfigFile) -> Result<Option<String>, ClientError> {
        self.client()?.read_file(&file.manager_name())
    }

    fn write_config(&self, file: ConfigFile, contents: &str) -> Result<(), ClientError> {
        self.client()?.write_file(&file.manager_name(), contents)
    }
}

/// How often a stop asks whether the instance is gone.
const STOP_POLL: Duration = Duration::from_millis(250);

/// A configuration file of the instance: read and written only through this
/// seam, from the manager on Linux and from the per-user files on `Direct`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ConfigFile {
    /// `server@default.ini` (`server.ini` per user).
    Server,
    /// `presets.ini`.
    Presets,
    /// `presets.ini.disabled`.
    PresetsDisabled,
}

impl ConfigFile {
    #[cfg(test)]
    pub const ALL: [ConfigFile; 3] = [
        ConfigFile::Server,
        ConfigFile::Presets,
        ConfigFile::PresetsDisabled,
    ];

    /// The file's name among the manager's files.
    pub fn manager_name(self) -> String {
        match self {
            ConfigFile::Server => format!("server@{INSTANCE}.ini"),
            ConfigFile::Presets => "presets.ini".into(),
            ConfigFile::PresetsDisabled => "presets.ini.disabled".into(),
        }
    }

    /// The per-user file `Direct` reads and writes.
    fn user_path(self) -> PathBuf {
        match self {
            ConfigFile::Server => crate::paths::server_ini(),
            ConfigFile::Presets => crate::paths::presets_ini(),
            ConfigFile::PresetsDisabled => crate::paths::presets_disabled_ini(),
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

    /// Where the file is, to show the user.
    pub fn config_path(&self, file: ConfigFile) -> PathBuf {
        match self {
            Backend::Direct => file.user_path(),
            Backend::Manager(_) => crate::paths::system_config_dir().join(file.manager_name()),
        }
    }
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
    /// Where the Log window reads the instance's log: `log_source_for` this
    /// backend's kind on this OS (no call to the manager).
    pub fn log_source(&self) -> LogSource {
        let kind = match self {
            Backend::Direct => Kind::Direct,
            Backend::Manager(_) => Kind::Manager,
        };
        log_source_for(kind, Os::current())
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
    #[cfg(test)]
    return &|_: &[String]| {
        Err(std::io::Error::new(
            std::io::ErrorKind::Unsupported,
            "a test build reads no journal",
        ))
    };
    #[cfg(not(test))]
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
    #[cfg(test)]
    return &|_: &[String]| {
        Err(std::io::Error::new(
            std::io::ErrorKind::Unsupported,
            "a test build reads no journal",
        ))
    };
    #[cfg(not(test))]
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
pub(crate) fn system_connect() -> Result<Client, ClientError> {
    if cfg!(test) || std::env::var_os("PRIVATE_AI_DATA_ROOT").is_some() {
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

/// A new backend of `os`: the manager over `system_connect`, or `Direct`.
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
    #[cfg(test)]
    return fake::current();
    #[cfg(not(test))]
    {
        static BACKEND: std::sync::OnceLock<Backend> = std::sync::OnceLock::new();
        BACKEND.get_or_init(os_backend)
    }
}

/// A recording [`ManagerClient`] for the tests of this seam and its callers.
#[cfg(test)]
pub(crate) mod fake {
    use pai_core::client::{ClientError, InstanceState, ManagerClient};
    use std::sync::{Arc, Mutex};

    /// One log shared by the fake and whatever else a test records (the
    /// llama-server stub), so the order of the two can be checked.
    pub(crate) type Log = Arc<Mutex<Vec<String>>>;

    pub(crate) fn log() -> Log {
        Arc::new(Mutex::new(Vec::new()))
    }

    /// What the fake was asked, `"<method> <id>"`, status probes and file
    /// reads left out.
    pub(crate) fn actions(log: &Log) -> Vec<String> {
        log.lock()
            .unwrap()
            .iter()
            .filter(|e| !e.starts_with("status ") && !e.starts_with("read_file "))
            .cloned()
            .collect()
    }

    /// The writes the fake was asked for, `"write_file <name>"`, in order.
    pub(crate) fn writes(log: &Log) -> Vec<String> {
        log.lock()
            .unwrap()
            .iter()
            .filter(|e| e.starts_with("write_file "))
            .cloned()
            .collect()
    }

    /// The manager's files as the fake holds them, by name; shared out so a
    /// test reads them after the fake moved into a backend.
    pub(crate) type Files = Arc<Mutex<std::collections::BTreeMap<String, String>>>;

    /// The manager as the tests need it: a state that start, stop and restart
    /// move (a stop goes through `Deactivating` once, as systemd's does), and
    /// optional failures.
    pub(crate) struct FakeClient {
        pub(crate) log: Log,
        /// The states `status` answers, the last one repeated.
        states: Mutex<Vec<InstanceState>>,
        /// What start, stop and restart fail with.
        pub(crate) fail_actions: Option<ClientError>,
        /// What status fails with.
        pub(crate) fail_status: Option<ClientError>,
        /// The files `read_file` answers and `write_file` replaces.
        pub(crate) files: Files,
        /// What read_file fails with.
        pub(crate) fail_read: Option<ClientError>,
        /// What write_file fails with (the file is then left as it was).
        pub(crate) fail_write: Option<ClientError>,
        /// What write_file fails with for one file name only, the others
        /// written: a refusal in the middle of several writes.
        pub(crate) fail_write_of: std::collections::BTreeMap<String, ClientError>,
    }

    impl FakeClient {
        pub(crate) fn new(log: &Log, state: InstanceState) -> FakeClient {
            FakeClient {
                log: log.clone(),
                states: Mutex::new(vec![state]),
                fail_actions: None,
                fail_status: None,
                files: Files::default(),
                fail_read: None,
                fail_write: None,
                fail_write_of: Default::default(),
            }
        }

        /// The fake with the manager's file `name` holding `text`.
        pub(crate) fn with_file(self, name: &str, text: &str) -> FakeClient {
            self.files
                .lock()
                .unwrap()
                .insert(name.to_string(), text.to_string());
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
        fn read_file(&self, name: &str) -> Result<Option<String>, ClientError> {
            self.record("read_file", name);
            if let Some(e) = &self.fail_read {
                return Err(e.clone());
            }
            Ok(self.files.lock().unwrap().get(name).cloned())
        }
        fn write_file(&self, name: &str, contents: &str) -> Result<(), ClientError> {
            self.record("write_file", name);
            if let Some(e) = self.fail_write.as_ref().or(self.fail_write_of.get(name)) {
                return Err(e.clone());
            }
            self.files
                .lock()
                .unwrap()
                .insert(name.to_string(), contents.to_string());
            Ok(())
        }
    }

    /// The manager a test binary's `backend()` has by default: no instance to
    /// run (every action and probe is `Unavailable`, as with no manager, so no
    /// test spawns or kills a llama-server), and the configuration files kept
    /// where `Direct` keeps them, the per-user files under the test redirect,
    /// so the e2e phases that read them back keep working. Where the manager's
    /// bytes go is pinned with `FakeClient`, not with this.
    pub(crate) struct DiskClient;

    impl DiskClient {
        fn path(name: &str) -> Result<std::path::PathBuf, ClientError> {
            match name {
                "server@default.ini" => Ok(crate::paths::server_ini()),
                "presets.ini" => Ok(crate::paths::presets_ini()),
                "presets.ini.disabled" => Ok(crate::paths::presets_disabled_ini()),
                _ => Err(ClientError::BadFileName),
            }
        }
    }

    impl ManagerClient for DiskClient {
        fn list_instances(&self) -> Result<Vec<String>, ClientError> {
            Err(ClientError::Unavailable)
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
        fn read_file(&self, name: &str) -> Result<Option<String>, ClientError> {
            match std::fs::read_to_string(Self::path(name)?) {
                Ok(text) => Ok(Some(text)),
                Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(None),
                Err(e) => Err(ClientError::Io(format!("{:?}", e.kind()))),
            }
        }
        fn write_file(&self, name: &str, contents: &str) -> Result<(), ClientError> {
            let path = Self::path(name)?;
            let io = |e: std::io::Error| ClientError::Io(format!("{:?}", e.kind()));
            if let Some(parent) = path.parent() {
                std::fs::create_dir_all(parent).map_err(io)?;
            }
            crate::ini::atomic_write(&path, contents).map_err(io)
        }
    }

    /// `backend()` in a test binary when no `scoped` is active on the thread.
    pub(crate) fn default_backend() -> &'static super::Backend {
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
    pub(crate) fn current() -> &'static super::Backend {
        SCOPED.with(|s| s.get()).unwrap_or_else(default_backend)
    }

    /// Run `f` with `backend()` returning `b` on this thread (not on threads
    /// `f` spawns), then put back what it returned before, even on a panic.
    pub(crate) fn scoped<R>(b: super::Backend, f: impl FnOnce() -> R) -> R {
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
    pub(crate) fn http_stub(log: &Log) -> u16 {
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
    pub(crate) fn backend(client: FakeClient) -> super::Backend {
        let client = std::sync::Mutex::new(Some(client));
        super::Backend::Manager(super::ManagerBackend::new(Box::new(move || {
            let c = client.lock().unwrap().take().expect("connected twice");
            Ok(Box::new(c) as super::Client)
        })))
    }
}

#[cfg(test)]
mod tests {
    use super::fake::{self, FakeClient};
    use super::*;
    use pai_core::client::InstanceState;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::sync::Arc;

    // These touch no path: start and restart (which load server.ini) are
    // pinned in `server_control_e2e`.

    #[test]
    fn manager_stop_stops_default_and_waits_until_it_is_gone() {
        let log = fake::log();
        let b = fake::backend(FakeClient::new(&log, InstanceState::Active));
        assert_eq!(b.stop(), Ok(true));
        assert_eq!(fake::actions(&log), ["stop default"]);
        // The stop is queued, not done, when the manager answers: the seam
        // asks again until the unit is past Deactivating.
        let entries = log.lock().unwrap().clone();
        let after_stop = entries
            .iter()
            .skip_while(|e| *e != "stop default")
            .filter(|e| *e == "status default")
            .count();
        assert!(
            after_stop >= 2,
            "status asked until Inactive, once per state: {entries:?}"
        );
    }

    #[test]
    fn manager_stop_refused_is_the_client_text() {
        let log = fake::log();
        let mut c = FakeClient::new(&log, InstanceState::Active);
        c.fail_actions = Some(ClientError::NotAuthorized);
        let b = fake::backend(c);
        assert_eq!(b.stop(), Err(ClientError::NotAuthorized.text()));
        assert_eq!(fake::actions(&log), ["stop default"]);
    }

    /// A process is there from Activating to Deactivating: it holds the GPU
    /// and the port, which is what every caller of `is_running` asks (the
    /// benchmarks wait for it to be gone before they touch the GPU).
    #[test]
    fn manager_is_running_reads_each_state() {
        for (state, running) in [
            (InstanceState::Active, true),
            (InstanceState::Activating, true),
            (InstanceState::Deactivating, true),
            (InstanceState::Inactive, false),
            (InstanceState::Failed, false),
            (InstanceState::Other("maintenance".into()), false),
        ] {
            let log = fake::log();
            let b = fake::backend(FakeClient::new(&log, state.clone()));
            assert_eq!(b.is_running(), running, "{state:?}");
            assert_eq!(
                log.lock().unwrap().clone(),
                ["status default"],
                "{state:?}: one status probe of default, nothing else"
            );
        }
    }

    #[test]
    fn manager_status_error_reads_as_not_running() {
        for e in [ClientError::Unavailable, ClientError::NotAuthorized] {
            let log = fake::log();
            let mut c = FakeClient::new(&log, InstanceState::Active);
            c.fail_status = Some(e.clone());
            assert!(!fake::backend(c).is_running(), "{e:?}");
        }
    }

    /// No manager: the user reads that the manager is not running, from the
    /// strings file, and nothing panics.
    #[test]
    fn no_manager_says_so_and_reads_as_not_running() {
        let log = fake::log();
        let mut c = FakeClient::new(&log, InstanceState::Inactive);
        c.fail_actions = Some(ClientError::Unavailable);
        c.fail_status = Some(ClientError::Unavailable);
        let b = fake::backend(c);
        assert!(!b.is_running());
        assert_eq!(b.stop(), Err(ClientError::Unavailable.text()));
    }

    #[test]
    fn a_connection_that_fails_is_unavailable() {
        let b = Backend::Manager(ManagerBackend::new(Box::new(|| {
            Err(ClientError::Unavailable)
        })));
        assert!(!b.is_running());
        assert_eq!(b.stop(), Err(ClientError::Unavailable.text()));
    }

    #[test]
    fn the_manager_is_connected_on_first_use_and_once() {
        let connects = Arc::new(AtomicUsize::new(0));
        let log = fake::log();
        let client = std::sync::Mutex::new(Some(FakeClient::new(&log, InstanceState::Active)));
        let counted = connects.clone();
        let b = Backend::Manager(ManagerBackend::new(Box::new(move || {
            counted.fetch_add(1, Ordering::SeqCst);
            let c = client.lock().unwrap().take().expect("connected twice");
            Ok(Box::new(c) as Client)
        })));
        assert_eq!(connects.load(Ordering::SeqCst), 0, "not at construction");
        assert!(b.is_running());
        assert_eq!(b.stop(), Ok(true));
        assert!(!b.is_running());
        assert_eq!(connects.load(Ordering::SeqCst), 1, "once, then reused");
    }

    /// The manager may start after the configurator: a connect that failed is
    /// tried again on the next call, and the client it then makes is used.
    #[test]
    fn a_failed_connect_is_retried_on_the_next_call() {
        let connects = Arc::new(AtomicUsize::new(0));
        let log = fake::log();
        let client = std::sync::Mutex::new(Some(FakeClient::new(&log, InstanceState::Active)));
        let counted = connects.clone();
        let b = Backend::Manager(ManagerBackend::new(Box::new(move || {
            if counted.fetch_add(1, Ordering::SeqCst) == 0 {
                return Err(ClientError::Unavailable);
            }
            let c = client
                .lock()
                .unwrap()
                .take()
                .expect("connected after success");
            Ok(Box::new(c) as Client)
        })));
        assert!(!b.is_running(), "no manager yet");
        assert_eq!(connects.load(Ordering::SeqCst), 1);
        assert!(
            b.is_running(),
            "the manager is up now: the new client answers"
        );
        assert_eq!(connects.load(Ordering::SeqCst), 2, "tried again");
        assert_eq!(b.stop(), Ok(true));
        assert_eq!(connects.load(Ordering::SeqCst), 2, "kept once connected");
        assert_eq!(fake::actions(&log), ["stop default"]);
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn the_backend_on_linux_is_the_manager() {
        assert!(matches!(os_backend(), Backend::Manager(_)));
    }

    #[cfg(not(target_os = "linux"))]
    #[test]
    fn the_backend_off_linux_is_direct() {
        assert!(matches!(os_backend(), Backend::Direct));
    }

    // ── The backend of each OS ──────────────────────────────────────────

    #[cfg(target_os = "linux")]
    #[test]
    fn the_current_os_is_linux_here() {
        assert_eq!(Os::current(), Os::Linux);
    }

    /// The manager runs llama-server on Linux (D-Bus) and Windows (the named
    /// pipe); macOS has no manager.
    #[test]
    fn the_manager_runs_llama_server_on_linux_and_windows_not_on_macos() {
        assert_eq!(backend_kind(Os::Linux), Kind::Manager);
        assert_eq!(backend_kind(Os::Windows), Kind::Manager);
        assert_eq!(backend_kind(Os::MacOs), Kind::Direct);
    }

    /// `os_backend_for` builds the kind `backend_kind` names; the Windows
    /// manager connects through `system_connect`, which a test build never
    /// dials: the instance reads as not running.
    #[test]
    fn the_backend_of_each_os_is_its_kind_and_never_dials_from_a_test() {
        for os in [Os::Linux, Os::Windows] {
            let b = os_backend_for(os);
            assert!(matches!(b, Backend::Manager(_)), "{os:?}");
            assert!(!b.is_running(), "{os:?}: no manager in a test build");
            assert_eq!(b.stop(), Err(ClientError::Unavailable.text()), "{os:?}");
        }
        assert!(matches!(os_backend_for(Os::MacOs), Backend::Direct));
    }

    /// Under the manager the log is the unit's journal on Linux and the
    /// service's file `llama-server@default.log` on Windows, read by the file
    /// tail; `Direct` reads the per-user file on every OS. (The Windows
    /// file's directory is pinned in `server_control_e2e`, under the redirect.)
    #[test]
    fn the_log_is_the_journal_on_linux_and_the_services_file_on_windows() {
        assert_eq!(
            log_source_for(Kind::Manager, Os::Linux),
            LogSource::Journal {
                unit: "llama.cpp@default.service".into()
            }
        );
        match log_source_for(Kind::Manager, Os::Windows) {
            LogSource::File(p) => {
                assert_eq!(p.file_name().unwrap(), "llama-server@default.log", "{p:?}")
            }
            other => panic!("Windows manager reads the service's file, got {other:?}"),
        }
        for os in [Os::Linux, Os::Windows, Os::MacOs] {
            match log_source_for(Kind::Direct, os) {
                LogSource::File(p) => {
                    assert_eq!(p.file_name().unwrap(), "llama-server.log", "{os:?}: {p:?}")
                }
                other => panic!("{os:?}: Direct reads a file, got {other:?}"),
            }
        }
    }

    /// The Settings note under the manager: journald rotates on Linux; on
    /// Windows the service keeps one file per run and the previous run's
    /// (`manager::windows::open_instance_log`), so the note names no journal.
    #[test]
    fn the_rotation_note_names_journald_on_linux_only() {
        assert_eq!(log_rotate_note_key(Os::Linux), "log-rotate-journald");
        let key = log_rotate_note_key(Os::Windows);
        assert_ne!(key, "log-rotate-journald");
        let text = crate::strings::t(key);
        assert_ne!(text, key, "{key} is in the strings file");
        assert!(
            !text.to_lowercase().contains("journal"),
            "the Windows note names no journal: {text}"
        );
    }

    // ── The configuration files ─────────────────────────────────────────

    #[test]
    fn config_files_have_the_manager_names() {
        assert_eq!(ConfigFile::Server.manager_name(), "server@default.ini");
        assert_eq!(ConfigFile::Presets.manager_name(), "presets.ini");
        assert_eq!(
            ConfigFile::PresetsDisabled.manager_name(),
            "presets.ini.disabled"
        );
    }

    #[test]
    fn manager_read_config_reads_each_file_from_the_manager() {
        let log = fake::log();
        let c = FakeClient::new(&log, InstanceState::Inactive)
            .with_file("server@default.ini", "[Server]\nPort = 8099\n")
            .with_file("presets.ini", "[Qwen3.8-27B]\nctx-size = 8192\n");
        let b = fake::backend(c);
        assert_eq!(
            b.read_config(ConfigFile::Server),
            Ok(Some("[Server]\nPort = 8099\n".to_string()))
        );
        assert_eq!(
            b.read_config(ConfigFile::Presets),
            Ok(Some("[Qwen3.8-27B]\nctx-size = 8192\n".to_string()))
        );
        assert_eq!(
            b.read_config(ConfigFile::PresetsDisabled),
            Ok(None),
            "a file the manager does not have is none, not an error"
        );
        assert_eq!(
            log.lock().unwrap().clone(),
            [
                "read_file server@default.ini",
                "read_file presets.ini",
                "read_file presets.ini.disabled"
            ]
        );
    }

    #[test]
    fn manager_write_config_hands_the_contents_to_the_manager() {
        for file in ConfigFile::ALL {
            let log = fake::log();
            let c = FakeClient::new(&log, InstanceState::Inactive);
            let files = c.files.clone();
            let b = fake::backend(c);
            let text = format!("; {file:?}\r\n[m]\r\nctx-size = 4096\r\n");
            assert_eq!(b.write_config(file, &text), Ok(()), "{file:?}");
            let name = file.manager_name();
            assert_eq!(fake::writes(&log), [format!("write_file {name}")]);
            assert_eq!(files.lock().unwrap().get(&name), Some(&text), "{file:?}");
        }
    }

    /// A write the manager refuses reaches the user as the refusal's own text
    /// (in the language of the strings file this process reads), and the
    /// manager's other errors as the client's text.
    #[test]
    fn a_refused_config_write_is_the_refusals_text() {
        let refusal = pai_core::checks::Refusal {
            key: "not-whole-number-in-range",
            values: ["Port", "99999", "1", "65535"].map(String::from).to_vec(),
        };
        for e in [
            ClientError::Invalid(refusal.clone()),
            ClientError::NotAuthorized,
            ClientError::Unavailable,
        ] {
            let log = fake::log();
            let mut c = FakeClient::new(&log, InstanceState::Inactive)
                .with_file("server@default.ini", "[Server]\nPort = 8080\n");
            c.fail_write = Some(e.clone());
            let files = c.files.clone();
            let b = fake::backend(c);
            assert_eq!(
                b.write_config(ConfigFile::Server, "[Server]\nPort = 99999\n"),
                Err(e.text()),
                "{e:?}"
            );
            assert_eq!(
                files.lock().unwrap().get("server@default.ini").cloned(),
                Some("[Server]\nPort = 8080\n".to_string()),
                "{e:?}: the file as it was"
            );
        }
        assert_eq!(
            ClientError::Invalid(refusal.clone()).text(),
            refusal.text(),
            "Invalid is the refusal's text"
        );
    }

    #[test]
    fn a_failed_config_read_is_the_client_text() {
        for e in [ClientError::NotAuthorized, ClientError::Unavailable] {
            let log = fake::log();
            let mut c =
                FakeClient::new(&log, InstanceState::Inactive).with_file("presets.ini", "[m]\n");
            c.fail_read = Some(e.clone());
            let b = fake::backend(c);
            assert_eq!(b.read_config(ConfigFile::Presets), Err(e.text()), "{e:?}");
        }
        let b = Backend::Manager(ManagerBackend::new(Box::new(|| {
            Err(ClientError::Unavailable)
        })));
        assert_eq!(
            b.read_config(ConfigFile::Server),
            Err(ClientError::Unavailable.text()),
            "no connection"
        );
    }

    /// No test binary of pai-config dials the system bus, whatever runs
    /// first: `system_connect` is `Unavailable` in a test build even without
    /// the redirect. The redirect cannot be unset in this process (the e2e
    /// test sets it), so the check runs in a child of this binary with no
    /// redirect and `DBUS_SYSTEM_BUS_ADDRESS` on a socket of ours, which
    /// nothing may connect to.
    #[cfg(target_os = "linux")]
    #[test]
    fn system_connect_never_dials_from_a_test_binary() {
        use std::time::Instant;
        let dir = tempfile::tempdir().expect("tempdir");
        let socket = dir.path().join("system-bus");
        let listener = std::os::unix::net::UnixListener::bind(&socket).expect("bind bus socket");
        listener.set_nonblocking(true).expect("nonblocking");
        let mut child = std::process::Command::new(std::env::current_exe().expect("test binary"))
            .args([
                "--exact",
                "server_control::tests::system_connect_child",
                "--include-ignored",
            ])
            .args(["--nocapture", "--test-threads", "1"])
            .env(CHILD_MARKER, "1")
            .env_remove("PRIVATE_AI_DATA_ROOT")
            .env(
                "DBUS_SYSTEM_BUS_ADDRESS",
                format!("unix:path={}", socket.display()),
            )
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::piped())
            .spawn()
            .expect("run the child test");
        let deadline = Instant::now() + Duration::from_secs(30);
        let status = loop {
            if let Some(status) = child.try_wait().expect("wait") {
                break status;
            }
            if Instant::now() > deadline {
                let _ = child.kill();
                let _ = child.wait();
                panic!("the child did not finish within 30 s");
            }
            std::thread::sleep(Duration::from_millis(50));
        };
        let out = child.wait_with_output().expect("child output");
        let stdout = String::from_utf8_lossy(&out.stdout);
        match listener.accept() {
            Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => {}
            Ok(_) => panic!("system_connect dialed the system bus from a test binary:\n{stdout}"),
            Err(e) => panic!("accept: {e}"),
        }
        assert!(
            status.success(),
            "system_connect_child failed:\n{stdout}\n{}",
            String::from_utf8_lossy(&out.stderr)
        );
        assert!(
            stdout.contains("1 passed"),
            "the child ran no test:\n{stdout}"
        );
    }

    /// Set by `system_connect_never_dials_from_a_test_binary` on its child.
    #[cfg(target_os = "linux")]
    const CHILD_MARKER: &str = "PAI_TEST_SYSTEM_CONNECT_CHILD";

    /// The child half of `system_connect_never_dials_from_a_test_binary`:
    /// ignored, and a no-op without the marker.
    #[cfg(target_os = "linux")]
    #[test]
    #[ignore = "run by system_connect_never_dials_from_a_test_binary in a child process"]
    fn system_connect_child() {
        if std::env::var_os(CHILD_MARKER).is_none() {
            return;
        }
        assert!(std::env::var_os("PRIVATE_AI_DATA_ROOT").is_none());
        let (tx, rx) = std::sync::mpsc::channel();
        std::thread::spawn(move || {
            let _ = tx.send(system_connect().map(|_| ()));
        });
        match rx.recv_timeout(Duration::from_secs(10)) {
            Ok(r) => assert_eq!(r, Err(ClientError::Unavailable)),
            Err(std::sync::mpsc::RecvTimeoutError::Timeout) => {
                panic!("system_connect did not return within 10 s")
            }
            Err(e) => panic!("the probe thread panicked: {e:?}"),
        }
    }

    // ── The log, through journald on the manager ────────────────────────

    #[test]
    fn the_manager_reads_the_log_from_the_units_journal_without_a_call() {
        let log = fake::log();
        let b = fake::backend(FakeClient::new(&log, InstanceState::Active));
        assert_eq!(
            b.log_source(),
            LogSource::Journal {
                unit: "llama.cpp@default.service".into()
            }
        );
        assert!(
            log.lock().unwrap().is_empty(),
            "no manager call to know where the log is"
        );
    }

    #[test]
    fn direct_reads_the_log_from_the_per_user_file() {
        match Backend::Direct.log_source() {
            LogSource::File(p) => {
                assert_eq!(p.file_name().unwrap(), "llama-server.log", "{p:?}")
            }
            other => panic!("Direct reads a file, got {other:?}"),
        }
    }

    /// `LogRotate`/`LogRotateKb` rotate the per-user file; under the manager
    /// journald rotates the journal and they do not apply.
    #[test]
    fn only_direct_rotates_its_log_file() {
        assert!(Backend::Direct.rotates_log_file());
        let log = fake::log();
        let b = fake::backend(FakeClient::new(&log, InstanceState::Active));
        assert!(!b.rotates_log_file());
        assert!(log.lock().unwrap().is_empty());
    }

    /// The manager runs `llama.cpp@<id>.service` (manager/dbus.rs).
    #[test]
    fn the_instance_unit_is_the_managers_template_instance() {
        assert_eq!(instance_unit(), "llama.cpp@default.service");
    }

    /// journalctl(1), systemd 259: `-u` the unit, `-f` follow, `-n` the last
    /// N entries first, `-o cat` the message alone, `--no-pager`. No `-q`:
    /// it hides the access hint `journal_access_denied` reads.
    #[test]
    fn journalctl_args_follow_the_unit_message_only_without_a_pager() {
        assert_eq!(
            journalctl_args("llama.cpp@default.service", 5000),
            [
                "-u",
                "llama.cpp@default.service",
                "-f",
                "-n",
                "5000",
                "-o",
                "cat",
                "--no-pager"
            ]
        );
        let args = journalctl_args("llama.cpp@default.service", 7);
        assert!(
            !args.iter().any(|a| a == "-q" || a == "--quiet"),
            "{args:?}"
        );
    }

    #[test]
    fn journalctl_command_runs_journalctl_with_the_args_unspawned() {
        let args = journalctl_args("llama.cpp@default.service", 5000);
        let cmd = journalctl_command(&args);
        assert_eq!(cmd.get_program(), "journalctl");
        let got: Vec<&std::ffi::OsStr> = cmd.get_args().collect();
        let want: Vec<&std::ffi::OsStr> = args.iter().map(std::ffi::OsStr::new).collect();
        assert_eq!(got, want);
    }

    /// No test reads the real journal: the spawner of a test build spawns
    /// nothing and fails.
    #[test]
    fn the_journal_spawner_of_a_test_build_spawns_nothing() {
        let args = journalctl_args(&instance_unit(), 10);
        if let Ok(mut child) = journal_spawner()(&args) {
            let _ = child.kill();
            let _ = child.wait();
            panic!("a test build spawned a journal reader (pid {})", child.id());
        }
    }

    /// The texts systemd's `journal_access_check_and_warn` and
    /// `access_check_var_log_journal` (src/shared/journal-util.c) print on
    /// stderr when the user is not root and not in a group the journal's ACL
    /// names (systemd-journal, adm, wheel). The hint comes with exit 0 when
    /// the user's own journal opened, the error with a failure exit when
    /// nothing did. Captured from systemd 259 (Ubuntu).
    #[test]
    fn journal_access_denied_reads_systemds_permission_texts() {
        let hint =
            "Hint: You are currently not seeing messages from other users and the system.\n      \
                    Users in groups 'adm', 'systemd-journal', 'wheel' can see all messages.\n      \
                    Pass -q to turn off this notice.\n";
        assert!(journal_access_denied(hint));
        let short = "Hint: You are currently not seeing messages from the system.\n      \
                     Users in the 'systemd-journal' group can see all messages. Pass -q to\n      \
                     turn off this notice.\n";
        assert!(journal_access_denied(short));
        assert!(journal_access_denied(
            "No journal files were opened due to insufficient permissions.\n"
        ));
        // Not a permission problem: there is simply no journal yet, or nothing.
        assert!(!journal_access_denied("No journal files were found.\n"));
        assert!(!journal_access_denied("-- No entries --\n"));
        assert!(!journal_access_denied(""));
    }

    // ── A bounded read of the instance's log (`read_log`, `log_text`) ───
    // For the readers that want the log's end once, not a follower: the
    // memory a benchmark run records, the "server-gone" message. Files are
    // temp files; the journal is a fake runner: nothing reads the real
    // journal or the user's log.

    use std::time::{Duration as Span, SystemTime, UNIX_EPOCH};

    /// A runner that must not be called: a file source never runs journalctl.
    fn no_journal(args: &[String]) -> std::io::Result<std::process::Output> {
        panic!("a file source ran journalctl with {args:?}");
    }

    fn file_source(dir: &tempfile::TempDir, text: &[u8]) -> LogSource {
        let path = dir.path().join("llama-server@default.log");
        std::fs::write(&path, text).expect("write the log");
        LogSource::File(path)
    }

    // Failure paths first.

    /// No file yet (the instance never started): an empty log, not an error.
    #[test]
    fn a_missing_log_file_reads_as_empty() {
        let dir = tempfile::tempdir().unwrap();
        let source = LogSource::File(dir.path().join("llama-server.log"));
        assert_eq!(
            read_log(&source, None, 4096, &no_journal),
            Ok(String::new())
        );
    }

    /// A log that cannot be read (here a directory where the file should be)
    /// is an error with the system's text, not an empty log.
    #[test]
    fn an_unreadable_log_file_is_an_error() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("llama-server.log");
        std::fs::create_dir(&path).unwrap();
        match read_log(&LogSource::File(path), None, 4096, &no_journal) {
            Err(e) => assert!(!e.is_empty(), "the error says why"),
            Ok(t) => panic!("a directory read as a log: {t:?}"),
        }
    }

    /// Bytes that are not UTF-8 (a cut multi-byte char, a binary line) are
    /// read lossily, never refused.
    #[test]
    fn a_log_with_invalid_utf8_reads_lossily() {
        let dir = tempfile::tempdir().unwrap();
        let source = file_source(&dir, b"srv  load: ok\n\xff\xfe broken\nlast line\n");
        let text = read_log(&source, None, 4096, &no_journal).expect("read");
        assert!(text.ends_with("last line\n"), "{text:?}");
        assert!(text.contains('\u{fffd}'), "{text:?}");
    }

    /// A file last written before `since` holds nothing of that window: the
    /// file's lines carry no time, its modification time is the cheap check.
    #[test]
    fn a_log_file_untouched_since_the_window_opened_reads_as_empty() {
        let dir = tempfile::tempdir().unwrap();
        let source = file_source(&dir, b"[52043] old load\n");
        let LogSource::File(path) = &source else {
            unreachable!()
        };
        let written = SystemTime::now() - Span::from_secs(3600);
        std::fs::File::options()
            .write(true)
            .open(path)
            .unwrap()
            .set_modified(written)
            .unwrap();
        let since = written + Span::from_secs(60);
        assert_eq!(
            read_log(&source, Some(since), 4096, &no_journal),
            Ok(String::new())
        );
        // Written inside the window: the whole tail (the lines carry no time).
        let since = written - Span::from_secs(60);
        assert_eq!(
            read_log(&source, Some(since), 4096, &no_journal),
            Ok("[52043] old load\n".to_string())
        );
    }

    /// A short file comes back whole.
    #[test]
    fn a_log_under_the_bound_reads_whole() {
        let dir = tempfile::tempdir().unwrap();
        let text = "srv  main: listening\n[52043] llama_server: model loaded\n";
        let source = file_source(&dir, text.as_bytes());
        assert_eq!(
            read_log(&source, None, 4096, &no_journal),
            Ok(text.to_string())
        );
    }

    /// A long file: only its end, at most `max_bytes`, starting at a line
    /// start (the partial first line dropped), ending with the file's end.
    #[test]
    fn a_log_over_the_bound_reads_its_last_whole_lines() {
        let dir = tempfile::tempdir().unwrap();
        let mut text = String::new();
        for i in 0..1000 {
            text.push_str(&format!("[52043] line {i:04} of the load\n"));
        }
        let source = file_source(&dir, text.as_bytes());
        let max = 100;
        let got = read_log(&source, None, max, &no_journal).expect("read");
        assert!(got.len() <= max, "{} bytes > {max}", got.len());
        assert!(got.ends_with("[52043] line 0999 of the load\n"), "{got:?}");
        assert!(
            got.starts_with("[52043] line "),
            "starts at a line: {got:?}"
        );
        assert!(text.ends_with(&got));
        // 30 bytes a line: 3 whole lines fit in 100 bytes.
        assert_eq!(got.lines().count(), 3, "{got:?}");
    }

    /// A Windows-style file (CRLF) is cut the same way.
    #[test]
    fn a_crlf_log_over_the_bound_starts_at_a_line() {
        let dir = tempfile::tempdir().unwrap();
        let text = "first line that is long\r\nsecond\r\nthird\r\n";
        let source = file_source(&dir, text.as_bytes());
        assert_eq!(
            read_log(&source, None, 18, &no_journal),
            Ok("second\r\nthird\r\n".to_string())
        );
    }

    // ── The journal, once (`journalctl` without `-f`) ───────────────────

    const UNIT: &str = "llama.cpp@default.service";

    /// journalctl(1), systemd 259: `-u` the unit, `-o cat` the message alone,
    /// `--no-pager`, `-n` the last N entries, `-S/--since` entries "on or
    /// newer than" a date, which systemd.time(7) lets be "an integer prefixed
    /// with @ ... relative to the UNIX epoch". No `-f` (one read to the end)
    /// and no `-q` (it hides the access hint `journal_access_denied` reads).
    #[test]
    fn journalctl_once_args_print_the_units_end_without_following() {
        assert_eq!(
            journalctl_once_args(UNIT, 4096, None),
            ["-u", UNIT, "-o", "cat", "--no-pager", "-n", "4096"]
        );
        let since = UNIX_EPOCH + Span::from_millis(1_795_000_123_900);
        assert_eq!(
            journalctl_once_args(UNIT, 10, Some(since)),
            [
                "-u",
                UNIT,
                "-o",
                "cat",
                "--no-pager",
                "-n",
                "10",
                "--since",
                // Whole seconds, rounded down: the entries of that second count.
                "@1795000123"
            ]
        );
        let args = journalctl_once_args(UNIT, 10, Some(since));
        assert!(
            !args
                .iter()
                .any(|a| a == "-f" || a == "--follow" || a == "-q" || a == "--quiet"),
            "{args:?}"
        );
    }

    /// The fake journalctl: records the argv, answers with `exit`, `stdout`
    /// and `stderr`.
    #[cfg(unix)]
    fn journal(
        argv: &Arc<Mutex<Vec<Vec<String>>>>,
        exit: i32,
        stdout: &'static str,
        stderr: &'static str,
    ) -> impl Fn(&[String]) -> std::io::Result<std::process::Output> + Send + Sync {
        use std::os::unix::process::ExitStatusExt;
        let argv = argv.clone();
        move |args: &[String]| {
            argv.lock().unwrap().push(args.to_vec());
            Ok(std::process::Output {
                status: std::process::ExitStatus::from_raw(exit << 8),
                stdout: stdout.as_bytes().to_vec(),
                stderr: stderr.as_bytes().to_vec(),
            })
        }
    }

    fn journal_source() -> LogSource {
        LogSource::Journal { unit: UNIT.into() }
    }

    /// Not root and in none of the journal's groups: journalctl prints the
    /// hint and exits 0 with nothing of the system unit. The same groups
    /// message as the Log window, not an empty log.
    #[cfg(unix)]
    #[test]
    fn a_journal_hint_that_the_system_is_hidden_is_the_groups_message() {
        let argv = Arc::new(Mutex::new(Vec::new()));
        let run = journal(
            &argv,
            0,
            "",
            "Hint: You are currently not seeing messages from other users and the system.\n      \
             Users in groups 'adm', 'systemd-journal', 'wheel' can see all messages.\n      \
             Pass -q to turn off this notice.\n",
        );
        assert_eq!(
            read_log(&journal_source(), None, 4096, &run),
            Err(strings::t("log-journal-denied"))
        );
    }

    #[cfg(unix)]
    #[test]
    fn no_journal_file_opened_is_the_groups_message() {
        let argv = Arc::new(Mutex::new(Vec::new()));
        let run = journal(
            &argv,
            1,
            "",
            "No journal files were opened due to insufficient permissions.\n",
        );
        assert_eq!(
            read_log(&journal_source(), None, 4096, &run),
            Err(strings::t("log-journal-denied"))
        );
    }

    /// Any other failure exit: the Log window's failure message with the
    /// last line journalctl wrote on stderr.
    #[cfg(unix)]
    #[test]
    fn a_journal_read_that_fails_otherwise_says_why() {
        let argv = Arc::new(Mutex::new(Vec::new()));
        let run = journal(
            &argv,
            1,
            "srv  load_model: loading\n",
            "Warning: something\nFailed to iterate through journal: Bad message\n",
        );
        assert_eq!(
            read_log(&journal_source(), None, 4096, &run),
            Err(strings::fill(
                "log-journal-failed",
                &[&"Failed to iterate through journal: Bad message"]
            ))
        );
    }

    #[test]
    fn a_journal_reader_that_cannot_run_says_why() {
        let run = |_: &[String]| -> std::io::Result<std::process::Output> {
            Err(std::io::Error::new(
                std::io::ErrorKind::NotFound,
                "journalctl not found",
            ))
        };
        assert_eq!(
            read_log(&journal_source(), None, 4096, &run),
            Err(strings::fill(
                "log-journal-failed",
                &[&"journalctl not found"]
            ))
        );
    }

    /// The happy path: one run for the unit with the bound as `-n` (every
    /// entry is at least one byte, so the last `max_bytes` entries hold the
    /// last `max_bytes` bytes) and the window as `--since`; stdout is the
    /// log, cut to `max_bytes` from a line start like a file.
    #[cfg(unix)]
    #[test]
    fn a_journal_read_runs_journalctl_once_for_the_unit_and_window() {
        let argv = Arc::new(Mutex::new(Vec::new()));
        let run = journal(
            &argv,
            0,
            "first entry that will be cut off\n[52043] llama_server: model loaded\n",
            "",
        );
        let since = UNIX_EPOCH + Span::from_secs(1_795_000_000);
        let got = read_log(&journal_source(), Some(since), 40, &run);
        assert_eq!(got, Ok("[52043] llama_server: model loaded\n".to_string()));
        assert_eq!(
            *argv.lock().unwrap(),
            [journalctl_once_args(UNIT, 40, Some(since))]
        );
    }

    /// No test reads the real journal: the runner of a test build runs
    /// nothing and fails.
    #[test]
    fn the_journal_runner_of_a_test_build_runs_nothing() {
        let args = journalctl_once_args(&instance_unit(), 10, None);
        if let Ok(out) = journal_runner()(&args) {
            panic!("a test build ran journalctl: {out:?}");
        }
    }

    /// The manager's `log_text` on Linux reads the unit's journal through
    /// `journal_runner()`: in a test build that is the failure message, with
    /// no call to the manager.
    #[cfg(target_os = "linux")]
    #[test]
    fn the_managers_log_text_reads_the_journal_without_a_manager_call() {
        let log = fake::log();
        let b = fake::backend(FakeClient::new(&log, InstanceState::Active));
        match b.log_text(None, 4096) {
            Err(e) => assert!(
                e.starts_with(&strings::fill("log-journal-failed", &[&""])),
                "{e:?}"
            ),
            Ok(t) => panic!("a test build read a journal: {t:?}"),
        }
        assert!(log.lock().unwrap().is_empty(), "no manager call");
    }
}

/// E2E phase (called from `editable_widgets_track_model_after_edit`): the
/// `Manager` backend's start and restart, which load server.ini, and that it
/// leaves the per-user tree to the manager (no presets or binary check, no log
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
            crate::paths::system_config_dir().join(file.manager_name()),
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
    // No presets.ini and no llama-server binary: those are the manager's to
    // check under this backend, not the configurator's.
    assert!(!crate::paths::presets_ini().exists());

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

/// E2E phase (called from `editable_widgets_track_model_after_edit`): every
/// reader and writer of the instance's configuration files goes through the
/// seam. `Direct` keeps the per-user files under their names; the `Manager`
/// backend reads and writes the manager's files and nothing under the
/// per-user config directory; a write the manager refuses reaches the caller
/// (the CLI, the GUI's save status) as the refusal's own text.
#[cfg(test)]
pub(crate) fn config_seam_e2e() {
    use crate::paths;
    use crate::presets::{self, Preset};
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
        (ConfigFile::Presets, paths::presets_ini()),
        (ConfigFile::PresetsDisabled, paths::presets_disabled_ini()),
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
    std::fs::remove_file(paths::presets_disabled_ini()).expect("remove");
    std::fs::create_dir(paths::presets_disabled_ini()).expect("dir in its place");
    assert!(
        d.read_config(ConfigFile::PresetsDisabled).is_err(),
        "an unreadable file is not none"
    );
    std::fs::remove_dir(paths::presets_disabled_ini()).expect("remove dir");

    // ── Manager: the manager's files, shown where the manager keeps them ──
    let log = fake::log();
    let m = fake::backend(FakeClient::new(&log, InstanceState::Inactive));
    for file in ConfigFile::ALL {
        assert_eq!(
            m.config_path(file),
            paths::system_config_dir().join(file.manager_name()),
            "{file:?}"
        );
    }

    // Reads: from the manager, never from the per-user files, which say
    // something else here.
    std::fs::write(paths::server_ini(), "[Server]\nPort = 9999\n").expect("user server.ini");
    std::fs::write(
        paths::presets_ini(),
        "[user-only]\nmodel = /models/user-only.gguf\n",
    )
    .expect("user presets.ini");
    let seeded = || {
        FakeClient::new(&log, InstanceState::Inactive)
            .with_file("server@default.ini", "[Server]\nPort = 8099\n")
            .with_file(
                "presets.ini",
                "[Qwen3.8-27B]\nmodel = /models/Qwen3.8-27B-Q8_0.gguf\n",
            )
            .with_file(
                "presets.ini.disabled",
                "[Qwen3.8-Flash-Next]\nmodel = /models/Qwen3.8-Flash-Next-IQ3_S.gguf\n",
            )
    };
    fake::scoped(fake::backend(seeded()), || {
        assert_eq!(server_cfg::load().port_or_default(), 8099, "server.ini");
        let enabled: Vec<String> = presets::load_all().into_iter().map(|p| p.id).collect();
        assert_eq!(enabled, ["Qwen3.8-27B"], "presets.ini");
        let listed: Vec<(String, bool)> = presets::load_listed()
            .into_iter()
            .map(|l| (l.preset.id, l.enabled))
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
        for name in ["server.ini", "presets.ini", "presets.ini.disabled"] {
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
    let q8 = Preset::new_default(
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
        presets::save(&q8).expect("preset save");
        assert_eq!(fake::writes(&log), ["write_file presets.ini"]);
        assert_eq!(ids(file("presets.ini")), ["Qwen3.8-27B", "Qwen3.8-27B-Q8"]);
        let section = crate::ini::parse_sections(&file("presets.ini"))
            .into_iter()
            .find(|s| s.id == "Qwen3.8-27B-Q8")
            .expect("saved section");
        assert_eq!(
            section.keys.get("model").map(String::as_str),
            Some("/models/Qwen3.8-27B-Q8_0.gguf")
        );
        nothing_per_user("presets::save");

        // Switched off: the destination written first, as with the files.
        log.lock().unwrap().clear();
        presets::set_enabled("Qwen3.8-27B", false).expect("switch off");
        assert_eq!(
            fake::writes(&log),
            ["write_file presets.ini.disabled", "write_file presets.ini"]
        );
        assert_eq!(ids(file("presets.ini")), ["Qwen3.8-27B-Q8"]);
        assert_eq!(
            ids(file("presets.ini.disabled")),
            ["Qwen3.8-Flash-Next", "Qwen3.8-27B"]
        );
        log.lock().unwrap().clear();
        presets::set_enabled("Qwen3.8-27B", true).expect("switch on");
        assert_eq!(
            fake::writes(&log),
            ["write_file presets.ini", "write_file presets.ini.disabled"]
        );
        assert_eq!(ids(file("presets.ini.disabled")), ["Qwen3.8-Flash-Next"]);
        assert_eq!(ids(file("presets.ini")), ["Qwen3.8-27B-Q8", "Qwen3.8-27B"]);

        presets::rename("Qwen3.8-27B-Q8", "Qwen3.8-27B-Q8_0").expect("rename");
        assert_eq!(
            ids(file("presets.ini")),
            ["Qwen3.8-27B-Q8_0", "Qwen3.8-27B"]
        );
        presets::delete("Qwen3.8-Flash-Next").expect("delete");
        assert!(ids(file("presets.ini.disabled")).is_empty());
        nothing_per_user("set_enabled, rename, delete");

        // The first preset saved with no ModelsDir seeds it in the manager's
        // server file.
        files.lock().unwrap().insert(
            "server@default.ini".into(),
            "[Server]\nPort = 8099\n".into(),
        );
        let llm = dir.path().join("llm");
        let model = llm.join("models").join("Qwen3.8-4B-Q4_K_M.gguf");
        presets::save(&Preset::new_default(
            "Qwen3.8-4B".into(),
            model.display().to_string(),
        ))
        .expect("save seeding ModelsDir");
        assert_eq!(
            server_cfg::from_text(&file("server@default.ini")).models_dir,
            Some(llm.display().to_string())
        );

        // A download's preset pins ModelsDir to where its files went.
        files.lock().unwrap().insert(
            "server@default.ini".into(),
            "[Server]\nPort = 8099\n".into(),
        );
        let hf_root = dir.path().join("hf-models");
        let downloaded = Preset::new_default(
            "Qwen3.8-8B".into(),
            hf_root
                .join("models")
                .join("example")
                .join("Qwen3.8-8B-GGUF")
                .join("Qwen3.8-8B-Q4_K_M.gguf")
                .display()
                .to_string(),
        );
        crate::hf::save_preset(&downloaded, &hf_root).expect("download's preset");
        assert_eq!(
            server_cfg::from_text(&file("server@default.ini")).models_dir,
            Some(hf_root.display().to_string())
        );
        assert!(ids(file("presets.ini")).contains(&"Qwen3.8-8B".to_string()));
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
            let err = presets::save(&q8).expect_err("preset save refused");
            assert_eq!(err.to_string(), e.text(), "{e:?}: presets::save");
            let err = presets::set_enabled("Qwen3.8-27B", false).expect_err("switch refused");
            assert_eq!(err.to_string(), e.text(), "{e:?}: set_enabled");
        });
        assert_eq!(*files.lock().unwrap(), before, "{e:?}: nothing changed");
        nothing_per_user("a refused write");
    }

    // ── A read that fails stops the write ────────────────────────────────
    // Writing what an empty read leaves would replace the manager's file
    // with one section, or none: every other preset lost.
    let e = ClientError::Io("PermissionDenied".into());
    let mut c = seeded();
    c.fail_read = Some(e.clone());
    let files = c.files.clone();
    let before = files.lock().unwrap().clone();
    log.lock().unwrap().clear();
    fake::scoped(fake::backend(c), || {
        for (what, result) in [
            ("presets::save", presets::save(&q8)),
            (
                "set_enabled off",
                presets::set_enabled("Qwen3.8-27B", false),
            ),
            (
                "set_enabled on",
                presets::set_enabled("Qwen3.8-Flash-Next", true),
            ),
            ("rename", presets::rename("Qwen3.8-27B", "Qwen3.8-27B-Q8_0")),
            ("delete", presets::delete("Qwen3.8-27B")),
            (
                "hf::save_preset",
                crate::hf::save_preset(&q8, &dir.path().join("hf-models")),
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
