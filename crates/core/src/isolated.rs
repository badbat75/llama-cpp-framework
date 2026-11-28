//! The isolated data-root test helper (TS2a, TS2a2): ARCHITECTURE.md "Isolated
//! data-root tests (TS2, decided 2026-11-28, test-only)".
//!
//! The data root is the process-wide `PRIVATE_AI_DATA_ROOT`, read again on
//! every call, so a test that redirects it must run alone in its process.
//! `isolated` gives such a test its own process: the parent spawns this
//! test binary again with `--exact <its own test path>` and a marker naming it,
//! the child sees the marker, redirects the data root to a temp dir of its own
//! and runs the body. libtest runs the parents in parallel, so every
//! redirected test has its own root, runs alone, and fails on its own.
//!
//! The shape:
//! - the child, when the marker equals `test_path`: `paths::redirect_for_tests`
//!   on a temp dir of its own made inside the base dir the PARENT made and
//!   named in `PAI_TEST_ISOLATED_ROOT`, then `body()`. The child's guard
//!   removes its dir when the body ends, the parent's guard removes the base
//!   when `isolated` returns: a killed child runs no destructor, so the base
//!   is what keeps a kill from leaving a tree in the temp dir.
//! - the parent: spawn `std::env::current_exe()` with `--exact <test_path>`,
//!   `--include-ignored` (a body that runs only as a child is `#[ignore]`d, so
//!   it fails nothing in the suite on its own) and `--nocapture`, with the
//!   marker set to `test_path`, forwarding the child's output, and fail unless
//!   the child exited 0 AND its output reports exactly one test passed: a
//!   mistyped path makes libtest run 0 tests and exit 0, and the failure
//!   answers with the child's counts, so the reader sees that no test ran.
//! - the bound: the parent waits for its child no longer than the bound, then
//!   kills it and fails naming the test and the bound. Without a bound nothing
//!   ends the wait, so a hung child hangs the suite. On Unix the kill reaches
//!   the child's whole process group (the child leads one), so a process the
//!   child started dies with it, and the parent's two pipe reads are bounded
//!   too: nothing the child leaves behind can hold the call open.

use std::io::Read;
use std::path::Path;
use std::process::{Child, Stdio};
use std::sync::mpsc;
use std::time::{Duration, Instant};

/// The marker `isolated` sets on the child it spawns, naming the test path it
/// re-runs there. A test of another path run in that process (by a filter, say)
/// sees a marker that is not its own and so never redirects its root.
const MARKER: &str = "PAI_TEST_ISOLATED_CHILD";

/// The base dir the parent makes for its child and names in its environment:
/// the child makes its data root inside it, and the parent removes the whole
/// base when the call returns. See the module header.
const ROOT: &str = "PAI_TEST_ISOLATED_ROOT";

/// The name a test sets to give its own calls a bound of their own, in
/// milliseconds. Read by the parent alone, so a test of the bound sets it and
/// the child it spawns inherits it untouched.
const BOUND_MS: &str = "PAI_TEST_ISOLATED_BOUND_MS";

/// The bound of a call that names none. It has to fit the slowest phase of the
/// crate with the most of them: the manager runs all of its phases through
/// this helper, the parents in parallel, so a phase runs beside
/// the whole suite. The measurement that set the figure, and the figure, live
/// in the TS2 entry of `PLAN.md`: re-time the phases of
/// `crates/manager/src/tests/mod.rs` there if the machine changes.
const DEFAULT_BOUND: Duration = Duration::from_secs(120);

/// How often the parent looks at its child: fine against a bound in seconds,
/// and a quick child costs one poll.
const POLL: Duration = Duration::from_millis(10);

/// How long the parent gives one pipe to close after its child ended. A child
/// closes its own pipes as it dies, so this only covers a process that
/// inherited them and sits outside what the kill reaches: on Unix only a
/// process the group kill missed, on Windows any of them (see `kill_child`).
/// The two pipes are read in turn, so this bounds each. Past it the parent
/// answers with what it has: an output with no summary line is no count, so
/// the child fails, and the call returns.
const PIPE_GRACE: Duration = Duration::from_secs(1);

/// Run `body` in a child process of this test binary, with the data root
/// redirected to a temp dir of its own, and fail the calling test unless the
/// child ran exactly the test `test_path` (its own path) and passed.
pub fn isolated(test_path: &str, body: impl FnOnce()) {
    if std::env::var(MARKER).is_ok_and(|marked| marked == test_path) {
        // The child: the process this call spawned, running exactly this test,
        // so the body runs here, alone. Its dir sits in the base the parent
        // made and named here: this guard removes it when the body ends, and
        // the parent's removes the base, so a kill leaves nothing behind.
        let base = std::env::var_os(ROOT).expect(
            "the base dir of the isolated child: only the parent of a call sets the marker, \
             and it names the dir here",
        );
        let dir = tempfile::tempdir_in(Path::new(&base)).expect("the child's dir in its base");
        crate::paths::redirect_for_tests(dir.path());
        body();
        return;
    }
    let bound = bound();
    // The parent's own base dir, handed to the child to make its data root in:
    // its guard drops when this call returns, so the tree is gone after a
    // passing, a failing or a killed child alike.
    let base = tempfile::tempdir().expect("the child's base dir");
    let run = run_child(test_path, base.path(), bound);
    let stdout = String::from_utf8_lossy(&run.stdout);
    let stderr = String::from_utf8_lossy(&run.stderr);
    print!("{stdout}");
    eprint!("{stderr}");
    let counts = passed_and_failed(&stdout);
    if run.killed {
        panic!(
            "the isolated child for {test_path} was killed at its bound of {} ms: it was \
             still running past the bound, so it hangs, and the bound is what frees the \
             suite. A phase that needs longer sets {BOUND_MS} in its own test.\n{stdout}{stderr}",
            bound.as_millis(),
        );
    }
    let report = match counts {
        Some((passed, failed)) => format!("{passed} passed and {failed} failed"),
        None => "no summary line at all".to_string(),
    };
    assert!(
        run.status.success() && counts == Some((1, 0)),
        "the isolated child for {test_path} did not run exactly one passing test \
         (exit {:?}, {report}):\n{stdout}{stderr}",
        run.status.code(),
    );
}

/// The bound of a call: the one the calling test set, else `DEFAULT_BOUND`. A
/// value that is not a number of milliseconds is no bound set.
fn bound() -> Duration {
    std::env::var(BOUND_MS)
        .ok()
        .and_then(|ms| ms.parse::<u64>().ok())
        .map(Duration::from_millis)
        .unwrap_or(DEFAULT_BOUND)
}

/// One child run: how it ended, what it left in its two pipes, and whether the
/// parent ended it at the bound.
struct Run {
    status: std::process::ExitStatus,
    stdout: Vec<u8>,
    stderr: Vec<u8>,
    /// The parent signalled the child at the bound AND the status shows the
    /// signal: a child that exited on its own in that same moment is reported
    /// as finished, not as killed.
    killed: bool,
}

/// Spawn the child running `test_path`, with its data root made inside the
/// base dir `base`, and wait for it no longer than `bound`, killing it past
/// that.
fn run_child(test_path: &str, base: &Path, bound: Duration) -> Run {
    let mut command =
        std::process::Command::new(std::env::current_exe().expect("this test binary"));
    command
        .args([
            "--exact",
            test_path,
            "--include-ignored",
            "--nocapture",
            "--test-threads=1",
        ])
        .env(MARKER, test_path)
        .env(ROOT, base)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    // Unix: the child leads a process group of its own, so the kill at the
    // bound is a signal to that group and reaches every process the child
    // started (see `kill_child`).
    #[cfg(unix)]
    std::os::unix::process::CommandExt::process_group(&mut command, 0);
    let mut child = command.spawn().expect("spawn the child test");
    // Each pipe is drained on a thread of its own: a child that writes more
    // than a pipe holds blocks in the write, so the parent would be counting a
    // bound of a child that is only waiting for its reader.
    let out = spawn_reader(child.stdout.take().expect("the child's stdout pipe"));
    let err = spawn_reader(child.stderr.take().expect("the child's stderr pipe"));
    let started = Instant::now();
    let mut past_bound = false;
    let status = loop {
        if let Some(status) = child.try_wait().expect("wait for the child test") {
            break status;
        }
        if started.elapsed() >= bound {
            past_bound = true;
            // The child can finish in the moment between the check above and
            // the signal: look once more, and take the status the kill leaves.
            match child.try_wait().expect("wait for the child test") {
                Some(status) => break status,
                None => {
                    kill_child(&mut child);
                    break child.wait().expect("reap the child killed at the bound");
                }
            }
        }
        std::thread::sleep(POLL);
    };
    // A child that exited of itself at the boundary leaves a status that says
    // it finished: the signal reached nothing, so the child is not killed.
    let killed = past_bound && !status.success();
    Run {
        status,
        stdout: read_pipe(&out),
        stderr: read_pipe(&err),
        killed,
    }
}

/// Drain `pipe` on a thread of its own and hand the parent the reading end of
/// what it collects. The parent takes it with a bound (`read_pipe`) instead of
/// joining the thread: a pipe a process the kill did not reach still holds
/// would block the join forever, and the thread left behind ends with the test
/// binary.
fn spawn_reader(mut pipe: impl Read + Send + 'static) -> mpsc::Receiver<Vec<u8>> {
    let (tx, rx) = mpsc::channel();
    std::thread::spawn(move || {
        let mut buf = Vec::new();
        if pipe.read_to_end(&mut buf).is_ok() {
            let _ = tx.send(buf);
        }
    });
    rx
}

/// What the reader of one pipe collected, giving up at `PIPE_GRACE`: `None`
/// from the channel is a pipe still held, or a reader that died, and an empty
/// output is as good a report of it as any.
fn read_pipe(pipe: &mpsc::Receiver<Vec<u8>>) -> Vec<u8> {
    pipe.recv_timeout(PIPE_GRACE).unwrap_or_default()
}

/// End the child and what it started, at the bound.
///
/// Unix: the child leads its own process group (`process_group` in `run_child`)
/// and `Child::kill` signals only the direct child, so a grandchild that
/// inherited its two pipes would keep the parent's readers open for its whole
/// life. The group's leader is the child, so its pid is the group's id, and
/// SIGKILL to the group ends both.
///
/// Windows: ending the tree needs a job object (`CreateJobObjectW` with
/// `JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE`, then `AssignProcessToJobObject`), a
/// pair of new windows-sys features and a handle kept alive beside the child,
/// for the two pipes of a child that spawns nothing today. So the kill here
/// ends the direct child only; what a child leaves behind holding the pipes
/// costs the call `PIPE_GRACE` and shows as output with no summary line, so
/// the child fails and the call returns.
#[cfg(unix)]
fn kill_child(child: &mut Child) {
    let group = nix::unistd::Pid::from_raw(child.id() as i32);
    // Best effort: the group can be gone already, the race `run_child` names,
    // and then there is nothing left to signal.
    let _ = nix::sys::signal::killpg(group, nix::sys::signal::Signal::SIGKILL);
}

#[cfg(not(unix))]
fn kill_child(child: &mut Child) {
    let _ = child.kill();
}

/// The `(passed, failed)` counts of one libtest summary line: the number
/// standing before each of the two words, in the order libtest writes them
/// (`<N> passed; <M> failed;`). Anything else is no count: the words in
/// another order, one of the two, a word with no number behind it.
fn counts(line: &str) -> Option<(usize, usize)> {
    let words: Vec<&str> = line.split_whitespace().collect();
    let passed_at = words.iter().position(|word| *word == "passed;")?;
    let passed = words.get(passed_at.checked_sub(1)?)?.parse().ok()?;
    let failed_at = passed_at + 2;
    if words.get(failed_at) != Some(&"failed;") {
        return None;
    }
    let failed = words[failed_at - 1].parse().ok()?;
    Some((passed, failed))
}

/// The child's totals over every `test result:` line of its output, or `None`
/// when it carries no summary at all (the child died before writing one).
fn passed_and_failed(stdout: &str) -> Option<(usize, usize)> {
    let mut total: Option<(usize, usize)> = None;
    for line in stdout
        .lines()
        .filter(|line| line.starts_with("test result:"))
    {
        let (passed, failed) = counts(line)?;
        let (sum_passed, sum_failed) = total.unwrap_or((0, 0));
        total = Some((sum_passed + passed, sum_failed + failed));
    }
    total
}

#[cfg(test)]
#[path = "isolated_tests.rs"]
mod tests;
