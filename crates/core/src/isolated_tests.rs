//! The tests of `isolated` (TS2a, ARCHITECTURE.md "Isolated data-root tests").
//!
//! Every body that writes under the data root checks the root sits under the
//! temp dir FIRST: a body running with no redirect would write into the user's
//! real tree.

use super::isolated;
use std::path::{Path, PathBuf};
use std::time::Duration;

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
/// would act as the parent of itself, spawn a grandchild whose body panics, and
/// fail on that. The helper re-runs it with the marker set, and to run it at
/// all it must pass `--include-ignored` (TS2a: the helper's one change).
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
/// and exits 0.
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

/// 5: the child's temp root is gone once `isolated` returns: the guard owning
/// it lives in the child, and a root left behind is a tree in the temp dir that
/// no test owns (the rule `src/tests/redirect_lint.rs` scans the sources for).
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
