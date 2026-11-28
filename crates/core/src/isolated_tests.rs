//! The tests of `isolated` (TS2a, ARCHITECTURE.md "Isolated data-root tests").
//!
//! Every body that writes under the data root checks the root sits under the
//! temp dir FIRST: a body running with no redirect would write into the user's
//! real tree.

use super::{counts, isolated, passed_and_failed};
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

/// The panic text of the body of `isolated_panicking_child`, which must reach
/// the failure of `isolated_panicking_body_fails_the_parent`.
const CHILD_PANIC: &str = "TS2A-CHILD-PANIC-MARKER";

/// The test path of `isolated_wrong_test_path_fails`: libtest runs no test for
/// it and exits 0, which the helper must not read as a pass.
const NO_SUCH_TEST: &str = "isolated::tests::no_such_isolated_test";

/// Set by `isolated_the_child_root_is_removed_after_the_test` (the child
/// inherits the parent's environment): the folder its child's body reports its
/// data root into, so the parent can check it afterwards. It is named from
/// outside the child's test function, so the child re-runs that function
/// without repointing it at a temp dir of its own.
const REPORT_DIR: &str = "PAI_TS2A_REPORT_DIR";

/// The message of a caught `panic!`: one with a formatted argument carries a
/// `String`, one with a literal a `&str`.
fn panic_text(caught: Box<dyn std::any::Any + Send>) -> String {
    if let Some(s) = caught.downcast_ref::<String>() {
        return s.clone();
    }
    let s = caught
        .downcast_ref::<&str>()
        .copied()
        .unwrap_or("the panic carried no message");
    (*s).to_string()
}

/// The data root this child sees, checked to be under the temp dir, then
/// written in: `marker` lands in it. `other` is the marker the sibling test of
/// the pair writes: finding it here means the two tests share one root.
fn write_marker_in_my_root(marker: &str, other: &str) {
    let root = crate::paths::data_root();
    let tmp = std::env::temp_dir();
    assert!(
        root.is_absolute(),
        "the data root is not absolute: {root:?}"
    );
    assert!(
        root.starts_with(&tmp),
        "the data root is not under the temp dir {}: {root:?}",
        tmp.display()
    );
    std::fs::create_dir_all(&root).expect("create the redirected data root");
    std::fs::write(root.join(marker), marker).expect("write under the data root");
    // The sibling does the same at the same time: both roots existing for a
    // moment is what a shared root shows itself in.
    std::thread::sleep(Duration::from_millis(200));
    assert!(
        !root.join(other).exists(),
        "the marker {other} of the other isolated test is in my root {root:?}: \
         the two isolated tests share one data root"
    );
}

/// The body of `isolated_child_reports_its_root`: writes a file under its data
/// root, so the removal has something to remove, and hands the root to the
/// parent through `REPORT_DIR`.
fn report_my_root() {
    let root = crate::paths::data_root();
    assert!(
        root.starts_with(std::env::temp_dir()),
        "the data root is not under the temp dir: {root:?}"
    );
    std::fs::create_dir_all(&root).expect("create the redirected data root");
    std::fs::write(root.join("file"), b"body ran").expect("write under the data root");
    let dir = std::env::var_os(REPORT_DIR).expect(
        "the report dir: the child must inherit the parent's environment (the temp \
         dir rule reads TMPDIR from it too)",
    );
    std::fs::write(Path::new(&dir).join("root"), root.display().to_string())
        .expect("report the root");
}

/// 1: a body that passes, passes the parent. The body writes under the data
/// root and reads back, so the parent passing means the body ran with the
/// redirect in force.
#[test]
fn isolated_passing_body_passes() {
    isolated("isolated::tests::isolated_passing_body_passes", || {
        let root = crate::paths::data_root();
        assert!(
            root.starts_with(std::env::temp_dir()),
            "the data root is not under the temp dir: {root:?}"
        );
        std::fs::create_dir_all(&root).expect("create the redirected data root");
        let file = root.join("file");
        std::fs::write(&file, b"body ran").expect("write under the data root");
        assert_eq!(std::fs::read(&file).expect("read back"), b"body ran");
    });
}

/// 2's body, the shape a TS2b phase has: a test of its own, with no catcher
/// around the helper call, so the child that re-runs it lets the panic out.
///
/// `#[ignore]`d so it fails nothing in the suite on its own: run plainly it
/// would act as the parent of itself, spawn a child of its own whose body
/// panics, and fail on that. The helper re-runs it with the marker set, and to
/// run it at all it must pass `--include-ignored` (TS2a: the helper's one
/// change).
#[test]
#[ignore = "TS2a 2: runs only as the isolated child of \
            isolated_panicking_body_fails_the_parent"]
fn isolated_panicking_child() {
    isolated("isolated::tests::isolated_panicking_child", || {
        panic!("{CHILD_PANIC}")
    });
}

/// 2: a body that panics fails the parent, and the parent's failure carries the
/// child's panic message. The panicking body lives in `isolated_panicking_child`
/// (its own test, no catcher): a `catch_unwind` around the helper call wraps the
/// child's run of the body too, so a body written here is swallowed in the
/// child and the parent sees a passing child.
#[test]
fn isolated_panicking_body_fails_the_parent() {
    let caught = std::panic::catch_unwind(|| {
        isolated("isolated::tests::isolated_panicking_child", || {
            unreachable!("the parent must not run the body of the child it spawns")
        });
    });
    let text = panic_text(caught.expect_err("a panicking body must fail the parent"));
    assert!(
        text.contains(CHILD_PANIC),
        "the parent's failure does not carry the child's panic message \
         {CHILD_PANIC}: {text}"
    );
}

/// 3: a test path libtest knows nothing about fails, because it runs 0 tests
/// and exits 0. TS2a2 2 adds the other half: the failure also reports what the
/// child said, that is 0 passed, so the reader sees "no test ran" and not only
/// "the child did not pass".
#[test]
fn isolated_wrong_test_path_fails() {
    let caught = std::panic::catch_unwind(|| {
        isolated(NO_SUCH_TEST, || {
            panic!("the body of a test libtest never ran must not run")
        });
    });
    let text = panic_text(caught.expect_err("a test path that runs 0 tests must fail"));
    assert!(
        text.contains(NO_SUCH_TEST),
        "the failure does not name the test path that ran no test {NO_SUCH_TEST}: {text}"
    );
    // TS2a2 review 3: the CHILD's summary line, forwarded into the failure, and
    // not a phrasing of the helper's own. The helper writes
    // `{passed} passed and {failed} failed`, so a search for `0 passed` is
    // answered by that sentence with none of the child's output in sight. The
    // words with their `;` are libtest's own (`test result: ok. 0 passed;
    // 0 failed; ...` for a path it knows nothing about) and reach the failure
    // only through what the child wrote.
    assert!(
        text.contains("0 passed; 0 failed;"),
        "the failure does not carry the child's own summary line, 0 passed; 0 failed;: {text}"
    );
}

/// 4: each isolated test sees its own data root, under a temp dir and distinct
/// from the other's. The pair runs together, in parallel.
#[test]
fn isolated_each_test_sees_its_own_root_a() {
    isolated(
        "isolated::tests::isolated_each_test_sees_its_own_root_a",
        || write_marker_in_my_root("marker-a", "marker-b"),
    );
}

/// 4, the sibling: the same, writing the marker the first one looks for.
#[test]
fn isolated_each_test_sees_its_own_root_b() {
    isolated(
        "isolated::tests::isolated_each_test_sees_its_own_root_b",
        || write_marker_in_my_root("marker-b", "marker-a"),
    );
}

/// 5's body, in a test of its own: `#[ignore]`d for the same reason as
/// `isolated_panicking_child`, and run only as the isolated child of
/// `isolated_the_child_root_is_removed_after_the_test`. Its preamble sets no
/// environment variable, so the `REPORT_DIR` the parent set reaches the child
/// pointing at the parent's folder.
#[test]
#[ignore = "TS2a 5: runs only as the isolated child of \
            isolated_the_child_root_is_removed_after_the_test"]
fn isolated_child_reports_its_root() {
    isolated(
        "isolated::tests::isolated_child_reports_its_root",
        report_my_root,
    );
}

/// 5: the child's temp root is gone once `isolated` returns: the root sits in
/// the base dir the PARENT made, so the parent's guard removes the tree, and a
/// killed child that runs no destructor of its own leaves nothing behind (the
/// child's own guard only covers a child that finishes). A root left behind is
/// a tree in the temp dir that no test owns (the rule
/// `src/tests/redirect_lint.rs` scans the sources for).
/// The report dir is created by this test and named in the environment BEFORE
/// the helper call, outside the child's own test function: a report dir made in
/// that function is recreated by the child, which then reports into a folder of
/// its own, removed before the parent looks.
#[test]
fn isolated_the_child_root_is_removed_after_the_test() {
    let report = tempfile::tempdir().expect("report dir");
    std::env::set_var(REPORT_DIR, report.path());
    isolated("isolated::tests::isolated_child_reports_its_root", || {
        unreachable!("the parent must not run the body of the child it spawns")
    });
    std::env::remove_var(REPORT_DIR);
    let text = std::fs::read_to_string(report.path().join("root")).expect(
        "the child never reported its root: the body did not run, or the child does \
         not inherit the parent's environment",
    );
    let root = PathBuf::from(text.trim_end());
    assert!(
        !root.exists(),
        "the child's data root {root:?} is still there after the test: its temp dir \
         leaked into the temp dir"
    );
    let owned = root
        .parent()
        .expect("the data root sits in the child's temp dir");
    assert!(
        !owned.exists(),
        "the child's temp dir {owned:?} is still there after the test: its temp dir \
         leaked into the temp dir"
    );
}

/// TS2a2 1's seam: the parent's bound on a child that has not finished, in
/// milliseconds, read by the parent alone (`bound` in `isolated.rs`). It is the
/// smallest seam the brief offers (an environment variable only the tests set,
/// read by the parent alone), and it keeps `isolated`'s signature, which every
/// phase of TS2b and TS2c calls.
const BOUND_MS: &str = "PAI_TEST_ISOLATED_BOUND_MS";

/// The turn to set that one process-wide name. The tests of the bound run in
/// parallel and each sets and removes it around its own call, so the parent of
/// the hanging child can read the quick test's bound, which is roomier than
/// this file's watchdog: it waits its child out, and the test fails saying no
/// bound is enforced while a bound was in force. The lock is held across the
/// set, the run and the remove of each test, which is what makes the two
/// windows not overlap. Poisoning is ignored: the hanging test's watchdog
/// panics by design, and the lock orders two sets, it guards no data.
static BOUND_MS_TURN: std::sync::Mutex<()> = std::sync::Mutex::new(());

/// Take the turn to set `BOUND_MS`: the guard, kept by the test until it has
/// removed the variable again.
fn bound_ms_turn() -> std::sync::MutexGuard<'static, ()> {
    BOUND_MS_TURN
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
}

/// The bound TS2a2 1 sets: far above a healthy child run of this file (a parent
/// with its child, the bodies of two of them sleeping 200 ms), so a bound that
/// roomy cannot kill the child of a parent running beside the test while the
/// variable is set. Time the real figure where the phases land (TS2b), not here.
const BOUND: Duration = Duration::from_millis(2000);

/// How long TS2a2 1's child sleeps: far past `BOUND`, so a parent that comes
/// back inside this has killed its child rather than waited it out.
const HANG: Duration = Duration::from_secs(30);

/// How long the test waits for `isolated` to come back: a multiple of `BOUND`,
/// so the red names the missing bound and the test never sits for `HANG`.
const WATCHDOG: Duration = Duration::from_secs(6);

/// The path of TS2a2 1's child.
const HANGING_CHILD: &str = "isolated::tests::isolated_hanging_child";

/// The path of TS2a2 1's quick child, the one a generous bound must spare.
const QUICK_CHILD: &str = "isolated::tests::isolated_quick_child";

/// The path of TS2a2 review 1's child, the one that leaves a grandchild holding
/// the two pipes.
#[cfg(unix)]
const GRANDCHILD_CHILD: &str = "isolated::tests::isolated_hanging_child_with_grandchild";

/// Set by the parents of the killed children (the children inherit it): the
/// folder their child reports into, so the parent can look at the kill through
/// what the child wrote OUTSIDE the tree the kill is meant to remove. Named
/// from outside the child's own test function, so the child re-runs that
/// function without repointing it at a temp dir of its own.
const TS2A2_REPORT_DIR: &str = "PAI_TS2A2_REPORT_DIR";

/// The room given to a parent that killed its child, on top of `BOUND`: the
/// kill, the reap and the two pipes closing all happen after the bound crossed,
/// and a parent that closes them in-process stays far inside this. A parent that
/// has to wait for a process the kill did not reach sits in it for that
/// process's whole life.
#[cfg(unix)]
const KILL_MARGIN: Duration = Duration::from_secs(2);

/// What a call of `isolated` made off the test thread did.
#[derive(Debug)]
enum Outcome {
    /// It came back with no failure, after this long.
    Passed(Duration),
    /// It failed with this message, after this long.
    Failed(Duration, String),
    /// It has not come back at all, after the test's own watchdog.
    Hung,
}

/// Call `isolated` on a thread the test does not wait for, and report what came
/// of it. The deadline here is what turns a parent that does not come back into
/// a failing test instead of a hung one: a call that blocks past the bound, on
/// a pipe a process the kill did not reach still holds, reds here at
/// `WATCHDOG`. The red path leaves that thread behind: its child writes nothing
/// and exits when its sleep ends.
fn isolated_off_thread(test_path: &'static str, body: impl FnOnce() + Send + 'static) -> Outcome {
    let (tx, rx) = std::sync::mpsc::channel();
    let started = Instant::now();
    std::thread::spawn(move || {
        let caught = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            isolated(test_path, body);
        }));
        tx.send(caught.err().map(panic_text))
            .expect("report the outcome of the call");
    });
    match rx.recv_timeout(WATCHDOG) {
        Ok(None) => Outcome::Passed(started.elapsed()),
        Ok(Some(text)) => Outcome::Failed(started.elapsed(), text),
        Err(_) => Outcome::Hung,
    }
}

/// The child's data root, created and written into, reported in
/// `<report dir>/root` when its parent named one (no report dir named, nothing
/// to report: the plain hanging child of TS2a2 1 runs this body too). A killed
/// child runs no destructor, so its parent learns the tree's path only from a
/// file the child wrote while it lived.
fn report_the_childs_root() {
    let Some(dir) = std::env::var_os(TS2A2_REPORT_DIR) else {
        return;
    };
    let root = crate::paths::data_root();
    assert!(
        root.starts_with(std::env::temp_dir()),
        "the data root is not under the temp dir: {root:?}"
    );
    std::fs::create_dir_all(&root).expect("create the redirected data root");
    std::fs::write(root.join("file"), b"body ran").expect("write under the data root");
    std::fs::write(Path::new(&dir).join("root"), root.display().to_string())
        .expect("report the data root");
}

/// TS2a2 review 1's grandchild: a `sleep` started with the default stdio, so it
/// inherits this process's stdout and stderr, which are the two pipes the parent
/// drains. Its PID is reported in `<report dir>/grandchild-pid`, so the parent
/// can look for it after the kill. The handle is dropped on purpose: the process
/// is meant to outlive this one, and the kill under test is what ends it (once
/// this process dies, init reaps it).
#[cfg(unix)]
#[allow(clippy::zombie_processes)]
fn spawn_grandchild_holding_the_pipes() {
    let Some(dir) = std::env::var_os(TS2A2_REPORT_DIR) else {
        return;
    };
    let grandchild = std::process::Command::new("sleep")
        .arg(HANG.as_secs().to_string())
        .spawn()
        .expect("spawn the grandchild inheriting stdout and stderr");
    std::fs::write(
        Path::new(&dir).join("grandchild-pid"),
        grandchild.id().to_string(),
    )
    .expect("report the grandchild's PID");
}

/// What a child reported into its parent's report dir: `<dir>/<name>`.
#[cfg(unix)]
fn reported(report: &Path, name: &str) -> String {
    std::fs::read_to_string(report.join(name)).unwrap_or_else(|read| {
        panic!(
            "the child never reported {name}: its body did not run, or the child does \
             not inherit the parent's environment: {read}"
        )
    })
}

/// A process that exists: Linux's `/proc/<pid>`, elsewhere `/bin/kill -0`, which
/// answers 0 while a signal can be sent to the process and not 0 once it is
/// gone.
#[cfg(target_os = "linux")]
fn process_alive(pid: u32) -> bool {
    std::path::Path::new(&format!("/proc/{pid}")).exists()
}

#[cfg(all(unix, not(target_os = "linux")))]
fn process_alive(pid: u32) -> bool {
    std::process::Command::new("kill")
        .args(["-0", &pid.to_string()])
        .status()
        .is_ok_and(|sent| sent.success())
}

/// TS2a2 1's child, `#[ignore]`d as the other bodies that run only as a child:
/// run on its own it would act as the parent of itself, spawn a child of its
/// own asleep for `HANG`, and hang there. It reports its data root when its
/// parent named a report dir (TS2a2 review 2).
#[test]
#[ignore = "TS2a2 1: runs only as the isolated child of \
            isolated_hanging_child_is_killed_at_the_bound"]
fn isolated_hanging_child() {
    isolated(HANGING_CHILD, || {
        report_the_childs_root();
        std::thread::sleep(HANG);
    });
}

/// TS2a2 1: a child that hangs is killed at the bound and the parent fails, with
/// a message naming the test and the bound (a hung child hanging the suite is
/// the defect the bound exists to close).
#[test]
fn isolated_hanging_child_is_killed_at_the_bound() {
    let _turn = bound_ms_turn();
    std::env::set_var(BOUND_MS, BOUND.as_millis().to_string());
    let outcome = isolated_off_thread(HANGING_CHILD, || {
        unreachable!("the parent must not run the body of the child it spawns")
    });
    std::env::remove_var(BOUND_MS);
    let (elapsed, text) = match outcome {
        Outcome::Hung => panic!(
            "isolated did not come back within {WATCHDOG:?} of a child asleep for \
             {HANG:?} under a {BOUND:?} bound: the parent enforces no bound, so a \
             hung child hangs the suite"
        ),
        Outcome::Passed(after) => panic!(
            "isolated passed a child asleep for {HANG:?} under a {BOUND:?} bound \
             (after {after:?}): a hung child must fail the parent"
        ),
        Outcome::Failed(after, text) => (after, text),
    };
    assert!(
        elapsed >= BOUND,
        "the parent gave up on its child after {elapsed:?}, before the {BOUND:?} bound \
         it was given: the bound is not what ends the wait"
    );
    assert!(
        elapsed < HANG,
        "the parent came back after {elapsed:?}, as long as the child's sleep of \
         {HANG:?}: the child was waited out, not killed at the {BOUND:?} bound"
    );
    assert!(
        text.contains(HANGING_CHILD),
        "the failure of the hung child does not name the test {HANGING_CHILD}: {text}"
    );
    // TS2a2 review 4: the bound named in the helper's OWN sentence, spelled in
    // the milliseconds the test set: one figure, so the coder cannot answer with
    // a duration of its own choosing. The child's forwarded output is cut off
    // first: a line of its own like `finished in 2.00s` would satisfy the search
    // by accident.
    let head = text.split("\nrunning ").next().unwrap_or(&text);
    let bound_ms = BOUND.as_millis().to_string();
    assert!(
        head.contains(&bound_ms),
        "the failure of the hung child does not name the bound it crossed, in the \
         milliseconds it was set ({bound_ms}): {text}"
    );
}

/// TS2a2 1's other edge: the bound is a bound, not a kill timer. The body of
/// `isolated_quick_child` is well inside a 10000 ms bound and writes under its
/// data root, so the parent passing means the child ran to the end of the body.
/// A bound that fires at once shows as the helper's own failure of a child it
/// killed before the child finished.
#[test]
fn isolated_bound_spares_a_child_inside_the_bound() {
    let _turn = bound_ms_turn();
    std::env::set_var(BOUND_MS, "10000");
    isolated(QUICK_CHILD, || {
        unreachable!("the parent must not run the body of the child it spawns")
    });
    std::env::remove_var(BOUND_MS);
}

/// TS2a2 1's quick child, `#[ignore]`d as the other bodies that run only as a
/// child: run on its own it would act as the parent of itself and spawn a child
/// of its own.
#[test]
#[ignore = "TS2a2 1: runs only as the isolated child of \
            isolated_bound_spares_a_child_inside_the_bound"]
fn isolated_quick_child() {
    isolated(QUICK_CHILD, || {
        let root = crate::paths::data_root();
        assert!(
            root.starts_with(std::env::temp_dir()),
            "the data root is not under the temp dir: {root:?}"
        );
        std::fs::create_dir_all(&root).expect("create the redirected data root");
        std::fs::write(root.join("file"), b"body ran").expect("write under the root");
    });
}

/// TS2a2 review 1 (must fix): a child that leaves a grandchild holding its two
/// pipes is killed WITH it, and the parent comes back at its bound.
/// `Child::kill()` ends the direct child only: the two pipes stay open while a
/// grandchild that inherited them lives, so the parent's readers never return
/// and the call sits there for the grandchild's whole sleep. Unix only: the
/// fixture forks a `sleep`, and the leak needs a real process the kill does not
/// reach.
#[cfg(unix)]
#[test]
fn isolated_a_grandchild_holding_the_pipes_dies_with_the_child() {
    let _turn = bound_ms_turn();
    let report = tempfile::tempdir().expect("report dir");
    std::env::set_var(TS2A2_REPORT_DIR, report.path());
    std::env::set_var(BOUND_MS, BOUND.as_millis().to_string());
    let outcome = isolated_off_thread(GRANDCHILD_CHILD, || {
        unreachable!("the parent must not run the body of the child it spawns")
    });
    std::env::remove_var(BOUND_MS);
    std::env::remove_var(TS2A2_REPORT_DIR);
    let (elapsed, text) = match outcome {
        Outcome::Hung => panic!(
            "isolated did not come back within {WATCHDOG:?} of a child that left a \
             grandchild asleep for {HANG:?} under a {BOUND:?} bound: the parent killed its \
             child, and its two readers stay blocked on the pipes the grandchild inherited \
             from it, so the call never returns"
        ),
        Outcome::Passed(after) => panic!(
            "isolated passed a child that left a grandchild holding its pipes under a \
             {BOUND:?} bound (after {after:?}): a hung child must fail the parent"
        ),
        Outcome::Failed(after, text) => (after, text),
    };
    assert!(
        elapsed >= BOUND,
        "the parent gave up on its child after {elapsed:?}, before the {BOUND:?} bound it \
         was given: the bound is not what ends the wait"
    );
    assert!(
        elapsed < BOUND + KILL_MARGIN,
        "the parent came back after {elapsed:?}, past the {BOUND:?} bound and its \
         {KILL_MARGIN:?} of margin: the kill reached only the direct child, and the call \
         waited out the grandchild holding the pipes"
    );
    assert!(
        text.contains(GRANDCHILD_CHILD),
        "the failure of the killed child does not name the test {GRANDCHILD_CHILD}: {text}"
    );
    // The other half of the same defect: the grandchild is not only out of the
    // parent's way, it is GONE, killed with the child. Left behind it is a
    // process nobody owns, alive past the test that started it.
    let pid = reported(report.path(), "grandchild-pid")
        .trim()
        .parse::<u32>()
        .expect("the grandchild's PID, a number");
    assert!(
        !process_alive(pid),
        "the grandchild {pid} is alive after the parent failed: the kill ended only the \
         direct child, and that is the process holding the two pipes"
    );
}

/// TS2a2 review 1's child: it reports its data root and the PID of a grandchild
/// that inherits its stdout and stderr and sleeps for `HANG`, then sleeps
/// itself, so the parent reaches its bound with a live process behind it.
/// `#[ignore]`d as the other bodies that run only as a child: run on its own it
/// would act as the parent of itself.
#[cfg(unix)]
#[test]
#[ignore = "TS2a2 review 1: runs only as the isolated child of \
            isolated_a_grandchild_holding_the_pipes_dies_with_the_child"]
fn isolated_hanging_child_with_grandchild() {
    isolated(GRANDCHILD_CHILD, || {
        report_the_childs_root();
        spawn_grandchild_holding_the_pipes();
        std::thread::sleep(HANG);
    });
}

/// TS2a2 review 2: a child killed at its bound leaves no data-root tree behind.
/// The killed child runs no destructor of its own, so it is the base dir the
/// PARENT made that has to take the child's dir with it. Day one green: the
/// base's guard drops when the call returns, on the kill path as on the passing
/// path test 5 pins.
#[cfg(unix)]
#[test]
fn isolated_a_killed_child_leaves_no_root_behind() {
    let _turn = bound_ms_turn();
    let report = tempfile::tempdir().expect("report dir");
    std::env::set_var(TS2A2_REPORT_DIR, report.path());
    std::env::set_var(BOUND_MS, BOUND.as_millis().to_string());
    let outcome = isolated_off_thread(HANGING_CHILD, || {
        unreachable!("the parent must not run the body of the child it spawns")
    });
    std::env::remove_var(BOUND_MS);
    std::env::remove_var(TS2A2_REPORT_DIR);
    match outcome {
        Outcome::Hung => panic!(
            "isolated did not come back within {WATCHDOG:?} of a child asleep for \
             {HANG:?} under a {BOUND:?} bound: the call never returns, so nothing can be \
             said of the tree a killed child leaves"
        ),
        Outcome::Passed(after) => panic!(
            "isolated passed a child asleep for {HANG:?} under a {BOUND:?} bound (after \
             {after:?}): a child killed at the bound must fail the parent"
        ),
        Outcome::Failed(_, _) => {}
    }
    let root = PathBuf::from(reported(report.path(), "root").trim_end());
    let child_dir = root
        .parent()
        .expect("the data root sits in the child's temp dir");
    let base = child_dir
        .parent()
        .expect("the child's temp dir sits in the base dir the parent made");
    assert!(
        !base.exists(),
        "the base dir {base:?} is still there after the child was killed at the \
         {BOUND:?} bound: the base dir is what keeps a child that runs no destructor \
         from leaving a tree in the temp dir"
    );
    assert!(
        !child_dir.exists(),
        "the killed child's temp dir {child_dir:?} is still there: it sits in the base \
         dir, and goes with it"
    );
    assert!(
        !root.exists(),
        "the killed child's data root {root:?} is still there: a tree in the temp dir no \
         test owns"
    );
}

/// TS2a2 3: the `(passed, failed)` parse of libtest's summary line, the shapes
/// with counts. The words carry a `;` in libtest's own output (`0 passed;
/// 0 failed;`), which the parse keys on.
#[test]
fn isolated_summary_line_reports_the_counts() {
    let lines = [
        (
            "test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 544 filtered out; finished in 0.01s",
            Some((1, 0)),
        ),
        (
            "test result: FAILED. 0 passed; 2 failed; 0 ignored; 0 measured; 544 filtered out; finished in 0.20s",
            Some((0, 2)),
        ),
        (
            "test result: ok. 12 passed; 3 failed;",
            Some((12, 3)),
        ),
    ];
    for (line, want) in lines {
        assert_eq!(counts(line), want, "the counts of {line:?}");
    }
}

/// TS2a2 3: the shapes with no `N passed; M failed` line, which the parse reads
/// as no count at all, and so the parent reads as a failure: a child that died,
/// was killed, or wrote something else must never pass.
#[test]
fn isolated_summary_line_without_counts_is_no_count() {
    for line in [
        "",
        "running 0 tests",
        "test isolated::tests::isolated_passing_body_passes ... ok",
        // A summary heading with no counts behind the words.
        "test result: ok.",
        // The words without their `;`: not libtest's line.
        "test result: ok. 1 passed",
        // One of the two words only.
        "test result: ok. 1 passed; 0 ignored; 0 measured;",
        "test result: ok. 0 failed; 0 ignored; 0 measured;",
        // A count that is not a number.
        "test result: ok. many passed; 0 failed;",
        // The words in the order libtest does not write them.
        "test result: ok. 0 failed; 1 passed;",
    ] {
        assert_eq!(counts(line), None, "the counts of {line:?}");
    }
}

/// TS2a2 3: the totals over a child's whole output, which is what the parent
/// judges: only a line that starts the summary counts, every `test result:` line
/// adds up, and an output the parse cannot read is no count.
#[test]
fn isolated_totals_of_a_childs_output() {
    // One child, one test: the output the parent of a passing phase sees.
    let passed = "running 1 test\ntest isolated::tests::isolated_passing_body_passes ... ok\n\
                  test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 544 filtered out; \
                  finished in 0.01s\n";
    assert_eq!(passed_and_failed(passed), Some((1, 0)));
    // A run of two targets: one summary line each, added up.
    let two = "test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out\n\
               test result: FAILED. 2 passed; 3 failed; 0 ignored; 0 measured; 0 filtered out\n";
    assert_eq!(passed_and_failed(two), Some((3, 3)));
    // No summary at all: the child died, was killed, or never ran a test.
    for output in [
        "",
        "running 0 tests\n",
        "   Compiling pai-core v0.0.0\n",
        "thread 'main' panicked at the child's temp dir\n",
        // A summary line that is indented is not the summary line.
        "    test result: ok. 1 passed; 0 failed;\n",
        // The run's own `... ok` lines, which are not totals.
        "test isolated::tests::isolated_passing_body_passes ... ok\n",
    ] {
        assert_eq!(passed_and_failed(output), None, "the totals of {output:?}");
    }
    // A line the parse cannot read is not skipped over: the whole output is
    // unreadable, so the child is not known to have passed.
    let unreadable = "test result: ok. 1 passed; 0 failed;\ntest result: ok. many passed;\n";
    assert_eq!(passed_and_failed(unreadable), None);
}
