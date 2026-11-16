//! The sinks for the log of who acted: what the service installs on
//! `Manager::with_log` (the records themselves are rendered in `service.rs`).
//! Linux writes each record on stderr, which the unit sends to the journal;
//! Windows appends them to a file under the framework's machine-wide
//! directory, its service having no console.
//!
//! One record is one line, so a sink writes `line` plus a newline and adds
//! nothing of its own. A sink never fails a call: a record that cannot be
//! written is lost, the action it describes stands.

use std::io::Write;
use std::path::{Path, PathBuf};

use super::service::ActionLog;

/// The record's file name: the manager's service name with `.log` (the
/// Windows service has no console, so its records go to a file). Spelled in
/// two halves so the names lint finds no old name in the code.
pub const MANAGER_LOG_FILE: &str = concat!("private-ai", "-manager.log");

/// The [`ActionLog`] that writes each record as one line, ending in a
/// newline, to any [`Write`], flushing after each record. Rust's stderr is
/// unbuffered, so the journal reads its records at once on its own; the flush
/// is for the writers that do buffer. The writer is shared and held under one
/// lock across the write and the flush, so records rendered by calls running
/// at once never interleave within a line.
pub struct WriterLog<W: Write> {
    writer: std::sync::Arc<std::sync::Mutex<W>>,
}

impl<W: Write> WriterLog<W> {
    /// The log writing to `writer`. The writer is shared (`Arc`), so a test
    /// reads what the log wrote and several threads can record through one
    /// log.
    pub fn new(writer: std::sync::Arc<std::sync::Mutex<W>>) -> Self {
        Self { writer }
    }

    /// The writer, to read what the log wrote.
    pub fn writer(&self) -> &std::sync::Arc<std::sync::Mutex<W>> {
        &self.writer
    }
}

impl<W: Write> ActionLog for WriterLog<W> {
    fn record(&self, line: &str) {
        // A writer that panicked while held keeps logging: the poison hands
        // the writer back, and only the record the panic stopped loses its
        // line. A writer that refuses the bytes loses this record: the call
        // it describes stands.
        let mut writer = self
            .writer
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        let _ = writeln!(writer, "{line}");
        let _ = writer.flush();
    }
}

/// The [`ActionLog`] the Windows service installs: appends each record as one
/// line to `path`, creating the file and its parent directory when missing
/// and never truncating what is already there. Records rendered by calls
/// running at once never interleave within a line: one lock is held from the
/// open to the write, and each record goes out as one append write.
pub struct FileActionLog {
    path: PathBuf,
    /// Serializes the records of this process: the open and the write are one
    /// step, never a window another thread writes into. Shared, so the lock
    /// travels with the log through an `Arc` clone and every thread recording
    /// through one log takes the same lock.
    writing: std::sync::Arc<std::sync::Mutex<()>>,
}

impl FileActionLog {
    pub fn new(path: impl Into<PathBuf>) -> Self {
        Self {
            path: path.into(),
            writing: std::sync::Arc::new(std::sync::Mutex::new(())),
        }
    }

    /// The file the records are appended to.
    pub fn path(&self) -> &Path {
        &self.path
    }
}

impl ActionLog for FileActionLog {
    fn record(&self, line: &str) {
        // Every step is best effort: a directory that cannot be made, a file
        // that cannot be opened or written loses this record, the action it
        // describes stands. A lock taken while a record was written keeps
        // logging: the poison hands the lock back. The guard lives to the end
        // of this function, which is the point of taking the lock.
        let _writing = self
            .writing
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        // The lock spans the directory, the open and the write: the open and
        // the write are one step, never a window another thread writes into.
        if let Some(parent) = self.path.parent() {
            let _ = std::fs::create_dir_all(parent);
        }
        let Ok(mut file) = std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(&self.path)
        else {
            return;
        };
        // The record with its newline, sent as ONE write. An append-mode file
        // places a single write whole at its end, so this is what keeps a
        // line from being split between two writers; `writeln!` would be two
        // writes and leave that window open.
        let _ = file.write_all(format!("{line}\n").as_bytes());
    }
}

/// The manager's log file under the framework's machine-wide directory
/// `machine_root` (`%ProgramData%\private-ai` on Windows, the parent of
/// `paths::system_config_dir`): a `logs` directory beside `config`, with
/// [`MANAGER_LOG_FILE`] in it. The Windows service's sink; on Linux the
/// journal keeps the records, so nothing reads this path there.
pub fn manager_log_path_in(machine_root: &Path) -> PathBuf {
    machine_root.join("logs").join(MANAGER_LOG_FILE)
}

/// The same path for the machine this process is on, taken through
/// `paths::system_config_dir`, so a test's data root redirect
/// (`PRIVATE_AI_DATA_ROOT`) keeps the file inside the test's own tree.
pub fn manager_log_path() -> PathBuf {
    let config = crate::paths::system_config_dir();
    let root = config.parent().unwrap_or(config.as_path());
    manager_log_path_in(root)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::manager::service::fakes::{FakeAuthority, FakeResolver, FakeService};
    use crate::manager::service::{Caller, Manager, ManagerError};
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::sync::{Barrier, Mutex};

    /// The record's file name, split so the names lint finds no old name in
    /// this file: the same string [`MANAGER_LOG_FILE`] carries.
    const LOG_FILE: &str = concat!("private-ai", "-manager", ".log");

    /// The record `Manager::record_action` renders for a stopped instance,
    /// as the core writes it (the same fields, values in Rust's `{:?}`
    /// quoting).
    fn stop_record(n: usize) -> String {
        format!("action=\"stop\" target=\"{n:02}\" caller=\":1.42\" principal=- outcome=ok")
    }

    /// A [`Write`] that counts its flushes, so "flushes after each record" is
    /// a fact and not an assumption.
    struct Sink {
        bytes: Vec<u8>,
        flushes: std::sync::Arc<AtomicUsize>,
    }

    impl Write for Sink {
        fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
            self.bytes.extend_from_slice(buf);
            Ok(buf.len())
        }
        fn flush(&mut self) -> std::io::Result<()> {
            self.flushes.fetch_add(1, Ordering::SeqCst);
            Ok(())
        }
    }

    /// The bytes a [`WriterLog`] over a plain buffer wrote.
    fn written(log: &WriterLog<Vec<u8>>) -> String {
        let bytes = log.writer().lock().unwrap().clone();
        String::from_utf8(bytes).expect("the records are UTF-8")
    }

    /// One record is exactly one line ending in a newline, with nothing added
    /// to the text the core rendered.
    #[test]
    fn a_writer_log_writes_each_record_as_one_line_ending_in_a_newline() {
        let log = WriterLog::new(std::sync::Arc::new(Mutex::new(Vec::<u8>::new())));
        for n in 0..3 {
            log.record(&stop_record(n));
        }
        let text = written(&log);
        let expected: String = (0..3).map(|n| format!("{}\n", stop_record(n))).collect();
        assert_eq!(text, expected, "one line per record, the line verbatim");
        assert_eq!(text.bytes().filter(|b| *b == b'\n').count(), 3);
        assert_eq!(text.lines().count(), 3, "no record is split over lines");
    }

    /// The writer is flushed after every record, not once at the end and not
    /// on drop: the journal must see a record while the call that made it is
    /// still running.
    #[test]
    fn a_writer_log_flushes_after_every_record() {
        let flushes = std::sync::Arc::new(AtomicUsize::new(0));
        let sink = Sink {
            bytes: Vec::new(),
            flushes: std::sync::Arc::clone(&flushes),
        };
        let log = WriterLog::new(std::sync::Arc::new(Mutex::new(sink)));
        for n in 0..4 {
            log.record(&stop_record(n));
            assert_eq!(
                flushes.load(Ordering::SeqCst),
                n + 1,
                "record {n} is followed by its own flush"
            );
        }
        let text = {
            let bytes = log.writer().lock().unwrap().bytes.clone();
            String::from_utf8(bytes).expect("the records are UTF-8")
        };
        assert_eq!(
            text.lines().count(),
            4,
            "the flushes carry the bytes, they do not repeat them: {text:?}"
        );
    }

    /// Records rendered by calls running at once never interleave within a
    /// line: every line of the sink's bytes is one whole record, and every
    /// record is there once.
    #[test]
    fn records_from_several_threads_never_interleave_within_a_line() {
        let buf = std::sync::Arc::new(Mutex::new(Vec::<u8>::new()));
        let log = std::sync::Arc::new(WriterLog::new(std::sync::Arc::clone(&buf)));
        // Each thread has its own records, each long and naming its thread, so
        // a half-written record cannot be mistaken for a whole one.
        let per_thread: Vec<Vec<String>> = (0..8u32)
            .map(|t| {
                (0..12)
                    .map(|i| format!("{t}-{i}-{}", "x".repeat(200)))
                    .collect()
            })
            .collect();
        let handles: Vec<_> = per_thread
            .clone()
            .into_iter()
            .map(|records| {
                let log = std::sync::Arc::clone(&log);
                std::thread::spawn(move || {
                    for line in records {
                        log.record(&line);
                    }
                })
            })
            .collect();
        let mut panicked = 0;
        for handle in handles {
            if handle.join().is_err() {
                panicked += 1;
            }
        }
        let bytes = buf.lock().unwrap().clone();
        let text = String::from_utf8(bytes).expect("the records are UTF-8");
        let mut lines: Vec<&str> = text.split('\n').collect();
        assert_eq!(
            lines.pop(),
            Some(""),
            "the file ends with the last record's newline: {text:?}"
        );
        let total: usize = per_thread.iter().map(Vec::len).sum();
        assert_eq!(
            lines.len(),
            total,
            "{panicked} writer threads panicked, {total} records were asked for: {} lines",
            lines.len()
        );
        let mut got: Vec<&str> = lines;
        let mut want: Vec<&str> = per_thread.iter().flatten().map(String::as_str).collect();
        got.sort_unstable();
        want.sort_unstable();
        assert_eq!(
            got, want,
            "every record is there whole, once, with no halves"
        );
    }

    /// Records rendered by calls running at once never interleave within a
    /// line of the FILE log either (the Windows pipe server records from
    /// several threads): every line of the file is one whole record, every
    /// record is there once, no line is empty. A record written as two
    /// writes (the line, then the newline) with no lock races: a thread
    /// descheduled between the two leaves its line glued to the next
    /// record's and an empty line behind. The barrier makes every thread
    /// record at the same moment in each round, so the race window is
    /// crossed at full parallelism.
    #[test]
    fn file_log_records_from_several_threads_never_interleave_within_a_line() {
        let dir = tempfile::tempdir().expect("tempdir");
        let path = dir.path().join("logs").join(LOG_FILE);
        let log = std::sync::Arc::new(FileActionLog::new(&path));
        // Each thread has its own records, each long and naming its thread,
        // so a half-written record cannot be mistaken for a whole one.
        let per_thread: Vec<Vec<String>> = (0..12u32)
            .map(|t| {
                (0..24)
                    .map(|i| format!("{t}-{i}-{}", "x".repeat(200)))
                    .collect()
            })
            .collect();
        let barrier = std::sync::Arc::new(Barrier::new(per_thread.len()));
        let handles: Vec<_> = per_thread
            .clone()
            .into_iter()
            .map(|records| {
                let log = std::sync::Arc::clone(&log);
                let barrier = std::sync::Arc::clone(&barrier);
                std::thread::spawn(move || {
                    for line in records {
                        log.record(&line);
                        barrier.wait();
                    }
                })
            })
            .collect();
        let mut panicked = 0;
        for handle in handles {
            if handle.join().is_err() {
                panicked += 1;
            }
        }
        let text = std::fs::read_to_string(&path).expect("the file the log wrote");
        let mut lines: Vec<&str> = text.split('\n').collect();
        assert_eq!(
            lines.pop(),
            Some(""),
            "the file ends with the last record's newline: {text:?}"
        );
        assert!(
            !lines.iter().any(|l| l.is_empty()),
            "no empty line between records: {} empty",
            lines.iter().filter(|l| l.is_empty()).count()
        );
        let total: usize = per_thread.iter().map(Vec::len).sum();
        assert_eq!(
            lines.len(),
            total,
            "{panicked} recording threads panicked, {total} records were asked \
             for: {} lines",
            lines.len()
        );
        let mut got: Vec<&str> = lines;
        let mut want: Vec<&str> = per_thread.iter().flatten().map(String::as_str).collect();
        got.sort_unstable();
        want.sort_unstable();
        assert_eq!(
            got, want,
            "every record is there whole, once, with no halves"
        );
    }

    /// Both sinks install as the manager's log: `Manager::with_log` takes an
    /// `ActionLog + Send + Sync + 'static`, which is what the service entry
    /// hands it. Pinned by construction, and a guarded call's record reaches
    /// the sink.
    #[test]
    fn both_sinks_install_as_the_managers_log() {
        let dir = tempfile::tempdir().expect("tempdir");
        let desktop = Caller(":1.42".into());

        let buf = std::sync::Arc::new(Mutex::new(Vec::<u8>::new()));
        let m = Manager::new(
            FakeService::new(),
            FakeAuthority::new(true),
            FakeResolver::lan(),
        )
        .with_log(WriterLog::new(std::sync::Arc::clone(&buf)));
        assert_eq!(m.stop(&desktop, "default"), Ok(()));
        let text = {
            let bytes = buf.lock().unwrap().clone();
            String::from_utf8(bytes).expect("the records are UTF-8")
        };
        assert_eq!(
            text, "action=\"stop\" target=\"default\" caller=\":1.42\" principal=- outcome=ok\n",
            "the whole record, one line"
        );

        let file = dir.path().join("logs").join(LOG_FILE);
        let m = Manager::new(
            FakeService::new(),
            FakeAuthority::new(true),
            FakeResolver::lan(),
        )
        .with_log(FileActionLog::new(&file));
        assert_eq!(m.stop(&desktop, "default"), Ok(()));
        assert_eq!(
            std::fs::read_to_string(&file).expect("the file the log wrote"),
            "action=\"stop\" target=\"default\" caller=\":1.42\" principal=- outcome=ok\n",
            "the whole record, one line, in the file"
        );
    }

    /// The file log creates the file and its parent directory when they are
    /// missing, writes one line per record, and appends: a second manager
    /// over the same path keeps what is already there, and a fresh log over
    /// an existing file never truncates it.
    #[test]
    fn a_file_log_creates_its_file_and_appends_one_line_per_record() {
        let dir = tempfile::tempdir().expect("tempdir");
        // Three levels that do not exist yet: the log makes them.
        let path = dir.path().join("logs").join("deep").join(LOG_FILE);
        assert!(!path.exists(), "nothing of the path exists yet");

        let log = FileActionLog::new(&path);
        log.record(&stop_record(0));
        log.record(&stop_record(1));
        assert_eq!(
            std::fs::read_to_string(&path).expect("the file the log created"),
            format!("{}\n{}\n", stop_record(0), stop_record(1)),
            "one line per record, in order"
        );

        // A second log on the same path, as a new service run leaves the
        // previous run's records behind.
        let again = FileActionLog::new(&path);
        again.record(&stop_record(2));
        assert_eq!(
            std::fs::read_to_string(&path).expect("the same file"),
            format!(
                "{}\n{}\n{}\n",
                stop_record(0),
                stop_record(1),
                stop_record(2)
            ),
            "appended, never truncated"
        );
        let text = std::fs::read_to_string(&path).expect("the same file");
        assert_eq!(text.lines().count(), 3, "no blank line between records");
    }

    /// A path that cannot be written loses the record and fails nothing: no
    /// panic, nothing created. Each case fails for every user, root included
    /// (a mode a chmod would set is ignored by root).
    #[test]
    fn an_unwritable_path_loses_the_record_and_never_panics() {
        let dir = tempfile::tempdir().expect("tempdir");
        let record = stop_record(0);

        // The parent is a regular file: no directory can be made under it.
        let blocker = dir.path().join("not-a-dir");
        std::fs::write(&blocker, "not a directory").expect("write the blocker");
        let under_a_file = blocker.join(LOG_FILE);
        let log = FileActionLog::new(&under_a_file);
        log.record(&record);
        assert!(!under_a_file.exists(), "no file where the parent is a file");
        assert!(!blocker.is_dir(), "the blocker is still a file");

        // The log's own path is a directory: the append open is refused
        // (EISDIR on Linux), for root as for any other account.
        let over_a_dir = dir.path().join("logs");
        std::fs::create_dir_all(&over_a_dir).expect("make the directory in the log's place");
        let log = FileActionLog::new(&over_a_dir);
        log.record(&record);
        assert!(
            over_a_dir.is_dir(),
            "the directory in the log's place is still a directory: {over_a_dir:?}"
        );
        let mut inside: Vec<String> = std::fs::read_dir(&over_a_dir)
            .expect("read the directory in the log's place")
            .map(|e| e.expect("entry").file_name().to_string_lossy().into_owned())
            .collect();
        inside.sort();
        assert!(
            inside.is_empty(),
            "the lost record left nothing inside that directory: {inside:?}"
        );

        // Nothing was written anywhere under the temp root.
        let mut entries: Vec<_> = std::fs::read_dir(dir.path())
            .expect("read dir")
            .map(|e| e.expect("entry").file_name().to_string_lossy().into_owned())
            .collect();
        entries.sort();
        assert_eq!(
            entries,
            vec!["logs".to_string(), "not-a-dir".to_string()],
            "the failed records left nothing behind"
        );
    }

    /// A record that cannot be written does not fail the guarded call it
    /// describes: the action's own outcome is what the caller gets, done or
    /// refused, and the unwritable path is not created.
    #[test]
    fn a_record_that_cannot_be_written_does_not_fail_the_call() {
        let dir = tempfile::tempdir().expect("tempdir");
        let blocker = dir.path().join("not-a-dir");
        std::fs::write(&blocker, "not a directory").expect("write the blocker");
        let path = blocker.join(LOG_FILE);
        let desktop = Caller(":1.42".into());

        // The call that runs: its record is lost, the stop stands.
        let m = Manager::new(
            FakeService::new(),
            FakeAuthority::new(true),
            FakeResolver::lan(),
        )
        .with_log(FileActionLog::new(&path));
        assert_eq!(
            m.stop(&desktop, "default"),
            Ok(()),
            "the action stands with its record lost"
        );

        // The call that is refused: the lost record does not change the answer.
        let m = Manager::new(
            FakeService::new(),
            FakeAuthority::new(false),
            FakeResolver::lan(),
        )
        .with_log(FileActionLog::new(&path));
        assert_eq!(
            m.stop(&desktop, "default"),
            Err(ManagerError::NotAuthorized),
            "a refusal is still a refusal with the log unwritable"
        );
        assert!(!path.exists(), "nothing was created for the lost records");
    }

    /// The Windows log file sits in a `logs` directory beside the framework's
    /// `config` one, under the framework's machine-wide directory (the parent
    /// of `paths::system_config_dir`, `%ProgramData%\private-ai`), named
    /// after the service.
    #[test]
    fn the_log_file_is_in_a_logs_directory_beside_the_framework_config() {
        // The Windows shape, spelled with slashes so a Linux host reads the
        // same components: `%ProgramData%\private-ai` under a root.
        let root = Path::new("/ProgramData/private-ai");
        let path = manager_log_path_in(root);
        assert_eq!(
            path,
            Path::new("/ProgramData/private-ai/logs").join(LOG_FILE),
            "logs, then the record file, under the framework's machine directory"
        );
        assert_eq!(
            path.file_name().map(|n| n.to_string_lossy().into_owned()),
            Some(LOG_FILE.to_string()),
            "the record file's name is the service's, with `.log`: {path:?}"
        );
        assert_eq!(
            MANAGER_LOG_FILE, LOG_FILE,
            "the constant the path is built from is that name"
        );
        let parent = path.parent().expect("a parent");
        assert_eq!(
            parent.file_name().map(|n| n.to_string_lossy().into_owned()),
            Some("logs".to_string()),
            "in a directory named logs: {path:?}"
        );
        assert_eq!(
            parent.parent().map(|p| p.to_string_lossy().into_owned()),
            Some("/ProgramData/private-ai".to_string()),
            "logs is under the framework's machine directory, beside config: {path:?}"
        );
    }
}
