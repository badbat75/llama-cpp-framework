/// Fakes of the two seams that record every call.
#[cfg(test)]
pub(crate) mod fakes {
    use super::super::*;
    use std::sync::Mutex;

    /// One recorded call to the service manager.
    #[derive(Debug, Clone, PartialEq, Eq)]
    pub enum Call {
        Start(String),
        Stop(String),
        Restart(String),
        Status(String),
        BootEnabled(String),
        SetBootEnabled(String, bool),
        Remove(String),
    }

    /// A service manager that records calls, reports `state`, starts at boot
    /// the ids in `enabled` and fails every call for the ids in `failing`.
    /// `failing_remove` fails only `remove`, so a remove that fails halfway is
    /// testable beside a status that answers.
    pub struct FakeService {
        pub calls: Mutex<Vec<Call>>,
        pub state: UnitState,
        /// The units `status` answers active, whatever `state` is: the state of
        /// a unit is its own, and a remove refuses the running ones.
        pub running: Vec<String>,
        pub failing: Vec<String>,
        pub failing_remove: Vec<String>,
        pub enabled: Mutex<std::collections::BTreeSet<String>>,
    }

    impl FakeService {
        pub fn new() -> Self {
            Self {
                calls: Mutex::new(Vec::new()),
                state: UnitState::Active,
                running: Vec::new(),
                failing: Vec::new(),
                failing_remove: Vec::new(),
                enabled: Mutex::new(Default::default()),
            }
        }

        fn record(&self, call: Call, id: &str) -> Result<(), ServiceError> {
            self.calls.lock().unwrap().push(call);
            if self.failing.iter().any(|f| f == id) {
                return Err(ServiceError(format!("llama.cpp@{id}.service failed")));
            }
            Ok(())
        }
    }

    impl FakeService {
        /// This service answering every unit as stopped. `#[cfg(unix)]`: its
        /// only callers are the D-Bus phases, run on Linux only, so a Windows
        /// `--all-targets` check would see dead code.
        #[cfg(unix)]
        pub fn stopped(mut self) -> Self {
            self.state = UnitState::Inactive;
            self
        }

        /// This service answering the units named in `ids` as active, and the
        /// rest as its `state` is. `#[cfg(unix)]`: its only caller is the
        /// pipe-client phase, run on Unix only, so a Windows `--all-targets`
        /// check would see dead code.
        #[cfg(unix)]
        pub fn running(mut self, ids: &[&str]) -> Self {
            self.running = ids.iter().map(|s| (*s).to_string()).collect();
            self
        }
    }

    impl ServiceControl for FakeService {
        fn start(&self, id: &str) -> Result<(), ServiceError> {
            self.record(Call::Start(id.into()), id)
        }
        fn stop(&self, id: &str) -> Result<(), ServiceError> {
            self.record(Call::Stop(id.into()), id)
        }
        fn restart(&self, id: &str) -> Result<(), ServiceError> {
            self.record(Call::Restart(id.into()), id)
        }
        fn status(&self, id: &str) -> Result<UnitState, ServiceError> {
            self.record(Call::Status(id.into()), id)?;
            Ok(if self.running.iter().any(|name| name == id) {
                UnitState::Active
            } else {
                self.state
            })
        }
        fn boot_enabled(&self, id: &str) -> Result<bool, ServiceError> {
            self.record(Call::BootEnabled(id.into()), id)?;
            Ok(self.enabled.lock().unwrap().contains(id))
        }
        fn set_boot_enabled(&self, id: &str, enabled: bool) -> Result<(), ServiceError> {
            self.record(Call::SetBootEnabled(id.into(), enabled), id)?;
            let mut set = self.enabled.lock().unwrap();
            if enabled {
                set.insert(id.to_string());
            } else {
                set.remove(id);
            }
            Ok(())
        }
        fn remove(&self, id: &str) -> Result<(), ServiceError> {
            self.record(Call::Remove(id.into()), id)?;
            if self.failing_remove.iter().any(|name| name == id) {
                return Err(ServiceError(format!("remove {id} fails")));
            }
            self.enabled.lock().unwrap().remove(id);
            Ok(())
        }
    }

    /// A resolver over a fixed table; any other name does not resolve.
    pub struct FakeResolver {
        pub table: Vec<(&'static str, Vec<IpAddr>)>,
        pub asked: Mutex<Vec<String>>,
    }

    impl FakeResolver {
        pub fn new(table: &[(&'static str, &[&str])]) -> Self {
            Self {
                table: table
                    .iter()
                    .map(|(name, ips)| {
                        (
                            *name,
                            ips.iter().map(|ip| ip.parse().expect("ip")).collect(),
                        )
                    })
                    .collect(),
                asked: Mutex::new(Vec::new()),
            }
        }

        /// The names the e2e uses: localhost, a LAN box with two names, a
        /// multi-homed one.
        pub fn lan() -> Self {
            Self::new(&[
                ("localhost", &["127.0.0.1"]),
                ("gpu-box", &["192.168.1.5"]),
                ("gpu-box.lan", &["192.168.1.5"]),
                ("multi-home", &["10.0.0.9", "192.168.1.5"]),
            ])
        }
    }

    impl Resolver for FakeResolver {
        fn resolve(&self, host: &str) -> Result<Vec<IpAddr>, ResolveError> {
            self.asked.lock().unwrap().push(host.to_string());
            self.table
                .iter()
                .find(|(name, _)| *name == host)
                .map(|(_, ips)| ips.clone())
                .ok_or_else(|| ResolveError(host.to_string()))
        }
    }

    /// An authority that answers `allow` and records every question; the
    /// questions asked without interaction answer `quiet` (`allow` unless
    /// set with [`FakeAuthority::quietly`]) and are recorded apart.
    /// Only the caller set with [`FakeAuthority::bff`] is the bff's account
    /// (nobody, unless set); those questions are recorded in `asked_bff`.
    pub struct FakeAuthority {
        pub allow: bool,
        pub quiet: bool,
        pub bff: Option<Caller>,
        pub asked: Mutex<Vec<(Caller, Action)>>,
        pub asked_quietly: Mutex<Vec<(Caller, Action)>>,
        pub asked_bff: Mutex<Vec<Caller>>,
    }

    impl FakeAuthority {
        pub fn new(allow: bool) -> Self {
            Self {
                allow,
                quiet: allow,
                bff: None,
                asked: Mutex::new(Vec::new()),
                asked_quietly: Mutex::new(Vec::new()),
                asked_bff: Mutex::new(Vec::new()),
            }
        }

        /// This authority taking `caller` for the bff's account.
        pub fn bff(mut self, caller: &Caller) -> Self {
            self.bff = Some(caller.clone());
            self
        }

        /// This authority answering `quiet` to the questions asked without
        /// interaction: false is a caller polkit would challenge.
        pub fn quietly(mut self, quiet: bool) -> Self {
            self.quiet = quiet;
            self
        }
    }

    impl Authority for FakeAuthority {
        fn is_authorized(&self, caller: &Caller, action: Action) -> bool {
            self.asked.lock().unwrap().push((caller.clone(), action));
            self.allow
        }
        fn is_authorized_quietly(&self, caller: &Caller, action: Action) -> bool {
            self.asked_quietly
                .lock()
                .unwrap()
                .push((caller.clone(), action));
            self.quiet
        }
        fn is_bff(&self, caller: &Caller) -> bool {
            self.asked_bff.lock().unwrap().push(caller.clone());
            self.bff.as_ref() == Some(caller)
        }
    }

    /// The accounts of the authentication tests: each `(user, password,
    /// admin)`; any other user is unknown. `fail` makes every check fail
    /// with that message. Records each check (user and password, to pin that
    /// they arrive unchanged) and each admin question.
    /// The (user, password) pairs a [`FakePasswords`] was asked to check.
    pub type Checked = std::sync::Arc<Mutex<Vec<(String, String)>>>;

    pub struct FakePasswords {
        pub accounts: Vec<(&'static str, &'static str, bool)>,
        /// The accounts [`is_admin`](PasswordChecker::is_admin) answers true
        /// for, seeded from `accounts` and shared so a test can remove a
        /// member between two calls of one manager (the manager must not
        /// cache membership).
        pub admins: std::sync::Arc<Mutex<std::collections::BTreeSet<String>>>,
        pub fail: Option<String>,
        pub checked: Checked,
        pub asked_admin: std::sync::Arc<Mutex<Vec<String>>>,
    }

    impl FakePasswords {
        /// alice (an administrator) and bob (not one).
        pub fn new() -> Self {
            let accounts = vec![("alice", "s3cret-Pw", true), ("bob", "hunter2!", false)];
            let admins = accounts
                .iter()
                .filter(|(_, _, admin)| *admin)
                .map(|(u, _, _)| (*u).to_string())
                .collect();
            Self {
                accounts,
                admins: std::sync::Arc::new(Mutex::new(admins)),
                fail: None,
                checked: Default::default(),
                asked_admin: Default::default(),
            }
        }

        pub fn failing(message: &str) -> Self {
            Self {
                fail: Some(message.to_string()),
                ..Self::new()
            }
        }
    }

    impl PasswordChecker for FakePasswords {
        fn check(&self, user: &str, password: &str) -> Result<bool, PasswordError> {
            self.checked
                .lock()
                .unwrap()
                .push((user.to_string(), password.to_string()));
            if let Some(message) = &self.fail {
                return Err(PasswordError(message.clone()));
            }
            Ok(self
                .accounts
                .iter()
                .any(|(u, p, _)| *u == user && *p == password))
        }
        fn is_admin(&self, user: &str) -> bool {
            self.asked_admin.lock().unwrap().push(user.to_string());
            self.admins.lock().unwrap().contains(user)
        }
    }

    /// A group seam that changes no group: it records each call, with the
    /// file's mode at that moment (Unix; 0 elsewhere), and fails with
    /// `fail` when set.
    pub struct FakeGroup {
        pub fail: Option<std::io::ErrorKind>,
        pub calls: std::sync::Arc<Mutex<Vec<GroupCall>>>,
    }

    /// One recorded [`FakeGroup`] call.
    #[derive(Debug, Clone, PartialEq, Eq)]
    pub struct GroupCall {
        pub path: std::path::PathBuf,
        pub group: String,
        pub mode: u32,
    }

    impl FakeGroup {
        pub fn new() -> Self {
            Self {
                fail: None,
                calls: Default::default(),
            }
        }

        pub fn failing(kind: std::io::ErrorKind) -> Self {
            Self {
                fail: Some(kind),
                ..Self::new()
            }
        }
    }

    impl FileGroup for FakeGroup {
        fn set_group(&self, path: &std::path::Path, group: &str) -> std::io::Result<()> {
            #[cfg(unix)]
            let mode = {
                use std::os::unix::fs::PermissionsExt;
                std::fs::metadata(path).map_or(0, |m| m.permissions().mode() & 0o7777)
            };
            #[cfg(not(unix))]
            let mode = 0;
            self.calls.lock().unwrap().push(GroupCall {
                path: path.to_path_buf(),
                group: group.to_string(),
                mode,
            });
            match self.fail {
                Some(kind) => Err(kind.into()),
                None => Ok(()),
            }
        }
    }

    /// One entry of the fake's device list: `(id, name, backend, total MiB,
    /// free MiB)`, and the whole list with the probe's moment beside it.
    type FakeDeviceRow = (String, String, String, u64, u64);
    type FakeDeviceList = (Option<u64>, Vec<FakeDeviceRow>);

    /// A GPU pool the memory check measures against: `gpus` answered as the
    /// node's cards, each written as `(device id, name, total MiB)`, and every
    /// answer recorded (lowered ids), so a test can pin how often the manager
    /// probed.
    pub struct FakeGpus {
        pub gpus: Option<Vec<(String, String, u64)>>,
        /// The device list G2's `Devices()` answers: the probe's moment
        /// (`None`: a probe that answered nothing) and the entries, each
        /// `(id, name, backend, total MiB, free MiB)`. `None` (the default):
        /// the seam speaks no device list, as a manager of an older protocol.
        pub devices: Option<FakeDeviceList>,
        /// The host's inventory R1 answers beside the list, `(ram_mib,
        /// cpu_threads)`. `None` (the default): the fake answers 0 and 0, the
        /// pair no test of these fakes asserts.
        /// The fake NEVER reads the host: every reading is the one the test
        /// handed, so a test that sees a number it did not write is reading
        /// `/proc/meminfo` / `GlobalMemoryStatusEx` / the CPU count in
        /// production, which a test must never do.
        pub inventory: Option<(u64, u32)>,
        pub asked: std::sync::Arc<Mutex<Vec<String>>>,
    }

    impl FakeGpus {
        pub fn new(gpus: &[(&str, u64)]) -> Self {
            Self {
                gpus: Some(
                    gpus.iter()
                        .map(|(id, mib)| (id.to_string(), format!("NVIDIA GeForce {id}"), *mib))
                        .collect(),
                ),
                devices: None,
                inventory: None,
                asked: std::sync::Arc::new(Mutex::new(Vec::new())),
            }
        }
        /// This seam answering the host's inventory as `ram_mib` MiB of RAM and
        /// `cpu_threads` logical CPUs.
        pub fn with_inventory(mut self, ram_mib: u64, cpu_threads: u32) -> Self {
            self.inventory = Some((ram_mib, cpu_threads));
            self
        }
        /// This seam answering `devices` as `list`, probed at `probed_at`.
        pub fn with_devices(
            mut self,
            probed_at: Option<u64>,
            list: &[(&str, &str, &str, u64, u64)],
        ) -> Self {
            self.devices = Some((
                probed_at,
                list.iter()
                    .map(|(id, name, backend, total, free)| {
                        (
                            id.to_string(),
                            name.to_string(),
                            backend.to_string(),
                            *total,
                            *free,
                        )
                    })
                    .collect(),
            ));
            self
        }
        /// A seam whose probe answered nothing: no time, no device.
        pub fn no_devices(mut self) -> Self {
            self.devices = Some((None, Vec::new()));
            self
        }
        /// A node with no card: the seam answers `None`.
        pub fn none() -> Self {
            Self {
                gpus: None,
                devices: None,
                inventory: None,
                asked: std::sync::Arc::new(Mutex::new(Vec::new())),
            }
        }
    }

    impl GpuTotals for FakeGpus {
        fn gpus(&self) -> Option<Vec<pai_core::devices::Gpu>> {
            self.gpus.as_ref().map(|list| {
                self.asked.lock().unwrap().push("gpus".to_string());
                list.iter()
                    .map(|(id, name, mib)| pai_core::devices::Gpu {
                        id: id.clone(),
                        name: name.clone(),
                        total_mib: *mib,
                    })
                    .collect()
            })
        }
        fn devices(&self) -> pai_core::client::NodeDevices {
            self.asked.lock().unwrap().push("devices".to_string());
            let (ram_mib, cpu_threads) = self.inventory.unwrap_or((0, 0));
            self.devices
                .as_ref()
                .map(|(probed_at, list)| pai_core::client::NodeDevices {
                    probed_at: *probed_at,
                    ram_mib,
                    cpu_threads,
                    devices: list
                        .iter()
                        .map(
                            |(id, name, backend, total, free)| pai_core::client::DeviceInfo {
                                id: id.clone(),
                                name: name.clone(),
                                backend: backend.clone(),
                                total_mib: *total,
                                free_mib: *free,
                            },
                        )
                        .collect(),
                })
                .unwrap_or(pai_core::client::NodeDevices {
                    probed_at: None,
                    ram_mib,
                    cpu_threads,
                    devices: vec![],
                })
        }
    }

    /// The node's own facts (AN3), every one written by the test: the
    /// operating system, the kernel's GPU rows, and the Vulkan child's answer.
    /// The fake reads no `os-release`, no `/sys` and no Vulkan driver, and it
    /// spawns no child, so a reply carrying these figures is the fake's answer
    /// and nothing else. Every reading is recorded (`os`, `gpus`, `vulkan`), so
    /// a test pins that a refusal read nothing and spawned nothing.
    pub struct FakeNodeFacts {
        pub os: pai_core::host::OsInfo,
        pub rows: Vec<pai_core::host::GpuDriver>,
        pub vulkan: pai_core::vulkan::Probe,
        pub asked: std::sync::Arc<Mutex<Vec<String>>>,
    }

    /// One text of a fixture row: `""` is a text the host did not answer, kept
    /// as `None`.
    fn text(one: &str) -> Option<String> {
        (!one.is_empty()).then(|| one.to_string())
    }

    impl FakeNodeFacts {
        /// A node whose os-release answers `name` and `version` and whose
        /// kernel is `kernel`, with no kernel row and a Vulkan read that gave
        /// nothing to trust.
        pub fn new(name: &str, version: &str, kernel: &str) -> Self {
            Self {
                os: pai_core::host::OsInfo {
                    name: text(name),
                    version: text(version),
                    kernel: text(kernel),
                },
                rows: Vec::new(),
                vulkan: pai_core::vulkan::Probe {
                    status: pai_core::vulkan::Status::Failed,
                    devices: Vec::new(),
                },
                asked: std::sync::Arc::new(Mutex::new(Vec::new())),
            }
        }
        /// This node answering the kernel's rows as `rows`, each
        /// `(pci, vendor, name, driver, version)`, the bus id as sysfs names it
        /// and `""` for a text the host did not answer.
        pub fn with_kernel_rows(mut self, rows: &[(&str, &str, &str, &str, &str)]) -> Self {
            self.rows = rows
                .iter()
                .map(
                    |(pci, vendor, name, driver, version)| pai_core::host::GpuDriver {
                        pci: (*pci).to_string(),
                        vendor: text(vendor),
                        name: text(name),
                        driver: text(driver),
                        version: text(version),
                    },
                )
                .collect();
            self
        }
        /// This node's Vulkan read answering `status` with `devices`, each
        /// `(pci, name, cpu, driver, info)`, `pci` in sysfs' form and `""` for
        /// a text the child printed none of.
        pub fn with_vulkan(
            mut self,
            status: pai_core::vulkan::Status,
            devices: &[(&str, &str, bool, &str, &str)],
        ) -> Self {
            self.vulkan = pai_core::vulkan::Probe {
                status,
                devices: devices
                    .iter()
                    .map(|(pci, name, cpu, driver, info)| pai_core::vulkan::Device {
                        pci: text(pci),
                        name: text(name),
                        driver: text(driver),
                        info: text(info),
                        cpu: *cpu,
                    })
                    .collect(),
            };
            self
        }
    }

    impl NodeFacts for FakeNodeFacts {
        fn os(&self) -> pai_core::host::OsInfo {
            self.asked.lock().unwrap().push("os".to_string());
            self.os.clone()
        }
        fn gpus(&self) -> Vec<pai_core::host::GpuDriver> {
            self.asked.lock().unwrap().push("gpus".to_string());
            self.rows.clone()
        }
        fn vulkan(&self) -> pai_core::vulkan::Probe {
            self.asked.lock().unwrap().push("vulkan".to_string());
            self.vulkan.clone()
        }
    }

    /// An action log that keeps every record line the manager renders, so a
    /// test pins the records whole: one per guarded call, refusals included.
    #[derive(Default)]
    pub struct FakeActionLog {
        pub lines: std::sync::Arc<Mutex<Vec<String>>>,
    }

    impl FakeActionLog {
        pub fn new() -> Self {
            Self::default()
        }
    }

    impl ActionLog for FakeActionLog {
        fn record(&self, line: &str) {
            self.lines.lock().unwrap().push(line.to_string());
        }
    }

    /// The longest a gate holds a call, and the longest a test waits for one:
    /// a test that never opens its gate fails instead of hanging.
    pub const GATE_WAIT: std::time::Duration = std::time::Duration::from_secs(5);

    /// A gate a fake seam holds its calls at until the test opens it, as
    /// polkit holds `CheckAuthorization` while a password prompt is up.
    pub struct Gate {
        /// How many calls arrived, and whether the gate is open.
        state: Mutex<(usize, bool)>,
        changed: std::sync::Condvar,
    }

    impl Gate {
        pub fn new() -> std::sync::Arc<Self> {
            std::sync::Arc::new(Self {
                state: Mutex::new((0, false)),
                changed: std::sync::Condvar::new(),
            })
        }

        /// For the fake: count this call and wait for the gate, at most
        /// [`GATE_WAIT`]. Whether it was opened.
        pub fn pass(&self) -> bool {
            let mut state = self.state.lock().unwrap();
            state.0 += 1;
            self.changed.notify_all();
            let (state, _) = self
                .changed
                .wait_timeout_while(state, GATE_WAIT, |s| !s.1)
                .unwrap();
            state.1
        }

        /// For the test: whether `n` calls reached the gate within [`GATE_WAIT`].
        pub fn arrived(&self, n: usize) -> bool {
            let state = self.state.lock().unwrap();
            let (state, _) = self
                .changed
                .wait_timeout_while(state, GATE_WAIT, |s| s.0 < n)
                .unwrap();
            state.0 >= n
        }

        /// Let every held call, and every later one, through.
        pub fn open(&self) {
            self.state.lock().unwrap().1 = true;
            self.changed.notify_all();
        }
    }

    /// An authority that holds the questions about `gated` at `gate`, as
    /// polkit does while that user types a password, then answers from
    /// `answers` (a caller not listed is refused). Records every question.
    pub struct GateAuthority {
        pub gated: Caller,
        pub gate: std::sync::Arc<Gate>,
        pub answers: Vec<(Caller, bool)>,
        pub asked: Mutex<Vec<(Caller, Action)>>,
    }

    impl GateAuthority {
        pub fn new(
            gated: &Caller,
            gate: &std::sync::Arc<Gate>,
            answers: &[(&Caller, bool)],
        ) -> Self {
            Self {
                gated: gated.clone(),
                gate: gate.clone(),
                answers: answers.iter().map(|(c, a)| ((*c).clone(), *a)).collect(),
                asked: Mutex::new(Vec::new()),
            }
        }
    }

    impl Authority for GateAuthority {
        fn is_authorized(&self, caller: &Caller, action: Action) -> bool {
            self.asked.lock().unwrap().push((caller.clone(), action));
            if *caller == self.gated && !self.gate.pass() {
                return false;
            }
            self.answers.iter().any(|(c, allow)| c == caller && *allow)
        }
        /// Never held: a quiet question prompts nobody. Not recorded.
        fn is_authorized_quietly(&self, caller: &Caller, _action: Action) -> bool {
            self.answers.iter().any(|(c, allow)| c == caller && *allow)
        }
        /// Nobody is the bff here.
        fn is_bff(&self, _caller: &Caller) -> bool {
            false
        }
    }

    /// A [`FakeResolver`] that holds the first lookup of `host` at `gate`
    /// (a slow DNS answer); later lookups answer at once.
    pub struct GateResolver {
        pub inner: FakeResolver,
        pub host: &'static str,
        pub gate: std::sync::Arc<Gate>,
        held: std::sync::atomic::AtomicBool,
    }

    impl GateResolver {
        pub fn new(inner: FakeResolver, host: &'static str, gate: &std::sync::Arc<Gate>) -> Self {
            Self {
                inner,
                host,
                gate: gate.clone(),
                held: std::sync::atomic::AtomicBool::new(false),
            }
        }
    }

    impl Resolver for GateResolver {
        fn resolve(&self, host: &str) -> Result<Vec<IpAddr>, ResolveError> {
            use std::sync::atomic::Ordering;
            if host == self.host && !self.held.swap(true, Ordering::SeqCst) {
                self.gate.pass();
            }
            self.inner.resolve(host)
        }
    }
}

/// A file as read by a caller nothing was withheld from.
#[cfg(test)]
pub(crate) fn plain(text: &str) -> pai_core::client::ManagerFile {
    pai_core::client::ManagerFile {
        text: text.into(),
        withheld: false,
    }
}

/// The file `text` as read by a caller the API key was withheld from.
#[cfg(test)]
pub(crate) fn withheld(text: &str) -> pai_core::client::ManagerFile {
    pai_core::client::ManagerFile {
        text: text.into(),
        withheld: true,
    }
}

/// Run `f` on a thread of its own; its result arrives on the receiver, read
/// with `recv_timeout` so a call that never returns fails the test instead
/// of hanging it.
#[cfg(test)]
pub(crate) fn on_thread<T: Send + 'static>(
    f: impl FnOnce() -> T + Send + 'static,
) -> std::sync::mpsc::Receiver<T> {
    let (tx, rx) = std::sync::mpsc::channel();
    std::thread::spawn(move || {
        let _ = tx.send(f());
    });
    rx
}

/// The record of the instances, written by a fixture: `instances.ini` in the
/// engine's config directory, one section per id (`paths::instance_ids` reads
/// the sections), each naming the server profile of the same name, the key the
/// launch reads (`launch::instance_command`). The manager adds a section when
/// it creates an instance, and that is a later slice, so a fixture that wants
/// an instance hands it the record. The whole file is rewritten: a section is
/// dropped with the instance it stands for.
#[cfg(test)]
fn write_record(dir: &std::path::Path, ids: &[&str]) {
    std::fs::create_dir_all(dir).expect("engine config dir");
    std::fs::write(
        dir.join(INSTANCES_FILE),
        ids.iter()
            .map(|id| format!("[{id}]\nServerProfile = {id}\n"))
            .collect::<Vec<String>>()
            .join("\n"),
    )
    .expect(INSTANCES_FILE);
}

/// The render `models@<id>.ini` of the engine's config directory `dir`, with one
/// model profile in it: a start loads the render and refuses one with nothing to
/// run, and a fixture that seeds the pool by hand writes no render of its own.
#[cfg(test)]
fn write_render(dir: &std::path::Path, id: &str) {
    std::fs::create_dir_all(dir).expect("engine config dir");
    std::fs::write(
        dir.join(format!("models@{id}.ini")),
        "[seed]\nmodel = /models/seed.gguf\n",
    )
    .expect("render");
}

/// A fixture's model profile text with files the node has. A write that puts a
/// model profile in effect is checked for its model files, so a fixture whose
/// pool is in effect has to name files that exist: each `model` value becomes
/// an empty file under `dir/models` and is replaced by its path. The ids, the
/// other keys, the comments and the order of the text are kept.
#[cfg(test)]
pub(crate) fn real_model_files(dir: &std::path::Path, text: &str) -> String {
    let root = dir.join("models");
    std::fs::create_dir_all(&root).expect("models dir");
    let mut out = String::new();
    for line in text.lines() {
        let Some((key, value)) = line.split_once('=') else {
            out.push_str(line);
            out.push('\n');
            continue;
        };
        if !key.trim().eq_ignore_ascii_case("model") {
            out.push_str(line);
            out.push('\n');
            continue;
        }
        // What follows the first `;` or `#` is a comment (`ini::parse_sections`).
        let value = &value[..value.find([';', '#']).unwrap_or(value.len())];
        let value = value.trim();
        if value.is_empty() {
            out.push_str(line);
            out.push('\n');
            continue;
        }
        let name = std::path::Path::new(value)
            .file_name()
            .unwrap_or(std::ffi::OsStr::new("model"));
        let path = root.join(name);
        std::fs::write(&path, "").expect("model file");
        out.push_str("model = ");
        out.push_str(&path.to_string_lossy());
        out.push('\n');
    }
    out
}

/// The umask window of the mode pins: the constructor masks the process with
/// the mode it is given and keeps the previous umask, `Drop` restores it. The
/// umask is process-wide and the e2e phases run on parallel threads, so a
/// panic between the mask and the restore must not leave 0o077 on the process.
#[cfg(all(test, target_os = "linux"))]
struct UmaskGuard(nix::sys::stat::Mode);

#[cfg(all(test, target_os = "linux"))]
impl UmaskGuard {
    /// Masks the process with `mask`, answering the guard that restores the
    /// umask it found when it is dropped.
    fn mask(mask: nix::sys::stat::Mode) -> Self {
        Self(nix::sys::stat::umask(mask))
    }
}

#[cfg(all(test, target_os = "linux"))]
impl Drop for UmaskGuard {
    fn drop(&mut self) {
        nix::sys::stat::umask(self.0);
    }
}

/// E2E phase for the manager core, for this crate's e2e test to call: it sets
/// the process-wide `PRIVATE_AI_DATA_ROOT` redirect (`src/tests/mod.rs`).
/// Nothing here touches D-Bus, systemd, polkit or the GPUs: both seams are
/// fakes.
#[cfg(test)]
pub(crate) fn manager_e2e() {
    use fakes::{Call, FakeAuthority, FakeResolver, FakeService};
    use std::collections::BTreeSet;

    let dir = tempfile::tempdir().expect("tempdir");
    crate::paths::redirect_for_tests(dir.path());
    let sys = crate::paths::engine_config_dir();
    let alice = Caller(":1.42".into());
    let files_in = |p: &std::path::Path| -> BTreeSet<String> {
        std::fs::read_dir(p)
            .map(|rd| {
                rd.flatten()
                    .map(|e| e.file_name().to_string_lossy().into_owned())
                    .collect()
            })
            .unwrap_or_default()
    };
    let set =
        |names: &[&str]| -> BTreeSet<String> { names.iter().map(|s| s.to_string()).collect() };
    // An instance is a section of the record, not a file in the directory: the
    // fixture hands the manager the record it reads (`write_record`).
    let record = |ids: &[&str]| write_record(&sys, ids);
    let bad_names = |sys: &std::path::Path| -> Vec<String> {
        vec![
            "../x".into(),
            "../models.ini".into(),
            "server.ini".into(),
            "settings.ini".into(),
            "server@Bad.ini".into(),
            "server@.ini".into(),
            "server@a_b.ini".into(),
            "server@default.ini.tmp".into(),
            "server@default".into(),
            "models.ini.bak".into(),
            "models@a.ini".into(),
            "models@default.ini".into(),
            "sub/models.ini".into(),
            "".into(),
            sys.join("models.ini").to_string_lossy().into_owned(),
            "/etc/passwd".into(),
            "Presets.ini".into(),
        ]
    };

    // ── A refused caller: nothing happens, not even the directory ────────
    {
        let m = Manager::new(
            FakeService::new(),
            FakeAuthority::new(false),
            FakeResolver::lan(),
        )
        .with_group(crate::manager::service::fakes::FakeGroup::new());
        assert_eq!(
            m.write_file(
                &alice,
                "server@default.ini",
                "[Server]\nPort = 8081\n",
                false
            ),
            Err(ManagerError::NotAuthorized),
            "refused write of an instance file"
        );
        assert_eq!(
            m.write_file(&alice, "models.ini", "[m]\nmodel = /m/a.gguf\n", false),
            Err(ManagerError::NotAuthorized),
            "refused write of models.ini"
        );
        assert!(
            !sys.exists(),
            "a refused write must not create the directory"
        );
        assert_eq!(
            *m.authority().asked.lock().unwrap(),
            vec![
                (alice.clone(), Action::Configure),
                (alice.clone(), Action::Configure)
            ],
            "writes ask for the configure action, for this caller"
        );
    }

    // ── No directory yet: no instances, unknown ids refused ──────────────
    {
        let m = Manager::new(
            FakeService::new(),
            FakeAuthority::new(true),
            FakeResolver::lan(),
        )
        .with_group(crate::manager::service::fakes::FakeGroup::new());
        assert_eq!(m.list_instances(), Vec::<String>::new());
        assert_eq!(m.start(&alice, "default"), Err(ManagerError::NoInstance));
        assert_eq!(m.restart(&alice, "default"), Err(ManagerError::NoInstance));
        for bad in ["", "../x", "..", "a/b", "Default", "a_b", "-a", "x.ini"] {
            assert_eq!(
                m.status(bad),
                Err(ManagerError::InvalidId),
                "status {bad:?}"
            );
            assert_eq!(
                m.start(&alice, bad),
                Err(ManagerError::InvalidId),
                "start {bad:?}"
            );
            assert_eq!(
                m.stop(&alice, bad),
                Err(ManagerError::InvalidId),
                "stop {bad:?}"
            );
            assert_eq!(
                m.restart(&alice, bad),
                Err(ManagerError::InvalidId),
                "restart {bad:?}"
            );
        }
        assert_eq!(
            m.read_file(Some(&alice), "models.ini"),
            Ok(None),
            "missing file reads as None"
        );
        assert!(
            m.service().calls.lock().unwrap().is_empty(),
            "no service call for a refused id"
        );
        assert!(!sys.exists(), "asking must not create the directory");
        // Stop and status need only a valid id: an instance whose file was
        // deleted while its unit runs stays visible and stoppable.
        assert_eq!(m.status("default"), Ok(UnitState::Active));
        assert_eq!(m.stop(&alice, "default"), Ok(()));
        assert_eq!(
            *m.service().calls.lock().unwrap(),
            vec![Call::Status("default".into()), Call::Stop("default".into())],
            "status and stop of a valid id with no file reach the service"
        );
        assert!(
            !sys.exists(),
            "stop and status must not create the directory"
        );
    }

    // ── Writes: the directory created, atomic, no temp file left ─────────
    let default_ini = "[Server]\nPort = 8081\nHostname = 127.0.0.1\n";
    let model_profiles = real_model_files(
        dir.path(),
        "[Qwen3.8-27B]\nmodel = /m/Qwen3.8-27B-Q8_0.gguf\n",
    );
    {
        let m = Manager::new(
            FakeService::new(),
            FakeAuthority::new(true),
            FakeResolver::lan(),
        )
        .with_group(crate::manager::service::fakes::FakeGroup::new());
        // The instance the writes belong to: its section of the record, which
        // is what the manager's render pass writes a render for.
        record(&["default"]);
        assert_eq!(
            m.write_file(&alice, "server@default.ini", default_ini, false),
            Ok(())
        );
        assert_eq!(
            m.write_file(&alice, "models.ini", &model_profiles, false),
            Ok(())
        );
        // The switched-off file is retired by the model data model
        // (`ARCHITECTURE.md`, "Instance profiles"): a profile switched off
        // sits in `models.ini` with no instance profile naming it, so a client
        // reads the file and never writes it.
        assert_eq!(
            m.write_file(
                &alice,
                "models.ini.disabled",
                &real_model_files(dir.path(), "[Old]\nmodel = /m/old.gguf\n"),
                false
            ),
            Err(ManagerError::BadFileName)
        );
        assert_eq!(
            std::fs::read_to_string(sys.join("server@default.ini")).expect("written"),
            default_ini
        );
        assert_eq!(
            files_in(&sys),
            set(&[
                "instances.ini",
                "server@default.ini",
                "models.ini",
                "models@default.ini",
                // The pair the apply writes: the launch file beside the render
                // (IP3c).
                "launch@default.ini",
            ]),
            "exactly the files written, no temp file left"
        );
        // The record's section is what makes the instance: the listing reads it.
        assert_eq!(m.list_instances(), ["default"]);
        assert_eq!(
            m.read_file(Some(&alice), "server@default.ini"),
            Ok(Some(plain(default_ini)))
        );
        assert_eq!(
            m.read_file(Some(&alice), "models.ini"),
            Ok(Some(plain(&model_profiles)))
        );
        assert!(
            m.service().calls.lock().unwrap().is_empty(),
            "a write calls no service"
        );
        assert_eq!(
            *m.authority().asked.lock().unwrap(),
            vec![
                (alice.clone(), Action::Configure),
                (alice.clone(), Action::Configure),
                (alice.clone(), Action::Configure),
                (alice.clone(), Action::Control),
            ],
            "writes ask for configure, the instance file's read for control, \
             the preset file's read for nothing"
        );

        // The render of an instance is checked against that instance's own
        // ModelsMax, and the check is a fit check of that instance (IP3c): two
        // model profiles load on startup, default's max is 1, so the pool LANDS
        // and default keeps its last good pair, reported over its max. The
        // assignments say the render holds the pool; the manager writes that
        // file in a later step, so the fixture writes it by hand.
        std::fs::write(sys.join("assignments.ini"), "[default]\n* = inherit\n")
            .expect("assignments.ini");
        std::fs::write(
            sys.join("server@default.ini"),
            format!("{default_ini}ModelsMax = 1\n"),
        )
        .expect("ModelsMax fixture");
        let two_loading = real_model_files(
            dir.path(),
            "[a]\nmodel = /m/a.gguf\nload-on-startup = true\n\n\
             [b]\nmodel = /m/b.gguf\nload-on-startup = true\n",
        );
        let pair = (
            std::fs::read(sys.join("models@default.ini")).expect("the render on disk"),
            std::fs::read(sys.join("launch@default.ini")).expect("the launch on disk"),
        );
        assert_eq!(
            m.write_file(&alice, "models.ini", &two_loading, false),
            Ok(()),
            "the count is a fit check of default, so the pool lands"
        );
        assert_eq!(
            m.not_converged_locked().expect("the rows"),
            vec![NotConverged {
                instance: "default".into(),
                key: "load-on-startup-over-max".into(),
                args: vec!["2".into(), "1".into(), "default".into()],
            }],
            "default is reported over its own max"
        );
        assert_eq!(
            (
                std::fs::read(sys.join("models@default.ini")).expect("the render"),
                std::fs::read(sys.join("launch@default.ini")).expect("the launch"),
            ),
            pair,
            "and default keeps the pair it had: the refused render wrote neither file"
        );
        // The same pool is accepted once that instance's own max holds it.
        std::fs::write(
            sys.join("server@default.ini"),
            format!("{default_ini}ModelsMax = 2\n"),
        )
        .expect("ModelsMax fixture");
        assert_eq!(
            m.write_file(&alice, "models.ini", &two_loading, false),
            Ok(())
        );
        // The pool and the instance file the blocks that follow expect.
        assert_eq!(
            m.write_file(&alice, "server@default.ini", default_ini, false),
            Ok(())
        );
        assert_eq!(
            m.write_file(&alice, "models.ini", &model_profiles, false),
            Ok(())
        );

        // Any other name is refused, and nothing is created or read.
        let before = files_in(&sys);
        for name in bad_names(&sys) {
            assert_eq!(
                m.write_file(&alice, &name, "[Server]\n", false),
                Err(ManagerError::BadFileName),
                "write {name:?}"
            );
            assert_eq!(
                m.read_file(Some(&alice), &name),
                Err(ManagerError::BadFileName),
                "read {name:?}"
            );
        }
        assert_eq!(files_in(&sys), before, "a refused name writes nothing");
        assert!(
            !dir.path().join("x").exists(),
            "nothing escapes the directory"
        );
    }

    // ── Model profile reads, list and status ask no authorization ───────────────
    {
        let m = Manager::new(
            FakeService::new(),
            FakeAuthority::new(false),
            FakeResolver::lan(),
        )
        .with_group(crate::manager::service::fakes::FakeGroup::new());
        assert_eq!(m.list_instances(), ["default"]);
        assert_eq!(
            m.read_file(Some(&alice), "models.ini"),
            Ok(Some(plain(&model_profiles)))
        );
        assert_eq!(m.status("default"), Ok(UnitState::Active));
        assert!(
            m.authority().asked.lock().unwrap().is_empty(),
            "no question for reads"
        );
        assert_eq!(
            *m.service().calls.lock().unwrap(),
            vec![Call::Status("default".into())]
        );
    }

    // ── A refused caller cannot control an existing instance ─────────────
    {
        let m = Manager::new(
            FakeService::new(),
            FakeAuthority::new(false),
            FakeResolver::lan(),
        )
        .with_group(crate::manager::service::fakes::FakeGroup::new());
        assert_eq!(m.start(&alice, "default"), Err(ManagerError::NotAuthorized));
        assert_eq!(m.stop(&alice, "default"), Err(ManagerError::NotAuthorized));
        assert_eq!(
            m.restart(&alice, "default"),
            Err(ManagerError::NotAuthorized)
        );
        assert!(
            m.service().calls.lock().unwrap().is_empty(),
            "no service call when refused"
        );
        assert_eq!(
            *m.authority().asked.lock().unwrap(),
            vec![(alice.clone(), Action::Control); 3],
            "start, stop and restart ask for the control action"
        );
        // A refused caller learns nothing about which ids exist.
        assert_eq!(m.start(&alice, "nope"), Err(ManagerError::NotAuthorized));
        assert_eq!(m.start(&alice, "../x"), Err(ManagerError::NotAuthorized));
        assert!(m.service().calls.lock().unwrap().is_empty());
        // Nor can it change a file.
        assert_eq!(
            m.write_file(&alice, "server@default.ini", "[Server]\nPort = 1\n", false),
            Err(ManagerError::NotAuthorized)
        );
        assert_eq!(
            m.write_file(&alice, "server@new.ini", "[Server]\nPort = 1\n", false),
            Err(ManagerError::NotAuthorized)
        );
        assert_eq!(
            std::fs::read_to_string(sys.join("server@default.ini")).expect("still there"),
            default_ini
        );
        assert!(!sys.join("server@new.ini").exists());
    }

    // ── Control of an existing instance goes to the service, by id ───────
    {
        let mut service = FakeService::new();
        service.state = UnitState::Failed;
        let m = Manager::new(service, FakeAuthority::new(true), FakeResolver::lan())
            .with_group(crate::manager::service::fakes::FakeGroup::new());
        assert_eq!(m.start(&alice, "default"), Ok(()));
        assert_eq!(m.stop(&alice, "default"), Ok(()));
        assert_eq!(m.restart(&alice, "default"), Ok(()));
        assert_eq!(m.status("default"), Ok(UnitState::Failed));
        assert_eq!(
            *m.service().calls.lock().unwrap(),
            vec![
                Call::Start("default".into()),
                Call::Stop("default".into()),
                Call::Restart("default".into()),
                Call::Status("default".into()),
            ]
        );
        // Another instance's file does not make an unknown id one.
        assert_eq!(m.start(&alice, "lan-2"), Err(ManagerError::NoInstance));
        assert_eq!(m.restart(&alice, "lan-2"), Err(ManagerError::NoInstance));
        assert_eq!(
            m.service().calls.lock().unwrap().len(),
            4,
            "no start or restart for an id with no file"
        );
        // Stop and status of it still reach the service.
        assert_eq!(m.stop(&alice, "lan-2"), Ok(()));
        assert_eq!(m.status("lan-2"), Ok(UnitState::Failed));
        assert_eq!(
            m.service().calls.lock().unwrap()[4..],
            [Call::Stop("lan-2".into()), Call::Status("lan-2".into())]
        );
    }

    // ── A service failure is reported as one ─────────────────────────────
    {
        let mut service = FakeService::new();
        service.failing = vec!["default".into()];
        let m = Manager::new(service, FakeAuthority::new(true), FakeResolver::lan())
            .with_group(crate::manager::service::fakes::FakeGroup::new());
        assert!(
            matches!(m.start(&alice, "default"), Err(ManagerError::Service(_))),
            "start failure"
        );
        assert!(
            matches!(m.status("default"), Err(ManagerError::Service(_))),
            "status failure"
        );
    }

    // ── Address collisions between instance files ────────────────────────
    {
        let m = Manager::new(
            FakeService::new(),
            FakeAuthority::new(true),
            FakeResolver::lan(),
        )
        .with_group(crate::manager::service::fakes::FakeGroup::new());
        // Every save below is of a profile an instance runs: a profile no
        // instance names binds nothing, so it is judged against nothing and
        // any address is free. Each refusal names the instance whose profile
        // holds the address.
        record(&["default", "lan-2"]);
        // The same address as `default` (127.0.0.1:8081): refused, naming it.
        assert_eq!(
            m.write_file(&alice, "server@lan-2.ini", default_ini, false),
            Err(ManagerError::AddressInUse("default".into()))
        );
        assert!(
            !sys.join("server@lan-2.ini").exists(),
            "refused: not written"
        );
        // The same port on another specific host: fine.
        let lan2 = "[Server]\nPort = 8081\nHostname = 192.168.1.5\n";
        assert_eq!(
            m.write_file(&alice, "server@lan-2.ini", lan2, false),
            Ok(())
        );
        record(&["default", "lan-2", "loop"]);
        // Host names are resolved: localhost is default's 127.0.0.1.
        assert_eq!(
            m.write_file(
                &alice,
                "server@loop.ini",
                "[Server]\nPort = 8081\nHostname = localhost\n",
                false
            ),
            Err(ManagerError::AddressInUse("default".into()))
        );
        assert!(
            !sys.join("server@loop.ini").exists(),
            "refused: not written"
        );
        // An instance whose profile file was never written (its own save was
        // refused) is skipped: only default and lan-2 hold an address.
        record(&["default", "lan-2", "loop", "named"]);
        // Two names of one address collide, whichever is written.
        assert_eq!(
            m.write_file(
                &alice,
                "server@named.ini",
                "[Server]\nPort = 8081\nHostname = gpu-box\n",
                false
            ),
            Err(ManagerError::AddressInUse("lan-2".into()))
        );
        record(&["default", "lan-2", "loop", "named", "multi-home"]);
        // A name with several addresses collides when one matches.
        assert_eq!(
            m.write_file(
                &alice,
                "server@multi-home.ini",
                "[Server]\nPort = 8081\nHostname = multi-home\n",
                false
            ),
            Err(ManagerError::AddressInUse("lan-2".into()))
        );
        // A host name that does not resolve: refused, naming it, not written.
        assert_eq!(
            m.write_file(
                &alice,
                "server@nowhere.ini",
                "[Server]\nPort = 8090\nHostname = nowhere.example\n",
                false
            ),
            Err(ManagerError::UnresolvableHost("nowhere.example".into()))
        );
        assert!(
            !sys.join("server@nowhere.ini").exists(),
            "refused: not written"
        );
        record(&["default", "lan-2", "loop", "named", "multi-home", "multi"]);
        // A host shared inside a Hostname list collides.
        assert_eq!(
            m.write_file(
                &alice,
                "server@multi.ini",
                "[Server]\nPort = 8081\nHostname = 10.0.0.1, 192.168.1.5\n",
                false
            ),
            Err(ManagerError::AddressInUse("lan-2".into()))
        );
        record(&[
            "default",
            "lan-2",
            "loop",
            "named",
            "multi-home",
            "multi",
            "wild",
        ]);
        // A wildcard host on the same port collides with every host.
        for wild in ["0.0.0.0", "::"] {
            let text = format!("[Server]\nPort = 8081\nHostname = {wild}\n");
            assert!(
                matches!(
                    m.write_file(&alice, "server@wild.ini", &text, false),
                    Err(ManagerError::AddressInUse(ref other)) if other == "default" || other == "lan-2"
                ),
                "wildcard {wild}"
            );
        }
        // A wildcard on another port is fine.
        assert_eq!(
            m.write_file(
                &alice,
                "server@wild.ini",
                "[Server]\nPort = 8082\nHostname = 0.0.0.0\n",
                false
            ),
            Ok(())
        );
        record(&["default", "lan-2", "wild", "late"]);
        // Then a specific host on that port collides with the wildcard.
        assert_eq!(
            m.write_file(
                &alice,
                "server@late.ini",
                "[Server]\nPort = 8082\nHostname = 10.0.0.1\n",
                false
            ),
            Err(ManagerError::AddressInUse("wild".into()))
        );
        // Rewriting an instance with its own address is fine.
        assert_eq!(
            m.write_file(&alice, "server@default.ini", default_ini, false),
            Ok(())
        );
        // Moving it onto another instance's address is refused, the file kept.
        assert_eq!(
            m.write_file(
                &alice,
                "server@default.ini",
                "[Server]\nPort = 8081\nHostname = 192.168.1.5\n",
                false
            ),
            Err(ManagerError::AddressInUse("lan-2".into()))
        );
        assert_eq!(
            std::fs::read_to_string(sys.join("server@default.ini")).expect("kept"),
            default_ini
        );
        // Defaults count: no Port key is the default port, no Hostname the
        // default host, so two bare files collide, and an explicit default
        // collides with an implicit one.
        assert_eq!(
            m.write_file(&alice, "server@bare.ini", "[Server]\n", false),
            Ok(())
        );
        record(&["default", "lan-2", "wild", "bare", "bare-2"]);
        assert_eq!(
            m.write_file(&alice, "server@bare-2.ini", "[Server]\n", false),
            Err(ManagerError::AddressInUse("bare".into()))
        );
        record(&["default", "lan-2", "wild", "bare", "bare-2", "explicit"]);
        let explicit = format!(
            "[Server]\nPort = {}\nHostname = localhost\n",
            crate::server_cfg::DEFAULT_PORT
        );
        assert_eq!(
            m.write_file(&alice, "server@explicit.ini", &explicit, false),
            Err(ManagerError::AddressInUse("bare".into()))
        );
        // Another instance whose host no longer resolves is skipped: it
        // does not block writing this one.
        std::fs::write(
            sys.join("server@stale.ini"),
            "[Server]\nPort = 8083\nHostname = gone.example\n",
        )
        .expect("stale fixture");
        // fresh is an instance too, so the check runs and walks stale's
        // unresolvable host: an unused profile would be accepted unread.
        record(&["default", "lan-2", "wild", "bare", "stale", "fresh"]);
        assert_eq!(
            m.write_file(
                &alice,
                "server@fresh.ini",
                "[Server]\nPort = 8083\nHostname = 10.0.0.1\n",
                false
            ),
            Ok(()),
            "an unresolvable other instance is skipped"
        );
        std::fs::remove_file(sys.join("server@stale.ini")).expect("drop stale");
        std::fs::remove_file(sys.join("server@fresh.ini")).expect("drop fresh");
        // Its section goes with it: the record is the listing.
        record(&["default", "lan-2", "wild", "bare"]);
        // models.ini has no address: never checked.
        assert_eq!(
            m.write_file(&alice, "models.ini", &model_profiles, false),
            Ok(())
        );
        assert_eq!(m.list_instances(), ["bare", "default", "lan-2", "wild"]);
        assert!(
            m.service().calls.lock().unwrap().is_empty(),
            "a write calls no service"
        );
    }

    // ── A failed write leaves the old content ────────────────────────────
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let m = Manager::new(
            FakeService::new(),
            FakeAuthority::new(true),
            FakeResolver::lan(),
        )
        .with_group(crate::manager::service::fakes::FakeGroup::new());
        let before = files_in(&sys);
        std::fs::set_permissions(&sys, std::fs::Permissions::from_mode(0o555)).expect("chmod");
        // Root writes into a read-only directory anyway: then there is no
        // failure to provoke.
        let probe = sys.join("probe");
        let enforced = std::fs::write(&probe, "").is_err();
        let _ = std::fs::remove_file(&probe);
        let result = m.write_file(
            &alice,
            "models.ini",
            &real_model_files(dir.path(), "[Other]\nmodel = /m/o.gguf\n"),
            false,
        );
        std::fs::set_permissions(&sys, std::fs::Permissions::from_mode(0o755)).expect("chmod back");
        if enforced {
            assert!(
                matches!(result, Err(ManagerError::Io(_))),
                "write failure: {result:?}"
            );
            assert_eq!(
                std::fs::read_to_string(sys.join("models.ini")).expect("kept"),
                model_profiles
            );
            assert_eq!(files_in(&sys), before, "no temp file left");
        }
    }

    // ── Start at boot: read by anyone, changed under configure ──────────
    // systemd and the SCM start the enabled instances; the manager only
    // reads and changes which ones are.
    {
        let bob = Caller(":1.43".into());
        let m = Manager::new(
            FakeService::new(),
            FakeAuthority::new(true),
            FakeResolver::lan(),
        )
        .with_group(crate::manager::service::fakes::FakeGroup::new());
        assert_eq!(
            m.boot_enabled("default"),
            Ok(false),
            "a new instance starts disabled"
        );
        assert_eq!(m.set_boot_enabled(&bob, "default", true), Ok(()));
        assert_eq!(m.boot_enabled("default"), Ok(true));
        assert_eq!(m.boot_enabled("lan-2"), Ok(false), "only default changed");
        assert_eq!(m.set_boot_enabled(&bob, "default", false), Ok(()));
        assert_eq!(m.boot_enabled("default"), Ok(false));
        assert_eq!(
            *m.service().calls.lock().unwrap(),
            vec![
                Call::BootEnabled("default".into()),
                Call::SetBootEnabled("default".into(), true),
                Call::BootEnabled("default".into()),
                Call::BootEnabled("lan-2".into()),
                Call::SetBootEnabled("default".into(), false),
                Call::BootEnabled("default".into()),
            ]
        );
        assert_eq!(
            *m.authority().asked.lock().unwrap(),
            vec![(bob.clone(), Action::Configure); 2],
            "only the changes ask, for configure"
        );

        // An id with no server@<id>.ini: refused before the service
        // manager is asked, either way.
        m.service().calls.lock().unwrap().clear();
        assert_eq!(m.boot_enabled("lan-3"), Err(ManagerError::NoInstance));
        assert_eq!(
            m.set_boot_enabled(&bob, "lan-3", true),
            Err(ManagerError::NoInstance)
        );
        for bad in ["", "../x", "Default", "a_b"] {
            assert_eq!(m.boot_enabled(bad), Err(ManagerError::InvalidId), "{bad:?}");
            assert_eq!(
                m.set_boot_enabled(&bob, bad, true),
                Err(ManagerError::InvalidId),
                "{bad:?}"
            );
        }
        assert_eq!(*m.service().calls.lock().unwrap(), vec![]);
    }

    // ── Start at boot: a refused caller and a failing service manager ───
    {
        let bob = Caller(":1.43".into());
        let m = Manager::new(
            FakeService::new(),
            FakeAuthority::new(false),
            FakeResolver::lan(),
        )
        .with_group(crate::manager::service::fakes::FakeGroup::new());
        assert_eq!(
            m.set_boot_enabled(&bob, "default", true),
            Err(ManagerError::NotAuthorized)
        );
        assert_eq!(
            m.boot_enabled("default"),
            Ok(false),
            "the read needs no authorization, and the refusal changed nothing"
        );
        assert_eq!(
            *m.service().calls.lock().unwrap(),
            vec![Call::BootEnabled("default".into())],
            "a refused caller reaches no service manager"
        );
        assert_eq!(
            *m.authority().asked.lock().unwrap(),
            vec![(bob.clone(), Action::Configure)]
        );

        let mut service = FakeService::new();
        service.failing = vec!["default".into()];
        let m = Manager::new(service, FakeAuthority::new(true), FakeResolver::lan())
            .with_group(crate::manager::service::fakes::FakeGroup::new());
        assert!(
            matches!(m.boot_enabled("default"), Err(ManagerError::Service(_))),
            "the service manager's failure"
        );
        assert!(matches!(
            m.set_boot_enabled(&bob, "default", true),
            Err(ManagerError::Service(_))
        ));
    }
}

/// E2E phase for the instance files' secret (`OpencodeApiKey`), for this
/// crate's e2e test to call (it sets the process-wide
/// `PRIVATE_AI_DATA_ROOT` redirect, `src/tests/mod.rs`): an authorized read
/// of an instance file, a model profile read with no caller, and the writes going
/// through the group seam with their modes.
#[cfg(test)]
pub(crate) fn manager_instance_files_e2e() {
    use fakes::{FakeAuthority, FakeGroup, FakeResolver, FakeService};

    let dir = tempfile::tempdir().expect("tempdir");
    crate::paths::redirect_for_tests(dir.path());
    let sys = crate::paths::engine_config_dir();
    let alice = Caller(":1.42".into());
    let names = || -> std::collections::BTreeSet<String> {
        std::fs::read_dir(&sys)
            .expect("read_dir")
            .flatten()
            .map(|e| e.file_name().to_string_lossy().into_owned())
            .collect()
    };
    let ini = "[Server]\nPort = 8080\nHostname = 127.0.0.1\nOpencodeApiKey = sk-local-3f9a\n";
    let fixtures = tempfile::tempdir().expect("tempdir");
    let model_profiles = real_model_files(
        fixtures.path(),
        "[Qwen3.8-27B]\nmodel = /models/Qwen3.8-27B-Q8_0.gguf\n",
    );

    // ── Writes go through the group seam: instance files only ────────────
    {
        let group = FakeGroup::new();
        let group_calls = group.calls.clone();
        let m = Manager::new(
            FakeService::new(),
            FakeAuthority::new(true),
            FakeResolver::lan(),
        )
        .with_group(group);
        assert_eq!(
            m.write_file(&alice, "server@default.ini", ini, false),
            Ok(())
        );
        assert_eq!(
            m.write_file(&alice, "models.ini", &model_profiles, false),
            Ok(())
        );
        let c = group_calls.lock().unwrap().clone();
        assert_eq!(c.len(), 1, "one group change, the instance file's: {c:?}");
        assert_eq!(c[0].group, INSTANCE_GROUP);
        assert_eq!(c[0].path.parent(), Some(sys.as_path()));
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mode =
                |n: &str| std::fs::metadata(sys.join(n)).unwrap().permissions().mode() & 0o7777;
            assert_eq!(mode("server@default.ini"), 0o640);
            assert_eq!(mode("models.ini"), 0o644);
            assert_eq!(c[0].mode, 0o640);
        }

        // ── An authorized read of an instance file: asked, then read ─────
        assert_eq!(
            m.read_file(Some(&alice), "server@default.ini"),
            Ok(Some(plain(ini)))
        );
        assert_eq!(m.read_file(Some(&alice), "server@nope.ini"), Ok(None));
        // ── The model profile files need no caller at all ───────────────────────
        assert_eq!(
            m.read_file(None::<&Caller>, "models.ini"),
            Ok(Some(plain(&model_profiles)))
        );
        assert_eq!(
            m.read_file(None::<&Caller>, "models.ini.disabled"),
            Ok(None)
        );
        assert_eq!(
            *m.authority().asked.lock().unwrap(),
            vec![
                (alice.clone(), Action::Configure),
                (alice.clone(), Action::Configure),
                (alice.clone(), Action::Control),
                (alice.clone(), Action::Control),
            ],
            "the instance file reads ask for control, the preset reads nothing"
        );
    }

    // ── No group private-ai: the write refused, the old file kept ────────
    {
        let m = Manager::new(
            FakeService::new(),
            FakeAuthority::new(true),
            FakeResolver::lan(),
        )
        .with_group(FakeGroup::failing(std::io::ErrorKind::NotFound));
        let before = names();
        assert_eq!(
            m.write_file(
                &alice,
                "server@default.ini",
                "[Server]\nPort = 8090\n",
                false
            ),
            Err(ManagerError::Io(std::io::ErrorKind::NotFound))
        );
        assert_eq!(
            std::fs::read_to_string(sys.join("server@default.ini")).unwrap(),
            ini,
            "the old file is kept"
        );
        assert_eq!(names(), before, "no temp file left");
        // The model profile file needs no group: still written. The switched-off
        // file is read only, so it is no longer the one this shows.
        assert_eq!(
            m.write_file(
                &alice,
                "models.ini",
                &real_model_files(dir.path(), "[Old]\nmodel = /m/old.gguf\n"),
                false
            ),
            Ok(())
        );
    }
}

/// E2E phase for the API key withheld from a reader who may not configure,
/// for this crate's e2e test to call (it sets the process-wide
/// `PRIVATE_AI_DATA_ROOT` redirect, `src/tests/mod.rs`). An instance file
/// needs only control to read, so a caller without configure (asked without
/// a prompt: a polkit challenge counts as no) gets the `OpencodeApiKey` line
/// blanked to `OpencodeApiKey =`, every other line as it was, and is told
/// the key was withheld; a caller with configure gets the file as saved.
/// The model profile files are never touched.
#[cfg(test)]
pub(crate) fn manager_withholds_api_key_e2e() {
    use fakes::{FakeAuthority, FakeGroup, FakeResolver, FakeService};

    let dir = tempfile::tempdir().expect("tempdir");
    crate::paths::redirect_for_tests(dir.path());
    let sys = crate::paths::engine_config_dir();
    std::fs::create_dir_all(&sys).expect("engine config dir");
    let put = |name: &str, text: &str| std::fs::write(sys.join(name), text).expect("write");
    let alice = Caller(":1.42".into());
    // `control` granted; `configure` asked quietly answers `configure`.
    let manager = |configure: bool| {
        Manager::new(
            FakeService::new(),
            FakeAuthority::new(true).quietly(configure),
            FakeResolver::lan(),
        )
        .with_group(FakeGroup::new())
    };
    let ini = "\
; Written by pai-config.
[Server]
Port = 8080
Hostname = 127.0.0.1
; OpencodeApiKey = sk-xxx  ; API key for proxy/gateway authentication
OpencodeBaseUrl = http://127.0.0.1:8080/v1
OpencodeApiKey = sk-local-3f9a
ModelsMax = 2
";
    let blanked = "\
; Written by pai-config.
[Server]
Port = 8080
Hostname = 127.0.0.1
; OpencodeApiKey = sk-xxx  ; API key for proxy/gateway authentication
OpencodeBaseUrl = http://127.0.0.1:8080/v1
OpencodeApiKey =
ModelsMax = 2
";
    put("server@default.ini", ini);

    // ── No configure: the value blanked, the rest as saved, withheld said ─
    {
        let m = manager(false);
        let read = m.read_file(Some(&alice), "server@default.ini");
        assert_eq!(read, Ok(Some(withheld(blanked))));
        let text = read.unwrap().unwrap().text;
        assert!(!text.contains("sk-local-3f9a"), "{text}");
        assert_eq!(
            crate::server_cfg::from_text(&text).opencode_api_key,
            None,
            "read back, the blanked file holds no key"
        );
        assert_eq!(
            *m.authority().asked.lock().unwrap(),
            vec![(alice.clone(), Action::Control)],
            "control asked as before; configure never asked with a prompt"
        );
        assert_eq!(
            *m.authority().asked_quietly.lock().unwrap(),
            vec![(alice.clone(), Action::Configure)],
            "configure asked without a prompt"
        );
    }

    // ── Configure: the file as saved, nothing withheld, still no prompt ───
    {
        let m = manager(true);
        assert_eq!(
            m.read_file(Some(&alice), "server@default.ini"),
            Ok(Some(plain(ini)))
        );
        assert_eq!(
            *m.authority().asked.lock().unwrap(),
            vec![(alice.clone(), Action::Control)],
            "an administrator's read prompts for nothing more than control"
        );
    }

    // ── Refused control: NotAuthorized, configure not even asked ──────────
    {
        let m = Manager::new(
            FakeService::new(),
            FakeAuthority::new(false).quietly(true),
            FakeResolver::lan(),
        );
        assert_eq!(
            m.read_file(Some(&alice), "server@default.ini"),
            Err(ManagerError::NotAuthorized)
        );
        assert_eq!(*m.authority().asked_quietly.lock().unwrap(), vec![]);
    }

    // ── Nothing to withhold: no key, a blank key, only the commented hint ─
    for text in [
        "[Server]\nPort = 8080\n",
        "[Server]\nPort = 8080\nOpencodeApiKey =\n",
        "[Server]\nPort = 8080\nOpencodeApiKey =   \n",
        "[Server]\nPort = 8080\n; OpencodeApiKey = sk-xxx  ; blank = none\n",
        "",
    ] {
        put("server@lan-2.ini", text);
        let m = manager(false);
        assert_eq!(
            m.read_file(Some(&alice), "server@lan-2.ini"),
            Ok(Some(plain(text))),
            "nothing withheld from {text:?}"
        );
    }

    // ── Every key line blanked: a duplicate, an inline comment, CRLF ──────
    {
        let m = manager(false);
        put(
            "server@lan-2.ini",
            "[Server]\r\nOpencodeApiKey = sk-old-1111 ; rotated\r\nPort = 8081\r\nOpencodeApiKey = sk-new-2222\r\n",
        );
        assert_eq!(
            m.read_file(Some(&alice), "server@lan-2.ini"),
            Ok(Some(withheld(
                "[Server]\r\nOpencodeApiKey =\r\nPort = 8081\r\nOpencodeApiKey =\r\n"
            ))),
            "both key lines blanked, the inline comment with the value, the line ends kept"
        );
        // Spelled as the parser still reads it: spaces around, none at all.
        put(
            "server@lan-2.ini",
            "[Server]\n  OpencodeApiKey=sk-tight-3333\nPort = 8081\n",
        );
        let read = m
            .read_file(Some(&alice), "server@lan-2.ini")
            .unwrap()
            .unwrap();
        assert!(read.withheld, "{read:?}");
        assert!(!read.text.contains("sk-tight-3333"), "{read:?}");
        assert!(read.text.contains("\nPort = 8081\n"), "{read:?}");
        assert_eq!(
            crate::server_cfg::from_text(&read.text).opencode_api_key,
            None
        );
    }

    // ── A missing instance file is still None ────────────────────────────
    assert_eq!(
        manager(false).read_file(Some(&alice), "server@nope.ini"),
        Ok(None)
    );

    // ── The model profile files are never blanked, nor is configure asked ───────
    {
        let m = manager(false);
        let model_profiles = real_model_files(dir.path(), "[Qwen3.8-27B]\nmodel = /models/Qwen3.8-27B-Q8_0.gguf\nOpencodeApiKey = not-a-secret-here\n");
        put("models.ini", &model_profiles);
        put("models.ini.disabled", &model_profiles);
        assert_eq!(
            m.read_file(None::<&Caller>, "models.ini"),
            Ok(Some(plain(&model_profiles)))
        );
        assert_eq!(
            m.read_file(Some(&alice), "models.ini.disabled"),
            Ok(Some(plain(&model_profiles)))
        );
        assert_eq!(*m.authority().asked_quietly.lock().unwrap(), vec![]);
    }
}

/// E2E phase for a write that keeps the API key, for this crate's e2e test
/// to call (it sets the process-wide `PRIVATE_AI_DATA_ROOT` redirect,
/// `src/tests/mod.rs`). A caller the key was withheld from saves the file
/// it read, its key line blanked or gone: with `keep_api_key`, once the write
/// is authorized (configure, as every write), the manager puts the key it
/// holds back. The incoming `OpencodeApiKey` line takes the current value,
/// in place; with no such line, one is added (a client renders an unset key
/// as a commented hint only, so adding none would drop the key all the
/// same). With nothing held, or without `keep_api_key`, the text is written
/// as sent. The model profile files are never touched.
#[cfg(test)]
pub(crate) fn manager_keeps_api_key_e2e() {
    use fakes::{FakeAuthority, FakeGroup, FakeResolver, FakeService};

    let dir = tempfile::tempdir().expect("tempdir");
    crate::paths::redirect_for_tests(dir.path());
    let sys = crate::paths::engine_config_dir();
    std::fs::create_dir_all(&sys).expect("engine config dir");
    let put = |name: &str, text: &str| std::fs::write(sys.join(name), text).expect("write");
    let get = |name: &str| std::fs::read_to_string(sys.join(name)).unwrap_or_default();
    let alice = Caller(":1.42".into());
    let manager = |allow: bool| {
        Manager::new(
            FakeService::new(),
            FakeAuthority::new(allow).quietly(false),
            FakeResolver::lan(),
        )
        .with_group(FakeGroup::new())
    };
    let key_of = |text: &str| crate::server_cfg::from_text(text).opencode_api_key;
    let key_lines = |text: &str| {
        text.lines()
            .filter(|l| {
                let t = l.trim();
                !t.starts_with([';', '#'])
                    && t.split_once('=')
                        .is_some_and(|(k, _)| k.trim() == "OpencodeApiKey")
            })
            .count()
    };
    let saved = "\
; Written by pai-config.
[Server]
Port = 8080
Hostname = 127.0.0.1
; OpencodeApiKey = sk-xxx  ; API key for proxy/gateway authentication
OpencodeBaseUrl = http://127.0.0.1:8080/v1
OpencodeApiKey = sk-local-3f9a
ModelsMax = 2
";
    // What a caller without configure read, then saved with Port changed.
    let blanked = "\
; Written by pai-config.
[Server]
Port = 8090
Hostname = 127.0.0.1
; OpencodeApiKey = sk-xxx  ; API key for proxy/gateway authentication
OpencodeBaseUrl = http://127.0.0.1:8080/v1
OpencodeApiKey =
ModelsMax = 2
";
    let kept = "\
; Written by pai-config.
[Server]
Port = 8090
Hostname = 127.0.0.1
; OpencodeApiKey = sk-xxx  ; API key for proxy/gateway authentication
OpencodeBaseUrl = http://127.0.0.1:8080/v1
OpencodeApiKey = sk-local-3f9a
ModelsMax = 2
";
    // A client's rendering of a config with no key: the hint, no key line.
    let no_line = "\
[Server]
Port = 8090
Hostname = 127.0.0.1
; OpencodeApiKey = sk-xxx  ; API key for proxy/gateway authentication (opencode.json apiKey + Claude Code); blank = none
ModelsMax = 2
";

    // ── The blanked line takes the held value, in place ──────────────────
    {
        put("server@default.ini", saved);
        let m = manager(true);
        assert_eq!(
            m.write_file(&alice, "server@default.ini", blanked, true),
            Ok(())
        );
        assert_eq!(get("server@default.ini"), kept);
        assert_eq!(
            *m.authority().asked.lock().unwrap(),
            vec![(alice.clone(), Action::Configure)],
            "configure asked, as for every write"
        );
    }

    // ── No key line at all: the held key is added, the rest as sent ──────
    {
        put("server@default.ini", saved);
        let m = manager(true);
        assert_eq!(
            m.write_file(&alice, "server@default.ini", no_line, true),
            Ok(())
        );
        let written = get("server@default.ini");
        assert!(
            written.starts_with(no_line),
            "the text as sent, first:\n{written}"
        );
        assert_eq!(key_of(&written), Some("sk-local-3f9a".into()), "{written}");
        assert_eq!(key_lines(&written), 1, "{written}");
        assert_eq!(crate::server_cfg::from_text(&written).port, Some(8090));
    }

    // ── keep_api_key wins over a value sent with it ──────────────────────
    {
        put("server@default.ini", saved);
        let m = manager(true);
        let typed = blanked.replace("OpencodeApiKey =\n", "OpencodeApiKey = sk-typed-0000\n");
        assert_eq!(
            m.write_file(&alice, "server@default.ini", &typed, true),
            Ok(())
        );
        assert_eq!(get("server@default.ini"), kept);
    }

    // ── Without keep_api_key: as sent, the key dropped (as before) ───────
    {
        put("server@default.ini", saved);
        let m = manager(true);
        assert_eq!(
            m.write_file(&alice, "server@default.ini", blanked, false),
            Ok(())
        );
        assert_eq!(get("server@default.ini"), blanked);
        assert_eq!(key_of(&get("server@default.ini")), None);
    }

    // ── Nothing held: as sent, no line invented ──────────────────────────
    {
        let m = manager(true);
        for held in [
            "[Server]\nPort = 8080\nHostname = 127.0.0.1\n",
            "[Server]\nPort = 8080\nHostname = 127.0.0.1\nOpencodeApiKey =\n",
        ] {
            put("server@default.ini", held);
            for sent in [blanked, no_line] {
                assert_eq!(
                    m.write_file(&alice, "server@default.ini", sent, true),
                    Ok(())
                );
                assert_eq!(get("server@default.ini"), sent, "held {held:?}");
            }
        }
        // A new instance: no current file at all.
        assert_eq!(
            m.write_file(
                &alice,
                "server@lan-2.ini",
                no_line.replace("8090", "8091").as_str(),
                true
            ),
            Ok(())
        );
        assert_eq!(get("server@lan-2.ini"), no_line.replace("8090", "8091"));
        std::fs::remove_file(sys.join("server@lan-2.ini")).expect("remove lan-2");
    }

    // ── Refused: nothing written, the key still there ────────────────────
    {
        put("server@default.ini", saved);
        let m = manager(false);
        assert_eq!(
            m.write_file(&alice, "server@default.ini", blanked, true),
            Err(ManagerError::NotAuthorized)
        );
        assert_eq!(get("server@default.ini"), saved);
    }

    // ── The content checks still run, on what would be written ───────────
    {
        put("server@default.ini", saved);
        let m = manager(true);
        let bad = blanked.replace("Port = 8090", "Port = 0");
        assert!(
            matches!(
                m.write_file(&alice, "server@default.ini", &bad, true),
                Err(ManagerError::Invalid(_))
            ),
            "a bad Port is refused with the key kept or not"
        );
        assert_eq!(get("server@default.ini"), saved);
    }

    // ── The held file cannot be read: refused, never written blanked ─────
    // An I/O error other than not-found (here permission denied) says
    // nothing about the key: writing the blanked text would erase it.
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let path = sys.join("server@default.ini");
        put("server@default.ini", saved);
        let mode = |m: u32| {
            std::fs::set_permissions(&path, std::fs::Permissions::from_mode(m)).expect("chmod")
        };
        mode(0o000);
        // Root ignores the mode: only a real read failure tests anything.
        if std::fs::read_to_string(&path).is_ok() {
            mode(0o644);
            eprintln!("skipped: the unreadable-file case needs a non-root user");
        } else {
            let m = manager(true);
            let got = m.write_file(&alice, "server@default.ini", blanked, true);
            mode(0o644);
            assert_eq!(
                got,
                Err(ManagerError::Io(std::io::ErrorKind::PermissionDenied)),
                "refused on the read error"
            );
            assert_eq!(get("server@default.ini"), saved, "the file as it was");
        }
    }

    // ── A model profile file is written as sent ─────────────────────────────────
    {
        put("server@default.ini", saved);
        let m = manager(true);
        let fixtures = tempfile::tempdir().expect("tempdir");
        let model_profiles = real_model_files(
            fixtures.path(),
            "[Qwen3.8-27B]\nmodel = /models/Qwen3.8-27B-Q8_0.gguf\n",
        );
        assert_eq!(
            m.write_file(&alice, "models.ini", &model_profiles, true),
            Ok(())
        );
        assert_eq!(get("models.ini"), model_profiles);
    }
}

/// E2E phase for where the instance files live, for this crate's e2e test to
/// call (it sets the process-wide `PRIVATE_AI_DATA_ROOT` redirect,
/// `src/tests/mod.rs`): the engine's directory (`paths::engine_config_dir`,
/// `/etc/llama.cpp` on Linux), never the framework's
/// (`paths::system_config_dir`, `/etc/private-ai`, which keeps `bff.ini`).
/// Files an earlier layout left in the framework's directory are neither
/// read, listed, nor moved.
#[cfg(test)]
pub(crate) fn manager_engine_dir_e2e() {
    use fakes::{FakeAuthority, FakeGroup, FakeResolver, FakeService};

    let dir = tempfile::tempdir().expect("tempdir");
    crate::paths::redirect_for_tests(dir.path());
    let engine = crate::paths::engine_config_dir();
    let framework = crate::paths::system_config_dir();
    let alice = Caller(":1.42".into());
    let names = |d: &std::path::Path| -> std::collections::BTreeSet<String> {
        std::fs::read_dir(d)
            .map(|r| {
                r.flatten()
                    .map(|e| e.file_name().to_string_lossy().into_owned())
                    .collect()
            })
            .unwrap_or_default()
    };

    // An earlier layout's files and the framework's own, in its directory.
    std::fs::create_dir_all(&framework).expect("framework dir");
    let stray_ini = "[Server]\nPort = 8090\nHostname = 127.0.0.1\n";
    let stray_model_profiles =
        real_model_files(dir.path(), "[stray]\nmodel = /models/stray.gguf\n");
    for (name, text) in [
        ("server@stray.ini", stray_ini),
        ("server@default.ini", stray_ini),
        ("models.ini", &stray_model_profiles),
        ("bff.ini", "[Bff]\nListen = 127.0.0.1:8590\n"),
    ] {
        std::fs::write(framework.join(name), text).expect("framework file");
    }
    let framework_before = names(&framework);

    let m = Manager::new(
        FakeService::new(),
        FakeAuthority::new(true),
        FakeResolver::lan(),
    )
    .with_group(FakeGroup::new());

    // ── Nothing of the framework's directory counts ───────────────────────
    assert_eq!(m.list_instances(), Vec::<String>::new(), "no instance yet");
    assert_eq!(m.read_file(None::<&Caller>, "models.ini"), Ok(None));
    assert_eq!(m.read_file(Some(&alice), "server@default.ini"), Ok(None));
    assert_eq!(
        m.start(&alice, "stray"),
        Err(ManagerError::NoInstance),
        "a file in the framework's directory is no instance"
    );
    assert!(m.service().calls.lock().unwrap().is_empty());

    // ── Writes land in the engine's directory ─────────────────────────────
    let ini = "[Server]\nPort = 8080\nHostname = 127.0.0.1\n";
    let fixtures = tempfile::tempdir().expect("tempdir");
    let model_profiles = real_model_files(
        fixtures.path(),
        "[Qwen3.8-27B]\nmodel = /models/Qwen3.8-27B-Q8_0.gguf\n",
    );
    let disabled = real_model_files(dir.path(), "[Old]\nmodel = /models/old.gguf\n");
    // The record and the assignment come first: a pool write renders
    // `models@<id>.ini` for the instances the record names, and the start below
    // needs a render with a model profile in it.
    write_record(&engine, &["default"]);
    std::fs::write(
        engine.join(ASSIGNMENTS_FILE),
        "[default]\nQwen3.8-27B = on\n",
    )
    .expect(ASSIGNMENTS_FILE);
    assert_eq!(
        m.write_file(&alice, "server@default.ini", ini, false),
        Ok(())
    );
    assert_eq!(
        m.write_file(&alice, "models.ini", &model_profiles, false),
        Ok(())
    );
    // The switched-off file is a client's read, never a write, so this seeds it
    // as an install that has one leaves it.
    std::fs::write(engine.join("models.ini.disabled"), &disabled).expect("disabled");
    for (name, text) in [
        ("server@default.ini", ini),
        ("models.ini", &model_profiles),
        ("models.ini.disabled", &disabled),
    ] {
        assert_eq!(
            std::fs::read_to_string(engine.join(name)).ok().as_deref(),
            Some(text),
            "{name} in the engine's directory"
        );
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mode = |n: &str| {
            std::fs::metadata(engine.join(n))
                .unwrap()
                .permissions()
                .mode()
                & 0o7777
        };
        assert_eq!(mode("server@default.ini"), 0o640);
        assert_eq!(mode("models.ini"), 0o644);
    }
    assert_eq!(names(&framework), framework_before, "nothing written there");
    for (name, text) in [
        ("server@default.ini", stray_ini),
        ("models.ini", &stray_model_profiles),
    ] {
        assert_eq!(
            std::fs::read_to_string(framework.join(name)).unwrap(),
            text,
            "{name}: the framework's directory is left as it was"
        );
    }

    // ── Reads and the list come from the engine's directory ───────────────
    assert_eq!(m.list_instances(), ["default"]);
    assert_eq!(
        m.read_file(None::<&Caller>, "models.ini"),
        Ok(Some(plain(&model_profiles)))
    );
    assert_eq!(
        m.read_file(Some(&alice), "server@default.ini"),
        Ok(Some(plain(ini)))
    );
    assert_eq!(m.start(&alice, "default"), Ok(()));

    // ── The address check reads the other instances there too ─────────────
    // server@stray.ini (port 8090) is in the framework's directory only: no
    // collision with it.
    assert_eq!(
        m.write_file(
            &alice,
            "server@lan-2.ini",
            "[Server]\nPort = 8090\nHostname = 127.0.0.1\n",
            false
        ),
        Ok(())
    );
    write_record(&engine, &["default", "lan-2"]);
    assert_eq!(m.list_instances(), ["default", "lan-2"]);
    assert!(
        !engine.join("server@stray.ini").exists(),
        "nothing moved from the framework's directory"
    );
}

/// E2E phase for the manager core while polkit waits for a user to
/// authenticate, for this crate's e2e test to call (it sets the process-wide
/// `PRIVATE_AI_DATA_ROOT` redirect, `src/tests/mod.rs`). A caller whose
/// authorization is pending holds up no other caller, reads or changes, and
/// gets its own answer once polkit gives it; writes still run one at a time,
/// so the address check of one sees the file another has just written.
#[cfg(test)]
pub(crate) fn manager_waits_e2e() {
    use fakes::{
        Call, FakeAuthority, FakeResolver, FakeService, Gate, GateAuthority, GateResolver,
        GATE_WAIT,
    };
    use std::sync::mpsc::Receiver;
    use std::sync::Arc;
    use std::time::Duration;

    /// The result on `rx`, which must come while another caller's question
    /// is still open at `gate`. On a timeout the gate is opened first, so
    /// the threads still waiting at it finish, then the test fails.
    fn answer<T>(gate: &Gate, what: &str, rx: Receiver<T>) -> T {
        let deadline = Duration::from_secs(2);
        rx.recv_timeout(deadline).unwrap_or_else(|_| {
            gate.open();
            panic!(
                "{what}: no answer within {deadline:?} while another caller's \
                 authorization was pending"
            )
        })
    }

    let alice = Caller(":1.42".into());
    let bob = Caller(":1.43".into());
    let carol = Caller(":1.44".into());
    let default_ini = "[Server]\nPort = 8080\nHostname = 127.0.0.1\n";
    let fixtures = tempfile::tempdir().expect("tempdir");
    let model_profiles = real_model_files(
        fixtures.path(),
        "[Qwen3.8-27B]\nmodel = /models/Qwen3.8-27B-Q8_0.gguf\n",
    );

    // ── A pending authorization holds up no other caller ─────────────────
    {
        let dir = tempfile::tempdir().expect("tempdir");
        crate::paths::redirect_for_tests(dir.path());
        let sys = crate::paths::engine_config_dir();
        std::fs::create_dir_all(&sys).expect("system config dir");
        std::fs::write(sys.join("server@default.ini"), default_ini).expect("write default");
        std::fs::write(sys.join("models.ini"), &model_profiles).expect("write presets");
        write_record(&sys, &["default"]);
        // A start needs a render with a model profile in it, so the fixture
        // assigns one and renders it, as a pool write would.
        std::fs::write(sys.join(ASSIGNMENTS_FILE), "[default]\nQwen3.8-27B = on\n")
            .expect(ASSIGNMENTS_FILE);
        std::fs::write(
            crate::paths::instance_models("default").expect("render path"),
            render_of("default", &model_profiles, ""),
        )
        .expect("render");

        let gate = Gate::new();
        let m = Arc::new(
            Manager::new(
                FakeService::new(),
                GateAuthority::new(&alice, &gate, &[(&alice, true), (&bob, true)]),
                FakeResolver::lan(),
            )
            .with_group(crate::manager::service::fakes::FakeGroup::new()),
        );
        let pending = {
            let (m, alice) = (m.clone(), alice.clone());
            on_thread(move || m.start(&alice, "default"))
        };
        assert!(gate.arrived(1), "alice's start reached polkit");

        let status = {
            let m = m.clone();
            on_thread(move || m.status("default"))
        };
        assert_eq!(
            answer(&gate, "status", status),
            Ok(UnitState::Active),
            "status of the instance alice is starting"
        );
        let list = {
            let m = m.clone();
            on_thread(move || m.list_instances())
        };
        assert_eq!(answer(&gate, "list_instances", list), vec!["default"]);
        let read = {
            let m = m.clone();
            on_thread(move || m.read_file(None::<&Caller>, "models.ini"))
        };
        assert_eq!(
            answer(&gate, "read_file", read),
            Ok(Some(plain(&model_profiles)))
        );
        let stop = {
            let (m, bob) = (m.clone(), bob.clone());
            on_thread(move || m.stop(&bob, "default"))
        };
        assert_eq!(
            answer(&gate, "bob's stop", stop),
            Ok(()),
            "another caller's authorization is asked and answered"
        );
        let refused = {
            let (m, carol) = (m.clone(), carol.clone());
            on_thread(move || m.write_file(&carol, "models.ini", "[x]\nmodel = /m/x.gguf\n", false))
        };
        assert_eq!(
            answer(&gate, "carol's write", refused),
            Err(ManagerError::NotAuthorized),
            "a caller polkit refuses is refused at once"
        );
        let pool = model_profiles.clone();
        let write = {
            let (m, bob) = (m.clone(), bob.clone());
            on_thread(move || m.write_file(&bob, "models.ini", &pool, false))
        };
        assert_eq!(answer(&gate, "bob's write", write), Ok(()));

        assert!(
            pending.try_recv().is_err(),
            "alice's start is still waiting for polkit"
        );
        gate.open();
        let started = pending
            .recv_timeout(GATE_WAIT)
            .expect("alice's start returns once polkit answers");
        assert_eq!(started, Ok(()), "alice's own answer, after the others");
        assert_eq!(
            *m.service().calls.lock().unwrap(),
            vec![
                Call::Status("default".into()),
                Call::Stop("default".into()),
                Call::Start("default".into())
            ],
            "alice's start reaches systemd only after polkit answered"
        );
        assert_eq!(
            *m.authority().asked.lock().unwrap(),
            vec![
                (alice.clone(), Action::Control),
                (bob.clone(), Action::Control),
                (carol.clone(), Action::Configure),
                (bob.clone(), Action::Configure)
            ]
        );
        assert_eq!(
            std::fs::read_to_string(sys.join("models.ini")).unwrap(),
            model_profiles,
            "the refused write left models.ini"
        );
    }

    // ── A pending caller refused in the end changes nothing ──────────────
    {
        let dir = tempfile::tempdir().expect("tempdir");
        crate::paths::redirect_for_tests(dir.path());
        let sys = crate::paths::engine_config_dir();
        std::fs::create_dir_all(&sys).expect("system config dir");
        std::fs::write(sys.join("server@default.ini"), default_ini).expect("write default");
        // bob's start needs the record: a `server@default.ini` no section
        // names is no instance.
        write_record(&sys, &["default"]);
        // bob's start also needs a render with a model profile to load.
        write_render(&sys, "default");

        let gate = Gate::new();
        let m = Arc::new(
            Manager::new(
                FakeService::new(),
                GateAuthority::new(&alice, &gate, &[(&alice, false), (&bob, true)]),
                FakeResolver::lan(),
            )
            .with_group(crate::manager::service::fakes::FakeGroup::new()),
        );
        let pending = {
            let (m, alice) = (m.clone(), alice.clone());
            on_thread(move || {
                m.write_file(&alice, "server@default.ini", "[Server]\nPort = 9\n", false)
            })
        };
        assert!(gate.arrived(1), "alice's write reached polkit");
        let start = {
            let (m, bob) = (m.clone(), bob.clone());
            on_thread(move || m.start(&bob, "default"))
        };
        assert_eq!(answer(&gate, "bob's start", start), Ok(()));
        gate.open();
        let refused = pending
            .recv_timeout(GATE_WAIT)
            .expect("alice's write returns once polkit answers");
        assert_eq!(refused, Err(ManagerError::NotAuthorized));
        assert_eq!(
            std::fs::read_to_string(sys.join("server@default.ini")).unwrap(),
            default_ini,
            "the refused write left the file"
        );
        assert_eq!(
            *m.service().calls.lock().unwrap(),
            vec![Call::Start("default".into())],
            "only bob's start reached systemd"
        );
    }

    // ── Writes run one at a time: a check sees the write before it ───────
    // The first write's address check is held at a slow host lookup (another
    // instance's name). The second, of a colliding address, must not slip in
    // between that check and the first write: one of them is written, the
    // other refused naming it, whichever went first.
    {
        let dir = tempfile::tempdir().expect("tempdir");
        crate::paths::redirect_for_tests(dir.path());
        let sys = crate::paths::engine_config_dir();
        std::fs::create_dir_all(&sys).expect("system config dir");
        std::fs::write(
            sys.join("server@slow.ini"),
            "[Server]\nPort = 9000\nHostname = slow-box\n",
        )
        .expect("write slow");
        // Both candidates of the race are in the record, so whichever write
        // lands second sees the other's file: a section whose file is missing
        // reads as the default binding, and 8081 on gpu-box is not that.
        write_record(&sys, &["lan-a", "lan-b", "slow"]);

        let gate = Gate::new();
        let resolver = GateResolver::new(
            FakeResolver::new(&[("gpu-box", &["192.168.1.5"]), ("slow-box", &["10.0.0.7"])]),
            "slow-box",
            &gate,
        );
        let m = Arc::new(
            Manager::new(FakeService::new(), FakeAuthority::new(true), resolver)
                .with_group(crate::manager::service::fakes::FakeGroup::new()),
        );
        let lan_a = "[Server]\nPort = 8081\nHostname = 192.168.1.5\n";
        let lan_b = "[Server]\nPort = 8081\nHostname = gpu-box\n";
        let first = {
            let (m, alice) = (m.clone(), alice.clone());
            on_thread(move || m.write_file(&alice, "server@lan-a.ini", lan_a, false))
        };
        assert!(
            gate.arrived(1),
            "the first write's address check reached the slow lookup"
        );
        let second = {
            let (m, bob) = (m.clone(), bob.clone());
            on_thread(move || m.write_file(&bob, "server@lan-b.ini", lan_b, false))
        };
        // Time for the second to run past the first, if it can.
        let early = second.recv_timeout(Duration::from_millis(500)).ok();
        gate.open();
        let first = first
            .recv_timeout(GATE_WAIT)
            .expect("the first write returns");
        let second = early
            .or_else(|| second.recv_timeout(GATE_WAIT).ok())
            .expect("the second write returns");
        let a = sys.join("server@lan-a.ini").is_file();
        let b = sys.join("server@lan-b.ini").is_file();
        match (&first, &second) {
            (Ok(()), Err(ManagerError::AddressInUse(other))) => {
                assert_eq!(other, "lan-a", "the second names the first");
                assert!(a && !b, "only lan-a written: {a} {b}");
                assert_eq!(
                    std::fs::read_to_string(sys.join("server@lan-a.ini")).unwrap(),
                    lan_a
                );
            }
            (Err(ManagerError::AddressInUse(other)), Ok(())) => {
                assert_eq!(other, "lan-b", "the first names the second");
                assert!(b && !a, "only lan-b written: {a} {b}");
            }
            _ => panic!(
                "two colliding writes ran at once, the second not seeing the first: \
                 first {first:?}, second {second:?}; lan-a written {a}, lan-b written {b}"
            ),
        }
    }
}

/// E2E phase for the content checks of `write_file`, for this crate's e2e
/// test to call (it sets the process-wide `PRIVATE_AI_DATA_ROOT` redirect,
/// `src/tests/mod.rs`). Invalid contents are refused with the check's
/// refusal after the authorization, and nothing is written.
#[cfg(test)]
pub(crate) fn manager_checks_e2e() {
    use fakes::{FakeAuthority, FakeResolver, FakeService};
    use pai_core::checks::Refusal;

    let dir = tempfile::tempdir().expect("tempdir");
    crate::paths::redirect_for_tests(dir.path());
    let sys = crate::paths::engine_config_dir();
    let alice = Caller(":1.42".into());
    let manager = |allow: bool| {
        Manager::new(
            FakeService::new(),
            FakeAuthority::new(allow),
            FakeResolver::lan(),
        )
        .with_group(crate::manager::service::fakes::FakeGroup::new())
    };
    let refusal = |key: &'static str, values: &[&str]| Refusal {
        key,
        values: values.iter().map(|v| v.to_string()).collect(),
    };
    let invalid =
        |key: &'static str, values: &[&str]| Err(ManagerError::Invalid(refusal(key, values)));
    let read = |name: &str| std::fs::read_to_string(sys.join(name)).ok();
    let listing = || -> Vec<String> {
        let mut names: Vec<String> = std::fs::read_dir(&sys)
            .map(|rd| {
                rd.flatten()
                    .map(|e| e.file_name().to_string_lossy().into_owned())
                    .collect()
            })
            .unwrap_or_default();
        names.sort();
        names
    };
    let flagged = |ids: &[&str]| -> String {
        real_model_files(
            dir.path(),
            &ids.iter()
                .map(|id| format!("[{id}]\nmodel = /models/{id}.gguf\nload-on-startup = true\n"))
                .collect::<String>(),
        )
    };

    // ── A refused caller is told only that: no check runs first ──────────
    {
        let m = manager(false);
        assert_eq!(
            m.write_file(&alice, "server@default.ini", "[Server]\nPort = 0\n", false),
            Err(ManagerError::NotAuthorized)
        );
        assert_eq!(
            m.write_file(&alice, "models.ini", "[a b]\nmodel = /m/a.gguf\n", false),
            Err(ManagerError::NotAuthorized)
        );
        assert!(!sys.exists(), "nothing created");
    }

    // ── An invalid instance file: refused, nothing created ───────────────
    {
        let m = manager(true);
        assert_eq!(
            m.write_file(&alice, "server@default.ini", "[Server]\nPort = 0\n", false),
            invalid("not-whole-number-in-range", &["Port", "0", "1", "65535"])
        );
        assert!(listing().is_empty(), "nothing written: {:?}", listing());
        assert_eq!(
            m.write_file(
                &alice,
                "server@default.ini",
                "[Server]\nPort = 8081\nHostname = 127.0.0.1\nModelsDir = /models #1\n",
                false
            ),
            invalid("comment-markers", &["ModelsDir"])
        );
        assert!(listing().is_empty(), "nothing written: {:?}", listing());
        // The Server checks below the GUI: a bind list mixing 0.0.0.0 with
        // another address, a negative ModelsMax, a log level past 5.
        for (text, want) in [
            (
                "[Server]\nPort = 8081\nHostname = 0.0.0.0,127.0.0.1\n",
                invalid("bind-all-with-others", &[]),
            ),
            (
                "[Server]\nPort = 8081\nHostname = 127.0.0.1\nModelsMax = -1\n",
                invalid(
                    "not-whole-number-in-range",
                    &["ModelsMax", "-1", "0", "2147483647"],
                ),
            ),
            (
                "[Server]\nPort = 8081\nHostname = 127.0.0.1\nLogVerbosity = 7\n",
                invalid(
                    "not-whole-number-in-range",
                    &["LogVerbosity", "7", "0", "5"],
                ),
            ),
        ] {
            assert_eq!(
                m.write_file(&alice, "server@default.ini", text, false),
                want,
                "{text:?}"
            );
            assert!(listing().is_empty(), "nothing written: {:?}", listing());
        }
        assert_eq!(
            m.list_instances(),
            Vec::<String>::new(),
            "no instance added"
        );
    }

    // ── Valid files are written; then an invalid rewrite keeps the old ───
    let default_ini = "[Server]\nPort = 8081\nHostname = 127.0.0.1\nModelsMax = 2\n";
    let model_profiles = real_model_files(
        dir.path(),
        "[Qwen3.8-27B]\nmodel = /models/Qwen3.8-27B-Q8_0.gguf\nctx-size = 172032\n",
    );
    {
        let m = manager(true);
        // default is an instance: its section of the record, before the writes
        // that make its render.
        write_record(&sys, &["default"]);
        assert_eq!(
            m.write_file(&alice, "server@default.ini", default_ini, false),
            Ok(())
        );
        assert_eq!(
            m.write_file(&alice, "models.ini", &model_profiles, false),
            Ok(())
        );

        assert_eq!(
            m.write_file(
                &alice,
                "server@default.ini",
                "[Server]\nPort = 8081\nHostname = 127.0.0.1\nModelsMemoryMargin = -1\n",
                false
            ),
            invalid(
                "not-whole-number-in-range",
                &["ModelsMemoryMargin", "-1", "0", "2147483647"]
            )
        );
        assert_eq!(
            read("server@default.ini").as_deref(),
            Some(default_ini),
            "old kept"
        );

        let bad = format!("{model_profiles}[Other]\nmodel = C:\\Models #1\\o.gguf\n");
        assert_eq!(
            m.write_file(&alice, "models.ini", &bad, false),
            invalid("comment-markers", &["model"])
        );
        let bad = real_model_files(dir.path(), &format!(
            "{model_profiles}[Other]\nmodel = /models/o.gguf\nmoe-cache-mib = 4096\nsplit-mode = tensor\n"
        ));
        assert_eq!(
            m.write_file(&alice, "models.ini", &bad, false),
            invalid("moe-cache-with-tensor-split", &[])
        );
        assert_eq!(
            read("models.ini").as_deref(),
            Some(model_profiles.as_str()),
            "old kept"
        );
        assert_eq!(
            listing(),
            [
                "instances.ini",
                "launch@default.ini",
                "models.ini",
                "models@default.ini",
                "server@default.ini"
            ],
            "no temp file left"
        );
        assert!(
            m.service().calls.lock().unwrap().is_empty(),
            "a write calls no service"
        );
    }

    // ── The GPUs of the server profile: `[Server] Devices` (R2) ───────────
    {
        let m = manager(true);
        // Every write here rewrites default's server profile, and the file's
        // text is a fixture of the next phase: take what this phase found and
        // put it back when it ends.
        let found = read("server@default.ini").expect("default's server profile");
        // The write reaches the list rule, naming the key.
        for (value, entry) in [
            ("ROCm0, ROCm1", " ROCm1"),
            ("CPU", "CPU"),
            ("none,ROCm0", "none"),
        ] {
            assert_eq!(
                m.write_file(
                    &alice,
                    "server@default.ini",
                    &format!("[Server]\nPort = 8081\nDevices = {value}\n"),
                    false
                ),
                invalid("device-bad-name", &["Devices", entry]),
                "`{value}` is refused at the write"
            );
        }
        assert_eq!(
            read("server@default.ini").as_deref(),
            Some(default_ini),
            "a refused Devices kept the old file"
        );
        // A name the node has no GPU for is taken, and so is `none` alone: the
        // rule judges the shape, never the probe's inventory.
        for value in ["ROCm7", "CUDA0,ROCm1", "none"] {
            let text = format!("[Server]\nPort = 8081\nDevices = {value}\n");
            assert_eq!(
                m.write_file(&alice, "server@default.ini", &text, false),
                Ok(()),
                "`{value}` is taken"
            );
            assert_eq!(
                read("server@default.ini").as_deref(),
                Some(text.as_str()),
                "the GPUs are in the file the manager wrote"
            );
        }
        // The phase ends with the file as it found it, so the next phase reads
        // the profile its own fixture pins.
        std::fs::write(sys.join("server@default.ini"), found).expect("restore");
    }

    // ── load-on-startup against the instances' ModelsMax ─────────────────
    {
        let m = manager(true);
        // The assignments decide what each render loads, so they decide what
        // each ModelsMax has to hold. The manager writes the file in a later
        // step; the fixtures write it by hand.
        let a_none = "[default]\n* = inherit\n\n[lan-2]\n";
        let a_lan = "[default]\n* = inherit\n\n[lan-2]\na = inherit\n";
        let a_split = "[default]\nb = inherit\n\n[lan-2]\n* = inherit\n";
        std::fs::write(sys.join("assignments.ini"), a_none).expect("assignments.ini");
        // default's ModelsMax is 2, and default is an instance of the record.
        write_record(&sys, &["default"]);
        let render_before = read("models@default.ini");
        assert_eq!(
            m.write_file(&alice, "models.ini", &flagged(&["a", "b", "c"]), false),
            Ok(()),
            "the count is a fit check of the instance (IP3c), so the pool lands"
        );
        assert_eq!(
            m.not_converged_locked().expect("the rows"),
            vec![NotConverged {
                instance: "default".into(),
                key: "load-on-startup-over-max".into(),
                args: vec!["3".into(), "2".into(), "default".into()],
            }],
            "the refusal names the instance whose render is over its max: it is \
             reported, not a refusal of the write"
        );
        assert_eq!(
            read("models.ini").as_deref(),
            Some(flagged(&["a", "b", "c"]).as_str())
        );
        assert_eq!(
            read("models@default.ini"),
            render_before,
            "and default keeps the pair it had"
        );
        assert_eq!(
            read("models@default.ini"),
            Some(pai_core::assign::render(&model_profiles, "", None)),
            "the pair it keeps is the one it had: it opens with the \
             generated header, so it is the pool rendered, never the pool text"
        );
        assert_eq!(
            m.write_file(&alice, "models.ini", &flagged(&["a", "b"]), false),
            Ok(())
        );
        assert_eq!(
            m.not_converged_locked().expect("the rows"),
            [],
            "the pool that fits converges default again"
        );
        // Lowering ModelsMax under them is NOT refused: the render's count is
        // what stands today, so it is no news (IP3c, the growth rule of every
        // fit check). The save lands, and default's pair stands as it is.
        assert_eq!(
            m.write_file(
                &alice,
                "server@default.ini",
                "[Server]\nPort = 8081\nHostname = 127.0.0.1\nModelsMax = 1\n",
                false
            ),
            Ok(())
        );
        assert_eq!(
            read("server@default.ini").as_deref(),
            Some("[Server]\nPort = 8081\nHostname = 127.0.0.1\nModelsMax = 1\n"),
            "the lowered cap lands"
        );
        assert_eq!(
            m.not_converged_locked().expect("the rows"),
            [],
            "and the count the cap now names was already standing, so default is not over it"
        );
        // So a new instance that could not start with what its own render
        // loads: lan-2 inherits a model profile and has no room for it. The
        // save lands (IP3c): the profile is written, lan-2 keeps its pair,
        // which for a new instance is no pair at all, and is reported.
        std::fs::write(sys.join("assignments.ini"), a_lan).expect("assignments.ini");
        // lan-2 joins the record: the render of a profile no instance runs is
        // judged against nothing, so its ModelsMax of 0 would be accepted.
        write_record(&sys, &["default", "lan-2"]);
        assert_eq!(
            m.write_file(
                &alice,
                "server@lan-2.ini",
                "[Server]\nPort = 8082\nHostname = 127.0.0.1\nModelsMax = 0\n",
                false
            ),
            Ok(()),
            "the profile the save writes lands"
        );
        assert_eq!(
            m.not_converged_locked().expect("the rows"),
            vec![NotConverged {
                instance: "lan-2".into(),
                key: "load-on-startup-unlimited".into(),
                args: vec!["1".into(), "lan-2".into()],
            }],
            "lan-2 is reported: its render loads a preset with no ModelsMax"
        );
        assert!(
            read("models@lan-2.ini").is_none(),
            "and lan-2 has no pair: its first render is the one the check stopped"
        );
        // The cross-instance minimum is gone: each instance is checked
        // against its own render. default is told one model profile, lan-2 all
        // three, and lan-2's own max of 3 holds them.
        std::fs::write(sys.join("assignments.ini"), a_split).expect("assignments.ini");
        assert_eq!(
            m.write_file(
                &alice,
                "server@lan-2.ini",
                "[Server]\nPort = 8082\nHostname = 127.0.0.1\nModelsMax = 3\n",
                false
            ),
            Ok(())
        );
        write_record(&sys, &["default", "lan-2"]);
        assert_eq!(
            m.write_file(&alice, "models.ini", &flagged(&["a", "b", "c"]), false),
            Ok(()),
            "the pool has three presets and no instance is over its own max"
        );
        assert_eq!(
            read("models@default.ini"),
            Some(pai_core::assign::render(
                &flagged(&["a", "b", "c"]),
                "",
                pai_core::ini::text::section_text(a_split, "default").as_deref(),
            )),
            "default's render holds the one preset it is told"
        );
        assert_eq!(
            read("models@lan-2.ini"),
            Some(pai_core::assign::render(
                &flagged(&["a", "b", "c"]),
                "",
                pai_core::ini::text::section_text(a_split, "lan-2").as_deref(),
            )),
            "lan-2's render holds every preset of the pool"
        );
        assert_eq!(
            listing(),
            [
                "assignments.ini",
                "instances.ini",
                "launch@default.ini",
                "launch@lan-2.ini",
                "models.ini",
                "models@default.ini",
                "models@lan-2.ini",
                "server@default.ini",
                "server@lan-2.ini"
            ],
            "one pair per instance, beside the pool"
        );
        // The phase ends with default's profile as it found it, so the blocks
        // that follow read the ModelsMax their own fixture pins.
        std::fs::write(sys.join("server@default.ini"), default_ini).expect("restore");
    }

    // ── A broken section already on disk blocks no other write ───────────
    let broken = real_model_files(
        dir.path(),
        "[old]\nmodel = /models/old.gguf\nctx-size = abc\n",
    );
    {
        let m = manager(true);
        std::fs::write(sys.join("models.ini"), &broken).expect("hand-edited fixture");
        let added = real_model_files(
            dir.path(),
            &format!("{broken}[new]\nmodel = /models/new.gguf\n"),
        );
        assert_eq!(m.write_file(&alice, "models.ini", &added, false), Ok(()));
        assert_eq!(read("models.ini"), Some(added.clone()));
        let edited = real_model_files(
            dir.path(),
            &format!("{broken}[new]\nmodel = /models/new.gguf\nparallel = 0\n"),
        );
        assert_eq!(
            m.write_file(&alice, "models.ini", &edited, false),
            invalid(
                "not-whole-number-in-range",
                &["parallel", "0", "1", "2147483647"]
            )
        );
        assert_eq!(read("models.ini"), Some(added));
    }

    // ── models.ini.disabled is a read, not a write ───────────────────────
    {
        let m = manager(true);
        // The file is retired with the model data model (`ARCHITECTURE.md`,
        // "Instance profiles"): a profile switched off sits in `models.ini`
        // with no instance profile naming it. So the manager's own verbs keep
        // it (the create and the delete move a section), and a client's write
        // of it is the name refusal, whatever the text.
        assert_eq!(
            m.write_file(&alice, "models.ini.disabled", &broken, false),
            Err(ManagerError::BadFileName)
        );
        assert_eq!(
            read("models.ini.disabled"),
            None,
            "the refused write created no file"
        );
        let edited = real_model_files(
            dir.path(),
            &format!("{broken}[parked]\nmodel = /models/p.gguf\nfit-target = 2048MiB\n"),
        );
        assert_eq!(
            m.write_file(&alice, "models.ini.disabled", &edited, false),
            Err(ManagerError::BadFileName)
        );
        let many = real_model_files(
            dir.path(),
            &format!("{broken}{}", flagged(&["a", "b", "c", "d", "e"])),
        );
        assert_eq!(
            m.write_file(&alice, "models.ini.disabled", &many, false),
            Err(ManagerError::BadFileName)
        );
        // Switching the broken one back on: new in models.ini, so checked.
        let enabled = real_model_files(dir.path(), "[new]\nmodel = /models/new.gguf\n");
        std::fs::write(sys.join("models.ini"), &enabled).expect("models.ini fixture");
        let on = real_model_files(dir.path(), &format!("{enabled}{broken}"));
        assert_eq!(
            m.write_file(&alice, "models.ini", &on, false),
            invalid(
                "not-whole-number-in-range",
                &["ctx-size", "abc", "0", "2147483647"]
            )
        );
    }

    // ── A model profile switched on with an inline comment: the disabled file is
    //    the other file check_contents hands to models_file ──────────────
    {
        let m = manager(true);
        let kept = real_model_files(dir.path(), "[kept]\nmodel = /models/kept.gguf\n");
        let parked = real_model_files(dir.path(), "[commented]\nmodel = /models/c.gguf ; Q8\n");
        std::fs::write(sys.join("models.ini"), &kept).expect("models.ini fixture");
        std::fs::write(sys.join("models.ini.disabled"), &parked).expect("disabled fixture");
        // The switch moves the section whole: new to models.ini, held
        // identically by the disabled file, so the commented line is known.
        let on = real_model_files(dir.path(), &format!("{kept}{parked}"));
        assert_eq!(m.write_file(&alice, "models.ini", &on, false), Ok(()));
        assert_eq!(
            read("models.ini").as_deref(),
            Some(on.as_str()),
            "the switch wrote through"
        );
    }

    // ── ctx-size = 0 (the context from the model) is writable ────────────
    {
        let m = manager(true);
        let zero = real_model_files(
            dir.path(),
            "[new]\nmodel = /models/new.gguf\nctx-size = 0\n",
        );
        assert_eq!(m.write_file(&alice, "models.ini", &zero, false), Ok(()));
        assert_eq!(read("models.ini"), Some(zero.clone()));
    }

    // ── One id across models.ini and models.ini.disabled ───────────────
    {
        let m = manager(true);
        let enabled = real_model_files(dir.path(), "[new]\nmodel = /models/new.gguf\n");
        let parked = real_model_files(
            dir.path(),
            "[Parked]\nmodel = /models/parked.gguf\nctx-size = 8192\n",
        );
        std::fs::write(sys.join("models.ini"), &enabled).expect("models.ini fixture");
        std::fs::write(sys.join("models.ini.disabled"), &parked).expect("disabled fixture");
        // A new enabled model profile named like a parked one, in another case.
        let namesake = real_model_files(
            dir.path(),
            &format!("{enabled}[parked]\nmodel = /models/other.gguf\n"),
        );
        assert_eq!(
            m.write_file(&alice, "models.ini", &namesake, false),
            invalid("model-profile-name-taken", &["parked"])
        );
        assert_eq!(read("models.ini"), Some(enabled.clone()), "old kept");
        // A new parked model profile named like an enabled one: the file is a
        // read, so the write is the name refusal, and the file is left.
        let namesake = real_model_files(
            dir.path(),
            &format!("{parked}[NEW]\nmodel = /models/other.gguf\n"),
        );
        assert_eq!(
            m.write_file(&alice, "models.ini.disabled", &namesake, false),
            Err(ManagerError::BadFileName)
        );
        assert_eq!(read("models.ini.disabled"), Some(parked.clone()));
        // Switching the parked one on writes models.ini first: a move.
        let on = real_model_files(dir.path(), &format!("{enabled}{parked}"));
        assert_eq!(m.write_file(&alice, "models.ini", &on, false), Ok(()));
        assert_eq!(read("models.ini"), Some(on));
    }

    // ── Floats, enums, JSON and min <= max: refused, the file kept ───────
    {
        let m = manager(true);
        let kept = real_model_files(dir.path(), "[kept]\nmodel = /models/kept.gguf\n");
        std::fs::write(sys.join("models.ini"), &kept).expect("models.ini fixture");
        std::fs::write(sys.join("models.ini.disabled"), "").expect("disabled fixture");
        let with = |lines: &str| {
            real_model_files(
                dir.path(),
                &format!("{kept}[new]\nmodel = /models/new.gguf\n{lines}"),
            )
        };
        assert_eq!(
            m.write_file(&alice, "models.ini", &with("temp = NaN\n"), false),
            invalid("not-a-number", &["temp", "NaN"])
        );
        assert_eq!(
            m.write_file(
                &alice,
                "models.ini",
                &with("chat-template-kwargs = [1]\n"),
                false
            ),
            invalid("not-json-object", &["chat-template-kwargs"])
        );
        assert_eq!(
            m.write_file(
                &alice,
                "models.ini",
                &with("spec-draft-n-min = 5\nspec-draft-n-max = 4\n"),
                false
            ),
            invalid(
                "min-above-max",
                &["spec-draft-n-min", "5", "spec-draft-n-max", "4"]
            )
        );
        let enum_refused =
            m.write_file(&alice, "models.ini", &with("split-mode = banana\n"), false);
        assert!(
            matches!(&enum_refused, Err(ManagerError::Invalid(r)) if r.key == "not-one-of"),
            "{enum_refused:?}"
        );
        assert_eq!(
            m.write_file(&alice, "models.ini", &with("top-p = inf\n"), false),
            invalid("not-a-number", &["top-p", "inf"])
        );
        assert_eq!(
            read("models.ini").as_deref(),
            Some(kept.as_str()),
            "old kept"
        );
        assert_eq!(read("models.ini.disabled").as_deref(), Some(""), "old kept");
    }
}

/// The model profiles render of every instance: `assignments.ini` is read through
/// the file interface and never written by the client; the creation of an
/// instance and every write of a pool file write `models@<id>.ini`, which
/// holds the pool sections that instance's section of the assignments
/// selects, plus the global `[*]` section for every instance.
#[cfg(test)]
pub(crate) fn manager_assignments_e2e() {
    use fakes::{FakeAuthority, FakeResolver, FakeService};
    use pai_core::assign::render;

    let dir = tempfile::tempdir().expect("tempdir");
    crate::paths::redirect_for_tests(dir.path());
    let sys = crate::paths::engine_config_dir();
    std::fs::create_dir_all(&sys).expect("engine config dir");
    let alice = Caller(":1.42".into());
    let m = Manager::new(
        FakeService::new(),
        FakeAuthority::new(true),
        FakeResolver::lan(),
    )
    .with_group(crate::manager::service::fakes::FakeGroup::new());
    let read = |name: &str| std::fs::read_to_string(sys.join(name)).ok();
    let listing = || -> Vec<String> {
        let mut names: Vec<String> = std::fs::read_dir(&sys)
            .map(|rd| {
                rd.flatten()
                    .map(|e| e.file_name().to_string_lossy().into_owned())
                    .collect()
            })
            .unwrap_or_default();
        names.sort();
        names
    };
    // The two sections really differ, and no `*` sits on `[default]`: it
    // would assign every model profile of the pool to default, leaving nothing to
    // tell the two renders apart. default is told `a`, lan-2 is told `b`.
    let assignments = "[default]\na = inherit\n\n[lan-2]\nb = inherit\n";
    let section = |id: &str| -> String {
        pai_core::ini::text::section_text(assignments, id).unwrap_or_default()
    };
    let default_ini = "[Server]\nPort = 8081\nHostname = 127.0.0.1\nModelsMax = 2\n";
    // The global `[*]` section reaches the pool only as a hand edit: a
    // client write of it is refused, `*` is not a model profile id.
    let global = "[*]\ntemp = 0.7\n\n";
    let pool = real_model_files(
        dir.path(),
        "[a]\nmodel = /models/a.gguf\nload-on-startup = true\n\n[b]\nmodel = /models/b.gguf\n",
    );
    let off = real_model_files(dir.path(), "[z]\nmodel = /models/z.gguf\n");

    // ── assignments.ini: the client reads it, and never writes it ────────
    assert_eq!(
        m.read_file(Some(&alice), "assignments.ini"),
        Ok(None),
        "no assignments.ini is nothing to read"
    );
    std::fs::write(sys.join("assignments.ini"), assignments).expect("assignments.ini");
    assert_eq!(
        m.read_file(Some(&alice), "assignments.ini"),
        Ok(Some(plain(assignments))),
        "the assignments are read as they stand"
    );
    assert_eq!(
        m.write_file(&alice, "assignments.ini", "[default]\n* = off\n", false),
        Err(ManagerError::BadFileName),
        "the client does not write the assignments"
    );
    assert_eq!(
        m.write_file_if(&alice, "assignments.ini", "[default]\n* = off\n", ""),
        Err(ManagerError::BadFileName),
        "not even a compare-and-swap"
    );
    assert_eq!(
        read("assignments.ini").as_deref(),
        Some(assignments),
        "a refused write leaves the assignments as they were"
    );

    // ── The render of an instance is its section of the assignments ──────
    // The pool stands first, so the renders of the created instances hold
    // model profiles: default is told `a`, lan-2 is told `b`.
    write_record(&sys, &["default", "lan-2", "lan-3"]);
    assert_eq!(m.write_file(&alice, "models.ini", &pool, false), Ok(()));
    assert_eq!(
        m.write_file(&alice, "server@default.ini", default_ini, false),
        Ok(())
    );
    assert_eq!(
        read("models@default.ini"),
        Some(render(&pool, "", Some(section("default").as_str()))),
        "default's render on the instance's creation"
    );
    assert_eq!(
        m.write_file(
            &alice,
            "server@lan-2.ini",
            "[Server]\nPort = 8082\nHostname = 127.0.0.1\nModelsMax = 3\n",
            false
        ),
        Ok(())
    );
    let lan2 = read("models@lan-2.ini").expect("lan-2's render");
    assert_eq!(
        lan2,
        render(&pool, "", Some(section("lan-2").as_str())),
        "lan-2's render is its own section"
    );
    assert!(
        lan2.contains("[b]") && !read("models@default.ini").unwrap().contains("[b]"),
        "the two renders differ: b is lan-2's alone"
    );
    assert!(
        read("models@default.ini").unwrap().contains("[a]") && !lan2.contains("[a]"),
        "and a is default's alone"
    );
    // An instance with no section gets the global section alone; the pool
    // has none until the hand edit below, so this render carries no model profile.
    assert_eq!(
        m.write_file(
            &alice,
            "server@lan-3.ini",
            "[Server]\nPort = 8083\nHostname = 127.0.0.1\n",
            false
        ),
        Ok(())
    );
    assert_eq!(
        read("models@lan-3.ini"),
        Some(render(&pool, "", None)),
        "no section: no preset of the pool"
    );
    assert_eq!(
        listing(),
        [
            "assignments.ini",
            "instances.ini",
            "launch@default.ini",
            "launch@lan-2.ini",
            "launch@lan-3.ini",
            "models.ini",
            "models@default.ini",
            "models@lan-2.ini",
            "models@lan-3.ini",
            "server@default.ini",
            "server@lan-2.ini",
            "server@lan-3.ini",
        ],
        "one pair per instance, and nothing else"
    );

    // ── Every write of a pool file re-renders every instance ─────────────
    // A client write of the global `[*]` section is refused: `*` is not a
    // model profile id. It arrives as a hand edit, and the next render takes it
    // from the file on disk.
    let on_disk = format!("{global}{pool}");
    assert!(
        matches!(
            m.write_file(&alice, "models.ini", &on_disk, false),
            Err(ManagerError::Invalid(r)) if r.key == "invalid-model-profile-id" && r.values == ["*"]
        ),
        "the client cannot write the global section"
    );
    assert_eq!(
        read("models.ini").as_deref(),
        Some(pool.as_str()),
        "the refused write changed nothing"
    );
    std::fs::write(sys.join("models.ini"), &on_disk).expect("hand-edited pool");
    // The switched-off pool is a client's read, never a write, so this seeds it
    // as an install that has one leaves it; the pool write below is the one
    // that re-renders.
    std::fs::write(sys.join("models.ini.disabled"), &off).expect("models.ini.disabled");
    assert_eq!(m.write_file(&alice, "models.ini", &pool, false), Ok(()));
    assert_eq!(
        read("models@default.ini"),
        Some(render(&pool, &off, Some(section("default").as_str()))),
        "the render takes the pool as the write left it: the global section of the
         hand edit is gone with it"
    );
    assert_eq!(
        read("models@lan-3.ini"),
        Some(render(&pool, &off, None)),
        "lan-3 is the pool alone: every instance is re-rendered by the write"
    );
    assert_eq!(m.write_file(&alice, "models.ini", &pool, false), Ok(()));
    assert_eq!(
        read("models@default.ini"),
        Some(render(&pool, &off, Some(section("default").as_str()))),
        "default keeps a alone: the pool's other presets are not its rows"
    );
    assert_eq!(
        read("models@lan-2.ini"),
        Some(render(&pool, &off, Some(section("lan-2").as_str()))),
        "lan-2 keeps b alone"
    );
    assert_eq!(
        read("models@lan-3.ini"),
        Some(render(&pool, &off, None)),
        "the client's write replaced the hand-edited global section"
    );
    let added = real_model_files(
        dir.path(),
        &format!("{pool}\n[c]\nmodel = /models/c.gguf\nload-on-startup = true\n"),
    );
    assert_eq!(m.write_file(&alice, "models.ini", &added, false), Ok(()));
    assert_eq!(
        read("models@default.ini"),
        Some(render(&added, &off, Some(section("default").as_str()))),
        "the second write of the pool re-renders"
    );

    // ── A refused write changes no render ────────────────────────────────
    let broken = real_model_files(
        dir.path(),
        &format!("{added}\n[bad]\nmodel = /models/bad.gguf\nctx-size = abc\n"),
    );
    let answer = m.write_file(&alice, "models.ini", &broken, false);
    assert!(
        matches!(answer, Err(ManagerError::Invalid(_))),
        "a pool with a broken section is refused: {answer:?}"
    );
    assert_eq!(
        read("models@default.ini"),
        Some(render(&added, &off, Some(section("default").as_str()))),
        "a refused write leaves every render as it was"
    );
    assert_eq!(
        listing(),
        [
            "assignments.ini",
            "instances.ini",
            "launch@default.ini",
            "launch@lan-2.ini",
            "launch@lan-3.ini",
            "models.ini",
            "models.ini.disabled",
            "models@default.ini",
            "models@lan-2.ini",
            "models@lan-3.ini",
            "server@default.ini",
            "server@lan-2.ini",
            "server@lan-3.ini",
        ],
        "a render of an id is never a temp file left behind"
    );

    // ── The manager's start writes the render of every instance ──────────
    assert!(
        m.render_failures().is_empty(),
        "the renders are rewritten from what is on disk"
    );
    assert_eq!(
        read("models@default.ini"),
        Some(render(&added, &off, Some(section("default").as_str()))),
        "the start render agrees with the last write"
    );
}

/// E2E phase: `SetModelProfile`, the one verb that writes `assignments.ini`
/// (protocol 3, its `As` form and the same name over the pipe). The key of one
/// model profile goes in the instance's section and every model profile file of
/// every instance renders again from the pool and the assignments on disk:
/// `on`, `off` and `inherit` are the key's three values, `none` removes the
/// key, and `*`, the whole pool, takes `none` and `inherit` only. A call
/// answers before it writes: an id that is no instance (`InvalidId`,
/// `NoInstance`), a model profile the pool does not hold
/// (`no-model-profile-named`, not asked of `*`), a value outside the list, and a
/// model file the render this record leaves ADDS and the node does not have
/// (IP3c review 4) each write and render nothing. What the render needs of the
/// cards and of the `ModelsMax` of the server profile the instance's section
/// names is the fit check of the render pass (IP3c, `ARCHITECTURE.md`
/// "Convergence"): the write stands, and the instance the check stops keeps its
/// last good pair. Guarded as a write of a served file is: `configure`
/// first, the `As` form answered only for the bff's account and a current
/// member of `pai`, and one record of the action log per call, done or refused.
#[cfg(test)]
pub(crate) fn manager_set_model_profile_e2e() {
    use fakes::{
        FakeActionLog, FakeAuthority, FakeGroup, FakePasswords, FakeResolver, FakeService,
    };
    use pai_core::assign::render;

    let dir = tempfile::tempdir().expect("tempdir");
    crate::paths::redirect_for_tests(dir.path());
    let sys = crate::paths::engine_config_dir();
    std::fs::create_dir_all(&sys).expect("engine config dir");

    // The bff's account, its request for alice (a member of `pai` in
    // [`FakePasswords`]) and for bob (who is not), and a desktop caller of its
    // own.
    let bff = Caller(":1.9".into());
    let member = Requester {
        caller: bff.clone(),
        principal: Some(Principal("alice".into())),
    };
    let outsider = Requester {
        caller: bff.clone(),
        principal: Some(Principal("bob".into())),
    };
    let desktop = Caller(":1.42".into());
    let core = |allow: bool, passwords: FakePasswords, log: FakeActionLog| {
        Manager::new(
            FakeService::new(),
            FakeAuthority::new(allow).quietly(true).bff(&bff),
            FakeResolver::lan(),
        )
        .with_group(FakeGroup::new())
        .with_passwords(passwords)
        .with_log(log)
    };
    let text_of = |name: &str| std::fs::read_to_string(sys.join(name)).expect("read");
    let section =
        |text: &str, id: &str| pai_core::ini::text::section_text(text, id).unwrap_or_default();
    let keys_of = |text: &str, id: &str, key: &str| -> Vec<String> {
        section(text, id)
            .lines()
            .filter(|line| line.trim_start().starts_with(&format!("{key} ")))
            .map(|line| line.to_string())
            .collect()
    };
    let listing = || -> Vec<String> {
        let mut names: Vec<String> = std::fs::read_dir(&sys)
            .map(|rd| {
                rd.flatten()
                    .map(|e| e.file_name().to_string_lossy().into_owned())
                    .collect()
            })
            .unwrap_or_default();
        names.sort();
        names
    };
    let records =
        |lines: &std::sync::Arc<std::sync::Mutex<Vec<String>>>| lines.lock().unwrap().clone();

    // The pool, and the record and the server profiles the fixture hands the
    // manager: `a` loads on startup, `b` does not, `m` sits in the switched-off
    // file so a pool test sees both halves.
    let pool = real_model_files(
        dir.path(),
        "[a]\nmodel = /models/a.gguf\nload-on-startup = true\n\n[b]\nmodel = /models/b.gguf\n",
    );
    let off_pool = real_model_files(
        dir.path(),
        "[m]\nmodel = /models/m.gguf\nload-on-startup = true\n",
    );
    write_record(&sys, &["default", "lan-2"]);
    std::fs::write(
        sys.join("server@default.ini"),
        "[Server]\nPort = 8080\nHostname = 127.0.0.1\n",
    )
    .expect("server@default.ini");
    std::fs::write(
        sys.join("server@lan-2.ini"),
        "[Server]\nPort = 8081\nHostname = 127.0.0.1\n",
    )
    .expect("server@lan-2.ini");
    std::fs::write(sys.join("models.ini"), &pool).expect("models.ini");
    std::fs::write(sys.join("models.ini.disabled"), &off_pool).expect("models.ini.disabled");
    // The assignments as an earlier call left them: a comment, a key of its
    // own, a second section. The verb writes one key of one section and
    // touches none of the rest.
    let seeded =
        "# the manager's own record\n[default]\nb = off\nnote = keep me\n\n[lan-2]\n* = inherit\n";

    // ── The protocol these verbs came with stays 3 ────────────────────────
    assert_eq!(
        crate::manager::pipe::PROTOCOL,
        3,
        "the version `Version` reports: 3 came with the first assignment verb, and with `Instances` and `SetInstanceTags` (the protocol stays 3, so they are part of 3)"
    );

    // ── Each value writes its key, and re-renders every instance ─────────
    {
        let log = FakeActionLog::new();
        let lines = log.lines.clone();
        let m = core(true, FakePasswords::new(), log);
        std::fs::write(sys.join("assignments.ini"), seeded).expect("seed");

        // `on`: the key, the pool's section in the render, the rest kept.
        assert_eq!(
            m.set_model_profile(&desktop, "default", "a", "on")
                .map(|_| ()),
            Ok(()),
            "a model profile of the pool, on"
        );
        let written = text_of("assignments.ini");
        assert_eq!(
            keys_of(&written, "default", "a"),
            vec!["a = on".to_string()],
            "one key for the model profile, at the value given"
        );
        assert!(
            keys_of(&written, "lan-2", "a").is_empty(),
            "the key is default's, not lan-2's"
        );
        assert!(
            written.contains("# the manager's own record"),
            "the comment of the file is kept"
        );
        assert!(
            written.contains("note = keep me"),
            "a key that is no model profile is kept"
        );
        assert!(
            written.contains("[lan-2]\n* = inherit\n"),
            "the section of another instance is kept as it was"
        );
        let d = text_of("models@default.ini");
        let l2 = text_of("models@lan-2.ini");
        assert_eq!(
            d,
            render(&pool, &off_pool, Some(&section(&written, "default"))),
            "default's render is the pool cut by its section, the one just written"
        );
        assert!(
            d.contains("[a]") && d.contains("load-on-startup = true"),
            "an `on` model profile is in the render the instance loads, its own keys with it"
        );
        assert_eq!(
            l2,
            render(&pool, &off_pool, Some(&section(&written, "lan-2"))),
            "the write re-renders every instance: lan-2 from its own section"
        );
        assert!(
            l2.contains("[a]"),
            "lan-2 keeps a: `* = inherit` is its own"
        );

        // `off`: the key, the section out of the render.
        assert_eq!(
            m.set_model_profile(&desktop, "default", "a", "off")
                .map(|_| ()),
            Ok(()),
            "the same key, off"
        );
        let written = text_of("assignments.ini");
        assert_eq!(
            keys_of(&written, "default", "a"),
            vec!["a = off".to_string()],
            "the key is replaced, not added to"
        );
        let d = text_of("models@default.ini");
        assert_eq!(
            d,
            render(&pool, &off_pool, Some(&section(&written, "default"))),
            "the render follows the section"
        );
        assert!(
            !d.contains("[a]"),
            "an `off` model profile is out of the render: {d:?}"
        );

        // `inherit`: the key kept, the pool's own state decides.
        assert_eq!(
            m.set_model_profile(&desktop, "default", "a", "inherit")
                .map(|_| ()),
            Ok(()),
            "the same key, inherit"
        );
        let written = text_of("assignments.ini");
        assert_eq!(
            keys_of(&written, "default", "a"),
            vec!["a = inherit".to_string()],
            "`inherit` is a value of the key, not its removal"
        );
        assert!(
            text_of("models@default.ini").contains("[a]"),
            "a model profile of the switched-on pool, inherited, is in the render"
        );

        // A second call with the same value writes one key, not two.
        assert_eq!(
            m.set_model_profile(&desktop, "default", "a", "inherit")
                .map(|_| ()),
            Ok(()),
            "the same value again"
        );
        assert_eq!(
            keys_of(&text_of("assignments.ini"), "default", "a"),
            vec!["a = inherit".to_string()],
            "the key stays one"
        );

        // `none`: the key out, everything else as it was.
        assert_eq!(
            m.set_model_profile(&desktop, "default", "a", "none")
                .map(|_| ()),
            Ok(()),
            "`none` is answered"
        );
        let written = text_of("assignments.ini");
        assert_eq!(
            section(&written, "default"),
            "[default]\nb = off\nnote = keep me\n\n",
            "`none` removes the key and leaves the section's other lines"
        );
        assert!(
            !text_of("models@default.ini").contains("[a]"),
            "a model profile with no key and no `*` is not rendered"
        );

        assert_eq!(
            records(&lines),
            vec![
                r#"action="set_model_profile" target="default" caller=":1.42" principal=- outcome=ok"#,
                r#"action="set_model_profile" target="default" caller=":1.42" principal=- outcome=ok"#,
                r#"action="set_model_profile" target="default" caller=":1.42" principal=- outcome=ok"#,
                r#"action="set_model_profile" target="default" caller=":1.42" principal=- outcome=ok"#,
                r#"action="set_model_profile" target="default" caller=":1.42" principal=- outcome=ok"#,
            ],
            "one record per call: the verb's action, the instance as its target, the caller, no principal as a bare -, ok"
        );
    }

    // ── The global `*`: `none` and `inherit` only ─────────────────────────
    {
        let m = core(true, FakePasswords::new(), FakeActionLog::new());
        std::fs::write(sys.join("assignments.ini"), "[default]\n").expect("seed");
        assert_eq!(
            m.set_model_profile(&desktop, "default", "*", "inherit")
                .map(|_| ()),
            Ok(()),
            "`*` takes inherit"
        );
        assert_eq!(
            section(&text_of("assignments.ini"), "default"),
            "[default]\n* = inherit\n",
            "the global key, written"
        );
        let d = text_of("models@default.ini");
        assert!(
            d.contains("[a]") && d.contains("[b]") && !d.contains("[m]"),
            "`* = inherit` is every model profile of the pool following its own file: the switched-off one stays out"
        );
        assert_eq!(
            m.set_model_profile(&desktop, "default", "*", "none")
                .map(|_| ()),
            Ok(()),
            "`*` takes none"
        );
        assert_eq!(
            section(&text_of("assignments.ini"), "default"),
            "[default]\n",
            "`none` removes the global key too"
        );

        // The pool check is not asked of `*`: with an empty pool the global key
        // is written, while one model profile is refused for want of a pool.
        std::fs::write(sys.join("models.ini"), "").expect("empty pool");
        std::fs::write(sys.join("models.ini.disabled"), "").expect("empty pool");
        assert_eq!(
            m.set_model_profile(&desktop, "default", "*", "inherit")
                .map(|_| ()),
            Ok(()),
            "`*` needs no model profile of the pool to exist"
        );
        assert!(
            matches!(
                m.set_model_profile(&desktop, "default", "a", "on"),
                Err(ManagerError::Invalid(r))
                    if r.key == "no-model-profile-named" && r.values == ["a"]
            ),
            "the same model profile, in an empty pool, is refused naming it"
        );
        std::fs::write(sys.join("models.ini"), &pool).expect("the pool back");

        // `on` and `off` under `*`: refused, with nothing written and no
        // render of their own.
        let seed = text_of("assignments.ini");
        let before = listing();
        let renders: Vec<(String, String)> = ["models@default.ini", "models@lan-2.ini"]
            .iter()
            .map(|name| (name.to_string(), text_of(name)))
            .collect();
        for value in ["on", "off"] {
            assert!(
                m.set_model_profile(&desktop, "default", "*", value)
                    .is_err(),
                "`*` refuses {value}"
            );
        }
        assert_eq!(
            text_of("assignments.ini"),
            seed,
            "a `*` refused writes nothing"
        );
        assert_eq!(listing(), before, "and creates no file");
        for (name, text) in renders {
            assert_eq!(text_of(&name), text, "and re-renders nothing: {name}");
        }
    }

    // ── A call that answers before it writes ──────────────────────────────
    {
        let m = core(true, FakePasswords::new(), FakeActionLog::new());
        // The fixture is made, not assumed: the block above leaves the
        // switched-off file empty and default's render built from a pool of
        // nothing. One call puts `a` in the record and in the render, the way
        // a real assignment does, so "rendered nothing" below has something to
        // keep.
        std::fs::write(sys.join("models.ini"), &pool).expect("pool back");
        std::fs::write(sys.join("models.ini.disabled"), &off_pool).expect("off pool back");
        std::fs::write(sys.join("assignments.ini"), "[default]\n").expect("seed");
        assert_eq!(
            m.set_model_profile(&desktop, "default", "a", "on")
                .map(|_| ()),
            Ok(()),
            "the fixture: a call assigns a to default"
        );
        let seed = text_of("assignments.ini");
        let before = listing();
        let renders: Vec<(String, String)> = ["models@default.ini", "models@lan-2.ini"]
            .iter()
            .map(|name| (name.to_string(), text_of(name)))
            .collect();
        assert!(
            renders[0].1.contains("[a]"),
            "the fixture: a is in default's render before the refusals"
        );

        assert_eq!(
            m.set_model_profile(&desktop, "lan-3", "a", "on"),
            Err(ManagerError::NoInstance),
            "an id that is no section of the record"
        );
        for bad in ["", "../x", "Default", "a_b"] {
            assert_eq!(
                m.set_model_profile(&desktop, bad, "a", "on"),
                Err(ManagerError::InvalidId),
                "not an instance id at all: {bad:?}"
            );
        }
        assert!(
            matches!(
                m.set_model_profile(&desktop, "default", "zzz", "on"),
                Err(ManagerError::Invalid(r))
                    if r.key == "no-model-profile-named" && r.values == ["zzz"]
            ),
            "a model profile the pool does not hold, named"
        );
        for value in ["maybe", "ON", "", "on,off", "none "] {
            assert!(
                matches!(
                    m.set_model_profile(&desktop, "default", "a", value),
                    Err(ManagerError::Invalid(r))
                        if r.key == "model-profile-bad-value" && r.values == [value]
                ),
                "a value outside none, inherit, on, off is refused: {value:?}"
            );
        }
        assert_eq!(
            text_of("assignments.ini"),
            seed,
            "every refusal above wrote nothing"
        );
        assert_eq!(listing(), before, "and created no file");
        for (name, text) in renders {
            assert_eq!(text_of(&name), text, "and re-rendered nothing: {name}");
        }
    }

    // ── `ModelsMax` of the server profile the section names ───────────────
    {
        let m = core(true, FakePasswords::new(), FakeActionLog::new());
        std::fs::write(
            sys.join("server@default.ini"),
            "[Server]\nPort = 8080\nHostname = 127.0.0.1\nModelsMax = 1\n",
        )
        .expect("ModelsMax 1");
        std::fs::write(sys.join("assignments.ini"), "[default]\n* = inherit\n").expect("seed");
        let renders: Vec<(String, String)> = ["models@default.ini", "models@lan-2.ini"]
            .iter()
            .map(|name| (name.to_string(), text_of(name)))
            .collect();
        // `* = inherit` puts the switched-on half in effect, so `a` loads on
        // startup; switching on `m`, whose own section asks for it, is the
        // second one, over the max of one. The count is a fit check of the
        // instance (IP3c): the assignment lands and the instance is reported.
        let over = m.set_model_profile(&desktop, "default", "m", "on");
        assert_eq!(
            over.as_ref().map(|_| ()),
            Ok(()),
            "the assignment over the max lands, answered {over:?}"
        );
        assert_eq!(
            over.expect("the answer").not_converged,
            vec![NotConverged {
                instance: "default".into(),
                key: "load-on-startup-over-max".into(),
                args: vec!["2".into(), "1".into(), "default".into()],
            }],
            "over the max, the instance named, reported"
        );
        assert_eq!(
            section(&text_of("assignments.ini"), "default"),
            "[default]\n* = inherit\nm = on\n",
            "the assignment landed in the record"
        );
        for (name, text) in renders {
            assert_eq!(
                text_of(&name),
                text,
                "and default keeps the pair it had: {name}"
            );
        }

        // The rule is one-sided: what leaves the render is never judged.
        std::fs::write(
            sys.join("assignments.ini"),
            "[default]\n* = inherit\nb = off\n",
        )
        .expect("b off");
        assert_eq!(
            m.set_model_profile(&desktop, "default", "a", "off")
                .map(|_| ()),
            Ok(()),
            "an assignment off is accepted whatever the max"
        );
        assert_eq!(
            section(&text_of("assignments.ini"), "default"),
            "[default]\n* = inherit\nb = off\na = off\n",
            "and its key landed"
        );
        assert_eq!(
            m.set_model_profile(&desktop, "default", "*", "none")
                .map(|_| ()),
            Ok(()),
            "a `*` removed, which leaves the whole pool out of the render, is accepted"
        );
        assert_eq!(
            section(&text_of("assignments.ini"), "default"),
            "[default]\nb = off\na = off\n",
            "the global key is out, the section's other keys kept"
        );
        assert_eq!(
            text_of("models@default.ini"),
            render(&pool, &off_pool, Some("[default]\nb = off\na = off\n")),
            "and the render follows: nothing of the pool is in effect"
        );
        std::fs::write(
            sys.join("server@default.ini"),
            "[Server]\nPort = 8080\nHostname = 127.0.0.1\n",
        )
        .expect("ModelsMax back");
    }

    // ── `ModelsMax` of the profile the section names, not of a file named as the id
    {
        // An instance whose server profile is not its own name: `lan` runs
        // `small`, and a `server@lan.ini` that no instance runs sits beside it
        // with a max of its own. Both directions are pinned: the max that
        // refuses is the profile's, and the max that lets the write land is
        // the profile's too.
        std::fs::write(
            sys.join(INSTANCES_FILE),
            "[default]\nServerProfile = default\n\n[lan-2]\nServerProfile = lan-2\n\n[lan]\nServerProfile = small\n",
        )
        .expect("instances.ini: lan runs the profile small");
        std::fs::write(
            sys.join("server@small.ini"),
            "[Server]\nPort = 8082\nHostname = 127.0.0.1\nModelsMax = 1\n",
        )
        .expect("server@small.ini");
        std::fs::write(
            sys.join("server@lan.ini"),
            "[Server]\nPort = 8083\nHostname = 127.0.0.1\nModelsMax = 8\n",
        )
        .expect("server@lan.ini, the file no instance runs");
        let m = core(true, FakePasswords::new(), FakeActionLog::new());
        std::fs::write(sys.join("assignments.ini"), "[lan]\n* = inherit\n").expect("seed");
        assert!(
            m.render_failures().is_empty(),
            "the record with a profile that is not the id renders every instance"
        );
        let renders: Vec<(String, String)> =
            ["models@default.ini", "models@lan-2.ini", "models@lan.ini"]
                .iter()
                .map(|name| (name.to_string(), text_of(name)))
                .collect();
        // `* = inherit` puts `a` in effect, so switching on `m` is the second
        // load on startup: over `small`'s max of 1, the instance is reported
        // with the max of 1. The 8 of `server@lan.ini` is not the number the
        // check reads. The assignment lands (IP3c), and `lan` keeps its pair.
        let over = m.set_model_profile(&desktop, "lan", "m", "on");
        assert_eq!(
            over.as_ref().map(|_| ()),
            Ok(()),
            "the assignment over the profile's max lands, answered {over:?}"
        );
        assert_eq!(
            over.expect("the answer").not_converged,
            vec![NotConverged {
                instance: "lan".into(),
                key: "load-on-startup-over-max".into(),
                args: vec!["2".into(), "1".into(), "lan".into()],
            }],
            "the max of the profile the section names, reported"
        );
        assert_eq!(
            section(&text_of("assignments.ini"), "lan"),
            "[lan]\n* = inherit\nm = on\n",
            "the key landed in the record"
        );
        for (name, text) in &renders {
            assert_eq!(
                text_of(name),
                *text,
                "and every instance keeps the pair it had: {name}"
            );
        }

        // The reverse: `small` raised to 8 while the file named after the id
        // drops to 1. The same call lands, so the check cannot be reading the
        // id's file either.
        std::fs::write(
            sys.join("server@small.ini"),
            "[Server]\nPort = 8082\nHostname = 127.0.0.1\nModelsMax = 8\n",
        )
        .expect("server@small.ini raised");
        std::fs::write(
            sys.join("server@lan.ini"),
            "[Server]\nPort = 8083\nHostname = 127.0.0.1\nModelsMax = 1\n",
        )
        .expect("server@lan.ini lowered");
        assert_eq!(
            m.set_model_profile(&desktop, "lan", "m", "on").map(|_| ()),
            Ok(()),
            "the profile's own max of 8 lets the second load on startup land"
        );
        assert_eq!(
            section(&text_of("assignments.ini"), "lan"),
            "[lan]\n* = inherit\nm = on\n",
            "the key landed in the section of the instance it was asked for"
        );
        assert_eq!(
            text_of("models@lan.ini"),
            render(&pool, &off_pool, Some("[lan]\n* = inherit\nm = on\n")),
            "and the render of the instance follows the record"
        );

        // The fixture's record and files back for the phases that follow.
        std::fs::remove_file(sys.join("server@small.ini")).expect("small out");
        std::fs::remove_file(sys.join("server@lan.ini")).expect("lan profile out");
        std::fs::remove_file(sys.join("models@lan.ini")).expect("lan render out");
        write_record(&sys, &["default", "lan-2"]);
    }

    // ── Guarded: `configure`, the principal, one record ──────────────────
    {
        // A caller without the right: NotAuthorized, nothing written.
        let log = FakeActionLog::new();
        let lines = log.lines.clone();
        let m = core(false, FakePasswords::new(), log);
        std::fs::write(sys.join("assignments.ini"), "[default]\n").expect("seed");
        let seed = text_of("assignments.ini");
        let before = listing();
        assert_eq!(
            m.set_model_profile(&desktop, "default", "a", "on"),
            Err(ManagerError::NotAuthorized),
            "a caller outside `pai`, polkit refusing"
        );
        assert_eq!(
            text_of("assignments.ini"),
            seed,
            "a refused caller writes nothing"
        );
        assert_eq!(listing(), before, "and leaves no render of its own behind");
        assert_eq!(
            records(&lines),
            vec![
                r#"action="set_model_profile" target="default" caller=":1.42" principal=- outcome=refused"#.to_string()
            ],
            "the refusal is recorded as one record too"
        );
    }
    {
        // The bff acting for a member: the pair grants the call, the record
        // names both.
        let p = FakePasswords::new();
        let asked_admin = p.asked_admin.clone();
        let log = FakeActionLog::new();
        let lines = log.lines.clone();
        let m = core(true, p, log);
        std::fs::write(sys.join("assignments.ini"), "[default]\n").expect("seed");
        assert_eq!(
            m.set_model_profile(&member, "default", "a", "on")
                .map(|_| ()),
            Ok(()),
            "the bff assigns for a member of `pai`"
        );
        assert_eq!(
            keys_of(&text_of("assignments.ini"), "default", "a"),
            vec!["a = on".to_string()],
            "for the member"
        );
        assert_eq!(
            *asked_admin.lock().unwrap(),
            vec!["alice".to_string()],
            "the membership of the principal, never of the caller"
        );
        assert_eq!(
            *m.authority().asked_bff.lock().unwrap(),
            vec![bff.clone()],
            "the caller is checked first, at every call"
        );
        assert_eq!(
            m.authority().asked.lock().unwrap().len(),
            0,
            "polkit is never asked for a principal: its subject is the bff"
        );
        assert_eq!(
            records(&lines),
            vec![
                r#"action="set_model_profile" target="default" caller=":1.9" principal="alice" outcome=ok"#.to_string()
            ],
            "the record carries the caller and the principal"
        );

        // A principal outside `pai`: the call is refused and nothing is written.
        std::fs::write(sys.join("assignments.ini"), "[default]\n").expect("seed");
        let seed = text_of("assignments.ini");
        assert_eq!(
            m.set_model_profile(&outsider, "default", "a", "on"),
            Err(ManagerError::NotAuthorized),
            "a right password of a caller outside `pai` is not enough"
        );
        assert_eq!(
            text_of("assignments.ini"),
            seed,
            "a refused principal writes nothing"
        );

        // A caller that is not the bff cannot name a principal at all.
        std::fs::write(sys.join("assignments.ini"), "[default]\n").expect("seed");
        let seed = text_of("assignments.ini");
        let desktop_member = Requester {
            caller: desktop.clone(),
            principal: Some(Principal("alice".into())),
        };
        assert_eq!(
            m.set_model_profile(&desktop_member, "default", "a", "on"),
            Err(ManagerError::NotAuthorized),
            "only the bff answers for a signed-in member"
        );
        assert_eq!(
            text_of("assignments.ini"),
            seed,
            "and a caller outside the bff writes nothing"
        );
    }

    // ── A file that cannot be read: the read's failure, and nothing written ─
    {
        let bytes_of = |name: &str| std::fs::read(sys.join(name)).expect("bytes");
        let kept_renders = || -> Vec<(String, String)> {
            ["models@default.ini", "models@lan-2.ini"]
                .iter()
                .map(|name| (name.to_string(), text_of(name)))
                .collect()
        };
        let restore_pool = || {
            std::fs::write(sys.join("models.ini"), &pool).expect("pool");
            std::fs::write(sys.join("models.ini.disabled"), &off_pool).expect("off pool");
        };

        // `assignments.ini` with bytes that are no text. A read taken as empty
        // here renders a record of one key and replaces the file with it: every
        // instance's assignments, gone.
        {
            let m = core(true, FakePasswords::new(), FakeActionLog::new());
            std::fs::write(
                sys.join("assignments.ini"),
                b"[default]\nb = off\nnote = keep me\n\xed\xa0\xbf\n",
            )
            .expect("broken record");
            let seed = bytes_of("assignments.ini");
            let before = listing();
            let renders = kept_renders();
            assert_eq!(
                m.set_model_profile(&desktop, "default", "a", "on"),
                Err(ManagerError::Io(std::io::ErrorKind::InvalidData)),
                "an assignments.ini that cannot be read answers the read's own kind, it is not an empty file"
            );
            assert_eq!(
                bytes_of("assignments.ini"),
                seed,
                "and its bytes are the bytes that were there: the atomic write never replaced them"
            );
            assert_eq!(
                listing(),
                before,
                "and the call left no file of its own behind: no temp, no render"
            );
            for (name, text) in renders {
                assert_eq!(text_of(&name), text, "and re-rendered nothing: {name}");
            }
        }

        // A directory at the record's path: the same answer, and the path
        // stays what it is.
        {
            let m = core(true, FakePasswords::new(), FakeActionLog::new());
            std::fs::remove_file(sys.join("assignments.ini")).expect("record out");
            std::fs::create_dir(sys.join("assignments.ini")).expect("record dir");
            let before = listing();
            let renders = kept_renders();
            assert_eq!(
                m.set_model_profile(&desktop, "default", "a", "on"),
                Err(ManagerError::Io(std::io::ErrorKind::IsADirectory)),
                "a directory at the record's path answers the read's own kind"
            );
            assert!(
                sys.join("assignments.ini").is_dir(),
                "the directory is still there: nothing was renamed over it"
            );
            assert_eq!(
                listing(),
                before,
                "and the call left no temp file of its own next to it"
            );
            for (name, text) in renders {
                assert_eq!(text_of(&name), text, "and re-rendered nothing: {name}");
            }
            std::fs::remove_dir(sys.join("assignments.ini")).expect("record dir out");
        }

        // A model profile file that cannot be read: the same. A pool read as
        // empty refuses a model profile of the real pool as
        // `no-model-profile-named`, and a `*` call renders every instance from
        // a pool of nothing.
        for (name, profile) in [("models.ini", "a"), ("models.ini.disabled", "m")] {
            let m = core(true, FakePasswords::new(), FakeActionLog::new());
            restore_pool();
            std::fs::write(sys.join("assignments.ini"), "[default]\n* = inherit\n").expect("seed");
            std::fs::write(sys.join(name), b"[a]\nmodel = \xff\n").expect("broken pool");
            let seed = bytes_of("assignments.ini");
            let before = listing();
            let renders = kept_renders();
            let answer = m
                .set_model_profile(&desktop, "default", profile, "on")
                .map(|_| ());
            assert_eq!(
                answer,
                Err(ManagerError::Io(std::io::ErrorKind::InvalidData)),
                "a {name} that cannot be read answers the read's own kind, for model profile {profile}"
            );
            assert!(
                !matches!(
                    &answer,
                    Err(ManagerError::Invalid(r)) if r.key == "no-model-profile-named"
                ),
                "an unreadable {name} is never the pool answering `no-model-profile-named`"
            );
            assert_eq!(
                bytes_of("assignments.ini"),
                seed,
                "and the unreadable {name} wrote nothing to the record"
            );
            assert_eq!(listing(), before, "and created no file: {name}");
            for (file, text) in renders {
                assert_eq!(text_of(&file), text, "and rendered nothing: {name}, {file}");
            }
        }

        // A record that is really absent is an empty one: the first assignment
        // of a fresh machine lands. The line endings of a new file are the INI
        // layer's own, so the section is compared with the endings folded.
        {
            let m = core(true, FakePasswords::new(), FakeActionLog::new());
            restore_pool();
            std::fs::remove_file(sys.join("assignments.ini")).expect("record out");
            assert_eq!(
                m.set_model_profile(&desktop, "default", "a", "on")
                    .map(|_| ()),
                Ok(()),
                "a first assignment with no assignments.ini at all is answered"
            );
            let written = text_of("assignments.ini");
            assert_eq!(
                section(&written.replace("\r\n", "\n"), "default"),
                "[default]\na = on\n",
                "the key lands in the section of the instance it was asked for: {written:?}"
            );
            assert!(
                text_of("models@default.ini").contains("[a]"),
                "and the render follows the record that just appeared"
            );
        }
    }

    // ── Slice 7 review: the resource rule is a step rule, the id is exact,
    //    the record keeps the bytes it was not asked to write, the text ────

    {
        // The instance starts OVER its max: three model profiles that load on
        // startup are in effect, and the server profile its section names caps
        // them at one. The resource rule reads the step a call takes, so a call
        // that lowers the count lands even when the render it produces is
        // still over the max.
        let three = real_model_files(
            dir.path(),
            "[a]\nmodel = /m/a.gguf\nload-on-startup = true\n\n\
                     [b]\nmodel = /m/b.gguf\nload-on-startup = true\n\n\
                     [c]\nmodel = /m/c.gguf\nload-on-startup = true\n\n\
                     [d]\nmodel = /m/d.gguf\n",
        );
        let m = core(true, FakePasswords::new(), FakeActionLog::new());
        write_record(&sys, &["default", "lan-2"]);
        // The cap is off while the fixture's pair is written, so the pair on
        // disk is the standing render (IP3c): the cap drops after it.
        std::fs::write(
            sys.join("server@default.ini"),
            "[Server]\nPort = 8080\nHostname = 127.0.0.1\n",
        )
        .expect("no cap");
        std::fs::write(
            sys.join("server@lan-2.ini"),
            "[Server]\nPort = 8081\nHostname = 127.0.0.1\n",
        )
        .expect("lan-2 profile");
        std::fs::write(sys.join("models.ini"), &three).expect("three loads");
        std::fs::write(sys.join("models.ini.disabled"), "").expect("no off file");
        std::fs::write(
            sys.join("assignments.ini"),
            "[default]\na = on\nb = on\nc = on\n",
        )
        .expect("three on");
        // The cap drops LAST, over a pair the pass has already written (IP3c):
        // the render on disk is then the standing news the step rule must not
        // refuse, and the fit check's growth rule reports it as no news.
        assert!(
            m.render_failures().is_empty(),
            "the fixture renders every instance under a cap of four"
        );
        std::fs::write(
            sys.join("server@default.ini"),
            "[Server]\nPort = 8080\nHostname = 127.0.0.1\nModelsMax = 1\n",
        )
        .expect("max 1");
        assert_eq!(
            pai_core::checks::startup_loads(&text_of("models@default.ini")),
            3,
            "the fixture starts over the max of one"
        );
        assert_eq!(
            m.set_model_profile(&desktop, "default", "a", "off")
                .map(|_| ()),
            Ok(()),
            "a step down lands: three loads to two, and two is still over one"
        );
        assert_eq!(
            keys_of(&text_of("assignments.ini"), "default", "a"),
            vec!["a = off".to_string()],
            "and its key landed"
        );
        assert_eq!(
            pai_core::checks::startup_loads(&text_of("models@default.ini")),
            2,
            "the render followed the record"
        );
    }

    {
        // `none` removes a key, so it lowers the count too: it lands over the
        // max, exactly as `off` does.
        let three = real_model_files(
            dir.path(),
            "[a]\nmodel = /m/a.gguf\nload-on-startup = true\n\n\
                     [b]\nmodel = /m/b.gguf\nload-on-startup = true\n\n\
                     [c]\nmodel = /m/c.gguf\nload-on-startup = true\n",
        );
        let m = core(true, FakePasswords::new(), FakeActionLog::new());
        write_record(&sys, &["default", "lan-2"]);
        // The cap is off while the fixture's pair is written, so the pair on
        // disk is the standing render (IP3c): the cap drops after it.
        std::fs::write(
            sys.join("server@default.ini"),
            "[Server]\nPort = 8080\nHostname = 127.0.0.1\n",
        )
        .expect("no cap");
        std::fs::write(
            sys.join("server@lan-2.ini"),
            "[Server]\nPort = 8081\nHostname = 127.0.0.1\n",
        )
        .expect("lan-2 profile");
        std::fs::write(sys.join("models.ini"), &three).expect("three loads");
        std::fs::write(sys.join("models.ini.disabled"), "").expect("no off file");
        std::fs::write(
            sys.join("assignments.ini"),
            "[default]\na = on\nb = on\nc = on\n",
        )
        .expect("three on");
        assert!(
            m.render_failures().is_empty(),
            "the fixture renders every instance under a cap of four"
        );
        // The cap drops LAST, over a pair the pass has already written (IP3c):
        // the render on disk is then the standing news the step rule must not
        // refuse, and the fit check's growth rule reports it as no news.
        std::fs::write(
            sys.join("server@default.ini"),
            "[Server]\nPort = 8080\nHostname = 127.0.0.1\nModelsMax = 1\n",
        )
        .expect("max 1");
        assert_eq!(
            m.set_model_profile(&desktop, "default", "b", "none")
                .map(|_| ()),
            Ok(()),
            "a key removed lands over the max: three loads to two, over one"
        );
        assert!(
            keys_of(&text_of("assignments.ini"), "default", "b").is_empty(),
            "the key is out of the record"
        );
        assert_eq!(
            pai_core::checks::startup_loads(&text_of("models@default.ini")),
            2,
            "and out of the render"
        );
    }

    {
        // `*` narrowed from `inherit` to `none` leaves the whole pool out of
        // the render: it lands whatever the max.
        let three = real_model_files(
            dir.path(),
            "[a]\nmodel = /m/a.gguf\nload-on-startup = true\n\n\
                     [b]\nmodel = /m/b.gguf\nload-on-startup = true\n\n\
                     [c]\nmodel = /m/c.gguf\nload-on-startup = true\n",
        );
        let m = core(true, FakePasswords::new(), FakeActionLog::new());
        write_record(&sys, &["default", "lan-2"]);
        // The cap is off while the fixture's pair is written, so the pair on
        // disk is the standing render (IP3c): the cap drops after it.
        std::fs::write(
            sys.join("server@default.ini"),
            "[Server]\nPort = 8080\nHostname = 127.0.0.1\n",
        )
        .expect("no cap");
        std::fs::write(
            sys.join("server@lan-2.ini"),
            "[Server]\nPort = 8081\nHostname = 127.0.0.1\n",
        )
        .expect("lan-2 profile");
        std::fs::write(sys.join("models.ini"), &three).expect("three loads");
        std::fs::write(sys.join("models.ini.disabled"), "").expect("no off file");
        std::fs::write(sys.join("assignments.ini"), "[default]\n* = inherit\n").expect("inherit");
        assert!(
            m.render_failures().is_empty(),
            "the fixture renders every instance under a cap of four"
        );
        // The cap drops LAST, over a pair the pass has already written (IP3c):
        // the render on disk is then the standing news the step rule must not
        // refuse, and the fit check's growth rule reports it as no news.
        std::fs::write(
            sys.join("server@default.ini"),
            "[Server]\nPort = 8080\nHostname = 127.0.0.1\nModelsMax = 1\n",
        )
        .expect("max 1");
        assert_eq!(
            pai_core::checks::startup_loads(&text_of("models@default.ini")),
            3,
            "`* = inherit` puts all three in effect, over the max of one"
        );
        assert_eq!(
            m.set_model_profile(&desktop, "default", "*", "none")
                .map(|_| ()),
            Ok(()),
            "the `*` narrowed lands over the max"
        );
        assert!(
            keys_of(&text_of("assignments.ini"), "default", "*").is_empty(),
            "the global key is out"
        );
        assert_eq!(
            pai_core::checks::startup_loads(&text_of("models@default.ini")),
            0,
            "and nothing of the pool is in effect"
        );
    }

    {
        // Assigning `on` a model profile that does NOT load on startup leaves
        // the count where it is, over the max: it lands, the count is what the
        // rule reads, not the key.
        let three = real_model_files(
            dir.path(),
            "[a]\nmodel = /m/a.gguf\nload-on-startup = true\n\n\
                     [b]\nmodel = /m/b.gguf\nload-on-startup = true\n\n\
                     [c]\nmodel = /m/c.gguf\nload-on-startup = true\n\n\
                     [d]\nmodel = /m/d.gguf\n",
        );
        let m = core(true, FakePasswords::new(), FakeActionLog::new());
        write_record(&sys, &["default", "lan-2"]);
        // The cap is off while the fixture's pair is written, so the pair on
        // disk is the standing render (IP3c): the cap drops after it.
        std::fs::write(
            sys.join("server@default.ini"),
            "[Server]\nPort = 8080\nHostname = 127.0.0.1\n",
        )
        .expect("no cap");
        std::fs::write(
            sys.join("server@lan-2.ini"),
            "[Server]\nPort = 8081\nHostname = 127.0.0.1\n",
        )
        .expect("lan-2 profile");
        std::fs::write(sys.join("models.ini"), &three).expect("three loads");
        std::fs::write(sys.join("models.ini.disabled"), "").expect("no off file");
        std::fs::write(
            sys.join("assignments.ini"),
            "[default]\na = on\nb = on\nc = on\n",
        )
        .expect("three on");
        assert!(
            m.render_failures().is_empty(),
            "the fixture renders every instance under a cap of four"
        );
        // The cap drops LAST, over a pair the pass has already written (IP3c):
        // the render on disk is then the standing news the step rule must not
        // refuse, and the fit check's growth rule reports it as no news.
        std::fs::write(
            sys.join("server@default.ini"),
            "[Server]\nPort = 8080\nHostname = 127.0.0.1\nModelsMax = 1\n",
        )
        .expect("max 1");
        assert_eq!(
            m.set_model_profile(&desktop, "default", "d", "on")
                .map(|_| ()),
            Ok(()),
            "`on` of a profile that never loads on startup does not raise the count"
        );
        assert_eq!(
            keys_of(&text_of("assignments.ini"), "default", "d"),
            vec!["d = on".to_string()],
            "and its key landed"
        );
        assert_eq!(
            pai_core::checks::startup_loads(&text_of("models@default.ini")),
            3,
            "the count is where it was, still over the max of one"
        );
    }

    {
        // The other side of the same fixture: `on` of a model profile that DOES
        // load on startup raises the count over the cap, so the pair of that
        // instance is the one the pass refuses (IP3c).
        let four = real_model_files(
            dir.path(),
            "[a]\nmodel = /m/a.gguf\nload-on-startup = true\n\n\
                    [b]\nmodel = /m/b.gguf\nload-on-startup = true\n\n\
                    [c]\nmodel = /m/c.gguf\nload-on-startup = true\n\n\
                    [e]\nmodel = /m/e.gguf\nload-on-startup = true\n",
        );
        let m = core(true, FakePasswords::new(), FakeActionLog::new());
        write_record(&sys, &["default", "lan-2"]);
        // The cap is off while the fixture's pair is written, so the pair on
        // disk is the standing render (IP3c): the cap drops after it.
        std::fs::write(
            sys.join("server@default.ini"),
            "[Server]\nPort = 8080\nHostname = 127.0.0.1\n",
        )
        .expect("no cap");
        std::fs::write(
            sys.join("server@lan-2.ini"),
            "[Server]\nPort = 8081\nHostname = 127.0.0.1\n",
        )
        .expect("lan-2 profile");
        std::fs::write(sys.join("models.ini"), &four).expect("four loads");
        std::fs::write(sys.join("models.ini.disabled"), "").expect("no off file");
        std::fs::write(
            sys.join("assignments.ini"),
            "[default]\na = on\nb = on\nc = on\n",
        )
        .expect("three on");
        assert!(
            m.render_failures().is_empty(),
            "the fixture renders every instance under a cap of four"
        );
        // The cap drops LAST, over a pair the pass has already written (IP3c):
        // the render on disk is then the standing news the step rule must not
        // refuse, and the fit check's growth rule reports it as no news.
        std::fs::write(
            sys.join("server@default.ini"),
            "[Server]\nPort = 8080\nHostname = 127.0.0.1\nModelsMax = 1\n",
        )
        .expect("max 1");
        let renders: Vec<(String, String)> = ["models@default.ini", "models@lan-2.ini"]
            .iter()
            .map(|name| (name.to_string(), text_of(name)))
            .collect();
        let over = m.set_model_profile(&desktop, "default", "e", "on");
        assert_eq!(
            over.as_ref().map(|_| ()),
            Ok(()),
            "the step that raises the count over the max lands: the count is a fit \
             check of default (IP3c), answered {over:?}"
        );
        assert_eq!(
            over.expect("the answer").not_converged,
            vec![NotConverged {
                instance: "default".into(),
                key: "load-on-startup-over-max".into(),
                args: vec!["4".into(), "1".into(), "default".into()],
            }],
            "and default is reported over its max"
        );
        assert_eq!(
            keys_of(&text_of("assignments.ini"), "default", "e"),
            vec!["e = on".to_string()],
            "the assignment landed in the record"
        );
        for (name, text) in renders {
            assert_eq!(
                text_of(&name),
                text,
                "and every instance keeps the pair it had: {name}"
            );
        }
    }

    {
        // An instance whose section names no server profile has no file to read
        // a `ModelsMax` from, and the launch refuses it anyway: its render is
        // not judged on the llama.cpp default cap. Five loads land.
        let five = real_model_files(
            dir.path(),
            "[a]\nmodel = /m/a.gguf\nload-on-startup = true\n\n\
                    [b]\nmodel = /m/b.gguf\nload-on-startup = true\n\n\
                    [c]\nmodel = /m/c.gguf\nload-on-startup = true\n\n\
                    [d]\nmodel = /m/d.gguf\nload-on-startup = true\n\n\
                    [e]\nmodel = /m/e.gguf\nload-on-startup = true\n",
        );
        let m = core(true, FakePasswords::new(), FakeActionLog::new());
        write_record(&sys, &["lan-2"]);
        std::fs::write(
            sys.join("server@lan-2.ini"),
            "[Server]\nPort = 8081\nHostname = 127.0.0.1\n",
        )
        .expect("lan-2 profile");
        std::fs::write(sys.join(INSTANCES_FILE), "[default]\n").expect("no profile named");
        std::fs::write(sys.join("models.ini"), &five).expect("five loads");
        std::fs::write(sys.join("models.ini.disabled"), "").expect("no off file");
        std::fs::write(
            sys.join("assignments.ini"),
            "[default]\na = on\nb = on\nc = on\nd = on\n",
        )
        .expect("four on");
        assert!(
            m.render_failures().is_empty(),
            "a section naming no profile still renders"
        );
        assert_eq!(
            m.set_model_profile(&desktop, "default", "e", "on")
                .map(|_| ()),
            Ok(()),
            "the fifth load on startup lands: no profile read, no cap judged"
        );
        assert_eq!(
            keys_of(&text_of("assignments.ini"), "default", "e"),
            vec!["e = on".to_string()],
            "and its key landed"
        );
        assert_eq!(
            pai_core::checks::startup_loads(&text_of("models@default.ini")),
            5,
            "the render holds five, over the llama.cpp default of four"
        );
    }

    {
        // The section names a profile whose file is missing: nothing to read a
        // cap from, so the render is not judged. Five loads land.
        let five = real_model_files(
            dir.path(),
            "[a]\nmodel = /m/a.gguf\nload-on-startup = true\n\n\
                    [b]\nmodel = /m/b.gguf\nload-on-startup = true\n\n\
                    [c]\nmodel = /m/c.gguf\nload-on-startup = true\n\n\
                    [d]\nmodel = /m/d.gguf\nload-on-startup = true\n\n\
                    [e]\nmodel = /m/e.gguf\nload-on-startup = true\n",
        );
        let m = core(true, FakePasswords::new(), FakeActionLog::new());
        write_record(&sys, &["lan-2"]);
        std::fs::write(
            sys.join("server@lan-2.ini"),
            "[Server]\nPort = 8081\nHostname = 127.0.0.1\n",
        )
        .expect("lan-2 profile");
        std::fs::write(
            sys.join(INSTANCES_FILE),
            "[default]\nServerProfile = gone\n",
        )
        .expect("profile named");
        std::fs::remove_file(sys.join("server@gone.ini")).ok();
        std::fs::write(sys.join("models.ini"), &five).expect("five loads");
        std::fs::write(sys.join("models.ini.disabled"), "").expect("no off file");
        std::fs::write(
            sys.join("assignments.ini"),
            "[default]\na = on\nb = on\nc = on\nd = on\n",
        )
        .expect("four on");
        assert!(
            m.render_failures().is_empty(),
            "a section naming a missing profile still renders"
        );
        assert_eq!(
            m.set_model_profile(&desktop, "default", "e", "on")
                .map(|_| ()),
            Ok(()),
            "the fifth load on startup lands: the profile the section names is not there"
        );
        assert_eq!(
            keys_of(&text_of("assignments.ini"), "default", "e"),
            vec!["e = on".to_string()],
            "and its key landed"
        );
    }

    {
        // The section names a profile whose file cannot be read (no text): the
        // same, an unreadable file is not an empty one and holds no cap.
        let five = real_model_files(
            dir.path(),
            "[a]\nmodel = /m/a.gguf\nload-on-startup = true\n\n\
                    [b]\nmodel = /m/b.gguf\nload-on-startup = true\n\n\
                    [c]\nmodel = /m/c.gguf\nload-on-startup = true\n\n\
                    [d]\nmodel = /m/d.gguf\nload-on-startup = true\n\n\
                    [e]\nmodel = /m/e.gguf\nload-on-startup = true\n",
        );
        let m = core(true, FakePasswords::new(), FakeActionLog::new());
        write_record(&sys, &["lan-2"]);
        std::fs::write(
            sys.join("server@lan-2.ini"),
            "[Server]\nPort = 8081\nHostname = 127.0.0.1\n",
        )
        .expect("lan-2 profile");
        std::fs::write(
            sys.join(INSTANCES_FILE),
            "[default]\nServerProfile = broken\n",
        )
        .expect("profile named");
        std::fs::write(
            sys.join("server@broken.ini"),
            b"[Server]\nPort = 8090\n\xff\xfe\n",
        )
        .expect("unreadable profile");
        std::fs::write(sys.join("models.ini"), &five).expect("five loads");
        std::fs::write(sys.join("models.ini.disabled"), "").expect("no off file");
        std::fs::write(
            sys.join("assignments.ini"),
            "[default]\na = on\nb = on\nc = on\nd = on\n",
        )
        .expect("four on");
        assert!(
            m.render_failures().is_empty(),
            "a section naming an unreadable profile still renders"
        );
        assert_eq!(
            m.set_model_profile(&desktop, "default", "e", "on")
                .map(|_| ()),
            Ok(()),
            "the fifth load on startup lands: the profile the section names is unreadable"
        );
        assert_eq!(
            keys_of(&text_of("assignments.ini"), "default", "e"),
            vec!["e = on".to_string()],
            "and its key landed"
        );
    }

    {
        // The model profile's name is exact: llama.cpp knows a pool section in
        // its own spelling, so the pool's `[Qwen]` answers no call for `qwen`,
        // and the call leaves the record, and the renders, as they were.
        let qwen = real_model_files(
            dir.path(),
            "[Qwen]\nmodel = /m/qwen.gguf\nload-on-startup = true\n",
        );
        let m = core(true, FakePasswords::new(), FakeActionLog::new());
        write_record(&sys, &["default", "lan-2"]);
        std::fs::write(
            sys.join("server@default.ini"),
            "[Server]\nPort = 8080\nHostname = 127.0.0.1\n",
        )
        .expect("default profile");
        std::fs::write(
            sys.join("server@lan-2.ini"),
            "[Server]\nPort = 8081\nHostname = 127.0.0.1\n",
        )
        .expect("lan-2 profile");
        std::fs::write(sys.join("models.ini"), &qwen).expect("pool with Qwen");
        std::fs::write(sys.join("models.ini.disabled"), "").expect("no off file");
        std::fs::write(sys.join("assignments.ini"), "[default]\nQwen = off\n").expect("seed");
        assert!(
            m.render_failures().is_empty(),
            "the fixture renders every instance"
        );
        let seed = text_of("assignments.ini");
        let renders: Vec<(String, String)> = ["models@default.ini", "models@lan-2.ini"]
            .iter()
            .map(|name| (name.to_string(), text_of(name)))
            .collect();
        let answer = m
            .set_model_profile(&desktop, "default", "qwen", "on")
            .map(|_| ());
        assert!(
            matches!(
                answer,
                Err(ManagerError::Invalid(ref r))
                    if r.key == "no-model-profile-named" && r.values == ["qwen"]
            ),
            "the id is looked up exactly as it was spelled, answered {answer:?}"
        );
        assert_eq!(
            keys_of(&text_of("assignments.ini"), "default", "Qwen"),
            vec!["Qwen = off".to_string()],
            "the line in the pool's own spelling is untouched"
        );
        assert_eq!(
            text_of("assignments.ini"),
            seed,
            "and the call for the name the pool does not hold wrote nothing"
        );
        for (name, text) in renders {
            assert_eq!(text_of(&name), text, "and rendered nothing: {name}");
        }
    }

    {
        // The verb writes one key of one section: the blank line that separates
        // that section from the next one is not its to write.
        let seeded = "[default]\nb = off\n\n[lan-2]\n* = inherit\n";
        let m = core(true, FakePasswords::new(), FakeActionLog::new());
        write_record(&sys, &["default", "lan-2"]);
        std::fs::write(
            sys.join("server@default.ini"),
            "[Server]\nPort = 8080\nHostname = 127.0.0.1\n",
        )
        .expect("default profile");
        std::fs::write(
            sys.join("server@lan-2.ini"),
            "[Server]\nPort = 8081\nHostname = 127.0.0.1\n",
        )
        .expect("lan-2 profile");
        std::fs::write(sys.join("models.ini"), &pool).expect("pool");
        std::fs::write(sys.join("models.ini.disabled"), &off_pool).expect("off pool");
        std::fs::write(sys.join("assignments.ini"), seeded).expect("seed");
        assert_eq!(
            m.set_model_profile(&desktop, "default", "a", "on")
                .map(|_| ()),
            Ok(()),
            "a lands in default's section"
        );
        assert_eq!(
            text_of("assignments.ini"),
            "[default]\nb = off\na = on\n\n[lan-2]\n* = inherit\n",
            "the blank line between the sections stayed"
        );
    }

    {
        // A CRLF record stays CRLF on every line the verb writes.
        let seeded = "[default]\r\nb = off\r\n\r\n[lan-2]\r\n* = inherit\r\n";
        let m = core(true, FakePasswords::new(), FakeActionLog::new());
        write_record(&sys, &["default", "lan-2"]);
        std::fs::write(
            sys.join("server@default.ini"),
            "[Server]\nPort = 8080\nHostname = 127.0.0.1\n",
        )
        .expect("default profile");
        std::fs::write(
            sys.join("server@lan-2.ini"),
            "[Server]\nPort = 8081\nHostname = 127.0.0.1\n",
        )
        .expect("lan-2 profile");
        std::fs::write(sys.join("models.ini"), &pool).expect("pool");
        std::fs::write(sys.join("models.ini.disabled"), &off_pool).expect("off pool");
        std::fs::write(sys.join("assignments.ini"), seeded).expect("CRLF seed");
        assert_eq!(
            m.set_model_profile(&desktop, "default", "a", "on")
                .map(|_| ()),
            Ok(()),
            "a lands in the CRLF record"
        );
        let written = text_of("assignments.ini");
        assert!(
            written.contains("a = on\r\n"),
            "the key line the verb wrote is CRLF like the file: {written:?}"
        );
        let bytes = written.as_bytes();
        assert!(
            bytes
                .iter()
                .enumerate()
                .all(|(i, b)| *b != b'\n' || i > 0 && bytes[i - 1] == b'\r'),
            "no line of the record the verb wrote ends with a bare LF: {written:?}"
        );
        assert!(
            written.contains("[lan-2]\r\n* = inherit\r\n"),
            "the other section keeps its CRLF bytes: {written:?}"
        );
    }

    {
        // A record whose last section has no trailing newline keeps it that
        // way: the verb adds the key, not a terminator the file did not have.
        let seeded = "[default]\r\n* = inherit\r\n\r\n[lan-2]\r\nb = off";
        let m = core(true, FakePasswords::new(), FakeActionLog::new());
        write_record(&sys, &["default", "lan-2"]);
        std::fs::write(
            sys.join("server@default.ini"),
            "[Server]\nPort = 8080\nHostname = 127.0.0.1\n",
        )
        .expect("default profile");
        std::fs::write(
            sys.join("server@lan-2.ini"),
            "[Server]\nPort = 8081\nHostname = 127.0.0.1\n",
        )
        .expect("lan-2 profile");
        std::fs::write(sys.join("models.ini"), &pool).expect("pool");
        std::fs::write(sys.join("models.ini.disabled"), &off_pool).expect("off pool");
        std::fs::write(sys.join("assignments.ini"), seeded).expect("no final newline");
        assert_eq!(
            m.set_model_profile(&desktop, "lan-2", "a", "on")
                .map(|_| ()),
            Ok(()),
            "a lands in lan-2's section, the last one"
        );
        assert_eq!(
            text_of("assignments.ini"),
            "[default]\r\n* = inherit\r\n\r\n[lan-2]\r\nb = off\r\na = on",
            "the file still ends on an unterminated line, and keeps the blank line"
        );
    }

    {
        // An instance the record has no section for: the verb adds one, and the
        // sections it did have keep their bytes.
        let seeded = "[default]\n* = inherit\n";
        let m = core(true, FakePasswords::new(), FakeActionLog::new());
        write_record(&sys, &["default", "lan-2"]);
        std::fs::write(
            sys.join("server@default.ini"),
            "[Server]\nPort = 8080\nHostname = 127.0.0.1\n",
        )
        .expect("default profile");
        std::fs::write(
            sys.join("server@lan-2.ini"),
            "[Server]\nPort = 8081\nHostname = 127.0.0.1\n",
        )
        .expect("lan-2 profile");
        std::fs::write(sys.join("models.ini"), &pool).expect("pool");
        std::fs::write(sys.join("models.ini.disabled"), &off_pool).expect("off pool");
        std::fs::write(sys.join("assignments.ini"), seeded).expect("seed");
        assert_eq!(
            m.set_model_profile(&desktop, "lan-2", "a", "on")
                .map(|_| ()),
            Ok(()),
            "lan-2 has no section of its own yet"
        );
        let written = text_of("assignments.ini");
        assert_eq!(
            section(&written, "lan-2"),
            "[lan-2]\na = on\n",
            "the section the record did not have is added: {written:?}"
        );
        assert_eq!(
            section(&written, "default"),
            "[default]\n* = inherit\n",
            "and default's section is the bytes it was: {written:?}"
        );
        assert!(
            text_of("models@lan-2.ini").contains("[a]"),
            "and the render of the instance follows the section that just appeared"
        );
    }

    {
        // The text a user reads for a value outside the list, byte for byte, in
        // both strings files: the CLI's and the GUI's.
        const BAD_VALUE: &str = "{0} is not a value here: a model profile takes none, inherit, on or off, and * takes none or inherit";
        for (file, source) in [
            ("cli.toml", include_str!("../../../core/strings/cli.toml")),
            (
                "en_US.toml",
                include_str!("../../../core/strings/en_US.toml"),
            ),
        ] {
            let all = crate::strings::parse(source);
            assert_eq!(
                all["model-profile-bad-value"], BAD_VALUE,
                "{file}: the text the refusal shows"
            );
        }
    }

    #[cfg(target_os = "linux")]
    {
        // The mode of the record the verb writes: the manager's config files
        // are read by every client, so a write lands them 0644 whatever the
        // process umask was, even over a file that was 0600. The umask is
        // process-wide, so it is masked only around the call and restored
        // before the first assert.
        use std::os::unix::fs::PermissionsExt;
        write_record(&sys, &["default", "lan-2"]);
        std::fs::write(
            sys.join("server@default.ini"),
            "[Server]\nPort = 8080\nHostname = 127.0.0.1\n",
        )
        .expect("server@default.ini");
        std::fs::write(
            sys.join("server@lan-2.ini"),
            "[Server]\nPort = 8081\nHostname = 127.0.0.1\n",
        )
        .expect("server@lan-2.ini");
        std::fs::write(sys.join("models.ini"), &pool).expect("models.ini");
        std::fs::write(sys.join("models.ini.disabled"), &off_pool).expect("models.ini.disabled");
        std::fs::write(sys.join("assignments.ini"), seeded).expect("seed");
        std::fs::set_permissions(
            sys.join("assignments.ini"),
            std::fs::Permissions::from_mode(0o600),
        )
        .expect("0600 assignments");
        let m = core(true, FakePasswords::new(), FakeActionLog::new());
        let before = listing();
        let guard = UmaskGuard::mask(nix::sys::stat::Mode::from_bits_retain(0o077));
        let answer = m
            .set_model_profile(&desktop, "default", "a", "on")
            .map(|_| ());
        let old_mask = guard.0;
        drop(guard);
        assert_eq!(
            nix::sys::stat::umask(old_mask),
            old_mask,
            "the window restored the umask"
        );
        let mode = std::fs::metadata(sys.join("assignments.ini"))
            .ok()
            .map(|f| f.permissions().mode());
        let after = listing();
        assert_eq!(
            answer,
            Ok(()),
            "the fixture: a model profile of the pool, on, lands"
        );
        assert_eq!(
            mode.map(|f| f & 0o777),
            Some(0o644),
            "the record was 0600 and the process was masked 0o077: the written assignments.ini is 0644"
        );
        assert_eq!(
            after, before,
            "and the write left no temp file of its own beside it"
        );
    }
}

/// E2E phase: the model-file check (PLAN.md step 10 item 13, slice 9;
/// ARCHITECTURE.md (b) 3, "Which writes run them", item 10 of the slices
/// notes, and the key's name under "Renamed error keys"). A `model` path must
/// be a regular file on the node, every shard of a split model included; a
/// relative `model` is resolved against the working directory of the instance
/// (`launch::instance_current_dir`, the folder the engine starts in) and the
/// refusal names the resolved path, the one the check looked at; a model
/// profile with `hf-repo`, `model-url` or `docker-repo` is not checked, the
/// engine filling that file from the hub or fetching it itself; readability by
/// the instance's account is not checked at all. The manager refuses a file
/// where a write puts the model profile in
/// effect on an instance: a new assignment or override in effect on, the global
/// switch on, and a model profile save on every instance where it is in effect.
/// The refusal is `model-profile-model-missing` with `[model profile, instance,
/// path]`, the first file of the value that is missing (a shard named whole,
/// the first missing one), the instances answered in id order. A switch off, an
/// assignment removed and a `*` narrowed never fail it. A refusal writes
/// nothing and renders nothing.
///
/// Every fixture here leaves `ModelsMax` out of the server profiles, so the
/// load-on-startup rule has no cap to judge: `set_model_profile` and
/// `check_renders` skip an instance with none. Each refusal below can therefore
/// only be the model file's.
#[cfg(test)]
pub(crate) fn manager_model_file_check_e2e() {
    use fakes::{
        FakeActionLog, FakeAuthority, FakeGroup, FakePasswords, FakeResolver, FakeService,
    };

    let dir = tempfile::tempdir().expect("tempdir");
    crate::paths::redirect_for_tests(dir.path());
    let sys = crate::paths::engine_config_dir();
    std::fs::create_dir_all(&sys).expect("engine config dir");
    let models = dir.path().join("models");
    std::fs::create_dir_all(&models).expect("models dir");

    // The node's model files: `path` is the text a `model` key holds, `make`
    // puts one there, `rm` takes it away.
    let path = |name: &str| models.join(name).to_string_lossy().into_owned();
    let make = |name: &str| {
        std::fs::write(models.join(name), "gguf").expect("model file");
    };
    let rm = |name: &str| {
        let _ = std::fs::remove_file(models.join(name));
    };

    let bff = Caller(":1.9".into());
    let desktop = Caller(":1.42".into());
    let manager = || {
        Manager::new(
            FakeService::new(),
            FakeAuthority::new(true).quietly(true).bff(&bff),
            FakeResolver::lan(),
        )
        .with_group(FakeGroup::new())
        .with_passwords(FakePasswords::new())
        .with_log(FakeActionLog::new())
    };

    let read_opt = |name: &str| std::fs::read_to_string(sys.join(name)).ok();
    let text_of = |name: &str| std::fs::read_to_string(sys.join(name)).expect(name);
    let section =
        |text: &str, id: &str| pai_core::ini::text::section_text(text, id).unwrap_or_default();
    let keys_of = |text: &str, id: &str, key: &str| -> Vec<String> {
        section(text, id)
            .lines()
            .filter(|line| line.trim_start().starts_with(&format!("{key} ")))
            .map(|line| line.to_string())
            .collect()
    };
    let listing = || -> Vec<String> {
        let mut names: Vec<String> = std::fs::read_dir(&sys)
            .map(|rd| {
                rd.flatten()
                    .map(|e| e.file_name().to_string_lossy().into_owned())
                    .collect()
            })
            .unwrap_or_default();
        names.sort();
        names
    };
    // The picture a refusal must leave exactly as it found it: the bytes of the
    // record, of the two pool files and of the renders, a render that is not
    // there included, and the folder's own listing, which is what a file
    // created out of nothing changes.
    let listing_text = || format!("{}\n", listing().join("\n"));
    let seen = |name: &str| -> String {
        if name == "listing" {
            listing_text()
        } else {
            read_opt(name).unwrap_or_default()
        }
    };
    let snapshot = |names: &[&str]| -> Vec<(String, String)> {
        names
            .iter()
            .map(|name| name.to_string())
            .chain(std::iter::once("listing".to_string()))
            .map(|name| (name.clone(), seen(&name)))
            .collect()
    };
    let kept = |before: &[(String, String)]| {
        for (name, text) in before {
            assert_eq!(&seen(name), text, "the refusal wrote nothing: {name}");
        }
    };
    let pool_files = |on: &str, off: &str| {
        std::fs::write(sys.join("models.ini"), on).expect("models.ini");
        std::fs::write(sys.join("models.ini.disabled"), off).expect("models.ini.disabled");
    };
    let assignments = |text: &str| {
        std::fs::write(sys.join("assignments.ini"), text).expect("assignments.ini");
    };
    // The refusal this phase is about, its three args whole.
    let named = |answer: &Result<(), ManagerError>, profile: &str, instance: &str, file: &str| {
        matches!(
            answer,
            Err(ManagerError::Invalid(r))
                if r.key == "model-profile-model-missing"
                    && r.values == [profile.to_string(), instance.to_string(), file.to_string()]
        )
    };
    // The same refusal, its path compared as a path: a relative `model` is
    // looked for resolved against the instance's folder, so the string the
    // refusal names is a join, compared by its components, whichever separator
    // and spelling of the folder it carries.
    let named_at = |answer: &Result<(), ManagerError>,
                    profile: &str,
                    instance: &str,
                    want: &std::path::Path| {
        let same = |a: &str| {
            std::path::absolute(std::path::Path::new(a)).ok() == std::path::absolute(want).ok()
        };
        matches!(
            answer,
            Err(ManagerError::Invalid(r))
                if r.key == "model-profile-model-missing"
                    && r.values.len() == 3
                    && r.values[0] == profile
                    && r.values[1] == instance
                    && same(&r.values[2])
        )
    };

    write_record(&sys, &["default", "lan-2"]);
    std::fs::write(
        sys.join("server@default.ini"),
        "[Server]\nPort = 8080\nHostname = 127.0.0.1\n",
    )
    .expect("server@default.ini");
    std::fs::write(
        sys.join("server@lan-2.ini"),
        "[Server]\nPort = 8081\nHostname = 127.0.0.1\n",
    )
    .expect("server@lan-2.ini");
    const WATCH: [&str; 5] = [
        "assignments.ini",
        "models.ini",
        "models.ini.disabled",
        "models@default.ini",
        "models@lan-2.ini",
    ];
    let m = manager();

    // ── An assignment in effect on: the file has to be there ──────────────
    {
        make("ok.gguf");
        make("offok.gguf");
        // The switched-on pool holds `ok` (its file there) and `gone` (its file
        // not); the switched-off file holds `offok` (there) and `offgone` (not).
        let pool = format!(
            "[ok]\nmodel = {}\n\n[gone]\nmodel = {}\n",
            path("ok.gguf"),
            path("gone.gguf")
        );
        let off_pool = format!(
            "[offok]\nmodel = {}\n\n[offgone]\nmodel = {}\n",
            path("offok.gguf"),
            path("offgone.gguf")
        );
        pool_files(&pool, &off_pool);
        let seed = "[default]\ngone = off\n\n[lan-2]\ngone = off\n";

        // All three land in one report, so each case answers for itself.
        let mut refused: Vec<String> = Vec::new();
        for (profile, value, instance, file) in [
            ("gone", "on", "default", path("gone.gguf")),
            ("offgone", "on", "default", path("offgone.gguf")),
            ("gone", "inherit", "default", path("gone.gguf")),
        ] {
            assignments(seed);
            let before = snapshot(&WATCH);
            let answer = m
                .set_model_profile(&desktop, instance, profile, value)
                .map(|_| ());
            if !named(&answer, profile, instance, &file) {
                refused.push(format!(
                    "`{profile} = {value}` on {instance} puts a model profile with no file in effect, answered {answer:?}"
                ));
            }
            kept(&before);
        }
        assert!(
            refused.is_empty(),
            "a new assignment in effect on a model profile with no file is refused:\n{}",
            refused.join("\n")
        );

        // Never refused: `off` and `none` (the model profile leaves the render),
        // `inherit` of one the pool has switched off, and `on` of one whose
        // file is there.
        for (profile, value) in [
            ("gone", "off"),
            ("offgone", "inherit"),
            ("ok", "on"),
            ("offok", "on"),
        ] {
            assignments(seed);
            let answer = m
                .set_model_profile(&desktop, "default", profile, value)
                .map(|_| ());
            assert_eq!(
                answer,
                Ok(()),
                "`{profile} = {value}` is not in effect with a missing file, or its file is there"
            );
            assert!(
                text_of("assignments.ini").contains(&format!("{profile} = {value}")),
                "and its key landed: {profile} = {value}"
            );
        }

        // `none` takes the key of a model profile with no file away, and a `*`
        // narrowed leaves every such model profile out: neither fails.
        assignments(seed);
        assert_eq!(
            m.set_model_profile(&desktop, "default", "gone", "none")
                .map(|_| ()),
            Ok(()),
            "an assignment removed is never refused"
        );
        assert!(
            keys_of(&text_of("assignments.ini"), "default", "gone").is_empty(),
            "the key is out"
        );
        assignments("[default]\n* = inherit\n\n[lan-2]\n* = inherit\n");
        assert_eq!(
            m.set_model_profile(&desktop, "default", "*", "none")
                .map(|_| ()),
            Ok(()),
            "a `*` narrowed is never refused, whatever the pool holds"
        );
        assert!(
            keys_of(&text_of("assignments.ini"), "default", "*").is_empty(),
            "the global key is out"
        );
    }

    // ── The file has to be a regular one: a directory, a FIFO, a symlink ──
    {
        let base = format!("[ok]\nmodel = {}\n", path("ok.gguf"));
        std::fs::create_dir(models.join("dir.gguf")).expect("a directory named .gguf");
        pool_files(&base, "");
        assignments("[default]\n* = inherit\n\n[lan-2]\n* = inherit\n");

        // A directory is not a model file.
        let with = format!("{base}\n[dir]\nmodel = {}\n", path("dir.gguf"));
        let before = snapshot(&WATCH);
        let answer = m.write_file(&desktop, "models.ini", &with, false);
        assert!(
            named(&answer, "dir", "default", &path("dir.gguf")),
            "a directory in `model` is refused as a file that is not there: {answer:?}"
        );
        kept(&before);

        // A FIFO is not a model file either: reading it would block, so it is
        // refused as a file that is not there.
        #[cfg(target_os = "linux")]
        {
            let fifo = models.join("fifo.gguf");
            nix::sys::stat::mknod(
                &fifo,
                nix::sys::stat::SFlag::S_IFIFO,
                nix::sys::stat::Mode::from_bits_truncate(0o644),
                0,
            )
            .expect("mkfifo");
            let with = format!("{base}\n[pipes]\nmodel = {}\n", path("fifo.gguf"));
            let before = snapshot(&WATCH);
            let answer = m.write_file(&desktop, "models.ini", &with, false);
            assert!(
                named(&answer, "pipes", "default", &path("fifo.gguf")),
                "a FIFO in `model` is refused as a file that is not there: {answer:?}"
            );
            kept(&before);
        }

        // A symlink to a regular file is that file: the save lands.
        #[cfg(unix)]
        {
            std::os::unix::fs::symlink(models.join("ok.gguf"), models.join("link.gguf"))
                .expect("symlink");
            let with = format!("{base}\n[link]\nmodel = {}\n", path("link.gguf"));
            assert_eq!(
                m.write_file(&desktop, "models.ini", &with, false),
                Ok(()),
                "a symlink to a regular file is the file"
            );
            assert_eq!(text_of("models.ini"), with, "and the save landed");
        }
    }

    // ── A split model: every shard, the first missing one named ───────────
    {
        make("big-00001-of-00003.gguf");
        make("big-00003-of-00003.gguf");
        let base = format!("[ok]\nmodel = {}\n", path("ok.gguf"));
        let with = format!(
            "{base}\n[big]\nmodel = {}\n",
            path("big-00001-of-00003.gguf")
        );
        pool_files(&base, "");

        // Shard 2 is not there: the refusal names it, the instance where the
        // model profile is in effect.
        assignments("[default]\n* = inherit\n\n[lan-2]\nbig = off\n");
        let before = snapshot(&WATCH);
        let answer = m.write_file(&desktop, "models.ini", &with, false);
        assert!(
            named(&answer, "big", "default", &path("big-00002-of-00003.gguf")),
            "the shard that is missing is named whole: {answer:?}"
        );
        kept(&before);

        // Shard 3 missing and 2 there: the refusal names 3.
        make("big-00002-of-00003.gguf");
        rm("big-00003-of-00003.gguf");
        let before = snapshot(&WATCH);
        let answer = m.write_file(&desktop, "models.ini", &with, false);
        assert!(
            named(&answer, "big", "default", &path("big-00003-of-00003.gguf")),
            "the first shard that is missing, wherever it falls: {answer:?}"
        );
        kept(&before);

        // Every shard there: the save lands.
        make("big-00003-of-00003.gguf");
        assert_eq!(
            m.write_file(&desktop, "models.ini", &with, false),
            Ok(()),
            "a split model with every shard is a file that is there"
        );
        assert_eq!(text_of("models.ini"), with, "and the save landed");

        // The instance named is the one whose render holds the model profile,
        // and where more than one does, the first in id order. The save lands
        // with every shard above, so re-seed the pool: the case below must
        // really ADD the model profile whose shard is missing, not re-write
        // one the pool already holds.
        pool_files(&base, "");
        rm("big-00002-of-00003.gguf");
        assignments("[default]\nbig = off\n\n[lan-2]\n* = inherit\n");
        let before = snapshot(&WATCH);
        let answer = m.write_file(&desktop, "models.ini", &with, false);
        assert!(
            named(&answer, "big", "lan-2", &path("big-00002-of-00003.gguf")),
            "in effect on lan-2 alone: lan-2 is named: {answer:?}"
        );
        kept(&before);

        pool_files(&base, "");
        assignments("[default]\n* = inherit\n\n[lan-2]\n* = inherit\n");
        rm("big-00002-of-00003.gguf");
        let before = snapshot(&WATCH);
        let answer = m.write_file(&desktop, "models.ini", &with, false);
        assert!(
            named(&answer, "big", "default", &path("big-00002-of-00003.gguf")),
            "in effect on both: the first instance in id order answers: {answer:?}"
        );
        kept(&before);
        make("big-00002-of-00003.gguf");
    }

    // ── `hf-repo`, `model-url`, `docker-repo`: that file is not the node's ─
    {
        // The engine fills `model` from the hub (`--hf-repo`) or replaces it
        // with a file it fetches itself (`--model-url`, `--docker-repo`:
        // llama.cpp `common/arg.cpp` 427-441 and 480-535 in v1.0.0). The file
        // the model profile names is therefore not on the node at save time,
        // so a model profile carrying any of the three keys is not checked.
        let base = format!("[base]\nmodel = {}\n", path("ok.gguf"));
        pool_files(&base, "");
        assignments("[default]\n* = inherit\n\n[lan-2]\n* = inherit\n");
        let fetches: [(&str, &str, &str); 3] = [
            ("hf", "hf-repo", "theos/Big-8B-Q4"),
            ("url", "model-url", "https://example.test/Big-8B-Q4.gguf"),
            ("docker", "docker-repo", "ggmlorg/llama:server"),
        ];

        for (id, key, value) in fetches {
            // Its `model` file is there: the key adds no file to look for.
            let with = format!(
                "{base}\n[{id}]\nmodel = {}\n{key} = {value}\n",
                path("ok.gguf")
            );
            assert_eq!(
                m.write_file(&desktop, "models.ini", &with, false),
                Ok(()),
                "`{key}` with its `model` file there lands"
            );
            assert_eq!(text_of("models.ini"), with, "and the save landed: {key}");

            // Its `model` file is not there: the engine is the one putting it
            // there, so the check looks at nothing and the save lands.
            let with = format!(
                "{base}\n[{id}]\nmodel = {}\n{key} = {value}\n",
                path(&format!("{id}-gone.gguf"))
            );
            assert_eq!(
                m.write_file(&desktop, "models.ini", &with, false),
                Ok(()),
                "`{key}` with a `model` that is not there is not checked: the engine fetches that file"
            );
            assert_eq!(text_of("models.ini"), with, "and the save landed: {key}");
        }

        // The skip is the keyed model profile's own: another one in the same
        // save, with none of the three keys, is checked as always.
        let with = format!(
            "{base}\n[hf]\nmodel = {}\nhf-repo = theos/Big-8B-Q4\n\n[plain]\nmodel = {}\n",
            path("hf-gone.gguf"),
            path("plain-gone.gguf")
        );
        let before = snapshot(&WATCH);
        let answer = m.write_file(&desktop, "models.ini", &with, false);
        assert!(
            named(&answer, "plain", "default", &path("plain-gone.gguf")),
            "`[hf]` is skipped, `[plain]` beside it is refused: {answer:?}"
        );
        kept(&before);

        // A model profile with `hf-repo` and no `model` at all names the file
        // the engine downloads, so it names no file of the node: the `model`
        // rule accepts the fetch key and the save lands.
        let with = format!("{base}\n[repo]\nhf-repo = theos/Big-8B-Q4\n");
        let answer = m.write_file(&desktop, "models.ini", &with, false);
        assert_eq!(
            answer,
            Ok(()),
            "`hf-repo` alone is a model the node downloads, refused by no check"
        );
        assert_eq!(text_of("models.ini"), with, "and the save landed");
    }

    // ── A model profile save: only where it is in effect ──────────────────
    {
        let on_disk = format!("[ok]\nmodel = {}\n", path("ok.gguf"));
        let save = format!("{on_disk}\n[new]\nmodel = {}\n", path("new.gguf"));
        pool_files(&on_disk, "");

        // In effect on no instance: every instance has it `off`, so the save
        // lands with the file missing.
        assignments("[default]\nnew = off\n\n[lan-2]\nnew = off\n");
        assert_eq!(
            m.write_file(&desktop, "models.ini", &save, false),
            Ok(()),
            "a save whose only new model profile is in effect nowhere lands"
        );
        assert_eq!(text_of("models.ini"), save, "and landed");

        // In effect on one instance: that instance is named. The save above
        // landed, so re-seed the pool: the case below must really ADD the
        // model profile whose file is missing.
        pool_files(&on_disk, "");
        assignments("[default]\nnew = off\n\n[lan-2]\n* = inherit\n");
        let before = snapshot(&WATCH);
        let answer = m.write_file(&desktop, "models.ini", &save, false);
        assert!(
            named(&answer, "new", "lan-2", &path("new.gguf")),
            "the save puts the model profile in effect on lan-2: {answer:?}"
        );
        kept(&before);

        // The switch off, in the one file the model data model leaves: the pool
        // write that leaves the model profile with no instance naming it
        // (`ARCHITECTURE.md`, "Instance profiles"). Never refused by a resource
        // check, not even while the assignment keeps it on. The state a real
        // switch off starts from, seeded directly: the model profile in the
        // pool, its file not there, in effect on `default`.
        pool_files(&save, "");
        assignments("[default]\nnew = on\n");
        assert_eq!(
            m.write_file(
                &desktop,
                "models.ini.disabled",
                &format!("[new]\nmodel = {}\n", path("new.gguf")),
                false,
            ),
            Err(ManagerError::BadFileName),
            "the switched-off file is a client's read, never a write"
        );
        assignments("[default]\nnew = inherit\n");
        assert_eq!(
            m.write_file(&desktop, "models.ini", &on_disk, false),
            Ok(()),
            "the switch off, a model profile out of the pool with its file missing, \
             never fails a resource check: the pool write lands"
        );
        assert_eq!(text_of("models.ini"), on_disk, "and landed");
    }

    // ── The global switch on ──────────────────────────────────────────────
    {
        let on_disk = format!("[ok]\nmodel = {}\n", path("ok.gguf"));
        let off = format!("[sw]\nmodel = {}\n", path("sw.gguf"));
        let on = format!("{on_disk}\n[sw]\nmodel = {}\n", path("sw.gguf"));
        pool_files(&on_disk, &off);

        // The model profile sits switched off with its file missing, assigned to
        // both instances with no override of the switch. Switching it on, the
        // write into the switched-on file, is refused on the first of them.
        assignments("[default]\nsw = inherit\n\n[lan-2]\nsw = inherit\n");
        let before = snapshot(&WATCH);
        let answer = m.write_file(&desktop, "models.ini", &on, false);
        assert!(
            named(&answer, "sw", "default", &path("sw.gguf")),
            "the global switch on, in effect on every instance that does not say off: {answer:?}"
        );
        kept(&before);

        // The same switch where every instance says off lands, and with it the
        // second write of the switch, the model profile out of the switched-off
        // file.
        assignments("[default]\nsw = off\n\n[lan-2]\nsw = off\n");
        assert_eq!(
            m.write_file(&desktop, "models.ini", &on, false),
            Ok(()),
            "the switch on lands where every instance has the model profile off"
        );
        // The switch's second half is retired with the switched-off file
        // (`ARCHITECTURE.md`, "Instance profiles"): the model profile stays in
        // `models.ini`, and no instance profile naming it is what switches it
        // off.
        assert_eq!(
            m.write_file(&desktop, "models.ini.disabled", "", false),
            Err(ManagerError::BadFileName),
            "the switched-off file is a read, never a write"
        );
    }

    // ── A relative `model`: the instance's own folder is where it lives ───
    {
        // The engine resolves a relative `model` against the working directory
        // of the process it launches, `launch::instance_current_dir` (the
        // engine's `state` directory on Windows, the data root elsewhere: the
        // folder the router and its children inherit). The check resolves the
        // value there, and the path it names is the one it looked at, the
        // resolved one, not the value as written.
        let cwd = crate::launch::instance_current_dir(
            cfg!(windows),
            &crate::paths::data_root(),
            &crate::paths::instance_state_dir(),
        );
        std::fs::create_dir_all(cwd.join("Qwen")).expect("the instance's folder");
        let on_disk = format!("[ok]\nmodel = {}\n", path("ok.gguf"));
        pool_files(&on_disk, "");
        assignments("[default]\n* = inherit\n\n[lan-2]\n* = inherit\n");

        // The file is in that folder: the save lands.
        std::fs::write(cwd.join("Qwen/Big-8B-Q4.gguf"), "gguf").expect("model file");
        let rel = format!("{on_disk}\n[rel]\nmodel = Qwen/Big-8B-Q4.gguf\n");
        assert_eq!(
            m.write_file(&desktop, "models.ini", &rel, false),
            Ok(()),
            "a relative `model` whose file is in the instance's folder lands"
        );
        assert_eq!(text_of("models.ini"), rel, "and the save landed");

        // The file is not there: refused, naming the resolved path.
        let gone = format!("{on_disk}\n[rel]\nmodel = Qwen/Gone-8B-Q4.gguf\n");
        let before = snapshot(&WATCH);
        let answer = m.write_file(&desktop, "models.ini", &gone, false);
        assert!(
            named_at(&answer, "rel", "default", &cwd.join("Qwen/Gone-8B-Q4.gguf")),
            "a relative `model` missing from the instance's folder is refused, naming the path the check looked at: {answer:?}"
        );
        kept(&before);
    }

    // ── An instance whose server profile is not there ─────────────────────
    {
        // `ghost` runs a server profile whose file is missing, so its launch
        // refuses it and the load-on-startup rule skips it (no cap to read).
        // The model-file check has no cap, so it runs on `ghost`'s render too.
        std::fs::write(
            sys.join(INSTANCES_FILE),
            "[default]\nServerProfile = default\n\n[ghost]\nServerProfile = ghost\n",
        )
        .expect("instances.ini: ghost has no profile file");
        let on_disk = format!("[ok]\nmodel = {}\n", path("ok.gguf"));
        let save = format!("{on_disk}\n[lost]\nmodel = {}\n", path("lost.gguf"));
        pool_files(&on_disk, "");
        let watch: Vec<&str> = WATCH
            .iter()
            .copied()
            .chain(std::iter::once("models@ghost.ini"))
            .collect();

        assignments("[default]\nlost = off\n\n[ghost]\n* = inherit\n");
        let before = snapshot(&watch);
        let answer = m.write_file(&desktop, "models.ini", &save, false);
        assert!(
            named(&answer, "lost", "ghost", &path("lost.gguf")),
            "in effect on the instance with no profile file: that instance answers: {answer:?}"
        );
        kept(&before);

        pool_files(&save, "");
        assignments("[default]\nlost = off\n\n[ghost]\nlost = off\n");
        let before = snapshot(&watch);
        let answer = m
            .set_model_profile(&desktop, "ghost", "lost", "on")
            .map(|_| ());
        assert!(
            named(&answer, "lost", "ghost", &path("lost.gguf")),
            "an assignment on an instance with no profile file is refused as any other: {answer:?}"
        );
        kept(&before);

        write_record(&sys, &["default", "lan-2"]);
    }

    // ── A broken profile in effect: the check refuses only what the write adds ──
    {
        // ARCHITECTURE.md (b) "Which writes run them": "A switch off, an
        // assignment removed, a `*` narrowed never fail a resource check", so a
        // check refuses what the write ADDS and never a file a model profile
        // already in effect lost earlier. Here `b` sits in effect on `default`
        // and its file is deleted after it was assigned, so every write that
        // touches only another model profile must land, and the two that add a
        // missing file answer naming the model profile they add, never `b`.
        let section = |id: &str, file: &str, extra: &str| {
            format!("[{id}]\nmodel = {}\n{extra}\n", path(file))
        };
        let b_sec = section("b", "b.gguf", "");
        let a_sec = section("a", "a.gguf", "ctx-size = 4096");
        let c_sec = section("c", "c.gguf", "");
        let d_sec = section("d", "d.gguf", "");
        let e_sec = section("e", "e.gguf", "");
        let pool = format!("{b_sec}\n{a_sec}\n{c_sec}\n{d_sec}\n{e_sec}");
        // `b`, `a` are in effect on `default` by their own key, `c` through
        // `* = inherit`; `d` and `e` are off. `lan-2` runs `a` alone.
        let seeded =
            "[default]\n* = inherit\na = on\nb = on\nd = off\ne = off\n\n[lan-2]\na = on\n";
        // The picture before each write: `b`'s file gone (deleted after the
        // assignment landed), every other file of the pool there.
        let seed = || {
            make("a.gguf");
            make("c.gguf");
            make("d.gguf");
            rm("b.gguf");
            rm("e.gguf");
            pool_files(&pool, "");
            assignments(seeded);
        };
        // The snapshot that is not there, told by name, so a case that wrote
        // something reports it and the next case still runs.
        let changed = |before: &[(String, String)]| -> Option<String> {
            before
                .iter()
                .find(|(name, text)| seen(name) != *text)
                .map(|(name, _)| name.clone())
        };
        let mut fails: Vec<String> = Vec::new();

        // (a) An assignment removed, and an override switched off, on a model
        // profile that is not the broken one.
        // S9B-PIN-1 BEGIN
        {
            seed();
            let answer = m
                .set_model_profile(&desktop, "default", "a", "none")
                .map(|_| ());
            if answer != Ok(()) {
                fails.push(format!(
                    "(a) `a = none` while `b` is broken in effect lands: {answer:?}"
                ));
            } else if !keys_of(&text_of("assignments.ini"), "default", "a").is_empty() {
                fails.push("(a) and the key `a` is out of the section".to_string());
            }
        }
        // S9B-PIN-1 END
        // S9B-PIN-2 BEGIN
        {
            seed();
            let answer = m
                .set_model_profile(&desktop, "default", "a", "off")
                .map(|_| ());
            if answer != Ok(()) {
                fails.push(format!(
                    "(a) `a = off` while `b` is broken in effect lands: {answer:?}"
                ));
            } else if !text_of("assignments.ini").contains("a = off") {
                fails.push("(a) and the key `a = off` landed".to_string());
            }
        }
        // S9B-PIN-2 END

        // (b) The global switch narrowed: `*` out, which takes model profiles
        // out of the render and adds none.
        // S9B-PIN-3 BEGIN
        {
            seed();
            let answer = m
                .set_model_profile(&desktop, "default", "*", "none")
                .map(|_| ());
            if answer != Ok(()) {
                fails.push(format!(
                    "(b) `* = none` while `b` is broken in effect lands: {answer:?}"
                ));
            } else if !keys_of(&text_of("assignments.ini"), "default", "*").is_empty() {
                fails.push("(b) and the global key is out".to_string());
            }
        }
        // S9B-PIN-3 END

        // (c) A save of `models.ini` that edits one non-model key of another
        // model profile, and (d) the switch off of a third one: the two writes
        // of the switch, the model profile out of the switched-on file and into
        // the switched-off one.
        // S9B-PIN-4 BEGIN
        {
            seed();
            let edit = pool.replace("ctx-size = 4096", "ctx-size = 8192");
            let answer = m.write_file(&desktop, "models.ini", &edit, false);
            if answer != Ok(()) {
                fails.push(format!(
                    "(c) a save of `models.ini` that edits only `a`'s ctx-size lands: {answer:?}"
                ));
            } else if text_of("models.ini") != edit {
                fails.push("(c) and the save landed".to_string());
            }
        }
        // S9B-PIN-4 END
        // S9B-PIN-5 BEGIN
        {
            seed();
            let no_c = format!("{b_sec}\n{a_sec}\n{d_sec}\n{e_sec}");
            let answer = m.write_file(&desktop, "models.ini", &no_c, false);
            if answer != Ok(()) {
                fails.push(format!(
                    "(d) the switch off of `c`: `c` out of `models.ini` lands: {answer:?}"
                ));
            } else {
                // The other half of the switch is retired with the switched-off
                // file: the profile stays in `models.ini`, unassigned.
                let off = format!("[c]\nmodel = {}\n", path("c.gguf"));
                let second = m.write_file(&desktop, "models.ini.disabled", &off, false);
                if second != Err(ManagerError::BadFileName) {
                    fails.push(format!(
                        "(d) the switch off of `c`: the write of `models.ini.disabled` is the name refusal: {second:?}"
                    ));
                }
            }
        }
        // S9B-PIN-5 END

        // (e) The switch on of a model profile whose file IS there, beside the
        // broken one.
        // S9B-PIN-6 BEGIN
        {
            seed();
            let answer = m
                .set_model_profile(&desktop, "default", "d", "on")
                .map(|_| ());
            if answer != Ok(()) {
                fails.push(format!(
                    "(e) `d = on`, its file there, while `b` is broken lands: {answer:?}"
                ));
            } else if !text_of("assignments.ini").contains("d = on") {
                fails.push("(e) and the key `d = on` landed".to_string());
            }
        }
        // S9B-PIN-6 END

        // (f) and (g) stay refused: the write that ADDS a model profile with no
        // file, and the save that points a model profile at a file that is not
        // there. Each names the model profile the write touches, not `b`, and
        // writes nothing.
        // S9B-PIN-7 BEGIN
        {
            seed();
            let before = snapshot(&WATCH);
            let answer = m
                .set_model_profile(&desktop, "default", "e", "on")
                .map(|_| ());
            if !named(&answer, "e", "default", &path("e.gguf")) {
                fails.push(format!(
                    "(f) `e = on` refused naming `e` while `b` is broken: {answer:?}"
                ));
            }
            if let Some(what) = changed(&before) {
                fails.push(format!("(f) the refusal wrote nothing: {what} changed"));
            }
        }
        // S9B-PIN-7 END
        // S9B-PIN-8 BEGIN
        {
            seed();
            let before = snapshot(&WATCH);
            let a_gone = pool.replace(
                &format!("model = {}\nctx-size", path("a.gguf")),
                &format!("model = {}\nctx-size", path("a-gone.gguf")),
            );
            let answer = m.write_file(&desktop, "models.ini", &a_gone, false);
            if !named(&answer, "a", "default", &path("a-gone.gguf")) {
                fails.push(format!(
                    "(g) a save pointing `a` at a missing file refused naming `a`: {answer:?}"
                ));
            }
            if let Some(what) = changed(&before) {
                fails.push(format!("(g) the refusal wrote nothing: {what} changed"));
            }
        }
        // S9B-PIN-8 END

        assert!(
            fails.is_empty(),
            "a resource check refuses only what the write adds, never a model profile already \
             in effect with a missing file:\n{}",
            fails.join("\n")
        );
    }

    // ── A model profile in effect with no file: the pool is checked ────────
    //
    // `assign::render` takes the pool and the instance's section, so a model
    // profile the instance has `on` sits in that instance's render. A save of
    // the pool changes that render, so it is judged by the model-file check
    // exactly as every other pool save is: it refuses what the write ADDS, on
    // every render the write moves, and nothing else.
    // S9C-PIN-1 BEGIN
    {
        // The pool holds `p` and `q`, every file there. `default` has `p = on`,
        // `lan-2` has `p = off`, and no instance says anything about `q`.
        let sec = |id: &str, file: &str, ctx: &str| {
            format!("[{id}]\nmodel = {}\nctx-size = {ctx}\n", path(file))
        };
        let off = format!(
            "{}\n{}",
            sec("p", "p.gguf", "4096"),
            sec("q", "q.gguf", "1024")
        );
        let seed = || {
            make("ok.gguf");
            make("p.gguf");
            make("q.gguf");
            // The pool is the one file the phase writes; the switched-off file
            // is a client's read (`ARCHITECTURE.md`, "Instance profiles").
            pool_files(&off, "");
            assignments("[default]\np = on\n\n[lan-2]\np = off\n");
        };

        seed();
        // The save points `p`, which `default` has `on`, at a file that is not
        // there: `default`'s render gains a model file the node does not have,
        // so the save is refused naming `p`, `default` and that path, and
        // writes nothing.
        let save = off.replace(
            &format!("model = {}\nctx-size", path("p.gguf")),
            &format!("model = {}\nctx-size", path("p-gone.gguf")),
        );
        let before = snapshot(&WATCH);
        let answer = m.write_file(&desktop, "models.ini", &save, false);
        assert!(
            named(&answer, "p", "default", &path("p-gone.gguf")),
            "a save of the pool that puts a model profile with no file in effect on an instance that has it `on` is refused: {answer:?}"
        );
        kept(&before);
    }
    // S9C-PIN-1 END
    // S9C-PIN-2 BEGIN
    {
        let sec = |id: &str, file: &str, ctx: &str| {
            format!("[{id}]\nmodel = {}\nctx-size = {ctx}\n", path(file))
        };
        let off = format!(
            "{}\n{}",
            sec("p", "p.gguf", "4096"),
            sec("q", "q.gguf", "1024")
        );
        let seed = || {
            make("ok.gguf");
            make("p.gguf");
            make("q.gguf");
            // The pool is the one file the phase writes; the switched-off file
            // is a client's read (`ARCHITECTURE.md`, "Instance profiles").
            pool_files(&off, "");
            assignments("[default]\np = on\n\n[lan-2]\np = off\n");
        };

        seed();
        // The same save for `q`: no instance has it `on`, so it is in no
        // render, its missing file is in effect nowhere, and the save lands.
        let save = off.replace(
            &format!("model = {}\nctx-size", path("q.gguf")),
            &format!("model = {}\nctx-size", path("q-gone.gguf")),
        );
        let answer = m.write_file(&desktop, "models.ini", &save, false);
        assert_eq!(
            answer,
            Ok(()),
            "a model profile no instance has `on` is in no render, so a missing file is not in effect anywhere: {answer:?}"
        );
        assert_eq!(text_of("models.ini"), save, "and the save landed");
    }
    // S9C-PIN-2 END
    // S9C-PIN-3 BEGIN
    {
        let sec = |id: &str, file: &str, ctx: &str| {
            format!("[{id}]\nmodel = {}\nctx-size = {ctx}\n", path(file))
        };
        let off = format!(
            "{}\n{}",
            sec("p", "p.gguf", "4096"),
            sec("q", "q.gguf", "1024")
        );
        let seed = || {
            make("ok.gguf");
            make("p.gguf");
            make("q.gguf");
            // The pool is the one file the phase writes; the switched-off file
            // is a client's read (`ARCHITECTURE.md`, "Instance profiles").
            pool_files(&off, "");
            assignments("[default]\np = on\n\n[lan-2]\np = off\n");
        };

        // `p` sits in effect on `default` with its file gone, taken away after
        // the assignment landed. The check refuses only what a write adds, so a
        // save of the switched-off file that touches another key lands: first
        // a key of `p` itself, then a key of `q` beside the broken one.
        seed();
        rm("p.gguf");
        let edit_p = off.replace("ctx-size = 4096", "ctx-size = 8192");
        let answer = m.write_file(&desktop, "models.ini", &edit_p, false);
        assert_eq!(
            answer,
            Ok(()),
            "a save of the pool that edits only `ctx-size` of a model profile already in effect with a missing file lands: {answer:?}"
        );
        assert_eq!(text_of("models.ini"), edit_p, "and the save landed");

        let edit_q = edit_p.replace("ctx-size = 1024", "ctx-size = 2048");
        let second = m.write_file(&desktop, "models.ini", &edit_q, false);
        assert_eq!(
            second,
            Ok(()),
            "a save of the pool that edits only another model profile, beside one in effect with a missing file, lands: {second:?}"
        );
        assert_eq!(text_of("models.ini"), edit_q, "and the save landed");
    }
    // S9C-PIN-3 END
}

/// A GGUF v3 header for the memory fixtures: a `llama` model of two layers
/// trained on `ctx_len` tokens, with the KV layout the reader of
/// `pai_core::gguf_shape` knows (the head counts written below), and one
/// tensor of `bytes` per `(name, bytes)`, laid out as llama.cpp does (each
/// aligned to 32). A copy of `pai-core`'s `gguf_shape::tests::gguf` and
/// `write_sparse`: a test module imports no other test module, and the bytes
/// both check are the same.
#[cfg(test)]
fn write_gguf(path: &std::path::Path, ctx_len: u32, tensors: &[(&str, u64)]) {
    let put_str = |out: &mut Vec<u8>, s: &str| {
        out.extend_from_slice(&(s.len() as u64).to_le_bytes());
        out.extend_from_slice(s.as_bytes());
    };
    let put_u32 = |out: &mut Vec<u8>, x: u32| out.extend_from_slice(&x.to_le_bytes());
    let put_u64 = |out: &mut Vec<u8>, x: u64| out.extend_from_slice(&x.to_le_bytes());
    let mut out = b"GGUF".to_vec();
    put_u32(&mut out, 3);
    put_u64(&mut out, tensors.len() as u64);
    put_u64(&mut out, 8);
    for (k, v) in [
        ("general.architecture", None),
        ("llama.context_length", Some(ctx_len)),
        ("llama.block_count", Some(2)),
        ("llama.embedding_length", Some(2048)),
        ("llama.attention.head_count", Some(32)),
        ("llama.attention.head_count_kv", Some(4)),
        ("llama.attention.key_length", Some(128)),
        ("llama.attention.value_length", Some(128)),
    ] {
        put_str(&mut out, k);
        match v {
            None => {
                put_u32(&mut out, 8);
                put_str(&mut out, "llama");
            }
            Some(x) => {
                put_u32(&mut out, 4);
                put_u32(&mut out, x);
            }
        }
    }
    let mut off = 0u64;
    for (name, bytes) in tensors {
        put_str(&mut out, name);
        put_u32(&mut out, 2);
        put_u64(&mut out, 4096);
        put_u64(&mut out, bytes / 4096);
        put_u32(&mut out, 12);
        put_u64(&mut out, off);
        off += bytes.div_ceil(32) * 32;
    }
    let data_start = (out.len() as u64).div_ceil(32) * 32;
    std::fs::write(path, out).expect("gguf header");
    std::fs::OpenOptions::new()
        .write(true)
        .open(path)
        .expect("open gguf")
        .set_len(data_start + off)
        .expect("sparse gguf");
}

/// E2E phase: the model profile memory check (PLAN.md step 10 item 13, slice
/// 10b-2; ARCHITECTURE.md (b) 1, 2, 3). The manager measures a write of the
/// model profile pool against the node's GPU memory with the pure estimate of
/// `pai_core::gguf_shape` (slice 10b-1), over every instance's render in id
/// order, and refuses with `model-profile-over-memory` and
/// `[model profile, instance, needed MiB rounded up, available MiB rounded
/// down]` when ONE model profile is over the node's usable memory AND this
/// write makes that profile larger than it stands in today's render: the need
/// named is that one profile's estimate, never a sum over the render (slice
/// 10b). The assignment verb `set_model_profile` measures the render its
/// record leaves in effect, and the GPU seam is asked only once a section has
/// grown. The estimate reads four keys,
/// `model`, `ctx-size`, `cache-type-k` and `cache-type-v`, with `[*]` filling
/// a key a section has none of, the trained context standing in for a
/// `ctx-size` that is absent or 0, and f16 standing in for a cache type that
/// is absent. A model profile with `hf-repo`, `model-url` or `docker-repo`,
/// one with no `model`, and one whose `model` is not a GGUF the reader knows,
/// are judged on nothing; a node the GPU seam answers no pool for, an empty
/// pool included, is measured on nothing. The refusal writes nothing and
/// renders nothing.
///
/// The pool of every fixture is a two-layer `llama` (`write_gguf`) whose
/// weights and KV terms are both far over the small card, and the cards are
/// sized from the estimate read off that file with the same pure API the
/// manager uses, so no figure here restates the arithmetic of 10b-1.
#[cfg(test)]
pub(crate) fn manager_memory_check_e2e() {
    use fakes::{
        FakeActionLog, FakeAuthority, FakeGpus, FakeGroup, FakePasswords, FakeResolver, FakeService,
    };
    use pai_core::gguf_shape::{full_gpu_bytes, local_shape, GPU_RESERVE, MIB};

    let dir = tempfile::tempdir().expect("tempdir");
    crate::paths::redirect_for_tests(dir.path());
    let sys = crate::paths::engine_config_dir();
    std::fs::create_dir_all(&sys).expect("engine config dir");
    let models = dir.path().join("models");
    std::fs::create_dir_all(&models).expect("models dir");

    let model = models.join("qwen.gguf");
    write_gguf(
        &model,
        262_144,
        &[
            ("token_embd.weight", 200 << 20),
            ("blk.0.attn_q.weight", 300 << 20),
            ("blk.1.attn_q.weight", 300 << 20),
            ("output.weight", 200 << 20),
        ],
    );
    std::fs::write(models.join("notes.txt"), "not a model").expect("notes.txt");
    let path = model.to_string_lossy().into_owned();
    let txt = models.join("notes.txt").to_string_lossy().into_owned();

    // What the model needs, read from the file with the pure estimate: the
    // phase sizes its cards and its refusals with these, never with figures it
    // worked out by hand.
    let shape = local_shape(&model).expect("fixture shape");
    let trained = shape.trained_ctx;
    let need = |ctx: u64, k: &str, v: &str| full_gpu_bytes(&shape, ctx, k, v);
    let mib_up = |bytes: u64| (bytes.div_ceil(MIB)).to_string();
    let mib_down = |bytes: u64| (bytes / MIB).to_string();
    // Every card carries the reserve the estimate keeps free (`GPU_RESERVE`),
    // so its usable memory is what the test adds to the total.
    let reserve = GPU_RESERVE / MIB;
    let small = need(1024, "f16", "f16") / MIB + reserve + 32;
    let big = need(trained, "f16", "f16") / MIB + reserve + 32;
    // A card sitting between the two contexts the growth case moves between:
    // it fits the model at 131072 tokens and not at the trained one.
    let mid = need(131_072, "f16", "f16") / MIB + reserve + 32;
    let usable = |total_mib: u64| {
        pai_core::devices::pool_usable(&[pai_core::devices::Gpu {
            id: "CUDA0".to_string(),
            name: "NVIDIA GeForce CUDA0".to_string(),
            total_mib,
        }])
    };

    let bff = Caller(":1.9".into());
    let desktop = Caller(":1.42".into());
    let manager_on = |gpus: FakeGpus| {
        Manager::new(
            FakeService::new(),
            FakeAuthority::new(true).quietly(true).bff(&bff),
            FakeResolver::lan(),
        )
        .with_group(FakeGroup::new())
        .with_passwords(FakePasswords::new())
        .with_log(FakeActionLog::new())
        .with_gpus(gpus)
    };

    let read_opt = |name: &str| std::fs::read_to_string(sys.join(name)).ok();
    let text_of = |name: &str| std::fs::read_to_string(sys.join(name)).expect(name);
    let listing = || -> Vec<String> {
        let mut names: Vec<String> = std::fs::read_dir(&sys)
            .map(|rd| {
                rd.flatten()
                    .map(|e| e.file_name().to_string_lossy().into_owned())
                    .collect()
            })
            .unwrap_or_default();
        names.sort();
        names
    };
    let seen = |name: &str| -> String {
        if name == "listing" {
            format!("{}\n", listing().join("\n"))
        } else {
            read_opt(name).unwrap_or_default()
        }
    };
    let snapshot = |names: &[&str]| -> Vec<(String, String)> {
        names
            .iter()
            .map(|name| name.to_string())
            .chain(std::iter::once("listing".to_string()))
            .map(|name| (name.clone(), seen(&name)))
            .collect()
    };
    // What the check stops: the rendered pair of every instance (IP3c). The
    // file the write itself names LANDS, so the pool texts are not part of this.
    let kept = |before: &[(String, String)]| {
        for (name, text) in before {
            if !(name.starts_with("models@") || name.starts_with("launch@") || name == "listing") {
                continue;
            }
            assert_eq!(
                &seen(name),
                text,
                "the instance keeps the pair it had: {name}"
            );
        }
    };
    let pool_files = |on: &str, off: &str| {
        std::fs::write(sys.join("models.ini"), on).expect("models.ini");
        std::fs::write(sys.join("models.ini.disabled"), off).expect("models.ini.disabled");
    };
    let assignments = |text: &str| {
        std::fs::write(sys.join("assignments.ini"), text).expect("assignments.ini");
    };
    // The memory news this phase is about, its four args whole. The check is a
    // fit check of ONE instance (IP3c, `ARCHITECTURE.md` "Convergence"): the
    // write that brings the news LANDS, and the instance whose render it stops
    // is reported by that write's own render pass, with the check's own key and
    // the same four args. True when the answer is that.
    let over = |m: &Manager<FakeService, FakeAuthority, FakeResolver>,
                answer: &Result<(), ManagerError>,
                profile: &str,
                instance: &str,
                needed: u64,
                available: u64| {
        answer.is_ok()
            && m.not_converged_locked()
                .expect("the rows")
                .iter()
                .any(|row| {
                    row.instance == instance
                        && row.key == "model-profile-over-memory"
                        && row.args
                            == [
                                profile.to_string(),
                                instance.to_string(),
                                mib_up(needed),
                                mib_down(available),
                            ]
                })
    };

    write_record(&sys, &["default"]);
    std::fs::write(
        sys.join("server@default.ini"),
        "[Server]\nPort = 8080\nHostname = 127.0.0.1\n",
    )
    .expect("server@default.ini");
    const WATCH: [&str; 5] = [
        "assignments.ini",
        "models.ini",
        "models.ini.disabled",
        "models@default.ini",
        "models@a-lan.ini",
    ];

    // The pool texts, one profile of the fixture model.
    let on_text = format!("[qwen]\nmodel = {path}\nctx-size = {trained}\n");
    let fits_text = format!("[qwen]\nmodel = {path}\nctx-size = 1024\n");

    // The cards are sized from the estimate of the fixture, and the phase
    // depends on them sitting between the model's needs: pinned here, so a
    // refusal that does not come means the check, not a fixture that stopped
    // being over its card.
    assert_eq!(
        local_shape(&model).map(|s| s.kv_f16_per_token.is_some()),
        Ok(true),
        "the fixture's KV layout is one the reader knows: with no KV term every \
         context and cache-type case below would measure the same bytes"
    );
    assert!(
        need(1024, "f16", "f16") <= usable(small),
        "the small card fits the model at 1024 tokens"
    );
    assert!(
        need(trained, "f16", "f16") > usable(small),
        "the small card does not fit it at the context it was trained on"
    );
    assert!(
        need(trained, "f16", "f16") <= usable(big),
        "the big card fits it at the trained context, f16 on both sides"
    );
    assert!(
        need(131_072, "f16", "f16") <= usable(mid),
        "the mid card fits it at 131072 tokens"
    );
    assert!(
        need(trained, "f16", "f16") > usable(mid),
        "the mid card does not fit it at the trained context: the growth case is refused \
         on this card and lands below it"
    );
    assert!(
        need(trained, "f32", "f32") > usable(big),
        "the big card does not fit it with an f32 KV cache"
    );

    // ── Over the node: refused, on an instance that loads it ───────────────
    {
        pool_files("", "");
        assignments("[default]\nqwen = on\n");
        let m = manager_on(FakeGpus::new(&[("CUDA0", small)]));
        let before = snapshot(&WATCH);
        let answer = m.write_file(&desktop, "models.ini", &on_text, false);
        assert!(
            over(
                &m,
                &answer,
                "qwen",
                "default",
                need(trained, "f16", "f16"),
                usable(small)
            ),
            "a model profile that does not fit the node's GPU memory is refused, naming \
             itself, the instance, the MiB it needs and the MiB the node has: {answer:?}"
        );
        kept(&before);
    }

    // ── The same profile, sized to fit: it lands ──────────────────────────
    {
        pool_files("", "");
        assignments("[default]\nqwen = on\n");
        let m = manager_on(FakeGpus::new(&[("CUDA0", small)]));
        assert_eq!(
            m.write_file(&desktop, "models.ini", &fits_text, false),
            Ok(()),
            "the same model profile at 1024 tokens fits the card and lands"
        );
        assert_eq!(text_of("models.ini"), fits_text, "the save landed");
    }

    // ── The four keys the estimate reads ─────────────────────────────────
    {
        let m = manager_on(FakeGpus::new(&[("CUDA0", small)]));
        // No `ctx-size` at all: the trained context, so this is the same need
        // as the one refused above.
        let bare = format!("[qwen]\nmodel = {path}\n");
        let before = snapshot(&WATCH);
        let answer = m.write_file(&desktop, "models.ini", &bare, false);
        assert!(
            over(
                &m,
                &answer,
                "qwen",
                "default",
                need(trained, "f16", "f16"),
                usable(small)
            ),
            "a `ctx-size` that is absent is the context the model was trained on: {answer:?}"
        );
        kept(&before);

        // `ctx-size = 0` is the same: 0 is llama.cpp's "unset".
        let zero = format!("[qwen]\nmodel = {path}\nctx-size = 0\n");
        let before = snapshot(&WATCH);
        let answer = m.write_file(&desktop, "models.ini", &zero, false);
        assert!(
            over(
                &m,
                &answer,
                "qwen",
                "default",
                need(trained, "f16", "f16"),
                usable(small)
            ),
            "a `ctx-size` of 0 is the trained context too: {answer:?}"
        );
        kept(&before);

        // No cache type keys at all: f16 on both sides, the engine's default,
        // which is what the card sized for f16 fits.
        let m_big = manager_on(FakeGpus::new(&[("CUDA0", big)]));
        assert_eq!(
            m_big.write_file(&desktop, "models.ini", &on_text, false),
            Ok(()),
            "cache types absent are f16: on the card sized for f16 this lands, \
             and one bit wider on either side does not"
        );
    }

    // ── A cache type makes it grow: refused ──────────────────────────────
    {
        pool_files("", "");
        assignments("[default]\nqwen = on\n");
        let m = manager_on(FakeGpus::new(&[("CUDA0", big)]));
        let f32 = format!(
            "[qwen]\nmodel = {path}\nctx-size = {trained}\ncache-type-k = f32\ncache-type-v = f32\n"
        );
        let before = snapshot(&WATCH);
        let answer = m.write_file(&desktop, "models.ini", &f32, false);
        assert!(
            over(
                &m,
                &answer,
                "qwen",
                "default",
                need(trained, "f32", "f32"),
                usable(big)
            ),
            "an f32 KV cache on both sides doubles the KV term and does not fit: {answer:?}"
        );
        kept(&before);
    }

    // ── `[*]` fills a key the section has none of ────────────────────────
    //
    // A client cannot write the global section (a client's `[*]` is refused
    // `invalid-model-profile-id`, pinned in the assignments phase), so it
    // reaches the pool as a hand edit of `models.ini` and a save that keeps it
    // byte for byte. The cascade is pinned on the cache types: the card fits
    // the model at f16 and not at f32, so a section that writes no cache key
    // is measured on `[*]`'s f32.
    {
        // The global section, f32 on both sides, hand-edited into the
        // switched-on file, and the model profile that says nothing about the
        // cache sitting switched off: today's render holds no model profile,
        // so the need the write puts in effect is all this write's.
        let global = "[*]\ncache-type-k = f32\ncache-type-v = f32\n";
        pool_files(global, &on_text);
        assignments("[default]\nqwen = inherit\n");
        let m = manager_on(FakeGpus::new(&[("CUDA0", big)]));

        // The switch on: the section moves to the switched-on file, the
        // `[*]` of the file kept byte for byte as a save keeps it.
        let switch_on = format!("{global}{on_text}");
        let before = snapshot(&WATCH);
        let answer = m.write_file(&desktop, "models.ini", &switch_on, false);
        assert!(
            over(
                &m,
                &answer,
                "qwen",
                "default",
                need(trained, "f32", "f32"),
                usable(big)
            ),
            "the cache types of `[*]` are the need of a model profile that writes none, on \
             a card that fits it at f16 and not at f32: {answer:?}"
        );
        kept(&before);

        // The section's own keys win over `[*]`'s: the same model profile
        // f16 of its own lands on the card `[*]`'s f32 does not fit. The pool
        // is hand-edited to the switched-on state, so the save is a plain edit
        // of a model profile that is in effect, not a switch.
        pool_files(&format!("{global}{on_text}"), "");
        let own = format!(
            "[qwen]\nmodel = {path}\nctx-size = {trained}\ncache-type-k = f16\ncache-type-v = f16\n"
        );
        let save = format!("{global}{own}");
        assert_eq!(
            m.write_file(&desktop, "models.ini", &save, false),
            Ok(()),
            "a model profile's own cache types replace `[*]`'s, and fit the card: {save:?}"
        );
        assert_eq!(text_of("models.ini"), save, "the save landed");

        // The same rule, from the other side, as a refusal: `[*]` is f16 and
        // the section's own keys are f32, so the render is over the card. An
        // implementation that let `[*]` win over a key the section writes
        // lands this save.
        let f16 = "[*]\ncache-type-k = f16\ncache-type-v = f16\n";
        pool_files(&format!("{f16}{on_text}"), "");
        let own32 = own.replace("f16", "f32");
        let wider = format!("{f16}{own32}");
        let before = snapshot(&WATCH);
        let answer = m.write_file(&desktop, "models.ini", &wider, false);
        assert!(
            over(
                &m,
                &answer,
                "qwen",
                "default",
                need(trained, "f32", "f32"),
                usable(big)
            ),
            "the section's own f32 cache types, not `[*]`'s f16, are the need: {answer:?}"
        );
        kept(&before);
    }

    // ── Judged on nothing ────────────────────────────────────────────────
    {
        pool_files("", "");
        assignments("[default]\nqwen = on\nhf = on\nnotes = on\n");
        let m = manager_on(FakeGpus::new(&[("CUDA0", small)]));
        // The engine fills a `hf-repo` model itself: not judged.
        let fetch = "[hf]\nmodel = /models/never-fetched.gguf\nhf-repo = theos/Big-8B-Q4\n";
        assert_eq!(
            m.write_file(&desktop, "models.ini", fetch, false),
            Ok(()),
            "a model profile with `hf-repo` names no file of the node, and is not measured"
        );

        // A `model` that is not a GGUF has no shape to measure: not judged,
        // and it does not blind the check for the profile beside it.
        let notes = format!("[notes]\nmodel = {txt}\nctx-size = {trained}\n");
        assert_eq!(
            m.write_file(&desktop, "models.ini", &notes, false),
            Ok(()),
            "a `model` that is not a GGUF is measured on nothing and lands"
        );
        let both = format!("{notes}{on_text}");
        let before = snapshot(&WATCH);
        let answer = m.write_file(&desktop, "models.ini", &both, false);
        assert!(
            over(&m, &answer, "qwen", "default", need(trained, "f16", "f16"), usable(small)),
            "an unmeasurable model profile beside one that does not fit does not hide it: {answer:?}"
        );
        kept(&before);
    }

    // ── A switch off, and an assignment that loads nothing ───────────────
    {
        let m = manager_on(FakeGpus::new(&[("CUDA0", small)]));
        // The switched-off file is a client's read, never a write: a model
        // profile switched off sits in the pool with no instance profile
        // naming it (`ARCHITECTURE.md`, "Instance profiles").
        pool_files("", "");
        assignments("[default]\nqwen = off\n");
        assert_eq!(
            m.write_file(&desktop, "models.ini.disabled", &on_text, false),
            Err(ManagerError::BadFileName),
            "the switched-off file is not served for a write"
        );
        assert_eq!(
            text_of("models.ini.disabled"),
            "",
            "the refused write created nothing of its own"
        );

        // In the pool, assigned to nobody.
        pool_files(&on_text, "");
        assignments("[default]\nqwen = off\n");
        assert_eq!(
            m.write_file(&desktop, "models.ini", &on_text, false),
            Ok(()),
            "an over-memory model profile no instance loads is in no render and lands"
        );
    }

    // ── A model profile in effect on an instance is measured on its render
    //
    // `assign::render` takes the pool and the instance's section, so a model
    // profile the instance has `on` sits in that instance's render. A save that
    // puts an over-memory profile there moves that render, so the memory check
    // judges it (the model-file check of slice 9 judges the same way). The
    // switched-off file left the picture with the model data model, so this is
    // the pool alone.
    {
        pool_files("", "");
        assignments("[default]\nqwen = on\n");
        let m = manager_on(FakeGpus::new(&[("CUDA0", small)]));
        // The write puts the over-memory model profile in effect on `default`.
        let before = snapshot(&WATCH);
        let answer = m.write_file(&desktop, "models.ini", &on_text, false);
        assert!(
            over(
                &m,
                &answer,
                "qwen",
                "default",
                need(trained, "f16", "f16"),
                usable(small)
            ),
            "a save of the pool that puts an over-memory model profile in effect \
             on an instance that has it `on` is refused: {answer:?}"
        );
        kept(&before);

        // The same write, sized to fit the card, lands.
        assert_eq!(
            m.write_file(&desktop, "models.ini", &fits_text, false),
            Ok(()),
            "a model profile that fits the node lands"
        );
        assert_eq!(text_of("models.ini"), fits_text, "the save landed");
    }

    // ── The growth rule: a save that changes nothing of the need lands ────
    {
        pool_files(&on_text, "");
        assignments("[default]\nqwen = on\n");
        let m = manager_on(FakeGpus::new(&[("CUDA0", small)]));
        // The section on disk is already over; this save adds one key of it and
        // leaves the `model`, `ctx-size` and cache keys byte for byte.
        let edited = on_text.replace(
            &format!("ctx-size = {trained}\n"),
            &format!("ctx-size = {trained}\ntemperature = 0.5\n"),
        );
        assert_eq!(
            m.write_file(&desktop, "models.ini", &edited, false),
            Ok(()),
            "a save of another key of a model profile that is already over memory is not \
             refused: the need it leaves is today's"
        );
        assert_eq!(text_of("models.ini"), edited, "the save landed");

        // The same save again, every byte of the model profile as it stands on
        // disk: today's render has that very section, so the need it leaves is
        // today's and the write is not this profile's news. It lands, even
        // though the profile is over the card.
        let before = snapshot(&WATCH);
        let answer = m.write_file(&desktop, "models.ini", &edited, false);
        assert_eq!(
            answer,
            Ok(()),
            "a save that changes no byte of a model profile that is already over memory is \
             not a need this write makes: {answer:?}"
        );
        kept(&before);

        // A save that raises `ctx-size` of that same profile is refused: the
        // need grew. The card is the mid one, which fits today's context and
        // not the trained one, so only the wider context is over it.
        let m_mid = manager_on(FakeGpus::new(&[("CUDA0", mid)]));
        let grown = format!("[qwen]\nmodel = {path}\nctx-size = 131072\n");
        pool_files(&grown, "");
        assignments("[default]\nqwen = on\n");
        let wider = format!("[qwen]\nmodel = {path}\nctx-size = {trained}\n");
        let before = snapshot(&WATCH);
        let answer = m_mid.write_file(&desktop, "models.ini", &wider, false);
        assert!(
            over(
                &m_mid,
                &answer,
                "qwen",
                "default",
                need(trained, "f16", "f16"),
                usable(mid)
            ),
            "raising `ctx-size` past what the node has is reported, and the pair default \
             runs stays the one that fits: {answer:?}"
        );
        kept(&before);

        // The same profile at the context the mid card has room for, from an
        // empty pool: it lands.
        pool_files("", "");
        assert_eq!(
            m_mid.write_file(&desktop, "models.ini", &grown, false),
            Ok(()),
            "the context the mid card fits, put in effect by the same write, lands"
        );
        assert_eq!(text_of("models.ini"), grown, "the save landed");
    }

    // ── A node with nothing to measure against ───────────────────────────
    {
        pool_files("", "");
        assignments("[default]\nqwen = on\n");
        // The seam answers no pool (no card, a probe that failed).
        let m = manager_on(FakeGpus::none());
        assert_eq!(
            m.write_file(&desktop, "models.ini", &on_text, false),
            Ok(()),
            "a node the GPU seam answers no pool for is not measured"
        );
        assert_eq!(text_of("models.ini"), on_text, "the save landed");

        // The seam answers a pool with no card: the same.
        pool_files("", "");
        let m = manager_on(FakeGpus::new(&[]));
        assert_eq!(
            m.write_file(&desktop, "models.ini", &on_text, false),
            Ok(()),
            "a pool with no card is not measured either"
        );
        assert_eq!(text_of("models.ini"), on_text, "the save landed");
    }

    // ── Whom the refusal names ───────────────────────────────────────────
    {
        // Two instances, the profile in effect on both: the first in id order
        // answers.
        write_record(&sys, &["default", "a-lan"]);
        std::fs::write(
            sys.join("server@a-lan.ini"),
            "[Server]\nPort = 8081\nHostname = 127.0.0.1\n",
        )
        .expect("server@a-lan.ini");
        // The pool is empty on disk, so every model profile the write puts in
        // effect is a need this write brings, on both instances.
        pool_files("", "");
        assignments("[a-lan]\nqwen = on\n\n[default]\nqwen = on\n");
        let m = manager_on(FakeGpus::new(&[("CUDA0", small)]));
        let before = snapshot(&WATCH);
        let answer = m.write_file(&desktop, "models.ini", &on_text, false);
        assert!(
            over(
                &m,
                &answer,
                "qwen",
                "a-lan",
                need(trained, "f16", "f16"),
                usable(small)
            ),
            "the instances are answered in id order: `a-lan` comes first: {answer:?}"
        );
        kept(&before);

        // Two model profiles in one render, one over the card on its own:
        // the refusal names that one profile with its OWN need, never the
        // render's total (slice 10b: `needed` is one profile's estimate).
        // `beta` needs a key of its own to be in effect on `a-lan`
        // (`assign::render` renders a model profile with no key and no `*`
        // nowhere).
        assignments("[a-lan]\nqwen = on\nbeta = on\n\n[default]\nqwen = on\n");
        let two = format!("{on_text}[beta]\nmodel = {path}\nctx-size = 1024\n");
        let before = snapshot(&WATCH);
        let answer = m.write_file(&desktop, "models.ini", &two, false);
        // 10B-PIN-1 BEGIN
        assert!(
            over(
                &m,
                &answer,
                "qwen",
                "a-lan",
                need(trained, "f16", "f16"),
                usable(small)
            ),
            "the profile that is over on its own answers with its own need, not the sum of \
             the render: {answer:?}"
        );
        // 10B-PIN-1 END
        kept(&before);

        // The assignments' global `*` puts every model profile of the pool in
        // effect: the check measures that render as it measures one built of
        // explicit keys. (The switch of the whole pool is a key of the record,
        // never a `[*]` section of the pool a client writes: the client cannot
        // write `[*]` at all.)
        write_record(&sys, &["default"]);
        pool_files(&fits_text, "");
        assignments("[default]\n* = inherit\n");
        assert!(
            m.render_failures().is_empty(),
            "the fixture's pair puts the profile on the card at 1024 tokens"
        );
        let before = snapshot(&WATCH);
        let answer = m.write_file(&desktop, "models.ini", &on_text, false);
        assert!(
            over(
                &m,
                &answer,
                "qwen",
                "default",
                need(trained, "f16", "f16"),
                usable(small)
            ),
            "a pool in effect under the assignment's `* = inherit` is measured: {answer:?}"
        );
        kept(&before);
    }

    // ── The assignment: the render this record leaves is measured ────
    //
    // `set_model_profile` changes `assignments.ini`, not the pool: the render
    // to measure is the one the record LEAVES in effect, not the one on disk.
    // The write passes today's render as both sides, so the check sees an
    // unchanged render and an assignment that puts an over-memory profile in
    // effect lands: this is the rule slice 10b adds.
    {
        write_record(&sys, &["default"]);
        pool_files(&on_text, "");
        assignments("[default]\nqwen = off\n");
        let m = manager_on(FakeGpus::new(&[("CUDA0", small)]));
        // 10B-PIN-2 BEGIN
        let before = snapshot(&WATCH);
        let answer = m
            .set_model_profile(&desktop, "default", "qwen", "on")
            .map(|_| ());
        assert!(
            over(
                &m,
                &answer,
                "qwen",
                "default",
                need(trained, "f16", "f16"),
                usable(small)
            ),
            "assigning a model profile that is in the pool and does not fit the card is \
             refused, naming the profile and the instance whose render grows: {answer:?}"
        );
        kept(&before);
        // 10B-PIN-2 END

        // The switch off lands: the profile is in effect over the card (the
        // record seeded by hand, as a GPUs swap leaves it), and switching it
        // off shrinks the render: this write adds no need.
        assignments("[default]\nqwen = on\n");
        assert_eq!(
            m.set_model_profile(&desktop, "default", "qwen", "off")
                .map(|_| ()),
            Ok(()),
            "assigning `off` to a model profile that is over the card lands: the render \
             shrinks, so the write is judged on no growth"
        );
        assert_eq!(
            text_of("assignments.ini"),
            "[default]\nqwen = off\n",
            "the switch landed in the record"
        );

        // 10B-PIN-2b BEGIN. The count pin of the same assignment, self-contained:
        // its own pool, record and manager, because a count pin must make the
        // call it counts (the pin above's own copy of this call is inside its
        // block, and a proof run that deletes that block makes a count over it
        // vacuous). It accepts either answer, so it is green today, when the
        // check probes the GPUs before it judges anything: a guard that the
        // fixed write probes exactly once for a render that grows.
        pool_files(&on_text, "");
        assignments("[default]\nqwen = off\n");
        let gpus = FakeGpus::new(&[("CUDA0", small)]);
        let asked = gpus.asked.clone();
        let m_grow = manager_on(gpus);
        let _ = m_grow.set_model_profile(&desktop, "default", "qwen", "on");
        let asks = asked.lock().unwrap().clone();
        assert_eq!(
            asks,
            vec!["gpus".to_string()],
            "the assignment that puts an over-memory model profile in effect asks the GPUs \
             once, the render it leaves is judged: {asks:?}"
        );
        // 10B-PIN-2b END
    }

    // ── The need is each profile's own, never a sum over the render ──
    //
    // A second fixture sized from the same pure estimate: one profile of it
    // is about 60 % of the small card's usable memory, so two of them in one
    // render together exceed the card while each of them alone fits. The sum
    // rule refuses that save; the per-profile rule lands it.
    {
        write_record(&sys, &["default"]);
        pool_files("", "");
        assignments("[default]\nsixtya = on\nsixtyb = on\n");
        let half = models.join("half.gguf");
        let target = usable(small) * 3 / 5;
        write_gguf(&half, 262_144, &[("output.weight", target)]);
        let hs = local_shape(&half).expect("half fixture shape");
        // The KV term is the model's shape, not its bytes: taken out of the
        // target, what is left is the weight tensor (aligned to the 4096 the
        // header counts).
        let kv = full_gpu_bytes(&hs, 1024, "f16", "f16") - full_gpu_bytes(&hs, 1, "f16", "f16");
        let bytes = (target - kv) / 4096 * 4096;
        write_gguf(&half, 262_144, &[("output.weight", bytes)]);
        let hs = local_shape(&half).expect("the resized half fixture shape");
        let half_need = full_gpu_bytes(&hs, 1024, "f16", "f16");
        // 10B-PIN-3 BEGIN
        assert!(
            half_need <= usable(small) && 2 * half_need > usable(small),
            "the fixture: one profile of `half.gguf` fits the small card and two of them \
             together do not: {half_need} against {}",
            usable(small)
        );
        let half_path = half.to_string_lossy().into_owned();
        let two_fit = format!(
            "[sixtya]\nmodel = {half_path}\nctx-size = 1024\n\n[sixtyb]\nmodel = {half_path}\nctx-size = 1024\n"
        );
        let m = manager_on(FakeGpus::new(&[("CUDA0", small)]));
        let answer = m.write_file(&desktop, "models.ini", &two_fit, false);
        assert_eq!(
            answer,
            Ok(()),
            "two model profiles each at about 60 % of the card land in one write: each fits \
             on its own, the render's sum is not the rule: {answer:?}"
        );
        assert_eq!(text_of("models.ini"), two_fit, "the save landed");
        // 10B-PIN-3 END
    }

    // ── The GPUs are asked only when a section grew ──────────────────
    //
    // The estimate comes from the files, the GPUs from a probe: a write that
    // grows no profile never runs the probe (slice 10b, "The GPUs"). The
    // count is the seam's own log (`FakeGpus::asked`), one per manager.
    {
        write_record(&sys, &["default"]);
        pool_files(&on_text, "");
        assignments("[default]\nqwen = on\n");

        // A save of an unrelated key of a profile that stays as it is. The
        // pair on disk is the LAST GOOD render, which is the block above's:
        // two profiles of `half.gguf` at 1024 tokens, no `qwen` at all. The
        // pool on disk now names `qwen` at the trained context, so the render
        // this save leaves is not the pair: the instance is not converged, and
        // the fit check of a render that differs from the last good one judges
        // it on the cards, so the probe is asked. (`ARCHITECTURE.md`
        // "Convergence": fit is judged against the instance's rendered pair on
        // disk, the last good state.)
        let gpus = FakeGpus::new(&[("CUDA0", small)]);
        let asked = gpus.asked.clone();
        let m = manager_on(gpus);
        let edited = on_text.replace(
            &format!("ctx-size = {trained}\n"),
            &format!("ctx-size = {trained}\ntemperature = 0.5\n"),
        );
        let pair = snapshot(&["models@default.ini", "launch@default.ini"]);
        assert_eq!(
            m.write_file(&desktop, "models.ini", &edited, false),
            Ok(()),
            "fixture: the unchanged-over profile lands (the rows below are the pin)"
        );
        // 10B-PIN-4 BEGIN
        let asks = asked.lock().unwrap().clone();
        assert!(
            asks.len() <= 1 && asks.iter().all(|ask| ask == "gpus"),
            "a save of an unrelated key of an unchanged profile asks the GPUs at most once, \
             the render it leaves judged on the cards: {asks:?}"
        );
        // The fit of the new render is over the card, so neither rendered file
        // moves: the pair on disk is byte for byte the last good state.
        kept(&pair);
        let rows = m.not_converged_locked().expect("the rows");
        assert!(
            rows.iter()
                .any(|row| row.instance == "default" && row.key == "model-profile-over-memory"),
            "the instance keeps its last good pair and reports not converged with the memory \
             refusal: {rows:?}"
        );
        // 10B-PIN-4 END

        // The switch off: the render shrinks, no section grew, so the probe
        // is not asked at all.
        let gpus = FakeGpus::new(&[("CUDA0", small)]);
        let asked = gpus.asked.clone();
        let m = manager_on(gpus);
        assert_eq!(
            m.set_model_profile(&desktop, "default", "qwen", "off")
                .map(|_| ()),
            Ok(()),
            "fixture: the switch off lands (the count below is the pin)"
        );
        // 10B-PIN-5 BEGIN
        let asks = asked.lock().unwrap().clone();
        assert!(
            asks.is_empty(),
            "a switch off asks the GPUs 0 times: {asks:?}"
        );
        // 10B-PIN-5 END

        // A save that grows a profile: the probe is asked, once. Green today
        // (the check asks the seam first whatever it judges): a guard.
        pool_files("", "");
        assignments("[default]\nqwen = on\n");
        let gpus = FakeGpus::new(&[("CUDA0", small)]);
        let asked = gpus.asked.clone();
        let m = manager_on(gpus);
        let answer = m.write_file(&desktop, "models.ini", &on_text, false);
        // 10B-PIN-6 BEGIN
        let asks = asked.lock().unwrap().clone();
        assert_eq!(
            asks,
            vec!["gpus".to_string()],
            "a save that grows a profile asks the GPUs once"
        );
        // 10B-PIN-6 END
        assert!(
            over(
                &m,
                &answer,
                "qwen",
                "default",
                need(trained, "f16", "f16"),
                usable(small)
            ),
            "fixture: the growing save leaves default on its last good pair (the count \
             above is the pin): {answer:?}"
        );
    }
}
/// E2E phase: R3, the memory check measured on the pair (the server profile of
/// the instance, the model profile) (ARCHITECTURE.md "Resources belong to the
/// server profile" (3)). `available` is the sum, over the cards the instance's
/// server profile names in `Devices`, of total less the reserve (one card seen
/// by two backends counted once, as `gpu_pool`): every card when `Devices` is
/// absent, as today, and nothing to measure for `none` or for a `Devices` whose
/// ids the probe does not know, where the check is skipped as it is for a node
/// with no card. The rule is judged where it is today (a write of the model
/// profile pool, an assignment) and at the two places where `available` can
/// shrink: `set_server_profile`, and a save of a server profile in use that
/// changes `Devices`. At the two new places a model profile of the instance's
/// render is refused only when it fits today's `available` and not the new one:
/// what was already over is never re-judged, and a save that changes no
/// `Devices` judges nothing. The refusal is `model-profile-over-memory` with
/// `[model profile, instance, needed MiB, available MiB]`, and writes nothing.
///
/// The cards are sized from the estimate of one `llama` GGUF (`write_gguf`)
/// with the same pure API the manager uses, and the fixture pins that the
/// profile fits the two cards together and not one of them alone: a refusal
/// that does not come means the check, not a fixture that stopped being over
/// its card.
///
/// Names the tests assume, the contract for the coder: the pure rule is
/// `pai_core::devices::devices_usable(&[Gpu], &str) -> u64`, pinned in that
/// module's tests; every test here drives a manager verb, so the name is free
/// for the manager's wiring.
#[cfg(test)]
pub(crate) fn manager_pair_memory_check_e2e() {
    use fakes::{
        FakeActionLog, FakeAuthority, FakeGpus, FakeGroup, FakePasswords, FakeResolver, FakeService,
    };
    use pai_core::gguf_shape::{full_gpu_bytes, local_shape, GPU_RESERVE, MIB};

    let dir = tempfile::tempdir().expect("tempdir");
    crate::paths::redirect_for_tests(dir.path());
    let sys = crate::paths::engine_config_dir();
    std::fs::create_dir_all(&sys).expect("engine config dir");

    // The tree as this phase found it. It adds server profiles and moves the
    // record, and the phases after it in `data_root_e2e` read both, so the
    // phase puts back the names and the texts it found, at its end.
    let found: Vec<(String, String)> = std::fs::read_dir(&sys)
        .expect("engine config dir")
        .flatten()
        .filter(|entry| entry.path().is_file())
        .map(|entry| {
            let name = entry.file_name().to_string_lossy().into_owned();
            (
                name.clone(),
                std::fs::read_to_string(sys.join(&name)).unwrap_or_default(),
            )
        })
        .collect();

    let models = dir.path().join("models");
    std::fs::create_dir_all(&models).expect("models dir");
    let model = models.join("qwen-pair.gguf");
    write_gguf(
        &model,
        262_144,
        &[
            ("token_embd.weight", 200 << 20),
            ("blk.0.attn_q.weight", 300 << 20),
            ("blk.1.attn_q.weight", 300 << 20),
            ("output.weight", 200 << 20),
        ],
    );
    let path = model.to_string_lossy().into_owned();

    // What the model needs at the context it was trained on, read from the
    // file: the phase sizes its two cards and its refusals with this.
    let shape = local_shape(&model).expect("fixture shape");
    let trained = shape.trained_ctx;
    let need = full_gpu_bytes(&shape, trained, "f16", "f16");
    let mib_up = |bytes: u64| (bytes.div_ceil(MIB)).to_string();
    let mib_down = |bytes: u64| (bytes / MIB).to_string();
    // Two cards the same size, each about half the need: the profile fits the
    // pair and not one of them.
    let card = need / MIB / 2 + GPU_RESERVE / MIB + 8;
    let ids = [("CUDA0", card), ("CUDA1", card)];
    let usable_of = |names: &[&str]| -> u64 {
        pai_core::devices::pool_usable(
            &names
                .iter()
                .map(|id| pai_core::devices::Gpu {
                    id: (*id).to_string(),
                    name: format!("NVIDIA GeForce {id}"),
                    total_mib: card,
                })
                .collect::<Vec<_>>(),
        )
    };
    let on_text = format!("[qwen]\nmodel = {path}\nctx-size = {trained}\n");

    let bff = Caller(":1.9".into());
    let desktop = Caller(":1.42".into());
    let manager_on = |gpus: FakeGpus| {
        Manager::new(
            FakeService::new(),
            FakeAuthority::new(true).quietly(true).bff(&bff),
            FakeResolver::lan(),
        )
        .with_group(FakeGroup::new())
        .with_passwords(FakePasswords::new())
        .with_log(FakeActionLog::new())
        .with_gpus(gpus)
    };

    let read_opt = |name: &str| std::fs::read_to_string(sys.join(name)).ok();
    let text_of = |name: &str| std::fs::read_to_string(sys.join(name)).expect(name);
    let listing = || -> Vec<String> {
        let mut names: Vec<String> = std::fs::read_dir(&sys)
            .map(|rd| {
                rd.flatten()
                    .map(|e| e.file_name().to_string_lossy().into_owned())
                    .collect()
            })
            .unwrap_or_default();
        names.sort();
        names
    };
    let seen = |name: &str| -> String {
        if name == "listing" {
            format!("{}\n", listing().join("\n"))
        } else {
            read_opt(name).unwrap_or_default()
        }
    };
    let snapshot = |names: &[&str]| -> Vec<(String, String)> {
        names
            .iter()
            .map(|name| name.to_string())
            .chain(std::iter::once("listing".to_string()))
            .map(|name| (name.clone(), seen(&name)))
            .collect()
    };
    // What the check stops: the rendered pair of every instance (IP3c). The
    // file the write itself names LANDS, so the pool texts and the server
    // profiles are not part of this.
    let kept = |before: &[(String, String)]| {
        for (name, text) in before {
            if !(name.starts_with("models@") || name.starts_with("launch@") || name == "listing") {
                continue;
            }
            assert_eq!(
                &seen(name),
                text,
                "the instance keeps the pair it had: {name}"
            );
        }
    };
    let pool_files = |on: &str, off: &str| {
        std::fs::write(sys.join("models.ini"), on).expect("models.ini");
        std::fs::write(sys.join("models.ini.disabled"), off).expect("models.ini.disabled");
    };
    let assignments = |text: &str| {
        std::fs::write(sys.join("assignments.ini"), text).expect("assignments.ini");
    };
    // The record with `default` running the server profile `profile`.
    let record = |profile: &str| {
        std::fs::write(
            sys.join(INSTANCES_FILE),
            format!("[default]\nServerProfile = {profile}\n"),
        )
        .expect(INSTANCES_FILE);
    };
    // A server profile on its own port, so the switch's address check passes
    // whatever the other profiles of the fixture hold.
    let server_text = |devices: &str, port: u16| {
        format!("[Server]\nPort = {port}\nHostname = 127.0.0.1\nDevices = {devices}\n")
    };
    let profile_with = |name: &str, devices: &str, port: u16| {
        std::fs::write(
            sys.join(format!("server@{name}.ini")),
            server_text(devices, port),
        )
        .expect("server profile");
    };
    let profile_plain = |name: &str, port: u16| {
        std::fs::write(
            sys.join(format!("server@{name}.ini")),
            format!("[Server]\nPort = {port}\nHostname = 127.0.0.1\n"),
        )
        .expect("server profile");
    };
    // The memory news this phase is about, its four args whole. The check is a
    // fit check of ONE instance (IP3c, `ARCHITECTURE.md` "Convergence"): the
    // write that brings the news LANDS, and the instance whose render it stops
    // keeps its pair and is reported by that write's own render pass, with the
    // check's own key and the same four args. True when the answer is that.
    let over = |m: &Manager<FakeService, FakeAuthority, FakeResolver>,
                answer: &Result<(), ManagerError>,
                profile: &str,
                instance: &str,
                needed: u64,
                available: u64| {
        answer.is_ok()
            && m.not_converged_locked()
                .expect("the rows")
                .iter()
                .any(|row| {
                    row.instance == instance
                        && row.key == "model-profile-over-memory"
                        && row.args
                            == [
                                profile.to_string(),
                                instance.to_string(),
                                mib_up(needed),
                                mib_down(available),
                            ]
                })
    };

    const WATCH: [&str; 8] = [
        "assignments.ini",
        "instances.ini",
        "models.ini",
        "models.ini.disabled",
        "models@default.ini",
        "server@default.ini",
        "server@narrow.ini",
        "server@wide.ini",
    ];

    // The fixture: the model profile fits the two cards and not one of them.
    assert!(
        need > usable_of(&["CUDA0"]),
        "the model profile is over one card: {need} against {}",
        usable_of(&["CUDA0"])
    );
    assert!(
        need <= usable_of(&["CUDA0", "CUDA1"]),
        "the model profile fits the pair of cards: {need} against {}",
        usable_of(&["CUDA0", "CUDA1"])
    );

    // ── R3-PIN-1 BEGIN. The pair at a model profile write ─────────────────
    //
    // The node has two cards and the instance's server profile names one: the
    // profile that fits the pair is over the card the instance runs on.
    {
        record("default");
        profile_with("default", "CUDA0", 8080);
        pool_files("", "");
        assignments("[default]\nqwen = on\n");
        let m = manager_on(FakeGpus::new(&ids));
        let before = snapshot(&WATCH);
        let answer = m.write_file(&desktop, "models.ini", &on_text, false);
        assert!(
            over(&m, &answer, "qwen", "default", need, usable_of(&["CUDA0"])),
            "a model profile that fits the node's two cards is refused for an instance whose \
             server profile names one of them, naming the MiB that one card has: {answer:?}"
        );
        kept(&before);
    }
    // R3-PIN-1 END

    // ── R3-PIN-2 BEGIN. `Devices` absent is every card, as today ──────────
    //
    // A server profile that names no card leaves the node's whole pool as the
    // measure, so the profile that fits the pair lands.
    {
        record("default");
        profile_plain("default", 8080);
        pool_files("", "");
        assignments("[default]\nqwen = on\n");
        let m = manager_on(FakeGpus::new(&ids));
        assert_eq!(
            m.write_file(&desktop, "models.ini", &on_text, false),
            Ok(()),
            "a server profile with no Devices is measured on every GPU, as today: the pair \
             fits and the save lands"
        );
        assert_eq!(text_of("models.ini"), on_text, "the save landed");
    }
    // R3-PIN-2 END

    // ── R3-PIN-3 BEGIN. `none` skips the check ────────────────────────────
    //
    // `Devices = none` is a CPU only server profile: no card to measure
    // against, so the write is judged on nothing, however big the model.
    {
        record("default");
        profile_with("default", "none", 8080);
        pool_files("", "");
        assignments("[default]\nqwen = on\n");
        let m = manager_on(FakeGpus::new(&ids));
        let answer = m.write_file(&desktop, "models.ini", &on_text, false);
        assert_eq!(
            answer,
            Ok(()),
            "`Devices = none` names no GPU: the memory check is skipped, as for a node with \
             no card"
        );
    }
    // R3-PIN-3 END

    // ── R3-PIN-4 BEGIN. an id the probe does not know skips the check ─────
    {
        record("default");
        profile_with("default", "ROCm9", 8080);
        pool_files("", "");
        assignments("[default]\nqwen = on\n");
        let m = manager_on(FakeGpus::new(&ids));
        assert_eq!(
            m.write_file(&desktop, "models.ini", &on_text, false),
            Ok(()),
            "a `Devices` of ids the probe knows none of leaves nothing to measure: the write \
             lands"
        );
    }
    // R3-PIN-4 END

    // ── R3-PIN-5 BEGIN. the pair at an assignment ─────────────────────────
    //
    // The record puts the profile in effect on an instance whose server profile
    // names one card: the render the record leaves is measured on that card.
    {
        record("default");
        profile_with("default", "CUDA0", 8080);
        pool_files(&on_text, "");
        assignments("[default]\nqwen = off\n");
        let m = manager_on(FakeGpus::new(&ids));
        assert!(
            m.render_failures().is_empty(),
            "the fixture's pair leaves the model profile out: the assignment is the news"
        );
        let before = snapshot(&WATCH);
        let answer = m
            .set_model_profile(&desktop, "default", "qwen", "on")
            .map(|_| ());
        assert!(
            over(&m, &answer, "qwen", "default", need, usable_of(&["CUDA0"])),
            "assigning a model profile that fits the node's two cards is reported for an \
             instance whose server profile names one of them, and its pair stands: {answer:?}"
        );
        kept(&before);
    }
    // R3-PIN-5 END

    // ── R3-PIN-6 BEGIN. the switch of the server profile ──────────────────
    //
    // The instance runs a profile naming both cards, where the model profile
    // fits. The switch to a one-card server profile shrinks `available`, so the
    // render is judged and the switch lands (IP3c): the record moves, and the
    // instance keeps its last good pair, reported over the one card.
    {
        record("wide");
        profile_with("wide", "CUDA0,CUDA1", 8080);
        profile_with("narrow", "CUDA0", 8090);
        pool_files(&on_text, "");
        assignments("[default]\nqwen = on\n");
        let m = manager_on(FakeGpus::new(&ids));
        let before = snapshot(&WATCH);
        let answer = m
            .set_server_profile(&desktop, "default", "narrow")
            .map(|_| ());
        assert!(
            over(&m, &answer, "qwen", "default", need, usable_of(&["CUDA0"])),
            "a model profile that fits the two cards the instance runs today is reported at a \
             switch to a one-card server profile, naming the MiB that card has: {answer:?}"
        );
        kept(&before);
        assert_eq!(
            text_of(INSTANCES_FILE),
            "[default]\nServerProfile = narrow\n",
            "the switch landed: the record names the profile it was asked for, and the pair \
             above is the last good one"
        );
    }
    // R3-PIN-6 END

    // ── R3-PIN-7 BEGIN. a save narrowing the profile's Devices ────────────
    //
    // The same shrink, arriving as a save of the server profile the instance
    // runs: the render it caps is judged on the cards the text being written
    // names. The save lands (IP3c) and the instance's pair keeps the bytes it
    // had, the instance reported.
    {
        record("default");
        profile_with("default", "CUDA0,CUDA1", 8080);
        pool_files(&on_text, "");
        assignments("[default]\nqwen = on\n");
        let m = manager_on(FakeGpus::new(&ids));
        let before = snapshot(&WATCH);
        let answer = m.write_file(
            &desktop,
            "server@default.ini",
            &server_text("CUDA0", 8080),
            false,
        );
        assert!(
            over(&m, &answer, "qwen", "default", need, usable_of(&["CUDA0"])),
            "a save that narrows an in-use server profile to one card is reported for a model \
             profile that fits the two cards it names today: {answer:?}"
        );
        kept(&before);
        assert!(
            text_of("server@default.ini").contains("Devices = CUDA0\n"),
            "the save landed: the profile names the one card it was saved with"
        );
    }
    // R3-PIN-7 END

    // ── R3-PIN-8 BEGIN. what was already over is not re-judged ────────────
    //
    // The growth rule turned round: at the two new judging points a model
    // profile the instance already runs over its card is no news, so the switch
    // and the save land. Green today (no memory check runs at either point),
    // the guard that the new check refuses only what the shrink makes over.
    {
        record("over1");
        profile_with("over1", "CUDA0", 8080);
        profile_with("over2", "CUDA0", 8090);
        pool_files(&on_text, "");
        assignments("[default]\nqwen = on\n");
        let m = manager_on(FakeGpus::new(&ids));
        assert_eq!(
            m.set_server_profile(&desktop, "default", "over2")
                .map(|_| ()),
            Ok(()),
            "a model profile already over the card it runs on is not re-judged at a switch: \
             it stays over, this write moved the profile, not the need"
        );
        assert!(
            text_of(INSTANCES_FILE).contains("ServerProfile = over2"),
            "the switch landed"
        );

        // A save of an in-use profile that changes no `Devices` judges nothing,
        // even on an instance whose render is over the card it names.
        record("over3");
        profile_with("over3", "CUDA0", 8080);
        pool_files(&on_text, "");
        assignments("[default]\nqwen = on\n");
        let m = manager_on(FakeGpus::new(&ids));
        let edited = "[Server]\nPort = 8080\nHostname = 127.0.0.1\nDevices = CUDA0\nThreads = 8\n";
        assert_eq!(
            m.write_file(&desktop, "server@over3.ini", edited, false),
            Ok(()),
            "a save of a server profile in use that changes no Devices is judged on nothing"
        );
        assert!(
            text_of("server@over3.ini").contains("Threads = 8"),
            "the save landed"
        );
    }
    // R3-PIN-8 END

    // ── R3-PIN-9 BEGIN. the skip rule beside the new judging points ───────
    //
    // `none` names no card, so a switch that moves an instance onto a CPU only
    // server profile is judged on nothing, whatever the render holds. Green
    // today (no check at the switch), the guard that the new check skips.
    {
        record("pair1");
        profile_with("pair1", "CUDA0,CUDA1", 8080);
        profile_with("cpu", "none", 8090);
        pool_files(&on_text, "");
        assignments("[default]\nqwen = on\n");
        let m = manager_on(FakeGpus::new(&ids));
        assert_eq!(
            m.set_server_profile(&desktop, "default", "cpu").map(|_| ()),
            Ok(()),
            "a switch to a `none` server profile names no GPU: the memory check is skipped"
        );
        assert!(
            text_of(INSTANCES_FILE).contains("ServerProfile = cpu"),
            "the switch landed"
        );
    }
    // R3-PIN-9 END

    // ── R3-PIN-10 BEGIN. the shrink check asks the seam only with an estimate
    //
    // The seam runs the engine's probe. `check_render_memory` asks it once a
    // section has grown; the shrink check has the same shape: a render with no
    // estimate to measure, judged on cards that shrank, is judged on nothing,
    // so the probe never runs. The count is the seam's own log, read from
    // after the manager is built: a manager that probes at its own construction
    // is the coder's choice and is not counted here.
    {
        // The instance runs a two-card server profile and its render holds no
        // model profile at all: the switch to a one-card profile shrinks the
        // memory of nothing.
        record("wide");
        profile_with("wide", "CUDA0,CUDA1", 8080);
        profile_with("narrow", "CUDA0", 8090);
        pool_files("", "");
        assignments("[default]\nqwen = on\n");
        let gpus = FakeGpus::new(&ids);
        let asked = gpus.asked.clone();
        let m = manager_on(gpus);
        let base = asked.lock().unwrap().len();
        assert_eq!(
            m.set_server_profile(&desktop, "default", "narrow")
                .map(|_| ()),
            Ok(()),
            "fixture: the switch of an instance whose render holds no model profile lands"
        );
        let asks = asked.lock().unwrap().clone();
        assert!(
            asks[base..].is_empty(),
            "a switch of an instance whose render has no model profile asks the GPU seam 0 \
             times: the probe runs only once the render has an estimate to measure: {asks:?}"
        );

        // The render holds a model profile whose estimate gives up (an `hf-repo`
        // profile names no file of the node, so there is no header to read):
        // the same, no probe, the switch landing.
        record("wide2");
        profile_with("wide2", "CUDA0,CUDA1", 8100);
        profile_with("narrow2", "CUDA0", 8110);
        pool_files("[fetch]\nhf-repo = a/b\n", "");
        assignments("[default]\nfetch = on\n");
        let gpus = FakeGpus::new(&ids);
        let asked = gpus.asked.clone();
        let m = manager_on(gpus);
        let base = asked.lock().unwrap().len();
        assert_eq!(
            m.set_server_profile(&desktop, "default", "narrow2")
                .map(|_| ()),
            Ok(()),
            "fixture: the switch of an instance whose model profile names no file lands"
        );
        let asks = asked.lock().unwrap().clone();
        assert!(
            asks[base..].is_empty(),
            "a switch of an instance whose render has no estimate asks the GPU seam 0 times, as \
             the model profile write does: {asks:?}"
        );
    }
    // R3-PIN-10 END

    // ── R3-PIN-11 BEGIN. the save, same shape ──────────────────────────────
    //
    // The shrink check's other judging point: a save of an in-use server
    // profile that changes `Devices` asks the seam only when the instance's
    // render has something to measure.
    {
        // No model profile in effect.
        record("default");
        profile_with("default", "CUDA0,CUDA1", 8080);
        pool_files("", "");
        assignments("[default]\nqwen = on\n");
        let gpus = FakeGpus::new(&ids);
        let asked = gpus.asked.clone();
        let m = manager_on(gpus);
        let base = asked.lock().unwrap().len();
        assert_eq!(
            m.write_file(
                &desktop,
                "server@default.ini",
                &server_text("CUDA0", 8080),
                false
            ),
            Ok(()),
            "fixture: the save narrowing an instance with no model profile lands"
        );
        let asks = asked.lock().unwrap().clone();
        assert!(
            asks[base..].is_empty(),
            "a save that narrows an in-use server profile of an instance whose render holds no \
             model profile asks the GPU seam 0 times: {asks:?}"
        );

        // A render whose estimate gives up.
        record("default");
        profile_with("default", "CUDA0,CUDA1", 8080);
        pool_files("[fetch]\nhf-repo = a/b\n", "");
        assignments("[default]\nfetch = on\n");
        let gpus = FakeGpus::new(&ids);
        let asked = gpus.asked.clone();
        let m = manager_on(gpus);
        let base = asked.lock().unwrap().len();
        assert_eq!(
            m.write_file(
                &desktop,
                "server@default.ini",
                &server_text("CUDA0", 8080),
                false
            ),
            Ok(()),
            "fixture: the save narrowing an instance whose profile names no file lands"
        );
        let asks = asked.lock().unwrap().clone();
        assert!(
            asks[base..].is_empty(),
            "a save that narrows an in-use server profile of an instance whose render has no \
             estimate asks the GPU seam 0 times: {asks:?}"
        );
    }
    // R3-PIN-11 END

    // ── R3-PIN-12 BEGIN. a mixed list counts the cards the probe knows ─────
    //
    // `Devices = ROCm0,ROCm9` on a node with a ROCm0 and a CUDA1: the name the
    // probe does not know adds nothing, so the instance is measured on one card
    // and the model profile that fits the pair is refused with that one card's
    // MiB. Green today (`devices_usable` skips an unknown name and the write
    // judge measures on `Devices`): the guard that the skip rule is a per name
    // rule, not a whole list one.
    {
        record("default");
        profile_with("default", "ROCm0,ROCm9", 8080);
        pool_files("", "");
        assignments("[default]\nqwen = on\n");
        let m = manager_on(FakeGpus::new(&[("ROCm0", card), ("CUDA1", card)]));
        let before = snapshot(&WATCH);
        let answer = m.write_file(&desktop, "models.ini", &on_text, false);
        assert!(
            over(&m, &answer, "qwen", "default", need, usable_of(&["ROCm0"])),
            "a `Devices` list of one name the probe knows and one it does not is measured on \
             the card it knows, refusing the profile that needs both cards: {answer:?}"
        );
        kept(&before);
    }
    // R3-PIN-12 END

    // The phase leaves the tree as it found it: the record it moved and the
    // server profiles it added belong to the phases after it.
    for entry in std::fs::read_dir(&sys).expect("listing").flatten() {
        let name = entry.file_name().to_string_lossy().into_owned();
        if !found.iter().any(|(found, _)| *found == name) {
            let _ = std::fs::remove_file(entry.path());
        }
    }
    for (name, text) in &found {
        std::fs::write(sys.join(name), text).expect("restore");
    }
}
/// E2E phase: the device bound for `device-draft` and `mmproj-device`
/// (ARCHITECTURE.md "Layers, MoE and tensor placement on the web model profile
/// page" (3), slice 3b). Every GPU a model profile names (the entries of
/// `device-draft` and `mmproj-device`, the section's value else the `[*]` one
/// as llama.cpp cascades; `CPU` and `none` name no GPU) must be one the
/// instance's server profile names in `Devices`: compared case-insensitively,
/// `Devices` unset is every GPU and refuses nothing, `Devices = none` makes
/// any GPU named outside, and the node's probe is not asked. Refused with
/// `model-profile-device-outside`, `[model profile, instance, key, device]`,
/// writing nothing, at the points R3's memory check already stands: a model
/// profile write (both `check_contents` and `set_model_profile`, judged on the
/// new text), an assignment, `SetServerProfile` and a save of an in-use server
/// profile that narrows `Devices`, where as in R3 only a profile that passes
/// the old set and fails the new one is refused: what was already outside is
/// never re-judged.
///
/// The fixture keeps the memory check out of the way: one `write_gguf` model of
/// a MiB of weights at a context of 8, on cards of 8 GiB, so every refusal
/// here is the device bound's and never R3's.
///
/// Names the tests assume, the contract for the coder: the pure rule is
/// `pai_core::checks::devices_outside(&str, &str) -> Option<(String, String)>`,
/// pinned in that module's tests; every test here drives a manager verb, so
/// the wiring is free.
#[cfg(test)]
pub(crate) fn manager_device_bound_check_e2e() {
    use fakes::{
        FakeActionLog, FakeAuthority, FakeGpus, FakeGroup, FakePasswords, FakeResolver, FakeService,
    };

    let dir = tempfile::tempdir().expect("tempdir");
    crate::paths::redirect_for_tests(dir.path());
    let sys = crate::paths::engine_config_dir();
    std::fs::create_dir_all(&sys).expect("engine config dir");

    // The tree as this phase found it, put back at its end as R3's does.
    let found: Vec<(String, String)> = std::fs::read_dir(&sys)
        .expect("engine config dir")
        .flatten()
        .filter(|entry| entry.path().is_file())
        .map(|entry| {
            let name = entry.file_name().to_string_lossy().into_owned();
            (
                name.clone(),
                std::fs::read_to_string(sys.join(&name)).unwrap_or_default(),
            )
        })
        .collect();

    let models = dir.path().join("models");
    std::fs::create_dir_all(&models).expect("models dir");
    let model = models.join("qwen-bound.gguf");
    write_gguf(&model, 8, &[("token_embd.weight", 1 << 20)]);
    let path = model.to_string_lossy().into_owned();
    // Small on 8 GiB cards: the memory check passes everywhere, so a refusal
    // is the device bound's.
    const CARD: u64 = 8192;
    let gpus = || FakeGpus::new(&[("ROCm0", CARD), ("CUDA1", CARD)]);

    let bff = Caller(":1.9".into());
    let desktop = Caller(":1.42".into());
    let manager_on = |gpus: FakeGpus| {
        Manager::new(
            FakeService::new(),
            FakeAuthority::new(true).quietly(true).bff(&bff),
            FakeResolver::lan(),
        )
        .with_group(FakeGroup::new())
        .with_passwords(FakePasswords::new())
        .with_log(FakeActionLog::new())
        .with_gpus(gpus)
    };

    let read_opt = |name: &str| std::fs::read_to_string(sys.join(name)).ok();
    let text_of = |name: &str| std::fs::read_to_string(sys.join(name)).expect(name);
    let listing = || -> Vec<String> {
        let mut names: Vec<String> = std::fs::read_dir(&sys)
            .map(|rd| {
                rd.flatten()
                    .map(|e| e.file_name().to_string_lossy().into_owned())
                    .collect()
            })
            .unwrap_or_default();
        names.sort();
        names
    };
    let seen = |name: &str| -> String {
        if name == "listing" {
            format!("{}\n", listing().join("\n"))
        } else {
            read_opt(name).unwrap_or_default()
        }
    };
    let snapshot = |names: &[&str]| -> Vec<(String, String)> {
        names
            .iter()
            .map(|name| name.to_string())
            .chain(std::iter::once("listing".to_string()))
            .map(|name| (name.clone(), seen(&name)))
            .collect()
    };
    // What the bound stops: the rendered pair of every instance (IP3c). The
    // file the write itself names LANDS, so the pools, the record and the
    // server profiles are not part of this.
    let kept = |before: &[(String, String)]| {
        for (name, text) in before {
            if !(name.starts_with("models@") || name.starts_with("launch@") || name == "listing") {
                continue;
            }
            assert_eq!(
                &seen(name),
                text,
                "the instance keeps the pair it had: {name}"
            );
        }
    };
    let pool_files = |on: &str, off: &str| {
        std::fs::write(sys.join("models.ini"), on).expect("models.ini");
        std::fs::write(sys.join("models.ini.disabled"), off).expect("models.ini.disabled");
    };
    let assignments = |text: &str| {
        std::fs::write(sys.join("assignments.ini"), text).expect("assignments.ini");
    };
    let record = |profile: &str| {
        std::fs::write(
            sys.join(INSTANCES_FILE),
            format!("[default]\nServerProfile = {profile}\n"),
        )
        .expect(INSTANCES_FILE);
    };
    let server_text = |devices: &str, port: u16| {
        format!("[Server]\nPort = {port}\nHostname = 127.0.0.1\nDevices = {devices}\n")
    };
    let profile_with = |name: &str, devices: &str, port: u16| {
        std::fs::write(
            sys.join(format!("server@{name}.ini")),
            server_text(devices, port),
        )
        .expect("server profile");
    };
    let profile_plain = |name: &str, port: u16| {
        std::fs::write(
            sys.join(format!("server@{name}.ini")),
            format!("[Server]\nPort = {port}\nHostname = 127.0.0.1\n"),
        )
        .expect("server profile");
    };
    // The device news this phase is about, its four args whole. The bound is a
    // fit check of ONE instance (IP3c, `ARCHITECTURE.md` "Convergence"): the
    // write that brings the news LANDS, and the instance whose render it stops
    // keeps its pair and is reported by that write's own render pass, with the
    // check's own key and the same four args. True when the answer is that.
    let outside = |m: &Manager<FakeService, FakeAuthority, FakeResolver>,
                   answer: &Result<(), ManagerError>,
                   profile: &str,
                   instance: &str,
                   key: &str,
                   device: &str| {
        answer.is_ok()
            && m.not_converged_locked()
                .expect("the rows")
                .iter()
                .any(|row| {
                    row.instance == instance
                        && row.key == "model-profile-device-outside"
                        && row.args
                            == [
                                profile.to_string(),
                                instance.to_string(),
                                key.to_string(),
                                device.to_string(),
                            ]
                })
    };

    const WATCH: [&str; 9] = [
        "assignments.ini",
        "instances.ini",
        "models.ini",
        "models.ini.disabled",
        "launch@default.ini",
        "models@default.ini",
        "server@default.ini",
        "server@narrow.ini",
        "server@wide.ini",
    ];

    // The fixture texts: the model profile naming one GPU, and its shapes.
    let draft =
        |device: &str| format!("[qwen]\nmodel = {path}\nctx-size = 8\ndevice-draft = {device}\n");
    let draft1 = draft("ROCm1");
    let mmproj =
        |device: &str| format!("[qwen]\nmodel = {path}\nctx-size = 8\nmmproj-device = {device}\n");

    // ── DB-PIN-1 BEGIN. a model profile write naming a GPU outside Devices ──
    //
    // The instance's server profile names ROCm0: a save of a profile naming
    // ROCm1 lands and is reported naming profile, instance, key and device, and
    // the pair `default` runs keeps its bytes.
    {
        record("default");
        profile_with("default", "ROCm0", 8080);
        pool_files("", "");
        assignments("[default]\nqwen = on\n");
        let m = manager_on(gpus());
        assert!(
            m.render_failures().is_empty(),
            "the fixture's pair names no GPU outside: the write below is the news"
        );
        let before = snapshot(&WATCH);
        let answer = m.write_file(&desktop, "models.ini", &draft1, false);
        assert!(
            outside(&m, &answer, "qwen", "default", "device-draft", "ROCm1"),
            "a model profile save naming a GPU the instance's server profile does not name is \
             reported naming the key and the device, the pair standing: {answer:?}"
        );
        kept(&before);
    }
    // DB-PIN-1 END

    // ── DB-PIN-2 BEGIN. the compare is case-insensitive ────────────────────
    //
    // `rocm0` names the card the server profile names: `ggml_backend_dev_by_name`
    // matches so, so the save lands.
    {
        record("default");
        profile_with("default", "ROCm0", 8080);
        pool_files("", "");
        assignments("[default]\nqwen = on\n");
        let m = manager_on(gpus());
        let lowered = draft("rocm0");
        assert_eq!(
            m.write_file(&desktop, "models.ini", &lowered, false),
            Ok(()),
            "a device-draft naming the server profile's card in another case is not outside"
        );
        assert_eq!(text_of("models.ini"), lowered, "the save landed");
    }
    // DB-PIN-2 END

    // ── DB-PIN-3 BEGIN. `mmproj-device` and `none` and no GPU named ────────
    //
    // `mmproj-device = none` names no GPU, and a profile naming no GPU at all
    // is judged on nothing: both saves land on a `Devices = ROCm0` server
    // profile.
    {
        record("default");
        profile_with("default", "ROCm0", 8080);
        pool_files("", "");
        assignments("[default]\nqwen = on\n");
        let m = manager_on(gpus());
        let off_vision = mmproj("none");
        assert_eq!(
            m.write_file(&desktop, "models.ini", &off_vision, false),
            Ok(()),
            "`mmproj-device = none` names no GPU: the save lands"
        );
    }
    // DB-PIN-3 END

    // ── DB-PIN-4 BEGIN. an unassigned profile save lands ───────────────────
    //
    // The profile names ROCm1 and is in no instance's render (`qwen = off`):
    // no instance to judge, so the save lands.
    {
        record("default");
        profile_with("default", "ROCm0", 8080);
        pool_files("", "");
        assignments("[default]\nqwen = off\n");
        let m = manager_on(gpus());
        assert_eq!(
            m.write_file(&desktop, "models.ini", &draft1, false),
            Ok(()),
            "a profile no instance loads is judged on no server profile: the save lands"
        );
        assert_eq!(text_of("models.ini"), draft1, "the save landed");
    }
    // DB-PIN-4 END

    // ── DB-PIN-5 BEGIN. the same refusal at an assignment ──────────────────
    //
    // `set_model_profile` is the path that renders the new record text: the
    // profile sits on disk with `device-draft = ROCm1` and the assignment `on`
    // puts it in effect on the `ROCm0` instance, so the record lands, the verb
    // reports the instance, and its pair keeps its bytes.
    {
        record("default");
        profile_with("default", "ROCm0", 8080);
        pool_files(&draft1, "");
        assignments("[default]\nqwen = off\n");
        let m = manager_on(gpus());
        assert!(
            m.render_failures().is_empty(),
            "the fixture's pair names no GPU outside: the write below is the news"
        );
        let before = snapshot(&WATCH);
        let answer = m
            .set_model_profile(&desktop, "default", "qwen", "on")
            .map(|_| ());
        assert!(
            outside(&m, &answer, "qwen", "default", "device-draft", "ROCm1"),
            "assigning a profile naming a GPU outside the instance's server profile is reported \
             by `set_model_profile`, judging the new record text: {answer:?}"
        );
        kept(&before);
    }
    // DB-PIN-5 END

    // ── DB-PIN-6 BEGIN. the switch to a `Devices = ROCm0` profile ──────────
    //
    // The instance runs a `ROCm0,ROCm1` server profile where the profile's
    // `ROCm1` passes (names are exact, case-insensitive: `CUDA1` would not
    // carry it); the switch to `ROCm0` puts it outside, so the record lands on
    // the new profile, the instance is reported, and its pair stands.
    {
        record("wide");
        profile_with("wide", "ROCm0,ROCm1", 8080);
        profile_with("narrow", "ROCm0", 8090);
        pool_files(&draft1, "");
        assignments("[default]\nqwen = on\n");
        let m = manager_on(gpus());
        assert!(
            m.render_failures().is_empty(),
            "the fixture's pair names no GPU outside: the write below is the news"
        );
        let before = snapshot(&WATCH);
        let answer = m
            .set_server_profile(&desktop, "default", "narrow")
            .map(|_| ());
        assert!(
            outside(&m, &answer, "qwen", "default", "device-draft", "ROCm1"),
            "a switch to a server profile that leaves a named GPU outside is reported: {answer:?}"
        );
        kept(&before);
        assert_eq!(
            text_of(INSTANCES_FILE),
            "[default]\nServerProfile = narrow\n",
            "the switch landed: the record names the profile it was asked for, and the pair \
             above is the last good one"
        );
    }
    // DB-PIN-6 END

    // ── DB-PIN-7 BEGIN. a save narrowing `ROCm0,ROCm1` to `ROCm0` ──────────
    {
        record("default");
        profile_with("default", "ROCm0,ROCm1", 8080);
        pool_files(&draft1, "");
        assignments("[default]\nqwen = on\n");
        let m = manager_on(gpus());
        assert!(
            m.render_failures().is_empty(),
            "the fixture's pair names no GPU outside: the write below is the news"
        );
        let before = snapshot(&WATCH);
        let answer = m.write_file(
            &desktop,
            "server@default.ini",
            &server_text("ROCm0", 8080),
            false,
        );
        assert!(
            outside(&m, &answer, "qwen", "default", "device-draft", "ROCm1"),
            "a save that narrows an in-use server profile past a GPU the render names is \
             reported: {answer:?}"
        );
        kept(&before);
        assert!(
            text_of("server@default.ini").contains("Devices = ROCm0\n"),
            "the narrowing save landed: the profile names the one card it was saved with, and \
             the pair above is the last good one"
        );
    }
    // DB-PIN-7 END

    // ── DB-PIN-8 BEGIN. a profile already outside is not re-judged ─────────
    //
    // `ROCm2` is outside the `ROCm0,CUDA1` set the instance runs today: at the
    // shrink to `ROCm0` it is no news, so the switch and the narrowing save
    // land, as R3's already-over rule.
    {
        record("wide");
        profile_with("wide", "ROCm0,CUDA1", 8080);
        profile_with("narrow", "ROCm0", 8090);
        pool_files(&draft("ROCm2"), "");
        assignments("[default]\nqwen = on\n");
        let m = manager_on(gpus());
        assert_eq!(
            m.set_server_profile(&desktop, "default", "narrow")
                .map(|_| ()),
            Ok(()),
            "a profile already outside the set the instance runs is not re-judged at a switch"
        );
        assert!(
            text_of(INSTANCES_FILE).contains("ServerProfile = narrow"),
            "the switch landed"
        );

        record("default");
        profile_with("default", "ROCm0,CUDA1", 8080);
        pool_files(&draft("ROCm2"), "");
        assignments("[default]\nqwen = on\n");
        let m = manager_on(gpus());
        assert_eq!(
            m.write_file(
                &desktop,
                "server@default.ini",
                &server_text("ROCm0", 8080),
                false
            ),
            Ok(()),
            "a narrowing save past an already-outside profile judges nothing: it lands"
        );
        assert!(
            text_of("server@default.ini").contains("Devices = ROCm0\n"),
            "the narrowing save landed"
        );
    }
    // DB-PIN-8 END

    // ── DB-PIN-9 BEGIN. `Devices` unset refuses nothing ────────────────────
    {
        record("default");
        profile_plain("default", 8080);
        pool_files("", "");
        assignments("[default]\nqwen = on\n");
        let m = manager_on(gpus());
        assert_eq!(
            m.write_file(&desktop, "models.ini", &draft1, false),
            Ok(()),
            "`Devices` unset is every GPU: `ROCm1` is not outside and the save lands"
        );
    }
    // DB-PIN-9 END

    // ── DB-PIN-10 BEGIN. `Devices = none` refuses any GPU ──────────────────
    {
        record("default");
        profile_with("default", "none", 8080);
        pool_files("", "");
        assignments("[default]\nqwen = on\n");
        let m = manager_on(gpus());
        assert!(
            m.render_failures().is_empty(),
            "the fixture's pair names no GPU outside: the write below is the news"
        );
        let before = snapshot(&WATCH);
        let answer = m.write_file(&desktop, "models.ini", &mmproj("CUDA1"), false);
        assert!(
            outside(&m, &answer, "qwen", "default", "mmproj-device", "CUDA1"),
            "`Devices = none`: any GPU named is outside, so a `mmproj-device = CUDA1` save is \
             reported naming the key and the device, the pair standing: {answer:?}"
        );
        kept(&before);
    }
    // DB-PIN-10 END

    // ── DB-PIN-11 BEGIN. the `[*]` value counts ────────────────────────────
    //
    // The section names no `device-draft` of its own and the pool-wide `[*]`
    // names ROCm1: llama.cpp cascades the `[*]` value into every profile, so
    // the bound counts it, naming the profile the value lands on. No client
    // write carries a `[*]` (it is refused `invalid-model-profile-id`), so the
    // cascade sits on disk as a hand edit and is judged where the pool is read
    // FROM DISK: at `set_model_profile`, the assignment that puts the profile
    // in effect.
    {
        record("default");
        profile_with("default", "ROCm0", 8080);
        let cascaded = format!("[*]\ndevice-draft = ROCm1\n[qwen]\nmodel = {path}\nctx-size = 8\n");
        pool_files(&cascaded, "");
        assignments("[default]\nqwen = off\n");
        let m = manager_on(gpus());
        assert!(
            m.render_failures().is_empty(),
            "the fixture's pair names no GPU outside: the write below is the news"
        );
        let before = snapshot(&WATCH);
        let answer = m
            .set_model_profile(&desktop, "default", "qwen", "on")
            .map(|_| ());
        assert!(
            outside(&m, &answer, "qwen", "default", "device-draft", "ROCm1"),
            "a `device-draft` the `[*]` section carries cascades into `[qwen]` and is judged \
             as the section's own at the assignment that puts it in effect: {answer:?}"
        );
        kept(&before);
    }
    // DB-PIN-11 END

    // ── DB-PIN-12 BEGIN. the `override-tensor` devices join the bound ─────
    //
    // J1 puts `override-tensor` on the web, so its rules' devices are the
    // page's to write and the bound judges them: the same refusal with
    // `override-tensor` as the key, at the four points. `CPU` names no GPU.
    {
        let ot = |device: &str| {
            format!("[qwen]\nmodel = {path}\nctx-size = 8\noverride-tensor = blk\\.0\\.={device}\n")
        };

        // The model profile write.
        record("default");
        profile_with("default", "ROCm0", 8080);
        pool_files("", "");
        assignments("[default]\nqwen = on\n");
        let m = manager_on(gpus());
        assert!(
            m.render_failures().is_empty(),
            "the fixture's pair names no GPU outside: the write below is the news"
        );
        let before = snapshot(&WATCH);
        let answer = m.write_file(&desktop, "models.ini", &ot("ROCm1"), false);
        assert!(
            outside(&m, &answer, "qwen", "default", "override-tensor", "ROCm1"),
            "a save naming in `override-tensor` a card the instance's server profile does not \
             name is reported naming the key and the device: {answer:?}"
        );
        kept(&before);

        // A rule naming `CPU` names no GPU: the save lands.
        let m = manager_on(gpus());
        assert_eq!(
            m.write_file(&desktop, "models.ini", &ot("CPU"), false),
            Ok(()),
            "`blk\\.0\\. = CPU` names no GPU: the save lands on a `Devices = ROCm0` profile"
        );
        assert!(
            text_of("models.ini").contains(r"override-tensor = blk\.0\.=CPU"),
            "the rule landed: {:?}",
            text_of("models.ini")
        );

        // The assignment: the profile sits on disk naming ROCm1, `off`; the
        // `on` puts it in effect on the `ROCm0` instance.
        record("default");
        profile_with("default", "ROCm0", 8080);
        pool_files(&ot("ROCm1"), "");
        assignments("[default]\nqwen = off\n");
        let m = manager_on(gpus());
        assert!(
            m.render_failures().is_empty(),
            "the fixture's pair names no GPU outside: the write below is the news"
        );
        let before = snapshot(&WATCH);
        let answer = m
            .set_model_profile(&desktop, "default", "qwen", "on")
            .map(|_| ());
        assert!(
            outside(&m, &answer, "qwen", "default", "override-tensor", "ROCm1"),
            "an assignment that puts a profile naming ROCm1 on a `ROCm0` instance is reported: {answer:?}"
        );
        kept(&before);

        // The switch to a server profile that leaves the named card outside.
        record("wide");
        profile_with("wide", "ROCm0,ROCm1", 8080);
        profile_with("narrow", "ROCm0", 8090);
        pool_files(&ot("ROCm1"), "");
        assignments("[default]\nqwen = on\n");
        let m = manager_on(gpus());
        assert!(
            m.render_failures().is_empty(),
            "the fixture's pair names no GPU outside: the write below is the news"
        );
        let before = snapshot(&WATCH);
        let answer = m
            .set_server_profile(&desktop, "default", "narrow")
            .map(|_| ());
        assert!(
            outside(&m, &answer, "qwen", "default", "override-tensor", "ROCm1"),
            "a switch to a `ROCm0` profile leaves the `override-tensor` card outside, reported: \
             {answer:?}"
        );
        kept(&before);

        // A save narrowing an in-use server profile past it.
        record("default");
        profile_with("default", "ROCm0,ROCm1", 8080);
        pool_files(&ot("ROCm1"), "");
        assignments("[default]\nqwen = on\n");
        let m = manager_on(gpus());
        assert!(
            m.render_failures().is_empty(),
            "the fixture's pair names no GPU outside: the write below is the news"
        );
        let before = snapshot(&WATCH);
        let answer = m.write_file(
            &desktop,
            "server@default.ini",
            &server_text("ROCm0", 8080),
            false,
        );
        assert!(
            outside(&m, &answer, "qwen", "default", "override-tensor", "ROCm1"),
            "a save narrowing `Devices` past a card `override-tensor` names is reported: {answer:?}"
        );
        kept(&before);
    }
    // DB-PIN-12 END

    // ── DB-PIN-13 BEGIN. an `override-tensor` device already outside ──────
    //
    // As DB-PIN-8 for the device lists: a card the render names today that is
    // already outside the set is no news at the narrowing, so the save lands.
    {
        let ot_rocm2 =
            format!("[qwen]\nmodel = {path}\nctx-size = 8\noverride-tensor = blk\\.0\\.=ROCm2\n");
        record("default");
        profile_with("default", "ROCm0,CUDA1", 8080);
        pool_files(&ot_rocm2, "");
        assignments("[default]\nqwen = on\n");
        let m = manager_on(gpus());
        assert_eq!(
            m.write_file(
                &desktop,
                "server@default.ini",
                &server_text("ROCm0", 8080),
                false
            ),
            Ok(()),
            "ROCm2 is outside the set the instance runs today: the narrowing judges nothing"
        );
        assert!(
            text_of("server@default.ini").contains("Devices = ROCm0\n"),
            "the narrowing save landed"
        );
    }
    // DB-PIN-13 END

    // ── DB-PIN-14 BEGIN. an `-ot` device named as a host buffer ───────────
    //
    // llama.cpp v1.0.0 offers only each device's own buffer type, named as the
    // device, and refuses every other name, a pinned host buffer's included
    // (`common/arg.cpp` `parse_tensor_buffer_overrides`: "unknown buffer
    // type"), so the bound judges an `-ot` right side AS a device and fails
    // closed: on `Devices = ROCm0`, `CUDA_Host` is a name the set does not
    // carry, reported with the key and the device, the pair standing.
    {
        let host = format!(
            "[qwen]\nmodel = {path}\nctx-size = 8\noverride-tensor = blk\\.0\\.=CUDA_Host\n"
        );
        record("default");
        profile_with("default", "ROCm0", 8080);
        pool_files("", "");
        assignments("[default]\nqwen = on\n");
        let m = manager_on(gpus());
        assert!(
            m.render_failures().is_empty(),
            "the fixture's pair names no GPU outside: the write below is the news"
        );
        let before = snapshot(&WATCH);
        let answer = m.write_file(&desktop, "models.ini", &host, false);
        assert!(
            outside(
                &m,
                &answer,
                "qwen",
                "default",
                "override-tensor",
                "CUDA_Host"
            ),
            "an `override-tensor` device named `CUDA_Host` is no device the instance's server \
             profile names: reported naming the key and the device: {answer:?}"
        );
        kept(&before);
    }
    // DB-PIN-14 END

    // The phase leaves the tree as it found it.
    for entry in std::fs::read_dir(&sys).expect("listing").flatten() {
        let name = entry.file_name().to_string_lossy().into_owned();
        if !found.iter().any(|(found, _)| *found == name) {
            let _ = std::fs::remove_file(entry.path());
        }
    }
    for (name, text) in &found {
        std::fs::write(sys.join(name), text).expect("restore");
    }
}
/// The tags of an instance, the two lists apart: the `Tags` key of the
/// instance's own section of `instances.ini`, and the `Tags` key of the server
/// profile its section names. `Instances` answers both, beside the id, the
/// engine and the port, to any caller as the list of ids is; `SetInstanceTags`
/// replaces the record's own list, guarded as a write of a served file is:
/// `configure` first, the `As` form answered only for the bff's account and a
/// current member of `pai`, and one record of the action log per call, done or
/// refused. A list the rule refuses (`server-bad-tags`) answers before the
/// record is read and writes nothing, and so does an id that is no instance.
/// The lists are metadata: the manager passes none of them to the engine.
#[cfg(test)]
pub(crate) fn manager_instance_tags_e2e() {
    use fakes::{
        FakeActionLog, FakeAuthority, FakeGroup, FakePasswords, FakeResolver, FakeService,
    };

    let dir = tempfile::tempdir().expect("tempdir");
    crate::paths::redirect_for_tests(dir.path());
    let sys = crate::paths::engine_config_dir();
    std::fs::create_dir_all(&sys).expect("engine config dir");

    let bff = Caller(":1.9".into());
    let member = Requester {
        caller: bff.clone(),
        principal: Some(Principal("alice".into())),
    };
    let outsider = Requester {
        caller: bff.clone(),
        principal: Some(Principal("bob".into())),
    };
    let desktop = Caller(":1.42".into());
    let core = |allow: bool, passwords: FakePasswords, log: FakeActionLog| {
        Manager::new(
            FakeService::new(),
            FakeAuthority::new(allow).quietly(true).bff(&bff),
            FakeResolver::lan(),
        )
        .with_group(FakeGroup::new())
        .with_passwords(passwords)
        .with_log(log)
    };
    let text_of = |name: &str| std::fs::read_to_string(sys.join(name)).expect("read");
    let section =
        |text: &str, id: &str| pai_core::ini::text::section_text(text, id).unwrap_or_default();
    let keys_of = |text: &str, id: &str, key: &str| -> Vec<String> {
        section(text, id)
            .lines()
            .filter(|line| line.trim_start().starts_with(&format!("{key} ")))
            .map(|line| line.trim_end().to_string())
            .collect()
    };
    let listing = || -> Vec<String> {
        let mut names: Vec<String> = std::fs::read_dir(&sys)
            .map(|rd| {
                rd.flatten()
                    .map(|e| e.file_name().to_string_lossy().into_owned())
                    .collect()
            })
            .unwrap_or_default();
        names.sort();
        names
    };
    let records =
        |lines: &std::sync::Arc<std::sync::Mutex<Vec<String>>>| lines.lock().unwrap().clone();
    let tags = |list: &[&str]| -> Vec<String> { list.iter().map(|t| t.to_string()).collect() };
    let refused = |reason: &str, list: &str| -> Result<(), ManagerError> {
        Err(ManagerError::Invalid(pai_core::checks::Refusal {
            key: "server-bad-tags",
            values: vec![
                reason.to_string(),
                list.to_string(),
                pai_core::checks::TAGS_MAX.to_string(),
                pai_core::checks::TAG_MIN.to_string(),
                pai_core::checks::TAG_MAX.to_string(),
            ],
        }))
    };
    // The record: `default`'s profile carries the tags, `gpu-1` carries a list
    // of its own, hand-spaced, beside a comment and a key of its own, and
    // `lan-2` points at `default`'s profile: the row must carry that profile's
    // port and tags, never the ones of the file named after the instance.
    const SEEDED: &str = "# the manager's own record\n[default]\nServerProfile = default\n\n[gpu-1]\nServerProfile = gpu-1\nTags = gpu , edge \n; a note\nnote = keep me\n\n[lan-2]\nServerProfile = default\nTags = lan\n";
    std::fs::write(sys.join(INSTANCES_FILE), SEEDED).expect(INSTANCES_FILE);
    std::fs::write(
        sys.join("server@default.ini"),
        "[Server]\nPort = 8080\nHostname = 127.0.0.1\nTags = edge,v2\n",
    )
    .expect("default profile");
    std::fs::write(
        sys.join("server@gpu-1.ini"),
        "[Server]\nPort = 8081\nHostname = 127.0.0.1\n",
    )
    .expect("gpu-1 profile");
    // A file named after `lan-2`, holding a port and tags no instance runs:
    // a render of a profile the launch never reads, and no row of `Instances`.
    std::fs::write(
        sys.join("server@lan-2.ini"),
        "[Server]\nPort = 8099\nHostname = 127.0.0.1\nTags = unused,file\n",
    )
    .expect("lan-2 render");

    {
        let m = core(true, FakePasswords::new(), FakeActionLog::new());
        let rows = m.instances();
        assert_eq!(
            rows.iter().map(|r| r.id.as_str()).collect::<Vec<_>>(),
            vec!["default", "gpu-1", "lan-2"],
            "the record's order, every section of it"
        );
        assert_eq!(rows[0].engine, "llama.cpp");
        assert_eq!(
            rows[0].server_profile.as_deref(),
            Some("default"),
            "the server profile the section names, the name of the file the launch reads"
        );
        assert_eq!(
            rows[0].port, 8080,
            "the port of the profile the section names"
        );
        assert_eq!(
            rows[0].profile_tags,
            tags(&["edge", "v2"]),
            "the profile's list, in the order it was written"
        );
        assert_eq!(
            rows[0].instance_tags,
            Vec::<String>::new(),
            "a section with no Tags key"
        );
        assert_eq!(
            rows[1].profile_tags,
            Vec::<String>::new(),
            "a profile with no Tags key"
        );
        assert_eq!(
            rows[1].instance_tags,
            tags(&["gpu", "edge"]),
            "the hand-spaced list, trimmed, in order"
        );
        assert_eq!(rows[1].port, 8081);
        assert_eq!(
            rows[1].server_profile.as_deref(),
            Some("gpu-1"),
            "the profile of its own section"
        );
        assert_eq!(
            rows[2].id, "lan-2",
            "the record's order, every section of it"
        );
        assert_eq!(
            rows[2].server_profile.as_deref(),
            Some("default"),
            "the name the section names, which is not the instance's id: the \
             client that reads the server file of lan-2 reads server@default.ini, \
             and server@lan-2.ini is a file no instance runs"
        );
        assert_eq!(
            rows[2].port, 8080,
            "the port of the profile the section names, not the one of the file named after the instance"
        );
        assert_eq!(
            rows[2].profile_tags,
            tags(&["edge", "v2"]),
            "the tags of that profile, not the ones of the file named after the instance"
        );
        assert_eq!(
            rows[2].instance_tags,
            tags(&["lan"]),
            "the section's own list beside the profile's"
        );
    }

    {
        // A section naming a profile whose file is not there: the read of a
        // file that is not there is no tags and no port, not a failure that
        // drops the whole list, and never the default port: 0 is what the web
        // UI shows as "no port".
        std::fs::write(
            sys.join(INSTANCES_FILE),
            format!("{SEEDED}\n[lan-3]\nServerProfile = gone\n"),
        )
        .expect(INSTANCES_FILE);
        let m = core(true, FakePasswords::new(), FakeActionLog::new());
        let rows = m.instances();
        assert_eq!(rows.len(), 4, "the section of a missing profile is listed");
        assert_eq!(rows[3].id, "lan-3");
        assert_eq!(
            rows[3].server_profile.as_deref(),
            Some("gone"),
            "the name is the section's, and a missing file does not hide it: the \
             client is told where to write, and the file appears when it does"
        );
        assert_eq!(rows[3].profile_tags, Vec::<String>::new());
        assert_eq!(rows[3].port, 0, "no profile file is no port");
    }

    {
        // A section naming no profile at all: no name to give, answered as no
        // profile, not as a name invented from the id.
        std::fs::write(sys.join(INSTANCES_FILE), "[bare]\nPort = 1\n").expect(INSTANCES_FILE);
        let m = core(true, FakePasswords::new(), FakeActionLog::new());
        let rows = m.instances();
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].id, "bare");
        assert_eq!(
            rows[0].server_profile, None,
            "a section naming no profile names no file"
        );
        assert_eq!(rows[0].port, 0, "no profile is no port");
    }

    {
        std::fs::write(sys.join(INSTANCES_FILE), SEEDED).expect(INSTANCES_FILE);
        let log = FakeActionLog::new();
        let lines = log.lines.clone();
        let m = core(true, FakePasswords::new(), log);
        let before = listing();

        assert_eq!(
            m.set_instance_tags(&desktop, "default", &tags(&["edge", "a-1"])),
            Ok(()),
            "a list of the rule's own shape"
        );
        let written = text_of(INSTANCES_FILE);
        assert_eq!(
            keys_of(&written, "default", "Tags"),
            vec!["Tags = edge,a-1".to_string()],
            "one key, the list joined with a comma"
        );
        assert_eq!(
            section(&written, "gpu-1"),
            section(SEEDED, "gpu-1"),
            "the section of another instance, its comment and its own key, as they were: {written:?}"
        );
        assert_eq!(
            m.instances()[0].instance_tags,
            tags(&["edge", "a-1"]),
            "the write reads back, in order"
        );

        assert_eq!(
            m.set_instance_tags(&desktop, "default", &tags(&["x"])),
            Ok(())
        );
        assert_eq!(
            keys_of(&text_of(INSTANCES_FILE), "default", "Tags"),
            vec!["Tags = x".to_string()],
            "one key after two writes, not two"
        );

        // The empty list clears the record's tags: the key stays, with nothing
        // in it, and reads back as no tags.
        assert_eq!(m.set_instance_tags(&desktop, "default", &[]), Ok(()));
        assert_eq!(
            keys_of(&text_of(INSTANCES_FILE), "default", "Tags"),
            vec!["Tags =".to_string()],
            "an empty list keeps the key, empty"
        );
        assert_eq!(
            m.instances()[0].instance_tags,
            Vec::<String>::new(),
            "the empty key is no tags"
        );

        // The limits are inclusive: 8 tags, each 32.
        let at_limits: Vec<String> = (0..8).map(|i| format!("{i}{}", "t".repeat(31))).collect();
        assert_eq!(
            m.set_instance_tags(&desktop, "default", &at_limits),
            Ok(()),
            "the two limits at once"
        );
        assert_eq!(m.instances()[0].instance_tags, at_limits);
        assert_eq!(
            listing(),
            before,
            "the verb writes the record and no file of its own"
        );
        assert_eq!(
            records(&lines),
            vec![
                r#"action="set_instance_tags" target="default" caller=":1.42" principal=- outcome=ok"#
                    .to_string();
                4
            ],
            "one record per call: the verb's action, the instance as its target, the caller, no principal as a bare -, ok"
        );
    }

    {
        std::fs::write(sys.join(INSTANCES_FILE), SEEDED).expect(INSTANCES_FILE);
        let log = FakeActionLog::new();
        let lines = log.lines.clone();
        let m = core(true, FakePasswords::new(), log);
        let seed = text_of(INSTANCES_FILE);
        let before = listing();

        assert_eq!(
            m.set_instance_tags(&desktop, "lan-3", &tags(&["gpu"])),
            Err(ManagerError::NoInstance),
            "an id that is no section of the record"
        );
        for id in ["", "../x", "Default", "a_b"] {
            assert_eq!(
                m.set_instance_tags(&desktop, id, &tags(&["gpu"])),
                Err(ManagerError::InvalidId),
                "not an instance id at all: {id:?}"
            );
        }

        let long = "t".repeat(33);
        let many: Vec<String> = (0..9).map(|i| format!("t{i}")).collect();
        for (list, reason) in [
            (tags(&[""]), "empty tag"),
            (tags(&[&long]), "too long"),
            (tags(&["Edge"]), "bad characters"),
            (tags(&["a b"]), "bad characters"),
            (tags(&["ok", "ok"]), "the same tag twice"),
            (many, "too many tags"),
        ] {
            let joined = list.join(",");
            assert_eq!(
                m.set_instance_tags(&desktop, "default", &list),
                refused(reason, &joined),
                "{list:?}"
            );
        }
        assert_eq!(
            text_of(INSTANCES_FILE),
            seed,
            "every refusal above wrote nothing"
        );
        assert_eq!(listing(), before, "and created no file");
        let mut want = vec![
            r#"action="set_instance_tags" target="lan-3" caller=":1.42" principal=- outcome="NoInstance""#.to_string(),
            r#"action="set_instance_tags" target="" caller=":1.42" principal=- outcome="InvalidId""#.to_string(),
            r#"action="set_instance_tags" target="../x" caller=":1.42" principal=- outcome="InvalidId""#.to_string(),
            r#"action="set_instance_tags" target="Default" caller=":1.42" principal=- outcome="InvalidId""#.to_string(),
            r#"action="set_instance_tags" target="a_b" caller=":1.42" principal=- outcome="InvalidId""#.to_string(),
        ];
        want.extend(vec![
            r#"action="set_instance_tags" target="default" caller=":1.42" principal=- outcome="Invalid""#.to_string();
            6
        ]);
        assert_eq!(
            records(&lines),
            want,
            "one record per refused call, the refusals of the rule all as Invalid"
        );
    }

    {
        std::fs::write(sys.join(INSTANCES_FILE), SEEDED).expect(INSTANCES_FILE);
        let log = FakeActionLog::new();
        let lines = log.lines.clone();
        let m = core(false, FakePasswords::new(), log);
        let seed = text_of(INSTANCES_FILE);
        assert_eq!(
            m.set_instance_tags(&desktop, "default", &tags(&["gpu"])),
            Err(ManagerError::NotAuthorized),
            "a caller the authority refuses"
        );
        assert_eq!(text_of(INSTANCES_FILE), seed, "the refusal wrote nothing");
        assert_eq!(
            records(&lines),
            vec![
                r#"action="set_instance_tags" target="default" caller=":1.42" principal=- outcome=refused"#
                    .to_string()
            ],
            "the guard's refusal is recorded"
        );

        let m = core(true, FakePasswords::new(), FakeActionLog::new());
        assert_eq!(
            m.set_instance_tags(&member, "default", &tags(&["gpu"])),
            Ok(()),
            "the bff's account, for a member of `pai`"
        );
        assert_eq!(
            m.set_instance_tags(&outsider, "default", &tags(&["gpu"])),
            Err(ManagerError::NotAuthorized),
            "the bff's account, for a caller outside `pai`"
        );
        let vouching = Requester {
            caller: desktop.clone(),
            principal: Some(Principal("alice".into())),
        };
        assert_eq!(
            m.set_instance_tags(&vouching, "default", &tags(&["gpu"])),
            Err(ManagerError::NotAuthorized),
            "a caller that is not the bff vouches for nobody"
        );
        assert_eq!(
            keys_of(&text_of(INSTANCES_FILE), "default", "Tags"),
            vec!["Tags = gpu".to_string()],
            "only the accepted call wrote"
        );
    }
}
/// The label of an instance, the `Label` key of its own section of
/// `instances.ini` (IL1): the text the web UI shows beside the id. `Instances`
/// answers it beside the id, the engine and the port, to any caller as the list
/// of ids is; `SetInstanceLabel` writes the record, guarded as a write of a
/// served file is: `configure` first, the `As` form answered only for the bff's
/// account and a current member of `pai`, and one record of the action log per
/// call, done or refused. The text is trimmed by `checks::instance_label`, so
/// the file holds no hanging space; the empty text, after the trim, REMOVES the
/// key (never an empty key), and the row answers no label. A label another
/// instance of the node carries, ignoring letter case, is refused
/// `instance-label-taken`. A label is metadata: the manager passes nothing to
/// the engine, so a running instance is relabelled without a stop.
#[cfg(test)]
pub(crate) fn manager_instance_label_e2e() {
    use fakes::{
        FakeActionLog, FakeAuthority, FakeGroup, FakePasswords, FakeResolver, FakeService,
    };

    let dir = tempfile::tempdir().expect("tempdir");
    crate::paths::redirect_for_tests(dir.path());
    let sys = crate::paths::engine_config_dir();
    std::fs::create_dir_all(&sys).expect("engine config dir");

    let bff = Caller(":1.9".into());
    let member = Requester {
        caller: bff.clone(),
        principal: Some(Principal("alice".into())),
    };
    let outsider = Requester {
        caller: bff.clone(),
        principal: Some(Principal("bob".into())),
    };
    let desktop = Caller(":1.42".into());
    let core = |allow: bool, passwords: FakePasswords, log: FakeActionLog| {
        Manager::new(
            FakeService::new(),
            FakeAuthority::new(allow).quietly(true).bff(&bff),
            FakeResolver::lan(),
        )
        .with_group(FakeGroup::new())
        .with_passwords(passwords)
        .with_log(log)
    };
    let text_of = |name: &str| std::fs::read_to_string(sys.join(name)).expect("read");
    let section =
        |text: &str, id: &str| pai_core::ini::text::section_text(text, id).unwrap_or_default();
    let keys_of = |text: &str, id: &str, key: &str| -> Vec<String> {
        section(text, id)
            .lines()
            .filter(|line| line.trim_start().starts_with(&format!("{key} ")))
            .map(|line| line.trim_end().to_string())
            .collect()
    };
    // Every line of the section that mentions the key, in any spacing, so a
    // cleared key shows up here and not only as an empty `keys_of`.
    let lines_with = |text: &str, id: &str, key: &str| -> Vec<String> {
        section(text, id)
            .lines()
            .filter(|line| line.trim_start().starts_with(key))
            .map(|line| line.trim_end().to_string())
            .collect()
    };
    let listing = || -> Vec<String> {
        let mut names: Vec<String> = std::fs::read_dir(&sys)
            .map(|rd| {
                rd.flatten()
                    .map(|e| e.file_name().to_string_lossy().into_owned())
                    .collect()
            })
            .unwrap_or_default();
        names.sort();
        names
    };
    let records =
        |lines: &std::sync::Arc<std::sync::Mutex<Vec<String>>>| lines.lock().unwrap().clone();
    let refused = |key: &'static str, values: &[&str]| -> Result<(), ManagerError> {
        Err(ManagerError::Invalid(pai_core::checks::Refusal {
            key,
            values: values.iter().map(|v| v.to_string()).collect(),
        }))
    };
    // The record: `default` has no label, `gpu-1` carries one beside a
    // hand-spaced tags list, a comment and a key of its own, and `lan-2` points
    // at `default`'s profile.
    const SEEDED: &str = "# the manager's own record\n[default]\nServerProfile = default\n\n[gpu-1]\nServerProfile = gpu-1\nLabel = GPU box\nTags = gpu , edge \n; a note\nnote = keep me\n\n[lan-2]\nServerProfile = default\nTags = lan\n";
    std::fs::write(sys.join(INSTANCES_FILE), SEEDED).expect(INSTANCES_FILE);
    std::fs::write(
        sys.join("server@default.ini"),
        "[Server]\nPort = 8080\nHostname = 127.0.0.1\nTags = edge,v2\n",
    )
    .expect("default profile");
    std::fs::write(
        sys.join("server@gpu-1.ini"),
        "[Server]\nPort = 8081\nHostname = 127.0.0.1\n",
    )
    .expect("gpu-1 profile");

    {
        let m = core(true, FakePasswords::new(), FakeActionLog::new());
        let rows = m.instances();
        assert_eq!(
            rows.iter().map(|r| r.id.as_str()).collect::<Vec<_>>(),
            vec!["default", "gpu-1", "lan-2"]
        );
        assert_eq!(
            rows[0].label, None,
            "a section with no Label key is no label, not an empty text"
        );
        assert_eq!(
            rows[1].label.as_deref(),
            Some("GPU box"),
            "the key of the section, as it was written"
        );
        assert_eq!(
            rows[2].label, None,
            "a section of another instance is untouched by its neighbour's key"
        );
        assert_eq!(
            (rows[1].engine.as_str(), rows[1].port),
            ("llama.cpp", 8081),
            "the engine and the port travel beside the label: the UI falls back \
             to them, in one read, when there is no label"
        );
    }

    {
        std::fs::write(sys.join(INSTANCES_FILE), SEEDED).expect(INSTANCES_FILE);
        let log = FakeActionLog::new();
        let lines = log.lines.clone();
        let m = core(true, FakePasswords::new(), log);
        let before = listing();

        assert_eq!(
            m.set_instance_label(&desktop, "default", "  Coding box "),
            Ok(()),
            "a text with hanging space"
        );
        let written = text_of(INSTANCES_FILE);
        assert_eq!(
            keys_of(&written, "default", "Label"),
            vec!["Label = Coding box".to_string()],
            "one key, trimmed at both ends"
        );
        assert_eq!(
            written,
            SEEDED.replacen(
                "[default]\nServerProfile = default\n",
                "[default]\nServerProfile = default\nLabel = Coding box\n",
                1
            ),
            "every other byte of the record kept, the comment and the other sections included"
        );
        assert_eq!(
            m.instances()[0].label.as_deref(),
            Some("Coding box"),
            "the write reads back"
        );

        assert_eq!(m.set_instance_label(&desktop, "default", "Box two"), Ok(()));
        assert_eq!(
            keys_of(&text_of(INSTANCES_FILE), "default", "Label"),
            vec!["Label = Box two".to_string()],
            "one key after two writes, not two"
        );

        // The limits are inclusive: 64 characters, and the spaces of the ends.
        let at_limit = "z".repeat(pai_core::checks::LABEL_MAX);
        assert_eq!(
            m.set_instance_label(&desktop, "default", &format!(" {at_limit} ")),
            Ok(()),
            "64 characters, trimmed"
        );
        assert_eq!(m.instances()[0].label.as_deref(), Some(at_limit.as_str()));

        // The empty text removes the key: the reader sees no label.
        assert_eq!(m.set_instance_label(&desktop, "default", ""), Ok(()));
        assert_eq!(
            lines_with(&text_of(INSTANCES_FILE), "default", "Label"),
            Vec::<String>::new(),
            "the line is gone, not left empty"
        );
        assert_eq!(m.instances()[0].label, None, "the removed key is no label");
        // Clearing a section that has no label is accepted and writes nothing.
        let cleared = text_of(INSTANCES_FILE);
        assert_eq!(m.set_instance_label(&desktop, "default", "   "), Ok(()));
        assert_eq!(
            text_of(INSTANCES_FILE),
            cleared,
            "a clear of a section with no label leaves the record as it is"
        );
        assert_eq!(
            listing(),
            before,
            "the verb writes the record and no file of its own"
        );
        assert_eq!(
            m.service().calls.lock().unwrap().clone(),
            Vec::<fakes::Call>::new(),
            "the label is nothing the engine reads: a relabel does not stop, \
             start or restart the instance, which is running the whole time"
        );
        assert_eq!(
            records(&lines),
            vec![
                r#"action="set_instance_label" target="default" caller=":1.42" principal=- outcome=ok"#
                    .to_string();
                5
            ],
            "one record per call: the verb's action, the instance as its target, the caller, no principal as a bare -, ok"
        );
    }

    {
        std::fs::write(sys.join(INSTANCES_FILE), SEEDED).expect(INSTANCES_FILE);
        let log = FakeActionLog::new();
        let lines = log.lines.clone();
        let m = core(true, FakePasswords::new(), log);
        let seed = text_of(INSTANCES_FILE);
        let before = listing();

        assert_eq!(
            m.set_instance_label(&desktop, "lan-3", "Coding box"),
            Err(ManagerError::NoInstance),
            "an id that is no section of the record"
        );
        for id in ["", "../x", "Default", "a_b"] {
            assert_eq!(
                m.set_instance_label(&desktop, id, "Coding box"),
                Err(ManagerError::InvalidId),
                "not an instance id at all: {id:?}"
            );
        }

        let long = "z".repeat(pai_core::checks::LABEL_MAX + 1);
        for text in [
            long.as_str(),
            "two\nlines",
            "tab\there",
            "nul\0end",
            "bell\u{7}",
        ] {
            assert_eq!(
                m.set_instance_label(&desktop, "default", text),
                refused("instance-bad-label", &[text]),
                "{text:?}"
            );
        }
        for text in ["a;b", "Coding #1", "#leading", ";"] {
            assert_eq!(
                m.set_instance_label(&desktop, "default", text),
                refused("comment-markers", &[text]),
                "{text:?}"
            );
        }
        // Unique on the node, ignoring letter case: the label of another
        // instance is refused, and the refusal names who holds it.
        for text in ["GPU BOX", "gpu box", "  GPU box  "] {
            let trimmed = text.trim();
            assert_eq!(
                m.set_instance_label(&desktop, "default", text),
                refused("instance-label-taken", &[trimmed, "gpu-1"]),
                "{text:?}"
            );
        }
        assert_eq!(
            m.set_instance_label(&desktop, "gpu-1", "GPU box"),
            Ok(()),
            "an instance already holding the label is writing it to itself, not a clash"
        );
        assert_eq!(
            text_of(INSTANCES_FILE),
            seed,
            "every refusal above wrote nothing, and the accepted relabel of gpu-1 \
             wrote the text the section already had"
        );
        assert_eq!(listing(), before, "and created no file");
        let mut want = vec![
            r#"action="set_instance_label" target="lan-3" caller=":1.42" principal=- outcome="NoInstance""#.to_string(),
            r#"action="set_instance_label" target="" caller=":1.42" principal=- outcome="InvalidId""#.to_string(),
            r#"action="set_instance_label" target="../x" caller=":1.42" principal=- outcome="InvalidId""#.to_string(),
            r#"action="set_instance_label" target="Default" caller=":1.42" principal=- outcome="InvalidId""#.to_string(),
            r#"action="set_instance_label" target="a_b" caller=":1.42" principal=- outcome="InvalidId""#.to_string(),
        ];
        want.extend(
            vec![
                r#"action="set_instance_label" target="default" caller=":1.42" principal=- outcome="Invalid""#.to_string();
            12
            ],
        );
        want.push(
            r#"action="set_instance_label" target="gpu-1" caller=":1.42" principal=- outcome=ok"#
                .to_string(),
        );
        assert_eq!(
            records(&lines),
            want,
            "one record per refused call, the refusals of the rule all as Invalid"
        );
    }

    // (IL1 review D1) The label is unique among OTHER instances. The section
    // writing the text is never the one the rule compares against: an instance
    // changing only the letter case of its own label relabels itself, it does
    // not clash with the text it already carries.
    {
        std::fs::write(sys.join(INSTANCES_FILE), SEEDED).expect(INSTANCES_FILE);
        let log = FakeActionLog::new();
        let lines = log.lines.clone();
        let m = core(true, FakePasswords::new(), log);
        let before = listing();
        let relabelled = SEEDED.replacen("Label = GPU box", "Label = GPU BOX", 1);

        assert_eq!(
            m.set_instance_label(&desktop, "gpu-1", "GPU BOX"),
            Ok(()),
            "gpu-1 carries `GPU box`: the same text in another case is its own \
             label, not a label another instance holds"
        );
        assert_eq!(
            keys_of(&text_of(INSTANCES_FILE), "gpu-1", "Label"),
            vec!["Label = GPU BOX".to_string()],
            "the file holds the new case, on one key"
        );
        assert_eq!(
            text_of(INSTANCES_FILE),
            relabelled,
            "every other byte of the record kept"
        );
        assert_eq!(
            m.instances()[1].label.as_deref(),
            Some("GPU BOX"),
            "the new case reads back"
        );
        assert_eq!(
            m.set_instance_label(&desktop, "gpu-1", "gpu BOX"),
            Ok(()),
            "again only the case: a second case-only relabel of the same instance"
        );
        // Skipping the writer's own section takes nothing off the rule: the new
        // case of gpu-1 is still taken for every OTHER instance.
        for text in ["gpu box", "GPU BOX", "  Gpu Box  "] {
            let trimmed = text.trim();
            assert_eq!(
                m.set_instance_label(&desktop, "default", text),
                refused("instance-label-taken", &[trimmed, "gpu-1"]),
                "{text:?}: taken by gpu-1, ignoring case, and the refusal names it"
            );
        }
        assert_eq!(
            m.set_instance_label(&desktop, "lan-2", "gpu BOX"),
            refused("instance-label-taken", &["gpu BOX", "gpu-1"]),
            "and not only for the instance that held the label first"
        );
        assert_eq!(
            text_of(INSTANCES_FILE),
            SEEDED.replacen("Label = GPU box", "Label = gpu BOX", 1),
            "the refusals wrote nothing, the accepted relabel is the only change"
        );
        assert_eq!(
            listing(),
            before,
            "the verb writes the record and no file of its own"
        );
        assert_eq!(
            records(&lines),
            {
                let mut want = vec![
                    r#"action="set_instance_label" target="gpu-1" caller=":1.42" principal=- outcome=ok"#
                        .to_string();
                    2
                ];
                want.extend(vec![
                    r#"action="set_instance_label" target="default" caller=":1.42" principal=- outcome="Invalid""#.to_string();
                    3
                ]);
                want.push(
                    r#"action="set_instance_label" target="lan-2" caller=":1.42" principal=- outcome="Invalid""#.to_string(),
                );
                want
            },
            "one record per call: the two accepted relabels ok, every refusal Invalid"
        );
    }

    // (IL1 review D2) The trim of the verb is the trim of the rule: spaces and
    // tabs at the ends, nothing else (`checks::instance_label`). A text whose
    // ends carry a non-breaking space, or another Unicode space, is a label the
    // rule accepts: it lands in the file exactly as the rule saw it, or the verb
    // refuses it. What never happens is a silent trim by a wider rule, which
    // writes, and clears, a text the check never saw.
    {
        // The file's line byte for byte: `keys_of` trims the line's end, and
        // U+00A0 is whitespace for `str::trim_end`.
        let raw_key = |text: &str, id: &str, key: &str| -> Vec<String> {
            section(text, id)
                .lines()
                .filter(|line| line.trim_start().starts_with(&format!("{key} ")))
                .map(|line| line.to_string())
                .collect()
        };
        for text in [
            "\u{a0}Coding box",
            "Coding box\u{a0}",
            " \u{3000}Coding box \u{a0} ",
            "\u{a0}",
            "\u{205f}\u{3000}",
        ] {
            std::fs::write(sys.join(INSTANCES_FILE), SEEDED).expect(INSTANCES_FILE);
            let log = FakeActionLog::new();
            let lines = log.lines.clone();
            let m = core(true, FakePasswords::new(), log);
            let seed = text_of(INSTANCES_FILE);
            // The text the rule sees, and accepts: no control character and no
            // comment marker, so a refusal here is the verb's own choice.
            let checked = text.trim_matches([' ', '\t']);
            assert!(
                pai_core::checks::instance_label(text).is_ok(),
                "{text:?}: the rule accepts this text"
            );
            let got = m.set_instance_label(&desktop, "default", text);
            let written = text_of(INSTANCES_FILE);
            match got {
                Ok(()) => assert_eq!(
                    raw_key(&written, "default", "Label"),
                    vec![format!("Label = {checked}")],
                    "{text:?}: the accepted text lands as the rule saw it, spaces \
                     and tabs trimmed at the ends and nothing else"
                ),
                Err(ManagerError::Invalid(_)) => {
                    assert_eq!(written, seed, "{text:?}: a refusal wrote nothing");
                }
                Err(other) => panic!("{text:?}: {other:?}, the rule's refusal or nothing"),
            }
            assert_eq!(
                records(&lines).len(),
                1,
                "one record for the call, {text:?}"
            );
        }
    }

    {
        std::fs::write(sys.join(INSTANCES_FILE), SEEDED).expect(INSTANCES_FILE);
        let log = FakeActionLog::new();
        let lines = log.lines.clone();
        let m = core(false, FakePasswords::new(), log);
        let seed = text_of(INSTANCES_FILE);
        assert_eq!(
            m.set_instance_label(&desktop, "default", "Coding box"),
            Err(ManagerError::NotAuthorized),
            "a caller the authority refuses"
        );
        assert_eq!(text_of(INSTANCES_FILE), seed, "the refusal wrote nothing");
        assert_eq!(
            records(&lines),
            vec![
                r#"action="set_instance_label" target="default" caller=":1.42" principal=- outcome=refused"#
                    .to_string()
            ],
            "the guard's refusal is recorded"
        );

        let m = core(true, FakePasswords::new(), FakeActionLog::new());
        assert_eq!(
            m.set_instance_label(&member, "default", "Coding box"),
            Ok(()),
            "the bff's account, for a member of `pai`"
        );
        assert_eq!(
            m.set_instance_label(&outsider, "default", "Coding box"),
            Err(ManagerError::NotAuthorized),
            "the bff's account, for a caller outside `pai`"
        );
        let vouching = Requester {
            caller: desktop.clone(),
            principal: Some(Principal("alice".into())),
        };
        assert_eq!(
            m.set_instance_label(&vouching, "default", "Coding box"),
            Err(ManagerError::NotAuthorized),
            "a caller that is not the bff vouches for nobody"
        );
        assert_eq!(
            keys_of(&text_of(INSTANCES_FILE), "default", "Label"),
            vec!["Label = Coding box".to_string()],
            "only the accepted call wrote"
        );
    }
}
/// Creating and deleting the node's server profiles, the files beside the
/// instances (ARCHITECTURE.md, slice 14). `CreateServerProfile(engine, profile,
/// port)` writes `server@<profile>.ini` the way a save writes it (`render_for`
/// with the directories left unset, mode 0640, group [`INSTANCE_GROUP`], port 0
/// no port), and overwrites nothing: any entry already at that name, a dangling
/// link included (the check is `symlink_metadata`), is the new
/// `ServerProfileExists`. `DeleteServerProfile` removes a file no section of
/// `instances.ini` names, refuses one a section names (`ServerProfileInUse`,
/// naming it) and one that is no file (`NoServerProfile`, a name the id rule
/// refuses included), and answers the record read's own `Io` rather than
/// "unused" when `instances.ini` cannot be read. `ServerProfiles` lists the
/// files, sorted, to any caller as `Instances` does: id, engine, port, tags and
/// the first section naming it, and nothing of the rest of the file. Both verbs
/// are guarded as a write of a served file is: `configure` first, the `As` form
/// answered only for the bff's account and a current member of `pai`, and one
/// record of the action log per call, done or refused.
#[cfg(test)]
pub(crate) fn manager_server_profile_crud_e2e() {
    use fakes::{
        FakeActionLog, FakeAuthority, FakeGroup, FakePasswords, FakeResolver, FakeService,
    };

    let dir = tempfile::tempdir().expect("tempdir");
    crate::paths::redirect_for_tests(dir.path());
    let sys = crate::paths::engine_config_dir();
    std::fs::create_dir_all(&sys).expect("engine config dir");

    let bff = Caller(":1.9".into());
    let member = Requester {
        caller: bff.clone(),
        principal: Some(Principal("alice".into())),
    };
    let outsider = Requester {
        caller: bff.clone(),
        principal: Some(Principal("bob".into())),
    };
    let desktop = Caller(":1.42".into());
    let core_in = |allow: bool, group: FakeGroup, log: FakeActionLog| {
        Manager::new(
            FakeService::new(),
            FakeAuthority::new(allow).quietly(true).bff(&bff),
            FakeResolver::lan(),
        )
        .with_group(group)
        .with_passwords(FakePasswords::new())
        .with_log(log)
    };
    let core = |allow: bool, log: FakeActionLog| core_in(allow, FakeGroup::new(), log);
    let profile_path = |name: &str| sys.join(format!("server@{name}.ini"));
    let profile = |name: &str, text: &str| {
        std::fs::write(profile_path(name), text).expect("server profile");
    };
    let text_of = |name: &str| std::fs::read_to_string(sys.join(name)).expect("read");
    let bytes_of = |name: &str| std::fs::read(sys.join(name)).expect("bytes");
    let section =
        |text: &str, id: &str| pai_core::ini::text::section_text(text, id).unwrap_or_default();
    let keys_of = |text: &str, id: &str, key: &str| -> Vec<String> {
        section(text, id)
            .lines()
            .filter(|line| line.trim_start().starts_with(&format!("{key} ")))
            .map(|line| line.trim_end().to_string())
            .collect()
    };
    let listing = || -> Vec<String> {
        let mut names: Vec<String> = std::fs::read_dir(&sys)
            .map(|rd| {
                rd.flatten()
                    .map(|e| e.file_name().to_string_lossy().into_owned())
                    .collect()
            })
            .unwrap_or_default();
        names.sort();
        names
    };
    let records =
        |lines: &std::sync::Arc<std::sync::Mutex<Vec<String>>>| lines.lock().unwrap().clone();
    let tags = |list: &[&str]| -> Vec<String> { list.iter().map(|t| t.to_string()).collect() };
    // The file a create is meant to write: the same render a save of a server
    // config with that port writes, its directories left unset.
    let render = |port: Option<i32>| -> String {
        pai_core::server_cfg::render_for(
            &pai_core::server_cfg::ServerConfig {
                port,
                ..Default::default()
            },
            true,
        )
    };
    let exists = |name: &str| -> Result<(), ManagerError> {
        Err(ManagerError::ServerProfileExists(name.to_string()))
    };
    let no_profile = |name: &str| -> Result<(), ManagerError> {
        Err(ManagerError::NoServerProfile(name.to_string()))
    };
    let in_use = |name: &str, id: &str| -> Result<(), ManagerError> {
        Err(ManagerError::ServerProfileInUse(
            name.to_string(),
            id.to_string(),
        ))
    };
    let refused = |key: &'static str, value: &str| -> Result<(), ManagerError> {
        Err(ManagerError::Invalid(pai_core::checks::Refusal {
            key,
            values: vec![value.to_string()],
        }))
    };

    // The record: `default` and `lan-2` name a profile of their own, `lan-3`
    // names a file that is not there (a section is no row of the list of files).
    const SEEDED: &str = "# the manager's own record\n[default]\nServerProfile = default\n; a note\nnote = keep me\n\n[lan-2]\nServerProfile = lan-2\n\n[lan-3]\nServerProfile = gone\n";
    std::fs::write(sys.join(INSTANCES_FILE), SEEDED).expect(INSTANCES_FILE);
    profile(
        "default",
        "[Server]\nPort = 8080\nHostname = 127.0.0.1\nTags = edge,v2\n",
    );
    profile("lan-2", "[Server]\nPort = 8081\nHostname = 127.0.0.1\n");
    profile("noport", "[Server]\nHostname = 127.0.0.1\n");
    profile(
        "secret",
        "[Server]\nPort = 8090\nOpencodeApiKey = sk-local-3f9a\n",
    );
    // A file whose name the engine refuses, and a file that is no profile at
    // all: neither is a row of the list.
    std::fs::write(sys.join("server@X.ini"), "[Server]\nPort = 1\n").expect("bad name");
    std::fs::write(sys.join("stray.ini"), "[Server]\nPort = 2\n").expect("stray");

    // ── The list ──────────────────────────────────────────────────────────
    {
        let log = FakeActionLog::new();
        let lines = log.lines.clone();
        let m = core(true, log);
        let rows = m.server_profiles();
        assert_eq!(
            rows.iter().map(|r| r.id.as_str()).collect::<Vec<_>>(),
            vec!["default", "lan-2", "noport", "secret"],
            "the files whose name is a profile, sorted: `gone` is a section with \
             no file, `server@X.ini` a name the engine refuses, `stray.ini` no profile"
        );
        assert_eq!(
            rows[0].engine, "llama.cpp",
            "the one engine the manager serves"
        );
        assert_eq!(rows[0].port, 8080);
        assert_eq!(
            rows[0].tags,
            tags(&["edge", "v2"]),
            "the profile's own tags"
        );
        assert_eq!(
            rows[0].instance.as_deref(),
            Some("default"),
            "the section that names it"
        );
        assert_eq!(
            rows[1].tags,
            Vec::<String>::new(),
            "a file with no Tags key"
        );
        assert_eq!(
            rows[2].port,
            u16::try_from(pai_core::server_cfg::DEFAULT_PORT).expect("DEFAULT_PORT"),
            "a file with no Port is the default port, as Instances reads it"
        );
        assert_eq!(
            rows[2].instance, None,
            "a profile no section names serves no instance"
        );
        let line = format!("{rows:?}");
        assert!(
            !line.contains("sk-local-3f9a"),
            "the row carries the port and the tags, and nothing else of the file: {line}"
        );
        assert_eq!(
            records(&lines),
            Vec::<String>::new(),
            "the list is a read: no action record"
        );

        let m = core(false, FakeActionLog::new());
        assert_eq!(
            m.server_profiles().len(),
            4,
            "a caller the authority refuses reads the list as any other: the list is \
             the class of data Instances answers to anyone"
        );
    }

    // ── A create writes the file a save writes ─────────────────────────────
    {
        let log = FakeActionLog::new();
        let lines = log.lines.clone();
        let group = FakeGroup::new();
        let calls = group.calls.clone();
        let m = core_in(true, group, log);
        let before = listing();
        let seed = text_of(INSTANCES_FILE);
        let default_bytes = bytes_of("server@default.ini");

        assert_eq!(
            m.create_server_profile(&desktop, "llama.cpp", "big", Some(8123)),
            Ok(()),
            "a name that is nothing yet, of the engine the manager serves"
        );
        let written = text_of("server@big.ini");
        assert_eq!(
            written,
            render(Some(8123)),
            "the file a save of a server config with that port writes, directories \
             left unset, and nothing beside it"
        );
        assert_eq!(
            keys_of(&written, "Server", "Port"),
            vec!["Port = 8123".to_string()],
            "the port of the call, as one key"
        );
        assert_eq!(
            text_of(INSTANCES_FILE),
            seed,
            "a create writes no instance record: the file serves nothing until a section names it"
        );
        let mut want = before.clone();
        want.push("server@big.ini".to_string());
        want.sort();
        assert_eq!(listing(), want, "one new file, and no temp file left");
        let g = calls.lock().unwrap().clone();
        assert_eq!(g.len(), 1, "the group set once, on the way in");
        assert_eq!(g[0].group, INSTANCE_GROUP, "the group of a served file");
        assert_eq!(
            g[0].path.file_name().and_then(|n| n.to_str()),
            Some("server@big.ini.tmp"),
            "the group is set on the temp file, before the rename"
        );
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            assert_eq!(
                std::fs::metadata(profile_path("big"))
                    .expect("metadata")
                    .permissions()
                    .mode()
                    & 0o7777,
                0o640,
                "the mode of a served profile"
            );
            assert_eq!(
                g[0].mode, 0o640,
                "the temp file had the mode when its group was set: the key of a \
                 profile is never world readable"
            );
        }
        assert_eq!(
            m.server_profiles()
                .iter()
                .find(|r| r.id == "big")
                .map(|r| (r.port, r.instance.clone())),
            Some((8123u16, None)),
            "the new file is a row, and an unused one"
        );

        assert_eq!(
            m.create_server_profile(&desktop, "llama.cpp", "zero", None),
            Ok(()),
            "port 0 on the wire is no port"
        );
        assert_eq!(
            text_of("server@zero.ini"),
            render(None),
            "the file of no port"
        );
        assert_eq!(
            keys_of(&text_of("server@zero.ini"), "Server", "Port"),
            Vec::<String>::new(),
            "no Port key at all"
        );

        // The never-overwrite: a name that is anything already is refused.
        assert_eq!(
            m.create_server_profile(&desktop, "llama.cpp", "big", Some(9001)),
            exists("big"),
            "a second create of the same name"
        );
        assert_eq!(
            m.create_server_profile(&desktop, "llama.cpp", "default", Some(9001)),
            exists("default"),
            "a name that is an existing profile with other bytes"
        );
        assert_eq!(
            bytes_of("server@default.ini"),
            default_bytes,
            "the bytes of the file that was there stay"
        );
        #[cfg(unix)]
        {
            std::os::unix::fs::symlink(sys.join("nowhere.ini"), profile_path("ghost"))
                .expect("symlink");
            assert_eq!(
                m.create_server_profile(&desktop, "llama.cpp", "ghost", Some(8123)),
                exists("ghost"),
                "a dangling link is an entry: the create would write through it"
            );
            assert!(
                !sys.join("nowhere.ini").exists(),
                "and the link's target was not created"
            );
            assert!(
                std::fs::symlink_metadata(profile_path("ghost"))
                    .expect("symlink_metadata")
                    .file_type()
                    .is_symlink(),
                "the link itself is still a link"
            );
        }

        // The `mut` is only used on unix, where the dangling link's record is
        // pushed.
        #[cfg_attr(not(unix), allow(unused_mut))]
        let mut want = vec![
            r#"action="create_server_profile" target="big" caller=":1.42" principal=- outcome=ok"#
                .to_string(),
            r#"action="create_server_profile" target="zero" caller=":1.42" principal=- outcome=ok"#
                .to_string(),
            r#"action="create_server_profile" target="big" caller=":1.42" principal=- outcome="ServerProfileExists""#.to_string(),
            r#"action="create_server_profile" target="default" caller=":1.42" principal=- outcome="ServerProfileExists""#.to_string(),
        ];
        #[cfg(unix)]
        want.push(
            r#"action="create_server_profile" target="ghost" caller=":1.42" principal=- outcome="ServerProfileExists""#
                .to_string(),
        );
        assert_eq!(
            records(&lines),
            want,
            "one record per call, the profile as its target"
        );
    }

    // ── A create the rules refuses writes nothing, anywhere ───────────────
    {
        let log = FakeActionLog::new();
        let lines = log.lines.clone();
        let m = core(true, log);
        let before = listing();
        let seed = text_of(INSTANCES_FILE);
        let long = "t".repeat(33);
        for (engine, name) in [
            ("vllm", "fine-name"),
            ("llama.cpp", "../x"),
            ("llama.cpp", "X"),
            ("llama.cpp", "a_b"),
            ("llama.cpp", ""),
            ("llama.cpp", long.as_str()),
        ] {
            let want = if engine != "llama.cpp" {
                refused("unknown-engine", engine)
            } else {
                refused("server-profile-bad-id", name)
            };
            assert_eq!(
                m.create_server_profile(&desktop, engine, name, Some(8123)),
                want,
                "create of engine {engine:?}, profile {name:?}"
            );
        }
        assert_eq!(
            text_of(INSTANCES_FILE),
            seed,
            "a refused create writes no record"
        );
        assert_eq!(listing(), before, "and creates nothing, anywhere");
        assert!(
            !sys.join("x.ini").exists() && !dir.path().join("x.ini").exists(),
            "the name rule holds: `../x` builds no path outside the engine's directory"
        );
        let want = vec![
            r#"action="create_server_profile" target="fine-name" caller=":1.42" principal=- outcome="Invalid""#.to_string(),
            r#"action="create_server_profile" target="../x" caller=":1.42" principal=- outcome="Invalid""#.to_string(),
            r#"action="create_server_profile" target="X" caller=":1.42" principal=- outcome="Invalid""#.to_string(),
            r#"action="create_server_profile" target="a_b" caller=":1.42" principal=- outcome="Invalid""#.to_string(),
            r#"action="create_server_profile" target="" caller=":1.42" principal=- outcome="Invalid""#.to_string(),
            format!(
                r#"action="create_server_profile" target="{long}" caller=":1.42" principal=- outcome="Invalid""#
            ),
        ];
        assert_eq!(
            records(&lines),
            want,
            "one record per refused call, both rules as Invalid"
        );
    }

    // ── A delete: unused goes, in use stays ───────────────────────────────
    {
        let log = FakeActionLog::new();
        let lines = log.lines.clone();
        let m = core(true, log);
        let default_bytes = bytes_of("server@default.ini");

        assert_eq!(
            m.delete_server_profile(&desktop, "llama.cpp", "default"),
            in_use("default", "default"),
            "the instance whose section names it"
        );
        assert_eq!(
            bytes_of("server@default.ini"),
            default_bytes,
            "the file of an in-use profile stays, byte for byte"
        );
        assert_eq!(
            m.delete_server_profile(&desktop, "llama.cpp", "lan-2"),
            in_use("lan-2", "lan-2"),
            "the same for the other instance"
        );
        assert_eq!(
            m.delete_server_profile(&desktop, "llama.cpp", "noport"),
            Ok(()),
            "a profile no section names"
        );
        assert!(!profile_path("noport").exists(), "its file is gone");
        assert_eq!(
            m.delete_server_profile(&desktop, "llama.cpp", "gone"),
            no_profile("gone"),
            "a section naming a file that is not there: the refusal is about the file"
        );
        for name in ["nope", "../x", "X", "a_b", ""] {
            assert_eq!(
                m.delete_server_profile(&desktop, "llama.cpp", name),
                no_profile(name),
                "a name with no file, and a name the rule refuses, answer the same: {name:?}"
            );
        }
        assert!(
            profile_path("X").exists(),
            "a name the rule refuses is refused before the file is looked at"
        );
        assert_eq!(
            text_of(INSTANCES_FILE),
            SEEDED,
            "a delete writes no record: a section is left naming a file, never the other way"
        );
        let mut want = vec![
            r#"action="delete_server_profile" target="default" caller=":1.42" principal=- outcome="ServerProfileInUse""#.to_string(),
            r#"action="delete_server_profile" target="lan-2" caller=":1.42" principal=- outcome="ServerProfileInUse""#.to_string(),
            r#"action="delete_server_profile" target="noport" caller=":1.42" principal=- outcome=ok"#
                .to_string(),
            r#"action="delete_server_profile" target="gone" caller=":1.42" principal=- outcome="NoServerProfile""#.to_string(),
        ];
        want.extend(
            ["nope", "../x", "X", "a_b", ""]
                .iter()
                .map(|name| {
                    format!(
                        r#"action="delete_server_profile" target="{name}" caller=":1.42" principal=- outcome="NoServerProfile""#
                    )
                }),
        );
        assert_eq!(records(&lines), want, "one record per call");
    }

    // ── A profile two sections name: the first of them ────────────────────
    {
        std::fs::write(
            sys.join(INSTANCES_FILE),
            "[default]\nServerProfile = shared\n\n[lan-3]\nServerProfile = shared\n",
        )
        .expect(INSTANCES_FILE);
        profile("shared", "[Server]\nPort = 8091\nHostname = 127.0.0.1\n");
        let m = core(true, FakeActionLog::new());
        assert_eq!(
            m.delete_server_profile(&desktop, "llama.cpp", "shared"),
            in_use("shared", "default"),
            "refused for the first section that names it"
        );
        assert_eq!(
            m.server_profiles()
                .iter()
                .find(|r| r.id == "shared")
                .and_then(|r| r.instance.clone()),
            Some("default".to_string()),
            "and the list names that same section"
        );
    }

    // ── A record the manager cannot read is never answered "unused" ───────
    {
        std::fs::write(sys.join(INSTANCES_FILE), SEEDED).expect(INSTANCES_FILE);
        profile("keepme", "[Server]\nPort = 8092\nHostname = 127.0.0.1\n");
        let kept = bytes_of("server@keepme.ini");
        // Bytes that are no text: the read of the record fails, and "no section
        // names it" would delete a profile a running instance reads.
        std::fs::write(
            sys.join(INSTANCES_FILE),
            b"[default]\nServerProfile = default\n\xed\xa0\xbf\n",
        )
        .expect("broken record");
        let m = core(true, FakeActionLog::new());
        let e = m.delete_server_profile(&desktop, "llama.cpp", "keepme");
        assert!(
            matches!(e, Err(ManagerError::Io(_))),
            "the read's own failure, not \"unused\": {e:?}"
        );
        assert_eq!(bytes_of("server@keepme.ini"), kept, "the file stays");

        std::fs::remove_file(sys.join(INSTANCES_FILE)).expect("no record");
        assert_eq!(
            m.delete_server_profile(&desktop, "llama.cpp", "keepme"),
            Ok(()),
            "a record that is not there is no section naming it: the profile is unused"
        );
        assert!(!profile_path("keepme").exists(), "and its file goes");
    }

    // ── A caller without configure, and the `As` form ─────────────────────
    {
        std::fs::write(sys.join(INSTANCES_FILE), SEEDED).expect(INSTANCES_FILE);
        profile("spare", "[Server]\nPort = 8093\nHostname = 127.0.0.1\n");
        let spare = bytes_of("server@spare.ini");
        let log = FakeActionLog::new();
        let lines = log.lines.clone();
        let m = core(false, log);
        let before = listing();
        assert_eq!(
            m.create_server_profile(&desktop, "llama.cpp", "new-one", Some(8123)),
            Err(ManagerError::NotAuthorized),
            "a caller the authority refuses for configure"
        );
        assert_eq!(
            m.delete_server_profile(&desktop, "llama.cpp", "spare"),
            Err(ManagerError::NotAuthorized),
            "the same for a delete"
        );
        // The rows, by name, never a count: this phase's own blocks put files
        // in the tree, so any absolute number reds on a row a block adds. What
        // is pinned is the set of names this phase leaves: the profiles it
        // seeded and kept, the ones its creates wrote (`big`, `zero`, `shared`,
        // `spare`), `ghost` the dangling link (the list is of entries, as the
        // create's `symlink_metadata` check is), and `noport` absent, the
        // profile whose delete landed.
        let rows = m.server_profiles();
        let ids = rows.iter().map(|r| r.id.as_str()).collect::<Vec<_>>();
        // `ghost`, the dangling link, exists on a unix node only, so the
        // `mut` is used on unix only: gate the binding with it (a Windows
        // check sees no unused `mut`), the same rows on both sides.
        #[cfg(unix)]
        let mut want = vec![
            "big", "default", "lan-2", "secret", "shared", "spare", "zero",
        ];
        #[cfg(not(unix))]
        let want = vec![
            "big", "default", "lan-2", "secret", "shared", "spare", "zero",
        ];
        #[cfg(unix)]
        want.insert(2, "ghost");
        assert_eq!(
            ids, want,
            "the list is answered to a caller the authority refuses for configure, in \
             full: it is a read, the gate does not shrink it"
        );
        assert!(
            ids.contains(&"spare"),
            "the profile this block wrote is one of its rows"
        );
        assert!(
            !ids.contains(&"noport"),
            "the profile an earlier block deleted is not"
        );
        assert_eq!(
            rows,
            core(true, FakeActionLog::new()).server_profiles(),
            "the same rows an authorized caller sees: the list is the class of data \
             Instances answers to anyone, and a fresh log records no read"
        );
        assert_eq!(listing(), before, "a refused caller creates nothing");
        assert_eq!(bytes_of("server@spare.ini"), spare, "and removes nothing");
        assert_eq!(text_of(INSTANCES_FILE), SEEDED, "and writes no record");
        assert_eq!(
            records(&lines),
            vec![
                r#"action="create_server_profile" target="new-one" caller=":1.42" principal=- outcome=refused"#.to_string(),
                r#"action="delete_server_profile" target="spare" caller=":1.42" principal=- outcome=refused"#.to_string(),
            ],
            "the guard's refusals are recorded, the read of the list is not"
        );
    }
    {
        std::fs::write(sys.join(INSTANCES_FILE), SEEDED).expect(INSTANCES_FILE);
        let log = FakeActionLog::new();
        let lines = log.lines.clone();
        let m = core(true, log);
        let vouching = Requester {
            caller: desktop.clone(),
            principal: Some(Principal("alice".into())),
        };
        assert_eq!(
            m.create_server_profile(&member, "llama.cpp", "alice-p", Some(8123)),
            Ok(()),
            "the bff's account, for a member of `pai`"
        );
        assert_eq!(
            m.delete_server_profile(&member, "llama.cpp", "alice-p"),
            Ok(()),
            "and the member's delete of it"
        );
        assert_eq!(
            m.create_server_profile(&outsider, "llama.cpp", "bob-p", Some(8123)),
            Err(ManagerError::NotAuthorized),
            "the bff's account, for a caller outside `pai`"
        );
        assert_eq!(
            m.delete_server_profile(&outsider, "llama.cpp", "noport"),
            Err(ManagerError::NotAuthorized),
            "the same for a delete"
        );
        assert_eq!(
            m.create_server_profile(&vouching, "llama.cpp", "x", Some(8123)),
            Err(ManagerError::NotAuthorized),
            "a caller that is not the bff vouches for nobody"
        );
        assert_eq!(
            m.delete_server_profile(&vouching, "llama.cpp", "lan-2"),
            Err(ManagerError::NotAuthorized),
            "and cannot delete through one either"
        );
        assert!(
            !profile_path("bob-p").exists() && !profile_path("x").exists(),
            "the refused `As` calls created nothing"
        );
        // The tree's own profiles survive the refusals, pinned by name, never
        // by a count of the directory (this phase's blocks write and remove
        // files): `lan-2` is the target of the refused vouching delete, an
        // in-use profile of the seeded record; `spare` is a profile the tree
        // holds that no refused call names.
        assert!(
            profile_path("spare").exists() && profile_path("lan-2").exists(),
            "and removed nothing"
        );
        assert!(
            !profile_path("alice-p").exists(),
            "the two accepted calls did: the create, then the delete"
        );
        let got = records(&lines);
        assert_eq!(got.len(), 6, "one record per call");
        for (line, want) in [
            (
                got[0].as_str(),
                r#"action="create_server_profile" target="alice-p" caller=":1.9""#,
            ),
            (
                got[1].as_str(),
                r#"action="delete_server_profile" target="alice-p" caller=":1.9""#,
            ),
            (
                got[2].as_str(),
                r#"action="create_server_profile" target="bob-p" caller=":1.9""#,
            ),
            (
                got[3].as_str(),
                r#"action="delete_server_profile" target="noport" caller=":1.9""#,
            ),
            (
                got[4].as_str(),
                r#"action="create_server_profile" target="x" caller=":1.42""#,
            ),
            (
                got[5].as_str(),
                r#"action="delete_server_profile" target="lan-2" caller=":1.42""#,
            ),
        ] {
            assert!(line.starts_with(want), "{line} is not the record of {want}");
        }
        assert_eq!(
            got.iter()
                .map(|l| l.rsplit_once("outcome=").expect("outcome").1.to_string())
                .collect::<Vec<_>>(),
            vec!["ok", "ok", "refused", "refused", "refused", "refused"],
            "the accepted calls ok, every `As` refusal recorded as refused"
        );
    }
}
/// Creating and removing the node's instances (ARCHITECTURE.md, slice 16).
/// `Start` and `Restart` of an instance with no model profile to run, an empty
/// render (only `[*]`) or none at all, are the keyed refusal
/// `instance-no-model-profiles` [id], asked after the id is known to exist and
/// before the service manager is reached: a `Restart` never stops an instance
/// it cannot start again. `Stop` is not refused, an instance is always
/// stoppable.
///
/// `CreateInstance(engine, id, serverProfile)` writes under `configure` and the
/// `writing` lock, one action record with the id as its target, and writes in
/// one order: a stale `[id]` section of `assignments.ini` (only a hand edit
/// leaves one) is deleted, the render `models@<id>.ini` is written from
/// [`Manager`]'s own render, and only then is `[id]` with
/// `ServerProfile = <profile>` and the `Port` the node proposes (the first free
/// one at or above 8581) appended to `instances.ini`: the record is the
/// birth, so every refusal (`unknown-engine` first, then `instance-bad-id`,
/// `instance-exists`, `NoServerProfile`, `ServerProfileInUse`,
/// `UnresolvableHost` and `AddressInUse`, an instance's and the web service's
/// `[Bff] Listen` alike) writes no file at all. No service call on either OS:
/// systemd's template serves any id and a new id has no `wants/` link, so
/// `BootEnabled` is false; Windows creates the service at the first `Start`.
///
/// `RemoveInstance(id)` reads the state first, and removes only an instance
/// seen stopped (`inactive` or `failed`): `default` is `instance-protected`,
/// anything else and a state that cannot be read is `instance-running` or the
/// failure. Then, each step idempotent and the record last, it calls the new
/// [`ServiceControl::remove`], deletes the `assignments.ini` section, the
/// render and the record section. It leaves the server profile, the model
/// profiles, the state directory and every other instance's bytes, and a
/// failure at the service manager leaves the record: the instance stays listed
/// and stopped, and a second call finishes it.
#[cfg(test)]
pub(crate) fn manager_instance_crud_e2e() {
    use fakes::{
        Call, FakeActionLog, FakeAuthority, FakeGroup, FakePasswords, FakeResolver, FakeService,
    };

    let dir = tempfile::tempdir().expect("tempdir");
    crate::paths::redirect_for_tests(dir.path());
    let sys = crate::paths::engine_config_dir();
    std::fs::create_dir_all(&sys).expect("engine config dir");

    let bff = Caller(":1.9".into());
    let member = Requester {
        caller: bff.clone(),
        principal: Some(Principal("alice".into())),
    };
    let outsider = Requester {
        caller: bff.clone(),
        principal: Some(Principal("bob".into())),
    };
    let desktop = Caller(":1.42".into());
    let core_with = |service: FakeService, allow: bool, group: FakeGroup, log: FakeActionLog| {
        Manager::new(
            service,
            FakeAuthority::new(allow).quietly(true).bff(&bff),
            FakeResolver::lan(),
        )
        .with_group(group)
        .with_passwords(FakePasswords::new())
        .with_log(log)
    };
    let core = |allow: bool, log: FakeActionLog| {
        core_with(FakeService::new(), allow, FakeGroup::new(), log)
    };
    let text_of = |name: &str| std::fs::read_to_string(sys.join(name)).expect("read");
    let bytes_of = |name: &str| std::fs::read(sys.join(name)).expect("bytes");
    let section =
        |text: &str, id: &str| pai_core::ini::text::section_text(text, id).unwrap_or_default();
    let render_of = |id: &str| sys.join(format!("models@{id}.ini"));
    let listing = || -> Vec<String> {
        let mut names: Vec<String> = std::fs::read_dir(&sys)
            .map(|rd| {
                rd.flatten()
                    .map(|e| e.file_name().to_string_lossy().into_owned())
                    .collect()
            })
            .unwrap_or_default();
        names.sort();
        names
    };
    // Every file of the engine's config directory, by name and bytes: a
    // refusal must leave the tree exactly as it was.
    let snapshot = || -> Vec<(String, Vec<u8>)> {
        let mut files: Vec<(String, Vec<u8>)> = std::fs::read_dir(&sys)
            .expect("dir")
            .flatten()
            .filter(|e| e.file_type().map(|t| t.is_file()).unwrap_or(false))
            .map(|e| {
                (
                    e.file_name().to_string_lossy().into_owned(),
                    std::fs::read(e.path()).expect("bytes"),
                )
            })
            .collect();
        files.sort();
        files
    };
    let records =
        |lines: &std::sync::Arc<std::sync::Mutex<Vec<String>>>| lines.lock().unwrap().clone();
    let refused = |key: &'static str, value: &str| -> Result<(), ManagerError> {
        Err(ManagerError::Invalid(pai_core::checks::Refusal {
            key,
            values: vec![value.to_string()],
        }))
    };

    // The record, seeded with the note of one section, so a write of the
    // record can be pinned against keeping every other byte.
    const SEEDED: &str = "# the manager's own record\n[default]\nServerProfile = default\n\n[lan-2]\nServerProfile = lan-2\n; a note\nnote = keep me\n\n[lan-3]\nServerProfile = lan-3\n";
    std::fs::write(sys.join(INSTANCES_FILE), SEEDED).expect(INSTANCES_FILE);
    let profile = |name: &str, text: &str| {
        std::fs::write(sys.join(format!("server@{name}.ini")), text).expect("server profile");
    };
    profile(
        "default",
        "[Server]\nPort = 8080\nHostname = 127.0.0.1\nTags = edge\n",
    );
    profile("lan-2", "[Server]\nPort = 8081\nHostname = 127.0.0.1\n");
    profile("lan-3", "[Server]\nPort = 8082\nHostname = 127.0.0.1\n");
    // A profile no instance uses, one at lan-2's address, one at the web
    // service's, and one naming a host that does not resolve.
    profile("free", "[Server]\nPort = 8123\nHostname = 127.0.0.1\n");
    profile("dupe", "[Server]\nPort = 8081\nHostname = 127.0.0.1\n");
    profile("bffdupe", "[Server]\nPort = 8590\nHostname = 127.0.0.1\n");
    profile(
        "gone2",
        "[Server]\nPort = 8124\nHostname = nowhere.invalid\n",
    );
    // The web service's own address, the one `check_binding` judges.
    let framework = crate::paths::system_config_dir();
    std::fs::create_dir_all(&framework).expect("framework config dir");
    std::fs::write(
        framework.join("bff.ini"),
        "[Bff]\nListen = 127.0.0.1:8590\n",
    )
    .expect("bff.ini");

    // The pool: two model profiles, and `lan-2` the only instance assigned one.
    const POOL: &str =
        "[*]\non = true\n\n[qwen]\nmodel = qwen.gguf\n\n[other]\nmodel = other.gguf\n";
    std::fs::write(sys.join("models.ini"), POOL).expect("models.ini");
    std::fs::write(
        sys.join(ASSIGNMENTS_FILE),
        "[lan-2]\nqwen = on\n\n[lan-3]\nother = off\n",
    )
    .expect(ASSIGNMENTS_FILE);
    // `default`'s render holds the global section alone: nothing to run.
    std::fs::write(render_of("default"), "[*]\non = true\n").expect("render");
    std::fs::write(
        render_of("lan-2"),
        "[*]\non = true\n\n[qwen]\nmodel = qwen.gguf\n",
    )
    .expect("render");
    // `lan-3` has no render at all. Its state directory and a snapshot of
    // `lan-2`'s are there to stay.
    let state = crate::paths::instance_state_dir();
    std::fs::create_dir_all(state.join("lan-2")).expect("state dir");
    std::fs::write(state.join("lan-2/keep.llamastate"), "snapshot").expect("snapshot");

    // ── Start and Restart: nothing to run ─────────────────────────────────
    {
        let log = FakeActionLog::new();
        let lines = log.lines.clone();
        let mut service = FakeService::new();
        service.state = UnitState::Inactive;
        let m = core_with(service, true, FakeGroup::new(), log);
        let taken = || -> Vec<Call> { m.service().calls.lock().unwrap().clone() };
        for (verb, id) in [
            ("start", "default"),
            ("start", "lan-3"),
            ("restart", "default"),
            ("restart", "lan-3"),
        ] {
            let got = match verb {
                "start" => m.start(&desktop, id),
                _ => m.restart(&desktop, id),
            };
            assert_eq!(
                got,
                refused("instance-no-model-profiles", id),
                "{verb} of {id}: an empty render (`default`) and no render at all \
                 (`lan-3`) are both refused"
            );
        }
        assert_eq!(
            taken(),
            Vec::<Call>::new(),
            "no service call at all, and no stop for a `Restart`"
        );
        // An instance with a model profile runs, and stopping needs no profile.
        assert_eq!(m.start(&desktop, "lan-2"), Ok(()), "lan-2 has qwen");
        assert_eq!(m.restart(&desktop, "lan-2"), Ok(()));
        assert_eq!(
            taken(),
            vec![Call::Start("lan-2".into()), Call::Restart("lan-2".into()),],
            "one call each, no stop"
        );
        m.service().calls.lock().unwrap().clear();
        assert_eq!(
            m.stop(&desktop, "default"),
            Ok(()),
            "an instance with nothing to run is always stoppable"
        );
        assert_eq!(
            *m.service().calls.lock().unwrap(),
            vec![Call::Stop("default".into())]
        );
        let got = records(&lines);
        assert_eq!(
            got.iter()
                .map(|l| l.rsplit_once("outcome=").expect("outcome").1.to_string())
                .collect::<Vec<_>>(),
            vec![
                "\"Invalid\"",
                "\"Invalid\"",
                "\"Invalid\"",
                "\"Invalid\"",
                "ok",
                "ok",
                "ok"
            ],
            "one record per call, the refusals keyed"
        );
        assert_eq!(
            got.iter()
                .map(|l| l
                    .split(" target=\"")
                    .nth(1)
                    .expect("target")
                    .split('"')
                    .next()
                    .expect("target")
                    .to_string())
                .collect::<Vec<_>>(),
            vec!["default", "lan-3", "default", "lan-3", "lan-2", "lan-2", "default"],
            "the target of every record is the instance id: {got:?}"
        );
        assert_eq!(
            got.iter()
                .map(|l| l
                    .split("action=\"")
                    .nth(1)
                    .unwrap_or_default()
                    .split('"')
                    .next()
                    .unwrap_or_default()
                    .to_string())
                .collect::<Vec<_>>(),
            vec!["start", "start", "restart", "restart", "start", "restart", "stop"],
            "the verbs in order: {got:?}"
        );
        assert!(
            got.iter().all(
                |l| l.starts_with(r#"action=""#) && l.contains(r#"caller=":1.42" principal=-"#)
            ),
            "the desktop caller, vouching for nobody: {got:?}"
        );
    }

    // ── A create: the section, the render, and no service call ────────────
    {
        let log = FakeActionLog::new();
        let lines = log.lines.clone();
        let mut service = FakeService::new();
        service.state = UnitState::Active;
        let m = core_with(service, true, FakeGroup::new(), log);
        // A stale `[newone]` section of the assignments, the mark of an id used
        // before: the create leaves that id assigned nothing.
        std::fs::write(
            sys.join(ASSIGNMENTS_FILE),
            "[lan-2]\nqwen = on\n\n[newone]\nqwen = on\nother = off\n",
        )
        .expect(ASSIGNMENTS_FILE);
        let before = listing();
        assert_eq!(m.create_instance(&member, ENGINE, "newone", "free"), Ok(()));

        let record = text_of(INSTANCES_FILE);
        assert_eq!(
            &record[SEEDED.len()..],
            "[newone]\nServerProfile = free\nPort = 8581\n",
            "the section is appended, naming the profile and carrying the port the \
             create proposed (IP3a: the first free one at or above 8581, this node's \
             instances sitting on 8080, 8081 and 8082 and the web service on 8590), \
             and every byte before it is the seed's, the note included"
        );
        assert_eq!(
            section(&text_of(ASSIGNMENTS_FILE), "newone"),
            "",
            "the stale section is gone"
        );
        assert_eq!(
            text_of(ASSIGNMENTS_FILE),
            "[lan-2]\nqwen = on\n\n",
            "every other assignment is the file's own: the section is cut out, \
             its separating blank line left as `delete_section` leaves it"
        );
        assert!(
            !pai_core::launch::has_model_profiles_in(&render_of("newone")),
            "the new instance has no model profile: a start of it is refused"
        );
        assert_eq!(
            text_of("models@newone.ini"),
            pai_core::assign::render(POOL, "", None),
            "the render is the manager's own, of the pool with the new instance \
             assigned nothing"
        );
        // The create runs the pass every write runs (IP3c): the new instance gets
        // the two halves of its pair, and the pass gives both halves to every
        // instance of the record whose pair the tree had never rendered.
        let mut want = before.clone();
        want.extend(
            [
                "models@newone.ini",
                "launch@default.ini",
                "launch@lan-2.ini",
                "launch@lan-3.ini",
                "launch@newone.ini",
                "models@lan-3.ini",
            ]
            .iter()
            .map(|name| name.to_string()),
        );
        want.sort();
        assert_eq!(
            listing(),
            want,
            "the new instance's pair, beside the launch halves the pass rendered for the \
             record's instances: no temp file is left"
        );
        assert_eq!(
            *m.service().calls.lock().unwrap(),
            Vec::<Call>::new(),
            "no service call: systemd's template serves the id and a new id has \
             no wants link, so boot is off"
        );
        let rows = m.instances();
        let new = rows.iter().find(|r| r.id == "newone").expect("listed");
        assert_eq!(new.engine, ENGINE);
        assert_eq!(new.server_profile.as_deref(), Some("free"));
        assert_eq!(
            new.port, 8581,
            "the port listed is the record's own, the one the create proposed (the \
             first free one at or above 8581: no section of this seed's record holds \
             a `Port`), not the 8123 of the profile it names"
        );
        assert_eq!(
            rows.iter().map(|r| r.id.as_str()).collect::<Vec<_>>(),
            vec!["default", "lan-2", "lan-3", "newone"],
            "the new instance is listed, sorted with the rest"
        );
        assert_eq!(
            m.boot_enabled("newone"),
            Ok(false),
            "a new instance does not start at boot"
        );
        assert_eq!(
            *m.service().calls.lock().unwrap(),
            vec![Call::BootEnabled("newone".into())],
            "boot is read from the service manager, and it answers off for an id \
             with no wants link"
        );
        let got = records(&lines);
        assert_eq!(got.len(), 1, "one record for the create: {got:?}");
        assert!(
            got[0].starts_with(
                r#"action="create_instance" target="newone" caller=":1.9" principal="alice""#
            ),
            "{}`, not the record of the create",
            got[0]
        );
        assert_eq!(
            got[0].rsplit_once("outcome=").expect("outcome").1,
            "ok",
            "`{}` is not ok",
            got[0]
        );
    }

    // ── Every refusal writes no file ──────────────────────────────────────
    {
        let log = FakeActionLog::new();
        let lines = log.lines.clone();
        let m = core(true, log);
        let before = snapshot();
        type CreateCase = (
            &'static str,
            &'static str,
            &'static str,
            Result<(), ManagerError>,
            &'static str,
        );
        let cases: Vec<CreateCase> = vec![
            (
                "vllama",
                "spared",
                "free",
                refused("unknown-engine", "vllama"),
                "the engine is asked first",
            ),
            (
                "vllama",
                "../x",
                "free",
                refused("unknown-engine", "vllama"),
                "a bad id with an unknown engine is the engine's refusal",
            ),
            (
                "llama.cpp",
                "../x",
                "free",
                refused("instance-bad-id", "../x"),
                "the id rule before any path: a `../x` id writes and deletes nothing",
            ),
            (
                "llama.cpp",
                "A",
                "free",
                refused("instance-bad-id", "A"),
                "an upper case id",
            ),
            (
                "llama.cpp",
                "",
                "free",
                refused("instance-bad-id", ""),
                "an empty id",
            ),
            (
                "llama.cpp",
                "a_b",
                "free",
                refused("instance-bad-id", "a_b"),
                "an underscore",
            ),
            (
                "llama.cpp",
                "lan-2",
                "free",
                refused("instance-exists", "lan-2"),
                "a section of the record has the id",
            ),
            (
                "llama.cpp",
                "lan-2",
                "gone2",
                refused("instance-exists", "lan-2"),
                "the record is read before the profile is looked at",
            ),
            (
                "llama.cpp",
                "spared",
                "gone",
                Err(ManagerError::NoServerProfile("gone".into())),
                "no file of that profile",
            ),
            (
                "llama.cpp",
                "spared",
                "../x",
                Err(ManagerError::NoServerProfile("../x".into())),
                "a profile name the id rule refuses is no profile",
            ),
            (
                "llama.cpp",
                "spared",
                "lan-2",
                Err(ManagerError::ServerProfileInUse(
                    "lan-2".into(),
                    "lan-2".into(),
                )),
                "the profile an instance runs",
            ),
            (
                "llama.cpp",
                "spared",
                "dupe",
                Err(ManagerError::AddressInUse("lan-2".into())),
                "the address of another instance",
            ),
            (
                "llama.cpp",
                "spared",
                "bffdupe",
                Err(ManagerError::AddressInUse(BFF_NAME.into())),
                "the web service's `[Bff] Listen`",
            ),
            (
                "llama.cpp",
                "spared",
                "gone2",
                Err(ManagerError::UnresolvableHost("nowhere.invalid".into())),
                "a host of the profile that does not resolve",
            ),
        ];
        for (engine, id, name, want, why) in cases {
            assert_eq!(
                m.create_instance(&member, engine, id, name),
                want,
                "create({engine}, {id}, {name}): {why}"
            );
        }
        assert_eq!(
            snapshot(),
            before,
            "every refused create wrote no file, anywhere"
        );
        assert_eq!(
            *m.service().calls.lock().unwrap(),
            Vec::<Call>::new(),
            "a refused create reaches no service manager"
        );
        assert_eq!(
            records(&lines)
                .iter()
                .map(|l| l.rsplit_once("outcome=").expect("outcome").1.to_string())
                .collect::<Vec<_>>(),
            vec![
                "\"Invalid\"",
                "\"Invalid\"",
                "\"Invalid\"",
                "\"Invalid\"",
                "\"Invalid\"",
                "\"Invalid\"",
                "\"Invalid\"",
                "\"Invalid\"",
                "\"NoServerProfile\"",
                "\"NoServerProfile\"",
                "\"ServerProfileInUse\"",
                "\"AddressInUse\"",
                "\"AddressInUse\"",
                "\"UnresolvableHost\"",
            ],
            "one record per refused call, keyed as the refusal is"
        );
    }

    // ── A create whose render cannot be written leaves no section ─────────
    // The record is the last thing a create writes, so a create that fails
    // writing its render leaves the instance unlisted: the id the create was
    // asked for is nowhere, and nothing of the tree moved.
    {
        let log = FakeActionLog::new();
        let lines = log.lines.clone();
        let m = core_with(FakeService::new(), true, FakeGroup::new(), log);
        profile("blockp", "[Server]\nPort = 8125\nHostname = 127.0.0.1\n");
        // The render's path is a directory: its write, the create's last step
        // before the record, fails on the host.
        std::fs::create_dir(sys.join("models@blocked.ini")).expect("render dir");
        let before = snapshot();
        let answer = m.create_instance(&member, ENGINE, "blocked", "blockp");
        assert!(
            matches!(answer, Err(ManagerError::Io(_))),
            "the render cannot be written: {answer:?}"
        );
        assert_eq!(
            snapshot(),
            before,
            "a create with no render wrote no file: the record is the last thing \
             it writes, so a failed render leaves no section"
        );
        assert_eq!(
            records(&lines).len(),
            1,
            "one action record for the create that failed"
        );
    }

    // ── The gates: polkit for a caller's own write, the pair for the `As` ──
    {
        let log = FakeActionLog::new();
        let lines = log.lines.clone();
        let before = snapshot();
        // polkit refuses the caller's own `configure`, and this authority does
        // not know the bff's account: every call here is refused, the write for
        // its action, the vouched ones because the `As` form is answered to
        // nobody.
        let m = Manager::new(
            FakeService::new(),
            FakeAuthority::new(false).quietly(true),
            FakeResolver::lan(),
        )
        .with_group(FakeGroup::new())
        .with_passwords(FakePasswords::new())
        .with_log(log);
        assert_eq!(
            m.create_instance(&desktop, ENGINE, "spared", "free"),
            Err(ManagerError::NotAuthorized),
            "a desktop caller's own create: polkit is asked, and refuses"
        );
        assert_eq!(
            m.remove_instance(&desktop, "lan-2"),
            Err(ManagerError::NotAuthorized),
            "the same for a desktop caller's own remove"
        );
        for (who, verb) in [
            (&member as &Requester, "create"),
            (&outsider, "create"),
            (&member, "remove"),
            (&outsider, "remove"),
        ] {
            let got = match verb {
                "create" => m.create_instance(who, ENGINE, "spared", "free"),
                _ => m.remove_instance(who, "lan-2"),
            };
            assert_eq!(
                got,
                Err(ManagerError::NotAuthorized),
                "{verb} as {}: an authority that does not know the bff answers \
                 the `As` form to nobody",
                who.principal.as_ref().map(|p| p.0.as_str()).unwrap_or("-")
            );
        }
        assert_eq!(
            snapshot(),
            before,
            "a refused call wrote no file: the create of `spared` is nowhere"
        );
        assert_eq!(
            records(&lines)
                .iter()
                .map(|l| l.rsplit_once("outcome=").expect("outcome").1.to_string())
                .collect::<Vec<_>>(),
            vec!["refused"; 6],
            "every refused call is recorded"
        );
    }

    // ── A principal outside `pai`, and a member the bff vouches for ────────
    {
        let log = FakeActionLog::new();
        let lines = log.lines.clone();
        // polkit answers and this authority knows the bff's account: the pair is
        // all an `As` call needs, so the gate is the principal's own.
        let mut service = FakeService::new();
        service.state = UnitState::Inactive;
        let m = core_with(service, true, FakeGroup::new(), log);
        // A profile no instance holds, so the member's create has one to bind.
        std::fs::write(
            sys.join("server@spared-p.ini"),
            "[Server]\nPort = 8131\nHostname = 127.0.0.1\n",
        )
        .expect("server@spared-p.ini");
        let before = snapshot();
        assert_eq!(
            m.create_instance(&outsider, ENGINE, "spared", "spared-p"),
            Err(ManagerError::NotAuthorized),
            "bob is outside `pai`: the create is refused whole"
        );
        assert_eq!(
            m.remove_instance(&outsider, "lan-2"),
            Err(ManagerError::NotAuthorized),
            "the remove asks the same pair: lan-2 stays"
        );
        assert_eq!(snapshot(), before, "a refused principal wrote nothing");
        assert_eq!(
            m.create_instance(&member, ENGINE, "spared", "spared-p"),
            Ok(()),
            "alice is a member the bff vouches for: no right of the caller's is asked"
        );
        assert!(
            text_of(INSTANCES_FILE).contains("[spared]\nServerProfile = spared-p\n"),
            "the member's create landed"
        );
        assert_eq!(
            m.remove_instance(&member, "spared"),
            Ok(()),
            "and her own remove"
        );
        assert!(
            !text_of(INSTANCES_FILE).contains("[spared]"),
            "the instance is gone again, the record's other sections untouched"
        );
        assert_eq!(
            records(&lines)
                .iter()
                .map(|l| l.rsplit_once("outcome=").expect("outcome").1.to_string())
                .collect::<Vec<_>>(),
            vec!["refused", "refused", "ok", "ok"],
            "the refusals and the writes, one record each"
        );
    }

    // ── A remove: the unit, the assignment, the render, the record last ───
    {
        let log = FakeActionLog::new();
        let lines = log.lines.clone();
        let mut service = FakeService::new();
        service.state = UnitState::Inactive;
        let m = core_with(service, true, FakeGroup::new(), log);
        let taken = || -> Vec<Call> { m.service().calls.lock().unwrap().clone() };
        let record = text_of(INSTANCES_FILE);
        let profile_bytes = bytes_of("server@lan-2.ini");
        let render_bytes = bytes_of("models@default.ini");
        assert_eq!(m.remove_instance(&member, "lan-2"), Ok(()));

        let taken = taken();
        assert_eq!(
            taken
                .iter()
                .filter(|call| **call == Call::Remove("lan-2".into()))
                .count(),
            1,
            "the service manager removes the unit once: {taken:?}"
        );
        assert!(
            matches!(&taken[0], Call::Status(id) if id.as_str() == "lan-2"),
            "the state is read before the unit goes: {taken:?}"
        );
        assert!(
            !render_of("lan-2").exists(),
            "the render is gone: the launch reads the record, and there is none"
        );
        assert_eq!(
            pai_core::ini::text::section_text(&text_of(ASSIGNMENTS_FILE), "lan-2"),
            None,
            "the assignments section is gone"
        );
        assert_eq!(
            section(&text_of(ASSIGNMENTS_FILE), "newone"),
            "",
            "and no other"
        );
        let after = text_of(INSTANCES_FILE);
        assert!(
            pai_core::ini::text::section_text(&after, "lan-2").is_none(),
            "the record section is gone"
        );
        assert_eq!(
            section(&after, "lan-3"),
            section(&record, "lan-3"),
            "every other section is the file's own bytes"
        );
        assert_eq!(
            bytes_of("server@lan-2.ini"),
            profile_bytes,
            "the server profile stays: it is not the instance's file"
        );
        assert_eq!(
            bytes_of("models@default.ini"),
            render_bytes,
            "the other instances' renders stay"
        );
        assert_eq!(
            bytes_of("models.ini"),
            POOL.as_bytes(),
            "the model profiles stay"
        );
        assert_eq!(
            std::fs::read(state.join("lan-2/keep.llamastate")).expect("snapshot"),
            b"snapshot".to_vec(),
            "the state directory stays: a later instance of the id finds its snapshots"
        );
        let got = records(&lines);
        assert_eq!(got.len(), 1, "one record for the remove: {got:?}");
        assert!(
            got[0].starts_with(
                r#"action="remove_instance" target="lan-2" caller=":1.9" principal="alice""#
            ) && got[0].ends_with("outcome=ok"),
            "{}`, not the record of the remove",
            got[0]
        );
    }

    // ── A remove of what must stay, or is not there, writes nothing ───────
    {
        type RemoveCase = (
            UnitState,
            Vec<String>,
            &'static str,
            Result<(), ManagerError>,
            &'static str,
        );
        let cases: Vec<RemoveCase> = vec![
            (
                UnitState::Inactive,
                Vec::new(),
                "default",
                refused("instance-protected", "default"),
                "the one protected id, even stopped",
            ),
            (
                UnitState::Active,
                Vec::new(),
                "lan-3",
                refused("instance-running", "lan-3"),
                "a running instance",
            ),
            (
                UnitState::Activating,
                Vec::new(),
                "lan-3",
                refused("instance-running", "lan-3"),
                "one coming up",
            ),
            (
                UnitState::Deactivating,
                Vec::new(),
                "lan-3",
                refused("instance-running", "lan-3"),
                "one coming down",
            ),
            (
                UnitState::Inactive,
                Vec::new(),
                "nope",
                Err(ManagerError::NoInstance),
                "no section has the id",
            ),
            (
                UnitState::Inactive,
                Vec::new(),
                "../x",
                Err(ManagerError::InvalidId),
                "the id rule before any path",
            ),
            (
                UnitState::Inactive,
                Vec::new(),
                "",
                Err(ManagerError::InvalidId),
                "an empty id",
            ),
        ];
        for (state, failing, id, want, why) in cases {
            let mut service = FakeService::new();
            service.state = state;
            service.failing = failing;
            let m = core_with(service, true, FakeGroup::new(), FakeActionLog::new());
            let before = snapshot();
            assert_eq!(m.remove_instance(&member, id), want, "remove({id}): {why}");
            assert!(
                m.service()
                    .calls
                    .lock()
                    .unwrap()
                    .iter()
                    .all(|call| !matches!(call, Call::Remove(_))),
                "no remove call for {id}: {:?}",
                m.service().calls.lock().unwrap()
            );
            assert_eq!(
                snapshot(),
                before,
                "the refused remove of {id} wrote no file"
            );
        }
        // A state that cannot be read is never a state seen stopped.
        let mut service = FakeService::new();
        service.state = UnitState::Inactive;
        service.failing = vec!["lan-3".into()];
        let m = core_with(service, true, FakeGroup::new(), FakeActionLog::new());
        let before = snapshot();
        assert!(
            matches!(
                m.remove_instance(&member, "lan-3"),
                Err(ManagerError::Service(_))
            ),
            "a `Status` failure is the service manager's own failure, never a removal"
        );
        assert!(
            m.service()
                .calls
                .lock()
                .unwrap()
                .iter()
                .all(|call| !matches!(call, Call::Remove(_))),
            "no remove after a state that cannot be read"
        );
        assert_eq!(snapshot(), before, "and no file written");
    }

    // ── A failed remove leaves a listed instance a second call finishes ───
    {
        let mut service = FakeService::new();
        service.state = UnitState::Inactive;
        service.failing_remove = vec!["lan-3".into()];
        let m = core_with(service, true, FakeGroup::new(), FakeActionLog::new());
        let before = snapshot();
        assert!(
            matches!(
                m.remove_instance(&member, "lan-3"),
                Err(ManagerError::Service(_))
            ),
            "the service manager failed to remove the unit"
        );
        assert_eq!(
            snapshot(),
            before,
            "the first step failed, so nothing after it ran: the tree is byte for \
             byte as it was"
        );
        assert!(
            m.instances().iter().any(|r| r.id == "lan-3"),
            "the instance is still listed, and stopped"
        );
        // The next call, with the service manager answering, finishes it: every
        // step is idempotent. The instance is stopped, as the refusal left it.
        let mut service = FakeService::new();
        service.state = UnitState::Inactive;
        let m = core_with(service, true, FakeGroup::new(), FakeActionLog::new());
        assert_eq!(m.remove_instance(&member, "lan-3"), Ok(()));
        assert!(
            !render_of("lan-3").exists()
                && pai_core::ini::text::section_text(&text_of(ASSIGNMENTS_FILE), "lan-3").is_none()
                && pai_core::ini::text::section_text(&text_of(INSTANCES_FILE), "lan-3").is_none(),
            "the second remove finished it"
        );
    }

    // ── The record unreadable is never "absent" ──────────────────────────
    {
        let mut service = FakeService::new();
        service.state = UnitState::Inactive;
        let m = core_with(service, true, FakeGroup::new(), FakeActionLog::new());
        std::fs::write(sys.join(INSTANCES_FILE), [0xff, 0xfe, 0x00]).expect("non UTF-8 record");
        let before = snapshot();
        assert!(
            matches!(
                m.create_instance(&member, ENGINE, "spared", "free"),
                Err(ManagerError::Io(_))
            ),
            "a record that cannot be read is `Io`, never a create"
        );
        assert!(
            matches!(
                m.remove_instance(&member, "lan-2"),
                Err(ManagerError::Io(_))
            ),
            "and never `NoInstance`"
        );
        assert_eq!(snapshot(), before, "neither wrote");
        // A tree with no record at all: the create writes it, the remove has
        // nothing to remove.
        std::fs::remove_file(sys.join(INSTANCES_FILE)).expect("no record");
        assert_eq!(
            m.remove_instance(&member, "lan-2"),
            Err(ManagerError::NoInstance)
        );
        assert_eq!(m.create_instance(&member, ENGINE, "spared", "free"), Ok(()));
        assert_eq!(
            text_of(INSTANCES_FILE),
            "[spared]\nServerProfile = free\nPort = 8581\n",
            "a record written from nothing holds the one section, its proposed \
             port included (IP3a)"
        );
    }
}
/// the node's server profiles (protocol 3, its `As` form and the same name over
/// the pipe). It writes one key, the `ServerProfile` of the instance's section
/// of `instances.ini`, and the instance's next launch reads the profile the key
/// names. The profile a switch aims at is unused at that moment, so the port
/// check of a profile save (`check_address`, which judges a profile no instance
/// runs against nothing) judges nothing here: the switch judges the target's
/// address against every other instance's profile and against the web service's,
/// leaving out the profile the switching instance runs now. A call answers
/// before it writes: an id that is no instance (`InvalidId`, `NoInstance`), a
/// profile the engine has none of (`NoServerProfile`, naming it), a profile
/// another instance runs (`ServerProfileInUse`, naming the profile and that
/// instance), an address the switch would take (`AddressInUse`, naming the
/// instance, or the web service as itself) and a `ModelsMax` under the loads the
/// instance's render starts (`load-on-startup-over-max`, naming the instance)
/// each write nothing, and a file that cannot be read is answered its own kind
/// rather than as an empty one. Guarded as a write of a served file is:
/// `configure` first, the `As` form answered only for the bff's account and a
/// current member of `pai`, and one record of the action log per call, done or
/// refused.
#[cfg(test)]
pub(crate) fn manager_set_server_profile_e2e() {
    use fakes::{
        FakeActionLog, FakeAuthority, FakeGroup, FakePasswords, FakeResolver, FakeService,
    };

    let dir = tempfile::tempdir().expect("tempdir");
    crate::paths::redirect_for_tests(dir.path());
    let sys = crate::paths::engine_config_dir();
    std::fs::create_dir_all(&sys).expect("engine config dir");
    let framework = crate::paths::system_config_dir();
    std::fs::create_dir_all(&framework).expect("framework config dir");

    // The bff's account, its request for alice (a member of `pai` in
    // [`FakePasswords`]) and for bob (who is not), and a desktop caller of its
    // own.
    let bff = Caller(":1.9".into());
    let member = Requester {
        caller: bff.clone(),
        principal: Some(Principal("alice".into())),
    };
    let outsider = Requester {
        caller: bff.clone(),
        principal: Some(Principal("bob".into())),
    };
    let desktop = Caller(":1.42".into());
    let core = |allow: bool, passwords: FakePasswords, log: FakeActionLog| {
        Manager::new(
            FakeService::new(),
            FakeAuthority::new(allow).quietly(true).bff(&bff),
            FakeResolver::lan(),
        )
        .with_group(FakeGroup::new())
        .with_passwords(passwords)
        .with_log(log)
    };
    let text_of = |name: &str| std::fs::read_to_string(sys.join(name)).expect("read");
    let bytes_of = |name: &str| std::fs::read(sys.join(name)).expect("bytes");
    let section =
        |text: &str, id: &str| pai_core::ini::text::section_text(text, id).unwrap_or_default();
    let keys_of = |text: &str, id: &str, key: &str| -> Vec<String> {
        section(text, id)
            .lines()
            .filter(|line| line.trim_start().starts_with(&format!("{key} ")))
            .map(|line| line.to_string())
            .collect()
    };
    let listing = || -> Vec<String> {
        let mut names: Vec<String> = std::fs::read_dir(&sys)
            .map(|rd| {
                rd.flatten()
                    .map(|e| e.file_name().to_string_lossy().into_owned())
                    .collect()
            })
            .unwrap_or_default();
        names.sort();
        names
    };
    let records =
        |lines: &std::sync::Arc<std::sync::Mutex<Vec<String>>>| lines.lock().unwrap().clone();
    let profile = |name: &str, text: &str| {
        std::fs::write(sys.join(format!("server@{name}.ini")), text).expect("server profile");
    };
    let no_profile = |name: &str| -> Result<Affected, ManagerError> {
        Err(ManagerError::NoServerProfile(name.to_string()))
    };
    let in_use = |name: &str, id: &str| -> Result<Affected, ManagerError> {
        Err(ManagerError::ServerProfileInUse(
            name.to_string(),
            id.to_string(),
        ))
    };
    let taken = |who: &str| -> Result<Affected, ManagerError> {
        Err(ManagerError::AddressInUse(who.to_string()))
    };
    // The cap news this phase is about, its three args whole. The cap is a fit
    // check of ONE instance (IP3c, `ARCHITECTURE.md` "Convergence"): the switch
    // that brings the news LANDS, the instance whose render it stops keeps the
    // pair it has, and that write's own render pass reports the check's key with
    // the same three args. True when the answer is that.
    let over_max = |m: &Manager<FakeService, FakeAuthority, FakeResolver>,
                    answer: &Result<Affected, ManagerError>,
                    count: &str,
                    max: &str,
                    id: &str|
     -> bool {
        answer.is_ok()
            && m.not_converged_locked()
                .expect("the rows")
                .iter()
                .any(|row| {
                    row.instance == id
                        && row.key == "load-on-startup-over-max"
                        && row.args == [count.to_string(), max.to_string(), id.to_string()]
                })
    };
    // The model profile renders of the two instances: a server profile switch
    // writes no model profile text, so these bytes must not move.
    let renders = || -> Vec<(String, String)> {
        ["models@default.ini", "models@lan-2.ini"]
            .iter()
            .map(|name| (name.to_string(), text_of(name)))
            .collect()
    };
    // The whole pair of the two instances: the model list and the launch. A
    // switch whose instance does not fit leaves BOTH halves where they stand
    // (IP3c: the instance keeps its last good pair).
    let pair = || -> Vec<(String, String)> {
        [
            "models@default.ini",
            "models@lan-2.ini",
            "launch@default.ini",
            "launch@lan-2.ini",
        ]
        .iter()
        .map(|name| (name.to_string(), text_of(name)))
        .collect()
    };
    // The instance's launch, read after a switch: `flag` is the value of a flag
    // in its argument list, `None` when the launch carries none.
    let exe = dir
        .path()
        .join("bin")
        .join(crate::paths::binary_name("llama-server"));
    let launch = |id: &str| {
        crate::launch::instance_command(id, &exe).expect("the instance has a profile to launch")
    };
    let flag = |cmd: &std::process::Command, name: &str| -> Option<String> {
        let args: Vec<String> = cmd
            .get_args()
            .map(|a| a.to_string_lossy().into_owned())
            .collect();
        let i = args.iter().position(|a| a == name)?;
        args.get(i + 1).cloned()
    };

    // The record, seeded the way an earlier write leaves it: a comment, a key
    // of its own, a `;` comment and a blank line in front of the next section.
    // The verb writes one key of one section, so the file it writes is compared
    // byte for byte with these bytes.
    const SEEDED: &str = "# the manager's own record\n[default]\nServerProfile = default\n; a note of its own\nnote = keep me\n\n[lan-2]\nServerProfile = lan-2\n";
    std::fs::write(sys.join(INSTANCES_FILE), SEEDED).expect(INSTANCES_FILE);
    // The server profiles. `own` shares `default`'s 8080 (one in use, one
    // spare), `clash` shares `lan-2`'s 8081 (spare), `big` sits on the free
    // 8082, `on-bff` on the web service's default port, `tiny` caps the pool at
    // 1, `roomy` at 3.
    profile("default", "[Server]\nPort = 8080\nHostname = 127.0.0.1\n");
    profile("lan-2", "[Server]\nPort = 8081\nHostname = 127.0.0.1\n");
    profile("own", "[Server]\nPort = 8080\nHostname = 127.0.0.1\n");
    profile("clash", "[Server]\nPort = 8081\nHostname = 127.0.0.1\n");
    profile("big", "[Server]\nPort = 8082\nHostname = 127.0.0.1\n");
    profile("on-bff", "[Server]\nPort = 8590\nHostname = 127.0.0.1\n");
    profile(
        "tiny",
        "[Server]\nPort = 8083\nHostname = 127.0.0.1\nModelsMax = 1\n",
    );
    profile(
        "roomy",
        "[Server]\nPort = 8084\nHostname = 127.0.0.1\nModelsMax = 3\n",
    );
    // The pool: three model profiles that load on startup and one that does
    // not. The switched-off half asks for a load too, so the count a check
    // reads is a render's, never the pool's.
    let pool = real_model_files(dir.path(), "[a]\nmodel = /models/a.gguf\nload-on-startup = true\n\n[b]\nmodel = /models/b.gguf\nload-on-startup = true\n\n[c]\nmodel = /models/c.gguf\nload-on-startup = true\n\n[d]\nmodel = /models/d.gguf\n");
    let off_pool = real_model_files(
        dir.path(),
        "[m]\nmodel = /models/m.gguf\nload-on-startup = true\n",
    );
    std::fs::write(sys.join("models.ini"), &pool).expect("models.ini");
    std::fs::write(sys.join("models.ini.disabled"), &off_pool).expect("models.ini.disabled");
    // The assignments, per instance: `default` inherits the pool, so its render
    // starts three model profiles; `lan-2` switches all three off, so its
    // render starts none. The same profile caps one and caps nothing for the
    // other, which is what the per-instance rule is for.
    std::fs::write(
        sys.join("assignments.ini"),
        "[default]\n* = inherit\n\n[lan-2]\n* = inherit\na = off\nb = off\nc = off\n",
    )
    .expect("assignments.ini");

    {
        let log = FakeActionLog::new();
        let lines = log.lines.clone();
        let m = core(true, FakePasswords::new(), log);
        assert!(
            m.render_failures().is_empty(),
            "the fixture's record renders every instance"
        );
        assert_eq!(
            flag(&launch("default"), "--port").as_deref(),
            Some("8080"),
            "the fixture: the launch reads the profile the section names"
        );
        let before = listing();
        let kept = renders();

        assert_eq!(
            m.set_server_profile(&desktop, "default", "big").map(|_| ()),
            Ok(()),
            "a spare profile on a port nobody runs"
        );
        let written = text_of(INSTANCES_FILE);
        assert_eq!(
            written,
            SEEDED.replace("ServerProfile = default", "ServerProfile = big"),
            "one key of one section: every other byte of the record, the comment, the `;` comment, the key of its own and the blank line in front of the next section, as it was"
        );
        assert_eq!(
            keys_of(&written, "default", "ServerProfile"),
            vec!["ServerProfile = big".to_string()],
            "the key is replaced, not added to"
        );
        assert_eq!(
            section(&written, "lan-2"),
            "[lan-2]\nServerProfile = lan-2\n",
            "the section of another instance is kept as it was"
        );
        let cmd = launch("default");
        assert_eq!(
            flag(&cmd, "--port").as_deref(),
            Some("8082"),
            "the next launch of the instance is the new profile's port"
        );
        assert_eq!(
            flag(&cmd, "--models-preset").as_deref(),
            Some(sys.join("models@default.ini").to_string_lossy().as_ref()),
            "and its model profiles stay the render of the instance id, not of the profile"
        );

        // A profile that caps the pool: its own key reaches the launch.
        assert_eq!(
            m.set_server_profile(&desktop, "default", "roomy")
                .map(|_| ()),
            Ok(()),
            "a cap equal to the loads the instance starts is not over it"
        );
        let cmd = launch("default");
        assert_eq!(
            flag(&cmd, "--port").as_deref(),
            Some("8084"),
            "the second switch landed"
        );
        assert_eq!(
            flag(&cmd, "--models-max").as_deref(),
            Some("3"),
            "the new profile's cap, read from its file"
        );

        // Back to the profile it started on.
        assert_eq!(
            m.set_server_profile(&desktop, "default", "default")
                .map(|_| ()),
            Ok(()),
            "a switch back"
        );
        assert_eq!(
            keys_of(&text_of(INSTANCES_FILE), "default", "ServerProfile"),
            vec!["ServerProfile = default".to_string()],
            "one key after three switches, not three"
        );
        assert_eq!(
            text_of(INSTANCES_FILE),
            SEEDED,
            "the record is the bytes it was before the switches"
        );
        assert_eq!(listing(), before, "the verb creates no file of its own");
        for (name, text) in kept {
            assert_eq!(
                text_of(&name),
                text,
                "and renders nothing: a server profile carries no model profile text, and the render's name is the instance's, not the profile's: {name}"
            );
        }
        assert_eq!(
            records(&lines),
            vec![
                r#"action="set_server_profile" target="default" caller=":1.42" principal=- outcome=ok"#.to_string();
                3
            ],
            "one record per call: the verb's action, the instance as its target, the caller, no principal as a bare -, ok"
        );
    }

    {
        let m = core(true, FakePasswords::new(), FakeActionLog::new());
        let seed = text_of(INSTANCES_FILE);
        let before = listing();
        assert_eq!(
            m.set_server_profile(&desktop, "lan-3", "big"),
            Err(ManagerError::NoInstance),
            "an id that is no section of the record"
        );
        for bad in ["", "../x", "Default", "a_b"] {
            assert_eq!(
                m.set_server_profile(&desktop, bad, "big"),
                Err(ManagerError::InvalidId),
                "not an instance id at all: {bad:?}"
            );
        }
        assert_eq!(
            text_of(INSTANCES_FILE),
            seed,
            "every refusal above wrote nothing"
        );
        assert_eq!(listing(), before, "and created no file");
    }

    {
        // A file outside the engine's directory, at the place a naive join of
        // the name `../x` lands. It stays the fixture's, so the refusals below
        // built no path.
        std::fs::write(dir.path().join("x.ini"), b"not a profile\n").expect("outside marker");
        let m = core(true, FakePasswords::new(), FakeActionLog::new());
        let seed = text_of(INSTANCES_FILE);
        let before = listing();
        assert_eq!(
            m.set_server_profile(&desktop, "default", "nope"),
            no_profile("nope"),
            "no `server@nope.ini` anywhere: the name the caller asked for comes back"
        );
        for bad in ["", "../x", "a/b", "Big", "x.ini"] {
            assert_eq!(
                m.set_server_profile(&desktop, "default", bad),
                no_profile(bad),
                "a name that builds no profile file, named back: {bad:?}"
            );
        }
        assert_eq!(
            std::fs::read(dir.path().join("x.ini")).expect("outside marker"),
            b"not a profile\n",
            "a name that builds no profile built no path beside it"
        );
        assert_eq!(
            text_of(INSTANCES_FILE),
            seed,
            "every refusal above wrote nothing"
        );
        assert_eq!(listing(), before, "and created no file");
    }

    {
        let m = core(true, FakePasswords::new(), FakeActionLog::new());
        let seed = text_of(INSTANCES_FILE);
        let before = listing();
        assert_eq!(
            m.set_server_profile(&desktop, "default", "lan-2"),
            in_use("lan-2", "lan-2"),
            "a profile another instance runs, naming the profile and that instance"
        );
        assert_eq!(
            m.set_server_profile(&desktop, "lan-2", "default"),
            in_use("default", "default"),
            "the same rule read from the other section"
        );
        // The profile the instance already runs is no conflict with itself: the
        // switching instance is out of the walk.
        // The answer comes at once, from a write of nothing: a write of this
        // record is an atomic rename that replaces the file, so the record that
        // is still the same inode is the record the call never rewrote.
        #[cfg(unix)]
        let ino = {
            use std::os::unix::fs::MetadataExt;
            std::fs::metadata(sys.join(INSTANCES_FILE))
                .expect("the record")
                .ino()
        };
        assert_eq!(
            m.set_server_profile(&desktop, "default", "default")
                .map(|_| ()),
            Ok(()),
            "the profile it already runs is no conflict with itself"
        );
        #[cfg(unix)]
        {
            use std::os::unix::fs::MetadataExt;
            assert_eq!(
                std::fs::metadata(sys.join(INSTANCES_FILE))
                    .expect("the record")
                    .ino(),
                ino,
                "the profile it runs is answered at once: the record is the file that was there, not a copy written over it"
            );
        }
        assert_eq!(
            text_of(INSTANCES_FILE),
            seed,
            "the refusals above wrote nothing, and the accepted call changed no byte: {seed:?}"
        );
        assert_eq!(listing(), before, "and neither created a file");
    }

    {
        let m = core(true, FakePasswords::new(), FakeActionLog::new());
        let seed = text_of(INSTANCES_FILE);
        // `clash` is spare and sits on lan-2's 8081. A profile no instance runs
        // binds nothing, so its own save is judged against nothing: the switch
        // is where this address is refused.
        assert_eq!(
            m.set_server_profile(&desktop, "default", "clash"),
            taken("lan-2"),
            "a spare profile on the port of another instance, naming that instance"
        );
        // The web service: no `bff.ini` is its default address, 127.0.0.1:8590.
        std::fs::remove_file(framework.join("bff.ini")).ok();
        assert_eq!(
            m.set_server_profile(&desktop, "default", "on-bff"),
            taken(BFF_NAME),
            "the web service's address, named as itself"
        );
        // Its file names the address it holds: move the service to 9999 and
        // 8590 is free, while 9999 is the service's.
        std::fs::write(
            framework.join("bff.ini"),
            "[Bff]\nListen = 127.0.0.1:9999\n",
        )
        .expect("bff.ini");
        profile("on-listen", "[Server]\nPort = 9999\nHostname = 127.0.0.1\n");
        // The tree is snapshotted after this block's own seeds: the pin below
        // says the refusals added no file of their own, and `server@on-listen.ini`
        // is a seed of this block, not one of theirs.
        let before = listing();
        assert_eq!(
            m.set_server_profile(&desktop, "default", "on-listen"),
            taken(BFF_NAME),
            "the address `bff.ini` names is the one the service holds"
        );
        assert_eq!(
            text_of(INSTANCES_FILE),
            seed,
            "every refusal above wrote nothing"
        );
        assert_eq!(
            listing(),
            before,
            "and left no file of its own beside the record"
        );
        assert_eq!(
            m.set_server_profile(&desktop, "default", "on-bff")
                .map(|_| ()),
            Ok(()),
            "the service's default address is free once its file moved"
        );
        assert_eq!(
            text_of(INSTANCES_FILE),
            seed.replace("ServerProfile = default", "ServerProfile = on-bff"),
            "the accepted switch is the only key that moved"
        );
        std::fs::remove_file(framework.join("bff.ini")).ok();
    }

    {
        std::fs::write(sys.join(INSTANCES_FILE), SEEDED).expect(INSTANCES_FILE);
        let m = core(true, FakePasswords::new(), FakeActionLog::new());
        // `own` is spare and holds 8080, which `default` runs right now: the
        // switch binds nothing new, so it lands.
        assert_eq!(
            m.set_server_profile(&desktop, "default", "own").map(|_| ()),
            Ok(()),
            "a spare profile on the port the instance already runs"
        );
        assert_eq!(
            keys_of(&text_of(INSTANCES_FILE), "default", "ServerProfile"),
            vec!["ServerProfile = own".to_string()],
            "and the key is the new profile"
        );
        // Moved: `default` runs `own` now, so lan-2's 8081 is back in the walk.
        let seed = text_of(INSTANCES_FILE);
        assert_eq!(
            m.set_server_profile(&desktop, "default", "clash"),
            taken("lan-2"),
            "the instance's current profile is `own` now: lan-2 is not left out"
        );
        assert_eq!(text_of(INSTANCES_FILE), seed, "the refusal wrote nothing");
        // An instance whose profile cannot be read is skipped, as a profile
        // save skips it: nothing is judged on the defaults of a file that
        // cannot be read, so 8081 is nobody's and `clash` lands.
        std::fs::write(
            sys.join("server@lan-2.ini"),
            b"[Server]\nPort = 8081\n\xed\xa0\xbf\n",
        )
        .expect("unreadable lan-2 profile");
        assert_eq!(
            m.set_server_profile(&desktop, "default", "clash")
                .map(|_| ()),
            Ok(()),
            "an instance whose profile cannot be read binds no address"
        );
        std::fs::write(
            sys.join("server@lan-2.ini"),
            "[Server]\nPort = 8081\nHostname = 127.0.0.1\n",
        )
        .expect("lan-2 profile back");
    }

    {
        std::fs::write(sys.join(INSTANCES_FILE), SEEDED).expect(INSTANCES_FILE);
        let m = core(true, FakePasswords::new(), FakeActionLog::new());
        assert!(
            m.render_failures().is_empty(),
            "the record's renders are rewritten from what is on disk"
        );
        let kept = pair();
        // `default`'s render starts three model profiles, `tiny` caps them at 1.
        // The count is that instance's own fit, so the switch LANDS and the
        // instance is reported (IP3c): the cap of the server profile the record
        // names is measured against the list the instance starts, and the pair
        // that does not fit on it stays as it is.
        let answer = m.set_server_profile(&desktop, "default", "tiny");
        assert!(
            over_max(&m, &answer, "3", "1", "default"),
            "the new profile's cap under the loads the instance starts, the instance named last, and the count is the render's, not the pool's four: {answer:?}"
        );
        assert_eq!(
            keys_of(&text_of(INSTANCES_FILE), "default", "ServerProfile"),
            vec!["ServerProfile = tiny".to_string()],
            "the record's key landed: the cap that is news for one instance does not stop the write"
        );
        for (name, text) in kept {
            assert_eq!(
                text_of(&name),
                text,
                "and the instance keeps the pair it had: {name}"
            );
        }
        // The cap is each instance's own render's: the row names `default`
        // alone, whose render starts three. `lan-2` switched its three off, so
        // the same cap caps nothing for it and it is not reported.
        assert_eq!(
            m.not_converged_locked()
                .expect("the rows")
                .iter()
                .map(|row| row.instance.clone())
                .collect::<Vec<String>>(),
            vec!["default".to_string()],
            "the cap is judged on each instance's own list"
        );
        // The binding rule of this verb is intrinsic and unchanged (IP3c): the
        // profile the first switch landed on is the profile `default` runs, so
        // no other instance can take it.
        assert_eq!(
            m.set_server_profile(&desktop, "lan-2", "tiny"),
            in_use("tiny", "default"),
            "the profile the switch landed on is in use by that instance now"
        );
        // A profile with no `ModelsMax` caps nothing.
        assert_eq!(
            m.set_server_profile(&desktop, "default", "big").map(|_| ()),
            Ok(()),
            "no ModelsMax is no cap"
        );
        // A cap raised to the count lands: the rule is a cap check, judged on
        // what the instance starts.
        profile(
            "tiny",
            "[Server]\nPort = 8083\nHostname = 127.0.0.1\nModelsMax = 3\n",
        );
        assert_eq!(
            m.set_server_profile(&desktop, "lan-2", "roomy").map(|_| ()),
            Ok(()),
            "and the instance on the raised profile moves on to the cap of 3"
        );
        let written = text_of(INSTANCES_FILE);
        assert_eq!(
            section(&written, "default"),
            "[default]\nServerProfile = big\n; a note of its own\nnote = keep me\n\n",
            "the switching sections, each its own: {written:?}"
        );
        assert_eq!(
            section(&written, "lan-2"),
            "[lan-2]\nServerProfile = roomy\n",
            "the switch that landed for the refused instance's own id, in its own section, its only key the profile: {written:?}"
        );
        assert_eq!(
            written,
            SEEDED
                .replace("ServerProfile = default", "ServerProfile = big")
                .replace("ServerProfile = lan-2", "ServerProfile = roomy"),
            "the record as it stood, only the two `ServerProfile` keys changed: the leading comment, the `;` comment, the key of a section's own and the blank line in front of the next section are the bytes they were, and the switch that is news for one instance moved only its own key: {written:?}"
        );
        // The `a`/`b`/`c` keys are the assignments' and stay there: this verb
        // writes the record, never the assignments.
        assert_eq!(
            text_of("assignments.ini"),
            "[default]\n* = inherit\n\n[lan-2]\n* = inherit\na = off\nb = off\nc = off\n",
            "the assignments, where those keys live, are the bytes they were"
        );
    }

    {
        std::fs::write(sys.join(INSTANCES_FILE), SEEDED).expect(INSTANCES_FILE);
        let m = core(true, FakePasswords::new(), FakeActionLog::new());
        // The target profile with bytes that are no text. A profile read as
        // empty lands as a switch to a profile with no port and no cap, so a
        // file the manager cannot read must never be answered as an empty one.
        profile("big", "[Server]\nPort = 8082\nHostname = 127.0.0.1\n");
        std::fs::write(
            sys.join("server@big.ini"),
            b"[Server]\nPort = 8082\n\xed\xa0\xbf\n",
        )
        .expect("broken profile");
        let record = bytes_of(INSTANCES_FILE);
        let broken = bytes_of("server@big.ini");
        let before = listing();
        assert_eq!(
            m.set_server_profile(&desktop, "default", "big"),
            Err(ManagerError::Io(std::io::ErrorKind::InvalidData)),
            "a profile that cannot be read answers the read's own kind"
        );
        assert_eq!(
            bytes_of(INSTANCES_FILE),
            record,
            "the record is the bytes that were there: the atomic write never replaced them"
        );
        assert_eq!(
            bytes_of("server@big.ini"),
            broken,
            "and the profile the call read keeps its bytes"
        );
        assert_eq!(
            listing(),
            before,
            "and the call left no file of its own behind: no temp, no render"
        );
        // A directory at the profile's path: the same answer, and the path
        // stays what it is.
        std::fs::remove_file(sys.join("server@big.ini")).expect("big out");
        std::fs::create_dir(sys.join("server@big.ini")).expect("big dir");
        let before = listing();
        assert_eq!(
            m.set_server_profile(&desktop, "default", "big"),
            Err(ManagerError::Io(std::io::ErrorKind::IsADirectory)),
            "a directory at the profile's path answers the read's own kind"
        );
        assert!(
            sys.join("server@big.ini").is_dir(),
            "the directory is still there: nothing was renamed over it"
        );
        assert_eq!(
            listing(),
            before,
            "and the call left no temp file of its own next to it"
        );
        std::fs::remove_dir(sys.join("server@big.ini")).expect("big dir out");
        profile("big", "[Server]\nPort = 8082\nHostname = 127.0.0.1\n");
        // The record itself unreadable: the read's own kind, not the empty list
        // `paths::instance_ids` answers for a file it cannot read, and the
        // record is never replaced with one the manager made up.
        std::fs::write(
            sys.join(INSTANCES_FILE),
            b"[default]\nServerProfile = default\n\xed\xa0\xbf\n",
        )
        .expect("broken record");
        let record = bytes_of(INSTANCES_FILE);
        let before = listing();
        assert_eq!(
            m.set_server_profile(&desktop, "default", "big"),
            Err(ManagerError::Io(std::io::ErrorKind::InvalidData)),
            "a record that cannot be read answers its read's own kind, it is not a record of no instances"
        );
        assert_eq!(
            bytes_of(INSTANCES_FILE),
            record,
            "and its bytes are the bytes that were there"
        );
        assert_eq!(
            listing(),
            before,
            "and the call left no file of its own behind"
        );
        std::fs::write(sys.join(INSTANCES_FILE), SEEDED).expect(INSTANCES_FILE);
    }

    {
        // The cap check judges the instance's render, and the render is cut by
        // `assignments.ini`. A record there that cannot be read is not an empty
        // one: answering it as if nothing were assigned lets through a switch
        // the same call refuses while the file can be read.
        std::fs::write(sys.join(INSTANCES_FILE), SEEDED).expect(INSTANCES_FILE);
        let assignments =
            "[default]\n* = inherit\n\n[lan-2]\n* = inherit\na = off\nb = off\nc = off\n";
        std::fs::write(sys.join("assignments.ini"), assignments).expect("assignments.ini");
        profile(
            "tiny",
            "[Server]\nPort = 8083\nHostname = 127.0.0.1\nModelsMax = 1\n",
        );
        let m = core(true, FakePasswords::new(), FakeActionLog::new());
        // The fixture: with the record readable the switch goes as far as its
        // cap, which is the news this call reports.
        let answer = m.set_server_profile(&desktop, "default", "tiny");
        assert!(
            over_max(&m, &answer, "3", "1", "default"),
            "the fixture: the same switch reaches its cap while the assignments can be read: {answer:?}"
        );
        // The fixture's switch LANDED (IP3c), so the record names `tiny` now.
        // The calls below switch to it again, so the record goes back to the
        // profile it named, which is what makes them switches.
        std::fs::write(sys.join(INSTANCES_FILE), SEEDED).expect(INSTANCES_FILE);
        // Bytes that are no text. The cap check runs in the render pass now
        // (IP3c), and the pass reads the record with the convention every
        // render had before the fit checks moved here: a file it cannot read
        // reads as the empty one. So the switch LANDS, its key moves, and the
        // rows are the ones the pass answers over the render it read. Nothing
        // of the call's own is left behind.
        std::fs::write(
            sys.join("assignments.ini"),
            b"[default]\n* = inherit\n\xed\xa0\xbf\n",
        )
        .expect("broken assignments");
        let before = listing();
        let answer = m.set_server_profile(&desktop, "default", "tiny");
        assert!(
            answer.is_ok(),
            "an unreadable assignments.ini is the empty record the render pass has always              read, not a refusal of the switch: {answer:?}"
        );
        assert_eq!(
            keys_of(&text_of(INSTANCES_FILE), "default", "ServerProfile"),
            vec!["ServerProfile = tiny".to_string()],
            "the record's key landed"
        );
        assert_eq!(
            listing(),
            before,
            "and the call left no file of its own behind: no temp, no render"
        );
        std::fs::write(sys.join(INSTANCES_FILE), SEEDED).expect(INSTANCES_FILE);
        // A directory at the record's path: the pass reads it as nothing, and
        // the switch lands the same.
        std::fs::remove_file(sys.join("assignments.ini")).expect("assignments out");
        std::fs::create_dir(sys.join("assignments.ini")).expect("assignments dir");
        let before = listing();
        assert!(
            m.set_server_profile(&desktop, "default", "tiny").is_ok(),
            "a directory at the assignments path is the empty record the pass reads, not a              refusal"
        );
        assert_eq!(
            keys_of(&text_of(INSTANCES_FILE), "default", "ServerProfile"),
            vec!["ServerProfile = tiny".to_string()],
            "and its key landed"
        );
        assert_eq!(
            listing(),
            before,
            "and the call left no file of its own behind"
        );
        std::fs::remove_dir(sys.join("assignments.ini")).expect("assignments dir out");
        std::fs::write(sys.join(INSTANCES_FILE), SEEDED).expect(INSTANCES_FILE);
        std::fs::write(sys.join("assignments.ini"), assignments).expect("assignments.ini");
    }

    {
        // Two instances naming one server profile by hand: each asking for the
        // profile its own section already names is no conflict with the other,
        // whichever of them sorts first, and the switch to it changes no byte.
        let twin = "# the record as an editor left it\n[default]\nServerProfile = twin\n\n[zulu]\nServerProfile = twin\n";
        std::fs::write(sys.join(INSTANCES_FILE), twin).expect(INSTANCES_FILE);
        profile("twin", "[Server]\nPort = 8090\nHostname = 127.0.0.1\n");
        let m = core(true, FakePasswords::new(), FakeActionLog::new());
        let record = bytes_of(INSTANCES_FILE);
        assert_eq!(
            m.set_server_profile(&desktop, "default", "twin")
                .map(|_| ()),
            Ok(()),
            "the instance whose section sorts first asks for the profile it already names"
        );
        assert_eq!(
            m.set_server_profile(&desktop, "zulu", "twin").map(|_| ()),
            Ok(()),
            "so does the one whose section sorts last: it names that profile too, the first instance is not the holder for it"
        );
        assert_eq!(
            bytes_of(INSTANCES_FILE),
            record,
            "the switch to the profile a section already names changes no byte"
        );
        std::fs::write(sys.join(INSTANCES_FILE), SEEDED).expect(INSTANCES_FILE);
    }

    #[cfg(target_os = "linux")]
    {
        // The mode of the record the verb writes: the manager's config files
        // are read by every client, so a write lands them 0644 whatever the
        // process umask was, even over a file that was 0600. The umask is
        // process-wide, so it is masked only around the call and restored
        // before the first assert.
        use std::os::unix::fs::PermissionsExt;
        std::fs::write(sys.join(INSTANCES_FILE), SEEDED).expect(INSTANCES_FILE);
        std::fs::set_permissions(
            sys.join(INSTANCES_FILE),
            std::fs::Permissions::from_mode(0o600),
        )
        .expect("0600 record");
        let m = core(true, FakePasswords::new(), FakeActionLog::new());
        let before = listing();
        let guard = UmaskGuard::mask(nix::sys::stat::Mode::from_bits_retain(0o077));
        let answer = m.set_server_profile(&desktop, "default", "big").map(|_| ());
        let old_mask = guard.0;
        drop(guard);
        assert_eq!(
            nix::sys::stat::umask(old_mask),
            old_mask,
            "the window restored the umask"
        );
        let mode = std::fs::metadata(sys.join(INSTANCES_FILE))
            .ok()
            .map(|f| f.permissions().mode());
        let after = listing();
        assert_eq!(answer, Ok(()), "the fixture: the switch lands");
        assert_eq!(
            mode.map(|f| f & 0o777),
            Some(0o644),
            "the record was 0600 and the process was masked 0o077: the written instances.ini is 0644"
        );
        assert_eq!(
            after, before,
            "and the write left no temp file of its own beside it"
        );
    }

    {
        // A caller without the right: NotAuthorized, nothing written.
        let log = FakeActionLog::new();
        let lines = log.lines.clone();
        let m = core(false, FakePasswords::new(), log);
        let seed = text_of(INSTANCES_FILE);
        let before = listing();
        assert_eq!(
            m.set_server_profile(&desktop, "default", "big"),
            Err(ManagerError::NotAuthorized),
            "a caller outside `pai`, polkit refusing"
        );
        assert_eq!(
            text_of(INSTANCES_FILE),
            seed,
            "a refused caller writes nothing"
        );
        assert_eq!(listing(), before, "and leaves no file of its own behind");
        assert_eq!(
            records(&lines),
            vec![
                r#"action="set_server_profile" target="default" caller=":1.42" principal=- outcome=refused"#.to_string()
            ],
            "the refusal is recorded as one record too"
        );
    }
    {
        // The bff acting for a member: the pair grants the call, the record
        // names both.
        let p = FakePasswords::new();
        let asked_admin = p.asked_admin.clone();
        let log = FakeActionLog::new();
        let lines = log.lines.clone();
        let m = core(true, p, log);
        assert_eq!(
            m.set_server_profile(&member, "default", "big").map(|_| ()),
            Ok(()),
            "the bff switches the server profile for a member of `pai`"
        );
        assert_eq!(
            keys_of(&text_of(INSTANCES_FILE), "default", "ServerProfile"),
            vec!["ServerProfile = big".to_string()],
            "for the member"
        );
        assert_eq!(
            *asked_admin.lock().unwrap(),
            vec!["alice".to_string()],
            "the membership of the principal, never of the caller"
        );
        assert_eq!(
            *m.authority().asked_bff.lock().unwrap(),
            vec![bff.clone()],
            "the caller is checked first, at every call"
        );
        assert_eq!(
            m.authority().asked.lock().unwrap().len(),
            0,
            "polkit is never asked for a principal: its subject is the bff"
        );
        assert_eq!(
            records(&lines),
            vec![
                r#"action="set_server_profile" target="default" caller=":1.9" principal="alice" outcome=ok"#.to_string()
            ],
            "the record carries the caller and the principal"
        );

        // A principal outside `pai`: the call is refused and nothing is written.
        std::fs::write(sys.join(INSTANCES_FILE), SEEDED).expect(INSTANCES_FILE);
        let seed = text_of(INSTANCES_FILE);
        assert_eq!(
            m.set_server_profile(&outsider, "default", "own"),
            Err(ManagerError::NotAuthorized),
            "a right password of a caller outside `pai` is not enough"
        );
        assert_eq!(
            text_of(INSTANCES_FILE),
            seed,
            "a refused principal writes nothing"
        );

        // A caller that is not the bff cannot name a principal at all.
        let desktop_member = Requester {
            caller: desktop.clone(),
            principal: Some(Principal("alice".into())),
        };
        assert_eq!(
            m.set_server_profile(&desktop_member, "default", "own"),
            Err(ManagerError::NotAuthorized),
            "only the bff answers for a signed-in member"
        );
        assert_eq!(
            text_of(INSTANCES_FILE),
            seed,
            "and a caller outside the bff writes nothing"
        );
    }
}

/// The instance record: `instances.ini`, one section per instance. The list
/// comes from its sections: a `server@<profile>.ini` no section names is no
/// instance, and a section whose profile file is missing is an instance. A
/// client reads the file like `assignments.ini` and can never write it: every
/// write is refused as a bad name and the file stays byte for byte.
#[cfg(test)]
pub(crate) fn manager_instances_file_e2e() {
    use fakes::{FakeActionLog, FakeAuthority, FakeGroup, FakeResolver, FakeService};

    let dir = tempfile::tempdir().expect("tempdir");
    crate::paths::redirect_for_tests(dir.path());
    let sys = crate::paths::engine_config_dir();
    std::fs::create_dir_all(&sys).expect("engine config dir");
    let alice = Caller(":1.42".into());
    let log = FakeActionLog::new();
    let lines = log.lines.clone();
    let m = Manager::new(
        FakeService::new(),
        FakeAuthority::new(true),
        FakeResolver::lan(),
    )
    .with_group(FakeGroup::new())
    .with_log(log);
    // The path is joined here, not through `file_path`: that resolves a
    // served name, and `instances.ini` is served by the read today, so a
    // phase that asked it would stop at its first line.
    let instances = sys.join(INSTANCES_FILE);
    let read = |name: &str| std::fs::read_to_string(sys.join(name)).ok();

    // The record: default on profile default, lan-2 on profile big. big is a
    // profile no instance names, so it serves nothing.
    let ini = "[Server]\nPort = 8080\nHostname = 127.0.0.1\n";
    std::fs::write(sys.join("server@default.ini"), ini).expect("server profile");
    std::fs::write(sys.join("server@big.ini"), ini).expect("spare profile");
    std::fs::write(sys.join("models.ini"), "[M1]\nmodel = /models/m1.gguf\n").expect("models.ini");
    let record = "[default]\nServerProfile = default\n\n[lan-2]\nServerProfile = big\n";
    std::fs::write(&instances, record).expect("instances.ini");

    // The list is the sections: lan-2 is an instance with no server@lan-2.ini,
    // the profiles default and big alone list nothing beside it.
    assert_eq!(
        m.list_instances(),
        vec!["default".to_string(), "lan-2".to_string()],
        "the sections of instances.ini are the instances"
    );

    // A client reads it like assignments.ini: the text as it stands, with and
    // without an identity. It is no `ConfigFile`, so the gates a server
    // profile read runs never see it.
    assert_eq!(
        m.read_file(None::<&Caller>, INSTANCES_FILE),
        Ok(Some(plain(record))),
        "the file, to a caller-less client, as assignments.ini"
    );
    assert_eq!(
        m.read_file(Some(&alice), INSTANCES_FILE),
        Ok(Some(plain(record))),
        "the file, to a caller"
    );
    assert_eq!(
        config_file(INSTANCES_FILE),
        None,
        "the file is served by the read, not as a config file"
    );

    // A client never writes it: refused as a bad name, through the plain and
    // the compare-and-swap write, forced or not, the file untouched.
    let swapped = "[default]\nServerProfile = big\n";
    let base = lines.lock().unwrap().len();
    assert_eq!(
        m.write_file(&alice, INSTANCES_FILE, swapped, false),
        Err(ManagerError::BadFileName)
    );
    assert_eq!(
        m.write_file_if(&alice, INSTANCES_FILE, swapped, record),
        Err(ManagerError::BadFileName),
        "not through the compare and swap write either"
    );
    for force in [false, true] {
        assert_eq!(
            m.write_file(&alice, INSTANCES_FILE, swapped, force),
            Err(ManagerError::BadFileName),
            "keep_api_key {force}"
        );
    }
    let recorded: Vec<String> = lines.lock().unwrap()[base..].to_vec();
    assert_eq!(
        recorded.len(),
        4,
        "each refused write records one line: {recorded:?}"
    );
    for line in recorded.iter() {
        assert!(
            line.starts_with("action=\"write_file\"")
                || line.starts_with("action=\"write_file_if\""),
            "a write record: {line}"
        );
        assert!(
            line.contains("target=\"instances.ini\""),
            "the refused write names the record: {line}"
        );
        assert!(
            line.contains("outcome=\"BadFileName\""),
            "refused as a bad name: {line}"
        );
    }
    assert_eq!(
        read(INSTANCES_FILE).as_deref(),
        Some(record),
        "a refused write leaves the record"
    );

    // The record gone: the profiles alone are no instances, and the read of a
    // missing record is no content, as for assignments.ini.
    std::fs::remove_file(&instances).expect("remove the record");
    assert_eq!(
        m.list_instances(),
        Vec::<String>::new(),
        "the sections are the instances: none without the file"
    );
    assert_eq!(
        m.read_file(Some(&alice), INSTANCES_FILE),
        Ok(None),
        "the file missing: no content"
    );
}

/// The verbs that act on an instance ask whether the instance exists: that is
/// its section of `instances.ini`, not a `server@<id>.ini` named after the id.
/// So a section naming another profile is an instance whose settings file is
/// `server@<profile>.ini`; an id with no section is no instance even when a
/// file named after it stands in the directory; and a section naming a profile
/// whose file is missing IS an instance for the manager, because the file is
/// the launch's check, made when the unit starts (its refusal is
/// `launch::instance_command`'s own `NoConfig`, pinned in `launch.rs`).
#[cfg(test)]
pub(crate) fn manager_existing_id_e2e() {
    use fakes::{Call, FakeAuthority, FakeGroup, FakeResolver, FakeService};

    let dir = tempfile::tempdir().expect("tempdir");
    crate::paths::redirect_for_tests(dir.path());
    let sys = crate::paths::engine_config_dir();
    std::fs::create_dir_all(&sys).expect("engine config dir");
    let alice = Caller(":1.42".into());
    let m = Manager::new(
        FakeService::new(),
        FakeAuthority::new(true),
        FakeResolver::lan(),
    )
    .with_group(FakeGroup::new());
    // The recorded service calls, so a pin can say which verb reached the
    // service manager and under which name. Nothing real is started.
    let taken = || -> Vec<Call> { m.service().calls.lock().unwrap().clone() };
    // The four verbs that ask whether the instance exists (stop and status
    // ask only for a valid id, pinned by `manager_e2e`).
    type VerbAnswers = (
        Result<(), ManagerError>,
        Result<(), ManagerError>,
        Result<bool, ManagerError>,
        Result<(), ManagerError>,
    );
    let verbs = |id: &str| {
        (
            m.start(&alice, id),
            m.restart(&alice, id),
            m.boot_enabled(id),
            m.set_boot_enabled(&alice, id, true),
        )
    };
    let no_instance = || -> VerbAnswers {
        (
            Err(ManagerError::NoInstance),
            Err(ManagerError::NoInstance),
            Err(ManagerError::NoInstance),
            Err(ManagerError::NoInstance),
        )
    };
    let ini = "[Server]\nPort = 8080\nHostname = 127.0.0.1\n";
    let record = |text: &str| {
        std::fs::write(sys.join(INSTANCES_FILE), text).expect(INSTANCES_FILE);
    };

    // The record names `default` on the profile `big`, and only
    // `server@big.ini` is on disk: default is an instance, and its unit is
    // the one named by the id.
    std::fs::write(sys.join("server@big.ini"), ini).expect("server@big.ini");
    // The render, so a start here is a start: these pins are about the id.
    write_render(&sys, "default");
    record("[default]\nServerProfile = big\n");
    assert_eq!(
        m.list_instances(),
        ["default"],
        "the section is the instance"
    );
    assert_eq!(
        m.boot_enabled("default"),
        Ok(false),
        "a section naming a profile with no server@<id>.ini: not NoInstance"
    );
    assert_eq!(
        m.set_boot_enabled(&alice, "default", true),
        Ok(()),
        "the start at boot of that instance"
    );
    assert_eq!(m.boot_enabled("default"), Ok(true), "and it reads back");
    assert_eq!(
        m.start(&alice, "default"),
        Ok(()),
        "start of default, whose section names `big`: not NoInstance"
    );
    assert_eq!(
        m.restart(&alice, "default"),
        Ok(()),
        "restart of default: not NoInstance"
    );
    assert_eq!(
        taken(),
        vec![
            Call::BootEnabled("default".into()),
            Call::SetBootEnabled("default".into(), true),
            Call::BootEnabled("default".into()),
            Call::Start("default".into()),
            Call::Restart("default".into()),
        ],
        "the verbs reach the service under the instance id: the profile never names a unit"
    );

    // An id with no section is no instance, even with a `server@<id>.ini` in
    // the directory: the record is the fact, the file names nothing.
    std::fs::write(sys.join("server@orphan.ini"), ini).expect("server@orphan.ini");
    let asked = taken().len();
    assert_eq!(
        m.list_instances(),
        ["default"],
        "a profile file no section names is no instance"
    );
    assert_eq!(
        verbs("orphan"),
        no_instance(),
        "start, restart, boot_enabled and set_boot_enabled of an id with no section"
    );
    assert_eq!(
        taken().len(),
        asked,
        "a refused verb reaches no service call"
    );

    // A section naming a profile whose file is missing: the manager has the
    // instance, so the verbs reach the service. The missing file is refused by
    // the launch, when the unit starts and reads the profile (`launch.rs`:
    // `NoConfig` naming `server@huge.ini`).
    record("[default]\nServerProfile = huge\n");
    assert!(
        !sys.join("server@huge.ini").exists(),
        "fixture sanity: the profile has no file"
    );
    assert_eq!(
        m.start(&alice, "default"),
        Ok(()),
        "a section naming a profile with no file passes the existence check"
    );
    assert_eq!(
        &taken()[asked..],
        &[Call::Start("default".into())][..],
        "the start reaches the service manager: nothing is refused here"
    );
    // A section with no `ServerProfile` key is an instance the same way (the
    // listing reads sections, and the launch is the one that refuses a section
    // naming no profile, pinned in `launch.rs`).
    record("[default]\n");
    assert_eq!(
        m.start(&alice, "default"),
        Ok(()),
        "a section naming no profile is an instance for the manager"
    );

    // A name that is no instance id stays InvalidId, whatever the record holds
    // and whatever file stands in the directory: the id is checked first.
    record(
        "[Big]\nServerProfile = big\n\n[a/b]\nServerProfile = big\n\n[../x]\nServerProfile = big\n",
    );
    std::fs::write(sys.join("server@Big.ini"), ini).expect("server@Big.ini");
    for bad in ["Big", "a/b", "../x", ""] {
        assert_eq!(
            m.start(&alice, bad),
            Err(ManagerError::InvalidId),
            "start {bad:?}"
        );
        assert_eq!(
            m.restart(&alice, bad),
            Err(ManagerError::InvalidId),
            "restart {bad:?}"
        );
        assert_eq!(
            m.boot_enabled(bad),
            Err(ManagerError::InvalidId),
            "boot_enabled {bad:?}"
        );
        assert_eq!(
            m.set_boot_enabled(&alice, bad, false),
            Err(ManagerError::InvalidId),
            "set_boot_enabled {bad:?}"
        );
    }

    // The record gone: every id is no instance, and a file named after one
    // stands in for nothing.
    std::fs::remove_file(sys.join(INSTANCES_FILE)).expect("remove the record");
    assert!(
        sys.join("server@orphan.ini").is_file(),
        "fixture sanity: the file is still there"
    );
    assert_eq!(
        verbs("orphan"),
        no_instance(),
        "no record: the file named after the id names no instance"
    );
}

/// E2E phase: the port check and the `ModelsMax` check follow the profile an
/// instance names in `instances.ini`, not the file named after its id. So two
/// profiles no instance uses share a port freely, a profile in use collides
/// with the profile of another instance and the refusal names the INSTANCE, an
/// instance whose profile cannot be read is skipped and never judged on the
/// defaults of a missing file, and a file named after an id that is not that
/// instance's profile is never read. Instances `default` and `lan` run the
/// profiles `big` and `small`: the names differ on purpose. The port of a
/// profile in use is a binding rule of the save itself, so it stays a refusal;
/// the `ModelsMax` is a FIT check of each instance's render (IP3c,
/// `ARCHITECTURE.md` "Convergence"): the save that brings a cap under a
/// standing count LANDS and reports nothing (what stands is no news), the save
/// that grows a count over the cap LANDS and reports the instance whose render
/// does not fit.
///
/// The pins here are IP3c: every write lands, and the rows of the write's own
/// render pass are what the retired refusals asserted.
#[cfg(test)]
pub(crate) fn manager_profiles_in_use_e2e() {
    use fakes::{FakeAuthority, FakeGroup, FakeResolver, FakeService};

    let dir = tempfile::tempdir().expect("tempdir");
    crate::paths::redirect_for_tests(dir.path());
    let sys = crate::paths::engine_config_dir();
    std::fs::create_dir_all(&sys).expect("engine config dir");
    let framework = crate::paths::system_config_dir();
    let alice = Caller(":1.42".into());
    let m = Manager::new(
        FakeService::new(),
        FakeAuthority::new(true),
        FakeResolver::lan(),
    )
    .with_group(FakeGroup::new());
    let read = |name: &str| std::fs::read_to_string(sys.join(name)).ok();
    let record = |text: &str| {
        std::fs::write(sys.join(INSTANCES_FILE), text).expect(INSTANCES_FILE);
    };
    let profile = |name: &str, text: &str| {
        std::fs::write(sys.join(format!("server@{name}.ini")), text).expect("server profile");
    };
    // The cap news this phase is about, its three args whole. The cap is a fit
    // check of ONE instance's render (IP3c): the save LANDS and the write's own
    // render pass reports the instance the count is over, with the check's key
    // and the same three args. True when `m` reports that row.
    let over_max = |m: &Manager<FakeService, FakeAuthority, FakeResolver>,
                    count: &str,
                    max: &str,
                    id: &str|
     -> bool {
        m.not_converged_locked()
            .expect("the rows")
            .iter()
            .any(|row| {
                row.instance == id
                    && row.key == "load-on-startup-over-max"
                    && row.args == [count.to_string(), max.to_string(), id.to_string()]
            })
    };
    // The rows the last write's pass reported, as instance names.
    let rows = |m: &Manager<FakeService, FakeAuthority, FakeResolver>| -> Vec<String> {
        m.not_converged_locked()
            .expect("the rows")
            .iter()
            .map(|row| format!("{} {} {}", row.instance, row.key, row.args.join(",")))
            .collect()
    };
    let flagged = |ids: &[&str]| -> String {
        real_model_files(
            dir.path(),
            &ids.iter()
                .map(|id| format!("[{id}]\nmodel = /models/{id}.gguf\nload-on-startup = true\n"))
                .collect::<String>(),
        )
    };
    // default on big (8081), lan on small (8082).
    let in_use = "[default]\nServerProfile = big\n\n[lan]\nServerProfile = small\n";
    let big_8081 = "[Server]\nPort = 8081\nHostname = 127.0.0.1\nModelsMax = 8\n";
    record(in_use);
    profile("big", big_8081);
    profile(
        "small",
        "[Server]\nPort = 8082\nHostname = 127.0.0.1\nModelsMax = 8\n",
    );

    // ── A profile no instance uses is checked against nothing ─────────────
    for name in ["spare-a", "spare-b"] {
        assert_eq!(
            m.write_file(
                &alice,
                &format!("server@{name}.ini"),
                "[Server]\nPort = 8081\nHostname = 127.0.0.1\n",
                false
            ),
            Ok(()),
            "{name}: two unused profiles with one port both save, they are \
             alternatives for one instance"
        );
    }
    assert_eq!(
        m.write_file(
            &alice,
            "server@spare-c.ini",
            "[Server]\nPort = 8082\nHostname = 127.0.0.1\n",
            false
        ),
        Ok(()),
        "an unused profile may share the port of a profile in use"
    );

    // ── A profile in use collides with another instance's profile ─────────
    assert_eq!(
        m.write_file(
            &alice,
            "server@big.ini",
            "[Server]\nPort = 8082\nHostname = 127.0.0.1\nModelsMax = 8\n",
            false
        ),
        Err(ManagerError::AddressInUse("lan".into())),
        "the refusal names the INSTANCE that runs the colliding profile, \
         never the profile's name"
    );
    assert_eq!(
        read("server@big.ini").as_deref(),
        Some(big_8081),
        "a refused save writes nothing"
    );
    assert_eq!(
        m.write_file(
            &alice,
            "server@small.ini",
            "[Server]\nPort = 8081\nHostname = 127.0.0.1\nModelsMax = 8\n",
            false
        ),
        Err(ManagerError::AddressInUse("default".into())),
        "the other way round names default, the instance running big"
    );
    // An instance never collides with itself, and its id is not the profile's
    // name: rewriting big with the address default already binds is its own.
    assert_eq!(
        m.write_file(&alice, "server@big.ini", big_8081, false),
        Ok(()),
        "big is the profile in use by default: it is not a second address"
    );

    // ── A section that cannot name a readable profile is skipped ──────────
    // ghost's file is missing, noprofile has no key, bogus an invalid value:
    // none of them is judged on the defaults of a missing file, so none of
    // them (all of them on the built-in default address) holds a save whose
    // port is that default. The save is of big, a profile in use, so the
    // check runs and walks those sections.
    record(&format!(
        "{in_use}\n[ghost]\nServerProfile = ghost\n\n[noprofile]\n\n[bogus]\nServerProfile = ../x\n"
    ));
    let on_default_port = format!(
        "[Server]\nPort = {}\nHostname = 127.0.0.1\nModelsMax = 8\n",
        crate::server_cfg::DEFAULT_PORT
    );
    assert_eq!(
        m.write_file(&alice, "server@big.ini", &on_default_port, false),
        Ok(()),
        "a missing profile file, a section with no ServerProfile and one with \
         an invalid value refuse nothing: none is judged on the defaults of a \
         missing file"
    );
    // The same port on a profile no instance uses: accepted as well.
    assert_eq!(
        m.write_file(&alice, "server@fresh.ini", &on_default_port, false),
        Ok(()),
        "and an unused profile is checked against nothing at all"
    );
    assert_eq!(
        m.write_file(&alice, "server@big.ini", big_8081, false),
        Ok(()),
        "fixture: big back on 8081"
    );

    // ── A file named after an instance, when its profile is another one ───
    // lan runs small on 8091; server@lan.ini stands on 8090 and is no part of
    // lan, so a profile in use may take 8090.
    profile(
        "small",
        "[Server]\nPort = 8091\nHostname = 127.0.0.1\nModelsMax = 8\n",
    );
    profile("lan", "[Server]\nPort = 8090\nHostname = 127.0.0.1\n");
    assert_eq!(
        m.write_file(
            &alice,
            "server@big.ini",
            "[Server]\nPort = 8090\nHostname = 127.0.0.1\nModelsMax = 8\n",
            false
        ),
        Ok(()),
        "server@lan.ini is not lan's profile: it is never read"
    );
    assert_eq!(
        m.write_file(&alice, "server@big.ini", big_8081, false),
        Ok(()),
        "fixture: big back on 8081"
    );

    // ── The bff's own listen address ──────────────────────────────────────
    std::fs::create_dir_all(&framework).expect("framework config dir");
    std::fs::write(
        framework.join("bff.ini"),
        "[Bff]\nListen = 127.0.0.1:8590\n",
    )
    .expect("bff.ini");
    assert_eq!(
        m.write_file(
            &alice,
            "server@big.ini",
            "[Server]\nPort = 8590\nHostname = 127.0.0.1\nModelsMax = 8\n",
            false
        ),
        Err(ManagerError::AddressInUse("private-ai-bff".into())),
        "the bff's listen address holds its port against a profile in use, \
         named as itself"
    );
    assert_eq!(
        read("server@big.ini").as_deref(),
        Some(big_8081),
        "a refused save writes nothing"
    );
    assert_eq!(
        m.write_file(&alice, "server@big.ini", big_8081, false),
        Ok(()),
        "a profile in use on another port saves with bff.ini in place"
    );
    // The address the bff serves, as `crates/bff/src/listen.rs` reads the file.
    let on_bff_port = "[Server]\nPort = 8590\nHostname = 127.0.0.1\nModelsMax = 8\n";

    // No bff.ini at all, and one whose [Bff] section sets no Listen, both leave
    // the bff on its default 127.0.0.1:8590: that address holds its port.
    std::fs::remove_file(framework.join("bff.ini")).expect("remove bff.ini");
    assert_eq!(
        m.write_file(&alice, "server@big.ini", on_bff_port, false),
        Err(ManagerError::AddressInUse("private-ai-bff".into())),
        "no bff.ini: the bff is on 127.0.0.1:8590 by default and holds it \
         against a profile in use"
    );
    std::fs::write(framework.join("bff.ini"), "[Bff]\nHosts = box.local\n").expect("bff.ini");
    assert_eq!(
        m.write_file(&alice, "server@big.ini", on_bff_port, false),
        Err(ManagerError::AddressInUse("private-ai-bff".into())),
        "a bff.ini that sets no Listen leaves the same default address: the \
         same refusal"
    );

    // The wildcard binds every address, so a bff on 0.0.0.0:8590 collides with
    // a profile bound to 127.0.0.1:8590 (`addresses_collide`); a bff bound to
    // 127.0.0.1:8590 leaves the same port on another address free.
    std::fs::write(framework.join("bff.ini"), "[Bff]\nListen = 0.0.0.0:8590\n").expect("bff.ini");
    assert_eq!(
        m.write_file(&alice, "server@big.ini", on_bff_port, false),
        Err(ManagerError::AddressInUse("private-ai-bff".into())),
        "a bff on 0.0.0.0:8590 collides with a profile in use on \
         127.0.0.1:8590"
    );
    assert_eq!(
        read("server@big.ini").as_deref(),
        Some(big_8081),
        "a refused save writes nothing"
    );
    std::fs::write(
        framework.join("bff.ini"),
        "[Bff]\nListen = 127.0.0.1:8590\n",
    )
    .expect("bff.ini");
    assert_eq!(
        m.write_file(
            &alice,
            "server@big.ini",
            "[Server]\nPort = 8590\nHostname = 192.168.1.5\nModelsMax = 8\n",
            false
        ),
        Ok(()),
        "the bff on 127.0.0.1:8590 does not hold 192.168.1.5:8590"
    );
    assert_eq!(
        m.write_file(&alice, "server@big.ini", big_8081, false),
        Ok(()),
        "fixture: big back on 8081"
    );

    // A Listen the bff will not start on, and a bff.ini that cannot be read,
    // refuse no save: the bff answers those by not starting, the manager has
    // no address to hold. An empty value is no address and not the default.
    std::fs::write(
        framework.join("bff.ini"),
        "[Bff]\nListen = box.local:8590\n",
    )
    .expect("bff.ini");
    assert_eq!(
        m.write_file(&alice, "server@big.ini", on_bff_port, false),
        Ok(()),
        "a Listen that is no ip:port, a host name: the save lands"
    );
    assert_eq!(
        m.write_file(&alice, "server@big.ini", big_8081, false),
        Ok(()),
        "fixture: big back on 8081"
    );
    std::fs::write(framework.join("bff.ini"), "[Bff]\nListen =\n").expect("bff.ini");
    assert_eq!(
        m.write_file(&alice, "server@big.ini", on_bff_port, false),
        Ok(()),
        "an empty Listen is no address and not the default: the save lands"
    );
    assert_eq!(
        m.write_file(&alice, "server@big.ini", big_8081, false),
        Ok(()),
        "fixture: big back on 8081"
    );
    // The file is a directory: reading it fails, and the manager asks nothing
    // of a file it cannot read.
    std::fs::remove_file(framework.join("bff.ini")).expect("remove bff.ini");
    std::fs::create_dir_all(framework.join("bff.ini")).expect("directory as bff.ini");
    assert_eq!(
        m.write_file(&alice, "server@big.ini", on_bff_port, false),
        Ok(()),
        "a bff.ini that cannot be read refuses no save"
    );
    assert_eq!(
        read("server@big.ini").as_deref(),
        Some(on_bff_port),
        "and it lands"
    );
    std::fs::remove_dir(framework.join("bff.ini")).expect("remove the directory");

    // The bff's port is free for a profile no instance uses.
    std::fs::write(
        framework.join("bff.ini"),
        "[Bff]\nListen = 127.0.0.1:8590\n",
    )
    .expect("bff.ini");
    assert_eq!(
        m.write_file(&alice, "server@spare-d.ini", on_bff_port, false),
        Ok(()),
        "an unused profile on the bff's port saves: it is checked against \
         nothing at all"
    );
    assert_eq!(
        m.write_file(&alice, "server@big.ini", big_8081, false),
        Ok(()),
        "fixture: big back on 8081, the bff's port left on an unused profile"
    );

    // ── ModelsMax of a profile in use ─────────────────────────────────────
    std::fs::write(
        sys.join("assignments.ini"),
        "[default]\n* = inherit\n\n[lan]\n",
    )
    .expect("assignments.ini");
    std::fs::write(sys.join("models.ini"), flagged(&["a", "b"])).expect("models.ini");
    let capped = "[Server]\nPort = 8081\nHostname = 127.0.0.1\nModelsMax = 1\n";
    assert_eq!(
        m.write_file(&alice, "server@big.ini", capped, false),
        Ok(()),
        "lowered under what default loads: the save lands, what stands is no news"
    );
    assert_eq!(
        read("server@big.ini").as_deref(),
        Some(capped),
        "the cap that used to refuse the save now lets it through"
    );
    assert!(
        over_max(&m, "2", "1", "default"),
        "the pair on disk is the render of the pool that stood before the fixture wrote it, so          default's two loads are news over the cap the save brings, and the instance is reported          while the save lands"
    );
    assert_eq!(
        m.write_file(
            &alice,
            "server@spare-a.ini",
            "[Server]\nPort = 8081\nHostname = 127.0.0.1\nModelsMax = 1\n",
            false
        ),
        Ok(()),
        "the same save on an unused profile: no instance loads anything from it"
    );

    // ── A model-profile write is judged against the profile the instance names
    // big goes to a max of 2 (default loads two: legal), and a misleading
    // server@default.ini (max 1) stands beside it: the profile is the cap.
    assert_eq!(
        m.write_file(
            &alice,
            "server@big.ini",
            "[Server]\nPort = 8081\nHostname = 127.0.0.1\nModelsMax = 2\n",
            false
        ),
        Ok(()),
        "fixture: big's max down to the two loads default makes"
    );
    profile(
        "default",
        "[Server]\nPort = 8080\nHostname = 127.0.0.1\nModelsMax = 1\n",
    );
    assert_eq!(
        m.write_file(&alice, "models.ini", &flagged(&["a", "b"]), false),
        Ok(()),
        "big has room for the two default loads: the file named after the id is not read"
    );
    assert_eq!(
        m.write_file(&alice, "models.ini", &flagged(&["a", "b", "c"]), false),
        Ok(()),
        "the pool the write names lands: the cap is the render's fit, not the save's"
    );
    assert!(
        over_max(&m, "3", "2", "default"),
        "the row carries the max of the profile the instance names, and names the instance"
    );
    assert_eq!(
        read("models.ini").as_deref(),
        Some(flagged(&["a", "b", "c"]).as_str()),
        "the pool that is over an instance's cap lands, that instance keeping its pair"
    );
    // The other pool file is a client's read, never a write: the model data
    // model retired it (`ARCHITECTURE.md`, "Instance profiles").
    assert_eq!(
        m.write_file(&alice, "models.ini.disabled", &flagged(&["f"]), false),
        Err(ManagerError::BadFileName),
        "the switched-off file is not served for a write"
    );

    // ── An instance whose profile cannot be read is skipped ───────────────
    // ghost's profile file is missing and broken names an invalid one: with
    // five loads in the pool neither is judged on the defaults of a missing
    // file, so neither refuses the write on its own account.
    record(
        "[default]\nServerProfile = big\n\n[ghost]\nServerProfile = ghost\n\n\
         [broken]\nServerProfile = ../x\n",
    );
    assert_eq!(
        m.write_file(&alice, "server@big.ini", big_8081, false),
        Ok(()),
        "fixture: big's max back to 8, which fits the five loads below"
    );
    std::fs::write(
        sys.join("assignments.ini"),
        "[default]\n* = inherit\n\n[ghost]\n* = inherit\n\n[broken]\n* = inherit\n",
    )
    .expect("assignments.ini");
    assert_eq!(
        m.write_file(
            &alice,
            "models.ini",
            &flagged(&["a", "b", "c", "d", "e"]),
            false
        ),
        Ok(()),
        "an instance with no readable profile refuses nothing"
    );
    assert_eq!(
        read("models.ini").as_deref(),
        Some(flagged(&["a", "b", "c", "d", "e"]).as_str()),
        "and it lands"
    );

    // ── Two instances run one profile: it is one binding, not two ────────
    // a and b both name shared (8081), c names other (8082). They are the
    // same address, so they never collide with each other; c is the one
    // holding 8082. The two pools of the record above are left behind.
    record(
        "[a]\nServerProfile = shared\n\n[b]\nServerProfile = shared\n\n\
         [c]\nServerProfile = other\n",
    );
    let shared_8081 = "[Server]\nPort = 8081\nHostname = 127.0.0.1\nModelsMax = 8\n";
    profile("shared", shared_8081);
    profile(
        "other",
        "[Server]\nPort = 8082\nHostname = 127.0.0.1\nModelsMax = 8\n",
    );
    std::fs::write(
        sys.join("assignments.ini"),
        "[a]\n* = inherit\n\n[b]\n* = inherit\n\n[c]\n* = inherit\n",
    )
    .expect("assignments.ini");
    std::fs::write(sys.join("models.ini"), flagged(&["m1", "m2"])).expect("models.ini");
    assert_eq!(
        m.write_file(&alice, "server@shared.ini", shared_8081, false),
        Ok(()),
        "a and b run shared on 127.0.0.1:8081: one binding, they are never \
         two colliding addresses"
    );
    assert_eq!(
        m.write_file(
            &alice,
            "server@shared.ini",
            "[Server]\nPort = 8082\nHostname = 127.0.0.1\nModelsMax = 8\n",
            false
        ),
        Err(ManagerError::AddressInUse("c".into())),
        "shared on other's port: the refusal names c, the instance running \
         other, and never a or b, which run the profile being saved"
    );
    assert_eq!(
        read("server@shared.ini").as_deref(),
        Some(shared_8081),
        "a refused save writes nothing"
    );

    // ── One profile, two instances: each instance's own load count ────────
    // Both a and b inherit the pool, so each loads two model profiles on
    // startup. A max of 1 is under the count of each, and the cap of the pair
    // on disk is 8: the save lands and the standing counts are no news (IP3c),
    // so nothing is reported. The cap is each instance's own, judged on its own
    // list, which the assignments cut.
    let shared_max_1 = "[Server]\nPort = 8081\nHostname = 127.0.0.1\nModelsMax = 1\n";
    assert_eq!(
        m.write_file(&alice, "server@shared.ini", shared_max_1, false),
        Ok(()),
        "under what a and b load: the save lands"
    );
    assert_eq!(
        read("server@shared.ini").as_deref(),
        Some(shared_max_1),
        "the cap of a profile in use lands with the instance over it"
    );
    assert_eq!(
        rows(&m),
        Vec::<String>::new(),
        "both counts stood under a cap of 8, so neither instance is over the new one"
    );
    // a given one of the two, b left with both: the cap is each one's own, and
    // a count that shrinks is never news, so the same save stays quiet.
    std::fs::write(
        sys.join("assignments.ini"),
        "[a]\nm1 = inherit\n\n[b]\n* = inherit\n",
    )
    .expect("assignments.ini");
    assert_eq!(
        m.write_file(&alice, "server@shared.ini", shared_max_1, false),
        Ok(()),
        "under what b loads: the save lands, a and b each judged on their own list"
    );
    assert_eq!(
        rows(&m),
        Vec::<String>::new(),
        "a's count shrank and b's stood, neither is news over the cap that stands"
    );

    // ── An unused profile is still the file's own content ─────────────────
    // No instance names spare-x, so it binds nothing and caps nothing; a
    // Hostname that does not resolve is a content error of the file, whoever
    // uses it: the save stays refused, naming the host.
    assert_eq!(
        m.write_file(
            &alice,
            "server@spare-x.ini",
            "[Server]\nPort = 8095\nHostname = nowhere.example\n",
            false
        ),
        Err(ManagerError::UnresolvableHost("nowhere.example".into())),
        "an unused profile whose Hostname does not resolve is refused"
    );
    assert!(
        !sys.join("server@spare-x.ini").exists(),
        "refused: not written"
    );
}

/// A render that cannot be written (its target is a directory, the disk is
/// full): the render does not stop there and it is not silent. Every instance
/// after it in id order is rendered, each failure comes back with its
/// instance, and the manager's start reports each one, naming the instance.
#[cfg(test)]
pub(crate) fn manager_render_failure_e2e() {
    use fakes::{FakeActionLog, FakeAuthority, FakeGroup, FakeResolver, FakeService};
    use pai_core::assign::render;

    let dir = tempfile::tempdir().expect("tempdir");
    crate::paths::redirect_for_tests(dir.path());
    let sys = crate::paths::engine_config_dir();
    std::fs::create_dir_all(&sys).expect("engine config dir");
    let alice = Caller(":1.42".into());
    let log = FakeActionLog::new();
    let lines = log.lines.clone();
    let m = Manager::new(
        FakeService::new(),
        FakeAuthority::new(true),
        FakeResolver::lan(),
    )
    .with_group(FakeGroup::new())
    .with_log(log);
    let read = |name: &str| std::fs::read_to_string(sys.join(name)).ok();
    let records =
        |lines: &std::sync::Arc<std::sync::Mutex<Vec<String>>>| lines.lock().unwrap().clone();
    let listing = |dir: &std::path::Path| -> Vec<String> {
        let mut names: Vec<String> = std::fs::read_dir(dir)
            .map(|rd| {
                rd.flatten()
                    .map(|e| e.file_name().to_string_lossy().into_owned())
                    .collect()
            })
            .unwrap_or_default();
        names.sort();
        names
    };
    // Three instances: the id order the render walks is default, lan-2,
    // lan-3, so lan-3 sits after the two renders broken below. Each section
    // is told its own model profile, and lan-3 is told a model profile only the later pool
    // has: its render says, in its own text, whether the render ran.
    let assignments = "[default]\na = inherit\n\n[lan-2]\nb = inherit\n\n[lan-3]\nd = inherit\n";
    let section = |id: &str| -> String {
        pai_core::ini::text::section_text(assignments, id).unwrap_or_default()
    };
    let pool = real_model_files(dir.path(), "[a]\nmodel = /models/a.gguf\n\n[b]\nmodel = /models/b.gguf\n\n[c]\nmodel = /models/c.gguf\n");
    // The pool a hand edit puts on disk, and the one a client write puts
    // there: d arrives with the hand edit, e with the write.
    let hand = real_model_files(
        dir.path(),
        &format!("{pool}\n[d]\nmodel = /models/d.gguf\n"),
    );
    let written = real_model_files(
        dir.path(),
        &format!("{hand}\n[e]\nmodel = /models/e.gguf\n"),
    );

    std::fs::write(sys.join("assignments.ini"), assignments).expect("assignments.ini");
    write_record(&sys, &["default", "lan-2", "lan-3"]);
    assert_eq!(m.write_file(&alice, "models.ini", &pool, false), Ok(()));
    for (id, port) in [("default", 8081u16), ("lan-2", 8082), ("lan-3", 8083)] {
        let ini = format!("[Server]\nPort = {port}\nHostname = 127.0.0.1\n");
        assert_eq!(
            m.write_file(&alice, &format!("server@{id}.ini"), &ini, false),
            Ok(()),
            "{id} created"
        );
    }
    assert_eq!(
        read("models@lan-3.ini"),
        Some(render(&pool, "", Some(section("lan-3").as_str()))),
        "every instance rendered, before anything is broken"
    );

    // Two renders cannot land, each in the place of a file: the atomic write
    // cannot rename a file onto a directory. A mode change is no fixture, the
    // service runs as root and root ignores modes.
    std::fs::write(sys.join("models.ini"), &hand).expect("hand-edited pool");
    for id in ["default", "lan-2"] {
        let path = sys.join(format!("models@{id}.ini"));
        std::fs::rename(&path, sys.join(format!("models@{id}.ini.held")))
            .expect("move the render aside");
        std::fs::create_dir(&path).expect("directory in a render's place");
    }

    // ── The start render: the instances after a failure are rendered ─────
    let failures = m.render_failures();
    let ids: Vec<String> = failures.iter().map(|(id, _)| id.clone()).collect();
    assert_eq!(
        ids,
        vec!["default".to_string(), "lan-2".to_string()],
        "one failure per instance whose render did not land, in id order, \
         the first failure first: {failures:?}"
    );
    assert!(
        failures
            .iter()
            .all(|(_, err)| matches!(err, ManagerError::Io(_))),
        "a render that cannot be written is an I/O failure: {failures:?}"
    );
    assert_eq!(
        read("models@lan-3.ini"),
        Some(render(&hand, "", Some(section("lan-3").as_str()))),
        "lan-3, after both failures in id order, holds the pool on disk"
    );
    for id in ["default", "lan-2"] {
        let path = sys.join(format!("models@{id}.ini"));
        assert!(
            path.is_dir(),
            "the directory that blocked {id} stands: {path:?}"
        );
        assert_eq!(
            listing(&path),
            Vec::<String>::new(),
            "a render that failed writes nothing inside the directory that blocked it"
        );
    }
    assert!(
        !listing(&sys).iter().any(|name| name.contains(".tmp")),
        "a failed render leaves no temp file: {:?}",
        listing(&sys)
    );

    // ── The manager's start reports each failure, naming the instance ────
    let before = records(&lines).len();
    m.report_render_failures(&failures);
    let recs = records(&lines);
    let reported: Vec<&str> = recs[before..].iter().map(String::as_str).collect();
    assert_eq!(
        reported.len(),
        2,
        "one line per failed render: {reported:?}"
    );
    for id in ["default", "lan-2"] {
        let named: Vec<&&str> = reported.iter().filter(|line| line.contains(id)).collect();
        assert_eq!(named.len(), 1, "exactly one line names {id}: {reported:?}");
        assert!(
            named[0].contains("render") && named[0].contains("Io"),
            "the line names the render and how it failed: {:?}",
            named[0]
        );
    }
    assert!(
        reported.iter().all(|line| !line.contains("lan-3")),
        "an instance that was rendered is not reported: {reported:?}"
    );

    // ── A write of a pool file: it lands, the render runs past a failure ─
    // The caller is told the render failed, with the first failure in id
    // order; the write itself stands, and every instance after the failures
    // is rendered with it.
    let answer = m.write_file(&alice, "models.ini", &written, false);
    assert_eq!(
        answer,
        Err(failures[0].1.clone()),
        "the caller is told the first render failure"
    );
    assert_eq!(
        read("models.ini").as_deref(),
        Some(written.as_str()),
        "the write itself landed: the render that failed is not its failure"
    );
    assert_eq!(
        read("models@lan-3.ini"),
        Some(render(&written, "", Some(section("lan-3").as_str()))),
        "the write's render runs past both failures"
    );

    // ── A start where every render lands reports nothing ─────────────────
    for id in ["default", "lan-2"] {
        std::fs::remove_dir(sys.join(format!("models@{id}.ini"))).expect("remove the blocker");
    }
    let clean = m.render_failures();
    assert_eq!(
        clean,
        Vec::<(String, ManagerError)>::new(),
        "with every render landing, nothing failed: {clean:?}"
    );
    let before_clean = records(&lines).len();
    m.report_render_failures(&clean);
    assert_eq!(
        records(&lines).len(),
        before_clean,
        "a start with no failed render records nothing"
    );
    assert_eq!(
        read("models@default.ini"),
        Some(render(&written, "", Some(section("default").as_str()))),
        "default's render lands once its path is free"
    );
}

/// E2E phase for acting for a signed-in user in the core: a trusted caller
/// (the bff's account) naming a `pai` member, over the files a guarded call
/// touches, so it redirects the data root (`src/tests/mod.rs`). Every seam is
/// a fake: no polkit, no systemd, no real account is asked.
#[cfg(test)]
pub(crate) fn manager_principal_e2e() {
    use fakes::{Call, FakeAuthority, FakeGroup, FakePasswords, FakeResolver, FakeService};

    let dir = tempfile::tempdir().expect("tempdir");
    crate::paths::redirect_for_tests(dir.path());
    let sys = crate::paths::engine_config_dir();
    std::fs::create_dir_all(&sys).expect("engine config dir");
    let ini = "[Server]\nPort = 8080\nHostname = 127.0.0.1\nOpencodeApiKey = sk-local-3f9a\n";
    let model_profiles = real_model_files(
        dir.path(),
        "[Qwen3.8-27B]\nmodel = /m/Qwen3.8-27B-Q8_0.gguf\n",
    );
    let other = real_model_files(
        dir.path(),
        "[Qwen3.8-32B]\nmodel = /m/Qwen3.8-32B-Q4_K_M.gguf\n",
    );
    // A model profile text no block of this phase writes on purpose: a refused write
    // of it is visible as the file on disk staying as `other`.
    let third = real_model_files(
        dir.path(),
        "[Qwen3.8-14B]\nmodel = /m/Qwen3.8-14B-Q4_K_M.gguf\n",
    );
    std::fs::write(sys.join("server@default.ini"), ini).expect("instance file");
    std::fs::write(sys.join("models.ini"), &model_profiles).expect("presets file");
    write_record(&sys, &["default"]);
    // A start loads the instance's render, which the manager renders from the
    // pool and the assignment: these pins are about the caller, so `default`
    // has a model profile assigned and the render that names it.
    std::fs::write(
        sys.join("assignments.ini"),
        "[default]\nQwen3.8-27B = on\nQwen3.8-32B = on\nQwen3.8-14B = on\n",
    )
    .expect("assignments");
    write_render(&sys, "default");
    let text_of = |name: &str| std::fs::read_to_string(sys.join(name)).expect("read");

    // The bff's account (a bus name here, the service SID on Windows) and its
    // request for alice, a member of `pai` in [`FakePasswords`].
    let bff = Caller(":1.9".into());
    let alice = Requester {
        caller: bff.clone(),
        principal: Some(Principal("alice".into())),
    };
    let bob = Requester {
        caller: bff.clone(),
        principal: Some(Principal("bob".into())),
    };
    let core = |allow: bool, quiet: bool, passwords: FakePasswords| {
        Manager::new(
            FakeService::new(),
            FakeAuthority::new(allow).quietly(quiet).bff(&bff),
            FakeResolver::lan(),
        )
        .with_group(FakeGroup::new())
        .with_passwords(passwords)
    };
    let log = |m: &Manager<FakeService, FakeAuthority, FakeResolver>| {
        m.service().calls.lock().unwrap().clone()
    };
    let polkit = |m: &Manager<FakeService, FakeAuthority, FakeResolver>| {
        m.authority().asked.lock().unwrap().clone()
    };
    let polkit_quiet = |m: &Manager<FakeService, FakeAuthority, FakeResolver>| {
        m.authority().asked_quietly.lock().unwrap().clone()
    };

    // ── The bff acting for a member: the right is the pair ──────────────
    // The bff's own polkit rights are allowed here and irrelevant: the pair
    // (the bff's account, a current `pai` member) is what grants the call, so
    // polkit is never asked, not even about the action the call needs.
    {
        let p = FakePasswords::new();
        let asked_admin = p.asked_admin.clone();
        let m = core(true, true, p);
        assert_eq!(
            m.start(&alice, "default"),
            Ok(()),
            "the bff starts an instance for a member"
        );
        assert_eq!(
            m.stop(&alice, "default"),
            Ok(()),
            "the bff stops an instance for a member"
        );
        assert_eq!(
            m.restart(&alice, "default"),
            Ok(()),
            "the bff restarts an instance for a member"
        );
        assert_eq!(
            m.set_boot_enabled(&alice, "default", true),
            Ok(()),
            "the bff sets start at boot for a member"
        );
        assert_eq!(
            log(&m),
            vec![
                Call::Start("default".into()),
                Call::Stop("default".into()),
                Call::Restart("default".into()),
                Call::SetBootEnabled("default".into(), true)
            ],
            "every call reaches the service manager"
        );
        assert_eq!(
            polkit(&m),
            vec![],
            "polkit is never asked for a principal: its subject is the bff"
        );
        assert_eq!(
            polkit_quiet(&m),
            vec![],
            "not even the question asked without a prompt"
        );
        assert_eq!(
            *m.authority().asked_bff.lock().unwrap(),
            vec![bff.clone(); 4],
            "the caller is checked first, at every call"
        );
        assert_eq!(
            *asked_admin.lock().unwrap(),
            vec!["alice".to_string(); 4],
            "the membership of the principal, never of the caller"
        );
    }

    // ── A principal's read is never withheld ─────────────────────────────
    // Here the bff is one polkit would challenge for `configure` (quiet
    // false): the member's read of an instance file still carries the key.
    {
        let m = core(true, false, FakePasswords::new());
        let read = m.read_file(Some(&alice), "server@default.ini");
        assert_eq!(
            read,
            Ok(Some(plain(ini))),
            "a principal's read of an instance file holds its key"
        );
        let file = read.unwrap().expect("read");
        assert!(!file.withheld, "nothing withheld from a member's read");
        assert!(
            file.text.contains("sk-local-3f9a"),
            "the key blanked: {file:?}"
        );
        assert_eq!(
            polkit_quiet(&m),
            vec![],
            "configure is not asked to decide a principal's read"
        );
        assert_eq!(polkit(&m), vec![], "control is not asked either");
    }

    // ── A principal call needs no right of the bff's own ───────────────
    // This bff has no polkit right at all: a member principal writes.
    {
        let m = core(false, false, FakePasswords::new());
        assert_eq!(
            m.write_file(&alice, "models.ini", &other, false),
            Ok(()),
            "granting the bff `configure` for this is what the pair replaces"
        );
        assert_eq!(text_of("models.ini"), other, "the write landed");
        assert_eq!(
            m.read_file(Some(&alice), "server@default.ini"),
            Ok(Some(plain(ini))),
            "the write is not the only call a member's principal opens"
        );
        assert_eq!(polkit(&m), vec![], "no polkit right of the bff is asked");
    }

    // ── The bff naming a non-member: refused, nothing done ─────────────
    {
        let p = FakePasswords::new();
        let asked_admin = p.asked_admin.clone();
        let m = core(true, true, p);
        assert_eq!(
            m.start(&bob, "default"),
            Err(ManagerError::NotAuthorized),
            "bob is no `pai` member"
        );
        assert_eq!(
            m.write_file(&bob, "models.ini", &model_profiles, false),
            Err(ManagerError::NotAuthorized),
            "writing needs the same membership"
        );
        assert_eq!(
            m.read_file(Some(&bob), "server@default.ini"),
            Err(ManagerError::NotAuthorized),
            "a non-member reads nothing, not even a blanked file"
        );
        assert_eq!(
            log(&m),
            vec![],
            "the refused start reached no service manager"
        );
        assert_eq!(
            text_of("models.ini"),
            other,
            "the refused write changed no file"
        );
        assert_eq!(polkit(&m), vec![], "polkit decides nothing here");
        assert_eq!(
            *asked_admin.lock().unwrap(),
            vec!["bob".to_string(); 3],
            "the membership is asked once per call"
        );
    }

    // ── Fail closed: a member removed between two calls ────────────────
    {
        let p = FakePasswords::new();
        let admins = p.admins.clone();
        let asked_admin = p.asked_admin.clone();
        let m = core(true, true, p);
        assert_eq!(
            m.start(&alice, "default"),
            Ok(()),
            "a member's first call works"
        );
        admins.lock().unwrap().remove("alice");
        assert_eq!(
            m.start(&alice, "default"),
            Err(ManagerError::NotAuthorized),
            "the next call is refused: the manager caches no membership"
        );
        assert_eq!(
            log(&m),
            vec![Call::Start("default".into())],
            "the refused call started nothing"
        );
        assert_eq!(
            *asked_admin.lock().unwrap(),
            vec!["alice".to_string(); 2],
            "membership looked up again at the second call"
        );
    }

    // ── A caller that is not the bff naming a member: refused, unwritten ──
    // The three real transports' identities that are not the bff's account: a
    // D-Bus bus name, the LocalSystem SID, and the pipe's caller with no
    // identity. The pair is the only right a principal call has, so naming a
    // member grants these nothing, and the refusal answers before the account
    // behind the name is looked up.
    {
        let p = FakePasswords::new();
        let asked_admin = p.asked_admin.clone();
        let m = core(true, true, p);
        let outsiders = [
            Caller(":1.42".into()),
            Caller("S-1-5-18".into()),
            Caller(String::new()),
        ];
        for caller in outsiders.clone() {
            let who = Requester {
                caller,
                principal: Some(Principal("alice".into())),
            };
            assert_eq!(
                m.write_file(&who, "models.ini", &third, false),
                Err(ManagerError::NotAuthorized),
                "{who:?}: only the bff vouches for a user"
            );
        }
        assert_eq!(
            text_of("models.ini"),
            other,
            "no caller outside the bff wrote the preset file"
        );
        assert_eq!(
            *asked_admin.lock().unwrap(),
            Vec::<String>::new(),
            "the caller is refused before the account behind the name is looked up"
        );
        assert_eq!(
            *m.authority().asked_bff.lock().unwrap(),
            outsiders.to_vec(),
            "the caller is the question asked, once per call"
        );
        assert_eq!(
            polkit(&m),
            vec![],
            "the gate, not polkit, is what refused these writes"
        );
    }

    // ── The bff naming an illegible name: refused, unwritten ─────────────
    // No name at all, and a name carrying a NUL, the character that would
    // split one record into two wherever the manager writes who acted. The
    // shape of the name answers, so the accounts are never asked: a name that
    // is no name is never answered from them.
    {
        let p = FakePasswords::new();
        let asked_admin = p.asked_admin.clone();
        let m = core(true, true, p);
        for name in ["", "ali\u{0}ce"] {
            assert_eq!(
                m.write_file(
                    &Requester {
                        caller: bff.clone(),
                        principal: Some(Principal(name.into())),
                    },
                    "models.ini",
                    &third,
                    false
                ),
                Err(ManagerError::NotAuthorized),
                "{name:?}: an illegible name is refused"
            );
        }
        assert_eq!(
            text_of("models.ini"),
            other,
            "an illegible name wrote nothing"
        );
        assert_eq!(
            *asked_admin.lock().unwrap(),
            Vec::<String>::new(),
            "the name is checked for shape before the account behind it is looked up"
        );
        assert_eq!(
            *m.authority().asked_bff.lock().unwrap(),
            vec![bff.clone(); 2],
            "the caller is the bff here: the name is what fails"
        );
        assert_eq!(polkit(&m), vec![], "polkit is not asked for a refused name");
    }

    // ── A model profile file read is gated too ──────────────────────────────────
    // The model profile files need no authorization of the caller, so the principal
    // gate is all that stands between these requests and the file: a
    // non-member's read and a caller outside the bff's read answer nothing.
    {
        let p = FakePasswords::new();
        let asked_admin = p.asked_admin.clone();
        let m = core(true, true, p);
        assert_eq!(
            m.read_file(Some(&bob), "models.ini"),
            Err(ManagerError::NotAuthorized),
            "a non-member reads no preset file, not even a blanked one"
        );
        assert_eq!(
            *asked_admin.lock().unwrap(),
            vec!["bob".to_string()],
            "the membership is what refused this read"
        );
        assert_eq!(
            polkit(&m),
            vec![],
            "polkit decides nothing about a preset file"
        );
    }
    {
        let p = FakePasswords::new();
        let asked_admin = p.asked_admin.clone();
        let m = core(true, true, p);
        assert_eq!(
            m.read_file(
                Some(&Requester {
                    caller: Caller(":1.42".into()),
                    principal: Some(Principal("alice".into())),
                }),
                "models.ini"
            ),
            Err(ManagerError::NotAuthorized),
            "a caller outside the bff reads nothing as alice"
        );
        assert_eq!(
            *asked_admin.lock().unwrap(),
            Vec::<String>::new(),
            "a caller outside the bff never reaches the membership lookup"
        );
        assert_eq!(
            m.read_file(Some(&alice), "models.ini"),
            Ok(Some(plain(&other))),
            "the member the bff names does read it: the gate refuses these calls, not all of them"
        );
        assert_eq!(
            *asked_admin.lock().unwrap(),
            vec!["alice".to_string()],
            "the read that is answered is the one that looked the membership up"
        );
    }
}

/// E2E phase for the log of who acted: every guarded call (start, stop,
/// restart, read_file, write_file, set_boot_enabled), done or refused,
/// writes exactly one record through the `ActionLog` seam installed with
/// `Manager::with_log`, naming the action, the target (instance id or file
/// name), the caller, the principal (`-` when none) and the outcome. Values
/// are written with Rust's `{:?}` quoting, so a caller or principal
/// carrying a quote, a newline or a NUL cannot forge a second record. Never
/// a password, never file contents. Unguarded calls record nothing. The
/// record format is literal text, as the comments written into the served
/// files: administrators and scripts read it.
#[cfg(test)]
pub(crate) fn manager_action_log_e2e() {
    use fakes::{
        FakeActionLog, FakeAuthority, FakeGroup, FakePasswords, FakeResolver, FakeService,
    };

    let dir = tempfile::tempdir().expect("tempdir");
    crate::paths::redirect_for_tests(dir.path());
    let sys = crate::paths::engine_config_dir();
    std::fs::create_dir_all(&sys).expect("engine config dir");
    let ini = "[Server]\nPort = 8080\nHostname = 127.0.0.1\nOpencodeApiKey = sk-local-3f9a\n";
    let model_profiles = real_model_files(
        dir.path(),
        "[Qwen3.8-27B]\nmodel = /m/Qwen3.8-27B-Q8_0.gguf\n",
    );
    let other = real_model_files(
        dir.path(),
        "[Qwen3.8-32B]\nmodel = /m/Qwen3.8-32B-Q4_K_M.gguf\n",
    );
    std::fs::write(sys.join("server@default.ini"), ini).expect("instance file");
    std::fs::write(sys.join("models.ini"), &model_profiles).expect("presets file");
    write_record(&sys, &["default"]);
    // A start loads the instance's render, which the manager renders from the
    // pool and the assignment: these pins are about the caller, so `default`
    // has a model profile assigned and the render that names it.
    std::fs::write(
        sys.join("assignments.ini"),
        "[default]\nQwen3.8-27B = on\nQwen3.8-32B = on\nQwen3.8-14B = on\n",
    )
    .expect("assignments");
    write_render(&sys, "default");
    let text_of = |name: &str| std::fs::read_to_string(sys.join(name)).expect("read");

    let bff = Caller(":1.9".into());
    let alice = Requester {
        caller: bff.clone(),
        principal: Some(Principal("alice".into())),
    };
    let bob = Requester {
        caller: bff.clone(),
        principal: Some(Principal("bob".into())),
    };
    let desktop = Caller(":1.42".into());

    // A manager wired to a fresh fake log: the records land in the `lines`
    // handle the test keeps beside it.
    let core = |allow: bool, passwords: FakePasswords, service: FakeService, log: FakeActionLog| {
        Manager::new(
            service,
            FakeAuthority::new(allow).quietly(true).bff(&bff),
            FakeResolver::lan(),
        )
        .with_group(FakeGroup::new())
        .with_passwords(passwords)
        .with_log(log)
    };
    let records =
        |lines: &std::sync::Arc<std::sync::Mutex<Vec<String>>>| lines.lock().unwrap().clone();

    // ── One record per guarded call done, in the pinned format ──────────
    {
        let log = FakeActionLog::new();
        let lines = log.lines.clone();
        let m = core(true, FakePasswords::new(), FakeService::new(), log);
        assert_eq!(m.start(&alice, "default"), Ok(()));
        assert_eq!(m.stop(&alice, "default"), Ok(()));
        assert_eq!(m.restart(&alice, "default"), Ok(()));
        assert_eq!(
            m.set_boot_enabled(&alice, "default", true),
            Ok(()),
            "start at boot is a guarded call too"
        );
        assert_eq!(m.write_file(&alice, "models.ini", &other, false), Ok(()));
        assert_eq!(
            m.read_file(Some(&alice), "server@default.ini"),
            Ok(Some(plain(ini)))
        );
        assert_eq!(
            m.read_file(Some(&alice), "models.ini"),
            Ok(Some(plain(&other))),
            "the preset file needs no right, yet the read of it is recorded"
        );
        let recs = records(&lines);
        let got: Vec<&str> = recs.iter().map(String::as_str).collect();
        assert_eq!(
            got,
            vec![
                r#"action="start" target="default" caller=":1.9" principal="alice" outcome=ok"#,
                r#"action="stop" target="default" caller=":1.9" principal="alice" outcome=ok"#,
                r#"action="restart" target="default" caller=":1.9" principal="alice" outcome=ok"#,
                r#"action="set_boot_enabled" target="default" caller=":1.9" principal="alice" outcome=ok"#,
                r#"action="write_file" target="models.ini" caller=":1.9" principal="alice" outcome=ok"#,
                r#"action="read_file" target="server@default.ini" caller=":1.9" principal="alice" outcome=ok"#,
                r#"action="read_file" target="models.ini" caller=":1.9" principal="alice" outcome=ok"#,
            ],
            "one record per guarded call: action, target (id or file name), caller, principal, outcome, in this order, all values {{:?}}-quoted, ok unquoted"
        );
        for line in records(&lines) {
            assert!(!line.contains('\n'), "a record is one line: {line:?}");
            assert!(!line.contains('\0'), "a record carries no NUL: {line:?}");
        }
    }

    // ── Refusals and failures: one record each, refused or the error name ─
    {
        let mut service = FakeService::new();
        service.failing = vec!["default".into()];
        let log = FakeActionLog::new();
        let lines = log.lines.clone();
        let m = core(true, FakePasswords::new(), service, log);
        assert_eq!(m.start(&bob, "default"), Err(ManagerError::NotAuthorized));
        assert_eq!(
            m.write_file(&bob, "models.ini", &model_profiles, false),
            Err(ManagerError::NotAuthorized)
        );
        assert_eq!(
            m.read_file(Some(&bob), "server@default.ini"),
            Err(ManagerError::NotAuthorized)
        );
        assert_eq!(
            m.start(&alice, "ghost"),
            Err(ManagerError::NoInstance),
            "a valid id with no server@ghost.ini"
        );
        assert_eq!(
            m.start(&alice, "../x"),
            Err(ManagerError::InvalidId),
            "not an id at all"
        );
        assert_eq!(
            m.write_file(&alice, "nope.ini", &model_profiles, false),
            Err(ManagerError::BadFileName)
        );
        let failed = m
            .start(&alice, "default")
            .expect_err("the fake service fails");
        assert!(
            matches!(failed, ManagerError::Service(_)),
            "the service manager's own failure: {failed:?}"
        );
        // logtest joins the record: a save of a profile no instance runs is
        // judged against nothing, so the collision with default would pass.
        write_record(&sys, &["default", "logtest"]);
        assert_eq!(
            m.write_file(&alice, "server@logtest.ini", ini, false),
            Err(ManagerError::AddressInUse("default".into())),
            "the new instance collides with default on 127.0.0.1:8080"
        );
        assert_eq!(
            m.read_file(None::<&Caller>, "server@default.ini"),
            Err(ManagerError::NotAuthorized),
            "a request with no requester reads nothing"
        );
        let recs = records(&lines);
        let got: Vec<&str> = recs.iter().map(String::as_str).collect();
        assert_eq!(
            got,
            vec![
                r#"action="start" target="default" caller=":1.9" principal="bob" outcome=refused"#,
                r#"action="write_file" target="models.ini" caller=":1.9" principal="bob" outcome=refused"#,
                r#"action="read_file" target="server@default.ini" caller=":1.9" principal="bob" outcome=refused"#,
                r#"action="start" target="ghost" caller=":1.9" principal="alice" outcome="NoInstance""#,
                r#"action="start" target="../x" caller=":1.9" principal="alice" outcome="InvalidId""#,
                r#"action="write_file" target="nope.ini" caller=":1.9" principal="alice" outcome="BadFileName""#,
                r#"action="start" target="default" caller=":1.9" principal="alice" outcome="Service""#,
                r#"action="write_file" target="server@logtest.ini" caller=":1.9" principal="alice" outcome="AddressInUse""#,
                r#"action="read_file" target="server@default.ini" caller=- principal=- outcome=refused"#,
            ],
            "a refusal is recorded too: `refused` for NotAuthorized, the error's own name (quoted) for a failure, with the target as requested and `caller=-` for a request with no requester"
        );
    }
    // The record is one instance again: the blocks below list the instances.
    write_record(&sys, &["default"]);

    // ── The caller's own call: recorded with `principal=-` ───────────────
    // The files here are as the blocks above left them: `models.ini` holds
    // `other`, the text the first block wrote (this block runs after it, in
    // this one phase).
    {
        let log = FakeActionLog::new();
        let lines = log.lines.clone();
        let m = core(true, FakePasswords::new(), FakeService::new(), log);
        assert_eq!(m.start(&desktop, "default"), Ok(()));
        assert_eq!(m.stop(&desktop, "default"), Ok(()));
        assert_eq!(
            m.read_file(Some(&desktop), "server@default.ini"),
            Ok(Some(plain(ini))),
            "a desktop reader here may configure, so nothing is withheld"
        );
        assert_eq!(
            m.read_file(Some(&desktop), "models.ini"),
            Ok(Some(plain(&other))),
            "a preset file needs no right, yet it is recorded like any guarded call"
        );
        let recs = records(&lines);
        let got: Vec<&str> = recs.iter().map(String::as_str).collect();
        assert_eq!(
            got,
            vec![
                r#"action="start" target="default" caller=":1.42" principal=- outcome=ok"#,
                r#"action="stop" target="default" caller=":1.42" principal=- outcome=ok"#,
                r#"action="read_file" target="server@default.ini" caller=":1.42" principal=- outcome=ok"#,
                r#"action="read_file" target="models.ini" caller=":1.42" principal=- outcome=ok"#,
            ],
            "a call with no principal is recorded with `principal=-`"
        );
    }
    {
        let log = FakeActionLog::new();
        let lines = log.lines.clone();
        let m = core(false, FakePasswords::new(), FakeService::new(), log);
        assert_eq!(
            m.start(&desktop, "default"),
            Err(ManagerError::NotAuthorized),
            "polkit refuses this caller"
        );
        assert_eq!(
            records(&lines),
            vec![
                r#"action="start" target="default" caller=":1.42" principal=- outcome=refused"#
                    .to_string()
            ],
            "the refused call of a caller with no principal is recorded, refused"
        );
    }

    // ── Never a password, never file contents ────────────────────────────
    // The text written and read here carries a marker: the write really
    // lands (the file on disk proves the record below exists for a real
    // write), and no record names the marker.
    {
        let secret = "sk-LOG-NEVER-9d21";
        let secret_model_profiles = real_model_files(
            dir.path(),
            &format!("[Qwen3.8-27B]\nmodel = /m/{secret}.gguf\n"),
        );
        let secret_ini = ini.replace("sk-local-3f9a", secret);
        let log = FakeActionLog::new();
        let lines = log.lines.clone();
        let m = core(true, FakePasswords::new(), FakeService::new(), log);
        assert_eq!(
            m.write_file(&alice, "models.ini", &secret_model_profiles, false),
            Ok(())
        );
        assert_eq!(
            m.write_file(&alice, "server@default.ini", &secret_ini, false),
            Ok(())
        );
        assert_eq!(
            m.read_file(Some(&alice), "server@default.ini"),
            Ok(Some(plain(&secret_ini)))
        );
        assert_eq!(
            m.write_file(&bob, "models.ini", &secret_model_profiles, false),
            Err(ManagerError::NotAuthorized),
            "the refused write of a secret is a record as well"
        );
        assert!(
            text_of("server@default.ini").contains(secret),
            "the write of the secret landed: its record is below"
        );
        let recs = records(&lines);
        let got: Vec<&str> = recs.iter().map(String::as_str).collect();
        assert_eq!(
            got,
            vec![
                r#"action="write_file" target="models.ini" caller=":1.9" principal="alice" outcome=ok"#,
                r#"action="write_file" target="server@default.ini" caller=":1.9" principal="alice" outcome=ok"#,
                r#"action="read_file" target="server@default.ini" caller=":1.9" principal="alice" outcome=ok"#,
                r#"action="write_file" target="models.ini" caller=":1.9" principal="bob" outcome=refused"#,
            ]
        );
        for line in records(&lines) {
            assert!(
                !line.contains(secret),
                "a record carries the target and the outcome, never the text: {line:?}"
            );
        }
    }

    // ── A record cannot be forged: names are written {:?}-quoted ─────────
    // Each hostile value runs ONE guarded call on a manager of its own: the
    // fake log must hold exactly one record, on one line, with the value
    // written whole (Rust's own escaping, pinned by formatting the same
    // value here).
    {
        let cases: Vec<(Caller, Option<Principal>, &str)> = vec![
            (bff.clone(), Some(Principal("ali\"ce".into())), "refused"),
            (bff.clone(), Some(Principal("ali\nce".into())), "refused"),
            (bff.clone(), Some(Principal("ali\u{0}ce".into())), "refused"),
            (Caller(":1.9\"x".into()), None, "ok"),
            (Caller(":1.9\nx".into()), None, "ok"),
            (Caller(":1.9\u{0}x".into()), None, "ok"),
        ];
        for (caller, principal, outcome) in cases {
            let who = Requester {
                caller: caller.clone(),
                principal: principal.clone(),
            };
            let log = FakeActionLog::new();
            let lines = log.lines.clone();
            let m = core(true, FakePasswords::new(), FakeService::new(), log);
            let answer = m.start(&who, "default");
            if outcome == "ok" {
                assert_eq!(answer, Ok(()), "{who:?}: the call itself runs");
            } else {
                assert_eq!(
                    answer,
                    Err(ManagerError::NotAuthorized),
                    "{who:?}: the principal gate refuses"
                );
            }
            let recs = records(&lines);
            assert_eq!(
                recs.len(),
                1,
                "{who:?}: exactly one record, a hostile name forged no second"
            );
            let line = &recs[0];
            assert!(!line.contains('\n'), "{who:?}: one line: {line:?}");
            assert!(!line.contains('\0'), "{who:?}: no raw NUL: {line:?}");
            assert!(
                line.contains(&format!("caller={:?}", caller.0)),
                "the caller written {{:?}}-quoted in: {line}"
            );
            if let Some(named) = &principal {
                assert!(
                    line.contains(&format!("principal={:?}", named.0)),
                    "the principal written {{:?}}-quoted in: {line}"
                );
            }
            assert!(
                line.ends_with(&format!("outcome={outcome}")),
                "the record ends with the outcome: {line}"
            );
        }
        // The quote is no control character: that principal is a well formed
        // name, so the refusal is the account lookup, and the record shows
        // the name exactly as asked, escaped inside the quoted value.
        let p = FakePasswords::new();
        let asked_admin = p.asked_admin.clone();
        let log = FakeActionLog::new();
        let lines = log.lines.clone();
        let m = core(true, p, FakeService::new(), log);
        assert_eq!(
            m.start(
                &Requester {
                    caller: bff.clone(),
                    principal: Some(Principal("ali\"ce".into())),
                },
                "default"
            ),
            Err(ManagerError::NotAuthorized)
        );
        assert_eq!(
            *asked_admin.lock().unwrap(),
            vec!["ali\"ce".to_string()],
            "refused as an unknown account, not as an illegible name"
        );
        assert_eq!(
            records(&lines),
            vec![
                r#"action="start" target="default" caller=":1.9" principal="ali\"ce" outcome=refused"#
                    .to_string()
            ],
            "the quote is escaped inside the quoted value: one line, one record"
        );
    }

    // ── Unguarded calls record nothing ───────────────────────────────────
    // The list, the reads, the sign-in: no record, so the sign-in password
    // is nowhere in the log. `Version` is answered by the transport faces
    // (manager/pipe.rs, manager/dbus.rs), not by the core, and reaches no
    // record.
    {
        let log = FakeActionLog::new();
        let lines = log.lines.clone();
        let m = core(true, FakePasswords::new(), FakeService::new(), log);
        assert_eq!(m.list_instances(), vec!["default".to_string()]);
        assert_eq!(m.status("default"), Ok(UnitState::Active));
        assert_eq!(m.status("../x"), Err(ManagerError::InvalidId));
        assert_eq!(m.boot_enabled("default"), Ok(false));
        assert_eq!(
            m.authenticate(&bff, "alice", "s3cret-Pw"),
            Ok((true, true)),
            "the sign-in that succeeds"
        );
        assert_eq!(
            m.authenticate(&desktop, "alice", "s3cret-Pw"),
            Err(ManagerError::NotAuthorized),
            "the sign-in refused to a caller that is not the bff"
        );
        assert_eq!(
            records(&lines),
            Vec::<String>::new(),
            "no record for a call that is not guarded, so no password ever reaches the log"
        );
        for line in records(&lines) {
            assert!(
                !line.contains("s3cret-Pw"),
                "the sign-in password is never in a record: {line:?}"
            );
        }
    }

    // ── A literal format, not a strings-file text ────────────────────────
    // Administrators and scripts read the records, like the comments written
    // into the served files, so the rendering is literal text in this file's
    // production code (before the first `#[cfg(test)]`: the fakes, the phases
    // and this test are cut away) and asks no strings file.
    {
        let source = std::include_str!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/src/manager/service.rs"
        ));
        let production = source
            .split("\n#[cfg(test)]\n")
            .next()
            .expect("production text of service.rs");
        for field in [
            concat!("act", "ion="),
            concat!("tar", "get="),
            concat!("call", "er="),
            concat!("prin", "cipal="),
            concat!("outc", "ome="),
        ] {
            assert!(
                production.contains(field),
                "the record renders {field:?} as literal text, in the code that builds the line"
            );
        }
        for call in [concat!("strings", "::t("), concat!("strings", "::fill(")] {
            assert!(
                !production.contains(call),
                "the record is not a strings-file text: {call:?} has no place in this file"
            );
        }
    }

    // ── The file sink under the data root: the path and the records ──────
    // The Windows service appends its records to a file (Linux writes stderr,
    // the journal reads that). Its path is derived from the framework's
    // machine-wide directory, taken through `paths::system_config_dir`, so the
    // redirect this phase set keeps the file inside this temp tree: the real
    // machine tree is never touched. One line per guarded call, appended, as
    // the sinks' own tests pin for a plain `Write`.
    {
        use super::super::action_log::{manager_log_path, FileActionLog, MANAGER_LOG_FILE};

        let path = manager_log_path();
        assert!(
            path.starts_with(dir.path()),
            "the record file is inside the redirected data root, never the real machine \
             tree: {path:?}"
        );
        assert_eq!(
            path,
            dir.path().join("logs").join(MANAGER_LOG_FILE),
            "a `logs` directory beside the framework's machine directory (its config dir \
             is `<root>/system-config`, so the root is its parent), named after the service"
        );

        // A manager whose log is the file sink, at the path above.
        let file_core = |allow: bool| {
            Manager::new(
                FakeService::new(),
                FakeAuthority::new(allow).quietly(true).bff(&bff),
                FakeResolver::lan(),
            )
            .with_log(FileActionLog::new(&path))
        };

        let m = file_core(true);
        assert_eq!(m.stop(&desktop, "default"), Ok(()));
        assert_eq!(m.restart(&desktop, "default"), Ok(()));
        let text = std::fs::read_to_string(&path).expect("the file the sink created");
        assert_eq!(
            text,
            "action=\"stop\" target=\"default\" caller=\":1.42\" principal=- outcome=ok\n\
             action=\"restart\" target=\"default\" caller=\":1.42\" principal=- outcome=ok\n",
            "one record per guarded call, whole lines, appended in order"
        );

        // A refusal reaches the file too, as the line the core renders for it.
        let m = file_core(false);
        assert_eq!(
            m.stop(&desktop, "default"),
            Err(ManagerError::NotAuthorized),
            "polkit refuses this caller"
        );
        let text = std::fs::read_to_string(&path).expect("the file after the refusal");
        assert_eq!(
            text.lines().count(),
            3,
            "the refusal is one more line: {text:?}"
        );
        assert_eq!(
            text.lines().last(),
            Some(r#"action="stop" target="default" caller=":1.42" principal=- outcome=refused"#),
            "the refusal is recorded with the answer the caller got: {text:?}"
        );

        // A later run opens the same path and appends: nothing is truncated.
        let m = file_core(true);
        assert_eq!(m.stop(&desktop, "default"), Ok(()));
        let text = std::fs::read_to_string(&path).expect("the file after the second run");
        assert_eq!(
            text.lines().count(),
            4,
            "a fresh sink over the same path appends, it never truncates: {text:?}"
        );
        assert!(
            text.starts_with(
                "action=\"stop\" target=\"default\" caller=\":1.42\" principal=- outcome=ok\n\
                 action=\"restart\""
            ),
            "the first run's lines are still there: {text:?}"
        );
        assert!(
            !text.contains("sk-local-3f9a"),
            "a record names the file it touched, never its contents: {text:?}"
        );

        // A record that cannot be written is lost and the action stands: the
        // path's parent is a regular file, so no log file can be made.
        let blocker = dir.path().join("not-a-dir");
        std::fs::write(&blocker, "not a directory").expect("write the blocker");
        let blocked = blocker.join(MANAGER_LOG_FILE);
        let m = Manager::new(
            FakeService::new(),
            FakeAuthority::new(true).quietly(true).bff(&bff),
            FakeResolver::lan(),
        )
        .with_log(FileActionLog::new(&blocked));
        assert_eq!(
            m.stop(&desktop, "default"),
            Ok(()),
            "the record is lost, the stop stands"
        );
        assert!(
            !blocked.exists() && blocker.is_file(),
            "the sink created nothing over an unwritable path: {blocker:?}"
        );
    }
}

/// E2E phase for the compare-and-swap write (`write_file_if`), for this
/// crate's e2e test to call (it sets the process-wide `PRIVATE_AI_DATA_ROOT`
/// redirect, `src/tests/mod.rs`). `expected` is the whole text the caller
/// read: equal bytes run `write_file`'s checks and its write; a file that is
/// missing, or holds any other bytes, is `Changed` with nothing written. Each
/// block redirects a data root of its own, so it reads only the files it
/// wrote. The seams are fakes: no D-Bus, systemd, polkit or DNS.
#[cfg(test)]
pub(crate) fn manager_write_file_if_e2e() {
    use fakes::{
        Call, FakeActionLog, FakeAuthority, FakeGroup, FakePasswords, FakeResolver, FakeService,
        Gate, GateResolver, GATE_WAIT,
    };
    use pai_core::checks::Refusal;
    use std::sync::Arc;
    use std::time::Duration;

    let bff = Caller(":1.9".into());
    let desktop = Caller(":1.42".into());
    let member = Requester {
        caller: bff.clone(),
        principal: Some(Principal("alice".into())),
    };
    let outsider = Requester {
        caller: bff.clone(),
        principal: Some(Principal("bob".into())),
    };
    let fixtures = tempfile::tempdir().expect("tempdir");
    let model_profiles = real_model_files(
        fixtures.path(),
        "[Qwen3.8-27B]\nmodel = /models/Qwen3.8-27B-Q8_0.gguf\nctx-size = 172032\n",
    );
    let other = real_model_files(
        fixtures.path(),
        "[Qwen3.8-32B]\nmodel = /models/Qwen3.8-32B-Q4_K_M.gguf\n",
    );
    let default_ini = "[Server]\nPort = 8081\nHostname = 127.0.0.1\nModelsMax = 2\n";
    let invalid = |key: &'static str, values: &[&str]| {
        Err(ManagerError::Invalid(Refusal {
            key,
            values: values.iter().map(|v| v.to_string()).collect(),
        }))
    };

    /// A data root of its own, holding `files`: a block never reads another
    /// block's writes.
    fn root(files: &[(&str, &str)]) -> (tempfile::TempDir, std::path::PathBuf) {
        let dir = tempfile::tempdir().expect("tempdir");
        crate::paths::redirect_for_tests(dir.path());
        let sys = crate::paths::engine_config_dir();
        std::fs::create_dir_all(&sys).expect("engine config dir");
        for (name, text) in files {
            std::fs::write(sys.join(name), text).expect("fixture file");
        }
        (dir, sys)
    }
    /// The names in the directory, sorted: a refused write leaves it as it was,
    /// with no temp file added.
    fn listing(sys: &std::path::Path) -> Vec<String> {
        let mut names: Vec<String> = std::fs::read_dir(sys)
            .map(|rd| {
                rd.flatten()
                    .map(|e| e.file_name().to_string_lossy().into_owned())
                    .collect()
            })
            .unwrap_or_default();
        names.sort();
        names
    }
    fn text_of(sys: &std::path::Path, name: &str) -> String {
        std::fs::read_to_string(sys.join(name)).expect("file on disk")
    }
    fn records(lines: &std::sync::Arc<std::sync::Mutex<Vec<String>>>) -> Vec<String> {
        lines.lock().unwrap().clone()
    }
    // A manager whose caller's own calls are gated by polkit `allow`, with
    // a fake password checker for the principal checks and a fake log.
    let core = |allow: bool, passwords: FakePasswords, log: FakeActionLog| {
        Manager::new(
            FakeService::new(),
            FakeAuthority::new(allow).quietly(true).bff(&bff),
            FakeResolver::lan(),
        )
        .with_group(FakeGroup::new())
        .with_passwords(passwords)
        .with_log(log)
    };

    // ── The gates run first, as write_file's: the read is never reached ──
    // The served files here are directories: a read of one fails, so a
    // compare that ran before the gate would answer `Io`, not the refusal.
    {
        let dir = tempfile::tempdir().expect("tempdir");
        crate::paths::redirect_for_tests(dir.path());
        let sys = crate::paths::engine_config_dir();
        let names = ["models.ini", "server@default.ini"];
        for name in names {
            std::fs::create_dir_all(sys.join(name)).expect("unreadable fixture");
        }
        let log = FakeActionLog::new();
        let lines = log.lines.clone();
        let m = core(false, FakePasswords::new(), log);
        for name in names {
            assert_eq!(
                m.write_file_if(&desktop, name, &other, &model_profiles),
                Err(ManagerError::NotAuthorized),
                "{name}: the refusal write_file gives a caller without configure"
            );
        }
        assert_eq!(
            *m.authority().asked.lock().unwrap(),
            vec![(desktop.clone(), Action::Configure); 2],
            "configure is asked once per call, before anything is read"
        );
        assert!(
            m.authority().asked_quietly.lock().unwrap().is_empty(),
            "a write never asks the question made without a prompt"
        );
        assert_eq!(
            records(&lines),
            names
                .iter()
                .map(|name| format!(
                    "action=\"write_file_if\" target={name:?} caller=\":1.42\" principal=- outcome=refused"
                ))
                .collect::<Vec<String>>(),
            "one record per refused call, naming the file as it was asked for"
        );
        assert!(
            m.service().calls.lock().unwrap().is_empty(),
            "a refused write reaches no service manager"
        );
        for name in names {
            assert!(
                std::fs::read_to_string(sys.join(name)).is_err(),
                "the fixture is still the unreadable directory: {name}"
            );
        }
        assert_eq!(
            listing(&sys),
            names,
            "the refused calls added nothing, no temp file"
        );
    }

    // ── Equal bytes: written, and checked as write_file checks ───────────
    {
        let (dir, sys) = root(&[
            ("models.ini", &model_profiles),
            ("server@default.ini", default_ini),
        ]);
        write_record(&sys, &["default"]);
        let log = FakeActionLog::new();
        let lines = log.lines.clone();
        let m = core(true, FakePasswords::new(), log);
        // A marker in the text written and one in the text expected: the
        // record names the file, never what is in it.
        let marker = "sk-CAS-NEVER-4b71";
        let newer = format!("{default_ini}OpencodeApiKey = {marker}\n");
        assert_eq!(
            m.write_file_if(&desktop, "server@default.ini", &newer, default_ini)
                .map(|_| ()),
            Ok(()),
            "the file holds the text the caller expected: written"
        );
        assert_eq!(
            text_of(&sys, "server@default.ini"),
            newer,
            "the write landed as sent"
        );
        assert_eq!(
            m.read_file(Some(&desktop), "server@default.ini"),
            Ok(Some(plain(&newer))),
            "the write reads back whole"
        );
        assert_eq!(
            listing(&sys),
            [
                "instances.ini",
                "launch@default.ini",
                "models.ini",
                "models@default.ini",
                "server@default.ini"
            ],
            "exactly the files written (the record of the instance, the pool, and \
             the server write's render of the instance's presets file and of its \
             launch), no temp file left"
        );
        assert_eq!(
            *m.service().calls.lock().unwrap(),
            Vec::<Call>::new(),
            "IP3c: the accepted compare-and-swap write answers the apply, whose drift compare \
             asks NOTHING of the service manager: `default` recorded no pair it launched with, \
             so no write moved it to a restart, and no unit verb (the phases that seed the \
             `applied` record, `manager_dbus_e2e` and `manager_dbus_client_e2e`, pin the ask)"
        );
        assert_eq!(
            *m.authority().asked.lock().unwrap(),
            vec![
                (desktop.clone(), Action::Configure),
                (desktop.clone(), Action::Control),
            ],
            "configure for the write, control for the read of the instance file"
        );
        // The checks of write_file run on the text written: an equal
        // expected does not skip them, and the old file is kept.
        assert_eq!(
            m.write_file_if(
                &desktop,
                "server@default.ini",
                "[Server]\nPort = 0\nHostname = 127.0.0.1\n",
                &newer
            ),
            invalid("not-whole-number-in-range", &["Port", "0", "1", "65535"]),
            "an equal expected does not skip the content checks"
        );
        assert_eq!(
            text_of(&sys, "server@default.ini"),
            newer,
            "the refused text left the file"
        );
        let bad_model_profiles = real_model_files(dir.path(), &format!(
            "{model_profiles}[Other]\nmodel = /models/o.gguf\nmoe-cache-mib = 4096\nsplit-mode = tensor\n"
        ));
        assert_eq!(
            m.write_file_if(&desktop, "models.ini", &bad_model_profiles, &model_profiles),
            invalid("moe-cache-with-tensor-split", &[]),
            "the preset file's checks run too"
        );
        assert_eq!(
            text_of(&sys, "models.ini"),
            model_profiles,
            "the refused text left the file"
        );
        // The compare is the first thing under the writing lock: a stale
        // expected is Changed, whatever the contents would have failed on.
        assert_eq!(
            m.write_file_if(&desktop, "models.ini", &bad_model_profiles, default_ini),
            Err(ManagerError::Changed),
            "the compare answers before any check runs"
        );
        assert_eq!(
            text_of(&sys, "models.ini"),
            model_profiles,
            "nothing written for a stale expected"
        );
        let recs = records(&lines);
        let got: Vec<&str> = recs.iter().map(String::as_str).collect();
        assert_eq!(
            got,
            vec![
                r#"action="write_file_if" target="server@default.ini" caller=":1.42" principal=- outcome=ok"#,
                r#"action="read_file" target="server@default.ini" caller=":1.42" principal=- outcome=ok"#,
                r#"action="write_file_if" target="server@default.ini" caller=":1.42" principal=- outcome="Invalid""#,
                r#"action="write_file_if" target="models.ini" caller=":1.42" principal=- outcome="Invalid""#,
                r#"action="write_file_if" target="models.ini" caller=":1.42" principal=- outcome="Changed""#,
            ],
            "one record per call, under the method's own name, the outcome the error's own name"
        );
        for line in records(&lines) {
            assert!(
                !line.contains(marker),
                "a record never carries the text written: {line:?}"
            );
            assert!(
                !line.contains("Qwen3.8-27B"),
                "a record never carries the text expected: {line:?}"
            );
            assert!(!line.contains('\n'), "a record is one line: {line:?}");
        }
    }

    // ── Any other bytes: Changed, nothing written ────────────────────────
    {
        let (_dir, sys) = root(&[
            ("models.ini", &model_profiles),
            ("server@default.ini", default_ini),
        ]);
        let log = FakeActionLog::new();
        let lines = log.lines.clone();
        let m = core(true, FakePasswords::new(), log);
        let before = listing(&sys);
        // Every one of these is the file's own text with a byte added,
        // dropped, moved or translated: the compare is byte for byte, not a
        // comparison of a normalized form.
        let stale: Vec<String> = vec![
            other.to_string(),
            model_profiles.trim_end().to_string(),
            format!("{model_profiles} "),
            model_profiles[..model_profiles.len() - 1].to_string(),
            format!("; note\n{model_profiles}"),
            model_profiles.replace('\n', "\r\n"),
            model_profiles.replace("172032", "172033"),
            model_profiles.to_uppercase(),
        ];
        for expected in &stale {
            assert_eq!(
                m.write_file_if(&desktop, "models.ini", &other, expected),
                Err(ManagerError::Changed),
                "expected {expected:?} against the file on disk"
            );
        }
        assert_eq!(
            text_of(&sys, "models.ini"),
            model_profiles,
            "the refused writes changed nothing"
        );
        assert_eq!(listing(&sys), before, "nothing added, no temp file");
        assert_eq!(
            *m.authority().asked.lock().unwrap(),
            vec![(desktop.clone(), Action::Configure); stale.len()],
            "the gate is asked for every call, before the read"
        );
        let recs = records(&lines);
        assert_eq!(
            recs.first().map(String::as_str),
            Some(
                r#"action="write_file_if" target="models.ini" caller=":1.42" principal=- outcome="Changed""#
            ),
            "the record of a refused compare-and-swap: the outcome is Changed"
        );
        assert_eq!(
            recs.iter().filter(|line| line.contains("Changed")).count(),
            stale.len(),
            "one record per refused call"
        );
        // The file's own bytes are accepted: the table above is not green
        // because every compare refuses.
        assert_eq!(
            m.write_file_if(&desktop, "models.ini", &other, &model_profiles)
                .map(|_| ()),
            Ok(()),
            "the expected text is the file's own: written"
        );
        assert_eq!(text_of(&sys, "models.ini"), other, "the write landed");
    }

    // ── A missing file: Changed, and nothing is created ──────────────────
    {
        let (_dir, sys) = root(&[("models.ini", &model_profiles)]);
        let log = FakeActionLog::new();
        let lines = log.lines.clone();
        let m = core(true, FakePasswords::new(), log);
        let ini = "[Server]\nPort = 8080\nHostname = 127.0.0.1\n";
        assert_eq!(
            m.write_file_if(&desktop, "server@default.ini", ini, &model_profiles),
            Err(ManagerError::Changed),
            "the caller expects a file that is not there"
        );
        assert_eq!(
            m.write_file_if(&desktop, "server@default.ini", ini, ini),
            Err(ManagerError::Changed),
            "an expected equal to the contents does not make the file exist"
        );
        assert!(
            !sys.join("server@default.ini").exists(),
            "a refused compare-and-swap creates no file"
        );
        assert_eq!(listing(&sys), ["models.ini"], "nothing created");
        assert_eq!(
            records(&lines),
            vec![
                r#"action="write_file_if" target="server@default.ini" caller=":1.42" principal=- outcome="Changed""#
                    .to_string(),
                r#"action="write_file_if" target="server@default.ini" caller=":1.42" principal=- outcome="Changed""#
                    .to_string(),
            ],
            "a missing file is recorded as Changed, not as a failure"
        );
    }
    // The directory is not there at all: the compare reads, it creates nothing.
    {
        let dir = tempfile::tempdir().expect("tempdir");
        crate::paths::redirect_for_tests(dir.path());
        let sys = crate::paths::engine_config_dir();
        let m = core(true, FakePasswords::new(), FakeActionLog::new());
        assert_eq!(
            m.write_file_if(&desktop, "models.ini", &model_profiles, &model_profiles),
            Err(ManagerError::Changed),
            "no file to compare with"
        );
        assert!(!sys.exists(), "the refused write created no directory");
    }

    // ── The principal form: the pair is the right, the compare is byte for byte
    {
        let ini = "[Server]\nPort = 8081\nHostname = 127.0.0.1\nOpencodeApiKey = sk-local-3f9a\n";
        let (_dir, sys) = root(&[("models.ini", &model_profiles), ("server@default.ini", ini)]);
        let passwords = FakePasswords::new();
        let asked_admin = passwords.asked_admin.clone();
        let log = FakeActionLog::new();
        let lines = log.lines.clone();
        // This bff has no polkit right of its own: the pair does the work.
        let m = core(false, passwords, log);
        let read = m
            .read_file(Some(&member), "server@default.ini")
            .expect("a member's read")
            .expect("the file exists");
        assert_eq!(
            read.text, ini,
            "a member's read carries the key, so the text to expect is the file's own"
        );
        assert_eq!(
            m.write_file_if(
                &member,
                "server@default.ini",
                &format!("{ini}ModelsMax = 2\n"),
                &read.text
            )
            .map(|_| ()),
            Ok(()),
            "the text a member read, unblanked, is what the compare accepts"
        );
        assert!(
            text_of(&sys, "server@default.ini").contains("sk-local-3f9a"),
            "the key line the caller sent is written as sent, with no key flag"
        );
        assert_eq!(
            m.write_file_if(&member, "models.ini", &other, &model_profiles)
                .map(|_| ()),
            Ok(()),
            "a member's compare-and-swap lands with the caller allowed nothing"
        );
        assert_eq!(text_of(&sys, "models.ini"), other, "the write landed");
        assert_eq!(
            m.write_file_if(
                &member,
                "models.ini",
                &model_profiles,
                "[Qwen3.8-9B]\nmodel = /m/x.gguf\n"
            ),
            Err(ManagerError::Changed),
            "a member's stale expected is refused as anyone's"
        );
        assert_eq!(
            text_of(&sys, "models.ini"),
            other,
            "the refused write changed no file"
        );
        assert_eq!(
            m.write_file_if(&outsider, "models.ini", &model_profiles, &other),
            Err(ManagerError::NotAuthorized),
            "a non-member is refused as write_file refuses it"
        );
        assert_eq!(
            text_of(&sys, "models.ini"),
            other,
            "nothing written for a non-member"
        );
        // A caller that is not the bff, naming a member: refused, and the
        // compare never runs (the expected text is the file's own, so a
        // compare that ran would write).
        assert_eq!(
            m.write_file_if(
                &Requester {
                    caller: desktop.clone(),
                    principal: Some(Principal("alice".into())),
                },
                "models.ini",
                &model_profiles,
                &other
            ),
            Err(ManagerError::NotAuthorized),
            "only the bff's account vouches for a user"
        );
        assert_eq!(text_of(&sys, "models.ini"), other, "nothing written");
        assert_eq!(
            *m.authority().asked.lock().unwrap(),
            Vec::<(Caller, Action)>::new(),
            "polkit decides nothing about a principal"
        );
        assert_eq!(
            *asked_admin.lock().unwrap(),
            vec![
                "alice".to_string(),
                "alice".to_string(),
                "alice".to_string(),
                "alice".to_string(),
                "bob".to_string()
            ],
            "the membership is asked once per call the bff vouches for, of the \
             principal named; the caller that is not the bff is refused before \
             the lookup (check_principal asks is_bff first)"
        );
        assert_eq!(
            records(&lines),
            vec![
                r#"action="read_file" target="server@default.ini" caller=":1.9" principal="alice" outcome=ok"#,
                r#"action="write_file_if" target="server@default.ini" caller=":1.9" principal="alice" outcome=ok"#,
                r#"action="write_file_if" target="models.ini" caller=":1.9" principal="alice" outcome=ok"#,
                r#"action="write_file_if" target="models.ini" caller=":1.9" principal="alice" outcome="Changed""#,
                r#"action="write_file_if" target="models.ini" caller=":1.9" principal="bob" outcome=refused"#,
                r#"action="write_file_if" target="models.ini" caller=":1.42" principal="alice" outcome=refused"#,
            ],
            "a principal's compare-and-swap is recorded with its caller and its principal"
        );
    }

    // ── The compare runs under the writing lock ──────────────────────────
    // The first write is held inside the lock at a slow host lookup. The
    // second is a compare-and-swap whose expected is the file as it is on disk
    // now: it must wait for the lock, find the text it expected gone and
    // answer Changed, leaving the first write on disk. A compare outside the
    // lock writes over the first.
    {
        let held = "[Server]\nPort = 9000\nHostname = slow-box\n";
        let moved = "[Server]\nPort = 9002\nHostname = slow-box\n";
        let late = "[Server]\nPort = 9003\nHostname = slow-box\n";
        let (_dir, sys) = root(&[("server@slow.ini", held)]);
        write_record(&sys, &["slow"]);
        let gate = Gate::new();
        let resolver = GateResolver::new(
            FakeResolver::new(&[("slow-box", &["10.0.0.7"])]),
            "slow-box",
            &gate,
        );
        let m = Arc::new(
            Manager::new(FakeService::new(), FakeAuthority::new(true), resolver)
                .with_group(FakeGroup::new()),
        );
        let first = {
            let (m, who) = (m.clone(), desktop.clone());
            on_thread(move || m.write_file(&who, "server@slow.ini", moved, false))
        };
        assert!(
            gate.arrived(1),
            "the first write's address check reached the slow lookup"
        );
        let second = {
            let (m, who) = (m.clone(), desktop.clone());
            on_thread(move || m.write_file_if(&who, "server@slow.ini", late, held))
        };
        // Time enough for the second to write, if it compared before the lock.
        let early = second.recv_timeout(Duration::from_millis(500));
        gate.open();
        let first = first
            .recv_timeout(GATE_WAIT)
            .expect("the first write returns once the lookup answers");
        let second = second
            .recv_timeout(GATE_WAIT)
            .expect("the compare-and-swap returns once it has the lock");
        assert!(
            !matches!(early, Ok(Ok(_))),
            "the compare-and-swap finished while the first write held the lock: {early:?}"
        );
        assert_eq!(first, Ok(()), "the first write lands");
        assert_eq!(
            second,
            Err(ManagerError::Changed),
            "the write it waited for made its expected text stale"
        );
        assert_eq!(
            text_of(&sys, "server@slow.ini"),
            moved,
            "the first write is what stands on disk"
        );
        assert_eq!(
            listing(&sys),
            [
                "instances.ini",
                "launch@slow.ini",
                "models@slow.ini",
                "server@slow.ini"
            ],
            "no temp file left; the landed write renders the instance's presets file \
             and its launch"
        );
    }
}

/// G2 of "The GPU distribution on the model profile page" (ARCHITECTURE.md 1):
/// the node's device list, `Devices()` under `control` asked quietly (no
/// interaction, as a read that can prompt for nothing is asked today) and
/// `DevicesAs(principal)` as every `As` verb: the bff's account only, and a
/// principal who is a current member of `pai`. Every refusal is
/// `NotAuthorized` with the probe never run: a caller with no right must not
/// make root spawn the engine. The read reaches no service manager, records no
/// action, and reads and writes no file. Names the tests assume, the contract
/// for the coder: `Manager::devices(&self, who: &Requester)`, and
/// `GpuTotals::devices(&self) -> pai_core::client::NodeDevices` beside `gpus()`.
#[cfg(test)]
pub(crate) fn manager_devices_e2e() {
    use fakes::{
        FakeActionLog, FakeAuthority, FakeGpus, FakeGroup, FakePasswords, FakeResolver, FakeService,
    };

    let dir = tempfile::tempdir().expect("tempdir");
    crate::paths::redirect_for_tests(dir.path());
    let sys = crate::paths::engine_config_dir();
    std::fs::create_dir_all(&sys).expect("engine config dir");

    let bff = Caller(":1.9".into());
    let member = Requester {
        caller: bff.clone(),
        principal: Some(Principal("alice".into())),
    };
    let outsider = Requester {
        caller: bff.clone(),
        principal: Some(Principal("bob".into())),
    };
    let desktop = Caller(":1.42".into());
    let own = Requester {
        caller: desktop.clone(),
        principal: None,
    };
    let own_bff = Requester {
        caller: bff.clone(),
        principal: None,
    };

    // The block of `tests/gpu_probe.rs`, as the widened probe caches it: three
    // entries in the probe's order, the CPU pseudo-device left out. The
    // inventory R1 adds (ARCHITECTURE.md (1) "The node inventory") is the
    // fake's own reading, a pair no test writes anywhere else: a production
    // code path that reads the host answers the host's own numbers, and these
    // asserts see a mismatch. A test never reads `/proc/meminfo`,
    // `GlobalMemoryStatusEx` or the CPU count.
    const RAM_MIB: u64 = 3_932_160;
    const CPU_THREADS: u32 = 8;
    let cards = |probed_at: Option<u64>| {
        FakeGpus::new(&[("CUDA0", 12281)])
            .with_devices(
                probed_at,
                &[
                    (
                        "CUDA0",
                        "NVIDIA GeForce RTX 4070 SUPER",
                        "CUDA",
                        12281u64,
                        10844u64,
                    ),
                    (
                        "Vulkan0",
                        "AMD Radeon(TM) Graphics",
                        "Vulkan",
                        33593u64,
                        31913u64,
                    ),
                    (
                        "Vulkan1",
                        "NVIDIA GeForce RTX 4070 SUPER",
                        "Vulkan",
                        11997u64,
                        10844u64,
                    ),
                ],
            )
            .with_inventory(RAM_MIB, CPU_THREADS)
    };
    let listed = |probed_at: Option<u64>| pai_core::client::NodeDevices {
        probed_at,
        ram_mib: RAM_MIB,
        cpu_threads: CPU_THREADS,
        devices: vec![
            pai_core::client::DeviceInfo {
                id: "CUDA0".into(),
                name: "NVIDIA GeForce RTX 4070 SUPER".into(),
                backend: "CUDA".into(),
                total_mib: 12281,
                free_mib: 10844,
            },
            pai_core::client::DeviceInfo {
                id: "Vulkan0".into(),
                name: "AMD Radeon(TM) Graphics".into(),
                backend: "Vulkan".into(),
                total_mib: 33593,
                free_mib: 31913,
            },
            pai_core::client::DeviceInfo {
                id: "Vulkan1".into(),
                name: "NVIDIA GeForce RTX 4070 SUPER".into(),
                backend: "Vulkan".into(),
                total_mib: 11997,
                free_mib: 10844,
            },
        ],
    };

    let core = |allow: bool, quiet: bool, gpus: FakeGpus| {
        let asked = gpus.asked.clone();
        let log = FakeActionLog::new();
        let lines = log.lines.clone();
        let manager = Manager::new(
            FakeService::new(),
            FakeAuthority::new(allow).quietly(quiet).bff(&bff),
            FakeResolver::lan(),
        )
        .with_group(FakeGroup::new())
        .with_passwords(FakePasswords::new())
        .with_log(log)
        .with_gpus(gpus);
        (manager, asked, lines)
    };

    // ── The bff asking for a member: the list, the probe asked once ──────
    {
        let (m, asked, lines) = core(true, true, cards(Some(1_766_271_243)));
        assert_eq!(m.devices(&member), Ok(listed(Some(1_766_271_243))));
        assert_eq!(
            *asked.lock().unwrap(),
            vec!["devices".to_string()],
            "one probe per call: the cache is the probe's, not the manager's"
        );
        assert_eq!(
            *m.service().calls.lock().unwrap(),
            Vec::<fakes::Call>::new(),
            "the read reaches no service manager"
        );
        assert_eq!(
            *lines.lock().unwrap(),
            Vec::<String>::new(),
            "a read records no action"
        );
        assert_eq!(
            *m.authority().asked_quietly.lock().unwrap(),
            vec![],
            "the pair is the right: polkit is asked for nothing, not even quietly"
        );
        let listing: Vec<String> = std::fs::read_dir(&sys)
            .expect("the config dir")
            .flatten()
            .map(|e| e.file_name().to_string_lossy().into_owned())
            .collect();
        assert_eq!(listing, Vec::<String>::new(), "the read writes no file");
    }

    // ── The host's inventory travels with the list (R1) ─────────────────────
    {
        // The two numbers are the fake's own readings, named above, and a
        // production answer that reads the host cannot carry them.
        let (m, _asked, _lines) = core(true, true, cards(Some(1_766_271_243)));
        let got = m.devices(&member).expect("the read answers");
        assert_eq!(
            got.ram_mib, RAM_MIB,
            "the host's total RAM, in MiB, the reading the seam was handed"
        );
        assert_eq!(
            got.cpu_threads, CPU_THREADS,
            "the host's logical CPUs, the reading the seam was handed"
        );
    }
    {
        // A node whose probe answered nothing: no moment, no device, and the
        // inventory is answered all the same. The two figures are the host's,
        // never the engine's, so a probe that died leaves nothing unknown.
        let (m, asked, _) = core(
            true,
            true,
            FakeGpus::new(&[("CUDA0", 12281)])
                .no_devices()
                .with_inventory(RAM_MIB, CPU_THREADS),
        );
        assert_eq!(
            m.devices(&member),
            Ok(pai_core::client::NodeDevices {
                probed_at: None,
                ram_mib: RAM_MIB,
                cpu_threads: CPU_THREADS,
                devices: vec![],
            }),
            "the inventory is answered with no probe and no device"
        );
        assert_eq!(*asked.lock().unwrap(), vec!["devices".to_string()]);
    }

    // ── The caller's own call: control asked quietly, never with a prompt ─
    {
        let (m, asked, _lines) = core(true, true, cards(Some(1_766_271_243)));
        assert_eq!(m.devices(&own_bff), Ok(listed(Some(1_766_271_243))));
        assert_eq!(
            *m.authority().asked_quietly.lock().unwrap(),
            vec![(bff.clone(), Action::Control)],
            "control asked without interaction, the way a read that may prompt for nothing is asked"
        );
        assert_eq!(
            *m.authority().asked.lock().unwrap(),
            Vec::<(Caller, Action)>::new(),
            "never asked with a prompt: a device list is worth no challenge"
        );
        assert_eq!(*asked.lock().unwrap(), vec!["devices".to_string()]);
    }

    // ── No control: refused, and the probe never runs ─────────────────────
    {
        // A caller with control for the prompting check and none for the quiet
        // one: the device list is the quiet check's, so it is refused.
        let (m, asked, _lines) = core(true, false, cards(Some(1_766_271_243)));
        assert_eq!(
            m.devices(&own),
            Err(ManagerError::NotAuthorized),
            "a caller polkit would challenge, or refuses quietly, gets no list"
        );
        assert_eq!(
            *asked.lock().unwrap(),
            Vec::<String>::new(),
            "a refused call runs no engine"
        );
    }
    {
        // No control at all.
        let (m, asked, _lines) = core(false, false, cards(Some(1_766_271_243)));
        assert_eq!(
            m.devices(&own),
            Err(ManagerError::NotAuthorized),
            "a caller without control"
        );
        assert_eq!(
            *asked.lock().unwrap(),
            Vec::<String>::new(),
            "a refused call runs no engine"
        );
    }

    // ── The As form: the bff's account, and a current member of `pai` ─────
    {
        let (m, asked, _) = core(true, true, cards(Some(1_766_271_243)));
        assert_eq!(
            m.devices(&outsider),
            Err(ManagerError::NotAuthorized),
            "bob is no member of `pai`"
        );
        assert_eq!(
            *asked.lock().unwrap(),
            Vec::<String>::new(),
            "a non-member's read runs no engine"
        );
    }
    {
        // A caller that is not the bff's account vouching for a user.
        let (m, asked, _) = core(true, true, cards(Some(1_766_271_243)));
        let forger = Requester {
            caller: desktop.clone(),
            principal: Some(Principal("alice".into())),
        };
        assert_eq!(
            m.devices(&forger),
            Err(ManagerError::NotAuthorized),
            "only the bff's account vouches for a user"
        );
        assert_eq!(
            *asked.lock().unwrap(),
            Vec::<String>::new(),
            "a forged pair runs no engine"
        );
    }
    {
        // A member removed between two calls is refused at the next one.
        let passwords = FakePasswords::new();
        let admins = passwords.admins.clone();
        let manager = Manager::new(
            FakeService::new(),
            FakeAuthority::new(true).quietly(true).bff(&bff),
            FakeResolver::lan(),
        )
        .with_group(FakeGroup::new())
        .with_passwords(passwords)
        .with_log(FakeActionLog::new())
        .with_gpus(cards(Some(1_766_271_243)));
        assert_eq!(manager.devices(&member), Ok(listed(Some(1_766_271_243))));
        admins.lock().unwrap().remove("alice");
        assert_eq!(
            manager.devices(&member),
            Err(ManagerError::NotAuthorized),
            "the membership is read at every call, never cached"
        );
    }

    // ── A probe that answered nothing: an empty list, no time ─────────────
    {
        let (m, _asked, _) = core(true, true, FakeGpus::new(&[]).no_devices());
        assert_eq!(
            m.devices(&member),
            Ok(pai_core::client::NodeDevices {
                probed_at: None,
                // A seam that names no inventory answers 0 for both, the
                // host's own "no figure", never a guessed number.
                ram_mib: 0,
                cpu_threads: 0,
                devices: Vec::new(),
            }),
            "no probe answer: no card and no time, never an error and never a card"
        );
    }
}

/// A GGUF v3 file for the model-info fixtures: arch `arch`, the KV keys of
/// `kvs` (each written as a u32, the caller spelling the `<arch>.` prefix), and
/// one tensor of `bytes` per `(name, bytes)`, laid out as `write_gguf` lays a
/// model out (each aligned to 32, the file sparse after the header). Same bytes
/// as `pai-core`'s `gguf_shape::tests::gguf`, so the reader sees what it sees in
/// its own tests.
#[cfg(test)]
pub(crate) fn write_gguf_kv(
    path: &std::path::Path,
    arch: &str,
    kvs: &[(&str, u32)],
    tensors: &[(&str, u64)],
) {
    let put_str = |out: &mut Vec<u8>, s: &str| {
        out.extend_from_slice(&(s.len() as u64).to_le_bytes());
        out.extend_from_slice(s.as_bytes());
    };
    let put_u32 = |out: &mut Vec<u8>, x: u32| out.extend_from_slice(&x.to_le_bytes());
    let put_u64 = |out: &mut Vec<u8>, x: u64| out.extend_from_slice(&x.to_le_bytes());
    let mut out = b"GGUF".to_vec();
    put_u32(&mut out, 3);
    // The header's order is the reader's: tensor infos first, then the kv.
    put_u64(&mut out, tensors.len() as u64);
    put_u64(&mut out, (kvs.len() + 1) as u64);
    put_str(&mut out, "general.architecture");
    put_u32(&mut out, 8);
    put_str(&mut out, arch);
    for (k, v) in kvs {
        put_str(&mut out, k);
        put_u32(&mut out, 4);
        put_u32(&mut out, *v);
    }
    let mut off = 0u64;
    for (name, bytes) in tensors {
        put_str(&mut out, name);
        put_u32(&mut out, 2);
        put_u64(&mut out, 4096);
        put_u64(&mut out, bytes / 4096);
        put_u32(&mut out, 12);
        put_u64(&mut out, off);
        off += bytes.div_ceil(32) * 32;
    }
    let data_start = (out.len() as u64).div_ceil(32) * 32;
    std::fs::write(path, out).expect("gguf header");
    std::fs::OpenOptions::new()
        .write(true)
        .open(path)
        .expect("open gguf")
        .set_len(data_start + off)
        .expect("sparse gguf");
}

/// L1 of "Layers, MoE and tensor placement on the web model profile page"
/// (ARCHITECTURE.md (1)): `ModelInfo(profile)` / `ModelInfoAs(principal,
/// profile)`, the layer facts of a SAVED model profile, read by the manager
/// because the bff cannot reach the file (`ProtectHome`). The gate is the gate
/// of reading `models.ini`: the caller's own read asks `control` WITHOUT
/// interaction, the `As` form is the bff's account vouching for a current
/// member of `pai` and asks polkit for nothing; every refusal is
/// `NotAuthorized` with nothing read. The section is looked for in `models.ini`,
/// else in `models.ini.disabled` (a switched-off profile has sliders too); no
/// section, and `*`, are the keyed `no-model-profile-named` `[id]`. `model` is
/// resolved as the GPU memory check resolves it (the section's own key, else
/// `[*]`'s; absolute, else against the instances' working directory), the one
/// helper `estimated_of` uses too. The outcomes are all a SUCCESS of the call,
/// with `info` null and a keyed `reason`: a `FETCH_KEYS` key answers
/// `model-info-fetched` `[key]` and opens no file; no `model` answers
/// `model-profile-no-model` `[id]`; a path, or a shard of it, that is not there
/// answers `model-info-missing` `[path]`; a file the GGUF reader refuses answers
/// `model-info-unreadable` `[path]` and carries none of its bytes; a read
/// answers `blockCount` (the header's key, else the blocks the tensors name),
/// `moe`, `expertCount` and `expertUsed`. A shard set declaring MORE than 256
/// shards is `model-info-unreadable` too, naming the model: the cap is judged
/// from the name, before any shard past the first is opened, and a set of 256
/// is read as any split is.
///
/// Names the tests assume, the contract for the coder:
/// `Manager::model_info(&self, who: &Requester, profile: &str) ->
/// Result<pai_core::client::ModelInfoReply, ManagerError>`, with
/// `pai_core::client::ModelInfoReply` of `model`, `info` (`block_count`, `moe`,
/// `expert_count`, `expert_used`) and `reason` (`key`, `args`).
#[cfg(test)]
pub(crate) fn manager_model_info_e2e() {
    use fakes::{
        FakeActionLog, FakeAuthority, FakeGroup, FakePasswords, FakeResolver, FakeService,
    };
    use pai_core::checks::Refusal;

    let dir = tempfile::tempdir().expect("tempdir");
    crate::paths::redirect_for_tests(dir.path());
    let sys = crate::paths::engine_config_dir();
    std::fs::create_dir_all(&sys).expect("engine config dir");

    // The models folder. A relative `model` resolves against the instances'
    // working directory (the data root, the instance's state dir on Windows):
    // `launch::instance_current_dir`, the same place the memory check resolves
    // it. Under `redirect_for_tests` the data root is `<dir>/private-ai`.
    let cwd = crate::launch::instance_current_dir(
        cfg!(windows),
        &crate::paths::data_root(),
        &crate::paths::instance_state_dir(),
    );
    let models = cwd.join("models");
    std::fs::create_dir_all(&models).expect("models dir");

    // The tensor list of 2j (J1): `[{name, bytes}]`, in the file's order.
    let rows = |tensors: &[(&str, u64)]| -> Vec<pai_core::client::TensorRow> {
        tensors
            .iter()
            .map(|(name, bytes)| pai_core::client::TensorRow {
                name: (*name).to_string(),
                bytes: *bytes,
            })
            .collect()
    };

    // A MoE file whose header says 40 blocks, 128 experts, 8 used, while its
    // tensors name only 3 of the blocks: the header's numbers are the reply.
    let moe = models.join("moe.gguf");
    write_gguf_kv(
        &moe,
        "qwen3moe",
        &[
            ("qwen3moe.context_length", 262_144),
            ("qwen3moe.block_count", 40),
            ("qwen3moe.expert_count", 128),
            ("qwen3moe.expert_used_count", 8),
        ],
        &[
            ("token_embd.weight", 1 << 20),
            ("blk.0.ffn_gate_exps.weight", 1 << 20),
            ("blk.1.attn_q.weight", 1 << 20),
            ("blk.39.ffn_down_exps.weight", 1 << 20),
            ("output.weight", 1 << 20),
        ],
    );
    let moe_facts = pai_core::client::ModelFacts {
        block_count: 40,
        moe: true,
        expert_count: 128,
        expert_used: 8,
        tensors: rows(&[
            ("token_embd.weight", 1 << 20),
            ("blk.0.ffn_gate_exps.weight", 1 << 20),
            ("blk.1.attn_q.weight", 1 << 20),
            ("blk.39.ffn_down_exps.weight", 1 << 20),
            ("output.weight", 1 << 20),
        ]),
        tensors_truncated: false,
    };
    // A dense file whose header names NO block count: 5, counted from
    // blk.0..blk.4, and no experts to count.
    let plain = models.join("plain.gguf");
    write_gguf_kv(
        &plain,
        "llama",
        &[("llama.context_length", 32_768)],
        &[
            ("token_embd.weight", 1 << 20),
            ("blk.0.attn_q.weight", 1 << 20),
            ("blk.1.attn_q.weight", 1 << 20),
            ("blk.2.attn_q.weight", 1 << 20),
            ("blk.3.attn_q.weight", 1 << 20),
            ("blk.4.attn_q.weight", 1 << 20),
            ("output.weight", 1 << 20),
        ],
    );
    let plain_facts = pai_core::client::ModelFacts {
        block_count: 5,
        moe: false,
        expert_count: 0,
        expert_used: 0,
        tensors: rows(&[
            ("token_embd.weight", 1 << 20),
            ("blk.0.attn_q.weight", 1 << 20),
            ("blk.1.attn_q.weight", 1 << 20),
            ("blk.2.attn_q.weight", 1 << 20),
            ("blk.3.attn_q.weight", 1 << 20),
            ("blk.4.attn_q.weight", 1 << 20),
            ("output.weight", 1 << 20),
        ]),
        tensors_truncated: false,
    };
    // A split model with only its FIRST shard on disk: the second is missing.
    let shard_one = models.join("split-00001-of-00002.gguf");
    let shard_two = models.join("split-00002-of-00002.gguf");
    write_gguf_kv(
        &shard_one,
        "llama",
        &[("llama.block_count", 1)],
        &[("blk.0.attn_q.weight", 1 << 20)],
    );
    // A file that is not a GGUF at all, with bytes no reply may carry.
    let junk = models.join("junk.gguf");
    std::fs::write(&junk, "GGUF-wannabe MARKER-8f2d-not-a-header\n").expect("junk");
    // A GGUF whose body is cut short: the reader's `Truncated`.
    let trunc = models.join("trunc.gguf");
    std::fs::write(
        &trunc,
        b"GGUF\x03\0\0\0"
            .to_vec()
            .into_iter()
            .chain(
                1u64.to_le_bytes()
                    .into_iter()
                    .chain(0u64.to_le_bytes())
                    .collect::<Vec<u8>>(),
            )
            .collect::<Vec<u8>>(),
    )
    .expect("truncated gguf");
    // Not a file at all: a directory where a model is named.
    let a_dir = models.join("a-dir.gguf");
    std::fs::create_dir(&a_dir).expect("a directory as a model");
    // The `hf-repo` profile's `model`: a path that is NOT there, beside the
    // fetch key. The fetch key wins: the answer is "fetched", and the file is
    // never opened.
    let absent = models.join("absent.gguf");
    // A split model whose shard set DECLARES 257 shards, with only the FIRST
    // shard on disk. The design caps a split model at 256, so the cap answers
    // `unreadable` naming the model, and the 257 shards that are not there
    // never turn the answer into `missing`: nothing past the first is opened.
    let over_one = models.join("over-00001-of-00257.gguf");
    write_gguf_kv(
        &over_one,
        "llama",
        &[("llama.block_count", 3)],
        &[("blk.0.attn_q.weight", 1 << 20)],
    );
    // The boundary: a 256-shard set with EVERY file there, answered normally.
    let cap_one = models.join("cap-00001-of-00256.gguf");
    write_gguf_kv(
        &cap_one,
        "llama",
        &[("llama.block_count", 3)],
        &[("blk.0.attn_q.weight", 1 << 20)],
    );
    for i in 2..=256u32 {
        // The later shards of a split carry no keys of their own: the reader
        // takes the kv from the shard that names the architecture.
        write_gguf_kv(
            &models.join(format!("cap-{i:05}-of-00256.gguf")),
            "llama",
            &[],
            &[],
        );
    }
    let cap_facts = pai_core::client::ModelFacts {
        block_count: 3,
        moe: false,
        expert_count: 0,
        expert_used: 0,
        // The later shards of the set carry no tensor of their own, so the list
        // is the first shard's.
        tensors: rows(&[("blk.0.attn_q.weight", 1 << 20)]),
        tensors_truncated: false,
    };
    // The cap again, with the SECOND shard a directory: were the shard set
    // walked at all, that shard answers `unreadable` naming itself, so the
    // path in the reply shows whether anything past the first was opened.
    let dir2_one = models.join("dirtwo-00001-of-00257.gguf");
    write_gguf_kv(
        &dir2_one,
        "llama",
        &[("llama.block_count", 3)],
        &[("blk.0.attn_q.weight", 1 << 20)],
    );
    std::fs::create_dir(models.join("dirtwo-00002-of-00257.gguf"))
        .expect("a directory as the second shard");

    std::fs::write(
        sys.join("models.ini"),
        format!(
            "[*]\nmodel = {}\nctx-size = 4096\n\n\
             [moe-abs]\nmodel = {}\n\n\
             [plain-rel]\nmodel = models/plain.gguf\n\n\
             [cascade]\n\n\
             [shards]\nmodel = {}\n\n\
             [junk]\nmodel = {}\n\n\
             [trunc]\nmodel = {}\n\n\
             [notreg]\nmodel = {}\n\n\
             [fetched]\nmodel = {}\nhf-repo = the-org/the-model\n\n\
             [ctx-only]\nctx-size = 1024\n\n\
             [split-over]\nmodel = {}\n\n\
             [split-dir2]\nmodel = {}\n\n\
             [split-cap]\nmodel = {}\n",
            moe.display(),
            moe.display(),
            shard_one.display(),
            junk.display(),
            trunc.display(),
            a_dir.display(),
            absent.display(),
            over_one.display(),
            dir2_one.display(),
            cap_one.display(),
        ),
    )
    .expect("models.ini");
    // A switched-off profile: the same facts, read from `models.ini.disabled`.
    std::fs::write(
        sys.join("models.ini.disabled"),
        format!("[switched-off]\nmodel = {}\n", moe.display()),
    )
    .expect("models.ini.disabled");

    let bff = Caller(":1.9".into());
    let member = Requester {
        caller: bff.clone(),
        principal: Some(Principal("alice".into())),
    };
    let outsider = Requester {
        caller: bff.clone(),
        principal: Some(Principal("bob".into())),
    };
    let desktop = Caller(":1.42".into());
    let own = Requester {
        caller: desktop.clone(),
        principal: None,
    };
    let own_bff = Requester {
        caller: bff.clone(),
        principal: None,
    };

    let core = |allow: bool, quiet: bool| {
        let log = FakeActionLog::new();
        let lines = log.lines.clone();
        let manager = Manager::new(
            FakeService::new(),
            FakeAuthority::new(allow).quietly(quiet).bff(&bff),
            FakeResolver::lan(),
        )
        .with_group(FakeGroup::new())
        .with_passwords(FakePasswords::new())
        .with_log(log);
        (manager, lines)
    };

    let facts = |got: &pai_core::client::ModelInfoReply| {
        got.info
            .clone()
            .unwrap_or_else(|| panic!("no facts in the reply: {got:?}"))
    };
    let reason = |got: &pai_core::client::ModelInfoReply| {
        got.reason
            .clone()
            .unwrap_or_else(|| panic!("no reason in the reply: {got:?}"))
    };
    // The reply's own words: a path in it is the resolved one, and no byte of
    // a file the reader refused travels in it.
    let wire = |got: &pai_core::client::ModelInfoReply| format!("{got:?}");
    let same_path = |got: &str, want: &std::path::Path| {
        std::fs::canonicalize(got).ok() == std::fs::canonicalize(want).ok()
    };
    let listing = |dir: &std::path::Path| -> Vec<String> {
        let mut names: Vec<String> = std::fs::read_dir(dir)
            .expect("the config dir")
            .flatten()
            .map(|e| {
                format!(
                    "{}:{}",
                    e.file_name().to_string_lossy(),
                    std::fs::read_to_string(e.path()).unwrap_or_default()
                )
            })
            .collect();
        names.sort();
        names
    };
    let before = listing(&sys);

    // ── An absolute `model`: the header's counts, and the resolved path ────
    {
        let (m, lines) = core(true, true);
        let got = m
            .model_info(&member, "moe-abs")
            .expect("a read of a member");
        assert_eq!(
            facts(&got),
            moe_facts,
            "the header's block and expert counts, and `moe` from the routed tensors"
        );
        assert!(
            got.model.as_deref().is_some_and(|p| same_path(p, &moe)),
            "`model` is the resolved absolute path: {got:?}"
        );
        assert_eq!(got.reason, None, "facts: no reason");
        assert_eq!(
            *lines.lock().unwrap(),
            Vec::<String>::new(),
            "a read records no action"
        );
        assert_eq!(
            *m.authority().asked_quietly.lock().unwrap(),
            vec![],
            "the pair is the right: polkit is asked for nothing, not even quietly"
        );
        assert_eq!(
            *m.authority().asked.lock().unwrap(),
            vec![],
            "a read prompts for nothing"
        );
    }

    // ── A relative `model`, and the pool's `[*]` standing in ───────────────
    {
        let (m, _) = core(true, true);
        let got = m
            .model_info(&member, "plain-rel")
            .expect("a relative model");
        assert_eq!(
            facts(&got),
            plain_facts,
            "no block_count key: the blocks the tensors name, no experts, and its OWN \
             file, not the pool's `[*]` model"
        );
        assert!(
            got.model
                .as_deref()
                .is_some_and(|p| same_path(p, &models.join("plain.gguf"))),
            "a relative `model` resolves against the instances' working directory, \
             as the memory check resolves it: {got:?}"
        );
        // A section with no `model` of its own: `[*]`'s, the same cascade.
        let cascaded = m.model_info(&member, "cascade").expect("the pool default");
        assert_eq!(
            facts(&cascaded),
            moe_facts,
            "[*]'s `model` is the section's, as `estimated_of` takes it"
        );
        // A section with a key of its own and NO `model`: the cascade is PER
        // KEY, exactly as `estimated_of` resolves it, so `[*]`'s `model` still
        // stands in for the key the section lacks.
        let keyed = m
            .model_info(&member, "ctx-only")
            .expect("a `ctx-size` of its own does not hide the pool's `model`");
        assert_eq!(
            facts(&keyed),
            moe_facts,
            "`[*]`'s `model` fills the key the section lacks, key by key, not only a \
             section with no keys at all: {keyed:?}"
        );
        assert_eq!(
            keyed.reason, None,
            "the pool's model is read, not refused: {keyed:?}"
        );
    }

    // ── A switched-off profile is answered ───────────────────────────────
    {
        let (m, _) = core(true, true);
        let off = m
            .model_info(&member, "switched-off")
            .expect("a switched-off profile has sliders too");
        assert_eq!(facts(&off), moe_facts, "read from `models.ini.disabled`");
        assert!(
            off.model.as_deref().is_some_and(|p| same_path(p, &moe)),
            "{off:?}"
        );
    }

    // ── An `hf-repo` profile: fetched, and the file is never opened ───────
    {
        let (m, _) = core(true, true);
        let got = m
            .model_info(&member, "fetched")
            .expect("a fetch key is an answer");
        assert_eq!(got.info, None, "no facts for a model not yet downloaded");
        assert_eq!(
            reason(&got),
            pai_core::client::Reason {
                key: "model-info-fetched".into(),
                args: vec!["hf-repo".into()],
            },
            "the fetch key the section names, and nothing read: the `model` beside \
             it does not exist and the answer is not \"missing\": {got:?}"
        );
    }

    // ── No `model` in the cascade: the existing key, and no path ─────────
    {
        // `[*]` here names NO model, so the per-key cascade finds none: a
        // section with a key of its own, and one with none, answer the same.
        // The pool as this block found it goes back at its end, so every later
        // block still reads it.
        let pool = std::fs::read_to_string(sys.join("models.ini")).expect("models.ini");
        let (m, _) = core(true, true);
        std::fs::write(
            sys.join("models.ini"),
            "[*]\nctx-size = 4096\n\n[no-model]\nctx-size = 1024\n\n[no-keys]\n",
        )
        .expect("models.ini, a pool naming no model");
        for id in ["no-model", "no-keys"] {
            let got = m
                .model_info(&member, id)
                .unwrap_or_else(|got| panic!("no model is an answer: {id}: {got:?}"));
            assert_eq!(got.info, None, "`{id}`: {got:?}");
            assert_eq!(
                got.model, None,
                "`{id}`: nothing to resolve, no path in the reply"
            );
            assert_eq!(
                reason(&got),
                pai_core::client::Reason {
                    key: "model-profile-no-model".into(),
                    args: vec![id.into()],
                },
                "the key the page shows for a profile with no Model file"
            );
        }
        std::fs::write(sys.join("models.ini"), pool).expect("models.ini restored");
    }

    // ── A missing file, and a missing shard ──────────────────────────────
    {
        let (m, _) = core(true, true);
        let got = m
            .model_info(&member, "shards")
            .expect("a missing shard is an answer");
        assert_eq!(got.info, None);
        let r = reason(&got);
        assert_eq!(r.key, "model-info-missing", "{got:?}");
        assert_eq!(
            r.args.len(),
            1,
            "the path the reply names, one value: {r:?}"
        );
        assert!(
            same_path(&r.args[0], &shard_two),
            "a split names the shard that is not there, not the one that is: {r:?}"
        );
        // A path that is not there at all (the fetch fixture's own `model`).
        // The pin needs a pool with ONLY that section, so the file is rewritten
        // here; the text as this block found it is written back at its end, so
        // every later block still reads the pool.
        let pool = std::fs::read_to_string(sys.join("models.ini")).expect("models.ini");
        let (m, _) = core(true, true);
        std::fs::write(
            sys.join("models.ini"),
            format!("[gone]\nmodel = {}\n", absent.display()),
        )
        .expect("models.ini, one section");
        let got = m
            .model_info(&member, "gone")
            .expect("a missing file is an answer");
        assert_eq!(
            reason(&got),
            pai_core::client::Reason {
                key: "model-info-missing".into(),
                args: vec![absent.to_string_lossy().into_owned()],
            },
            "the resolved path, as the page shows it"
        );
        std::fs::write(sys.join("models.ini"), pool).expect("models.ini restored");
    }

    // ── Not a GGUF, cut short, no file: unreadable, with none of the bytes ─
    {
        let (m, _) = core(true, true);
        for (id, path) in [("junk", &junk), ("trunc", &trunc), ("notreg", &a_dir)] {
            let got = m
                .model_info(&member, id)
                .unwrap_or_else(|got| panic!("an unreadable model is an answer: {id}: {got:?}"));
            assert_eq!(got.info, None, "no facts from `{id}`: {got:?}");
            let r = reason(&got);
            assert_eq!(r.key, "model-info-unreadable", "`{id}`: {got:?}");
            assert!(
                r.args.iter().any(|a| same_path(a, path)),
                "the path the reader refused, `{id}`: {r:?}"
            );
            assert!(
                !wire(&got).contains("MARKER-8f2d"),
                "the reader's text, and so no byte of the file, reaches the reply: {got:?}"
            );
        }
    }

    // ── A shard set over the 256-shard cap ──────────────────────────────
    {
        // Declares 257 shards and only its first is on disk: the cap answers
        // before the shard set is walked, so the 257 that are not there never
        // turn the answer into `missing`.
        let (m, _) = core(true, true);
        let got = m
            .model_info(&member, "split-over")
            .expect("a shard set past the cap is an answer");
        assert_eq!(
            got.info, None,
            "no facts from a model past the cap: {got:?}"
        );
        let r = reason(&got);
        assert_eq!(
            r.key, "model-info-unreadable",
            "over 256 shards is the table's `unreadable`, not a shard's `missing`: {r:?}"
        );
        assert!(
            r.args.iter().any(|a| same_path(a, &over_one)),
            "the answer names the model path, the first shard: {r:?}"
        );
        // The same cap, with a shard past the first that is no file: were the
        // shard set walked at all, the answer would name THAT shard.
        let got = m
            .model_info(&member, "split-dir2")
            .expect("a shard set past the cap is an answer, whatever a shard is");
        let r = reason(&got);
        assert_eq!(r.key, "model-info-unreadable", "{got:?}");
        assert!(
            r.args.iter().all(|a| same_path(a, &dir2_one)),
            "the cap is checked before any shard past the first is opened, so no shard's \
             own name is in the answer: {r:?}"
        );
        // The boundary: 256 shards, every file there, answered normally.
        let at = m
            .model_info(&member, "split-cap")
            .expect("256 shards is answered");
        assert_eq!(
            facts(&at),
            cap_facts,
            "256 shards is at the cap, not over it: read as any split is"
        );
        assert!(
            at.model.as_deref().is_some_and(|p| same_path(p, &cap_one)),
            "the resolved first shard: {at:?}"
        );
    }

    // ── An id the pool has no section for, and `*` ───────────────────────
    {
        let (m, _) = core(true, true);
        for id in ["nope", "*", "disabled"] {
            assert_eq!(
                m.model_info(&member, id),
                Err(ManagerError::Invalid(Refusal {
                    key: "no-model-profile-named",
                    values: vec![id.into()],
                })),
                "no section named `{id}`, in either file: the keyed refusal"
            );
        }
    }

    // ── The gate: the caller's own read asks `control` quietly ───────────
    {
        let (m, _) = core(true, true);
        assert_eq!(
            facts(
                &m.model_info(&own, "moe-abs")
                    .expect("the caller's own read")
            ),
            moe_facts,
            "a caller with `control` asked quietly reads its own node"
        );
        assert_eq!(
            *m.authority().asked_quietly.lock().unwrap(),
            vec![(desktop.clone(), Action::Control)],
            "the quiet check, never the prompting one"
        );
        assert_eq!(
            *m.authority().asked.lock().unwrap(),
            vec![],
            "a read that can prompt for nothing never prompts"
        );
    }
    {
        // A caller polkit would challenge, or refuses quietly: no facts.
        let (m, lines) = core(true, false);
        assert_eq!(
            m.model_info(&own, "moe-abs"),
            Err(ManagerError::NotAuthorized),
            "the quiet check is the read's gate"
        );
        assert_eq!(
            m.model_info(&own_bff, "moe-abs"),
            Err(ManagerError::NotAuthorized),
            "the bff's own read is gated the same way as any other's"
        );
        assert_eq!(*lines.lock().unwrap(), Vec::<String>::new());
    }

    // ── The As form: the bff's account, and a current member of `pai` ────
    {
        let (m, lines) = core(true, true);
        assert_eq!(
            m.model_info(&outsider, "moe-abs"),
            Err(ManagerError::NotAuthorized),
            "bob is no member of `pai`: no facts, and no path of the profile's file"
        );
        assert_eq!(
            m.model_info(&outsider, "junk"),
            Err(ManagerError::NotAuthorized),
            "not even the reason of a profile whose file is unreadable: the read \
             is gated before the file"
        );
        assert_eq!(
            *m.authority().asked_quietly.lock().unwrap(),
            vec![],
            "the pair is the right: polkit is asked for nothing"
        );
        assert_eq!(*lines.lock().unwrap(), Vec::<String>::new());
    }
    {
        // A caller that is not the bff's account vouching for a user.
        let (m, _) = core(true, true);
        let forger = Requester {
            caller: desktop.clone(),
            principal: Some(Principal("alice".into())),
        };
        assert_eq!(
            m.model_info(&forger, "moe-abs"),
            Err(ManagerError::NotAuthorized),
            "only the bff's account vouches for a user"
        );
    }
    {
        // A member removed between two calls is refused at the next one.
        let passwords = FakePasswords::new();
        let admins = passwords.admins.clone();
        let manager = Manager::new(
            FakeService::new(),
            FakeAuthority::new(true).quietly(true).bff(&bff),
            FakeResolver::lan(),
        )
        .with_group(FakeGroup::new())
        .with_passwords(passwords)
        .with_log(FakeActionLog::new());
        assert_eq!(
            facts(
                &manager
                    .model_info(&member, "moe-abs")
                    .expect("a member reads")
            ),
            moe_facts
        );
        admins.lock().unwrap().remove("alice");
        assert_eq!(
            manager.model_info(&member, "moe-abs"),
            Err(ManagerError::NotAuthorized),
            "the membership is read at every call, never cached"
        );
    }

    // ── A read writes nothing ────────────────────────────────────────────
    assert_eq!(
        listing(&sys),
        before,
        "every read above left the pool and the records as it found them"
    );
}

/// The tensor list 2j adds to the model info reply (J1 of "Layers, MoE and
/// tensor placement on the web model profile page", `ARCHITECTURE.md` 5):
/// `info.tensors` is every tensor's `{name, bytes}` over EVERY shard, in file
/// order, at most 20 000 entries; past that the reply carries the first 20 000
/// and `info.tensorsTruncated` is true. A header of exactly 20 000 is the
/// boundary: every entry travels and nothing is truncated. The list is bounded
/// by bytes as well (J1's follow-up, `ARCHITECTURE.md` 5): 917 504 bytes of the
/// rows' JSON, each row counted as `serde_json::to_string` spells a
/// `{"name","bytes"}` row plus the `,` after it, so the reply stays under the
/// pipe's frame. The rows are kept up to the FIRST one that would pass either
/// bound.
///
/// Names the tests assume, the contract for the coder: `pai_core::client::ModelFacts`
/// gains `tensors` (a `Vec` of rows of `name` and `bytes`) and
/// `tensors_truncated` (a `bool`).
#[cfg(test)]
pub(crate) fn manager_model_tensors_e2e() {
    use fakes::{
        FakeActionLog, FakeAuthority, FakeGroup, FakePasswords, FakeResolver, FakeService,
    };

    let dir = tempfile::tempdir().expect("tempdir");
    crate::paths::redirect_for_tests(dir.path());
    let sys = crate::paths::engine_config_dir();
    std::fs::create_dir_all(&sys).expect("engine config dir");
    let cwd = crate::launch::instance_current_dir(
        cfg!(windows),
        &crate::paths::data_root(),
        &crate::paths::instance_state_dir(),
    );
    let models = cwd.join("models");
    std::fs::create_dir_all(&models).expect("models dir");

    // A model split over THREE shards: the list is every shard's tensors, each
    // shard's after the one before it. Every size a multiple of the page, so
    // the bytes the header declares are the ones the list answers.
    let shard = |i: u32| models.join(format!("tensors-{i:05}-of-00003.gguf"));
    write_gguf_kv(
        &shard(1),
        "llama",
        &[("llama.block_count", 2)],
        &[
            ("token_embd.weight", 1 << 20),
            ("blk.0.attn_q.weight", 1 << 12),
        ],
    );
    write_gguf_kv(
        &shard(2),
        "llama",
        &[],
        &[
            ("blk.1.attn_q.weight", 1 << 13),
            ("blk.1.ffn_up.weight", 1 << 14),
        ],
    );
    write_gguf_kv(
        &shard(3),
        "llama",
        &[],
        &[("blk.2.attn_q.weight", 1 << 15), ("output.weight", 1 << 16)],
    );

    // Past the cap, and the boundary: a header of 20 001 tensors, and one of
    // exactly 20 000.
    let many = models.join("many.gguf");
    let exact = models.join("exact.gguf");
    for (path, n) in [(&many, 20_001usize), (&exact, 20_000)] {
        let names: Vec<String> = (0..n).map(|i| format!("t{i:05}")).collect();
        write_gguf_kv(
            path,
            "llama",
            &[("llama.block_count", 1)],
            &names
                .iter()
                .map(|name| (name.as_str(), 4096u64))
                .collect::<Vec<(&str, u64)>>(),
        );
    }

    // Long names: 20 000 tensors of `LONG` characters each, so the rows' JSON
    // passes the byte bound long before the 20 000th row. No character of a
    // name is one JSON escapes, so the bound is arithmetic, never an escape.
    const LONG: usize = 100;
    let long = models.join("long.gguf");
    let long_names: Vec<String> = (0..20_000)
        .map(|i| format!("t{i:05}{}", "w".repeat(LONG - 6)))
        .collect();
    write_gguf_kv(
        &long,
        "llama",
        &[("llama.block_count", 1)],
        &long_names
            .iter()
            .map(|name| (name.as_str(), 4096u64))
            .collect::<Vec<(&str, u64)>>(),
    );

    std::fs::write(
        sys.join("models.ini"),
        format!(
            "[tensors]\nmodel = {}\n\n[many]\nmodel = {}\n\n[exact]\nmodel = {}\n\n[long]\nmodel = {}\n",
            shard(1).display(),
            many.display(),
            exact.display(),
            long.display()
        ),
    )
    .expect("models.ini");

    let bff = Caller(":1.9".into());
    let own_bff = Requester {
        caller: bff.clone(),
        principal: None,
    };
    let m = Manager::new(
        FakeService::new(),
        FakeAuthority::new(true).quietly(true).bff(&bff),
        FakeResolver::lan(),
    )
    .with_group(FakeGroup::new())
    .with_passwords(FakePasswords::new())
    .with_log(FakeActionLog::new());

    // One row of the list as the reply spells it, with the `,` that follows it:
    // the byte count the bound runs on.
    let row_bytes = |name: &str, bytes: u64| -> usize {
        serde_json::to_string(&serde_json::json!({ "name": name, "bytes": bytes }))
            .expect("row json")
            .len()
            + 1
    };

    let listed = |id: &str| -> (Vec<(String, u64)>, bool) {
        let got = m.model_info(&own_bff, id).expect("the bff's own read");
        let facts = got
            .info
            .as_ref()
            .unwrap_or_else(|| panic!("no facts: {got:?}"));
        (
            facts
                .tensors
                .iter()
                .map(|row| (row.name.clone(), row.bytes))
                .collect(),
            facts.tensors_truncated,
        )
    };

    // ── Every shard's tensor, in file order ──────────────────────────────
    let (list, truncated) = listed("tensors");
    assert_eq!(
        list,
        vec![
            ("token_embd.weight".to_string(), 1 << 20),
            ("blk.0.attn_q.weight".to_string(), 1 << 12),
            ("blk.1.attn_q.weight".to_string(), 1 << 13),
            ("blk.1.ffn_up.weight".to_string(), 1 << 14),
            ("blk.2.attn_q.weight".to_string(), 1 << 15),
            ("output.weight".to_string(), 1 << 16),
        ],
        "the three shards' tensors, each shard after the one before it, with the bytes \
         the header declares"
    );
    assert!(!truncated, "six tensors are under the cap: {list:?}");

    // ── Past the cap: the FIRST 20 000, and the flag ─────────────────────
    let (list, truncated) = listed("many");
    assert_eq!(list.len(), 20_000, "the cap is 20 000 entries");
    assert!(truncated, "a header of 20 001 tensors answers truncated");
    assert_eq!(
        list.first().map(|(name, _)| name.as_str()),
        Some("t00000"),
        "the entries kept are the first ones in file order"
    );
    assert_eq!(
        list.last().map(|(name, _)| name.as_str()),
        Some("t19999"),
        "the 20 000th tensor is the last one kept"
    );
    assert!(
        list.iter().all(|(_, bytes)| *bytes == 4096),
        "every row carries the tensor's bytes"
    );
    let short: usize = list
        .iter()
        .map(|(name, bytes)| row_bytes(name, *bytes))
        .sum();
    assert!(
        short <= pai_core::gguf_shape::MAX_TENSOR_LIST_BYTES,
        "20 001 short names answer 20 000 rows of {short} bytes: under the byte bound, so the \
         ROW bound cut this list and the byte bound never touched it"
    );

    // ── The boundary: 20 000 travel, nothing is truncated ────────────────
    let (list, truncated) = listed("exact");
    assert_eq!(list.len(), 20_000, "the cap is inclusive");
    assert!(
        !truncated,
        "a header of exactly 20 000 tensors is at the cap, not past it"
    );
    assert_eq!(
        list.last().map(|(name, _)| name.as_str()),
        Some("t19999"),
        "the whole list travels"
    );

    // ── The byte bound: long names, cut before the row bound ───────────────
    //
    // The rows of 20 000 tensors of 100-character names pass 917 504 bytes of
    // JSON long before the 20 000th row: the reply carries the rows that fit,
    // from the front, and the flag. The cut is TIGHT: the first row the reply
    // drops is one that would pass the bound, so the list is never shorted.
    let (list, truncated) = listed("long");
    let mut fits = 0usize;
    let mut kept_bytes = 0usize;
    for name in &long_names {
        let row = row_bytes(name, 4096);
        if kept_bytes + row > pai_core::gguf_shape::MAX_TENSOR_LIST_BYTES {
            break;
        }
        kept_bytes += row;
        fits += 1;
    }
    assert!(
        (1..pai_core::gguf_shape::MAX_TENSOR_ROWS).contains(&fits),
        "the fixture's names are long enough for the byte bound to bite before the row \
         bound: {fits} rows fit in {} bytes",
        pai_core::gguf_shape::MAX_TENSOR_LIST_BYTES
    );
    assert_eq!(
        list.iter()
            .map(|(name, _)| name.as_str())
            .collect::<Vec<_>>(),
        long_names
            .iter()
            .take(fits)
            .map(String::as_str)
            .collect::<Vec<_>>(),
        "the rows kept are the first ones in file order, up to the byte bound: {} rows \
         answered, {fits} fit",
        list.len()
    );
    assert!(
        truncated,
        "20 000 tensors whose rows pass the byte bound answer the list truncated"
    );
    assert!(
        kept_bytes + row_bytes(&long_names[fits], 4096)
            > pai_core::gguf_shape::MAX_TENSOR_LIST_BYTES,
        "the row the list drops, `{}`, would pass the bound: {}, so the cut is the first row \
         that passes it, not an earlier one",
        long_names[fits],
        kept_bytes + row_bytes(&long_names[fits], 4096)
    );
}

#[cfg(test)]
use super::fakes::FakeResolver;
use super::*;

fn cfg(port: Option<i32>, host: Option<&str>) -> ServerConfig {
    ServerConfig {
        port,
        hostname: host.map(str::to_string),
        ..Default::default()
    }
}

/// With no live key line in the incoming text, the held key goes inside
/// `[Server]`, not at the end of the file: a hand-edited file with
/// another section after `[Server]` would otherwise put it in that
/// section, where nothing reads it, and the key is lost.
#[test]
fn a_kept_key_with_no_line_goes_into_the_server_section() {
    let key = "sk-local-3f9a";
    for sent in [
        "[Server]\nPort = 8090\nModelsMax = 2\n\n[Extra]\nFoo = 1\n",
        "; Hand-edited.\n[Server]\nPort = 8090\n; OpencodeApiKey = sk-xxx  ; hint\n[Extra]\nFoo = 1\n",
        "[Server]\r\nPort = 8090\r\n\r\n[Extra]\r\nFoo = 1\r\n",
    ] {
        let out = keep_held_api_key(sent, key);
        let cfg = crate::server_cfg::from_text(&out);
        assert_eq!(cfg.opencode_api_key.as_deref(), Some(key), "{out:?}");
        assert_eq!(cfg.port, Some(8090), "{out:?}");
        let line = out.find("OpencodeApiKey = sk-local").expect("key line");
        let extra = out.find("[Extra]").expect("[Extra] kept");
        assert!(line < extra, "the key before [Extra]: {out:?}");
        assert!(
            out[extra..].trim_end().ends_with("Foo = 1"),
            "[Extra] as sent: {out:?}"
        );
        assert_eq!(
            out.lines()
                .filter(|l| l.trim_start().starts_with("OpencodeApiKey ="))
                .count(),
            1,
            "{out:?}"
        );
    }
    // Only [Server]: the line follows its last key, as before.
    assert_eq!(
        keep_held_api_key("[Server]\nPort = 8090\n", key),
        "[Server]\nPort = 8090\nOpencodeApiKey = sk-local-3f9a\n"
    );
}

#[test]
fn action_ids_are_the_agreed_polkit_ids() {
    assert_eq!(Action::Control.id(), "org.privateai.manager.control");
    assert_eq!(Action::Configure.id(), "org.privateai.manager.configure");
}

#[test]
fn config_file_accepts_the_served_files() {
    assert_eq!(
        config_file("server@default.ini"),
        Some(ConfigFile::ServerProfile("default".into()))
    );
    assert_eq!(
        config_file("server@lan-2.ini"),
        Some(ConfigFile::ServerProfile("lan-2".into()))
    );
    assert_eq!(config_file("models.ini"), Some(ConfigFile::Models));
    assert_eq!(
        config_file("models.ini.disabled"),
        Some(ConfigFile::ModelsDisabled)
    );
}

#[test]
fn config_file_refuses_every_other_name() {
    for name in [
        "",
        "../x",
        "../models.ini",
        "./models.ini",
        "sub/models.ini",
        "sub\\models.ini",
        "/etc/private-ai/models.ini",
        "C:\\ProgramData\\private-ai\\config\\models.ini",
        "server.ini",
        "settings.ini",
        "server@Bad.ini",
        "server@.ini",
        "server@a_b.ini",
        "server@../x.ini",
        "server@default.ini.tmp",
        "server@default",
        "models.ini.bak",
        "models.ini.tmp",
        "Presets.ini",
        "models.ini ",
    ] {
        assert_eq!(config_file(name), None, "name {name:?}");
    }
}

// ── Instance files hold a secret: reads need control ───────────────
// Refused before any path is resolved, so these never reach `paths::`.

fn core(
    allow: bool,
) -> Manager<super::fakes::FakeService, super::fakes::FakeAuthority, FakeResolver> {
    Manager::new(
        super::fakes::FakeService::new(),
        super::fakes::FakeAuthority::new(allow),
        FakeResolver::lan(),
    )
    .with_group(super::fakes::FakeGroup::new())
}

fn asked(
    m: &Manager<super::fakes::FakeService, super::fakes::FakeAuthority, FakeResolver>,
) -> Vec<(Caller, Action)> {
    m.authority().asked.lock().unwrap().clone()
}

/// An instance file can hold `OpencodeApiKey`: reading it asks the
/// authority for control, with the caller; a refusal is NotAuthorized
/// and nothing is read, whether the instance exists or not (a refused
/// caller learns nothing about which ids exist).
#[test]
fn reading_an_instance_file_asks_control_and_a_refusal_reads_nothing() {
    let alice = Caller(":1.42".into());
    let m = core(false);
    assert_eq!(
        m.read_file(Some(&alice), "server@default.ini"),
        Err(ManagerError::NotAuthorized)
    );
    assert_eq!(
        m.read_file(Some(&alice), "server@nope.ini"),
        Err(ManagerError::NotAuthorized)
    );
    assert_eq!(asked(&m), vec![(alice.clone(), Action::Control); 2]);
}

/// A transport with no identity for the call (a D-Bus call with no
/// sender, a pipe connection with no token) gets no instance file, and
/// nobody is asked, even an authority that would allow it.
#[test]
fn reading_an_instance_file_with_no_caller_is_refused_unasked() {
    let m = core(true);
    assert_eq!(
        m.read_file(None::<&Caller>, "server@default.ini"),
        Err(ManagerError::NotAuthorized)
    );
    assert_eq!(asked(&m), vec![]);
}

/// A name that is no served file is refused as one before anybody is
/// asked, with or without a caller.
#[test]
fn a_read_of_a_bad_name_asks_nobody() {
    let alice = Caller(":1.42".into());
    let m = core(false);
    for name in ["server.ini", "../server@default.ini", "server@Bad.ini"] {
        assert_eq!(
            m.read_file(Some(&alice), name),
            Err(ManagerError::BadFileName),
            "{name}"
        );
        assert_eq!(
            m.read_file(None::<&Caller>, name),
            Err(ManagerError::BadFileName),
            "{name}"
        );
    }
    assert_eq!(asked(&m), vec![]);
}

// ── Compare-and-swap writes: gates first, as write_file's ───────────
// These resolve no path: every refusal lands before a path is built,
// as write_file's do, so these unit tests never reach `paths::`.

/// A caller without the configure right gets write_file's own refusal,
/// asked once for configure, never the question made without a prompt,
/// and the call reaches nothing past the gate.
#[test]
fn a_compare_and_swap_write_without_configure_is_the_same_refusal() {
    let alice = Caller(":1.42".into());
    let m = core(false);
    assert_eq!(
        m.write_file_if(&alice, "models.ini", "[X]\n", "[Y]\n"),
        Err(ManagerError::NotAuthorized),
        "the same refusal write_file gives"
    );
    assert_eq!(
        asked(&m),
        vec![(alice.clone(), Action::Configure)],
        "configure is asked once, before the expected text is read"
    );
    assert_eq!(
        *m.authority().asked_quietly.lock().unwrap(),
        vec![],
        "a write never asks the question made without a prompt"
    );
    assert_eq!(*m.service().calls.lock().unwrap(), vec![]);
}

/// A name that is no served file is refused as one, the gate asked
/// first: NotAuthorized where write_file is refused, BadFileName where
/// the gate allows, the gate asked once per call either way.
#[test]
fn a_compare_and_swap_write_of_a_bad_name_is_refused_as_write_files() {
    let alice = Caller(":1.42".into());
    let m = core(false);
    for name in ["server.ini", "../models.ini", "models.ini.tmp"] {
        assert_eq!(
            m.write_file_if(&alice, name, "[X]\n", "[Y]\n"),
            Err(ManagerError::NotAuthorized),
            "{name}"
        );
    }
    assert_eq!(
        asked(&m),
        vec![(alice.clone(), Action::Configure); 3],
        "the gate runs before the name is looked at"
    );
    let m = core(true);
    for name in ["server.ini", "../models.ini", "models.ini.tmp"] {
        assert_eq!(
            m.write_file_if(&alice, name, "[X]\n", "[Y]\n"),
            Err(ManagerError::BadFileName),
            "{name}"
        );
    }
    assert_eq!(asked(&m), vec![(alice.clone(), Action::Configure); 3]);
    assert_eq!(*m.service().calls.lock().unwrap(), vec![]);
}

// ── Start at boot ───────────────────────────────────────────────────
// Refused before any path is resolved, so these never reach `paths::`.

/// Enabling an instance at boot is systemd's own `enable`, which asks for
/// an administrator: the configure action. A refused caller is asked
/// once, with its identity, and nothing reaches the service manager,
/// whatever the id.
#[test]
fn setting_the_start_at_boot_asks_configure_and_a_refusal_changes_nothing() {
    let alice = Caller(":1.42".into());
    let m = core(false);
    for (id, on) in [("default", true), ("default", false), ("../x", true)] {
        assert_eq!(
            m.set_boot_enabled(&alice, id, on),
            Err(ManagerError::NotAuthorized),
            "{id} {on}"
        );
    }
    assert_eq!(asked(&m), vec![(alice.clone(), Action::Configure); 3]);
    assert_eq!(*m.service().calls.lock().unwrap(), vec![]);
}

/// The read is open to anyone, like Status: an invalid id is refused as
/// one, nobody asked, the service manager untouched.
#[test]
fn reading_the_start_at_boot_of_an_invalid_id_asks_nobody() {
    let m = core(false);
    for bad in ["", "../x", "Default", "a_b"] {
        assert_eq!(m.boot_enabled(bad), Err(ManagerError::InvalidId), "{bad:?}");
    }
    assert_eq!(asked(&m), vec![]);
    assert_eq!(*m.service().calls.lock().unwrap(), vec![]);
}

/// systemd and the SCM start the enabled instances at boot: the manager
/// starts none at its own start, on either OS.
#[test]
fn the_manager_starts_no_instance_at_its_own_start() {
    let old = concat!("start_all", "_at_boot");
    for (file, text) in [
        ("service.rs", include_str!("service.rs")),
        ("dbus.rs", include_str!("dbus.rs")),
        ("windows/entry.rs", include_str!("windows/entry.rs")),
    ] {
        let code = text.split("#[cfg(test)]").next().unwrap_or_default();
        assert!(!code.contains(old), "{file} still names {old}");
    }
}

/// The manager's start reports the renders that did not land, on both
/// OSes: the failures reach the report, they are not thrown away.
#[test]
fn the_start_reports_the_renders_that_did_not_land() {
    for (name, text) in [
        ("dbus.rs", include_str!("dbus.rs")),
        ("windows/entry.rs", include_str!("windows/entry.rs")),
    ] {
        let code = text.split("#[cfg(test)]").next().unwrap_or_default();
        assert!(
            code.contains("report_render_failures"),
            "{name}: the start reports the renders that did not land"
        );
    }
}

/// The rename of the release: `server@<id>.ini` is a server profile, so
/// the served file is `ConfigFile::ServerProfile`, and the instance
/// record is the `instances.ini` constant, not a name typed at each use.
#[test]
fn the_served_server_file_is_a_server_profile() {
    let code = std::include_str!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/src/manager/service.rs"
    ))
    .split("\n#[cfg(test)]\n")
    .next()
    .unwrap_or_default();
    assert!(
        code.contains("ConfigFile::ServerProfile"),
        "the served file is named for the profile"
    );
    assert!(
        !code.contains("ConfigFile::Instance"),
        "the instance is the section of instances.ini, not a file"
    );
    assert!(
        code.contains("INSTANCES_FILE"),
        "the record is named by one constant"
    );
}

// ── Instance files: mode 0640, group private-ai ─────────────────────

/// The instance file can hold a secret: 0640, read by root and the
/// instances' group. The model profile files hold none: 0644, as before.
#[test]
fn file_mode_is_0640_for_instance_files_and_0644_for_the_model_profiles() {
    assert_eq!(
        file_mode(&ConfigFile::ServerProfile("default".into())),
        0o640
    );
    assert_eq!(file_mode(&ConfigFile::ServerProfile("lan-2".into())), 0o640);
    assert_eq!(file_mode(&ConfigFile::Models), 0o644);
    assert_eq!(file_mode(&ConfigFile::ModelsDisabled), 0o644);
}

/// The group is the service user's, as the Linux package's sysusers.d
/// entry names it (xtask `system_files::sysusers`).
#[test]
fn the_instance_group_is_the_service_users() {
    assert_eq!(INSTANCE_GROUP, "private-ai");
}

#[cfg(unix)]
mod unix_files {
    use super::super::fakes::{FakeGroup, GroupCall};
    use super::super::*;
    use std::collections::BTreeSet;
    use std::io::ErrorKind;
    use std::os::unix::fs::{MetadataExt, PermissionsExt};
    use std::path::Path;

    fn mode(p: &Path) -> u32 {
        std::fs::metadata(p).expect("metadata").permissions().mode() & 0o7777
    }

    fn names(dir: &Path) -> BTreeSet<String> {
        std::fs::read_dir(dir)
            .expect("read_dir")
            .flatten()
            .map(|e| e.file_name().to_string_lossy().into_owned())
            .collect()
    }

    fn calls(g: &FakeGroup) -> Vec<GroupCall> {
        g.calls.lock().unwrap().clone()
    }

    const INI: &str = "[Server]\nPort = 8080\nOpencodeApiKey = sk-local-3f9a\n";

    /// A new instance file: 0640 from the start (the temp file had the
    /// mode when the group was set, so the secret was never world
    /// readable), its group set to private-ai on the temp file before
    /// the rename, no temp file left.
    #[test]
    fn an_instance_file_is_written_0640_with_its_group_set_before_the_rename() {
        let dir = tempfile::tempdir().expect("tempdir");
        let path = dir.path().join("server@default.ini");
        let g = FakeGroup::new();
        write_served_file(&path, &ConfigFile::ServerProfile("default".into()), INI, &g)
            .expect("write");
        assert_eq!(std::fs::read_to_string(&path).unwrap(), INI);
        assert_eq!(mode(&path), 0o640);
        let c = calls(&g);
        assert_eq!(c.len(), 1, "one group change: {c:?}");
        assert_eq!(c[0].group, "private-ai");
        assert_eq!(
            c[0].mode, 0o640,
            "the temp file was 0640 when its group was set"
        );
        assert_ne!(
            c[0].path, path,
            "the group is set on the temp file, before the rename"
        );
        assert_eq!(
            c[0].path.parent(),
            Some(dir.path()),
            "the temp file is beside it"
        );
        assert!(!c[0].path.exists(), "the temp file was renamed");
        assert_eq!(
            names(dir.path()),
            BTreeSet::from(["server@default.ini".to_string()])
        );
    }

    /// A file the previous version wrote 0644 becomes 0640 on its next
    /// write: the rename replaces it with the new file.
    #[test]
    fn a_0644_instance_file_is_0640_after_its_next_write() {
        let dir = tempfile::tempdir().expect("tempdir");
        let path = dir.path().join("server@default.ini");
        std::fs::write(&path, "[Server]\nPort = 8081\n").unwrap();
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o644)).unwrap();
        let g = FakeGroup::new();
        write_served_file(&path, &ConfigFile::ServerProfile("default".into()), INI, &g)
            .expect("write");
        assert_eq!(std::fs::read_to_string(&path).unwrap(), INI);
        assert_eq!(mode(&path), 0o640);
        assert_eq!(calls(&g).len(), 1);
    }

    /// The model profile files hold no secret: 0644, whatever was there, and
    /// their group is left alone (root's, the manager's own).
    #[test]
    fn model_profile_files_are_written_0644_and_keep_their_group() {
        let dir = tempfile::tempdir().expect("tempdir");
        let model_profiles = dir.path().join("models.ini");
        std::fs::write(&model_profiles, "[Old]\nmodel = /m/old.gguf\n").unwrap();
        std::fs::set_permissions(&model_profiles, std::fs::Permissions::from_mode(0o600)).unwrap();
        let g = FakeGroup::new();
        let text = "[Qwen3.8-27B]\nmodel = /models/Qwen3.8-27B-Q8_0.gguf\n";
        write_served_file(&model_profiles, &ConfigFile::Models, text, &g).expect("model-profiles");
        let disabled = dir.path().join("models.ini.disabled");
        write_served_file(&disabled, &ConfigFile::ModelsDisabled, text, &g).expect("disabled");
        assert_eq!(std::fs::read_to_string(&model_profiles).unwrap(), text);
        assert_eq!(std::fs::read_to_string(&disabled).unwrap(), text);
        assert_eq!(mode(&model_profiles), 0o644);
        assert_eq!(mode(&disabled), 0o644);
        assert_eq!(calls(&g), vec![], "no group change for a preset file");
        assert_eq!(
            names(dir.path()),
            BTreeSet::from(["models.ini".to_string(), "models.ini.disabled".to_string()])
        );
    }

    /// No group private-ai (the package's sysusers.d entry did not run):
    /// a 0640 root:root file would stop the instance from reading its
    /// config at the next start, and 0644 would show the secret, so the
    /// write is refused with the seam's NotFound, the old file kept and
    /// no temp file left.
    #[test]
    fn a_missing_group_refuses_the_write_and_keeps_the_old_file() {
        let dir = tempfile::tempdir().expect("tempdir");
        let path = dir.path().join("server@default.ini");
        let old = "[Server]\nPort = 8081\n";
        std::fs::write(&path, old).unwrap();
        let g = FakeGroup::failing(ErrorKind::NotFound);
        let e = write_served_file(&path, &ConfigFile::ServerProfile("default".into()), INI, &g)
            .expect_err("a missing group refuses the write");
        assert_eq!(e.kind(), ErrorKind::NotFound);
        assert_eq!(
            std::fs::read_to_string(&path).unwrap(),
            old,
            "the old file is kept"
        );
        assert_eq!(calls(&g).len(), 1);
        assert_eq!(
            names(dir.path()),
            BTreeSet::from(["server@default.ini".to_string()])
        );

        // A new instance: nothing created at all.
        let new = dir.path().join("server@lan-2.ini");
        let e = write_served_file(&new, &ConfigFile::ServerProfile("lan-2".into()), INI, &g)
            .expect_err("refused");
        assert_eq!(e.kind(), ErrorKind::NotFound);
        assert!(!new.exists());
        assert_eq!(
            names(dir.path()),
            BTreeSet::from(["server@default.ini".to_string()])
        );
    }

    /// A write that cannot create its temp file fails and leaves the
    /// old file and the directory as they were.
    #[test]
    fn a_failed_write_leaves_the_old_file_and_no_temp() {
        let dir = tempfile::tempdir().expect("tempdir");
        let path = dir.path().join("server@default.ini");
        let old = "[Server]\nPort = 8081\n";
        std::fs::write(&path, old).unwrap();
        std::fs::set_permissions(dir.path(), std::fs::Permissions::from_mode(0o555)).unwrap();
        let probe = std::fs::write(dir.path().join("probe"), "x");
        let g = FakeGroup::new();
        let result =
            write_served_file(&path, &ConfigFile::ServerProfile("default".into()), INI, &g);
        std::fs::set_permissions(dir.path(), std::fs::Permissions::from_mode(0o755)).unwrap();
        if probe.is_ok() {
            eprintln!("skipped: the directory's mode does not stop this user (root)");
            return;
        }
        assert!(result.is_err(), "the write must fail: {result:?}");
        assert_eq!(std::fs::read_to_string(&path).unwrap(), old);
        assert_eq!(
            names(dir.path()),
            BTreeSet::from(["server@default.ini".to_string()])
        );
        assert_eq!(
            calls(&g),
            vec![],
            "no group change for a file never written"
        );
    }

    /// The group of `name` and its gid, from the system's group
    /// database (`getent group`): read only.
    fn gid_of(name: &str) -> u32 {
        let out = std::process::Command::new("getent")
            .args(["group", name])
            .output()
            .expect("getent");
        let line = String::from_utf8(out.stdout).expect("utf-8");
        line.trim()
            .split(':')
            .nth(2)
            .expect("gid field")
            .parse()
            .expect("gid")
    }

    /// The real seam: an unknown group is NotFound and the file's group
    /// is unchanged.
    #[test]
    fn the_system_group_seam_refuses_an_unknown_group_with_not_found() {
        let dir = tempfile::tempdir().expect("tempdir");
        let path = dir.path().join("server@default.ini");
        std::fs::write(&path, INI).unwrap();
        let before = std::fs::metadata(&path).unwrap().gid();
        let e = SystemGroup
            .set_group(&path, "pai-no-such-group-3f9a")
            .expect_err("unknown group");
        assert_eq!(e.kind(), ErrorKind::NotFound, "{e:?}");
        assert_eq!(std::fs::metadata(&path).unwrap().gid(), before);
    }

    /// The real seam on a group this user belongs to (chown(2) lets an
    /// owner give a file any group it is a member of, no root needed):
    /// a supplementary group when there is one, so the change shows.
    #[test]
    fn the_system_group_seam_sets_a_group_of_this_user() {
        let out = std::process::Command::new("id")
            .arg("-Gn")
            .output()
            .expect("id -Gn");
        let groups: Vec<String> = String::from_utf8(out.stdout)
            .expect("utf-8")
            .split_whitespace()
            .map(str::to_string)
            .collect();
        let group = groups.get(1).or(groups.first()).expect("a group").clone();
        let dir = tempfile::tempdir().expect("tempdir");
        let path = dir.path().join("server@default.ini");
        std::fs::write(&path, INI).unwrap();
        let owner = std::fs::metadata(&path).unwrap().uid();
        SystemGroup.set_group(&path, &group).expect("set_group");
        let m = std::fs::metadata(&path).unwrap();
        assert_eq!(m.gid(), gid_of(&group), "group {group}");
        assert_eq!(m.uid(), owner, "the owner is unchanged");
        assert_eq!(std::fs::read_to_string(&path).unwrap(), INI);
    }
}

fn ip(s: &str) -> IpAddr {
    s.parse().expect("ip")
}

fn bind(port: i32, ips: &[&str]) -> Binding {
    Binding {
        port,
        ips: ips.iter().map(|s| ip(s)).collect(),
    }
}

/// The binding of a config through the LAN fake resolver, or the name
/// that did not resolve.
fn resolved(port: Option<i32>, host: Option<&str>) -> Result<Binding, String> {
    binding(&cfg(port, host), &FakeResolver::lan())
}

fn collide(a: (Option<i32>, Option<&str>), b: (Option<i32>, Option<&str>)) -> bool {
    let a = resolved(a.0, a.1).expect("a resolves");
    let b = resolved(b.0, b.1).expect("b resolves");
    addresses_collide(&a, &b)
}

#[test]
fn binding_refuses_a_host_that_does_not_resolve() {
    assert_eq!(
        resolved(Some(8081), Some("nowhere.example")),
        Err("nowhere.example".into())
    );
    // The first unresolvable host of a list is named.
    assert_eq!(
        resolved(Some(8081), Some("127.0.0.1, gone.example, nowhere.example")),
        Err("gone.example".into())
    );
}

#[test]
fn binding_takes_literal_ips_as_themselves_without_resolving() {
    let r = FakeResolver::new(&[]);
    assert_eq!(
        binding(&cfg(Some(8081), Some("127.0.0.1, ::1, 0.0.0.0, ::")), &r),
        Ok(bind(8081, &["127.0.0.1", "::1", "0.0.0.0", "::"]))
    );
    assert!(
        r.asked.lock().unwrap().is_empty(),
        "a literal IP is not resolved"
    );
}

#[test]
fn binding_resolves_names_and_applies_defaults() {
    let r = FakeResolver::lan();
    assert_eq!(
        binding(&cfg(Some(8081), Some("multi-home")), &r),
        Ok(bind(8081, &["10.0.0.9", "192.168.1.5"]))
    );
    // No Port and no Hostname: the default port on localhost, resolved.
    assert_eq!(
        binding(&cfg(None, None), &r),
        Ok(bind(crate::server_cfg::DEFAULT_PORT, &["127.0.0.1"]))
    );
    assert_eq!(
        binding(&cfg(None, Some("   ")), &r),
        Ok(bind(crate::server_cfg::DEFAULT_PORT, &["127.0.0.1"])),
        "a blank Hostname is localhost"
    );
}

#[test]
fn same_port_and_a_common_address_collide() {
    let a = bind(8081, &["127.0.0.1"]);
    assert!(addresses_collide(&a, &a.clone()));
    assert!(addresses_collide(
        &bind(8081, &["10.0.0.1", "192.168.1.5"]),
        &bind(8081, &["192.168.1.5"])
    ));
    assert!(addresses_collide(
        &bind(8081, &["::1"]),
        &bind(8081, &["::1"])
    ));
}

#[test]
fn different_port_or_disjoint_addresses_do_not_collide() {
    assert!(!addresses_collide(
        &bind(8081, &["127.0.0.1"]),
        &bind(8082, &["127.0.0.1"])
    ));
    assert!(!addresses_collide(
        &bind(8081, &["127.0.0.1"]),
        &bind(8081, &["192.168.1.5"])
    ));
    assert!(!addresses_collide(
        &bind(8081, &["127.0.0.1"]),
        &bind(8081, &["::1"])
    ));
    assert!(!addresses_collide(
        &bind(8081, &["0.0.0.0"]),
        &bind(8082, &["0.0.0.0"])
    ));
}

#[test]
fn a_wildcard_collides_with_every_address_on_its_port() {
    // Both wildcards, IPv4 and IPv6 alike: `::` is dual-stack on Linux by
    // default, and `0.0.0.0` is pinned the same way to stay on the safe
    // side.
    for wild in ["0.0.0.0", "::"] {
        for other in [
            "127.0.0.1",
            "192.168.1.5",
            "::1",
            "fe80::1",
            "0.0.0.0",
            "::",
        ] {
            let w = bind(8081, &[wild]);
            let o = bind(8081, &[other]);
            assert!(addresses_collide(&w, &o), "{wild} vs {other}");
            assert!(addresses_collide(&o, &w), "{other} vs {wild}");
        }
    }
    assert!(addresses_collide(
        &bind(8081, &["10.0.0.1", "0.0.0.0"]),
        &bind(8081, &["192.168.1.5"])
    ));
}

#[test]
fn resolved_names_collide_by_address() {
    // localhost is 127.0.0.1.
    assert!(collide(
        (Some(8081), Some("localhost")),
        (Some(8081), Some("127.0.0.1"))
    ));
    assert!(collide(
        (Some(8081), Some("127.0.0.1")),
        (Some(8081), Some("localhost"))
    ));
    // The default host is localhost.
    assert!(collide(
        (None, None),
        (Some(crate::server_cfg::DEFAULT_PORT), Some("127.0.0.1"))
    ));
    // Two different names of one address.
    assert!(collide(
        (Some(8081), Some("gpu-box")),
        (Some(8081), Some("gpu-box.lan"))
    ));
    assert!(collide(
        (Some(8081), Some("gpu-box")),
        (Some(8081), Some("192.168.1.5"))
    ));
    // A name with several addresses, one matching.
    assert!(collide(
        (Some(8081), Some("multi-home")),
        (Some(8081), Some("gpu-box"))
    ));
    assert!(collide(
        (Some(8081), Some("multi-home")),
        (Some(8081), Some("10.0.0.9"))
    ));
    // No common address.
    assert!(!collide(
        (Some(8081), Some("localhost")),
        (Some(8081), Some("gpu-box"))
    ));
    // Names on different ports never collide.
    for (a, b) in [
        ("localhost", "127.0.0.1"),
        ("gpu-box", "gpu-box.lan"),
        ("multi-home", "gpu-box"),
        ("0.0.0.0", "localhost"),
    ] {
        assert!(
            !collide((Some(8081), Some(a)), (Some(8082), Some(b))),
            "{a} vs {b}"
        );
    }
}

#[test]
fn defaults_count_in_a_collision() {
    assert!(collide((None, None), (None, None)));
    assert!(collide(
        (None, None),
        (Some(crate::server_cfg::DEFAULT_PORT), Some("localhost"))
    ));
    assert!(!collide((None, None), (Some(8081), None)));
    assert!(!collide((None, None), (None, Some("192.168.1.5"))));
}

// ── Authenticate: the bff's sign-in, checked by the manager ────────
// No file is read, so these never reach `paths::`.

type AuthCore = Manager<super::fakes::FakeService, super::fakes::FakeAuthority, FakeResolver>;

/// The bff's account as the core sees it (a bus name here; on Windows the
/// service SID of `NT SERVICE\private-ai-bff`).
fn bff() -> Caller {
    Caller(":1.9".into())
}

/// A manager whose authority takes [`bff`] for the bff's account, answering
/// `allow` to every polkit question, over `passwords`.
fn auth_core(allow: bool, passwords: super::fakes::FakePasswords) -> AuthCore {
    Manager::new(
        super::fakes::FakeService::new(),
        super::fakes::FakeAuthority::new(allow).bff(&bff()),
        FakeResolver::lan(),
    )
    .with_group(super::fakes::FakeGroup::new())
    .with_passwords(passwords)
}

/// The password checker's logs: (checked, asked_admin).
type AuthLogs = (
    std::sync::Arc<std::sync::Mutex<Vec<(String, String)>>>,
    std::sync::Arc<std::sync::Mutex<Vec<String>>>,
);

fn passwords() -> (super::fakes::FakePasswords, AuthLogs) {
    let p = super::fakes::FakePasswords::new();
    let logs = (p.checked.clone(), p.asked_admin.clone());
    (p, logs)
}

fn checked(logs: &AuthLogs) -> Vec<(String, String)> {
    logs.0.lock().unwrap().clone()
}

fn asked_admin(logs: &AuthLogs) -> Vec<String> {
    logs.1.lock().unwrap().clone()
}

/// Only the bff's account may ask: any other caller, one polkit would
/// allow everything included, is NotAuthorized and no password is
/// checked; nor is the empty caller (a pipe connection with no token).
#[test]
fn authenticate_answers_only_the_bff() {
    for caller in [
        Caller(":1.42".into()),
        Caller(String::new()),
        Caller("S-1-5-18".into()),
    ] {
        let (p, logs) = passwords();
        let m = auth_core(true, p);
        assert_eq!(
            m.authenticate(&caller, "alice", "s3cret-Pw"),
            Err(ManagerError::NotAuthorized),
            "{caller:?}"
        );
        assert_eq!(checked(&logs), vec![], "{caller:?}: a password checked");
        assert_eq!(asked_admin(&logs), Vec::<String>::new(), "{caller:?}");
        assert_eq!(
            *m.authority().asked_bff.lock().unwrap(),
            vec![caller.clone()],
            "the authority is asked whether {caller:?} is the bff"
        );
    }
}

/// The bff's own polkit rights do not matter: it is answered for being
/// the bff, and nothing asks polkit about it.
#[test]
fn authenticate_asks_polkit_nothing() {
    let (p, logs) = passwords();
    let m = auth_core(false, p);
    assert_eq!(
        m.authenticate(&bff(), "alice", "s3cret-Pw"),
        Ok((true, true))
    );
    assert_eq!(asked(&m), vec![]);
    assert_eq!(*m.authority().asked_quietly.lock().unwrap(), vec![]);
    assert_eq!(checked(&logs).len(), 1);
}

/// An empty user or password is no: (false, false), the checker never
/// asked (PAM would take an empty password for some accounts).
#[test]
fn an_empty_user_or_password_is_no_and_asks_no_checker() {
    for (user, password) in [("", "s3cret-Pw"), ("alice", ""), ("", "")] {
        let (p, logs) = passwords();
        let m = auth_core(true, p);
        assert_eq!(
            m.authenticate(&bff(), user, password),
            Ok((false, false)),
            "{user:?}"
        );
        assert_eq!(checked(&logs), vec![], "{user:?}");
        assert_eq!(asked_admin(&logs), Vec::<String>::new(), "{user:?}");
    }
}

/// A wrong password and an unknown user get the same answer, and the
/// group membership of nobody is looked up.
#[test]
fn a_wrong_password_and_an_unknown_user_are_the_same_no() {
    for (user, password) in [
        ("alice", "wrong"),
        ("alice", "S3CRET-PW"),
        ("bob", "s3cret-Pw"),
        ("mallory", "s3cret-Pw"),
        ("mallory", "anything"),
    ] {
        let (p, logs) = passwords();
        let m = auth_core(true, p);
        assert_eq!(
            m.authenticate(&bff(), user, password),
            Ok((false, false)),
            "{user:?} / {password:?}"
        );
        assert_eq!(
            checked(&logs),
            vec![(user.to_string(), password.to_string())]
        );
        assert_eq!(
            asked_admin(&logs),
            Vec::<String>::new(),
            "{user:?}: membership looked up after a failed check"
        );
    }
}

/// The right password: ok, and whether the user may configure, asked
/// for that user only after the check.
#[test]
fn a_right_password_says_whether_the_user_may_configure() {
    for (user, password, admin) in [("alice", "s3cret-Pw", true), ("bob", "hunter2!", false)] {
        let (p, logs) = passwords();
        let m = auth_core(true, p);
        assert_eq!(
            m.authenticate(&bff(), user, password),
            Ok((true, admin)),
            "{user}"
        );
        assert_eq!(
            checked(&logs),
            vec![(user.to_string(), password.to_string())]
        );
        assert_eq!(asked_admin(&logs), vec![user.to_string()]);
    }
}

/// The user and password reach the checker exactly as sent: nothing
/// trimmed or folded (a password may start or end with a space).
#[test]
fn the_user_and_password_reach_the_checker_unchanged() {
    let (p, logs) = passwords();
    let m = auth_core(true, p);
    let _ = m.authenticate(&bff(), " alice", " s3cret-Pw ");
    let _ = m.authenticate(&bff(), "Alice", "pässwörd\u{1f511}");
    assert_eq!(
        checked(&logs),
        vec![
            (" alice".to_string(), " s3cret-Pw ".to_string()),
            ("Alice".to_string(), "pässwörd\u{1f511}".to_string()),
        ]
    );
}

/// A checker that fails (PAM unusable) is a Service error with its own
/// message, not a no: the bff can tell "try again later" from "wrong
/// password". The password is in neither the error nor its reply.
#[test]
fn a_failing_checker_is_a_service_error_without_the_password() {
    let failure = "pam_authenticate: Authentication service cannot retrieve authentication info";
    let p = super::fakes::FakePasswords::failing(failure);
    let logs = p.asked_admin.clone();
    let m = auth_core(true, p);
    let result = m.authenticate(&bff(), "alice", "s3cret-Pw");
    assert_eq!(
        result,
        Err(ManagerError::Service(ServiceError(failure.into())))
    );
    let (name, message) = error_reply(&result.clone().unwrap_err());
    for text in [format!("{result:?}"), name, message] {
        assert!(!text.contains("s3cret-Pw"), "the password in {text:?}");
    }
    assert_eq!(*logs.lock().unwrap(), Vec::<String>::new());
}

/// No answer carries the password, whatever the outcome.
#[test]
fn no_answer_carries_the_password() {
    for (caller, user, password) in [
        (bff(), "alice", "s3cret-Pw"),
        (bff(), "alice", "wrong-Pw-xyz"),
        (bff(), "mallory", "wrong-Pw-xyz"),
        (Caller(":1.42".into()), "alice", "s3cret-Pw"),
    ] {
        let (p, _) = passwords();
        let m = auth_core(true, p);
        let result = m.authenticate(&caller, user, password);
        let mut texts = vec![format!("{result:?}")];
        if let Err(e) = &result {
            let (name, message) = error_reply(e);
            texts.extend([name, message]);
        }
        for text in texts {
            assert!(!text.contains(password), "{password:?} in {text:?}");
        }
    }
}

/// Linux: only a member of `pai`, the group the packages create, may
/// configure; `sudo` and `wheel` give nothing here. Group names are
/// case-sensitive and matched whole.
#[test]
fn only_a_pai_member_may_configure() {
    let groups = |names: &[&str]| names.iter().map(|n| n.to_string()).collect::<Vec<_>>();
    for yes in [
        &["pai"][..],
        &["alice", "adm", "pai", "plugdev"],
        &["sudo", "wheel", "pai"],
    ] {
        assert!(in_pai_group(&groups(yes)), "{yes:?}");
    }
    for no in [
        &[][..],
        &["alice", "users", "adm", "plugdev"],
        &["sudo"],
        &["wheel"],
        &["sudo", "wheel", "root"],
        &["Pai"],
        &["PAI"],
        &["pai2"],
        &["pai-users"],
        &[" pai"],
        &["pai "],
        &["admin"],
        &["Administrators"],
        &["private-ai"],
        &[""],
    ] {
        assert!(!in_pai_group(&groups(no)), "{no:?}");
    }
}

// ── Acting for a signed-in user: the caller, the name, the membership ─
// Every call here is refused before a path is resolved and no file is
// written, so these never reach `paths::`. The e2e phase
// `manager_principal_e2e` pins the calls that run, with the files.

/// A request from `caller` vouching for `principal`.
fn request(caller: &Caller, principal: &str) -> Requester {
    Requester {
        caller: caller.clone(),
        principal: Some(Principal(principal.to_string())),
    }
}

/// Every guarded call of `m` for `who`, in the order (start, stop,
/// restart, start at boot, read an instance file). Writing a file needs
/// the data root redirect, so the e2e phase pins that one.
fn guarded_calls(m: &AuthCore, who: &Requester) -> Vec<Result<(), ManagerError>> {
    vec![
        m.start(who, "default"),
        m.stop(who, "default"),
        m.restart(who, "default"),
        m.set_boot_enabled(who, "default", true),
        m.read_file(Some(who), "server@default.ini").map(|_| ()),
    ]
}

/// A request with no principal is the caller's own call, exactly as
/// today: the same polkit questions in the same order, with the caller,
/// no account lookup, and no question about whose account the bff's is.
#[test]
fn a_request_with_no_principal_is_the_callers_own() {
    let alice = Caller(":1.42".into());
    let who = Requester {
        caller: alice.clone(),
        principal: None,
    };
    let (p, logs) = passwords();
    let m = auth_core(true, p);
    let _ = guarded_calls(&m, &who);
    assert_eq!(
        asked(&m),
        vec![
            (alice.clone(), Action::Control),
            (alice.clone(), Action::Control),
            (alice.clone(), Action::Control),
            (alice.clone(), Action::Configure),
            (alice.clone(), Action::Control),
        ],
        "the calls for the action, with the caller, as today"
    );
    assert_eq!(
        *m.authority().asked_quietly.lock().unwrap(),
        vec![(alice, Action::Configure)],
        "the read asks configure without a prompt, as today"
    );
    assert_eq!(
        asked_admin(&logs),
        Vec::<String>::new(),
        "no account is looked up for a caller's own call"
    );
    assert_eq!(
        *m.authority().asked_bff.lock().unwrap(),
        Vec::<Caller>::new(),
        "a call with no principal is never checked against the bff"
    );
}

/// Only the bff's account may name a principal. A caller polkit would
/// allow everything, and the empty caller of a pipe connection with no
/// token, are refused the principal: NotAuthorized, the membership of the
/// name not looked up, nothing asked beyond the caller, and the call
/// refused rather than run as the caller's own.
#[test]
fn a_principal_named_by_a_caller_that_is_not_the_bff_is_refused_unasked() {
    for caller in [
        Caller(":1.42".into()),
        Caller(String::new()),
        Caller("S-1-5-18".into()),
    ] {
        let (p, logs) = passwords();
        let m = auth_core(true, p);
        let who = request(&caller, "alice");
        assert_eq!(
            guarded_calls(&m, &who),
            vec![Err(ManagerError::NotAuthorized); 5],
            "{caller:?} naming alice"
        );
        assert_eq!(
            asked(&m),
            vec![],
            "{caller:?}: polkit asked, so the call ran as the caller"
        );
        assert_eq!(
            *m.authority().asked_quietly.lock().unwrap(),
            vec![],
            "{caller:?}"
        );
        assert_eq!(
            asked_admin(&logs),
            Vec::<String>::new(),
            "{caller:?}: a membership looked up for a caller that is not the bff"
        );
        assert_eq!(
            *m.service().calls.lock().unwrap(),
            vec![],
            "{caller:?}: something reached the service manager"
        );
        assert_eq!(
            *m.authority().asked_bff.lock().unwrap(),
            vec![caller.clone(); 5],
            "the caller is checked first, once per call"
        );
    }
}

/// The bff naming an account that is no member of `pai` (bob, an ordinary
/// account), an unknown account, or root: refused, with the membership of
/// the name asked once per call and nothing else done. A failed lookup is
/// no membership, and being the machine's superuser is not one either.
#[test]
fn a_principal_who_is_not_a_member_is_refused_and_nothing_is_done() {
    for user in ["bob", "mallory", "root"] {
        let (p, logs) = passwords();
        let m = auth_core(true, p);
        let who = request(&bff(), user);
        assert_eq!(
            guarded_calls(&m, &who),
            vec![Err(ManagerError::NotAuthorized); 5],
            "{user}"
        );
        assert_eq!(
            asked_admin(&logs),
            vec![user.to_string(); 5],
            "{user}: the membership is asked once per call"
        );
        assert_eq!(asked(&m), vec![], "{user}: polkit asked");
        assert_eq!(
            *m.authority().asked_quietly.lock().unwrap(),
            vec![],
            "{user}"
        );
        assert_eq!(
            *m.service().calls.lock().unwrap(),
            vec![],
            "{user}: a call reached the service manager"
        );
    }
}

/// A name the manager will not put in a record: no account is looked up,
/// nothing is asked beyond the caller, nothing runs. The length bound is
/// the bff's own, `MAX_USER_BYTES` in `crates/bff/src/session.rs`.
#[test]
fn an_illegible_principal_name_is_refused_with_nothing_asked() {
    let long = "a".repeat(257);
    for name in [
        "",
        "a\0b",
        "ali\nce",
        "alice\n",
        "alice\r\n",
        "ali\tce",
        "ali\u{7f}ce",
        &long,
    ] {
        let (p, logs) = passwords();
        let m = auth_core(true, p);
        let who = request(&bff(), name);
        assert_eq!(
            guarded_calls(&m, &who),
            vec![Err(ManagerError::NotAuthorized); 5],
            "{name:?}"
        );
        assert_eq!(
            asked_admin(&logs),
            Vec::<String>::new(),
            "{name:?}: the name reached the account lookup"
        );
        assert_eq!(asked(&m), vec![], "{name:?}: polkit asked");
        assert_eq!(
            *m.service().calls.lock().unwrap(),
            vec![],
            "{name:?}: a call reached the service manager"
        );
        assert_eq!(
            *m.authority().asked_bff.lock().unwrap(),
            vec![bff(); 5],
            "{name:?}: the caller is checked first, once per call"
        );
    }
}

/// 256 bytes, the longest the bff sends, is well formed: what refuses it
/// is the membership, asked for exactly that name, not the name.
#[test]
fn a_256_byte_principal_name_is_well_formed() {
    let name = "a".repeat(256);
    let (p, logs) = passwords();
    let m = auth_core(true, p);
    let who = request(&bff(), &name);
    assert_eq!(
        guarded_calls(&m, &who),
        vec![Err(ManagerError::NotAuthorized); 5]
    );
    assert_eq!(
        asked_admin(&logs),
        vec![name.clone(); 5],
        "asked as a name, and unknown as an account"
    );
    assert_eq!(asked(&m), vec![]);
}
/// The bff naming a non-member, and a caller that is not the bff naming
/// a member: `write_file_if` is refused exactly as the other guarded
/// calls are, with the membership of the name asked (or not) and the
/// file never read.
#[test]
fn a_compare_and_swap_write_answers_to_the_principal_gates() {
    let (p, logs) = passwords();
    let m = auth_core(true, p);
    assert_eq!(
        m.write_file_if(&request(&bff(), "bob"), "models.ini", "[X]\n", "[Y]\n"),
        Err(ManagerError::NotAuthorized),
        "a non-member is refused as the other guarded calls refuse it"
    );
    assert_eq!(
        asked_admin(&logs),
        vec!["bob".to_string()],
        "the membership is asked once, of the principal named"
    );
    assert_eq!(
        m.write_file_if(
            &request(&Caller(":1.42".into()), "alice"),
            "models.ini",
            "[X]\n",
            "[Y]\n"
        ),
        Err(ManagerError::NotAuthorized),
        "only the bff's account vouches for a user"
    );
    assert_eq!(
        asked_admin(&logs),
        vec!["bob".to_string()],
        "a caller that is not the bff gets no account looked up"
    );
    assert_eq!(
        asked(&m),
        vec![],
        "polkit decides nothing about a principal"
    );
    assert_eq!(
        *m.authority().asked_bff.lock().unwrap(),
        vec![bff(), Caller(":1.42".into())],
        "the caller is checked against the bff, once per call"
    );
    assert_eq!(*m.service().calls.lock().unwrap(), vec![]);
}

/// `Changed` answers with its own transport name and the kind's own
/// word as message, and `error_kind` gives the same word the action
/// record writes: the bff and the logs pin it.
#[test]
fn the_changed_refusal_has_its_own_error_name() {
    let e = ManagerError::Changed;
    assert_eq!(
        error_reply(&e),
        (
            "org.privateai.Manager1.Error.Changed".to_string(),
            "Changed".to_string()
        )
    );
    assert_eq!(error_kind(&e), "Changed");
}

/// The method's shape, pinned in the production text: the caller's
/// whole text to compare (`expected`) and the text to write
/// (`contents`), and no `keep_api_key`: the caller sent the file as it
/// read it, key line included.
#[test]
fn write_file_if_takes_expected_and_contents_and_no_key_flag() {
    let source = std::include_str!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/src/manager/service.rs"
    ));
    let production = source
        .split("\n#[cfg(test)]\n")
        .next()
        .expect("production text of service.rs");
    let at = production
        .find("pub fn write_file_if")
        .expect("the method is there to be tested");
    let signature = &production[at..at + production[at..].find('{').expect("a body")];
    for param in ["who: &W", "name: &str", "contents: &str", "expected: &str"] {
        assert!(signature.contains(param), "the signature lacks {param:?}");
    }
    assert!(
        !signature.contains("keep_api_key"),
        "the compare-and-swap takes no key flag"
    );
}

/// Creating a model profile (`CreateModelProfile`, ARCHITECTURE.md "Create,
/// clone and delete model profiles from the web", slice MP1, as amended by
/// "Instance profiles" and its key split): `[<profile>]` and one line per
/// value, appended to `models.ini`, the one pool, listed in no instance
/// profile, so it renders nowhere. Never overwriting a name the pool holds,
/// and every other byte of the tree is kept: the instance record, the
/// instance profiles, every render, and the retired `models.ini.disabled` and
/// `assignments.ini` of an install, which no verb reads or writes any more.
/// The values are the file pick, the keys llama.cpp reads, sorted by key
/// before anything is built, and the manager judges every byte itself before
/// the lock, in this order: the shape (exactly one of `from` and `values`;
/// `model` xor `hf-repo`; `hf-file` only with `hf-repo`; the three keys only)
/// is `model-profile-bad-source [profile]`, an empty, control-space ridden or
/// marker-carrying value is `model-profile-bad-text [key]` or `comment-markers
/// [key]`, then the id is `invalid-model-profile-id [profile]`, a name the
/// pool holds in any letter case is `model-profile-name-taken [profile]`, a
/// value naming a placement key of the keys that moved to the entry
/// (`checks::PLACEMENT_KEYS`) is `model-profile-placement-key [key]`, and a
/// clone's source that is no pool id, `*` included, is
/// `no-model-profile-named [from]`. A clone is the source section's stay keys
/// (`checks::MODEL_PROFILE_STAY_KEYS`), byte for byte, under the new header,
/// comments, key order and `load-on-startup` included, and carries none of
/// the keys that moved to the entry. The write is a served write of the pool:
/// `configure` first, the `As` form answered only to the bff's account for a
/// current member of `pai`, one record of the action log per call.
#[cfg(test)]
pub(crate) fn manager_model_profile_create_e2e() {
    use fakes::{
        FakeActionLog, FakeAuthority, FakeGroup, FakePasswords, FakeResolver, FakeService,
    };

    let dir = tempfile::tempdir().expect("tempdir");
    crate::paths::redirect_for_tests(dir.path());
    let sys = crate::paths::engine_config_dir();
    std::fs::create_dir_all(&sys).expect("engine config dir");

    let bff = Caller(":1.9".into());
    let member = Requester {
        caller: bff.clone(),
        principal: Some(Principal("alice".into())),
    };
    let outsider = Requester {
        caller: bff.clone(),
        principal: Some(Principal("bob".into())),
    };
    let desktop = Caller(":1.42".into());
    let core_in = |allow: bool, group: FakeGroup, log: FakeActionLog| {
        Manager::new(
            FakeService::new(),
            FakeAuthority::new(allow).quietly(true).bff(&bff),
            FakeResolver::lan(),
        )
        .with_group(group)
        .with_passwords(FakePasswords::new())
        .with_log(log)
    };
    let core = |allow: bool, log: FakeActionLog| core_in(allow, FakeGroup::new(), log);
    let text_of = |name: &str| std::fs::read_to_string(sys.join(name)).expect("read");
    let bytes_of = |name: &str| std::fs::read(sys.join(name)).expect("bytes");
    let records =
        |lines: &std::sync::Arc<std::sync::Mutex<Vec<String>>>| lines.lock().unwrap().clone();
    // The outcome of each record, with the quotes the record puts around a
    // kind's name taken off.
    let outcomes = |lines: &[String]| -> Vec<String> {
        lines
            .iter()
            .map(|l| {
                l.rsplit_once("outcome=")
                    .expect("outcome")
                    .1
                    .trim_matches('"')
                    .to_string()
            })
            .collect()
    };
    // The values of a call, as the wire carries them: pairs of strings, in the
    // order the caller sent them (the manager sorts them).
    let vals = |pairs: &[(&str, &str)]| -> Vec<(String, String)> {
        pairs
            .iter()
            .map(|(key, value)| (key.to_string(), value.to_string()))
            .collect()
    };
    let refused = |key: &'static str, values: &[&str]| -> Result<(), ManagerError> {
        Err(ManagerError::Invalid(pai_core::checks::Refusal {
            key,
            values: values.iter().map(|v| v.to_string()).collect(),
        }))
    };
    // The `model` line of a pool text, as it stands: `real_model_files` turned
    // its value into a path of this test's tree.
    let model_line_of = |text: &str| -> String {
        text.lines()
            .find(|line| line.starts_with("model = "))
            .expect("a model line")
            .to_string()
    };
    let listing = || -> Vec<String> {
        let mut names: Vec<String> = std::fs::read_dir(&sys)
            .map(|rd| {
                rd.flatten()
                    .map(|e| e.file_name().to_string_lossy().into_owned())
                    .collect()
            })
            .unwrap_or_default();
        names.sort();
        names
    };
    // Every file of the engine's directory, by name with its bytes: a refusal
    // is pinned by the whole tree standing, not by the two pool files alone.
    let snapshot = || -> Vec<(String, Vec<u8>)> {
        let mut out: Vec<(String, Vec<u8>)> = std::fs::read_dir(&sys)
            .expect("listing")
            .flatten()
            .filter_map(|entry| {
                let name = entry.file_name().to_string_lossy().into_owned();
                std::fs::read(entry.path()).ok().map(|bytes| (name, bytes))
            })
            .collect();
        out.sort();
        out
    };

    // The pool, and the records. `models.ini` is the one pool: it holds
    // `On-Profile`, cloned below, and `Twin`. `instance-profiles.ini` has one
    // profile, `gpu`, with its entry for `On-Profile`; the record runs it. The
    // retired `models.ini.disabled`, holding `Off-Profile`, and the retired
    // `assignments.ini`, with its `X = on` naming the id the first create
    // writes in the other letter case, stand in the tree as an install leaves
    // them: no verb reads them, and the test pins that no verb writes them.
    let pool_on = real_model_files(
        dir.path(),
        "[On-Profile]\n; a note\ntemp = 0.7\nmodel = /models/on.gguf\nload-on-startup = true\n\n[Twin]\nmodel = /models/twin.gguf\n",
    );
    let pool_off = real_model_files(dir.path(), "[Off-Profile]\nmodel = /models/off.gguf\n");
    let seed_assignments = "[default]\nX = on\nkeep = inherit\n";
    const PROFILES: &str = "[gpu]\nServerProfile = default\n\n[gpu/On-Profile]\nctx-size = 4096\n";
    std::fs::write(sys.join(INSTANCE_PROFILES_FILE), PROFILES).expect(INSTANCE_PROFILES_FILE);
    std::fs::write(
        sys.join(INSTANCES_FILE),
        "[default]\nInstanceProfile = gpu\nPort = 8581\n",
    )
    .expect(INSTANCES_FILE);
    std::fs::write(
        sys.join("server@default.ini"),
        "[Server]\nPort = 8080\nHostname = 127.0.0.1\n",
    )
    .expect("server profile");
    std::fs::write(sys.join(ASSIGNMENTS_FILE), seed_assignments).expect(ASSIGNMENTS_FILE);
    std::fs::write(sys.join("models.ini"), &pool_on).expect("models.ini");
    std::fs::write(sys.join(MODELS_DISABLED_FILE), &pool_off).expect(MODELS_DISABLED_FILE);
    let render_of = pai_core::assign::render_profile(&pool_on, PROFILES, "gpu");
    std::fs::write(sys.join("models@default.ini"), &render_of).expect("render");
    let render_of = text_of("models@default.ini");
    assert!(
        render_of.to_lowercase().contains("[on-profile]") && !render_of.contains("[Off-Profile]"),
        "the entry `[gpu/On-Profile]` lists the model, so the render carries it, and the \
         retired switched-off file's profile is in no render: {render_of}"
    );

    // ── A create writes the section at the end of the pool ──────────────────
    // `want` is the whole pool, kept byte for byte as the creates grow it.
    let mut want;
    {
        let log = FakeActionLog::new();
        let lines = log.lines.clone();
        let group = FakeGroup::new();
        let g = group.calls.clone();
        let m = core_in(true, group, log);
        let off_before = bytes_of(MODELS_DISABLED_FILE);
        let keys_before = bytes_of(ASSIGNMENTS_FILE);
        let record_before = bytes_of(INSTANCES_FILE);
        let render_before = bytes_of("models@default.ini");
        let before = listing();
        want = text_of("models.ini");

        assert_eq!(
            m.create_model_profile(&desktop, "x", "", &vals(&[("model", "/m/x.gguf")])),
            Ok(()),
            "a new id with a model file"
        );
        want = format!("{want}[x]\nmodel = /m/x.gguf\n");
        assert_eq!(
            text_of("models.ini"),
            want,
            "the section at the end of the pool, every other byte of it kept, and \
             no blank line added to a text that ends with an ending"
        );
        assert_eq!(
            bytes_of(MODELS_DISABLED_FILE),
            off_before,
            "the retired switched-off file keeps every byte: the create writes the pool, \
             and there is no switch any more"
        );
        assert_eq!(
            bytes_of("models@default.ini"),
            render_before,
            "the render is kept byte for byte: the new profile is listed in no entry, so \
             it renders nowhere"
        );
        assert_eq!(
            bytes_of(ASSIGNMENTS_FILE),
            keys_before,
            "the retired record keeps every byte, the dead `X = on` and all: no verb reads \
             or writes assignments.ini any more"
        );
        assert_eq!(
            bytes_of(INSTANCES_FILE),
            record_before,
            "a create writes no instance record"
        );
        assert_eq!(listing(), before, "no new file, and no temp file left");
        let calls = g.lock().unwrap().clone();
        assert_eq!(
            calls,
            Vec::new(),
            "a create touches no server profile, so it asks the group seam nothing: \
             the pool, the record and the renders are model profile text, whose mode \
             lets every user read them"
        );
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let name = "models.ini";
            assert_eq!(
                std::fs::metadata(sys.join(name))
                    .expect("metadata")
                    .permissions()
                    .mode()
                    & 0o7777,
                0o644,
                "the mode of {name}: the pool, written by the create as a served \
                 write writes it"
            );
        }

        // The Hugging Face pick: the two lines, sorted, and `hf-repo` alone.
        assert_eq!(
            m.create_model_profile(
                &desktop,
                "hf-pair",
                "",
                &vals(&[("hf-file", "m-Q8_0.gguf"), ("hf-repo", "org/model")])
            ),
            Ok(()),
            "the pick of `GET api/hf/files`: a repository and the file inside it"
        );
        want = format!("{want}[hf-pair]\nhf-file = m-Q8_0.gguf\nhf-repo = org/model\n");
        assert_eq!(
            text_of("models.ini"),
            want,
            "the two lines sorted by key, whatever order the call sent them in"
        );
        assert_eq!(
            m.create_model_profile(&desktop, "hf-only", "", &vals(&[("hf-repo", "org/other")])),
            Ok(()),
            "`hf-repo` alone names the model the node downloads: the `model` rule accepts it"
        );
        want = format!("{want}[hf-only]\nhf-repo = org/other\n");
        assert_eq!(text_of("models.ini"), want);
        assert_eq!(
            m.create_model_profile(
                &desktop,
                "nofile",
                "",
                &vals(&[("model", &dir.path().join("absent.gguf").to_string_lossy())])
            ),
            Ok(()),
            "a model file the node has not is accepted: the profile is listed in no \
             entry, so it renders nowhere and the file is judged when an entry lists it"
        );
        want = format!(
            "{want}[nofile]\nmodel = {}\n",
            dir.path().join("absent.gguf").to_string_lossy()
        );
        assert_eq!(text_of("models.ini"), want);

        assert_eq!(
            records(&lines),
            vec![
                r#"action="create_model_profile" target="x" caller=":1.42" principal=- outcome=ok"#
                    .to_string(),
                r#"action="create_model_profile" target="hf-pair" caller=":1.42" principal=- outcome=ok"#
                    .to_string(),
                r#"action="create_model_profile" target="hf-only" caller=":1.42" principal=- outcome=ok"#
                    .to_string(),
                r#"action="create_model_profile" target="nofile" caller=":1.42" principal=- outcome=ok"#
                    .to_string(),
            ],
            "one record per call, the new profile as its target"
        );
    }

    // ── The ending style of the file, kept ────────────────────────────────
    {
        let m = core(true, FakeActionLog::new());

        std::fs::write(sys.join("models.ini"), "[Hand]\r\nmodel = /m/hand.gguf\r\n")
            .expect("a CRLF pool");
        assert_eq!(
            m.create_model_profile(&desktop, "crlf", "", &vals(&[("model", "/m/crlf.gguf")])),
            Ok(()),
            "a create into a pool a hand edit wrote with CRLF"
        );
        assert_eq!(
            text_of("models.ini"),
            "[Hand]\r\nmodel = /m/hand.gguf\r\n[crlf]\r\nmodel = /m/crlf.gguf\r\n",
            "the new section takes the file's ending, and the bytes before it stand"
        );

        std::fs::write(sys.join("models.ini"), "[Hand]\nmodel = /m/hand.gguf")
            .expect("a pool with no ending on its last line");
        assert_eq!(
            m.create_model_profile(&desktop, "noeol", "", &vals(&[("model", "/m/noeol.gguf")])),
            Ok(()),
            "a create into a text whose last line has no ending"
        );
        assert_eq!(
            text_of("models.ini"),
            "[Hand]\nmodel = /m/hand.gguf\n[noeol]\nmodel = /m/noeol.gguf\n",
            "one ending first: the last line of the file and the new header do not share a line"
        );

        std::fs::remove_file(sys.join("models.ini")).expect("no pool");
        assert_eq!(
            m.create_model_profile(
                &desktop,
                "brand-new",
                "",
                &vals(&[("model", "/m/new.gguf")])
            ),
            Ok(()),
            "a node with no pool at all"
        );
        assert_eq!(
            text_of("models.ini"),
            "[brand-new]\r\nmodel = /m/new.gguf\r\n",
            "the file is created holding the section alone, with the CRLF ending the \
             GUI writes when it has no file to read one from"
        );
    }

    // ── A clone: the source's stay keys, byte for byte, listed nowhere ──────
    {
        let log = FakeActionLog::new();
        let lines = log.lines.clone();
        let m = core(true, log);
        std::fs::write(sys.join("models.ini"), &pool_on).expect("the pool");
        let keys_before = bytes_of(ASSIGNMENTS_FILE);
        let render_before = bytes_of("models@default.ini");

        assert_eq!(
            m.create_model_profile(&desktop, "x-clone", "On-Profile", &vals(&[])),
            Ok(()),
            "a clone of a profile the entry lists"
        );
        assert_eq!(
            text_of("models.ini"),
            // The source's stay keys under the new header, its trailing blank
            // lines left out: the separator before the next section is layout,
            // not the source's lines (MP1, ARCHITECTURE.md done-when).
            format!(
                "{pool_on}[x-clone]\n; a note\ntemp = 0.7\n{}\n",
                model_line_of(&pool_on)
            ),
            "the source's stay keys as they stand, under the new header: the comment and \
             the key order, with no blank line at the end; `load-on-startup` is a key of \
             the entry now, so the clone carries it not"
        );
        assert_eq!(
            bytes_of(ASSIGNMENTS_FILE),
            keys_before,
            "the retired record keeps every byte: a clone assigns nothing to nothing"
        );
        assert_eq!(
            bytes_of("models@default.ini"),
            render_before,
            "the render is kept: the clone is listed in no entry, so it renders nowhere"
        );

        // The source is looked up as the render does: any letter case.
        assert_eq!(
            m.create_model_profile(&desktop, "off-clone", "OFF-PROFILE", &vals(&[])),
            refused("no-model-profile-named", &["OFF-PROFILE"]),
            "the retired switched-off file is no source: its profiles are in the pool, \
             or they are gone"
        );
        assert_eq!(
            m.create_model_profile(&desktop, "off-clone", "on-profile", &vals(&[])),
            Ok(()),
            "a source named in the other letter case"
        );
        assert!(
            text_of("models.ini").contains(&format!(
                "[off-clone]\n; a note\ntemp = 0.7\n{}\n",
                model_line_of(&pool_on)
            )),
            "the source's own lines, copied: {}",
            text_of("models.ini")
        );
        assert_eq!(
            records(&lines),
            vec![
                r#"action="create_model_profile" target="x-clone" caller=":1.42" principal=- outcome=ok"#
                    .to_string(),
                // A keyed refusal is `ManagerError::Invalid`: the record carries the
                // error's own kind, quoted. `refused` is the bare `NotAuthorized`'s.
                r#"action="create_model_profile" target="off-clone" caller=":1.42" principal=- outcome="Invalid""#
                    .to_string(),
                r#"action="create_model_profile" target="off-clone" caller=":1.42" principal=- outcome=ok"#
                    .to_string(),
            ],
            "one record per call: the refused clone, a keyed refusal, records the \
             quoted kind, as every keyed refusal of every verb does"
        );
    }

    // ── A mid-file source and a last-section source clone the same ─────────
    {
        let log = FakeActionLog::new();
        let m = core(true, log);
        let same = real_model_files(
            dir.path(),
            "[mid-src]\n; a note\ntemp = 0.3\nmodel = /models/clone-same.gguf\n\n[last-src]\n; a note\ntemp = 0.3\nmodel = /models/clone-same.gguf\n",
        );
        std::fs::write(sys.join("models.ini"), &same).expect("the pool");
        assert_eq!(
            m.create_model_profile(&desktop, "from-mid", "mid-src", &vals(&[])),
            Ok(()),
            "a clone of a source sitting in the middle of the file"
        );
        assert_eq!(
            m.create_model_profile(&desktop, "from-last", "last-src", &vals(&[])),
            Ok(()),
            "a clone of a source that is the file's last section"
        );
        let off = text_of("models.ini");
        let sec = |id: &str| -> String {
            pai_core::ini::text::section_text(&off, id).expect("the clone is there")
        };
        assert_eq!(
            sec("from-mid")["[from-mid]\n".len()..],
            sec("from-last")["[from-last]\n".len()..],
            "the source's trailing blank lines are layout, not its lines: a mid-file \
             source and a last-section source of the same lines clone the same bytes: {off}"
        );
        assert_eq!(
            off,
            format!(
                "{same}[from-mid]\n; a note\ntemp = 0.3\n{model}\n[from-last]\n; a note\ntemp = 0.3\n{model}\n",
                model = model_line_of(&same)
            ),
            "the whole file the two clones leave: each ends with one line ending, and \
             the call's section follows the last line with no blank between"
        );
    }

    // ── Every refusal, and the whole tree standing behind it ───────────────
    {
        let log = FakeActionLog::new();
        let lines = log.lines.clone();
        let m = core(true, log);
        std::fs::write(sys.join("models.ini"), &pool_on).expect("the pool");
        std::fs::write(sys.join(MODELS_DISABLED_FILE), &pool_off).expect(MODELS_DISABLED_FILE);
        std::fs::write(sys.join(ASSIGNMENTS_FILE), seed_assignments).expect(ASSIGNMENTS_FILE);
        std::fs::write(
            sys.join(INSTANCES_FILE),
            "[default]\nInstanceProfile = gpu\nPort = 8581\n",
        )
        .expect(INSTANCES_FILE);
        let before = snapshot();

        for (id, from, values, key) in [
            // A name the pool holds, in any letter case: a create never overwrites.
            (
                "twin",
                "",
                vec![("model", "/m/x.gguf")],
                "model-profile-name-taken",
            ),
            (
                "TWIN",
                "",
                vec![("model", "/m/x.gguf")],
                "model-profile-name-taken",
            ),
            (
                "on-profile",
                "",
                vec![("model", "/m/x.gguf")],
                "model-profile-name-taken",
            ),
            // The id rule.
            (
                "*",
                "",
                vec![("model", "/m/x.gguf")],
                "invalid-model-profile-id",
            ),
            (
                "a b",
                "",
                vec![("model", "/m/x.gguf")],
                "invalid-model-profile-id",
            ),
            (
                "",
                "",
                vec![("model", "/m/x.gguf")],
                "invalid-model-profile-id",
            ),
            (
                "../x",
                "",
                vec![("model", "/m/x.gguf")],
                "invalid-model-profile-id",
            ),
            (
                "a[b]",
                "",
                vec![("model", "/m/x.gguf")],
                "invalid-model-profile-id",
            ),
            (
                "a;b",
                "",
                vec![("model", "/m/x.gguf")],
                "invalid-model-profile-id",
            ),
            // The source: from the id, from the pick, from both, from neither.
            (
                "dup",
                "On-Profile",
                vec![("model", "/m/x.gguf")],
                "model-profile-bad-source",
            ),
            ("dup", "", vec![], "model-profile-bad-source"),
            (
                "dup",
                "",
                vec![("hf-file", "m.gguf")],
                "model-profile-bad-source",
            ),
            (
                "dup",
                "",
                vec![("model", "/m/a.gguf"), ("hf-repo", "org/model")],
                "model-profile-bad-source",
            ),
            (
                "dup",
                "",
                vec![("n-gpu-layers", "99")],
                "model-profile-bad-source",
            ),
            (
                "dup",
                "",
                vec![("model", "/m/a.gguf"), ("model", "/m/b.gguf")],
                "model-profile-bad-source",
            ),
            ("nosuch-clone", "nosuch", vec![], "no-model-profile-named"),
            ("star-clone", "*", vec![], "no-model-profile-named"),
            // The values: nothing to write, a control character, a space at an end.
            ("dup", "", vec![("model", "")], "model-profile-bad-text"),
            ("dup", "", vec![("model", "  ")], "model-profile-bad-text"),
            ("dup", "", vec![("model", "\t")], "model-profile-bad-text"),
            (
                "dup",
                "",
                vec![("model", "/m/a.gguf\n[evil]\ntemp = 1")],
                "model-profile-bad-text",
            ),
            (
                "dup",
                "",
                vec![("model", "/m/a.gguf\rx")],
                "model-profile-bad-text",
            ),
            (
                "dup",
                "",
                vec![("model", "/m/a\0b")],
                "model-profile-bad-text",
            ),
            (
                "dup",
                "",
                vec![("model", "/m/a\u{7f}b")],
                "model-profile-bad-text",
            ),
            (
                "dup",
                "",
                vec![("model", "/m/a.gguf\u{2003}")],
                "model-profile-bad-text",
            ),
            (
                "dup",
                "",
                vec![("model", "\u{2003}/m/a.gguf")],
                "model-profile-bad-text",
            ),
            (
                "dup",
                "",
                vec![("hf-repo", "org/model\u{a0}")],
                "model-profile-bad-text",
            ),
            (
                "dup",
                "",
                vec![("hf-repo", "org/model"), ("hf-file", "m.gguf\nx")],
                "model-profile-bad-text",
            ),
            // A marker: the value would be cut as an inline comment on reload.
            ("dup", "", vec![("model", "/m/a;b")], "comment-markers"),
            ("dup", "", vec![("model", "/m/a#b")], "comment-markers"),
            (
                "dup",
                "",
                vec![("hf-repo", "org/model#q8")],
                "comment-markers",
            ),
            ("dup", "", vec![("hf-file", "m;m.gguf")], "comment-markers"),
            // The keys: the call writes every key as a line of the file, so a
            // key that is no key of the INI forges a line, a comment marker or
            // a whole section the name, id and source checks never saw.
            (
                "dup",
                "",
                vec![("a\n[x]\nmodel", "1")],
                "model-profile-bad-text",
            ),
            ("dup", "", vec![("a=b", "1")], "model-profile-bad-text"),
            ("dup", "", vec![("a;b", "1")], "model-profile-bad-text"),
            ("dup", "", vec![("a#b", "1")], "model-profile-bad-text"),
            ("dup", "", vec![("[x]", "1")], "model-profile-bad-text"),
            ("dup", "", vec![("a b", "1")], "model-profile-bad-text"),
            ("dup", "", vec![(" a", "1")], "model-profile-bad-text"),
            ("dup", "", vec![("", "1")], "model-profile-bad-text"),
            // The same keys on a clone: the shape lets any key through as an
            // override, so today the line is written raw beside the source's.
            (
                "kc-forge",
                "On-Profile",
                vec![("a\n[x]\nmodel", "1")],
                "model-profile-bad-text",
            ),
            (
                "kc-eq",
                "On-Profile",
                vec![("a=b", "1")],
                "model-profile-bad-text",
            ),
            (
                "kc-semi",
                "On-Profile",
                vec![("a;b", "1")],
                "model-profile-bad-text",
            ),
            (
                "kc-hash",
                "On-Profile",
                vec![("a#b", "1")],
                "model-profile-bad-text",
            ),
            (
                "kc-bracket",
                "On-Profile",
                vec![("[x]", "1")],
                "model-profile-bad-text",
            ),
            (
                "kc-space",
                "On-Profile",
                vec![("a b", "1")],
                "model-profile-bad-text",
            ),
            (
                "kc-lead",
                "On-Profile",
                vec![(" a", "1")],
                "model-profile-bad-text",
            ),
            (
                "kc-empty",
                "On-Profile",
                vec![("", "1")],
                "model-profile-bad-text",
            ),
            // The order: the shape and the values are judged before the id, and
            // every one of them before the files are read.
            (
                "*",
                "*",
                vec![("model", "/m/x.gguf")],
                "model-profile-bad-source",
            ),
            (
                "*",
                "",
                vec![("model", "/m/a\nb")],
                "model-profile-bad-text",
            ),
            (
                "twin",
                "",
                vec![("model", "/m/a\nb")],
                "model-profile-bad-text",
            ),
        ] {
            let wanted: Vec<&str> = if key == "no-model-profile-named" {
                vec![from]
            } else if key == "model-profile-bad-text" || key == "comment-markers" {
                // Both name the key the bad value sits under, as the pipe and
                // D-Bus tests of the same rule expect it.
                vec![values
                    .iter()
                    .find(|(_, v)| {
                        v.is_empty()
                            || v.trim() != *v
                            || v.chars().any(|c| c.is_control())
                            || v.contains(';')
                            || v.contains('#')
                    })
                    .map(|(k, _)| *k)
                    .unwrap_or(values[0].0)]
            } else {
                vec![id]
            };
            assert_eq!(
                m.create_model_profile(&desktop, id, from, &vals(&values)),
                refused(key, &wanted),
                "create of {id:?} from {from:?} with {values:?}"
            );
            assert_eq!(
                snapshot(),
                before,
                "a create refused at {id:?} {from:?} {values:?} changed no byte, anywhere"
            );
        }
        assert_eq!(
            text_of(ASSIGNMENTS_FILE),
            seed_assignments,
            "the retired record keeps every byte, the dead `X = on` and all: a create \
             refused at the shape, the id, the values, the taken name or the source writes \
             nothing, and no create writes this file at all"
        );
        assert_eq!(
            listing(),
            before
                .iter()
                .map(|(name, _)| name.clone())
                .collect::<Vec<_>>(),
            "and no file was created"
        );
        let got = records(&lines);
        assert_eq!(
            outcomes(&got),
            vec!["Invalid"; 51],
            "one record per refused call, every one of them the check's refusal"
        );
        assert!(
            got.iter()
                .all(|l| l.starts_with(r#"action="create_model_profile" target="#)),
            "every record is the create's own: {got:?}"
        );
    }

    // ── The retired record stands when its own key is the id ────────────────
    // (IP3b review D1) The create drops a dead key of the retired
    // `assignments.ini` before the pool's content checks, so a create whose id
    // IS one of that file's keys rewrites a file no verb reads any more. The id
    // here is a key of the record in its own letters (the cleanup compares the
    // letters, so the seeded `X = on` of the blocks above is never dropped).
    {
        let log = FakeActionLog::new();
        let m = core(true, log);
        let seed = "[default]\nDead-Key = on\nkeep = inherit\n";
        std::fs::write(sys.join("models.ini"), &pool_on).expect("the pool");
        std::fs::write(sys.join(MODELS_DISABLED_FILE), &pool_off).expect(MODELS_DISABLED_FILE);
        std::fs::write(sys.join(ASSIGNMENTS_FILE), seed).expect(ASSIGNMENTS_FILE);
        let keys_before = bytes_of(ASSIGNMENTS_FILE);

        assert_eq!(
            m.create_model_profile(
                &desktop,
                "Dead-Key",
                "",
                &vals(&[("model", "/m/dead.gguf")])
            ),
            Ok(()),
            "an id the pool holds not, spelled as a retired key"
        );
        assert_eq!(
            bytes_of(ASSIGNMENTS_FILE),
            keys_before,
            "the accepted create writes the pool and no byte of the retired record: its \
             `Dead-Key = on` is a key nothing reads, so dropping it is a write of a file no \
             verb reads: {}",
            text_of(ASSIGNMENTS_FILE)
        );
        assert!(
            text_of("models.ini").contains("[Dead-Key]\nmodel = /m/dead.gguf\n"),
            "the pool write itself lands: {}",
            text_of("models.ini")
        );
    }

    // ── A create the content checks refuse leaves the retired record alone ───
    // (IP3b review D1) The key drop of the block above runs before
    // `check_contents`, so a create that drops a key and is then refused by the
    // content checks leaves a changed `assignments.ini` behind. The refusal here
    // is the pool rule that an id stays unique across the pool and the retired
    // switched-off file: the create's own checks read the pool alone, so they
    // pass it and the content check is the one that refuses.
    {
        let log = FakeActionLog::new();
        let m = core(true, log);
        std::fs::write(sys.join("models.ini"), &pool_on).expect("the pool");
        std::fs::write(
            sys.join(MODELS_DISABLED_FILE),
            real_model_files(dir.path(), "[Dead-Key]\nmodel = /models/dead.gguf\n"),
        )
        .expect(MODELS_DISABLED_FILE);
        std::fs::write(
            sys.join(ASSIGNMENTS_FILE),
            "[default]\nDead-Key = on\nkeep = inherit\n",
        )
        .expect(ASSIGNMENTS_FILE);
        let keys_before = bytes_of(ASSIGNMENTS_FILE);
        let pool_before = bytes_of("models.ini");
        let before = snapshot();

        assert_eq!(
            m.create_model_profile(
                &desktop,
                "Dead-Key",
                "",
                &vals(&[("model", "/m/dead.gguf")])
            ),
            refused("model-profile-name-taken", &["Dead-Key"]),
            "an id the retired switched-off file holds is refused by the content check of \
             the pool write, not by a check the create runs itself"
        );
        assert_eq!(
            bytes_of(ASSIGNMENTS_FILE),
            keys_before,
            "a create the content check refuses leaves every byte of the retired record: \
             the key drop belongs to the pool write, and this create wrote no pool: {}",
            text_of(ASSIGNMENTS_FILE)
        );
        assert_eq!(
            bytes_of("models.ini"),
            pool_before,
            "and the pool keeps every byte, the refused section absent"
        );
        assert_eq!(
            snapshot(),
            before,
            "a create refused by the content check changed no byte, anywhere"
        );

        // The pool and the retired files back as the blocks below read them.
        std::fs::write(sys.join(MODELS_DISABLED_FILE), &pool_off).expect(MODELS_DISABLED_FILE);
        std::fs::write(sys.join(ASSIGNMENTS_FILE), seed_assignments).expect(ASSIGNMENTS_FILE);
    }

    // ── A create renders nothing, even where a render cannot land ───────────
    // IP3b: the model list is the instance profile's entries, and a create joins
    // none, so it writes the pool and renders nothing. The id is named by an
    // entry here, and the render's place is a directory an atomic write cannot
    // rename onto: the create lands, and the render is untouched. A mode change
    // is no fixture, the service runs as root.
    {
        let log = FakeActionLog::new();
        let lines = log.lines.clone();
        let m = core(true, log);
        // The retired record, as an install leaves it: no verb reads it.
        std::fs::write(sys.join(ASSIGNMENTS_FILE), seed_assignments).expect(ASSIGNMENTS_FILE);
        std::fs::write(sys.join("models.ini"), "[Hand]\nmodel = /m/hand.gguf\n").expect("the pool");
        std::fs::write(
            sys.join(INSTANCE_PROFILES_FILE),
            "[gpu]\nServerProfile = default\n\n[gpu/y]\n",
        )
        .expect(INSTANCE_PROFILES_FILE);
        // The model file the entry's render would judge, real under this tree.
        let y = real_model_files(dir.path(), "[y]\nmodel = /models/y.gguf\n");
        let y_model = y
            .lines()
            .find(|line| line.starts_with("model = "))
            .expect("a model line")["model = ".len()..]
            .to_string();
        let render = sys.join("models@default.ini");
        std::fs::remove_file(&render).expect("the render aside");
        std::fs::create_dir(&render).expect("a directory in the render's place");

        assert_eq!(
            m.create_model_profile(&desktop, "y", "", &vals(&[("model", y_model.as_str())])),
            Ok(()),
            "the create renders nothing, so a render that cannot be written stops it not"
        );
        assert!(
            text_of("models.ini").contains(&format!("[y]\nmodel = {y_model}\n")),
            "the pool write landed: {}",
            text_of("models.ini")
        );
        assert!(
            render.is_dir()
                && std::fs::read_dir(&render)
                    .expect("the render's place")
                    .count()
                    == 0,
            "no render was written, and no temp file was left in its place: the new \
             profile joins no entry, so it is in no instance's render"
        );
        assert_eq!(
            bytes_of(ASSIGNMENTS_FILE),
            seed_assignments.as_bytes(),
            "the retired record keeps every byte, the dead `X = on` and all: its \
             cleanup was a rule of the assignment record, retired with the file: {}",
            text_of(ASSIGNMENTS_FILE)
        );
        assert_eq!(
            records(&lines),
            vec![
                r#"action="create_model_profile" target="y" caller=":1.42" principal=- outcome=ok"#
                    .to_string()
            ],
            "the create is recorded, as every guarded call is"
        );

        std::fs::remove_dir(&render).expect("the directory away");
        std::fs::write(&render, "[*]\n").expect("the render back");
        std::fs::write(sys.join(INSTANCE_PROFILES_FILE), PROFILES).expect(INSTANCE_PROFILES_FILE);
    }

    // ── The gate: configure, and the `As` form for a member of `pai` ──────
    {
        std::fs::write(sys.join("models.ini"), &pool_on).expect("the on pool");
        std::fs::write(sys.join("models.ini"), &pool_off).expect("the off pool");
        std::fs::write(sys.join(ASSIGNMENTS_FILE), seed_assignments).expect(ASSIGNMENTS_FILE);

        let log = FakeActionLog::new();
        let lines = log.lines.clone();
        let m = core(false, log);
        let before = snapshot();
        assert_eq!(
            m.create_model_profile(&desktop, "gate-1", "", &vals(&[("model", "/m/a.gguf")])),
            Err(ManagerError::NotAuthorized),
            "a caller the authority refuses for configure"
        );
        assert_eq!(
            snapshot(),
            before,
            "a refused caller wrote nothing, anywhere"
        );
        assert_eq!(
            records(&lines),
            vec![
                r#"action="create_model_profile" target="gate-1" caller=":1.42" principal=- outcome=refused"#
                    .to_string()
            ],
            "the guard's refusal is recorded"
        );
    }
    {
        std::fs::write(sys.join("models.ini"), &pool_on).expect("the on pool");
        std::fs::write(sys.join("models.ini"), &pool_off).expect("the off pool");
        let log = FakeActionLog::new();
        let lines = log.lines.clone();
        let m = core(true, log);
        let vouching = Requester {
            caller: desktop.clone(),
            principal: Some(Principal("alice".into())),
        };
        assert_eq!(
            m.create_model_profile(&member, "alice-mp", "", &vals(&[("model", "/m/a.gguf")])),
            Ok(()),
            "the bff's account, for a member of `pai`"
        );
        assert!(
            text_of("models.ini").contains("[alice-mp]\nmodel = /m/a.gguf\n"),
            "and the profile is there"
        );
        assert_eq!(
            m.create_model_profile(&outsider, "bob-mp", "", &vals(&[("model", "/m/a.gguf")])),
            Err(ManagerError::NotAuthorized),
            "the bff's account, for a caller outside `pai`"
        );
        assert_eq!(
            m.create_model_profile(&vouching, "x", "", &vals(&[("model", "/m/a.gguf")])),
            Err(ManagerError::NotAuthorized),
            "a caller that is not the bff vouches for nobody"
        );
        assert!(
            !text_of("models.ini").contains("[bob-mp]"),
            "the refused `As` create wrote nothing"
        );
        assert!(
            !text_of("models.ini").contains("[x]"),
            "nor did the one that is not the bff's"
        );
        let got = records(&lines);
        assert_eq!(
            got.iter()
                .map(|l| l.rsplit_once("outcome=").expect("outcome").1.to_string())
                .collect::<Vec<_>>(),
            vec!["ok", "refused", "refused"],
            "the accepted call ok, every `As` refusal recorded as refused"
        );
        assert!(
            got[0].starts_with(r#"action="create_model_profile" target="alice-mp" caller=":1.9""#),
            "the record names the bff's account, as every `As` record does: {got:?}"
        );
    }
}

/// Deleting a model profile (`DeleteModelProfile`, ARCHITECTURE.md "Create,
/// clone and delete model profiles from the web", slice MP1, as amended by
/// "Instance profiles" and its key split): the profile's own section, alone, out
/// of `models.ini`, the one pool, every other byte of the tree kept, the retired
/// `models.ini.disabled` and `assignments.ini` included. The checks, under the
/// lock, in this order: the id rule fails, or the id is `*`, and it is
/// `no-model-profile-named [profile]`, never `invalid-model-profile-id`, and
/// before any file is read; a profile the pool has no section for is the same
/// refusal, a key of the retired record or a section of the retired
/// switched-off file no proof it exists; a profile an instance profile's entry
/// `[<profile>/<model>]` lists, in any letter case, is **`model-profile-assigned
/// [profile, "<a>, <b>"]`** with the ids of every such instance profile, in the
/// pool's own order, joined with `, `: the model sits in those profiles'
/// renders, and a delete would leave an entry naming nothing. An instance
/// profile file that cannot be read is `Io`, never the "unused" of an empty
/// list. `* = inherit` in the retired record serves nothing of itself, so it
/// does not block. The write is `check_and_write(Models, …)`: every section of
/// the id, repeated in any letter case, is gone with `ini::text::delete_section`
/// and nothing else moves, the renders of the instances running a profile that
/// lists the model excepted. Guarded as a write of a served file is:
/// `configure` first, the `As` form answered only to the bff's account for a
/// current member of `pai`, one record of the action log per call.
#[cfg(test)]
pub(crate) fn manager_model_profile_delete_e2e() {
    use fakes::{
        FakeActionLog, FakeAuthority, FakeGroup, FakePasswords, FakeResolver, FakeService,
    };
    use pai_core::ini::text::delete_section;

    let dir = tempfile::tempdir().expect("tempdir");
    crate::paths::redirect_for_tests(dir.path());
    let sys = crate::paths::engine_config_dir();
    std::fs::create_dir_all(&sys).expect("engine config dir");

    let bff = Caller(":1.9".into());
    let member = Requester {
        caller: bff.clone(),
        principal: Some(Principal("alice".into())),
    };
    let outsider = Requester {
        caller: bff.clone(),
        principal: Some(Principal("bob".into())),
    };
    let desktop = Caller(":1.42".into());
    let core_in = |allow: bool, group: FakeGroup, log: FakeActionLog| {
        Manager::new(
            FakeService::new(),
            FakeAuthority::new(allow).quietly(true).bff(&bff),
            FakeResolver::lan(),
        )
        .with_group(group)
        .with_passwords(FakePasswords::new())
        .with_log(log)
    };
    let core = |allow: bool, log: FakeActionLog| core_in(allow, FakeGroup::new(), log);
    let text_of = |name: &str| std::fs::read_to_string(sys.join(name)).expect("read");
    let bytes_of = |name: &str| std::fs::read(sys.join(name)).expect("bytes");
    let records =
        |lines: &std::sync::Arc<std::sync::Mutex<Vec<String>>>| lines.lock().unwrap().clone();
    // The outcome of each record, with the quotes the record puts around a
    // kind's name taken off.
    let outcomes = |lines: &[String]| -> Vec<String> {
        lines
            .iter()
            .map(|l| {
                l.rsplit_once("outcome=")
                    .expect("outcome")
                    .1
                    .trim_matches('"')
                    .to_string()
            })
            .collect()
    };
    let section =
        |text: &str, id: &str| pai_core::ini::text::section_text(text, id).unwrap_or_default();
    let refused = |key: &'static str, values: &[&str]| -> Result<(), ManagerError> {
        Err(ManagerError::Invalid(pai_core::checks::Refusal {
            key,
            values: values.iter().map(|v| v.to_string()).collect(),
        }))
    };
    let listing = || -> Vec<String> {
        let mut names: Vec<String> = std::fs::read_dir(&sys)
            .map(|rd| {
                rd.flatten()
                    .map(|e| e.file_name().to_string_lossy().into_owned())
                    .collect()
            })
            .unwrap_or_default();
        names.sort();
        names
    };
    let snapshot = || -> Vec<(String, Vec<u8>)> {
        let mut out: Vec<(String, Vec<u8>)> = std::fs::read_dir(&sys)
            .expect("listing")
            .flatten()
            .filter_map(|entry| {
                let name = entry.file_name().to_string_lossy().into_owned();
                std::fs::read(entry.path()).ok().map(|bytes| (name, bytes))
            })
            .collect();
        out.sort();
        out
    };
    // Every render, by name with its bytes.
    let renders = || -> Vec<(String, Vec<u8>)> {
        listing()
            .into_iter()
            .filter(|name| name.starts_with("models@"))
            .map(|name| (name.clone(), bytes_of(&name)))
            .collect()
    };

    // The pool, the instance profiles and the record. `default` and `lan-2` run
    // `gpu`, `lan-3` runs `plain`, which lists no model. The one pool holds
    // `On-One`, `Twin`, `off-one`, `two-places`, `Repeated` twice (the twin an
    // interrupted migration leaves) and `Star-Served`; the entry `[gpu/On-One]`
    // and the entry `[gpu/TWO-PLACES]` list two of them, in the other case. The
    // retired `models.ini.disabled` holds `Retired` and the retired
    // `assignments.ini` names `repeated` and `star-served`: neither file is read,
    // so neither blocks a delete, and the test pins that neither is written.
    const PROFILE_OF: [(&str, &str); 3] =
        [("default", "gpu"), ("lan-2", "gpu"), ("lan-3", "plain")];
    const RECORD: &str = "[default]\nInstanceProfile = gpu\nPort = 8581\n\n[lan-2]\nInstanceProfile = gpu\nPort = 8582\n\n[lan-3]\nInstanceProfile = plain\nPort = 8583\n";
    const PROFILES: &str = "[gpu]\nServerProfile = gpu\n\n[gpu/On-One]\nctx-size = 4096\n\n[gpu/TWO-PLACES]\n\n[plain]\nServerProfile = plain\n";
    const RETIRED_OFF: &str = "[Retired]\nmodel = /models/retired.gguf\n";
    const RETIRED_KEYS: &str = "[default]\nrepeated = inherit\nstar-served = on\n";
    let pool = real_model_files(
        dir.path(),
        "[On-One]\nmodel = /models/on1.gguf\n\n[Twin]\nmodel = /models/twin.gguf\n\n[off-one]\nmodel = /models/off1.gguf\n\n[two-places]\n; a note\ntemp = 0.3\nmodel = /models/two.gguf\n\n[Repeated]\nmodel = /models/rep1.gguf\n\n[repeated]\nmodel = /models/rep2.gguf\n\n[Star-Served]\nmodel = /models/star.gguf\n",
    );

    // The tree as a start leaves it: the pool, the profiles, the record, the
    // retired files, and the pair of every instance as its own instance profile
    // gives it. A fixture that seeds the pool by hand writes its own render, the
    // same text the render builds.
    let seed = |pool: &str| {
        std::fs::write(sys.join("models.ini"), pool).expect("models.ini");
        std::fs::write(sys.join(INSTANCE_PROFILES_FILE), PROFILES).expect(INSTANCE_PROFILES_FILE);
        std::fs::write(sys.join(INSTANCES_FILE), RECORD).expect(INSTANCES_FILE);
        std::fs::write(sys.join(MODELS_DISABLED_FILE), RETIRED_OFF).expect(MODELS_DISABLED_FILE);
        std::fs::write(sys.join(ASSIGNMENTS_FILE), RETIRED_KEYS).expect(ASSIGNMENTS_FILE);
        for (id, profile) in PROFILE_OF {
            std::fs::write(
                sys.join(format!("server@{profile}.ini")),
                "[Server]\nDevices = CUDA0\n",
            )
            .expect("server profile");
            std::fs::write(
                sys.join(format!("models@{id}.ini")),
                pai_core::assign::render_profile(pool, PROFILES, profile),
            )
            .expect("render");
            // The launch half of the pair (IP3c): a start lays both halves, so
            // the tree this seed leaves has them too, written as the pass writes
            // them.
            std::fs::write(
                sys.join(format!("launch@{id}.ini")),
                launch_render(id).expect("launch render"),
            )
            .expect("launch render");
        }
    };

    // ── Every refusal, and the whole tree standing behind it ───────────────
    {
        seed(&pool);
        let log = FakeActionLog::new();
        let lines = log.lines.clone();
        let group = FakeGroup::new();
        let g = group.calls.clone();
        let m = core_in(true, group, log);
        let before = snapshot();

        for (id, key, values) in [
            // Listed by an entry of the profile two instances run: the entry is
            // judged before the pool is read, and the other letter case is the
            // same entry.
            ("on-one", "model-profile-assigned", &["on-one", "gpu"][..]),
            ("ON-ONE", "model-profile-assigned", &["ON-ONE", "gpu"][..]),
            (
                "two-places",
                "model-profile-assigned",
                &["two-places", "gpu"][..],
            ),
            // Not in the pool at all: a key of the retired record and a section
            // of the retired switched-off file are no proof it exists.
            ("nope", "no-model-profile-named", &["nope"][..]),
            ("On-Three", "no-model-profile-named", &["On-Three"][..]),
            ("retired", "no-model-profile-named", &["retired"][..]),
            // No id of its own: the id rule and `*` answer the pool's refusal,
            // not `invalid-model-profile-id`, and no path is built.
            ("*", "no-model-profile-named", &["*"][..]),
            ("a b", "no-model-profile-named", &["a b"][..]),
            ("", "no-model-profile-named", &[""][..]),
            ("../x", "no-model-profile-named", &["../x"][..]),
            ("a[b]", "no-model-profile-named", &["a[b]"][..]),
        ] {
            assert_eq!(
                m.delete_model_profile(&desktop, id),
                refused(key, values),
                "delete of {id:?}"
            );
            assert_eq!(
                snapshot(),
                before,
                "a delete refused at {id:?} changed no byte, anywhere"
            );
        }
        assert_eq!(
            g.lock().unwrap().clone(),
            Vec::new(),
            "a refused delete asks the group seam nothing"
        );
        let got = records(&lines);
        assert_eq!(
            outcomes(&got),
            vec!["Invalid"; 11],
            "one record per refused call, every one of them the check's refusal"
        );
        assert!(
            got.iter()
                .all(|l| l.starts_with(r#"action="delete_model_profile" target="#)),
            "every record is the delete's own: {got:?}"
        );
    }

    // ── The instance profiles cannot be read: `Io`, never "unused" ──────────
    {
        seed(&pool);
        let log = FakeActionLog::new();
        let lines = log.lines.clone();
        let m = core(true, log);
        let pool_before = bytes_of("models.ini");
        let off_before = bytes_of(MODELS_DISABLED_FILE);
        let kept = renders();
        std::fs::remove_file(sys.join(INSTANCE_PROFILES_FILE)).expect("the profiles out");
        std::fs::create_dir(sys.join(INSTANCE_PROFILES_FILE)).expect("a directory in its place");

        assert_eq!(
            m.delete_model_profile(&desktop, "repeated"),
            Err(ManagerError::Io(std::io::ErrorKind::IsADirectory)),
            "the entries are the ground of the refusal: their file cannot be read, so the \
             delete answers the read's own kind, not the empty list of a node with no profile"
        );
        // The id is judged before any file: `*` answers the pool's refusal even
        // with the entries unreadable.
        assert_eq!(
            m.delete_model_profile(&desktop, "*"),
            refused("no-model-profile-named", &["*"]),
            "the id rule runs before the entries are read"
        );
        assert_eq!(
            bytes_of("models.ini"),
            pool_before,
            "nothing removed from the pool"
        );
        assert_eq!(
            bytes_of(MODELS_DISABLED_FILE),
            off_before,
            "and nothing from the retired switched-off file"
        );
        assert_eq!(renders(), kept, "and no render written");
        assert!(sys.join(INSTANCE_PROFILES_FILE).is_dir(), "the path stands");
        assert_eq!(
            records(&lines),
            vec![
                r#"action="delete_model_profile" target="repeated" caller=":1.42" principal=- outcome="Io""#.to_string(),
                r#"action="delete_model_profile" target="*" caller=":1.42" principal=- outcome="Invalid""#.to_string(),
            ],
            "the read's failure is recorded as every guarded call is"
        );
        std::fs::remove_dir(sys.join(INSTANCE_PROFILES_FILE)).expect("the directory away");
    }

    // ── The pool cannot be read: `Io`, nothing removed ─────────────────────
    {
        seed(&pool);
        let m = core(true, FakeActionLog::new());
        let off_before = bytes_of(MODELS_DISABLED_FILE);
        let kept = renders();
        std::fs::remove_file(sys.join("models.ini")).expect("models.ini out");
        std::fs::create_dir(sys.join("models.ini")).expect("a directory in its place");
        assert_eq!(
            m.delete_model_profile(&desktop, "repeated"),
            Err(ManagerError::Io(std::io::ErrorKind::IsADirectory)),
            "the pool is the file the delete writes: its read's failure is the answer"
        );
        assert_eq!(
            bytes_of(MODELS_DISABLED_FILE),
            off_before,
            "the retired switched-off file is never opened: it keeps every byte"
        );
        assert_eq!(renders(), kept, "and no render written");
        std::fs::remove_dir(sys.join("models.ini")).expect("the directory away");
    }

    // ── An accepted delete: the section alone, out of the pool ─────────────
    // The profiles this block deletes are listed by no entry, so no render of
    // any instance carries them.
    {
        let log = FakeActionLog::new();
        let lines = log.lines.clone();
        let m = core(true, log);
        seed(&pool);
        let off_before = bytes_of(MODELS_DISABLED_FILE);
        let keys_before = bytes_of(ASSIGNMENTS_FILE);
        let record_before = bytes_of(INSTANCES_FILE);
        let profiles_before = bytes_of(INSTANCE_PROFILES_FILE);
        let kept = renders();
        let before = listing();

        // Both copies of `repeated`, spelled in two cases, go: the reader finds a
        // section ignoring the letter case, so one copy left would be the same
        // profile.
        assert_eq!(
            m.delete_model_profile(&desktop, "repeated"),
            Ok(()),
            "a profile no entry lists, in the one pool"
        );
        let want = delete_section(
            &delete_section(&pool, "repeated").expect("one copy"),
            "repeated",
        )
        .expect("the other copy");
        assert_eq!(
            text_of("models.ini"),
            want,
            "every section of the id is gone, repeated copies and all, every other byte \
             of the pool kept"
        );
        assert!(
            !text_of("models.ini").contains("[Repeated]")
                && !text_of("models.ini").contains("[repeated]"),
            "no copy is left"
        );
        assert_eq!(
            section(&text_of("models.ini"), "star-served"),
            section(&pool, "Star-Served"),
            "the neighbour's section is the bytes it was, its case and all"
        );
        assert_eq!(
            bytes_of(MODELS_DISABLED_FILE),
            off_before,
            "the retired switched-off file keeps every byte"
        );
        assert_eq!(
            bytes_of(ASSIGNMENTS_FILE),
            keys_before,
            "the retired record keeps every byte, the key naming this very profile and all"
        );
        assert_eq!(
            bytes_of(INSTANCES_FILE),
            record_before,
            "a delete writes no instance record"
        );
        assert_eq!(
            bytes_of(INSTANCE_PROFILES_FILE),
            profiles_before,
            "and no instance profile: the profile it deletes is listed in none"
        );
        assert_eq!(
            renders(),
            kept,
            "and no render moves: an unlisted profile is in none of them"
        );
        assert_eq!(listing(), before, "no new file, and no temp file left");
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            assert_eq!(
                std::fs::metadata(sys.join("models.ini"))
                    .expect("metadata")
                    .permissions()
                    .mode()
                    & 0o7777,
                0o644,
                "the mode of the pool the delete wrote"
            );
        }
        assert_eq!(
            records(&lines),
            vec![
                r#"action="delete_model_profile" target="repeated" caller=":1.42" principal=- outcome=ok"#
                    .to_string()
            ],
            "one record, the profile as its target"
        );

        // The id in the other case, and a file whose last line has no ending.
        assert_eq!(
            m.delete_model_profile(&desktop, "STAR-SERVED"),
            Ok(()),
            "the id given in the other letter case is the same profile"
        );
        assert_eq!(
            text_of("models.ini"),
            delete_section(&want, "star-served").expect("the section"),
            "the section is found ignoring the case, as the reader finds it"
        );

        // The last section, with no ending of its own: the file keeps standing,
        // empty.
        std::fs::write(sys.join("models.ini"), "[Last]\nmodel = /m/last.gguf")
            .expect("an unterminated pool");
        assert_eq!(
            m.delete_model_profile(&desktop, "last"),
            Ok(()),
            "the only profile of the pool"
        );
        assert_eq!(
            text_of("models.ini"),
            "",
            "the file stands, empty: a delete removes a section, not the file"
        );

        // The CRLF file a hand edit wrote, with its ending kept for what is left.
        let crlf_seed = "[Keep]\r\nmodel = /m/keep.gguf\r\n\r\n[Crlf]\r\nmodel = /m/crlf.gguf\r\n";
        std::fs::write(sys.join("models.ini"), crlf_seed).expect("a CRLF pool");
        assert_eq!(
            m.delete_model_profile(&desktop, "crlf"),
            Ok(()),
            "a delete from a pool a hand edit wrote with CRLF"
        );
        let crlf_want = delete_section(crlf_seed, "crlf").expect("the section");
        assert_eq!(
            text_of("models.ini"),
            crlf_want,
            "the section out, every other byte kept"
        );
        assert!(
            crlf_want.starts_with("[Keep]\r\nmodel = /m/keep.gguf\r\n"),
            "the neighbour's section keeps its CRLF ending: {crlf_want:?}"
        );
        assert!(
            !text_of("models.ini").contains("[Crlf]"),
            "and the profile's section is gone"
        );

        // Gone already: the second delete of the same id is the pool's refusal,
        // and writes nothing.
        let pool_before = bytes_of("models.ini");
        let before = snapshot();
        assert_eq!(
            m.delete_model_profile(&desktop, "crlf"),
            refused("no-model-profile-named", &["crlf"]),
            "a profile that is not in the pool"
        );
        assert_eq!(
            snapshot(),
            before,
            "and a second delete of it changed no byte"
        );
        assert_eq!(bytes_of("models.ini"), pool_before);
    }

    // ── A listed model, once its entry is gone: the pool, and the render ────
    // The entry is what lists the model. Remove it, as taking the model out of
    // the profile does, and the model deletes: its section leaves the pool and
    // every instance running the profile renders again without it, while the
    // render of an instance running a profile that never listed it keeps its
    // bytes.
    {
        let log = FakeActionLog::new();
        let lines = log.lines.clone();
        let m = core(true, log);
        // The tree as a start leaves it, the entries of `gpu` standing and every
        // render written from them: the render carries the model the entry lists,
        // with the POOL's own section header (the entry is `[gpu/On-One]`, the
        // pool's section is `[On-One]`, and the render copies the section's bytes).
        seed(&pool);
        // Then the entry is taken away, as the profile's page does: no render is
        // rewritten by hand, the render that carries the model is the one the
        // entries wrote while the entry stood.
        let stripped = PROFILES.replace("\n\n[gpu/On-One]\nctx-size = 4096", "");
        std::fs::write(sys.join(INSTANCE_PROFILES_FILE), &stripped).expect("the entry gone");
        let gpu_before = bytes_of("models@default.ini");
        let plain_before = bytes_of("models@lan-3.ini");
        assert!(
            gpu_before.starts_with(b"; Generated")
                && text_of("models@default.ini").contains("[On-One]"),
            "the render of an instance running `gpu` carries the model the entry \
             listed, under the pool's header: {}",
            text_of("models@default.ini")
        );
        assert!(
            !text_of("models@lan-3.ini").contains("[On-One]"),
            "and the render of `plain` does not: {}",
            text_of("models@lan-3.ini")
        );

        assert_eq!(
            m.delete_model_profile(&desktop, "on-one"),
            Ok(()),
            "a model no entry lists any more"
        );
        assert_eq!(
            text_of("models.ini"),
            delete_section(&pool, "on-one").expect("the section"),
            "the section alone, out of the pool"
        );
        assert_eq!(
            bytes_of(INSTANCE_PROFILES_FILE),
            stripped.as_bytes(),
            "and no entry written: the entry is the profile's page's to take away"
        );
        let gpu_after = text_of("models@default.ini");
        assert_ne!(bytes_of("models@default.ini"), gpu_before);
        assert!(
            !gpu_after.contains("[On-One]"),
            "the render of an instance running the profile renders again, without the \
             model it listed: {gpu_after}"
        );
        assert!(
            gpu_after.contains("[two-places]"),
            "the model the entry `[gpu/TWO-PLACES]` still lists stays in the render, under \
             the pool's own header: {gpu_after}"
        );
        assert_eq!(
            bytes_of("models@lan-3.ini"),
            plain_before,
            "the render of an instance running a profile that lists nothing keeps its bytes"
        );
        assert_eq!(
            records(&lines),
            vec![
                r#"action="delete_model_profile" target="on-one" caller=":1.42" principal=- outcome=ok"#
                    .to_string()
            ],
            "one record for the accepted delete"
        );
    }

    // ── The gate: configure, and the `As` form for a member of `pai` ───────
    {
        seed(&pool);
        let log = FakeActionLog::new();
        let lines = log.lines.clone();
        let m = core(false, log);
        let before = snapshot();
        assert_eq!(
            m.delete_model_profile(&desktop, "star-served"),
            Err(ManagerError::NotAuthorized),
            "a caller the authority refuses for configure"
        );
        assert_eq!(
            snapshot(),
            before,
            "a refused caller removed nothing, anywhere"
        );
        assert_eq!(
            records(&lines),
            vec![
                r#"action="delete_model_profile" target="star-served" caller=":1.42" principal=- outcome=refused"#
                    .to_string()
            ],
            "the guard's refusal is recorded"
        );
    }
    {
        seed(&pool);
        let log = FakeActionLog::new();
        let lines = log.lines.clone();
        let m = core(true, log);
        let vouching = Requester {
            caller: desktop.clone(),
            principal: Some(Principal("alice".into())),
        };
        assert_eq!(
            m.delete_model_profile(&member, "star-served"),
            Ok(()),
            "the bff's account, for a member of `pai`"
        );
        assert_eq!(
            m.delete_model_profile(&outsider, "twin"),
            Err(ManagerError::NotAuthorized),
            "the bff's account, for a caller outside `pai`"
        );
        assert_eq!(
            m.delete_model_profile(&vouching, "off-one"),
            Err(ManagerError::NotAuthorized),
            "a caller that is not the bff vouches for nobody"
        );
        let pool_after = text_of("models.ini");
        assert!(
            !pool_after.contains("[star-served]") && !pool_after.contains("[Star-Served]"),
            "the accepted delete landed"
        );
        assert!(
            pool_after.contains("twin") && pool_after.contains("off-one"),
            "every refused delete left its profile: {pool_after}"
        );
        let got = records(&lines);
        assert_eq!(
            got.iter()
                .map(|l| l.rsplit_once("outcome=").expect("outcome").1.to_string())
                .collect::<Vec<_>>(),
            vec!["ok", "refused", "refused"],
            "the accepted call ok, every `As` refusal recorded as refused"
        );
        assert!(
            got[0]
                .starts_with(r#"action="delete_model_profile" target="star-served" caller=":1.9""#),
            "the record names the bff's account, as every `As` record does: {got:?}"
        );
    }
}
/// M3 of "Choosing the model file on the web" (`ARCHITECTURE.md` (4) and (7),
/// amended by "One models folder for every engine" (3)): `ModelFiles` /
/// `ModelFilesAs` lists the node's one models folder
/// (`paths::instance_models_dir()` until F1 introduces `paths::models_dir()`
/// and repoints the listing). The listing: regular files ending `.gguf` (any
/// case) by ABSOLUTE path, recursive to depth 6, symlinked directories not
/// followed, dot-folders skipped (`.hf-staging`), at most 5000 entries per
/// folder (then `truncated` true), a split model shown once as its first shard
/// (`-00001-of-0000N.gguf`) with the shard count, and `category` the first
/// subfolder under the folder (the GUI's `model_scan::Category::subdir`
/// layout). A folder that is missing, is not a folder, or cannot be read is an
/// OK answer with `files` empty and the keyed `models-dir-unreadable` `[dir]`,
/// never the call's failure. The gate is the `devices` gate: the caller's own
/// read is `control` asked WITHOUT interaction, the `As` form is the bff's
/// account vouching for a current member of `pai`, and every refusal is
/// `NotAuthorized` with the folder never opened.
#[cfg(test)]
pub(crate) fn manager_model_files_e2e() {
    use fakes::{
        FakeActionLog, FakeAuthority, FakeGroup, FakePasswords, FakeResolver, FakeService,
    };
    use pai_core::client::{ModelFileRow, Reason};

    let dir = tempfile::tempdir().expect("tempdir");
    crate::paths::redirect_for_tests(dir.path());
    let models = crate::paths::instance_models_dir();
    std::fs::create_dir_all(&models).expect("models dir");

    let bff = Caller(":1.9".into());
    let member = Requester {
        caller: bff.clone(),
        principal: Some(Principal("alice".into())),
    };
    let outsider = Requester {
        caller: bff.clone(),
        principal: Some(Principal("bob".into())),
    };
    let desktop = Caller(":1.42".into());
    let own = Requester {
        caller: desktop.clone(),
        principal: None,
    };
    let own_bff = Requester {
        caller: bff.clone(),
        principal: None,
    };

    let core = |allow: bool, quiet: bool| {
        let log = FakeActionLog::new();
        let lines = log.lines.clone();
        let manager = Manager::new(
            FakeService::new(),
            FakeAuthority::new(allow).quietly(quiet).bff(&bff),
            FakeResolver::lan(),
        )
        .with_group(FakeGroup::new())
        .with_passwords(FakePasswords::new())
        .with_log(log);
        (manager, lines)
    };

    let file = |rel: &str, bytes: usize| {
        let p = models.join(rel);
        std::fs::create_dir_all(p.parent().expect("parent")).expect("parent dir");
        std::fs::write(&p, vec![b'g'; bytes]).expect("file");
    };
    let row = |rel: &str, size: u64, shards: u64, category: Option<&str>| ModelFileRow {
        path: models.join(rel).to_string_lossy().into_owned(),
        size,
        shards,
        category: category.map(str::to_string),
    };
    let sorted = |rows: Vec<ModelFileRow>| -> Vec<ModelFileRow> {
        let mut rows = rows;
        rows.sort_by(|a, b| a.path.cmp(&b.path));
        rows
    };

    // The fixture: every category, a loose file, a split model, a file of
    // another extension, a dot-folder, an upper-case extension, and a
    // symlinked folder whose file must not travel.
    file("loose.gguf", 5);
    file("UPPER.Gguf", 7);
    file("models/a.gguf", 100);
    file("models/o/r/deep.gguf", 50);
    file("models/big-00001-of-00003.gguf", 1000);
    file("models/big-00002-of-00003.gguf", 1000);
    file("models/big-00003-of-00003.gguf", 1000);
    file("mmprojs/mm.gguf", 30);
    file("mtps/mtp.gguf", 20);
    file("dflashs/d.gguf", 10);
    file("notes.txt", 9);
    file(".hf-staging/staged.gguf", 99);
    #[cfg(unix)]
    {
        let outside = dir.path().join("outside");
        std::fs::create_dir_all(&outside).expect("outside dir");
        std::fs::write(outside.join("y.gguf"), b"g").expect("outside file");
        std::os::unix::fs::symlink(&outside, models.join("link")).expect("symlink");
    }

    // ── The member's listing: absolute paths, shards collapsed, the
    //    category the first subfolder, the dot-folder and the link out ─────
    {
        let (m, lines) = core(true, true);
        let got = m.model_files(&member).expect("the member is served");
        assert_eq!(got.dir, models.to_string_lossy(), "the folder");
        assert!(
            std::path::Path::new(&got.dir).is_absolute(),
            "the folder, absolute"
        );
        assert!(!got.truncated, "the fixture is small");
        assert_eq!(got.error, None, "the folder reads");
        assert_eq!(
            sorted(got.files),
            sorted(vec![
                row("UPPER.Gguf", 7, 1, None),
                row("dflashs/d.gguf", 10, 1, Some("dflashs")),
                row("loose.gguf", 5, 1, None),
                row("mmprojs/mm.gguf", 30, 1, Some("mmprojs")),
                row("models/a.gguf", 100, 1, Some("models")),
                row("models/big-00001-of-00003.gguf", 1000, 3, Some("models")),
                row("models/o/r/deep.gguf", 50, 1, Some("models")),
                row("mtps/mtp.gguf", 20, 1, Some("mtps")),
            ]),
            "every .gguf once by absolute path, the split model as its first
             shard with its count, the txt, the dot-folder and the linked
             folder's file left out"
        );
        assert_eq!(
            *m.service().calls.lock().unwrap(),
            Vec::<fakes::Call>::new(),
            "the read reaches no service manager"
        );
        assert_eq!(
            *lines.lock().unwrap(),
            Vec::<String>::new(),
            "a read records no action"
        );
        assert_eq!(
            *m.authority().asked_quietly.lock().unwrap(),
            vec![],
            "the pair is the right: polkit is asked for nothing, not even quietly"
        );
    }

    // ── The caller's own call: control asked quietly, never with a prompt ──
    {
        let (m, _lines) = core(true, true);
        let got = m
            .model_files(&own_bff)
            .expect("the bff's own read is served");
        assert_eq!(got.dir, models.to_string_lossy());
        assert_eq!(
            *m.authority().asked_quietly.lock().unwrap(),
            vec![(bff.clone(), Action::Control)],
            "control asked without interaction, the way a read that may prompt for nothing is asked"
        );
        assert_eq!(
            *m.authority().asked.lock().unwrap(),
            Vec::<(Caller, Action)>::new(),
            "never asked with a prompt: a file list is worth no challenge"
        );
    }
    {
        let (m, _lines) = core(true, true);
        let got = m
            .model_files(&own)
            .expect("the desktop user's own read is served");
        assert_eq!(got.dir, models.to_string_lossy());
        assert_eq!(
            *m.authority().asked_quietly.lock().unwrap(),
            vec![(desktop.clone(), Action::Control)]
        );
    }

    // ── No control: refused, the folder never opened ───────────────────────
    {
        // A caller with control for the prompting check and none for the quiet
        // one: the listing is the quiet check's, so it is refused.
        let (m, _lines) = core(true, false);
        assert_eq!(
            m.model_files(&own),
            Err(ManagerError::NotAuthorized),
            "a caller polkit would challenge, or refuses quietly, gets no list"
        );
    }
    {
        // No control at all.
        let (m, _lines) = core(false, false);
        assert_eq!(
            m.model_files(&own),
            Err(ManagerError::NotAuthorized),
            "a caller without control"
        );
    }

    // ── The As form: the bff's account, and a current member of `pai` ──────
    {
        let (m, _lines) = core(true, true);
        assert_eq!(
            m.model_files(&outsider),
            Err(ManagerError::NotAuthorized),
            "bob is no member of `pai`"
        );
        assert_eq!(
            *m.authority().asked_quietly.lock().unwrap(),
            vec![],
            "a non-member's read asks polkit for nothing"
        );
    }
    {
        // A caller that is not the bff's account vouching for a user.
        let (m, _lines) = core(true, true);
        let forger = Requester {
            caller: desktop.clone(),
            principal: Some(Principal("alice".into())),
        };
        assert_eq!(
            m.model_files(&forger),
            Err(ManagerError::NotAuthorized),
            "only the bff's account vouches for a user"
        );
    }
    {
        // A member removed between two calls is refused at the next one.
        let passwords = FakePasswords::new();
        let admins = passwords.admins.clone();
        let manager = Manager::new(
            FakeService::new(),
            FakeAuthority::new(true).quietly(true).bff(&bff),
            FakeResolver::lan(),
        )
        .with_group(FakeGroup::new())
        .with_passwords(passwords)
        .with_log(FakeActionLog::new());
        assert!(manager.model_files(&member).is_ok(), "alice is a member");
        admins.lock().unwrap().remove("alice");
        assert_eq!(
            manager.model_files(&member),
            Err(ManagerError::NotAuthorized),
            "the membership is read at every call, never cached"
        );
    }

    // ── Depth: the walk descends at most 6 folders ─────────────────────────
    {
        file("d1/d2/d3/d4/d5/d6/deep6.gguf", 3);
        file("d1/d2/d3/d4/d5/d6/d7/deep7.gguf", 3);
        let (m, _lines) = core(true, true);
        let got = m.model_files(&member).expect("served");
        let deep6 = models
            .join("d1/d2/d3/d4/d5/d6/deep6.gguf")
            .to_string_lossy()
            .into_owned();
        let deep7 = models
            .join("d1/d2/d3/d4/d5/d6/d7/deep7.gguf")
            .to_string_lossy()
            .into_owned();
        assert!(
            got.files.iter().any(|f| f.path == deep6),
            "a file at depth 6 is listed: {deep6}"
        );
        assert!(
            !got.files.iter().any(|f| f.path == deep7),
            "a file at depth 7 is not: {deep7}"
        );
    }

    // ── Entries: at most 5000 per folder, then truncated ───────────────────
    {
        for i in 0..5001 {
            file(&format!("many/f{i:04}.gguf"), 1);
        }
        let (m, _lines) = core(true, true);
        let got = m.model_files(&member).expect("served");
        assert!(got.truncated, "5001 entries in one folder pass the bound");
        let many = got.files.iter().filter(|f| f.path.contains("many")).count();
        assert!(many <= 5000, "the bound is 5000 entries, not more: {many}");
    }

    // ── A folder that is not there: an OK answer with the keyed error ──────
    {
        std::fs::remove_dir_all(&models).expect("remove models dir");
        let (m, _lines) = core(true, true);
        let got = m
            .model_files(&member)
            .expect("a missing folder is not the call's failure");
        assert_eq!(got.dir, models.to_string_lossy());
        assert_eq!(got.files, Vec::new(), "nothing to list");
        assert!(!got.truncated);
        assert_eq!(
            got.error,
            Some(Reason {
                key: "models-dir-unreadable".into(),
                args: vec![models.to_string_lossy().into_owned()],
            }),
            "the folder's own keyed error, the picker's text"
        );
    }
    // ── The folder path is a regular file: the same answer ─────────────────
    {
        std::fs::write(&models, b"not a folder").expect("file at the folder path");
        let (m, _lines) = core(true, true);
        let got = m
            .model_files(&member)
            .expect("not a folder is not the call's failure");
        assert_eq!(got.files, Vec::new());
        assert_eq!(
            got.error,
            Some(Reason {
                key: "models-dir-unreadable".into(),
                args: vec![models.to_string_lossy().into_owned()],
            })
        );
    }
}

/// M3 review G2: the models-folder listing's bound counts the ENTRIES THE WALK
/// VISITS, not the rows it keeps. A bound on the rows alone leaves a tree of a
/// hundred thousand sidecars read in full, by the service's own account, on
/// every request. The walk stops at [`MODELS_WALK_MAX_ENTRIES`] entries
/// visited and answers `truncated` true; the bound is lowered through
/// [`Manager::with_walk_max_entries`], and the marker file under a subfolder
/// shows WHERE it stopped: a walk that stops inside the first folder never
/// descends to the marker. A tree below the bound is listed whole, with
/// `truncated` false.
#[cfg(test)]
pub(crate) fn manager_model_files_walk_bound_e2e() {
    use fakes::{FakeAuthority, FakeGroup, FakePasswords, FakeResolver, FakeService};

    let bff = Caller(":1.9".into());
    let member = Requester {
        caller: bff.clone(),
        principal: Some(Principal("alice".into())),
    };
    let core = |entries: usize| {
        Manager::new(
            FakeService::new(),
            FakeAuthority::new(true).quietly(true).bff(&bff),
            FakeResolver::lan(),
        )
        .with_group(FakeGroup::new())
        .with_passwords(FakePasswords::new())
        .with_walk_max_entries(entries)
    };
    let plain_core = || {
        Manager::new(
            FakeService::new(),
            FakeAuthority::new(true).quietly(true).bff(&bff),
            FakeResolver::lan(),
        )
        .with_group(FakeGroup::new())
        .with_passwords(FakePasswords::new())
    };
    // A fresh redirect per block: the bound counts the entries of the tree, so
    // no block may inherit another's files. The TempDir travels with the path:
    // a guard dropped here would delete the folder, and the block's fixture
    // writes would recreate it as a tree nothing owns (the /tmp leak).
    let fresh = || -> (tempfile::TempDir, std::path::PathBuf) {
        let dir = tempfile::tempdir().expect("tempdir");
        crate::paths::redirect_for_tests(dir.path());
        let models = crate::paths::instance_models_dir();
        std::fs::create_dir_all(&models).expect("models dir");
        (dir, models)
    };
    let file = |models: &std::path::Path, rel: &str| {
        let p = models.join(rel);
        std::fs::create_dir_all(p.parent().expect("parent")).expect("parent dir");
        std::fs::write(&p, b"g").expect("file");
    };

    // ── The bound's own number, and that it is not the rows bound ──────────
    assert_eq!(
        MODELS_WALK_MAX_ENTRIES, 50_000,
        "the bound of entries visited is 50 000"
    );
    const {
        assert!(
            MODELS_WALK_MAX_ENTRIES > MODELS_WALK_MAX_FILES,
            "the entries bound is not the rows bound wearing another name"
        );
    }

    // ── The walk stops inside the first folder: the marker is never reached ─
    {
        let (_dir, models) = fresh();
        for i in 0..250 {
            file(&models, &format!("side{i:03}.txt"));
        }
        file(&models, "sub/marker.gguf");

        let m = core(200);
        let got = m.model_files(&member).expect("served");
        assert_eq!(got.error, None, "the folder reads");
        assert!(
            got.truncated,
            "251 entries visited pass a bound of 200, and not one row was kept: {got:?}"
        );
        assert_eq!(
            got.files,
            Vec::new(),
            "the walk stopped inside the first folder: the marker under `sub` is not listed"
        );
    }

    // ── Entries, not rows: 150 rows under the rows bound, 250 entries over ──
    {
        let (_dir, models) = fresh();
        for i in 0..150 {
            file(&models, &format!("m{i:03}.gguf"));
        }
        for i in 0..100 {
            file(&models, &format!("s{i:03}.txt"));
        }
        let m = core(200);
        let got = m.model_files(&member).expect("served");
        assert!(
            got.truncated,
            "250 entries visited pass a bound of 200, and the rows are far under \
             the rows bound, so the rows cannot explain the flag: {} rows",
            got.files.len()
        );
        assert!(
            got.files.len() < 200,
            "the walk keeps no more rows than the entries it visited: {}",
            got.files.len()
        );
        assert!(
            got.files.iter().all(|f| f.path.ends_with(".gguf")),
            "every kept row is a model file: {got:?}"
        );
    }

    // ── Below the bound: the whole tree, the flag false ────────────────────
    {
        let (_dir, models) = fresh();
        for i in 0..150 {
            file(&models, &format!("m{i:03}.gguf"));
        }
        for i in 0..100 {
            file(&models, &format!("s{i:03}.txt"));
        }
        file(&models, "sub/marker.gguf");
        let marker = models
            .join("sub/marker.gguf")
            .to_string_lossy()
            .into_owned();

        let m = core(1000);
        let got = m.model_files(&member).expect("served");
        assert!(!got.truncated, "251 entries under a bound of 1000");
        assert_eq!(got.files.len(), 151, "every model file of the tree");
        assert!(
            got.files.iter().any(|f| f.path == marker),
            "the walk descended: {marker}"
        );

        // The default bound, untouched by the seam: the same tree is whole.
        let m = plain_core();
        let got = m.model_files(&member).expect("served");
        assert!(
            !got.truncated,
            "the default bound is far above a tree of 251 entries"
        );
        assert_eq!(got.files.len(), 151, "the default bound lists the tree");
    }
}

// ── GT5a: a ROCm instance's render loads models with `load-mode = none` ──────
//
// `ARCHITECTURE.md` "ROCm instances load models with `load-mode = none`", slice
// GT5a. The render of `models@<id>.ini` puts `load-mode = none` in its `[*]`
// section when the instance is ROCm (its server profile's `Devices` names a
// `rocm` device, or leaves `Devices` unset while the engine's installed backends
// hold `rocm`) and the pool's `[*]` carries no `load-mode` of its own. The pool
// files are never written; an explicit pool value is copied as it is; a model
// section's own `load-mode` is left alone (llama-server's own precedence puts
// the model section first); the launch passes no `--load-mode`. Linux only: the
// Windows rule is the cfg lint of `src/tests/gt5a_windows_gate.rs`.
#[cfg(all(test, not(windows)))]
pub(crate) fn manager_gt5a_rocm_load_mode_e2e() {
    use fakes::{FakeActionLog, FakeAuthority, FakeGroup, FakeResolver, FakeService};

    let dir = tempfile::tempdir().expect("tempdir");
    crate::paths::redirect_for_tests(dir.path());
    let sys = crate::paths::engine_config_dir();
    std::fs::create_dir_all(&sys).expect("engine config dir");
    let alice = Caller(":1.42".into());
    let m = Manager::new(
        FakeService::new(),
        FakeAuthority::new(true),
        FakeResolver::lan(),
    )
    .with_group(FakeGroup::new())
    .with_log(FakeActionLog::new());

    let render = |id: &str| std::fs::read_to_string(sys.join(format!("models@{id}.ini")));
    let render_text = |id: &str| -> String {
        render(id).unwrap_or_else(|e| panic!("GT5a: {id} has a render to read: {e}"))
    };
    let bytes_of = |name: &str| -> Vec<u8> { std::fs::read(sys.join(name)).unwrap_or_default() };
    // One key of one section of a render, read the way llama-server reads it,
    // so the line's spacing is the coder's and the key and its value are not.
    let key_of = |text: &str, section: &str, key: &str| -> Option<String> {
        pai_core::ini::parse_sections(text)
            .into_iter()
            .find(|s| s.id == section)
            .and_then(|s| {
                s.keys
                    .iter()
                    .find(|(k, _)| k.eq_ignore_ascii_case(key))
                    .map(|(_, v)| v.trim().to_string())
            })
    };
    // Every key of one section, as `key = value`: the section's whole content,
    // so "nothing was added" is pinned, not just the key.
    let keys_of = |text: &str, section: &str| -> Vec<String> {
        pai_core::ini::parse_sections(text)
            .into_iter()
            .find(|s| s.id == section)
            .map(|s| {
                s.keys
                    .iter()
                    .map(|(k, v)| format!("{k} = {}", v.trim()))
                    .collect()
            })
            .unwrap_or_default()
    };
    // The engine's installed backends, as its package files name them
    // (`versions::installed`: the engine's own `llama.cpp` file and one
    // `llama.cpp-<backend>` per runtime). Which directory the manager reads is
    // the coder's, so the fixture writes every place a redirect-aware
    // resolution of the engine's share folder sits: beside the pool files, and
    // the redirect root's siblings of `engine-config`.
    let versions_dirs = || -> Vec<std::path::PathBuf> {
        vec![
            sys.join("versions"),
            dir.path().join("engine-versions"),
            dir.path().join("engine-share").join("versions"),
            dir.path().join("share").join("versions"),
        ]
    };
    let installed_backends = |backends: &[&str]| {
        for d in versions_dirs() {
            std::fs::create_dir_all(&d).expect("versions dir");
            std::fs::write(d.join(ENGINE), "b1000\n").expect("engine version file");
            for b in ["cuda", "rocm", "vulkan"] {
                let path = d.join(format!("llama.cpp-{b}"));
                if backends.contains(&b) {
                    std::fs::write(&path, "1.0\n").expect("backend version file");
                } else {
                    let _ = std::fs::remove_file(&path);
                }
            }
        }
    };
    // The instances, each naming the server profile of the same name, and each
    // told the one model profile of the pool: the render carries its section, so
    // the pins read both the `[*]` and the model section.
    write_record(&sys, &["cpu", "default", "plain", "vk"]);
    std::fs::write(
        sys.join("assignments.ini"),
        "[cpu]\na = inherit\n\n[default]\na = inherit\n\n[plain]\na = inherit\n\n[vk]\na = inherit\n",
    )
    .expect("assignments.ini");
    let profile = |id: &str, port: u16, devices: Option<&str>| {
        let ini = match devices {
            Some(d) => format!("[Server]\nPort = {port}\nHostname = 127.0.0.1\nDevices = {d}\n",),
            None => format!("[Server]\nPort = {port}\nHostname = 127.0.0.1\n"),
        };
        assert_eq!(
            m.write_file(&alice, &format!("server@{id}.ini"), &ini, false),
            Ok(()),
            "GT5a: the server profile of {id} lands"
        );
    };
    profile("default", 8081, Some("ROCm0"));
    profile("vk", 8082, Some("Vulkan0,Vulkan1"));
    profile("cpu", 8083, Some("none"));
    profile("plain", 8084, None);

    // ── GT5a 1: `Devices = ROCm0` renders `load-mode = none` in `[*]` ───────
    let pool = real_model_files(dir.path(), "[a]\nmodel = /models/a.gguf\n");
    assert_eq!(
        m.write_file(&alice, "models.ini", &pool, false),
        Ok(()),
        "GT5a: the pool lands"
    );
    let r = render_text("default");
    assert!(
        r.starts_with("; Generated by pai-manager"),
        "GT5a 1: the render keeps the generated file's header line: {r}"
    );
    assert!(
        pai_core::ini::text::section_text(&r, "*").is_some(),
        "GT5a 1: the render's `[*]` section is created when the pool has none: {r}"
    );
    assert_eq!(
        key_of(&r, "*", "load-mode").as_deref(),
        Some("none"),
        "GT5a 1: an instance whose server profile names `ROCm0` renders \
         `load-mode = none` in `[*]`: {r}"
    );

    // ── GT5a 2: Vulkan-only, CPU and unset Devices render nothing ───────────
    // (The unset case with no engine package files: the installed backends are
    // read below, where the fixture puts them.)
    for id in ["vk", "cpu", "plain"] {
        let r = render_text(id);
        assert!(
            !r.contains("load-mode"),
            "GT5a 2: the instance {id} is not ROCm, so its render names no \
             `load-mode` anywhere: {r}"
        );
    }

    // ── GT5a 3: the pool's own `[*] load-mode` is copied as it is ───────────
    // The pool's global section arrives by a hand edit: a client write of the
    // pool is refused a `[*]` section (`invalid-model-profile-id`), so the
    // render is re-run over the file on disk.
    for value in crate::server_cfg::LOAD_MODES {
        let with_star = real_model_files(
            dir.path(),
            &format!("[*]\nload-mode = {value}\n\n[a]\nmodel = /models/a.gguf\n"),
        );
        std::fs::write(sys.join("models.ini"), &with_star).expect("hand-edited pool");
        assert!(
            m.render_failures().is_empty(),
            "GT5a 3: the render of the pool with `[*] load-mode = {value}` lands"
        );
        let r = render_text("default");
        assert_eq!(
            key_of(&r, "*", "load-mode").as_deref(),
            Some(value),
            "GT5a 3: the pool's `[*] load-mode = {value}` is copied as it is: {r}"
        );
        assert_eq!(
            keys_of(&r, "*"),
            vec![format!("load-mode = {value}")],
            "GT5a 3: an explicit pool `[*]` value is kept and nothing is added: {r}"
        );
    }

    // ── GT5a 9: a pool `[*]` with other keys and no `load-mode` gains the line
    // The `set_key` branch: the pool's `[*]` is copied verbatim and the line is
    // appended to it; the model section is not touched.
    let with_other = real_model_files(
        dir.path(),
        "[*]\nctx-size = 4096\n\n[a]\nmodel = /models/a.gguf\n",
    );
    std::fs::write(sys.join("models.ini"), &with_other).expect("hand-edited pool");
    assert!(
        m.render_failures().is_empty(),
        "GT5a 9: the render of the pool with `[*] ctx-size = 4096` lands"
    );
    let r = render_text("default");
    assert_eq!(
        keys_of(&r, "*"),
        vec![
            "ctx-size = 4096".to_string(),
            "load-mode = none".to_string()
        ],
        "GT5a 9: the pool's `[*]` key is kept and `load-mode = none` is added to it: {r}"
    );
    assert_eq!(
        pai_core::ini::text::section_text(&r, "a"),
        pai_core::ini::text::section_text(&with_other, "a"),
        "GT5a 9: the render's model section `[a]` is the pool's byte for byte"
    );

    // ── GT5a 10: the added line follows the pool's CRLF ────────────────────
    // The same pool with CRLF endings: the line lands with CRLF and the render
    // carries no LF-only line ending.
    let crlf = with_other.replace('\n', "\r\n");
    std::fs::write(sys.join("models.ini"), &crlf).expect("CRLF pool");
    assert!(
        m.render_failures().is_empty(),
        "GT5a 10: the render of the CRLF pool lands"
    );
    let r = render_text("default");
    let line = r
        .split_inclusive('\n')
        .find(|l| l.contains("load-mode"))
        .expect("GT5a 10: the render of the CRLF pool carries the line");
    assert!(
        line.ends_with("\r\n"),
        "GT5a 10: the added line ends with the pool's CRLF: {line:?}"
    );
    let b = r.as_bytes();
    let lf_only = (0..b.len())
        .filter(|i| b[*i] == b'\n' && (*i == 0 || b[i - 1] != b'\r'))
        .count();
    assert_eq!(
        lf_only, 0,
        "GT5a 10: the render of a CRLF pool carries no LF-only line ending: {r:?}"
    );

    // ── GT5a 4: a model section's own `load-mode` is untouched ──────────────
    let model_own = real_model_files(
        dir.path(),
        "[a]\nmodel = /models/a.gguf\nload-mode = mmap\n",
    );
    assert_eq!(
        m.write_file(&alice, "models.ini", &model_own, false),
        Ok(()),
        "GT5a 4: the pool whose model profile sets `load-mode = mmap` lands"
    );
    let r = render_text("default");
    assert_eq!(
        key_of(&r, "a", "load-mode").as_deref(),
        Some("mmap"),
        "GT5a 4: the model section keeps its own `load-mode = mmap`: {r}"
    );
    assert_eq!(
        key_of(&r, "*", "load-mode").as_deref(),
        Some("none"),
        "GT5a 4: the `[*]` line is added beside the model section's own (the \
         precedence is llama-server's, the render changes neither): {r}"
    );

    // ── GT5a 5: a server profile save that adds `ROCm0` re-renders ──────────
    // The pool back to a plain one, and the engine installed with Vulkan only:
    // `plain` (no `Devices`) is not ROCm, and becomes one by the save alone.
    assert_eq!(
        m.write_file(&alice, "models.ini", &pool, false),
        Ok(()),
        "GT5a 5: the plain pool lands"
    );
    installed_backends(&["vulkan"]);
    assert!(
        m.render_failures().is_empty(),
        "GT5a 5: the re-render lands"
    );
    assert!(
        !render_text("plain").contains("load-mode"),
        "GT5a 5: `plain` with no `Devices` and a Vulkan-only engine renders \
         no `load-mode`, before the save"
    );
    assert_eq!(
        m.write_file(
            &alice,
            "server@plain.ini",
            "[Server]\nPort = 8084\nHostname = 127.0.0.1\nDevices = ROCm0\n",
            false
        ),
        Ok(()),
        "GT5a 5: the save that adds `ROCm0` lands"
    );
    assert_eq!(
        key_of(&render_text("plain"), "*", "load-mode").as_deref(),
        Some("none"),
        "GT5a 5: a server profile save that adds `ROCm0` re-renders with the line"
    );
    assert_eq!(
        m.write_file(
            &alice,
            "server@plain.ini",
            "[Server]\nPort = 8084\nHostname = 127.0.0.1\n",
            false
        ),
        Ok(()),
        "GT5a 5: the save that drops `Devices` lands"
    );
    assert!(
        !render_text("plain").contains("load-mode"),
        "GT5a 5: the save that drops `Devices` re-renders without the line \
         (a Vulkan-only engine is not ROCm)"
    );

    // ── GT5a 6: unset Devices with a rocm backend installed renders it ──────
    installed_backends(&["vulkan", "rocm"]);
    assert!(
        m.render_failures().is_empty(),
        "GT5a 6: the re-render lands"
    );
    assert_eq!(
        key_of(&render_text("plain"), "*", "load-mode").as_deref(),
        Some("none"),
        "GT5a 6: `Devices` unset with a rocm backend file installed renders \
         `load-mode = none` in `[*]`"
    );
    for id in ["vk", "cpu"] {
        let r = render_text(id);
        assert!(
            !r.contains("load-mode"),
            "GT5a 6: {id} names its own non-rocm cards, so a rocm backend \
             installed changes nothing: {r}"
        );
    }
    installed_backends(&["vulkan"]);
    assert!(
        m.render_failures().is_empty(),
        "GT5a 6: the re-render lands"
    );
    assert!(
        !render_text("plain").contains("load-mode"),
        "GT5a 6: `Devices` unset with no rocm backend file renders no `load-mode`"
    );

    // ── GT5a 7: the launch passes no `--load-mode` ──────────────────────────
    let exe = dir
        .path()
        .join("bin")
        .join(crate::paths::binary_name("llama-server"));
    let cmd = crate::launch::instance_command("default", &exe)
        .expect("GT5a 7: the ROCm instance has a render to launch");
    let args: Vec<String> = cmd
        .get_args()
        .map(|a| a.to_string_lossy().into_owned())
        .collect();
    assert!(
        !args
            .iter()
            .any(|a| a == "--load-mode" || a.starts_with("--load-mode=")),
        "GT5a 7: the launch carries no `--load-mode` (a command line beats every \
         preset value, so the per-profile choice would be lost): {args:?}"
    );

    // ── GT5a 8: the pool files are never written ────────────────────────────
    // The bytes the last pool write left, read back before the renders above:
    // every render of this phase must leave them byte for byte where they are.
    let pool_bytes = bytes_of("models.ini");
    let disabled_bytes = bytes_of("models.ini.disabled");
    assert!(m.render_failures().is_empty(), "GT5a 8: the renders land");
    assert_eq!(
        bytes_of("models.ini"),
        pool_bytes,
        "GT5a 8: a render never writes the pool"
    );
    assert_eq!(
        bytes_of("models.ini.disabled"),
        disabled_bytes,
        "GT5a 8: a render never writes the switched-off pool"
    );
}

/// The 2.99.0 files of a two-instance install, seeded by the IP2 phases:
/// instance `default` runs the server profile `default` (127.0.0.1:8581) and
/// is given `alpha` and `beta`; instance `lab` runs `lab` (0.0.0.0:8681, one
/// rocm card) and is given `alpha` alone, so `alpha` sits in two renders and
/// the migration copies its keys into both entries; `gamma` is switched off
/// and assigned to nobody, so its placement moves to the profile
/// `unassigned`. The model files are real, under `root`. Answers the four
/// pool and record texts, so a phase can compare the bytes.
#[cfg(test)]
fn ip2_seed(root: &std::path::Path, sys: &std::path::Path) -> (String, String, String, String) {
    std::fs::create_dir_all(sys).expect("engine config dir");
    let models = real_model_files(
        root,
        "[*]\ntemp = 0.7\n\n[alpha]\nmodel = /models/alpha.gguf\ndevice = cuda0\nctx-size = 4096\n\n[beta]\nmodel = /models/beta.gguf\nn-cpu-moe = 8\n",
    );
    let disabled = real_model_files(
        root,
        "[gamma]\nmodel = /models/gamma.gguf\ndevice = cuda1\n",
    );
    let assignments = "[default]\nalpha = on\nbeta = on\n\n[lab]\nalpha = on\n".to_string();
    let instances =
        "[default]\nServerProfile = default\nLabel = First\n\n[lab]\nServerProfile = lab\n"
            .to_string();
    std::fs::write(sys.join("models.ini"), &models).expect("models.ini");
    std::fs::write(sys.join("models.ini.disabled"), &disabled).expect("models.ini.disabled");
    std::fs::write(sys.join("assignments.ini"), &assignments).expect("assignments.ini");
    std::fs::write(sys.join(INSTANCES_FILE), &instances).expect(INSTANCES_FILE);
    std::fs::write(
        sys.join("server@default.ini"),
        "[Server]\nHostname = 127.0.0.1\nPort = 8581\nModelsMax = 3\n",
    )
    .expect("server@default.ini");
    std::fs::write(
        sys.join("server@lab.ini"),
        "[Server]\nHostname = 0.0.0.0\nPort = 8681\nDevices = rocm0\n",
    )
    .expect("server@lab.ini");
    (models, disabled, assignments, instances)
}

/// E2E phase: IP2, the migration of the model data model at the manager's
/// start (`ARCHITECTURE.md` "Migration and rollback", the IP2 slice):
/// `pai_core::migrate_model` run over the engine's config dir before the
/// renders. It backs up `instances.ini`, `assignments.ini`, `models.ini` and
/// `models.ini.disabled` as `<name>.2.99.0`, once: a backup that is already
/// there is never overwritten. It then writes the new texts, merges the
/// switched-off pool into `models.ini` with every moved key out of it, moves
/// the address into the record and the placement into `instance-profiles.ini`
/// (with `unassigned` for the model no render holds), and only then deletes
/// `assignments.ini` and `models.ini.disabled`. The start runs it every time:
/// a second start of a migrated install changes no byte.
#[cfg(test)]
pub(crate) fn manager_ip2_migration_e2e() {
    use fakes::{FakeAuthority, FakeResolver, FakeService};

    let dir = tempfile::tempdir().expect("tempdir");
    crate::paths::redirect_for_tests(dir.path());
    let sys = crate::paths::engine_config_dir();
    let (models, disabled, assignments, instances) = ip2_seed(dir.path(), &sys);
    let m = Manager::new(
        FakeService::new(),
        FakeAuthority::new(true),
        FakeResolver::lan(),
    )
    .with_group(fakes::FakeGroup::new());
    let read = |name: &str| std::fs::read_to_string(sys.join(name)).ok();
    let bytes = |name: &str| std::fs::read(sys.join(name)).ok();

    // A backup that stands already is the first start's bytes: the sentinel must
    // survive every start. It stands for a file the seed DOES have, one the
    // migration moves, so the rule has teeth: the migration must keep the
    // install's own bytes and not overwrite them with the live file's.
    // (IP2 review D1 moved it off `instance-profiles.ini`, a name the first start
    // has no live file for: standing there it hid the backup the second start
    // added of the NEW shape.)
    std::fs::write(sys.join("server@default.ini.2.99.0"), "SENTINEL").expect("sentinel");

    m.start_up();

    assert!(
        m.render_failures().is_empty(),
        "the renders of the migrated install land"
    );

    // ── The backups, once ─────────────────────────────────────────────────
    assert_eq!(
        bytes("instances.ini.2.99.0").as_deref(),
        Some(instances.as_bytes()),
        "instances.ini backed up as it stood"
    );
    assert_eq!(
        bytes("assignments.ini.2.99.0").as_deref(),
        Some(assignments.as_bytes()),
        "assignments.ini backed up as it stood"
    );
    assert_eq!(
        bytes("models.ini.2.99.0").as_deref(),
        Some(models.as_bytes()),
        "models.ini backed up as it stood"
    );
    assert_eq!(
        bytes("models.ini.disabled.2.99.0").as_deref(),
        Some(disabled.as_bytes()),
        "models.ini.disabled backed up as it stood"
    );
    // The sentinel of the seed: a name the migration moves, whose backup stands
    // already, so the bytes that stand are the install's own and not the live
    // file's copied over them.
    assert_eq!(
        bytes("server@default.ini.2.99.0").as_deref(),
        Some("SENTINEL".as_bytes()),
        "an existing backup is never overwritten"
    );
    // `instance-profiles.ini` is the new shape's own file: the first start has no
    // old-shape text of it to keep, so it writes no backup of it (IP2 review D1).
    assert_eq!(
        bytes("instance-profiles.ini.2.99.0"),
        None,
        "the file the migration creates is not backed up"
    );

    // ── instance-profiles.ini: the profiles, the entries, unassigned ─────
    let profiles =
        pai_core::instance_profiles::parse(&read("instance-profiles.ini").expect("profiles file"));
    let profile = |id: &str| profiles.iter().find(|p| p.id == id).cloned();
    let entry_models = |id: &str| -> Vec<String> {
        profile(id)
            .map(|p| p.entries.iter().map(|e| e.model.clone()).collect())
            .unwrap_or_default()
    };
    let entry_keys = |id: &str, model: &str| -> Vec<(String, String)> {
        profile(id)
            .and_then(|p| p.entries.into_iter().find(|e| e.model == model))
            .map(|e| e.keys)
            .unwrap_or_default()
    };
    assert_eq!(
        profile("default").and_then(|p| p.server_profile),
        Some("default".into()),
        "default's profile names its server profile"
    );
    assert_eq!(
        entry_models("default"),
        ["alpha", "beta"],
        "the entries are the models its render held, in render order"
    );
    assert_eq!(
        entry_models("lab"),
        ["alpha"],
        "alpha is in two renders: the keys are copied, not moved"
    );
    assert!(
        entry_keys("default", "alpha")
            .iter()
            .any(|(k, v)| k == "device" && v == "cuda0"),
        "alpha's device moved into its entry: {:?}",
        entry_keys("default", "alpha")
    );
    assert!(
        entry_keys("default", "alpha")
            .iter()
            .any(|(k, v)| k == "ctx-size" && v == "4096"),
        "the memory-sized keys move too: {:?}",
        entry_keys("default", "alpha")
    );
    assert!(
        entry_keys("lab", "alpha")
            .iter()
            .any(|(k, v)| k == "device" && v == "cuda0"),
        "the copy under lab carries the same keys: {:?}",
        entry_keys("lab", "alpha")
    );
    assert!(
        entry_keys("default", "beta")
            .iter()
            .any(|(k, v)| k == "n-cpu-moe" && v == "8"),
        "beta's n-cpu-moe moved into its entry: {:?}",
        entry_keys("default", "beta")
    );
    assert_eq!(
        profile("unassigned").and_then(|p| p.server_profile),
        Some("default".into()),
        "the migration-made profile is of the only server profile"
    );
    assert_eq!(
        entry_models("unassigned"),
        ["gamma"],
        "the model no render holds goes to unassigned"
    );
    assert!(
        entry_keys("unassigned", "gamma")
            .iter()
            .any(|(k, v)| k == "device" && v == "cuda1"),
        "gamma's placement is kept in its unassigned entry: {:?}",
        entry_keys("unassigned", "gamma")
    );

    // ── The record: the profile, the address, the gone keys ──────────────
    let rec = |id: &str| -> std::collections::BTreeMap<String, String> {
        pai_core::ini::parse_sections(&read(INSTANCES_FILE).expect(INSTANCES_FILE))
            .into_iter()
            .find(|s| s.id == id)
            .map(|s| s.keys.clone())
            .unwrap_or_default()
    };
    assert_eq!(
        rec("default").get("InstanceProfile").map(String::as_str),
        Some("default"),
        "the record names the instance profile"
    );
    assert_eq!(
        rec("default").get("Hostname").map(String::as_str),
        Some("127.0.0.1"),
        "the address moved into the record"
    );
    assert_eq!(
        rec("default").get("Port").map(String::as_str),
        Some("8581"),
        "the port moved into the record"
    );
    assert_eq!(
        rec("default").get("Label").map(String::as_str),
        Some("First"),
        "the label is kept"
    );
    assert!(
        !rec("default").contains_key("ServerProfile"),
        "ServerProfile leaves the record: {:?}",
        rec("default")
    );
    assert_eq!(
        rec("lab").get("InstanceProfile").map(String::as_str),
        Some("lab"),
        "lab names its profile"
    );
    assert_eq!(
        rec("lab").get("Port").map(String::as_str),
        Some("8681"),
        "lab's port is in the record"
    );

    // ── The merged pool, every moved key out of it ───────────────────────
    let pool = pai_core::ini::parse_sections(&read("models.ini").expect("models.ini"));
    let pk = |id: &str| -> std::collections::BTreeMap<String, String> {
        pool.iter()
            .find(|s| s.id == id)
            .map(|s| s.keys.clone())
            .unwrap_or_default()
    };
    assert!(pk("alpha").contains_key("model"), "the file keys stay");
    for key in ["device", "ctx-size"] {
        assert!(
            !pk("alpha").contains_key(key),
            "{key} left the model profile: {:?}",
            pk("alpha")
        );
    }
    assert!(
        !pk("beta").contains_key("n-cpu-moe"),
        "n-cpu-moe left the model profile: {:?}",
        pk("beta")
    );
    assert!(
        pk("gamma").contains_key("model") && !pk("gamma").contains_key("device"),
        "the switched-off profile merged into the pool, placement out: {:?}",
        pk("gamma")
    );

    // ── The server profiles give up the address ──────────────────────────
    for name in ["server@default.ini", "server@lab.ini"] {
        let sk = pai_core::ini::parse_sections(&read(name).expect(name))
            .into_iter()
            .find(|s| s.id == "Server")
            .map(|s| s.keys.clone())
            .unwrap_or_default();
        assert!(
            !sk.contains_key("Hostname") && !sk.contains_key("Port"),
            "{name} gave up the address: {sk:?}"
        );
    }
    assert!(
        pai_core::ini::parse_sections(&read("server@default.ini").expect("profile"))
            .into_iter()
            .find(|s| s.id == "Server")
            .map(|s| s.keys.contains_key("ModelsMax"))
            == Some(true),
        "the other keys stay"
    );

    // ── The gone files ────────────────────────────────────────────────────
    assert_eq!(read("assignments.ini"), None, "assignments.ini is deleted");
    assert_eq!(
        read("models.ini.disabled"),
        None,
        "models.ini.disabled is deleted"
    );

    // ── A second start adds no file ──────────────────────────────────────
    // The names only: the full rule of the second start, bytes and mtimes of
    // every file included, is `manager_ip2_backups_once_e2e` (IP2 review D1),
    // where a failure names the files that moved instead of printing them.
    let names = || -> Vec<String> {
        let mut v: Vec<String> = std::fs::read_dir(&sys)
            .expect("listing")
            .flatten()
            .filter(|e| e.file_type().map(|t| t.is_file()).unwrap_or(false))
            .map(|e| e.file_name().to_string_lossy().into_owned())
            .collect();
        v.sort();
        v
    };
    let before = names();
    m.start_up();
    assert_eq!(names(), before, "the second start adds no file");
}

/// E2E phase: IP2, the resolution through the record: after the migration at
/// the start, every `models@<id>.ini` renders the same sections, with the
/// same key and value set in each, in the same order (the relaxed check,
/// `ARCHITECTURE.md`), and the launch of every instance answers the same
/// argv, `--host` and `--port` included; the address the list answers is the
/// record's, and GT5's rocm load rule is resolved through the instance
/// profile.
#[cfg(test)]
pub(crate) fn manager_ip2_resolution_e2e() {
    use fakes::{FakeAuthority, FakeResolver, FakeService};

    let dir = tempfile::tempdir().expect("tempdir");
    crate::paths::redirect_for_tests(dir.path());
    let sys = crate::paths::engine_config_dir();
    let (models, disabled, _assignments, _instances) = ip2_seed(dir.path(), &sys);
    let m = Manager::new(
        FakeService::new(),
        FakeAuthority::new(true),
        FakeResolver::lan(),
    )
    .with_group(fakes::FakeGroup::new());
    let read = |name: &str| std::fs::read_to_string(sys.join(name)).ok();

    // The section shape a render must keep: one entry per section, id with
    // its key and value set, in the file's own order.
    let shape = |text: &str| -> Vec<(String, std::collections::BTreeMap<String, String>)> {
        pai_core::ini::parse_sections(text)
            .into_iter()
            .map(|s| (s.id, s.keys))
            .collect()
    };
    let exe = std::path::PathBuf::from("/usr/bin/llama-server");
    let args_of = |id: &str| -> Vec<std::ffi::OsString> {
        crate::launch::instance_command(id, &exe)
            .expect("the instance launches")
            .get_args()
            .map(std::ffi::OsString::from)
            .collect()
    };

    // The install as it stands: the renders of the 2.99.0 manager, seeded on
    // disk, and the launch they answer.
    let ids = ["default", "lab"];
    let mut pre_shapes = Vec::new();
    let mut pre_args = Vec::new();
    for id in ids {
        let text = render_of(id, &models, &disabled);
        std::fs::write(sys.join(format!("models@{id}.ini")), &text).expect("seed render");
        pre_shapes.push(shape(&text));
        pre_args.push(args_of(id));
    }

    m.start_up();

    // Precondition: the start migrated the install; the pins below are the
    // post-migration resolution.
    assert_eq!(
        pai_core::ini::parse_sections(&read(INSTANCES_FILE).expect(INSTANCES_FILE))
            .into_iter()
            .find(|s| s.id == "default")
            .and_then(|s| s.keys.get("InstanceProfile").cloned()),
        Some("default".into()),
        "the start migrated the install: the record names the instance profile"
    );
    assert!(
        m.render_failures().is_empty(),
        "the post-migration renders land"
    );

    for (i, id) in ids.iter().enumerate() {
        let text = read(&format!("models@{id}.ini")).expect("render");
        assert_eq!(
            shape(&text),
            pre_shapes[i],
            "the render of {id} keeps its sections, its key and value sets and the order"
        );
        assert_eq!(
            args_of(id),
            pre_args[i],
            "the launch argv of {id} is unchanged (--host, --port, --models-preset)"
        );
    }

    // The binding: the address the record carries is the one the instance
    // list answers.
    let ports: Vec<(String, u16)> = m.instances().into_iter().map(|r| (r.id, r.port)).collect();
    assert_eq!(
        ports,
        vec![("default".into(), 8581u16), ("lab".into(), 8681)],
        "the address resolves through the record"
    );

    // GT5: the rocm instance's load rule is resolved through the instance
    // profile: the render still carries it, and the plain instance's not.
    #[cfg(target_os = "linux")]
    {
        let lab = read("models@lab.ini").expect("lab render");
        assert!(
            lab.contains("load-mode = none"),
            "GT5 resolves the rocm rule through the instance profile: {lab}"
        );
        assert!(
            !read("models@default.ini")
                .expect("default render")
                .contains("load-mode"),
            "the non-rocm instance keeps no rule"
        );
    }
}

/// E2E phase: IP2, the stray `assignments.ini` rule (the package pitfall,
/// `ARCHITECTURE.md`): llama.cpp's package copies a missing `assignments.ini`
/// back at its next upgrade, so a start of a migrated install deletes a
/// stray one and touches nothing else.
#[cfg(test)]
pub(crate) fn manager_ip2_stray_assignments_e2e() {
    use fakes::{FakeAuthority, FakeResolver, FakeService};

    let dir = tempfile::tempdir().expect("tempdir");
    crate::paths::redirect_for_tests(dir.path());
    let sys = crate::paths::engine_config_dir();
    let _seed = ip2_seed(dir.path(), &sys);
    let m = Manager::new(
        FakeService::new(),
        FakeAuthority::new(true),
        FakeResolver::lan(),
    )
    .with_group(fakes::FakeGroup::new());
    let read = |name: &str| std::fs::read_to_string(sys.join(name)).ok();
    let bytes = |name: &str| std::fs::read(sys.join(name)).ok();

    m.start_up();
    assert_eq!(
        read("assignments.ini"),
        None,
        "the start migrated the install: assignments.ini is gone"
    );

    // The package's next upgrade copies it back.
    std::fs::write(sys.join("assignments.ini"), "[default]\nalpha = on\n").expect("stray");
    let record = bytes(INSTANCES_FILE);
    let pool = bytes("models.ini");
    let render = bytes("models@default.ini");

    m.start_up();

    assert_eq!(
        read("assignments.ini"),
        None,
        "a stray assignments.ini is deleted at the start"
    );
    assert_eq!(
        bytes(INSTANCES_FILE),
        record,
        "the stray delete leaves the record alone"
    );
    assert_eq!(bytes("models.ini"), pool, "and the pool");
    assert_eq!(bytes("models@default.ini"), render, "and every render");
}

/// E2E phase: IP2, the write refusals of the gone files and of the moved
/// keys: `WriteFile` of `assignments.ini` and of `models.ini.disabled` is
/// refused (the names are no longer served files), and a `models.ini` write
/// carrying a key that moved to the instance profile's entry is refused with
/// `model-profile-placement-key`, naming the key. The keys that stayed land.
#[cfg(test)]
pub(crate) fn manager_ip2_write_refusals_e2e() {
    use fakes::{FakeAuthority, FakeResolver, FakeService};

    let dir = tempfile::tempdir().expect("tempdir");
    crate::paths::redirect_for_tests(dir.path());
    let sys = crate::paths::engine_config_dir();
    let _seed = ip2_seed(dir.path(), &sys);
    let m = Manager::new(
        FakeService::new(),
        FakeAuthority::new(true),
        FakeResolver::lan(),
    )
    .with_group(fakes::FakeGroup::new());
    let alice = Caller(":1.42".into());
    let read = |name: &str| std::fs::read_to_string(sys.join(name)).ok();

    m.start_up();

    // ── The gone files answer the name refusal ────────────────────────────
    assert_eq!(
        m.write_file(&alice, "assignments.ini", "[default]\nalpha = on\n", false),
        Err(ManagerError::BadFileName),
        "assignments.ini is not a served file any more"
    );
    assert_eq!(
        m.write_file_if(&alice, "assignments.ini", "[default]\nalpha = on\n", ""),
        Err(ManagerError::BadFileName),
        "not even a compare-and-swap"
    );
    assert_eq!(
        m.write_file(
            &alice,
            "models.ini.disabled",
            "[epsilon]\nmodel = /models/epsilon.gguf\n",
            false
        ),
        Err(ManagerError::BadFileName),
        "the switch file is gone"
    );
    assert_eq!(
        read("models.ini.disabled"),
        None,
        "the refused write created no file"
    );

    // ── A moved key in a models.ini write is refused, naming it ──────────
    let moved = real_model_files(
        dir.path(),
        "[alpha]\nmodel = /models/alpha.gguf\ndevice = cuda0\n",
    );
    let answer = m.write_file(&alice, "models.ini", &moved, false);
    assert!(
        matches!(&answer, Err(ManagerError::Invalid(r))
            if r.key == "model-profile-placement-key" && r.values == ["device"]),
        "a moved key is refused, naming it: {answer:?}"
    );

    // The keys that stayed land.
    let stayed = real_model_files(
        dir.path(),
        "[alpha]\nmodel = /models/alpha.gguf\ntemp = 0.7\n",
    );
    assert_eq!(
        m.write_file(&alice, "models.ini", &stayed, false),
        Ok(()),
        "a models.ini of the stayed keys lands"
    );
}

/// The engine's config folder as a sorted list of (name, text), the text read
/// exactly as it stands.
#[cfg(test)]
fn ip2_tree_raw(sys: &std::path::Path) -> Vec<(String, String)> {
    let mut v: Vec<(String, String)> = std::fs::read_dir(sys)
        .expect("listing")
        .flatten()
        .filter(|e| e.file_type().map(|t| t.is_file()).unwrap_or(false))
        .map(|e| {
            (
                e.file_name().to_string_lossy().into_owned(),
                std::fs::read_to_string(e.path()).expect("text"),
            )
        })
        .collect();
    v.sort();
    v
}

/// [`ip2_tree_raw`] with the fixture's own folder spelled `<root>` in every text:
/// the files name their model files by absolute path, so two trees seeded the
/// same way differ by the folder they sit in and by nothing else. The IP2
/// crash-phase fixtures compare whole trees with it.
#[cfg(test)]
fn ip2_tree(root: &std::path::Path, sys: &std::path::Path) -> Vec<(String, String)> {
    let root = root.to_string_lossy().into_owned();
    ip2_tree_raw(sys)
        .into_iter()
        .map(|(name, text)| (name, text.replace(root.as_str(), "<root>")))
        .collect()
}

/// The names the two tree listings do not agree on: a file one of them has and
/// the other has not, or has with other bytes. The names alone, so a failure
/// reads as a list of files and not a wall of text.
#[cfg(test)]
fn ip2_tree_map(tree: &[(String, String)]) -> std::collections::BTreeMap<&str, &str> {
    tree.iter()
        .map(|(name, text)| (name.as_str(), text.as_str()))
        .collect()
}

#[cfg(test)]
fn ip2_tree_diff(a: &[(String, String)], b: &[(String, String)]) -> Vec<String> {
    let (a, b) = (ip2_tree_map(a), ip2_tree_map(b));
    let names: std::collections::BTreeSet<&str> = a.keys().chain(b.keys()).copied().collect();
    names
        .into_iter()
        .filter(|n| a.get(n) != b.get(n))
        .map(|n| n.to_string())
        .collect()
}

/// A manager over the redirected tree, for the IP2 fix-round phases (the review
/// of the slice, D1 to D5). No unit runs: the fake's default answers EVERY unit
/// as running, and the running list a verb answers follows the units in `running`.
#[cfg(test)]
pub(crate) fn ip2_manager() -> Manager<fakes::FakeService, fakes::FakeAuthority, fakes::FakeResolver>
{
    use fakes::{FakeAuthority, FakeResolver, FakeService};
    Manager::new(
        FakeService {
            state: crate::manager::service::UnitState::Inactive,
            ..FakeService::new()
        },
        FakeAuthority::new(true),
        FakeResolver::lan(),
    )
    .with_group(fakes::FakeGroup::new())
}

/// E2E phase: IP2 review D1: the backups are written once, and only of the OLD
/// shape. Nothing is planted here: the first start of the install backs up the
/// old-shape files it moves, and no other name, a second start adds no file and
/// changes no byte and no mtime of any file (so the `instance-profiles.ini` the
/// first start wrote in the NEW shape is never backed up as the install's own
/// text), and a server profile created after the migration gets no `.2.99.0`.
#[cfg(test)]
pub(crate) fn manager_ip2_backups_once_e2e() {
    let dir = tempfile::tempdir().expect("tempdir");
    crate::paths::redirect_for_tests(dir.path());
    let sys = crate::paths::engine_config_dir();
    let _seed = ip2_seed(dir.path(), &sys);
    let m = ip2_manager();

    // Every file of the engine's config folder: name, bytes, and the moment it
    // was last written.
    let snapshot = || -> Vec<(String, Vec<u8>, std::time::SystemTime)> {
        let mut v: Vec<(String, Vec<u8>, std::time::SystemTime)> = std::fs::read_dir(&sys)
            .expect("listing")
            .flatten()
            .filter(|e| e.file_type().map(|t| t.is_file()).unwrap_or(false))
            .map(|e| {
                (
                    e.file_name().to_string_lossy().into_owned(),
                    std::fs::read(e.path()).expect("read"),
                    std::fs::metadata(e.path())
                        .expect("metadata")
                        .modified()
                        .expect("mtime"),
                )
            })
            .collect();
        v.sort();
        v
    };
    let backups = |tree: &[(String, Vec<u8>, std::time::SystemTime)]| -> Vec<String> {
        tree.iter()
            .map(|(name, _, _)| name.clone())
            .filter(|name| name.ends_with(BACKUP_SUFFIX))
            .collect()
    };

    m.start_up();
    let first = snapshot();
    assert_eq!(
        backups(&first),
        vec![
            "assignments.ini.2.99.0",
            "instances.ini.2.99.0",
            "models.ini.2.99.0",
            "models.ini.disabled.2.99.0",
            "server@default.ini.2.99.0",
            "server@lab.ini.2.99.0",
        ],
        "IP2 review D1: the first start backs up the old-shape files it moves, and no \
         other name: the file it creates has no old text to keep"
    );

    // A second start of the migrated install leaves the tree where it stands. The
    // sleep puts a clock tick between the two starts, so a file rewritten with the
    // same bytes shows on its mtime. The diff names what moved, so a failure reads
    // as a list of files and not a wall of bytes.
    std::thread::sleep(std::time::Duration::from_millis(50));
    m.start_up();
    let moved = |after: &[(String, Vec<u8>, std::time::SystemTime)]| -> Vec<String> {
        let before: std::collections::BTreeMap<&str, (&[u8], &std::time::SystemTime)> = first
            .iter()
            .map(|(name, bytes, mtime)| (name.as_str(), (bytes.as_slice(), mtime)))
            .collect();
        let names: std::collections::BTreeSet<&str> = after
            .iter()
            .map(|(name, _, _)| name.as_str())
            .chain(before.keys().copied())
            .collect();
        names
            .into_iter()
            .filter(|name| match before.get(name) {
                // Stands in the new tree only: the start added the file.
                None => true,
                Some((bytes, mtime)) => after
                    .iter()
                    .any(|(n, b, m)| n == name && (b != *bytes || m != *mtime)),
            })
            .map(|n| n.to_string())
            .collect()
    };
    assert_eq!(
        moved(&snapshot()),
        Vec::<String>::new(),
        "IP2 review D1: a second start adds no file and changes no byte and no mtime \
         (a new-shape file backed up as the install's own text, or a file rewritten \
         every start)"
    );

    // A server profile written after the migration is no old-shape file: the next
    // start backs nothing up of it.
    std::fs::write(sys.join("server@gamma.ini"), "[Server]\nDevices = cuda0\n").expect("gamma");
    std::thread::sleep(std::time::Duration::from_millis(50));
    m.start_up();
    assert_eq!(
        backups(&snapshot()),
        backups(&first),
        "IP2 review D1: a server profile created after the migration gets no `.2.99.0`"
    );
}

/// E2E phase: IP2 review D2: `server_profiles` answers each profile's port as
/// the record of the instance using it holds it. The migration moves the address
/// into `instances.ini` and takes it out of every `server@<id>.ini`, so a list
/// that reads the profile file alone answers `server_cfg::DEFAULT_PORT` for a
/// migrated install. The rule the server-profile list phase pins stays: a profile
/// no instance uses answers its file's own `Port`, and a file with no `Port` is
/// `DEFAULT_PORT`.
#[cfg(test)]
pub(crate) fn manager_ip2_server_profile_port_from_record_e2e() {
    let dir = tempfile::tempdir().expect("tempdir");
    crate::paths::redirect_for_tests(dir.path());
    let sys = crate::paths::engine_config_dir();
    let _seed = ip2_seed(dir.path(), &sys);
    let m = ip2_manager();
    // A profile no instance uses, with no `Port` of its own: the row keeps the
    // file's answer before and after the migration.
    std::fs::write(sys.join("server@gamma.ini"), "[Server]\nDevices = cuda0\n").expect("gamma");
    let rows = || -> Vec<(String, u16, Option<String>)> {
        m.server_profiles()
            .into_iter()
            .map(|r| (r.id, r.port, r.instance))
            .collect()
    };
    let want = || -> Vec<(String, u16, Option<String>)> {
        vec![
            ("default".to_string(), 8581, Some("default".to_string())),
            (
                "gamma".to_string(),
                u16::try_from(crate::server_cfg::DEFAULT_PORT).expect("DEFAULT_PORT"),
                None,
            ),
            ("lab".to_string(), 8681, Some("lab".to_string())),
        ]
    };

    // The install as it stands: the file answers its own `Port`, and a file with
    // no `Port` and no section naming it is `DEFAULT_PORT`.
    assert_eq!(
        rows(),
        want(),
        "the unmigrated install: each profile file answers its own `Port`"
    );

    m.start_up();
    // Precondition: the migration ran, and the profile file gave the address up.
    assert!(
        !pai_core::ini::section_keys(
            &std::fs::read_to_string(sys.join("server@default.ini")).expect("profile"),
            "Server"
        )
        .into_iter()
        .any(|(k, _)| k == "Port"),
        "the migration took `Port` out of the profile file"
    );
    assert_eq!(
        pai_core::ini::section_keys(
            &std::fs::read_to_string(sys.join(INSTANCES_FILE)).expect(INSTANCES_FILE),
            "default"
        )
        .into_iter()
        .find(|(k, _)| k == "Port")
        .map(|(_, v)| v),
        Some("8581".to_string()),
        "the record of the instance using the profile holds the port"
    );
    // The list answers the record's port, not the default. A profile no instance
    // uses keeps the file's own answer, no `Port` being `DEFAULT_PORT` (the rule
    // the server-profile list phase pins).
    assert_eq!(
        rows(),
        want(),
        "IP2 review D2: after the migration the row carries the port as the record of \
         the instance using the profile holds it, and an unused profile keeps the file's \
         own answer"
    );
}

/// E2E phase: IP2 review D3: a crash mid-backup leaves no truncated
/// `<name>.2.99.0`. A backup is a file the NEXT start keeps as the install's own
/// text, so a backup that appears at its final name before it is whole is a
/// truncated install the recovery reads. The manager has no write seam to fail
/// mid-copy, so this pins the observable half of the rule: a start whose backup
/// write fails leaves NO entry at any backup name, and no temp name of its own,
/// and says so through the renders it cannot write. The fixture closes the
/// engine's folder to writes (a unix mode, the mode the manager ships these files
/// with), so no write of the start lands, whichever name it aims at.
#[cfg(test)]
pub(crate) fn manager_ip2_backup_atomic_e2e() {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;

        let dir = tempfile::tempdir().expect("tempdir");
        crate::paths::redirect_for_tests(dir.path());
        let sys = crate::paths::engine_config_dir();
        let _seed = ip2_seed(dir.path(), &sys);
        let m = ip2_manager();

        std::fs::set_permissions(&sys, std::fs::Permissions::from_mode(0o555)).expect("chmod");
        let failures = m.start_up();
        std::fs::set_permissions(&sys, std::fs::Permissions::from_mode(0o755)).expect("chmod back");

        assert!(
            failures.iter().any(|(id, e)| id == "default"
                && matches!(e, ManagerError::Io(std::io::ErrorKind::PermissionDenied))),
            "IP2 review D3: a start that cannot write says so: {failures:?}"
        );
        let names: Vec<String> = std::fs::read_dir(&sys)
            .expect("listing")
            .flatten()
            .map(|e| e.file_name().to_string_lossy().into_owned())
            .collect();
        assert!(
            !names.iter().any(|n| n.ends_with(BACKUP_SUFFIX)),
            "IP2 review D3: a backup that failed leaves no entry at its final name, so \
             no truncated one is there for the next start to keep: {names:?}"
        );
        assert!(
            !names.iter().any(|n| n.ends_with(".tmp")),
            "IP2 review D3: and no temp file of a write that did not finish: {names:?}"
        );
    }
    #[cfg(not(unix))]
    {
        // The fixture is a unix mode, the mode the manager ships these files
        // with; Windows has no per-file mode to close the folder with.
    }
}

/// E2E phase: IP2 review D4a: a start that finds the tree stopped mid-migration
/// finishes it. Two crash points: just past the write of `instance-profiles.ini`,
/// and just past the write of `instances.ini`. The fixture runs a clean migration
/// in one tree, and plants in a second one what the crash left: the backups (they
/// go before every write) and the new text of every file written so far, over the
/// seed's old text of the rest. The start there must end with a tree equal, byte
/// for byte, to the clean one.
#[cfg(test)]
pub(crate) fn manager_ip2_crash_mid_migration_e2e() {
    // The clean migration, in its own tree: the tree every crash must end in. The
    // crash leaves exactly these texts, so the fixture plants them (the folder is
    // spelled `<root>`, the one placeholder that survives the move to the other
    // tree: a file the start rewrites lands with the new tree's own paths).
    let clean = {
        let dir = tempfile::tempdir().expect("tempdir");
        crate::paths::redirect_for_tests(dir.path());
        let sys = crate::paths::engine_config_dir();
        let _seed = ip2_seed(dir.path(), &sys);
        ip2_manager().start_up();
        ip2_tree(dir.path(), &sys)
    };
    assert!(
        clean.iter().any(|(name, _)| name == INSTANCES_FILE),
        "the clean migration wrote the record"
    );

    for crash_after in [INSTANCE_PROFILES_FILE, INSTANCES_FILE] {
        let dir = tempfile::tempdir().expect("tempdir");
        crate::paths::redirect_for_tests(dir.path());
        let sys = crate::paths::engine_config_dir();
        let _seed = ip2_seed(dir.path(), &sys);
        for (name, text) in &clean {
            // What the crash left: every backup the clean start wrote (they go
            // first), the pool of instance profiles (its write is the first), and
            // the record when the crash is the later one.
            let written = name.ends_with(BACKUP_SUFFIX)
                || *name == INSTANCE_PROFILES_FILE
                || (*name == INSTANCES_FILE && crash_after == INSTANCES_FILE);
            if written {
                std::fs::write(sys.join(name), text).expect("plant");
            }
        }
        // Precondition: the planted tree really is a half-migrated one: it is not
        // the clean tree the start must end in.
        assert_ne!(
            ip2_tree(dir.path(), &sys),
            clean,
            "IP2 review D4a: the fixture planted a real mid-migration state"
        );
        ip2_manager().start_up();
        assert_eq!(
            ip2_tree_diff(&ip2_tree(dir.path(), &sys), &clean),
            Vec::<String>::new(),
            "IP2 review D4a: a start after a crash just past `{crash_after}` ends equal \
             to a clean migration (these files differ)"
        );
    }
}

/// E2E phase: IP2 review D4b: a failed migration leaves the manager serving the
/// tree as it stands, with the retired files in place. Two failures: a file that
/// cannot be read (a folder where `models.ini` should be), and a backup that
/// cannot be written (the engine's folder closed to writes). Both leave
/// `assignments.ini` and `models.ini.disabled` where they are, the record as the
/// install wrote it, no backup at all, and the instance list answering the
/// address the old files hold.
#[cfg(test)]
pub(crate) fn manager_ip2_failed_migration_serves_e2e() {
    // ── A file that cannot be read ────────────────────────────────────────
    {
        let dir = tempfile::tempdir().expect("tempdir");
        crate::paths::redirect_for_tests(dir.path());
        let sys = crate::paths::engine_config_dir();
        let (_models, disabled, assignments, instances) = ip2_seed(dir.path(), &sys);
        std::fs::remove_file(sys.join("models.ini")).expect("remove");
        std::fs::create_dir(sys.join("models.ini")).expect("folder in place of the file");
        let m = ip2_manager();
        m.start_up();
        assert_eq!(
            std::fs::read_to_string(sys.join(INSTANCES_FILE))
                .ok()
                .as_deref(),
            Some(instances.as_str()),
            "IP2 review D4b: the unreadable file stopped the migration: the record is \
             the install's own"
        );
        assert_eq!(
            std::fs::read_to_string(sys.join(ASSIGNMENTS_FILE))
                .ok()
                .as_deref(),
            Some(assignments.as_str()),
            "IP2 review D4b: the retired file is not deleted"
        );
        assert_eq!(
            std::fs::read_to_string(sys.join(MODELS_DISABLED_FILE))
                .ok()
                .as_deref(),
            Some(disabled.as_str()),
            "IP2 review D4b: nor the switched-off pool"
        );
        let names: Vec<String> = std::fs::read_dir(&sys)
            .expect("listing")
            .flatten()
            .map(|e| e.file_name().to_string_lossy().into_owned())
            .collect();
        assert!(
            !names.iter().any(|n| n.ends_with(BACKUP_SUFFIX)),
            "IP2 review D4b: a migration that stopped at a read wrote no backup: {names:?}"
        );
        let ports: Vec<(String, u16)> = m.instances().into_iter().map(|r| (r.id, r.port)).collect();
        assert_eq!(
            ports,
            vec![("default".to_string(), 8581u16), ("lab".to_string(), 8681)],
            "IP2 review D4b: the manager serves the old files: the address is the \
             profile's own"
        );
    }

    // ── A backup that cannot be written ──────────────────────────────────
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;

        let dir = tempfile::tempdir().expect("tempdir");
        crate::paths::redirect_for_tests(dir.path());
        let sys = crate::paths::engine_config_dir();
        let (models, disabled, _assignments, instances) = ip2_seed(dir.path(), &sys);
        let m = ip2_manager();
        // The renders of the install as it stands, written before the folder
        // closes: they are the unmigrated ones the manager keeps serving.
        let renders: Vec<(String, String)> = ["default", "lab"]
            .into_iter()
            .map(|id| {
                let text = render_of(id, &models, &disabled);
                std::fs::write(sys.join(format!("models@{id}.ini")), &text).expect("render");
                (id.to_string(), text)
            })
            .collect();
        std::fs::set_permissions(&sys, std::fs::Permissions::from_mode(0o555)).expect("chmod");
        let failures = m.start_up();
        std::fs::set_permissions(&sys, std::fs::Permissions::from_mode(0o755)).expect("chmod back");

        assert!(
            failures.iter().any(|(id, _)| id == "default"),
            "IP2 review D4b: the start says what did not land: {failures:?}"
        );
        for (id, text) in renders {
            assert_eq!(
                std::fs::read_to_string(sys.join(format!("models@{id}.ini"))).ok(),
                Some(text),
                "IP2 review D4b: the instance keeps the render of the install as it stood"
            );
        }
        assert_eq!(
            std::fs::read_to_string(sys.join(INSTANCES_FILE))
                .ok()
                .as_deref(),
            Some(instances.as_str()),
            "IP2 review D4b: the failed backup stopped the writes: the record is the \
             install's own"
        );
        assert!(
            std::fs::read_to_string(sys.join(ASSIGNMENTS_FILE)).is_ok(),
            "IP2 review D4b: the retired file is not deleted by a failed migration"
        );
        assert!(
            std::fs::read_to_string(sys.join(MODELS_DISABLED_FILE)).is_ok(),
            "IP2 review D4b: nor the switched-off pool"
        );
        let ports: Vec<(String, u16)> = m.instances().into_iter().map(|r| (r.id, r.port)).collect();
        assert_eq!(
            ports,
            vec![("default".to_string(), 8581u16), ("lab".to_string(), 8681)],
            "IP2 review D4b: the manager serves the unmigrated install"
        );
    }
}

/// E2E phase: IP2 review D5: the render of an instance given no model profile.
/// `render_failures`' doc says such an instance is "left alone"; the render the
/// manager writes is the empty one (the pool's global section, and no model
/// profile), as it was before the migration for an instance with no assignment.
/// The empty render is the old behaviour, so the code is right and the doc is the
/// mismatch: this pins the render lands, and carries no model profile.
#[cfg(test)]
pub(crate) fn manager_ip2_empty_render_e2e() {
    let dir = tempfile::tempdir().expect("tempdir");
    crate::paths::redirect_for_tests(dir.path());
    let sys = crate::paths::engine_config_dir();
    let _seed = ip2_seed(dir.path(), &sys);
    let m = ip2_manager();
    m.start_up();
    assert!(
        m.render_failures().is_empty(),
        "the migrated install's renders land"
    );

    // An instance given nothing: the record names an instance profile, and that
    // profile has no entry section.
    std::fs::write(
        sys.join(INSTANCES_FILE),
        format!(
            "{}\n[bare]\nInstanceProfile = bare\n",
            std::fs::read_to_string(sys.join(INSTANCES_FILE)).expect(INSTANCES_FILE)
        ),
    )
    .expect("record");
    std::fs::write(
        sys.join(INSTANCE_PROFILES_FILE),
        format!(
            "{}\n[bare]\nServerProfile = default\n",
            std::fs::read_to_string(sys.join(INSTANCE_PROFILES_FILE)).expect("profiles")
        ),
    )
    .expect("profiles");

    assert!(
        m.render_failures().is_empty(),
        "IP2 review D5: the render of an instance given nothing lands"
    );
    let text = std::fs::read_to_string(sys.join("models@bare.ini"));
    assert!(
        text.is_ok(),
        "IP2 review D5: an instance given no model profile is NOT left alone: its \
         render is written (the doc says \"left alone\", the code writes the empty render)"
    );
    let ids: Vec<String> = pai_core::ini::parse_sections(&text.expect("render"))
        .into_iter()
        .map(|s| s.id)
        .collect();
    assert!(
        ids.iter().all(|id| id == "*"),
        "IP2 review D5: the empty render is the pool's global section and no model \
         profile section: {ids:?}"
    );
}

/// E2E phase: IP3a, the instance profile verbs at the manager
/// (`ARCHITECTURE.md` "Instance profiles", slice IP3, part a): the pool reads,
/// `CreateInstanceProfile` / `DeleteInstanceProfile`, the entry add and remove,
/// `SetInstanceProfile` (the record's `InstanceProfile`, the pool's text as
/// revision), `SetInstanceAddress` (the record's `Hostname` and `Port`, the
/// record's text as revision) and `CreateInstance` proposing the first free
/// port at or above 8581. Every write is refused before it touches a file, the
/// pool's and the record's bytes are pinned around each answer, and a taken
/// address is `AddressInUse`, naming who holds it (another instance, the web
/// service).
#[cfg(test)]
pub(crate) fn manager_ip3a_instance_profiles_e2e() {
    let dir = tempfile::tempdir().expect("tempdir");
    crate::paths::redirect_for_tests(dir.path());
    let sys = crate::paths::engine_config_dir();
    let _seed = ip2_seed(dir.path(), &sys);
    let m = ip2_manager();
    let alice = Caller(":1.42".into());

    let read = |name: &str| std::fs::read_to_string(sys.join(name)).expect("read");
    let pool = || read(INSTANCE_PROFILES_FILE);
    let record = || read(INSTANCES_FILE);
    let render = |id: &str| read(&format!("models@{id}.ini"));
    let keys = |text: &str, section: &str| pai_core::ini::section_keys(text, section);
    let key_of = |text: &str, section: &str, key: &str| -> Option<String> {
        keys(text, section)
            .into_iter()
            .find(|(k, _)| k == key)
            .map(|(_, v)| v)
    };
    let sections = |text: &str| -> Vec<String> {
        pai_core::ini::parse_sections(text)
            .into_iter()
            .map(|s| s.id)
            .collect()
    };
    let refused = |key: &'static str, values: &[&str]| -> ManagerError {
        ManagerError::Invalid(pai_core::checks::Refusal {
            key,
            values: values.iter().map(|v| v.to_string()).collect(),
        })
    };
    let no_profile = |name: &str| ManagerError::NoInstanceProfile(name.to_string());

    m.start_up();
    assert!(
        m.render_failures().is_empty(),
        "IP3a: the migrated install renders, before any verb runs"
    );

    // ── The list: each profile with its server profile and its entries ───
    let rows = || -> Vec<(String, String, Vec<String>)> {
        m.instance_profiles(&alice)
            .expect("IP3a: the pool list answers")
            .into_iter()
            .map(|p| (p.id, p.server_profile, p.entries))
            .collect()
    };
    let mut got = rows();
    got.sort();
    assert_eq!(
        got,
        vec![
            (
                "default".to_string(),
                "default".to_string(),
                vec!["alpha".to_string(), "beta".to_string()]
            ),
            (
                "lab".to_string(),
                "lab".to_string(),
                vec!["alpha".to_string()]
            ),
            (
                "unassigned".to_string(),
                "default".to_string(),
                vec!["gamma".to_string()]
            ),
        ],
        "IP3a: the list answers each profile with the server profile its section \
         names and the model profiles its entry sections give it"
    );

    // ── One profile, with the pool's text as its revision ────────────────
    let one = m
        .instance_profile(&alice, "default")
        .expect("IP3a: the read");
    assert_eq!(one.id, "default", "IP3a: the row carries its id");
    assert_eq!(
        (one.server_profile.as_str(), one.entries.as_slice()),
        ("default", &["alpha".to_string(), "beta".to_string()][..]),
        "IP3a: the read answers the profile's server profile and its entries"
    );
    assert_eq!(
        one.revision,
        pool(),
        "IP3a: the revision is the text of `instance-profiles.ini` the read returned"
    );
    assert_eq!(
        m.instance_profile(&alice, "nope"),
        Err(no_profile("nope")),
        "IP3a: a profile the pool has no section for is refused, naming it"
    );

    // ── Create: refusals first, the pool untouched ───────────────────────
    let pool_before = pool();
    let record_before = record();
    for bad in ["", "Bad Id", "a_b", "../x", &"a".repeat(33)] {
        assert_eq!(
            m.create_instance_profile(&alice, bad, "lab"),
            Err(refused("instance-profile-bad-id", &[bad])),
            "IP3a: the id `{bad}` is no instance profile id"
        );
    }
    assert_eq!(
        m.create_instance_profile(&alice, "default", "lab"),
        Err(ManagerError::InstanceProfileExists("default".into())),
        "IP3a: a create writes over nothing: the name is taken"
    );
    assert_eq!(
        m.create_instance_profile(&alice, "edge", "nope"),
        Err(ManagerError::NoServerProfile("nope".into())),
        "IP3a: a profile of no server profile the node has is refused, naming it"
    );
    assert_eq!(pool(), pool_before, "IP3a: every refusal wrote the pool");
    assert_eq!(
        record(),
        record_before,
        "IP3a: every refusal wrote the record"
    );

    // The accepted create: the section, appended, every byte before it the file's.
    assert_eq!(
        m.create_instance_profile(&alice, "edge", "lab"),
        Ok(()),
        "IP3a: the create of `edge` lands"
    );
    assert_eq!(
        pool(),
        pai_core::instance_profiles::add_profile(&pool_before, "edge", "lab"),
        "IP3a: the new section is appended in the pool's shape, every byte before it \
         the file's own"
    );
    assert_eq!(record(), record_before, "IP3a: the create writes no record");
    assert!(
        m.render_failures().is_empty(),
        "IP3a: a profile no instance runs changes no render"
    );
    assert!(
        rows()
            .iter()
            .any(|r| r.0 == "edge" && r.1 == "lab" && r.2.is_empty()),
        "IP3a: the list answers the new profile with no entry: {:?}",
        rows()
    );

    // ── The entry add ────────────────────────────────────────────────────
    assert_eq!(
        m.add_instance_profile_model(&alice, "nope", "alpha"),
        Err(no_profile("nope")),
        "IP3a: an entry of no profile is refused, naming the profile"
    );
    assert_eq!(
        m.add_instance_profile_model(&alice, "default", "nope"),
        Err(refused("no-model-profile-named", &["nope"])),
        "IP3a: an entry of no model profile is refused, naming the model"
    );
    let pool_before = pool();
    assert_eq!(
        m.add_instance_profile_model(&alice, "default", "gamma"),
        Ok(Vec::new()),
        "IP3a: the entry add lands (IP3b 4 pins the running list it answers)"
    );
    assert_eq!(
        pool(),
        pai_core::instance_profiles::add_entry(&pool_before, "default", "gamma"),
        "IP3a: the empty entry section is appended, every byte before it the file's own"
    );
    assert!(
        sections(&render("default")).contains(&"gamma".to_string()),
        "IP3a: the entry add re-renders the instance with the model: {:?}",
        sections(&render("default"))
    );
    assert!(
        m.render_failures().is_empty(),
        "IP3a: the render of the entry add lands"
    );
    let pool_before = pool();
    assert_eq!(
        m.add_instance_profile_model(&alice, "default", "alpha"),
        Err(refused(
            "instance-profile-model-listed",
            &["default", "alpha"]
        )),
        "IP3b 4: the add of a model the profile already lists is refused on its own \
         conflict"
    );
    assert_eq!(
        pool(),
        pool_before,
        "IP3b 4: the refused add writes no byte (the entry's keys are its state)"
    );

    // ── The entry remove ─────────────────────────────────────────────────
    assert_eq!(
        m.remove_instance_profile_model(&alice, "nope", "alpha"),
        Err(no_profile("nope")),
        "IP3a: an entry removal of no profile is refused, naming the profile"
    );
    let pool_before = pool();
    assert!(
        sections(&pool_before).contains(&"default/beta".to_string()),
        "the entry `[default/beta]` is there to remove"
    );
    assert_eq!(
        m.remove_instance_profile_model(&alice, "default", "beta"),
        Ok(Vec::new()),
        "IP3a: the entry removal lands (IP3b 4 pins the running list it answers)"
    );
    assert_eq!(
        pool(),
        pai_core::ini::text::delete_section(&pool_before, "default/beta")
            .expect("the section the pool held"),
        "IP3a: the entry section is cut out, every other byte the file's own"
    );
    assert!(
        sections(&pool()).contains(&"default/alpha".to_string()),
        "IP3a: the removal leaves the profile's other entries: {:?}",
        sections(&pool())
    );
    assert!(
        !sections(&render("default")).contains(&"beta".to_string()),
        "IP3a: the entry removal re-renders the instance without the model: {:?}",
        sections(&render("default"))
    );
    let pool_before = pool();
    assert_eq!(
        m.remove_instance_profile_model(&alice, "unassigned", "beta"),
        Err(refused(
            "no-instance-profile-model",
            &["unassigned", "beta"]
        )),
        "IP3b 4: the remove of an entry the profile does not list is refused on its \
         own conflict"
    );
    assert_eq!(
        pool(),
        pool_before,
        "IP3b 4: the refused remove writes no byte"
    );

    // ── SetInstanceProfile: the record's pointer, the pool's text as revision
    let rec = record();
    let rev = pool();
    assert_eq!(
        m.set_instance_profile(&alice, "ghost", "default", &rev),
        Err(ManagerError::NoInstance),
        "IP3a: an instance the record has no section for is refused"
    );
    assert_eq!(
        m.set_instance_profile(&alice, "lab", "nope", &rev),
        Err(no_profile("nope")),
        "IP3a: a profile the pool has no section for is refused, naming it"
    );
    assert_eq!(
        m.set_instance_profile(&alice, "lab", "default", "STALE TEXT"),
        Err(ManagerError::Changed),
        "IP3a: a revision the pool no longer holds is the compare-and-swap refusal"
    );
    assert_eq!(record(), rec, "IP3a: every refusal wrote no record");
    assert_eq!(pool(), rev, "IP3a: the switch never writes the pool");
    assert_eq!(
        m.set_instance_profile(&alice, "lab", "default", &rev)
            .map(|_| ()),
        Ok(()),
        "IP3a: the switch of `lab` to `default` lands"
    );
    let rec = record();
    assert_eq!(
        key_of(&rec, "lab", "InstanceProfile").as_deref(),
        Some("default"),
        "IP3a: the record's section names the new instance profile"
    );
    assert_eq!(
        key_of(&rec, "lab", "Port").as_deref(),
        Some("8681"),
        "IP3a: the switch leaves the section's address alone"
    );
    assert_eq!(
        key_of(&rec, "default", "InstanceProfile").as_deref(),
        Some("default"),
        "IP3a: the switch leaves every other section alone"
    );
    assert_eq!(
        key_of(&rec, "default", "Label").as_deref(),
        Some("First"),
        "IP3a: the switch keeps the record's other keys"
    );
    // The profile `default` runs the entries `alpha` and `gamma` at this point
    // (`gamma` added above, `beta` removed above), and `lab` ran `alpha` alone.
    let lab = sections(&render("lab"));
    assert!(
        lab.contains(&"alpha".to_string()) && lab.contains(&"gamma".to_string()),
        "IP3a: the switch re-renders the instance as the profile it now runs: {lab:?}"
    );
    assert!(
        m.render_failures().is_empty(),
        "IP3a: the render of the switch lands"
    );

    // ── SetInstanceAddress: the record's address, the record's text as revision
    let rec = record();
    assert_eq!(
        m.set_instance_address(&alice, "ghost", "127.0.0.1", "8595", &rec),
        Err(ManagerError::NoInstance),
        "IP3a: an instance the record has no section for is refused"
    );
    assert_eq!(
        m.set_instance_address(&alice, "default", "127.0.0.1", "8595", "STALE TEXT"),
        Err(ManagerError::Changed),
        "IP3a: a revision the record no longer holds is the compare-and-swap refusal"
    );
    for port in ["0", "70000", "not-a-port"] {
        let answer = m
            .set_instance_address(&alice, "default", "127.0.0.1", port, &rec)
            .map(|_| ());
        match &answer {
            Err(ManagerError::Invalid(r))
                if r.key == "not-whole-number-in-range"
                    && r.values.first().map(String::as_str) == Some("Port")
                    && r.values.get(1).map(String::as_str) == Some(port) => {}
            other => panic!(
                "IP3a: the port `{port}` is refused as {other:?}, not as a \
                             port out of range"
            ),
        }
    }
    assert_eq!(
        m.set_instance_address(&alice, "default", "127.0.0.1", "8681", &rec),
        Err(ManagerError::AddressInUse("lab".into())),
        "IP3a: an address another instance holds is refused, naming it (`lab` binds \
         0.0.0.0:8681)"
    );
    assert_eq!(
        m.set_instance_address(&alice, "default", "127.0.0.1", "8590", &rec),
        Err(ManagerError::AddressInUse("private-ai-bff".into())),
        "IP3a: the web service's address is refused too, named as its own"
    );
    assert_eq!(record(), rec, "IP3a: every address refusal wrote no record");
    assert_eq!(
        m.set_instance_address(&alice, "default", "127.0.0.1", "8595", &rec)
            .map(|_| ()),
        Ok(()),
        "IP3a: a free address lands"
    );
    let rec2 = record();
    assert_eq!(
        (
            key_of(&rec2, "default", "Hostname").as_deref(),
            key_of(&rec2, "default", "Port").as_deref()
        ),
        (Some("127.0.0.1"), Some("8595")),
        "IP3a: the record carries the address the verb was given"
    );
    assert_eq!(
        key_of(&rec2, "default", "Label").as_deref(),
        Some("First"),
        "IP3a: the address write keeps the section's other keys"
    );
    assert_eq!(
        key_of(&rec2, "lab", "Port").as_deref(),
        Some("8681"),
        "IP3a: the address write leaves every other section alone"
    );
    assert!(
        m.render_failures().is_empty(),
        "IP3a: the render of the address write lands"
    );

    // The empty host removes the key.
    let rec = record();
    assert_eq!(
        m.set_instance_address(&alice, "lab", "", "8692", &rec)
            .map(|_| ()),
        Ok(()),
        "IP3a: the empty host is accepted"
    );
    let rec2 = record();
    assert!(
        !keys(&rec2, "lab").iter().any(|(k, _)| k == "Hostname"),
        "IP3a: the empty host removes `Hostname` from the section: {:?}",
        keys(&rec2, "lab")
    );
    assert_eq!(
        key_of(&rec2, "lab", "Port").as_deref(),
        Some("8692"),
        "IP3a: the clear of the host still sets the port"
    );

    // ── Delete: the profile in use first ─────────────────────────────────
    assert_eq!(
        m.delete_instance_profile(&alice, "nope"),
        Err(no_profile("nope")),
        "IP3a: a profile the pool has no section for is refused, naming it"
    );
    assert_eq!(
        m.delete_instance_profile(&alice, "default"),
        Err(ManagerError::InstanceProfileInUse(
            "default".into(),
            "default".into()
        )),
        "IP3a: the profile an instance runs is refused, naming the instance"
    );
    let pool_before = pool();
    let rec_before = record();
    assert_eq!(
        m.delete_instance_profile(&alice, "unassigned"),
        Ok(()),
        "IP3a: a profile no instance runs is removed"
    );
    let after = pai_core::ini::text::delete_section(&pool_before, "unassigned/gamma")
        .expect("the entry the pool held");
    assert_eq!(
        pool(),
        pai_core::ini::text::delete_section(&after, "unassigned").expect("the profile"),
        "IP3a: the profile and its entry sections are cut out, every other byte the \
         file's own"
    );
    assert!(
        sections(&pool()).contains(&"default/alpha".to_string()),
        "IP3a: the delete leaves every other profile and entry: {:?}",
        sections(&pool())
    );
    assert_eq!(record(), rec_before, "IP3a: the delete writes no record");
    assert!(
        rows().iter().all(|r| r.0 != "unassigned"),
        "IP3a: the list no longer answers the removed profile: {:?}",
        rows()
    );

    // ── CreateInstance: the port the manager proposes ────────────────────
    // `default` moved to 8595 and `lab` to 8692 with their own
    // `SetInstanceAddress` above, so no section of the record holds 8581: the
    // first free port at or above 8581 is 8581, and the next create takes the
    // next free one.
    assert_eq!(
        m.create_instance(&alice, ENGINE, "newone", "lab"),
        Ok(()),
        "IP3a: the create lands"
    );
    let rec = record();
    assert_eq!(
        key_of(&rec, "newone", "Port").as_deref(),
        Some("8581"),
        "IP3a: 8581 is free at this point of the phase, `default` sitting on 8595"
    );
    assert_eq!(
        key_of(&rec, "newone", "Hostname").as_deref(),
        None,
        "IP3a: the create names no host: the profile's and the launch's default bind \
         stand"
    );
    assert_eq!(
        m.create_instance(&alice, ENGINE, "newtwo", "lab"),
        Ok(()),
        "IP3a: the second create lands"
    );
    assert_eq!(
        key_of(&record(), "newtwo", "Port").as_deref(),
        Some("8582"),
        "IP3a: the next free port after 8581, which the first create took"
    );
}

/// A tree of the IP3a review-fix phases: the redirect set, the engine's config
/// directory made, and the guard the caller keeps so the files stand.
#[cfg(test)]
fn ip3fix_tree() -> tempfile::TempDir {
    let dir = tempfile::tempdir().expect("tempdir");
    crate::paths::redirect_for_tests(dir.path());
    std::fs::create_dir_all(crate::paths::engine_config_dir()).expect("engine config dir");
    dir
}

/// The manager of the IP3a review-fix phases: the fakes, the bff's account as
/// the only caller that vouches for a user, and the accounts of
/// [`fakes::FakePasswords`] (alice in `pai`, bob outside it).
#[cfg(test)]
fn ip3fix_manager(
    bff: &Caller,
) -> Manager<fakes::FakeService, fakes::FakeAuthority, fakes::FakeResolver> {
    Manager::new(
        fakes::FakeService::new(),
        fakes::FakeAuthority::new(true).bff(bff),
        fakes::FakeResolver::lan(),
    )
    .with_group(fakes::FakeGroup::new())
    .with_passwords(fakes::FakePasswords::new())
}

/// The caller of the IP3a review-fix phases: the bff carrying a member of `pai`.
#[cfg(test)]
fn ip3fix_member(bff: &Caller, name: &str) -> Requester {
    Requester {
        caller: bff.clone(),
        principal: Some(Principal(name.to_string())),
    }
}

/// The keyed refusal of the IP3a review-fix phases.
#[cfg(test)]
fn ip3fix_refusal(key: &'static str, values: &[&str]) -> ManagerError {
    ManagerError::Invalid(pai_core::checks::Refusal {
        key,
        values: values.iter().map(|v| v.to_string()).collect(),
    })
}

/// E2E phase: IP3a review D1, the empty host of `SetInstanceAddress`. The host
/// the verb is given is the address to overlay on the instance's own; an empty
/// one removes the record's `Hostname`, so the instance keeps the bind of its
/// server profile. The clash is judged on THAT binding, the profile's host under
/// the new port, never on the host an empty config implies.
#[cfg(test)]
pub(crate) fn manager_ip3a_fix_empty_host_address_e2e() {
    let _tree = ip3fix_tree();
    let sys = crate::paths::engine_config_dir();
    let bff = Caller(":1.9".into());
    let m = ip3fix_manager(&bff);
    let alice = ip3fix_member(&bff, "alice");

    std::fs::write(
        sys.join("server@wide.ini"),
        "[Server]\nHostname = 0.0.0.0\nPort = 8601\n",
    )
    .expect("server@wide.ini");
    std::fs::write(
        sys.join("server@lan.ini"),
        "[Server]\nHostname = 10.0.0.5\nPort = 8681\n",
    )
    .expect("server@lan.ini");
    let seed = "[wide-one]\nServerProfile = wide\nPort = 9001\n\n[lan-one]\nServerProfile = lan\n";
    std::fs::write(sys.join(INSTANCES_FILE), seed).expect(INSTANCES_FILE);

    // `lan-one` names no address, so it binds its profile's 10.0.0.5:8681.
    // `wide-one`'s profile binds 0.0.0.0, every address of the node: the empty
    // host leaves that bind standing, so the port it is given lands on 10.0.0.5
    // too, the address `lan-one` holds.
    let got = m.set_instance_address(&alice, "wide-one", "", "8681", seed);
    assert_eq!(
        got,
        Err(ManagerError::AddressInUse("lan-one".to_string())),
        "IP3a review D1: the empty host keeps `wide-one` on its profile's 0.0.0.0, so \
         8681 is `lan-one`'s address: the refusal names it"
    );
    assert_eq!(
        std::fs::read(sys.join(INSTANCES_FILE)).expect("record"),
        seed.as_bytes(),
        "IP3a review D1: the refusal wrote the record"
    );
}

/// E2E phase: IP3a review D2a, the port `CreateInstance` proposes skips the
/// address of every instance of the record, the ones naming no `Port` included:
/// they sit on their server profile's port, the address their launch binds.
#[cfg(test)]
pub(crate) fn manager_ip3a_fix_port_of_recordless_e2e() {
    let _tree = ip3fix_tree();
    let sys = crate::paths::engine_config_dir();
    let bff = Caller(":1.9".into());
    let m = ip3fix_manager(&bff);
    let alice = ip3fix_member(&bff, "alice");

    std::fs::write(
        sys.join("server@default.ini"),
        "[Server]\nHostname = 127.0.0.1\nPort = 8581\n",
    )
    .expect("server@default.ini");
    std::fs::write(
        sys.join("server@lab.ini"),
        "[Server]\nHostname = 0.0.0.0\nPort = 8681\n",
    )
    .expect("server@lab.ini");
    std::fs::write(
        sys.join("server@spare.ini"),
        "[Server]\nHostname = 127.0.0.1\nPort = 8699\n",
    )
    .expect("server@spare.ini");
    let seed = "[default]\nServerProfile = default\n\n[lab]\nServerProfile = lab\n";
    std::fs::write(sys.join(INSTANCES_FILE), seed).expect(INSTANCES_FILE);

    assert_eq!(
        m.create_instance(&alice, ENGINE, "fresh", "spare"),
        Ok(()),
        "IP3a review D2a: the create of `fresh` lands: its profile binds 127.0.0.1:8699, \
         which nothing holds"
    );
    let record = std::fs::read_to_string(sys.join(INSTANCES_FILE)).expect(INSTANCES_FILE);
    assert_eq!(
        record_key(&record, "fresh", "Port").as_deref(),
        Some("8582"),
        "IP3a review D2a: `default` and `lab` name no `Port` and sit on their profiles' \
         8581 and 8681, and with no `bff.ini` the web service holds 127.0.0.1:8590: the \
         first port of the range the node holds nothing of is 8582"
    );
}

/// E2E phase: IP3a review D2b, the port `CreateInstance` proposes skips the web
/// service's own port, the one `bff.ini` names.
#[cfg(test)]
pub(crate) fn manager_ip3a_fix_port_of_bff_e2e() {
    let _tree = ip3fix_tree();
    let sys = crate::paths::engine_config_dir();
    let bff = Caller(":1.9".into());
    let m = ip3fix_manager(&bff);
    let alice = ip3fix_member(&bff, "alice");

    std::fs::write(
        sys.join("server@a.ini"),
        "[Server]\nHostname = 127.0.0.1\nPort = 8681\n",
    )
    .expect("server@a.ini");
    std::fs::write(
        sys.join("server@b.ini"),
        "[Server]\nHostname = 127.0.0.1\nPort = 8699\n",
    )
    .expect("server@b.ini");
    let seed = "[one]\nServerProfile = a\nPort = 8581\n";
    std::fs::write(sys.join(INSTANCES_FILE), seed).expect(INSTANCES_FILE);
    let framework = crate::paths::system_config_dir();
    std::fs::create_dir_all(&framework).expect("framework config dir");
    std::fs::write(
        framework.join("bff.ini"),
        "[Bff]\nListen = 127.0.0.1:8582\n",
    )
    .expect("bff.ini");

    assert_eq!(
        m.create_instance(&alice, ENGINE, "two", "b"),
        Ok(()),
        "IP3a review D2b: the create of `two` lands: its profile binds 127.0.0.1:8699, \
         which nothing holds"
    );
    let record = std::fs::read_to_string(sys.join(INSTANCES_FILE)).expect(INSTANCES_FILE);
    assert_eq!(
        record_key(&record, "two", "Port").as_deref(),
        Some("8583"),
        "IP3a review D2b: the record holds 8581 and `bff.ini` puts the web service on \
         8582: the create proposes the next port, 8583"
    );
}

/// E2E phase: IP3a review D3, `CreateInstance` refused when the node holds every
/// port of the range: keyed `no-free-port`, naming the range's two ends, and the
/// walk ENDS, it does not run past the last port for ever.
#[cfg(test)]
pub(crate) fn manager_ip3a_fix_no_free_port_e2e() {
    let _tree = ip3fix_tree();
    let sys = crate::paths::engine_config_dir();
    let bff = Caller(":1.9".into());
    // The node's whole range is 57 000 ports, too many to seed: the seam narrows
    // it to the two ports held here, and the constants pin the range the create
    // of a real node walks.
    let m = ip3fix_manager(&bff).with_port_search(8581, 8582);
    let alice = ip3fix_member(&bff, "alice");

    std::fs::write(
        sys.join("server@p.ini"),
        "[Server]\nHostname = 127.0.0.1\nPort = 8581\n",
    )
    .expect("server@p.ini");
    std::fs::write(
        sys.join("server@q.ini"),
        "[Server]\nHostname = 127.0.0.1\nPort = 8699\n",
    )
    .expect("server@q.ini");
    let seed = "[one]\nServerProfile = p\nPort = 8581\n\n[two]\nServerProfile = q\nPort = 8582\n";
    std::fs::write(sys.join(INSTANCES_FILE), seed).expect(INSTANCES_FILE);
    let listing = || -> Vec<String> {
        let mut names: Vec<String> = std::fs::read_dir(&sys)
            .expect("listing")
            .flatten()
            .map(|entry| entry.file_name().to_string_lossy().into_owned())
            .collect();
        names.sort();
        names
    };
    let before = listing();

    let (tx, rx) = std::sync::mpsc::channel();
    std::thread::spawn(move || {
        let _ = tx.send(m.create_instance(&alice, ENGINE, "three", "q"));
    });
    let got = match rx.recv_timeout(std::time::Duration::from_secs(10)) {
        Ok(answer) => answer,
        Err(std::sync::mpsc::RecvTimeoutError::Timeout) => panic!(
            "IP3a review D3: the create of a node holding every port of 8581..=8582 was \
             still walking after 10 s: it must stop at the end of the range and refuse"
        ),
        Err(e) => panic!("IP3a review D3: the thread carrying the create died: {e}"),
    };
    assert_eq!(
        got.expect_err("IP3a review D3: a create with no port left is refused"),
        ip3fix_refusal("no-free-port", &["8581", "8582"]),
        "IP3a review D3: the create of a node holding every port of its range is refused \
         with `no-free-port`, naming the range it walked"
    );
    assert_eq!(
        std::fs::read(sys.join(INSTANCES_FILE)).expect("record"),
        seed.as_bytes(),
        "IP3a review D3: the refusal wrote the record"
    );
    assert_eq!(
        listing(),
        before,
        "IP3a review D3: the refusal created a file: every check answers before a byte \
         is written, the port included"
    );
    assert_eq!(
        (FIRST_PROPOSED_PORT, LAST_PROPOSED_PORT),
        (8581, 65535),
        "IP3a review D3: the range a real node's create walks is 8581 to 65535, the two \
         ends the refusal names"
    );
}

/// E2E phase: IP3a review D4, a host carrying a control character is refused
/// before any write: it is written as the value of the record's `Hostname` key,
/// so a newline forges a line of the file.
#[cfg(test)]
pub(crate) fn manager_ip3a_fix_host_control_char_e2e() {
    let _tree = ip3fix_tree();
    let sys = crate::paths::engine_config_dir();
    let bff = Caller(":1.9".into());
    let m = ip3fix_manager(&bff);
    let alice = ip3fix_member(&bff, "alice");

    std::fs::write(
        sys.join("server@wide.ini"),
        "[Server]\nHostname = 0.0.0.0\nPort = 8601\n",
    )
    .expect("server@wide.ini");
    let seed = "[wide-one]\nServerProfile = wide\nPort = 9001\n";
    std::fs::write(sys.join(INSTANCES_FILE), seed).expect(INSTANCES_FILE);

    let got = m.set_instance_address(
        &alice,
        "wide-one",
        "127.0.0.1\nServerProfile = x",
        "8699",
        seed,
    );
    assert!(
        matches!(got, Err(ManagerError::UnresolvableHost(_))),
        "IP3a review D4: a host carrying a newline is no host of this node, and the verb \
         answers before the record is touched: {got:?}"
    );
    assert_eq!(
        std::fs::read(sys.join(INSTANCES_FILE)).expect("record"),
        seed.as_bytes(),
        "IP3a review D4: the refusal wrote the record"
    );
}

/// E2E phase: IP3a review D5, the six IP3a write verbs are refused at the
/// manager for a principal outside `pai`, and nothing of the tree moves. The
/// same six calls carried by a member land, so each refusal is the gate's and not
/// a later check's.
#[cfg(test)]
pub(crate) fn manager_ip3a_fix_principal_gate_e2e() {
    let _tree = ip3fix_tree();
    let sys = crate::paths::engine_config_dir();
    let bff = Caller(":1.9".into());
    let m = ip3fix_manager(&bff);
    let alice = ip3fix_member(&bff, "alice");
    let bob = ip3fix_member(&bff, "bob");

    std::fs::write(
        sys.join("server@default.ini"),
        "[Server]\nHostname = 127.0.0.1\nPort = 8581\n",
    )
    .expect("server@default.ini");
    std::fs::write(
        sys.join("server@lab.ini"),
        "[Server]\nHostname = 127.0.0.1\nPort = 8681\n",
    )
    .expect("server@lab.ini");
    std::fs::write(
        sys.join(INSTANCES_FILE),
        "[default]\nServerProfile = default\nPort = 8581\n",
    )
    .expect(INSTANCES_FILE);
    std::fs::write(sys.join("models.ini"), "[alpha]\nmodel = alpha.gguf\n").expect("models.ini");
    assert_eq!(
        m.create_instance_profile(&alice, "spare", "lab"),
        Ok(()),
        "IP3a review D5: the seed, a spare instance profile of the pool"
    );
    let pool = std::fs::read_to_string(sys.join(INSTANCE_PROFILES_FILE)).expect("pool");
    let record = std::fs::read_to_string(sys.join(INSTANCES_FILE)).expect(INSTANCES_FILE);
    let pool_bytes = std::fs::read(sys.join(INSTANCE_PROFILES_FILE)).expect("pool bytes");
    let record_bytes = std::fs::read(sys.join(INSTANCES_FILE)).expect("record bytes");

    // Every call answers on its own merits for a member (the round below), so a
    // refusal here is the principal gate and nothing else.
    let answers: Vec<(&str, Result<Vec<String>, ManagerError>)> = vec![
        (
            "create_instance_profile",
            m.create_instance_profile(&bob, "brand", "lab")
                .map(|()| Vec::new()),
        ),
        (
            "delete_instance_profile",
            m.delete_instance_profile(&bob, "spare")
                .map(|()| Vec::new()),
        ),
        (
            "add_instance_profile_model",
            m.add_instance_profile_model(&bob, "spare", "alpha"),
        ),
        (
            "remove_instance_profile_model",
            m.remove_instance_profile_model(&bob, "spare", "alpha"),
        ),
        // The verbs that answer no list, as the same shape.
        (
            "set_instance_profile",
            m.set_instance_profile(&bob, "default", "spare", &pool)
                .map(|_| Vec::new()),
        ),
        (
            "set_instance_address",
            m.set_instance_address(&bob, "default", "127.0.0.1", "8699", &record)
                .map(|_| Vec::new()),
        ),
    ];
    for (verb, answer) in answers {
        assert_eq!(
            answer,
            Err(ManagerError::NotAuthorized),
            "IP3a review D5: `{verb}` carried by `bob`, who is no member of `pai`, is \
             refused at the manager"
        );
    }
    assert_eq!(
        std::fs::read(sys.join(INSTANCE_PROFILES_FILE)).expect("pool"),
        pool_bytes,
        "IP3a review D5: the six refusals wrote the pool"
    );
    assert_eq!(
        std::fs::read(sys.join(INSTANCES_FILE)).expect("record"),
        record_bytes,
        "IP3a review D5: the six refusals wrote the record"
    );

    // The teeth: the same six verbs, carried by a member, all land.
    let member: Vec<(&str, Result<Vec<String>, ManagerError>)> = vec![
        (
            "add_instance_profile_model",
            m.add_instance_profile_model(&alice, "spare", "alpha"),
        ),
        (
            "remove_instance_profile_model",
            m.remove_instance_profile_model(&alice, "spare", "alpha"),
        ),
        (
            "create_instance_profile",
            m.create_instance_profile(&alice, "brand", "lab")
                .map(|_| Vec::new()),
        ),
        (
            "delete_instance_profile",
            m.delete_instance_profile(&alice, "brand")
                .map(|_| Vec::new()),
        ),
    ];
    for (verb, answer) in member {
        assert!(
            answer.is_ok(),
            "IP3a review D5: `{verb}` carried by `alice`, a member of `pai`, lands:              {answer:?}"
        );
    }
    assert_eq!(
        m.set_instance_profile(
            &alice,
            "default",
            "spare",
            &std::fs::read_to_string(sys.join(INSTANCE_PROFILES_FILE)).expect("pool")
        )
        .map(|_| ()),
        Ok(()),
        "IP3a review D5: the switch carried by a member lands"
    );
    assert_eq!(
        m.set_instance_address(
            &alice,
            "default",
            "127.0.0.1",
            "8699",
            &std::fs::read_to_string(sys.join(INSTANCES_FILE)).expect("record")
        )
        .map(|_| ()),
        Ok(()),
        "IP3a review D5: the address carried by a member lands"
    );
}
